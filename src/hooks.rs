//! Hooks system for running commands at lifecycle events.
//!
//! Hooks are shell commands executed via `sh -c` at various points during treebeard operations.
//! Template variables are expanded before execution.

use crate::error::{Result, TreebeardError};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use tokio::process::Command;

/// Context for hook execution, providing template variables and working directory.
#[derive(Debug, Clone)]
pub struct HookContext {
    /// Branch name
    pub branch: String,
    /// FUSE mount path
    pub mount_path: PathBuf,
    /// Git worktree path
    pub worktree_path: PathBuf,
    /// Main repository path
    pub repo_path: PathBuf,
    /// Diff content (only used for commit_message hook)
    pub diff: Option<String>,
}

impl HookContext {
    /// Create a new hook context without diff (for post_create, pre_cleanup, post_cleanup).
    pub fn new(branch: &str, mount_path: &Path, worktree_path: &Path, repo_path: &Path) -> Self {
        Self {
            branch: branch.to_string(),
            mount_path: mount_path.to_path_buf(),
            worktree_path: worktree_path.to_path_buf(),
            repo_path: repo_path.to_path_buf(),
            diff: None,
        }
    }

    /// Create a new hook context with diff (for commit_message hook).
    pub fn with_diff(
        branch: &str,
        mount_path: &Path,
        worktree_path: &Path,
        repo_path: &Path,
        diff: String,
    ) -> Self {
        Self {
            branch: branch.to_string(),
            mount_path: mount_path.to_path_buf(),
            worktree_path: worktree_path.to_path_buf(),
            repo_path: repo_path.to_path_buf(),
            diff: Some(diff),
        }
    }

    /// Get environment variables to set for hook execution.
    pub fn env_vars(&self) -> Vec<(&'static str, String)> {
        vec![
            ("TREEBEARD_BRANCH", self.branch.clone()),
            (
                "TREEBEARD_MOUNT_PATH",
                self.mount_path.to_string_lossy().to_string(),
            ),
            (
                "TREEBEARD_WORKTREE_PATH",
                self.worktree_path.to_string_lossy().to_string(),
            ),
            (
                "TREEBEARD_REPO_PATH",
                self.repo_path.to_string_lossy().to_string(),
            ),
        ]
    }
}

/// Escape a string for safe use within single quotes in shell commands.
///
/// This replaces single quotes with the sequence `'\''` which:
/// 1. Ends the current single-quoted string
/// 2. Adds an escaped single quote
/// 3. Starts a new single-quoted string
///
/// This is necessary to prevent shell injection when user-controlled content
/// (like git diffs) is embedded in shell commands.
fn shell_escape(s: &str) -> String {
    s.replace('\'', "'\\''")
}

/// Expand template variables in a hook command.
///
/// Supported variables:
/// - `{{branch}}` - Branch name
/// - `{{mount_path}}` - FUSE mount path
/// - `{{worktree_path}}` - Git worktree path
/// - `{{repo_path}}` - Main repository path
/// - `{{diff}}` - Diff content (only for commit_message hook, shell-escaped)
pub fn expand_template(template: &str, context: &HookContext) -> String {
    let mut result = template.to_string();

    result = result.replace("{{branch}}", &context.branch);
    result = result.replace("{{mount_path}}", &context.mount_path.to_string_lossy());
    result = result.replace(
        "{{worktree_path}}",
        &context.worktree_path.to_string_lossy(),
    );
    result = result.replace("{{repo_path}}", &context.repo_path.to_string_lossy());

    let diff_value = context.diff.as_deref().unwrap_or("");
    // Shell-escape the diff content to prevent injection attacks
    result = result.replace("{{diff}}", &shell_escape(diff_value));

    result
}

/// Run a list of hook commands sequentially.
///
/// Each hook is executed via `sh -c` in the specified working directory.
/// If any hook fails (non-zero exit code), execution stops and an error is returned.
///
/// # Arguments
/// * `hooks` - List of hook commands to execute
/// * `context` - Hook context providing template variables
/// * `working_dir` - Directory to execute hooks in
///
/// # Returns
/// * `Ok(())` if all hooks succeed or if the hooks list is empty
/// * `Err(TreebeardError::Hook)` if any hook fails
pub async fn run_hooks(hooks: &[String], context: &HookContext, working_dir: &Path) -> Result<()> {
    if hooks.is_empty() {
        return Ok(());
    }

    for hook in hooks {
        let expanded = expand_template(hook, context);
        tracing::info!("Running hook: {}", expanded);

        let status = Command::new("sh")
            .arg("-c")
            .arg(&expanded)
            .current_dir(working_dir)
            .envs(context.env_vars())
            .stdin(Stdio::null())
            .status()
            .await
            .map_err(|e| {
                TreebeardError::Hook(format!("Failed to execute hook '{}': {}", expanded, e))
            })?;

        if !status.success() {
            let exit_code = status
                .code()
                .map(|c| c.to_string())
                .unwrap_or_else(|| "unknown".to_string());
            return Err(TreebeardError::Hook(format!(
                "Hook '{}' failed with exit code {}",
                expanded, exit_code
            )));
        }

        tracing::debug!("Hook completed successfully: {}", expanded);
    }

    Ok(())
}

/// Run a commit message hook and return the generated message.
///
/// The hook command is executed via `sh -c` and its stdout is captured.
/// The output is trimmed of leading/trailing whitespace.
///
/// # Arguments
/// * `hook` - The hook command to execute
/// * `context` - Hook context providing template variables (should include diff)
/// * `working_dir` - Directory to execute the hook in
///
/// # Returns
/// * `Ok(Some(message))` if the hook succeeds and produces output
/// * `Ok(None)` if the hook produces no output
/// * `Err(TreebeardError::Hook)` if the hook fails
pub async fn run_commit_message_hook(
    hook: &str,
    context: &HookContext,
    working_dir: &Path,
) -> Result<Option<String>> {
    let expanded = expand_template(hook, context);
    tracing::info!("Running commit message hook: {}", expanded);

    let output = Command::new("sh")
        .arg("-c")
        .arg(&expanded)
        .current_dir(working_dir)
        .envs(context.env_vars())
        .stdin(Stdio::null())
        .output()
        .await
        .map_err(|e| {
            TreebeardError::Hook(format!(
                "Failed to execute commit message hook '{}': {}",
                expanded, e
            ))
        })?;

    if !output.status.success() {
        let exit_code = output
            .status
            .code()
            .map(|c| c.to_string())
            .unwrap_or_else(|| "unknown".to_string());
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(TreebeardError::Hook(format!(
            "Commit message hook '{}' failed with exit code {}: {}",
            expanded,
            exit_code,
            stderr.trim()
        )));
    }

    let message = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if message.is_empty() {
        tracing::warn!("Commit message hook produced empty output");
        Ok(None)
    } else {
        tracing::debug!("Commit message hook output: {}", message);
        Ok(Some(message))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn test_context() -> HookContext {
        HookContext {
            branch: "feature-test".to_string(),
            mount_path: PathBuf::from("/mounts/repo/feature-test"),
            worktree_path: PathBuf::from("/worktrees/repo/feature-test"),
            repo_path: PathBuf::from("/repos/myrepo"),
            diff: None,
        }
    }

    fn test_context_with_diff() -> HookContext {
        HookContext {
            branch: "feature-test".to_string(),
            mount_path: PathBuf::from("/mounts/repo/feature-test"),
            worktree_path: PathBuf::from("/worktrees/repo/feature-test"),
            repo_path: PathBuf::from("/repos/myrepo"),
            diff: Some("diff --git a/file.txt b/file.txt\n+new line".to_string()),
        }
    }

    #[test]
    fn test_expand_template_branch() {
        let ctx = test_context();
        let result = expand_template("echo {{branch}}", &ctx);
        assert_eq!(result, "echo feature-test");
    }

    #[test]
    fn test_expand_template_mount_path() {
        let ctx = test_context();
        let result = expand_template("cd {{mount_path}}", &ctx);
        assert_eq!(result, "cd /mounts/repo/feature-test");
    }

    #[test]
    fn test_expand_template_worktree_path() {
        let ctx = test_context();
        let result = expand_template("ls {{worktree_path}}", &ctx);
        assert_eq!(result, "ls /worktrees/repo/feature-test");
    }

    #[test]
    fn test_expand_template_repo_path() {
        let ctx = test_context();
        let result = expand_template("git -C {{repo_path}} status", &ctx);
        assert_eq!(result, "git -C /repos/myrepo status");
    }

    #[test]
    fn test_expand_template_multiple_variables() {
        let ctx = test_context();
        let result = expand_template("echo 'Branch {{branch}} at {{mount_path}}'", &ctx);
        assert_eq!(
            result,
            "echo 'Branch feature-test at /mounts/repo/feature-test'"
        );
    }

    #[test]
    fn test_expand_template_diff_empty_when_none() {
        let ctx = test_context();
        let result = expand_template("echo '{{diff}}'", &ctx);
        assert_eq!(result, "echo ''");
    }

    #[test]
    fn test_expand_template_diff_with_content() {
        let ctx = test_context_with_diff();
        let result = expand_template("echo '{{diff}}'", &ctx);
        assert_eq!(result, "echo 'diff --git a/file.txt b/file.txt\n+new line'");
    }

    #[test]
    fn test_expand_template_diff_escapes_single_quotes() {
        // Verify that single quotes in diff content are escaped
        // to prevent shell injection attacks
        let ctx = HookContext {
            branch: "feature-test".to_string(),
            mount_path: PathBuf::from("/mounts/repo/feature-test"),
            worktree_path: PathBuf::from("/worktrees/repo/feature-test"),
            repo_path: PathBuf::from("/repos/myrepo"),
            diff: Some("test'injection".to_string()),
        };
        let result = expand_template("echo '{{diff}}'", &ctx);
        // The single quote in the diff should be escaped as '\''
        // So "test'injection" becomes "test'\''injection"
        assert_eq!(result, "echo 'test'\\''injection'");
    }

    #[test]
    fn test_shell_escape() {
        assert_eq!(shell_escape("hello"), "hello");
        assert_eq!(shell_escape("it's"), "it'\\''s");
        assert_eq!(shell_escape("'quoted'"), "'\\''quoted'\\''");
        assert_eq!(shell_escape("no quotes here"), "no quotes here");
    }

    #[test]
    fn test_expand_template_no_variables() {
        let ctx = test_context();
        let result = expand_template("npm install", &ctx);
        assert_eq!(result, "npm install");
    }

    #[test]
    fn test_expand_template_repeated_variable() {
        let ctx = test_context();
        let result = expand_template("{{branch}}-{{branch}}", &ctx);
        assert_eq!(result, "feature-test-feature-test");
    }

    #[test]
    fn test_env_vars() {
        let ctx = test_context();
        let vars = ctx.env_vars();

        assert_eq!(vars.len(), 4);
        assert!(vars
            .iter()
            .any(|(k, v)| *k == "TREEBEARD_BRANCH" && v == "feature-test"));
        assert!(vars
            .iter()
            .any(|(k, v)| *k == "TREEBEARD_MOUNT_PATH" && v == "/mounts/repo/feature-test"));
        assert!(vars
            .iter()
            .any(|(k, v)| *k == "TREEBEARD_WORKTREE_PATH" && v == "/worktrees/repo/feature-test"));
        assert!(vars
            .iter()
            .any(|(k, v)| *k == "TREEBEARD_REPO_PATH" && v == "/repos/myrepo"));
    }

    #[tokio::test]
    async fn test_run_hooks_empty_list() {
        let ctx = test_context();
        let result = run_hooks(&[], &ctx, Path::new("/tmp")).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_run_hooks_success() {
        let ctx = test_context();
        let hooks = vec!["true".to_string()];
        let result = run_hooks(&hooks, &ctx, Path::new("/tmp")).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_run_hooks_failure() {
        let ctx = test_context();
        let hooks = vec!["false".to_string()];
        let result = run_hooks(&hooks, &ctx, Path::new("/tmp")).await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("Hook"));
        assert!(err.to_string().contains("failed"));
    }

    #[tokio::test]
    async fn test_run_hooks_stops_on_failure() {
        let ctx = test_context();
        // Create a temp file to track execution
        let temp_dir = tempfile::tempdir().unwrap();
        let marker_file = temp_dir.path().join("marker");

        let hooks = vec![
            "false".to_string(),                        // This will fail
            format!("touch {}", marker_file.display()), // This should not run
        ];

        let result = run_hooks(&hooks, &ctx, temp_dir.path()).await;
        assert!(result.is_err());
        // The second hook should not have run
        assert!(!marker_file.exists());
    }

    #[tokio::test]
    async fn test_run_commit_message_hook_success() {
        let ctx = test_context();
        let result =
            run_commit_message_hook("echo 'test commit message'", &ctx, Path::new("/tmp")).await;

        assert!(result.is_ok());
        let message = result.unwrap();
        assert_eq!(message, Some("test commit message".to_string()));
    }

    #[tokio::test]
    async fn test_run_commit_message_hook_with_template() {
        let ctx = test_context();
        let result =
            run_commit_message_hook("echo 'Changes on {{branch}}'", &ctx, Path::new("/tmp")).await;

        assert!(result.is_ok());
        let message = result.unwrap();
        assert_eq!(message, Some("Changes on feature-test".to_string()));
    }

    #[tokio::test]
    async fn test_run_commit_message_hook_empty_output() {
        let ctx = test_context();
        // Use printf "" instead of echo -n '' for portable empty output
        let result = run_commit_message_hook("printf ''", &ctx, Path::new("/tmp")).await;

        assert!(result.is_ok());
        let message = result.unwrap();
        assert_eq!(message, None);
    }

    #[tokio::test]
    async fn test_run_commit_message_hook_failure() {
        let ctx = test_context();
        let result = run_commit_message_hook("exit 1", &ctx, Path::new("/tmp")).await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("Commit message hook"));
        assert!(err.to_string().contains("failed"));
    }

    #[tokio::test]
    async fn test_run_commit_message_hook_trims_whitespace() {
        let ctx = test_context();
        let result =
            run_commit_message_hook("echo '  trimmed message  '", &ctx, Path::new("/tmp")).await;

        assert!(result.is_ok());
        let message = result.unwrap();
        assert_eq!(message, Some("trimmed message".to_string()));
    }
}
