use crate::config::SyncConfig;
use crate::error::{Result, TreebeardError};
use crate::overlay::MutationType;
use crate::sync::aggregation::{aggregate_changes, filter_syncable_items};
use crate::sync::display::show_file_preview;
use crate::sync::ops::{sync_all, sync_single_file};
use crate::sync::tui::{install_panic_hook, run_interactive_selection};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

pub mod aggregation;
pub mod config;
pub mod display;
pub mod files;
pub mod ops;
pub mod tui;
pub mod types;

pub const MAX_VIEWABLE_FILES: usize = 50;
pub const MAX_DIFF_LINES: usize = 15;
pub const MAX_DIFF_FILE_SIZE: u64 = 100 * 1024;
pub const MAX_LINE_LENGTH: usize = 200;

pub use crate::sync::types::{ChangeItem, FileChange, SyncResult};

pub fn run_sync_flow(
    mutations: &HashMap<PathBuf, MutationType>,
    repo_path: &Path,
    worktree_path: &Path,
    sync_config: &SyncConfig,
) -> Result<SyncResult> {
    install_panic_hook();

    let aggregate_result = aggregate_changes(mutations, repo_path, worktree_path);

    if aggregate_result.symlinks_skipped > 0 {
        println!(
            "Note: {} symlink(s) skipped (symlinks cannot be synced)",
            aggregate_result.symlinks_skipped
        );
    }

    if aggregate_result.items.is_empty() {
        println!("No ignored files were modified.");
        return Ok(SyncResult::Skipped);
    }

    let filtered = match filter_syncable_items(aggregate_result.items, repo_path, sync_config) {
        Ok(f) => f,
        Err(e) => {
            // Git check-ignore failed - we couldn't determine which files are gitignored.
            // This is a serious issue because modified files may exist that the user should
            // have the opportunity to sync. We return GitCheckFailed so the caller can
            // handle this appropriately (e.g., require extra confirmation before deletion).
            eprintln!();
            eprintln!("Warning: Could not determine which files are gitignored.");
            eprintln!("         Reason: {}", e);
            eprintln!();
            eprintln!("         Some modified files may not have been shown for sync.");
            eprintln!();
            return Ok(SyncResult::GitCheckFailed);
        }
    };

    if filtered.items_to_show.is_empty() {
        if !filtered.auto_skipped.is_empty() {
            println!("All modified ignored files were auto-skipped per your config settings.");
            for item in &filtered.auto_skipped {
                match item {
                    ChangeItem::File(file) => {
                        println!("  Skipped: {}", file.path.display());
                    }
                    ChangeItem::Directory(dir) => {
                        println!(
                            "  Skipped: {} ({} files)",
                            dir.path.display(),
                            dir.files.len()
                        );
                    }
                }
            }
        } else {
            println!("No ignored files were modified.");
        }
        return Ok(SyncResult::Skipped);
    }

    if filtered.items_to_show.len() == 1 {
        if let ChangeItem::File(file) = &filtered.items_to_show[0] {
            return handle_single_file(file, repo_path, worktree_path);
        }
    }

    run_sync_summary(
        &filtered.items_to_show,
        &filtered.auto_skipped,
        repo_path,
        worktree_path,
        sync_config,
    )
}

fn handle_single_file(
    file: &FileChange,
    repo_path: &Path,
    worktree_path: &Path,
) -> Result<SyncResult> {
    use crate::cleanup::prompt_yes_no;
    use crate::sync::display::PreviewResult;

    println!("1 ignored file was modified:\n");

    let worktree_file = worktree_path.join(&file.path);
    let repo_file = repo_path.join(&file.path);

    if let PreviewResult::Skipped = show_file_preview(file, &repo_file, &worktree_file)? {
        return Ok(SyncResult::Skipped);
    }

    if prompt_yes_no("Sync back to main repo?", false)? {
        sync_single_file(file, repo_path, worktree_path)?;
        println!("\n✓ Synced {}", file.path.display());
        Ok(SyncResult::Synced(1))
    } else {
        println!("\nNo files synced.");
        Ok(SyncResult::Skipped)
    }
}

fn run_sync_summary(
    items: &[ChangeItem],
    auto_skipped: &[ChangeItem],
    repo_path: &Path,
    worktree_path: &Path,
    sync_config: &SyncConfig,
) -> Result<SyncResult> {
    println!("The following ignored files were modified:\n");

    for item in items {
        match item {
            ChangeItem::File(file) => {
                println!(
                    "  [file] {:30} {}",
                    file.path.display(),
                    file.change_type.as_str()
                );
            }
            ChangeItem::Directory(dir) => {
                let count = dir.files.len();
                println!(
                    "  [dir]  {:30} {} files (+{} added, ~{} modified, -{} deleted)",
                    dir.path.display(),
                    count,
                    dir.added_count,
                    dir.modified_count,
                    dir.deleted_count
                );
            }
        }
    }

    if !auto_skipped.is_empty() {
        println!("\n  Auto-skipped (per global config):");
        for item in auto_skipped {
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

    println!("\nSync changes back to main repo? [a]ll / [s]elect interactive / [N]one (N): ");

    let mut input = String::new();
    std::io::stdin()
        .read_line(&mut input)
        .map_err(|e| TreebeardError::Config(format!("Failed to read input: {}", e)))?;
    let choice = input.trim().to_lowercase();
    match choice.as_str() {
        "a" => sync_all(items, repo_path, worktree_path),
        "s" => run_interactive_selection(items, repo_path, worktree_path, sync_config),
        "" | "n" => {
            println!("\nNo files synced.");
            Ok(SyncResult::Skipped)
        }
        _ => {
            println!("\nInvalid choice. No files synced.");
            Ok(SyncResult::Skipped)
        }
    }
}
