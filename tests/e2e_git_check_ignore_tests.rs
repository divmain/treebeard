//! E2E tests for git check-ignore failure handling (ERR-001).
//!
//! These tests verify that the normal sync flow still works correctly.
//! The git check-ignore failure path is tested via unit tests in
//! src/sync/aggregation.rs since simulating git failures in e2e tests
//! would require modifying the treebeard process's PATH, which is complex.

mod common;
mod e2e_helpers;

use common::{get_treebeard_path, TestWorkspace};
use e2e_helpers::{exit_session, start_session, SessionExitConfig, SessionStartConfig};
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

// Note: Testing the sync flow with gitignored files is complex because:
// 1. The sync flow triggers based on FUSE overlay mutations
// 2. Gitignored files need to exist in the main repo's .gitignore before treebeard starts
// 3. The file must be modified through the FUSE mount path, not the worktree path
//
// The sync flow is tested more thoroughly in e2e_workflow_tests.rs.
// The git check-ignore error handling is tested via unit tests in src/sync/aggregation.rs.
