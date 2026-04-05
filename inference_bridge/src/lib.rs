pub mod backend;
pub mod onnx_vitis;

use std::path::PathBuf;

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
/// Assumes ~2 bytes per parameter (float16/bfloat16 quantised models).
fn estimate_params_from_file_size(model_path: &PathBuf) -> f32 {
    match std::fs::metadata(model_path) {
        Ok(meta) => {
            let bytes = meta.len() as f64;
            // ~2 bytes per param for fp16/bf16; adjust for overhead (~10%).
            let estimated_params = bytes / 2.2;
            (estimated_params / 1_000_000_000.0) as f32
        }
        Err(_) => 0.0,
    }
}

/// Detect which execution provider would be used for this config.
fn detect_execution_provider(config: &ModelConfig) -> String {
    if config.backend_override.as_deref() == Some("vitis")
        || config.backend_config.contains_key("config_file")
    {
        "VitisAIExecutionProvider".to_string()
    } else if cfg!(feature = "onnx") {
        // Without Vitis config, ONNX Runtime defaults to CPU.
        "CPUExecutionProvider".to_string()
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
}

impl OnnxVitisEngine {
    pub fn new(config: ModelConfig) -> Self {
        Self { config }
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
        let lower = prompt.to_ascii_lowercase();
        let task = Self::extract_task_from_prompt(prompt).unwrap_or(prompt);
        let task_lower = task.to_ascii_lowercase();

        if lower.contains("observation:") {
            return "<final>Dry-run cycle complete. Review the latest tool observations and escalate manually if indicators persist.</final>".to_string();
        }

        if (task_lower.contains("baseline") || task_lower.contains("golden"))
            && (task_lower.contains("capture")
                || task_lower.contains("snapshot")
                || task_lower.contains("collect")
                || task_lower.contains("build"))
            && (task_lower.contains("coverage")
                || task_lower.contains("persistence")
                || task_lower.contains("account")
                || task_lower.contains("network")
                || task_lower.contains("host"))
        {
            return r#"<call>{"tool":"capture_coverage_baseline","args":{"persistence_limit":256,"listener_limit":128}}</call>"#.to_string();
        }

        if task_lower.contains("hash")
            || task_lower.contains("sha256")
            || task_lower.contains("checksum")
            || task_lower.contains("integrity")
        {
            let path =
                Self::guess_path_from_task(task).unwrap_or_else(|| "./Cargo.toml".to_string());
            let path = Self::escape_json_string(&path);
            return format!(r#"<call>{{"tool":"hash_binary","args":{{"path":"{path}"}}}}</call>"#);
        }

        if task_lower.contains("log") || task_lower.contains("syslog") {
            let path =
                Self::guess_path_from_task(task).unwrap_or_else(|| "./README.md".to_string());
            let path = Self::escape_json_string(&path);
            let max_lines = Self::guess_line_count_from_task(task).unwrap_or(200);
            return format!(
                r#"<call>{{"tool":"read_syslog","args":{{"path":"{path}","max_lines":{max_lines}}}}}</call>"#
            );
        }

        if task_lower.contains("persistence")
            || task_lower.contains("startup")
            || task_lower.contains("autorun")
            || task_lower.contains("run key")
        {
            return r#"<call>{"tool":"inspect_persistence_locations","args":{"limit":200}}</call>"#
                .to_string();
        }

        if task_lower.contains("account")
            && (task_lower.contains("change")
                || task_lower.contains("admin")
                || task_lower.contains("group")
                || task_lower.contains("privilege")
                || task_lower.contains("baseline")
                || task_lower.contains("drift"))
        {
            return r#"<call>{"tool":"audit_account_changes","args":{}}</call>"#.to_string();
        }

        if (task_lower.contains("baseline") || task_lower.contains("drift"))
            && (task_lower.contains("network")
                || task_lower.contains("listener")
                || task_lower.contains("port")
                || task_lower.contains("socket"))
        {
            return r#"<call>{"tool":"correlate_process_network","args":{"limit":64}}</call>"#
                .to_string();
        }

        if (task_lower.contains("process")
            && (task_lower.contains("network")
                || task_lower.contains("listener")
                || task_lower.contains("port")
                || task_lower.contains("socket")))
            || (task_lower.contains("correlat") && task_lower.contains("network"))
        {
            return r#"<call>{"tool":"correlate_process_network","args":{"limit":64}}</call>"#
                .to_string();
        }

        if task_lower.contains("network")
            || task_lower.contains("listener")
            || task_lower.contains("port")
            || task_lower.contains("socket")
        {
            return r#"<call>{"tool":"scan_network","args":{"limit":40}}</call>"#.to_string();
        }

        if task_lower.contains("ssh")
            || task_lower.contains("privilege")
            || task_lower.contains("escalation")
            || task_lower.contains("sudo")
            || task_lower.contains("whoami")
        {
            return r#"<call>{"tool":"check_privilege_escalation_vectors","args":{}}</call>"#
                .to_string();
        }

        r#"<call>{"tool":"check_privilege_escalation_vectors","args":{}}</call>"#.to_string()
    }
}

#[async_trait]
impl InferenceEngine for OnnxVitisEngine {
    async fn generate(&self, prompt: &str) -> Result<String> {
        if self.config.dry_run {
            debug!("using dry-run inference path");
            return Ok(self.dry_run_response(prompt));
        }

        onnx_vitis::run_prompt(&self.config, prompt)
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

    #[test]
    fn returns_final_answer_after_observation() {
        let engine = dry_run_engine();
        let output =
            engine.dry_run_response("Task: Investigate unauthorized SSH keys\nObservation: {}\n");
        assert!(output.contains("<final>"));
    }
}
