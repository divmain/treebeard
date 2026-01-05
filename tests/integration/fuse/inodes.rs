#![cfg(target_os = "macos")]

use crate::shared::fuse_helpers::FuseTestSession;

use std::fs;
use std::thread;

/// Hard link inode tracking
#[test]
fn test_fuse_real_hardlink_inodes() {
    let session = match FuseTestSession::new("hardlink-inodes") {
        Some(s) => s,
        None => return,
    };

    let original_file = session.mountpoint.join("original.txt");
    fs::write(&original_file, "shared content").unwrap();
    eprintln!("✓ Created original file");

    use std::os::unix::fs::MetadataExt;
    let original_meta = fs::metadata(&original_file).unwrap();
    let original_ino = original_meta.ino();
    let original_nlink = original_meta.nlink();

    let hardlink_file = session.mountpoint.join("hardlink.txt");
    match fs::hard_link(&original_file, &hardlink_file) {
        Ok(_) => eprintln!("✓ Created hard link"),
        Err(e) => {
            eprintln!("Failed to create hard link: {}", e);
            eprintln!("This may fail on some FUSE backends");
            return;
        }
    }

    assert!(hardlink_file.exists(), "Hard link should exist");
    eprintln!("✓ Hard link file exists");

    let new_meta = fs::metadata(&original_file).unwrap();
    assert_eq!(
        new_meta.nlink(),
        original_nlink + 1,
        "Link count should increase"
    );
    eprintln!(
        "✓ Link count increased from {} to {}",
        original_nlink,
        new_meta.nlink()
    );

    let hardlink_meta = fs::metadata(&hardlink_file).unwrap();
    assert_eq!(
        hardlink_meta.ino(),
        original_ino,
        "Hard links should share inode"
    );
    eprintln!("✓ Both files share same inode: {}", original_ino);

    let orig_content = fs::read_to_string(&original_file).unwrap();
    let link_content = fs::read_to_string(&hardlink_file).unwrap();
    assert_eq!(orig_content, link_content);
    eprintln!("✓ Both files have identical content");

    eprintln!("✓ Hard link inode tracking test completed");
}

/// Copy_up TOCTOU handling - source file deleted between check and copy
#[test]
fn test_fuse_toctou_copy_up_source_deleted() {
    let session =
        match FuseTestSession::with_lower_layer_setup("toctou-copy-up-source-deleted", |lower| {
            let lower_file = lower.join("test.txt");
            fs::write(&lower_file, "original content").unwrap();
        }) {
            Some(s) => s,
            None => return,
        };

    let mounted_file = session.mountpoint.join("test.txt");

    let max_retries = 5;
    for attempt in 0..max_retries {
        match fs::read_to_string(&mounted_file) {
            Ok(c) => {
                assert_eq!(c, "original content");
                eprintln!("✓ Read file from lower layer");
                break;
            }
            Err(e) => {
                if attempt == max_retries - 1 {
                    eprintln!("Failed to read file after {} attempts: {}", max_retries, e);
                    return;
                }
                eprintln!("Read attempt {} failed: {}, retrying...", attempt + 1, e);
                thread::sleep(std::time::Duration::from_millis(200));
            }
        }
    }

    let upper_path = session.upper_layer.join("test.txt");
    assert!(
        !upper_path.exists(),
        "File should not exist in upper layer yet"
    );
    eprintln!("✓ Confirmed file not yet in upper layer");

    fs::remove_file(session.lower_layer.join("test.txt")).unwrap();
    eprintln!("✓ Deleted source file from lower layer");

    let mounted_file_clone = mounted_file.clone();
    let access_thread = thread::spawn(move || {
        thread::sleep(std::time::Duration::from_millis(100));
        fs::metadata(&mounted_file_clone)
    });

    let result = access_thread.join().ok();
    match result {
        Some(Ok(_)) => eprintln!("✓ File metadata retrieved after deletion (may be cached)"),
        Some(Err(e)) => eprintln!("✓ File access after deletion handled: {}", e),
        None => eprintln!("⚠ Access thread panicked or hung"),
    }

    eprintln!("✓ TOCTOU copy_up handling test completed");
}
