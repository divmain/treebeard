//! Happy path tests for branch creation.

use crate::shared::common::{get_treebeard_path, TestWorkspace};
use expectrl::spawn;
use expectrl::{Eof, Expect};

#[test]
fn test_treebeard_creates_branch() {
    let treebeard_path = get_treebeard_path();
    let workspace = TestWorkspace::new();
    workspace.switch_to_repo();

    let branch_name = "e2e-creation-branch";
    let mut p = spawn(format!(
        "{} branch {} --no-shell",
        treebeard_path.display(),
        branch_name
    ))
    .expect("Failed to spawn treebeard");

    p.expect(Eof)
        .expect("Failed to wait for treebeard to complete");

    let output = std::process::Command::new("git")
        .args(["branch"])
        .current_dir(&workspace.repo_path)
        .output()
        .expect("Failed to check for branch");

    assert!(
        output.status.success(),
        "Branch check command should succeed"
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains(branch_name),
        "Branch '{}' not found in branches: {}",
        branch_name,
        stdout
    );

    workspace.restore_dir();
}
