mod common;

use common::{get_treebeard_path, TestWorkspace};
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

#[test]
fn test_branch_already_exists_message() {
    let treebeard_path = get_treebeard_path();
    let workspace = TestWorkspace::new();
    workspace.switch_to_repo();

    let branch_name = "ip-branch-exists";

    let mut p = spawn(format!(
        "{} branch {} --no-shell",
        treebeard_path.display(),
        branch_name
    ))
    .expect("Failed to spawn treebeard");

    p.expect(Eof)
        .expect("Failed to wait for treebeard to complete");

    let output = std::process::Command::new("git")
        .args(["branch", "--list", branch_name])
        .current_dir(&workspace.repo_path)
        .output()
        .expect("Failed to check for branch");

    assert!(output.status.success(), "Branch should exist");

    let output = std::process::Command::new(&treebeard_path)
        .args(["branch", branch_name, "--no-shell"])
        .current_dir(&workspace.repo_path)
        .output()
        .expect("Failed to run treebeard for existing branch");

    assert!(
        output.status.success(),
        "Command should succeed (branches can exist)"
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("already exists"),
        "Output should mention branch already exists. Output: {}",
        stdout
    );
    assert!(
        stdout.contains(branch_name),
        "Output should mention the branch name. Output: {}",
        stdout
    );

    workspace.restore_dir();
}

#[test]
fn test_worktree_already_exists_message() {
    let treebeard_path = get_treebeard_path();
    let workspace = TestWorkspace::new();
    workspace.switch_to_repo();

    let branch_name = "ip-worktree-exists";

    let mut p = spawn(format!(
        "{} branch {} --no-shell",
        treebeard_path.display(),
        branch_name
    ))
    .expect("Failed to spawn treebeard");

    p.expect(Eof)
        .expect("Failed to wait for treebeard to complete");

    let output = std::process::Command::new("git")
        .args(["branch", branch_name, "-D"])
        .current_dir(&workspace.repo_path)
        .output()
        .expect("Failed to delete branch");

    let _ = output;

    let worktree_path = workspace.get_worktree_path(branch_name);
    assert!(
        worktree_path.exists(),
        "Worktree should still exist after deleting branch"
    );

    let output = std::process::Command::new(&treebeard_path)
        .args(["branch", branch_name, "--no-shell"])
        .current_dir(&workspace.repo_path)
        .output()
        .expect("Failed to run treebeard with existing worktree");

    assert!(
        output.status.success(),
        "Command should succeed when worktree exists"
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("Worktree") || stdout.contains("worktree"),
        "Output should mention worktree. Output: {}",
        stdout
    );
    assert!(
        stdout.contains("already exists") || stdout.contains("exists"),
        "Output should mention existence. Output: {}",
        stdout
    );

    workspace.restore_dir();
}

#[test]
fn test_branch_creation_with_collision() {
    let treebeard_path = get_treebeard_path();
    let workspace = TestWorkspace::new();
    workspace.switch_to_repo();

    let branch_name = "ip-collision-test";

    let mut p = spawn(format!(
        "{} branch {} --no-shell",
        treebeard_path.display(),
        branch_name
    ))
    .expect("Failed to spawn treebeard");

    p.expect(Eof)
        .expect("Failed to wait for treebeard to complete");

    let worktree_path = workspace.get_worktree_path(branch_name);
    assert!(worktree_path.exists(), "Worktree should exist");

    let output = std::process::Command::new(&treebeard_path)
        .args(["branch", branch_name, "--no-shell"])
        .current_dir(&workspace.repo_path)
        .output()
        .expect("Failed to run treebeard with collision");

    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(
        stdout.contains(branch_name),
        "Output should reference the branch. Output: {}",
        stdout
    );

    workspace.restore_dir();
}
