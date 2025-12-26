mod common;
mod e2e_helpers;

use common::{get_treebeard_path, TestWorkspace};
use e2e_helpers::{spawn_treebeard_test_mode, terminate_treebeard};
use expectrl::{spawn, Eof, Expect};
use std::process::Command;

#[test]
fn test_multiple_sessions_sequential() {
    let treebeard_path = get_treebeard_path();
    let workspace = TestWorkspace::new();
    workspace.switch_to_repo();

    for i in 1..=3 {
        let branch_name = format!("e2e-session-{}", i);
        let mut p = spawn(format!(
            "{} branch {} --no-shell",
            treebeard_path.display(),
            branch_name
        ))
        .expect("Failed to spawn treebeard session");

        p.expect(Eof)
            .expect("Failed to wait for treebeard to complete");

        assert!(
            !workspace.repo_path.join(".treebeard").exists(),
            "Should clean up worktree after each session"
        );
    }

    workspace.restore_dir();
}

#[test]
fn test_list_active_sessions() {
    let workspace = TestWorkspace::new();
    workspace.switch_to_repo();

    let branch_name = "ni-list-single-session";

    let treebeard = spawn_treebeard_test_mode(branch_name, &workspace.repo_path);

    let treebeard_path = get_treebeard_path();
    let output = Command::new(&treebeard_path)
        .arg("list")
        .current_dir(&workspace.repo_path)
        .output()
        .expect("Failed to run list command");

    assert!(output.status.success(), "List command should succeed");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains(branch_name),
        "Output should contain branch name '{}'. Output: {}",
        branch_name,
        stdout
    );

    terminate_treebeard(treebeard);
}

#[test]
fn test_list_with_no_active_sessions() {
    let workspace = TestWorkspace::new();
    workspace.switch_to_repo();

    let treebeard_path = get_treebeard_path();
    let output = Command::new(&treebeard_path)
        .arg("list")
        .current_dir(&workspace.repo_path)
        .output()
        .expect("Failed to run list command");

    assert!(output.status.success(), "List command should succeed");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("No active worktrees") || stdout.contains("(No active worktrees)"),
        "Output should indicate no active worktrees. Output: {}",
        stdout
    );
}

#[test]
fn test_list_multiple_concurrent_sessions() {
    let workspace = TestWorkspace::new();
    workspace.switch_to_repo();

    let treebeard_path = get_treebeard_path();
    let branch_names = vec!["ni-multi-1", "ni-multi-2", "ni-multi-3"];

    let mut children = Vec::new();
    for branch_name in &branch_names {
        let child = spawn_treebeard_test_mode(branch_name, &workspace.repo_path);
        children.push((branch_name.to_string(), child));
    }

    let output = Command::new(&treebeard_path)
        .arg("list")
        .current_dir(&workspace.repo_path)
        .output()
        .expect("Failed to run list command");

    assert!(output.status.success(), "List command should succeed");

    let stdout = String::from_utf8_lossy(&output.stdout);
    for branch_name in &branch_names {
        assert!(
            stdout.contains(branch_name),
            "Output should contain branch name '{}'. Output: {}",
            branch_name,
            stdout
        );
    }

    for (_, child) in children {
        terminate_treebeard(child);
    }
}

#[test]
fn test_cleanup_existing_branch() {
    let workspace = TestWorkspace::new();
    workspace.switch_to_repo();

    let branch_name = "ni-cleanup-existing";

    let treebeard = spawn_treebeard_test_mode(branch_name, &workspace.repo_path);

    std::thread::sleep(std::time::Duration::from_millis(200));

    let worktree_dir = workspace.get_worktree_path(branch_name);
    assert!(
        worktree_dir.exists(),
        "Worktree should exist before cleanup"
    );

    terminate_treebeard(treebeard);
    std::thread::sleep(std::time::Duration::from_millis(200));

    let treebeard_path = get_treebeard_path();
    let output = Command::new(&treebeard_path)
        .args(["cleanup", branch_name])
        .current_dir(&workspace.repo_path)
        .output()
        .expect("Failed to run cleanup command");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains(&format!("Worktree for '{}' does not exist", branch_name))
            || stdout.contains(branch_name),
        "Cleanup command should provide output about the worktree. Output: {}",
        stdout
    );
}

#[test]
fn test_cleanup_non_existent_branch() {
    let workspace = TestWorkspace::new();
    workspace.switch_to_repo();

    let branch_name = "ni-cleanup-nonexistent";

    let treebeard_path = get_treebeard_path();
    let output = Command::new(&treebeard_path)
        .args(["cleanup", branch_name])
        .current_dir(&workspace.repo_path)
        .output()
        .expect("Failed to run cleanup command");

    assert!(
        output.status.success(),
        "Cleanup command should succeed for non-existent branch"
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains(&format!("Worktree for '{}' does not exist", branch_name))
            || stdout.contains(branch_name),
        "Output should indicate worktree does not exist. Output: {}",
        stdout
    );
}

#[test]
fn test_list_porcelain_format() {
    let workspace = TestWorkspace::new();
    workspace.switch_to_repo();

    let branch_name = "ni-porcelain-format";

    let treebeard = spawn_treebeard_test_mode(branch_name, &workspace.repo_path);

    let treebeard_path = get_treebeard_path();
    let output = Command::new(&treebeard_path)
        .args(["list", "--porcelain"])
        .current_dir(&workspace.repo_path)
        .output()
        .expect("Failed to run list command");

    if !output.status.success() {
        eprintln!("List command failed with status: {:?}", output.status);
        eprintln!("stdout: {}", String::from_utf8_lossy(&output.stdout));
        eprintln!("stderr: {}", String::from_utf8_lossy(&output.stderr));
    }

    assert!(output.status.success(), "List command should succeed");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let lines: Vec<&str> = stdout.lines().collect();

    assert!(
        !lines.is_empty(),
        "Porcelain output should have at least one line"
    );

    let first_line = lines.first().expect("Should have first line");
    let parts: Vec<&str> = first_line.split('\t').collect();

    assert_eq!(
        parts.len(),
        4,
        "Porcelain output should have 4 tab-separated fields. Found: {:?}",
        parts
    );

    assert_eq!(parts[0], branch_name, "First field should be branch name");
    assert!(
        parts[1].contains(branch_name),
        "Second field should contain mount path with branch name"
    );
    assert!(
        parts[2] == "mounted" || parts[2] == "unmounted",
        "Third field should be 'mounted' or 'unmounted'"
    );
    assert!(
        parts[3].parse::<usize>().is_ok(),
        "Fourth field should be a number (dirty files count)"
    );

    terminate_treebeard(treebeard);
}

#[test]
fn test_list_json_format() {
    let workspace = TestWorkspace::new();
    workspace.switch_to_repo();

    let branch_name = "ni-json-format";

    let treebeard = spawn_treebeard_test_mode(branch_name, &workspace.repo_path);

    let treebeard_path = get_treebeard_path();
    let output = Command::new(&treebeard_path)
        .args(["list", "--json"])
        .current_dir(&workspace.repo_path)
        .output()
        .expect("Failed to run list command");

    assert!(output.status.success(), "List command should succeed");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let sessions: Vec<serde_json::Value> =
        serde_json::from_str(&stdout).expect("JSON output should be valid JSON array");

    assert!(
        !sessions.is_empty(),
        "JSON output should have at least one session"
    );

    let session = sessions.first().expect("Should have first session");
    assert_eq!(session["branch"], branch_name, "Branch field should match");
    assert!(
        session["mount_path"].is_string(),
        "Mount path should be a string"
    );
    assert!(
        session["status"] == "mounted" || session["status"] == "unmounted",
        "Status should be 'mounted' or 'unmounted'"
    );
    assert!(
        session["dirty_files"].is_number() || session["dirty_files"].is_u64(),
        "Dirty files should be a number"
    );

    terminate_treebeard(treebeard);
}

#[test]
fn test_list_porcelain_format_multiple_sessions() {
    let workspace = TestWorkspace::new();
    workspace.switch_to_repo();

    let treebeard_path = get_treebeard_path();
    let branch_names = vec!["ni-porcelain-1", "ni-porcelain-2", "ni-porcelain-3"];

    let mut children = Vec::new();
    for branch_name in &branch_names {
        let child = spawn_treebeard_test_mode(branch_name, &workspace.repo_path);
        children.push((branch_name.to_string(), child));
    }

    let output = Command::new(&treebeard_path)
        .args(["list", "--porcelain"])
        .current_dir(&workspace.repo_path)
        .output()
        .expect("Failed to run list command");

    if !output.status.success() {
        eprintln!("List command failed with status: {:?}", output.status);
        eprintln!("stdout: {}", String::from_utf8_lossy(&output.stdout));
        eprintln!("stderr: {}", String::from_utf8_lossy(&output.stderr));
    }

    assert!(output.status.success(), "List command should succeed");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let lines: Vec<&str> = stdout.lines().filter(|line| !line.is_empty()).collect();

    assert_eq!(
        lines.len(),
        branch_names.len(),
        "Should have one line per branch. Expected {}, got {}",
        branch_names.len(),
        lines.len()
    );

    for branch_name in &branch_names {
        let found = lines.iter().any(|line| line.starts_with(branch_name));
        assert!(
            found,
            "Should find branch '{}' in porcelain output. Lines: {:?}",
            branch_name, lines
        );
    }

    for (_, child) in children {
        terminate_treebeard(child);
    }
}

#[test]
fn test_list_default_format_still_works() {
    let workspace = TestWorkspace::new();
    workspace.switch_to_repo();

    let branch_name = "ni-default-format";

    let treebeard = spawn_treebeard_test_mode(branch_name, &workspace.repo_path);

    let treebeard_path = get_treebeard_path();
    let output = Command::new(&treebeard_path)
        .arg("list")
        .current_dir(&workspace.repo_path)
        .output()
        .expect("Failed to run list command");

    assert!(output.status.success(), "List command should succeed");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains(branch_name),
        "Output should contain branch name '{}'. Output: {}",
        branch_name,
        stdout
    );
    assert!(
        stdout.contains("Branch:") || stdout.contains("Active sessions"),
        "Output should contain human-readable format indicators"
    );

    terminate_treebeard(treebeard);
}
