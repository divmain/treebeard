use crate::error::{Result, TreebeardError};
use crate::sync::files::{both_files_exist, detect_binary, should_skip_diff};
use crate::sync::types::FileChange;
use similar::{ChangeTag, TextDiff};
use std::fs;
use std::path::Path;

pub fn format_diff(old_content: &str, new_content: &str) -> String {
    let diff = TextDiff::from_lines(old_content, new_content);
    let mut result = String::new();
    for (idx, group) in diff.grouped_ops(3).iter().enumerate() {
        if idx > 0 {
            result.push_str("...\n");
        }
        for op in group {
            for change in diff.iter_changes(op) {
                match change.tag() {
                    ChangeTag::Delete => {
                        result.push('-');
                        result.push_str(change.value());
                    }
                    ChangeTag::Insert => {
                        result.push('+');
                        result.push_str(change.value());
                    }
                    ChangeTag::Equal => {
                        result.push(' ');
                        result.push_str(change.value());
                    }
                }
            }
        }
    }
    result
}

pub enum PreviewResult {
    Shown,
    Skipped,
}

pub fn show_file_preview(
    file: &FileChange,
    repo_file: &Path,
    worktree_file: &Path,
) -> Result<PreviewResult> {
    use crate::sync::types::ChangeType;
    match file.change_type {
        ChangeType::Added => {
            show_new_file(worktree_file, &file.path)?;
        }
        ChangeType::Deleted => {
            show_deleted_file(&file.path)?;
        }
        ChangeType::Modified => {
            if !both_files_exist(repo_file, worktree_file) {
                return Ok(PreviewResult::Skipped);
            }

            if should_skip_diff(repo_file, worktree_file) {
                show_large_file(&file.path, worktree_file)?;
            } else {
                let new_content = fs::read_to_string(worktree_file).map_err(|e| {
                    TreebeardError::Config(format!(
                        "Failed to read file {}: {}",
                        worktree_file.display(),
                        e
                    ))
                })?;
                let orig_content = if repo_file.exists() {
                    fs::read_to_string(repo_file).map_err(|e| {
                        TreebeardError::Config(format!(
                            "Failed to read file {}: {}",
                            repo_file.display(),
                            e
                        ))
                    })?
                } else {
                    String::new()
                };

                if detect_binary(orig_content.as_bytes()) || detect_binary(new_content.as_bytes()) {
                    show_binary_file(&file.path, worktree_file)?;
                } else {
                    let diff_output = format_diff(&orig_content, &new_content);
                    show_diff(&file.path, &diff_output)?;
                }
            }
        }
    }
    Ok(PreviewResult::Shown)
}

fn show_new_file(worktree_file: &Path, path: &Path) -> Result<()> {
    let content = fs::read_to_string(worktree_file).unwrap_or_else(|_| String::new());

    println!("{} — NEW FILE\n", path.display());

    if detect_binary(content.as_bytes()) {
        println!("Cannot display content of binary file.\n");
    } else {
        for line in content.lines() {
            println!("+{}", line);
        }
    }

    println!("\n[y] Create in main repo  [n] Skip  [q] Back: ");
    Ok(())
}

fn show_deleted_file(path: &Path) -> Result<()> {
    println!("{} — DELETED\n", path.display());
    println!("This file exists in main repo but was deleted in the worktree.");
    println!("Syncing will delete it from main repo.\n");
    println!("[y] Delete from main repo  [n] Keep in main repo  [q] Back: ");
    Ok(())
}

fn show_binary_file(path: &Path, worktree_file: &Path) -> Result<()> {
    let metadata = fs::metadata(worktree_file).map_err(|e| {
        TreebeardError::Config(format!(
            "Failed to get metadata for {}: {}",
            worktree_file.display(),
            e
        ))
    })?;
    let size = metadata.len();

    println!("{} — BINARY FILE\n", path.display());
    println!("Cannot display diff for binary files.");
    println!("File size: {} bytes\n", size);
    println!("[y] Sync  [n] Skip  [q] Back: ");
    Ok(())
}

fn show_large_file(path: &Path, worktree_file: &Path) -> Result<()> {
    let metadata = fs::metadata(worktree_file).map_err(|e| {
        TreebeardError::Config(format!(
            "Failed to get metadata for {}: {}",
            worktree_file.display(),
            e
        ))
    })?;
    let size = metadata.len();

    println!("{} — LARGE FILE\n", path.display());
    println!("File is too large for diff comparison ({} bytes).\n", size);
    println!("[y] Sync  [n] Skip  [q] Back: ");
    Ok(())
}

fn show_diff(path: &Path, diff_output: &str) -> Result<()> {
    let lines: Vec<&str> = diff_output.lines().collect();
    let diff_too_large = lines.len() > super::MAX_DIFF_LINES
        || lines.iter().any(|l| l.len() > super::MAX_LINE_LENGTH);

    if diff_too_large {
        println!("{} — LARGE DIFF\n", path.display());
        println!(
            "Diff is too large to display ({} changed lines).\n",
            lines.len()
        );
        println!("[y] Sync  [n] Skip  [q] Back: ");
    } else {
        println!("--- {} (main repo)", path.display());
        println!("+++ {} (worktree)", path.display());
        println!("{}", diff_output);
        println!("\n[y] Sync  [n] Skip  [q] Back: ");
    }

    Ok(())
}
