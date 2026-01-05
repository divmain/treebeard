//! Sandbox support for subprocess isolation using macOS sandbox-exec.
//!
//! This module provides SBPL (Sandbox Profile Language) profile generation
//! for restricting subprocess access to filesystem and network resources.
//! This is especially useful for AI coding tools that should not have access
//! to sensitive data like SSH keys, AWS credentials, etc.

use crate::config::{expand_tilde, NetworkMode, SandboxConfig};
use crate::error::{Result, TreebeardError};
use std::path::Path;

/// Validates a path for safe inclusion in an SBPL profile.
///
/// This function checks that a path doesn't contain characters that
/// could be interpreted as SBPL syntax, preventing injection attacks.
///
/// # Arguments
/// * `path` - The path to validate
///
/// # Returns
/// An error if the path contains invalid characters, Ok otherwise
#[cfg(target_os = "macos")]
fn validate_sbpl_path(path: &Path) -> Result<()> {
    let path_str = path.to_str().ok_or_else(|| {
        TreebeardError::Config("Sandbox path contains invalid UTF-8 characters".to_string())
    })?;

    // Check for SBPL special characters that could be interpreted as directives
    let forbidden_chars = ['(', ')', ';', '"', '\\', '\n', '\r'];
    for (i, c) in path_str.chars().enumerate() {
        if forbidden_chars.contains(&c) {
            return Err(TreebeardError::Config(format!(
                "Sandbox path contains forbidden character '{}' at position {}: {}",
                c, i, path_str
            )));
        }
    }

    Ok(())
}

/// Generates an SBPL (Sandbox Profile Language) profile string for sandbox-exec.
///
/// The profile:
/// - Allows reading everything by default, except paths in `deny_read`
/// - Denies all writes by default, except:
///   - The mount path (worktree overlay)
///   - Temp directories (/tmp, /private/tmp, /var/folders)
///   - Paths in `allow_write`
/// - Allows all process execution
/// - Configures network access based on the network mode
///
/// # Arguments
/// * `config` - The sandbox configuration
/// * `mount_path` - The FUSE mount path (always allowed for writes)
///
/// # Returns
/// The SBPL profile as a string, ready to be passed to `sandbox-exec -p`
#[cfg(target_os = "macos")]
pub fn generate_sbpl_profile(config: &SandboxConfig, mount_path: &Path) -> String {
    // Canonicalize the mount path to resolve symlinks.
    // On macOS, /var is a symlink to /private/var, and sandbox-exec doesn't
    // resolve symlinks in profile paths. Without canonicalization, writes to
    // paths like /var/folders/... would be denied even if allowed in the profile.
    let canonical_mount_path = mount_path
        .canonicalize()
        .unwrap_or_else(|_| mount_path.to_path_buf());
    tracing::debug!(
        "Generating SBPL profile for mount_path: {:?} (canonical: {:?})",
        mount_path,
        canonical_mount_path
    );

    let mut profile = String::new();

    // Version declaration
    profile.push_str("(version 1)\n\n");

    // Default allow for most operations
    profile.push_str("; Default allows\n");
    profile.push_str("(allow default)\n\n");

    // Allow reading everything by default
    profile.push_str("; Allow reading everything by default\n");
    profile.push_str("(allow file-read*)\n\n");

    // Deny reads to sensitive paths
    if !config.deny_read.is_empty() {
        profile.push_str("; Deny reads to sensitive paths (from deny_read config)\n");
        for path in &config.deny_read {
            let expanded = expand_tilde(path);
            // Validate path to prevent SBPL injection
            if let Err(e) = validate_sbpl_path(&expanded) {
                tracing::warn!("Skipping invalid deny_read path {:?}: {}", path, e);
                continue;
            }
            let path_str = expanded.to_string_lossy();
            // Use subpath to include all files under the directory
            profile.push_str(&format!("(deny file-read* (subpath \"{}\"))\n", path_str));
        }
        profile.push('\n');
    }

    // Deny writes by default
    profile.push_str("; Deny writes by default\n");
    profile.push_str("(deny file-write*)\n\n");

    // Allow writes to mount path (using canonical path to handle symlinks like /var -> /private/var)
    profile.push_str("; Allow writes to mount path\n");
    let mount_path_str = canonical_mount_path.to_string_lossy();
    profile.push_str(&format!(
        "(allow file-write* (subpath \"{}\"))\n\n",
        mount_path_str
    ));

    // Allow writes to temp directories
    profile.push_str("; Allow writes to temp directories\n");
    profile.push_str("(allow file-write* (subpath \"/tmp\"))\n");
    profile.push_str("(allow file-write* (subpath \"/private/tmp\"))\n");
    profile.push_str("(allow file-write* (subpath \"/var/folders\"))\n\n");

    // Allow writes to /dev for terminal/device access (null, tty, ptys, etc.)
    // This is required for shell operation - many commands redirect to /dev/null
    // and interactive shells need PTY device access.
    profile.push_str("; Allow writes to /dev for terminal and device access\n");
    profile.push_str("(allow file-write* (subpath \"/dev\"))\n\n");

    // Allow additional user-specified write paths
    if !config.allow_write.is_empty() {
        profile.push_str("; Allow additional user-specified write paths\n");
        for path in &config.allow_write {
            let expanded = expand_tilde(path);
            // Validate path to prevent SBPL injection
            if let Err(e) = validate_sbpl_path(&expanded) {
                tracing::warn!("Skipping invalid allow_write path {:?}: {}", path, e);
                continue;
            }
            let path_str = expanded.to_string_lossy();
            profile.push_str(&format!("(allow file-write* (subpath \"{}\"))\n", path_str));
        }
        profile.push('\n');
    }

    // Allow process execution
    profile.push_str("; Allow process execution\n");
    profile.push_str("(allow process-exec*)\n");
    profile.push_str("(allow process-fork)\n\n");

    // Network rules based on mode
    profile.push_str("; Network rules\n");
    match config.network.mode {
        NetworkMode::Allow => {
            profile.push_str("; Network mode: allow (no restrictions)\n");
            profile.push_str("(allow network*)\n");
        }
        NetworkMode::Localhost => {
            profile.push_str("; Network mode: localhost (only localhost + allow_hosts)\n");
            profile.push_str("(deny network*)\n");
            profile.push_str("(allow network* (remote ip \"localhost:*\"))\n");
            profile.push_str("(allow network* (remote ip \"127.0.0.1:*\"))\n");
            profile.push_str("(allow network* (remote ip \"::1:*\"))\n");
            // Allow local unix sockets
            profile.push_str("(allow network* (local unix-socket))\n");
            profile.push_str("(allow network* (remote unix-socket))\n");
            for host in &config.network.allow_hosts {
                profile.push_str(&format!("(allow network* (remote ip \"{}:*\"))\n", host));
            }
        }
        NetworkMode::Deny => {
            profile.push_str("; Network mode: deny (only allow_hosts)\n");
            profile.push_str("(deny network*)\n");
            // Allow local unix sockets even in deny mode (needed for many tools)
            profile.push_str("(allow network* (local unix-socket))\n");
            profile.push_str("(allow network* (remote unix-socket))\n");
            for host in &config.network.allow_hosts {
                profile.push_str(&format!("(allow network* (remote ip \"{}:*\"))\n", host));
            }
        }
    }

    profile
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{NetworkMode, SandboxConfig, SandboxNetworkConfig};
    use std::path::PathBuf;

    fn default_test_config() -> SandboxConfig {
        SandboxConfig {
            enabled: true,
            deny_read: vec!["~/.ssh".to_string(), "~/.aws".to_string()],
            allow_write: vec![],
            network: SandboxNetworkConfig {
                mode: NetworkMode::Allow,
                allow_hosts: vec![],
            },
        }
    }

    #[test]
    fn test_generate_sbpl_profile_contains_version() {
        let config = default_test_config();
        let mount_path = PathBuf::from("/mounts/repo/branch");
        let profile = generate_sbpl_profile(&config, &mount_path);

        assert!(profile.contains("(version 1)"));
    }

    #[test]
    fn test_generate_sbpl_profile_denies_sensitive_paths() {
        let config = default_test_config();
        let mount_path = PathBuf::from("/mounts/repo/branch");
        let profile = generate_sbpl_profile(&config, &mount_path);

        // Check that deny_read paths are in the profile
        // The paths should be expanded from ~ to HOME
        assert!(profile.contains("(deny file-read*"));
        assert!(profile.contains(".ssh"));
        assert!(profile.contains(".aws"));
    }

    #[test]
    fn test_generate_sbpl_profile_allows_mount_path() {
        let config = default_test_config();
        let mount_path = PathBuf::from("/mounts/repo/branch");
        let profile = generate_sbpl_profile(&config, &mount_path);

        // The mount path should be in the profile (may be canonicalized if it exists)
        // For non-existent paths, canonicalize falls back to the original path
        assert!(profile.contains("(allow file-write* (subpath \"/mounts/repo/branch\"))"));
    }

    #[test]
    fn test_generate_sbpl_profile_canonicalizes_mount_path() {
        // Test with a real path that exists and has symlinks (like /var -> /private/var)
        let config = default_test_config();
        let mount_path = PathBuf::from("/var/folders");
        let profile = generate_sbpl_profile(&config, &mount_path);

        // On macOS, /var is a symlink to /private/var, so the profile should
        // contain the canonical path for proper sandbox-exec behavior
        assert!(
            profile.contains("(allow file-write* (subpath \"/private/var/folders\"))")
                || profile.contains("(allow file-write* (subpath \"/var/folders\"))"),
            "Profile should contain canonicalized mount path. Profile:\n{}",
            profile
        );
    }

    #[test]
    fn test_generate_sbpl_profile_allows_temp_dirs() {
        let config = default_test_config();
        let mount_path = PathBuf::from("/mounts/repo/branch");
        let profile = generate_sbpl_profile(&config, &mount_path);

        assert!(profile.contains("(allow file-write* (subpath \"/tmp\"))"));
        assert!(profile.contains("(allow file-write* (subpath \"/private/tmp\"))"));
        assert!(profile.contains("(allow file-write* (subpath \"/var/folders\"))"));
    }

    #[test]
    fn test_generate_sbpl_profile_allows_dev_writes() {
        let config = default_test_config();
        let mount_path = PathBuf::from("/mounts/repo/branch");
        let profile = generate_sbpl_profile(&config, &mount_path);

        // /dev writes are required for shell operation (e.g., /dev/null, /dev/tty)
        assert!(profile.contains("(allow file-write* (subpath \"/dev\"))"));
    }

    #[test]
    fn test_generate_sbpl_profile_allows_process_exec() {
        let config = default_test_config();
        let mount_path = PathBuf::from("/mounts/repo/branch");
        let profile = generate_sbpl_profile(&config, &mount_path);

        assert!(profile.contains("(allow process-exec*)"));
        assert!(profile.contains("(allow process-fork)"));
    }

    #[test]
    fn test_generate_sbpl_profile_network_allow() {
        let config = SandboxConfig {
            enabled: true,
            deny_read: vec![],
            allow_write: vec![],
            network: SandboxNetworkConfig {
                mode: NetworkMode::Allow,
                allow_hosts: vec![],
            },
        };
        let mount_path = PathBuf::from("/mounts/repo/branch");
        let profile = generate_sbpl_profile(&config, &mount_path);

        assert!(profile.contains("(allow network*)"));
        assert!(!profile.contains("(deny network*)"));
    }

    #[test]
    fn test_generate_sbpl_profile_network_localhost() {
        let config = SandboxConfig {
            enabled: true,
            deny_read: vec![],
            allow_write: vec![],
            network: SandboxNetworkConfig {
                mode: NetworkMode::Localhost,
                allow_hosts: vec!["192.168.1.1".to_string()],
            },
        };
        let mount_path = PathBuf::from("/mounts/repo/branch");
        let profile = generate_sbpl_profile(&config, &mount_path);

        assert!(profile.contains("(deny network*)"));
        assert!(profile.contains("(allow network* (remote ip \"localhost:*\"))"));
        assert!(profile.contains("(allow network* (remote ip \"127.0.0.1:*\"))"));
        assert!(profile.contains("(allow network* (remote ip \"::1:*\"))"));
        assert!(profile.contains("(allow network* (remote ip \"192.168.1.1:*\"))"));
    }

    #[test]
    fn test_generate_sbpl_profile_network_deny() {
        let config = SandboxConfig {
            enabled: true,
            deny_read: vec![],
            allow_write: vec![],
            network: SandboxNetworkConfig {
                mode: NetworkMode::Deny,
                allow_hosts: vec!["api.example.com".to_string()],
            },
        };
        let mount_path = PathBuf::from("/mounts/repo/branch");
        let profile = generate_sbpl_profile(&config, &mount_path);

        assert!(profile.contains("(deny network*)"));
        assert!(profile.contains("(allow network* (remote ip \"api.example.com:*\"))"));
        // Localhost should NOT be allowed in deny mode unless specified
        assert!(!profile.contains("(allow network* (remote ip \"localhost:*\"))"));
    }

    #[test]
    fn test_generate_sbpl_profile_additional_write_paths() {
        let config = SandboxConfig {
            enabled: true,
            deny_read: vec![],
            allow_write: vec!["~/custom-cache".to_string()],
            network: SandboxNetworkConfig::default(),
        };
        let mount_path = PathBuf::from("/mounts/repo/branch");
        let profile = generate_sbpl_profile(&config, &mount_path);

        // Should contain the expanded custom-cache path
        assert!(profile.contains("custom-cache"));
        assert!(profile.contains("(allow file-write*"));
    }

    #[test]
    fn test_validate_sbpl_path_rejects_parentheses() {
        let path = PathBuf::from("/path/with).injection");
        assert!(validate_sbpl_path(&path).is_err());
    }

    #[test]
    fn test_validate_sbpl_path_rejects_semicolon() {
        let path = PathBuf::from("/path;injection");
        assert!(validate_sbpl_path(&path).is_err());
    }

    #[test]
    fn test_validate_sbpl_path_rejects_quotes() {
        let path = PathBuf::from("/path\"injection");
        assert!(validate_sbpl_path(&path).is_err());
    }

    #[test]
    fn test_validate_sbpl_path_rejects_backslash() {
        let path = PathBuf::from("/path\\injection");
        assert!(validate_sbpl_path(&path).is_err());
    }

    #[test]
    fn test_validate_sbpl_path_rejects_newlines() {
        let path = PathBuf::from("/path\ninjection");
        assert!(validate_sbpl_path(&path).is_err());
    }

    #[test]
    fn test_validate_sbpl_path_rejects_carriage_return() {
        let path = PathBuf::from("/path\rinjection");
        assert!(validate_sbpl_path(&path).is_err());
    }

    #[test]
    fn test_validate_sbpl_path_accepts_valid_paths() {
        let valid_paths = vec![
            PathBuf::from("/home/user/.ssh"),
            PathBuf::from("/var/folders/abc"),
            PathBuf::from("/tmp/file.txt"),
            PathBuf::from("/home/user/name with spaces"),
            PathBuf::from("/home/user/unicode_ñ"),
        ];
        for path in valid_paths {
            assert!(
                validate_sbpl_path(&path).is_ok(),
                "Valid path {:?} should pass validation",
                path
            );
        }
    }

    #[test]
    fn test_generate_sbpl_profile_skips_invalid_deny_read_paths() {
        let mut config = default_test_config();
        // Add a malicious path that tries to inject SBPL directives
        config
            .deny_read
            .push("~/.ssh\") (allow file-read* (subpath \"/etc/shadow".to_string());
        let mount_path = PathBuf::from("/mounts/repo/branch");
        let profile = generate_sbpl_profile(&config, &mount_path);

        // The malicious path should be skipped, so we shouldn't see the injection
        assert!(!profile.contains("(allow file-read* (subpath \"/etc/shadow\")"));
        // Legitimate paths should still be present
        assert!(profile.contains(".ssh"));
        assert!(profile.contains(".aws"));
    }

    #[test]
    fn test_generate_sbpl_profile_skips_invalid_allow_write_paths() {
        let config = SandboxConfig {
            enabled: true,
            deny_read: vec![],
            // Add a malicious path that tries to inject SBPL directives
            allow_write: vec!["~/cache\") (allow file-write* (subpath \"/etc".to_string()],
            network: SandboxNetworkConfig::default(),
        };
        let mount_path = PathBuf::from("/mounts/repo/branch");
        let profile = generate_sbpl_profile(&config, &mount_path);

        // The malicious path should be skipped, so we shouldn't see the injection
        assert!(!profile.contains("(allow file-write* (subpath \"/etc\")"));
        assert!(!profile.contains("cache"));
    }

    #[test]
    fn test_generate_sbpl_profile_handles_valid_special_characters() {
        let config = SandboxConfig {
            enabled: true,
            deny_read: vec![
                "~/path with spaces".to_string(),
                "~/path_with_underscore".to_string(),
            ],
            allow_write: vec!["~/cache-dash".to_string()],
            network: SandboxNetworkConfig::default(),
        };
        let mount_path = PathBuf::from("/mounts/repo/branch");
        let profile = generate_sbpl_profile(&config, &mount_path);

        // Valid characters like spaces, dashes, underscores should work
        assert!(profile.contains("path with spaces"));
        assert!(profile.contains("path_with_underscore"));
        assert!(profile.contains("cache-dash"));
    }
}
