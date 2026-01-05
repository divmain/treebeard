//! Happy path tests for path commands.

use crate::shared::common::{get_treebeard_path, TestWorkspace};
use crate::shared::e2e_helpers::{spawn_treebeard_test_mode, terminate_treebeard};
use std::process::Command;

#[test]
fn test_path_returns_mount_path() {
    let workspace = TestWorkspace::new();
    workspace.switch_to_repo();

    let branch_name = "test-path-mount";

    let treebeard = spawn_treebeard_test_mode(branch_name, &workspace.repo_path);

    let treebeard_path = get_treebeard_path();
    let output = Command::new(&treebeard_path)
        .args(["path", branch_name])
        .current_dir(&workspace.repo_path)
        .output()
        .expect("Failed to run path command");

    assert!(output.status.success(), "Path command should succeed");

    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let expected_path = workspace.get_mount_path(branch_name);

    assert!(
        stdout == expected_path.display().to_string(),
        "Output should match expected mount path. Expected: {}, got: {}",
        expected_path.display(),
        stdout
    );

    terminate_treebeard(treebeard);
}

#[test]
fn test_path_returns_worktree_path() {
    let workspace = TestWorkspace::new();
    workspace.switch_to_repo();

    let branch_name = "test-path-worktree";

    let treebeard = spawn_treebeard_test_mode(branch_name, &workspace.repo_path);

    let treebeard_path = get_treebeard_path();
    let output = Command::new(&treebeard_path)
        .args(["path", branch_name, "--worktree"])
        .current_dir(&workspace.repo_path)
        .output()
        .expect("Failed to run path command");

    assert!(output.status.success(), "Path command should succeed");

    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let expected_path = workspace.get_worktree_path(branch_name);

    assert!(
        stdout == expected_path.display().to_string(),
        "Output should match expected worktree path. Expected: {}, got: {}",
        expected_path.display(),
        stdout
    );

    terminate_treebeard(treebeard);
}

#[test]
fn test_path_works_for_nonexistent_branch() {
    let workspace = TestWorkspace::new();
    workspace.switch_to_repo();

    let branch_name = "test-path-nonexistent";

    let treebeard_path = get_treebeard_path();
    let output = Command::new(&treebeard_path)
        .args(["path", branch_name])
        .current_dir(&workspace.repo_path)
        .output()
        .expect("Failed to run path command");

    assert!(
        output.status.success(),
        "Path command should succeed for non-existent branch"
    );

    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let expected_path = workspace.get_mount_path(branch_name);

    assert!(
        stdout == expected_path.display().to_string(),
        "Output should match expected path for non-existent branch. Expected: {}, got: {}",
        expected_path.display(),
        stdout
    );
}

#[test]
fn test_path_worktree_flag_short() {
    let workspace = TestWorkspace::new();
    workspace.switch_to_repo();

    let branch_name = "test-path-worktree-short";

    let treebeard = spawn_treebeard_test_mode(branch_name, &workspace.repo_path);

    let treebeard_path = get_treebeard_path();
    let output = Command::new(&treebeard_path)
        .args(["path", branch_name, "-w"])
        .current_dir(&workspace.repo_path)
        .output()
        .expect("Failed to run path command");

    assert!(output.status.success(), "Path command should succeed");

    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let expected_path = workspace.get_worktree_path(branch_name);

    assert!(
        stdout == expected_path.display().to_string(),
        "Output should match expected worktree path with short flag. Expected: {}, got: {}",
        expected_path.display(),
        stdout
    );

    terminate_treebeard(treebeard);
}
