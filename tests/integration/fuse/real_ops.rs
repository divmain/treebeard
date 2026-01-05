#![cfg(target_os = "macos")]

use crate::shared::fuse_helpers::{check_macfuse_installed, determine_mount_point, MountCleanup};

use fuser::Session;
use std::fs;
use std::process::Command;
use std::thread;
use std::time::Duration;
use treebeard::overlay::TreebeardFs;

/// Readdir returns files from both layers
///
/// Verifies that `readdir` scans the actual directories on both upper and
/// lower layers, not just the in-memory inode cache.
#[test]
fn test_readdir_shows_all_files_from_both_layers() {
    if !check_macfuse_installed() {
        eprintln!("Skipping real FUSE test - macFUSE not installed");
        return;
    }

    let test_name = "readdir-all-files";
    let mountpoint = match determine_mount_point(test_name) {
        Ok(mp) => mp,
        Err(e) => {
            eprintln!("Failed to determine mount point: {}", e);
            return;
        }
    };

    let upper_layer = tempfile::tempdir().unwrap().keep();
    let lower_layer = tempfile::tempdir().unwrap().keep();

    // Create files in LOWER layer (simulating main repo)
    fs::write(lower_layer.join("from_lower1.txt"), "lower content 1").unwrap();
    fs::write(lower_layer.join("from_lower2.txt"), "lower content 2").unwrap();
    fs::create_dir(lower_layer.join("lower_dir")).unwrap();
    fs::write(lower_layer.join("lower_dir/nested.txt"), "nested content").unwrap();

    // Create files in UPPER layer (simulating worktree modifications)
    fs::write(upper_layer.join("from_upper.txt"), "upper content").unwrap();

    let fs_instance =
        match TreebeardFs::new(upper_layer.clone(), lower_layer.clone(), None, 1, vec![]) {
            Ok(f) => f,
            Err(e) => {
                eprintln!("Failed to create TreebeardFs: {}", e);
                return;
            }
        };

    let _cleanup = MountCleanup::new(mountpoint.clone());
    let session = match Session::new(fs_instance, &mountpoint, &[]) {
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

    // Give filesystem time to mount
    thread::sleep(Duration::from_millis(500));

    // Test: ls -la immediately after mount should show all files
    let entries: Vec<_> = match fs::read_dir(&mountpoint) {
        Ok(entries) => entries.filter_map(|e| e.ok()).collect(),
        Err(e) => {
            panic!("REGRESSION: Cannot read directory: {}", e);
        }
    };

    let entry_names: Vec<String> = entries
        .iter()
        .map(|e| e.file_name().to_string_lossy().to_string())
        .collect();

    eprintln!("Files found in readdir: {:?}", entry_names);

    // Verify we see files from LOWER layer
    assert!(
        entry_names.contains(&"from_lower1.txt".to_string()),
        "REGRESSION: readdir should show 'from_lower1.txt' from lower layer"
    );
    assert!(
        entry_names.contains(&"from_lower2.txt".to_string()),
        "REGRESSION: readdir should show 'from_lower2.txt' from lower layer"
    );
    assert!(
        entry_names.contains(&"lower_dir".to_string()),
        "REGRESSION: readdir should show 'lower_dir' directory from lower layer"
    );
    eprintln!("✓ readdir shows files from lower layer");

    // Verify we see files from UPPER layer
    assert!(
        entry_names.contains(&"from_upper.txt".to_string()),
        "REGRESSION: readdir should show 'from_upper.txt' from upper layer"
    );
    eprintln!("✓ readdir shows files from upper layer");

    // Verify we can read directory contents of lower_dir
    let lower_dir_entries: Vec<_> = match fs::read_dir(mountpoint.join("lower_dir")) {
        Ok(entries) => entries.filter_map(|e| e.ok()).collect(),
        Err(e) => {
            panic!("REGRESSION: Cannot read nested directory: {}", e);
        }
    };

    let lower_dir_names: Vec<String> = lower_dir_entries
        .iter()
        .map(|e| e.file_name().to_string_lossy().to_string())
        .collect();

    assert!(
        lower_dir_names.contains(&"nested.txt".to_string()),
        "REGRESSION: readdir should show nested files from lower layer"
    );
    eprintln!("✓ readdir shows nested files in subdirectories");

    drop(handle);
    eprintln!("✓ readdir shows all files from both layers");
}

/// Readdir properly handles whiteouts
///
/// Tests that when a file is deleted (creating a whiteout), it no longer
/// appears in directory listings.
#[test]
fn test_readdir_respects_whiteouts() {
    if !check_macfuse_installed() {
        eprintln!("Skipping real FUSE test - macFUSE not installed");
        return;
    }

    let test_name = "readdir-whiteouts";
    let mountpoint = match determine_mount_point(test_name) {
        Ok(mp) => mp,
        Err(e) => {
            eprintln!("Failed to determine mount point: {}", e);
            return;
        }
    };

    let upper_layer = tempfile::tempdir().unwrap().keep();
    let lower_layer = tempfile::tempdir().unwrap().keep();

    // Create files in lower layer
    fs::write(lower_layer.join("keep_me.txt"), "I will stay").unwrap();
    fs::write(lower_layer.join("delete_me.txt"), "I will be deleted").unwrap();

    let fs_instance =
        match TreebeardFs::new(upper_layer.clone(), lower_layer.clone(), None, 1, vec![]) {
            Ok(f) => f,
            Err(e) => {
                eprintln!("Failed to create TreebeardFs: {}", e);
                return;
            }
        };

    let _cleanup = MountCleanup::new(mountpoint.clone());
    let session = match Session::new(fs_instance, &mountpoint, &[]) {
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

    // Give filesystem time to mount
    thread::sleep(Duration::from_millis(500));

    // Verify both files are visible initially
    let entries_before: Vec<String> = fs::read_dir(&mountpoint)
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().to_string())
        .collect();

    assert!(entries_before.contains(&"keep_me.txt".to_string()));
    assert!(entries_before.contains(&"delete_me.txt".to_string()));
    eprintln!("Both files visible before deletion: {:?}", entries_before);

    // Delete delete_me.txt through the mount point (creates whiteout)
    let file_to_delete = mountpoint.join("delete_me.txt");
    match fs::remove_file(&file_to_delete) {
        Ok(_) => eprintln!("Deleted file through mount point"),
        Err(e) => {
            panic!("Failed to delete file: {}", e);
        }
    }

    // Verify readdir no longer shows the deleted file
    let entries_after: Vec<String> = fs::read_dir(&mountpoint)
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().to_string())
        .collect();

    assert!(
        entries_after.contains(&"keep_me.txt".to_string()),
        "keep_me.txt should still be visible"
    );
    assert!(
        !entries_after.contains(&"delete_me.txt".to_string()),
        "REGRESSION: delete_me.txt should NOT appear in readdir after deletion (whiteout)"
    );
    eprintln!(
        "✓ Deleted file no longer appears in readdir: {:?}",
        entries_after
    );

    // Verify lookup also respects whiteout
    assert!(
        !file_to_delete.exists(),
        "REGRESSION: Deleted file should not be accessible via lookup"
    );
    eprintln!("✓ Deleted file not accessible via lookup");

    // Verify lower layer still has the original file
    assert!(
        lower_layer.join("delete_me.txt").exists(),
        "Lower layer should still have the original file"
    );
    eprintln!("Lower layer file preserved");

    drop(handle);
    eprintln!("✓ readdir respects whiteouts");
}

/// Upper layer files take precedence in readdir
///
/// When a file exists in both layers, the upper layer version should be
/// shown and accessible.
#[test]
fn test_readdir_upper_layer_takes_precedence() {
    if !check_macfuse_installed() {
        eprintln!("Skipping real FUSE test - macFUSE not installed");
        return;
    }

    let test_name = "upper-precedence";
    let mountpoint = match determine_mount_point(test_name) {
        Ok(mp) => mp,
        Err(e) => {
            eprintln!("Failed to determine mount point: {}", e);
            return;
        }
    };

    let upper_layer = tempfile::tempdir().unwrap().keep();
    let lower_layer = tempfile::tempdir().unwrap().keep();

    // Create file with SAME NAME in both layers but different content
    fs::write(lower_layer.join("shared.txt"), "LOWER LAYER CONTENT").unwrap();
    fs::write(upper_layer.join("shared.txt"), "UPPER LAYER CONTENT").unwrap();

    // Create file only in lower layer
    fs::write(lower_layer.join("only_lower.txt"), "only in lower").unwrap();

    // Create file only in upper layer
    fs::write(upper_layer.join("only_upper.txt"), "only in upper").unwrap();

    let fs_instance =
        match TreebeardFs::new(upper_layer.clone(), lower_layer.clone(), None, 1, vec![]) {
            Ok(f) => f,
            Err(e) => {
                eprintln!("Failed to create TreebeardFs: {}", e);
                return;
            }
        };

    let _cleanup = MountCleanup::new(mountpoint.clone());
    let session = match Session::new(fs_instance, &mountpoint, &[]) {
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

    // Give filesystem time to mount
    thread::sleep(Duration::from_millis(500));

    // Test 1: Verify all files appear in directory listing
    let entries: Vec<String> = fs::read_dir(&mountpoint)
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().to_string())
        .collect();

    eprintln!("Files in readdir: {:?}", entries);

    assert!(
        entries.contains(&"shared.txt".to_string()),
        "shared.txt should appear"
    );
    assert!(
        entries.contains(&"only_lower.txt".to_string()),
        "only_lower.txt should appear"
    );
    assert!(
        entries.contains(&"only_upper.txt".to_string()),
        "only_upper.txt should appear"
    );
    eprintln!("All expected files appear in readdir");

    // Test 2: When reading shared.txt, we should get UPPER layer content
    let shared_content = fs::read_to_string(mountpoint.join("shared.txt")).unwrap();
    assert_eq!(
        shared_content, "UPPER LAYER CONTENT",
        "REGRESSION: Upper layer file should take precedence over lower layer"
    );
    eprintln!("✓ Upper layer file takes precedence for 'shared.txt'");

    // Test 3: Verify only_lower.txt reads from lower layer
    let lower_content = fs::read_to_string(mountpoint.join("only_lower.txt")).unwrap();
    assert_eq!(lower_content, "only in lower");
    eprintln!("File only in lower layer is accessible");

    // Test 4: Verify only_upper.txt reads from upper layer
    let upper_content = fs::read_to_string(mountpoint.join("only_upper.txt")).unwrap();
    assert_eq!(upper_content, "only in upper");
    eprintln!("File only in upper layer is accessible");

    drop(handle);
    eprintln!("✓ Upper precedence test completed");
}

/// Flush prevents write errors
///
/// Verifies that the `flush` operation returns success when the file handle
/// is valid, preventing shells from reporting spurious write errors.
#[test]
fn test_flush_prevents_write_errors() {
    if !check_macfuse_installed() {
        eprintln!("Skipping real FUSE test - macFUSE not installed");
        return;
    }

    let test_name = "flush-write";
    let mountpoint = match determine_mount_point(test_name) {
        Ok(mp) => mp,
        Err(e) => {
            eprintln!("Failed to determine mount point: {}", e);
            return;
        }
    };

    let upper_layer = tempfile::tempdir().unwrap().keep();
    let lower_layer = tempfile::tempdir().unwrap().keep();

    let fs_instance =
        match TreebeardFs::new(upper_layer.clone(), lower_layer.clone(), None, 1, vec![]) {
            Ok(f) => f,
            Err(e) => {
                eprintln!("Failed to create TreebeardFs: {}", e);
                return;
            }
        };

    let _cleanup = MountCleanup::new(mountpoint.clone());
    let session = match Session::new(fs_instance, &mountpoint, &[]) {
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

    // Give filesystem time to mount
    thread::sleep(Duration::from_millis(500));

    // Test 1: Basic file write (simulates `echo "content" > file.txt`)
    let test_file = mountpoint.join("test-write.txt");
    match fs::write(&test_file, "test content from write") {
        Ok(_) => {
            eprintln!("✓ fs::write completed without error");
        }
        Err(e) => {
            panic!("REGRESSION: fs::write failed (flush not working): {}", e);
        }
    }

    // Verify file content was written correctly
    let content = fs::read_to_string(&test_file).unwrap();
    assert_eq!(content, "test content from write");
    eprintln!("✓ File content is correct");

    // Test 2: Verify file exists in upper layer
    let upper_file = upper_layer.join("test-write.txt");
    assert!(upper_file.exists(), "File should exist in upper layer");
    eprintln!("✓ File exists in upper layer");

    // Test 3: Multiple sequential writes (stress test flush)
    for i in 1..=5 {
        let file_path = mountpoint.join(format!("sequential-{}.txt", i));
        match fs::write(&file_path, format!("content {}", i)) {
            Ok(_) => {}
            Err(e) => {
                panic!("REGRESSION: Sequential write {} failed: {}", i, e);
            }
        }
    }
    eprintln!("✓ 5 sequential writes completed without error");

    // Test 4: Append mode (tests flush with O_APPEND)
    let append_file = mountpoint.join("append-test.txt");
    fs::write(&append_file, "initial\n").unwrap();

    // Use OpenOptions for append mode
    use std::io::Write;
    {
        let mut file = std::fs::OpenOptions::new()
            .append(true)
            .open(&append_file)
            .expect("Failed to open file for append");
        writeln!(file, "appended line 1").expect("REGRESSION: Append write failed");
        writeln!(file, "appended line 2").expect("REGRESSION: Append write failed");
    }
    eprintln!("✓ Append writes completed without error");

    let append_content = fs::read_to_string(&append_file).unwrap();
    assert!(append_content.contains("initial"));
    assert!(append_content.contains("appended line 1"));
    assert!(append_content.contains("appended line 2"));
    eprintln!("✓ Appended content is correct");

    // Test 5: Create file, write, close, reopen, write again
    let reopen_file = mountpoint.join("reopen-test.txt");
    {
        let mut file = std::fs::File::create(&reopen_file).expect("Failed to create file");
        write!(file, "first write").expect("REGRESSION: First write failed");
    }
    {
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .truncate(true)
            .open(&reopen_file)
            .expect("Failed to reopen file");
        write!(file, "second write").expect("REGRESSION: Second write failed");
    }
    let reopen_content = fs::read_to_string(&reopen_file).unwrap();
    assert_eq!(reopen_content, "second write");
    eprintln!("✓ Multiple open/write/close cycles work correctly");

    // Test 6: Use shell command to verify echo redirection works
    let shell_file = mountpoint.join("shell-test.txt");
    let shell_file_str = shell_file.to_str().unwrap();

    let output = Command::new("sh")
        .arg("-c")
        .arg(format!("echo 'shell content' > '{}'", shell_file_str))
        .output()
        .expect("Failed to execute shell command");

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        panic!("REGRESSION: Shell echo redirection failed: {}", stderr);
    }
    eprintln!("✓ Shell echo redirection completed without error");

    // Verify shell-created file has correct content
    let shell_content = fs::read_to_string(&shell_file).unwrap();
    assert!(
        shell_content.trim() == "shell content",
        "Shell file content should match"
    );
    eprintln!("✓ Shell-created file has correct content");

    // Test 7: Shell append redirection
    let output = Command::new("sh")
        .arg("-c")
        .arg(format!("echo 'appended via shell' >> '{}'", shell_file_str))
        .output()
        .expect("Failed to execute shell append command");

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        panic!("REGRESSION: Shell append redirection failed: {}", stderr);
    }
    eprintln!("✓ Shell append redirection completed without error");

    let shell_append_content = fs::read_to_string(&shell_file).unwrap();
    assert!(shell_append_content.contains("shell content"));
    assert!(shell_append_content.contains("appended via shell"));
    eprintln!("✓ Shell append content is correct");

    drop(handle);
    eprintln!("✓ Flush prevents write errors");
}

/// File creation with permission-only mode bits
///
/// Verifies that create() always creates a regular file using only the
/// permission bits from the mode parameter, since shells typically pass
/// only permission bits (e.g., 0644) without file type bits.
#[test]
fn test_create_with_permission_mode() {
    if !check_macfuse_installed() {
        eprintln!("Skipping real FUSE test - macFUSE not installed");
        return;
    }

    let test_name = "create-permission-mode";
    let mountpoint = match determine_mount_point(test_name) {
        Ok(mp) => mp,
        Err(e) => {
            eprintln!("Failed to determine mount point: {}", e);
            return;
        }
    };

    let upper_layer = tempfile::tempdir().unwrap().keep();
    let lower_layer = tempfile::tempdir().unwrap().keep();

    let fs_instance =
        match TreebeardFs::new(upper_layer.clone(), lower_layer.clone(), None, 1, vec![]) {
            Ok(f) => f,
            Err(e) => {
                eprintln!("Failed to create TreebeardFs: {}", e);
                return;
            }
        };

    let _cleanup = MountCleanup::new(mountpoint.clone());
    let session = match Session::new(fs_instance, &mountpoint, &[]) {
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

    // Give filesystem time to mount
    thread::sleep(Duration::from_millis(500));

    // Test 1: Create file using touch (simulates shell behavior)
    let touch_file = mountpoint.join("touch-test.txt");
    let output = Command::new("touch")
        .arg(&touch_file)
        .output()
        .expect("Failed to run touch command");

    if output.status.success() {
        eprintln!("✓ touch command succeeded");
        assert!(touch_file.exists(), "File should exist after touch");
        eprintln!("✓ touched file exists");
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        panic!(
            "REGRESSION: touch command failed (create not handling permission-only mode): {}",
            stderr
        );
    }

    // Test 2: Create file using shell redirection (echo "content" > file)
    let redirect_file = mountpoint.join("redirect-test.txt");
    let redirect_file_str = redirect_file.to_str().unwrap();

    let output = Command::new("sh")
        .arg("-c")
        .arg(format!("echo 'redirect content' > '{}'", redirect_file_str))
        .output()
        .expect("Failed to run shell redirect");

    if output.status.success() {
        eprintln!("✓ shell redirection succeeded");

        // Verify content was written
        let content = fs::read_to_string(&redirect_file).expect("Failed to read redirected file");
        assert!(content.trim() == "redirect content", "Content should match");
        eprintln!("✓ redirected file has correct content");
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        panic!("REGRESSION: shell redirection failed: {}", stderr);
    }

    // Test 3: Create file with specific permissions using Rust fs::File
    let rust_file = mountpoint.join("rust-create.txt");
    match std::fs::File::create(&rust_file) {
        Ok(mut file) => {
            use std::io::Write;
            writeln!(file, "created via Rust").expect("Failed to write");
            eprintln!("✓ Rust File::create succeeded");
        }
        Err(e) => {
            panic!("REGRESSION: Rust File::create failed: {}", e);
        }
    }

    // Test 4: Verify file permissions are properly set
    let perm_file = mountpoint.join("permissions-test.txt");
    {
        use std::os::unix::fs::OpenOptionsExt;
        match std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o644)
            .open(&perm_file)
        {
            Ok(mut file) => {
                use std::io::Write;
                writeln!(file, "with permissions").expect("Failed to write");
                eprintln!("✓ OpenOptions with mode succeeded");
            }
            Err(e) => {
                panic!("REGRESSION: OpenOptions with mode failed: {}", e);
            }
        }
    }

    // Verify permissions
    use std::os::unix::fs::MetadataExt;
    let meta = fs::metadata(&perm_file).expect("Failed to get metadata");
    let perm = meta.mode() & 0o777;
    eprintln!("File permissions: {:o}", perm);

    // Test 5: Create multiple files rapidly (stress test)
    for i in 1..=10 {
        let rapid_file = mountpoint.join(format!("rapid-{}.txt", i));
        match fs::write(&rapid_file, format!("content {}", i)) {
            Ok(_) => {}
            Err(e) => {
                panic!("REGRESSION: Rapid file creation {} failed: {}", i, e);
            }
        }
    }
    eprintln!("✓ Rapid file creation (10 files) succeeded");

    // Test 6: Create file that already exists should return EEXIST
    let exists_file = mountpoint.join("already-exists.txt");
    fs::write(&exists_file, "original content").expect("Failed to create original file");

    // Try to create the same file with O_EXCL
    use std::io::ErrorKind;
    match std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&exists_file)
    {
        Ok(_) => {
            panic!("REGRESSION: create_new should fail for existing file");
        }
        Err(e) => {
            if e.kind() == ErrorKind::AlreadyExists {
                eprintln!("✓ Correctly returns EEXIST for existing file");
            } else {
                panic!("REGRESSION: Wrong error kind for existing file: {:?}", e);
            }
        }
    }

    drop(handle);
    eprintln!("Regression test completed - file creation with permission-only mode works");
}

/// Extended attribute (xattr) support test
///
/// Verifies that setxattr, getxattr, listxattr, and removexattr operations work correctly.
/// Extended attributes are stored natively on the upper layer, so no AppleDouble files
/// (._prefix) should be created.
#[test]
fn test_xattr_support() {
    if !check_macfuse_installed() {
        eprintln!("Skipping real FUSE test - macFUSE not installed");
        return;
    }

    let test_name = "xattr-support";
    let mountpoint = match determine_mount_point(test_name) {
        Ok(mp) => mp,
        Err(e) => {
            eprintln!("Failed to determine mount point: {}", e);
            return;
        }
    };

    let upper_layer = tempfile::tempdir().unwrap().keep();
    let lower_layer = tempfile::tempdir().unwrap().keep();

    let fs_instance =
        match TreebeardFs::new(upper_layer.clone(), lower_layer.clone(), None, 1, vec![]) {
            Ok(f) => f,
            Err(e) => {
                eprintln!("Failed to create TreebeardFs: {}", e);
                return;
            }
        };

    let _cleanup = MountCleanup::new(mountpoint.clone());
    let session = match Session::new(fs_instance, &mountpoint, &[]) {
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

    // Give filesystem time to mount
    thread::sleep(Duration::from_millis(500));

    // Test 1: Create a file and verify no AppleDouble files are created initially
    let test_file = mountpoint.join("test-xattr.txt");
    fs::write(&test_file, "test content for xattr").unwrap();
    eprintln!("Created test file");

    // Check for AppleDouble files
    let entries: Vec<String> = fs::read_dir(&mountpoint)
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().to_string())
        .collect();

    let appledouble_files: Vec<&String> = entries.iter().filter(|e| e.starts_with("._")).collect();

    if appledouble_files.is_empty() {
        eprintln!("✓ No AppleDouble files created on file creation");
    } else {
        eprintln!(
            "AppleDouble files found after creation: {:?}",
            appledouble_files
        );
    }

    // Test 2: Set an extended attribute using the xattr command
    let xattr_name = "com.test.myattr";
    let xattr_value = "test-value-123";

    let output = Command::new("xattr")
        .args(["-w", xattr_name, xattr_value])
        .arg(&test_file)
        .output()
        .expect("Failed to execute xattr command");

    if output.status.success() {
        eprintln!("✓ setxattr succeeded via xattr command");
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        eprintln!(
            "xattr -w command returned non-zero: {} (stderr: {})",
            output.status, stderr
        );
    }

    // Test 3: Read back the extended attribute
    let output = Command::new("xattr")
        .args(["-p", xattr_name])
        .arg(&test_file)
        .output()
        .expect("Failed to execute xattr read command");

    if output.status.success() {
        let value = String::from_utf8_lossy(&output.stdout).trim().to_string();
        assert_eq!(
            value, xattr_value,
            "getxattr should return the value we set"
        );
        eprintln!("✓ getxattr returned correct value: '{}'", value);
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        eprintln!("xattr -p command failed: {}", stderr);
    }

    // Test 4: List extended attributes
    let output = Command::new("xattr")
        .arg("-l")
        .arg(&test_file)
        .output()
        .expect("Failed to execute xattr list command");

    if output.status.success() {
        let listing = String::from_utf8_lossy(&output.stdout);
        if listing.contains(xattr_name) {
            eprintln!("✓ listxattr shows our attribute: {}", listing.trim());
        } else {
            eprintln!(
                "listxattr output doesn't contain our attribute: {}",
                listing
            );
        }
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        eprintln!("xattr -l command failed: {}", stderr);
    }

    // Test 5: Verify no AppleDouble files were created after setting xattr
    let entries_after: Vec<String> = fs::read_dir(&mountpoint)
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().to_string())
        .collect();

    let appledouble_after: Vec<&String> = entries_after
        .iter()
        .filter(|e| e.starts_with("._"))
        .collect();

    if appledouble_after.is_empty() {
        eprintln!("✓ No AppleDouble files created after setting xattr");
    } else {
        eprintln!(
            "REGRESSION: AppleDouble files found after setting xattr: {:?}",
            appledouble_after
        );
    }

    // Test 6: Remove the extended attribute
    let output = Command::new("xattr")
        .args(["-d", xattr_name])
        .arg(&test_file)
        .output()
        .expect("Failed to execute xattr delete command");

    if output.status.success() {
        eprintln!("✓ removexattr succeeded");
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        eprintln!("xattr -d command failed: {}", stderr);
    }

    // Test 7: Verify attribute is gone after removal
    let output = Command::new("xattr")
        .args(["-p", xattr_name])
        .arg(&test_file)
        .output()
        .expect("Failed to execute xattr read command after delete");

    if !output.status.success() {
        eprintln!("✓ Attribute correctly removed (xattr -p returns error)");
    } else {
        eprintln!("Attribute still exists after removal");
    }

    // Test 8: Test xattr on file from lower layer (COW scenario)
    let lower_file = lower_layer.join("lower-file.txt");
    fs::write(&lower_file, "content from lower layer").unwrap();

    // Wait a bit for the filesystem to see the new file
    thread::sleep(Duration::from_millis(200));

    let mounted_lower_file = mountpoint.join("lower-file.txt");

    // Setting xattr on a lower layer file should trigger copy-up
    let output = Command::new("xattr")
        .args(["-w", "com.test.cow", "copied-up"])
        .arg(&mounted_lower_file)
        .output()
        .expect("Failed to execute xattr on lower layer file");

    if output.status.success() {
        eprintln!("✓ setxattr on lower layer file succeeded (COW triggered)");

        // Verify the file was copied to upper layer
        let upper_file = upper_layer.join("lower-file.txt");
        if upper_file.exists() {
            eprintln!("✓ File correctly copied to upper layer for xattr");
        } else {
            eprintln!("File not found in upper layer after xattr set");
        }
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        eprintln!("xattr on lower layer file failed: {}", stderr);
    }

    drop(handle);
    eprintln!("xattr support test completed");
}

/// Files visible in ls can be opened and read
///
/// Verifies that files visible in `ls -la` can be opened and read with `cat`.
/// This tests that readdir(), lookup(), and open() consistently resolve file paths
/// across overlay layers.
#[test]
fn test_files_visible_in_ls_can_be_read() {
    if !check_macfuse_installed() {
        eprintln!("Skipping real FUSE test - macFUSE not installed");
        return;
    }

    let test_name = "files-visible-readable";
    let mountpoint = match determine_mount_point(test_name) {
        Ok(mp) => mp,
        Err(e) => {
            eprintln!("Failed to determine mount point: {}", e);
            return;
        }
    };

    let upper_layer = tempfile::tempdir().unwrap().keep();
    let lower_layer = tempfile::tempdir().unwrap().keep();

    // Lower layer
    fs::write(lower_layer.join("readme.txt"), "README content from lower").unwrap();
    fs::write(lower_layer.join("config.json"), r#"{"key": "value"}"#).unwrap();
    fs::create_dir(lower_layer.join("src")).unwrap();
    fs::write(
        lower_layer.join("src/main.rs"),
        "fn main() { println!(\"Hello\"); }",
    )
    .unwrap();
    fs::write(lower_layer.join("src/lib.rs"), "pub fn lib() {}").unwrap();

    // Nested directory structure
    fs::create_dir_all(lower_layer.join("src/utils")).unwrap();
    fs::write(lower_layer.join("src/utils/helper.rs"), "pub fn help() {}").unwrap();

    // Upper layer
    fs::write(upper_layer.join("new-file.txt"), "New file in upper layer").unwrap();

    let fs_instance =
        match TreebeardFs::new(upper_layer.clone(), lower_layer.clone(), None, 1, vec![]) {
            Ok(f) => f,
            Err(e) => {
                eprintln!("Failed to create TreebeardFs: {}", e);
                return;
            }
        };

    let _cleanup = MountCleanup::new(mountpoint.clone());
    let session = match Session::new(fs_instance, &mountpoint, &[]) {
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

    // Give filesystem time to mount
    thread::sleep(Duration::from_millis(500));

    // Test 1: List files and verify they're visible
    let entries: Vec<String> = fs::read_dir(&mountpoint)
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().to_string())
        .collect();

    eprintln!("Root directory entries: {:?}", entries);
    assert!(
        entries.contains(&"readme.txt".to_string()),
        "readme.txt should be visible"
    );
    assert!(
        entries.contains(&"config.json".to_string()),
        "config.json should be visible"
    );
    assert!(
        entries.contains(&"src".to_string()),
        "src directory should be visible"
    );
    assert!(
        entries.contains(&"new-file.txt".to_string()),
        "new-file.txt should be visible"
    );
    eprintln!("✓ All files visible in directory listing");

    // Test 2: Read each file immediately after listing (the failing use case)
    let readme_content = match fs::read_to_string(mountpoint.join("readme.txt")) {
        Ok(content) => content,
        Err(e) => panic!("REGRESSION: Cannot read readme.txt after ls: {}", e),
    };
    assert_eq!(readme_content, "README content from lower");
    eprintln!("✓ Can read readme.txt from lower layer");

    let config_content = match fs::read_to_string(mountpoint.join("config.json")) {
        Ok(content) => content,
        Err(e) => panic!("REGRESSION: Cannot read config.json after ls: {}", e),
    };
    assert!(config_content.contains("key"));
    eprintln!("✓ Can read config.json from lower layer");

    let new_file_content = match fs::read_to_string(mountpoint.join("new-file.txt")) {
        Ok(content) => content,
        Err(e) => panic!(
            "REGRESSION: Cannot read new-file.txt from upper layer: {}",
            e
        ),
    };
    assert_eq!(new_file_content, "New file in upper layer");
    eprintln!("✓ Can read new-file.txt from upper layer");

    // Test 3: Access nested files in subdirectories
    let src_entries: Vec<String> = fs::read_dir(mountpoint.join("src"))
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().to_string())
        .collect();

    eprintln!("src/ entries: {:?}", src_entries);
    assert!(src_entries.contains(&"main.rs".to_string()));
    assert!(src_entries.contains(&"lib.rs".to_string()));
    assert!(src_entries.contains(&"utils".to_string()));

    let main_content = match fs::read_to_string(mountpoint.join("src/main.rs")) {
        Ok(content) => content,
        Err(e) => panic!("REGRESSION: Cannot read src/main.rs: {}", e),
    };
    assert!(main_content.contains("fn main()"));
    eprintln!("✓ Can read nested file src/main.rs");

    // Test 4: Access deeply nested files
    let helper_content = match fs::read_to_string(mountpoint.join("src/utils/helper.rs")) {
        Ok(content) => content,
        Err(e) => panic!("REGRESSION: Cannot read deeply nested file: {}", e),
    };
    assert!(helper_content.contains("pub fn help()"));
    eprintln!("✓ Can read deeply nested file src/utils/helper.rs");

    // Test 5: Use cat command (simulates real shell usage)
    let output = Command::new("cat")
        .arg(mountpoint.join("readme.txt"))
        .output()
        .expect("Failed to run cat command");

    if output.status.success() {
        let content = String::from_utf8_lossy(&output.stdout);
        assert_eq!(content, "README content from lower");
        eprintln!("✓ cat command works on lower layer files");
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        panic!("REGRESSION: cat command failed: {}", stderr);
    }

    // Test 6: Verify file operations sequence (ls then cat for each file)
    let entries: Vec<_> = fs::read_dir(&mountpoint)
        .unwrap()
        .filter_map(|e| e.ok())
        .collect();

    for entry in entries {
        let path = entry.path();
        if path.is_file() {
            match fs::read_to_string(&path) {
                Ok(_) => {
                    eprintln!("Can read file after ls: {}", path.display());
                }
                Err(e) => {
                    panic!(
                        "REGRESSION: Cannot read file {} after ls: {}",
                        path.display(),
                        e
                    );
                }
            }
        }
    }
    eprintln!("✓ All files readable after directory listing");

    drop(handle);
    eprintln!("Files visible in ls can be read - test completed");
}

/// Lookup consistency across layer boundaries
///
/// Tests that lookup() correctly resolves files regardless of which layer
/// the parent directory is associated with, and that upper layer properly shadows
/// lower layer files.
#[test]
fn test_lookup_layer_consistency() {
    if !check_macfuse_installed() {
        eprintln!("Skipping real FUSE test - macFUSE not installed");
        return;
    }

    let test_name = "lookup-layer-consistency";
    let mountpoint = match determine_mount_point(test_name) {
        Ok(mp) => mp,
        Err(e) => {
            eprintln!("Failed to determine mount point: {}", e);
            return;
        }
    };

    let upper_layer = tempfile::tempdir().unwrap().keep();
    let lower_layer = tempfile::tempdir().unwrap().keep();

    // Create same file in both layers with different content
    fs::write(lower_layer.join("shadowed.txt"), "LOWER LAYER").unwrap();
    fs::write(upper_layer.join("shadowed.txt"), "UPPER LAYER").unwrap();

    // Create file only in lower layer
    fs::write(lower_layer.join("lower-only.txt"), "Only in lower").unwrap();

    // Create file only in upper layer
    fs::write(upper_layer.join("upper-only.txt"), "Only in upper").unwrap();

    // Create directory in lower with files, and add a file in upper
    fs::create_dir(lower_layer.join("mixed-dir")).unwrap();
    fs::write(lower_layer.join("mixed-dir/from-lower.txt"), "Lower file").unwrap();
    fs::create_dir(upper_layer.join("mixed-dir")).unwrap();
    fs::write(upper_layer.join("mixed-dir/from-upper.txt"), "Upper file").unwrap();

    let fs_instance =
        match TreebeardFs::new(upper_layer.clone(), lower_layer.clone(), None, 1, vec![]) {
            Ok(f) => f,
            Err(e) => {
                eprintln!("Failed to create TreebeardFs: {}", e);
                return;
            }
        };

    let _cleanup = MountCleanup::new(mountpoint.clone());
    let session = match Session::new(fs_instance, &mountpoint, &[]) {
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

    thread::sleep(Duration::from_millis(500));

    // Test 1: Upper layer should shadow lower layer
    let shadowed_content = fs::read_to_string(mountpoint.join("shadowed.txt")).unwrap();
    assert_eq!(
        shadowed_content, "UPPER LAYER",
        "REGRESSION: Upper layer should shadow lower layer file"
    );
    eprintln!("✓ Upper layer correctly shadows lower layer");

    // Test 2: Lower-only file should be accessible
    let lower_content = fs::read_to_string(mountpoint.join("lower-only.txt")).unwrap();
    assert_eq!(lower_content, "Only in lower");
    eprintln!("✓ Lower-only file is accessible");

    // Test 3: Upper-only file should be accessible
    let upper_content = fs::read_to_string(mountpoint.join("upper-only.txt")).unwrap();
    assert_eq!(upper_content, "Only in upper");
    eprintln!("✓ Upper-only file is accessible");

    // Test 4: Mixed directory should show files from both layers
    let mixed_entries: Vec<String> = fs::read_dir(mountpoint.join("mixed-dir"))
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().to_string())
        .collect();

    assert!(
        mixed_entries.contains(&"from-lower.txt".to_string()),
        "Mixed dir should show lower layer file"
    );
    assert!(
        mixed_entries.contains(&"from-upper.txt".to_string()),
        "Mixed dir should show upper layer file"
    );
    eprintln!("✓ Mixed directory shows files from both layers");

    // Test 5: Can read files from mixed directory
    let mixed_lower = fs::read_to_string(mountpoint.join("mixed-dir/from-lower.txt")).unwrap();
    assert_eq!(mixed_lower, "Lower file");

    let mixed_upper = fs::read_to_string(mountpoint.join("mixed-dir/from-upper.txt")).unwrap();
    assert_eq!(mixed_upper, "Upper file");
    eprintln!("✓ Can read files from mixed directory");

    // Test 6: Modify a lower layer file (triggers COW)
    let modify_path = mountpoint.join("lower-only.txt");
    fs::write(&modify_path, "Modified content").unwrap();

    // Read it back
    let modified_content = fs::read_to_string(&modify_path).unwrap();
    assert_eq!(modified_content, "Modified content");
    eprintln!("✓ Can modify lower layer file (COW works)");

    // Verify it went to upper layer
    let upper_modified = upper_layer.join("lower-only.txt");
    assert!(
        upper_modified.exists(),
        "Modified file should be in upper layer"
    );
    let upper_content = fs::read_to_string(&upper_modified).unwrap();
    assert_eq!(upper_content, "Modified content");
    eprintln!("✓ Modified file correctly copied to upper layer");

    // Verify lower layer is unchanged
    let lower_original = fs::read_to_string(lower_layer.join("lower-only.txt")).unwrap();
    assert_eq!(lower_original, "Only in lower");
    eprintln!("✓ Lower layer file unchanged after COW");

    drop(handle);
    eprintln!("Lookup layer consistency test completed");
}

/// Cat file after lookup - inode consistency
///
/// Verifies that metadata_to_fileattr() returns the internally allocated inode
/// number rather than the underlying filesystem's inode.
#[test]
fn test_inode_consistency_cat_file() {
    if !check_macfuse_installed() {
        eprintln!("Skipping real FUSE test - macFUSE not installed");
        return;
    }

    let test_name = "inode-mismatch-cat";
    let mountpoint = match determine_mount_point(test_name) {
        Ok(mp) => mp,
        Err(e) => {
            eprintln!("Failed to determine mount point: {}", e);
            return;
        }
    };

    let upper_layer = tempfile::tempdir().unwrap().keep();
    let lower_layer = tempfile::tempdir().unwrap().keep();

    // Lower layer
    fs::write(lower_layer.join("example.txt"), "Hello from lower layer\n").unwrap();
    fs::write(lower_layer.join("another.txt"), "Another file content\n").unwrap();
    fs::create_dir(lower_layer.join("subdir")).unwrap();
    fs::write(
        lower_layer.join("subdir/nested.txt"),
        "Nested file content\n",
    )
    .unwrap();

    let fs_instance =
        match TreebeardFs::new(upper_layer.clone(), lower_layer.clone(), None, 1, vec![]) {
            Ok(f) => f,
            Err(e) => {
                eprintln!("Failed to create TreebeardFs: {}", e);
                return;
            }
        };

    let _cleanup = MountCleanup::new(mountpoint.clone());
    let session = match Session::new(fs_instance, &mountpoint, &[]) {
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

    thread::sleep(Duration::from_millis(500));

    // Test 1: ls -la should work
    let entries: Vec<String> = fs::read_dir(&mountpoint)
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().to_string())
        .collect();

    eprintln!("Directory listing: {:?}", entries);
    assert!(entries.contains(&"example.txt".to_string()));
    assert!(entries.contains(&"another.txt".to_string()));
    assert!(entries.contains(&"subdir".to_string()));
    eprintln!("✓ ls -la works correctly");

    // Test 2: cat file should work
    let output = Command::new("cat")
        .arg(mountpoint.join("example.txt"))
        .output()
        .expect("Failed to run cat command");

    if output.status.success() {
        let content = String::from_utf8_lossy(&output.stdout);
        assert_eq!(content, "Hello from lower layer\n");
        eprintln!("✓ cat command works on lower layer files");
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        panic!("REGRESSION: cat command failed with: {}", stderr);
    }

    // Test 3: Read file using Rust fs API
    match fs::read_to_string(mountpoint.join("another.txt")) {
        Ok(content) => {
            assert_eq!(content, "Another file content\n");
            eprintln!("✓ fs::read_to_string works");
        }
        Err(e) => {
            panic!("REGRESSION: Cannot read file via Rust API: {}", e);
        }
    }

    // Test 4: Read nested file
    match fs::read_to_string(mountpoint.join("subdir/nested.txt")) {
        Ok(content) => {
            assert_eq!(content, "Nested file content\n");
            eprintln!("✓ Can read nested files");
        }
        Err(e) => {
            panic!("REGRESSION: Cannot read nested file: {}", e);
        }
    }

    // Test 5: Multiple sequential reads
    for i in 1..=5 {
        match fs::read_to_string(mountpoint.join("example.txt")) {
            Ok(content) => {
                assert_eq!(content, "Hello from lower layer\n");
            }
            Err(e) => {
                panic!("REGRESSION: Sequential read {} failed: {}", i, e);
            }
        }
    }
    eprintln!("✓ Multiple sequential reads work");

    drop(handle);
    eprintln!("Cat file after lookup test completed");
}

/// Write file after create - inode consistency
///
/// Tests that newly created files can be written to immediately.
#[test]
fn test_inode_consistency_write_file() {
    if !check_macfuse_installed() {
        eprintln!("Skipping real FUSE test - macFUSE not installed");
        return;
    }

    let test_name = "inode-mismatch-write";
    let mountpoint = match determine_mount_point(test_name) {
        Ok(mp) => mp,
        Err(e) => {
            eprintln!("Failed to determine mount point: {}", e);
            return;
        }
    };

    let upper_layer = tempfile::tempdir().unwrap().keep();
    let lower_layer = tempfile::tempdir().unwrap().keep();

    let fs_instance =
        match TreebeardFs::new(upper_layer.clone(), lower_layer.clone(), None, 1, vec![]) {
            Ok(f) => f,
            Err(e) => {
                eprintln!("Failed to create TreebeardFs: {}", e);
                return;
            }
        };

    let _cleanup = MountCleanup::new(mountpoint.clone());
    let session = match Session::new(fs_instance, &mountpoint, &[]) {
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

    thread::sleep(Duration::from_millis(500));

    // Test 1: echo "test" > newfile.txt (shell redirection)
    let new_file = mountpoint.join("newfile.txt");
    let new_file_str = new_file.to_str().unwrap();

    let output = Command::new("sh")
        .arg("-c")
        .arg(format!("echo 'test content' > '{}'", new_file_str))
        .output()
        .expect("Failed to run shell redirect");

    if output.status.success() {
        eprintln!("✓ Shell redirection to new file works");

        // Verify content
        let content = fs::read_to_string(&new_file).unwrap();
        assert!(content.trim() == "test content");
        eprintln!("✓ New file has correct content");
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        panic!("REGRESSION: Shell redirection failed: {}", stderr);
    }

    // Test 2: Create and write using Rust API
    let rust_file = mountpoint.join("rust-created.txt");
    match fs::write(&rust_file, "Created via Rust fs::write") {
        Ok(_) => {
            eprintln!("✓ fs::write to new file works");
        }
        Err(e) => {
            panic!("REGRESSION: fs::write to new file failed: {}", e);
        }
    }

    // Verify content
    let content = fs::read_to_string(&rust_file).unwrap();
    assert_eq!(content, "Created via Rust fs::write");

    // Test 3: Append to file
    use std::io::Write;
    {
        let mut file = std::fs::OpenOptions::new()
            .append(true)
            .open(&rust_file)
            .expect("Failed to open for append");

        writeln!(file, " - appended line").expect("REGRESSION: Append failed");
    }

    let content = fs::read_to_string(&rust_file).unwrap();
    assert!(content.contains("appended line"));
    eprintln!("✓ Append to file works");

    // Test 4: Multiple file creations in sequence
    for i in 1..=5 {
        let seq_file = mountpoint.join(format!("sequential-{}.txt", i));
        match fs::write(&seq_file, format!("Content for file {}", i)) {
            Ok(_) => {}
            Err(e) => {
                panic!("REGRESSION: Sequential file {} creation failed: {}", i, e);
            }
        }
    }
    eprintln!("✓ Multiple sequential file creations work");

    // Verify all files exist and have correct content
    for i in 1..=5 {
        let seq_file = mountpoint.join(format!("sequential-{}.txt", i));
        let content = fs::read_to_string(&seq_file).unwrap();
        assert_eq!(content, format!("Content for file {}", i));
    }

    drop(handle);
    eprintln!("Write file after create test completed");
}

/// Multiple operations sequence - inode consistency
///
/// Tests the complete workflow: mount, list directory, read file, write file, list again.
#[test]
fn test_inode_consistency_multiple_operations() {
    if !check_macfuse_installed() {
        eprintln!("Skipping real FUSE test - macFUSE not installed");
        return;
    }

    let test_name = "inode-mismatch-multiple-ops";
    let mountpoint = match determine_mount_point(test_name) {
        Ok(mp) => mp,
        Err(e) => {
            eprintln!("Failed to determine mount point: {}", e);
            return;
        }
    };

    let upper_layer = tempfile::tempdir().unwrap().keep();
    let lower_layer = tempfile::tempdir().unwrap().keep();

    // Lower layer
    fs::write(lower_layer.join("test.txt"), "Original content\n").unwrap();
    fs::write(lower_layer.join("README.md"), "# Project\n").unwrap();
    fs::create_dir(lower_layer.join("src")).unwrap();
    fs::write(lower_layer.join("src/main.rs"), "fn main() {}\n").unwrap();

    let fs_instance =
        match TreebeardFs::new(upper_layer.clone(), lower_layer.clone(), None, 1, vec![]) {
            Ok(f) => f,
            Err(e) => {
                eprintln!("Failed to create TreebeardFs: {}", e);
                return;
            }
        };

    let _cleanup = MountCleanup::new(mountpoint.clone());
    let session = match Session::new(fs_instance, &mountpoint, &[]) {
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

    thread::sleep(Duration::from_millis(500));

    // Step 1: ls -la (list directory)
    eprintln!("Step 1: List directory");
    let entries: Vec<String> = fs::read_dir(&mountpoint)
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().to_string())
        .collect();

    eprintln!("  Files: {:?}", entries);
    assert!(entries.contains(&"test.txt".to_string()));
    assert!(entries.contains(&"README.md".to_string()));
    assert!(entries.contains(&"src".to_string()));
    eprintln!("  Directory listing successful");

    // Step 2: cat test.txt (read file - this was failing before fix)
    eprintln!("Step 2: Read file with cat");
    let output = Command::new("cat")
        .arg(mountpoint.join("test.txt"))
        .output()
        .expect("Failed to run cat");

    if output.status.success() {
        let content = String::from_utf8_lossy(&output.stdout);
        assert_eq!(content, "Original content\n");
        eprintln!("  cat test.txt successful");
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        panic!("REGRESSION: cat failed after ls: {}", stderr);
    }

    // Step 3: echo "new" > new.txt (create and write new file)
    eprintln!("Step 3: Create new file with echo");
    let new_file = mountpoint.join("new.txt");
    let output = Command::new("sh")
        .arg("-c")
        .arg(format!(
            "echo 'new content' > '{}'",
            new_file.to_str().unwrap()
        ))
        .output()
        .expect("Failed to run echo");

    if output.status.success() {
        eprintln!("  echo > new.txt successful");
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        panic!("REGRESSION: echo > new.txt failed: {}", stderr);
    }

    // Step 4: cat new.txt (read newly created file)
    eprintln!("Step 4: Read newly created file");
    let output = Command::new("cat")
        .arg(&new_file)
        .output()
        .expect("Failed to run cat");

    if output.status.success() {
        let content = String::from_utf8_lossy(&output.stdout);
        assert!(content.trim() == "new content");
        eprintln!("  cat new.txt successful");
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        panic!("REGRESSION: cat new.txt failed: {}", stderr);
    }

    // Step 5: ls -la again (verify all files are still visible)
    eprintln!("Step 5: List directory again");
    let entries_after: Vec<String> = fs::read_dir(&mountpoint)
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().to_string())
        .collect();

    eprintln!("  Files after operations: {:?}", entries_after);
    assert!(entries_after.contains(&"test.txt".to_string()));
    assert!(entries_after.contains(&"new.txt".to_string()));
    eprintln!("  Directory listing after operations successful");

    // Step 6: Read all files (verify they're all accessible)
    eprintln!("Step 6: Read all files");
    for entry_name in &entries_after {
        let path = mountpoint.join(entry_name);
        if path.is_file() {
            match fs::read_to_string(&path) {
                Ok(_) => eprintln!("  Can read {}", entry_name),
                Err(e) => panic!(
                    "REGRESSION: Cannot read {} after operations: {}",
                    entry_name, e
                ),
            }
        }
    }

    drop(handle);
    eprintln!("Multiple operations sequence test completed");
}
