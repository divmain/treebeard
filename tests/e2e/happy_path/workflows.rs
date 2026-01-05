//! Happy path tests for complete workflows.

use crate::shared::common::{get_branch_commits, get_treebeard_path, TestWorkspace};
use crate::shared::e2e_helpers::{
    exit_session, shell_exec, shell_mkdir, shell_sleep, start_session, SessionExitConfig,
    SessionStartConfig,
};
use expectrl::{spawn, Eof, Expect};
use std::fs;
use std::process::Command;

#[test]
fn test_basic_workflow_spawn_exit() {
    let treebeard_path = get_treebeard_path();
    let workspace = TestWorkspace::new();
    workspace.switch_to_repo();

    let mut p = spawn(format!(
        "{} branch workflow-e2e-test-1 --no-shell",
        treebeard_path.display()
    ))
    .expect("Failed to spawn treebeard");

    p.expect(Eof)
        .expect("Failed to wait for treebeard to complete");

    assert!(
        !workspace.repo_path.join(".treebeard").exists(),
        "Cleanup should have removed .treebeard directory"
    );

    workspace.restore_dir();
}

#[test]
fn test_workflow_feature_development() {
    let workspace = TestWorkspace::new();
    workspace.switch_to_repo();
    let treebeard_path = get_treebeard_path();

    let branch_name = "wf-feature-user-auth";

    let mut session = start_session(
        &treebeard_path,
        branch_name,
        &workspace.repo_path,
        SessionStartConfig::default(),
    );

    shell_mkdir(&mut session, "src");

    shell_exec(
        &mut session,
        "printf '%s\\n' '// Authentication module' 'pub fn authenticate() -> bool {' '    true' '}' > src/auth.rs && sync",
    );

    shell_sleep(&mut session, 3.0);

    shell_exec(
        &mut session,
        "printf '%s\\n' '// User module' 'pub struct User {' '    name: String,' '}' > src/user.rs && sync",
    );

    shell_sleep(&mut session, 3.0);

    shell_exec(
        &mut session,
        "printf '%s\\n' '// Authentication module' 'pub fn authenticate() -> bool {' '    false' '}' 'pub fn logout() {}' > src/auth.rs && sync",
    );

    shell_sleep(&mut session, 3.0);

    exit_session(&mut session, SessionExitConfig::default());
    workspace.restore_dir();

    let commits = get_branch_commits(&workspace.repo_path, branch_name);
    let commit_count = commits.len();

    assert!(
        commit_count >= 2,
        "Expected at least 2 commits after squash (initial + squashed), got {}",
        commit_count
    );

    let output = Command::new("git")
        .args(["diff", "main", branch_name, "--stat"])
        .current_dir(&workspace.repo_path)
        .output()
        .expect("Failed to get diff stats");

    assert!(output.status.success(), "Diff should succeed");
    let diff_output = String::from_utf8_lossy(&output.stdout);
    assert!(
        diff_output.contains("src/auth.rs") || diff_output.contains("src/user.rs"),
        "Feature work should include auth and user files. Diff: {}",
        diff_output
    );
}

#[test]
fn test_workflow_bugfix() {
    let workspace = TestWorkspace::new();
    workspace.switch_to_repo();
    let treebeard_path = get_treebeard_path();

    let bug_file = workspace.repo_path.join("src/lib.rs");
    let src_dir = workspace.repo_path.join("src");
    fs::create_dir_all(&src_dir).expect("Failed to create src dir");
    fs::write(
        &bug_file,
        "// Library module\npub fn calculate(x: i32) -> i32 {\n    x * 2\n}\n",
    )
    .expect("Failed to write initial lib.rs");

    let output = Command::new("git")
        .args(["add", "."])
        .current_dir(&workspace.repo_path)
        .output()
        .expect("Failed to git add");

    assert!(output.status.success(), "Git add should succeed");

    let output = Command::new("git")
        .args(["commit", "-m", "Add initial library with bug"])
        .current_dir(&workspace.repo_path)
        .output()
        .expect("Failed to git commit");

    assert!(output.status.success(), "Git commit should succeed");

    let branch_name = "wf-bugfix-calculate-issue";
    let mut session = start_session(
        &treebeard_path,
        branch_name,
        &workspace.repo_path,
        SessionStartConfig::default(),
    );

    shell_exec(
        &mut session,
        "printf '%s\\n' '// Library module (fixed)' 'pub fn calculate(x: i32) -> i32 {' '    x * 2 + 1' '}' > src/lib.rs && sync",
    );

    shell_sleep(&mut session, 3.0);

    shell_exec(
        &mut session,
        "printf '%s\\n' '// Test module' '#[cfg(test)]' 'mod tests {' '    use super::calculate;' '    #[test]' '    fn test_calculate() {' '        assert_eq!(calculate(5), 11);' '    }' '}' > src/lib_test.rs && sync",
    );

    shell_sleep(&mut session, 3.0);

    exit_session(&mut session, SessionExitConfig::default());
    workspace.restore_dir();

    let commits = get_branch_commits(&workspace.repo_path, branch_name);
    assert!(
        commits.len() >= 2,
        "Bugfix branch should have at least 2 commits (auto-commits), got {}",
        commits.len()
    );

    let output = Command::new("git")
        .args(["show", &format!("{}:src/lib.rs", branch_name)])
        .current_dir(&workspace.repo_path)
        .output()
        .expect("Failed to read fixed file");

    assert!(output.status.success(), "Fixed file should exist in branch");
    let content = String::from_utf8_lossy(&output.stdout);
    assert!(
        content.contains("x * 2 + 1"),
        "Bugfix should be present in the file. Content: {}",
        content
    );

    let output = Command::new("git")
        .args(["log", &format!("main..{}", branch_name), "--oneline"])
        .current_dir(&workspace.repo_path)
        .output()
        .expect("Failed to get branch history");

    assert!(output.status.success(), "Log should succeed");
    let log_output = String::from_utf8_lossy(&output.stdout);
    assert!(
        !log_output.is_empty(),
        "Bugfix branch should have commits beyond main. Log: {}",
        log_output
    );
}

#[test]
fn test_workflow_experimentation() {
    let workspace = TestWorkspace::new();
    workspace.switch_to_repo();
    let treebeard_path = get_treebeard_path();

    let branch_name = "wf-experiment-new-api";

    let mut session = start_session(
        &treebeard_path,
        branch_name,
        &workspace.repo_path,
        SessionStartConfig::default(),
    );

    shell_exec(
        &mut session,
        "printf '%s\\n' '// Experimental API design' 'pub struct ExpApi {' '    data: Vec<String>,' '}' '' 'impl ExpApi {' '    pub fn new() -> Self {' '        Self { data: vec![] }' '    }' '}' > experimental_api.rs && sync",
    );
    shell_sleep(&mut session, 3.0);

    shell_exec(
        &mut session,
        "printf '%s\\n' '// Experimental API design v2' 'pub struct ExpApi {' '    data: Vec<String>,' '    cache: bool,' '}' '' 'impl ExpApi {' '    pub fn new() -> Self {' '        Self { data: vec![], cache: false }' '    }' '}' > experimental_api.rs && sync",
    );
    shell_sleep(&mut session, 3.0);

    shell_exec(
        &mut session,
        "printf '%s\\n' '# Experiment Notes' '' 'Trying out a new API design with caching.' 'See results below.' > experiment_notes.md && sync",
    );
    shell_sleep(&mut session, 3.0);

    shell_exec(
        &mut session,
        "printf '%s\\n' '// Experimental API design v3' 'pub struct ExpApi {' '    data: Vec<String>,' '    cache: bool,' '}' '' 'impl ExpApi {' '    pub fn new() -> Self {' '        Self { data: vec![], cache: true }' '    }' '}' > experimental_api.rs && sync",
    );
    shell_sleep(&mut session, 3.0);

    exit_session(&mut session, SessionExitConfig::default());
    workspace.restore_dir();

    let commits = get_branch_commits(&workspace.repo_path, branch_name);
    assert!(
        commits.len() >= 2,
        "Experiment branch should have at least 2 commits after squash, got {}",
        commits.len()
    );

    let output = Command::new("git")
        .args(["show", &format!("{}:experimental_api.rs", branch_name)])
        .current_dir(&workspace.repo_path)
        .output()
        .expect("Failed to read final experiment");

    assert!(
        output.status.success(),
        "Final experiment should exist in branch"
    );
    let content = String::from_utf8_lossy(&output.stdout);
    assert!(
        content.contains("cache: true"),
        "Final version should have cache: true. Content: {}",
        content
    );

    let output = Command::new("git")
        .args(["ls-tree", "-r", branch_name, "--name-only"])
        .current_dir(&workspace.repo_path)
        .output()
        .expect("Failed to list files in experiment branch");

    assert!(output.status.success(), "List should succeed");
    let files = String::from_utf8_lossy(&output.stdout);
    assert!(
        files.contains("experimental_api.rs"),
        "Experiment branch should have experimental_api.rs. Files: {}",
        files
    );
    assert!(
        files.contains("experiment_notes.md"),
        "Experiment branch should have notes. Files: {}",
        files
    );
}

#[test]
fn test_workflow_documentation_updates() {
    let workspace = TestWorkspace::new();
    workspace.switch_to_repo();
    let treebeard_path = get_treebeard_path();

    let docs_dir = workspace.repo_path.join("docs");
    fs::create_dir_all(&docs_dir).expect("Failed to create docs dir");

    let overview_file = docs_dir.join("overview.md");
    fs::write(
        &overview_file,
        "# Overview\n\nThis is the initial overview.\n",
    )
    .expect("Failed to write overview");

    let output = Command::new("git")
        .args(["add", "."])
        .current_dir(&workspace.repo_path)
        .output()
        .expect("Failed to git add");

    assert!(output.status.success(), "Git add should succeed");

    let output = Command::new("git")
        .args(["commit", "-m", "Add initial documentation"])
        .current_dir(&workspace.repo_path)
        .output()
        .expect("Failed to git commit");

    assert!(output.status.success(), "Git commit should succeed");

    let branch_name = "wf-docs-update-readme";
    let mut session = start_session(
        &treebeard_path,
        branch_name,
        &workspace.repo_path,
        SessionStartConfig::default(),
    );

    shell_exec(
        &mut session,
        "printf '%s\\n' '# Treebeard' '' 'A Git worktree management tool.' '' '## Getting Started' '' 'Create a branch: treebeard branch feature-name' > README.md && sync",
    );
    shell_sleep(&mut session, 3.0);

    shell_exec(
        &mut session,
        "printf '%s\\n' '# User Guide' '' 'Step-by-step guide for using treebeard.' '' '1. Create a branch' '2. Make changes' '3. Exit' > docs/guide.md && sync",
    );
    shell_sleep(&mut session, 3.0);

    shell_exec(
        &mut session,
        "printf '%s\\n' '# Treebeard' '' 'A Git worktree management tool with auto-commit.' '' '## Features' '' '- Auto-commit' '- Squash on exit' '' '## Getting Started' '' 'Create a branch: treebeard branch feature-name' > README.md && sync",
    );
    shell_sleep(&mut session, 3.0);

    shell_exec(
        &mut session,
        "printf '%s\\n' '# Changelog' '' '## Unreleased' '' '- Added auto-commit feature' '- Improved documentation' > CHANGELOG.md && sync",
    );
    shell_sleep(&mut session, 3.0);

    exit_session(&mut session, SessionExitConfig::default());
    workspace.restore_dir();

    let commits = get_branch_commits(&workspace.repo_path, branch_name);
    assert!(
        commits.len() >= 2,
        "Documentation updates should have at least 2 commits after squash, got {}",
        commits.len()
    );

    let output = Command::new("git")
        .args(["show", &format!("{}:README.md", branch_name)])
        .current_dir(&workspace.repo_path)
        .output()
        .expect("Failed to read README");

    assert!(
        output.status.success(),
        "Updated README should exist in branch"
    );
    let content = String::from_utf8_lossy(&output.stdout);
    assert!(
        content.contains("auto-commit") && content.contains("Features"),
        "README should have been updated. Content: {}",
        content
    );

    let output = Command::new("git")
        .args(["ls-tree", "-r", branch_name, "--name-only"])
        .current_dir(&workspace.repo_path)
        .output()
        .expect("Failed to list files in documentation branch");

    assert!(output.status.success(), "List should succeed");
    let files = String::from_utf8_lossy(&output.stdout);
    assert!(
        files.contains("README.md"),
        "Documentation branch should have updated README. Files: {}",
        files
    );
    assert!(
        files.contains("docs/guide.md"),
        "Documentation branch should have guide. Files: {}",
        files
    );
    assert!(
        files.contains("CHANGELOG.md"),
        "Documentation branch should have changelog. Files: {}",
        files
    );
}
