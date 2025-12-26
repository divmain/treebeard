mod common;

use common::create_test_repo;
use treebeard::load_config;

#[test]
fn test_config_default_values() {
    let _temp_dir = create_test_repo().0;

    let config = load_config().expect("Failed to load config");

    assert!(
        !config.paths.worktree_dir.is_empty(),
        "worktree_dir should have a default"
    );
    assert!(
        !config.paths.mount_dir.is_empty(),
        "mount_dir should have a default"
    );

    assert!(
        !config.commit.auto_commit_message.is_empty(),
        "auto_commit_message should have a default"
    );
    assert!(
        !config.commit.squash_commit_message.is_empty(),
        "squash_commit_message should have a default"
    );
    assert!(
        config.auto_commit_timing.get_debounce_ms() > 0,
        "auto_commit_debounce_ms should be positive"
    );
    assert_eq!(
        config.auto_commit_timing.get_debounce_ms(),
        5000,
        "auto_commit_debounce_ms should default to 5000ms"
    );
    assert!(
        config.fuse_ttl_secs > 0,
        "fuse_ttl_secs should be positive and default to 1"
    );
    assert_eq!(
        config.fuse_ttl_secs, 1,
        "fuse_ttl_secs should default to 1 second"
    );
}

#[test]
fn test_squash_commit_message_branch_placeholder() {
    let _temp_dir = create_test_repo().0;

    let config = load_config().expect("Failed to load config");

    let message_with_branch = config
        .commit
        .squash_commit_message
        .replace("{branch}", "feature-test");

    assert!(
        message_with_branch.contains("feature-test"),
        "{{branch}} should be replaced in squash_commit_message"
    );
}
