pub mod account_audit;
pub mod log_parser;
pub mod network_scanner;
pub mod persistence_checker;
pub mod process_correlation;

use std::{
    collections::{HashMap, HashSet},
    env,
    path::{Path, PathBuf},
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

#[cfg(target_os = "windows")]
use std::sync::OnceLock;

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
        let command_allowlist: HashSet<String> = ["whoami", "netstat", "net", "tasklist", "reg"]
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
        registry.register(Arc::new(InspectPersistenceLocationsTool));
        registry.register(Arc::new(AuditAccountChangesTool));
        registry.register(Arc::new(CorrelateProcessNetworkTool));
        registry.register(Arc::new(CaptureCoverageBaselineTool));
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
            "inspect_persistence_locations" => {
                #[cfg(target_os = "windows")]
                self.policy.ensure_command_allowed("reg")?;
            }
            "audit_account_changes" => {
                #[cfg(target_os = "windows")]
                self.policy.ensure_command_allowed("net")?;
            }
            "correlate_process_network" => {
                #[cfg(target_os = "windows")]
                {
                    self.policy.ensure_command_allowed("netstat")?;
                    self.policy.ensure_command_allowed("tasklist")?;
                }

                #[cfg(not(target_os = "windows"))]
                self.policy.ensure_command_allowed("ss")?;
            }
            "capture_coverage_baseline" => {
                #[cfg(target_os = "windows")]
                {
                    self.policy.ensure_command_allowed("reg")?;
                    self.policy.ensure_command_allowed("net")?;
                    self.policy.ensure_command_allowed("netstat")?;
                    self.policy.ensure_command_allowed("tasklist")?;
                }

                #[cfg(not(target_os = "windows"))]
                self.policy.ensure_command_allowed("ss")?;
            }
            _ => {}
        }

        Ok(())
    }

    fn sorted_specs(&self) -> Vec<ToolSpec> {
        let mut specs: Vec<ToolSpec> = self.tools.values().map(|t| t.spec()).collect();
        specs.sort_by(|a, b| a.name.cmp(&b.name));
        specs
    }

    pub fn tool_specs(&self) -> Vec<ToolSpec> {
        self.sorted_specs()
    }

    pub fn manifest_json_pretty(&self) -> String {
        serde_json::to_string_pretty(&self.sorted_specs()).unwrap_or_else(|_| "[]".to_string())
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
    parse_bounded_count(args, "max_lines", default, 1000)
}

fn parse_bounded_count(args: &Value, field: &str, default: usize, max: usize) -> usize {
    let value = args
        .get(field)
        .and_then(Value::as_u64)
        .unwrap_or(default as u64);
    value.clamp(1, max as u64) as usize
}

fn parse_string_list(args: &Value, field: &str, max_items: usize, max_chars: usize) -> Vec<String> {
    let mut parsed = Vec::new();
    let Some(values) = args.get(field).and_then(Value::as_array) else {
        return parsed;
    };

    for value in values.iter().take(max_items) {
        let Some(raw) = value.as_str() else {
            continue;
        };

        let trimmed = raw.trim();
        if trimmed.is_empty() {
            continue;
        }

        parsed.push(trimmed.chars().take(max_chars).collect());
    }

    sort_dedup_case_insensitive(&mut parsed);
    parsed
}

fn normalize_lookup_value(value: &str) -> String {
    value.trim().to_ascii_lowercase()
}

fn sort_dedup_case_insensitive(values: &mut Vec<String>) {
    values.sort_by_cached_key(|value| value.to_ascii_lowercase());
    values.dedup_by(|left, right| left.eq_ignore_ascii_case(right));
}

fn persistence_entry_matches_allowlist(
    entry: &persistence_checker::PersistenceEntry,
    allowlist_terms: &HashSet<String>,
) -> bool {
    if allowlist_terms.is_empty() {
        return false;
    }

    let haystack =
        format!("{} {} {}", entry.location, entry.kind, entry.entry).to_ascii_lowercase();
    allowlist_terms
        .iter()
        .any(|term| !term.is_empty() && haystack.contains(term))
}

fn is_high_risk_process_name(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    [
        "powershell",
        "pwsh",
        "cmd",
        "wscript",
        "cscript",
        "mshta",
        "rundll32",
        "python",
        "node",
        "bash",
        "sh",
        "netcat",
        "ncat",
        "nc",
    ]
    .iter()
    .any(|needle| lower.contains(needle))
}

fn calculate_network_risk_score(
    externally_exposed_count: usize,
    high_risk_exposed_count: usize,
    unknown_exposed_process_count: usize,
    new_exposed_binding_count: usize,
    unresolved_count: usize,
) -> u32 {
    let weighted_score = (externally_exposed_count as u32)
        + (high_risk_exposed_count as u32 * 22)
        + (unknown_exposed_process_count as u32 * 12)
        + (new_exposed_binding_count as u32 * 9)
        + (unresolved_count as u32 * 2);
    weighted_score.min(100)
}

fn network_risk_level(score: u32) -> &'static str {
    match score {
        0..=14 => "low",
        15..=39 => "medium",
        40..=69 => "high",
        _ => "critical",
    }
}

fn unix_epoch_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[cfg(target_os = "windows")]
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

pub struct InspectPersistenceLocationsTool;

#[async_trait]
impl Tool for InspectPersistenceLocationsTool {
    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: "inspect_persistence_locations".to_string(),
            description:
                "Inventories common host persistence locations (startup paths, autoruns, cron/system units)."
                    .to_string(),
            args_schema: json!({
                "type": "object",
                "properties": {
                    "limit": {"type": "integer", "minimum": 1, "maximum": 512},
                    "baseline_entries": {
                        "type": "array",
                        "items": {"type": "string"},
                        "maxItems": 512
                    },
                    "allowlist_terms": {
                        "type": "array",
                        "items": {"type": "string"},
                        "maxItems": 128
                    }
                }
            }),
        }
    }

    async fn run(&self, args: Value) -> Result<Value, ToolError> {
        let limit = parse_bounded_count(&args, "limit", 128, 512);
        let baseline_entries = parse_string_list(&args, "baseline_entries", 512, 512);
        let baseline_entry_set: HashSet<String> = baseline_entries
            .iter()
            .map(|entry| normalize_lookup_value(entry))
            .collect();
        let allowlist_terms = parse_string_list(&args, "allowlist_terms", 128, 128);
        let allowlist_term_set: HashSet<String> = allowlist_terms
            .iter()
            .map(|entry| normalize_lookup_value(entry))
            .collect();

        let entries = persistence_checker::collect_persistence_entries(limit).await?;
        let suspicious_entry_count = entries.iter().filter(|entry| entry.suspicious).count();
        let actionable_suspicious_entries: Vec<persistence_checker::PersistenceEntry> = entries
            .iter()
            .filter(|entry| {
                entry.suspicious && !persistence_entry_matches_allowlist(entry, &allowlist_term_set)
            })
            .cloned()
            .collect();

        let mut baseline_new_entries = Vec::new();
        if !baseline_entry_set.is_empty() {
            for entry in &entries {
                let normalized = normalize_lookup_value(&entry.entry);
                if !baseline_entry_set.contains(&normalized) {
                    baseline_new_entries.push(entry.entry.clone());
                }
            }
            sort_dedup_case_insensitive(&mut baseline_new_entries);
        }

        Ok(json!({
            "entry_count": entries.len(),
            "suspicious_entry_count": suspicious_entry_count,
            "actionable_suspicious_count": actionable_suspicious_entries.len(),
            "actionable_suspicious_entries": actionable_suspicious_entries,
            "baseline_reference_count": baseline_entry_set.len(),
            "baseline_new_count": baseline_new_entries.len(),
            "baseline_new_entries": baseline_new_entries,
            "allowlist_term_count": allowlist_term_set.len(),
            "entries": entries,
        }))
    }
}

pub struct AuditAccountChangesTool;

#[async_trait]
impl Tool for AuditAccountChangesTool {
    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: "audit_account_changes".to_string(),
            description:
                "Captures privileged account and admin-group snapshot for change-focused triage."
                    .to_string(),
            args_schema: json!({
                "type": "object",
                "properties": {
                    "baseline_privileged_accounts": {
                        "type": "array",
                        "items": {"type": "string"},
                        "maxItems": 512
                    },
                    "approved_privileged_accounts": {
                        "type": "array",
                        "items": {"type": "string"},
                        "maxItems": 512
                    }
                }
            }),
        }
    }

    async fn run(&self, args: Value) -> Result<Value, ToolError> {
        let baseline_accounts = parse_string_list(&args, "baseline_privileged_accounts", 512, 256);
        let approved_accounts = parse_string_list(&args, "approved_privileged_accounts", 512, 256);

        let baseline_account_set: HashSet<String> = baseline_accounts
            .iter()
            .map(|account| normalize_lookup_value(account))
            .collect();
        let approved_account_set: HashSet<String> = approved_accounts
            .iter()
            .map(|account| normalize_lookup_value(account))
            .collect();

        let snapshot = account_audit::collect_account_privilege_snapshot().await?;

        let current_account_set: HashSet<String> = snapshot
            .privileged_accounts
            .iter()
            .map(|account| normalize_lookup_value(account))
            .collect();

        let mut newly_privileged_accounts = Vec::new();
        if !baseline_account_set.is_empty() {
            for account in &snapshot.privileged_accounts {
                let normalized = normalize_lookup_value(account);
                if !baseline_account_set.contains(&normalized) {
                    newly_privileged_accounts.push(account.clone());
                }
            }
            sort_dedup_case_insensitive(&mut newly_privileged_accounts);
        }

        let mut removed_privileged_accounts = Vec::new();
        if !baseline_account_set.is_empty() {
            for account in &baseline_accounts {
                let normalized = normalize_lookup_value(account);
                if !current_account_set.contains(&normalized) {
                    removed_privileged_accounts.push(account.clone());
                }
            }
            sort_dedup_case_insensitive(&mut removed_privileged_accounts);
        }

        let mut unapproved_privileged_accounts = Vec::new();
        if !approved_account_set.is_empty() {
            for account in &snapshot.privileged_accounts {
                let normalized = normalize_lookup_value(account);
                if !approved_account_set.contains(&normalized) {
                    unapproved_privileged_accounts.push(account.clone());
                }
            }
            sort_dedup_case_insensitive(&mut unapproved_privileged_accounts);
        }

        Ok(json!({
            "privileged_account_count": snapshot.privileged_accounts.len(),
            "non_default_privileged_account_count": snapshot.non_default_privileged_accounts.len(),
            "baseline_reference_count": baseline_account_set.len(),
            "newly_privileged_account_count": newly_privileged_accounts.len(),
            "removed_privileged_account_count": removed_privileged_accounts.len(),
            "approved_account_count": approved_account_set.len(),
            "unapproved_privileged_account_count": unapproved_privileged_accounts.len(),
            "privileged_accounts": snapshot.privileged_accounts,
            "non_default_privileged_accounts": snapshot.non_default_privileged_accounts,
            "newly_privileged_accounts": newly_privileged_accounts,
            "removed_privileged_accounts": removed_privileged_accounts,
            "unapproved_privileged_accounts": unapproved_privileged_accounts,
            "evidence": snapshot.evidence,
        }))
    }
}

pub struct CorrelateProcessNetworkTool;

#[async_trait]
impl Tool for CorrelateProcessNetworkTool {
    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: "correlate_process_network".to_string(),
            description:
                "Correlates listening sockets with owning processes for faster containment triage."
                    .to_string(),
            args_schema: json!({
                "type": "object",
                "properties": {
                    "limit": {"type": "integer", "minimum": 1, "maximum": 512},
                    "baseline_exposed_bindings": {
                        "type": "array",
                        "items": {"type": "string"},
                        "maxItems": 512
                    },
                    "expected_processes": {
                        "type": "array",
                        "items": {"type": "string"},
                        "maxItems": 512
                    }
                }
            }),
        }
    }

    async fn run(&self, args: Value) -> Result<Value, ToolError> {
        let limit = parse_bounded_count(&args, "limit", 128, 512);
        let baseline_exposed_bindings =
            parse_string_list(&args, "baseline_exposed_bindings", 512, 256);
        let baseline_binding_set: HashSet<String> = baseline_exposed_bindings
            .iter()
            .map(|binding| normalize_lookup_value(binding))
            .collect();
        let expected_processes = parse_string_list(&args, "expected_processes", 512, 256);
        let expected_process_set: HashSet<String> = expected_processes
            .iter()
            .map(|process| normalize_lookup_value(process))
            .collect();

        let records = process_correlation::correlate_process_network(limit).await?;
        let correlated_count = records
            .iter()
            .filter(|record| record.process_name.is_some())
            .count();
        let externally_exposed_count = records
            .iter()
            .filter(|record| record.externally_exposed)
            .count();
        let unresolved_count = records.len().saturating_sub(correlated_count);

        let high_risk_exposed_records: Vec<process_correlation::ProcessNetworkRecord> = records
            .iter()
            .filter(|record| {
                record.externally_exposed
                    && record
                        .process_name
                        .as_deref()
                        .map(is_high_risk_process_name)
                        .unwrap_or(false)
            })
            .cloned()
            .collect();

        let unknown_exposed_records: Vec<process_correlation::ProcessNetworkRecord> =
            if expected_process_set.is_empty() {
                Vec::new()
            } else {
                records
                    .iter()
                    .filter(|record| {
                        record.externally_exposed
                            && record
                                .process_name
                                .as_deref()
                                .map(|name| {
                                    let normalized = normalize_lookup_value(name);
                                    !expected_process_set.contains(&normalized)
                                })
                                .unwrap_or(true)
                    })
                    .cloned()
                    .collect()
            };

        let mut new_exposed_bindings = Vec::new();
        if !baseline_binding_set.is_empty() {
            for record in records.iter().filter(|record| record.externally_exposed) {
                let normalized = normalize_lookup_value(&record.local_address);
                if !baseline_binding_set.contains(&normalized) {
                    new_exposed_bindings.push(record.local_address.clone());
                }
            }
            sort_dedup_case_insensitive(&mut new_exposed_bindings);
        }

        let network_risk_score = calculate_network_risk_score(
            externally_exposed_count,
            high_risk_exposed_records.len(),
            unknown_exposed_records.len(),
            new_exposed_bindings.len(),
            unresolved_count,
        );
        let network_risk_level = network_risk_level(network_risk_score);

        Ok(json!({
            "listener_count": records.len(),
            "correlated_count": correlated_count,
            "unresolved_count": unresolved_count,
            "externally_exposed_count": externally_exposed_count,
            "high_risk_exposed_count": high_risk_exposed_records.len(),
            "high_risk_exposed_records": high_risk_exposed_records,
            "expected_process_count": expected_process_set.len(),
            "unknown_exposed_process_count": unknown_exposed_records.len(),
            "unknown_exposed_records": unknown_exposed_records,
            "baseline_binding_count": baseline_binding_set.len(),
            "new_exposed_binding_count": new_exposed_bindings.len(),
            "new_exposed_bindings": new_exposed_bindings,
            "network_risk_score": network_risk_score,
            "network_risk_level": network_risk_level,
            "records": records,
        }))
    }
}

pub struct CaptureCoverageBaselineTool;

#[async_trait]
impl Tool for CaptureCoverageBaselineTool {
    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: "capture_coverage_baseline".to_string(),
            description:
                "Captures reusable baseline arrays for persistence, privileged accounts, and exposed process-network bindings."
                    .to_string(),
            args_schema: json!({
                "type": "object",
                "properties": {
                    "persistence_limit": {"type": "integer", "minimum": 1, "maximum": 512},
                    "listener_limit": {"type": "integer", "minimum": 1, "maximum": 512}
                }
            }),
        }
    }

    async fn run(&self, args: Value) -> Result<Value, ToolError> {
        let persistence_limit = parse_bounded_count(&args, "persistence_limit", 256, 512);
        let listener_limit = parse_bounded_count(&args, "listener_limit", 128, 512);

        let persistence_entries =
            persistence_checker::collect_persistence_entries(persistence_limit).await?;
        let suspicious_entry_count = persistence_entries
            .iter()
            .filter(|entry| entry.suspicious)
            .count();

        let mut baseline_entries: Vec<String> = persistence_entries
            .iter()
            .map(|entry| entry.entry.clone())
            .collect();
        sort_dedup_case_insensitive(&mut baseline_entries);

        let account_snapshot = account_audit::collect_account_privilege_snapshot().await?;
        let mut baseline_privileged_accounts = account_snapshot.privileged_accounts.clone();
        sort_dedup_case_insensitive(&mut baseline_privileged_accounts);

        let mut approved_privileged_accounts = baseline_privileged_accounts.clone();
        sort_dedup_case_insensitive(&mut approved_privileged_accounts);

        let records = process_correlation::correlate_process_network(listener_limit).await?;
        let externally_exposed_records: Vec<&process_correlation::ProcessNetworkRecord> = records
            .iter()
            .filter(|record| record.externally_exposed)
            .collect();

        let mut baseline_exposed_bindings: Vec<String> = externally_exposed_records
            .iter()
            .map(|record| record.local_address.clone())
            .collect();
        sort_dedup_case_insensitive(&mut baseline_exposed_bindings);

        let mut expected_processes: Vec<String> = externally_exposed_records
            .iter()
            .filter_map(|record| record.process_name.clone())
            .collect();
        sort_dedup_case_insensitive(&mut expected_processes);

        Ok(json!({
            "baseline_version": "coverage-v1",
            "captured_epoch_seconds": unix_epoch_seconds(),
            "persistence_limit": persistence_limit,
            "listener_limit": listener_limit,
            "baseline_entries_count": baseline_entries.len(),
            "baseline_privileged_account_count": baseline_privileged_accounts.len(),
            "baseline_exposed_binding_count": baseline_exposed_bindings.len(),
            "expected_process_count": expected_processes.len(),
            "persistence": {
                "entry_count": persistence_entries.len(),
                "suspicious_entry_count": suspicious_entry_count,
                "baseline_entries": baseline_entries,
            },
            "accounts": {
                "privileged_account_count": baseline_privileged_accounts.len(),
                "non_default_privileged_account_count": account_snapshot
                    .non_default_privileged_accounts
                    .len(),
                "baseline_privileged_accounts": baseline_privileged_accounts,
                "approved_privileged_accounts": approved_privileged_accounts,
                "non_default_privileged_accounts": account_snapshot.non_default_privileged_accounts,
                "evidence": account_snapshot.evidence,
            },
            "network": {
                "listener_count": records.len(),
                "externally_exposed_count": externally_exposed_records.len(),
                "baseline_exposed_bindings": baseline_exposed_bindings,
                "expected_processes": expected_processes,
            },
        }))
    }
}
