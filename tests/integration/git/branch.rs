use crate::shared::common::create_test_repo;

use treebeard::git::GitRepo;

#[test]
fn test_branch_creation() {
    let (_temp_dir, repo_path) = create_test_repo();
    let repo = GitRepo::from_path(&repo_path).expect("Failed to discover repo");

    assert!(
        !repo.branch_exists("test-branch"),
        "Branch should not exist yet"
    );

    repo.create_branch("test-branch")
        .expect("Failed to create branch");

    assert!(
        repo.branch_exists("test-branch"),
        "Branch should exist after creation"
    );
}

#[test]
fn test_duplicate_branch_creation() {
    let (_temp_dir, repo_path) = create_test_repo();
    let repo = GitRepo::from_path(&repo_path).expect("Failed to discover repo");

    repo.create_branch("test-branch")
        .expect("Failed to create first branch");

    let result = repo.create_branch("test-branch");
    assert!(result.is_err(), "Should fail to create duplicate branch");
}
