use crate::config::validate_config;
use crate::config::{get_config_path, get_macos_version, Config};
use crate::error::Result;
use crate::session::load_active_sessions;

struct DiagnosticCheck {
    name: String,
    status: DiagnosticStatus,
    details: Option<String>,
}

enum DiagnosticStatus {
    Ok,
    Warning,
    Error,
}

impl DiagnosticCheck {
    fn ok(name: impl Into<String>, details: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            status: DiagnosticStatus::Ok,
            details: Some(details.into()),
        }
    }

    fn warning(name: impl Into<String>, details: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            status: DiagnosticStatus::Warning,
            details: Some(details.into()),
        }
    }

    fn error(name: impl Into<String>, details: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            status: DiagnosticStatus::Error,
            details: Some(details.into()),
        }
    }

    fn symbol(&self) -> &'static str {
        match self.status {
            DiagnosticStatus::Ok => "\u{2713}",
            DiagnosticStatus::Warning => "\u{26a0}",
            DiagnosticStatus::Error => "\u{2717}",
        }
    }
}

pub fn run_doctor() -> Result<()> {
    println!();
    println!("Treebeard Diagnostics");
    println!("=====================");
    println!();

    let mut checks: Vec<DiagnosticCheck> = Vec::new();
    let mut suggestions: Vec<String> = Vec::new();

    checks.push(check_macos_version());

    let macfuse_check = check_macfuse();
    if matches!(macfuse_check.status, DiagnosticStatus::Error) {
        suggestions.push("Install macFUSE: brew install --cask macfuse".to_string());
    }
    checks.push(macfuse_check);

    checks.push(check_git());

    let config_check = check_config();
    if matches!(config_check.status, DiagnosticStatus::Warning) {
        suggestions.push("Create config: git treebeard config edit".to_string());
    }
    checks.push(config_check);

    let (stale_check, stale_count) = check_stale_mounts();
    if stale_count > 0 {
        suggestions.push("Run 'git treebeard cleanup --stale' to remove stale mounts".to_string());
    }
    checks.push(stale_check);

    let disk_check = check_disk_space();
    if matches!(disk_check.status, DiagnosticStatus::Warning) {
        suggestions.push("Free up disk space to ensure treebeard can create worktrees".to_string());
    }
    checks.push(disk_check);

    let session_check = check_active_sessions();
    checks.push(session_check);

    for check in &checks {
        let details = check.details.as_deref().unwrap_or("");
        println!("{} {} - {}", check.symbol(), check.name, details);
    }

    if !suggestions.is_empty() {
        println!();
        println!("Suggestions:");
        for suggestion in &suggestions {
            println!("  -> {}", suggestion);
        }
    }

    println!();

    Ok(())
}

fn check_macos_version() -> DiagnosticCheck {
    #[cfg(target_os = "macos")]
    {
        match get_macos_version() {
            Some((major, minor)) => {
                let macos_name = match major {
                    15 => "Sequoia",
                    14 => "Sonoma",
                    13 => "Ventura",
                    12 => "Monterey",
                    _ => "macOS",
                };
                let version_str = format!("{}.{}", major, minor);
                if major >= 15 {
                    DiagnosticCheck::ok(
                        format!("macOS {} ({})", version_str, macos_name),
                        "supported",
                    )
                } else {
                    DiagnosticCheck::error(
                        format!("macOS {} ({})", version_str, macos_name),
                        "requires macOS 15 (Sequoia) or later",
                    )
                }
            }
            None => DiagnosticCheck::error("macOS version", "could not determine version"),
        }
    }

    #[cfg(not(target_os = "macos"))]
    {
        DiagnosticCheck::error("Operating System", "treebeard requires macOS")
    }
}

fn check_macfuse() -> DiagnosticCheck {
    #[cfg(target_os = "macos")]
    {
        let framework_path = std::path::Path::new("/Library/Frameworks/macFUSE.framework");
        if !framework_path.exists() {
            return DiagnosticCheck::error("macFUSE", "not installed");
        }

        let version = get_macfuse_version().unwrap_or_else(|| "installed".to_string());

        let kext_loaded = std::process::Command::new("kextstat")
            .output()
            .map(|output| {
                let stdout = String::from_utf8_lossy(&output.stdout);
                stdout.contains("macfuse") || stdout.contains("io.macfuse")
            })
            .unwrap_or(false);

        if kext_loaded {
            DiagnosticCheck::ok(format!("macFUSE {}", version), "kernel extension loaded")
        } else {
            DiagnosticCheck::warning(
                format!("macFUSE {}", version),
                "kernel extension not loaded (will load on first mount)",
            )
        }
    }

    #[cfg(not(target_os = "macos"))]
    {
        DiagnosticCheck::error("macFUSE", "only available on macOS")
    }
}

#[cfg(target_os = "macos")]
fn get_macfuse_version() -> Option<String> {
    let plist_path = "/Library/Frameworks/macFUSE.framework/Resources/Info.plist";
    let content = std::fs::read_to_string(plist_path).ok()?;

    let version_key = "CFBundleShortVersionString";
    if let Some(key_pos) = content.find(version_key) {
        let after_key = &content[key_pos..];
        if let Some(string_start) = after_key.find("<string>") {
            let version_start = string_start + "<string>".len();
            if let Some(string_end) = after_key[version_start..].find("</string>") {
                return Some(after_key[version_start..version_start + string_end].to_string());
            }
        }
    }
    None
}

fn check_git() -> DiagnosticCheck {
    let output = std::process::Command::new("git")
        .args(["--version"])
        .output();

    match output {
        Ok(output) => {
            let version_str = String::from_utf8_lossy(&output.stdout).trim().to_string();
            let version = version_str
                .strip_prefix("git version ")
                .unwrap_or(&version_str);

            let worktree_check = std::process::Command::new("git")
                .args(["worktree", "list", "--porcelain"])
                .output();

            match worktree_check {
                Ok(wt_output) => {
                    let code = wt_output.status.code().unwrap_or(-1);
                    if code == 0 || code == 128 {
                        DiagnosticCheck::ok(
                            format!("Git {}", version),
                            "worktree support available",
                        )
                    } else {
                        DiagnosticCheck::warning(
                            format!("Git {}", version),
                            "worktree command not available",
                        )
                    }
                }
                Err(_) => DiagnosticCheck::warning(
                    format!("Git {}", version),
                    "could not check worktree support",
                ),
            }
        }
        Err(_) => DiagnosticCheck::error("Git", "not found in PATH"),
    }
}

fn check_config() -> DiagnosticCheck {
    let config_path = get_config_path();

    if !config_path.exists() {
        return DiagnosticCheck::warning(
            "Config file",
            format!("not found at {}", config_path.display()),
        );
    }

    match std::fs::read_to_string(&config_path) {
        Ok(content) => match toml::from_str::<Config>(&content) {
            Ok(config) => {
                if let Err(e) = validate_config(&config) {
                    DiagnosticCheck::warning("Config file", format!("validation warning: {}", e))
                } else {
                    DiagnosticCheck::ok("Config file", format!("{} (valid)", config_path.display()))
                }
            }
            Err(e) => DiagnosticCheck::error("Config file", format!("parse error: {}", e)),
        },
        Err(e) => DiagnosticCheck::error("Config file", format!("read error: {}", e)),
    }
}

fn check_stale_mounts() -> (DiagnosticCheck, usize) {
    #[cfg(target_os = "macos")]
    {
        let mount_output = match std::process::Command::new("mount").output() {
            Ok(output) => output,
            Err(_) => {
                return (
                    DiagnosticCheck::warning(
                        "Stale mounts",
                        "could not check (mount command failed)",
                    ),
                    0,
                );
            }
        };

        let mount_text = String::from_utf8_lossy(&mount_output.stdout);
        let mount_regex = regex::Regex::new(r"/dev/\S+ on (\S+) \(.*treebeard.*\)").unwrap();

        let stale_mounts: Vec<&str> = mount_text
            .lines()
            .filter_map(|line| {
                mount_regex
                    .captures(line)
                    .and_then(|cap| cap.get(1).map(|m| m.as_str()))
            })
            .collect();

        let count = stale_mounts.len();
        if count == 0 {
            (DiagnosticCheck::ok("Stale mounts", "none detected"), 0)
        } else {
            let details = format!(
                "{} stale mount(s) detected:\n{}",
                count,
                stale_mounts
                    .iter()
                    .map(|p| format!("    - {}", p))
                    .collect::<Vec<_>>()
                    .join("\n")
            );
            (DiagnosticCheck::warning("Stale mounts", details), count)
        }
    }

    #[cfg(not(target_os = "macos"))]
    {
        (
            DiagnosticCheck::ok("Stale mounts", "check not applicable on this platform"),
            0,
        )
    }
}

fn check_disk_space() -> DiagnosticCheck {
    let home = match std::env::var("HOME") {
        Ok(h) => h,
        Err(_) => {
            return DiagnosticCheck::warning("Disk space", "could not determine home directory");
        }
    };

    #[cfg(target_os = "macos")]
    {
        let output = std::process::Command::new("df")
            .args(["-k", &home])
            .output();

        match output {
            Ok(output) => {
                let stdout = String::from_utf8_lossy(&output.stdout);
                if let Some(line) = stdout.lines().nth(1) {
                    let parts: Vec<&str> = line.split_whitespace().collect();
                    if parts.len() >= 4 {
                        if let Ok(available_kb) = parts[3].parse::<u64>() {
                            let available_gb = available_kb / 1024 / 1024;
                            let available_mb = (available_kb / 1024) % 1024;

                            if available_gb >= 10 {
                                return DiagnosticCheck::ok(
                                    "Disk space",
                                    format!("{} GB available", available_gb),
                                );
                            } else if available_gb >= 1 {
                                return DiagnosticCheck::warning(
                                    "Disk space",
                                    format!(
                                        "{}.{} GB available (low)",
                                        available_gb,
                                        available_mb / 100
                                    ),
                                );
                            } else {
                                return DiagnosticCheck::error(
                                    "Disk space",
                                    format!(
                                        "{} MB available (critically low)",
                                        available_kb / 1024
                                    ),
                                );
                            }
                        }
                    }
                }
                DiagnosticCheck::warning("Disk space", "could not parse available space")
            }
            Err(_) => DiagnosticCheck::warning("Disk space", "could not check (df command failed)"),
        }
    }

    #[cfg(not(target_os = "macos"))]
    {
        DiagnosticCheck::ok("Disk space", "check not implemented for this platform")
    }
}

fn check_active_sessions() -> DiagnosticCheck {
    let sessions = match load_active_sessions() {
        Ok(s) => s,
        Err(_) => {
            return DiagnosticCheck::ok("Active sessions", "no session data");
        }
    };

    if sessions.is_empty() {
        return DiagnosticCheck::ok("Active sessions", "none");
    }

    let mut healthy = 0;
    let mut unhealthy = 0;

    for session in &sessions {
        if session.is_healthy() {
            healthy += 1;
        } else {
            unhealthy += 1;
        }
    }

    if unhealthy == 0 {
        DiagnosticCheck::ok("Active sessions", format!("{} session(s) healthy", healthy))
    } else {
        DiagnosticCheck::warning(
            "Active sessions",
            format!(
                "{} healthy, {} unhealthy (may need cleanup)",
                healthy, unhealthy
            ),
        )
    }
}
