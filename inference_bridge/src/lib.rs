pub mod backend;
pub mod onnx_vitis;

use std::any::Any;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use anyhow::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tracing::debug;

/// Raw probe signals extracted from a model without running full inference.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelCapabilityProbe {
    /// Estimated parameter count in billions, derived from model file size.
    pub estimated_param_billions: f32,
    /// Execution provider assigned by the runtime (e.g. "CPUExecutionProvider").
    pub execution_provider: String,
    /// Wall-clock latency of a single-token forward pass in milliseconds.
    pub smoke_latency_ms: u64,
    /// Vocabulary size extracted from the logits output tensor shape.
    pub vocab_size: usize,
}

impl Default for ModelCapabilityProbe {
    fn default() -> Self {
        Self {
            estimated_param_billions: 0.0,
            execution_provider: "CPUExecutionProvider".to_string(),
            smoke_latency_ms: 999,
            vocab_size: 0,
        }
    }
}

/// Probe a model's capability signals without running full inference.
///
/// On non-onnx builds, returns a sensible default (Basic-tier signals).
/// On onnx builds, extracts file size, EP, smoke latency, and vocab size.
pub fn probe_model_capability(config: &ModelConfig) -> ModelCapabilityProbe {
    let estimated_param_billions = estimate_params_from_file_size(&config.model_path);
    let execution_provider = detect_execution_provider(config);
    let smoke_latency_ms = measure_smoke_latency(config);
    let vocab_size = detect_vocab_size(config);

    ModelCapabilityProbe {
        estimated_param_billions,
        execution_provider,
        smoke_latency_ms,
        vocab_size,
    }
}

/// Estimate parameter count (in billions) from model file size.
/// Detects quantization level from filename conventions (q4, q8, fp16, fp32)
/// and adjusts the bytes-per-parameter divisor accordingly.
/// Sums external data files (`*.onnx_data`, `*.onnx.data`) that ONNX models
/// commonly use for weights that exceed the 2 GB protobuf limit (#115).
fn estimate_params_from_file_size(model_path: &PathBuf) -> f32 {
    let main_size = match std::fs::metadata(model_path) {
        Ok(meta) => meta.len(),
        Err(_) => return 0.0,
    };

    // Scan siblings for external weight files matching the model stem.
    let external_size = model_path
        .file_name()
        .and_then(|n| n.to_str())
        .and_then(|model_name| {
            let parent = model_path.parent()?;
            let total: u64 = std::fs::read_dir(parent)
                .ok()?
                .filter_map(|e| e.ok())
                .filter(|e| {
                    let name = e.file_name();
                    let name = name.to_string_lossy();
                    // Match: model_q4.onnx_data, model_q4.onnx_data_0, model_q4.onnx.data
                    name != model_name
                        && (name.starts_with(&format!("{model_name}_data"))
                            || name.starts_with(&format!("{model_name}.data")))
                })
                .filter_map(|e| e.metadata().ok().map(|m| m.len()))
                .sum();
            Some(total)
        })
        .unwrap_or(0);

    let bytes = (main_size + external_size) as f64;

    // Detect quantization from the model path to choose the right divisor.
    // Bytes per parameter: Q4 ≈ 0.55 (4-bit + overhead), Q8 ≈ 1.1,
    // FP16/BF16 ≈ 2.2, FP32 ≈ 4.4 (includes ~10% ONNX structure overhead).
    let bytes_per_param = detect_quant_bytes_per_param(model_path);
    let estimated_params = bytes / bytes_per_param;
    (estimated_params / 1_000_000_000.0) as f32
}

/// Infer bytes-per-parameter from filename conventions.
/// Falls back to 2.2 (FP16) if no quantization hint is found.
fn detect_quant_bytes_per_param(model_path: &Path) -> f64 {
    let path_lower = model_path
        .to_string_lossy()
        .to_lowercase();

    // Check filename and parent directory for quantization hints.
    if path_lower.contains("q4") || path_lower.contains("int4") || path_lower.contains("4bit") {
        0.55
    } else if path_lower.contains("q8") || path_lower.contains("int8") || path_lower.contains("8bit") {
        1.1
    } else if path_lower.contains("fp32") || path_lower.contains("float32") {
        4.4
    } else {
        // Default: assume FP16/BF16 with ~10% overhead.
        2.2
    }
}

// ── Dry-run investigation templates (#117) ──
//
// Mirrors the templates in `core_engine::builtin_investigation_templates()`
// without a compile-time dependency on core_engine (which depends on us).
// Order matters: earlier templates win on tied keyword scores.

struct DryRunTemplate {
    keywords: &'static [&'static str],
    tools: &'static [&'static str],
}

static DRY_RUN_TEMPLATES: &[DryRunTemplate] = &[
    // baseline-capture — must outrank baseline-drift when "capture"/"coverage" present.
    DryRunTemplate {
        keywords: &["capture", "snapshot", "collect", "coverage", "baseline", "golden"],
        tools: &["capture_coverage_baseline"],
    },
    // file-integrity
    DryRunTemplate {
        keywords: &["hash", "sha256", "checksum", "integrity", "tamper"],
        tools: &["hash_binary", "inspect_persistence_locations"],
    },
    // syslog-summary
    DryRunTemplate {
        keywords: &["log", "syslog", "journal"],
        tools: &["read_syslog", "audit_account_changes"],
    },
    // account-audit — before baseline-drift so "account drift" picks accounts first.
    DryRunTemplate {
        keywords: &["account", "user", "group", "member", "drift"],
        tools: &[
            "audit_account_changes",
            "inspect_persistence_locations",
            "check_privilege_escalation_vectors",
        ],
    },
    // baseline-drift
    DryRunTemplate {
        keywords: &["drift", "baseline", "deviation"],
        tools: &[
            "correlate_process_network",
            "audit_account_changes",
            "inspect_persistence_locations",
        ],
    },
    // process-network correlation
    DryRunTemplate {
        keywords: &["correlat", "process", "network"],
        tools: &[
            "correlate_process_network",
            "scan_network",
            "audit_account_changes",
        ],
    },
    // ssh-key-investigation
    DryRunTemplate {
        keywords: &["ssh", "authorized_keys", "key"],
        tools: &[
            "enumerate_ssh_keys",
            "audit_account_changes",
            "inspect_persistence_locations",
            "check_privilege_escalation_vectors",
        ],
    },
    // persistence-analysis
    DryRunTemplate {
        keywords: &["persistence", "autorun", "startup", "cron", "scheduled"],
        tools: &[
            "inspect_persistence_locations",
            "audit_account_changes",
            "read_syslog",
        ],
    },
    // network-exposure-audit
    DryRunTemplate {
        keywords: &[
            "network", "connection", "port", "listen", "listener", "lateral",
            "beacon", "socket",
        ],
        tools: &[
            "scan_network",
            "correlate_process_network",
            "audit_account_changes",
        ],
    },
    // privilege-escalation
    DryRunTemplate {
        keywords: &["privilege", "escalat", "admin", "root", "sudo", "whoami", "unauthori"],
        tools: &[
            "check_privilege_escalation_vectors",
            "audit_account_changes",
            "inspect_persistence_locations",
        ],
    },
];

/// Broad-host-triage fallback when no template matches.
static BROAD_TRIAGE_TOOLS: &[&str] = &[
    "audit_account_changes",
    "inspect_persistence_locations",
    "read_syslog",
    "scan_network",
    "check_privilege_escalation_vectors",
];

/// Resolve the best-matching dry-run template by keyword scoring.
fn resolve_dry_run_template(task: &str) -> &'static [&'static str] {
    let lower = task.to_lowercase();
    let mut best: Option<(&'static [&'static str], usize)> = None;

    for tmpl in DRY_RUN_TEMPLATES {
        let score = tmpl
            .keywords
            .iter()
            .filter(|kw| lower.contains(**kw))
            .count();
        if score > 0 && best.is_none_or(|(_, s)| score > s) {
            best = Some((tmpl.tools, score));
        }
    }

    best.map(|(tools, _)| tools).unwrap_or(BROAD_TRIAGE_TOOLS)
}

/// Detect which execution provider would be used for this config.
fn detect_execution_provider(config: &ModelConfig) -> String {
    if config.backend_override.as_deref() == Some("vitis")
        || config.backend_config.contains_key("config_file")
    {
        "VitisAIExecutionProvider".to_string()
    } else if config.backend_override.as_deref() == Some("directml") {
        "DmlExecutionProvider".to_string()
    } else if config.backend_override.as_deref() == Some("cuda") {
        "CUDAExecutionProvider".to_string()
    } else {
        "CPUExecutionProvider".to_string()
    }
}

/// Measure smoke latency. In dry-run or non-onnx builds, returns a default.
fn measure_smoke_latency(config: &ModelConfig) -> u64 {
    if config.dry_run {
        return 1;
    }
    // Without live ONNX session, estimate from file size heuristic:
    // ~50ms per billion params on CPU as baseline estimate.
    let params_b = estimate_params_from_file_size(&config.model_path);
    if params_b > 0.0 {
        (params_b * 50.0) as u64
    } else {
        999
    }
}

/// Detect vocabulary size from model config. Returns 0 if unknown.
fn detect_vocab_size(config: &ModelConfig) -> usize {
    // Try reading tokenizer.json to extract vocab size.
    if let Some(tokenizer_path) = &config.tokenizer_path {
        if let Ok(data) = std::fs::read_to_string(tokenizer_path) {
            if let Ok(json) = serde_json::from_str::<serde_json::Value>(&data) {
                // HuggingFace tokenizer format: model.vocab has the vocab entries.
                if let Some(vocab) = json
                    .get("model")
                    .and_then(|m| m.get("vocab"))
                    .and_then(|v| v.as_object())
                {
                    return vocab.len();
                }
                // Alternative: added_tokens array length + base vocab.
                if let Some(added) = json.get("added_tokens").and_then(|a| a.as_array()) {
                    if let Some(base) = json
                        .get("model")
                        .and_then(|m| m.get("merges"))
                        .and_then(|m| m.as_array())
                    {
                        // BPE vocab ≈ merges + 256 byte tokens + added tokens
                        return base.len() + 256 + added.len();
                    }
                }
            }
        }
    }
    0
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct VitisEpConfig {
    pub config_file: Option<String>,
    pub cache_dir: Option<String>,
    pub cache_key: Option<String>,
}

impl VitisEpConfig {
    /// Convert to a generic backend config map.
    pub fn into_backend_config(self) -> std::collections::HashMap<String, String> {
        let mut map = std::collections::HashMap::new();
        if let Some(v) = self.config_file {
            map.insert("config_file".to_string(), v);
        }
        if let Some(v) = self.cache_dir {
            map.insert("cache_dir".to_string(), v);
        }
        if let Some(v) = self.cache_key {
            map.insert("cache_key".to_string(), v);
        }
        map
    }

    /// Reconstruct from a generic backend config map.
    pub fn from_backend_config(map: &std::collections::HashMap<String, String>) -> Self {
        Self {
            config_file: map.get("config_file").cloned(),
            cache_dir: map.get("cache_dir").cloned(),
            cache_key: map.get("cache_key").cloned(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelConfig {
    pub model_path: PathBuf,
    pub tokenizer_path: Option<PathBuf>,
    pub max_new_tokens: usize,
    pub temperature: f32,
    pub dry_run: bool,
    /// Explicit backend override (e.g., "cpu", "vitis", "cuda").
    #[serde(default)]
    pub backend_override: Option<String>,
    /// Provider-specific key-value configuration.
    #[serde(default)]
    pub backend_config: std::collections::HashMap<String, String>,
}

#[async_trait]
pub trait InferenceEngine: Send + Sync {
    async fn generate(&self, prompt: &str) -> Result<String>;
}

#[derive(Debug, Clone)]
pub struct OnnxVitisEngine {
    config: ModelConfig,
    /// Cached ONNX Runtime session reused across generate() calls (#64).
    session_cache: Arc<Mutex<Option<Box<dyn Any + Send>>>>,
    /// Tracks which tool in the matched template to emit next during dry-run (#117, #118).
    dry_run_tool_index: Arc<Mutex<usize>>,
}

impl OnnxVitisEngine {
    pub fn new(config: ModelConfig) -> Self {
        Self {
            config,
            session_cache: Arc::new(Mutex::new(None)),
            dry_run_tool_index: Arc::new(Mutex::new(0)),
        }
    }

    fn extract_task_from_prompt(prompt: &str) -> Option<&str> {
        prompt
            .lines()
            .find_map(|line| line.trim().strip_prefix("Task:").map(str::trim))
    }

    fn guess_path_from_task(task: &str) -> Option<String> {
        task.split_whitespace()
            .map(|token| {
                token.trim_matches(|ch: char| {
                    matches!(
                        ch,
                        '"' | '\'' | ',' | ';' | '(' | ')' | '[' | ']' | '{' | '}'
                    )
                })
            })
            .find(|token| {
                token.contains('/')
                    || token.contains('\\')
                    || token.contains(':')
                    || token.ends_with(".exe")
                    || token.ends_with(".dll")
                    || token.ends_with(".log")
                    || token.ends_with(".txt")
                    || token.ends_with(".md")
                    || token.ends_with(".json")
                    || token.ends_with(".toml")
            })
            .map(|token| token.to_string())
    }

    fn escape_json_string(value: &str) -> String {
        value.replace('\\', "\\\\").replace('"', "\\\"")
    }

    fn guess_line_count_from_task(task: &str) -> Option<usize> {
        let tokens: Vec<String> = task
            .split_whitespace()
            .map(|token| {
                token
                    .trim_matches(|ch: char| {
                        matches!(
                            ch,
                            '"' | '\'' | ',' | ';' | '(' | ')' | '[' | ']' | '{' | '}' | '.'
                        )
                    })
                    .to_string()
            })
            .collect();

        for (idx, token) in tokens.iter().enumerate() {
            let Ok(value) = token.parse::<usize>() else {
                continue;
            };

            if value == 0 {
                continue;
            }

            if let Some(next) = tokens.get(idx + 1) {
                let next = next.to_ascii_lowercase();
                if next.starts_with("line") {
                    return Some(value.min(5000));
                }
            }
        }

        None
    }

    fn dry_run_response(&self, prompt: &str) -> String {
        let task = Self::extract_task_from_prompt(prompt).unwrap_or(prompt);

        // Resolve the matching investigation template for this task (#117).
        let template_tools = resolve_dry_run_template(task);

        let mut idx = self
            .dry_run_tool_index
            .lock()
            .unwrap_or_else(|e| e.into_inner());

        // If we've already emitted all template tools, finish (#118).
        if *idx >= template_tools.len() {
            *idx = 0; // reset for next investigation
            return "<final>Dry-run cycle complete. Review the latest tool observations and escalate manually if indicators persist.</final>".to_string();
        }

        let tool_name = template_tools[*idx];
        *idx += 1;

        Self::format_dry_run_tool_call(tool_name, task)
    }

    /// Produce a dry-run `<call>` tag for the given tool, using heuristic args from the task.
    fn format_dry_run_tool_call(tool_name: &str, task: &str) -> String {
        match tool_name {
            "capture_coverage_baseline" => {
                r#"<call>{"tool":"capture_coverage_baseline","args":{"persistence_limit":256,"listener_limit":128}}</call>"#.to_string()
            }
            "hash_binary" => {
                let path = Self::guess_path_from_task(task)
                    .unwrap_or_else(|| "./Cargo.toml".to_string());
                let path = Self::escape_json_string(&path);
                format!(r#"<call>{{"tool":"hash_binary","args":{{"path":"{path}"}}}}</call>"#)
            }
            "read_syslog" => {
                let path = Self::guess_path_from_task(task)
                    .unwrap_or_else(|| "./README.md".to_string());
                let path = Self::escape_json_string(&path);
                let max_lines = Self::guess_line_count_from_task(task).unwrap_or(200);
                format!(
                    r#"<call>{{"tool":"read_syslog","args":{{"path":"{path}","max_lines":{max_lines}}}}}</call>"#
                )
            }
            "correlate_process_network" => {
                r#"<call>{"tool":"correlate_process_network","args":{"limit":64}}</call>"#
                    .to_string()
            }
            "scan_network" => {
                r#"<call>{"tool":"scan_network","args":{"limit":40}}</call>"#.to_string()
            }
            "inspect_persistence_locations" => {
                r#"<call>{"tool":"inspect_persistence_locations","args":{"limit":200}}</call>"#
                    .to_string()
            }
            "audit_account_changes" => {
                r#"<call>{"tool":"audit_account_changes","args":{}}</call>"#.to_string()
            }
            "check_privilege_escalation_vectors" => {
                r#"<call>{"tool":"check_privilege_escalation_vectors","args":{}}</call>"#
                    .to_string()
            }
            other => {
                format!(r#"<call>{{"tool":"{other}","args":{{}}}}</call>"#)
            }
        }
    }
}

#[async_trait]
impl InferenceEngine for OnnxVitisEngine {
    async fn generate(&self, prompt: &str) -> Result<String> {
        if self.config.dry_run {
            debug!("using dry-run inference path");
            return Ok(self.dry_run_response(prompt));
        }

        let mut guard = self
            .session_cache
            .lock()
            .map_err(|e| anyhow::anyhow!("session cache lock poisoned: {e}"))?;
        onnx_vitis::run_prompt_cached(&mut guard, &self.config, prompt)
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::{ModelConfig, OnnxVitisEngine};

    fn dry_run_engine() -> OnnxVitisEngine {
        OnnxVitisEngine::new(ModelConfig {
            model_path: PathBuf::from("./models/llm.onnx"),
            tokenizer_path: None,
            max_new_tokens: 16,
            temperature: 0.2,
            dry_run: true,
            backend_override: None,
            backend_config: Default::default(),
        })
    }

    #[test]
    fn routes_hash_task_to_hash_binary_using_task_line() {
        let engine = dry_run_engine();
        let prompt = "Available tools JSON: [{\"name\":\"scan_network\"}]\nReAct transcript so far:\nTask: Hash ./README.md and report integrity context\nDecide your next action.";

        let output = engine.dry_run_response(prompt);
        assert!(output.contains("\"tool\":\"hash_binary\""));
        assert!(output.contains("\"path\":\"./README.md\""));
    }

    #[test]
    fn routes_network_task_to_scan_network() {
        let engine = dry_run_engine();
        let output =
            engine.dry_run_response("Task: Check suspicious listener ports and summarize risk");
        assert!(output.contains("\"tool\":\"scan_network\""));
    }

    #[test]
    fn routes_persistence_task_to_inspect_persistence_locations() {
        let engine = dry_run_engine();
        let output =
            engine.dry_run_response("Task: Inspect persistence locations for suspicious autoruns");
        assert!(output.contains("\"tool\":\"inspect_persistence_locations\""));
    }

    #[test]
    fn routes_account_change_task_to_audit_account_changes() {
        let engine = dry_run_engine();
        let output = engine
            .dry_run_response("Task: Audit account change activity in admin group membership");
        assert!(output.contains("\"tool\":\"audit_account_changes\""));
    }

    #[test]
    fn routes_process_network_task_to_correlate_process_network() {
        let engine = dry_run_engine();
        let output =
            engine.dry_run_response("Task: Correlate process and network listener exposure");
        assert!(output.contains("\"tool\":\"correlate_process_network\""));
    }

    #[test]
    fn routes_baseline_capture_task_to_capture_coverage_baseline() {
        let engine = dry_run_engine();
        let output = engine.dry_run_response(
            "Task: Capture host coverage baseline for persistence account and network",
        );
        assert!(output.contains("\"tool\":\"capture_coverage_baseline\""));
        assert!(output.contains("\"persistence_limit\":256"));
    }

    #[test]
    fn routes_network_drift_task_to_correlate_process_network() {
        let engine = dry_run_engine();
        let output =
            engine.dry_run_response("Task: Detect baseline drift in externally exposed listeners");
        assert!(output.contains("\"tool\":\"correlate_process_network\""));
    }

    #[test]
    fn routes_account_drift_task_to_audit_account_changes() {
        let engine = dry_run_engine();
        let output =
            engine.dry_run_response("Task: Compare account privilege drift against baseline");
        assert!(output.contains("\"tool\":\"audit_account_changes\""));
    }

    #[test]
    fn routes_log_task_to_read_syslog() {
        let engine = dry_run_engine();
        let output = engine.dry_run_response("Task: Read and summarize ./README.md log context");
        assert!(output.contains("\"tool\":\"read_syslog\""));
        assert!(output.contains("\"max_lines\":200"));
    }

    #[test]
    fn routes_log_task_with_requested_line_count() {
        let engine = dry_run_engine();
        let output = engine.dry_run_response(
            "Task: Read and summarize last 50 lines from ./README.md log context",
        );
        assert!(output.contains("\"tool\":\"read_syslog\""));
        assert!(output.contains("\"max_lines\":50"));
    }

    /// Verify dry-run iterates all template tools then emits `<final>` (#118).
    #[test]
    fn iterates_all_template_tools_then_final() {
        let engine = dry_run_engine();
        // SSH-key template has 4 tools.
        let task = "Task: Investigate unauthorized SSH keys";

        let out1 = engine.dry_run_response(task);
        assert!(out1.contains("\"tool\":\"enumerate_ssh_keys\""));

        let out2 = engine.dry_run_response(&format!("{task}\nObservation: {{}}"));
        assert!(out2.contains("\"tool\":\"audit_account_changes\""));

        let out3 = engine.dry_run_response(&format!("{task}\nObservation: {{}}\nObservation: {{}}"));
        assert!(out3.contains("\"tool\":\"inspect_persistence_locations\""));

        let out4 = engine.dry_run_response(&format!("{task}\n...Observation x3..."));
        assert!(out4.contains("\"tool\":\"check_privilege_escalation_vectors\""));

        let out5 = engine.dry_run_response(&format!("{task}\n...Observation x4..."));
        assert!(out5.contains("<final>"));
    }
}
