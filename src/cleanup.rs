use crate::config::Config;
use crate::error::{Result, TreebeardError};
use crate::git::GitRepo;
use crate::hooks::{self, HookContext};
use crate::overlay::MutationType;
use crate::session::remove_active_session;
use crate::sync;
use std::collections::HashMap;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::{atomic::AtomicUsize, Arc, Mutex, OnceLock};
use tokio::signal;

pub struct SquashContext<'a> {
    pub repo: GitRepo,
    pub branch_name: String,
    pub on_exit: &'a crate::config::OnExitBehavior,
    pub squash_message: String,
    pub base_commit: String,
}

pub struct WorktreeCleanupContext {
    pub worktree_path: std::path::PathBuf,
    pub main_repo_path: std::path::PathBuf,
}

pub struct SyncContext<'a> {
    pub mutations: &'a HashMap<PathBuf, MutationType>,
    pub main_repo_path: &'a std::path::Path,
    pub worktree_path: &'a std::path::Path,
    pub config: &'a Config,
}

pub struct CleanupContext {
    pub mount_path: Option<std::path::PathBuf>,
    pub worktree_path: std::path::PathBuf,
    pub main_repo_path: std::path::PathBuf,
    pub repo: GitRepo,
    pub branch_name: String,
    pub config: Config,
    pub mutations: HashMap<PathBuf, MutationType>,
    pub base_commit: String,
    pub auto_commit_failure_count: Arc<AtomicUsize>,
}

impl CleanupContext {
    pub fn as_squash_context(&self) -> SquashContext<'_> {
        SquashContext {
            repo: self.repo.clone(),
            branch_name: self.branch_name.clone(),
            on_exit: &self.config.cleanup.on_exit,
            squash_message: self
                .config
                .commit
                .squash_commit_message
                .replace("{branch}", &self.branch_name),
            base_commit: self.base_commit.clone(),
        }
    }

    pub fn as_worktree_cleanup_context(&self) -> WorktreeCleanupContext {
        WorktreeCleanupContext {
            worktree_path: self.worktree_path.clone(),
            main_repo_path: self.main_repo_path.clone(),
        }
    }

    pub fn as_sync_context(&self) -> SyncContext<'_> {
        SyncContext {
            mutations: &self.mutations,
            main_repo_path: &self.main_repo_path,
            worktree_path: &self.worktree_path,
            config: &self.config,
        }
    }

    /// Create a HookContext for running cleanup hooks.
    pub fn as_hook_context(&self) -> HookContext {
        let mount_path = self
            .mount_path
            .clone()
            .unwrap_or_else(|| self.worktree_path.clone());
        HookContext::new(
            &self.branch_name,
            &mount_path,
            &self.worktree_path,
            &self.main_repo_path,
        )
    }
}

/// Guard to prevent concurrent cleanup operations. Signal handlers (Ctrl+C) and
/// normal exit paths can both trigger cleanup; this ensures only one runs.
static CLEANUP_RUNNING: OnceLock<Mutex<bool>> = OnceLock::new();

pub async fn run_with_cancel_handler<Fut, FCleanup>(
    fut: Fut,
    critical_cleanup: FCleanup,
) -> Result<()>
where
    Fut: std::future::Future<Output = Result<()>>,
    FCleanup: FnOnce() -> Result<()>,
{
    tokio::select! {
        result = fut => result,
        _ = signal::ctrl_c() => {
            eprintln!("\nInterrupted");
            critical_cleanup()?;
            Ok(())
        }
    }
}

/// Perform FUSE cleanup with user-facing output.
///
/// This is a thin wrapper around `overlay::perform_fuse_cleanup` that adds
/// user-friendly console output for the interactive cleanup flow.
pub fn perform_fuse_cleanup_with_output(mount_path: &Path) {
    println!("Unmounting FUSE filesystem...");

    let result = crate::overlay::perform_fuse_cleanup(mount_path);

    if result.unmount_succeeded {
        println!("FUSE filesystem unmounted");
    } else {
        eprintln!("Warning: Failed to unmount (may already be unmounted)");
    }

    // Warn if directory removal failed (unmount succeeded but directory still exists)
    if result.unmount_succeeded && !result.directory_removed && mount_path.exists() {
        eprintln!("Warning: Failed to remove mount directory");
    }
}

fn squash_branch_commits(ctx: &SquashContext) -> Result<()> {
    let commit_count = match ctx
        .repo
        .get_commit_count_since(&ctx.branch_name, &ctx.base_commit)
    {
        Ok(count) => count,
        Err(_) => {
            eprintln!("Warning: Could not get commit count, skipping squash prompt");
            return Ok(());
        }
    };

    let should_squash = match commit_count {
        0 => {
            tracing::debug!("No new commits since branch creation, skipping squash");
            return Ok(());
        }
        _ => match ctx.on_exit {
            crate::config::OnExitBehavior::Squash => {
                if commit_count == 1 {
                    println!(
                        "Squashing {} auto-commit into a single commit...",
                        commit_count
                    );
                } else {
                    println!(
                        "Squashing {} auto-commits into a single commit...",
                        commit_count
                    );
                }
                true
            }
            crate::config::OnExitBehavior::Keep => {
                if commit_count == 1 {
                    println!("Keeping {} auto-commit", commit_count);
                } else {
                    println!("Keeping all {} auto-commits", commit_count);
                }
                false
            }
            crate::config::OnExitBehavior::Prompt => {
                if commit_count == 1 {
                    println!(
                        "\nYou have {} auto-commit on branch '{}'",
                        commit_count, ctx.branch_name
                    );
                } else {
                    println!(
                        "\nYou have {} auto-commits on branch '{}'",
                        commit_count, ctx.branch_name
                    );
                }
                println!("These commits will be preserved unless you choose to squash them\n");
                if prompt_yes_no("Squash auto-commits into a single commit?", false)? {
                    println!("\nSquashing {} auto-commit(s)...", commit_count);
                    true
                } else {
                    println!("\nKeeping all {} auto-commit(s)", commit_count);
                    false
                }
            }
        },
    };

    if !should_squash {
        return Ok(());
    }

    if let Err(e) = ctx
        .repo
        .squash_commits(&ctx.branch_name, &ctx.squash_message)
    {
        eprintln!("Warning: Failed to squash commits: {}", e);
    } else {
        println!("Commits squashed");
    }

    Ok(())
}

fn handle_worktree_cleanup(
    ctx: &WorktreeCleanupContext,
    was_cancelled: bool,
    git_check_failed: bool,
) -> Result<()> {
    // UX flow: If user pressed Ctrl+C during sync, they likely want to preserve their work.
    // We skip the deletion prompt entirely to avoid accidental data loss.
    if was_cancelled {
        println!(
            "Cleanup cancelled. Worktree left at: {}",
            ctx.worktree_path.display()
        );
        return Ok(());
    }

    // Check for uncommitted changes before prompting for deletion
    let worktree_repo = GitRepo::from_path(&ctx.worktree_path);
    if let Ok(wt_repo) = worktree_repo {
        if let Ok(has_changes) = wt_repo.has_uncommitted_changes() {
            if has_changes {
                eprintln!("Warning: Worktree has uncommitted changes");
                eprintln!();
            }
        }
    }

    // If git check-ignore failed, we couldn't determine which files were gitignored.
    // Modified files may exist that the user didn't have the opportunity to sync.
    // Require extra confirmation before allowing deletion.
    if git_check_failed {
        eprintln!("WARNING: Could not check which files are gitignored.");
        eprintln!("         Some modified files may not have been shown for sync.");
        eprintln!("         If you delete the worktree, these changes will be lost.");
        eprintln!();

        if !prompt_destructive_action("delete")? {
            println!("Worktree preserved at: {}", ctx.worktree_path.display());
            return Ok(());
        }
    } else {
        // Normal flow: require explicit 'y' since deletion is destructive
        if !prompt_yes_no("Delete worktree directory?", false)? {
            println!("Worktree preserved at: {}", ctx.worktree_path.display());
            return Ok(());
        }
    }

    println!("Removing worktree...");

    let main_repo = GitRepo::from_path(&ctx.main_repo_path)?;

    if let Err(e) = main_repo.remove_worktree(&ctx.worktree_path, false) {
        eprintln!("Warning: Failed to remove worktree via git: {}", e);
    }

    if ctx.worktree_path.exists() {
        if let Err(e) = std::fs::remove_dir_all(&ctx.worktree_path) {
            eprintln!("Warning: Failed to remove worktree directory: {}", e);
        }
    }

    println!("Worktree removed.");

    Ok(())
}

pub async fn perform_cleanup(ctx: &CleanupContext) -> Result<()> {
    {
        let cleanup_lock = CLEANUP_RUNNING.get_or_init(|| Mutex::new(false));
        let mut already_running = cleanup_lock.lock().unwrap();
        if *already_running {
            eprintln!("Cleanup already in progress, skipping duplicate cleanup");
            return Ok(());
        }
        *already_running = true;
        drop(already_running);
    }

    let failure_count = ctx
        .auto_commit_failure_count
        .load(std::sync::atomic::Ordering::Relaxed);
    if failure_count > 0 {
        eprintln!(
            "Warning: {} auto-commit(s) failed during this session",
            failure_count
        );
    }

    // Pre-cleanup hooks
    if !ctx.config.hooks.pre_cleanup.is_empty() {
        println!("Running pre_cleanup hooks...");
        let hook_context = ctx.as_hook_context();
        // Determine working directory: prefer mount_path if it exists, otherwise worktree_path
        let working_dir = ctx
            .mount_path
            .as_ref()
            .filter(|p| p.exists())
            .unwrap_or(&ctx.worktree_path);
        if let Err(e) =
            hooks::run_hooks(&ctx.config.hooks.pre_cleanup, &hook_context, working_dir).await
        {
            eprintln!("Warning: pre_cleanup hook failed: {}", e);
            // Continue with cleanup even if hook fails
        }
    }

    if let Err(e) = remove_active_session(&ctx.main_repo_path, &ctx.branch_name) {
        tracing::warn!("Failed to remove session state: {}", e);
    }

    // Unmount FUSE (always required)
    if let Some(ref mount_path) = ctx.mount_path {
        perform_fuse_cleanup_with_output(mount_path);
    }

    // Squash commits (skip if cancelled)
    let squash_ctx = ctx.as_squash_context();
    if let Err(e) = squash_branch_commits(&squash_ctx) {
        tracing::warn!("Squash error: {}", e);
    }

    // Sync modified ignored files (may cancel)
    let sync_ctx = ctx.as_sync_context();
    let (was_cancelled, git_check_failed) = if !ctx.mutations.is_empty() {
        match sync::run_sync_flow(
            sync_ctx.mutations,
            sync_ctx.main_repo_path,
            sync_ctx.worktree_path,
            &sync_ctx.config.sync,
        ) {
            Ok(sync::SyncResult::Synced(count)) => {
                tracing::info!("Synced {} files to main repo", count);
                (false, false)
            }
            Ok(sync::SyncResult::Cancelled) => {
                tracing::info!("Sync cancelled by user");
                (true, false)
            }
            Ok(sync::SyncResult::Skipped) => {
                tracing::debug!("No files synced");
                (false, false)
            }
            Ok(sync::SyncResult::Partial(progress)) => {
                tracing::info!(
                    "Partial sync: {} synced, {} failed",
                    progress.synced_files.len(),
                    progress.failed_files.len()
                );
                for (path, error) in &progress.failed_files {
                    tracing::warn!("Failed to sync {}: {}", path.display(), error);
                }
                (false, false)
            }
            Ok(sync::SyncResult::GitCheckFailed) => {
                tracing::warn!("Git check-ignore failed during sync flow");
                (false, true)
            }
            Err(e) => {
                tracing::warn!("Sync flow error: {}", e);
                (false, false)
            }
        }
    } else {
        (false, false)
    };

    // Worktree deletion (skip if cancelled, extra confirmation if git check failed)
    let worktree_cleanup_ctx = ctx.as_worktree_cleanup_context();
    if let Err(e) = handle_worktree_cleanup(&worktree_cleanup_ctx, was_cancelled, git_check_failed)
    {
        tracing::warn!("Worktree cleanup error: {}", e);
    }

    // Post-cleanup hooks
    if !ctx.config.hooks.post_cleanup.is_empty() {
        println!("Running post_cleanup hooks...");
        let hook_context = ctx.as_hook_context();
        // Run in main repo path since worktree may be deleted
        if let Err(e) = hooks::run_hooks(
            &ctx.config.hooks.post_cleanup,
            &hook_context,
            &ctx.main_repo_path,
        )
        .await
        {
            eprintln!("Warning: post_cleanup hook failed: {}", e);
        }
    }

    let cleanup_lock = CLEANUP_RUNNING.get_or_init(|| Mutex::new(false));
    *cleanup_lock.lock().unwrap() = false;

    Ok(())
}

pub fn prompt_yes_no(prompt: &str, default: bool) -> Result<bool> {
    let stdin = io::stdin();
    let mut stdout = io::stdout();

    let default_hint = if default { "[Y/n]" } else { "[y/N]" };

    loop {
        print!("{} {}: ", prompt, default_hint);
        stdout
            .flush()
            .map_err(|e| TreebeardError::Config(format!("Failed to flush stdout: {}", e)))?;

        let mut input = String::new();
        stdin
            .read_line(&mut input)
            .map_err(|e| TreebeardError::Config(format!("Failed to read input: {}", e)))?;
        let choice = input.trim().to_lowercase();

        match choice.as_str() {
            "" => return Ok(default),
            "y" | "yes" => return Ok(true),
            "n" | "no" => return Ok(false),
            _ => println!("Please enter 'y' or 'n'."),
        }
    }
}

/// Prompts the user to confirm a destructive action by typing a specific word.
/// Used when extra confirmation is needed beyond a simple yes/no.
pub fn prompt_destructive_action(confirmation_word: &str) -> Result<bool> {
    let stdin = io::stdin();
    let mut stdout = io::stdout();

    print!("Type '{}' to confirm: ", confirmation_word);
    stdout
        .flush()
        .map_err(|e| TreebeardError::Config(format!("Failed to flush stdout: {}", e)))?;

    let mut input = String::new();
    stdin
        .read_line(&mut input)
        .map_err(|e| TreebeardError::Config(format!("Failed to read input: {}", e)))?;

    Ok(input.trim() == confirmation_word)
}
