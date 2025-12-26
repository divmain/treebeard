use crate::config::SandboxConfig;
use crate::error::Result;
use nix::sys::signal::{self, SigHandler, Signal};
use nix::unistd::{setpgid, tcsetpgrp, Pid};
use std::path::Path;
use tokio::process::Command as TokioCommand;

use tracing::debug;

#[cfg(target_os = "macos")]
use crate::sandbox::generate_sbpl_profile;

/// Spawns a subprocess in the given working directory.
///
/// On macOS, if sandbox configuration is provided and enabled, the subprocess
/// will run inside a sandbox-exec sandbox with restricted filesystem and network access.
///
/// # Arguments
/// * `working_dir` - The directory to run the subprocess in
/// * `branch_name` - The branch name (set as TREEBEARD_BRANCH env var)
/// * `command` - Optional command to run (defaults to user's shell)
/// * `sandbox_config` - Optional sandbox configuration for macOS
/// * `mount_path` - The FUSE mount path (used for sandbox write permissions)
pub fn spawn_subprocess_async(
    working_dir: &Path,
    branch_name: &str,
    command: Option<&[String]>,
    sandbox_config: Option<&SandboxConfig>,
    mount_path: Option<&Path>,
) -> Result<tokio::process::Child> {
    let (program, args) = match command {
        Some(cmd) if !cmd.is_empty() => (
            cmd[0].clone(),
            cmd[1..].iter().map(|s| s.as_str()).collect::<Vec<_>>(),
        ),
        _ => {
            let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".to_string());
            (shell, Vec::new())
        }
    };

    let mut cmd_args = Vec::new();
    for arg in &args {
        cmd_args.push(arg.to_string());
    }

    // Determine if we should use sandbox
    #[cfg(target_os = "macos")]
    let use_sandbox = sandbox_config
        .map(|c| c.enabled && mount_path.is_some())
        .unwrap_or(false);

    #[cfg(not(target_os = "macos"))]
    let use_sandbox = false;

    // Build the actual command, potentially wrapping with sandbox-exec
    #[cfg(target_os = "macos")]
    let (final_program, final_args) = if use_sandbox {
        let config = sandbox_config.unwrap();
        let mount = mount_path.unwrap();
        let profile = generate_sbpl_profile(config, mount);

        debug!("Sandbox enabled, using profile:\n{}", profile);

        // Build sandbox-exec command: sandbox-exec -p <profile> <program> <args...>
        let mut sandbox_args = vec!["-p".to_string(), profile, program];
        sandbox_args.extend(cmd_args);

        ("sandbox-exec".to_string(), sandbox_args)
    } else {
        (program, cmd_args)
    };

    #[cfg(not(target_os = "macos"))]
    let (final_program, final_args) = (program, cmd_args);

    // SAFETY: We're setting up the child to be in its own process group
    // and making it the foreground process group of the terminal.
    // This ensures Ctrl+C goes to the subprocess, not to treebeard.
    unsafe {
        let mut cmd = TokioCommand::new(&final_program);
        cmd.current_dir(working_dir)
            .args(&final_args)
            .stdin(std::process::Stdio::inherit())
            .stdout(std::process::Stdio::inherit())
            .stderr(std::process::Stdio::inherit())
            .env("TREEBEARD_ACTIVE", "1")
            .env("TREEBEARD_BRANCH", branch_name)
            .pre_exec(|| {
                // Put the child in its own process group
                let pid = Pid::from_raw(0); // 0 means "this process"
                setpgid(pid, pid).map_err(std::io::Error::other)?;
                Ok(())
            });

        let child = cmd.spawn().map_err(crate::error::TreebeardError::Io)?;

        // Make the child's process group the foreground process group of the terminal.
        // This ensures Ctrl+C signals go to the subprocess, not to us.
        if let Some(pid) = child.id() {
            let child_pid = Pid::from_raw(pid as i32);
            // tcsetpgrp may fail if stdin isn't a terminal (e.g., piped input in tests).
            // This is acceptable; the subprocess will still run, just without foreground control.
            if let Err(e) = tcsetpgrp(std::io::stdin(), child_pid) {
                debug!("Failed to set child process group as foreground: {}", e);
            }
        }

        Ok(child)
    }
}

/// Restore treebeard as the foreground process group after the shell exits
pub fn restore_foreground() {
    // SAFETY: Temporarily ignoring SIGTTOU is safe and necessary here.
    // Without this, tcsetpgrp would send SIGTTOU to our process (stopping it)
    // because we're a background process trying to modify the terminal's
    // foreground process group.
    unsafe {
        let _ = signal::signal(Signal::SIGTTOU, SigHandler::SigIgn);
    }

    let our_pgid = nix::unistd::getpgrp();
    // May fail if stdin isn't a terminal; this is acceptable in non-TTY contexts.
    if let Err(e) = tcsetpgrp(std::io::stdin(), our_pgid) {
        debug!("Failed to restore our process group as foreground: {}", e);
    }

    // SAFETY: Restoring default signal handling. No resources to clean up.
    unsafe {
        let _ = signal::signal(Signal::SIGTTOU, SigHandler::SigDfl);
    }
}
