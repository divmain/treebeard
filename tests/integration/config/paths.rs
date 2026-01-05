use crate::shared::common::create_test_repo;

use std::fs;
use tempfile::TempDir;
use treebeard::{expand_path, get_config_path, get_mount_dir, get_worktree_dir, load_config};

#[test]
fn test_get_config_path() {
    let config_path = get_config_path();

    assert!(
        config_path.ends_with("config.toml"),
        "Config path should end with config.toml"
    );
    assert!(
        config_path.to_string_lossy().contains("treebeard"),
        "Config path should contain treebeard"
    );
}

#[test]
fn test_get_worktree_dir() {
    let worktree_dir = get_worktree_dir().expect("Failed to get worktree dir");

    assert!(
        worktree_dir.to_string_lossy().contains("treebeard"),
        "Worktree dir should contain treebeard"
    );
    assert!(
        worktree_dir.to_string_lossy().contains(".local")
            || worktree_dir.to_string_lossy().contains(".cache")
            || worktree_dir.to_string_lossy().contains(".var"),
        "Worktree dir should be in a data/cache directory"
    );
}

#[test]
fn test_get_mount_dir() {
    let mount_dir = get_mount_dir().expect("Failed to get mount dir");

    assert!(
        mount_dir.to_string_lossy().contains("treebeard"),
        "Mount dir should contain treebeard"
    );
    assert!(
        mount_dir.to_string_lossy().contains(".local")
            || mount_dir.to_string_lossy().contains(".cache")
            || mount_dir.to_string_lossy().contains(".var"),
        "Mount dir should be in a data/cache directory"
    );
}

#[test]
fn test_config_shell_expansion() {
    let _temp_dir = create_test_repo().0;

    let config = load_config().expect("Failed to load config");

    let worktree_dir = get_worktree_dir().expect("Failed to get worktree dir");

    let raw_worktree_dir = &config.paths.get_worktree_dir();

    assert!(
        raw_worktree_dir.contains("~"),
        "Raw config should contain ~"
    );
    assert!(
        worktree_dir.is_absolute(),
        "Expanded worktree dir should be absolute path"
    );
    assert!(
        !worktree_dir.to_string_lossy().contains("~"),
        "Expanded worktree dir should not contain ~"
    );
}

/// Test that valid home-relative paths are accepted (direct expand_path test)
#[test]
fn test_expand_path_valid_home_relative() {
    let home = std::env::var("HOME").expect("HOME env var should be set");

    let result = expand_path("~/treebeard-test").expect("Should expand valid path");

    assert!(
        result.starts_with(&home),
        "Expanded path should be within home directory"
    );
    assert!(
        !result.to_string_lossy().contains("~"),
        "Expanded path should not contain ~"
    );
}

/// Test that absolute paths outside home are rejected (direct expand_path test)
#[test]
fn test_expand_path_absolute_outside_home_rejected() {
    let result = expand_path("/tmp/treebeard-test");

    assert!(
        result.is_err(),
        "Absolute path outside home should be rejected: {:?}",
        result
    );

    if let Err(e) = result {
        let error_msg = format!("{}", e);
        assert!(
            error_msg.contains("within the home"),
            "Error message should mention home directory constraint: {}",
            error_msg
        );
    }
}

/// Test that paths escaping to /etc are rejected (direct expand_path test)
#[test]
fn test_expand_path_etc_rejected() {
    let result = expand_path("/etc/treebeard");

    assert!(
        result.is_err(),
        "Path escaping to /etc should be rejected: {:?}",
        result
    );
}

/// Test that paths with ../ sequences that escape home are rejected (direct expand_path test)
#[test]
fn test_expand_path_traversal_rejected() {
    let home = std::env::var("HOME").expect("HOME env var should be set");
    let traversal_path = format!("{}/../../etc", home);

    let result = expand_path(&traversal_path);

    assert!(
        result.is_err(),
        "Path with ../ escaping home should be rejected: {:?}",
        result
    );
}

/// Test that valid absolute paths within home are accepted
#[test]
fn test_expand_path_valid_absolute_in_home() {
    let home = std::env::var("HOME").expect("HOME env var should be set");
    let valid_path = format!("{}/treebeard-test", home);

    let result = expand_path(&valid_path).expect("Should accept absolute path within home");

    assert!(
        result.starts_with(&home),
        "Path should be within home directory"
    );
}

/// Test that home-relative paths with internal ../ that stay within home are accepted
#[test]
fn test_expand_path_home_relative_with_internal_dotdot() {
    let home = std::env::var("HOME").expect("HOME env var should be set");

    let result = expand_path("~/Documents/../treebeard-tests")
        .expect("Should accept home-relative path with internal ../");

    assert!(
        result.starts_with(&home),
        "Path should still be within home after canonicalization"
    );
}

#[test]
fn test_missing_config_uses_defaults() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");

    let config_dir = temp_dir.path().join(".config/treebeard");
    fs::create_dir_all(&config_dir).expect("Failed to create config dir");

    // Note: we don't create a config file - we're testing that defaults work
    // if the config file is not present

    let current_dir = std::env::current_dir().unwrap();
    std::env::set_current_dir(temp_dir.path()).unwrap();

    let config = load_config();

    assert!(
        config.is_ok(),
        "Should load config with defaults even if file doesn't exist"
    );
    let config = config.unwrap();
    assert!(
        !config.paths.get_worktree_dir().is_empty(),
        "Should have default worktree_dir"
    );

    std::env::set_current_dir(current_dir).unwrap();
}

/// Test that absolute paths outside home are rejected via config loading
#[test]
fn test_absolute_paths_outside_home_rejected_via_get_worktree_dir() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let config_dir = temp_dir.path().join(".config/treebeard");
    fs::create_dir_all(&config_dir).expect("Failed to create config dir");

    let config_content = r#"
[paths]
worktree_dir = "/tmp/treebeard-test"
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

    let config_path = config_dir.join("config.toml");
    fs::write(&config_path, config_content).expect("Failed to write config");

    let _old_data_dir = std::env::var("TREEBEARD_DATA_DIR").ok();
    let _old_config_dir = std::env::var("TREEBEARD_CONFIG_DIR").ok();

    std::env::set_var("TREEBEARD_CONFIG_DIR", config_dir);
    std::env::remove_var("TREEBEARD_DATA_DIR");

    let result = get_worktree_dir();

    std::env::remove_var("TREEBEARD_CONFIG_DIR");
    std::env::remove_var("TREEBEARD_DATA_DIR");
    if let Some(old_val) = _old_data_dir {
        std::env::set_var("TREEBEARD_DATA_DIR", old_val);
    }
    if let Some(old_val) = _old_config_dir {
        std::env::set_var("TREEBEARD_CONFIG_DIR", old_val);
    }

    assert!(
        result.is_err(),
        "Absolute path outside home should be rejected: {:?}",
        result
    );

    if let Err(e) = result {
        let error_msg = format!("{}", e);
        assert!(
            error_msg.contains("within the home"),
            "Error message should mention home directory constraint: {}",
            error_msg
        );
    }
}

/// Test that absolute paths outside home are rejected via config loading
#[test]
fn test_absolute_paths_outside_home_rejected_via_get_mount_dir() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let config_dir = temp_dir.path().join(".config/treebeard");
    fs::create_dir_all(&config_dir).expect("Failed to create config dir");

    let config_content = r#"
[paths]
worktree_dir = "~/.local/share/treebeard/worktrees"
mount_dir = "/var/tmp/treebeard-mounts"

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

    let config_path = config_dir.join("config.toml");
    fs::write(&config_path, config_content).expect("Failed to write config");

    let _old_data_dir = std::env::var("TREEBEARD_DATA_DIR").ok();
    let _old_config_dir = std::env::var("TREEBEARD_CONFIG_DIR").ok();

    std::env::set_var("TREEBEARD_CONFIG_DIR", config_dir);
    std::env::remove_var("TREEBEARD_DATA_DIR");

    let result = get_mount_dir();

    std::env::remove_var("TREEBEARD_CONFIG_DIR");
    std::env::remove_var("TREEBEARD_DATA_DIR");
    if let Some(old_val) = _old_data_dir {
        std::env::set_var("TREEBEARD_DATA_DIR", old_val);
    }
    if let Some(old_val) = _old_config_dir {
        std::env::set_var("TREEBEARD_CONFIG_DIR", old_val);
    }

    assert!(
        result.is_err(),
        "Absolute path outside home should be rejected: {:?}",
        result
    );
}

/// Test that paths escaping to /etc are rejected (path traversal attack prevention)
#[test]
fn test_etc_path_rejected_via_config() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let config_dir = temp_dir.path().join(".config/treebeard");
    fs::create_dir_all(&config_dir).expect("Failed to create config dir");

    let config_content = r#"
[paths]
worktree_dir = "/etc/treebeard"
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

    let config_path = config_dir.join("config.toml");
    fs::write(&config_path, config_content).expect("Failed to write config");

    let _old_data_dir = std::env::var("TREEBEARD_DATA_DIR").ok();
    let _old_config_dir = std::env::var("TREEBEARD_CONFIG_DIR").ok();

    std::env::set_var("TREEBEARD_CONFIG_DIR", config_dir);
    std::env::remove_var("TREEBEARD_DATA_DIR");

    let result = get_worktree_dir();

    std::env::remove_var("TREEBEARD_CONFIG_DIR");
    std::env::remove_var("TREEBEARD_DATA_DIR");
    if let Some(old_val) = _old_data_dir {
        std::env::set_var("TREEBEARD_DATA_DIR", old_val);
    }
    if let Some(old_val) = _old_config_dir {
        std::env::set_var("TREEBEARD_CONFIG_DIR", old_val);
    }

    assert!(
        result.is_err(),
        "Path escaping to /etc should be rejected: {:?}",
        result
    );
}

/// Test that paths with ../ sequences that escape home are rejected
#[test]
fn test_path_traversal_rejected_via_config() {
    let home = std::env::var("HOME").expect("HOME env var should be set");

    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let config_dir = temp_dir.path().join(".config/treebeard");
    fs::create_dir_all(&config_dir).expect("Failed to create config dir");

    let config_content = format!(
        r#"
[paths]
worktree_dir = "{}/../../etc"
mount_dir = "~/.local/share/treebeard/mounts"

[cleanup]
on_exit = "squash"

[commit]
auto_commit_message = "treebeard: auto-save"
squash_commit_message = "treebeard: {{branch}}

[auto_commit_timing]
auto_commit_debounce_ms = 500

[sync]
sync_always_skip = []
sync_always_include = []
"#,
        home
    );

    let config_path = config_dir.join("config.toml");
    fs::write(&config_path, config_content).expect("Failed to write config");

    let _old_data_dir = std::env::var("TREEBEARD_DATA_DIR").ok();
    let _old_config_dir = std::env::var("TREEBEARD_CONFIG_DIR").ok();

    std::env::set_var("TREEBEARD_CONFIG_DIR", config_dir);
    std::env::remove_var("TREEBEARD_DATA_DIR");

    let result = get_worktree_dir();

    std::env::remove_var("TREEBEARD_CONFIG_DIR");
    std::env::remove_var("TREEBEARD_DATA_DIR");
    if let Some(old_val) = _old_data_dir {
        std::env::set_var("TREEBEARD_DATA_DIR", old_val);
    }
    if let Some(old_val) = _old_config_dir {
        std::env::set_var("TREEBEARD_CONFIG_DIR", old_val);
    }

    assert!(
        result.is_err(),
        "Path with ../ escaping home should be rejected: {:?}",
        result
    );
}
