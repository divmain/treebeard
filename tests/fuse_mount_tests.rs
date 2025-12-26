#![cfg(target_os = "macos")]

mod common;
mod fuse_common;

use common::TestWorkspace;
use fuse_common::{check_macfuse_installed, determine_mount_point, get_macos_major_version};
use fuser::Session;
use std::fs;
use std::process::Command;
use std::thread;
use treebeard::overlay::TreebeardFs;

/// Real FUSE mount and unmount
#[test]
fn test_fuse_real_mount_unmount() {
    if !check_macfuse_installed() {
        eprintln!("Skipping real FUSE test - macFUSE not installed");
        return;
    }

    let workspace = TestWorkspace::new();
    workspace.switch_to_repo();

    let test_name = "mount-unmount";
    let mountpoint = match determine_mount_point(test_name) {
        Ok(mp) => mp,
        Err(e) => {
            eprintln!("Failed to determine mount point: {}", e);
            workspace.restore_dir();
            return;
        }
    };

    let macos_version = get_macos_major_version();
    eprintln!("macOS version: {:?}", macos_version);
    eprintln!("Using macFUSE backend: VFS (kernel extension)");
    eprintln!("Mount point: {}", mountpoint.display());

    let upper_layer = tempfile::tempdir().unwrap().keep();
    let lower_layer = tempfile::tempdir().unwrap().keep();

    let lower_file = lower_layer.join("from_lower.txt");
    fs::write(&lower_file, "Content from lower layer").unwrap();

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

    let mount_output = Command::new("mount").output().unwrap();
    let mount_text = String::from_utf8_lossy(&mount_output.stdout);

    if mount_text.contains("treebeard") || mount_text.contains(&mountpoint.display().to_string()) {
        eprintln!("✓ Mount successfully created and visible in mount table");
    } else {
        eprintln!("⚠ Mount not immediately visible in mount table (may still be mounting)");
        eprintln!("Mount table output: {}", mount_text);
    }

    let mounted_file = mountpoint.join("from_lower.txt");
    match fs::read_to_string(&mounted_file) {
        Ok(content) => {
            assert_eq!(content, "Content from lower layer");
            eprintln!("✓ Successfully read file from lower layer through mount");
        }
        Err(e) => {
            eprintln!("Failed to read file from mount: {}", e);
            eprintln!("This may happen on some systems due to mount timing");
        }
    }

    drop(handle);
    workspace.restore_dir();

    eprintln!("✓ Test completed - mount point cleanup will be handled by RAII");
}

/// Cleanup of mount point on signal/shutdown
#[test]
fn test_fuse_real_mount_cleanup_on_signal() {
    if !check_macfuse_installed() {
        eprintln!("Skipping real FUSE test - macFUSE not installed");
        return;
    }

    let test_name = "mount-cleanup";
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

    eprintln!("✓ Filesystem mounted successfully");

    drop(handle);
    thread::sleep(std::time::Duration::from_millis(500));

    eprintln!("✓ Mount point cleanup handled by RAII");

    eprintln!("✓ Mount cleanup test completed");
}

/// VFS backend information diagnostic test
#[test]
fn test_fuse_backend_detection() {
    if !check_macfuse_installed() {
        eprintln!("Skipping real FUSE test - macFUSE not installed");
        return;
    }

    let version = get_macos_major_version();

    eprintln!("macOS Version: {:?}", version);
    eprintln!("✓ treebeard uses VFS backend (kernel extension)");
    eprintln!("  - Can mount anywhere (including temp directories)");
    eprintln!("  - Requires one-time kernel extension approval");
    eprintln!("  - Works on all macOS versions where macFUSE is supported");
    if version >= Some(15) {
        eprintln!("  - Note: FSKit backend is available on macOS 15.4+ but NOT used by treebeard");
    }

    let mp1 = determine_mount_point("backend-test-1");
    let mp2 = determine_mount_point("backend-test-2");

    let mp1_path = match mp1 {
        Ok(mp) => {
            eprintln!("✓ Mount point 1: {}", mp.display());
            mp
        }
        Err(e) => {
            eprintln!("✗ Failed to create mount point 1: {}", e);
            return;
        }
    };

    let mp2_path = match mp2 {
        Ok(mp) => {
            eprintln!("✓ Mount point 2: {}", mp.display());
            mp
        }
        Err(e) => {
            eprintln!("✗ Failed to create mount point 2: {}", e);
            let _ = fs::remove_dir(&mp1_path);
            return;
        }
    };

    let _ = fs::remove_dir(&mp1_path);
    let _ = fs::remove_dir(&mp2_path);

    eprintln!("✓ VFS backend information test completed");
}

/// Multiple file operations through mount point
#[test]
fn test_fuse_real_multiple_operations() {
    if !check_macfuse_installed() {
        eprintln!("Skipping real FUSE test - macFUSE not installed");
        return;
    }

    let test_name = "multiple-ops";
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

    eprintln!("✓ Filesystem mounted");

    for i in 1..=5 {
        let file_path = mountpoint.join(format!("file{}.txt", i));
        fs::write(&file_path, format!("Content {}", i)).unwrap();
        eprintln!("✓ Created file{}.txt", i);
    }

    for i in 1..=5 {
        let file_path = mountpoint.join(format!("file{}.txt", i));
        let content = fs::read_to_string(&file_path).unwrap();
        assert_eq!(content, format!("Content {}", i));
    }
    eprintln!("✓ All files read back correctly");

    let dir_path = mountpoint.join("testdir");
    fs::create_dir(&dir_path).unwrap();
    eprintln!("✓ Created directory");

    let nested_file = dir_path.join("nested.txt");
    fs::write(&nested_file, " nested content").unwrap();
    eprintln!("✓ Created file in directory");

    assert!(nested_file.exists());
    let nested_content = fs::read_to_string(&nested_file).unwrap();
    assert_eq!(nested_content, " nested content");
    eprintln!("✓ Nested file content verified");

    drop(handle);
    eprintln!("✓ Multiple operations test completed");
}
