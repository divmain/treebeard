use crate::error::{Result, TreebeardError};
use crate::git::GitRepo;
use directories::ProjectDirs;
use std::path::{Path, PathBuf};

pub fn expand_path(path: &str) -> Result<PathBuf> {
    let path = path.trim();
    let home = std::env::var("HOME")
        .map_err(|_| TreebeardError::Config("HOME environment variable not set".to_string()))?;

    let (expanded_path, is_home_relative) = if let Some(rest) = path.strip_prefix("~/") {
        (PathBuf::from(&home).join(rest), true)
    } else {
        (PathBuf::from(path), false)
    };

    let normalized = normalize_path(&expanded_path)?;

    if is_home_relative {
        if !normalized.starts_with(&home) {
            return Err(TreebeardError::Config(format!(
                "Invalid path '{}': Configuration paths starting with ~ must stay within home directory ({})",
                path,
                home
            )));
        }
    } else if !normalized.starts_with(&home) {
        return Err(TreebeardError::Config(format!(
            "Invalid path '{}': Configuration paths must be within the home directory ({})",
            path, home
        )));
    }

    Ok(normalized)
}

fn normalize_path(path: &Path) -> Result<PathBuf> {
    let components: Vec<std::path::Component<'_>> = path.components().collect();

    let mut result = PathBuf::new();
    for component in components {
        match component {
            std::path::Component::CurDir => {
                return Err(TreebeardError::Config(
                    "Invalid path: contains '.' component".to_string(),
                ));
            }
            std::path::Component::ParentDir => {
                if !result.pop() {
                    return Err(TreebeardError::Config(
                        "Invalid path: attempts to escape root directory with ../".to_string(),
                    ));
                }
            }
            _ => {
                result.push(component);
            }
        }
    }

    Ok(result)
}

pub fn expand_tilde(path: &str) -> PathBuf {
    let path = path.trim();
    if let Some(rest) = path.strip_prefix("~/") {
        if let Ok(home) = std::env::var("HOME") {
            return PathBuf::from(home).join(rest);
        }
    } else if path == "~" {
        if let Ok(home) = std::env::var("HOME") {
            return PathBuf::from(home);
        }
    }
    PathBuf::from(path)
}

pub fn get_config_dir() -> Result<PathBuf> {
    if let Ok(config_dir) = std::env::var("TREEBEARD_CONFIG_DIR") {
        return Ok(PathBuf::from(config_dir));
    }

    let project_dirs = ProjectDirs::from("com", "treebeard", "treebeard").ok_or_else(|| {
        TreebeardError::Config("Could not determine config directory".to_string())
    })?;

    Ok(project_dirs.config_dir().to_path_buf())
}

pub fn get_config_path() -> PathBuf {
    if let Ok(config_dir) = get_config_dir() {
        return config_dir.join("config.toml");
    }
    let project_dirs = ProjectDirs::from("com", "treebeard", "treebeard")
        .expect("Could not determine config directory");
    project_dirs.config_dir().join("config.toml")
}

pub fn get_project_config_path() -> Option<PathBuf> {
    let current_dir = std::env::current_dir().ok()?;
    let repo = GitRepo::from_path(&current_dir).ok()?;
    let project_config_path = repo.workdir().join(".treebeard.toml");
    if project_config_path.exists() {
        Some(project_config_path)
    } else {
        None
    }
}

pub fn get_worktree_dir() -> Result<PathBuf> {
    if let Ok(env_dir) = std::env::var("TREEBEARD_DATA_DIR") {
        return Ok(PathBuf::from(env_dir).join("worktrees"));
    }
    let config = super::load_config()?;
    expand_path(&config.paths.get_worktree_dir())
}

pub fn get_mount_dir() -> Result<PathBuf> {
    if let Ok(env_dir) = std::env::var("TREEBEARD_DATA_DIR") {
        return Ok(PathBuf::from(env_dir).join("mounts"));
    }
    let config = super::load_config()?;
    expand_path(&config.paths.get_mount_dir())
}

#[cfg(target_os = "macos")]
pub fn get_macos_version() -> Option<(u32, u32)> {
    let output = std::process::Command::new("sw_vers")
        .args(["-productVersion"])
        .output()
        .ok()?;

    let version_str = String::from_utf8_lossy(&output.stdout).trim().to_string();

    let parts: Vec<&str> = version_str.split('.').collect();

    let major = parts.first()?.parse::<u32>().ok()?;
    let minor = parts
        .get(1)
        .and_then(|s| s.parse::<u32>().ok())
        .unwrap_or(0);

    Some((major, minor))
}
