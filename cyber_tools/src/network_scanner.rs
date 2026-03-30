use serde::Serialize;
use tokio::process::Command;

use crate::ToolError;

#[derive(Debug, Clone, Serialize)]
pub struct ListenerRecord {
    pub protocol: String,
    pub local_address: String,
    pub state: String,
    pub pid: Option<u32>,
}

pub async fn list_local_listeners(limit: usize) -> Result<Vec<ListenerRecord>, ToolError> {
    let bounded_limit = limit.clamp(1, 512);

    #[cfg(target_os = "windows")]
    let output = Command::new("netstat").args(["-ano"]).output().await?;

    #[cfg(not(target_os = "windows"))]
    let output = Command::new("sh")
        .arg("-c")
        .arg("ss -tulpen")
        .output()
        .await?;

    if !output.status.success() {
        return Err(ToolError::Execution(format!(
            "socket inventory command failed with status {:?}",
            output.status.code()
        )));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);

    let mut listeners = Vec::new();
    for line in stdout.lines() {
        if listeners.len() >= bounded_limit {
            break;
        }

        if let Some(record) = parse_line(line) {
            listeners.push(record);
        }
    }

    Ok(listeners)
}

#[cfg(target_os = "windows")]
fn parse_line(line: &str) -> Option<ListenerRecord> {
    parse_windows_netstat_line(line)
}

#[cfg(not(target_os = "windows"))]
fn parse_line(line: &str) -> Option<ListenerRecord> {
    parse_unix_ss_line(line)
}

#[cfg(target_os = "windows")]
fn parse_windows_netstat_line(line: &str) -> Option<ListenerRecord> {
    let trimmed = line.trim();
    if !(trimmed.starts_with("TCP") || trimmed.starts_with("UDP")) {
        return None;
    }

    let parts: Vec<&str> = trimmed.split_whitespace().collect();
    if parts.len() < 4 {
        return None;
    }

    let protocol = parts[0].to_string();
    let local_address = parts[1].to_string();

    if parts[0] == "TCP" {
        if parts.len() < 5 {
            return None;
        }

        let state = parts[3].to_string();
        let pid = parts[4].parse::<u32>().ok();

        return Some(ListenerRecord {
            protocol,
            local_address,
            state,
            pid,
        });
    }

    let pid = parts.last().and_then(|v| v.parse::<u32>().ok());
    Some(ListenerRecord {
        protocol,
        local_address,
        state: "N/A".to_string(),
        pid,
    })
}

#[cfg(not(target_os = "windows"))]
fn parse_unix_ss_line(line: &str) -> Option<ListenerRecord> {
    let trimmed = line.trim();
    if !(trimmed.starts_with("tcp") || trimmed.starts_with("udp")) {
        return None;
    }

    let parts: Vec<&str> = trimmed.split_whitespace().collect();
    if parts.len() < 6 {
        return None;
    }

    let pid = extract_pid(trimmed);
    Some(ListenerRecord {
        protocol: parts[0].to_uppercase(),
        state: parts[1].to_uppercase(),
        local_address: parts[4].to_string(),
        pid,
    })
}

#[cfg(not(target_os = "windows"))]
fn extract_pid(line: &str) -> Option<u32> {
    let marker = "pid=";
    let start = line.find(marker)? + marker.len();
    let digits: String = line[start..]
        .chars()
        .take_while(|ch| ch.is_ascii_digit())
        .collect();
    digits.parse::<u32>().ok()
}
