use crate::shared::common::{create_test_file, create_test_repo};

use std::process::Command;
use treebeard::git::GitRepo;

/// Helper to get the stash list output
fn get_stash_list(repo_path: &std::path::Path) -> Vec<String> {
    let output = Command::new("git")
        .args(["stash", "list"])
        .current_dir(repo_path)
        .output()
        .expect("Failed to run git stash list");

    let stdout = String::from_utf8_lossy(&output.stdout);
    stdout
        .lines()
        .filter(|line| !line.is_empty())
        .map(|line| line.to_string())
        .collect()
}

#[test]
fn test_stash_push_with_uncommitted_changes() {
    let (_temp_dir, repo_path) = create_test_repo();
    let repo = GitRepo::from_path(&repo_path).expect("Failed to discover repo");

    // Create uncommitted changes
    create_test_file(&repo_path, "new_file.txt", "some content");

    // Verify there are uncommitted changes
    assert!(
        repo.has_uncommitted_changes().unwrap(),
        "Should have uncommitted changes"
    );

    // Stash the changes
    let stash_message = "test-stash-message";
    repo.stash_push(stash_message, true)
        .expect("Failed to stash changes");

    // Verify changes were stashed (no uncommitted changes now)
    assert!(
        !repo.has_uncommitted_changes().unwrap(),
        "Should not have uncommitted changes after stash"
    );

    // Verify stash exists with correct message
    let stash_list = get_stash_list(&repo_path);
    assert_eq!(stash_list.len(), 1, "Should have exactly one stash entry");
    assert!(
        stash_list[0].contains(stash_message),
        "Stash message should be present in stash list"
    );
}

#[test]
fn test_stash_push_includes_untracked_files() {
    let (_temp_dir, repo_path) = create_test_repo();
    let repo = GitRepo::from_path(&repo_path).expect("Failed to discover repo");

    // Create an untracked file (not staged)
    let untracked_file = repo_path.join("untracked.txt");
    std::fs::write(&untracked_file, "untracked content").expect("Failed to write file");

    // Verify file exists
    assert!(untracked_file.exists(), "Untracked file should exist");

    // Stash with include_untracked = true
    repo.stash_push("include-untracked-test", true)
        .expect("Failed to stash changes");

    // Verify untracked file is now gone (stashed)
    assert!(
        !untracked_file.exists(),
        "Untracked file should be removed after stash with include_untracked=true"
    );

    // Verify stash exists
    let stash_list = get_stash_list(&repo_path);
    assert_eq!(stash_list.len(), 1, "Should have exactly one stash entry");
}

#[test]
fn test_stash_push_without_untracked_files() {
    let (_temp_dir, repo_path) = create_test_repo();
    let repo = GitRepo::from_path(&repo_path).expect("Failed to discover repo");

    // Modify an existing tracked file and add an untracked file
    std::fs::write(repo_path.join("README.md"), "modified content").expect("Failed to modify file");
    let untracked_file = repo_path.join("untracked.txt");
    std::fs::write(&untracked_file, "untracked content").expect("Failed to write file");

    // Stash with include_untracked = false
    repo.stash_push("exclude-untracked-test", false)
        .expect("Failed to stash changes");

    // Untracked file should still exist (not stashed)
    assert!(
        untracked_file.exists(),
        "Untracked file should remain when include_untracked=false"
    );

    // Verify stash exists (tracked changes were stashed)
    let stash_list = get_stash_list(&repo_path);
    assert_eq!(stash_list.len(), 1, "Should have exactly one stash entry");
}

#[test]
fn test_stash_push_with_no_changes() {
    let (_temp_dir, repo_path) = create_test_repo();
    let repo = GitRepo::from_path(&repo_path).expect("Failed to discover repo");

    // Verify no uncommitted changes
    assert!(
        !repo.has_uncommitted_changes().unwrap(),
        "Should not have uncommitted changes"
    );

    // git stash push with no changes succeeds (but creates no stash entry)
    // This is git's actual behavior - it outputs "No local changes to save" but exits 0
    let result = repo.stash_push("no-changes-test", true);
    assert!(
        result.is_ok(),
        "git stash push succeeds even with no changes"
    );

    // Verify no stash entry was created
    let stash_list = get_stash_list(&repo_path);
    assert!(
        stash_list.is_empty(),
        "No stash entry should be created when there are no changes"
    );
}

#[test]
fn test_stash_push_with_staged_changes() {
    let (_temp_dir, repo_path) = create_test_repo();
    let repo = GitRepo::from_path(&repo_path).expect("Failed to discover repo");

    // Create and stage a new file
    create_test_file(&repo_path, "staged_file.txt", "staged content");

    Command::new("git")
        .args(["add", "staged_file.txt"])
        .current_dir(&repo_path)
        .output()
        .expect("Failed to stage file");

    // Verify there are uncommitted changes
    assert!(
        repo.has_uncommitted_changes().unwrap(),
        "Should have staged changes"
    );

    // Stash the changes
    repo.stash_push("staged-test", true)
        .expect("Failed to stash changes");

    // Verify changes were stashed
    assert!(
        !repo.has_uncommitted_changes().unwrap(),
        "Should not have uncommitted changes after stash"
    );

    // Verify stash exists
    let stash_list = get_stash_list(&repo_path);
    assert_eq!(stash_list.len(), 1, "Should have exactly one stash entry");
}

#[test]
fn test_has_uncommitted_changes_with_modified_file() {
    let (_temp_dir, repo_path) = create_test_repo();
    let repo = GitRepo::from_path(&repo_path).expect("Failed to discover repo");

    // Initial state: no uncommitted changes
    assert!(
        !repo.has_uncommitted_changes().unwrap(),
        "Should not have uncommitted changes initially"
    );

    // Modify the README
    std::fs::write(repo_path.join("README.md"), "modified content").expect("Failed to modify file");

    // Now should have uncommitted changes
    assert!(
        repo.has_uncommitted_changes().unwrap(),
        "Should have uncommitted changes after modifying a file"
    );
}

#[test]
fn test_has_uncommitted_changes_with_new_file() {
    let (_temp_dir, repo_path) = create_test_repo();
    let repo = GitRepo::from_path(&repo_path).expect("Failed to discover repo");

    // Create a new untracked file
    create_test_file(&repo_path, "new_file.txt", "new content");

    // Should have uncommitted changes (untracked files count)
    assert!(
        repo.has_uncommitted_changes().unwrap(),
        "Should have uncommitted changes with new untracked file"
    );
}
