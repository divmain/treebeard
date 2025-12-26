#![cfg(target_os = "macos")]

mod common;
mod fuse_common;

use fuse_common::FuseTestSession;
use std::fs;

#[test]
fn test_passthrough_read_from_lower() {
    let Some(session) = FuseTestSession::with_lower_layer_setup_and_passthrough(
        "passthrough-read",
        |lower| {
            fs::create_dir_all(lower.join(".claude")).unwrap();
            fs::write(lower.join(".claude/config.toml"), "passthrough content").unwrap();
        },
        vec![".claude/**".to_string()],
    ) else {
        return;
    };

    let content = fs::read_to_string(session.mountpoint.join(".claude/config.toml")).unwrap();
    assert_eq!(content, "passthrough content");
}

#[test]
fn test_passthrough_ignores_upper() {
    let Some(session) = FuseTestSession::with_lower_layer_setup_and_passthrough(
        "passthrough-ignore-upper",
        |_lower| {},
        vec![".claude/**".to_string()],
    ) else {
        return;
    };

    // Add file to upper layer
    fs::create_dir_all(session.upper_layer.join(".claude")).unwrap();
    fs::write(
        session.upper_layer.join(".claude/config.toml"),
        "upper content",
    )
    .unwrap();

    // Should be ENOENT because it's not in lower layer, and upper is ignored
    assert!(!session.mountpoint.join(".claude/config.toml").exists());
}

#[test]
fn test_passthrough_write_to_lower() {
    let Some(session) = FuseTestSession::with_lower_layer_setup_and_passthrough(
        "passthrough-write",
        |lower| {
            fs::create_dir_all(lower.join(".claude")).unwrap();
            fs::write(lower.join(".claude/config.toml"), "original content").unwrap();
        },
        vec![".claude/**".to_string()],
    ) else {
        return;
    };

    fs::write(
        session.mountpoint.join(".claude/config.toml"),
        "new content",
    )
    .unwrap();

    let lower_content =
        fs::read_to_string(session.lower_layer.join(".claude/config.toml")).unwrap();
    assert_eq!(lower_content, "new content");

    assert!(!session.upper_layer.join(".claude/config.toml").exists());
}

#[test]
fn test_passthrough_create_in_lower() {
    let Some(session) = FuseTestSession::with_lower_layer_setup_and_passthrough(
        "passthrough-create",
        |lower| {
            fs::create_dir_all(lower.join(".claude")).unwrap();
        },
        vec![".claude/**".to_string()],
    ) else {
        return;
    };

    fs::write(session.mountpoint.join(".claude/new.txt"), "new file").unwrap();

    assert!(session.lower_layer.join(".claude/new.txt").exists());
    assert!(!session.upper_layer.join(".claude/new.txt").exists());
}

#[test]
fn test_passthrough_delete_from_lower() {
    let Some(session) = FuseTestSession::with_lower_layer_setup_and_passthrough(
        "passthrough-delete",
        |lower| {
            fs::create_dir_all(lower.join(".claude")).unwrap();
            fs::write(lower.join(".claude/delete_me.txt"), "delete me").unwrap();
        },
        vec![".claude/**".to_string()],
    ) else {
        return;
    };

    fs::remove_file(session.mountpoint.join(".claude/delete_me.txt")).unwrap();

    assert!(!session.lower_layer.join(".claude/delete_me.txt").exists());
    // Should NOT have a whiteout
    assert!(!session
        .upper_layer
        .join(".claude/.wh.delete_me.txt")
        .exists());
}

#[test]
fn test_passthrough_readdir_lower_only() {
    let Some(session) = FuseTestSession::with_both_layers_setup_and_passthrough(
        "passthrough-readdir",
        |lower| {
            fs::create_dir_all(lower.join(".claude")).unwrap();
            fs::write(lower.join(".claude/lower.txt"), "lower").unwrap();
        },
        |upper| {
            fs::create_dir_all(upper.join(".claude")).unwrap();
            fs::write(upper.join(".claude/upper.txt"), "upper").unwrap();
        },
        vec![".claude/**".to_string()],
    ) else {
        return;
    };

    let entries: Vec<String> = fs::read_dir(session.mountpoint.join(".claude"))
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().to_string())
        .collect();

    assert!(entries.contains(&"lower.txt".to_string()));
    assert!(!entries.contains(&"upper.txt".to_string()));
}
