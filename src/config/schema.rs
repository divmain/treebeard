use crate::error::Result;
use serde::{Deserialize, Serialize};

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
            .unwrap_or_else(super::default_worktree_dir)
    }

    pub fn get_mount_dir(&self) -> String {
        self.mount_dir
            .clone()
            .unwrap_or_else(super::default_mount_dir)
    }

    pub fn get_passthrough(&self) -> Vec<String> {
        self.passthrough
            .clone()
            .unwrap_or_else(super::default_passthrough)
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
            .unwrap_or_else(super::default_auto_commit_message)
    }

    pub fn get_squash_commit_message(&self) -> String {
        self.squash_commit_message
            .clone()
            .unwrap_or_else(super::default_squash_commit_message)
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
            .unwrap_or_else(super::default_sync_always_skip)
    }

    pub fn get_sync_always_include(&self) -> Vec<String> {
        self.sync_always_include
            .clone()
            .unwrap_or_else(super::default_sync_always_include)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CleanupConfig {
    #[serde(default)]
    pub on_exit: OnExitBehavior,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct HooksConfig {
    #[serde(default)]
    pub post_create: Vec<String>,
    #[serde(default)]
    pub pre_cleanup: Vec<String>,
    #[serde(default)]
    pub post_cleanup: Vec<String>,
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
        self.fuse_ttl_secs
            .unwrap_or_else(super::default_fuse_ttl_secs)
    }
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum NetworkMode {
    #[default]
    Allow,
    Localhost,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxNetworkConfig {
    #[serde(default)]
    pub mode: NetworkMode,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxConfig {
    #[serde(default = "default_sandbox_enabled")]
    pub enabled: bool,
    #[serde(default = "default_deny_read")]
    pub deny_read: Vec<String>,
    #[serde(default)]
    pub allow_write: Vec<String>,
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
