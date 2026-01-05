//! Happy path tests for config commands.

use crate::shared::common::{get_treebeard_path, TestWorkspace};
use std::process::Command;

#[test]
fn test_config_default_shows_config() {
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
        "config should succeed. stderr: {}",
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
fn test_config_path_subcommand() {
    let treebeard_path = get_treebeard_path();
    let workspace = TestWorkspace::new();

    let output = Command::new(&treebeard_path)
        .args(["config", "path"])
        .stdin(std::process::Stdio::null())
        .current_dir(&workspace.repo_path)
        .output()
        .expect("Failed to run treebeard");

    assert!(
        output.status.success(),
        "config path should succeed. stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("Config location:"),
        "Should show config location. stdout: {}",
        stdout
    );
}

#[test]
fn test_config_show_subcommand() {
    let treebeard_path = get_treebeard_path();
    let workspace = TestWorkspace::new();

    let output = Command::new(&treebeard_path)
        .args(["config", "show"])
        .stdin(std::process::Stdio::null())
        .current_dir(&workspace.repo_path)
        .output()
        .expect("Failed to run treebeard");

    assert!(
        output.status.success(),
        "config show should succeed. stderr: {}",
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
    assert!(
        stdout.contains("on_exit:"),
        "Should show on_exit. stdout: {}",
        stdout
    );
    assert!(
        stdout.contains("auto_commit_message:"),
        "Should show auto_commit_message. stdout: {}",
        stdout
    );
    assert!(
        stdout.contains("auto_commit_debounce_ms:"),
        "Should show auto_commit_debounce_ms. stdout: {}",
        stdout
    );
    assert!(
        stdout.contains("fuse_ttl_secs:"),
        "Should show fuse_ttl_secs. stdout: {}",
        stdout
    );
}
