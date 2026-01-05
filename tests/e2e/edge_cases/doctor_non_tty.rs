//! Edge case tests for doctor command without TTY.

use crate::shared::common::{get_treebeard_path, TestWorkspace};
use std::process::Command;

#[test]
fn test_doctor_without_tty() {
    let treebeard_path = get_treebeard_path();
    let workspace = TestWorkspace::new();

    // Doctor command should work without TTY
    let output = Command::new(&treebeard_path)
        .arg("doctor")
        .stdin(std::process::Stdio::null())
        .current_dir(&workspace.repo_path)
        .output()
        .expect("Failed to run treebeard doctor");

    assert!(
        output.status.success(),
        "doctor should succeed without TTY. stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}
