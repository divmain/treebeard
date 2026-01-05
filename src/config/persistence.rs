use crate::error::{Result, TreebeardError};
use std::io::{IsTerminal, Write};

use crate::config::paths::*;
use crate::config::schema::*;

pub fn load_config() -> Result<Config> {
    let config_dir = get_config_dir()?;
    let config_path = config_dir.join("config.toml");

    let mut config = if !config_path.exists() {
        let is_test_mode = std::env::var("TREEBEARD_TEST_MODE").is_ok();
        let is_explicit_config_dir = std::env::var("TREEBEARD_CONFIG_DIR").is_ok();
        let is_non_interactive = !std::io::stdin().is_terminal();

        let should_create = if is_test_mode || is_explicit_config_dir || is_non_interactive {
            true
        } else {
            println!("No config file found at {}", config_path.display());

            let stdin = std::io::stdin();
            let mut stdout = std::io::stdout();

            loop {
                print!("Create default config? [y/N]: ");
                stdout.flush().map_err(|e| {
                    TreebeardError::Config(format!("Failed to flush stdout: {}", e))
                })?;

                let mut input = String::new();
                stdin
                    .read_line(&mut input)
                    .map_err(|e| TreebeardError::Config(format!("Failed to read input: {}", e)))?;
                let choice = input.trim().to_lowercase();

                match choice.as_str() {
                    "" | "n" | "no" => break false,
                    "y" | "yes" => break true,
                    _ => eprintln!("Please enter 'y' or 'n'."),
                }
            }
        };

        if should_create {
            std::fs::create_dir_all(&config_dir).map_err(|e| {
                TreebeardError::Config(format!(
                    "Failed to create config directory {}: {}",
                    config_dir.display(),
                    e
                ))
            })?;
            let toml_str = toml::to_string_pretty(&Config::default()).map_err(|e| {
                TreebeardError::Config(format!("Failed to serialize config: {}", e))
            })?;
            std::fs::write(&config_path, toml_str).map_err(|e| {
                TreebeardError::Config(format!("Failed to write config file: {}", e))
            })?;

            eprintln!("Created default config at {}", config_path.display());
        }

        Config::default()
    } else {
        let toml_content = std::fs::read_to_string(&config_path)
            .map_err(|e| TreebeardError::Config(format!("Failed to read config file: {}", e)))?;

        toml::from_str(&toml_content)
            .map_err(|e| TreebeardError::Config(format!("Failed to parse config: {}", e)))?
    };

    if let Some(project_config) = load_project_config()? {
        config = merge_configs(config, project_config);
    }

    validate_config(&config)?;
    Ok(config)
}

fn load_project_config() -> Result<Option<Config>> {
    if let Some(project_config_path) = get_project_config_path() {
        let toml_content = std::fs::read_to_string(&project_config_path).map_err(|e| {
            TreebeardError::Config(format!("Failed to read project config file: {}", e))
        })?;

        let config: Config = toml::from_str(&toml_content).map_err(|e| {
            TreebeardError::Config(format!("Failed to parse project config: {}", e))
        })?;

        return Ok(Some(config));
    }
    Ok(None)
}

fn merge_configs(base: Config, overlay: Config) -> Config {
    Config {
        paths: PathsConfig {
            worktree_dir: overlay.paths.worktree_dir.or(base.paths.worktree_dir),
            mount_dir: overlay.paths.mount_dir.or(base.paths.mount_dir),
            passthrough: overlay.paths.passthrough.or(base.paths.passthrough),
        },
        commit: CommitConfig {
            auto_commit_message: overlay
                .commit
                .auto_commit_message
                .or(base.commit.auto_commit_message),
            squash_commit_message: overlay
                .commit
                .squash_commit_message
                .or(base.commit.squash_commit_message),
        },
        sync: SyncConfig {
            sync_always_skip: overlay.sync.sync_always_skip.or(base.sync.sync_always_skip),
            sync_always_include: overlay
                .sync
                .sync_always_include
                .or(base.sync.sync_always_include),
        },
        cleanup: CleanupConfig {
            on_exit: overlay.cleanup.on_exit,
        },
        auto_commit_timing: AutoCommitTimingConfig {
            auto_commit_debounce_ms: overlay
                .auto_commit_timing
                .auto_commit_debounce_ms
                .or(base.auto_commit_timing.auto_commit_debounce_ms),
        },
        hooks: HooksConfig {
            post_create: overlay.hooks.post_create,
            pre_cleanup: overlay.hooks.pre_cleanup,
            post_cleanup: overlay.hooks.post_cleanup,
            commit_message: overlay.hooks.commit_message.or(base.hooks.commit_message),
        },
        fuse_ttl_secs: overlay.fuse_ttl_secs.or(base.fuse_ttl_secs),
        sandbox: overlay.sandbox,
    }
}

pub fn save_config(config: &Config) -> Result<()> {
    let config_path = get_config_path();
    if let Some(parent) = config_path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| {
            TreebeardError::Config(format!("Failed to create config directory: {}", e))
        })?;
    }
    let toml_str = toml::to_string_pretty(config)
        .map_err(|e| TreebeardError::Config(format!("Failed to serialize config: {}", e)))?;
    std::fs::write(&config_path, toml_str)
        .map_err(|e| TreebeardError::Config(format!("Failed to write config file: {}", e)))?;
    Ok(())
}
