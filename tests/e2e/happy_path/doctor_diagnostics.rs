//! Happy path tests for doctor diagnostics.

use crate::shared::common::{get_treebeard_path, TestWorkspace};
use std::process::Command;

#[test]
fn test_doctor_command_runs_successfully() {
    let treebeard_path = get_treebeard_path();
    let workspace = TestWorkspace::new();

    let output = Command::new(&treebeard_path)
        .arg("doctor")
        .stdin(std::process::Stdio::null())
        .current_dir(&workspace.repo_path)
        .output()
        .expect("Failed to run treebeard doctor");

    assert!(
        output.status.success(),
        "doctor should succeed. stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn test_doctor_shows_diagnostics_header() {
    let treebeard_path = get_treebeard_path();
    let workspace = TestWorkspace::new();

    let output = Command::new(&treebeard_path)
        .arg("doctor")
        .stdin(std::process::Stdio::null())
        .current_dir(&workspace.repo_path)
        .output()
        .expect("Failed to run treebeard doctor");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("Treebeard Diagnostics"),
        "Should show diagnostics header. stdout: {}",
        stdout
    );
}

#[test]
fn test_doctor_checks_macos_version() {
    let treebeard_path = get_treebeard_path();
    let workspace = TestWorkspace::new();

    let output = Command::new(&treebeard_path)
        .arg("doctor")
        .stdin(std::process::Stdio::null())
        .current_dir(&workspace.repo_path)
        .output()
        .expect("Failed to run treebeard doctor");

    let stdout = String::from_utf8_lossy(&output.stdout);
    // Should contain macOS version info (either supported or error)
    assert!(
        stdout.contains("macOS") || stdout.contains("Operating System"),
        "Should show macOS version check. stdout: {}",
        stdout
    );
}

#[test]
fn test_doctor_checks_macfuse() {
    let treebeard_path = get_treebeard_path();
    let workspace = TestWorkspace::new();

    let output = Command::new(&treebeard_path)
        .arg("doctor")
        .stdin(std::process::Stdio::null())
        .current_dir(&workspace.repo_path)
        .output()
        .expect("Failed to run treebeard doctor");

    let stdout = String::from_utf8_lossy(&output.stdout);
    // Should contain macFUSE info
    assert!(
        stdout.contains("macFUSE"),
        "Should show macFUSE check. stdout: {}",
        stdout
    );
}

#[test]
fn test_doctor_checks_git() {
    let treebeard_path = get_treebeard_path();
    let workspace = TestWorkspace::new();

    let output = Command::new(&treebeard_path)
        .arg("doctor")
        .stdin(std::process::Stdio::null())
        .current_dir(&workspace.repo_path)
        .output()
        .expect("Failed to run treebeard doctor");

    let stdout = String::from_utf8_lossy(&output.stdout);
    // Should contain Git info
    assert!(
        stdout.contains("Git"),
        "Should show Git check. stdout: {}",
        stdout
    );
    assert!(
        stdout.contains("worktree"),
        "Should show worktree support info. stdout: {}",
        stdout
    );
}

#[test]
fn test_doctor_checks_config_file() {
    let treebeard_path = get_treebeard_path();
    let workspace = TestWorkspace::new();

    let output = Command::new(&treebeard_path)
        .arg("doctor")
        .stdin(std::process::Stdio::null())
        .current_dir(&workspace.repo_path)
        .output()
        .expect("Failed to run treebeard doctor");

    let stdout = String::from_utf8_lossy(&output.stdout);
    // Should contain config file info
    assert!(
        stdout.contains("Config file"),
        "Should show config file check. stdout: {}",
        stdout
    );
}

#[test]
fn test_doctor_checks_stale_mounts() {
    let treebeard_path = get_treebeard_path();
    let workspace = TestWorkspace::new();

    let output = Command::new(&treebeard_path)
        .arg("doctor")
        .stdin(std::process::Stdio::null())
        .current_dir(&workspace.repo_path)
        .output()
        .expect("Failed to run treebeard doctor");

    let stdout = String::from_utf8_lossy(&output.stdout);
    // Should contain stale mounts info
    assert!(
        stdout.contains("Stale mounts"),
        "Should show stale mounts check. stdout: {}",
        stdout
    );
}

#[test]
fn test_doctor_checks_disk_space() {
    let treebeard_path = get_treebeard_path();
    let workspace = TestWorkspace::new();

    let output = Command::new(&treebeard_path)
        .arg("doctor")
        .stdin(std::process::Stdio::null())
        .current_dir(&workspace.repo_path)
        .output()
        .expect("Failed to run treebeard doctor");

    let stdout = String::from_utf8_lossy(&output.stdout);
    // Should contain disk space info
    assert!(
        stdout.contains("Disk space"),
        "Should show disk space check. stdout: {}",
        stdout
    );
}

#[test]
fn test_doctor_checks_active_sessions() {
    let treebeard_path = get_treebeard_path();
    let workspace = TestWorkspace::new();

    let output = Command::new(&treebeard_path)
        .arg("doctor")
        .stdin(std::process::Stdio::null())
        .current_dir(&workspace.repo_path)
        .output()
        .expect("Failed to run treebeard doctor");

    let stdout = String::from_utf8_lossy(&output.stdout);
    // Should contain active sessions info
    assert!(
        stdout.contains("Active sessions"),
        "Should show active sessions check. stdout: {}",
        stdout
    );
}

#[test]
fn test_doctor_uses_status_symbols() {
    let treebeard_path = get_treebeard_path();
    let workspace = TestWorkspace::new();

    let output = Command::new(&treebeard_path)
        .arg("doctor")
        .stdin(std::process::Stdio::null())
        .current_dir(&workspace.repo_path)
        .output()
        .expect("Failed to run treebeard doctor");

    let stdout = String::from_utf8_lossy(&output.stdout);
    // Should contain at least one status symbol (checkmark, warning, or error)
    let has_checkmark = stdout.contains("\u{2713}"); // checkmark
    let has_warning = stdout.contains("\u{26a0}"); // warning sign
    let has_error = stdout.contains("\u{2717}"); // X mark

    assert!(
        has_checkmark || has_warning || has_error,
        "Should use status symbols. stdout: {}",
        stdout
    );
}
