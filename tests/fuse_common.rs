#![cfg(target_os = "macos")]

use std::path::Path;
use std::process::Command;
use std::thread;
use std::time::Duration;

/// Check if macFUSE is installed
pub fn check_macfuse_installed() -> bool {
    Path::new("/Library/Filesystems/macfuse.fs").exists()
}

/// Get macOS major version
#[allow(dead_code)]
pub fn get_macos_major_version() -> Option<u32> {
    let output = Command::new("sw_vers")
        .args(["-productVersion"])
        .output()
        .ok()?;

    let version_str = String::from_utf8_lossy(&output.stdout);
    let major_version = version_str.split('.').next()?.parse().ok()?;
    Some(major_version)
}

/// Determine mount point for FUSE tests
pub fn determine_mount_point(test_name: &str) -> Result<std::path::PathBuf, String> {
    let temp_dir = std::env::temp_dir();
    let mountpoint = temp_dir.join(format!("treebeard-{}", test_name));
    std::fs::create_dir_all(&mountpoint)
        .map_err(|e| format!("Failed to create mount point: {}", e))?;
    Ok(mountpoint)
}

/// Mount cleanup struct with RAII
pub struct MountCleanup {
    pub mountpoint: std::path::PathBuf,
}

impl MountCleanup {
    pub fn new(mountpoint: std::path::PathBuf) -> Self {
        Self { mountpoint }
    }

    pub fn unmount(&self) {
        let _ = Command::new("diskutil")
            .args(["unmount", "force", self.mountpoint.to_str().unwrap()])
            .output();

        let _ = Command::new("umount").arg(&self.mountpoint).output();

        thread::sleep(Duration::from_millis(500));
    }
}

impl Drop for MountCleanup {
    fn drop(&mut self) {
        self.unmount();

        thread::sleep(Duration::from_millis(200));
        let _ = std::fs::remove_dir_all(&self.mountpoint);
    }
}

#[allow(dead_code)]
pub const TEST_SETUP_DELAY_MS: u64 = 500;

use fuser::{BackgroundSession, Session};
use std::path::PathBuf;
use treebeard::overlay::TreebeardFs;

/// A test session that manages the FUSE mount lifecycle.
/// Holds temporary directories and handles cleanup on drop.
#[allow(dead_code)]
pub struct FuseTestSession {
    pub mountpoint: PathBuf,
    pub upper_layer: PathBuf,
    pub lower_layer: PathBuf,
    #[allow(dead_code)]
    pub handle: BackgroundSession,
    _cleanup: MountCleanup,
    _upper_dir: tempfile::TempDir,
    _lower_dir: tempfile::TempDir,
}

#[allow(dead_code)]
impl FuseTestSession {
    /// Create a new FUSE test session with the given test name.
    /// Returns None if macFUSE is not installed or setup fails.
    pub fn new(test_name: &str) -> Option<Self> {
        if !check_macfuse_installed() {
            eprintln!("Skipping real FUSE test - macFUSE not installed");
            return None;
        }

        let mountpoint = match determine_mount_point(test_name) {
            Ok(mp) => mp,
            Err(e) => {
                eprintln!("Failed to determine mount point: {}", e);
                return None;
            }
        };

        let upper_dir = tempfile::tempdir().ok()?;
        let lower_dir = tempfile::tempdir().ok()?;
        let upper_layer = upper_dir.path().to_path_buf();
        let lower_layer = lower_dir.path().to_path_buf();

        let fs_instance =
            match TreebeardFs::new(upper_layer.clone(), lower_layer.clone(), None, 1, vec![]) {
                Ok(f) => f,
                Err(e) => {
                    eprintln!("Failed to create TreebeardFs: {}", e);
                    return None;
                }
            };

        let cleanup = MountCleanup::new(mountpoint.clone());
        let session = match Session::new(fs_instance, &mountpoint, &[]) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("Failed to create FUSE session: {}", e);
                return None;
            }
        };

        let handle = match session.spawn() {
            Ok(h) => h,
            Err(e) => {
                eprintln!("Failed to spawn FUSE session: {}", e);
                return None;
            }
        };

        // Give filesystem time to mount
        thread::sleep(Duration::from_millis(TEST_SETUP_DELAY_MS));

        Some(FuseTestSession {
            mountpoint,
            upper_layer,
            lower_layer,
            handle,
            _cleanup: cleanup,
            _upper_dir: upper_dir,
            _lower_dir: lower_dir,
        })
    }

    /// Create a new FUSE test session with pre-populated lower layer files.
    /// The setup function receives the lower layer path and can create files there.
    pub fn with_lower_layer_setup<F>(test_name: &str, setup: F) -> Option<Self>
    where
        F: FnOnce(&Path),
    {
        if !check_macfuse_installed() {
            eprintln!("Skipping real FUSE test - macFUSE not installed");
            return None;
        }

        let mountpoint = match determine_mount_point(test_name) {
            Ok(mp) => mp,
            Err(e) => {
                eprintln!("Failed to determine mount point: {}", e);
                return None;
            }
        };

        let upper_dir = tempfile::tempdir().ok()?;
        let lower_dir = tempfile::tempdir().ok()?;
        let upper_layer = upper_dir.path().to_path_buf();
        let lower_layer = lower_dir.path().to_path_buf();

        // Run setup function to populate lower layer
        setup(&lower_layer);

        let fs_instance =
            match TreebeardFs::new(upper_layer.clone(), lower_layer.clone(), None, 1, vec![]) {
                Ok(f) => f,
                Err(e) => {
                    eprintln!("Failed to create TreebeardFs: {}", e);
                    return None;
                }
            };

        let cleanup = MountCleanup::new(mountpoint.clone());
        let session = match Session::new(fs_instance, &mountpoint, &[]) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("Failed to create FUSE session: {}", e);
                return None;
            }
        };

        let handle = match session.spawn() {
            Ok(h) => h,
            Err(e) => {
                eprintln!("Failed to spawn FUSE session: {}", e);
                return None;
            }
        };

        // Give filesystem time to mount
        thread::sleep(Duration::from_millis(TEST_SETUP_DELAY_MS));

        Some(FuseTestSession {
            mountpoint,
            upper_layer,
            lower_layer,
            handle,
            _cleanup: cleanup,
            _upper_dir: upper_dir,
            _lower_dir: lower_dir,
        })
    }

    /// Create a new FUSE test session with pre-populated upper and lower layer files.
    /// The setup functions receive the respective layer paths and can create files there.
    pub fn with_both_layers_setup<F, G>(
        test_name: &str,
        lower_setup: F,
        upper_setup: G,
    ) -> Option<Self>
    where
        F: FnOnce(&Path),
        G: FnOnce(&Path),
    {
        if !check_macfuse_installed() {
            eprintln!("Skipping real FUSE test - macFUSE not installed");
            return None;
        }

        let mountpoint = match determine_mount_point(test_name) {
            Ok(mp) => mp,
            Err(e) => {
                eprintln!("Failed to determine mount point: {}", e);
                return None;
            }
        };

        let upper_dir = tempfile::tempdir().ok()?;
        let lower_dir = tempfile::tempdir().ok()?;
        let upper_layer = upper_dir.path().to_path_buf();
        let lower_layer = lower_dir.path().to_path_buf();

        // Run setup functions to populate layers
        lower_setup(&lower_layer);
        upper_setup(&upper_layer);

        let fs_instance =
            match TreebeardFs::new(upper_layer.clone(), lower_layer.clone(), None, 1, vec![]) {
                Ok(f) => f,
                Err(e) => {
                    eprintln!("Failed to create TreebeardFs: {}", e);
                    return None;
                }
            };

        let cleanup = MountCleanup::new(mountpoint.clone());
        let session = match Session::new(fs_instance, &mountpoint, &[]) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("Failed to create FUSE session: {}", e);
                return None;
            }
        };

        let handle = match session.spawn() {
            Ok(h) => h,
            Err(e) => {
                eprintln!("Failed to spawn FUSE session: {}", e);
                return None;
            }
        };

        // Give filesystem time to mount
        thread::sleep(Duration::from_millis(TEST_SETUP_DELAY_MS));

        Some(FuseTestSession {
            mountpoint,
            upper_layer,
            lower_layer,
            handle,
            _cleanup: cleanup,
            _upper_dir: upper_dir,
            _lower_dir: lower_dir,
        })
    }

    /// Create a new FUSE test session with custom passthrough patterns.
    /// Returns None if macFUSE is not installed or setup fails.
    pub fn with_passthrough(test_name: &str, passthrough: Vec<String>) -> Option<Self> {
        if !check_macfuse_installed() {
            eprintln!("Skipping real FUSE test - macFUSE not installed");
            return None;
        }

        let mountpoint = match determine_mount_point(test_name) {
            Ok(mp) => mp,
            Err(e) => {
                eprintln!("Failed to determine mount point: {}", e);
                return None;
            }
        };

        let upper_dir = tempfile::tempdir().ok()?;
        let lower_dir = tempfile::tempdir().ok()?;
        let upper_layer = upper_dir.path().to_path_buf();
        let lower_layer = lower_dir.path().to_path_buf();

        let fs_instance = match TreebeardFs::new(
            upper_layer.clone(),
            lower_layer.clone(),
            None,
            1,
            passthrough,
        ) {
            Ok(f) => f,
            Err(e) => {
                eprintln!("Failed to create TreebeardFs: {}", e);
                return None;
            }
        };

        let cleanup = MountCleanup::new(mountpoint.clone());
        let session = match Session::new(fs_instance, &mountpoint, &[]) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("Failed to create FUSE session: {}", e);
                return None;
            }
        };

        let handle = match session.spawn() {
            Ok(h) => h,
            Err(e) => {
                eprintln!("Failed to spawn FUSE session: {}", e);
                return None;
            }
        };

        thread::sleep(Duration::from_millis(TEST_SETUP_DELAY_MS));

        Some(FuseTestSession {
            mountpoint,
            upper_layer,
            lower_layer,
            handle,
            _cleanup: cleanup,
            _upper_dir: upper_dir,
            _lower_dir: lower_dir,
        })
    }

    /// Create a new FUSE test session with pre-populated lower layer and passthrough.
    pub fn with_lower_layer_setup_and_passthrough<F>(
        test_name: &str,
        setup: F,
        passthrough: Vec<String>,
    ) -> Option<Self>
    where
        F: FnOnce(&Path),
    {
        if !check_macfuse_installed() {
            eprintln!("Skipping real FUSE test - macFUSE not installed");
            return None;
        }

        let mountpoint = match determine_mount_point(test_name) {
            Ok(mp) => mp,
            Err(e) => {
                eprintln!("Failed to determine mount point: {}", e);
                return None;
            }
        };

        let upper_dir = tempfile::tempdir().ok()?;
        let lower_dir = tempfile::tempdir().ok()?;
        let upper_layer = upper_dir.path().to_path_buf();
        let lower_layer = lower_dir.path().to_path_buf();

        setup(&lower_layer);

        let fs_instance = match TreebeardFs::new(
            upper_layer.clone(),
            lower_layer.clone(),
            None,
            1,
            passthrough,
        ) {
            Ok(f) => f,
            Err(e) => {
                eprintln!("Failed to create TreebeardFs: {}", e);
                return None;
            }
        };

        let cleanup = MountCleanup::new(mountpoint.clone());
        let session = match Session::new(fs_instance, &mountpoint, &[]) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("Failed to create FUSE session: {}", e);
                return None;
            }
        };

        let handle = match session.spawn() {
            Ok(h) => h,
            Err(e) => {
                eprintln!("Failed to spawn FUSE session: {}", e);
                return None;
            }
        };

        thread::sleep(Duration::from_millis(TEST_SETUP_DELAY_MS));

        Some(FuseTestSession {
            mountpoint,
            upper_layer,
            lower_layer,
            handle,
            _cleanup: cleanup,
            _upper_dir: upper_dir,
            _lower_dir: lower_dir,
        })
    }

    /// Create a new FUSE test session with pre-populated upper and lower layer files and passthrough.
    pub fn with_both_layers_setup_and_passthrough<F, G>(
        test_name: &str,
        lower_setup: F,
        upper_setup: G,
        passthrough: Vec<String>,
    ) -> Option<Self>
    where
        F: FnOnce(&Path),
        G: FnOnce(&Path),
    {
        if !check_macfuse_installed() {
            eprintln!("Skipping real FUSE test - macFUSE not installed");
            return None;
        }

        let mountpoint = match determine_mount_point(test_name) {
            Ok(mp) => mp,
            Err(e) => {
                eprintln!("Failed to determine mount point: {}", e);
                return None;
            }
        };

        let upper_dir = tempfile::tempdir().ok()?;
        let lower_dir = tempfile::tempdir().ok()?;
        let upper_layer = upper_dir.path().to_path_buf();
        let lower_layer = lower_dir.path().to_path_buf();

        lower_setup(&lower_layer);
        upper_setup(&upper_layer);

        let fs_instance = match TreebeardFs::new(
            upper_layer.clone(),
            lower_layer.clone(),
            None,
            1,
            passthrough,
        ) {
            Ok(f) => f,
            Err(e) => {
                eprintln!("Failed to create TreebeardFs: {}", e);
                return None;
            }
        };

        let cleanup = MountCleanup::new(mountpoint.clone());
        let session = match Session::new(fs_instance, &mountpoint, &[]) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("Failed to create FUSE session: {}", e);
                return None;
            }
        };

        let handle = match session.spawn() {
            Ok(h) => h,
            Err(e) => {
                eprintln!("Failed to spawn FUSE session: {}", e);
                return None;
            }
        };

        thread::sleep(Duration::from_millis(TEST_SETUP_DELAY_MS));

        Some(FuseTestSession {
            mountpoint,
            upper_layer,
            lower_layer,
            handle,
            _cleanup: cleanup,
            _upper_dir: upper_dir,
            _lower_dir: lower_dir,
        })
    }
}
