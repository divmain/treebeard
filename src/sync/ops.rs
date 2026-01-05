use crate::error::{Result, TreebeardError};
use crate::sync::types::{ChangeItem, DirectoryChange, FileChange, SyncProgress, SyncResult};
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

struct SyncStats {
    synced_files: Vec<PathBuf>,
    failed_files: Vec<(PathBuf, String)>,
    total_count: usize,
}

fn sync_items_internal(items: &[ChangeItem], repo_path: &Path, worktree_path: &Path) -> SyncStats {
    let mut synced_files: Vec<PathBuf> = Vec::new();
    let mut failed_files: Vec<(PathBuf, String)> = Vec::new();
    let mut total_count = 0;

    for item in items {
        match item {
            ChangeItem::File(file) => match sync_single_file(file, repo_path, worktree_path) {
                Ok(()) => {
                    synced_files.push(file.path.clone());
                    total_count += 1;
                    println!("  ✓ {}", file.path.display());
                }
                Err(e) => {
                    failed_files.push((file.path.clone(), e.to_string()));
                    println!("  ✗ {} - {}", file.path.display(), e);
                }
            },
            ChangeItem::Directory(dir) => {
                let result = sync_directory_with_progress(dir, repo_path, worktree_path);
                for (path, res) in result {
                    match res {
                        Ok(()) => {
                            synced_files.push(path);
                        }
                        Err(e) => {
                            failed_files.push((path, e.to_string()));
                        }
                    }
                }
                total_count += dir.files.len();
                println!(
                    "  ✓ {} ({} files - {} synced, {} failed)",
                    dir.path.display(),
                    dir.files.len(),
                    synced_files.len() + failed_files.len() - total_count + dir.files.len(),
                    failed_files.len()
                );
            }
        }
    }

    SyncStats {
        synced_files,
        failed_files,
        total_count,
    }
}

fn create_sync_result(stats: SyncStats, done_message: &str) -> Result<SyncResult> {
    if !stats.failed_files.is_empty() {
        let progress = SyncProgress {
            total_count: stats.total_count,
            synced_files: stats.synced_files,
            failed_files: stats.failed_files,
        };
        println!(
            "\nPartial sync: {} synced, {} failed.",
            progress.synced_files.len(),
            progress.failed_files.len()
        );
        for (path, error) in &progress.failed_files {
            eprintln!("  ✗ {}: {}", path.display(), error);
        }
        Ok(SyncResult::Partial(progress))
    } else {
        println!("\n{}: {} files.", done_message, stats.total_count);
        Ok(SyncResult::Synced(stats.total_count))
    }
}

pub fn sync_all(
    items: &[ChangeItem],
    repo_path: &Path,
    worktree_path: &Path,
) -> Result<SyncResult> {
    println!("\nSyncing all files...");
    let stats = sync_items_internal(items, repo_path, worktree_path);
    create_sync_result(stats, "Synced")
}

pub fn sync_single_file(file: &FileChange, repo_path: &Path, worktree_path: &Path) -> Result<()> {
    use crate::sync::types::ChangeType;
    let worktree_file = worktree_path.join(&file.path);
    let repo_file = repo_path.join(&file.path);

    match file.change_type {
        ChangeType::Deleted => match fs::remove_file(&repo_file) {
            Ok(()) => {}
            Err(e) if e.kind() == ErrorKind::NotFound => {}
            Err(e) => {
                return Err(TreebeardError::Config(format!(
                    "Failed to remove file {}: {}",
                    repo_file.display(),
                    e
                )))
            }
        },
        ChangeType::Added | ChangeType::Modified => {
            if let Some(parent) = repo_file.parent() {
                fs::create_dir_all(parent).map_err(|e| {
                    TreebeardError::Config(format!(
                        "Failed to create directory {}: {}",
                        parent.display(),
                        e
                    ))
                })?;
            }
            fs::copy(&worktree_file, &repo_file).map_err(|e| {
                TreebeardError::Config(format!(
                    "Failed to copy {} to {}: {}",
                    worktree_file.display(),
                    repo_file.display(),
                    e
                ))
            })?;
        }
    }
    Ok(())
}

fn sync_directory_with_progress(
    dir: &DirectoryChange,
    repo_path: &Path,
    worktree_path: &Path,
) -> Vec<(PathBuf, Result<()>)> {
    dir.files
        .iter()
        .map(|file| {
            let path = file.path.clone();
            let result = sync_single_file(file, repo_path, worktree_path);
            (path, result)
        })
        .collect()
}

pub fn sync_selected(
    items: &[ChangeItem],
    selected: &std::collections::HashSet<usize>,
    repo_path: &Path,
    worktree_path: &Path,
) -> Result<SyncResult> {
    let mut to_sync: Vec<ChangeItem> = Vec::new();
    let mut skipped: Vec<ChangeItem> = Vec::new();

    for (idx, item) in items.iter().enumerate() {
        if selected.contains(&idx) {
            to_sync.push(item.clone());
        } else {
            skipped.push(item.clone());
        }
    }

    if to_sync.is_empty() {
        println!("\nNo files selected for sync.");
        return Ok(SyncResult::Skipped);
    }

    println!("\nSync summary:");
    for item in &to_sync {
        match item {
            ChangeItem::File(file) => {
                println!("  ✓ {}", file.path.display());
            }
            ChangeItem::Directory(dir) => {
                println!("  ✓ {} ({} files)", dir.path.display(), dir.files.len());
            }
        }
    }

    if !skipped.is_empty() {
        println!("\n  Skipped:");
        for item in &skipped {
            match item {
                ChangeItem::File(file) => {
                    println!("    • {}", file.path.display());
                }
                ChangeItem::Directory(dir) => {
                    println!("    • {} ({} files)", dir.path.display(), dir.files.len());
                }
            }
        }
    }

    use crate::cleanup::prompt_yes_no;
    if !prompt_yes_no("Proceed?", true)? {
        println!("\nNo files synced.");
        return Ok(SyncResult::Skipped);
    }

    println!("\nSyncing...");
    let stats = sync_items_internal(&to_sync, repo_path, worktree_path);
    create_sync_result(stats, "Done! Synced")
}
