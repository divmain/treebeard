use crate::cleanup;
use crate::cli::validate_branch_name;
use crate::config::{get_mount_dir, get_worktree_dir};
use crate::error::Result;
use crate::git::GitRepo;
use crate::overlay;
use std::path::Path;

/// Print a warning to stderr if the given repo has uncommitted changes.
fn warn_if_uncommitted_changes(repo: &GitRepo, context: &str) {
    if let Ok(has_changes) = repo.has_uncommitted_changes() {
        if has_changes {
            eprintln!("Warning: {} has uncommitted changes", context);
            eprintln!();
        }
    }
}

/// Print a warning if the worktree at the given path has uncommitted changes.
fn warn_if_worktree_has_uncommitted_changes(worktree_path: &Path) {
    if let Ok(wt_repo) = GitRepo::from_path(worktree_path) {
        warn_if_uncommitted_changes(&wt_repo, "Worktree");
    }
}

pub fn cleanup_branch(
    branch_names: Vec<String>,
    all: bool,
    delete_branch: bool,
    yes: bool,
    force: bool,
    stale: bool,
) -> Result<()> {
    for branch_name in &branch_names {
        validate_branch_name(branch_name)?;
    }

    if stale {
        println!("Cleaning up stale FUSE mounts from crashed sessions...");
        overlay::cleanup_stale_mounts();
        println!("Stale mount cleanup complete.");
        if branch_names.is_empty() && !all {
            return Ok(());
        }
    }

    let repo = GitRepo::discover()?;
    let worktree_base_dir = get_worktree_dir()?;

    let branches_to_cleanup: Vec<String> = if all {
        let repo_worktree_base = worktree_base_dir.join(repo.repo_name());
        repo.list_worktrees()?
            .into_iter()
            .filter(|(_, path)| path.starts_with(&repo_worktree_base))
            .map(|(branch, _)| branch)
            .collect()
    } else if branch_names.is_empty() {
        return Err(crate::error::TreebeardError::Config(
            "No branch names provided. Use --all to clean up all worktrees.".to_string(),
        ));
    } else {
        branch_names
    };

    if branches_to_cleanup.is_empty() {
        println!("No treebeard worktrees found to clean up.");
        return Ok(());
    }

    println!(
        "Found {} worktree(s) to clean up.",
        branches_to_cleanup.len()
    );

    for branch_name in &branches_to_cleanup {
        println!("\n--- Cleaning up '{}' ---", branch_name);

        if let Err(e) = cleanup_single_branch(&repo, branch_name, delete_branch, yes, force) {
            eprintln!("Error cleaning up '{}': {}", branch_name, e);
        }
    }

    println!("\nCleanup complete.");
    Ok(())
}

fn cleanup_fuse_mount(mount_path: &Path) {
    if mount_path.exists() {
        println!("Unmounting FUSE filesystem...");
        let result = overlay::perform_fuse_cleanup(mount_path);
        if result.unmount_succeeded {
            println!("FUSE filesystem unmounted.");
        } else {
            eprintln!("Warning: Failed to unmount (may already be unmounted).");
        }
    }
}

fn delete_worktree_directory(repo: &GitRepo, worktree_path: &Path, force: bool) {
    println!("Removing worktree...");
    if let Err(e) = repo.remove_worktree(worktree_path, force) {
        eprintln!("Warning: Failed to remove worktree via git: {}", e);
    }

    if worktree_path.exists() {
        if let Err(e) = std::fs::remove_dir_all(worktree_path) {
            eprintln!("Warning: Failed to remove worktree directory: {}", e);
        }
    }

    println!("Worktree removed.");
}

fn prompt_and_delete_worktree(worktree_path: &Path, yes: bool) -> bool {
    if yes {
        return true;
    }
    warn_if_worktree_has_uncommitted_changes(worktree_path);
    cleanup::prompt_yes_no("Delete worktree directory?", false).unwrap_or(false)
}

fn prompt_and_delete_branch(
    repo: &GitRepo,
    branch_name: &str,
    delete_branch: bool,
    yes: bool,
    force: bool,
) {
    let should_delete_branch = if yes {
        delete_branch
    } else {
        warn_if_uncommitted_changes(repo, &format!("Branch '{}'", branch_name));
        cleanup::prompt_yes_no(&format!("Delete branch '{}'?", branch_name), false).unwrap_or(false)
    };

    if should_delete_branch {
        println!("Deleting branch...");
        match repo.delete_branch(branch_name, force) {
            Ok(_) => println!("Branch '{}' deleted.", branch_name),
            Err(e) => eprintln!("Warning: Failed to delete branch: {}", e),
        }
    } else {
        println!("Branch '{}' preserved.", branch_name);
    }
}

fn cleanup_single_branch(
    repo: &GitRepo,
    branch_name: &str,
    delete_branch: bool,
    yes: bool,
    force: bool,
) -> Result<()> {
    if !repo.worktree_exists(branch_name) {
        println!("Worktree for '{}' does not exist.", branch_name);
        return Ok(());
    }

    let worktree_path = repo.get_worktree_path(branch_name)?;
    println!("Worktree path: {}", worktree_path.display());

    let mount_base_dir = get_mount_dir()?;
    let mount_path = mount_base_dir.join(repo.repo_name()).join(branch_name);

    cleanup_fuse_mount(&mount_path);

    if !prompt_and_delete_worktree(&worktree_path, yes) {
        println!("Skipping '{}'.", branch_name);
        return Ok(());
    }

    delete_worktree_directory(repo, &worktree_path, force);
    prompt_and_delete_branch(repo, branch_name, delete_branch, yes, force);

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cleanup_stale_only() {
        std::env::set_var("TREEBEARD_TEST_MODE", "1");
        let result = cleanup_branch(vec![], false, false, true, false, true);
        std::env::remove_var("TREEBEARD_TEST_MODE");
        assert!(result.is_ok());
    }
}
