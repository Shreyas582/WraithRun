pub mod log_parser;
pub mod network_scanner;

use std::{
    collections::HashMap,
    path::Path,
    sync::{Arc, OnceLock},
};

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use thiserror::Error;
use tokio::process::Command;
use tracing::debug;

#[derive(Debug, Error)]
pub enum ToolError {
    #[error("unknown tool: {0}")]
    UnknownTool(String),
    #[error("invalid arguments: {0}")]
    InvalidArguments(String),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("tool execution error: {0}")]
    Execution(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolSpec {
    pub name: String,
    pub description: String,
    pub args_schema: Value,
}

#[async_trait]
pub trait Tool: Send + Sync {
    fn spec(&self) -> ToolSpec;
    async fn run(&self, args: Value) -> Result<Value, ToolError>;
}

#[derive(Default, Clone)]
pub struct ToolRegistry {
    tools: HashMap<String, Arc<dyn Tool>>,
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self {
            tools: HashMap::new(),
        }
    }

    pub fn with_default_tools() -> Self {
        let mut registry = Self::new();
        registry.register(Arc::new(ReadSyslogTool));
        registry.register(Arc::new(ScanNetworkTool));
        registry.register(Arc::new(CheckPrivilegeEscalationVectorsTool));
        registry.register(Arc::new(HashBinaryTool));
        registry
    }

    pub fn register(&mut self, tool: Arc<dyn Tool>) {
        let name = tool.spec().name;
        self.tools.insert(name, tool);
    }

    pub fn manifest_json_pretty(&self) -> String {
        let mut specs: Vec<ToolSpec> = self.tools.values().map(|t| t.spec()).collect();
        specs.sort_by(|a, b| a.name.cmp(&b.name));
        serde_json::to_string_pretty(&specs).unwrap_or_else(|_| "[]".to_string())
    }

    pub async fn execute(&self, tool_name: &str, args: Value) -> Result<Value, ToolError> {
        let tool = self
            .tools
            .get(tool_name)
            .ok_or_else(|| ToolError::UnknownTool(tool_name.to_string()))?;

        debug!(tool = tool_name, "executing tool");
        tool.run(args).await
    }
}

fn parse_max_lines(args: &Value, default: usize) -> usize {
    let value = args
        .get("max_lines")
        .and_then(Value::as_u64)
        .unwrap_or(default as u64);
    value.clamp(1, 1000) as usize
}

fn suspicious_windows_privilege_markers() -> &'static Vec<&'static str> {
    static MARKERS: OnceLock<Vec<&'static str>> = OnceLock::new();
    MARKERS.get_or_init(|| {
        vec![
            "SeImpersonatePrivilege",
            "SeAssignPrimaryTokenPrivilege",
            "SeDebugPrivilege",
            "SeTakeOwnershipPrivilege",
        ]
    })
}

pub struct ReadSyslogTool;

#[async_trait]
impl Tool for ReadSyslogTool {
    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: "read_syslog".to_string(),
            description: "Reads local log file tail lines in a bounded, parse-friendly format."
                .to_string(),
            args_schema: json!({
                "type": "object",
                "properties": {
                    "path": {"type": "string"},
                    "max_lines": {"type": "integer", "minimum": 1, "maximum": 1000}
                },
                "required": ["path"]
            }),
        }
    }

    async fn run(&self, args: Value) -> Result<Value, ToolError> {
        let path = args
            .get("path")
            .and_then(Value::as_str)
            .unwrap_or("./agent.log");
        let max_lines = parse_max_lines(&args, 200);

        let lines = log_parser::read_log_tail(Path::new(path), max_lines)?;
        Ok(json!({
            "path": path,
            "line_count": lines.len(),
            "lines": lines
        }))
    }
}

pub struct ScanNetworkTool;

#[async_trait]
impl Tool for ScanNetworkTool {
    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: "scan_network".to_string(),
            description: "Lists active local listening sockets from host networking stack."
                .to_string(),
            args_schema: json!({
                "type": "object",
                "properties": {
                    "limit": {"type": "integer", "minimum": 1, "maximum": 512}
                }
            }),
        }
    }

    async fn run(&self, args: Value) -> Result<Value, ToolError> {
        let limit = args.get("limit").and_then(Value::as_u64).unwrap_or(128) as usize;
        let listeners = network_scanner::list_local_listeners(limit).await?;

        Ok(json!({
            "listener_count": listeners.len(),
            "listeners": listeners,
        }))
    }
}

pub struct HashBinaryTool;

#[async_trait]
impl Tool for HashBinaryTool {
    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: "hash_binary".to_string(),
            description: "Computes SHA-256 hash of a file for local integrity triage.".to_string(),
            args_schema: json!({
                "type": "object",
                "properties": {
                    "path": {"type": "string"}
                },
                "required": ["path"]
            }),
        }
    }

    async fn run(&self, args: Value) -> Result<Value, ToolError> {
        let path = args
            .get("path")
            .and_then(Value::as_str)
            .ok_or_else(|| ToolError::InvalidArguments("missing string field 'path'".to_string()))?;

        let digest = log_parser::sha256_file(Path::new(path))?;

        Ok(json!({
            "path": path,
            "sha256": digest,
        }))
    }
}

pub struct CheckPrivilegeEscalationVectorsTool;

#[async_trait]
impl Tool for CheckPrivilegeEscalationVectorsTool {
    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: "check_privilege_escalation_vectors".to_string(),
            description: "Collects a host-local privilege surface snapshot for quick escalation triage."
                .to_string(),
            args_schema: json!({ "type": "object" }),
        }
    }

    async fn run(&self, _args: Value) -> Result<Value, ToolError> {
        #[cfg(target_os = "windows")]
        let output = Command::new("whoami").arg("/priv").output().await?;

        #[cfg(not(target_os = "windows"))]
        let output = Command::new("sh")
            .arg("-c")
            .arg("id; (sudo -n -l 2>/dev/null || true)")
            .output()
            .await?;

        if !output.status.success() {
            return Err(ToolError::Execution(format!(
                "privilege snapshot command failed with status {:?}",
                output.status.code()
            )));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let sample: Vec<String> = stdout.lines().take(200).map(|line| line.to_string()).collect();

        #[cfg(target_os = "windows")]
        let indicators: Vec<String> = sample
            .iter()
            .filter(|line| {
                suspicious_windows_privilege_markers()
                    .iter()
                    .any(|marker| line.contains(marker))
            })
            .cloned()
            .collect();

        #[cfg(not(target_os = "windows"))]
        let indicators: Vec<String> = sample
            .iter()
            .filter(|line| line.contains("NOPASSWD") || line.contains("(ALL)"))
            .cloned()
            .collect();

        Ok(json!({
            "indicator_count": indicators.len(),
            "potential_vectors": indicators,
            "sample": sample,
        }))
    }
}
