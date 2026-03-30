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

    fn dry_run_response(&self, prompt: &str) -> String {
        let lower = prompt.to_ascii_lowercase();

        if lower.contains("observation:") {
            return "<final>Dry-run cycle complete. Review the latest tool observations and escalate manually if indicators persist.</final>".to_string();
        }

        if lower.contains("ssh") {
            return r#"<call>{"tool":"check_privilege_escalation_vectors","args":{}}</call>"#
                .to_string();
        }

        if lower.contains("network") {
            return r#"<call>{"tool":"scan_network","args":{"limit":40}}</call>"#.to_string();
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
