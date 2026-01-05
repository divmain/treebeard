//! Happy path tests for FUSE overlay operations.

use crate::shared::common::{create_test_file, get_treebeard_path, TestWorkspace};
use crate::shared::e2e_helpers::send_signal;
use nix::sys::signal;
use std::fs;
use std::io::Write;
use std::process::Command;
use std::thread;
use std::time::Duration;

#[test]
fn test_append_to_ignored_file_visible_through_mount() {
    let workspace = TestWorkspace::new();

    create_test_file(&workspace.repo_path, ".gitignore", "*.ignore\n");
    create_test_file(&workspace.repo_path, "example.ignore", "original content\n");

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

    let branch_name = "fo-append-ignored";
    let treebeard_path = get_treebeard_path();

    let mut treebeard = Command::new(&treebeard_path)
        .arg("branch")
        .arg(branch_name)
        .current_dir(&workspace.repo_path)
        .env("TREEBEARD_TEST_MODE", "1")
        .spawn()
        .expect("Failed to spawn treebeard");

    thread::sleep(Duration::from_millis(800));

    let mount_point = workspace
        .worktree_base_dir
        .join("mounts")
        .join("test-repo")
        .join(branch_name);

    assert!(mount_point.exists(), "Mount point should exist");

    let ignored_file = mount_point.join("example.ignore");

    let original_content = fs::read_to_string(&ignored_file).expect("Failed to read original file");
    assert!(
        original_content.contains("original content"),
        "Should see original content"
    );

    let mut file = fs::OpenOptions::new()
        .append(true)
        .open(&ignored_file)
        .expect("Failed to open file for append");
    file.write_all(b"appended content\n")
        .expect("Failed to append");
    file.flush().expect("Failed to flush");
    drop(file);

    thread::sleep(Duration::from_millis(100));

    let updated_content = fs::read_to_string(&ignored_file).expect("Failed to read updated file");
    assert!(
        updated_content.contains("original content"),
        "Should still see original content"
    );
    assert!(
        updated_content.contains("appended content"),
        "Should see appended content: got: {}",
        updated_content
    );

    let metadata = fs::metadata(&ignored_file).expect("Failed to get metadata");
    assert_eq!(
        metadata.len(),
        updated_content.len() as u64,
        "File size should match content length"
    );

    send_signal(&treebeard, signal::Signal::SIGINT);
    let _ = treebeard.wait();
    std::env::remove_var("TREEBEARD_TEST_MODE");
    workspace.restore_dir();
}

#[test]
fn test_multiple_writes_accumulate_correctly_through_mount() {
    let workspace = TestWorkspace::new();

    create_test_file(&workspace.repo_path, ".gitignore", "*.log\n");
    create_test_file(&workspace.repo_path, "test.log", "line1\n");

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

    let branch_name = "fo-multi-write";
    let treebeard_path = get_treebeard_path();

    let mut treebeard = Command::new(&treebeard_path)
        .arg("branch")
        .arg(branch_name)
        .current_dir(&workspace.repo_path)
        .env("TREEBEARD_TEST_MODE", "1")
        .spawn()
        .expect("Failed to spawn treebeard");

    thread::sleep(Duration::from_millis(800));

    let mount_point = workspace
        .worktree_base_dir
        .join("mounts")
        .join("test-repo")
        .join(branch_name);

    let log_file = mount_point.join("test.log");

    for i in 2..=4 {
        let mut file = fs::OpenOptions::new()
            .append(true)
            .open(&log_file)
            .expect("Failed to open file for append");
        writeln!(file, "line{}", i).expect("Failed to write line");
        file.flush().expect("Failed to flush");
        drop(file);
        thread::sleep(Duration::from_millis(50));
    }

    thread::sleep(Duration::from_millis(100));

    let content = fs::read_to_string(&log_file).expect("Failed to read file");
    let line_count = content.lines().count();
    assert_eq!(line_count, 4, "Should have 4 lines, got: {}", content);

    assert!(content.contains("line1"), "Should see line1");
    assert!(content.contains("line2"), "Should see line2");
    assert!(content.contains("line3"), "Should see line3");
    assert!(content.contains("line4"), "Should see line4");

    send_signal(&treebeard, signal::Signal::SIGINT);
    let _ = treebeard.wait();
    std::env::remove_var("TREEBEARD_TEST_MODE");
    workspace.restore_dir();
}

#[test]
fn test_truncate_and_write_updates_size_through_mount() {
    let workspace = TestWorkspace::new();

    create_test_file(&workspace.repo_path, ".gitignore", "*.dat\n");
    create_test_file(
        &workspace.repo_path,
        "data.dat",
        "this is a long original content that will be truncated\n",
    );

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

    let branch_name = "fo-truncate";
    let treebeard_path = get_treebeard_path();

    let mut treebeard = Command::new(&treebeard_path)
        .arg("branch")
        .arg(branch_name)
        .current_dir(&workspace.repo_path)
        .env("TREEBEARD_TEST_MODE", "1")
        .spawn()
        .expect("Failed to spawn treebeard");

    thread::sleep(Duration::from_millis(800));

    let mount_point = workspace
        .worktree_base_dir
        .join("mounts")
        .join("test-repo")
        .join(branch_name);

    let data_file = mount_point.join("data.dat");

    fs::write(&data_file, "short\n").expect("Failed to write truncated content");

    thread::sleep(Duration::from_millis(100));

    let content = fs::read_to_string(&data_file).expect("Failed to read file");
    assert_eq!(
        content, "short\n",
        "Should see only new content: {}",
        content
    );

    let metadata = fs::metadata(&data_file).expect("Failed to get metadata");
    assert_eq!(metadata.len(), 6, "File should be 6 bytes");

    send_signal(&treebeard, signal::Signal::SIGINT);
    let _ = treebeard.wait();
    std::env::remove_var("TREEBEARD_TEST_MODE");
    workspace.restore_dir();
}

#[test]
fn test_mount_read_consistency_with_worktree() {
    let workspace = TestWorkspace::new();

    create_test_file(&workspace.repo_path, ".gitignore", "*.tmp\n");
    create_test_file(&workspace.repo_path, "cache.tmp", "initial\n");

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

    let branch_name = "fo-consistency";
    let treebeard_path = get_treebeard_path();

    let mut treebeard = Command::new(&treebeard_path)
        .arg("branch")
        .arg(branch_name)
        .current_dir(&workspace.repo_path)
        .env("TREEBEARD_TEST_MODE", "1")
        .spawn()
        .expect("Failed to spawn treebeard");

    thread::sleep(Duration::from_millis(800));

    let mount_point = workspace
        .worktree_base_dir
        .join("mounts")
        .join("test-repo")
        .join(branch_name);

    let worktree_dir = workspace.get_worktree_path(branch_name);

    let mount_file = mount_point.join("cache.tmp");
    let worktree_file = worktree_dir.join("cache.tmp");

    let mut file = fs::OpenOptions::new()
        .append(true)
        .open(&mount_file)
        .expect("Failed to open file for append");
    file.write_all(b"updated content here\n")
        .expect("Failed to write");
    file.flush().expect("Failed to flush");
    drop(file);

    thread::sleep(Duration::from_millis(100));

    let mount_content = fs::read_to_string(&mount_file).expect("Failed to read from mount");
    assert!(
        mount_content.contains("initial"),
        "Should see initial content through mount"
    );
    assert!(
        mount_content.contains("updated content"),
        "Should see updated content through mount"
    );

    let metadata = fs::metadata(&mount_file).expect("Failed to get metadata");
    assert_eq!(metadata.len(), 29, "File should be 29 bytes through mount");

    let worktree_content =
        fs::read_to_string(&worktree_file).expect("Failed to read from worktree");
    assert_eq!(
        mount_content, worktree_content,
        "Mount and worktree content should match"
    );

    send_signal(&treebeard, signal::Signal::SIGINT);
    let _ = treebeard.wait();
    std::env::remove_var("TREEBEARD_TEST_MODE");
    workspace.restore_dir();
}
