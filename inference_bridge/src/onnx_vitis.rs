use anyhow::Result;

#[cfg(feature = "vitis")]
use anyhow::anyhow;

#[cfg(not(feature = "vitis"))]
use anyhow::bail;

use crate::ModelConfig;

#[cfg(feature = "vitis")]
use ort::{
    ep,
    session::{builder::GraphOptimizationLevel, Session},
};

#[cfg(feature = "vitis")]
fn ort_result<T, E: std::fmt::Display>(result: std::result::Result<T, E>) -> Result<T> {
    result.map_err(|err| anyhow!(err.to_string()))
}

#[cfg(feature = "vitis")]
fn build_session(config: &ModelConfig) -> Result<Session> {
    let mut vitis = ep::Vitis::default();

    if let Some(vitis_cfg) = &config.vitis_config {
        if let Some(config_file) = &vitis_cfg.config_file {
            vitis = vitis.with_config_file(config_file);
        }
        if let Some(cache_dir) = &vitis_cfg.cache_dir {
            vitis = vitis.with_cache_dir(cache_dir);
        }
        if let Some(cache_key) = &vitis_cfg.cache_key {
            vitis = vitis.with_cache_key(cache_key);
        }
    }

    let builder = ort_result(Session::builder())?;
    let builder = ort_result(builder.with_optimization_level(GraphOptimizationLevel::Level3))?;
    let builder = ort_result(builder.with_execution_providers([vitis.build()]))?;
    let mut builder = ort_result(builder.with_disable_cpu_fallback())?;
    let session = ort_result(builder.commit_from_file(&config.model_path))?;

    Ok(session)
}

#[cfg(feature = "vitis")]
pub fn run_prompt(config: &ModelConfig, prompt: &str) -> Result<String> {
    let session = build_session(config)?;

    let input_count = session.inputs().len();
    let output_count = session.outputs().len();

    Ok(format!(
        "<final>Vitis session initialized (inputs={input_count}, outputs={output_count}) for model '{}'. Prompt length is {} characters. Implement tokenizer and token decode loop to produce generated text from model logits.</final>",
        config.model_path.display(),
        prompt.len()
    ))
}

#[cfg(not(feature = "vitis"))]
pub fn run_prompt(_config: &ModelConfig, _prompt: &str) -> Result<String> {
    bail!("Vitis inference is disabled. Rebuild with '--features inference_bridge/vitis'.")
}
