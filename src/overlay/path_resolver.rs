use std::path::{Path, PathBuf};

use crate::overlay::types::LayerType;
use crate::overlay::whiteout::Whiteout;

/// Handles path resolution, passthrough detection, and layer path computation.
///
/// This type encapsulates the single responsibility of managing paths within
/// the overlay filesystem. It knows about the upper and lower layer directories
/// and can resolve relative paths to absolute filesystem paths.
pub struct PathResolver {
    /// The upper layer directory (where writes go)
    pub(crate) upper_layer: PathBuf,
    /// The lower layer directory (the original read-only source)
    pub(crate) lower_layer: PathBuf,
    /// Glob patterns for paths that bypass the upper layer entirely
    passthrough_patterns: Vec<glob::Pattern>,
}

impl PathResolver {
    /// Create a new PathResolver with the given layer directories and passthrough patterns.
    pub fn new(
        upper_layer: PathBuf,
        lower_layer: PathBuf,
        passthrough_patterns: Vec<String>,
    ) -> crate::error::Result<Self> {
        let compiled_patterns = passthrough_patterns
            .into_iter()
            .map(|p| {
                glob::Pattern::new(&p).map_err(|e| {
                    crate::error::TreebeardError::Config(format!(
                        "Invalid passthrough glob pattern '{}': {}",
                        p, e
                    ))
                })
            })
            .collect::<crate::error::Result<Vec<glob::Pattern>>>()?;

        Ok(PathResolver {
            upper_layer,
            lower_layer,
            passthrough_patterns: compiled_patterns,
        })
    }

    /// Returns the base path for a given layer type.
    ///
    /// This is a helper to avoid repeated match statements when converting
    /// LayerType to the corresponding filesystem path.
    pub fn layer_base_path(&self, layer: LayerType) -> PathBuf {
        match layer {
            LayerType::Upper => self.upper_layer.clone(),
            LayerType::Lower => self.lower_layer.clone(),
        }
    }

    /// Check if a path should bypass the upper layer entirely.
    ///
    /// A path is considered passthrough if:
    /// 1. It directly matches a passthrough pattern (e.g., `.claude/config.toml` matches `.claude/**`)
    /// 2. It is a parent directory of a passthrough pattern (e.g., `.claude` matches if pattern is `.claude/**`)
    pub fn is_passthrough(&self, relative_path: &Path) -> bool {
        let path_to_match = relative_path.strip_prefix(".").unwrap_or(relative_path);
        let path_str = path_to_match.to_string_lossy();

        self.passthrough_patterns.iter().any(|p| {
            // Direct match
            if p.matches_path(path_to_match) {
                return true;
            }

            // Check if this path is a parent/prefix of the pattern.
            // For example, if pattern is ".claude/**", then ".claude" should also be passthrough.
            let pattern_str = p.as_str();
            if let Some(prefix) = pattern_str.strip_suffix("/**") {
                // Path matches the directory prefix exactly
                if path_str == prefix {
                    return true;
                }
                // Path is a parent of the prefix (e.g., "foo" when pattern is "foo/bar/**")
                if prefix.starts_with(&format!("{}/", path_str)) {
                    return true;
                }
            }

            false
        })
    }

    /// Resolve the actual filesystem path for a relative path.
    ///
    /// This function returns the absolute path where the file actually exists,
    /// checking upper layer first (overlay semantics: upper shadows lower).
    ///
    /// Returns (absolute_path, actual_layer) or None if the file doesn't exist.
    pub fn resolve_path(
        &self,
        relative_path: &Path,
        _expected_layer: LayerType,
    ) -> Option<(PathBuf, LayerType)> {
        if self.is_passthrough(relative_path) {
            let lower_path = self.lower_layer.join(relative_path);
            return if lower_path.exists() {
                Some((lower_path, LayerType::Lower))
            } else {
                None
            };
        }

        let upper_path = self.upper_layer.join(relative_path);
        let lower_path = self.lower_layer.join(relative_path);

        tracing::trace!(
            "resolve_path: relative={:?}, expected_layer={:?}",
            relative_path,
            _expected_layer
        );
        tracing::trace!(
            "resolve_path: upper={} (exists={}), lower={} (exists={})",
            upper_path.display(),
            upper_path.exists(),
            lower_path.display(),
            lower_path.exists()
        );

        // Check for whiteout first
        if self.is_whiteout(&upper_path) {
            tracing::trace!("resolve_path: whiteout detected");
            return None;
        }

        // Try upper layer first (overlay semantics)
        if upper_path.exists() {
            tracing::trace!("resolve_path: found in upper layer");
            // Canonicalize to resolve symlinks and prevent TOCTOU via symlink replacement
            match upper_path.canonicalize() {
                Ok(path) => return Some((path, LayerType::Upper)),
                Err(e) => {
                    tracing::warn!(
                        "Failed to canonicalize {}: {}, using uncanonicalized path",
                        upper_path.display(),
                        e
                    );
                    return Some((upper_path, LayerType::Upper));
                }
            }
        }

        // Fall back to lower layer
        if lower_path.exists() {
            tracing::trace!("resolve_path: found in lower layer");
            // Canonicalize to resolve symlinks and prevent TOCTOU via symlink replacement
            match lower_path.canonicalize() {
                Ok(path) => return Some((path, LayerType::Lower)),
                Err(e) => {
                    tracing::warn!(
                        "Failed to canonicalize {}: {}, using uncanonicalized path",
                        lower_path.display(),
                        e
                    );
                    return Some((lower_path, LayerType::Lower));
                }
            }
        }

        tracing::trace!("resolve_path: file not found in any layer");
        None
    }

    /// Check if a file has been whited-out using AUFS-style `.wh.` prefix files.
    ///
    /// For a file at `path`, checks if a whiteout marker exists at `.wh.<filename>`
    /// in the same directory within the upper layer.
    ///
    /// See [`Whiteout`] for more details on whiteout handling.
    pub fn is_whiteout(&self, path: &Path) -> bool {
        Whiteout::is_whiteout(path)
    }

    /// Check if a file has been whited-out, given a relative path.
    ///
    /// This is a convenience method that constructs the upper layer path
    /// before checking for the whiteout marker.
    #[allow(dead_code)]
    pub fn is_whiteout_relative(&self, relative_path: &Path) -> bool {
        let upper_path = self.upper_layer.join(relative_path);
        self.is_whiteout(&upper_path)
    }

    /// Get the upper layer path for a relative path.
    pub fn upper_path(&self, relative_path: &Path) -> PathBuf {
        self.upper_layer.join(relative_path)
    }

    /// Get the lower layer path for a relative path.
    pub fn lower_path(&self, relative_path: &Path) -> PathBuf {
        self.lower_layer.join(relative_path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn test_layer_base_path() {
        let temp_dir = tempdir().unwrap();
        let upper = temp_dir.path().join("upper");
        let lower = temp_dir.path().join("lower");

        let resolver = PathResolver::new(upper.clone(), lower.clone(), vec![]).unwrap();

        assert_eq!(resolver.layer_base_path(LayerType::Upper), upper);
        assert_eq!(resolver.layer_base_path(LayerType::Lower), lower);
    }

    #[test]
    fn test_is_passthrough() {
        let temp_dir = tempdir().unwrap();
        let upper = temp_dir.path().join("upper");
        let lower = temp_dir.path().join("lower");

        let resolver = PathResolver::new(
            upper,
            lower,
            vec![".claude/**".to_string(), "node_modules/**".to_string()],
        )
        .unwrap();

        // Direct matches
        assert!(resolver.is_passthrough(Path::new(".claude/config.toml")));
        assert!(resolver.is_passthrough(Path::new("node_modules/package/index.js")));

        // Parent directory matches
        assert!(resolver.is_passthrough(Path::new(".claude")));

        // Non-matches
        assert!(!resolver.is_passthrough(Path::new("src/main.rs")));
        assert!(!resolver.is_passthrough(Path::new(".git/config")));
    }

    #[test]
    fn test_is_whiteout() {
        let temp_dir = tempdir().unwrap();
        let upper = temp_dir.path().join("upper");
        let lower = temp_dir.path().join("lower");

        fs::create_dir_all(&upper).unwrap();

        let resolver = PathResolver::new(upper.clone(), lower, vec![]).unwrap();

        // No whiteout initially
        let test_path = upper.join("test.txt");
        assert!(!resolver.is_whiteout(&test_path));

        // Create a whiteout marker
        let whiteout_path = upper.join(".wh.test.txt");
        fs::File::create(&whiteout_path).unwrap();

        // Now it should be detected as whiteout
        assert!(resolver.is_whiteout(&test_path));
    }

    #[test]
    fn test_resolve_path() {
        let temp_dir = tempdir().unwrap();
        let upper = temp_dir.path().join("upper");
        let lower = temp_dir.path().join("lower");

        fs::create_dir_all(&upper).unwrap();
        fs::create_dir_all(&lower).unwrap();

        let resolver = PathResolver::new(upper.clone(), lower.clone(), vec![]).unwrap();

        // File only in lower layer
        let lower_file = lower.join("lower_only.txt");
        fs::write(&lower_file, "lower content").unwrap();

        let result = resolver.resolve_path(Path::new("lower_only.txt"), LayerType::Lower);
        assert!(result.is_some());
        let (path, layer) = result.unwrap();
        assert_eq!(layer, LayerType::Lower);
        assert!(path.ends_with("lower_only.txt"));

        // File in both layers (upper shadows lower)
        let upper_file = upper.join("both.txt");
        let lower_file2 = lower.join("both.txt");
        fs::write(&upper_file, "upper content").unwrap();
        fs::write(&lower_file2, "lower content").unwrap();

        let result = resolver.resolve_path(Path::new("both.txt"), LayerType::Lower);
        assert!(result.is_some());
        let (path, layer) = result.unwrap();
        assert_eq!(layer, LayerType::Upper); // Upper shadows lower
        assert!(path.ends_with("both.txt"));

        // File that doesn't exist
        let result = resolver.resolve_path(Path::new("nonexistent.txt"), LayerType::Lower);
        assert!(result.is_none());
    }

    #[test]
    fn test_resolve_path_with_whiteout() {
        let temp_dir = tempdir().unwrap();
        let upper = temp_dir.path().join("upper");
        let lower = temp_dir.path().join("lower");

        fs::create_dir_all(&upper).unwrap();
        fs::create_dir_all(&lower).unwrap();

        let resolver = PathResolver::new(upper.clone(), lower.clone(), vec![]).unwrap();

        // Create file in lower layer
        let lower_file = lower.join("deleted.txt");
        fs::write(&lower_file, "content").unwrap();

        // Create whiteout in upper layer
        let whiteout_path = upper.join(".wh.deleted.txt");
        fs::File::create(&whiteout_path).unwrap();

        // File should not be found (whiteout hides it)
        let result = resolver.resolve_path(Path::new("deleted.txt"), LayerType::Lower);
        assert!(result.is_none());
    }
}
