use crate::config::get_config_dir;
use crate::error::{Result, TreebeardError};
use crate::session::types::ActiveSession;
use fs2::FileExt;
use std::fs::OpenOptions;
use std::path::Path;

pub fn get_session_state_path() -> Result<std::path::PathBuf> {
    let config_dir = get_config_dir()?;
    Ok(config_dir.join("active_sessions.json"))
}

pub fn load_active_sessions() -> Result<Vec<ActiveSession>> {
    let sessions_path = get_session_state_path()?;

    if !sessions_path.exists() {
        return Ok(Vec::new());
    }

    let file = OpenOptions::new()
        .read(true)
        .open(&sessions_path)
        .map_err(|e| TreebeardError::Config(format!("Failed to open session state: {}", e)))?;

    file.try_lock_shared()
        .map_err(|e| TreebeardError::Config(format!("Failed to acquire read lock: {}", e)))?;

    let content = std::fs::read_to_string(&sessions_path)
        .map_err(|e| TreebeardError::Config(format!("Failed to read session state: {}", e)))?;

    let result = serde_json::from_str::<Vec<ActiveSession>>(&content)
        .map_err(|e| TreebeardError::Config(format!("Failed to parse session state: {}", e)));

    if let Err(e) = file.unlock() {
        tracing::warn!("Failed to release read lock: {}", e);
    }

    result
}

pub fn save_active_sessions(sessions: &[ActiveSession]) -> Result<()> {
    let sessions_path = get_session_state_path()?;

    if let Some(parent) = sessions_path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| {
            TreebeardError::Config(format!("Failed to create config directory: {}", e))
        })?;
    }

    let file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(&sessions_path)
        .map_err(|e| TreebeardError::Config(format!("Failed to open session state: {}", e)))?;

    file.try_lock_exclusive()
        .map_err(|e| TreebeardError::Config(format!("Failed to acquire write lock: {}", e)))?;

    let content = serde_json::to_string_pretty(sessions)
        .map_err(|e| TreebeardError::Config(format!("Failed to serialize session state: {}", e)))?;

    std::fs::write(&sessions_path, content)
        .map_err(|e| TreebeardError::Config(format!("Failed to write session state: {}", e)))?;

    if let Err(e) = file.unlock() {
        tracing::warn!("Failed to release write lock: {}", e);
    }

    Ok(())
}

pub fn add_active_session(
    repo_path: &std::path::Path,
    branch_name: &str,
    worktree_path: &std::path::Path,
    mount_path: &std::path::Path,
) -> Result<()> {
    let mut sessions = load_active_sessions()?;

    let session = ActiveSession {
        repo_path: repo_path.to_string_lossy().to_string(),
        branch_name: branch_name.to_string(),
        worktree_path: worktree_path.to_string_lossy().to_string(),
        mount_path: mount_path.to_string_lossy().to_string(),
        start_time: chrono::Utc::now(),
    };

    sessions.push(session);
    save_active_sessions(&sessions)?;
    tracing::info!("Session state saved for branch: {}", branch_name);

    Ok(())
}

pub fn remove_active_session(repo_path: &Path, branch_name: &str) -> Result<()> {
    let sessions_path = get_session_state_path()?;

    if !sessions_path.exists() {
        return Ok(());
    }

    let content = std::fs::read_to_string(&sessions_path)
        .map_err(|e| TreebeardError::Config(format!("Failed to read session state: {}", e)))?;

    let mut sessions: Vec<ActiveSession> = serde_json::from_str(&content)
        .map_err(|e| TreebeardError::Config(format!("Failed to parse session state: {}", e)))?;

    let repo_path_str = repo_path.to_string_lossy().to_string();
    let branch_name_str = branch_name.to_string();

    let original_len = sessions.len();
    sessions.retain(|s| !(s.repo_path == repo_path_str && s.branch_name == branch_name_str));

    if sessions.len() < original_len {
        save_active_sessions(&sessions)?;
        tracing::info!("Session state removed for branch: {}", branch_name);
    }

    Ok(())
}
