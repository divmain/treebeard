//! Edge case tests for cleanup conditions.
//!
//! Tests from e2e_git_check_ignore_tests.rs for cleanup flow edge cases.

use crate::shared::common::{get_treebeard_path, TestWorkspace};
use crate::shared::e2e_helpers::{
    exit_session, start_session, SessionExitConfig, SessionStartConfig,
};
use std::time::Duration;

/// Test that the normal cleanup flow (without git check-ignore failure) still uses y/n prompt.
/// This verifies we didn't break the normal path when adding the git check failure handling.
#[test]
fn test_normal_cleanup_flow_uses_yes_no_prompt() {
    let workspace = TestWorkspace::new();
    workspace.switch_to_repo();

    let branch_name = "normal-cleanup-yn";
    let treebeard_path = get_treebeard_path();

    // Start the session (no fake git script - normal flow)
    let mut session = start_session(
        &treebeard_path,
        branch_name,
        &workspace.repo_path,
        SessionStartConfig::default(),
    );

    // Don't create any files - just exit quickly
    // No squash prompt expected since there are no auto-commits
    std::thread::sleep(Duration::from_millis(500));

    // Use the standard exit flow - no squash prompt since no files created
    exit_session(
        &mut session,
        SessionExitConfig::standard()
            .with_expect_squash_prompt(false)
            .with_delete_worktree(true),
    );

    // Verify worktree was deleted (normal y/n flow worked)
    let worktree_path = workspace.get_worktree_path(branch_name);
    assert!(
        !worktree_path.exists(),
        "Worktree should be deleted in normal flow"
    );

    workspace.restore_dir();
}

/// Test that declining to delete worktree preserves it in normal flow.
#[test]
fn test_normal_cleanup_flow_preserve_worktree() {
    let workspace = TestWorkspace::new();
    workspace.switch_to_repo();

    let branch_name = "normal-cleanup-preserve";
    let treebeard_path = get_treebeard_path();

    let mut session = start_session(
        &treebeard_path,
        branch_name,
        &workspace.repo_path,
        SessionStartConfig::default(),
    );

    std::thread::sleep(Duration::from_millis(500));

    // Decline to delete worktree - no squash prompt since no files created
    exit_session(
        &mut session,
        SessionExitConfig::standard()
            .with_expect_squash_prompt(false)
            .with_delete_worktree(false),
    );

    // Verify worktree was preserved
    let worktree_path = workspace.get_worktree_path(branch_name);
    assert!(
        worktree_path.exists(),
        "Worktree should be preserved when user declines deletion"
    );

    workspace.restore_dir();
}
