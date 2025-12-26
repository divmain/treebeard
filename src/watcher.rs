use crate::config::HooksConfig;
use crate::error::Result;
use crate::git::GitRepo;
use crate::hooks::{self, HookContext};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::mpsc::UnboundedReceiver;

/// Configuration for generating commit messages.
#[derive(Clone)]
pub struct CommitConfig {
    /// Default commit message to use when no hook is configured
    pub default_message: String,
    /// Optional hook command to generate commit message
    pub commit_message_hook: Option<String>,
    /// Branch name for template expansion
    pub branch_name: String,
    /// Mount path for template expansion
    pub mount_path: PathBuf,
    /// Worktree path for template expansion and hook execution
    pub worktree_path: PathBuf,
    /// Main repository path for template expansion
    pub repo_path: PathBuf,
}

impl CommitConfig {
    /// Create a CommitConfig from hooks and other parameters.
    pub fn new(
        default_message: &str,
        hooks: &HooksConfig,
        branch_name: &str,
        mount_path: &Path,
        worktree_path: &Path,
        repo_path: &Path,
    ) -> Self {
        Self {
            default_message: default_message.to_string(),
            commit_message_hook: hooks.commit_message.clone(),
            branch_name: branch_name.to_string(),
            mount_path: mount_path.to_path_buf(),
            worktree_path: worktree_path.to_path_buf(),
            repo_path: repo_path.to_path_buf(),
        }
    }

    /// Generate a commit message, using the hook if configured.
    ///
    /// If a commit_message hook is configured, it runs the hook with the diff
    /// and uses its output as the commit message. Otherwise, returns the default message.
    async fn generate_message(&self, diff: &str) -> String {
        if let Some(ref hook) = self.commit_message_hook {
            let context = HookContext::with_diff(
                &self.branch_name,
                &self.mount_path,
                &self.worktree_path,
                &self.repo_path,
                diff.to_string(),
            );

            match hooks::run_commit_message_hook(hook, &context, &self.worktree_path).await {
                Ok(Some(message)) => {
                    tracing::info!("Using commit message from hook");
                    return message;
                }
                Ok(None) => {
                    tracing::warn!("Commit message hook produced empty output, using default");
                }
                Err(e) => {
                    tracing::warn!("Commit message hook failed: {}, using default", e);
                }
            }
        }

        self.default_message.clone()
    }
}

/// Watch for mutation events and auto-commit changes.
///
/// Receives mutation signals from the FUSE filesystem and commits changes
/// after a debounce period of inactivity.
pub async fn watch_and_commit(
    mutation_rx: UnboundedReceiver<PathBuf>,
    repo: &GitRepo,
    debounce_ms: u64,
    commit_message: &str,
    failure_count: Arc<AtomicUsize>,
) -> Result<()> {
    tracing::debug!(
        "watch_and_commit() started, debounce={}ms, repo.workdir={:?}",
        debounce_ms,
        repo.workdir()
    );

    watch_and_commit_internal(
        mutation_rx,
        repo,
        debounce_ms,
        failure_count,
        CommitMode::Simple(commit_message.to_string()),
    )
    .await
}

/// Watch for mutation events and auto-commit changes with hook support.
///
/// Similar to `watch_and_commit`, but supports the commit_message hook
/// for generating commit messages dynamically.
pub async fn watch_and_commit_with_hooks(
    mutation_rx: UnboundedReceiver<PathBuf>,
    repo: &GitRepo,
    debounce_ms: u64,
    commit_config: CommitConfig,
    failure_count: Arc<AtomicUsize>,
) -> Result<()> {
    tracing::debug!(
        "watch_and_commit_with_hooks() started, debounce={}ms, repo.workdir={:?}",
        debounce_ms,
        repo.workdir()
    );

    watch_and_commit_internal(
        mutation_rx,
        repo,
        debounce_ms,
        failure_count,
        CommitMode::WithHooks(commit_config),
    )
    .await
}

/// Internal implementation for watching and committing changes.
async fn watch_and_commit_internal(
    mut mutation_rx: UnboundedReceiver<PathBuf>,
    repo: &GitRepo,
    debounce_ms: u64,
    failure_count: Arc<AtomicUsize>,
    commit_mode: CommitMode,
) -> Result<()> {
    let debounce_duration = Duration::from_millis(debounce_ms);
    let mut pending_paths: HashSet<PathBuf> = HashSet::new();
    let mut last_event: Option<Instant> = None;
    let repo = Arc::new(repo.clone());

    loop {
        let timeout = match last_event {
            Some(instant) => {
                let elapsed = instant.elapsed();
                if elapsed >= debounce_duration {
                    Duration::ZERO
                } else {
                    debounce_duration - elapsed
                }
            }
            // When no events are pending, use a very long timeout. This effectively
            // means "wait forever for the next event" without busy-looping.
            None => Duration::from_secs(86400),
        };

        tokio::select! {
            result = mutation_rx.recv() => {
                match result {
                    Some(path) => {
                        tracing::debug!("Received mutation signal for: {:?}", path);
                        pending_paths.insert(path);
                        last_event = Some(Instant::now());
                    }
                    None => {
                        // Channel closed, FUSE filesystem shutting down
                        tracing::debug!("Mutation channel closed, performing final commit if needed");
                        if !pending_paths.is_empty() {
                            match &commit_mode {
                                CommitMode::Simple(msg) => do_commit(repo.clone(), msg, &pending_paths, failure_count.clone()).await,
                                CommitMode::WithHooks(config) => do_commit_with_hooks(repo.clone(), config, &pending_paths, failure_count.clone()).await,
                            }
                        }
                        break;
                    }
                }
            }
            // Guard: only enable the sleep branch when we have pending events.
            // Without this, we'd sleep for 86400 seconds even when there's nothing to commit.
            _ = tokio::time::sleep(timeout), if last_event.is_some() => {
                // Debounce timer expired
                if !pending_paths.is_empty() {
                    match &commit_mode {
                        CommitMode::Simple(msg) => do_commit(repo.clone(), msg, &pending_paths, failure_count.clone()).await,
                        CommitMode::WithHooks(config) => do_commit_with_hooks(repo.clone(), config, &pending_paths, failure_count.clone()).await,
                    }
                    pending_paths.clear();
                    last_event = None;
                }
            }
        }
    }

    Ok(())
}

enum CommitMode {
    Simple(String),
    WithHooks(CommitConfig),
}

async fn do_commit(
    repo: Arc<GitRepo>,
    commit_message: &str,
    paths: &HashSet<PathBuf>,
    failure_count: Arc<AtomicUsize>,
) {
    // paths is logged for debugging but not passed to stage_and_commit because
    // git add -A stages all changes, which is simpler and handles edge cases
    // like files created then deleted within the debounce window.
    tracing::debug!(
        "Debounce timer expired, committing {} changed paths",
        paths.len()
    );
    // Auto-commit is best-effort. Failures are logged but don't interrupt the
    // user's work; they can manually commit if needed.
    let message = commit_message.to_string();
    let count = failure_count.clone();
    tokio::task::spawn_blocking(move || {
        if let Err(e) = repo.stage_and_commit(&message) {
            tracing::warn!("Auto-commit failed: {}", e);
            count.fetch_add(1, Ordering::Relaxed);
        }
    })
    .await
    .ok();
}

async fn do_commit_with_hooks(
    repo: Arc<GitRepo>,
    commit_config: &CommitConfig,
    paths: &HashSet<PathBuf>,
    failure_count: Arc<AtomicUsize>,
) {
    tracing::debug!(
        "Debounce timer expired, committing {} changed paths (with hooks)",
        paths.len()
    );

    let config = commit_config.clone();
    let count = failure_count.clone();

    // Stage changes and get the diff in a blocking task
    let stage_result = tokio::task::spawn_blocking({
        let repo = repo.clone();
        move || repo.stage_changes()
    })
    .await;

    let diff = match stage_result {
        Ok(Ok(Some(diff))) => diff,
        Ok(Ok(None)) => {
            tracing::debug!("No changes to commit");
            return;
        }
        Ok(Err(e)) => {
            tracing::warn!("Failed to stage changes: {}", e);
            count.fetch_add(1, Ordering::Relaxed);
            return;
        }
        Err(e) => {
            tracing::warn!("Task panicked while staging: {}", e);
            count.fetch_add(1, Ordering::Relaxed);
            return;
        }
    };

    // Generate commit message (may use hook)
    let message = config.generate_message(&diff).await;

    // Commit staged changes in a blocking task
    let commit_result = tokio::task::spawn_blocking({
        let repo = repo.clone();
        let message = message.clone();
        move || repo.commit_staged(&message)
    })
    .await;

    match commit_result {
        Ok(Ok(())) => {
            tracing::debug!("Auto-commit succeeded");
        }
        Ok(Err(e)) => {
            tracing::warn!("Auto-commit failed: {}", e);
            count.fetch_add(1, Ordering::Relaxed);
        }
        Err(e) => {
            tracing::warn!("Task panicked while committing: {}", e);
            count.fetch_add(1, Ordering::Relaxed);
        }
    }
}
