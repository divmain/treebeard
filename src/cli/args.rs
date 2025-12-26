use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(name = "git treebeard")]
#[command(
    about = "Create isolated Git worktree environments with copy-on-write semantics for ignored files"
)]
pub struct Args {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    #[command(about = "Create a new branch environment")]
    Branch {
        #[arg(help = "Name of the new branch")]
        branch_name: String,
        #[arg(long, hide = true, help = "Skip spawning shell (for testing)")]
        no_shell: bool,
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        command: Vec<String>,
    },
    #[command(about = "Manage configuration")]
    Config {
        #[command(subcommand)]
        action: Option<ConfigAction>,
    },
    #[command(about = "Diagnose common issues and system state")]
    Doctor,
    #[command(about = "List active sessions")]
    List {
        #[arg(long, help = "Machine-readable output (tab-separated values)")]
        porcelain: bool,

        #[arg(long, help = "JSON output")]
        json: bool,
    },
    #[command(about = "Print path to a worktree's mount point")]
    Path {
        #[arg(help = "Branch name")]
        branch_name: String,

        #[arg(short, long, help = "Print worktree path instead of mount path")]
        worktree: bool,
    },
    #[command(about = "Manually clean up branch(es)")]
    Cleanup {
        #[arg(help = "Name of the branch(es) to clean up")]
        branch_names: Vec<String>,

        #[arg(long, help = "Clean up all treebeard worktrees for this repo")]
        all: bool,

        #[arg(long, help = "Also delete the branch(es)")]
        delete_branch: bool,

        #[arg(short = 'y', long, help = "Skip confirmation prompts")]
        yes: bool,

        #[arg(long, help = "Force cleanup even with uncommitted changes")]
        force: bool,

        #[arg(long, help = "Clean up stale FUSE mounts from crashed sessions")]
        stale: bool,
    },
}

#[derive(Subcommand, Debug)]
pub enum ConfigAction {
    #[command(about = "Show current configuration values")]
    Show,
    #[command(about = "Open config file in editor")]
    Edit,
    #[command(about = "Show config file path")]
    Path,
}
