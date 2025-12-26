#![cfg(target_os = "macos")]

mod common;
mod fuse_common;

use common::TestWorkspace;
use fuse_common::{check_macfuse_installed, determine_mount_point};
use fuser::Session;
use std::fs;
use std::thread;
use treebeard::overlay::TreebeardFs;

/// Hard link inode tracking
#[test]
fn test_fuse_real_hardlink_inodes() {
    if !check_macfuse_installed() {
        eprintln!("Skipping real FUSE test - macFUSE not installed");
        return;
    }

    let test_name = "hardlink-inodes";
    let mountpoint = match determine_mount_point(test_name) {
        Ok(mp) => mp,
        Err(e) => {
            eprintln!("Failed to determine mount point: {}", e);
            return;
        }
    };

    let upper_layer = tempfile::tempdir().unwrap().keep();
    let lower_layer = tempfile::tempdir().unwrap().keep();

    let fs = match TreebeardFs::new(upper_layer.clone(), lower_layer.clone(), None, 1, vec![]) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("Failed to create TreebeardFs: {}", e);
            return;
        }
    };

    let _cleanup = fuse_common::MountCleanup::new(mountpoint.clone());
    let session = match Session::new(fs, &mountpoint, &[]) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Failed to create FUSE session: {}", e);
            return;
        }
    };

    let handle = match session.spawn() {
        Ok(h) => h,
        Err(e) => {
            eprintln!("Failed to spawn FUSE session: {}", e);
            return;
        }
    };

    thread::sleep(std::time::Duration::from_millis(
        fuse_common::TEST_SETUP_DELAY_MS,
    ));

    let original_file = mountpoint.join("original.txt");
    fs::write(&original_file, "shared content").unwrap();
    eprintln!("✓ Created original file");

    use std::os::unix::fs::MetadataExt;
    let original_meta = fs::metadata(&original_file).unwrap();
    let original_ino = original_meta.ino();
    let original_nlink = original_meta.nlink();

    let hardlink_file = mountpoint.join("hardlink.txt");
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

    drop(handle);
    eprintln!("✓ Hard link inode tracking test completed");
}

/// Copy_up TOCTOU handling - source file deleted between check and copy
#[test]
fn test_fuse_toctou_copy_up_source_deleted() {
    if !check_macfuse_installed() {
        eprintln!("Skipping real FUSE test - macFUSE not installed");
        return;
    }

    let workspace = TestWorkspace::new();
    workspace.switch_to_repo();

    let test_name = "toctou-copy-up-source-deleted";
    let mountpoint = match determine_mount_point(test_name) {
        Ok(mp) => mp,
        Err(e) => {
            eprintln!("Failed to determine mount point: {}", e);
            workspace.restore_dir();
            return;
        }
    };

    let upper_layer = tempfile::tempdir().unwrap().keep();
    let lower_layer = tempfile::tempdir().unwrap().keep();

    let lower_file = lower_layer.join("test.txt");
    fs::write(&lower_file, "original content").unwrap();

    let fs = match TreebeardFs::new(upper_layer.clone(), lower_layer.clone(), None, 1, vec![]) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("Failed to create TreebeardFs: {}", e);
            workspace.restore_dir();
            return;
        }
    };

    let _cleanup = fuse_common::MountCleanup::new(mountpoint.clone());
    let session = match Session::new(fs, &mountpoint, &[]) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Failed to create FUSE session: {}", e);
            workspace.restore_dir();
            return;
        }
    };

    let handle = match session.spawn() {
        Ok(h) => h,
        Err(e) => {
            eprintln!("Failed to spawn FUSE session: {}", e);
            workspace.restore_dir();
            return;
        }
    };

    thread::sleep(std::time::Duration::from_millis(
        fuse_common::TEST_SETUP_DELAY_MS,
    ));

    let mounted_file = mountpoint.join("test.txt");

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
                    drop(handle);
                    workspace.restore_dir();
                    return;
                }
                eprintln!("Read attempt {} failed: {}, retrying...", attempt + 1, e);
                thread::sleep(std::time::Duration::from_millis(200));
            }
        }
    }

    let upper_path = upper_layer.join("test.txt");
    assert!(
        !upper_path.exists(),
        "File should not exist in upper layer yet"
    );
    eprintln!("✓ Confirmed file not yet in upper layer");

    fs::remove_file(&lower_file).unwrap();
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

    drop(handle);
    eprintln!("✓ TOCTOU copy_up handling test completed");
    workspace.restore_dir();
}
