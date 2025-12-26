mod common;

use common::TestConfigContext;
use treebeard::{load_config, save_config};

/// Test that save_config persists sync_always_skip patterns correctly
#[test]
fn test_save_config_persists_sync_always_skip() {
    let _ctx = TestConfigContext::new();

    let mut config = load_config().expect("Failed to load config");
    config
        .sync
        .sync_always_skip
        .push("node_modules/**".to_string());
    config.sync.sync_always_skip.push("build/**".to_string());

    save_config(&config).expect("Failed to save config");

    let reloaded = load_config().expect("Failed to reload config");
    assert_eq!(reloaded.sync.sync_always_skip.len(), 2);
    assert!(reloaded
        .sync
        .sync_always_skip
        .contains(&"node_modules/**".to_string()));
    assert!(reloaded
        .sync
        .sync_always_skip
        .contains(&"build/**".to_string()));
}

/// Test that save_config persists sync_always_include patterns correctly
#[test]
fn test_save_config_persists_sync_always_include() {
    let _ctx = TestConfigContext::new();

    let mut config = load_config().expect("Failed to load config");
    config.sync.sync_always_include.push(".env".to_string());
    config
        .sync
        .sync_always_include
        .push(".env.local".to_string());

    save_config(&config).expect("Failed to save config");

    let reloaded = load_config().expect("Failed to reload config");
    assert_eq!(reloaded.sync.sync_always_include.len(), 2);
    assert!(reloaded
        .sync
        .sync_always_include
        .contains(&".env".to_string()));
    assert!(reloaded
        .sync
        .sync_always_include
        .contains(&".env.local".to_string()));
}

/// Test that save_config preserves other config fields when saving
#[test]
fn test_save_config_preserves_other_fields() {
    let _ctx = TestConfigContext::new();

    let original_config = load_config().expect("Failed to load config");
    let original_on_exit = original_config.cleanup.on_exit;
    let original_auto_commit_message = original_config.commit.auto_commit_message.clone();

    let mut config = original_config;
    config.sync.sync_always_skip.push("test/**".to_string());

    save_config(&config).expect("Failed to save config");

    let reloaded = load_config().expect("Failed to reload config");
    assert_eq!(reloaded.cleanup.on_exit, original_on_exit);
    assert_eq!(
        reloaded.commit.auto_commit_message,
        original_auto_commit_message
    );
    assert_eq!(reloaded.sync.sync_always_skip.len(), 1);
}

/// Test that fuse_ttl_secs can be customized via config
#[test]
fn test_config_custom_fuse_ttl() {
    let _ctx = TestConfigContext::new();

    let mut config = load_config().expect("Failed to load config");
    config.fuse_ttl_secs = 60;

    save_config(&config).expect("Failed to save config");

    let reloaded = load_config().expect("Failed to reload config");
    assert_eq!(
        reloaded.fuse_ttl_secs, 60,
        "fuse_ttl_secs should be persisted correctly"
    );
}

/// Test that fuse_ttl_secs can be parsed from TOML
#[test]
fn test_config_parses_fuse_ttl_from_toml() {
    let config_content = r#"
fuse_ttl_secs = 30

[paths]
worktree_dir = "~/.local/share/treebeard/worktrees"
mount_dir = "~/.local/share/treebeard/mounts"

[cleanup]
on_exit = "squash"

[commit]
auto_commit_message = "treebeard: auto-save"
squash_commit_message = "treebeard: {branch}"

[auto_commit_timing]
auto_commit_debounce_ms = 500

[sync]
sync_always_skip = []
sync_always_include = []
"#;

    let parsed: treebeard::Config =
        toml::from_str(config_content).expect("Failed to parse config TOML");

    assert_eq!(
        parsed.fuse_ttl_secs, 30,
        "fuse_ttl_secs should be parsed from TOML"
    );
}

/// Test that config serialization/deserialization preserves all fields
#[test]
fn test_config_round_trip_with_sync_fields() {
    let _ctx = TestConfigContext::new();

    let mut config = load_config().expect("Failed to load config");
    config.sync.sync_always_skip = vec![
        "node_modules/**".to_string(),
        "dist/**".to_string(),
        ".cache/**".to_string(),
    ];
    config.sync.sync_always_include = vec![".env".to_string(), "config/secrets.yml".to_string()];

    save_config(&config).expect("Failed to save config");

    let reloaded = load_config().expect("Failed to reload config");

    assert_eq!(reloaded.sync.sync_always_skip, config.sync.sync_always_skip);
    assert_eq!(
        reloaded.sync.sync_always_include,
        config.sync.sync_always_include
    );
}
