use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::{Mutex, RwLock};
use uuid::Uuid;

use core_engine::RunReport;
use serde::{Deserialize, Serialize};

use crate::audit::{AuditLog, AuditLogConfig};
use crate::data_store::DataStore;

/// Server configuration for `wraithrun serve`.
#[derive(Debug, Clone)]
pub struct ServerConfig {
    /// Port to listen on. Default: 8080.
    pub port: u16,
    /// Bind address. Always 127.0.0.1 for security.
    pub bind_addr: [u8; 4],
    /// Maximum concurrent runs allowed.
    pub max_concurrent_runs: usize,
    /// Bearer token for API authentication. Auto-generated if not provided.
    pub api_token: String,
    /// Maximum request body size in bytes. Default: 1 MiB.
    pub max_request_body_bytes: usize,
    /// Path to the SQLite database file. If None, uses in-memory storage only.
    pub database_path: Option<PathBuf>,
    /// Path to the audit log file. If None, audit events are kept in-memory only.
    pub audit_log_path: Option<PathBuf>,
    /// Names of loaded plugin tools (populated at startup).
    pub plugin_tool_names: Vec<String>,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            port: 8080,
            bind_addr: [127, 0, 0, 1],
            max_concurrent_runs: 4,
            api_token: Uuid::new_v4().to_string(),
            max_request_body_bytes: 1_048_576,
            database_path: None,
            audit_log_path: None,
            plugin_tool_names: Vec::new(),
        }
    }
}

/// Status of an investigation run.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RunStatus {
    Queued,
    Running,
    Completed,
    Failed,
    Cancelled,
}

/// A tracked investigation run.
#[derive(Debug, Clone, Serialize)]
pub struct RunEntry {
    pub id: Uuid,
    pub task: String,
    pub status: RunStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub report: Option<RunReport>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    pub created_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub case_id: Option<Uuid>,
}

/// Status of an investigation case.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CaseStatus {
    Open,
    Investigating,
    Closed,
}

/// A tracked investigation case that groups related runs.
#[derive(Debug, Clone, Serialize)]
pub struct CaseEntry {
    pub id: Uuid,
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub status: CaseStatus,
    pub created_at: String,
    pub updated_at: String,
    pub run_count: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_severity: Option<String>,
}

/// Shared application state visible to all route handlers.
#[derive(Clone)]
pub struct AppState {
    pub runs: Arc<RwLock<HashMap<Uuid, RunEntry>>>,
    pub active_run_count: Arc<Mutex<usize>>,
    pub config: ServerConfig,
    pub started_at: String,
    pub db: Option<DataStore>,
    pub audit: AuditLog,
}

impl AppState {
    pub fn new(config: ServerConfig) -> Self {
        let db = config
            .database_path
            .as_ref()
            .map(|path| DataStore::open(path).expect("failed to open database"));
        let audit = AuditLog::new(AuditLogConfig {
            file_path: config.audit_log_path.clone(),
            max_buffer: None,
        })
        .expect("failed to open audit log");
        Self {
            runs: Arc::new(RwLock::new(HashMap::new())),
            active_run_count: Arc::new(Mutex::new(0)),
            config,
            started_at: chrono_now(),
            db,
            audit,
        }
    }
}

/// Returns the current UTC time as an ISO-8601 string (e.g. `"2026-04-24T15:30:00Z"`).
pub fn chrono_now() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    secs_to_iso8601(secs)
}

fn is_leap_year(y: u64) -> bool {
    (y % 4 == 0 && y % 100 != 0) || y % 400 == 0
}

pub(crate) fn secs_to_iso8601(secs: u64) -> String {
    let s = secs % 60;
    let m = (secs / 60) % 60;
    let h = (secs / 3600) % 24;
    let mut days = secs / 86400;

    let mut year = 1970u64;
    loop {
        let days_in_year = if is_leap_year(year) { 366 } else { 365 };
        if days < days_in_year {
            break;
        }
        days -= days_in_year;
        year += 1;
    }

    const MONTH_DAYS_COMMON: [u64; 12] = [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    let mut month = 1u64;
    for (i, &md) in MONTH_DAYS_COMMON.iter().enumerate() {
        let days_this_month = if i == 1 && is_leap_year(year) { 29 } else { md };
        if days < days_this_month {
            break;
        }
        days -= days_this_month;
        month += 1;
    }
    let day = days + 1;

    format!("{year:04}-{month:02}-{day:02}T{h:02}:{m:02}:{s:02}Z")
}

#[cfg(test)]
mod tests {
    use super::secs_to_iso8601;

    #[test]
    fn iso8601_unix_epoch() {
        assert_eq!(secs_to_iso8601(0), "1970-01-01T00:00:00Z");
    }

    #[test]
    fn iso8601_known_timestamp() {
        // 2026-04-24T00:00:00Z = 1745452800
        assert_eq!(secs_to_iso8601(1_745_452_800), "2026-04-24T00:00:00Z");
    }

    #[test]
    fn iso8601_leap_day() {
        // 2024-02-29T00:00:00Z = 1709164800
        assert_eq!(secs_to_iso8601(1_709_164_800), "2024-02-29T00:00:00Z");
    }

    #[test]
    fn iso8601_format_matches_pattern() {
        let ts = secs_to_iso8601(1_700_000_000);
        assert!(
            ts.len() == 20 && ts.ends_with('Z') && ts.contains('T'),
            "unexpected format: {ts}"
        );
    }
}
