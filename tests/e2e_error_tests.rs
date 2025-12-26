mod common;

use common::{get_treebeard_path, TestWorkspace};
use std::process::Command;

#[test]
fn test_err_non_git_directory() {
    let treebeard_path = get_treebeard_path();
    let temp_dir = tempfile::tempdir().expect("Failed to create temp directory");

    let output = Command::new(&treebeard_path)
        .args(["branch", "test-branch", "--no-shell"])
        .current_dir(temp_dir.path())
        .output()
        .expect("Failed to run treebeard");

    assert!(
        !output.status.success(),
        "Treebeard should fail when run in non-git directory"
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("Not a git repository") || stderr.contains("not a git"),
        "Error message should mention not a git repository. stderr: {}",
        stderr
    );
}

#[test]
fn test_err_invalid_branch_names() {
    let treebeard_path = get_treebeard_path();
    let workspace = TestWorkspace::new();
    workspace.switch_to_repo();

    let invalid_branch_names = vec!["", "  ", "\t"];

    for branch_name in invalid_branch_names {
        let output = Command::new(&treebeard_path)
            .args(["branch", branch_name, "--no-shell"])
            .current_dir(&workspace.repo_path)
            .output()
            .expect("Failed to run treebeard");

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);

        assert!(
            !output.status.success(),
            "Treebeard should reject invalid branch name '{}'. stdout: {}, stderr: {}",
            branch_name,
            stdout,
            stderr
        );

        let combined_output = format!("{} {}", stdout, stderr);
        assert!(
            combined_output.contains("error") || combined_output.contains("invalid") || combined_output.contains("required"),
            "Error message should indicate problem with empty/whitespace branch name '{}'. Output: {}",
            branch_name,
            combined_output
        );
    }

    workspace.restore_dir();
}

#[test]
fn test_err_cleanup_non_existent_worktree() {
    let treebeard_path = get_treebeard_path();
    let workspace = TestWorkspace::new();
    workspace.switch_to_repo();

    let branch_name = "err-cleanup-missing";
    let output = Command::new(&treebeard_path)
        .args(["cleanup", branch_name])
        .current_dir(&workspace.repo_path)
        .output()
        .expect("Failed to run cleanup command");

    assert!(
        output.status.success(),
        "Cleanup should succeed even for non-existent worktree"
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains(&format!("Worktree for '{}' does not exist", branch_name))
            || stdout.contains(branch_name),
        "Output should mention the worktree. Output: {}",
        stdout
    );

    workspace.restore_dir();
}

#[test]
fn test_err_worktree_permission_denied() {
    let treebeard_path = get_treebeard_path();
    let workspace = TestWorkspace::new();
    workspace.switch_to_repo();

    let temp_dir = tempfile::tempdir().expect("Failed to create temp directory");

    std::env::set_var("TREEBEARD_DATA_DIR", temp_dir.path());
    let branch_name = "err-permission-test";

    let output = Command::new(&treebeard_path)
        .args(["branch", branch_name, "--no-shell"])
        .current_dir(&workspace.repo_path)
        .output()
        .expect("Failed to run treebeard");

    std::env::remove_var("TREEBEARD_DATA_DIR");

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains("permission")
                || stderr.contains("Permission")
                || stderr.contains("denied")
                || stderr.contains("error"),
            "If permission error occurs, it should be mentioned. stderr: {}",
            stderr
        );
    } else {
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            stdout.contains(branch_name),
            "Should succeed if permissions allow"
        );
    }

    workspace.restore_dir();
}

#[test]
fn test_err_branch_with_already_existing_branch() {
    let treebeard_path = get_treebeard_path();
    let workspace = TestWorkspace::new();
    workspace.switch_to_repo();

    let branch_name = "err-duplicate-branch";

    let output = Command::new(&treebeard_path)
        .args(["branch", branch_name, "--no-shell"])
        .current_dir(&workspace.repo_path)
        .output()
        .expect("Failed to run treebeard first time");

    assert!(
        output.status.success(),
        "First branch creation should succeed"
    );

    let output = Command::new(&treebeard_path)
        .args(["branch", branch_name, "--no-shell"])
        .current_dir(&workspace.repo_path)
        .output()
        .expect("Failed to run treebeard second time");

    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(
        output.status.success(),
        "Command should succeed even when branch exists"
    );
    assert!(
        stdout.contains("already exists") || stdout.contains(branch_name),
        "Output should mention branch already exists. stdout: {}",
        stdout
    );

    workspace.restore_dir();
}

#[test]
fn test_err_empty_branch_name() {
    let treebeard_path = get_treebeard_path();
    let workspace = TestWorkspace::new();
    workspace.switch_to_repo();

    let output = Command::new(&treebeard_path)
        .args(["branch", "", "--no-shell"])
        .current_dir(&workspace.repo_path)
        .output()
        .expect("Failed to run treebeard");

    assert!(
        !output.status.success(),
        "Treebeard should reject empty branch name"
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("required") || stderr.contains("error") || stderr.contains("invalid"),
        "Error message should indicate required argument. stderr: {}",
        stderr
    );

    workspace.restore_dir();
}
