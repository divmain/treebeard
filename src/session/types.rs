use serde::{Deserialize, Serialize};
use std::path::Path;
use std::time::Duration;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActiveSession {
    pub repo_path: String,
    pub branch_name: String,
    pub worktree_path: String,
    pub mount_path: String,
    pub start_time: chrono::DateTime<chrono::Utc>,
}

impl ActiveSession {
    pub fn is_healthy(&self) -> bool {
        Path::new(&self.mount_path).exists() && Path::new(&self.worktree_path).exists()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum SessionStatus {
    #[allow(dead_code)] // Planned: detect when shell is actively running
    Active,
    Idle,
    Stale,
}

impl SessionStatus {
    pub fn symbol(&self) -> &'static str {
        match self {
            SessionStatus::Active => "●",
            SessionStatus::Idle => "○",
            SessionStatus::Stale => "↯",
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            SessionStatus::Active => "active",
            SessionStatus::Idle => "idle",
            SessionStatus::Stale => "stale",
        }
    }
}

#[derive(Debug, Clone)]
pub struct SessionDisplay {
    pub branch: String,
    pub status: SessionStatus,
    pub mount_status: String,
    pub dirty_files: usize,
    pub age: Duration,
}
