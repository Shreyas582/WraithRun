//! Structured JSON audit logging for all API and CLI actions.
//!
//! Every significant action emits an [`AuditEvent`] to a dedicated log sink.
//! Events are structured JSON with consistent fields: timestamp, event type,
//! actor identity, resource identifier, and action-specific details.

use serde::Serialize;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;

// ---------------------------------------------------------------------------
// Event definitions
// ---------------------------------------------------------------------------

/// All auditable event types.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AuditEventKind {
    AuthSuccess,
    AuthFailure,
    RunCreated,
    RunCompleted,
    RunFailed,
    RunCancelled,
    CaseCreated,
    CaseUpdated,
    ToolExecuted,
    ToolPolicyDenied,
    ServerStarted,
    ServerStopped,
}

/// A structured audit event.
#[derive(Debug, Clone, Serialize)]
pub struct AuditEvent {
    /// ISO-8601-ish epoch timestamp (seconds since UNIX epoch).
    pub timestamp: String,
    /// The type of event.
    pub event: AuditEventKind,
    /// Identity of the actor (e.g. `"api-token:default"`, `"cli"`).
    pub actor: String,
    /// The resource acted upon (e.g. `"run/<id>"`, `"server"`).
    pub resource: String,
    /// Event-specific key-value details.
    #[serde(skip_serializing_if = "serde_json::Map::is_empty")]
    pub details: serde_json::Map<String, serde_json::Value>,
}

// ---------------------------------------------------------------------------
// Audit sink
// ---------------------------------------------------------------------------

/// Destination for audit events.
#[derive(Clone)]
pub struct AuditLog {
    inner: Arc<Mutex<AuditLogInner>>,
}

struct AuditLogInner {
    /// File writer (if file-based logging is configured).
    file: Option<std::fs::File>,
    /// In-memory buffer used when no file is configured (and for tests).
    buffer: Vec<AuditEvent>,
    /// Maximum number of in-memory events to retain (ring buffer).
    max_buffer: usize,
}

/// Configuration for the audit log.
#[derive(Debug, Clone, Default)]
pub struct AuditLogConfig {
    /// Path to the audit log file. If None, events are kept in-memory only.
    pub file_path: Option<PathBuf>,
    /// Maximum in-memory events to retain. Default: 10_000.
    pub max_buffer: Option<usize>,
}

impl AuditLog {
    /// Create a new audit log with the given configuration.
    pub fn new(config: AuditLogConfig) -> std::io::Result<Self> {
        let file = if let Some(ref path) = config.file_path {
            // Open or create the file in append mode.
            Some(
                std::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(path)?,
            )
        } else {
            None
        };

        Ok(Self {
            inner: Arc::new(Mutex::new(AuditLogInner {
                file,
                buffer: Vec::new(),
                max_buffer: config.max_buffer.unwrap_or(10_000),
            })),
        })
    }

    /// Create an in-memory-only audit log (for tests and when no file is configured).
    pub fn in_memory() -> Self {
        Self {
            inner: Arc::new(Mutex::new(AuditLogInner {
                file: None,
                buffer: Vec::new(),
                max_buffer: 10_000,
            })),
        }
    }

    /// Emit an audit event.
    pub async fn emit(&self, event: AuditEvent) {
        let mut inner = self.inner.lock().await;

        // Write to file if configured.
        if let Some(ref mut file) = inner.file {
            use std::io::Write;
            if let Ok(json_line) = serde_json::to_string(&event) {
                let _ = writeln!(file, "{json_line}");
            }
        }

        // Add to in-memory buffer (ring buffer eviction).
        if inner.buffer.len() >= inner.max_buffer {
            inner.buffer.remove(0);
        }
        inner.buffer.push(event);
    }

    /// Retrieve recent audit events (newest last). For dashboard/API consumption.
    pub async fn recent_events(&self, limit: usize) -> Vec<AuditEvent> {
        let inner = self.inner.lock().await;
        let start = inner.buffer.len().saturating_sub(limit);
        inner.buffer[start..].to_vec()
    }

    /// Return the total number of events recorded.
    pub async fn event_count(&self) -> usize {
        let inner = self.inner.lock().await;
        inner.buffer.len()
    }
}

/// Helper to build an AuditEvent with the current timestamp.
pub fn audit_event(
    event: AuditEventKind,
    actor: impl Into<String>,
    resource: impl Into<String>,
    details: serde_json::Map<String, serde_json::Value>,
) -> AuditEvent {
    AuditEvent {
        timestamp: crate::state::chrono_now(),
        event,
        actor: actor.into(),
        resource: resource.into(),
        details,
    }
}

/// Convenience to build a details map from key-value pairs.
pub fn details(pairs: &[(&str, &str)]) -> serde_json::Map<String, serde_json::Value> {
    let mut map = serde_json::Map::new();
    for (k, v) in pairs {
        map.insert((*k).to_string(), serde_json::Value::String((*v).to_string()));
    }
    map
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn emit_and_retrieve_events() {
        let log = AuditLog::in_memory();

        log.emit(audit_event(
            AuditEventKind::ServerStarted,
            "system",
            "server",
            details(&[("port", "8080")]),
        ))
        .await;

        log.emit(audit_event(
            AuditEventKind::AuthSuccess,
            "api-token:default",
            "auth",
            details(&[]),
        ))
        .await;

        assert_eq!(log.event_count().await, 2);
        let events = log.recent_events(10).await;
        assert_eq!(events.len(), 2);
        assert!(matches!(events[0].event, AuditEventKind::ServerStarted));
        assert!(matches!(events[1].event, AuditEventKind::AuthSuccess));
    }

    #[tokio::test]
    async fn ring_buffer_evicts_oldest() {
        let log = AuditLog::new(AuditLogConfig {
            file_path: None,
            max_buffer: Some(3),
        })
        .unwrap();

        for i in 0..5 {
            log.emit(audit_event(
                AuditEventKind::RunCreated,
                "cli",
                &format!("run/{i}"),
                details(&[]),
            ))
            .await;
        }

        assert_eq!(log.event_count().await, 3);
        let events = log.recent_events(10).await;
        assert_eq!(events[0].resource, "run/2");
        assert_eq!(events[2].resource, "run/4");
    }

    #[tokio::test]
    async fn event_serializes_to_json() {
        let event = audit_event(
            AuditEventKind::RunCreated,
            "api-token:default",
            "run/abc-123",
            details(&[("task", "Check SSH keys"), ("profile", "default")]),
        );
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("\"event\":\"run_created\""));
        assert!(json.contains("\"actor\":\"api-token:default\""));
        assert!(json.contains("\"resource\":\"run/abc-123\""));
        assert!(json.contains("\"task\":\"Check SSH keys\""));
    }

    #[tokio::test]
    async fn file_based_audit_log() {
        let dir = std::env::temp_dir().join("wraithrun-test-audit");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("audit.jsonl");
        let _ = std::fs::remove_file(&path);

        let log = AuditLog::new(AuditLogConfig {
            file_path: Some(path.clone()),
            max_buffer: None,
        })
        .unwrap();

        log.emit(audit_event(
            AuditEventKind::AuthFailure,
            "unknown",
            "auth",
            details(&[("reason", "invalid token")]),
        ))
        .await;

        // File should have one JSON line.
        let content = std::fs::read_to_string(&path).unwrap();
        let lines: Vec<&str> = content.lines().collect();
        assert_eq!(lines.len(), 1);
        let parsed: serde_json::Value = serde_json::from_str(lines[0]).unwrap();
        assert_eq!(parsed["event"], "auth_failure");

        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_dir(&dir);
    }
}
