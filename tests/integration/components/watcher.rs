use crate::shared::common::create_test_repo;

use std::fs;
use std::path::PathBuf;
use std::sync::atomic::AtomicUsize;
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio::time::{sleep, Duration};
use treebeard::git::GitRepo;

/// Test that the watcher commits when it receives mutation signals via channel
#[tokio::test]
async fn test_watcher_commits_on_channel_signal() {
    let (_temp_dir, repo_path) = create_test_repo();

    let repo = GitRepo::from_path(&repo_path).expect("Failed to discover repo");

    let worktree_path = repo_path.join(".treebeard-test-watcher1");

    repo.create_branch("test-watcher1")
        .expect("Failed to create branch");
    repo.create_worktree("test-watcher1", &worktree_path)
        .expect("Failed to create worktree");

    // Create a channel to simulate FUSE mutation signals
    let (tx, rx) = mpsc::unbounded_channel::<PathBuf>();

    // Create a file in the worktree (simulating what would happen through FUSE)
    let test_file = worktree_path.join("new-file.txt");
    fs::write(&test_file, "test content").expect("Failed to write test file");

    let worktree_repo = GitRepo::from_path(&worktree_path).expect("Failed to get worktree repo");

    let failure_count = Arc::new(AtomicUsize::new(0));

    // Spawn the watcher task
    // 200ms debounce is short enough for fast tests but long enough to batch signals
    let watcher_handle = tokio::spawn(async move {
        treebeard::watcher::watch_and_commit(
            rx,
            &worktree_repo,
            200,
            "treebeard: test auto-commit",
            failure_count,
        )
        .await
    });

    // Send a mutation signal through the channel
    tx.send(PathBuf::from("new-file.txt"))
        .expect("Failed to send signal");

    // Wait for debounce (200ms) + buffer for commit execution
    sleep(Duration::from_millis(500)).await;

    // Drop the sender to close the channel and end the watcher
    drop(tx);

    // Wait for the watcher to finish
    let _ = watcher_handle.await;

    // Check if commit was made
    let output = std::process::Command::new("git")
        .args(["log", "--oneline", "test-watcher1"])
        .current_dir(&repo_path)
        .output()
        .expect("Failed to get log");

    let log = String::from_utf8_lossy(&output.stdout);
    assert!(
        log.contains("treebeard: test auto-commit"),
        "Expected auto-commit in log, got: {}",
        log
    );
}

/// Test that the watcher debounces rapid mutation signals
#[tokio::test]
async fn test_watcher_debounces_rapid_signals() {
    let (_temp_dir, repo_path) = create_test_repo();

    let repo = GitRepo::from_path(&repo_path).expect("Failed to discover repo");

    let worktree_path = repo_path.join(".treebeard-test-watcher2");

    repo.create_branch("test-watcher2")
        .expect("Failed to create branch");
    repo.create_worktree("test-watcher2", &worktree_path)
        .expect("Failed to create worktree");

    // Get initial commit count
    let initial_commit_count = {
        let output = std::process::Command::new("git")
            .args(["rev-list", "--count", "test-watcher2"])
            .current_dir(&repo_path)
            .output()
            .expect("Failed to get commit count");

        let count = String::from_utf8_lossy(&output.stdout);
        count.trim().parse::<usize>().unwrap_or(0)
    };

    // Create a channel to simulate FUSE mutation signals
    let (tx, rx) = mpsc::unbounded_channel::<PathBuf>();

    // Create a file in the worktree
    let test_file = worktree_path.join("rapid.txt");
    fs::write(&test_file, "content 5").expect("Failed to write test file");

    let worktree_repo = GitRepo::from_path(&worktree_path).expect("Failed to get worktree repo");

    let failure_count = Arc::new(AtomicUsize::new(0));

    // Spawn the watcher task
    // 300ms debounce: longer than inter-signal delays (50ms) to test batching
    let watcher_handle = tokio::spawn(async move {
        treebeard::watcher::watch_and_commit(
            rx,
            &worktree_repo,
            300,
            "treebeard: debounce test",
            failure_count,
        )
        .await
    });

    // Send multiple rapid mutation signals (simulating rapid file writes)
    for i in 1..=5 {
        tx.send(PathBuf::from("rapid.txt".to_string()))
            .expect("Failed to send signal");
        // Small delay between signals, but less than debounce period
        sleep(Duration::from_millis(50)).await;
        // Also write the file content (simulating real writes)
        fs::write(&test_file, format!("content {}", i)).expect("Failed to write test file");
    }

    // Wait for debounce period (300ms) + buffer for commit execution
    sleep(Duration::from_millis(600)).await;

    // Drop the sender to close the channel
    drop(tx);

    // Wait for the watcher to finish
    let _ = watcher_handle.await;

    // Get final commit count
    let final_commit_count = {
        let output = std::process::Command::new("git")
            .args(["rev-list", "--count", "test-watcher2"])
            .current_dir(&repo_path)
            .output()
            .expect("Failed to get commit count");

        let count = String::from_utf8_lossy(&output.stdout);
        count.trim().parse::<usize>().unwrap_or(0)
    };

    // Should have exactly 1 new commit (all rapid changes debounced into one)
    let new_commits = final_commit_count - initial_commit_count;
    assert!(
        new_commits <= 1,
        "Rapid changes should be debounced into at most 1 commit (got {} new commits)",
        new_commits
    );
}

/// Test that the watcher handles channel closure gracefully
#[tokio::test]
async fn test_watcher_handles_channel_closure() {
    let (_temp_dir, repo_path) = create_test_repo();

    let repo = GitRepo::from_path(&repo_path).expect("Failed to discover repo");

    let worktree_path = repo_path.join(".treebeard-test-watcher3");

    repo.create_branch("test-watcher3")
        .expect("Failed to create branch");
    repo.create_worktree("test-watcher3", &worktree_path)
        .expect("Failed to create worktree");

    // Create a channel
    let (tx, rx) = mpsc::unbounded_channel::<PathBuf>();

    // Create a file and send a signal
    let test_file = worktree_path.join("final.txt");
    fs::write(&test_file, "final content").expect("Failed to write test file");

    let worktree_repo = GitRepo::from_path(&worktree_path).expect("Failed to get worktree repo");

    let failure_count = Arc::new(AtomicUsize::new(0));

    // Spawn the watcher task
    let watcher_handle = tokio::spawn(async move {
        treebeard::watch_and_commit(
            rx,
            &worktree_repo,
            200, // 200ms debounce
            "treebeard: final commit",
            failure_count,
        )
        .await
    });

    // Send a signal
    tx.send(PathBuf::from("final.txt"))
        .expect("Failed to send signal");

    // Small delay then close channel (simulates FUSE unmount)
    sleep(Duration::from_millis(50)).await;
    drop(tx);

    // Wait for the watcher to finish - it should make a final commit
    let result = watcher_handle.await;
    assert!(result.is_ok(), "Watcher should complete successfully");

    // Check that the final commit was made
    let output = std::process::Command::new("git")
        .args(["log", "--oneline", "test-watcher3"])
        .current_dir(&repo_path)
        .output()
        .expect("Failed to get log");

    let log = String::from_utf8_lossy(&output.stdout);
    assert!(
        log.contains("treebeard: final commit"),
        "Expected final commit on channel closure, got: {}",
        log
    );
}
