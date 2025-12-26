mod common;

use common::{get_treebeard_path, TestWorkspace};
use std::process::Command;

#[test]
fn test_non_tty_rejection_branch_command() {
    let treebeard_path = get_treebeard_path();
    let workspace = TestWorkspace::new();

    let output = Command::new(&treebeard_path)
        .args(["branch", "test-non-tty"])
        .stdin(std::process::Stdio::null())
        .current_dir(&workspace.repo_path)
        .output()
        .expect("Failed to run treebeard");

    assert!(
        !output.status.success(),
        "treebeard branch should fail without TTY"
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("interactive terminal") || stderr.contains("TTY"),
        "Error message should mention TTY requirement. stderr: {}",
        stderr
    );
}

#[test]
fn test_non_tty_allowed_branch_no_shell() {
    let treebeard_path = get_treebeard_path();
    let workspace = TestWorkspace::new();

    let output = Command::new(&treebeard_path)
        .args(["branch", "test-no-shell-no-tty", "--no-shell"])
        .stdin(std::process::Stdio::null())
        .current_dir(&workspace.repo_path)
        .output()
        .expect("Failed to run treebeard");

    assert!(
        output.status.success(),
        "treebeard branch --no-shell should succeed without TTY. stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn test_non_tty_allowed_config_command_shows_config() {
    let treebeard_path = get_treebeard_path();
    let workspace = TestWorkspace::new();

    let output = Command::new(&treebeard_path)
        .arg("config")
        .stdin(std::process::Stdio::null())
        .current_dir(&workspace.repo_path)
        .output()
        .expect("Failed to run treebeard");

    assert!(
        output.status.success(),
        "treebeard config should succeed without TTY. stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("Current configuration:"),
        "Should show 'Current configuration:'. stdout: {}",
        stdout
    );
    assert!(
        stdout.contains("worktree_dir:"),
        "Should show worktree_dir. stdout: {}",
        stdout
    );
    assert!(
        stdout.contains("mount_dir:"),
        "Should show mount_dir. stdout: {}",
        stdout
    );
}

#[test]
fn test_non_tty_allowed_list_command() {
    let treebeard_path = get_treebeard_path();
    let workspace = TestWorkspace::new();

    let output = Command::new(&treebeard_path)
        .arg("list")
        .stdin(std::process::Stdio::null())
        .current_dir(&workspace.repo_path)
        .output()
        .expect("Failed to run treebeard");

    assert!(
        output.status.success(),
        "treebeard list should succeed without TTY. stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn test_non_tty_allowed_cleanup_command() {
    let treebeard_path = get_treebeard_path();
    let workspace = TestWorkspace::new();

    let output = Command::new(&treebeard_path)
        .args(["branch", "test-cleanup-no-tty", "--no-shell"])
        .current_dir(&workspace.repo_path)
        .output()
        .expect("Failed to create branch");
    assert!(output.status.success(), "Branch creation should succeed");

    let output = Command::new(&treebeard_path)
        .args(["cleanup", "test-cleanup-no-tty", "-y"])
        .stdin(std::process::Stdio::null())
        .current_dir(&workspace.repo_path)
        .output()
        .expect("Failed to run treebeard cleanup");

    assert!(
        output.status.success(),
        "treebeard cleanup should succeed without TTY. stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}
