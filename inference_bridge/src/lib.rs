pub mod onnx_vitis;

use std::path::PathBuf;

use anyhow::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tracing::debug;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct VitisEpConfig {
    pub config_file: Option<String>,
    pub cache_dir: Option<String>,
    pub cache_key: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelConfig {
    pub model_path: PathBuf,
    pub tokenizer_path: Option<PathBuf>,
    pub max_new_tokens: usize,
    pub temperature: f32,
    pub dry_run: bool,
    pub vitis_config: Option<VitisEpConfig>,
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

    fn dry_run_response(&self, prompt: &str) -> String {
        let lower = prompt.to_ascii_lowercase();
        let task = Self::extract_task_from_prompt(prompt).unwrap_or(prompt);
        let task_lower = task.to_ascii_lowercase();

        if lower.contains("observation:") {
            return "<final>Dry-run cycle complete. Review the latest tool observations and escalate manually if indicators persist.</final>".to_string();
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
            return format!(
                r#"<call>{{"tool":"read_syslog","args":{{"path":"{path}","max_lines":200}}}}</call>"#
            );
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
            vitis_config: None,
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
    fn routes_log_task_to_read_syslog() {
        let engine = dry_run_engine();
        let output = engine.dry_run_response("Task: Read and summarize ./README.md log context");
        assert!(output.contains("\"tool\":\"read_syslog\""));
        assert!(output.contains("\"max_lines\":200"));
    }

    #[test]
    fn returns_final_answer_after_observation() {
        let engine = dry_run_engine();
        let output =
            engine.dry_run_response("Task: Investigate unauthorized SSH keys\nObservation: {}\n");
        assert!(output.contains("<final>"));
    }
}
