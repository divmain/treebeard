mod common;
mod e2e_helpers;

use common::{create_test_file, get_treebeard_path, TestWorkspace};
use e2e_helpers::{
    exit_session, shell_exec, shell_sleep, start_session, SessionExitConfig, SessionStartConfig,
};
use std::fs;
use std::process::Command;

// Sync flow tests

/// Test that skipping the sync flow preserves the worktree and original files.
#[test]
fn test_sync_skip_preserves_worktree() {
    let workspace = TestWorkspace::new();

    // Create .gitignore and an ignored file in the repo
    create_test_file(&workspace.repo_path, ".gitignore", ".env\n");
    create_test_file(&workspace.repo_path, ".env", "SECRET=original\n");

    // Commit the .gitignore (the .env is ignored)
    let output = Command::new("git")
        .args(["add", ".gitignore"])
        .current_dir(&workspace.repo_path)
        .output()
        .expect("Failed to git add");
    assert!(output.status.success(), "Failed to git add .gitignore");

    let output = Command::new("git")
        .args(["commit", "-m", "Add gitignore"])
        .current_dir(&workspace.repo_path)
        .output()
        .expect("Failed to git commit");
    assert!(output.status.success(), "Failed to git commit");

    workspace.switch_to_repo();

    let branch_name = "ctrl-c-sync-test";
    let treebeard_path = get_treebeard_path();

    let mut session = start_session(
        &treebeard_path,
        branch_name,
        &workspace.repo_path,
        SessionStartConfig::default(),
    );

    // Create file and wait for auto-commit
    shell_exec(&mut session, "echo 'content1' > file1.txt && sync");
    shell_sleep(&mut session, 3.0);

    // Modify the ignored .env file
    shell_exec(&mut session, "echo 'SECRET=modified' > .env && sync");
    shell_sleep(&mut session, 3.0);

    // Exit and skip the sync flow (simulating user choosing not to sync)
    // This preserves the worktree and verifies the sync flow was triggered
    exit_session(
        &mut session,
        SessionExitConfig::standard()
            .with_sync_skip()
            .preserve_worktree(),
    );

    workspace.restore_dir();

    // Verify the worktree was preserved
    let worktree_path = workspace.get_worktree_path(branch_name);
    assert!(
        worktree_path.exists(),
        "Worktree should be preserved. Path: {}",
        worktree_path.display()
    );

    // Verify the original .env in the main repo was NOT modified (sync was skipped)
    let main_env_content =
        fs::read_to_string(workspace.repo_path.join(".env")).expect("Failed to read .env");
    assert!(
        main_env_content.contains("original"),
        "Main repo .env should not be modified (sync was skipped). Content: {}",
        main_env_content
    );
}

/// Test that sync flow skipping preserves original files in main repo.
#[test]
fn test_sync_skip_preserves_original_files() {
    let workspace = TestWorkspace::new();

    // Create .gitignore and multiple ignored files
    create_test_file(&workspace.repo_path, ".gitignore", "*.ignore\n");
    create_test_file(&workspace.repo_path, "file1.ignore", "content1\n");
    create_test_file(&workspace.repo_path, "file2.ignore", "content2\n");

    // Commit the .gitignore
    let output = Command::new("git")
        .args(["add", ".gitignore"])
        .current_dir(&workspace.repo_path)
        .output()
        .expect("Failed to git add");
    assert!(output.status.success(), "Failed to git add .gitignore");

    let output = Command::new("git")
        .args(["commit", "-m", "Add gitignore"])
        .current_dir(&workspace.repo_path)
        .output()
        .expect("Failed to git commit");
    assert!(output.status.success(), "Failed to git commit");

    workspace.switch_to_repo();

    let branch_name = "sync-skip-test";
    let treebeard_path = get_treebeard_path();

    let mut session = start_session(
        &treebeard_path,
        branch_name,
        &workspace.repo_path,
        SessionStartConfig::default(),
    );

    // Create some regular files (not gitignored) to trigger auto-commits
    shell_exec(&mut session, "echo 'content1' > file1.txt && sync");
    shell_exec(&mut session, "echo 'content2' > file2.txt && sync");
    shell_sleep(&mut session, 3.0);

    // Modify both ignored files
    shell_exec(&mut session, "echo 'modified1' > file1.ignore && sync");
    shell_exec(&mut session, "echo 'modified2' > file2.ignore && sync");
    shell_sleep(&mut session, 3.0);

    // Exit and skip the sync flow
    exit_session(
        &mut session,
        SessionExitConfig::standard()
            .with_sync_skip()
            .preserve_worktree(),
    );

    workspace.restore_dir();

    // Verify files were NOT synced (original content preserved)
    let file1_content =
        fs::read_to_string(workspace.repo_path.join("file1.ignore")).expect("Failed to read file1");
    assert!(
        file1_content.contains("content1"),
        "file1.ignore should not be modified. Content: {}",
        file1_content
    );

    let file2_content =
        fs::read_to_string(workspace.repo_path.join("file2.ignore")).expect("Failed to read file2");
    assert!(
        file2_content.contains("content2"),
        "file2.ignore should not be modified. Content: {}",
        file2_content
    );
}
