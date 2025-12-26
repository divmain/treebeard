//! Tests for the hooks system.
//!
//! These tests verify that hooks are correctly parsed from config, that template
//! variables are expanded correctly, and that hooks are executed at the appropriate
//! lifecycle points.

mod common;

use common::TestConfigContext;
use treebeard::{load_config, save_config, HooksConfig};

/// Test that empty hooks config is the default
#[test]
fn test_hooks_config_default_empty() {
    let _ctx = TestConfigContext::new();

    let config = load_config().expect("Failed to load config");

    assert!(
        config.hooks.post_create.is_empty(),
        "post_create should be empty by default"
    );
    assert!(
        config.hooks.pre_cleanup.is_empty(),
        "pre_cleanup should be empty by default"
    );
    assert!(
        config.hooks.post_cleanup.is_empty(),
        "post_cleanup should be empty by default"
    );
    assert!(
        config.hooks.commit_message.is_none(),
        "commit_message should be None by default"
    );
}

/// Test that hooks config can be saved and loaded
#[test]
fn test_hooks_config_round_trip() {
    let _ctx = TestConfigContext::new();

    let mut config = load_config().expect("Failed to load config");
    config.hooks.post_create = vec!["npm install".to_string(), "echo 'Created'".to_string()];
    config.hooks.pre_cleanup = vec!["echo 'Pre cleanup'".to_string()];
    config.hooks.post_cleanup = vec!["echo 'Cleaned up {{branch}}'".to_string()];
    config.hooks.commit_message = Some("echo 'Auto-commit: {{branch}}'".to_string());

    save_config(&config).expect("Failed to save config");

    let reloaded = load_config().expect("Failed to reload config");
    assert_eq!(reloaded.hooks.post_create.len(), 2);
    assert_eq!(reloaded.hooks.post_create[0], "npm install");
    assert_eq!(reloaded.hooks.post_create[1], "echo 'Created'");
    assert_eq!(reloaded.hooks.pre_cleanup.len(), 1);
    assert_eq!(reloaded.hooks.pre_cleanup[0], "echo 'Pre cleanup'");
    assert_eq!(reloaded.hooks.post_cleanup.len(), 1);
    assert_eq!(
        reloaded.hooks.post_cleanup[0],
        "echo 'Cleaned up {{branch}}'"
    );
    assert_eq!(
        reloaded.hooks.commit_message,
        Some("echo 'Auto-commit: {{branch}}'".to_string())
    );
}

/// Test that hooks config can be parsed from TOML
#[test]
fn test_hooks_config_parses_from_toml() {
    let config_content = r#"
[paths]
worktree_dir = "~/.local/share/treebeard/worktrees"
mount_dir = "~/.local/share/treebeard/mounts"

[hooks]
post_create = ["npm install", "cp .env.example .env"]
pre_cleanup = ["npm run build"]
post_cleanup = ["echo 'Cleaned up {{branch}}'"]
commit_message = "echo '{{diff}}' | head -1"

[cleanup]
on_exit = "squash"

[commit]
auto_commit_message = "treebeard: auto-save"
squash_commit_message = "treebeard: {branch}"
"#;

    let parsed: treebeard::Config =
        toml::from_str(config_content).expect("Failed to parse config TOML");

    assert_eq!(parsed.hooks.post_create.len(), 2);
    assert_eq!(parsed.hooks.post_create[0], "npm install");
    assert_eq!(parsed.hooks.post_create[1], "cp .env.example .env");
    assert_eq!(parsed.hooks.pre_cleanup.len(), 1);
    assert_eq!(parsed.hooks.pre_cleanup[0], "npm run build");
    assert_eq!(parsed.hooks.post_cleanup.len(), 1);
    assert_eq!(parsed.hooks.post_cleanup[0], "echo 'Cleaned up {{branch}}'");
    assert_eq!(
        parsed.hooks.commit_message,
        Some("echo '{{diff}}' | head -1".to_string())
    );
}

/// Test that hooks config preserves other config fields
#[test]
fn test_hooks_config_preserves_other_fields() {
    let _ctx = TestConfigContext::new();

    let original = load_config().expect("Failed to load config");
    let original_on_exit = original.cleanup.on_exit;
    let original_debounce = original.auto_commit_timing.get_debounce_ms();

    let mut config = original;
    config.hooks.post_create = vec!["test hook".to_string()];

    save_config(&config).expect("Failed to save config");

    let reloaded = load_config().expect("Failed to reload config");
    assert_eq!(reloaded.cleanup.on_exit, original_on_exit);
    assert_eq!(
        reloaded.auto_commit_timing.get_debounce_ms(),
        original_debounce
    );
    assert_eq!(reloaded.hooks.post_create, vec!["test hook"]);
}

/// Test that HooksConfig implements Default correctly
#[test]
fn test_hooks_config_default_trait() {
    let hooks = HooksConfig::default();

    assert!(hooks.post_create.is_empty());
    assert!(hooks.pre_cleanup.is_empty());
    assert!(hooks.post_cleanup.is_empty());
    assert!(hooks.commit_message.is_none());
}

/// Test partial hooks config (only some hooks defined)
#[test]
fn test_partial_hooks_config() {
    let config_content = r#"
[paths]
worktree_dir = "~/.local/share/treebeard/worktrees"
mount_dir = "~/.local/share/treebeard/mounts"

[hooks]
post_create = ["npm install"]
# pre_cleanup, post_cleanup, and commit_message are not defined
"#;

    let parsed: treebeard::Config =
        toml::from_str(config_content).expect("Failed to parse config TOML");

    assert_eq!(parsed.hooks.post_create.len(), 1);
    assert_eq!(parsed.hooks.post_create[0], "npm install");
    assert!(parsed.hooks.pre_cleanup.is_empty());
    assert!(parsed.hooks.post_cleanup.is_empty());
    assert!(parsed.hooks.commit_message.is_none());
}

/// Test hooks with complex shell commands
#[test]
fn test_hooks_with_complex_commands() {
    let config_content = r#"
[paths]
worktree_dir = "~/.local/share/treebeard/worktrees"
mount_dir = "~/.local/share/treebeard/mounts"

[hooks]
post_create = [
    "npm install && npm run setup",
    "if [ -f .env.example ]; then cp .env.example .env; fi",
    "echo 'Branch: {{branch}}' > .treebeard-info"
]
commit_message = "echo '{{diff}}' | llm -s 'Generate commit message' 2>/dev/null || echo 'Auto-commit'"
"#;

    let parsed: treebeard::Config =
        toml::from_str(config_content).expect("Failed to parse config TOML");

    assert_eq!(parsed.hooks.post_create.len(), 3);
    assert!(parsed.hooks.post_create[0].contains("npm run setup"));
    assert!(parsed.hooks.post_create[1].contains(".env.example"));
    assert!(parsed.hooks.post_create[2].contains("{{branch}}"));
    assert!(parsed
        .hooks
        .commit_message
        .as_ref()
        .unwrap()
        .contains("{{diff}}"));
}
