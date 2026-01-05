//! Infrastructure tests for output formatting.

use crate::shared::common::{get_treebeard_path, TestWorkspace};
use crate::shared::e2e_helpers::{spawn_treebeard_test_mode, terminate_treebeard};
use std::process::Command;

#[test]
fn test_enhanced_list_output_format() {
    let workspace = TestWorkspace::new();
    workspace.switch_to_repo();

    let branch_name = "ni-enhanced-output";

    let treebeard = spawn_treebeard_test_mode(branch_name, &workspace.repo_path);

    let treebeard_path = get_treebeard_path();
    let output = Command::new(&treebeard_path)
        .arg("list")
        .current_dir(&workspace.repo_path)
        .output()
        .expect("Failed to run list command");

    assert!(output.status.success(), "List command should succeed");

    let stdout = String::from_utf8_lossy(&output.stdout);
    eprintln!("List output:\n{}", stdout);

    let lines: Vec<&str> = stdout.lines().collect();

    assert!(
        lines.len() >= 5,
        "Output should have at least repo name + blank + header + separator + data line"
    );

    let header_line = lines.get(2).expect("Should have header line");
    assert!(
        header_line.contains("BRANCH"),
        "Header should contain BRANCH column"
    );
    assert!(
        header_line.contains("STATUS"),
        "Header should contain STATUS column"
    );
    assert!(
        header_line.contains("MOUNT"),
        "Header should contain MOUNT column"
    );
    assert!(
        header_line.contains("FILES"),
        "Header should contain FILES column"
    );
    assert!(
        header_line.contains("AGE"),
        "Header should contain AGE column"
    );

    let separator_line = lines.get(3).expect("Should have separator line");
    assert!(
        separator_line.starts_with('─'),
        "Separator should start with dash character"
    );

    let data_line = lines.get(4).expect("Should have data line");
    assert!(
        data_line.contains(branch_name),
        "Data line should contain branch name"
    );

    assert!(
        stdout.contains('○') || stdout.contains('●') || stdout.contains('↯'),
        "Output should contain status symbol (○, ●, or ↯)"
    );

    assert!(
        stdout.contains("active") || stdout.contains("idle") || stdout.contains("stale"),
        "Output should contain status text (active, idle, or stale)"
    );

    let legend_line = lines.last().expect("Should have legend line");
    assert!(
        legend_line.contains("● active")
            || legend_line.contains("○ idle")
            || legend_line.contains("↯ stale"),
        "Output should have legend explaining status symbols"
    );

    terminate_treebeard(treebeard);
}
