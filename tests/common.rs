use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Get the path to the treebeard binary for testing.
#[allow(dead_code)]
pub fn get_treebeard_path() -> PathBuf {
    assert_cmd::cargo::cargo_bin!("treebeard").to_path_buf()
}

use tempfile::TempDir;

/// Test workspace providing isolated git repo and treebeard data directories.
/// Used by various test files - #[allow(dead_code)] because not all tests use all fields.
#[allow(dead_code)]
pub struct TestWorkspace {
    pub temp_dir: TempDir,
    pub repo_path: PathBuf,
    pub original_dir: PathBuf,
    pub worktree_base_dir: PathBuf,
    pub config_dir: PathBuf,
}

impl Default for TestWorkspace {
    fn default() -> Self {
        Self::new()
    }
}

#[allow(dead_code)]
impl TestWorkspace {
    pub fn new() -> Self {
        Self::with_sandbox(true)
    }

    /// Create a test workspace with explicit sandbox control.
    ///
    /// When `enable_sandbox` is `false`, the sandbox is disabled in the
    /// generated config file. This is useful for debugging test failures
    /// related to sandbox-exec PTY interactions.
    pub fn with_sandbox(enable_sandbox: bool) -> Self {
        let original_dir = env::current_dir().expect("Failed to get current dir");
        let (temp_dir, repo_path) = create_test_repo();

        // Set TREEBEARD_DATA_DIR for test isolation: ensures tests don't
        // interfere with the user's real treebeard data or with each other.
        let worktree_base_dir = temp_dir.path().to_path_buf();
        env::set_var(
            "TREEBEARD_DATA_DIR",
            worktree_base_dir.to_string_lossy().to_string(),
        );

        // Set TREEBEARD_CONFIG_DIR to isolate config from user's real config
        let config_dir = temp_dir.path().join("config");
        env::set_var(
            "TREEBEARD_CONFIG_DIR",
            config_dir.to_string_lossy().to_string(),
        );

        // Create config directory and config file with sandbox setting
        // Also set a short debounce time for faster auto-commits in tests
        fs::create_dir_all(&config_dir).expect("Failed to create config dir");
        let config_content = format!(
            r#"[sandbox]
enabled = {}

[auto_commit_timing]
auto_commit_debounce_ms = 500
"#,
            enable_sandbox
        );
        fs::write(config_dir.join("config.toml"), config_content)
            .expect("Failed to write config file");

        Self {
            temp_dir,
            repo_path,
            original_dir,
            worktree_base_dir,
            config_dir,
        }
    }

    pub fn switch_to_repo(&self) {
        env::set_current_dir(&self.repo_path).expect("Failed to change directory");
    }

    pub fn restore_dir(&self) {
        env::set_current_dir(&self.original_dir).expect("Failed to restore directory");
    }

    /// Get the worktree path for a branch.
    /// Uses "test-repo" as the repo name, matching create_test_repo().
    pub fn get_worktree_path(&self, branch_name: &str) -> PathBuf {
        self.worktree_base_dir
            .join("worktrees")
            .join("test-repo")
            .join(branch_name)
    }

    /// Get the FUSE mount path for a branch.
    ///
    /// This is where files should be written when testing with the FUSE overlay.
    /// The watcher receives mutation events from the FUSE layer, so files must
    /// be written to the mount path (not the worktree path) for auto-commit to work.
    /// Uses "test-repo" as the repo name, matching create_test_repo().
    pub fn get_mount_path(&self, branch_name: &str) -> PathBuf {
        self.worktree_base_dir
            .join("mounts")
            .join("test-repo")
            .join(branch_name)
    }
}

impl Drop for TestWorkspace {
    fn drop(&mut self) {
        // Clean up any FUSE mounts created by this test workspace before removing temp dir
        self.cleanup_fuse_mounts();

        env::remove_var("TREEBEARD_DATA_DIR");
        env::remove_var("TREEBEARD_CONFIG_DIR");
        self.restore_dir();
    }
}

#[allow(dead_code)]
impl TestWorkspace {
    /// Clean up any FUSE mounts that were created in this workspace's temp directory.
    ///
    /// This is called automatically in Drop to ensure stale mounts don't accumulate.
    fn cleanup_fuse_mounts(&self) {
        let mounts_dir = self.worktree_base_dir.join("mounts");
        if !mounts_dir.exists() {
            return;
        }

        // Find all treebeard mounts that are within our temp directory
        let mount_output = match Command::new("mount").output() {
            Ok(output) => output,
            Err(_) => return,
        };

        let mount_text = String::from_utf8_lossy(&mount_output.stdout);
        let temp_dir_str = self.temp_dir.path().to_string_lossy();

        for line in mount_text.lines() {
            if !line.contains("treebeard") {
                continue;
            }

            // Parse mount point from line: "X on /path/to/mount (options)"
            if let Some(start) = line.find(" on ") {
                let after_on = &line[start + 4..];
                if let Some(end) = after_on.find(" (") {
                    let mount_path = &after_on[..end];

                    // Only unmount if it's within our temp directory
                    if mount_path.contains(&*temp_dir_str) {
                        eprintln!("TestWorkspace cleanup: unmounting {}", mount_path);
                        let _ = Command::new("diskutil")
                            .args(["unmount", "force", mount_path])
                            .output();
                    }
                }
            }
        }
    }

    /// Verify that this workspace's FUSE mount was properly cleaned up.
    ///
    /// Returns true if no stale mounts exist for this workspace.
    /// This can be used by tests to verify cleanup worked correctly.
    pub fn verify_mount_cleaned_up(&self, branch_name: &str) -> bool {
        let mount_path = self.get_mount_path(branch_name);
        !is_mount_active(&mount_path)
    }
}

/// Check if a path is currently an active mount point.
#[allow(dead_code)]
pub fn is_mount_active(path: &Path) -> bool {
    let mount_output = match Command::new("mount").output() {
        Ok(output) => output,
        Err(_) => return false,
    };

    let mount_text = String::from_utf8_lossy(&mount_output.stdout);
    let path_str = path.to_string_lossy();

    for line in mount_text.lines() {
        if line.contains("treebeard") {
            if let Some(start) = line.find(" on ") {
                let after_on = &line[start + 4..];
                if let Some(end) = after_on.find(" (") {
                    let mount_path = &after_on[..end];
                    if mount_path == path_str {
                        return true;
                    }
                }
            }
        }
    }

    false
}

/// Count the number of active treebeard FUSE mounts.
#[allow(dead_code)]
pub fn count_treebeard_mounts() -> usize {
    let mount_output = match Command::new("mount").output() {
        Ok(output) => output,
        Err(_) => return 0,
    };

    let mount_text = String::from_utf8_lossy(&mount_output.stdout);
    mount_text
        .lines()
        .filter(|line| line.contains("treebeard"))
        .count()
}

/// Get all active treebeard mount paths.
#[allow(dead_code)]
pub fn get_treebeard_mount_paths() -> Vec<String> {
    let mount_output = match Command::new("mount").output() {
        Ok(output) => output,
        Err(_) => return Vec::new(),
    };

    let mount_text = String::from_utf8_lossy(&mount_output.stdout);
    let mut paths = Vec::new();

    for line in mount_text.lines() {
        if !line.contains("treebeard") {
            continue;
        }

        if let Some(start) = line.find(" on ") {
            let after_on = &line[start + 4..];
            if let Some(end) = after_on.find(" (") {
                let mount_path = &after_on[..end];
                paths.push(mount_path.to_string());
            }
        }
    }

    paths
}

/// Force unmount all treebeard mounts in temp directories.
///
/// This is a cleanup function that can be used to clean up stale mounts
/// from crashed tests. It only unmounts mounts in /private/var/folders
/// or /tmp to avoid accidentally unmounting production mounts.
#[allow(dead_code)]
pub fn cleanup_all_test_mounts() -> usize {
    let paths = get_treebeard_mount_paths();
    let mut cleaned = 0;

    for path in paths {
        // Only clean up mounts in temp directories (test mounts)
        if path.contains("/var/folders/")
            || path.contains("/tmp/")
            || path.contains("/private/var/folders/")
        {
            eprintln!("Cleaning up stale test mount: {}", path);
            let result = Command::new("diskutil")
                .args(["unmount", "force", &path])
                .output();

            if let Ok(output) = result {
                if output.status.success() {
                    cleaned += 1;
                }
            }
        }
    }

    cleaned
}

/// A lighter-weight test context that only sets up an isolated config directory.
/// Use this for tests that don't need a full git repo workspace.
#[allow(dead_code)]
pub struct TestConfigContext {
    pub temp_dir: TempDir,
    pub config_dir: PathBuf,
}

impl Default for TestConfigContext {
    fn default() -> Self {
        Self::new()
    }
}

#[allow(dead_code)]
impl TestConfigContext {
    pub fn new() -> Self {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let config_dir = temp_dir.path().join("config");

        // Set TREEBEARD_CONFIG_DIR to isolate config from user's real config
        env::set_var(
            "TREEBEARD_CONFIG_DIR",
            config_dir.to_string_lossy().to_string(),
        );

        Self {
            temp_dir,
            config_dir,
        }
    }
}

impl Drop for TestConfigContext {
    fn drop(&mut self) {
        env::remove_var("TREEBEARD_CONFIG_DIR");
    }
}

#[allow(dead_code)]
pub fn create_test_repo() -> (TempDir, PathBuf) {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let repo_path = temp_dir.path().join("test-repo");

    fs::create_dir_all(&repo_path).expect("Failed to create repo dir");

    let output = Command::new("git")
        .args(["init", &repo_path.to_string_lossy()])
        .output()
        .expect("Failed to init git repo");

    assert!(output.status.success(), "Failed to init git repo");

    // Git requires user.email and user.name for commits. Setting them per-repo
    // avoids depending on the user's global git config, making tests reproducible.
    let output = Command::new("git")
        .args(["config", "user.email", "test@example.com"])
        .current_dir(&repo_path)
        .output()
        .expect("Failed to set git user.email");

    assert!(output.status.success(), "Failed to set git user.email");

    let output = Command::new("git")
        .args(["config", "user.name", "Test User"])
        .current_dir(&repo_path)
        .output()
        .expect("Failed to set git user.name");

    assert!(output.status.success(), "Failed to set git user.name");

    let file_path = repo_path.join("README.md");
    fs::write(&file_path, "# Test Repo\n").expect("Failed to write README");

    let output = Command::new("git")
        .args(["add", "."])
        .current_dir(&repo_path)
        .output()
        .expect("Failed to git add");

    assert!(output.status.success(), "Failed to git add");

    let output = Command::new("git")
        .args(["commit", "-m", "Initial commit"])
        .current_dir(&repo_path)
        .output()
        .expect("Failed to git commit");

    assert!(output.status.success(), "Failed to git commit");

    (temp_dir, repo_path)
}

#[allow(dead_code)]
pub fn create_test_file(repo_path: &Path, filename: &str, content: &str) -> PathBuf {
    let file_path = repo_path.join(filename);

    if let Some(parent) = file_path.parent() {
        fs::create_dir_all(parent).expect("Failed to create parent directories");
    }

    fs::write(&file_path, content).expect("Failed to write test file");

    file_path
}

#[allow(dead_code)]
pub fn git_commit_count(repo_path: &PathBuf, branch: &str) -> usize {
    let output = Command::new("git")
        .args(["rev-list", "--count", branch])
        .current_dir(repo_path)
        .output()
        .expect("Failed to run git rev-list --count");

    assert!(output.status.success(), "Failed to count commits");

    let stdout = String::from_utf8_lossy(&output.stdout);
    stdout.trim().parse().expect("Failed to parse commit count")
}

#[allow(dead_code)]
pub fn get_branch_commits(repo_path: &PathBuf, branch: &str) -> Vec<String> {
    let output = Command::new("git")
        .args(["log", "--oneline", branch])
        .current_dir(repo_path)
        .output()
        .expect("Failed to run git log");

    assert!(output.status.success(), "Failed to get commit log");

    let stdout = String::from_utf8_lossy(&output.stdout);
    stdout.lines().map(|line| line.to_string()).collect()
}
