#[cfg(not(target_os = "windows"))]
use std::{fs, io::ErrorKind};

use std::collections::HashMap;

use serde::Serialize;

#[cfg(target_os = "windows")]
use tokio::process::Command;

use crate::{network_scanner, ToolError};

#[derive(Debug, Clone, Serialize)]
pub struct ProcessNetworkRecord {
    pub protocol: String,
    pub local_address: String,
    pub state: String,
    pub pid: Option<u32>,
    pub process_name: Option<String>,
    pub externally_exposed: bool,
}

pub async fn correlate_process_network(
    limit: usize,
) -> Result<Vec<ProcessNetworkRecord>, ToolError> {
    let listeners = network_scanner::list_local_listeners(limit).await?;

    let mut records = Vec::with_capacity(listeners.len());
    for listener in listeners {
        let process_name = match listener.pid {
            Some(pid) => resolve_process_name(pid).await?,
            None => None,
        };

        records.push(ProcessNetworkRecord {
            protocol: listener.protocol,
            local_address: listener.local_address.clone(),
            state: listener.state,
            pid: listener.pid,
            process_name,
            externally_exposed: is_externally_exposed(&listener.local_address),
        });
    }

    Ok(records)
}

#[cfg(target_os = "windows")]
async fn resolve_process_name(pid: u32) -> Result<Option<String>, ToolError> {
    let filter = format!("PID eq {pid}");
    let output = Command::new("tasklist")
        .args(["/FI", &filter, "/FO", "CSV", "/NH"])
        .output()
        .await?;

    if !output.status.success() {
        return Ok(None);
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    Ok(parse_tasklist_process_name(&stdout, pid))
}

#[cfg(target_os = "windows")]
fn parse_tasklist_process_name(stdout: &str, pid: u32) -> Option<String> {
    let pid_text = pid.to_string();

    for line in stdout.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if trimmed.to_ascii_lowercase().starts_with("info:") {
            return None;
        }

        let columns = parse_csv_row(trimmed);
        if columns.len() < 2 {
            continue;
        }

        if columns[1] == pid_text {
            return Some(columns[0].clone());
        }
    }

    None
}

#[cfg(target_os = "windows")]
fn parse_csv_row(line: &str) -> Vec<String> {
    line.trim()
        .trim_matches('"')
        .split("\",\"")
        .map(|value| value.trim().trim_matches('"').to_string())
        .collect()
}

#[cfg(not(target_os = "windows"))]
async fn resolve_process_name(pid: u32) -> Result<Option<String>, ToolError> {
    let path = format!("/proc/{pid}/comm");
    match fs::read_to_string(path) {
        Ok(name) => {
            let name = name.trim();
            if name.is_empty() {
                Ok(None)
            } else {
                Ok(Some(name.to_string()))
            }
        }
        Err(err) if err.kind() == ErrorKind::NotFound => Ok(None),
        Err(err) => Err(ToolError::Io(err)),
    }
}

fn is_externally_exposed(local_address: &str) -> bool {
    let lower = local_address.to_ascii_lowercase();
    if lower.contains("127.0.0.1")
        || lower.contains("localhost")
        || lower.contains("[::1]")
        || lower.starts_with("::1:")
    {
        return false;
    }

    true
}

// ---------------------------------------------------------------------------
// Process tree analysis (MITRE T1057 / T1059) — #169
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct ProcessTreeEntry {
    pub pid: u32,
    pub ppid: u32,
    pub name: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub command_line: String,
    pub suspicious: bool,
    pub child_pids: Vec<u32>,
}

fn is_suspicious_process(name: &str, command_line: &str) -> bool {
    let haystack = format!("{} {}", name, command_line).to_ascii_lowercase();
    [
        "powershell",
        "pwsh",
        "cmd.exe",
        "wscript",
        "cscript",
        "mshta",
        "rundll32",
        "regsvr32",
        "certutil",
        "bitsadmin",
        "msiexec",
        "base64",
        "/tmp/",
        "\\temp\\",
        "appdata\\local\\temp",
    ]
    .iter()
    .any(|marker| haystack.contains(marker))
}

pub async fn collect_process_tree(limit: usize) -> Result<Vec<ProcessTreeEntry>, ToolError> {
    let bounded = limit.clamp(1, 512);
    let mut entries = Vec::new();

    #[cfg(target_os = "windows")]
    collect_windows_process_tree(&mut entries, bounded).await?;

    #[cfg(not(target_os = "windows"))]
    collect_unix_process_tree(&mut entries, bounded);

    // Populate child_pids from the collected parent relationships.
    let pid_to_idx: HashMap<u32, usize> = entries
        .iter()
        .enumerate()
        .map(|(i, e)| (e.pid, i))
        .collect();

    let ppids: Vec<(u32, u32)> = entries.iter().map(|e| (e.pid, e.ppid)).collect();
    for (pid, ppid) in ppids {
        if let Some(&parent_idx) = pid_to_idx.get(&ppid) {
            entries[parent_idx].child_pids.push(pid);
        }
    }

    Ok(entries)
}

#[cfg(target_os = "windows")]
async fn collect_windows_process_tree(
    entries: &mut Vec<ProcessTreeEntry>,
    limit: usize,
) -> Result<(), ToolError> {
    // wmic process get columns output: Node,CommandLine,Name,ParentProcessId,ProcessId
    let output = Command::new("wmic")
        .args(["process", "get", "ProcessId,ParentProcessId,Name,CommandLine", "/format:csv"])
        .output()
        .await
        .map_err(|e| ToolError::Execution(format!("wmic process query failed: {e}")))?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut header_found = false;
    let mut col_cmdline = 1usize;
    let mut col_name = 2usize;
    let mut col_ppid = 3usize;
    let mut col_pid = 4usize;

    for line in stdout.lines() {
        if entries.len() >= limit {
            break;
        }
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let cols: Vec<&str> = trimmed.split(',').collect();

        if !header_found {
            // First non-empty line is the header: Node,CommandLine,Name,ParentProcessId,ProcessId
            for (i, col) in cols.iter().enumerate() {
                match col.trim().to_ascii_lowercase().as_str() {
                    "commandline" => col_cmdline = i,
                    "name" => col_name = i,
                    "parentprocessid" => col_ppid = i,
                    "processid" => col_pid = i,
                    _ => {}
                }
            }
            header_found = true;
            continue;
        }

        let get = |idx: usize| cols.get(idx).map(|s| s.trim().to_string()).unwrap_or_default();

        let pid: u32 = match get(col_pid).parse() {
            Ok(v) => v,
            Err(_) => continue,
        };
        let ppid: u32 = get(col_ppid).parse().unwrap_or(0);
        let name = get(col_name);
        let command_line = get(col_cmdline);
        let suspicious = is_suspicious_process(&name, &command_line);

        entries.push(ProcessTreeEntry {
            pid,
            ppid,
            name,
            command_line,
            suspicious,
            child_pids: Vec::new(),
        });
    }

    Ok(())
}

#[cfg(not(target_os = "windows"))]
fn collect_unix_process_tree(entries: &mut Vec<ProcessTreeEntry>, limit: usize) {
    let Ok(proc_dir) = fs::read_dir("/proc") else {
        return;
    };

    for entry in proc_dir.flatten() {
        if entries.len() >= limit {
            break;
        }

        let fname = entry.file_name();
        let pid_str = fname.to_string_lossy();
        let Ok(pid) = pid_str.parse::<u32>() else {
            continue;
        };

        let proc_path = entry.path();

        // Read Name and PPid from /proc/<pid>/status
        let status_path = proc_path.join("status");
        let Ok(status) = fs::read_to_string(&status_path) else {
            continue;
        };

        let mut name = String::new();
        let mut ppid = 0u32;
        for line in status.lines() {
            if let Some(v) = line.strip_prefix("Name:\t") {
                name = v.trim().to_string();
            } else if let Some(v) = line.strip_prefix("PPid:\t") {
                ppid = v.trim().parse().unwrap_or(0);
            }
            if !name.is_empty() && ppid > 0 {
                break;
            }
        }

        if name.is_empty() {
            continue;
        }

        // Read command line from /proc/<pid>/cmdline (null-byte delimited)
        let command_line = fs::read(proc_path.join("cmdline"))
            .map(|bytes| {
                bytes
                    .split(|&b| b == 0)
                    .filter(|s| !s.is_empty())
                    .map(|s| String::from_utf8_lossy(s).into_owned())
                    .collect::<Vec<_>>()
                    .join(" ")
            })
            .unwrap_or_default();

        let suspicious = is_suspicious_process(&name, &command_line);

        entries.push(ProcessTreeEntry {
            pid,
            ppid,
            name,
            command_line,
            suspicious,
            child_pids: Vec::new(),
        });
    }
}
