use crate::shared::common::TestWorkspace;

use std::fs;
use treebeard::load_config;

/// Test that project config at .treebeard.toml in repo root is loaded and takes precedence over user config
#[test]
fn test_project_config_overrides_user_config() {
    let ctx = TestWorkspace::new();
    let repo_path = &ctx.repo_path;

    // Set up user config with some values
    let user_config_content = r#"
[cleanup]
on_exit = "prompt"

[hooks]
post_create = ["echo 'user hook 1'", "echo 'user hook 2'"]

[sync]
sync_always_skip = ["node_modules/**"]
"#;

    let user_config_path = ctx.config_dir.join("config.toml");
    fs::create_dir_all(&ctx.config_dir).expect("Failed to create config dir");
    fs::write(&user_config_path, user_config_content).expect("Failed to write user config");

    // Create project config with overrides
    let project_config_content = r#"
[cleanup]
on_exit = "squash"

[hooks]
post_create = ["echo 'project hook 1'", "echo 'project hook 2'"]

[sync]
sync_always_skip = ["build/**", "dist/**"]

[commit]
auto_commit_message = "project: auto-commit"
"#;

    let project_config_path = repo_path.join(".treebeard.toml");
    fs::create_dir_all(repo_path).expect("Failed to create repo dir");
    fs::write(&project_config_path, project_config_content)
        .expect("Failed to write project config");

    // Switch to repo directory so we're in the git repo
    ctx.switch_to_repo();

    // Load config and verify project config overrides user config
    let config = load_config().expect("Failed to load config");

    assert_eq!(
        config.cleanup.on_exit,
        treebeard::OnExitBehavior::Squash,
        "Project config should override user config for on_exit"
    );

    assert_eq!(
        config.hooks.post_create.len(),
        2,
        "Project config should override user config for post_create hooks"
    );
    assert!(config
        .hooks
        .post_create
        .contains(&"echo 'project hook 1'".to_string()));
    assert!(config
        .hooks
        .post_create
        .contains(&"echo 'project hook 2'".to_string()));

    assert_eq!(
        config.sync.get_sync_always_skip().len(),
        2,
        "Project config should override user config for sync_always_skip"
    );
    assert!(config
        .sync
        .get_sync_always_skip()
        .contains(&"build/**".to_string()));
    assert!(config
        .sync
        .get_sync_always_skip()
        .contains(&"dist/**".to_string()));

    assert_eq!(
        config.commit.get_auto_commit_message(),
        "project: auto-commit",
        "Project config should provide values not set in user config"
    );
}

/// Test that user config is used when project config is not present
#[test]
fn test_user_config_used_when_project_config_missing() {
    let ctx = TestWorkspace::new();

    // Set up user config
    let user_config_content = r#"
[cleanup]
on_exit = "squash"

[hooks]
post_create = ["pnpm install"]
"#;

    let user_config_path = ctx.config_dir.join("config.toml");
    fs::create_dir_all(&ctx.config_dir).expect("Failed to create config dir");
    fs::write(&user_config_path, user_config_content).expect("Failed to write user config");

    // Don't create project config
    ctx.switch_to_repo();

    let config = load_config().expect("Failed to load config");

    assert_eq!(
        config.cleanup.on_exit,
        treebeard::OnExitBehavior::Squash,
        "User config should be used when project config is missing"
    );

    assert_eq!(
        config.hooks.post_create.len(),
        1,
        "User config hooks should be used when project config is missing"
    );
    assert!(config
        .hooks
        .post_create
        .contains(&"pnpm install".to_string()));
}

/// Test that defaults are used when neither user nor project config exists
#[test]
fn test_defaults_used_when_no_config_exists() {
    let ctx = TestWorkspace::new();

    // Don't create any config files
    ctx.switch_to_repo();

    let config = load_config().expect("Failed to load config");

    assert_eq!(
        config.cleanup.on_exit,
        treebeard::OnExitBehavior::Prompt,
        "Default on_exit should be used when no config exists"
    );

    assert!(
        config.hooks.post_create.is_empty(),
        "Default post_create hooks should be empty when no config exists"
    );

    assert!(
        config.sync.get_sync_always_skip().is_empty(),
        "Default sync_always_skip should be empty when no config exists"
    );

    assert!(
        config.sync.get_sync_always_include().is_empty(),
        "Default sync_always_include should be empty when no config exists"
    );
}

/// Test that project config works in subdirectories of a git repo
#[test]
fn test_project_config_found_in_subdirectory() {
    let ctx = TestWorkspace::new();
    let repo_path = &ctx.repo_path;

    // Create project config in repo root
    let project_config_content = r#"
[cleanup]
on_exit = "keep"
"#;

    let project_config_path = repo_path.join(".treebeard.toml");
    fs::write(&project_config_path, project_config_content)
        .expect("Failed to write project config");

    // Create a subdirectory and switch to it
    let subdir = repo_path.join("src");
    fs::create_dir_all(&subdir).expect("Failed to create subdirectory");
    std::env::set_current_dir(&subdir).expect("Failed to change to subdirectory");

    let config = load_config().expect("Failed to load config");

    assert_eq!(
        config.cleanup.on_exit,
        treebeard::OnExitBehavior::Keep,
        "Project config should be found even when in a subdirectory"
    );
}

/// Test that project config is not loaded when not in a git repo
#[test]
fn test_project_config_not_loaded_outside_git_repo() {
    let ctx = TestWorkspace::new();

    // Switch to temp dir (which is not a git repo)
    let temp_dir = ctx.temp_dir.path().to_path_buf();
    std::env::set_current_dir(&temp_dir).expect("Failed to change to temp dir");

    // Create .treebeard.toml in temp dir (should not be loaded as it's not in a git repo)
    let project_config_content = r#"
[cleanup]
on_exit = "squash"
"#;

    let project_config_path = temp_dir.join(".treebeard.toml");
    fs::write(&project_config_path, project_config_content)
        .expect("Failed to write project config");

    // Create user config
    let user_config_content = r#"
[cleanup]
on_exit = "keep"
"#;

    let user_config_path = ctx.config_dir.join("config.toml");
    fs::create_dir_all(&ctx.config_dir).expect("Failed to create config dir");
    fs::write(&user_config_path, user_config_content).expect("Failed to write user config");

    let config = load_config().expect("Failed to load config");

    assert_eq!(
        config.cleanup.on_exit,
        treebeard::OnExitBehavior::Keep,
        "Project config should not be loaded outside a git repo"
    );
}

/// Test merge order: user config -> project config
#[test]
fn test_merge_order_user_then_project() {
    let ctx = TestWorkspace::new();
    let repo_path = &ctx.repo_path;

    // Set up user config
    let user_config_content = r#"
[hooks]
post_create = ["user-hook-1"]

[commit]
auto_commit_message = "user: commit"

[sync]
sync_always_skip = ["user-skip-1"]
sync_always_include = ["user-include-1"]
"#;

    let user_config_path = ctx.config_dir.join("config.toml");
    fs::create_dir_all(&ctx.config_dir).expect("Failed to create config dir");
    fs::write(&user_config_path, user_config_content).expect("Failed to write user config");

    // Set up project config that overrides and adds
    let project_config_content = r#"
[hooks]
post_create = ["project-hook-1"]

[commit]
auto_commit_message = "project: commit"

[sync]
sync_always_skip = ["project-skip-1"]
"#;

    let project_config_path = repo_path.join(".treebeard.toml");
    fs::write(&project_config_path, project_config_content)
        .expect("Failed to write project config");

    ctx.switch_to_repo();

    let config = load_config().expect("Failed to load config");

    // Project config should replace user config values
    assert_eq!(
        config.commit.get_auto_commit_message(),
        "project: commit",
        "Project config should override user config auto_commit_message"
    );

    assert_eq!(
        config.hooks.post_create,
        vec!["project-hook-1".to_string()],
        "Project config should replace user config post_create"
    );

    assert_eq!(
        config.sync.get_sync_always_skip(),
        vec!["project-skip-1".to_string()],
        "Project config should replace user config sync_always_skip"
    );

    // When project config doesn't specify a field, it falls back to user config
    assert_eq!(
        config.sync.get_sync_always_include(),
        vec!["user-include-1".to_string()],
        "sync_always_include should fall back to user config when not in project config"
    );
}

/// Test that project config can set empty vectors to override user config
#[test]
fn test_project_config_can_override_user_vectors_with_empty() {
    let ctx = TestWorkspace::new();

    // User config with values
    let user_config_content = r#"
[hooks]
post_create = ["user-hook-1", "user-hook-2"]

[sync]
sync_always_skip = ["node_modules/**"]
sync_always_include = [".env"]
"#;

    let user_config_path = ctx.config_dir.join("config.toml");
    fs::create_dir_all(&ctx.config_dir).expect("Failed to create config dir");
    fs::write(&user_config_path, user_config_content).expect("Failed to write user config");

    // Project config that explicitly sets empty vectors
    let project_config_content = r#"
[hooks]
post_create = []

[sync]
sync_always_skip = []
sync_always_include = []
"#;

    let project_config_path = ctx.repo_path.join(".treebeard.toml");
    fs::write(&project_config_path, project_config_content)
        .expect("Failed to write project config");

    ctx.switch_to_repo();

    let config = load_config().expect("Failed to load config");

    assert!(
        config.hooks.post_create.is_empty(),
        "Empty project config should override user config for post_create"
    );
    assert!(
        config.sync.get_sync_always_skip().is_empty(),
        "Empty project config should override user config for sync_always_skip"
    );
    assert!(
        config.sync.get_sync_always_include().is_empty(),
        "Empty project config should override user config for sync_always_include"
    );
}
