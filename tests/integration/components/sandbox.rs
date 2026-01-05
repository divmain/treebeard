use crate::shared::common::create_test_repo;

use std::path::PathBuf;
use treebeard::{expand_tilde, load_config, NetworkMode, SandboxConfig, SandboxNetworkConfig};

#[test]
fn test_expand_tilde_with_home_path() {
    let expanded = expand_tilde("~/.ssh");
    assert!(!expanded.to_string_lossy().contains('~'));
    assert!(expanded.to_string_lossy().contains(".ssh"));
}

#[test]
fn test_expand_tilde_just_tilde() {
    let expanded = expand_tilde("~");
    assert!(!expanded.to_string_lossy().contains('~'));
    // Should return home directory
    if let Ok(home) = std::env::var("HOME") {
        assert_eq!(expanded, PathBuf::from(home));
    }
}

#[test]
fn test_expand_tilde_absolute_path() {
    let expanded = expand_tilde("/etc/passwd");
    assert_eq!(expanded, PathBuf::from("/etc/passwd"));
}

#[test]
fn test_expand_tilde_relative_path() {
    let expanded = expand_tilde("foo/bar");
    assert_eq!(expanded, PathBuf::from("foo/bar"));
}

#[test]
fn test_sandbox_config_default_enabled_on_macos() {
    let config = SandboxConfig::default();
    #[cfg(target_os = "macos")]
    assert!(
        config.enabled,
        "Sandbox should be enabled by default on macOS"
    );
    #[cfg(not(target_os = "macos"))]
    assert!(
        !config.enabled,
        "Sandbox should be disabled by default on non-macOS"
    );
}

#[test]
fn test_sandbox_config_default_deny_read() {
    let config = SandboxConfig::default();
    assert!(
        !config.deny_read.is_empty(),
        "deny_read should have defaults"
    );
    assert!(
        config.deny_read.iter().any(|p| p.contains(".ssh")),
        "deny_read should include ~/.ssh"
    );
    assert!(
        config.deny_read.iter().any(|p| p.contains(".aws")),
        "deny_read should include ~/.aws"
    );
}

#[test]
fn test_sandbox_config_default_allow_write_empty() {
    let config = SandboxConfig::default();
    assert!(
        config.allow_write.is_empty(),
        "allow_write should be empty by default"
    );
}

#[test]
fn test_sandbox_network_config_default_mode_allow() {
    let config = SandboxNetworkConfig::default();
    assert_eq!(
        config.mode,
        NetworkMode::Allow,
        "Network mode should default to Allow"
    );
}

#[test]
fn test_sandbox_network_config_default_allow_hosts_empty() {
    let config = SandboxNetworkConfig::default();
    assert!(
        config.allow_hosts.is_empty(),
        "allow_hosts should be empty by default"
    );
}

#[test]
fn test_config_includes_sandbox_defaults() {
    let _temp_dir = create_test_repo().0;
    let config = load_config().expect("Failed to load config");

    // Sandbox config should have defaults
    #[cfg(target_os = "macos")]
    assert!(config.sandbox.enabled);
    assert!(!config.sandbox.deny_read.is_empty());
    assert!(config.sandbox.allow_write.is_empty());
    assert_eq!(config.sandbox.network.mode, NetworkMode::Allow);
}

#[test]
fn test_sandbox_config_parses_from_toml() {
    let toml_str = r#"
[sandbox]
enabled = false
deny_read = ["~/.ssh", "~/custom-secret"]
allow_write = ["~/.cache"]

[sandbox.network]
mode = "deny"
allow_hosts = ["api.example.com", "192.168.1.1"]
"#;

    let config: treebeard::Config = toml::from_str(toml_str).expect("Failed to parse TOML");

    assert!(!config.sandbox.enabled);
    assert_eq!(config.sandbox.deny_read.len(), 2);
    assert!(config.sandbox.deny_read.contains(&"~/.ssh".to_string()));
    assert!(config
        .sandbox
        .deny_read
        .contains(&"~/custom-secret".to_string()));
    assert_eq!(config.sandbox.allow_write.len(), 1);
    assert!(config.sandbox.allow_write.contains(&"~/.cache".to_string()));
    assert_eq!(config.sandbox.network.mode, NetworkMode::Deny);
    assert_eq!(config.sandbox.network.allow_hosts.len(), 2);
}

#[test]
fn test_sandbox_network_mode_localhost_parses() {
    let toml_str = r#"
[sandbox.network]
mode = "localhost"
allow_hosts = ["custom-host.local"]
"#;

    let config: treebeard::Config = toml::from_str(toml_str).expect("Failed to parse TOML");

    assert_eq!(config.sandbox.network.mode, NetworkMode::Localhost);
    assert!(config
        .sandbox
        .network
        .allow_hosts
        .contains(&"custom-host.local".to_string()));
}

#[test]
fn test_network_mode_display() {
    assert_eq!(format!("{}", NetworkMode::Allow), "allow");
    assert_eq!(format!("{}", NetworkMode::Localhost), "localhost");
    assert_eq!(format!("{}", NetworkMode::Deny), "deny");
}
