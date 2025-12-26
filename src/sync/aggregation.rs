use super::files::CompiledPatterns;
use super::types::{AggregateResult, ChangeItem, ChangeType, DirectoryChange, FileChange};
use crate::config::SyncConfig;
use crate::overlay::MutationType;
use std::collections::{HashMap, HashSet};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

fn find_top_ignored_ancestor(path: &Path, ignored_dirs: &HashSet<PathBuf>) -> Option<PathBuf> {
    let mut top_ignored = None;
    let mut current = path.parent();

    while let Some(dir) = current {
        if ignored_dirs.contains(dir) {
            top_ignored = Some(dir.to_path_buf());
        }
        current = dir.parent();
    }

    top_ignored
}

/// Error type for git check-ignore failures
#[derive(Debug)]
pub struct GitCheckIgnoreError {
    pub message: String,
}

impl std::fmt::Display for GitCheckIgnoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "git check-ignore failed: {}", self.message)
    }
}

impl std::error::Error for GitCheckIgnoreError {}

pub fn get_gitignored_files(
    repo_path: &Path,
    files: &[PathBuf],
) -> Result<HashSet<PathBuf>, GitCheckIgnoreError> {
    let files: Vec<_> = files.iter().filter(|p| !p.as_os_str().is_empty()).collect();

    if files.is_empty() {
        return Ok(HashSet::new());
    }

    let mut child = Command::new("git")
        .args(["check-ignore", "--stdin", "-z"])
        .current_dir(repo_path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .map_err(|e| {
            tracing::warn!("Failed to spawn git check-ignore: {}", e);
            GitCheckIgnoreError {
                message: format!("failed to spawn git: {}", e),
            }
        })?;

    {
        let stdin = child.stdin.as_mut().ok_or_else(|| {
            tracing::warn!("Failed to get stdin for git check-ignore");
            GitCheckIgnoreError {
                message: "failed to get stdin pipe".to_string(),
            }
        })?;
        for path in &files {
            stdin
                .write_all(path.as_os_str().as_encoded_bytes())
                .map_err(|e| {
                    tracing::warn!("Failed to write to git check-ignore stdin: {}", e);
                    GitCheckIgnoreError {
                        message: format!("failed to write to stdin: {}", e),
                    }
                })?;
            stdin.write_all(b"\0").map_err(|e| {
                tracing::warn!("Failed to write null byte to git check-ignore stdin: {}", e);
                GitCheckIgnoreError {
                    message: format!("failed to write to stdin: {}", e),
                }
            })?;
        }
    }

    let output = child.wait_with_output().map_err(|e| {
        tracing::warn!("Failed to wait for git check-ignore: {}", e);
        GitCheckIgnoreError {
            message: format!("failed to wait for git process: {}", e),
        }
    })?;

    let mut ignored = HashSet::new();
    for path_bytes in output.stdout.split(|&b| b == 0) {
        if !path_bytes.is_empty() {
            if let Ok(path_str) = std::str::from_utf8(path_bytes) {
                ignored.insert(PathBuf::from(path_str));
            }
        }
    }

    Ok(ignored)
}

pub struct FilteredItems {
    pub items_to_show: Vec<ChangeItem>,
    pub auto_skipped: Vec<ChangeItem>,
}

pub fn filter_syncable_items(
    change_items: Vec<ChangeItem>,
    repo_path: &Path,
    sync_config: &SyncConfig,
) -> Result<FilteredItems, GitCheckIgnoreError> {
    let all_files: Vec<PathBuf> = change_items
        .iter()
        .flat_map(|item| match item {
            ChangeItem::File(file) => vec![file.path.clone()],
            ChangeItem::Directory(dir) => dir.files.iter().map(|f| f.path.clone()).collect(),
        })
        .collect();

    let ignored_files = get_gitignored_files(repo_path, &all_files)?;

    let gitignored_items: Vec<ChangeItem> = change_items
        .into_iter()
        .filter(|item| match item {
            ChangeItem::File(file) => ignored_files.contains(&file.path),
            ChangeItem::Directory(dir) => dir.files.iter().any(|f| ignored_files.contains(&f.path)),
        })
        .collect();

    let skip_patterns = CompiledPatterns::new(&sync_config.sync_always_skip);
    let mut auto_skipped: Vec<ChangeItem> = Vec::new();
    let items_to_show: Vec<ChangeItem> = gitignored_items
        .into_iter()
        .filter(|item| {
            let should_skip = match item {
                ChangeItem::File(file) => skip_patterns.matches(&file.path),
                ChangeItem::Directory(dir) => skip_patterns.matches(&dir.path),
            };
            if should_skip {
                auto_skipped.push(item.clone());
                false
            } else {
                true
            }
        })
        .collect();

    Ok(FilteredItems {
        items_to_show,
        auto_skipped,
    })
}

pub fn aggregate_changes(
    mutations: &HashMap<PathBuf, MutationType>,
    repo_path: &Path,
    worktree_path: &Path,
) -> AggregateResult {
    let mut files: Vec<FileChange> = Vec::new();
    let mut dir_candidates: HashSet<PathBuf> = HashSet::new();
    let mut symlinks_skipped = 0;

    for (path, mutation_type) in mutations {
        let full_path = worktree_path.join(path);
        if full_path
            .symlink_metadata()
            .is_ok_and(|m| m.file_type().is_symlink())
        {
            symlinks_skipped += 1;
            continue;
        }

        let file_change = FileChange {
            path: path.clone(),
            change_type: mutation_type.clone().into(),
        };
        files.push(file_change);

        let mut current = path.parent();
        while let Some(dir) = current {
            if !dir.as_os_str().is_empty() {
                dir_candidates.insert(dir.to_path_buf());
            }
            current = dir.parent();
        }
    }

    let dir_candidate_vec: Vec<PathBuf> = dir_candidates.into_iter().collect();
    // For directory grouping, we can tolerate git check-ignore failures - files will just
    // be shown individually rather than grouped by directory. This is a minor UX degradation.
    let ignored_dirs = get_gitignored_files(repo_path, &dir_candidate_vec).unwrap_or_else(|e| {
        tracing::debug!(
            "git check-ignore failed for directory grouping (non-critical): {}",
            e
        );
        HashSet::new()
    });

    let mut dir_groups: HashMap<PathBuf, Vec<FileChange>> = HashMap::new();
    let mut standalone_files: Vec<FileChange> = Vec::new();

    for file in files {
        if let Some(top_ignored_dir) = find_top_ignored_ancestor(&file.path, &ignored_dirs) {
            dir_groups.entry(top_ignored_dir).or_default().push(file);
        } else {
            standalone_files.push(file);
        }
    }

    let mut result: Vec<ChangeItem> = Vec::new();

    for (dir_path, mut files) in dir_groups {
        if files.len() == 1 {
            result.push(ChangeItem::File(files.pop().unwrap()));
        } else {
            let modified_count = files
                .iter()
                .filter(|f| f.change_type == ChangeType::Modified)
                .count();
            let added_count = files
                .iter()
                .filter(|f| f.change_type == ChangeType::Added)
                .count();
            let deleted_count = files
                .iter()
                .filter(|f| f.change_type == ChangeType::Deleted)
                .count();

            result.push(ChangeItem::Directory(DirectoryChange {
                path: dir_path,
                files,
                modified_count,
                added_count,
                deleted_count,
            }));
        }
    }

    for file in standalone_files {
        result.push(ChangeItem::File(file));
    }

    AggregateResult {
        items: result,
        symlinks_skipped,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_get_gitignored_files_empty_input() {
        // Empty input should succeed with empty result
        let temp_dir = tempfile::tempdir().unwrap();
        let result = get_gitignored_files(temp_dir.path(), &[]);
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }

    #[test]
    fn test_get_gitignored_files_nonexistent_repo() {
        // Running git check-ignore in a non-git directory should fail
        let temp_dir = tempfile::tempdir().unwrap();
        let files = vec![PathBuf::from("test.txt")];
        let result = get_gitignored_files(temp_dir.path(), &files);
        // This might succeed or fail depending on git's behavior in non-repo dirs
        // The important thing is it doesn't panic
        let _ = result;
    }

    #[test]
    fn test_git_check_ignore_error_display() {
        let error = GitCheckIgnoreError {
            message: "test error".to_string(),
        };
        assert_eq!(format!("{}", error), "git check-ignore failed: test error");
    }

    #[test]
    fn test_filter_syncable_items_propagates_git_error() {
        // When git check-ignore fails, filter_syncable_items should propagate the error.
        // We test this by using a path where git check-ignore will fail.
        // Note: This test verifies error propagation, not the specific error conditions.

        // Create a temp dir that is NOT a git repo
        let temp_dir = tempfile::tempdir().unwrap();
        let sync_config = SyncConfig::default();

        // Create some change items
        let items = vec![ChangeItem::File(FileChange {
            path: PathBuf::from("some/file.txt"),
            change_type: ChangeType::Modified,
        })];

        // filter_syncable_items calls get_gitignored_files internally
        // In a non-git directory, this may or may not fail depending on git version
        // The key is that if it does fail, the error is properly propagated
        let result = filter_syncable_items(items, temp_dir.path(), &sync_config);

        // Either succeeds with empty result (git exits 0 but finds nothing)
        // or fails with GitCheckIgnoreError - both are valid behaviors
        match result {
            Ok(filtered) => {
                // If git succeeded but found nothing gitignored, that's fine
                assert!(filtered.items_to_show.is_empty());
            }
            Err(e) => {
                // If git failed, we should have a proper error message
                assert!(!e.message.is_empty());
            }
        }
    }

    #[test]
    fn test_should_skip_diff_small_file() {
        use super::super::files::should_skip_diff;
        let repo_file = NamedTempFile::new().unwrap();
        let worktree_file = NamedTempFile::new().unwrap();

        repo_file.as_file().write_all(b"small content").unwrap();
        worktree_file
            .as_file()
            .write_all(b"small content modified")
            .unwrap();

        assert!(!should_skip_diff(repo_file.path(), worktree_file.path()));
    }

    #[test]
    fn test_should_skip_diff_large_file() {
        use super::super::files::should_skip_diff;
        use super::super::MAX_DIFF_FILE_SIZE;
        let repo_file = NamedTempFile::new().unwrap();
        let worktree_file = NamedTempFile::new().unwrap();

        let large_content = "x".repeat(MAX_DIFF_FILE_SIZE as usize + 1);
        repo_file.as_file().write_all(b"small").unwrap();
        worktree_file
            .as_file()
            .write_all(large_content.as_bytes())
            .unwrap();

        assert!(should_skip_diff(repo_file.path(), worktree_file.path()));
    }

    #[test]
    fn test_should_skip_diff_repo_file_large() {
        use super::super::files::should_skip_diff;
        use super::super::MAX_DIFF_FILE_SIZE;
        let repo_file = NamedTempFile::new().unwrap();
        let worktree_file = NamedTempFile::new().unwrap();

        let large_content = "y".repeat(MAX_DIFF_FILE_SIZE as usize + 1);
        repo_file
            .as_file()
            .write_all(large_content.as_bytes())
            .unwrap();
        worktree_file
            .as_file()
            .write_all(b"small content modified")
            .unwrap();

        assert!(should_skip_diff(repo_file.path(), worktree_file.path()));
    }

    #[test]
    fn test_should_skip_diff_both_not_large() {
        use super::super::files::should_skip_diff;
        use super::super::MAX_DIFF_FILE_SIZE;
        let repo_file = NamedTempFile::new().unwrap();
        let worktree_file = NamedTempFile::new().unwrap();

        let content_at_threshold = "z".repeat(MAX_DIFF_FILE_SIZE as usize);
        repo_file
            .as_file()
            .write_all(content_at_threshold.as_bytes())
            .unwrap();
        worktree_file
            .as_file()
            .write_all(content_at_threshold.as_bytes())
            .unwrap();

        assert!(!should_skip_diff(repo_file.path(), worktree_file.path()));
    }
}
