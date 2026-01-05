//! Happy path tests for file operations.

use crate::shared::common::{
    get_branch_commits, get_treebeard_path, git_commit_count, TestWorkspace,
};
use crate::shared::e2e_helpers::{spawn_treebeard_test_mode, terminate_treebeard};
use expectrl::Expect;
use std::fs;
use std::process::Command;
use std::thread;
use std::time::Duration;

#[test]
fn test_worktree_is_created() {
    let treebeard_path = get_treebeard_path();
    let workspace = TestWorkspace::new();
    workspace.switch_to_repo();

    let branch_name = "e2e-worktree-test";

    let mut p = expectrl::spawn(format!(
        "{} branch {} --no-shell",
        treebeard_path.display(),
        branch_name
    ))
    .expect("Failed to spawn treebeard");

    p.expect(expectrl::Eof)
        .expect("Failed to wait for treebeard to complete");

    let worktree_path = workspace.get_worktree_path(branch_name);

    assert!(worktree_path.exists(), "Worktree directory should exist");
    assert!(
        worktree_path.join("README.md").exists(),
        "Files should exist in worktree"
    );

    workspace.restore_dir();
}

#[test]
fn test_create_file_with_autocommit() {
    let workspace = TestWorkspace::new();
    workspace.switch_to_repo();

    let branch_name = "fo-create-file";

    let treebeard = spawn_treebeard_test_mode(branch_name, &workspace.repo_path);

    let mount_dir = workspace.get_mount_path(branch_name);

    let test_file = mount_dir.join("newfile.txt");
    fs::write(&test_file, "hello world").expect("Failed to write file");

    thread::sleep(Duration::from_millis(700));

    terminate_treebeard(treebeard);
    workspace.restore_dir();

    let commit_count = git_commit_count(&workspace.repo_path, branch_name);
    assert!(
        commit_count >= 1,
        "Expected at least 1 commit, got {}",
        commit_count
    );

    let output = Command::new("git")
        .args(["show", &format!("{}:newfile.txt", branch_name)])
        .current_dir(&workspace.repo_path)
        .output()
        .expect("Failed to read file from git");

    assert!(output.status.success(), "File should exist in branch");
    let content = String::from_utf8_lossy(&output.stdout);
    assert_eq!(content.trim(), "hello world", "File content should match");
}

#[test]
fn test_edit_tracked_file_with_autocommit() {
    let workspace = TestWorkspace::new();
    workspace.switch_to_repo();

    let branch_name = "fo-edit-file";

    let treebeard = spawn_treebeard_test_mode(branch_name, &workspace.repo_path);

    let mount_dir = workspace.get_mount_path(branch_name);
    let test_file = mount_dir.join("tracked_file.txt");

    fs::write(&test_file, "original content").expect("Failed to write file");
    thread::sleep(Duration::from_millis(700));

    fs::write(&test_file, "modified content").expect("Failed to modify file");
    thread::sleep(Duration::from_millis(700));

    terminate_treebeard(treebeard);
    workspace.restore_dir();

    let output = Command::new("git")
        .args(["show", &format!("{}:tracked_file.txt", branch_name)])
        .current_dir(&workspace.repo_path)
        .output()
        .expect("Failed to read file from git");

    assert!(output.status.success(), "File should exist in branch");
    let content = String::from_utf8_lossy(&output.stdout);
    assert_eq!(
        content.trim(),
        "modified content",
        "File content should reflect modification"
    );
}

#[test]
fn test_delete_file_with_tracking() {
    let workspace = TestWorkspace::new();
    workspace.switch_to_repo();

    let branch_name = "fo-delete-file";

    let treebeard = spawn_treebeard_test_mode(branch_name, &workspace.repo_path);

    let mount_dir = workspace.get_mount_path(branch_name);
    let test_file = mount_dir.join("file_for_deletion.txt");

    fs::write(&test_file, "to be deleted").expect("Failed to write file");
    thread::sleep(Duration::from_millis(700));

    fs::remove_file(&test_file).expect("Failed to delete file");
    thread::sleep(Duration::from_millis(700));

    terminate_treebeard(treebeard);
    workspace.restore_dir();

    let output = Command::new("git")
        .args(["ls-tree", "-r", branch_name, "--name-only"])
        .current_dir(&workspace.repo_path)
        .output()
        .expect("Failed to list files in branch");

    let files = String::from_utf8_lossy(&output.stdout);
    assert!(
        !files.contains("file_for_deletion.txt"),
        "File should be deleted from branch"
    );
}

#[test]
fn test_multiple_file_operations_sequence() {
    let workspace = TestWorkspace::new();
    workspace.switch_to_repo();

    let branch_name = "fo-multiples";

    let treebeard = spawn_treebeard_test_mode(branch_name, &workspace.repo_path);

    let mount_dir = workspace.get_mount_path(branch_name);

    for i in 1..=3 {
        let filename = format!("file{}.txt", i);
        let content = format!("content {}", i);
        let test_file = mount_dir.join(&filename);
        fs::write(&test_file, content).expect("Failed to write file");
        thread::sleep(Duration::from_millis(200));
    }

    thread::sleep(Duration::from_millis(700));

    terminate_treebeard(treebeard);
    workspace.restore_dir();

    let output = Command::new("git")
        .args(["ls-tree", "-r", branch_name, "--name-only"])
        .current_dir(&workspace.repo_path)
        .output()
        .expect("Failed to list files in branch");

    let files = String::from_utf8_lossy(&output.stdout);
    assert!(files.contains("file1.txt"), "file1.txt should exist");
    assert!(files.contains("file2.txt"), "file2.txt should exist");
    assert!(files.contains("file3.txt"), "file3.txt should exist");
}

#[test]
fn test_create_directory_with_file() {
    let workspace = TestWorkspace::new();
    workspace.switch_to_repo();

    let branch_name = "fo-directory";

    let treebeard = spawn_treebeard_test_mode(branch_name, &workspace.repo_path);

    let mount_dir = workspace.get_mount_path(branch_name);
    let nested_dir = mount_dir.join("subdir/subsubdir");
    let nested_file = nested_dir.join("nested.txt");

    fs::create_dir_all(&nested_dir).expect("Failed to create directory");
    fs::write(&nested_file, "nested file").expect("Failed to write file");
    thread::sleep(Duration::from_millis(700));

    terminate_treebeard(treebeard);
    workspace.restore_dir();

    let output = Command::new("git")
        .args(["ls-tree", "-r", branch_name, "--name-only"])
        .current_dir(&workspace.repo_path)
        .output()
        .expect("Failed to list files in branch");

    let files = String::from_utf8_lossy(&output.stdout);
    assert!(
        files.contains("subdir/subsubdir/nested.txt"),
        "Nested file should exist"
    );
}

#[test]
fn test_commits_appear_in_real_git_history() {
    let workspace = TestWorkspace::new();
    workspace.switch_to_repo();

    let branch_name = "fo-git-history";

    let treebeard = spawn_treebeard_test_mode(branch_name, &workspace.repo_path);

    let mount_dir = workspace.get_mount_path(branch_name);
    let test_file = mount_dir.join("committed_file.txt");

    fs::write(&test_file, "committed file").expect("Failed to write file");
    thread::sleep(Duration::from_millis(700));

    terminate_treebeard(treebeard);
    workspace.restore_dir();

    let output = Command::new("git")
        .args(["log", "--oneline", branch_name])
        .current_dir(&workspace.repo_path)
        .output()
        .expect("Failed to get git log");

    assert!(output.status.success(), "Git log should succeed");

    let commits = git_commit_count(&workspace.repo_path, branch_name);
    assert!(
        commits >= 1,
        "Should have at least 1 commit, got {}",
        commits
    );

    let branch_commits = get_branch_commits(&workspace.repo_path, branch_name);
    assert!(!branch_commits.is_empty(), "Branch should have commits");
}
