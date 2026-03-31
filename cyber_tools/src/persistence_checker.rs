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
    ]
    .iter()
    .any(|marker| lower.contains(marker))
}
