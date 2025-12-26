mod common;
mod e2e_helpers;

use common::{get_branch_commits, get_treebeard_path, git_commit_count, TestWorkspace};
use e2e_helpers::{send_signal, spawn_treebeard_test_mode};
use nix::sys::signal;
use std::fs;
use std::process::Command;
use std::thread;
use std::time::Duration;

#[test]
fn test_ctrl_c_graceful_cleanup_preserves_changes() {
    let workspace = TestWorkspace::new();
    workspace.switch_to_repo();

    let branch_name = "sh-graceful-preserve";

    let mut treebeard = spawn_treebeard_test_mode(branch_name, &workspace.repo_path);

    let mount_dir = workspace.get_mount_path(branch_name);

    let test_file = mount_dir.join("to_be_preserved.txt");
    fs::write(&test_file, "important data").expect("Failed to write file");

    thread::sleep(Duration::from_millis(700));

    send_signal(&treebeard, signal::Signal::SIGINT);

    let status = treebeard.wait().expect("Failed to wait for treebeard");

    assert!(status.success(), "Process should exit gracefully");
    std::env::remove_var("TREEBEARD_TEST_MODE");
    workspace.restore_dir();

    let commit_count = git_commit_count(&workspace.repo_path, branch_name);
    assert!(
        commit_count >= 1,
        "Expected at least 1 commit after graceful cleanup, got {}",
        commit_count
    );

    let output = Command::new("git")
        .args(["show", &format!("{}:to_be_preserved.txt", branch_name)])
        .current_dir(&workspace.repo_path)
        .output()
        .expect("Failed to read file from git");

    assert!(
        output.status.success(),
        "File should be committed to branch"
    );
    let content = String::from_utf8_lossy(&output.stdout);
    assert_eq!(
        content.trim(),
        "important data",
        "File content should be preserved"
    );

    let output = Command::new("git")
        .args(["branch", "--list", branch_name])
        .current_dir(&workspace.repo_path)
        .output()
        .expect("Failed to check branch existence");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains(branch_name),
        "Branch should still exist after cleanup"
    );
}

#[test]
fn test_worktree_still_exists_after_cleanup() {
    let workspace = TestWorkspace::new();
    workspace.switch_to_repo();

    let branch_name = "sh-worktree-preservation";

    let worktree_dir = workspace.get_worktree_path(branch_name);

    assert!(
        !worktree_dir.exists(),
        "Worktree should not exist before treebeard"
    );

    let mut treebeard = spawn_treebeard_test_mode(branch_name, &workspace.repo_path);

    assert!(
        worktree_dir.exists(),
        "Worktree should exist after spawning treebeard"
    );

    thread::sleep(Duration::from_millis(200));

    send_signal(&treebeard, signal::Signal::SIGINT);

    let status = treebeard.wait().expect("Failed to wait for treebeard");

    assert!(status.success(), "Process should exit gracefully");
    std::env::remove_var("TREEBEARD_TEST_MODE");

    thread::sleep(Duration::from_millis(200));
    workspace.restore_dir();

    let output = Command::new("git")
        .args(["worktree", "list"])
        .current_dir(&workspace.repo_path)
        .output()
        .expect("Failed to list worktrees");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains(&worktree_dir.to_string_lossy().to_string()),
        "Worktree should remain after cleanup (user chose not to delete)"
    );
}

#[test]
fn test_cleanup_with_pending_uncommitted_changes() {
    let workspace = TestWorkspace::new();
    workspace.switch_to_repo();

    let branch_name = "sh-uncommitted-changes";

    let mut treebeard = spawn_treebeard_test_mode(branch_name, &workspace.repo_path);

    let mount_dir = workspace.get_mount_path(branch_name);

    let test_file = mount_dir.join("pending_file.txt");

    for i in 1..=3 {
        let content = format!("change {}", i);
        fs::write(&test_file, &content).expect("Failed to write file");
        thread::sleep(Duration::from_millis(200));
    }

    thread::sleep(Duration::from_millis(700));

    send_signal(&treebeard, signal::Signal::SIGINT);

    let status = treebeard.wait().expect("Failed to wait for treebeard");

    assert!(status.success(), "Process should exit gracefully");
    std::env::remove_var("TREEBEARD_TEST_MODE");
    workspace.restore_dir();

    let commit_count = git_commit_count(&workspace.repo_path, branch_name);
    assert!(
        commit_count >= 1,
        "Expected at least 1 commit despite rapid changes, got {}",
        commit_count
    );

    let output = Command::new("git")
        .args(["show", &format!("{}:pending_file.txt", branch_name)])
        .current_dir(&workspace.repo_path)
        .output()
        .expect("Failed to read file from git");

    assert!(
        output.status.success(),
        "Latest file state should be committed"
    );
    let content = String::from_utf8_lossy(&output.stdout);
    assert!(
        content.contains("change 3"),
        "Latest change should be committed"
    );
}

#[test]
fn test_on_exit_squash_mode_single_commit() {
    let workspace = TestWorkspace::new();
    workspace.switch_to_repo();

    let branch_name = "rs-squash-single";
    let treebeard_path = get_treebeard_path();
    let mut treebeard = Command::new(&treebeard_path)
        .arg("branch")
        .arg(branch_name)
        .current_dir(&workspace.repo_path)
        .env("TREEBEARD_TEST_MODE", "1")
        .spawn()
        .expect("Failed to spawn treebeard");

    thread::sleep(Duration::from_millis(500));

    let mount_dir = workspace.get_mount_path(branch_name);

    fs::write(mount_dir.join("file1.txt"), "content 1").expect("Failed to write file");
    thread::sleep(Duration::from_millis(700));
    fs::write(mount_dir.join("file2.txt"), "content 2").expect("Failed to write file");
    thread::sleep(Duration::from_millis(700));
    fs::write(mount_dir.join("file3.txt"), "content 3").expect("Failed to write file");
    thread::sleep(Duration::from_millis(700));

    send_signal(&treebeard, signal::Signal::SIGINT);
    let status = treebeard.wait().expect("Failed to wait for treebeard");
    assert!(status.success(), "Process should exit gracefully");
    std::env::remove_var("TREEBEARD_TEST_MODE");
    workspace.restore_dir();

    let commits = get_branch_commits(&workspace.repo_path, branch_name);
    assert!(
        commits.len() <= 4,
        "After squash, commits should be <= 4. Actual: {}",
        commits.len()
    );
    assert!(
        commits.len() >= 2,
        "Should have at least 2 commits. Actual: {}",
        commits.len()
    );

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
fn test_on_exit_keep_mode_multiple_commits() {
    let workspace = TestWorkspace::new();
    workspace.switch_to_repo();

    let branch_name = "rs-keep-multiple";
    let treebeard_path = get_treebeard_path();
    let mut treebeard = Command::new(&treebeard_path)
        .arg("branch")
        .arg(branch_name)
        .current_dir(&workspace.repo_path)
        .env("TREEBEARD_TEST_MODE", "1")
        .spawn()
        .expect("Failed to spawn treebeard");

    thread::sleep(Duration::from_millis(500));

    let mount_dir = workspace.get_mount_path(branch_name);

    for i in 1..=3 {
        fs::write(
            mount_dir.join(format!("file{}.txt", i)),
            format!("content {}", i),
        )
        .expect("Failed to write file");
        thread::sleep(Duration::from_millis(700));
    }

    send_signal(&treebeard, signal::Signal::SIGINT);
    let status = treebeard.wait().expect("Failed to wait for treebeard");
    assert!(status.success(), "Process should exit gracefully");
    std::env::remove_var("TREEBEARD_TEST_MODE");
    workspace.restore_dir();

    let commits = get_branch_commits(&workspace.repo_path, branch_name);
    assert!(
        commits.len() >= 2,
        "Should have at least 2 commits (initial + auto-commits), got {}",
        commits.len()
    );
}

#[test]
fn test_on_exit_default_squash_behavior() {
    let workspace = TestWorkspace::new();
    workspace.switch_to_repo();

    let branch_name = "rs-default-squash";
    let treebeard_path = get_treebeard_path();
    let mut treebeard = Command::new(&treebeard_path)
        .arg("branch")
        .arg(branch_name)
        .current_dir(&workspace.repo_path)
        .env("TREEBEARD_TEST_MODE", "1")
        .spawn()
        .expect("Failed to spawn treebeard");

    thread::sleep(Duration::from_millis(500));

    let mount_dir = workspace.get_mount_path(branch_name);

    fs::write(mount_dir.join("test.txt"), "test content").expect("Failed to write file");
    thread::sleep(Duration::from_millis(700));

    send_signal(&treebeard, signal::Signal::SIGINT);
    let status = treebeard.wait().expect("Failed to wait for treebeard");
    assert!(status.success(), "Process should exit gracefully");
    std::env::remove_var("TREEBEARD_TEST_MODE");
    workspace.restore_dir();

    let commits = get_branch_commits(&workspace.repo_path, branch_name);
    assert_eq!(
        commits.len(),
        2,
        "Default squash should result in 2 commits (initial + squashed)"
    );
}

#[test]
fn test_squash_succeeds_without_warning() {
    let workspace = TestWorkspace::new();
    workspace.switch_to_repo();

    fs::create_dir_all(&workspace.config_dir).expect("Failed to create config dir");
    fs::write(
        workspace.config_dir.join("config.toml"),
        "[cleanup]\non_exit = \"squash\"\n",
    )
    .expect("Failed to write config");

    let branch_name = "rs-squash-no-warning";
    let treebeard_path = get_treebeard_path();

    let treebeard = Command::new(&treebeard_path)
        .arg("branch")
        .arg(branch_name)
        .current_dir(&workspace.repo_path)
        .env("TREEBEARD_TEST_MODE", "1")
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("Failed to spawn treebeard");

    thread::sleep(Duration::from_millis(500));

    let mount_dir = workspace.get_mount_path(branch_name);

    fs::write(mount_dir.join("test1.txt"), "content 1").expect("Failed to write file");
    thread::sleep(Duration::from_millis(700));
    fs::write(mount_dir.join("test2.txt"), "content 2").expect("Failed to write file");
    thread::sleep(Duration::from_millis(700));

    send_signal(&treebeard, signal::Signal::SIGINT);

    let output = treebeard
        .wait_with_output()
        .expect("Failed to wait for treebeard");
    std::env::remove_var("TREEBEARD_TEST_MODE");
    workspace.restore_dir();

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined_output = format!("{}{}", stdout, stderr);

    assert!(
        combined_output.contains("Commits squashed"),
        "Output should contain 'Commits squashed' indicating success. Output:\n{}",
        combined_output
    );

    assert!(
        !combined_output.contains("Warning: Failed to squash"),
        "Output should NOT contain squash failure warning. Output:\n{}",
        combined_output
    );

    let file_output = Command::new("git")
        .args(["ls-tree", "-r", branch_name, "--name-only"])
        .current_dir(&workspace.repo_path)
        .output()
        .expect("Failed to list files in branch");

    let files = String::from_utf8_lossy(&file_output.stdout);
    assert!(
        files.contains("test1.txt"),
        "test1.txt should exist after squash"
    );
    assert!(
        files.contains("test2.txt"),
        "test2.txt should exist after squash"
    );
}

#[test]
fn test_file_operations_parametrized_by_file_count() {
    let workspace = TestWorkspace::new();
    workspace.switch_to_repo();

    let branch_name = "rs-file-count-param";
    let mut treebeard = spawn_treebeard_test_mode(branch_name, &workspace.repo_path);

    let mount_dir = workspace.get_mount_path(branch_name);

    for i in 1..=5 {
        fs::write(
            mount_dir.join(format!("param_file_{}.txt", i)),
            format!("content {}", i),
        )
        .expect("Failed to write file");
        thread::sleep(Duration::from_millis(200));
    }

    thread::sleep(Duration::from_millis(700));
    send_signal(&treebeard, signal::Signal::SIGINT);
    let status = treebeard.wait().expect("Failed to wait for treebeard");
    assert!(status.success(), "Process should exit gracefully");
    std::env::remove_var("TREEBEARD_TEST_MODE");
    workspace.restore_dir();

    let output = Command::new("git")
        .args(["ls-tree", "-r", branch_name, "--name-only"])
        .current_dir(&workspace.repo_path)
        .output()
        .expect("Failed to list files in branch");

    let files = String::from_utf8_lossy(&output.stdout);
    for i in 1..=5 {
        assert!(
            files.contains(&format!("param_file_{}.txt", i)),
            "File param_file_{}.txt should exist",
            i
        );
    }
}

#[test]
fn test_cleanup_preserves_branch_with_empty_worktree() {
    let workspace = TestWorkspace::new();
    workspace.switch_to_repo();

    let branch_name = "rs-empty-worktree";
    let mut treebeard = spawn_treebeard_test_mode(branch_name, &workspace.repo_path);

    let mount_dir = workspace.get_mount_path(branch_name);

    let test_file = mount_dir.join("to_be_deleted.txt");
    fs::write(&test_file, "temporary").expect("Failed to write file");
    thread::sleep(Duration::from_millis(700));
    fs::remove_file(&test_file).expect("Failed to delete file");
    thread::sleep(Duration::from_millis(700));

    send_signal(&treebeard, signal::Signal::SIGINT);
    let status = treebeard.wait().expect("Failed to wait for treebeard");
    assert!(status.success(), "Process should exit gracefully");
    std::env::remove_var("TREEBEARD_TEST_MODE");
    workspace.restore_dir();

    let output = Command::new("git")
        .args(["branch", "--list", branch_name])
        .current_dir(&workspace.repo_path)
        .output()
        .expect("Failed to check branch existence");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains(branch_name), "Branch should still exist");
}
