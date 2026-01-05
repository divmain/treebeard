use crate::shared::common::create_test_repo;

use std::path::PathBuf;
use std::process::Command;
use treebeard::git::GitRepo;

/// Regression test: squash_commits must use the worktree's GitRepo, not the main repo,
/// otherwise git commands run in the wrong directory and fail silently.
#[test]
fn test_squash_commits_in_worktree() {
    let (_temp_dir, repo_path) = create_test_repo();
    let repo = GitRepo::from_path(&repo_path).expect("Failed to discover repo");

    let branch_name = "test-squash-worktree";
    repo.create_branch(branch_name)
        .expect("Failed to create branch");

    let worktree_path = repo_path.join(".treebeard-worktree-squash");
    repo.create_worktree(branch_name, &worktree_path)
        .expect("Failed to create worktree");

    let worktree_repo =
        GitRepo::from_path(&worktree_path).expect("Failed to create GitRepo for worktree");

    std::fs::write(worktree_path.join("file1.txt"), "content 1").expect("Failed to write file1");
    worktree_repo
        .stage_and_commit("First commit")
        .expect("Failed to commit file1");

    let output = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(&worktree_path)
        .output()
        .expect("Failed to get HEAD");
    let head_before = String::from_utf8_lossy(&output.stdout).trim().to_string();

    let result = worktree_repo.squash_commits(branch_name, "Squashed commit");
    assert!(
        result.is_ok(),
        "squash_commits should succeed when using worktree repo: {:?}",
        result.err()
    );

    let output = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(&worktree_path)
        .output()
        .expect("Failed to get HEAD after squash");
    let head_after = String::from_utf8_lossy(&output.stdout).trim().to_string();

    assert_ne!(
        head_before, head_after,
        "HEAD should change after squash (commit should be replaced)"
    );

    let output = Command::new("git")
        .args(["log", "-1", "--format=%s"])
        .current_dir(&worktree_path)
        .output()
        .expect("Failed to get commit message");
    let commit_msg = String::from_utf8_lossy(&output.stdout).trim().to_string();
    assert_eq!(
        commit_msg, "Squashed commit",
        "Commit message should be the squash message"
    );

    let output = Command::new("git")
        .args(["ls-tree", "-r", branch_name, "--name-only"])
        .current_dir(&worktree_path)
        .output()
        .expect("Failed to list files");
    let files = String::from_utf8_lossy(&output.stdout);
    assert!(
        files.contains("file1.txt"),
        "file1.txt should exist after squash"
    );
}

/// Regression test: a failed commit should not leave the repository in a broken
/// state with HEAD reset but no squash commit.
#[test]
fn test_squash_commits_rollback_on_failure() {
    let (_temp_dir, repo_path) = create_test_repo();
    let repo = GitRepo::from_path(&repo_path).expect("Failed to discover repo");

    let branch_name = "test-squash-rollback";
    repo.create_branch(branch_name)
        .expect("Failed to create branch");

    let worktree_path = repo_path.join(".treebeard-worktree-rollback");
    repo.create_worktree(branch_name, &worktree_path)
        .expect("Failed to create worktree");

    let worktree_repo =
        GitRepo::from_path(&worktree_path).expect("Failed to create GitRepo for worktree");

    std::fs::write(worktree_path.join("file1.txt"), "content 1").expect("Failed to write file1");
    worktree_repo
        .stage_and_commit("Initial commit")
        .expect("Failed to commit file1");

    std::fs::write(worktree_path.join("file2.txt"), "content 2").expect("Failed to write file2");
    worktree_repo
        .stage_and_commit("Second commit")
        .expect("Failed to commit file2");

    let original_head = worktree_repo.get_head().expect("Failed to get HEAD");

    let worktree_git_dir = worktree_path.join(".git");
    let index_path = match worktree_git_dir.is_file() {
        true => {
            let gitdir_content =
                std::fs::read_to_string(&worktree_git_dir).expect("Failed to read .git file");
            let gitdir = gitdir_content
                .strip_prefix("gitdir: ")
                .expect("Invalid .git file")
                .trim();
            if gitdir.starts_with("/") {
                PathBuf::from(gitdir)
            } else {
                worktree_path.join(gitdir)
            }
        }
        false => worktree_git_dir,
    };
    std::fs::write(index_path.join("index"), b"corrupted index data")
        .expect("Failed to corrupt index");

    let result = worktree_repo.squash_commits(branch_name, "Squashed commit");
    assert!(
        result.is_err(),
        "squash_commits should fail with corrupted index"
    );

    let head_after = worktree_repo
        .get_head()
        .expect("Failed to get HEAD after rollback");

    assert_eq!(
        original_head, head_after,
        "HEAD should be rolled back to original position after commit failure"
    );
}
