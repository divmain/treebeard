use crate::shared::common::create_test_repo;

use std::process::Command;
use treebeard::git::GitRepo;

#[test]
fn test_worktree_creation() {
    let (_temp_dir, repo_path) = create_test_repo();
    let repo = GitRepo::from_path(&repo_path).expect("Failed to discover repo");

    let worktree_path = repo_path.join(".treebeard-test-worktree");

    repo.create_branch("test-branch")
        .expect("Failed to create branch");

    repo.create_worktree("test-branch", &worktree_path)
        .expect("Failed to create worktree");

    assert!(worktree_path.exists(), "Worktree directory should exist");
    assert!(
        worktree_path.join("README.md").exists(),
        "README.md should exist in worktree"
    );
}

/// Regression test: remove_worktree requires a path, not a branch name.
#[test]
fn test_remove_worktree_with_path() {
    let (temp_dir, repo_path) = create_test_repo();
    let repo = GitRepo::from_path(&repo_path).expect("Failed to discover repo");

    let branch_name = "test-remove-worktree";
    repo.create_branch(branch_name)
        .expect("Failed to create branch");

    let worktree_path = temp_dir.path().join("worktrees").join(branch_name);
    repo.create_worktree(branch_name, &worktree_path)
        .expect("Failed to create worktree");

    assert!(worktree_path.exists(), "Worktree directory should exist");
    assert!(
        repo.worktree_exists(branch_name),
        "Git should track the worktree"
    );

    repo.remove_worktree(&worktree_path, false)
        .expect("Failed to remove worktree");

    assert!(
        !worktree_path.exists(),
        "Worktree directory should be removed"
    );

    assert!(
        !repo.worktree_exists(branch_name),
        "Git should no longer track the worktree"
    );

    let output = Command::new("git")
        .args(["checkout", branch_name])
        .current_dir(&repo_path)
        .output()
        .expect("Failed to run git checkout");

    assert!(
        output.status.success(),
        "Should be able to checkout the branch after worktree removal: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let _ = Command::new("git")
        .args(["checkout", "-"])
        .current_dir(&repo_path)
        .output();
}

/// Test that remove_worktree handles already-deleted worktree directories gracefully.
/// This tests the fallback to `git worktree prune`.
#[test]
fn test_remove_worktree_already_deleted() {
    let (temp_dir, repo_path) = create_test_repo();
    let repo = GitRepo::from_path(&repo_path).expect("Failed to discover repo");

    let branch_name = "test-already-deleted";
    repo.create_branch(branch_name)
        .expect("Failed to create branch");

    let worktree_path = temp_dir.path().join("worktrees").join(branch_name);
    repo.create_worktree(branch_name, &worktree_path)
        .expect("Failed to create worktree");

    std::fs::remove_dir_all(&worktree_path).expect("Failed to delete worktree directory");

    assert!(
        repo.worktree_exists(branch_name),
        "Git should still track the stale worktree reference"
    );

    repo.remove_worktree(&worktree_path, false)
        .expect("remove_worktree should succeed even when directory is gone");

    assert!(
        !repo.worktree_exists(branch_name),
        "Git should no longer track the worktree after prune"
    );

    let output = Command::new("git")
        .args(["checkout", branch_name])
        .current_dir(&repo_path)
        .output()
        .expect("Failed to run git checkout");

    assert!(
        output.status.success(),
        "Should be able to checkout the branch after worktree prune: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

/// Test that remove_worktree works with worktrees at external (non-repo) locations.
/// This is the typical treebeard use case.
#[test]
fn test_remove_worktree_external_location() {
    let (temp_dir, repo_path) = create_test_repo();
    let repo = GitRepo::from_path(&repo_path).expect("Failed to discover repo");

    let branch_name = "test-external-worktree";
    repo.create_branch(branch_name)
        .expect("Failed to create branch");

    let worktree_path = temp_dir.path().join("external-worktrees").join(branch_name);
    repo.create_worktree(branch_name, &worktree_path)
        .expect("Failed to create external worktree");

    assert!(worktree_path.exists(), "External worktree should exist");

    repo.remove_worktree(&worktree_path, false)
        .expect("Failed to remove external worktree");

    assert!(
        !worktree_path.exists(),
        "External worktree should be removed"
    );
    assert!(
        !repo.worktree_exists(branch_name),
        "Git should no longer track the external worktree"
    );
}

/// Test prune_worktrees function directly.
#[test]
fn test_prune_worktrees() {
    let (temp_dir, repo_path) = create_test_repo();
    let repo = GitRepo::from_path(&repo_path).expect("Failed to discover repo");

    let branch_name = "test-prune";
    repo.create_branch(branch_name)
        .expect("Failed to create branch");

    let worktree_path = temp_dir.path().join("worktrees").join(branch_name);
    repo.create_worktree(branch_name, &worktree_path)
        .expect("Failed to create worktree");

    std::fs::remove_dir_all(&worktree_path).expect("Failed to delete worktree directory");

    assert!(
        repo.worktree_exists(branch_name),
        "Stale worktree reference should exist"
    );

    repo.prune_worktrees().expect("Failed to prune worktrees");

    assert!(
        !repo.worktree_exists(branch_name),
        "Stale worktree reference should be pruned"
    );
}
