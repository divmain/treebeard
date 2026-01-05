use std::sync::atomic::AtomicUsize;
use std::sync::Arc;

use crate::cleanup;
use crate::config::{Config, SandboxConfig};
use crate::error::{Result, TreebeardError};
use crate::git::GitRepo;
use crate::overlay::MutationTracker;
use crate::shell;
use tokio::task;

pub async fn run_shell_session(
    shell_path: &std::path::Path,
    branch_name: &str,
    command: Option<&[String]>,
    sandbox_config: Option<&SandboxConfig>,
    mount_path: Option<&std::path::Path>,
) -> Result<i32> {
    let is_test_mode = std::env::var("TREEBEARD_TEST_MODE").is_ok();

    if is_test_mode {
        println!("[Test mode: watcher running in background]");
        println!("Worktree: {}", shell_path.display());

        tokio::signal::ctrl_c()
            .await
            .map_err(|e| TreebeardError::Config(format!("Failed to wait for Ctrl+C: {}", e)))?;
        println!("\nTest mode terminated by signal");
        return Ok(0);
    }

    let mut child = match shell::spawn_subprocess_async(
        shell_path,
        branch_name,
        command,
        sandbox_config,
        mount_path,
    ) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Failed to spawn subprocess: {}", e);
            return Err(e);
        }
    };

    let status = child
        .wait()
        .await
        .map_err(|e| TreebeardError::Config(format!("Failed to wait for subprocess: {}", e)))?;

    shell::restore_foreground();

    let subprocess_name = match command {
        Some(cmd) if !cmd.is_empty() => &cmd[0],
        _ => "shell",
    };

    println!("\n{} exited with status: {}", subprocess_name, status);

    Ok(status.code().unwrap_or(1))
}

#[allow(clippy::too_many_arguments)]
pub fn build_cleanup_context(
    worktree_path: &std::path::Path,
    branch_name: &str,
    config: &Config,
    repo: &GitRepo,
    mutations: &MutationTracker,
    mount_path: Option<std::path::PathBuf>,
    worktree_repo: &GitRepo,
    base_commit: &str,
    failure_count: Arc<AtomicUsize>,
) -> cleanup::CleanupContext {
    let mutation_map = {
        let guard = mutations.read();
        guard.clone()
    };

    cleanup::CleanupContext {
        mount_path,
        worktree_path: worktree_path.to_path_buf(),
        main_repo_path: repo.workdir().to_path_buf(),
        repo: worktree_repo.clone(),
        branch_name: branch_name.to_string(),
        config: config.clone(),
        mutations: mutation_map,
        base_commit: base_commit.to_string(),
        auto_commit_failure_count: failure_count,
    }
}

#[allow(clippy::too_many_arguments)]
pub async fn run_shell_and_cleanup(
    shell_path: &std::path::Path,
    worktree_path: &std::path::Path,
    branch_name: &str,
    config: &Config,
    repo: &GitRepo,
    mutations: MutationTracker,
    mount_path: Option<std::path::PathBuf>,
    command: Option<&[String]>,
    worktree_repo: &GitRepo,
    base_commit: &str,
    failure_count: Arc<AtomicUsize>,
    watcher_handle: task::JoinHandle<()>,
) -> Result<i32> {
    tokio::spawn(async move {
        match watcher_handle.await {
            Err(e) if e.is_cancelled() => {
                tracing::debug!("Watcher task was cancelled");
            }
            Err(e) if e.is_panic() => {
                eprintln!("Warning: Watcher task panicked");
                eprintln!("Auto-commit may not be working properly");
                eprintln!(
                    "Your session will continue, but changes may not be automatically committed"
                );
            }
            Err(e) => {
                tracing::warn!("Watcher task exited with error: {}", e);
                eprintln!("Warning: Watcher task error");
                eprintln!("Auto-commit may not be working properly");
            }
            Ok(()) => {
                tracing::debug!("Watcher task completed normally");
            }
        }
    });

    let exit_code = run_shell_session(
        shell_path,
        branch_name,
        command,
        Some(&config.sandbox),
        mount_path.as_deref(),
    )
    .await?;

    let ctx = build_cleanup_context(
        worktree_path,
        branch_name,
        config,
        repo,
        &mutations,
        mount_path.clone(),
        worktree_repo,
        base_commit,
        failure_count,
    );

    let mount_path_for_cleanup = mount_path.clone();
    let critical_cleanup = move || -> Result<()> {
        if let Some(ref mp) = mount_path_for_cleanup {
            let result = crate::overlay::perform_fuse_cleanup(mp);
            if !result.unmount_succeeded || !result.directory_removed {
                tracing::warn!(
                    "FUSE cleanup incomplete - unmount: {}, dir_removed: {}",
                    result.unmount_succeeded,
                    result.directory_removed
                );
            }
        }
        Ok(())
    };

    cleanup::run_with_cancel_handler(
        async { cleanup::perform_cleanup(&ctx).await },
        critical_cleanup,
    )
    .await?;

    if exit_code == 0 {
        println!("Done!");
        println!("Branch '{}' is ready to push", branch_name);
    } else {
        println!("Subprocess exited with non-zero status ({})", exit_code);
        println!("Branch '{}' may have incomplete changes", branch_name);
    }

    Ok(exit_code)
}
