use crate::error::Result;
use crate::git::GitRepo;
use crate::session::{load_active_sessions, SessionDisplay, SessionStatus};
use std::path::Path;
use std::time::Duration;

/// Get the count of dirty (uncommitted) files in a worktree
fn get_worktree_dirty_files_count(worktree_path: &Path) -> usize {
    GitRepo::from_path(worktree_path)
        .ok()
        .and_then(|repo| repo.get_dirty_files_count().ok())
        .unwrap_or(0)
}

fn format_age(duration: Duration) -> String {
    let total_secs = duration.as_secs();
    let hours = total_secs / 3600;
    let minutes = (total_secs % 3600) / 60;
    let seconds = total_secs % 60;

    if hours > 0 {
        format!("{}h {}m", hours, minutes)
    } else if minutes > 0 {
        format!("{}m {}s", minutes, seconds)
    } else {
        format!("{}s", seconds)
    }
}

struct SessionInfo<'a> {
    session: &'a crate::session::ActiveSession,
    is_mounted: bool,
    dirty_files_count: usize,
}

pub fn list_active_sessions(porcelain: bool, json: bool) -> Result<()> {
    let repo = GitRepo::discover()?;

    let sessions = load_active_sessions()?;
    let repo_path_str = repo.workdir().to_string_lossy().to_string();

    let repo_name = repo.repo_name();

    let repo_sessions: Vec<_> = sessions
        .into_iter()
        .filter(|s| s.repo_path == repo_path_str)
        .collect();

    let session_infos: Vec<_> = repo_sessions
        .iter()
        .map(|session| SessionInfo {
            session,
            is_mounted: Path::new(&session.mount_path).exists(),
            dirty_files_count: get_worktree_dirty_files_count(Path::new(&session.worktree_path)),
        })
        .collect();

    if json {
        let sessions_display: Vec<_> = session_infos
            .iter()
            .map(|info| {
                serde_json::json!({
                    "branch": info.session.branch_name,
                    "mount_path": info.session.mount_path,
                    "status": if info.is_mounted { "mounted" } else { "unmounted" },
                    "dirty_files": info.dirty_files_count,
                })
            })
            .collect();

        println!("{}", serde_json::to_string(&sessions_display)?);
    } else if porcelain {
        for info in &session_infos {
            let mount_status = if info.is_mounted {
                "mounted"
            } else {
                "unmounted"
            };

            println!(
                "{}\t{}\t{}\t{}",
                info.session.branch_name,
                info.session.mount_path,
                mount_status,
                info.dirty_files_count
            );
        }
    } else {
        if session_infos.is_empty() {
            println!("Active sessions for: {}", repo_name);
            println!();
            println!("  (No active worktrees)");
            return Ok(());
        }

        let sessions_display: Vec<SessionDisplay> = session_infos
            .iter()
            .map(|info| {
                let status = if info.session.is_healthy() {
                    SessionStatus::Idle
                } else {
                    SessionStatus::Stale
                };

                let now = chrono::Utc::now();
                let age = now.signed_duration_since(info.session.start_time);
                let age_duration = std::time::Duration::from_secs(age.num_seconds().unsigned_abs());

                SessionDisplay {
                    branch: info.session.branch_name.clone(),
                    status,
                    mount_status: if info.is_mounted {
                        "mounted".to_string()
                    } else {
                        "unmounted".to_string()
                    },
                    dirty_files: info.dirty_files_count,
                    age: age_duration,
                }
            })
            .collect();

        println!("Active sessions for: {}", repo_name);
        println!();

        let branch_header_width = sessions_display
            .iter()
            .map(|s| s.branch.len())
            .max()
            .unwrap_or(0)
            .max(16);
        let status_header_width = 16;
        let mount_header_width = 10;
        let files_header_width = 8;
        let age_header_width = 10;

        println!(
            "{:<width_branch$}{:<width_status$}{:<width_mount$}{:<width_files$}{:<width_age$}",
            "BRANCH",
            "STATUS",
            "MOUNT",
            "FILES",
            "AGE",
            width_branch = branch_header_width,
            width_status = status_header_width,
            width_mount = mount_header_width,
            width_files = files_header_width,
            width_age = age_header_width,
        );

        let separator_width = branch_header_width
            + status_header_width
            + mount_header_width
            + files_header_width
            + age_header_width;
        println!("{}", "─".repeat(separator_width));

        for session in &sessions_display {
            let age_str = format_age(session.age);
            println!(
                "{:<width_branch$}{} {:<width_status$}{:<width_mount$}{:<width_files$}{:>width_age$}",
                session.branch,
                session.status.symbol(),
                session.status.as_str(),
                session.mount_status,
                session.dirty_files,
                age_str,
                width_branch = branch_header_width,
                width_status = status_header_width - 2,
                width_mount = mount_header_width,
                width_files = files_header_width,
                width_age = age_header_width,
            );
        }

        println!();
        println!("● active (shell running)  ○ idle (mounted, no shell)  ↯ stale (needs cleanup)");
    }

    Ok(())
}
