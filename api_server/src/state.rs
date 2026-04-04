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
        let db = config.database_path.as_ref().map(|path| {
            DataStore::open(path).expect("failed to open database")
        });
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

/// Simple ISO-8601 timestamp without pulling in chrono.
pub fn chrono_now() -> String {
    // Use std SystemTime for a dependency-free timestamp.
    let now = std::time::SystemTime::now();
    let duration = now
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let secs = duration.as_secs();
    // Basic UTC timestamp: seconds since epoch formatted.
    format!("{secs}")
}
