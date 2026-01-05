use crate::cli::ConfigAction;
use crate::config::{get_config_path, load_config};
use crate::error::{Result, TreebeardError};

pub fn handle_config_command(action: Option<ConfigAction>) -> Result<()> {
    match action {
        Some(ConfigAction::Path) => {
            let config_path = get_config_path();
            println!("Config location: {}", config_path.display());
        }
        None | Some(ConfigAction::Show) => {
            let config_path = get_config_path();
            if !config_path.exists() {
                let _ = load_config()?;
            }
            let config = load_config()?;
            println!("Config file: {}", config_path.display());
            println!();
            println!("Current configuration:");
            println!("  Paths:");
            println!("    worktree_dir: {}", config.paths.get_worktree_dir());
            println!("    mount_dir: {}", config.paths.get_mount_dir());
            println!("  Cleanup:");
            println!("    on_exit: {}", config.cleanup.on_exit);
            println!("  Commit:");
            println!(
                "    auto_commit_message: {}",
                config.commit.get_auto_commit_message()
            );
            println!(
                "    squash_commit_message: {}",
                config.commit.get_squash_commit_message()
            );
            println!("  Auto Commit Timing:");
            println!(
                "    auto_commit_debounce_ms: {}",
                config.auto_commit_timing.get_debounce_ms()
            );
            println!("  Other:");
            println!("    fuse_ttl_secs: {}", config.get_fuse_ttl_secs());
            if !config.sync.get_sync_always_skip().is_empty() {
                println!(
                    "    sync_always_skip: {:?}",
                    config.sync.get_sync_always_skip()
                );
            }
            if !config.sync.get_sync_always_include().is_empty() {
                println!(
                    "    sync_always_include: {:?}",
                    config.sync.get_sync_always_include()
                );
            }
        }
        Some(ConfigAction::Edit) => {
            let config_path = get_config_path();
            if !config_path.exists() {
                let _ = load_config()?;
                println!("Created default config at {}", config_path.display());
            }

            let editor = std::env::var("EDITOR").unwrap_or_else(|_| "vi".to_string());
            let status = std::process::Command::new(&editor)
                .arg(&config_path)
                .status()
                .map_err(|e| {
                    TreebeardError::Config(format!(
                        "Failed to open editor '{}': {}. Set EDITOR environment variable to your preferred editor.",
                        editor, e
                    ))
                })?;

            if !status.success() {
                return Err(TreebeardError::Config(format!(
                    "Editor '{}' exited with non-zero status",
                    editor
                )));
            }
        }
    }
    Ok(())
}
