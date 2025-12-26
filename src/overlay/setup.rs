use crate::config::{get_mount_dir, Config};
use crate::error::Result;
use crate::git::GitRepo;
use crate::overlay::mount::mount_fuse;
use crate::watcher;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicUsize;
use std::sync::Arc;
use tokio::task;

use crate::overlay::types::MutationTracker;

/// Result of setting up the overlay filesystem and watcher
pub struct OverlaySetup {
    pub mutations: MutationTracker,
    pub mount_path: PathBuf,
    pub worktree_repo: GitRepo,
    pub watcher_handle: task::JoinHandle<()>,
}

/// Sets up the FUSE overlay filesystem and spawns the file watcher
pub fn setup_overlay_and_watcher(
    branch_name: &str,
    worktree_path: &Path,
    main_repo_path: &Path,
    config: &Config,
    failure_count: Arc<AtomicUsize>,
) -> Result<OverlaySetup> {
    let mount_base_dir = get_mount_dir()?;
    let repo_name = worktree_path
        .parent()
        .and_then(|p| p.file_name())
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "unknown".to_string());
    let mount_path = mount_base_dir.join(&repo_name).join(branch_name);

    println!("Mounting overlay filesystem at: {}", mount_path.display());

    let (mutations, mutation_rx) = match mount_fuse(
        &mount_path,
        worktree_path,
        main_repo_path,
        config.fuse_ttl_secs,
        config.paths.passthrough.clone(),
    ) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("Failed to mount FUSE filesystem: {}", e);
            eprintln!("Make sure macFUSE is installed: brew install --cask macfuse");
            return Err(e);
        }
    };

    tracing::debug!(
        "Creating GitRepo for worktree at: {}",
        worktree_path.display()
    );

    let worktree_repo = match GitRepo::from_path(worktree_path) {
        Ok(r) => r,
        Err(e) => {
            tracing::error!("Failed to create GitRepo for worktree: {}", e);
            return Err(e);
        }
    };

    tracing::debug!(
        "Watcher will use worktree workdir: {}",
        worktree_repo.workdir().display()
    );
    tracing::debug!("Spawning watcher for worktree: {}", worktree_path.display());

    let debounce_ms = config.auto_commit_timing.get_debounce_ms();
    let worktree_repo_for_watcher = worktree_repo.clone();
    let failure_count_for_watcher = failure_count.clone();

    // Use the hooks-aware watcher if a commit_message hook is configured
    let watcher_handle: task::JoinHandle<()> = if config.hooks.commit_message.is_some() {
        let commit_config = watcher::CommitConfig::new(
            &config.commit.auto_commit_message,
            &config.hooks,
            branch_name,
            &mount_path,
            worktree_path,
            main_repo_path,
        );
        tokio::spawn(async move {
            if let Err(e) = watcher::watch_and_commit_with_hooks(
                mutation_rx,
                &worktree_repo_for_watcher,
                debounce_ms,
                commit_config,
                failure_count_for_watcher,
            )
            .await
            {
                tracing::warn!("Watcher task ended: {}", e);
            }
        })
    } else {
        let auto_commit_message = config.commit.auto_commit_message.clone();
        tokio::spawn(async move {
            if let Err(e) = watcher::watch_and_commit(
                mutation_rx,
                &worktree_repo_for_watcher,
                debounce_ms,
                &auto_commit_message,
                failure_count_for_watcher,
            )
            .await
            {
                tracing::warn!("Watcher task ended: {}", e);
            }
        })
    };

    println!("FUSE filesystem mounted successfully");

    Ok(OverlaySetup {
        mutations,
        mount_path,
        worktree_repo,
        watcher_handle,
    })
}
