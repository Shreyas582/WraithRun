#[cfg(not(target_os = "windows"))]
use std::{fs, io::ErrorKind};

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
