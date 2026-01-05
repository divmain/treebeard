use crate::error::{Result, TreebeardError};
use crate::git::GitRepo;
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::io::{self, IsTerminal, Write};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum OnExitBehavior {
    Squash,
    Keep,
    #[default]
    Prompt,
}

impl std::fmt::Display for OnExitBehavior {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OnExitBehavior::Squash => write!(f, "squash"),
            OnExitBehavior::Keep => write!(f, "keep"),
            OnExitBehavior::Prompt => write!(f, "prompt"),
        }
    }
}

impl std::str::FromStr for OnExitBehavior {
    type Err = String;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "squash" => Ok(OnExitBehavior::Squash),
            "keep" => Ok(OnExitBehavior::Keep),
            "prompt" => Ok(OnExitBehavior::Prompt),
            _ => Err(format!(
                "Invalid on_exit value '{}'. Must be one of: squash, keep, prompt",
                s
            )),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PathsConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub worktree_dir: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mount_dir: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub passthrough: Option<Vec<String>>,
}

impl PathsConfig {
    pub fn get_worktree_dir(&self) -> String {
        self.worktree_dir
            .clone()
            .unwrap_or_else(default_worktree_dir)
    }

    pub fn get_mount_dir(&self) -> String {
        self.mount_dir.clone().unwrap_or_else(default_mount_dir)
    }

    pub fn get_passthrough(&self) -> Vec<String> {
        self.passthrough.clone().unwrap_or_else(default_passthrough)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CommitConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auto_commit_message: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub squash_commit_message: Option<String>,
}

impl CommitConfig {
    pub fn get_auto_commit_message(&self) -> String {
        self.auto_commit_message
            .clone()
            .unwrap_or_else(default_auto_commit_message)
    }

    pub fn get_squash_commit_message(&self) -> String {
        self.squash_commit_message
            .clone()
            .unwrap_or_else(default_squash_commit_message)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SyncConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sync_always_skip: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sync_always_include: Option<Vec<String>>,
}

impl SyncConfig {
    pub fn get_sync_always_skip(&self) -> Vec<String> {
        self.sync_always_skip
            .clone()
            .unwrap_or_else(default_sync_always_skip)
    }

    pub fn get_sync_always_include(&self) -> Vec<String> {
        self.sync_always_include
            .clone()
            .unwrap_or_else(default_sync_always_include)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CleanupConfig {
    #[serde(default)]
    pub on_exit: OnExitBehavior,
}

/// Configuration for lifecycle hooks that run at various points during treebeard operations.
///
/// Hooks are shell commands executed via `sh -c`. Template variables are expanded before execution:
/// - `{{branch}}` - Branch name
/// - `{{mount_path}}` - FUSE mount path
/// - `{{worktree_path}}` - Git worktree path
/// - `{{repo_path}}` - Main repository path
/// - `{{diff}}` - (Only for `commit_message` hook) Diff of changes being committed
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct HooksConfig {
    /// Commands to run after worktree and mount are created.
    /// Executed in the mount path directory.
    #[serde(default)]
    pub post_create: Vec<String>,

    /// Commands to run before cleanup starts.
    /// Executed in the mount path directory (if still mounted) or worktree path.
    #[serde(default)]
    pub pre_cleanup: Vec<String>,

    /// Commands to run after successful cleanup.
    /// Executed in the main repository path.
    #[serde(default)]
    pub post_cleanup: Vec<String>,

    /// Command to generate commit messages for auto-commits.
    /// If defined, stdout of this command is used as the commit message.
    /// The `{{diff}}` template variable contains the diff of changes.
    #[serde(default)]
    pub commit_message: Option<String>,
}

const MIN_DEBOUNCE_MS: u64 = 50;
const MAX_DEBOUNCE_MS: u64 = 60000;
const DEFAULT_DEBOUNCE_MS: u64 = 5000;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AutoCommitTimingConfig {
    #[serde(default)]
    pub auto_commit_debounce_ms: Option<u64>,
}

impl AutoCommitTimingConfig {
    pub fn get_debounce_ms(&self) -> u64 {
        self.auto_commit_debounce_ms.unwrap_or(DEFAULT_DEBOUNCE_MS)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Config {
    #[serde(default)]
    pub paths: PathsConfig,
    #[serde(default)]
    pub commit: CommitConfig,
    #[serde(default)]
    pub sync: SyncConfig,
    #[serde(default)]
    pub cleanup: CleanupConfig,
    #[serde(default)]
    pub auto_commit_timing: AutoCommitTimingConfig,
    #[serde(default)]
    pub hooks: HooksConfig,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fuse_ttl_secs: Option<u64>,
    #[serde(default)]
    pub sandbox: SandboxConfig,
}

impl Config {
    pub fn get_fuse_ttl_secs(&self) -> u64 {
        self.fuse_ttl_secs.unwrap_or_else(default_fuse_ttl_secs)
    }
}

fn default_worktree_dir() -> String {
    "~/.local/share/treebeard/worktrees".to_string()
}

fn default_mount_dir() -> String {
    "~/.local/share/treebeard/mounts".to_string()
}

fn default_passthrough() -> Vec<String> {
    vec![]
}

fn default_auto_commit_message() -> String {
    "treebeard: auto-save".to_string()
}

fn default_squash_commit_message() -> String {
    "treebeard: {branch}".to_string()
}

fn default_sync_always_skip() -> Vec<String> {
    vec![]
}

fn default_sync_always_include() -> Vec<String> {
    vec![]
}

fn default_fuse_ttl_secs() -> u64 {
    1
}

pub fn validate_config(config: &Config) -> Result<()> {
    let debounce_ms = config.auto_commit_timing.get_debounce_ms();

    if debounce_ms < MIN_DEBOUNCE_MS {
        eprintln!(
            "Warning: auto_commit_debounce_ms ({}) is below recommended minimum of {}ms. \
             This may cause excessive commits on every change.",
            debounce_ms, MIN_DEBOUNCE_MS
        );
    }

    if debounce_ms > MAX_DEBOUNCE_MS {
        eprintln!(
            "Warning: auto_commit_debounce_ms ({}) is above recommended maximum of {}ms. \
             Auto-commit will be significantly delayed.",
            debounce_ms, MAX_DEBOUNCE_MS
        );
    }

    Ok(())
}

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

pub fn get_config_dir() -> Result<PathBuf> {
    if let Ok(config_dir) = std::env::var("TREEBEARD_CONFIG_DIR") {
        return Ok(PathBuf::from(config_dir));
    }

    let project_dirs = ProjectDirs::from("com", "treebeard", "treebeard").ok_or_else(|| {
        TreebeardError::Config("Could not determine config directory".to_string())
    })?;

    Ok(project_dirs.config_dir().to_path_buf())
}

pub fn load_config() -> Result<Config> {
    let config_dir = get_config_dir()?;
    let config_path = config_dir.join("config.toml");

    let mut config = if !config_path.exists() {
        // Skip prompt and auto-create in these contexts:
        // - TREEBEARD_TEST_MODE is set (automated tests)
        // - TREEBEARD_CONFIG_DIR is set (explicit config path, likely programmatic use)
        // - Non-interactive terminal (piped input)
        let is_test_mode = std::env::var("TREEBEARD_TEST_MODE").is_ok();
        let is_explicit_config_dir = std::env::var("TREEBEARD_CONFIG_DIR").is_ok();
        let is_non_interactive = !std::io::stdin().is_terminal();

        let should_create = if is_test_mode || is_explicit_config_dir || is_non_interactive {
            true
        } else {
            println!("No config file found at {}", config_path.display());

            let stdin = io::stdin();
            let mut stdout = io::stdout();

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
        // Project-level sandbox config completely replaces global config
        sandbox: overlay.sandbox,
    }
}

pub fn get_worktree_dir() -> Result<PathBuf> {
    if let Ok(env_dir) = std::env::var("TREEBEARD_DATA_DIR") {
        return Ok(PathBuf::from(env_dir).join("worktrees"));
    }
    let config = load_config()?;
    expand_path(&config.paths.get_worktree_dir())
}

pub fn get_mount_dir() -> Result<PathBuf> {
    if let Ok(env_dir) = std::env::var("TREEBEARD_DATA_DIR") {
        return Ok(PathBuf::from(env_dir).join("mounts"));
    }
    let config = load_config()?;
    expand_path(&config.paths.get_mount_dir())
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

/// Expands `~` to the home directory without validation.
///
/// Unlike `expand_path()`, this function does not enforce that paths stay within
/// the home directory. This is needed for sandbox configuration where users may
/// want to deny access to paths outside their home directory (e.g., `/etc`).
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

/// macOS version information.
///
/// Returns `None` if the version cannot be determined or if not running on macOS.
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

/// Network access mode for sandboxed subprocesses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum NetworkMode {
    /// No network restrictions (default)
    #[default]
    Allow,
    /// Only localhost connections plus allow_hosts
    Localhost,
    /// Block all network except allow_hosts
    Deny,
}

impl std::fmt::Display for NetworkMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            NetworkMode::Allow => write!(f, "allow"),
            NetworkMode::Localhost => write!(f, "localhost"),
            NetworkMode::Deny => write!(f, "deny"),
        }
    }
}

/// Network configuration for sandbox.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxNetworkConfig {
    /// Network access mode
    #[serde(default)]
    pub mode: NetworkMode,

    /// Hosts to allow when mode is "localhost" or "deny"
    #[serde(default)]
    pub allow_hosts: Vec<String>,
}

impl Default for SandboxNetworkConfig {
    fn default() -> Self {
        Self {
            mode: NetworkMode::Allow,
            allow_hosts: Vec::new(),
        }
    }
}

/// Configuration for sandbox-exec subprocess isolation (macOS only).
///
/// When enabled, subprocesses spawned by treebeard run inside a macOS sandbox
/// with restricted filesystem and network access.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxConfig {
    /// Master switch for sandboxing (default: true on macOS)
    #[serde(default = "default_sandbox_enabled")]
    pub enabled: bool,

    /// Paths to deny reading (sensitive data).
    /// These paths are blocked from read access by sandboxed subprocesses.
    /// Supports ~ expansion.
    #[serde(default = "default_deny_read")]
    pub deny_read: Vec<String>,

    /// Additional paths to allow writing (beyond mount path and /tmp).
    /// Supports ~ expansion.
    #[serde(default)]
    pub allow_write: Vec<String>,

    /// Network configuration
    #[serde(default)]
    pub network: SandboxNetworkConfig,
}

fn default_sandbox_enabled() -> bool {
    cfg!(target_os = "macos")
}

fn default_deny_read() -> Vec<String> {
    vec![
        "~/.ssh".to_string(),
        "~/.aws".to_string(),
        "~/.gnupg".to_string(),
        "~/.config/gh".to_string(),
        "~/Documents".to_string(),
        "~/Pictures".to_string(),
        "~/Desktop".to_string(),
    ]
}

impl Default for SandboxConfig {
    fn default() -> Self {
        Self {
            enabled: default_sandbox_enabled(),
            deny_read: default_deny_read(),
            allow_write: Vec::new(),
            network: SandboxNetworkConfig::default(),
        }
    }
}
