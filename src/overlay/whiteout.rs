//! Whiteout handling for AUFS-style overlay filesystems.
//!
//! In overlay filesystems, a "whiteout" is a marker that indicates a file
//! from the lower layer should be hidden (deleted from the perspective
//! of the overlay). This module consolidates all whiteout-related logic
//! into a single location for consistency and maintainability.
//!
//! We use AUFS-style whiteouts: a whiteout for a file named `foo` is
//! represented by an empty file named `.wh.foo` in the same directory
//! in the upper layer.

use std::ffi::{OsStr, OsString};
use std::fs::File;
use std::path::Path;

#[cfg(unix)]
use std::os::unix::ffi::OsStrExt;

/// The prefix used for AUFS-style whiteout marker files.
pub const WHITEOUT_PREFIX: &str = ".wh.";

/// Whiteout handling utilities for overlay filesystems.
///
/// This type provides methods for creating, checking, and parsing whiteout
/// markers. All whiteout-related operations should go through this type
/// to ensure consistent handling throughout the codebase.
pub struct Whiteout;

impl Whiteout {
    /// Create a whiteout marker file for the given filename in the specified directory.
    ///
    /// This creates an empty file named `.wh.<name>` in `parent_dir` to mark
    /// that the original file should be hidden from the overlay view.
    ///
    /// # Arguments
    /// * `parent_dir` - The directory where the whiteout marker should be created
    /// * `name` - The name of the file to white out (the marker will be `.wh.<name>`)
    ///
    /// # Returns
    /// * `Ok(())` on success
    /// * `Err(errno)` on failure (e.g., I/O error)
    pub fn create(parent_dir: &Path, name: &OsStr) -> Result<(), i32> {
        let whiteout_name = Self::marker_name(name);
        let whiteout_path = parent_dir.join(whiteout_name);

        File::create(&whiteout_path).map_err(|e| e.raw_os_error().unwrap_or(libc::EIO))?;

        Ok(())
    }

    /// Check if a file at the given path has been whited-out.
    ///
    /// A file is considered whited-out if a corresponding `.wh.<filename>`
    /// marker file exists in the same directory.
    ///
    /// # Arguments
    /// * `path` - The absolute path to check for whiteout status
    ///
    /// # Returns
    /// `true` if a whiteout marker exists for this path, `false` otherwise
    pub fn is_whiteout(path: &Path) -> bool {
        if let (Some(parent), Some(name)) = (path.parent(), path.file_name()) {
            let whiteout_name = Self::marker_name(name);
            let whiteout_path = parent.join(whiteout_name);
            whiteout_path.exists()
        } else {
            false
        }
    }

    /// Generate the whiteout marker filename for a given filename.
    ///
    /// For a file named `foo`, this returns `.wh.foo`.
    ///
    /// # Arguments
    /// * `name` - The original filename
    ///
    /// # Returns
    /// The whiteout marker filename (`.wh.<name>`)
    pub fn marker_name(name: &OsStr) -> OsString {
        let mut whiteout_name = OsString::from(WHITEOUT_PREFIX);
        whiteout_name.push(name);
        whiteout_name
    }

    /// Check if a filename is a whiteout marker (starts with `.wh.`).
    ///
    /// This is used during directory scanning to identify and filter
    /// whiteout markers from the file listing.
    ///
    /// # Arguments
    /// * `name` - The filename to check
    ///
    /// # Returns
    /// `true` if the filename is a whiteout marker, `false` otherwise
    #[allow(dead_code)]
    #[cfg(unix)]
    pub fn is_whiteout_marker(name: &OsStr) -> bool {
        name.as_bytes().starts_with(WHITEOUT_PREFIX.as_bytes())
    }

    #[allow(dead_code)]
    #[cfg(not(unix))]
    pub fn is_whiteout_marker(name: &OsStr) -> bool {
        name.to_string_lossy().starts_with(WHITEOUT_PREFIX)
    }

    /// Extract the target filename from a whiteout marker name.
    ///
    /// For a whiteout marker `.wh.foo`, this returns `foo`.
    ///
    /// # Arguments
    /// * `whiteout_name` - The whiteout marker filename (must start with `.wh.`)
    ///
    /// # Returns
    /// `Some(target_name)` if the name is a valid whiteout marker,
    /// `None` if the name doesn't start with `.wh.`
    #[cfg(unix)]
    pub fn extract_target(whiteout_name: &OsStr) -> Option<OsString> {
        let name_bytes = whiteout_name.as_bytes();
        let prefix_bytes = WHITEOUT_PREFIX.as_bytes();

        if name_bytes.starts_with(prefix_bytes) {
            let target_bytes = &name_bytes[prefix_bytes.len()..];
            Some(OsString::from(OsStr::from_bytes(target_bytes)))
        } else {
            None
        }
    }

    #[cfg(not(unix))]
    pub fn extract_target(whiteout_name: &OsStr) -> Option<OsString> {
        let name_str = whiteout_name.to_string_lossy();
        name_str
            .strip_prefix(WHITEOUT_PREFIX)
            .map(|target| OsString::from(target))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn test_marker_name() {
        let name = OsStr::new("test.txt");
        let marker = Whiteout::marker_name(name);
        assert_eq!(marker, OsString::from(".wh.test.txt"));
    }

    #[test]
    fn test_is_whiteout_marker() {
        assert!(Whiteout::is_whiteout_marker(OsStr::new(".wh.foo")));
        assert!(Whiteout::is_whiteout_marker(OsStr::new(".wh.bar.txt")));
        assert!(!Whiteout::is_whiteout_marker(OsStr::new("foo")));
        assert!(!Whiteout::is_whiteout_marker(OsStr::new(".hidden")));
        assert!(!Whiteout::is_whiteout_marker(OsStr::new("wh.foo")));
    }

    #[test]
    fn test_extract_target() {
        assert_eq!(
            Whiteout::extract_target(OsStr::new(".wh.foo")),
            Some(OsString::from("foo"))
        );
        assert_eq!(
            Whiteout::extract_target(OsStr::new(".wh.bar.txt")),
            Some(OsString::from("bar.txt"))
        );
        assert_eq!(Whiteout::extract_target(OsStr::new("foo")), None);
        assert_eq!(Whiteout::extract_target(OsStr::new(".hidden")), None);
    }

    #[test]
    fn test_create_and_is_whiteout() {
        let temp_dir = tempdir().unwrap();
        let parent = temp_dir.path();
        let name = OsStr::new("test.txt");

        // Initially no whiteout
        let file_path = parent.join(name);
        assert!(!Whiteout::is_whiteout(&file_path));

        // Create whiteout
        Whiteout::create(parent, name).unwrap();

        // Verify whiteout marker was created
        let marker_path = parent.join(".wh.test.txt");
        assert!(marker_path.exists());

        // File should now be considered whited-out
        assert!(Whiteout::is_whiteout(&file_path));
    }

    #[test]
    fn test_is_whiteout_with_existing_marker() {
        let temp_dir = tempdir().unwrap();
        let parent = temp_dir.path();

        // Create a whiteout marker manually
        let marker_path = parent.join(".wh.deleted.txt");
        fs::File::create(&marker_path).unwrap();

        // The original file path should be detected as whited-out
        let file_path = parent.join("deleted.txt");
        assert!(Whiteout::is_whiteout(&file_path));

        // A different file should not be whited-out
        let other_path = parent.join("other.txt");
        assert!(!Whiteout::is_whiteout(&other_path));
    }
}
