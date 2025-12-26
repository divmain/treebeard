use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::config::get_mount_dir;
use crate::error::{Result, TreebeardError};
use crate::overlay::types::MutationTracker;
use crate::overlay::TreebeardFs;

/// Mount the FUSE filesystem in a background thread.
/// Returns:
/// - MutationTracker for checking mutations on cleanup
/// - Receiver for mutation events to trigger commits
///
/// # Arguments
/// * `mount_point` - Directory where the FUSE filesystem will be mounted
/// * `upper_layer` - Directory for the upper (writable) layer
/// * `lower_layer` - Directory for the lower (read-only) layer  
/// * `ttl_secs` - Cache TTL in seconds for FUSE attributes and entries
pub fn mount_fuse(
    mount_point: &Path,
    upper_layer: &Path,
    lower_layer: &Path,
    ttl_secs: u64,
    passthrough_patterns: Vec<String>,
) -> crate::error::Result<(
    MutationTracker,
    tokio::sync::mpsc::UnboundedReceiver<PathBuf>,
)> {
    fs::create_dir_all(mount_point).map_err(|e| {
        TreebeardError::Config(format!(
            "Failed to create mount directory {}: {}",
            mount_point.display(),
            e
        ))
    })?;

    let (tx, rx) = tokio::sync::mpsc::unbounded_channel();

    let fs = TreebeardFs::new(
        upper_layer.to_path_buf(),
        lower_layer.to_path_buf(),
        Some(tx),
        ttl_secs,
        passthrough_patterns,
    )?;

    let mutations = Arc::clone(&fs.mutations);
    let mount_point_clone = mount_point.to_path_buf();

    // Channel to communicate mount status from the spawned thread back to the main thread.
    // This ensures we don't return Ok(...) if the mount actually failed.
    let (mount_status_tx, mount_status_rx) =
        std::sync::mpsc::channel::<std::result::Result<(), String>>();

    std::thread::spawn(move || {
        let mount_options = vec![
            fuser::MountOption::FSName("treebeard".to_string()),
            fuser::MountOption::AutoUnmount,
        ];

        tracing::info!(
            "Mounting FUSE filesystem at {}",
            mount_point_clone.display()
        );

        match fuser::mount2(fs, &mount_point_clone, &mount_options) {
            Ok(_) => {
                // Mount succeeded and then was unmounted (normal shutdown).
                // By the time we get here, the mount was working, so we signal success
                // before proceeding to unmount.
                tracing::info!("FUSE filesystem unmounted");
            }
            Err(e) => {
                tracing::error!("FUSE mount error: {}", e);
                // Signal mount failure to the main thread
                let _ = mount_status_tx.send(Err(e.to_string()));
            }
        }
    });

    // Wait for either mount success (verified by mount point being accessible)
    // or mount failure (signaled via channel).
    // We use a loop with short sleeps to check both conditions.
    let mount_timeout = std::time::Duration::from_millis(2000);
    let check_interval = std::time::Duration::from_millis(50);
    let start = std::time::Instant::now();

    loop {
        // Check if the mount thread reported an error
        match mount_status_rx.try_recv() {
            Ok(Err(e)) => {
                return Err(crate::error::TreebeardError::Fuse(format!(
                    "FUSE mount failed: {}",
                    e
                )));
            }
            Ok(Ok(())) => {
                // This shouldn't happen during mount, but handle it gracefully
                break;
            }
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                // Channel disconnected without error means mount succeeded and is running
                // (or the thread panicked, but we can't distinguish that easily)
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => {
                // No message yet, continue checking
            }
        }

        // Check if the mount point is accessible (indicates successful mount)
        // We check if we can read the directory, which requires the FUSE to be mounted
        if mount_point.is_dir() && fs::read_dir(mount_point).is_ok() {
            tracing::debug!("FUSE mount verified accessible");
            break;
        }

        // Check timeout
        if start.elapsed() >= mount_timeout {
            return Err(crate::error::TreebeardError::Fuse(
                "FUSE mount timed out - mount point not accessible after 2 seconds".to_string(),
            ));
        }

        std::thread::sleep(check_interval);
    }

    Ok((mutations, rx))
}

/// Validates that a mount path is within the expected treebeard mount directory.
///
/// This prevents path traversal attacks where a malicious user could manipulate
/// the mount path via config to unmount arbitrary filesystem locations.
///
/// # Security
/// This function performs path validation to prevent unmounting filesystems that
/// are not managed by treebeard.
pub fn validate_mount_path(mount_path: &Path) -> Result<()> {
    let expected_mount_dir = get_mount_dir().map_err(|e| {
        TreebeardError::Config(format!(
            "Failed to get mount directory for validation: {}",
            e
        ))
    })?;

    let canonical_expected = expected_mount_dir.canonicalize().map_err(|e| {
        TreebeardError::Config(format!(
            "Failed to resolve expected mount directory {}: {}",
            expected_mount_dir.display(),
            e
        ))
    })?;

    let canonical_mount = mount_path.canonicalize().map_err(|e| {
        TreebeardError::Config(format!(
            "Failed to resolve mount path {}: {}",
            mount_path.display(),
            e
        ))
    })?;

    if !canonical_mount.starts_with(&canonical_expected) {
        return Err(TreebeardError::Config(format!(
            "Mount path validation failed: {} is not within treebeard mount directory {}",
            canonical_mount.display(),
            canonical_expected.display()
        )));
    }

    Ok(())
}

/// Unmount a FUSE filesystem at the given path.
///
/// This function handles platform-specific unmount commands and validates
/// that the mount path is within the expected treebeard directory.
///
/// Returns Ok(true) if unmount succeeded, Ok(false) if it may already be unmounted,
/// or an error if validation fails.
pub fn unmount_fuse(mount_path: &Path) -> Result<bool> {
    validate_mount_path(mount_path)?;

    let unmount_result = if cfg!(target_os = "macos") {
        std::process::Command::new("diskutil")
            .args(["unmount", "force", mount_path.to_str().unwrap()])
            .status()
    } else {
        std::process::Command::new("umount")
            .arg(mount_path)
            .status()
    };

    match unmount_result {
        Ok(status) => Ok(status.success()),
        Err(e) => {
            eprintln!("Warning: Failed to run unmount command: {}", e);
            Ok(false)
        }
    }
}

/// Result of a FUSE cleanup operation.
pub struct FuseCleanupResult {
    /// Whether the unmount succeeded.
    pub unmount_succeeded: bool,
    /// Whether the mount directory was removed.
    pub directory_removed: bool,
}

/// Unmount a FUSE filesystem and remove its mount directory.
///
/// This is the canonical way to clean up a FUSE mount. It handles:
/// - Unmounting the FUSE filesystem with platform-specific commands
/// - Removing the mount directory after successful unmount
/// - Validation that the path is within treebeard's mount directory
///
/// Returns a `FuseCleanupResult` indicating what operations succeeded.
/// Errors are logged but don't cause the function to fail - cleanup is best-effort.
pub fn perform_fuse_cleanup(mount_path: &Path) -> FuseCleanupResult {
    let unmount_succeeded = match unmount_fuse(mount_path) {
        Ok(true) => true,
        Ok(false) => {
            tracing::warn!(
                "Failed to unmount {} (may already be unmounted)",
                mount_path.display()
            );
            false
        }
        Err(e) => {
            tracing::warn!("Failed to unmount {}: {}", mount_path.display(), e);
            false
        }
    };

    // Only attempt directory removal if unmount succeeded
    let directory_removed = if unmount_succeeded && mount_path.exists() {
        match std::fs::remove_dir_all(mount_path) {
            Ok(_) => true,
            Err(e) => {
                tracing::warn!(
                    "Failed to remove mount directory {}: {}",
                    mount_path.display(),
                    e
                );
                false
            }
        }
    } else {
        false
    };

    FuseCleanupResult {
        unmount_succeeded,
        directory_removed,
    }
}

/// Clean up stale FUSE mounts from crashed sessions.
///
/// This function checks for existing FUSE mounts that may have been left behind
/// when treebeard crashed or was killed without proper cleanup. macFUSE has a
/// limit of 64 simultaneous mounts, so stale mounts can prevent new mounts from
/// being created.
///
/// This is called by the `treebeard cleanup --stale` command and logs what it does,
/// but doesn't fail if cleanup encounters errors. Users can skip this with the
/// TREEBEARD_NO_CLEANUP=1 env var.
pub fn cleanup_stale_mounts() {
    // Allow users to skip automatic cleanup
    if std::env::var("TREEBEARD_NO_CLEANUP").is_ok() {
        tracing::debug!("Skipping stale mount cleanup (TREEBEARD_NO_CLEANUP=1)");
        return;
    }

    if cfg!(not(target_os = "macos")) {
        return;
    }

    tracing::debug!("Checking for stale FUSE mounts...");

    // Get list of mounts
    let mount_output = match std::process::Command::new("mount").output() {
        Ok(output) => output,
        Err(e) => {
            tracing::warn!("Failed to run mount command: {}", e);
            return;
        }
    };

    let mount_text = String::from_utf8_lossy(&mount_output.stdout);
    let mut stale_mounts = Vec::new();
    let mount_regex = regex::Regex::new(r"/dev/\S+ on (\S+) \(.*treebeard.*\)").unwrap();

    // Find treebeard mounts using regex
    for line in mount_text.lines() {
        if let Some(captures) = mount_regex.captures(line) {
            if let Some(mount_path) = captures.get(1) {
                let path = mount_path.as_str();
                tracing::debug!("Found treebeard mount: {}", path);
                stale_mounts.push(path.to_string());
            }
        }
    }

    if stale_mounts.is_empty() {
        tracing::debug!("No stale treebeard mounts found");
        return;
    }

    tracing::info!("Found {} stale treebeard mount(s)", stale_mounts.len());

    // Try to unmount each stale mount
    for mount_path in &stale_mounts {
        tracing::info!("Attempting to unmount stale mount: {}", mount_path);

        let mount_path_buf = PathBuf::from(mount_path);

        // Use perform_fuse_cleanup which handles validation, unmount, and directory removal
        let result = perform_fuse_cleanup(&mount_path_buf);
        if result.unmount_succeeded {
            tracing::info!("Successfully unmounted: {}", mount_path);
        } else {
            tracing::warn!("Failed to unmount stale mount: {}", mount_path);
        }
    }
}
