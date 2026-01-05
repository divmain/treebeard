use crate::cleanup;
use crate::config::get_worktree_dir;
use crate::error::{Result, TreebeardError};
use std::io::IsTerminal;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;

const MAX_ATTEMPTS: u32 = 3;
const INITIAL_BACKOFF_MS: u64 = 100;

fn is_transient_git_error(error: &TreebeardError) -> bool {
    match error {
        TreebeardError::Config(msg) | TreebeardError::Git(msg) => {
            let lower = msg.to_lowercase();
            lower.contains("index.lock")
                || lower.contains("unable to create")
                || lower.contains("file exists")
                || lower.contains("permission denied")
                || lower.contains("could not lock")
        }
        _ => false,
    }
}

fn with_retry<T, F>(operation: F, description: &str) -> Result<T>
where
    F: Fn() -> Result<T>,
{
    let mut last_error = None;

    for attempt in 0..MAX_ATTEMPTS {
        match operation() {
            Ok(result) => return Ok(result),
            Err(e) => {
                last_error = Some(e);

                if attempt < MAX_ATTEMPTS - 1
                    && is_transient_git_error(last_error.as_ref().unwrap())
                {
                    let backoff = INITIAL_BACKOFF_MS * 2_u64.pow(attempt);
                    tracing::warn!(
                        "{} failed (attempt {}/{}), retrying in {}ms...",
                        description,
                        attempt + 1,
                        MAX_ATTEMPTS,
                        backoff
                    );
                    thread::sleep(Duration::from_millis(backoff));
                } else {
                    break;
                }
            }
        }
    }

    Err(last_error.unwrap())
}

fn run_git(workdir: &Path, args: &[&str], error_prefix: &str) -> Result<std::process::Output> {
    let output = std::process::Command::new("git")
        .args(args)
        .current_dir(workdir)
        .output()
        .map_err(|e| {
            TreebeardError::Config(format!(
                "Failed to execute 'git {}' in {}: {}",
                args.join(" "),
                workdir.display(),
                e
            ))
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(TreebeardError::Git(format!("{}: {}", error_prefix, stderr)));
    }

    Ok(output)
}

#[derive(Clone)]
pub struct GitRepo {
    pub repo_name: String,
    pub workdir: PathBuf,
    pub git_dir: PathBuf,
}

impl GitRepo {
    pub fn discover() -> Result<Self> {
        let current_dir = std::env::current_dir().map_err(|e| {
            TreebeardError::Config(format!("Failed to get current directory: {}", e))
        })?;
        Self::from_path(&current_dir)
    }

    pub fn from_path(start_dir: &Path) -> Result<Self> {
        let (git_dir, workdir) = find_git_dir(start_dir)?;

        let repo_name = workdir
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or_else(|| TreebeardError::Config("Could not determine repo name".to_string()))?
            .to_string();

        Ok(GitRepo {
            repo_name,
            workdir,
            git_dir,
        })
    }

    pub fn workdir(&self) -> &Path {
        &self.workdir
    }

    pub fn repo_name(&self) -> &str {
        &self.repo_name
    }

    pub fn branch_exists(&self, branch_name: &str) -> bool {
        let branch_ref = format!("refs/heads/{}", branch_name);

        std::process::Command::new("git")
            .args(["show-ref", "--verify", "--quiet", &branch_ref])
            .current_dir(&self.workdir)
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false)
    }

    pub fn create_branch(&self, branch_name: &str) -> Result<()> {
        if self.branch_exists(branch_name) {
            return Err(TreebeardError::BranchAlreadyExists(branch_name.to_string()));
        }

        run_git(
            &self.workdir,
            &["branch", branch_name],
            "Failed to create branch",
        )?;

        Ok(())
    }

    pub fn worktree_exists(&self, branch_name: &str) -> bool {
        let worktrees_dir = self.git_dir.join("worktrees").join(branch_name);
        worktrees_dir.exists()
    }

    pub fn create_worktree(&self, branch_name: &str, worktree_path: &Path) -> Result<()> {
        if self.worktree_exists(branch_name) {
            return Err(TreebeardError::WorktreeAlreadyExists(
                branch_name.to_string(),
            ));
        }

        let parent_dir = worktree_path.parent().ok_or_else(|| {
            TreebeardError::Config(format!(
                "Invalid worktree path: {}",
                worktree_path.display()
            ))
        })?;

        std::fs::create_dir_all(parent_dir).map_err(|e| {
            TreebeardError::Config(format!(
                "Failed to create parent directory {}: {}",
                parent_dir.display(),
                e
            ))
        })?;

        let worktree_path_str = worktree_path.to_string_lossy();
        run_git(
            &self.workdir,
            &["worktree", "add", &worktree_path_str, branch_name],
            "Failed to create worktree",
        )?;

        Ok(())
    }

    pub fn get_worktree_path(&self, branch_name: &str) -> Result<PathBuf> {
        let worktrees_dir = self.git_dir.join("worktrees").join(branch_name);

        if !worktrees_dir.exists() {
            return Err(TreebeardError::WorktreeNotFound(branch_name.to_string()));
        }

        let gitdir_file = worktrees_dir.join("gitdir");
        let gitdir = std::fs::read_to_string(&gitdir_file).map_err(|e| {
            TreebeardError::Config(format!(
                "Failed to read gitdir file {}: {}",
                gitdir_file.display(),
                e
            ))
        })?;

        let gitdir_path = PathBuf::from(gitdir.trim());

        if gitdir_path.parent().is_none() {
            return Err(TreebeardError::Config(format!(
                "Invalid gitdir in worktree: {}",
                gitdir
            )));
        }

        Ok(gitdir_path.parent().unwrap().to_path_buf())
    }

    fn stage_all(&self) -> Result<()> {
        let stage_output = std::process::Command::new("git")
            .args(["add", "-A"])
            .current_dir(&self.workdir)
            .output()
            .map_err(|e| {
                TreebeardError::Config(format!(
                    "Failed to execute 'git add' in {}: {}",
                    self.workdir.display(),
                    e
                ))
            })?;

        if !stage_output.status.success() {
            let stderr = String::from_utf8_lossy(&stage_output.stderr);
            return Err(TreebeardError::Git(format!(
                "Failed to stage changes: {}",
                stderr.trim()
            )));
        }

        Ok(())
    }

    pub fn stage_and_commit(&self, message: &str) -> Result<()> {
        tracing::debug!("stage_and_commit() in directory: {:?}", self.workdir);

        with_retry(
            || {
                self.stage_all()?;

                let output = std::process::Command::new("git")
                    .args(["diff", "--cached"])
                    .current_dir(&self.workdir)
                    .output()
                    .map_err(|e| {
                        TreebeardError::Config(format!(
                            "Failed to execute 'git diff --cached' in {}: {}",
                            self.workdir.display(),
                            e
                        ))
                    })?;

                if output.stdout.is_empty() {
                    return Ok(());
                }

                run_git(
                    &self.workdir,
                    &["commit", "-m", message],
                    "Failed to commit",
                )?;

                Ok(())
            },
            "stage_and_commit",
        )
    }

    pub fn get_head(&self) -> Result<String> {
        let output = run_git(&self.workdir, &["rev-parse", "HEAD"], "Failed to get HEAD")?;
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    }

    /// Stage all changes and return the staged diff.
    ///
    /// This is useful for generating commit messages from the diff.
    /// Returns Ok(None) if there are no changes to commit.
    pub fn stage_changes(&self) -> Result<Option<String>> {
        tracing::debug!("stage_changes() in directory: {:?}", self.workdir);

        self.stage_all()?;

        let output = std::process::Command::new("git")
            .args(["diff", "--cached"])
            .current_dir(&self.workdir)
            .output()
            .map_err(|e| {
                TreebeardError::Config(format!(
                    "Failed to execute 'git diff --cached' in {}: {}",
                    self.workdir.display(),
                    e
                ))
            })?;

        if output.stdout.is_empty() {
            return Ok(None);
        }

        Ok(Some(String::from_utf8_lossy(&output.stdout).to_string()))
    }

    /// Commit staged changes with the given message.
    ///
    /// This assumes changes have already been staged with `stage_changes()`.
    pub fn commit_staged(&self, message: &str) -> Result<()> {
        tracing::debug!("commit_staged() in directory: {:?}", self.workdir);

        with_retry(
            || {
                run_git(
                    &self.workdir,
                    &["commit", "-m", message],
                    "Failed to commit",
                )?;
                Ok(())
            },
            "commit_staged",
        )
    }

    pub fn squash_commits(&self, branch_name: &str, base_message: &str) -> Result<()> {
        let original_head = self.get_head()?;

        let output = std::process::Command::new("git")
            .args(["reset", "--soft", &format!("refs/heads/{}~1", branch_name)])
            .current_dir(&self.workdir)
            .output()
            .map_err(|e| {
                TreebeardError::Config(format!(
                    "Failed to execute 'git reset --soft' in {}: {}",
                    self.workdir.display(),
                    e
                ))
            })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let error_msg = if stderr.trim().is_empty() {
                String::from_utf8_lossy(&output.stdout).to_string()
            } else {
                stderr.to_string()
            };

            if error_msg.contains("ambiguous argument") || error_msg.contains("unknown revision") {
                tracing::debug!("Branch has only one commit, skipping squash (nothing to squash)");
                return Ok(());
            }

            return Err(TreebeardError::Git(format!(
                "Failed to reset: {}",
                error_msg.trim()
            )));
        }

        let commit_output = std::process::Command::new("git")
            .args(["commit", "-m", base_message])
            .current_dir(&self.workdir)
            .output()
            .map_err(|e| {
                TreebeardError::Config(format!(
                    "Failed to execute 'git commit' in {}: {}",
                    self.workdir.display(),
                    e
                ))
            })?;

        if !commit_output.status.success() {
            let rollback_result = std::process::Command::new("git")
                .args(["reset", "--hard", &original_head])
                .current_dir(&self.workdir)
                .output();

            let stderr = String::from_utf8_lossy(&commit_output.stderr);
            let error_msg = if stderr.trim().is_empty() {
                String::from_utf8_lossy(&commit_output.stdout).to_string()
            } else {
                stderr.to_string()
            };

            let original_error = TreebeardError::Git(format!(
                "Failed to create squash commit: {}",
                error_msg.trim()
            ));

            match rollback_result {
                Ok(ref rollback) if rollback.status.success() => {}
                Ok(ref rollback) => {
                    let rollback_stderr = String::from_utf8_lossy(&rollback.stderr);
                    let rollback_error = if rollback_stderr.trim().is_empty() {
                        String::from_utf8_lossy(&rollback.stdout).to_string()
                    } else {
                        rollback_stderr.to_string()
                    };
                    return Err(TreebeardError::Git(format!(
                        "{}\n\nCRITICAL: Rollback also failed: {}. Repository may be in inconsistent state!",
                        original_error,
                        rollback_error.trim()
                    )));
                }
                Err(e) => {
                    return Err(TreebeardError::Git(format!(
                        "{}\n\nCRITICAL: Failed to execute rollback: {}. Repository may be in inconsistent state!",
                        original_error, e
                    )));
                }
            }

            return Err(original_error);
        }

        Ok(())
    }

    /// Remove a worktree by its path.
    ///
    /// The `git worktree remove` command requires the path to the worktree directory,
    /// not the branch name. This function accepts an absolute path to the worktree.
    ///
    /// If the worktree directory has already been deleted, this function will
    /// fall back to `git worktree prune` to clean up stale worktree references.
    pub fn remove_worktree(&self, worktree_path: &Path, force: bool) -> Result<()> {
        let path_str = worktree_path.to_string_lossy();

        let mut args = vec!["worktree", "remove"];
        if force {
            args.push("--force");
        }
        args.push(&path_str);

        let output = std::process::Command::new("git")
            .args(&args)
            .current_dir(&self.workdir)
            .output()
            .map_err(|e| {
                TreebeardError::Config(format!(
                    "Failed to execute 'git worktree remove' in {}: {}",
                    self.workdir.display(),
                    e
                ))
            })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let stderr_lower = stderr.to_lowercase();

            // If the worktree directory is already gone (e.g., manually deleted or from
            // a previous incomplete cleanup), `git worktree remove` fails. Prune cleans
            // up the stale .git/worktrees/<name> references instead.
            if stderr_lower.contains("is not a working tree")
                || stderr_lower.contains("no such file or directory")
                || stderr_lower.contains("does not exist")
            {
                tracing::debug!("Worktree directory missing, falling back to git worktree prune");
                return self.prune_worktrees();
            }

            return Err(TreebeardError::Git(format!(
                "Failed to remove worktree: {}",
                stderr.trim()
            )));
        }

        Ok(())
    }

    /// Prune stale worktree references.
    ///
    /// This cleans up `.git/worktrees/<name>` entries for worktrees whose
    /// directories have been deleted.
    pub fn prune_worktrees(&self) -> Result<()> {
        run_git(
            &self.workdir,
            &["worktree", "prune"],
            "Failed to prune worktrees",
        )?;
        Ok(())
    }

    pub fn delete_branch(&self, branch_name: &str, force: bool) -> Result<()> {
        let flag = if force { "-D" } else { "-d" };
        run_git(
            &self.workdir,
            &["branch", flag, branch_name],
            "Failed to delete branch",
        )?;
        Ok(())
    }

    pub fn list_worktrees(&self) -> Result<Vec<(String, PathBuf)>> {
        let output = std::process::Command::new("git")
            .args(["worktree", "list", "--format=%(worktree)%%%(refname:short)"])
            .current_dir(&self.workdir)
            .output()
            .map_err(|e| {
                TreebeardError::Config(format!(
                    "Failed to execute 'git worktree list' in {}: {}",
                    self.workdir.display(),
                    e
                ))
            })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(TreebeardError::Git(format!(
                "Failed to list worktrees: {}",
                stderr.trim()
            )));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut worktrees = Vec::new();

        for line in stdout.lines() {
            if line.is_empty() {
                continue;
            }
            let parts: Vec<&str> = line.split("%%%").collect();
            if parts.len() == 2 {
                let path = PathBuf::from(parts[0]);
                let branch = parts[1]
                    .strip_prefix("refs/heads/")
                    .unwrap_or(parts[1])
                    .to_string();
                worktrees.push((branch, path));
            }
        }

        Ok(worktrees)
    }

    pub fn has_uncommitted_changes(&self) -> Result<bool> {
        let output = run_git(
            &self.workdir,
            &["status", "--porcelain"],
            "Failed to check for uncommitted changes",
        )?;
        Ok(!output.stdout.is_empty())
    }

    pub fn get_dirty_files_count(&self) -> Result<usize> {
        let output = run_git(
            &self.workdir,
            &["status", "--porcelain"],
            "Failed to check for uncommitted changes",
        )?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        Ok(stdout.lines().filter(|line| !line.is_empty()).count())
    }

    pub fn get_commit_count_since(&self, branch_name: &str, base_commit: &str) -> Result<usize> {
        let range = format!("{}..{}", base_commit, branch_name);
        let output = run_git(
            &self.workdir,
            &["rev-list", "--count", &range],
            "Failed to count commits",
        )?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        let count = stdout.trim().parse().map_err(|_| {
            TreebeardError::Git(format!("Failed to parse commit count: {}", stdout.trim()))
        })?;
        Ok(count)
    }

    /// Stash uncommitted changes with a message.
    ///
    /// This pushes all uncommitted changes (staged, unstaged, and optionally untracked)
    /// to the stash with the given message.
    ///
    /// # Arguments
    /// * `message` - The stash message
    /// * `include_untracked` - Whether to include untracked files in the stash
    pub fn stash_push(&self, message: &str, include_untracked: bool) -> Result<()> {
        let mut args = vec!["stash", "push", "-m", message];
        if include_untracked {
            args.push("--include-untracked");
        }

        run_git(&self.workdir, &args, "Failed to stash changes")?;
        Ok(())
    }
}

/// Result of setting up the git environment for a branch
pub struct GitEnvironmentSetup {
    pub repo: GitRepo,
    pub worktree_path: PathBuf,
    pub main_repo_path: PathBuf,
    pub base_commit: String,
    /// If changes were auto-stashed, contains the stash message
    pub auto_stash_message: Option<String>,
}

/// Offer to stash uncommitted changes if present.
///
/// Prompts the user to stash their changes before creating a worktree.
/// Returns the stash message if changes were stashed, or None if no changes
/// were present or the user declined.
pub fn offer_stash_if_needed(repo: &GitRepo) -> Result<Option<String>> {
    if !repo.has_uncommitted_changes()? {
        return Ok(None);
    }

    // In test mode or non-interactive mode, skip the prompt and auto-stash
    let is_test_mode = std::env::var("TREEBEARD_TEST_MODE").is_ok();
    let is_non_interactive = !std::io::stdin().is_terminal();

    let should_stash = if is_test_mode || is_non_interactive {
        // Auto-stash in non-interactive contexts
        true
    } else {
        println!("You have uncommitted changes in this repository.");
        cleanup::prompt_yes_no("Stash changes before creating worktree?", true)?
    };

    if !should_stash {
        return Ok(None);
    }

    let timestamp = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%S");
    let message = format!("treebeard-auto-stash-{}", timestamp);

    repo.stash_push(&message, true)?;

    Ok(Some(message))
}

/// Sets up the git environment: discovers repo, creates branch and worktree
pub fn setup_git_environment(branch_name: &str) -> Result<GitEnvironmentSetup> {
    let repo = GitRepo::discover()?;

    println!("Repository: {}", repo.workdir().display());

    // Offer to stash uncommitted changes before creating worktree
    let auto_stash_message = offer_stash_if_needed(&repo)?;
    if let Some(ref stash_message) = auto_stash_message {
        println!("Stashed uncommitted changes: {}", stash_message);
    }

    println!("Creating branch: {}", branch_name);

    if repo.branch_exists(branch_name) {
        eprintln!(
            "Warning: Branch '{}' already exists, continuing with existing branch",
            branch_name
        );
    } else {
        repo.create_branch(branch_name)?;
        println!("Created branch: {}", branch_name);
    }

    let worktree_base_dir = get_worktree_dir()?;
    let repo_name = repo.repo_name();
    let worktree_path = worktree_base_dir.join(repo_name).join(branch_name);

    if repo.worktree_exists(branch_name) {
        println!(
            "Worktree for '{}' already exists at {}",
            branch_name,
            worktree_path.display()
        );
    } else {
        if let Err(e) = repo.create_worktree(branch_name, &worktree_path) {
            eprintln!("Failed to create worktree: {}", e);
            eprintln!("Rolling back branch creation...");
            let _ = repo.delete_branch(branch_name, true);
            return Err(e);
        }
        println!("Created worktree at: {}", worktree_path.display());
    }

    let main_repo_path = repo.workdir().to_path_buf();

    let worktree_repo = GitRepo::from_path(&worktree_path)?;
    let base_commit = worktree_repo.get_head()?;

    Ok(GitEnvironmentSetup {
        repo,
        worktree_path,
        main_repo_path,
        base_commit,
        auto_stash_message,
    })
}

fn find_git_dir(start_dir: &Path) -> Result<(PathBuf, PathBuf)> {
    let mut current = start_dir;

    loop {
        let git_dir_candidate = current.join(".git");

        if git_dir_candidate.is_dir() {
            return Ok((git_dir_candidate, current.to_path_buf()));
        }

        let git_file_path = current.join(".git");

        if git_file_path.is_file() {
            let content = std::fs::read_to_string(&git_file_path).map_err(|e| {
                TreebeardError::Config(format!(
                    "Failed to read .git file {}: {}",
                    git_file_path.display(),
                    e
                ))
            })?;

            if let Some(gitdir) = content.strip_prefix("gitdir: ") {
                let gitdir = gitdir.trim();
                // Worktree .git files can contain absolute or relative paths.
                // Relative paths are relative to the worktree directory.
                let absolute_path = if gitdir.starts_with("/") {
                    PathBuf::from(gitdir)
                } else {
                    current.join(gitdir)
                };

                return Ok((absolute_path, current.to_path_buf()));
            }
        }

        match current.parent() {
            Some(parent) => current = parent,
            None => break,
        }
    }

    Err(TreebeardError::NotAGitRepository(start_dir.to_path_buf()))
}
