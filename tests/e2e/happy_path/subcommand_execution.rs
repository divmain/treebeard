//! Happy path tests for subcommand execution.

use crate::shared::common::{get_treebeard_path, TestWorkspace};
use crate::shared::e2e_helpers::spawn_treebeard_with_subcommand;

#[test]
fn test_subcommand_successful_exit() {
    let treebeard_path = get_treebeard_path();
    let workspace = TestWorkspace::new();
    workspace.switch_to_repo();

    let branch_name = "cmd-success-exit";

    let (output, exit_code) = spawn_treebeard_with_subcommand(
        &treebeard_path,
        branch_name,
        &workspace.repo_path,
        &["true"],
    );

    assert!(
        output.contains("Running: true"),
        "Should show the command being run. Output: {}",
        output
    );
    assert!(
        output.contains("subprocess terminates"),
        "Should mention cleanup on termination. Output: {}",
        output
    );
    assert_eq!(
        exit_code, 0,
        "Treebeard should exit with 0 when subcommand succeeds"
    );

    workspace.restore_dir();
}

#[test]
fn test_subcommand_with_arguments() {
    let treebeard_path = get_treebeard_path();
    let workspace = TestWorkspace::new();
    workspace.switch_to_repo();

    let branch_name = "cmd-with-args";

    let (output, exit_code) = spawn_treebeard_with_subcommand(
        &treebeard_path,
        branch_name,
        &workspace.repo_path,
        &["echo", "-n", "test"],
    );

    assert!(
        output.contains("Running: echo -n test"),
        "Should show the full command with arguments. Output: {}",
        output
    );
    assert_eq!(
        exit_code, 0,
        "Treebeard should succeed when echo command succeeds"
    );

    workspace.restore_dir();
}

#[test]
fn test_success_shows_done_message() {
    let treebeard_path = get_treebeard_path();
    let workspace = TestWorkspace::new();
    workspace.switch_to_repo();

    let branch_name = "success-done-msg";

    let (output, exit_code) = spawn_treebeard_with_subcommand(
        &treebeard_path,
        branch_name,
        &workspace.repo_path,
        &["true"],
    );

    assert_eq!(exit_code, 0, "Should exit with 0 on success");
    assert!(
        output.contains("Done!"),
        "Should show 'Done!' on successful exit. Output: {}",
        output
    );
    assert!(
        output.contains("is ready to push"),
        "Should show 'ready to push' on successful exit. Output: {}",
        output
    );

    workspace.restore_dir();
}
