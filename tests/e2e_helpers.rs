//! Helper functions for e2e tests using expectrl.
//!
//! These helpers provide a consistent way to interact with treebeard
//! through a pseudo-TTY, testing the actual interactive user experience.
//!
//! ## Overview
//!
//! The main entry points are:
//! - [`start_session()`] - Spawns treebeard and handles all startup interactions
//! - [`exit_session()`] - Handles all cleanup prompts after subprocess exits
//!
//! These functions use configuration structs ([`SessionStartConfig`] and
//! [`SessionExitConfig`]) that document and control all interactive prompts.
//! When treebeard adds new prompts, update these structs in one place rather
//! than modifying every test.
//!
//! ## Example
//!
//! ```ignore
//! let mut session = start_session(
//!     &treebeard_path,
//!     "my-branch",
//!     &workspace.repo_path,
//!     SessionStartConfig::default(),
//! );
//!
//! // ... interact with shell ...
//!
//! exit_session(&mut session, SessionExitConfig::default());
//! ```

use expectrl::{Eof, Expect};
use std::path::Path;
use std::time::Duration;

/// Type alias for the concrete session type used in tests.
/// expectrl::session::OsSession is the platform-specific session type.
pub type TestSession = expectrl::session::OsSession;

/// Unique prompt string for shell synchronization.
/// This is set as PS1 so we know when a command has completed.
pub const TEST_SHELL_PROMPT: &str = "TREEBEARD_TEST_READY>";

/// Default timeout for expect operations (30 seconds).
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);

// =============================================================================
// Configuration Types
// =============================================================================

/// How to handle the sync flow TUI if it appears during cleanup.
///
/// The sync flow appears when ignored files were modified in the worktree
/// and treebeard offers to copy them back to the main repo.
#[allow(dead_code)]
#[derive(Debug, Clone, Default)]
pub enum SyncFlowResponse {
    /// Select all items and confirm with 'd' (done)
    SelectAll,
    /// Skip without syncing (press 'q' to quit)
    Skip,
    /// Send Ctrl+C at the sync prompt to cancel (requires sync to appear)
    /// This skips subsequent prompts and exits immediately
    CtrlCAtPrompt,
    /// Enter selection mode with 's', then send Ctrl+C (requires sync to appear)
    /// This tests cancellation during interactive selection
    CtrlCDuringSelection,
    /// Try to send Ctrl+C at sync prompt, but handle gracefully if sync doesn't appear.
    /// If sync prompt appears: send Ctrl+C, verify process exits.
    /// If delete worktree prompt appears instead: answer based on delete_worktree config.
    CtrlCAtPromptIfPresent,
    /// Try to enter selection mode and send Ctrl+C, but handle gracefully if sync doesn't appear.
    /// If sync prompt appears: enter selection, send Ctrl+C, verify process exits.
    /// If delete worktree prompt appears instead: answer based on delete_worktree config.
    CtrlCDuringSelectionIfPresent,
    /// Sync flow should not appear; test proceeds directly to next prompt.
    /// If sync flow unexpectedly appears, the test will fail when it tries
    /// to match the next expected prompt.
    #[default]
    ShouldNotAppear,
}

/// Configuration for session startup interactions.
///
/// Controls how [`start_session()`] handles the treebeard startup sequence.
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct SessionStartConfig {
    /// Whether to set up a custom PS1 prompt for reliable command synchronization.
    /// When true (default), shell commands can be synchronized using [`shell_exec()`].
    pub setup_custom_prompt: bool,
}

impl Default for SessionStartConfig {
    fn default() -> Self {
        Self {
            setup_custom_prompt: true,
        }
    }
}

#[allow(dead_code)]
impl SessionStartConfig {
    /// Create a new config with default settings.
    pub fn new() -> Self {
        Self::default()
    }

    /// Disable custom PS1 setup.
    ///
    /// Use this when testing scenarios where you don't need shell command
    /// synchronization (e.g., testing immediate exit behavior).
    pub fn without_custom_prompt(mut self) -> Self {
        self.setup_custom_prompt = false;
        self
    }
}

/// Configuration for session cleanup interactions after subprocess exits.
///
/// Controls how [`exit_session()`] responds to cleanup prompts.
/// All fields have sensible defaults for common test scenarios.
#[derive(Debug, Clone)]
pub struct SessionExitConfig {
    /// Whether to expect the squash prompt.
    ///
    /// Set to `false` for fast subcommands that don't trigger auto-commits,
    /// or when testing with `on_exit = "squash"` or `on_exit = "keep"` config.
    ///
    /// Default: `true` (prompt appears with default `on_exit = "prompt"` config)
    pub expect_squash_prompt: bool,

    /// Response to "Squash auto-commits into a single commit? [y/N]:"
    ///
    /// Only used when `expect_squash_prompt` is `true`.
    ///
    /// Default: `false` (keep commits)
    pub squash_commits: bool,

    /// How to handle the sync flow TUI if modified ignored files exist.
    ///
    /// Default: [`SyncFlowResponse::ShouldNotAppear`]
    pub sync_flow: SyncFlowResponse,

    /// Response to "Delete worktree directory? [y/N]:"
    ///
    /// Default: `true` (delete worktree)
    pub delete_worktree: bool,
}

impl Default for SessionExitConfig {
    fn default() -> Self {
        Self {
            expect_squash_prompt: true,
            squash_commits: false,
            sync_flow: SyncFlowResponse::ShouldNotAppear,
            delete_worktree: true,
        }
    }
}

#[allow(dead_code)]
impl SessionExitConfig {
    /// Standard exit config: expect squash prompt (decline), no sync, delete worktree.
    pub fn standard() -> Self {
        Self::default()
    }

    /// Configure whether to expect the squash prompt.
    pub fn with_expect_squash_prompt(mut self, expect: bool) -> Self {
        self.expect_squash_prompt = expect;
        self
    }

    /// Configure squash response (implies expecting the squash prompt).
    pub fn with_squash(mut self, squash: bool) -> Self {
        self.expect_squash_prompt = true;
        self.squash_commits = squash;
        self
    }

    /// Expect sync flow and select all items.
    pub fn with_sync_select_all(mut self) -> Self {
        self.sync_flow = SyncFlowResponse::SelectAll;
        self
    }

    /// Expect sync flow and skip without syncing.
    pub fn with_sync_skip(mut self) -> Self {
        self.sync_flow = SyncFlowResponse::Skip;
        self
    }

    /// Expect sync flow, then send Ctrl+C at the prompt to cancel.
    /// This causes the process to exit immediately, skipping subsequent prompts.
    /// Panics if sync prompt doesn't appear.
    pub fn with_sync_ctrl_c_at_prompt(mut self) -> Self {
        self.sync_flow = SyncFlowResponse::CtrlCAtPrompt;
        self
    }

    /// Expect sync flow, enter selection mode, then send Ctrl+C to cancel.
    /// This causes the process to exit immediately, skipping subsequent prompts.
    /// Panics if sync prompt doesn't appear.
    pub fn with_sync_ctrl_c_during_selection(mut self) -> Self {
        self.sync_flow = SyncFlowResponse::CtrlCDuringSelection;
        self
    }

    /// Try to send Ctrl+C at sync prompt if it appears.
    /// If sync prompt doesn't appear within timeout, gracefully continues to delete worktree prompt.
    /// Use this for tests where sync flow may or may not trigger depending on timing.
    pub fn with_sync_ctrl_c_at_prompt_if_present(mut self) -> Self {
        self.sync_flow = SyncFlowResponse::CtrlCAtPromptIfPresent;
        self
    }

    /// Try to enter selection and send Ctrl+C if sync prompt appears.
    /// If sync prompt doesn't appear within timeout, gracefully continues to delete worktree prompt.
    /// Use this for tests where sync flow may or may not trigger depending on timing.
    pub fn with_sync_ctrl_c_during_selection_if_present(mut self) -> Self {
        self.sync_flow = SyncFlowResponse::CtrlCDuringSelectionIfPresent;
        self
    }

    /// Configure worktree deletion response.
    pub fn with_delete_worktree(mut self, delete: bool) -> Self {
        self.delete_worktree = delete;
        self
    }

    /// Alias for preserving worktree (sets `delete_worktree` to `false`).
    pub fn preserve_worktree(self) -> Self {
        self.with_delete_worktree(false)
    }
}

// =============================================================================
// Main Session Functions
// =============================================================================

/// Spawns treebeard and handles all pre-subprocess startup interactions.
///
/// This function:
/// 1. Spawns treebeard with the given branch name and a bash shell
/// 2. Waits for the startup message indicating the subprocess is ready
/// 3. Optionally sets up a custom PS1 for reliable command synchronization
///
/// Returns a session ready for shell commands.
///
/// # Arguments
///
/// * `treebeard_path` - Path to the treebeard binary
/// * `branch_name` - Name of the branch to create/use
/// * `repo_path` - Path to the git repository
/// * `config` - Configuration for startup behavior
///
/// # Example
///
/// ```ignore
/// let mut session = start_session(
///     &treebeard_path,
///     "feature-branch",
///     &workspace.repo_path,
///     SessionStartConfig::default(),
/// );
/// ```
#[allow(dead_code)]
pub fn start_session(
    treebeard_path: &Path,
    branch_name: &str,
    repo_path: &Path,
    config: SessionStartConfig,
) -> TestSession {
    use std::process::Command;

    let mut cmd = Command::new(treebeard_path);
    cmd.arg("branch")
        .arg(branch_name)
        .arg("--")
        .arg("bash")
        // --norc and --noprofile ensure a clean shell environment without user
        // customizations that could interfere with test expectations.
        .arg("--norc")
        .arg("--noprofile")
        .current_dir(repo_path);

    let mut session = expectrl::session::Session::spawn(cmd).expect("Failed to spawn treebeard");
    session.set_expect_timeout(Some(DEFAULT_TIMEOUT));

    // Wait for treebeard to print its startup messages and launch bash.
    // We look for the message that indicates treebeard has launched the subprocess.
    let result = session.expect("subprocess terminates");
    if let Err(ref e) = result {
        eprintln!("Failed to see treebeard startup. Error: {:?}", e);
        eprintln!("This may indicate treebeard failed to start or exited early.");
    }
    result.expect("Should see treebeard startup message");

    // Now bash should be running. Wait a moment for it to fully initialize.
    std::thread::sleep(Duration::from_millis(200));

    if config.setup_custom_prompt {
        // Set our custom PS1 for reliable command synchronization
        session
            .send_line(format!("export PS1='{}'", TEST_SHELL_PROMPT))
            .expect("Failed to set PS1");

        // The first expect will see the echoed command, then our prompt
        session
            .expect(TEST_SHELL_PROMPT)
            .expect("Failed to see custom prompt after setting PS1");
    }

    session
}

/// Exits a treebeard session and handles all cleanup prompts.
///
/// Sends "exit" to the shell, then handles prompts in order:
/// 1. Squash prompt (if `expect_squash_prompt` is true)
/// 2. Sync flow (if `sync_flow` is not `ShouldNotAppear`)
/// 3. Worktree deletion prompt
/// 4. Waits for EOF
///
/// # Arguments
///
/// * `session` - The expectrl session to exit
/// * `config` - Configuration controlling responses to each prompt
///
/// # Example
///
/// ```ignore
/// // Standard exit: decline squash, no sync expected, delete worktree
/// exit_session(&mut session, SessionExitConfig::default());
///
/// // Custom: squash commits and preserve worktree
/// exit_session(&mut session, SessionExitConfig::standard()
///     .with_squash(true)
///     .preserve_worktree());
/// ```
#[allow(dead_code)]
pub fn exit_session(session: &mut TestSession, config: SessionExitConfig) {
    session.send_line("exit").expect("Failed to send exit");

    // 1. Handle squash prompt (if expected)
    if config.expect_squash_prompt {
        session
            .expect("Squash auto-commits")
            .expect("Should see squash prompt");

        let squash_response = if config.squash_commits { "y" } else { "n" };
        session
            .send_line(squash_response)
            .expect("Failed to send squash response");
    }

    // 2. Handle sync flow based on config
    // The sync flow has two stages:
    //   1. Initial prompt: "Sync changes back to main repo? [S]ync all / [s]elect interactive / [N]one"
    //   2. Selection menu (only if 's' chosen): "Select items to sync:"
    // For Ctrl+C variants, we exit early as the process terminates immediately
    match config.sync_flow {
        SyncFlowResponse::SelectAll => {
            // Wait for initial sync prompt, then enter selection mode
            // Note: prompt varies - "Sync back to main repo?" (single file) or
            // "Sync changes back to main repo?" (multiple files)
            session
                .expect(expectrl::Regex("Sync (back|changes back) to main repo"))
                .expect("Should see sync prompt");
            session.send_line("s").expect("Failed to send 's'");
            // Now in selection menu
            session
                .expect("Select items to sync")
                .expect("Should see selection menu");
            std::thread::sleep(Duration::from_millis(100));
            session
                .send("a")
                .expect("Failed to send 'a' for select all");
            std::thread::sleep(Duration::from_millis(100));
            session.send("d").expect("Failed to send 'd' for done");
        }
        SyncFlowResponse::Skip => {
            // Wait for initial sync prompt, choose 'N' to skip
            session
                .expect(expectrl::Regex("Sync (back|changes back) to main repo"))
                .expect("Should see sync prompt");
            session
                .send_line("n")
                .expect("Failed to send 'n' to skip sync");
        }
        SyncFlowResponse::CtrlCAtPrompt => {
            // Wait for the initial sync prompt, then send Ctrl+C
            session
                .expect(expectrl::Regex("Sync (back|changes back) to main repo"))
                .expect("Should see sync prompt");
            std::thread::sleep(Duration::from_millis(100));
            session.send("\x03").expect("Failed to send Ctrl+C");
            // Process exits immediately after Ctrl+C, skip remaining prompts
            session
                .expect(Eof)
                .expect("Process should exit after Ctrl+C");
            std::thread::sleep(Duration::from_millis(500));
            return;
        }
        SyncFlowResponse::CtrlCDuringSelection => {
            // Wait for initial sync prompt, enter selection mode, then Ctrl+C
            session
                .expect(expectrl::Regex("Sync (back|changes back) to main repo"))
                .expect("Should see sync prompt");
            session.send_line("s").expect("Failed to send 's'");
            session
                .expect("Select items to sync")
                .expect("Should see selection menu");
            std::thread::sleep(Duration::from_millis(100));
            session.send("\x03").expect("Failed to send Ctrl+C");
            // Process exits immediately after Ctrl+C, skip remaining prompts
            session
                .expect(Eof)
                .expect("Process should exit after Ctrl+C");
            std::thread::sleep(Duration::from_millis(500));
            return;
        }
        SyncFlowResponse::CtrlCAtPromptIfPresent => {
            // Try to find sync prompt with a shorter timeout
            // If sync appears, send Ctrl+C and exit early
            // If delete worktree appears instead, continue to normal flow
            let short_timeout = Duration::from_secs(10);
            session.set_expect_timeout(Some(short_timeout));

            let sync_result =
                session.expect(expectrl::Regex("Sync (back|changes back) to main repo"));
            session.set_expect_timeout(Some(DEFAULT_TIMEOUT));

            if sync_result.is_ok() {
                // Sync prompt appeared, send Ctrl+C
                std::thread::sleep(Duration::from_millis(100));
                session.send("\x03").expect("Failed to send Ctrl+C");
                session
                    .expect(Eof)
                    .expect("Process should exit after Ctrl+C");
                std::thread::sleep(Duration::from_millis(500));
                return;
            }
            // Sync didn't appear, fall through to delete worktree prompt
        }
        SyncFlowResponse::CtrlCDuringSelectionIfPresent => {
            // Try to find sync prompt with a shorter timeout
            // If sync appears, enter selection mode, send Ctrl+C and exit early
            // If delete worktree appears instead, continue to normal flow
            let short_timeout = Duration::from_secs(10);
            session.set_expect_timeout(Some(short_timeout));

            let sync_result =
                session.expect(expectrl::Regex("Sync (back|changes back) to main repo"));
            session.set_expect_timeout(Some(DEFAULT_TIMEOUT));

            if sync_result.is_ok() {
                // Sync prompt appeared, enter selection mode
                session.send_line("s").expect("Failed to send 's'");
                session
                    .expect("Select items to sync")
                    .expect("Should see selection menu");
                std::thread::sleep(Duration::from_millis(100));
                session.send("\x03").expect("Failed to send Ctrl+C");
                session
                    .expect(Eof)
                    .expect("Process should exit after Ctrl+C");
                std::thread::sleep(Duration::from_millis(500));
                return;
            }
            // Sync didn't appear, fall through to delete worktree prompt
        }
        SyncFlowResponse::ShouldNotAppear => {
            // Don't wait for sync flow, proceed directly to worktree prompt
        }
    }

    // 3. Handle worktree deletion prompt
    session
        .expect("Delete worktree directory?")
        .expect("Should see cleanup prompt");

    let delete_response = if config.delete_worktree { "y" } else { "n" };
    session
        .send_line(delete_response)
        .expect("Failed to send cleanup response");

    // 4. Wait for process exit
    session
        .expect(Eof)
        .expect("Process should exit after cleanup");

    // Give FUSE time to unmount before next test
    std::thread::sleep(Duration::from_millis(500));
}
// =============================================================================
// Shell Command Helpers
// =============================================================================

/// Executes a command in the shell and waits for completion.
///
/// Sends the command, then waits for the prompt to reappear,
/// indicating the command has finished.
#[allow(dead_code)]
pub fn shell_exec(session: &mut TestSession, command: &str) {
    session.send_line(command).expect("Failed to send command");
    session
        .expect(TEST_SHELL_PROMPT)
        .unwrap_or_else(|_| panic!("Command did not complete: {}", command));
}

/// Executes a command and returns its output.
///
/// Useful for commands like `pwd`, `cat`, `ls` where you need
/// to capture and verify the output.
#[allow(dead_code)]
pub fn shell_exec_output(session: &mut TestSession, command: &str) -> String {
    session.send_line(command).expect("Failed to send command");

    let output = session
        .expect(TEST_SHELL_PROMPT)
        .unwrap_or_else(|_| panic!("Command did not complete: {}", command));

    // Parse output: it's between the command echo and the prompt
    let output_str = String::from_utf8_lossy(output.as_bytes()).to_string();

    // Remove the echoed command from the start and prompt from the end
    output_str
        .lines()
        .skip(1) // Skip echoed command
        .take_while(|line| !line.contains(TEST_SHELL_PROMPT))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Gets the current working directory from the shell.
#[allow(dead_code)]
pub fn get_pwd(session: &mut TestSession) -> String {
    shell_exec_output(session, "pwd").trim().to_string()
}

/// Creates a file via shell command.
#[allow(dead_code)]
pub fn shell_create_file(session: &mut TestSession, filename: &str, content: &str) {
    // Escape single quotes in content by replacing ' with '\''
    let escaped_content = content.replace("'", "'\\''");
    let cmd = format!("echo '{}' > '{}'", escaped_content, filename);
    shell_exec(session, &cmd);
}

/// Appends content to a file via shell command.
#[allow(dead_code)]
pub fn shell_append_file(session: &mut TestSession, filename: &str, content: &str) {
    let escaped_content = content.replace("'", "'\\''");
    let cmd = format!("echo '{}' >> '{}'", escaped_content, filename);
    shell_exec(session, &cmd);
}

/// Reads file content via shell command.
#[allow(dead_code)]
pub fn shell_read_file(session: &mut TestSession, filename: &str) -> String {
    shell_exec_output(session, &format!("cat '{}'", filename))
}

/// Creates a directory (including parents) via shell command.
#[allow(dead_code)]
pub fn shell_mkdir(session: &mut TestSession, path: &str) {
    shell_exec(session, &format!("mkdir -p '{}'", path));
}

/// Removes a file via shell command.
#[allow(dead_code)]
pub fn shell_rm(session: &mut TestSession, path: &str) {
    shell_exec(session, &format!("rm -f '{}'", path));
}

/// Removes a directory recursively via shell command.
#[allow(dead_code)]
pub fn shell_rm_rf(session: &mut TestSession, path: &str) {
    shell_exec(session, &format!("rm -rf '{}'", path));
}

/// Lists directory contents via shell command.
#[allow(dead_code)]
pub fn shell_ls(session: &mut TestSession, path: &str) -> String {
    shell_exec_output(session, &format!("ls -la '{}'", path))
}

/// Checks if a file exists via shell command.
#[allow(dead_code)]
pub fn shell_file_exists(session: &mut TestSession, path: &str) -> bool {
    let output = shell_exec_output(
        session,
        &format!("test -e '{}' && echo 'EXISTS' || echo 'NOTFOUND'", path),
    );
    output.contains("EXISTS")
}

/// Sleep for a duration via shell command.
#[allow(dead_code)]
pub fn shell_sleep(session: &mut TestSession, seconds: f64) {
    shell_exec(session, &format!("sleep {}", seconds));
}

// =============================================================================
// Specialized Session Functions
// =============================================================================

/// Spawns treebeard for a command that's expected to fail/error.
///
/// Does not spawn a shell - just runs treebeard and expects it to
/// output an error and exit (without showing cleanup prompt).
#[allow(dead_code)]
pub fn spawn_treebeard_expect_error(
    treebeard_path: &Path,
    args: &[&str],
    repo_path: &Path,
) -> TestSession {
    use std::process::Command;

    let mut cmd = Command::new(treebeard_path);
    cmd.args(args).current_dir(repo_path);

    let mut session = expectrl::session::Session::spawn(cmd).expect("Failed to spawn treebeard");
    session.set_expect_timeout(Some(DEFAULT_TIMEOUT));

    session
}

/// Waits for an error message and EOF (for tests expecting failures).
#[allow(dead_code)]
pub fn expect_error_contains(session: &mut TestSession, expected_error: &str) {
    let output = session.expect(Eof).expect("Process should exit");
    let output_str = String::from_utf8_lossy(output.as_bytes());

    assert!(
        output_str
            .to_lowercase()
            .contains(&expected_error.to_lowercase()),
        "Expected error containing '{}', got: {}",
        expected_error,
        output_str
    );
}

/// Spawns treebeard with a custom subcommand (not a shell).
///
/// This is used for testing `treebeard branch <name> -- <command> [args...]`.
/// The subcommand should be a short-running command (like `true`, `false`, `echo`).
///
/// This helper:
/// 1. Spawns treebeard with the subcommand via PTY
/// 2. Waits for the subprocess to complete
/// 3. Handles the cleanup prompt (answers 'n' to preserve worktree)
/// 4. Returns the captured output and exit code
#[allow(dead_code)]
pub fn spawn_treebeard_with_subcommand(
    treebeard_path: &Path,
    branch_name: &str,
    repo_path: &Path,
    subcommand: &[&str],
) -> (String, i32) {
    spawn_treebeard_with_subcommand_and_config(
        treebeard_path,
        branch_name,
        repo_path,
        subcommand,
        SessionExitConfig::standard()
            .with_expect_squash_prompt(false)
            .preserve_worktree(),
    )
}

/// Spawns treebeard with a custom subcommand and configurable exit behavior.
///
/// Like [`spawn_treebeard_with_subcommand()`] but allows customizing cleanup responses.
///
/// # Arguments
///
/// * `treebeard_path` - Path to the treebeard binary
/// * `branch_name` - Name of the branch to create/use
/// * `repo_path` - Path to the git repository
/// * `subcommand` - The command and arguments to run instead of a shell
/// * `exit_config` - Configuration for cleanup prompts
///
/// # Returns
///
/// A tuple of (captured output, exit code).
#[allow(dead_code)]
pub fn spawn_treebeard_with_subcommand_and_config(
    treebeard_path: &Path,
    branch_name: &str,
    repo_path: &Path,
    subcommand: &[&str],
    exit_config: SessionExitConfig,
) -> (String, i32) {
    use std::process::Command;

    let mut cmd = Command::new(treebeard_path);
    cmd.arg("branch").arg(branch_name).arg("--");
    for arg in subcommand {
        cmd.arg(arg);
    }
    cmd.current_dir(repo_path);

    let mut session = expectrl::session::Session::spawn(cmd).expect("Failed to spawn treebeard");
    session.set_expect_timeout(Some(DEFAULT_TIMEOUT));

    // Collect output before cleanup prompts
    let mut output_parts = Vec::new();

    // Handle squash prompt if expected
    if exit_config.expect_squash_prompt {
        let output = session
            .expect("Squash auto-commits")
            .expect("Should see squash prompt after subprocess exits");
        output_parts.push(String::from_utf8_lossy(output.as_bytes()).to_string());

        let squash_response = if exit_config.squash_commits { "y" } else { "n" };
        session
            .send_line(squash_response)
            .expect("Failed to send squash response");
    }

    // Handle sync flow
    // The sync flow has two stages:
    //   1. Initial prompt: "Sync changes back to main repo? [S]ync all / [s]elect interactive / [N]one"
    //   2. Selection menu (only if 's' chosen): "Select items to sync:"
    // For Ctrl+C variants, we exit early as the process terminates immediately
    let early_exit = match exit_config.sync_flow {
        SyncFlowResponse::SelectAll => {
            // Wait for initial sync prompt, then enter selection mode
            let output = session
                .expect(expectrl::Regex("Sync (back|changes back) to main repo"))
                .expect("Should see sync prompt");
            output_parts.push(String::from_utf8_lossy(output.as_bytes()).to_string());
            session.send_line("s").expect("Failed to send 's'");
            // Now in selection menu
            let output = session
                .expect("Select items to sync")
                .expect("Should see selection menu");
            output_parts.push(String::from_utf8_lossy(output.as_bytes()).to_string());
            std::thread::sleep(Duration::from_millis(100));
            session.send("a").expect("Failed to send 'a'");
            std::thread::sleep(Duration::from_millis(100));
            session.send("d").expect("Failed to send 'd'");
            false
        }
        SyncFlowResponse::Skip => {
            // Wait for initial sync prompt, choose 'N' to skip
            let output = session
                .expect(expectrl::Regex("Sync (back|changes back) to main repo"))
                .expect("Should see sync prompt");
            output_parts.push(String::from_utf8_lossy(output.as_bytes()).to_string());
            session
                .send_line("n")
                .expect("Failed to send 'n' to skip sync");
            false
        }
        SyncFlowResponse::CtrlCAtPrompt => {
            let output = session
                .expect(expectrl::Regex("Sync (back|changes back) to main repo"))
                .expect("Should see sync prompt");
            output_parts.push(String::from_utf8_lossy(output.as_bytes()).to_string());
            std::thread::sleep(Duration::from_millis(100));
            session.send("\x03").expect("Failed to send Ctrl+C");
            true
        }
        SyncFlowResponse::CtrlCDuringSelection => {
            let output = session
                .expect(expectrl::Regex("Sync (back|changes back) to main repo"))
                .expect("Should see sync prompt");
            output_parts.push(String::from_utf8_lossy(output.as_bytes()).to_string());
            session.send_line("s").expect("Failed to send 's'");
            let output = session
                .expect("Select items to sync")
                .expect("Should see selection menu");
            output_parts.push(String::from_utf8_lossy(output.as_bytes()).to_string());
            std::thread::sleep(Duration::from_millis(100));
            session.send("\x03").expect("Failed to send Ctrl+C");
            true
        }
        SyncFlowResponse::CtrlCAtPromptIfPresent => {
            // Try to find sync prompt with a shorter timeout
            let short_timeout = Duration::from_secs(10);
            session.set_expect_timeout(Some(short_timeout));
            let sync_result =
                session.expect(expectrl::Regex("Sync (back|changes back) to main repo"));
            session.set_expect_timeout(Some(DEFAULT_TIMEOUT));

            if let Ok(output) = sync_result {
                output_parts.push(String::from_utf8_lossy(output.as_bytes()).to_string());
                std::thread::sleep(Duration::from_millis(100));
                session.send("\x03").expect("Failed to send Ctrl+C");
                true
            } else {
                false // Sync didn't appear, continue to delete worktree prompt
            }
        }
        SyncFlowResponse::CtrlCDuringSelectionIfPresent => {
            // Try to find sync prompt with a shorter timeout
            let short_timeout = Duration::from_secs(10);
            session.set_expect_timeout(Some(short_timeout));
            let sync_result =
                session.expect(expectrl::Regex("Sync (back|changes back) to main repo"));
            session.set_expect_timeout(Some(DEFAULT_TIMEOUT));

            if let Ok(output) = sync_result {
                output_parts.push(String::from_utf8_lossy(output.as_bytes()).to_string());
                session.send_line("s").expect("Failed to send 's'");
                let output = session
                    .expect("Select items to sync")
                    .expect("Should see selection menu");
                output_parts.push(String::from_utf8_lossy(output.as_bytes()).to_string());
                std::thread::sleep(Duration::from_millis(100));
                session.send("\x03").expect("Failed to send Ctrl+C");
                true
            } else {
                false // Sync didn't appear, continue to delete worktree prompt
            }
        }
        SyncFlowResponse::ShouldNotAppear => false,
    };

    if !early_exit {
        // Handle worktree deletion prompt
        let output_before_prompt = session
            .expect("Delete worktree directory?")
            .expect("Should see cleanup prompt after subprocess exits");
        output_parts.push(String::from_utf8_lossy(output_before_prompt.as_bytes()).to_string());

        let delete_response = if exit_config.delete_worktree {
            "y"
        } else {
            "n"
        };
        session
            .send_line(delete_response)
            .expect("Failed to send cleanup response");
    }

    // Wait for process to exit and capture remaining output
    let output_after = session
        .expect(Eof)
        .expect("Process should exit after cleanup");
    output_parts.push(String::from_utf8_lossy(output_after.as_bytes()).to_string());

    let output_str = output_parts.join("");

    // Get exit status
    use expectrl::process::unix::WaitStatus;
    let status = session
        .get_process_mut()
        .wait()
        .expect("Failed to wait for process");
    // POSIX convention: signal exits are 128 + signal number
    let exit_code = match status {
        WaitStatus::Exited(_, code) => code,
        WaitStatus::Signaled(_, sig, _) => 128 + sig as i32,
        _ => -1, // Unknown status (should not occur in normal operation)
    };

    // Brief delay to allow PTY cleanup to complete before next test
    // This prevents race conditions when tests run in sequence
    std::thread::sleep(Duration::from_millis(100));

    (output_str, exit_code)
}

// =============================================================================
// Test Session Helpers
// =============================================================================

use nix::sys::signal;
use nix::unistd::Pid;
use std::process::{Child, Command};
use std::thread;

#[allow(dead_code)]
pub fn spawn_treebeard_test_mode(branch_name: &str, repo_path: &Path) -> Child {
    std::env::set_var("TREEBEARD_TEST_MODE", "1");

    let child = Command::new(assert_cmd::cargo::cargo_bin!("treebeard"))
        .arg("branch")
        .arg(branch_name)
        .current_dir(repo_path)
        .spawn()
        .expect("Failed to spawn treebeard");

    thread::sleep(Duration::from_millis(500));
    child
}

#[allow(dead_code)]
pub fn terminate_treebeard(child: Child) {
    terminate_treebeard_with_timeout(child, Duration::from_millis(500));
}

#[allow(dead_code)]
pub fn terminate_treebeard_with_timeout(mut child: Child, timeout: Duration) {
    send_signal(&child, signal::Signal::SIGINT);

    let start = std::time::Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(_status)) => {
                break;
            }
            Ok(None) => {
                if start.elapsed() >= timeout {
                    eprintln!("Process did not exit within {:?}, sending SIGKILL", timeout);
                    let _ = child.kill();
                    let _ = child.wait();
                    break;
                }
                thread::sleep(Duration::from_millis(50));
            }
            Err(e) => {
                eprintln!("Error checking process status: {}", e);
                let _ = child.kill();
                let _ = child.wait();
                break;
            }
        }
    }

    std::env::remove_var("TREEBEARD_TEST_MODE");
}

#[allow(dead_code)]
pub fn send_signal(child: &Child, sig: signal::Signal) {
    let pid = Pid::from_raw(child.id() as i32);
    signal::kill(pid, sig).expect("Failed to send signal");
}
