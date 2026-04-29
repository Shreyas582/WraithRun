use std::{
    env, fs,
    path::{Path, PathBuf},
};

use serde::Serialize;

#[cfg(target_os = "windows")]
use tokio::process::Command;

use crate::ToolError;

#[derive(Debug, Clone, Serialize)]
pub struct PersistenceEntry {
    pub location: String,
    pub kind: String,
    pub entry: String,
    pub suspicious: bool,
}

pub async fn collect_persistence_entries(limit: usize) -> Result<Vec<PersistenceEntry>, ToolError> {
    let bounded_limit = limit.clamp(1, 512);
    let mut entries = Vec::new();

    #[cfg(target_os = "windows")]
    collect_windows_entries(&mut entries, bounded_limit).await?;

    #[cfg(not(target_os = "windows"))]
    collect_unix_entries(&mut entries, bounded_limit);

    entries.truncate(bounded_limit);
    Ok(entries)
}

fn push_entry(
    entries: &mut Vec<PersistenceEntry>,
    location: &str,
    kind: &str,
    entry: String,
    limit: usize,
) {
    if entries.len() >= limit {
        return;
    }

    entries.push(PersistenceEntry {
        location: location.to_string(),
        kind: kind.to_string(),
        suspicious: is_suspicious_persistence_entry(&entry),
        entry,
    });
}

fn scan_directory_entries(
    location: &str,
    kind: &str,
    path: &Path,
    entries: &mut Vec<PersistenceEntry>,
    limit: usize,
) {
    if entries.len() >= limit {
        return;
    }

    let Ok(dir_entries) = fs::read_dir(path) else {
        return;
    };

    for item in dir_entries.flatten() {
        if entries.len() >= limit {
            break;
        }

        let name = item.file_name().to_string_lossy().to_string();
        if name.trim().is_empty() {
            continue;
        }

        push_entry(entries, location, kind, name, limit);
    }
}

#[cfg(target_os = "windows")]
async fn collect_windows_entries(
    entries: &mut Vec<PersistenceEntry>,
    limit: usize,
) -> Result<(), ToolError> {
    let startup_paths = windows_startup_paths();
    for path in startup_paths {
        let location = path.display().to_string();
        scan_directory_entries(&location, "startup_directory", &path, entries, limit);
        if entries.len() >= limit {
            return Ok(());
        }
    }

    for key in windows_run_keys() {
        if entries.len() >= limit {
            break;
        }

        let output = Command::new("reg").args(["query", key]).output().await?;

        if !output.status.success() {
            continue;
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        for line in stdout.lines() {
            if entries.len() >= limit {
                break;
            }

            let trimmed = line.trim();
            if trimmed.is_empty() || !trimmed.contains("REG_") {
                continue;
            }

            push_entry(
                entries,
                key,
                "run_key",
                normalize_registry_line(trimmed),
                limit,
            );
        }
    }

    Ok(())
}

#[cfg(not(target_os = "windows"))]
fn collect_unix_entries(entries: &mut Vec<PersistenceEntry>, limit: usize) {
    for directory in unix_persistence_directories() {
        let path = Path::new(directory);
        scan_directory_entries(directory, "system_directory", path, entries, limit);
        if entries.len() >= limit {
            return;
        }
    }

    if let Some(home_autostart) = home_autostart_path() {
        let location = home_autostart.display().to_string();
        scan_directory_entries(&location, "user_autostart", &home_autostart, entries, limit);
    }

    // User-level systemd units (#126).
    if let Ok(home) = env::var("HOME") {
        let user_systemd = PathBuf::from(&home).join(".config/systemd/user");
        if user_systemd.is_dir() {
            let location = user_systemd.display().to_string();
            scan_directory_entries(&location, "user_systemd", &user_systemd, entries, limit);
        }
    }

    // User crontab via /var/spool/cron (#126).
    for crontab_dir in &["/var/spool/cron/crontabs", "/var/spool/cron"] {
        let path = Path::new(crontab_dir);
        if path.is_dir() {
            scan_directory_entries(crontab_dir, "user_crontab", path, entries, limit);
        }
    }
}

#[cfg(target_os = "windows")]
fn windows_startup_paths() -> Vec<PathBuf> {
    let mut paths = vec![PathBuf::from(
        r"C:\ProgramData\Microsoft\Windows\Start Menu\Programs\StartUp",
    )];

    if let Ok(app_data) = env::var("APPDATA") {
        paths.push(PathBuf::from(app_data).join(r"Microsoft\Windows\Start Menu\Programs\Startup"));
    }

    paths
}

#[cfg(target_os = "windows")]
fn windows_run_keys() -> &'static [&'static str] {
    &[
        r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run",
        r"HKLM\Software\Microsoft\Windows\CurrentVersion\Run",
        // RunOnce keys (#126)
        r"HKCU\Software\Microsoft\Windows\CurrentVersion\RunOnce",
        r"HKLM\Software\Microsoft\Windows\CurrentVersion\RunOnce",
        // Winlogon persistence (#126)
        r"HKLM\SOFTWARE\Microsoft\Windows NT\CurrentVersion\Winlogon",
        // Image File Execution Options / debugger hijack (#126)
        r"HKLM\SOFTWARE\Microsoft\Windows NT\CurrentVersion\Image File Execution Options",
        // AppInit_DLLs (#126)
        r"HKLM\SOFTWARE\Microsoft\Windows NT\CurrentVersion\Windows",
    ]
}

#[cfg(target_os = "windows")]
fn normalize_registry_line(line: &str) -> String {
    line.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(not(target_os = "windows"))]
fn unix_persistence_directories() -> &'static [&'static str] {
    &[
        "/etc/cron.d",
        "/etc/cron.daily",
        "/etc/cron.hourly",
        "/etc/systemd/system",
        "/etc/init.d",
        // Additional persistence locations (#126)
        "/etc/cron.weekly",
        "/etc/cron.monthly",
        "/etc/xdg/autostart",
        "/usr/lib/systemd/system",
    ]
}

#[cfg(not(target_os = "windows"))]
fn home_autostart_path() -> Option<PathBuf> {
    let home = env::var("HOME").ok()?;
    Some(PathBuf::from(home).join(".config").join("autostart"))
}

fn is_suspicious_persistence_entry(entry: &str) -> bool {
    let lower = entry.to_ascii_lowercase();
    [
        "temp",
        "appdata",
        "powershell",
        "cmd.exe",
        "rundll32",
        "wscript",
        "cscript",
        // Additional suspicious markers (#126)
        "mshta",
        "regsvr32",
        "certutil",
        "bitsadmin",
        "msiexec",
        "base64",
        "/tmp/",
    ]
    .iter()
    .any(|marker| lower.contains(marker))
}

// ---------------------------------------------------------------------------
// Scheduled task / cron enumeration (MITRE T1053) — #171
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct ScheduledTaskEntry {
    /// Platform-specific source (e.g. "schtasks", "crontab", "cron.d", "at").
    pub source: String,
    /// Human-readable name or file path of the task.
    pub name: String,
    /// The command / action string.
    pub command: String,
    /// Trigger / schedule description.
    pub schedule: String,
    /// True if any suspicious command patterns are detected.
    pub suspicious: bool,
}

pub async fn enumerate_scheduled_tasks(limit: usize) -> Result<Vec<ScheduledTaskEntry>, ToolError> {
    let bounded = limit.clamp(1, 512);
    let mut tasks = Vec::new();

    #[cfg(target_os = "windows")]
    collect_windows_scheduled_tasks(&mut tasks, bounded).await?;

    #[cfg(not(target_os = "windows"))]
    collect_unix_cron_tasks(&mut tasks, bounded);

    tasks.truncate(bounded);
    Ok(tasks)
}

#[cfg(target_os = "windows")]
async fn collect_windows_scheduled_tasks(
    tasks: &mut Vec<ScheduledTaskEntry>,
    limit: usize,
) -> Result<(), ToolError> {
    let output = Command::new("schtasks")
        .args(["/query", "/fo", "CSV", "/nh"])
        .output()
        .await
        .map_err(|e| ToolError::Execution(format!("schtasks query failed: {e}")))?;

    // schtasks returns one row per next-run-time; a task with N triggers gets N rows.
    // Deduplicate by task name so each task appears exactly once.
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();

    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout.lines() {
        if tasks.len() >= limit {
            break;
        }
        // CSV columns: "TaskName","Next Run Time","Status"
        let cols: Vec<&str> = line.splitn(3, ',').map(|s| s.trim_matches('"')).collect();
        if cols.is_empty() || cols[0].is_empty() {
            continue;
        }
        let name = cols[0].trim().to_string();
        if !seen.insert(name.clone()) {
            continue; // duplicate trigger row for an already-processed task
        }
        let schedule = cols.get(1).unwrap_or(&"").trim().to_string();
        // Fetch the task action via a second query on the task name.
        let command = match Command::new("schtasks")
            .args(["/query", "/tn", &name, "/fo", "LIST", "/v"])
            .output()
            .await
        {
            Ok(cmd_out) => {
                let cmd_text = String::from_utf8_lossy(&cmd_out.stdout);
                cmd_text
                    .lines()
                    .find(|l| l.trim_start().starts_with("Task To Run:"))
                    .and_then(|l| l.split_once(':').map(|(_, v)| v))
                    .map(|s| s.trim().to_string())
                    .unwrap_or_default()
            }
            Err(_) => String::new(),
        };

        tasks.push(ScheduledTaskEntry {
            source: "schtasks".to_string(),
            suspicious: is_suspicious_persistence_entry(&command),
            name,
            command,
            schedule,
        });
    }
    Ok(())
}

#[cfg(not(target_os = "windows"))]
fn collect_unix_cron_tasks(tasks: &mut Vec<ScheduledTaskEntry>, limit: usize) {
    // System-wide crontab
    if let Ok(content) = fs::read_to_string("/etc/crontab") {
        parse_crontab_lines("/etc/crontab", "crontab", &content, tasks, limit);
    }

    // cron.d and periodic directories
    for dir in &[
        "/etc/cron.d",
        "/etc/cron.daily",
        "/etc/cron.hourly",
        "/etc/cron.weekly",
        "/etc/cron.monthly",
    ] {
        if let Ok(entries) = fs::read_dir(dir) {
            for entry in entries.flatten() {
                if tasks.len() >= limit {
                    return;
                }
                let path = entry.path();
                if let Ok(content) = fs::read_to_string(&path) {
                    parse_crontab_lines(&path.to_string_lossy(), "cron.d", &content, tasks, limit);
                }
            }
        }
    }

    // User crontabs
    for spool_dir in &["/var/spool/cron/crontabs", "/var/spool/cron"] {
        if let Ok(entries) = fs::read_dir(spool_dir) {
            for entry in entries.flatten() {
                if tasks.len() >= limit {
                    return;
                }
                let path = entry.path();
                if let Ok(content) = fs::read_to_string(&path) {
                    parse_crontab_lines(
                        &path.to_string_lossy(),
                        "user_crontab",
                        &content,
                        tasks,
                        limit,
                    );
                }
            }
        }
    }

    // systemd timer units
    for unit_dir in &["/etc/systemd/system", "/usr/lib/systemd/system"] {
        if let Ok(entries) = fs::read_dir(unit_dir) {
            for entry in entries.flatten() {
                if tasks.len() >= limit {
                    return;
                }
                let path = entry.path();
                if path.to_string_lossy().ends_with(".timer") {
                    let name = path
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .to_string();
                    tasks.push(ScheduledTaskEntry {
                        source: "systemd_timer".to_string(),
                        suspicious: false,
                        name: name.clone(),
                        command: path.to_string_lossy().to_string(),
                        schedule: "systemd timer".to_string(),
                    });
                }
            }
        }
    }
}

#[cfg(not(target_os = "windows"))]
fn parse_crontab_lines(
    source: &str,
    kind: &str,
    content: &str,
    tasks: &mut Vec<ScheduledTaskEntry>,
    limit: usize,
) {
    for line in content.lines() {
        if tasks.len() >= limit {
            return;
        }
        let trimmed = line.trim();
        // Skip comments and empty lines
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        // Skip environment-variable lines (KEY=VALUE)
        if trimmed.contains('=') && !trimmed.starts_with("*/") && !trimmed.starts_with('@') {
            let before_eq = trimmed.split('=').next().unwrap_or("").trim();
            if !before_eq.contains(' ') {
                continue;
            }
        }
        // @reboot/@daily/etc. shorthand
        let (schedule, command) = if trimmed.starts_with('@') {
            let mut parts = trimmed.splitn(2, ' ');
            (
                parts.next().unwrap_or("").to_string(),
                parts.next().unwrap_or("").to_string(),
            )
        } else {
            // Standard 5-field crontab: m h dom mon dow command
            let fields: Vec<&str> = trimmed.splitn(6, ' ').collect();
            if fields.len() < 6 {
                continue;
            }
            let sched = fields[..5].join(" ");
            let cmd = fields[5].to_string();
            (sched, cmd)
        };

        tasks.push(ScheduledTaskEntry {
            source: format!("{kind}:{source}"),
            suspicious: is_suspicious_persistence_entry(&command),
            name: command.split_whitespace().next().unwrap_or("").to_string(),
            command,
            schedule,
        });
    }
}
