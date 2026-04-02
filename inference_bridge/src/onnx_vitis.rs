use anyhow::Result;

#[cfg(feature = "vitis")]
use anyhow::{anyhow, bail};

#[cfg(not(feature = "vitis"))]
use anyhow::bail;

use crate::ModelConfig;

#[cfg(feature = "vitis")]
use std::{
    borrow::Cow,
    collections::{HashMap, HashSet},
    path::PathBuf,
};

#[cfg(feature = "vitis")]
use half::f16;

#[cfg(feature = "vitis")]
use ort::{
    ep,
    session::{builder::GraphOptimizationLevel, Session, SessionInputValue},
    value::{DynValue, Outlet, Tensor, TensorElementType, ValueType},
};

#[cfg(feature = "vitis")]
use tokenizers::Tokenizer;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeCompatibilitySeverity {
    Warn,
    Fail,
}

#[derive(Debug, Clone)]
pub struct RuntimeCompatibilityIssue {
    pub severity: RuntimeCompatibilitySeverity,
    pub reason_code: &'static str,
    pub detail: String,
}

#[derive(Debug, Clone, Default)]
pub struct RuntimeCompatibilityReport {
    pub cache_input_count: usize,
    pub cache_output_count: usize,
    pub smoke_check_ran: bool,
    pub issues: Vec<RuntimeCompatibilityIssue>,
}

impl RuntimeCompatibilityReport {
    pub fn has_failures(&self) -> bool {
        self.issues
            .iter()
            .any(|issue| issue.severity == RuntimeCompatibilitySeverity::Fail)
    }

    #[cfg(any(not(feature = "vitis"), test))]
    fn push_warn(&mut self, reason_code: &'static str, detail: impl Into<String>) {
        self.issues.push(RuntimeCompatibilityIssue {
            severity: RuntimeCompatibilitySeverity::Warn,
            reason_code,
            detail: detail.into(),
        });
    }

    #[cfg(feature = "vitis")]
    fn push_fail(&mut self, reason_code: &'static str, detail: impl Into<String>) {
        self.issues.push(RuntimeCompatibilityIssue {
            severity: RuntimeCompatibilitySeverity::Fail,
            reason_code,
            detail: detail.into(),
        });
    }
}

#[cfg(any(feature = "vitis", test))]
fn is_cache_tensor_name(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    let has_cache_marker = lower.contains("past_key_values")
        || lower.starts_with("past.")
        || lower.starts_with("present.")
        || lower.contains(".past.")
        || lower.contains(".present.");

    has_cache_marker && (lower.contains("key") || lower.contains("value"))
}

#[cfg(feature = "vitis")]
fn is_cache_position_input_name(name: &str) -> bool {
    name.eq_ignore_ascii_case("cache_position")
}

#[cfg(feature = "vitis")]
fn is_use_cache_input_name(name: &str) -> bool {
    name.eq_ignore_ascii_case("use_cache") || name.eq_ignore_ascii_case("use_cache_branch")
}

#[cfg(any(feature = "vitis", test))]
fn cache_slot_key(name: &str) -> String {
    name.to_ascii_lowercase()
        .replace("past_key_values.", "")
        .replace("present.", "")
        .replace("past.", "")
        .replace("key_values.", "")
}

#[cfg(any(feature = "vitis", test))]
fn resolve_cache_output_name(input_name: &str, output_names: &[String]) -> Option<String> {
    let mut candidates = vec![input_name.to_string()];

    if let Some(rest) = input_name.strip_prefix("past_key_values.") {
        candidates.push(format!("present.{rest}"));
    }
    if let Some(rest) = input_name.strip_prefix("past.") {
        candidates.push(format!("present.{rest}"));
    }

    for candidate in candidates {
        if output_names.iter().any(|name| name == &candidate) {
            return Some(candidate);
        }
    }

    let key = cache_slot_key(input_name);
    let mut matches: Vec<String> = output_names
        .iter()
        .filter(|name| is_cache_tensor_name(name) && cache_slot_key(name) == key)
        .cloned()
        .collect();

    if matches.is_empty() {
        return None;
    }

    matches.sort_by_key(|name| usize::from(!name.to_ascii_lowercase().contains("present")));
    matches.into_iter().next()
}

#[cfg(feature = "vitis")]
#[derive(Debug, Clone)]
struct InputSpec {
    name: String,
    element_type: TensorElementType,
    shape: Vec<i64>,
}

#[cfg(feature = "vitis")]
#[derive(Debug, Clone)]
struct CacheTensorSpec {
    input: InputSpec,
    output_name: String,
    past_axis: usize,
}

#[cfg(feature = "vitis")]
#[derive(Debug, Clone)]
struct SessionLayout {
    input_ids: InputSpec,
    attention_mask: Option<InputSpec>,
    position_ids: Option<InputSpec>,
    token_type_ids: Option<InputSpec>,
    cache_position: Option<InputSpec>,
    use_cache: Option<InputSpec>,
    logits_output_name: String,
    cache_specs: Vec<CacheTensorSpec>,
}

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
            return Tokenizer::from_file(&candidate).map_err(|err| {
                anyhow!("failed to load tokenizer '{}': {err}", candidate.display())
            });
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
fn find_input_name(session: &Session, preferred: &[&str]) -> Option<String> {
    for candidate in preferred {
        if session
            .inputs()
            .iter()
            .any(|outlet| outlet.name() == *candidate)
        {
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
fn input_spec_from_outlet(outlet: &Outlet) -> Result<InputSpec> {
    match outlet.dtype() {
        ValueType::Tensor { ty, shape, .. } => Ok(InputSpec {
            name: outlet.name().to_string(),
            element_type: *ty,
            shape: shape.iter().copied().collect(),
        }),
        _ => bail!("input '{}' is not a tensor", outlet.name()),
    }
}

#[cfg(feature = "vitis")]
fn infer_cache_past_axis(shape: &[i64]) -> usize {
    if shape.len() >= 2 {
        shape.len() - 2
    } else {
        0
    }
}

#[cfg(feature = "vitis")]
fn is_supported_cache_element_type(element_type: TensorElementType) -> bool {
    matches!(
        element_type,
        TensorElementType::Float32
            | TensorElementType::Float16
            | TensorElementType::Int64
            | TensorElementType::Int32
            | TensorElementType::Bool
    )
}

#[cfg(feature = "vitis")]
fn is_supported_sequence_element_type(element_type: TensorElementType) -> bool {
    matches!(
        element_type,
        TensorElementType::Int64 | TensorElementType::Int32
    )
}

#[cfg(feature = "vitis")]
fn is_supported_use_cache_element_type(element_type: TensorElementType) -> bool {
    matches!(
        element_type,
        TensorElementType::Bool | TensorElementType::Int64 | TensorElementType::Int32
    )
}

#[cfg(feature = "vitis")]
fn analyze_session_layout(
    session: &Session,
) -> (Option<SessionLayout>, RuntimeCompatibilityReport) {
    let mut report = RuntimeCompatibilityReport::default();

    let input_ids_name = find_input_name(session, &["input_ids", "tokens"]).or_else(|| {
        session
            .inputs()
            .first()
            .map(|outlet| outlet.name().to_string())
    });

    let Some(input_ids_name) = input_ids_name else {
        report.push_fail(
            "runtime_input_ids_missing",
            "Model signature does not expose an input_ids/tokens input.",
        );
        return (None, report);
    };

    let input_ids_outlet = session
        .inputs()
        .iter()
        .find(|outlet| outlet.name() == input_ids_name)
        .expect("input_ids outlet should exist");

    let input_ids = match input_spec_from_outlet(input_ids_outlet) {
        Ok(spec) => spec,
        Err(err) => {
            report.push_fail(
                "runtime_input_ids_invalid",
                format!("Unable to parse input_ids spec: {err}"),
            );
            return (None, report);
        }
    };

    let attention_mask = find_input_name(session, &["attention_mask"]).and_then(|name| {
        session
            .inputs()
            .iter()
            .find(|outlet| outlet.name() == name)
            .and_then(|outlet| input_spec_from_outlet(outlet).ok())
    });
    let position_ids = find_input_name(session, &["position_ids"]).and_then(|name| {
        session
            .inputs()
            .iter()
            .find(|outlet| outlet.name() == name)
            .and_then(|outlet| input_spec_from_outlet(outlet).ok())
    });
    let token_type_ids = find_input_name(session, &["token_type_ids"]).and_then(|name| {
        session
            .inputs()
            .iter()
            .find(|outlet| outlet.name() == name)
            .and_then(|outlet| input_spec_from_outlet(outlet).ok())
    });
    let cache_position = session
        .inputs()
        .iter()
        .find(|outlet| is_cache_position_input_name(outlet.name()))
        .and_then(|outlet| input_spec_from_outlet(outlet).ok());
    let use_cache = session
        .inputs()
        .iter()
        .find(|outlet| is_use_cache_input_name(outlet.name()))
        .and_then(|outlet| input_spec_from_outlet(outlet).ok());

    let mut known_inputs: HashSet<String> = [input_ids.name.clone()].into_iter().collect();
    if let Some(spec) = &attention_mask {
        let _ = known_inputs.insert(spec.name.clone());
    }
    if let Some(spec) = &position_ids {
        let _ = known_inputs.insert(spec.name.clone());
    }
    if let Some(spec) = &token_type_ids {
        let _ = known_inputs.insert(spec.name.clone());
    }
    if let Some(spec) = &cache_position {
        let _ = known_inputs.insert(spec.name.clone());
    }
    if let Some(spec) = &use_cache {
        let _ = known_inputs.insert(spec.name.clone());
    }

    let mut cache_specs = Vec::new();
    let mut unsupported_inputs = Vec::new();

    for outlet in session.inputs() {
        let name = outlet.name().to_string();
        if known_inputs.contains(&name) {
            continue;
        }

        if is_cache_tensor_name(&name) {
            match input_spec_from_outlet(outlet) {
                Ok(spec) => {
                    if !is_supported_cache_element_type(spec.element_type) {
                        report.push_fail(
                            "runtime_cache_dtype_unsupported",
                            format!(
                                "Cache input '{}' uses unsupported tensor type {:?}.",
                                spec.name, spec.element_type
                            ),
                        );
                    }

                    cache_specs.push(CacheTensorSpec {
                        past_axis: infer_cache_past_axis(&spec.shape),
                        input: spec,
                        output_name: String::new(),
                    });
                }
                Err(err) => report.push_fail(
                    "runtime_cache_input_invalid",
                    format!("Cache input '{name}' is invalid: {err}"),
                ),
            }
            continue;
        }

        unsupported_inputs.push(name);
    }

    if !unsupported_inputs.is_empty() {
        report.push_fail(
            "runtime_input_unsupported",
            format!(
                "Model requires unsupported runtime inputs: {}",
                unsupported_inputs.join(", ")
            ),
        );
    }

    if !is_supported_sequence_element_type(input_ids.element_type) {
        report.push_fail(
            "runtime_input_dtype_unsupported",
            format!(
                "input_ids/tokens expects unsupported tensor type {:?}.",
                input_ids.element_type
            ),
        );
    }

    for spec in [
        attention_mask.as_ref(),
        position_ids.as_ref(),
        token_type_ids.as_ref(),
        cache_position.as_ref(),
    ]
    .into_iter()
    .flatten()
    {
        if !is_supported_sequence_element_type(spec.element_type) {
            report.push_fail(
                "runtime_input_dtype_unsupported",
                format!(
                    "Input '{}' expects unsupported tensor type {:?}.",
                    spec.name, spec.element_type
                ),
            );
        }
    }

    if let Some(spec) = use_cache.as_ref() {
        if !is_supported_use_cache_element_type(spec.element_type) {
            report.push_fail(
                "runtime_input_dtype_unsupported",
                format!(
                    "Input '{}' expects unsupported tensor type {:?}.",
                    spec.name, spec.element_type
                ),
            );
        }
    }

    let output_names: Vec<String> = session
        .outputs()
        .iter()
        .map(|outlet| outlet.name().to_string())
        .collect();

    let logits_output_name = ["logits", "lm_logits"]
        .iter()
        .find(|candidate| output_names.iter().any(|name| name == *candidate))
        .map(|candidate| (*candidate).to_string())
        .or_else(|| {
            output_names
                .iter()
                .find(|name| !is_cache_tensor_name(name))
                .cloned()
        });

    let Some(logits_output_name) = logits_output_name else {
        report.push_fail(
            "runtime_logits_output_missing",
            "Model outputs do not expose logits/lm_logits or any non-cache output.",
        );
        return (None, report);
    };

    if !cache_specs.is_empty() {
        let cache_output_names: Vec<String> = output_names
            .iter()
            .filter(|name| is_cache_tensor_name(name))
            .cloned()
            .collect();

        if cache_output_names.is_empty() {
            report.push_fail(
                "runtime_cache_output_missing",
                "Model exposes cache inputs but no cache outputs were detected.",
            );
        }

        for spec in &mut cache_specs {
            match resolve_cache_output_name(&spec.input.name, &cache_output_names) {
                Some(output_name) => {
                    spec.output_name = output_name;
                }
                None => report.push_fail(
                    "runtime_cache_output_missing",
                    format!(
                        "Could not map cache input '{}' to any cache output.",
                        spec.input.name
                    ),
                ),
            }
        }
    }

    report.cache_input_count = cache_specs.len();
    report.cache_output_count = cache_specs.len();

    if report.has_failures() {
        return (None, report);
    }

    (
        Some(SessionLayout {
            input_ids,
            attention_mask,
            position_ids,
            token_type_ids,
            cache_position,
            use_cache,
            logits_output_name,
            cache_specs,
        }),
        report,
    )
}

#[cfg(feature = "vitis")]
fn num_elements(shape: &[i64]) -> Result<usize> {
    let mut total = 1usize;
    for dim in shape {
        if *dim < 0 {
            bail!("tensor shape contains unresolved dynamic dimension: {dim}");
        }
        total = total
            .checked_mul(*dim as usize)
            .ok_or_else(|| anyhow!("tensor shape is too large"))?;
    }

    Ok(total)
}

#[cfg(feature = "vitis")]
fn materialize_cache_shape(template: &[i64], past_axis: usize, past_len: usize) -> Vec<i64> {
    let mut shape = if template.is_empty() {
        vec![past_len as i64]
    } else {
        template.to_vec()
    };

    for (idx, dim) in shape.iter_mut().enumerate() {
        if *dim < 0 {
            *dim = if idx == past_axis { past_len as i64 } else { 1 };
        }
        if idx != past_axis && *dim == 0 {
            *dim = 1;
        }
    }

    if past_axis < shape.len() && shape[past_axis] <= 0 {
        shape[past_axis] = past_len as i64;
    }

    shape
}

#[cfg(feature = "vitis")]
fn materialize_sequence_shape(template: &[i64], sequence_len: usize) -> Vec<i64> {
    let mut shape = if template.is_empty() {
        vec![sequence_len as i64]
    } else {
        template.to_vec()
    };

    let seq_axis = shape.len().saturating_sub(1);
    for (idx, dim) in shape.iter_mut().enumerate() {
        if *dim < 0 {
            *dim = if idx == seq_axis {
                sequence_len as i64
            } else {
                1
            };
        }
        if idx != seq_axis && *dim == 0 {
            *dim = 1;
        }
    }

    if seq_axis < shape.len() && shape[seq_axis] <= 0 {
        shape[seq_axis] = sequence_len as i64;
    }

    shape
}

#[cfg(feature = "vitis")]
fn fit_int_values(values: &[i64], target_len: usize) -> Vec<i64> {
    match target_len {
        0 => Vec::new(),
        n if n <= values.len() => values[..n].to_vec(),
        n => {
            let mut fitted = values.to_vec();
            let pad_value = values.last().copied().unwrap_or(0);
            fitted.resize(n, pad_value);
            fitted
        }
    }
}

#[cfg(feature = "vitis")]
fn build_int_tensor(
    shape: Vec<i64>,
    values: &[i64],
    element_type: TensorElementType,
) -> Result<DynValue> {
    let target_len = num_elements(&shape)?;
    let fitted = fit_int_values(values, target_len);

    match element_type {
        TensorElementType::Int64 => {
            let tensor = ort_result(Tensor::from_array((shape, fitted)))?;
            Ok(tensor.into_dyn())
        }
        TensorElementType::Int32 => {
            let mut data = Vec::with_capacity(fitted.len());
            for value in fitted {
                let cast = i32::try_from(value)
                    .map_err(|_| anyhow!("value out of i32 range for int32 tensor: {value}"))?;
                data.push(cast);
            }
            let tensor = ort_result(Tensor::from_array((shape, data)))?;
            Ok(tensor.into_dyn())
        }
        _ => bail!("unsupported integer tensor type: {element_type:?}"),
    }
}

#[cfg(feature = "vitis")]
fn build_use_cache_tensor(spec: &InputSpec, enabled: bool) -> Result<DynValue> {
    let shape = materialize_sequence_shape(&spec.shape, 1);
    let target_len = num_elements(&shape)?;

    match spec.element_type {
        TensorElementType::Bool => {
            let tensor = ort_result(Tensor::from_array((shape, vec![enabled; target_len])))?;
            Ok(tensor.into_dyn())
        }
        TensorElementType::Int64 | TensorElementType::Int32 => {
            let value = if enabled { 1_i64 } else { 0_i64 };
            build_int_tensor(shape, &[value], spec.element_type)
        }
        _ => bail!(
            "unsupported use_cache input tensor type {:?} for '{}'.",
            spec.element_type,
            spec.name
        ),
    }
}

#[cfg(feature = "vitis")]
fn build_zero_tensor(shape: Vec<i64>, element_type: TensorElementType) -> Result<DynValue> {
    let target_len = num_elements(&shape)?;

    match element_type {
        TensorElementType::Float32 => {
            let tensor = ort_result(Tensor::from_array((shape, vec![0_f32; target_len])))?;
            Ok(tensor.into_dyn())
        }
        TensorElementType::Float16 => {
            let tensor = ort_result(Tensor::from_array((
                shape,
                vec![f16::from_f32(0.0); target_len],
            )))?;
            Ok(tensor.into_dyn())
        }
        TensorElementType::Int64 | TensorElementType::Int32 => {
            build_int_tensor(shape, &[0_i64], element_type)
        }
        TensorElementType::Bool => {
            let tensor = ort_result(Tensor::from_array((shape, vec![false; target_len])))?;
            Ok(tensor.into_dyn())
        }
        _ => bail!("unsupported zero-tensor type: {element_type:?}"),
    }
}

#[cfg(feature = "vitis")]
fn initialize_cache_state(
    layout: &SessionLayout,
    past_len: usize,
) -> Result<HashMap<String, DynValue>> {
    let mut state = HashMap::new();

    for spec in &layout.cache_specs {
        let cache_shape = materialize_cache_shape(&spec.input.shape, spec.past_axis, past_len);
        let cache_value = build_zero_tensor(cache_shape, spec.input.element_type)?;
        state.insert(spec.input.name.clone(), cache_value);
    }

    Ok(state)
}

#[cfg(feature = "vitis")]
fn build_model_inputs<'a>(
    layout: &SessionLayout,
    step_input_ids: &[i64],
    attention_len: usize,
    use_cache_branch: bool,
    cache_state: &'a HashMap<String, DynValue>,
) -> Result<Vec<(Cow<'a, str>, SessionInputValue<'a>)>> {
    let mut model_inputs: Vec<(Cow<'a, str>, SessionInputValue<'a>)> = Vec::new();

    let input_shape = materialize_sequence_shape(&layout.input_ids.shape, step_input_ids.len());
    let input_tensor =
        build_int_tensor(input_shape, step_input_ids, layout.input_ids.element_type)?;
    model_inputs.push((
        Cow::Owned(layout.input_ids.name.clone()),
        input_tensor.into(),
    ));

    if let Some(spec) = layout.attention_mask.as_ref() {
        let mask_values = vec![1_i64; attention_len.max(1)];
        let mask_shape = materialize_sequence_shape(&spec.shape, mask_values.len());
        let mask_tensor = build_int_tensor(mask_shape, &mask_values, spec.element_type)?;
        model_inputs.push((Cow::Owned(spec.name.clone()), mask_tensor.into()));
    }

    if let Some(spec) = layout.position_ids.as_ref() {
        let position_values: Vec<i64> =
            if !layout.cache_specs.is_empty() && step_input_ids.len() == 1 {
                vec![attention_len.saturating_sub(1) as i64]
            } else {
                (0..step_input_ids.len() as i64).collect()
            };
        let position_shape = materialize_sequence_shape(&spec.shape, position_values.len());
        let position_tensor =
            build_int_tensor(position_shape, &position_values, spec.element_type)?;
        model_inputs.push((Cow::Owned(spec.name.clone()), position_tensor.into()));
    }

    if let Some(spec) = layout.token_type_ids.as_ref() {
        let token_type_values = vec![0_i64; step_input_ids.len().max(1)];
        let token_type_shape = materialize_sequence_shape(&spec.shape, token_type_values.len());
        let token_type_tensor =
            build_int_tensor(token_type_shape, &token_type_values, spec.element_type)?;
        model_inputs.push((Cow::Owned(spec.name.clone()), token_type_tensor.into()));
    }

    if let Some(spec) = layout.cache_position.as_ref() {
        let cache_position_values: Vec<i64> =
            if !layout.cache_specs.is_empty() && step_input_ids.len() == 1 {
                vec![attention_len.saturating_sub(1) as i64]
            } else {
                (0..step_input_ids.len() as i64).collect()
            };
        let cache_position_shape =
            materialize_sequence_shape(&spec.shape, cache_position_values.len());
        let cache_position_tensor = build_int_tensor(
            cache_position_shape,
            &cache_position_values,
            spec.element_type,
        )?;
        model_inputs.push((Cow::Owned(spec.name.clone()), cache_position_tensor.into()));
    }

    if let Some(spec) = layout.use_cache.as_ref() {
        let use_cache_tensor = build_use_cache_tensor(spec, use_cache_branch)?;
        model_inputs.push((Cow::Owned(spec.name.clone()), use_cache_tensor.into()));
    }

    for spec in &layout.cache_specs {
        let Some(value) = cache_state.get(&spec.input.name) else {
            bail!("cache state is missing tensor for '{}'.", spec.input.name);
        };
        model_inputs.push((Cow::Owned(spec.input.name.clone()), value.view().into()));
    }

    Ok(model_inputs)
}

#[cfg(feature = "vitis")]
fn select_next_token(logits_value: &DynValue) -> Result<i64> {
    if let Ok((shape, logits)) = ort_result(logits_value.try_extract_tensor::<f32>()) {
        return select_next_token_from_f32(shape, logits);
    }

    if let Ok((shape, logits)) = ort_result(logits_value.try_extract_tensor::<f16>()) {
        return select_next_token_from_f16(shape, logits);
    }

    bail!("logits tensor is neither f32 nor f16");
}

#[cfg(feature = "vitis")]
fn select_next_token_from_f32(shape: &[i64], logits: &[f32]) -> Result<i64> {
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

    let start = logits.len() - vocab_size;
    let mut best_index = 0usize;
    let mut best_score = f32::NEG_INFINITY;

    for idx in 0..vocab_size {
        let score = logits[start + idx];
        if score > best_score {
            best_index = idx;
            best_score = score;
        }
    }

    Ok(best_index as i64)
}

#[cfg(feature = "vitis")]
fn select_next_token_from_f16(shape: &[i64], logits: &[f16]) -> Result<i64> {
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

    let start = logits.len() - vocab_size;
    let mut best_index = 0usize;
    let mut best_score = f32::NEG_INFINITY;

    for idx in 0..vocab_size {
        let score = logits[start + idx].to_f32();
        if score > best_score {
            best_index = idx;
            best_score = score;
        }
    }

    Ok(best_index as i64)
}

#[cfg(feature = "vitis")]
fn run_runtime_smoke_check(session: &mut Session, layout: &SessionLayout) -> Result<()> {
    let cache_state = if layout.cache_specs.is_empty() {
        HashMap::new()
    } else {
        initialize_cache_state(layout, 0)?
    };

    let step_input_ids = vec![1_i64];
    let model_inputs = build_model_inputs(
        layout,
        &step_input_ids,
        step_input_ids.len(),
        false,
        &cache_state,
    )?;
    let mut outputs = ort_result(session.run(model_inputs))?;

    let logits_value = outputs
        .get(&layout.logits_output_name)
        .ok_or_else(|| anyhow!("logits output '{}' missing", layout.logits_output_name))?;
    let _ = select_next_token(logits_value)?;

    for spec in &layout.cache_specs {
        if outputs.remove(&spec.output_name).is_none() {
            bail!(
                "cache output '{}' was not produced during smoke check",
                spec.output_name
            );
        }
    }

    Ok(())
}

#[cfg(feature = "vitis")]
pub fn inspect_runtime_compatibility(
    config: &ModelConfig,
    run_smoke_check: bool,
) -> RuntimeCompatibilityReport {
    let mut report = RuntimeCompatibilityReport::default();

    let mut session = match build_session(config) {
        Ok(session) => session,
        Err(err) => {
            report.push_fail(
                "runtime_session_init_failed",
                format!("Unable to initialize ONNX session: {err}"),
            );
            return report;
        }
    };

    let (layout, mut analysis) = analyze_session_layout(&session);
    report.cache_input_count = analysis.cache_input_count;
    report.cache_output_count = analysis.cache_output_count;
    report.issues.append(&mut analysis.issues);

    if report.has_failures() {
        return report;
    }

    if run_smoke_check {
        report.smoke_check_ran = true;

        if let Some(layout) = layout.as_ref() {
            if let Err(err) = run_runtime_smoke_check(&mut session, layout) {
                report.push_fail(
                    "runtime_forward_smoke_failed",
                    format!("Runtime smoke check failed: {err}"),
                );
            }
        }
    }

    report
}

#[cfg(not(feature = "vitis"))]
pub fn inspect_runtime_compatibility(
    _config: &ModelConfig,
    _run_smoke_check: bool,
) -> RuntimeCompatibilityReport {
    let mut report = RuntimeCompatibilityReport::default();
    report.push_warn(
        "vitis_feature_disabled",
        "Runtime compatibility checks were skipped because Vitis support is disabled in this build.",
    );
    report
}

#[cfg(feature = "vitis")]
pub fn run_prompt(config: &ModelConfig, prompt: &str) -> Result<String> {
    let mut session = build_session(config)?;
    let tokenizer = load_tokenizer(config)?;

    let (layout, compatibility) = analyze_session_layout(&session);
    if compatibility.has_failures() {
        let details = compatibility
            .issues
            .iter()
            .map(|issue| format!("{}: {}", issue.reason_code, issue.detail))
            .collect::<Vec<_>>()
            .join("; ");
        bail!("Model runtime compatibility checks failed: {details}");
    }

    let Some(layout) = layout else {
        bail!("Model runtime compatibility checks did not produce a runnable layout.");
    };

    let stop_ids = discover_stop_token_ids(&tokenizer);
    let mut context_ids = encode_prompt(&tokenizer, prompt)?;
    let mut generated_ids: Vec<i64> = Vec::new();

    let cache_enabled = !layout.cache_specs.is_empty();
    let mut cache_state = if cache_enabled {
        initialize_cache_state(&layout, 0)?
    } else {
        HashMap::new()
    };

    for step in 0..config.max_new_tokens.max(1) {
        let decode_with_cache = cache_enabled && step > 0;
        let step_input_ids = if decode_with_cache {
            vec![*context_ids
                .last()
                .ok_or_else(|| anyhow!("empty context ids"))?]
        } else {
            context_ids.clone()
        };

        let attention_len = if cache_enabled {
            context_ids.len().max(1)
        } else {
            step_input_ids.len().max(1)
        };

        let model_inputs = build_model_inputs(
            &layout,
            &step_input_ids,
            attention_len,
            decode_with_cache,
            &cache_state,
        )?;

        let mut outputs = ort_result(session.run(model_inputs))?;
        let logits_value = outputs
            .get(&layout.logits_output_name)
            .or_else(|| outputs.get("logits"))
            .or_else(|| outputs.get("lm_logits"))
            .unwrap_or(&outputs[0]);

        let next_token = select_next_token(logits_value)?;

        if cache_enabled {
            let mut next_cache = HashMap::new();
            for spec in &layout.cache_specs {
                let cache_value = outputs.remove(&spec.output_name).ok_or_else(|| {
                    anyhow!("cache output '{}' missing while decoding", spec.output_name)
                })?;
                next_cache.insert(spec.input.name.clone(), cache_value);
            }
            cache_state = next_cache;
        }

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

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use crate::ModelConfig;

    use super::{
        cache_slot_key, inspect_runtime_compatibility, is_cache_tensor_name,
        resolve_cache_output_name, RuntimeCompatibilitySeverity,
    };

    #[test]
    fn detects_qwen_style_cache_names() {
        assert!(is_cache_tensor_name("past_key_values.0.key"));
        assert!(is_cache_tensor_name("past_key_values.0.value"));
        assert!(is_cache_tensor_name("present.11.key"));
        assert!(is_cache_tensor_name("present.11.value"));
        assert!(!is_cache_tensor_name("attention_mask"));
        assert!(!is_cache_tensor_name("cache_position"));
    }

    #[test]
    fn normalizes_cache_slot_key_for_past_and_present() {
        let input_key = cache_slot_key("past_key_values.7.key");
        let output_key = cache_slot_key("present.7.key");
        assert_eq!(input_key, output_key);
    }

    #[test]
    fn resolves_cache_output_name_preferring_present_prefix() {
        let outputs = vec![
            "logits".to_string(),
            "present.0.key".to_string(),
            "present.0.value".to_string(),
        ];

        let mapped = resolve_cache_output_name("past_key_values.0.key", &outputs)
            .expect("cache output should resolve");
        assert_eq!(mapped, "present.0.key");
    }

    #[cfg(not(feature = "vitis"))]
    #[test]
    fn runtime_compatibility_reports_feature_disabled_warning_without_vitis() {
        let config = ModelConfig {
            model_path: PathBuf::from("./missing.onnx"),
            tokenizer_path: None,
            max_new_tokens: 1,
            temperature: 0.0,
            dry_run: false,
            vitis_config: None,
        };

        let report = inspect_runtime_compatibility(&config, true);
        assert!(!report.has_failures());
        assert!(report.issues.iter().any(|issue| {
            issue.severity == RuntimeCompatibilitySeverity::Warn
                && issue.reason_code == "vitis_feature_disabled"
        }));
    }
}
