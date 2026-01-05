//! Happy path tests for session management.

use crate::shared::common::{get_treebeard_path, TestWorkspace};
use crate::shared::e2e_helpers::{spawn_treebeard_test_mode, terminate_treebeard};
use expectrl::{spawn, Eof, Expect};
use std::process::Command;

#[test]
fn test_multiple_sessions_sequential() {
    let treebeard_path = get_treebeard_path();
    let workspace = TestWorkspace::new();
    workspace.switch_to_repo();

    for i in 1..=3 {
        let branch_name = format!("e2e-session-{}", i);
        let mut p = spawn(format!(
            "{} branch {} --no-shell",
            treebeard_path.display(),
            branch_name
        ))
        .expect("Failed to spawn treebeard session");

        p.expect(Eof)
            .expect("Failed to wait for treebeard to complete");

        assert!(
            !workspace.repo_path.join(".treebeard").exists(),
            "Should clean up worktree after each session"
        );
    }

    workspace.restore_dir();
}

#[test]
fn test_list_active_sessions() {
    let workspace = TestWorkspace::new();
    workspace.switch_to_repo();

    let branch_name = "ni-list-single-session";

    let treebeard = spawn_treebeard_test_mode(branch_name, &workspace.repo_path);

    let treebeard_path = get_treebeard_path();
    let output = Command::new(&treebeard_path)
        .arg("list")
        .current_dir(&workspace.repo_path)
        .output()
        .expect("Failed to run list command");

    assert!(output.status.success(), "List command should succeed");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains(branch_name),
        "Output should contain branch name '{}'. Output: {}",
        branch_name,
        stdout
    );

    terminate_treebeard(treebeard);
}

#[test]
fn test_list_multiple_concurrent_sessions() {
    let workspace = TestWorkspace::new();
    workspace.switch_to_repo();

    let treebeard_path = get_treebeard_path();
    let branch_names = vec!["ni-multi-1", "ni-multi-2", "ni-multi-3"];

    let mut children = Vec::new();
    for branch_name in &branch_names {
        let child = spawn_treebeard_test_mode(branch_name, &workspace.repo_path);
        children.push((branch_name.to_string(), child));
    }

    let output = Command::new(&treebeard_path)
        .arg("list")
        .current_dir(&workspace.repo_path)
        .output()
        .expect("Failed to run list command");

    assert!(output.status.success(), "List command should succeed");

    let stdout = String::from_utf8_lossy(&output.stdout);
    for branch_name in &branch_names {
        assert!(
            stdout.contains(branch_name),
            "Output should contain branch name '{}'. Output: {}",
            branch_name,
            stdout
        );
    }

    for (_, child) in children {
        terminate_treebeard(child);
    }
}

#[test]
fn test_cleanup_existing_branch() {
    let workspace = TestWorkspace::new();
    workspace.switch_to_repo();

    let branch_name = "ni-cleanup-existing";

    let treebeard = spawn_treebeard_test_mode(branch_name, &workspace.repo_path);

    std::thread::sleep(std::time::Duration::from_millis(200));

    let worktree_dir = workspace.get_worktree_path(branch_name);
    assert!(
        worktree_dir.exists(),
        "Worktree should exist before cleanup"
    );

    terminate_treebeard(treebeard);
    std::thread::sleep(std::time::Duration::from_millis(200));

    let treebeard_path = get_treebeard_path();
    let output = Command::new(&treebeard_path)
        .args(["cleanup", branch_name])
        .current_dir(&workspace.repo_path)
        .output()
        .expect("Failed to run cleanup command");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains(&format!("Worktree for '{}' does not exist", branch_name))
            || stdout.contains(branch_name),
        "Cleanup command should provide output about the worktree. Output: {}",
        stdout
    );
}

#[test]
fn test_cleanup_non_existent_branch() {
    let workspace = TestWorkspace::new();
    workspace.switch_to_repo();

    let branch_name = "ni-cleanup-nonexistent";

    let treebeard_path = get_treebeard_path();
    let output = Command::new(&treebeard_path)
        .args(["cleanup", branch_name])
        .current_dir(&workspace.repo_path)
        .output()
        .expect("Failed to run cleanup command");

    assert!(
        output.status.success(),
        "Cleanup command should succeed for non-existent branch"
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains(&format!("Worktree for '{}' does not exist", branch_name))
            || stdout.contains(branch_name),
        "Output should indicate worktree does not exist. Output: {}",
        stdout
    );
}
