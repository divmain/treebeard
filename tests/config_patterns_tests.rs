mod common;

use common::TestConfigContext;
use std::fs;
use std::path::Path;
use tempfile::TempDir;
use treebeard::load_config;

/// Test that glob pattern matching works correctly with ./ prefix stripping.
/// Exact-match patterns like ".env" need prefix stripping to match paths
/// that include "./" (e.g., "./.env" in the inode table).
#[test]
fn test_glob_pattern_matching_with_dot_prefix() {
    let patterns: Vec<glob::Pattern> = vec![
        glob::Pattern::new(".env").unwrap(),
        glob::Pattern::new(".env.local").unwrap(),
        glob::Pattern::new("*.ignore").unwrap(),
        glob::Pattern::new("**/.env").unwrap(),
    ];

    let test_cases = vec![
        ("./.env", true),
        ("./.env.local", true),
        ("./test.ignore", true),
        ("./subdir/.env", true),
        ("./.gitignore", false),
        ("./README.md", false),
    ];

    for (path, should_match) in test_cases {
        let relative_path_str = path.strip_prefix("./").unwrap_or(path);

        let is_match = patterns.iter().any(|p| p.matches(relative_path_str));

        assert_eq!(
            is_match,
            should_match,
            "Path '{}' (stripped: '{}') should {} match patterns",
            path,
            relative_path_str,
            if should_match { "" } else { "NOT" }
        );
    }
}

/// Regression test: exact-match patterns fail WITHOUT ./ prefix stripping.
/// This verifies the stripping logic is necessary for correct matching.
#[test]
fn test_glob_pattern_exact_match_bug_without_fix() {
    let pattern = glob::Pattern::new(".env").unwrap();

    assert!(
        !pattern.matches("./.env"),
        "Bug confirmation: '.env' pattern should NOT match './.env' directly"
    );

    let path = "./.env";
    let stripped = path.strip_prefix("./").unwrap_or(path);
    assert!(
        pattern.matches(stripped),
        "After stripping './', '.env' pattern should match"
    );
}

/// Test that wildcard patterns work with both prefixed and non-prefixed paths.
#[test]
fn test_glob_pattern_wildcard_flexibility() {
    let pattern = glob::Pattern::new("*.ignore").unwrap();

    assert!(
        pattern.matches("test.ignore"),
        "'*.ignore' should match 'test.ignore'"
    );
    assert!(
        pattern.matches("./test.ignore"),
        "'*.ignore' matches './test.ignore' because * is flexible"
    );
}

/// Test that glob patterns from sync_always_skip work correctly
#[test]
fn test_sync_always_skip_glob_matching() {
    let patterns: Vec<String> = vec![
        "node_modules/**".to_string(),
        ".env".to_string(),
        "build/**".to_string(),
    ];

    fn path_matches(path: &Path, patterns: &[String]) -> bool {
        let path_str = path.to_string_lossy();
        for pattern_str in patterns {
            if let Ok(pattern) = glob::Pattern::new(pattern_str) {
                if pattern.matches(&path_str) {
                    return true;
                }
            }
        }
        false
    }

    assert!(
        path_matches(Path::new("node_modules/lodash/index.js"), &patterns),
        "node_modules/** should match nested files"
    );
    assert!(
        path_matches(Path::new("build/output.js"), &patterns),
        "build/** should match files in build"
    );
    assert!(
        path_matches(Path::new(".env"), &patterns),
        ".env should match exact file"
    );

    assert!(
        !path_matches(Path::new("src/index.js"), &patterns),
        "src/index.js should not match any pattern"
    );
    assert!(
        !path_matches(Path::new(".env.local"), &patterns),
        ".env.local should not match .env exactly"
    );
}

/// Test that config with sync patterns can be parsed from TOML
#[test]
fn test_config_parses_sync_patterns_from_toml() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let config_dir = temp_dir.path().join(".config/treebeard");
    fs::create_dir_all(&config_dir).expect("Failed to create config dir");

    let config_content = r#"
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
sync_always_skip = ["node_modules/**", "build/**", ".cache/**"]
sync_always_include = [".env", ".env.local"]
"#;

    let config_path = config_dir.join("config.toml");
    fs::write(&config_path, config_content).expect("Failed to write config");

    let parsed: treebeard::Config =
        toml::from_str(config_content).expect("Failed to parse config TOML");

    assert_eq!(parsed.sync.get_sync_always_skip().len(), 3);
    assert!(parsed
        .sync
        .get_sync_always_skip()
        .contains(&"node_modules/**".to_string()));
    assert!(parsed
        .sync
        .get_sync_always_skip()
        .contains(&"build/**".to_string()));
    assert!(parsed
        .sync
        .get_sync_always_skip()
        .contains(&".cache/**".to_string()));

    assert_eq!(parsed.sync.get_sync_always_include().len(), 2);
    assert!(parsed
        .sync
        .get_sync_always_include()
        .contains(&".env".to_string()));
    assert!(parsed
        .sync
        .get_sync_always_include()
        .contains(&".env.local".to_string()));
}

/// Test that sync_always_skip field exists and defaults to empty
#[test]
fn test_config_sync_always_skip_default() {
    let _ctx = TestConfigContext::new();

    let config = load_config().expect("Failed to load config");

    assert!(
        config.sync.get_sync_always_skip().is_empty(),
        "sync_always_skip should default to empty"
    );
}

/// Test that sync_always_include field exists and defaults to empty
#[test]
fn test_config_sync_always_include_default() {
    let _ctx = TestConfigContext::new();

    let config = load_config().expect("Failed to load config");

    assert!(
        config.sync.get_sync_always_include().is_empty(),
        "sync_always_include should default to empty"
    );
}
