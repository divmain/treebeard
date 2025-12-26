use std::path::PathBuf;
use thiserror::Error;

pub type Result<T> = std::result::Result<T, TreebeardError>;

#[derive(Error, Debug)]
pub enum TreebeardError {
    #[error("Not a git repository: {0}")]
    NotAGitRepository(PathBuf),

    #[error("Git error: {0}")]
    Git(String),

    #[error("Config error: {0}")]
    Config(String),

    #[error("IO error: {0}")]
    Io(std::io::Error),

    #[error("FUSE error: {0}")]
    Fuse(String),

    #[error("Worktree already exists: {0}")]
    WorktreeAlreadyExists(String),

    #[error("Branch already exists: {0}")]
    BranchAlreadyExists(String),

    #[error("Worktree not found: {0}")]
    WorktreeNotFound(String),

    #[error("JSON error: {0}")]
    Json(String),

    #[error("Hook failed: {0}")]
    Hook(String),
}

impl From<serde_json::Error> for TreebeardError {
    fn from(err: serde_json::Error) -> Self {
        TreebeardError::Json(err.to_string())
    }
}
