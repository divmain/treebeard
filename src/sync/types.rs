use crate::overlay::MutationType;
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChangeType {
    Modified,
    Added,
    Deleted,
}

impl ChangeType {
    pub fn as_str(&self) -> &'static str {
        match self {
            ChangeType::Modified => "modified",
            ChangeType::Added => "added",
            ChangeType::Deleted => "deleted",
        }
    }

    pub fn as_prefix(&self) -> &'static str {
        match self {
            ChangeType::Modified => "~",
            ChangeType::Added => "+",
            ChangeType::Deleted => "-",
        }
    }
}

impl From<MutationType> for ChangeType {
    fn from(mt: MutationType) -> Self {
        match mt {
            MutationType::CopiedUp => ChangeType::Modified,
            MutationType::Created => ChangeType::Added,
            MutationType::Deleted => ChangeType::Deleted,
        }
    }
}

#[derive(Debug, Clone)]
pub struct FileChange {
    pub path: PathBuf,
    pub change_type: ChangeType,
}

#[derive(Debug, Clone)]
pub struct DirectoryChange {
    pub path: PathBuf,
    pub files: Vec<FileChange>,
    pub modified_count: usize,
    pub added_count: usize,
    pub deleted_count: usize,
}

#[derive(Debug, Clone)]
pub enum ChangeItem {
    File(FileChange),
    Directory(DirectoryChange),
}

impl std::fmt::Display for ChangeItem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ChangeItem::File(file) => write!(f, "{}", file.path.display()),
            ChangeItem::Directory(dir) => {
                write!(f, "{} ({} files)", dir.path.display(), dir.files.len())
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyncProgress {
    pub synced_files: Vec<PathBuf>,
    pub failed_files: Vec<(PathBuf, String)>,
    pub total_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SyncResult {
    Synced(usize),
    Cancelled,
    Skipped,
    /// Sync partially succeeded - some files were synced, others failed.
    /// The caller should present the failure details to the user.
    Partial(SyncProgress),
    /// Git check-ignore failed, so we couldn't determine which files are gitignored.
    /// The caller should warn the user that modified files may not have been shown for sync,
    /// and require extra confirmation before any destructive actions like worktree deletion.
    GitCheckFailed,
}

pub struct AggregateResult {
    pub items: Vec<ChangeItem>,
    pub symlinks_skipped: usize,
}
