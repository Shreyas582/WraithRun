use std::path::Path;
use std::sync::Arc;

use anyhow::{Context, Result};
use rusqlite::Connection;
use tokio::sync::Mutex;
use uuid::Uuid;

use crate::state::{RunEntry, RunStatus};

/// Current schema version. Increment when adding migrations.
const SCHEMA_VERSION: u32 = 1;

/// Persistent data store backed by SQLite.
#[derive(Clone)]
pub struct DataStore {
    conn: Arc<Mutex<Connection>>,
}

impl DataStore {
    /// Open or create a database at the given path. Runs migrations automatically.
    pub fn open(path: &Path) -> Result<Self> {
        let conn =
            Connection::open(path).with_context(|| format!("opening database at {}", path.display()))?;
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")?;
        let store = Self {
            conn: Arc::new(Mutex::new(conn)),
        };
        store.migrate_sync()?;
        Ok(store)
    }

    /// Create an in-memory database (for tests).
    pub fn open_in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        conn.execute_batch("PRAGMA foreign_keys=ON;")?;
        let store = Self {
            conn: Arc::new(Mutex::new(conn)),
        };
        store.migrate_sync()?;
        Ok(store)
    }

    fn migrate_sync(&self) -> Result<()> {
        // We hold the Arc<Mutex<Connection>> but since this is called from the
        // constructor (before any async context), we use try_lock.
        let conn = self.conn.try_lock().expect("migrate called during init");
        migrate(&conn)
    }

    pub async fn insert_run(&self, entry: &RunEntry) -> Result<()> {
        let conn = self.conn.lock().await;
        conn.execute(
            "INSERT INTO runs (id, task, status, report_json, error, created_at, completed_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            rusqlite::params![
                entry.id.to_string(),
                entry.task,
                status_to_str(&entry.status),
                entry.report.as_ref().map(|r| serde_json::to_string(r).unwrap_or_default()),
                entry.error,
                entry.created_at,
                entry.completed_at,
            ],
        )?;

        // Insert findings from the report if present.
        if let Some(report) = &entry.report {
            for (i, finding) in report.findings.iter().enumerate() {
                conn.execute(
                    "INSERT INTO findings (id, run_id, seq, title, severity, confidence, recommendation)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                    rusqlite::params![
                        Uuid::new_v4().to_string(),
                        entry.id.to_string(),
                        i as i64,
                        finding.title,
                        format!("{:?}", finding.severity),
                        finding.confidence,
                        finding.recommended_action,
                    ],
                )?;
            }
        }
        Ok(())
    }

    pub async fn update_run(&self, entry: &RunEntry) -> Result<()> {
        let conn = self.conn.lock().await;
        conn.execute(
            "UPDATE runs SET status = ?1, report_json = ?2, error = ?3, completed_at = ?4
             WHERE id = ?5",
            rusqlite::params![
                status_to_str(&entry.status),
                entry.report.as_ref().map(|r| serde_json::to_string(r).unwrap_or_default()),
                entry.error,
                entry.completed_at,
                entry.id.to_string(),
            ],
        )?;

        // Upsert findings: delete old, insert new.
        if let Some(report) = &entry.report {
            conn.execute(
                "DELETE FROM findings WHERE run_id = ?1",
                [entry.id.to_string()],
            )?;
            for (i, finding) in report.findings.iter().enumerate() {
                conn.execute(
                    "INSERT INTO findings (id, run_id, seq, title, severity, confidence, recommendation)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                    rusqlite::params![
                        Uuid::new_v4().to_string(),
                        entry.id.to_string(),
                        i as i64,
                        finding.title,
                        format!("{:?}", finding.severity),
                        finding.confidence,
                        finding.recommended_action,
                    ],
                )?;
            }
        }
        Ok(())
    }

    pub async fn get_run(&self, id: Uuid) -> Result<Option<RunEntry>> {
        let conn = self.conn.lock().await;
        let mut stmt = conn.prepare(
            "SELECT id, task, status, report_json, error, created_at, completed_at
             FROM runs WHERE id = ?1",
        )?;
        let mut rows = stmt.query([id.to_string()])?;
        match rows.next()? {
            Some(row) => Ok(Some(row_to_run_entry(row)?)),
            None => Ok(None),
        }
    }

    pub async fn list_runs(&self) -> Result<Vec<RunEntry>> {
        let conn = self.conn.lock().await;
        let mut stmt = conn.prepare(
            "SELECT id, task, status, report_json, error, created_at, completed_at
             FROM runs ORDER BY created_at DESC",
        )?;
        let rows = stmt.query_map([], |row| Ok(row_to_run_entry(row).unwrap()))?;
        let mut entries = Vec::new();
        for row in rows {
            entries.push(row?);
        }
        Ok(entries)
    }

    /// Copy the database to the given path for backup.
    pub async fn backup(&self, dest: &Path) -> Result<()> {
        let conn = self.conn.lock().await;
        let mut dst = Connection::open(dest)?;
        let backup = rusqlite::backup::Backup::new(&conn, &mut dst)?;
        backup.run_to_completion(100, std::time::Duration::from_millis(10), None)?;
        Ok(())
    }

    /// Export all runs as a JSON array.
    pub async fn export_json(&self) -> Result<String> {
        let runs = self.list_runs().await?;
        Ok(serde_json::to_string_pretty(&runs)?)
    }
}

fn migrate(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS schema_version (
            version INTEGER NOT NULL
        );",
    )?;

    let current: u32 = conn
        .query_row(
            "SELECT COALESCE(MAX(version), 0) FROM schema_version",
            [],
            |row| row.get(0),
        )
        .unwrap_or(0);

    if current < 1 {
        conn.execute_batch(
            "CREATE TABLE runs (
                id TEXT PRIMARY KEY,
                task TEXT NOT NULL,
                status TEXT NOT NULL DEFAULT 'queued',
                report_json TEXT,
                error TEXT,
                created_at TEXT NOT NULL,
                completed_at TEXT
            );

            CREATE TABLE findings (
                id TEXT PRIMARY KEY,
                run_id TEXT NOT NULL REFERENCES runs(id),
                seq INTEGER NOT NULL,
                title TEXT NOT NULL,
                severity TEXT NOT NULL,
                confidence REAL NOT NULL,
                recommendation TEXT NOT NULL
            );

            CREATE INDEX idx_findings_run_id ON findings(run_id);
            CREATE INDEX idx_runs_status ON runs(status);
            CREATE INDEX idx_runs_created_at ON runs(created_at);

            INSERT INTO schema_version (version) VALUES (1);",
        )?;
    }

    assert!(
        current <= SCHEMA_VERSION,
        "database schema version {current} is newer than supported {SCHEMA_VERSION}"
    );

    Ok(())
}

fn status_to_str(status: &RunStatus) -> &'static str {
    match status {
        RunStatus::Queued => "queued",
        RunStatus::Running => "running",
        RunStatus::Completed => "completed",
        RunStatus::Failed => "failed",
        RunStatus::Cancelled => "cancelled",
    }
}

fn str_to_status(s: &str) -> RunStatus {
    match s {
        "queued" => RunStatus::Queued,
        "running" => RunStatus::Running,
        "completed" => RunStatus::Completed,
        "failed" => RunStatus::Failed,
        "cancelled" => RunStatus::Cancelled,
        _ => RunStatus::Failed,
    }
}

fn row_to_run_entry(row: &rusqlite::Row) -> Result<RunEntry> {
    let id_str: String = row.get(0)?;
    let report_json: Option<String> = row.get(3)?;
    Ok(RunEntry {
        id: Uuid::parse_str(&id_str)?,
        task: row.get(1)?,
        status: str_to_status(&row.get::<_, String>(2)?),
        report: report_json
            .as_deref()
            .filter(|s| !s.is_empty())
            .map(|s| serde_json::from_str(s))
            .transpose()?,
        error: row.get(4)?,
        created_at: row.get(5)?,
        completed_at: row.get(6)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_entry() -> RunEntry {
        RunEntry {
            id: Uuid::new_v4(),
            task: "Test investigation".to_string(),
            status: RunStatus::Queued,
            report: None,
            error: None,
            created_at: "1700000000".to_string(),
            completed_at: None,
        }
    }

    #[tokio::test]
    async fn create_and_retrieve_run() {
        let store = DataStore::open_in_memory().unwrap();
        let entry = sample_entry();
        store.insert_run(&entry).await.unwrap();

        let loaded = store.get_run(entry.id).await.unwrap().unwrap();
        assert_eq!(loaded.id, entry.id);
        assert_eq!(loaded.task, entry.task);
    }

    #[tokio::test]
    async fn update_run_status() {
        let store = DataStore::open_in_memory().unwrap();
        let mut entry = sample_entry();
        store.insert_run(&entry).await.unwrap();

        entry.status = RunStatus::Completed;
        entry.completed_at = Some("1700000060".to_string());
        store.update_run(&entry).await.unwrap();

        let loaded = store.get_run(entry.id).await.unwrap().unwrap();
        assert_eq!(loaded.status, RunStatus::Completed);
        assert_eq!(loaded.completed_at.as_deref(), Some("1700000060"));
    }

    #[tokio::test]
    async fn list_runs_ordered_by_created_at() {
        let store = DataStore::open_in_memory().unwrap();

        let mut e1 = sample_entry();
        e1.created_at = "1700000001".to_string();
        store.insert_run(&e1).await.unwrap();

        let mut e2 = sample_entry();
        e2.created_at = "1700000002".to_string();
        store.insert_run(&e2).await.unwrap();

        let runs = store.list_runs().await.unwrap();
        assert_eq!(runs.len(), 2);
        // Most recent first.
        assert_eq!(runs[0].id, e2.id);
        assert_eq!(runs[1].id, e1.id);
    }

    #[tokio::test]
    async fn get_nonexistent_run_returns_none() {
        let store = DataStore::open_in_memory().unwrap();
        let result = store.get_run(Uuid::new_v4()).await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn export_json_produces_valid_array() {
        let store = DataStore::open_in_memory().unwrap();
        let entry = sample_entry();
        store.insert_run(&entry).await.unwrap();

        let json_str = store.export_json().await.unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json_str).unwrap();
        assert!(parsed.is_array());
        assert_eq!(parsed.as_array().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn migration_is_idempotent() {
        let store = DataStore::open_in_memory().unwrap();
        // Running migrate again should succeed (schema already exists).
        store.migrate_sync().unwrap();
    }
}
