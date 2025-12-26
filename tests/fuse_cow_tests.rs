#![cfg(target_os = "macos")]

mod common;
mod fuse_common;

use fuse_common::FuseTestSession;
use std::fs;

/// COW behavior - modify file from lower layer
#[test]
fn test_fuse_real_cow_behavior() {
    let Some(session) = FuseTestSession::with_lower_layer_setup("cow-behavior", |lower| {
        let lower_file = lower.join("original.txt");
        fs::write(&lower_file, "original content from lower").unwrap();
    }) else {
        return;
    };

    let lower_file = session.lower_layer.join("original.txt");
    let mounted_file = session.mountpoint.join("original.txt");
    let content_before = match fs::read_to_string(&mounted_file) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Failed to read file before modification: {}", e);
            return;
        }
    };

    assert_eq!(content_before, "original content from lower");
    eprintln!("✓ Read file from lower layer: {}", content_before);

    match fs::write(&mounted_file, "modified content in upper layer") {
        Ok(_) => eprintln!("✓ Modified file through mount point"),
        Err(e) => {
            eprintln!("Failed to write file: {}", e);
            return;
        }
    }

    let content_after = fs::read_to_string(&mounted_file).unwrap();
    assert_eq!(content_after, "modified content in upper layer");
    eprintln!("✓ Successfully modified file: {}", content_after);

    let upper_file = session.upper_layer.join("original.txt");
    assert!(upper_file.exists(), "File should be copied to upper layer");
    let upper_content = fs::read_to_string(&upper_file).unwrap();
    assert_eq!(upper_content, "modified content in upper layer");
    eprintln!("✓ File successfully copied to upper layer");

    let lower_content = fs::read_to_string(&lower_file).unwrap();
    assert_eq!(lower_content, "original content from lower");
    eprintln!("✓ Lower layer still contains original content");

    drop(session.handle);
    eprintln!("✓ COW behavior test completed");
}
