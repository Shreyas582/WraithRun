use std::{
    collections::VecDeque,
    fs::File,
    io::{BufRead, BufReader, Read},
    path::Path,
};

use sha2::{Digest, Sha256};

use crate::ToolError;

/// Well-known Windows Event Log channel names accepted by `wevtutil qe`.
pub const WINDOWS_EVENT_CHANNELS: &[&str] = &[
    "System",
    "Security",
    "Application",
    "Setup",
    "Microsoft-Windows-PowerShell/Operational",
    "Microsoft-Windows-Sysmon/Operational",
    "Microsoft-Windows-TaskScheduler/Operational",
    "Microsoft-Windows-TerminalServices-LocalSessionManager/Operational",
];

/// Returns true if `path_str` is a known Windows Event Log channel name (not a file path).
pub fn is_windows_event_channel(path_str: &str) -> bool {
    WINDOWS_EVENT_CHANNELS
        .iter()
        .any(|ch| ch.eq_ignore_ascii_case(path_str))
}

/// Read Windows Event Log entries via `wevtutil qe`.
///
/// Accepts either a channel name (e.g. "Security") or an absolute `.evtx` file path.
/// Returns the most recent `max_lines` output lines from `wevtutil`, newest-first.
#[cfg(target_os = "windows")]
pub fn read_windows_event_log(
    channel_or_path: &str,
    max_events: usize,
) -> Result<Vec<String>, ToolError> {
    use std::process::Command;

    let capped = max_events.clamp(1, 500);
    let is_file = channel_or_path.to_ascii_lowercase().ends_with(".evtx")
        || Path::new(channel_or_path).is_absolute();

    let mut cmd = Command::new("wevtutil");
    cmd.arg("qe").arg(channel_or_path);
    cmd.arg("/f:text");
    cmd.arg(format!("/c:{capped}"));
    cmd.arg("/rd:true"); // newest events first
    if is_file {
        cmd.arg("/lf:true");
    }

    let output = cmd
        .output()
        .map_err(|e| ToolError::Execution(format!("wevtutil failed to launch: {e}")))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(ToolError::Execution(format!(
            "wevtutil exited with {}: {}",
            output.status,
            stderr.trim()
        )));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    Ok(stdout.lines().map(|l| l.to_string()).collect())
}

/// Stub for non-Windows builds so callers can compile everywhere.
#[cfg(not(target_os = "windows"))]
pub fn read_windows_event_log(
    _channel_or_path: &str,
    _max_events: usize,
) -> Result<Vec<String>, ToolError> {
    Err(ToolError::Execution(
        "Windows Event Log is only available on Windows".to_string(),
    ))
}

pub fn read_log_tail(path: &Path, max_lines: usize) -> Result<Vec<String>, ToolError> {
    if !path.exists() {
        return Err(ToolError::Execution(format!(
            "log file does not exist: {}",
            path.display()
        )));
    }

    let bounded_max = max_lines.clamp(1, 1000);
    let file = File::open(path)?;
    let reader = BufReader::new(file);
    let mut tail = VecDeque::with_capacity(bounded_max);

    for line in reader.lines() {
        let line = line?;
        if tail.len() == bounded_max {
            let _ = tail.pop_front();
        }
        tail.push_back(line);
    }

    Ok(tail.into_iter().collect())
}

pub fn sha256_file(path: &Path) -> Result<String, ToolError> {
    if !path.exists() {
        return Err(ToolError::Execution(format!(
            "file does not exist: {}",
            path.display()
        )));
    }

    let mut file = File::open(path)?;
    let mut buffer = [0u8; 8192];
    let mut hasher = Sha256::new();

    loop {
        let bytes_read = file.read(&mut buffer)?;
        if bytes_read == 0 {
            break;
        }
        hasher.update(&buffer[..bytes_read]);
    }

    let digest = hasher.finalize();
    Ok(digest.iter().map(|b| format!("{b:02x}")).collect())
}
