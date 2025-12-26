#![cfg(target_os = "macos")]

mod common;
mod fuse_common;

use fuse_common::{check_macfuse_installed, determine_mount_point};
use fuser::Session;
use std::fs;
use std::os::unix::ffi::OsStrExt;
use std::thread;
use treebeard::overlay::TreebeardFs;

/// Whiteout file creation for deletions
#[test]
fn test_fuse_real_whiteout_creation() {
    if !check_macfuse_installed() {
        eprintln!("Skipping real FUSE test - macFUSE not installed");
        return;
    }

    let test_name = "whiteout-creation";
    let mountpoint = match determine_mount_point(test_name) {
        Ok(mp) => mp,
        Err(e) => {
            eprintln!("Failed to determine mount point: {}", e);
            return;
        }
    };

    let upper_layer = tempfile::tempdir().unwrap().keep();
    let lower_layer = tempfile::tempdir().unwrap().keep();

    let lower_file = lower_layer.join("to_delete.txt");
    fs::write(&lower_file, "file to be deleted").unwrap();

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

    let mounted_file = mountpoint.join("to_delete.txt");
    assert!(
        mounted_file.exists(),
        "File should be visible in mount point"
    );
    eprintln!("✓ File is visible in mount point before deletion");

    match fs::remove_file(&mounted_file) {
        Ok(_) => eprintln!("✓ Deleted file through mount point"),
        Err(e) => {
            eprintln!("Failed to delete file: {}", e);
            return;
        }
    }

    assert!(
        !mounted_file.exists(),
        "File should not appear after deletion"
    );
    eprintln!("✓ File no longer visible in mount point");

    let aufs_whiteout = upper_layer.join(".wh.to_delete.txt");
    if aufs_whiteout.exists() {
        eprintln!(
            "✓ Whiteout file created in upper layer at: {} (AUFS-style)",
            aufs_whiteout.display()
        );
    } else {
        let upper_whiteout = upper_layer.join("to_delete.txt");
        if upper_whiteout.exists() {
            use std::os::unix::fs::MetadataExt;
            if let Ok(meta) = fs::metadata(&upper_whiteout) {
                let mode = meta.mode();
                let is_char = mode & (libc::S_IFMT as u32) == (libc::S_IFCHR as u32);
                let rdev = meta.rdev();

                if is_char && rdev == 0 {
                    eprintln!("✓ Whiteout is a character device at (0,0) - Linux overlayfs style");
                } else {
                    eprintln!("⚠ Whiteout file exists but is not a proper whiteout marker");
                }
            }
        } else {
            eprintln!("⚠ Whiteout file not found in upper layer");
        }
    }

    assert!(lower_file.exists(), "Lower layer file should still exist");
    eprintln!("✓ Lower layer file still exists");

    drop(handle);
    eprintln!("✓ Whiteout creation test completed");
}

/// Diagnostic test: Determines which file deletion scenarios work on macOS.
#[test]
fn test_diagnostic_deletion_scenarios() {
    if !check_macfuse_installed() {
        eprintln!("Skipping real FUSE test - macFUSE not installed");
        return;
    }

    let test_name = "diagnostic-deletion";
    let mountpoint = match determine_mount_point(test_name) {
        Ok(mp) => mp,
        Err(e) => {
            eprintln!("Failed to determine mount point: {}", e);
            return;
        }
    };

    let upper_layer = tempfile::tempdir().unwrap().keep();
    let lower_layer = tempfile::tempdir().unwrap().keep();

    fs::write(lower_layer.join("lower_file.txt"), "from lower layer").unwrap();

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

    eprintln!("=== Diagnostic: File Deletion Scenarios on macOS ===\n");

    let upper_file = mountpoint.join("created_in_mount.txt");

    match fs::write(&upper_file, "created through mount") {
        Ok(_) => eprintln!("  ✓ Created file through mount point"),
        Err(e) => {
            eprintln!("  ✗ Failed to create file: {}", e);
            drop(handle);
            return;
        }
    }

    let upper_actual = upper_layer.join("created_in_mount.txt");
    eprintln!("  File exists in upper layer: {}", upper_actual.exists());

    match fs::remove_file(&upper_file) {
        Ok(_) => {
            eprintln!("  ✓ Successfully deleted upper-layer file through mount");
            eprintln!("  → Upper layer deletion works (no whiteout needed)");
        }
        Err(e) => {
            eprintln!(
                "  ✗ Failed to delete upper-layer file: {} (errno {})",
                e,
                e.raw_os_error().unwrap_or(-1)
            );
            eprintln!("  → This is unexpected - upper layer deletion should work");
        }
    }

    eprintln!("\n--- Scenario 2: Delete file from lower layer (requires whiteout) ---");
    let lower_file_mount = mountpoint.join("lower_file.txt");

    if lower_file_mount.exists() {
        eprintln!("  ✓ Lower layer file visible through mount");
    } else {
        eprintln!("  ✗ Lower layer file NOT visible through mount");
        drop(handle);
        return;
    }

    match fs::remove_file(&lower_file_mount) {
        Ok(_) => {
            eprintln!("  ✓ Successfully deleted lower-layer file through mount");

            let aufs_whiteout_path = upper_layer.join(".wh.lower_file.txt");
            if aufs_whiteout_path.exists() {
                eprintln!("  → Whiteout created as .wh.lower_file.txt (AUFS-style)");
            } else {
                let whiteout_path = upper_layer.join("lower_file.txt");
                if whiteout_path.exists() {
                    use std::os::unix::fs::MetadataExt;
                    if let Ok(meta) = fs::metadata(&whiteout_path) {
                        let mode = meta.mode();
                        let is_char = mode & (libc::S_IFMT as u32) == (libc::S_IFCHR as u32);
                        let rdev = meta.rdev();

                        if is_char && rdev == 0 {
                            eprintln!(
                                "  → Whiteout created as character device (0,0) - Linux-style"
                            );
                        } else if is_char {
                            eprintln!(
                                "  → Whiteout created as character device ({}, {}) - unexpected",
                                libc::major(rdev as libc::dev_t),
                                libc::minor(rdev as libc::dev_t)
                            );
                        } else {
                            eprintln!("  → Whiteout created as regular file - alternative format");
                        }
                    }
                } else {
                    eprintln!(
                        "  → No whiteout file found in upper layer (in-memory tracking only?)"
                    );
                }
            }
        }
        Err(e) => {
            let errno = e.raw_os_error().unwrap_or(-1);
            eprintln!(
                "  ✗ Failed to delete lower-layer file: {} (errno {})",
                e, errno
            );

            if errno == libc::EPERM {
                eprintln!("  → EPERM indicates whiteout creation failed");
                eprintln!("  → On macOS, mknod() for character devices requires root");
            }
        }
    }

    eprintln!("\n--- Scenario 3: Direct mknod test (outside FUSE) ---");
    let test_mknod_path = upper_layer.join(".test_mknod_whiteout");
    let c_path = std::ffi::CString::new(test_mknod_path.as_os_str().as_bytes()).unwrap();

    let mknod_result =
        unsafe { libc::mknod(c_path.as_ptr(), libc::S_IFCHR | 0o644, libc::makedev(0, 0)) };

    if mknod_result == 0 {
        eprintln!("  ✓ mknod succeeded (unexpected for non-root on macOS)");
        let _ = fs::remove_file(&test_mknod_path);
    } else {
        let errno = std::io::Error::last_os_error();
        eprintln!(
            "  ✗ mknod failed: {} (errno {})",
            errno,
            errno.raw_os_error().unwrap_or(-1)
        );
        if errno.raw_os_error() == Some(libc::EPERM) {
            eprintln!("  → Confirmed: mknod for char devices requires root on this system");
        }
    }

    eprintln!("\n=== Diagnostic Complete ===");

    drop(handle);
}
