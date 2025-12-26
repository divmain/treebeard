mod common;

use common::create_test_repo;
use treebeard::git::GitRepo;

#[test]
fn test_git_repo_from_path() {
    let (_temp_dir, repo_path) = create_test_repo();
    let repo = GitRepo::from_path(&repo_path).expect("Failed to discover repo");
    assert!(repo.workdir().starts_with(&repo_path));
}

#[test]
fn test_repo_name() {
    let (_temp_dir, repo_path) = create_test_repo();
    let repo = GitRepo::from_path(&repo_path).expect("Failed to discover repo");
    let repo_name = repo.repo_name();

    assert!(!repo_name.is_empty(), "Repo name should not be empty");
    assert_eq!(
        repo_name, "test-repo",
        "Repo name should match directory name"
    );
}
