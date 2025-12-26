use clap::Parser;
use std::sync::atomic::AtomicUsize;
use std::sync::Arc;

mod cleanup;
mod cli;
mod commands;
mod config;
mod error;
mod git;
mod hooks;
mod overlay;
mod sandbox;
mod session;
mod shell;
mod sync;
mod watcher;

use cli::{Args, Commands};
use config::load_config;
use git::setup_git_environment;
use overlay::setup_overlay_and_watcher;
use session::{add_active_session, run_shell_and_cleanup};

#[tokio::main]
async fn main() {
    match run().await {
        Ok(code) => {
            std::process::exit(code);
        }
        Err(e) => {
            eprintln!("Error: {}", e);
            std::process::exit(1);
        }
    }
}

async fn run() -> error::Result<i32> {
    // macOS 15+ is required because macFUSE 4.x with kernel extension support
    // is only available on Sequoia and later due to Apple's security changes
    // that affect how kernel extensions are loaded on older macOS versions.
    #[cfg(target_os = "macos")]
    {
        if let Some((major, minor)) = config::get_macos_version() {
            if major < 15 {
                eprintln!("Error: treebeard requires macOS 15 (Sequoia) or later.");
                eprintln!("Current version: macOS {}.{}", major, minor);
                eprintln!();
                eprintln!("treebeard uses macFUSE for its overlay filesystem, which requires");
                eprintln!("macOS 15+ for proper operation.");
                std::process::exit(1);
            }
        }
    }

    tracing_subscriber::fmt()
        .with_env_filter(std::env::var("TREEBEARD_LOG").unwrap_or_else(|_| "info".to_string()))
        .init();

    let args = Args::parse();

    cli::check_tty_requirement_for_command(&args.command)?;

    match args.command {
        Commands::Branch {
            branch_name,
            no_shell,
            command,
        } => Ok(create_branch(&branch_name, no_shell, command).await?),
        Commands::Config { action } => {
            commands::handle_config_command(action)?;
            Ok(0)
        }
        Commands::Doctor => {
            commands::run_doctor()?;
            Ok(0)
        }
        Commands::List { porcelain, json } => {
            commands::list_active_sessions(porcelain, json)?;
            Ok(0)
        }
        Commands::Path {
            branch_name,
            worktree,
        } => {
            commands::print_path(&branch_name, worktree)?;
            Ok(0)
        }
        Commands::Cleanup {
            branch_names,
            all,
            delete_branch,
            yes,
            force,
            stale,
        } => {
            commands::cleanup_branch(branch_names, all, delete_branch, yes, force, stale)?;
            Ok(0)
        }
    }
}

async fn create_branch(
    branch_name: &str,
    no_shell: bool,
    command: Vec<String>,
) -> error::Result<i32> {
    cli::validate_branch_name(branch_name)?;

    let config = load_config()?;

    let git_env = setup_git_environment(branch_name)?;

    if no_shell {
        println!(
            "Branch '{}' created successfully (no shell mode)",
            branch_name
        );
        return Ok(0);
    }

    let failure_count = Arc::new(AtomicUsize::new(0));
    let overlay = setup_overlay_and_watcher(
        branch_name,
        &git_env.worktree_path,
        &git_env.main_repo_path,
        &config,
        failure_count.clone(),
    )?;

    if let Err(e) = add_active_session(
        &git_env.main_repo_path,
        branch_name,
        &git_env.worktree_path,
        &overlay.mount_path,
    ) {
        tracing::warn!("Failed to save session state: {}", e);
    }

    // Run post_create hooks after the environment is fully set up
    if !config.hooks.post_create.is_empty() {
        println!("Running post_create hooks...");
        let hook_context = hooks::HookContext::new(
            branch_name,
            &overlay.mount_path,
            &git_env.worktree_path,
            &git_env.main_repo_path,
        );
        if let Err(e) = hooks::run_hooks(
            &config.hooks.post_create,
            &hook_context,
            &overlay.mount_path,
        )
        .await
        {
            eprintln!("Warning: post_create hook failed: {}", e);
            // Continue with shell spawn even if hook fails
        }
    }

    if command.is_empty() {
        println!("[treebeard will clean up the ephemeral environment when the shell terminates]");
    } else {
        println!("Running: {}", command.join(" "));
        println!(
            "[treebeard will clean up the ephemeral environment when the subprocess terminates]"
        );
    }

    // Remind user about stashed changes
    if git_env.auto_stash_message.is_some() {
        println!("Note: Your stashed changes are available via 'git stash list'");
    }

    // Warn user about sandbox restrictions (macOS only)
    #[cfg(target_os = "macos")]
    {
        if config.sandbox.enabled && !config.sandbox.deny_read.is_empty() {
            println!("Sandbox enabled. Read access is blocked to:");
            for path in &config.sandbox.deny_read {
                println!("   {}", path);
            }
            println!(
                "To disable, set [sandbox] enabled = false in ~/.config/treebeard/config.toml"
            );
        }
    }
    println!();

    run_shell_and_cleanup(
        &overlay.mount_path,
        &git_env.worktree_path,
        branch_name,
        &config,
        &git_env.repo,
        overlay.mutations,
        Some(overlay.mount_path.clone()),
        if command.is_empty() {
            None
        } else {
            Some(&command)
        },
        &overlay.worktree_repo,
        &git_env.base_commit,
        failure_count,
        overlay.watcher_handle,
    )
    .await
}
