pub mod log_parser;
pub mod network_scanner;

use std::{
    collections::{HashMap, HashSet},
    env,
    path::{Path, PathBuf},
    sync::{Arc, OnceLock},
};

#[cfg(not(target_os = "windows"))]
use std::io::ErrorKind;

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
    #[error("policy denied: {0}")]
    PolicyDenied(String),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("tool execution error: {0}")]
    Execution(String),
}

#[derive(Debug, Clone)]
pub struct SandboxPolicy {
    pub allowed_read_roots: Vec<PathBuf>,
    pub denied_read_roots: Vec<PathBuf>,
    pub command_allowlist: HashSet<String>,
    pub command_denylist: HashSet<String>,
}

impl Default for SandboxPolicy {
    fn default() -> Self {
        Self::strict_default()
    }
}

impl SandboxPolicy {
    pub fn strict_default() -> Self {
        let cwd = env::current_dir().unwrap_or_else(|_| PathBuf::from("."));

        let mut allowed_read_roots = vec![cwd];
        let mut denied_read_roots = Vec::new();

        #[cfg(target_os = "windows")]
        {
            allowed_read_roots.push(PathBuf::from(r"C:\ProgramData"));
            allowed_read_roots.push(PathBuf::from(r"C:\Windows\System32\winevt\Logs"));

            denied_read_roots.push(PathBuf::from(r"C:\Windows\System32\config"));
            denied_read_roots.push(PathBuf::from(r"C:\Windows\System32\drivers\etc"));
        }

        #[cfg(not(target_os = "windows"))]
        {
            allowed_read_roots.push(PathBuf::from("/var/log"));
            allowed_read_roots.push(PathBuf::from("/tmp"));

            denied_read_roots.push(PathBuf::from("/root"));
            denied_read_roots.push(PathBuf::from("/etc/shadow"));
            denied_read_roots.push(PathBuf::from("/proc"));
        }

        #[cfg(target_os = "windows")]
        let command_allowlist: HashSet<String> = ["whoami", "netstat"]
            .into_iter()
            .map(|c| c.to_string())
            .collect();

        #[cfg(not(target_os = "windows"))]
        let command_allowlist: HashSet<String> = ["id", "ss", "sudo"]
            .into_iter()
            .map(|c| c.to_string())
            .collect();

        let command_denylist: HashSet<String> = [
            "cmd",
            "powershell",
            "pwsh",
            "bash",
            "sh",
            "python",
            "curl",
            "wget",
            "nc",
            "ncat",
        ]
        .into_iter()
        .map(|c| c.to_string())
        .collect();

        Self {
            allowed_read_roots,
            denied_read_roots,
            command_allowlist,
            command_denylist,
        }
    }

    pub fn from_env_or_default() -> Self {
        let mut policy = Self::strict_default();

        if let Some(raw_roots) = env::var_os("WRAITHRUN_ALLOWED_READ_ROOTS") {
            let roots: Vec<PathBuf> = env::split_paths(&raw_roots).collect();
            if !roots.is_empty() {
                policy.allowed_read_roots = roots;
            }
        }

        if let Some(raw_roots) = env::var_os("WRAITHRUN_DENIED_READ_ROOTS") {
            let roots: Vec<PathBuf> = env::split_paths(&raw_roots).collect();
            if !roots.is_empty() {
                policy.denied_read_roots = roots;
            }
        }

        if let Ok(raw) = env::var("WRAITHRUN_COMMAND_ALLOWLIST") {
            let commands = parse_command_list(&raw);
            if !commands.is_empty() {
                policy.command_allowlist = commands;
            }
        }

        if let Ok(raw) = env::var("WRAITHRUN_COMMAND_DENYLIST") {
            let commands = parse_command_list(&raw);
            if !commands.is_empty() {
                policy.command_denylist = commands;
            }
        }

        policy
    }

    pub fn ensure_path_allowed(&self, path: &Path) -> Result<(), ToolError> {
        let target = normalize_path(path)?;

        for denied in &self.denied_read_roots {
            let denied = normalize_path(denied)?;
            if target.starts_with(&denied) {
                return Err(ToolError::PolicyDenied(format!(
                    "path '{}' falls under denied root '{}'",
                    target.display(),
                    denied.display()
                )));
            }
        }

        if self.allowed_read_roots.is_empty() {
            return Ok(());
        }

        let allowed = self
            .allowed_read_roots
            .iter()
            .filter_map(|root| normalize_path(root).ok())
            .any(|root| target.starts_with(&root));

        if !allowed {
            return Err(ToolError::PolicyDenied(format!(
                "path '{}' is outside allowlisted roots",
                target.display()
            )));
        }

        Ok(())
    }

    pub fn ensure_command_allowed(&self, command: &str) -> Result<(), ToolError> {
        let normalized = command.trim().to_ascii_lowercase();

        if self.command_denylist.contains(&normalized) {
            return Err(ToolError::PolicyDenied(format!(
                "command '{command}' is denylisted"
            )));
        }

        if !self.command_allowlist.is_empty() && !self.command_allowlist.contains(&normalized) {
            return Err(ToolError::PolicyDenied(format!(
                "command '{command}' is not in the command allowlist"
            )));
        }

        Ok(())
    }
}

fn parse_command_list(raw: &str) -> HashSet<String> {
    raw.split(',')
        .map(str::trim)
        .filter(|entry| !entry.is_empty())
        .map(|entry| entry.to_ascii_lowercase())
        .collect()
}

fn normalize_path(path: &Path) -> Result<PathBuf, ToolError> {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        env::current_dir()?.join(path)
    };

    Ok(std::fs::canonicalize(&absolute).unwrap_or(absolute))
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

#[derive(Clone)]
pub struct ToolRegistry {
    tools: HashMap<String, Arc<dyn Tool>>,
    policy: Arc<SandboxPolicy>,
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self::with_policy(SandboxPolicy::from_env_or_default())
    }

    pub fn with_policy(policy: SandboxPolicy) -> Self {
        Self {
            tools: HashMap::new(),
            policy: Arc::new(policy),
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

    pub fn policy(&self) -> &SandboxPolicy {
        &self.policy
    }

    fn enforce_policy(&self, tool_name: &str, args: &Value) -> Result<(), ToolError> {
        match tool_name {
            "read_syslog" => {
                let path = args
                    .get("path")
                    .and_then(Value::as_str)
                    .unwrap_or("./agent.log");
                self.policy.ensure_path_allowed(Path::new(path))?;
            }
            "hash_binary" => {
                if let Some(path) = args.get("path").and_then(Value::as_str) {
                    self.policy.ensure_path_allowed(Path::new(path))?;
                }
            }
            "scan_network" => {
                #[cfg(target_os = "windows")]
                self.policy.ensure_command_allowed("netstat")?;

                #[cfg(not(target_os = "windows"))]
                self.policy.ensure_command_allowed("ss")?;
            }
            "check_privilege_escalation_vectors" => {
                #[cfg(target_os = "windows")]
                self.policy.ensure_command_allowed("whoami")?;

                #[cfg(not(target_os = "windows"))]
                {
                    self.policy.ensure_command_allowed("id")?;
                    self.policy.ensure_command_allowed("sudo")?;
                }
            }
            _ => {}
        }

        Ok(())
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

        self.enforce_policy(tool_name, &args)?;

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
        let path = args.get("path").and_then(Value::as_str).ok_or_else(|| {
            ToolError::InvalidArguments("missing string field 'path'".to_string())
        })?;

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
            description:
                "Collects a host-local privilege surface snapshot for quick escalation triage."
                    .to_string(),
            args_schema: json!({ "type": "object" }),
        }
    }

    async fn run(&self, _args: Value) -> Result<Value, ToolError> {
        #[cfg(target_os = "windows")]
        {
            let output = Command::new("whoami").arg("/priv").output().await?;
            if !output.status.success() {
                return Err(ToolError::Execution(format!(
                    "privilege snapshot command failed with status {:?}",
                    output.status.code()
                )));
            }

            let stdout = String::from_utf8_lossy(&output.stdout);
            let sample: Vec<String> = stdout
                .lines()
                .take(200)
                .map(|line| line.to_string())
                .collect();

            let indicators: Vec<String> = sample
                .iter()
                .filter(|line| {
                    suspicious_windows_privilege_markers()
                        .iter()
                        .any(|marker| line.contains(marker))
                })
                .cloned()
                .collect();

            return Ok(json!({
                "indicator_count": indicators.len(),
                "potential_vectors": indicators,
                "sample": sample,
            }));
        }

        #[cfg(not(target_os = "windows"))]
        {
            let id_output = Command::new("id").output().await?;
            if !id_output.status.success() {
                return Err(ToolError::Execution(format!(
                    "id command failed with status {:?}",
                    id_output.status.code()
                )));
            }

            let mut lines: Vec<String> = String::from_utf8_lossy(&id_output.stdout)
                .lines()
                .map(|line| line.to_string())
                .collect();

            match Command::new("sudo").args(["-n", "-l"]).output().await {
                Ok(sudo_output) => {
                    lines.extend(
                        String::from_utf8_lossy(&sudo_output.stdout)
                            .lines()
                            .map(|line| line.to_string()),
                    );

                    if !sudo_output.status.success() {
                        lines.push(format!(
                            "sudo -n -l exited with status {:?}",
                            sudo_output.status.code()
                        ));
                    }
                }
                Err(err) if err.kind() == ErrorKind::NotFound => {
                    lines.push("sudo command not available on host".to_string());
                }
                Err(err) => return Err(ToolError::Io(err)),
            }

            let sample: Vec<String> = lines.into_iter().take(200).collect();
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
}
