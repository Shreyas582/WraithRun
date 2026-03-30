use anyhow::Result;

#[cfg(feature = "vitis")]
use anyhow::{anyhow, bail};

#[cfg(not(feature = "vitis"))]
use anyhow::bail;

use crate::ModelConfig;

#[cfg(feature = "vitis")]
use std::{collections::HashSet, path::PathBuf};

#[cfg(feature = "vitis")]
use ort::{
    ep,
    session::{builder::GraphOptimizationLevel, Session},
    value::{DynValue, Tensor},
};

#[cfg(feature = "vitis")]
use tokenizers::Tokenizer;

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
fn tokenizer_candidates(config: &ModelConfig) -> Vec<PathBuf> {
    let mut candidates = Vec::new();

    if let Some(path) = &config.tokenizer_path {
        candidates.push(path.clone());
    }

    if let Some(parent) = config.model_path.parent() {
        candidates.push(parent.join("tokenizer.json"));
    }

    candidates.push(PathBuf::from("tokenizer.json"));

    let mut unique = Vec::new();
    for candidate in candidates {
        if !unique.iter().any(|existing| existing == &candidate) {
            unique.push(candidate);
        }
    }

    unique
}

#[cfg(feature = "vitis")]
fn load_tokenizer(config: &ModelConfig) -> Result<Tokenizer> {
    for candidate in tokenizer_candidates(config) {
        if candidate.exists() {
            return Tokenizer::from_file(&candidate)
                .map_err(|err| anyhow!("failed to load tokenizer '{}': {err}", candidate.display()));
        }
    }

    bail!(
        "Unable to locate tokenizer.json. Provide --tokenizer <path> or place tokenizer.json beside the ONNX model."
    )
}

#[cfg(feature = "vitis")]
fn encode_prompt(tokenizer: &Tokenizer, prompt: &str) -> Result<Vec<i64>> {
    let encoding = tokenizer
        .encode(prompt, true)
        .map_err(|err| anyhow!("failed to tokenize prompt: {err}"))?;

    let ids: Vec<i64> = encoding.get_ids().iter().map(|id| *id as i64).collect();
    if ids.is_empty() {
        bail!("tokenizer produced an empty input sequence");
    }

    Ok(ids)
}

#[cfg(feature = "vitis")]
fn to_u32_ids(ids: &[i64]) -> Result<Vec<u32>> {
    ids.iter()
        .map(|id| {
            if *id < 0 || *id > u32::MAX as i64 {
                Err(anyhow!("token id out of u32 range: {id}"))
            } else {
                Ok(*id as u32)
            }
        })
        .collect()
}

#[cfg(feature = "vitis")]
fn select_next_token(logits_value: &DynValue) -> Result<i64> {
    let (shape, logits) = ort_result(logits_value.try_extract_tensor::<f32>())?;

    if shape.is_empty() {
        bail!("logits tensor has rank 0, expected rank >= 2");
    }

    let vocab_size = *shape.last().unwrap_or(&0);
    if vocab_size <= 0 {
        bail!("invalid logits vocab dimension: {vocab_size}");
    }

    let vocab_size = vocab_size as usize;
    if logits.len() < vocab_size {
        bail!(
            "logits buffer too small for vocab projection: len={} vocab={vocab_size}",
            logits.len()
        );
    }

    let last_projection = &logits[logits.len() - vocab_size..];
    let mut best_index = 0usize;
    let mut best_score = f32::NEG_INFINITY;

    for (idx, score) in last_projection.iter().enumerate() {
        if score > &best_score {
            best_index = idx;
            best_score = *score;
        }
    }

    Ok(best_index as i64)
}

#[cfg(feature = "vitis")]
fn find_input_name(session: &Session, preferred: &[&str]) -> Option<String> {
    for candidate in preferred {
        if session.inputs().iter().any(|outlet| outlet.name() == *candidate) {
            return Some((*candidate).to_string());
        }
    }

    None
}

#[cfg(feature = "vitis")]
fn discover_stop_token_ids(tokenizer: &Tokenizer) -> HashSet<i64> {
    const COMMON_STOP_TOKENS: &[&str] = &["</s>", "<eos>", "<|end_of_text|>", "<|eot_id|>"];

    COMMON_STOP_TOKENS
        .iter()
        .filter_map(|token| tokenizer.token_to_id(token).map(|id| id as i64))
        .collect()
}

#[cfg(feature = "vitis")]
fn decode_generated(tokenizer: &Tokenizer, generated_ids: &[i64]) -> Result<String> {
    if generated_ids.is_empty() {
        return Ok(String::new());
    }

    let generated_u32 = to_u32_ids(generated_ids)?;
    tokenizer
        .decode(&generated_u32, true)
        .map_err(|err| anyhow!("failed to decode generated token stream: {err}"))
}

#[cfg(feature = "vitis")]
pub fn run_prompt(config: &ModelConfig, prompt: &str) -> Result<String> {
    let mut session = build_session(config)?;
    let tokenizer = load_tokenizer(config)?;

    let input_ids_name = find_input_name(&session, &["input_ids", "tokens"])
        .or_else(|| session.inputs().first().map(|outlet| outlet.name().to_string()))
        .ok_or_else(|| anyhow!("model has no inputs"))?;
    let attention_mask_name = find_input_name(&session, &["attention_mask"]);
    let position_ids_name = find_input_name(&session, &["position_ids"]);
    let token_type_ids_name = find_input_name(&session, &["token_type_ids"]);

    let mut supported_inputs: HashSet<String> = [input_ids_name.clone()].into_iter().collect();
    if let Some(name) = &attention_mask_name {
        let _ = supported_inputs.insert(name.clone());
    }
    if let Some(name) = &position_ids_name {
        let _ = supported_inputs.insert(name.clone());
    }
    if let Some(name) = &token_type_ids_name {
        let _ = supported_inputs.insert(name.clone());
    }

    let unsupported_inputs: Vec<String> = session
        .inputs()
        .iter()
        .map(|outlet| outlet.name().to_string())
        .filter(|name| !supported_inputs.contains(name))
        .collect();

    if !unsupported_inputs.is_empty() {
        bail!(
            "model requires unsupported inputs: {}. Supported inputs currently are input_ids, attention_mask, position_ids, and token_type_ids.",
            unsupported_inputs.join(", ")
        );
    }

    let stop_ids = discover_stop_token_ids(&tokenizer);
    let mut context_ids = encode_prompt(&tokenizer, prompt)?;
    let mut generated_ids: Vec<i64> = Vec::new();

    for _ in 0..config.max_new_tokens.max(1) {
        let sequence_len = context_ids.len();
        let sequence_shape = vec![1_i64, sequence_len as i64];

        let input_ids_tensor = ort_result(Tensor::from_array((
            sequence_shape.clone(),
            context_ids.clone(),
        )))?;

        let mut model_inputs = ort::inputs![input_ids_name.clone() => input_ids_tensor];

        if let Some(name) = &attention_mask_name {
            let attention_tensor = ort_result(Tensor::from_array((
                sequence_shape.clone(),
                vec![1_i64; sequence_len],
            )))?;
            model_inputs.push((name.clone().into(), attention_tensor.into()));
        }

        if let Some(name) = &position_ids_name {
            let position_values: Vec<i64> = (0..sequence_len as i64).collect();
            let position_tensor = ort_result(Tensor::from_array((
                sequence_shape.clone(),
                position_values,
            )))?;
            model_inputs.push((name.clone().into(), position_tensor.into()));
        }

        if let Some(name) = &token_type_ids_name {
            let token_type_tensor = ort_result(Tensor::from_array((
                sequence_shape,
                vec![0_i64; sequence_len],
            )))?;
            model_inputs.push((name.clone().into(), token_type_tensor.into()));
        }

        let outputs = ort_result(session.run(model_inputs))?;
        let logits_value = outputs
            .get("logits")
            .or_else(|| outputs.get("lm_logits"))
            .unwrap_or(&outputs[0]);

        let next_token = select_next_token(logits_value)?;
        context_ids.push(next_token);
        generated_ids.push(next_token);

        if stop_ids.contains(&next_token) {
            break;
        }
    }

    let mut generated_text = decode_generated(&tokenizer, &generated_ids)?;
    if generated_text.trim().is_empty() {
        generated_text = "(model produced no decodable continuation)".to_string();
    }

    Ok(format!("<final>{generated_text}</final>"))
}

#[cfg(not(feature = "vitis"))]
pub fn run_prompt(_config: &ModelConfig, _prompt: &str) -> Result<String> {
    bail!("Vitis inference is disabled. Rebuild with '--features inference_bridge/vitis'.")
}
