use anyhow::Result;

#[cfg(feature = "onnx")]
use anyhow::{anyhow, bail};

#[cfg(not(feature = "onnx"))]
use anyhow::bail;

use crate::ModelConfig;

#[cfg(feature = "onnx")]
use std::{
    borrow::Cow,
    collections::{HashMap, HashSet},
    ffi::CString,
    fs,
    path::PathBuf,
    time::Instant,
};

#[cfg(feature = "onnx")]
#[cfg_attr(not(feature = "vitis"), allow(unused_imports))]
use std::path::Path;

#[cfg(feature = "onnx")]
use half::f16;

#[cfg(feature = "onnx")]
use ort::{
    session::{
        builder::GraphOptimizationLevel,
        IoBinding, Session, SessionInputValue,
    },
    value::{DynValue, Outlet, Tensor, TensorElementType, ValueType},
    AsPointer,
};

#[cfg(feature = "vitis")]
use ort::{
    ep,
    session::builder::SessionBuilder,
};

#[cfg(feature = "onnx")]
use serde::Deserialize;

#[cfg(feature = "onnx")]
use tokenizers::Tokenizer;

#[cfg(feature = "onnx")]
use tracing::debug;

#[cfg(feature = "vitis")]
use tracing::warn;

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

    #[cfg(any(not(feature = "onnx"), test))]
    fn push_warn(&mut self, reason_code: &'static str, detail: impl Into<String>) {
        self.issues.push(RuntimeCompatibilityIssue {
            severity: RuntimeCompatibilitySeverity::Warn,
            reason_code,
            detail: detail.into(),
        });
    }

    #[cfg(feature = "onnx")]
    fn push_fail(&mut self, reason_code: &'static str, detail: impl Into<String>) {
        self.issues.push(RuntimeCompatibilityIssue {
            severity: RuntimeCompatibilitySeverity::Fail,
            reason_code,
            detail: detail.into(),
        });
    }
}

#[cfg(any(feature = "onnx", test))]
fn is_cache_tensor_name(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    let has_cache_marker = lower.contains("past_key_values")
        || lower.starts_with("past.")
        || lower.starts_with("present.")
        || lower.contains(".past.")
        || lower.contains(".present.");

    has_cache_marker && (lower.contains("key") || lower.contains("value"))
}

#[cfg(feature = "onnx")]
fn is_cache_position_input_name(name: &str) -> bool {
    name.eq_ignore_ascii_case("cache_position")
}

#[cfg(feature = "onnx")]
fn is_use_cache_input_name(name: &str) -> bool {
    name.eq_ignore_ascii_case("use_cache") || name.eq_ignore_ascii_case("use_cache_branch")
}

#[cfg(any(feature = "onnx", test))]
fn cache_slot_key(name: &str) -> String {
    name.to_ascii_lowercase()
        .replace("past_key_values.", "")
        .replace("present.", "")
        .replace("past.", "")
        .replace("key_values.", "")
}

#[cfg(any(feature = "onnx", test))]
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

#[cfg(feature = "onnx")]
#[derive(Debug, Clone)]
struct InputSpec {
    name: String,
    element_type: TensorElementType,
    shape: Vec<i64>,
}

#[cfg(feature = "onnx")]
#[derive(Debug, Clone)]
struct CacheTensorSpec {
    input: InputSpec,
    output_name: String,
    past_axis: usize,
}

#[cfg(feature = "onnx")]
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

#[cfg(feature = "onnx")]
#[derive(Debug, Default, Deserialize)]
#[cfg_attr(not(feature = "vitis"), allow(dead_code))]
struct GenAiConfig {
    model: Option<GenAiModelSection>,
    search: Option<GenAiSearchSection>,
}

#[cfg(feature = "onnx")]
#[derive(Debug, Default, Deserialize)]
struct GenAiSearchSection {
    past_present_share_buffer: Option<bool>,
    max_length: Option<usize>,
}

#[cfg(feature = "onnx")]
#[derive(Debug, Default, Deserialize)]
#[cfg_attr(not(feature = "vitis"), allow(dead_code))]
struct GenAiModelSection {
    decoder: Option<GenAiDecoderSection>,
}

#[cfg(feature = "onnx")]
#[derive(Debug, Default, Deserialize)]
#[cfg_attr(not(feature = "vitis"), allow(dead_code))]
struct GenAiDecoderSection {
    session_options: Option<GenAiSessionOptions>,
}

#[cfg(feature = "onnx")]
#[derive(Debug, Default, Deserialize)]
#[cfg_attr(not(feature = "vitis"), allow(dead_code))]
struct GenAiSessionOptions {
    log_id: Option<String>,
    custom_ops_library: Option<String>,
    custom_allocator: Option<String>,
    external_data_file: Option<String>,
    #[serde(default)]
    config_entries: HashMap<String, String>,
    #[serde(default)]
    provider_options: Vec<HashMap<String, HashMap<String, String>>>,
}

#[cfg(feature = "onnx")]
fn ort_result<T, E: std::fmt::Display + std::fmt::Debug>(
    result: std::result::Result<T, E>,
) -> Result<T> {
    result.map_err(|err| anyhow!(format!("{} | debug: {err:?}", err)))
}

#[cfg(feature = "onnx")]
fn classify_session_init_reason_code(error_text: &str) -> &'static str {
    let normalized = error_text.to_ascii_lowercase();

    if normalized.contains("sin_cos_cache_token") || normalized.contains("_ort_mem_addr_") {
        return "runtime_external_initializer_unresolved";
    }

    if normalized.contains("external file") && normalized.contains("not found") {
        return "runtime_external_data_file_missing";
    }

    if normalized.contains("onnxruntime_providers_vitisai.dll")
        || normalized.contains("vitis ai")
        || normalized.contains("vitis execution provider")
    {
        return "runtime_vitis_provider_missing";
    }

    if normalized.contains("onnxruntime.dll") && normalized.contains("not found") {
        return "runtime_ort_dylib_missing";
    }

    if normalized.contains("assigned to the default cpu ep")
        && normalized.contains("fallback to cpu ep has been explicitly disabled")
    {
        return "runtime_ep_assignment_failed";
    }

    if normalized.contains("custom ops") {
        return "runtime_custom_ops_unavailable";
    }

    if normalized.contains("com.ryzenai")
        && normalized.contains("not a registered function/op")
    {
        return "runtime_custom_ops_unavailable";
    }

    if normalized.contains("invalid model") {
        return "runtime_model_invalid";
    }

    "runtime_session_init_failed"
}

#[cfg(feature = "vitis")]
fn session_init_reason_priority(reason_code: &str) -> u8 {
    match reason_code {
        "runtime_external_initializer_unresolved" => 90,
        "runtime_external_data_file_missing" => 80,
        "runtime_custom_ops_unavailable" => 70,
        "runtime_vitis_provider_missing" => 60,
        "runtime_ort_dylib_missing" => 50,
        "runtime_model_invalid" => 40,
        "runtime_ep_assignment_failed" => 10,
        _ => 0,
    }
}

#[cfg(feature = "vitis")]
fn truncate_session_init_error(error_text: &str, max_chars: usize) -> String {
    let mut chars = error_text.chars();
    let truncated: String = chars.by_ref().take(max_chars).collect();
    if chars.next().is_some() {
        format!("{truncated}...")
    } else {
        truncated
    }
}

#[cfg(feature = "vitis")]
fn resolve_model_companion_path(model_path: &Path, configured_path: &str) -> PathBuf {
    let configured = PathBuf::from(configured_path);
    if configured.is_absolute() {
        return configured;
    }

    model_path
        .parent()
        .map(|parent| parent.join(&configured))
        .unwrap_or(configured)
}

#[cfg(feature = "vitis")]
fn first_existing_path(candidates: Vec<PathBuf>) -> Option<PathBuf> {
    let mut seen = HashSet::new();
    for candidate in candidates {
        if !seen.insert(candidate.clone()) {
            continue;
        }

        if candidate.is_file() {
            return Some(candidate);
        }
    }

    None
}

#[cfg(feature = "vitis")]
fn parse_version_components(value: &str) -> Option<Vec<u32>> {
    if value.is_empty() {
        return None;
    }

    let mut components = Vec::new();
    for part in value.split('.') {
        if part.is_empty() || !part.chars().all(|ch| ch.is_ascii_digit()) {
            return None;
        }

        components.push(part.parse::<u32>().ok()?);
    }

    if components.is_empty() {
        None
    } else {
        Some(components)
    }
}

#[cfg(feature = "vitis")]
fn discover_ryzen_ai_install_roots() -> Vec<PathBuf> {
    let root = PathBuf::from(r"C:\Program Files\RyzenAI");
    let Ok(entries) = fs::read_dir(&root) else {
        return Vec::new();
    };

    let mut installs: Vec<PathBuf> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.is_dir())
        .collect();

    installs.sort_by(|left, right| {
        let left_name = left
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default();
        let right_name = right
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default();

        match (
            parse_version_components(left_name),
            parse_version_components(right_name),
        ) {
            (Some(left_version), Some(right_version)) => right_version.cmp(&left_version),
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (None, None) => right_name.cmp(left_name),
        }
    });

    installs
}

#[cfg(feature = "vitis")]
fn discover_custom_ops_library_path(model_path: &Path, configured_path: &str) -> Option<PathBuf> {
    let resolved = resolve_model_companion_path(model_path, configured_path);
    if resolved.is_file() {
        return Some(resolved);
    }

    let file_name = Path::new(configured_path).file_name()?.to_os_string();
    let mut candidates = Vec::new();

    for env_name in ["ORT_DYLIB_PATH", "WRAITHRUN_ORT_DYLIB_PATH"] {
        let Some(env_path) = std::env::var_os(env_name) else {
            continue;
        };

        let runtime_path = PathBuf::from(env_path);
        if !runtime_path.is_file() {
            continue;
        }

        if let Some(runtime_dir) = runtime_path.parent() {
            candidates.push(runtime_dir.join(&file_name));

            if runtime_dir
                .file_name()
                .map(|name| name.to_string_lossy().eq_ignore_ascii_case("bin"))
                .unwrap_or(false)
            {
                if let Some(parent_dir) = runtime_dir.parent() {
                    candidates.push(parent_dir.join("deployment").join(&file_name));
                }
            }
        }
    }

    for install_root in discover_ryzen_ai_install_roots() {
        candidates.push(install_root.join("deployment").join(&file_name));
        candidates.push(
            install_root
                .join("onnxruntime")
                .join("bin")
                .join(&file_name),
        );
    }

    candidates.push(PathBuf::from(r"C:\Program Files\Amuse").join(&file_name));
    candidates.push(PathBuf::from(r"C:\Program Files\AMD").join(&file_name));
    candidates.push(PathBuf::from(r"C:\Windows\System32").join(&file_name));

    first_existing_path(candidates)
}

#[cfg(feature = "vitis")]
fn discover_ort_dylib_path(config: &ModelConfig) -> Option<PathBuf> {
    let mut candidates = Vec::new();

    if let Some(vitis_config_path) = config
        .vitis_config
        .as_ref()
        .and_then(|cfg| cfg.config_file.as_deref())
    {
        let vitis_config_path = PathBuf::from(vitis_config_path);
        if let Some(parent) = vitis_config_path.parent() {
            candidates.push(parent.join("onnxruntime.dll"));
            candidates.push(parent.join("bin").join("onnxruntime.dll"));
            candidates.push(
                parent
                    .join("onnxruntime")
                    .join("bin")
                    .join("onnxruntime.dll"),
            );

            if let Some(grand_parent) = parent.parent() {
                candidates.push(grand_parent.join("onnxruntime.dll"));
                candidates.push(grand_parent.join("bin").join("onnxruntime.dll"));
                candidates.push(
                    grand_parent
                        .join("onnxruntime")
                        .join("bin")
                        .join("onnxruntime.dll"),
                );
            }
        }
    }

    if let Some(model_dir) = config.model_path.parent() {
        candidates.push(model_dir.join("onnxruntime.dll"));
    }

    if let Some(options) = load_model_pack_session_options(config) {
        if let Some(custom_ops_library) = options.custom_ops_library.as_deref() {
            let custom_ops_path =
                resolve_model_companion_path(&config.model_path, custom_ops_library);
            if custom_ops_path.is_file() {
                if let Some(parent) = custom_ops_path.parent() {
                    candidates.push(parent.join("onnxruntime.dll"));
                }
            }
        }
    }

    for install_root in discover_ryzen_ai_install_roots() {
        candidates.push(
            install_root
                .join("onnxruntime")
                .join("bin")
                .join("onnxruntime.dll"),
        );
        candidates.push(install_root.join("deployment").join("onnxruntime.dll"));
    }

    first_existing_path(candidates)
}

#[cfg(feature = "onnx")]
fn configure_ort_dylib_path(config: &ModelConfig) {
    if let Some(path) = std::env::var_os("ORT_DYLIB_PATH") {
        let path = PathBuf::from(path);
        if path.is_file() {
            debug!(
                ort_dylib = %path.display(),
                "ORT_DYLIB_PATH already configured; keeping existing runtime path"
            );
            return;
        }

        debug!(
            ort_dylib = %path.display(),
            "ORT_DYLIB_PATH was set but did not point to a readable file; attempting deterministic discovery"
        );
    }

    if let Some(path) = std::env::var_os("WRAITHRUN_ORT_DYLIB_PATH") {
        let path = PathBuf::from(path);
        if path.is_file() {
            std::env::set_var("ORT_DYLIB_PATH", &path);
            debug!(
                ort_dylib = %path.display(),
                "set ORT_DYLIB_PATH from WRAITHRUN_ORT_DYLIB_PATH override"
            );
            return;
        }

        debug!(
            ort_dylib = %path.display(),
            "WRAITHRUN_ORT_DYLIB_PATH override did not point to a readable file; falling back to discovery"
        );
    }

    // Vitis builds: use full RyzenAI / model-pack discovery.
    #[cfg(feature = "vitis")]
    {
        if let Some(dylib_path) = discover_ort_dylib_path(config) {
            std::env::set_var("ORT_DYLIB_PATH", &dylib_path);
            debug!(
                ort_dylib = %dylib_path.display(),
                "set ORT_DYLIB_PATH for deterministic ONNX Runtime loading"
            );
            return;
        }
    }

    // Generic builds: check model directory for runtime library.
    #[cfg(not(feature = "vitis"))]
    {
        let _ = config;
        let rt_lib_names: &[&str] = if cfg!(windows) {
            &["onnxruntime.dll"]
        } else if cfg!(target_os = "macos") {
            &["libonnxruntime.dylib"]
        } else {
            &["libonnxruntime.so"]
        };

        if let Some(model_dir) = config.model_path.parent() {
            for name in rt_lib_names {
                let candidate = model_dir.join(name);
                if candidate.is_file() {
                    std::env::set_var("ORT_DYLIB_PATH", &candidate);
                    debug!(
                        ort_dylib = %candidate.display(),
                        "set ORT_DYLIB_PATH from model directory"
                    );
                    return;
                }
            }
        }
    }

    debug!("no ONNX Runtime library candidate discovered; using default dynamic loader resolution");
}

#[cfg(feature = "vitis")]
fn load_model_pack_session_options(config: &ModelConfig) -> Option<GenAiSessionOptions> {
    let config_path = config.model_path.parent()?.join("genai_config.json");
    let bytes = fs::read(&config_path).ok()?;
    let parsed: GenAiConfig = serde_json::from_slice(&bytes).ok()?;
    let mut options = parsed.model?.decoder?.session_options?;

    if parsed
        .search
        .as_ref()
        .and_then(|search| search.past_present_share_buffer)
        .unwrap_or(false)
    {
        options
            .config_entries
            .entry("past_present_share_buffer".to_string())
            .or_insert_with(|| "1".to_string());
    }

    debug!(
        path = %config_path.display(),
        has_custom_ops_library = options.custom_ops_library.is_some(),
        has_custom_allocator = options.custom_allocator.is_some(),
        has_external_data_file = options.external_data_file.is_some(),
        config_entry_count = options.config_entries.len(),
        "loaded model-pack session options"
    );

    Some(options)
}

#[cfg(feature = "onnx")]
fn load_genai_search_config(config: &ModelConfig) -> GenAiSearchSection {
    let config_path = match config.model_path.parent() {
        Some(dir) => dir.join("genai_config.json"),
        None => return GenAiSearchSection::default(),
    };
    let bytes = match fs::read(&config_path) {
        Ok(b) => b,
        Err(_) => return GenAiSearchSection::default(),
    };
    let parsed: GenAiConfig = match serde_json::from_slice(&bytes) {
        Ok(c) => c,
        Err(_) => return GenAiSearchSection::default(),
    };
    parsed.search.unwrap_or_default()
}

#[cfg(feature = "vitis")]
fn extract_ryzenai_provider_options(options: &GenAiSessionOptions) -> HashMap<String, String> {
    let mut entries = HashMap::new();

    for provider_option in &options.provider_options {
        for (provider_name, provider_entries) in provider_option {
            let normalized_provider = provider_name.to_ascii_lowercase().replace('_', "");
            if normalized_provider != "ryzenai" {
                continue;
            }

            for (key, value) in provider_entries {
                entries.insert(key.clone(), value.clone());
            }
        }
    }

    entries
}

#[cfg(feature = "vitis")]
fn resolve_model_pack_external_data_file(options: &GenAiSessionOptions) -> Option<String> {
    if let Some(value) = options
        .external_data_file
        .as_ref()
        .filter(|value| !value.trim().is_empty())
    {
        return Some(value.clone());
    }

    extract_ryzenai_provider_options(options)
        .remove("external_data_file")
        .filter(|value| !value.trim().is_empty())
}

#[cfg(feature = "vitis")]
fn resolve_model_pack_custom_allocator(options: &GenAiSessionOptions) -> Option<String> {
    if let Some(value) = options
        .custom_allocator
        .as_ref()
        .filter(|value| !value.trim().is_empty())
    {
        return Some(value.clone());
    }

    if let Some(value) = extract_ryzenai_provider_options(options)
        .remove("custom_allocator")
        .filter(|value| !value.trim().is_empty())
    {
        return Some(value);
    }

    if !options.provider_options.is_empty() {
        return Some("shared_d3d_xrt".to_string());
    }

    None
}

#[cfg(feature = "vitis")]
fn resolve_model_pack_config_entries(options: &GenAiSessionOptions) -> HashMap<String, String> {
    let mut entries = options.config_entries.clone();

    for (key, value) in extract_ryzenai_provider_options(options) {
        if key.eq_ignore_ascii_case("external_data_file") {
            continue;
        }

        entries.entry(key).or_insert(value);
    }

    if !options.provider_options.is_empty() {
        if !entries.contains_key("max_length_for_kv_cache") {
            if let Some(max_seq_len) = entries.get("hybrid_opt_max_seq_length").cloned() {
                entries.insert("max_length_for_kv_cache".to_string(), max_seq_len);
            }
        }

        entries
            .entry("hybrid_opt_token_backend".to_string())
            .or_insert_with(|| "npu".to_string());
        entries
            .entry("hybrid_opt_gpu_jit".to_string())
            .or_insert_with(|| "5".to_string());
    }

    entries
}

#[cfg(feature = "vitis")]
fn discover_default_custom_ops_library_paths(model_path: &Path) -> Vec<PathBuf> {
    let library_names = ["onnx_custom_ops.dll", "onnxruntime_vitis_ai_custom_ops.dll"];
    let mut candidates = Vec::new();

    for env_name in ["ORT_DYLIB_PATH", "WRAITHRUN_ORT_DYLIB_PATH"] {
        let Some(env_path) = std::env::var_os(env_name) else {
            continue;
        };

        let runtime_path = PathBuf::from(env_path);
        if !runtime_path.is_file() {
            continue;
        }

        if let Some(runtime_dir) = runtime_path.parent() {
            let runtime_is_bin = runtime_dir
                .file_name()
                .map(|name| name.to_string_lossy().eq_ignore_ascii_case("bin"))
                .unwrap_or(false);
            let runtime_parent = if runtime_is_bin {
                runtime_dir.parent()
            } else {
                None
            };
            let runtime_grand_parent = runtime_parent.and_then(Path::parent);

            for library_name in library_names {
                candidates.push(runtime_dir.join(library_name));

                if let Some(parent_dir) = runtime_parent {
                    candidates.push(parent_dir.join("deployment").join(library_name));
                }

                if let Some(grand_parent_dir) = runtime_grand_parent {
                    candidates.push(grand_parent_dir.join("deployment").join(library_name));
                }
            }
        }
    }

    for install_root in discover_ryzen_ai_install_roots() {
        for library_name in library_names {
            candidates.push(install_root.join("deployment").join(library_name));
            candidates.push(
                install_root
                    .join("onnxruntime")
                    .join("bin")
                    .join(library_name),
            );
        }
    }

    if let Some(model_dir) = model_path.parent() {
        for library_name in library_names {
            candidates.push(model_dir.join(library_name));
        }
    }

    let mut seen = HashSet::new();
    let mut resolved = Vec::new();
    for candidate in candidates {
        if !seen.insert(candidate.clone()) {
            continue;
        }

        if candidate.is_file() {
            resolved.push(candidate);
        }
    }

    resolved
}

#[cfg(feature = "vitis")]
fn apply_config_entry_or_recover(
    builder: SessionBuilder,
    key: &str,
    value: &str,
) -> SessionBuilder {
    match builder.with_config_entry(key, value) {
        Ok(next) => next,
        Err(err) => {
            debug!(
                key,
                value,
                error = %err,
                "session config entry was rejected; continuing without it"
            );
            err.recover()
        }
    }
}

#[cfg(feature = "vitis")]
fn apply_external_initializer_file_or_recover(
    builder: SessionBuilder,
    file_name: &Path,
    buffer: Vec<u8>,
) -> SessionBuilder {
    match builder.with_external_initializer_file_in_memory(file_name, Cow::Owned(buffer)) {
        Ok(next) => next,
        Err(err) => {
            debug!(
                external_data_file = %file_name.display(),
                error = %err,
                "external initializer file registration was rejected; continuing"
            );
            err.recover()
        }
    }
}

#[cfg(feature = "vitis")]
fn apply_model_pack_external_initializer(
    mut builder: SessionBuilder,
    declared_external_data_file: &str,
    resolved_external_data_file: &Path,
) -> SessionBuilder {
    let external_bytes = match fs::read(resolved_external_data_file) {
        Ok(bytes) => bytes,
        Err(err) => {
            debug!(
                external_data_file = %resolved_external_data_file.display(),
                error = %err,
                "failed reading external initializer file; continuing"
            );
            return builder;
        }
    };

    let mut aliases = Vec::new();
    aliases.push(PathBuf::from(declared_external_data_file));
    aliases.push(resolved_external_data_file.to_path_buf());

    if let Some(file_name) = resolved_external_data_file
        .file_name()
        .and_then(|name| name.to_str())
    {
        aliases.push(PathBuf::from(file_name));
        aliases.push(PathBuf::from(format!("./{file_name}")));
        aliases.push(PathBuf::from(format!(".\\{file_name}")));
        aliases.push(PathBuf::from(format!("_ORT_MEM_ADDR_/{file_name}")));
        aliases.push(PathBuf::from(format!("_ORT_MEM_ADDR_\\{file_name}")));
        aliases.push(PathBuf::from(format!("/_ORT_MEM_ADDR_/{file_name}")));
        aliases.push(PathBuf::from(format!("\\_ORT_MEM_ADDR_\\{file_name}")));

        if let Some(parent) = resolved_external_data_file.parent() {
            aliases.push(parent.join("_ORT_MEM_ADDR_").join(file_name));
        }
    }

    let mut seen = HashSet::new();
    for alias in aliases {
        if !seen.insert(alias.clone()) {
            continue;
        }

        let debug_alias = alias.display().to_string();
        builder =
            apply_external_initializer_file_or_recover(builder, &alias, external_bytes.clone());
        debug!(
            external_data_file_alias = debug_alias,
            source = %resolved_external_data_file.display(),
            "attempted external initializer registration alias"
        );
    }

    builder
}

#[cfg(feature = "vitis")]
fn discover_model_external_data_companion(model_path: &Path) -> Option<(String, PathBuf)> {
    let model_file_name = model_path.file_name()?.to_str()?;
    let companion_name = format!("{model_file_name}.data");
    let companion_path = model_path.with_file_name(&companion_name);

    if !companion_path.is_file() {
        return None;
    }

    // ORT external initializer registration marshals bytes through protobuf APIs
    // that reject payloads at or above 2 GiB.
    let max_protobuf_bytes = (i32::MAX - 1) as u64;
    let companion_size = fs::metadata(&companion_path).ok()?.len();
    if companion_size >= max_protobuf_bytes {
        return None;
    }

    Some((companion_name, companion_path))
}

#[cfg(feature = "vitis")]
fn model_external_data_companion_size(model_path: &Path) -> Option<u64> {
    let model_file_name = model_path.file_name()?.to_str()?;
    let companion_name = format!("{model_file_name}.data");
    let companion_path = model_path.with_file_name(companion_name);
    let metadata = fs::metadata(companion_path).ok()?;
    Some(metadata.len())
}

#[cfg(feature = "vitis")]
fn has_oversized_model_external_data_companion(model_path: &Path) -> bool {
    let max_protobuf_bytes = (i32::MAX - 1) as u64;
    model_external_data_companion_size(model_path)
        .map(|size| size >= max_protobuf_bytes)
        .unwrap_or(false)
}

#[cfg(feature = "vitis")]
fn should_prefer_disk_external_data_resolution(resolved_external_data_file: &Path) -> bool {
    if !is_pb_external_manifest_file(resolved_external_data_file) {
        return false;
    }

    resolved_external_data_file
        .parent()
        .map(|parent| parent.join("_ORT_MEM_ADDR_").is_dir())
        .unwrap_or(false)
}

#[cfg(feature = "vitis")]
fn is_pb_external_manifest_file(path: &Path) -> bool {
    let Some(file_name) = path
        .file_name()
        .and_then(|name| name.to_str())
    else {
        return false;
    };

    file_name.to_ascii_lowercase().ends_with(".pb.bin")
}

#[cfg(feature = "vitis")]
fn external_initializer_unresolved_hint(config: &ModelConfig) -> Option<String> {
    let options = load_model_pack_session_options(config)?;
    let external_data_file = options.external_data_file?;
    let resolved_external = resolve_model_companion_path(&config.model_path, &external_data_file);

    if !resolved_external.is_file() {
        return None;
    }

    if !is_pb_external_manifest_file(&resolved_external) {
        return None;
    }

    let ort_mem_addr_dir = resolved_external.parent()?.join("_ORT_MEM_ADDR_");
    let ort_mem_addr_state = if ort_mem_addr_dir.is_dir() {
        "present"
    } else {
        "missing"
    };

    Some(format!(
        "Model pack external data appears to be a pb manifest ('{external_data_file}'). The expected nested '_ORT_MEM_ADDR_' payload location '{}' is {ort_mem_addr_state}. This failure pattern commonly indicates unresolved external initializer mapping for model tensors.",
        ort_mem_addr_dir.display(),
    ))
}

#[cfg(feature = "vitis")]
#[derive(Debug, Clone, Copy)]
struct ModelPackSessionOptionPolicy {
    apply_custom_allocator: bool,
    apply_external_data: bool,
}

#[cfg(feature = "vitis")]
fn apply_model_pack_session_options(
    mut builder: SessionBuilder,
    config: &ModelConfig,
    policy: ModelPackSessionOptionPolicy,
) -> SessionBuilder {
    let Some(options) = load_model_pack_session_options(config) else {
        return builder;
    };

    if let Some(log_id) = options.log_id.as_deref() {
        builder = match builder.with_log_id(log_id) {
            Ok(next) => next,
            Err(err) => {
                debug!(
                    log_id,
                    error = %err,
                    "unable to apply model-pack log_id; continuing"
                );
                err.recover()
            }
        };
    }

    if policy.apply_custom_allocator {
        if let Some(custom_allocator) = resolve_model_pack_custom_allocator(&options).as_deref() {
            builder = apply_config_entry_or_recover(builder, "custom_allocator", custom_allocator);
        }
    }

    let external_data_file = resolve_model_pack_external_data_file(&options);
    let mut resolved_external_data_for_config: Option<PathBuf> = None;

    if policy.apply_external_data {
        if let Some(external_data_file) = external_data_file.as_deref() {
            let resolved_external =
                resolve_model_companion_path(&config.model_path, external_data_file);
            resolved_external_data_for_config = Some(resolved_external.clone());
            let model_companion = discover_model_external_data_companion(&config.model_path);
            let resolved_external_exists = resolved_external.is_file();
            let prefer_disk_resolution = resolved_external_exists
                && should_prefer_disk_external_data_resolution(&resolved_external);

            // Preserve the model-pack configured path so ORT can resolve relative
            // in-model location keys as authored by the pack.
            let configured_value = if prefer_disk_resolution {
                resolved_external.display().to_string()
            } else if resolved_external_exists {
                external_data_file.to_string()
            } else if let Some((companion_name, _)) = model_companion.as_ref() {
                companion_name.clone()
            } else {
                external_data_file.to_string()
            };
            builder = apply_config_entry_or_recover(
                builder,
                "external_data_file",
                configured_value.as_str(),
            );

            if resolved_external_exists {
                if prefer_disk_resolution {
                    debug!(
                        external_data_file = %resolved_external.display(),
                        "using disk-based external data resolution for pb manifest pack"
                    );
                } else {
                    builder = apply_model_pack_external_initializer(
                        builder,
                        external_data_file,
                        &resolved_external,
                    );
                }
            } else {
                debug!(
                    external_data_file = %resolved_external.display(),
                    "resolved external data file is missing; skipping external initializer registration"
                );
            }

            if let Some((companion_name, companion_path)) = model_companion {
                if !resolved_external_exists || companion_path != resolved_external {
                    builder = apply_model_pack_external_initializer(
                        builder,
                        companion_name.as_str(),
                        &companion_path,
                    );
                    debug!(
                        external_data_file = %companion_path.display(),
                        "registered model companion external data file"
                    );
                }
            }
        }
    }

    let mut merged_config_entries = resolve_model_pack_config_entries(&options);
    if let Some(external_data_file) = external_data_file.as_deref() {
        let external_data_value = if let Some(resolved_external) =
            resolved_external_data_for_config.as_ref()
        {
            if resolved_external.is_file() {
                resolved_external.to_string_lossy().to_string()
            } else {
                external_data_file.to_string()
            }
        } else {
            let resolved_external =
                resolve_model_companion_path(&config.model_path, external_data_file);
            if resolved_external.is_file() {
                resolved_external.to_string_lossy().to_string()
            } else {
                external_data_file.to_string()
            }
        };

        merged_config_entries
            .entry("external_data_file".to_string())
            .or_insert(external_data_value);
    }

    let mut config_entries: Vec<(String, String)> = merged_config_entries.into_iter().collect();
    config_entries.sort_by(|left, right| left.0.cmp(&right.0));
    for (key, value) in config_entries {
        builder = apply_config_entry_or_recover(builder, key.as_str(), value.as_str());
    }

    if let Some(custom_ops_library) = options.custom_ops_library.as_deref() {
        if let Some(path) = discover_custom_ops_library_path(&config.model_path, custom_ops_library)
        {
            builder = match builder.with_operator_library(&path) {
                Ok(next) => {
                    debug!(
                        custom_ops_library = %path.display(),
                        "loaded model-pack custom ops library"
                    );
                    next
                }
                Err(err) => {
                    warn!(
                        custom_ops_library = %path.display(),
                        error = %err,
                        "unable to load model-pack custom ops library; continuing"
                    );
                    err.recover()
                }
            };
        } else {
            warn!(
                requested_custom_ops_library = custom_ops_library,
                "model-pack custom ops library was not found; continuing"
            );
        }
    } else {
        let mut loaded_default_custom_ops = false;
        for path in discover_default_custom_ops_library_paths(&config.model_path) {
            if loaded_default_custom_ops {
                break;
            }

            builder = match builder.with_operator_library(&path) {
                Ok(next) => {
                    loaded_default_custom_ops = true;
                    eprintln!(
                        "wraithrun: loaded default custom ops library: {}",
                        path.display()
                    );
                    debug!(
                        custom_ops_library = %path.display(),
                        "loaded default RyzenAI custom ops library"
                    );
                    next
                }
                Err(err) => {
                    eprintln!(
                        "wraithrun: failed to load default custom ops library {}: {}",
                        path.display(),
                        err
                    );
                    warn!(
                        custom_ops_library = %path.display(),
                        error = %err,
                        "unable to load default RyzenAI custom ops library; continuing"
                    );
                    err.recover()
                }
            };
        }
    }

    builder
}

#[cfg(feature = "vitis")]
fn build_base_session_builder(config: &ModelConfig) -> Result<SessionBuilder> {
    build_base_session_builder_with_provider(config, true)
}

#[cfg(feature = "onnx")]
fn env_var_truthy(name: &str) -> bool {
    std::env::var(name)
        .map(|value| {
            matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            )
        })
        .unwrap_or(false)
}

#[cfg(feature = "vitis")]
fn build_base_session_builder_with_provider(
    config: &ModelConfig,
    use_vitis_provider: bool,
) -> Result<SessionBuilder> {
    let mut vitis = ep::Vitis::default();

    if use_vitis_provider {
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
    }

    debug!("creating ONNX Runtime session builder");
    let builder = ort_result(Session::builder())?;

    debug!("applying graph optimization level");
    let builder = match builder.with_optimization_level(GraphOptimizationLevel::Level3) {
        Ok(next) => next,
        Err(err) => {
            debug!(
                error = %err,
                "graph optimization level was rejected by runtime; continuing with default"
            );
            err.recover()
        }
    };

    if use_vitis_provider {
        debug!("registering Vitis execution provider");
        let builder = ort_result(builder.with_execution_providers([vitis.build()]))?;

        if env_var_truthy("WRAITHRUN_DISABLE_CPU_FALLBACK") {
            debug!("disabling CPU fallback for session");
            return ort_result(builder.with_disable_cpu_fallback());
        }

        debug!("leaving CPU fallback enabled for hybrid Vitis execution");
        return Ok(builder);
    }

    debug!("using default CPU execution provider for session");
    Ok(builder)
}

#[cfg(feature = "vitis")]
fn build_session_with_vitis_cascade(config: &ModelConfig) -> Result<Session> {
    let session_started = Instant::now();
    let force_cpu_provider = env_var_truthy("WRAITHRUN_FORCE_CPU_EP");
    debug!(
        model = %config.model_path.display(),
        has_vitis_config = config.vitis_config.is_some(),
        force_cpu_provider,
        "building Vitis ONNX Runtime session"
    );

    if force_cpu_provider {
        let mut builder = build_base_session_builder_with_provider(config, false)?;

        debug!(
            model = %config.model_path.display(),
            "committing ONNX Runtime session from model file with CPU provider"
        );

        match builder.commit_from_file(&config.model_path) {
            Ok(session) => {
                debug!(
                    model = %config.model_path.display(),
                    elapsed_ms = session_started.elapsed().as_millis(),
                    "CPU ONNX Runtime session initialized"
                );
                return Ok(session);
            }
            Err(err) => {
                bail!("session initialization failed in CPU mode: {err}");
            }
        }
    }

    let mut policies: Vec<(&str, ModelPackSessionOptionPolicy)> = Vec::new();
    let full_policy = ModelPackSessionOptionPolicy {
        apply_custom_allocator: true,
        apply_external_data: true,
    };
    let no_external_data_policy = ModelPackSessionOptionPolicy {
        apply_custom_allocator: true,
        apply_external_data: false,
    };
    let minimal_policy = ModelPackSessionOptionPolicy {
        apply_custom_allocator: false,
        apply_external_data: false,
    };

    if has_oversized_model_external_data_companion(&config.model_path) {
        policies.push(("no-external-data", no_external_data_policy));
        policies.push(("minimal", minimal_policy));
    } else {
        policies.push(("full", full_policy));
        policies.push(("no-external-data", no_external_data_policy));
        policies.push(("minimal", minimal_policy));
    }

    let mut attempt_errors: Vec<(String, String)> = Vec::new();

    for (index, (label, policy)) in policies.iter().enumerate() {
        debug!(
            attempt = index + 1,
            policy = *label,
            apply_custom_allocator = policy.apply_custom_allocator,
            apply_external_data = policy.apply_external_data,
            "attempting ONNX Runtime session initialization"
        );

        let builder = build_base_session_builder(config)?;
        let mut builder = apply_model_pack_session_options(builder, config, *policy);

        debug!(
            model = %config.model_path.display(),
            policy = *label,
            "committing ONNX Runtime session from model file"
        );

        match builder.commit_from_file(&config.model_path) {
            Ok(session) => {
                debug!(
                    model = %config.model_path.display(),
                    policy = *label,
                    elapsed_ms = session_started.elapsed().as_millis(),
                    "Vitis ONNX Runtime session initialized"
                );
                return Ok(session);
            }
            Err(err) => {
                let err_text = err.to_string();
                let has_next = index + 1 < policies.len();
                let retry = has_next;
                debug!(
                    policy = *label,
                    retry,
                    error = %err_text,
                    "session initialization attempt failed"
                );

                attempt_errors.push(((*label).to_string(), err_text));
                if !retry {
                    break;
                }
            }
        }
    }

    if let Some((best_policy, best_error)) = attempt_errors.iter().max_by_key(|(_, err)| {
        session_init_reason_priority(classify_session_init_reason_code(err.as_str()))
    }) {
        let compact_attempts = attempt_errors
            .iter()
            .map(|(policy, err)| {
                format!(
                    "{policy}: {}",
                    truncate_session_init_error(err.as_str(), 240)
                )
            })
            .collect::<Vec<_>>()
            .join(" | ");

        bail!(
            "session initialization failed (best policy={best_policy}): {best_error}; attempts: {compact_attempts}"
        )
    } else {
        bail!("session initialization failed")
    }
}

#[cfg(feature = "onnx")]
fn build_session(config: &ModelConfig) -> Result<Session> {
    configure_ort_dylib_path(config);

    // When the vitis feature is available, delegate to the Vitis EP cascade
    // unless the user explicitly forces CPU-only mode.
    #[cfg(feature = "vitis")]
    {
        return build_session_with_vitis_cascade(config);
    }

    // Generic CPU-only session (works on Windows, Mac, Linux with any ONNX model).
    #[cfg(not(feature = "vitis"))]
    {
        let session_started = Instant::now();
        debug!(
            model = %config.model_path.display(),
            "building CPU ONNX Runtime session"
        );

        let builder = ort_result(Session::builder())?;
        let mut builder = match builder.with_optimization_level(GraphOptimizationLevel::Level3) {
            Ok(b) => b,
            Err(err) => {
                debug!(
                    error = %err,
                    "graph optimization level was rejected; continuing with default"
                );
                err.recover()
            }
        };

        match builder.commit_from_file(&config.model_path) {
            Ok(session) => {
                debug!(
                    model = %config.model_path.display(),
                    elapsed_ms = session_started.elapsed().as_millis(),
                    "CPU ONNX Runtime session initialized"
                );
                Ok(session)
            }
            Err(err) => {
                bail!("session initialization failed: {err}")
            }
        }
    }
}

#[cfg(feature = "onnx")]
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

#[cfg(feature = "onnx")]
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

#[cfg(feature = "onnx")]
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

#[cfg(feature = "onnx")]
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

#[cfg(feature = "onnx")]
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

#[cfg(feature = "onnx")]
fn discover_stop_token_ids(tokenizer: &Tokenizer) -> HashSet<i64> {
    const COMMON_STOP_TOKENS: &[&str] = &["</s>", "<eos>", "<|end_of_text|>", "<|eot_id|>"];

    COMMON_STOP_TOKENS
        .iter()
        .filter_map(|token| tokenizer.token_to_id(token).map(|id| id as i64))
        .collect()
}

#[cfg(feature = "onnx")]
fn decode_generated(tokenizer: &Tokenizer, generated_ids: &[i64]) -> Result<String> {
    if generated_ids.is_empty() {
        return Ok(String::new());
    }

    let generated_u32 = to_u32_ids(generated_ids)?;
    tokenizer
        .decode(&generated_u32, true)
        .map_err(|err| anyhow!("failed to decode generated token stream: {err}"))
}

#[cfg(feature = "onnx")]
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

#[cfg(feature = "onnx")]
fn infer_cache_past_axis(shape: &[i64]) -> usize {
    if shape.len() >= 2 {
        shape.len() - 2
    } else {
        0
    }
}

#[cfg(feature = "onnx")]
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

#[cfg(feature = "onnx")]
fn is_supported_sequence_element_type(element_type: TensorElementType) -> bool {
    matches!(
        element_type,
        TensorElementType::Int64 | TensorElementType::Int32
    )
}

#[cfg(feature = "onnx")]
fn is_supported_use_cache_element_type(element_type: TensorElementType) -> bool {
    matches!(
        element_type,
        TensorElementType::Bool | TensorElementType::Int64 | TensorElementType::Int32
    )
}

#[cfg(feature = "onnx")]
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

#[cfg(feature = "onnx")]
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

#[cfg(feature = "onnx")]
fn materialize_cache_shape(template: &[i64], past_axis: usize, past_len: usize) -> Vec<i64> {
    // Some runtime/provider combinations reject zero-length tensor dimensions
    // even when the cache branch is disabled on the first prompt step.
    let effective_past_len = if env_var_truthy("WRAITHRUN_ALLOW_ZERO_CACHE_DIMS") {
        past_len
    } else {
        past_len.max(1)
    };

    let mut shape = if template.is_empty() {
        vec![effective_past_len as i64]
    } else {
        template.to_vec()
    };

    for (idx, dim) in shape.iter_mut().enumerate() {
        if *dim < 0 {
            *dim = if idx == past_axis {
                effective_past_len as i64
            } else {
                1
            };
        }
        if idx != past_axis && *dim == 0 {
            *dim = 1;
        }
    }

    if past_axis < shape.len() && shape[past_axis] <= 0 {
        shape[past_axis] = effective_past_len as i64;
    }

    shape
}

#[cfg(feature = "onnx")]
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

#[cfg(feature = "onnx")]
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

#[cfg(feature = "onnx")]
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

#[cfg(feature = "onnx")]
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

#[cfg(feature = "onnx")]
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

#[cfg(feature = "onnx")]
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

/// Pre-allocate KV-cache buffers sized for `max_seq_len` and bind them as
/// both inputs and outputs via IO binding.  GQO (Grouped Query Optimized)
/// nodes in RyzenAI hybrid models require the past-KV input and present-KV
/// output to share the same underlying ORT value ("shared buffer" mode).
///
/// Safety: we call the raw `ort_sys::BindOutput` API so that the *same*
/// `OrtValue*` is bound on both the input and output sides.  The `DynValue`
/// objects returned in the Vec keep the ORT values alive for the entire
/// inference loop; they must NOT be dropped before the IoBinding is done.
#[cfg(feature = "onnx")]
fn bind_shared_cache_buffers(
    binding: &mut IoBinding,
    layout: &SessionLayout,
    max_seq_len: usize,
) -> Result<Vec<DynValue>> {
    let mut owned_buffers: Vec<DynValue> = Vec::with_capacity(layout.cache_specs.len());

    // We need a CPU MemoryInfo for registering output names via
    // bind_output_to_device (this only registers the name; we'll
    // immediately override the binding with the shared buffer pointer).
    let cpu_mem_info = ort_result(ort::memory::MemoryInfo::new(
        ort::memory::AllocationDevice::CPU,
        0,
        ort::memory::AllocatorType::Device,
        ort::memory::MemoryType::CPUOutput,
    ))?;

    for spec in &layout.cache_specs {
        let cache_shape =
            materialize_cache_shape(&spec.input.shape, spec.past_axis, max_seq_len);
        let buffer = build_zero_tensor(cache_shape, spec.input.element_type)?;

        // Bind as input (borrows the value).
        ort_result(binding.bind_input(&spec.input.name, &buffer))?;

        // First register the output name in IoBinding's internal tracking
        // so that run_binding can reconcile names with output values.
        ort_result(binding.bind_output_to_device(&spec.output_name, &cpu_mem_info))?;

        // Override the output binding with the *same* OrtValue pointer as the
        // input, satisfying the GQO shared-buffer check.
        let c_name = CString::new(spec.output_name.as_str())
            .map_err(|e| anyhow!("invalid cache output name: {e}"))?;
        let status = unsafe {
            (ort::api().BindOutput)(
                binding.ptr_mut(),
                c_name.as_ptr(),
                buffer.ptr(),
            )
        };
        unsafe {
            ort::error::Error::result_from_status(status)
                .map_err(|e| anyhow!("BindOutput for '{}' failed: {e}", spec.output_name))?;
        }

        owned_buffers.push(buffer);
    }

    Ok(owned_buffers)
}

/// Re-bind non-cache inputs (input_ids, attention_mask, position_ids, etc.)
/// on an existing IoBinding.  Cache bindings are left untouched.
#[cfg(feature = "onnx")]
fn rebind_step_inputs(
    binding: &mut IoBinding,
    layout: &SessionLayout,
    step_input_ids: &[i64],
    attention_len: usize,
    use_cache_branch: bool,
) -> Result<()> {
    // input_ids
    let input_shape = materialize_sequence_shape(&layout.input_ids.shape, step_input_ids.len());
    let input_tensor =
        build_int_tensor(input_shape, step_input_ids, layout.input_ids.element_type)?;
    ort_result(binding.bind_input(&layout.input_ids.name, &input_tensor))?;

    // attention_mask
    if let Some(spec) = layout.attention_mask.as_ref() {
        let mask_values = vec![1_i64; attention_len.max(1)];
        let mask_shape = materialize_sequence_shape(&spec.shape, mask_values.len());
        let mask_tensor = build_int_tensor(mask_shape, &mask_values, spec.element_type)?;
        ort_result(binding.bind_input(&spec.name, &mask_tensor))?;
    }

    // position_ids
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
        ort_result(binding.bind_input(&spec.name, &position_tensor))?;
    }

    // token_type_ids
    if let Some(spec) = layout.token_type_ids.as_ref() {
        let token_type_values = vec![0_i64; step_input_ids.len().max(1)];
        let token_type_shape = materialize_sequence_shape(&spec.shape, token_type_values.len());
        let token_type_tensor =
            build_int_tensor(token_type_shape, &token_type_values, spec.element_type)?;
        ort_result(binding.bind_input(&spec.name, &token_type_tensor))?;
    }

    // cache_position
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
        ort_result(binding.bind_input(&spec.name, &cache_position_tensor))?;
    }

    // use_cache branch flag
    if let Some(spec) = layout.use_cache.as_ref() {
        let use_cache_tensor = build_use_cache_tensor(spec, use_cache_branch)?;
        ort_result(binding.bind_input(&spec.name, &use_cache_tensor))?;
    }

    Ok(())
}

/// Bind the logits output to device so ORT allocates the output buffer.
#[cfg(feature = "onnx")]
fn bind_logits_output(binding: &mut IoBinding, layout: &SessionLayout) -> Result<()> {
    let mem_info = ort_result(ort::memory::MemoryInfo::new(
        ort::memory::AllocationDevice::CPU,
        0,
        ort::memory::AllocatorType::Device,
        ort::memory::MemoryType::CPUOutput,
    ))?;
    ort_result(binding.bind_output_to_device(&layout.logits_output_name, &mem_info))?;
    Ok(())
}

#[cfg(feature = "onnx")]
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

    let include_cache_inputs = (use_cache_branch || layout.use_cache.is_none())
        && !cache_state.is_empty();
    if include_cache_inputs {
        for spec in &layout.cache_specs {
            let Some(value) = cache_state.get(&spec.input.name) else {
                bail!("cache state is missing tensor for '{}'.", spec.input.name);
            };
            model_inputs.push((Cow::Owned(spec.input.name.clone()), value.view().into()));
        }
    }

    Ok(model_inputs)
}

#[cfg(feature = "onnx")]
fn select_next_token(logits_value: &DynValue) -> Result<i64> {
    if let Ok((shape, logits)) = ort_result(logits_value.try_extract_tensor::<f32>()) {
        return select_next_token_from_f32(shape, logits);
    }

    if let Ok((shape, logits)) = ort_result(logits_value.try_extract_tensor::<f16>()) {
        return select_next_token_from_f16(shape, logits);
    }

    bail!("logits tensor is neither f32 nor f16");
}

#[cfg(feature = "onnx")]
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

#[cfg(feature = "onnx")]
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

#[cfg(feature = "onnx")]
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

#[cfg(feature = "onnx")]
pub fn inspect_runtime_compatibility(
    config: &ModelConfig,
    run_smoke_check: bool,
) -> RuntimeCompatibilityReport {
    let mut report = RuntimeCompatibilityReport::default();

    let mut session = match build_session(config) {
        Ok(session) => session,
        Err(err) => {
            let reason_code = classify_session_init_reason_code(&err.to_string());
            let detail = format!("Unable to initialize ONNX session: {err}");

            #[cfg(feature = "vitis")]
            let (reason_code, detail) = {
                let mut reason_code = reason_code;
                let mut detail = detail;
                if reason_code == "runtime_ep_assignment_failed" {
                    if let Some(hint) = external_initializer_unresolved_hint(config) {
                        reason_code = "runtime_external_initializer_unresolved";
                        detail = format!("{detail} {hint}");
                    }
                }
                (reason_code, detail)
            };

            report.push_fail(
                reason_code,
                detail,
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

#[cfg(not(feature = "onnx"))]
pub fn inspect_runtime_compatibility(
    _config: &ModelConfig,
    _run_smoke_check: bool,
) -> RuntimeCompatibilityReport {
    let mut report = RuntimeCompatibilityReport::default();
    report.push_warn(
        "onnx_feature_disabled",
        "Runtime compatibility checks were skipped because ONNX inference support is disabled in this build. Rebuild with '--features inference_bridge/onnx' or '--features inference_bridge/vitis'.",
    );
    report
}

#[cfg(feature = "onnx")]
fn run_prompt_shared_buffer(
    session: &mut Session,
    layout: &SessionLayout,
    tokenizer: &Tokenizer,
    context_ids: &[i64],
    max_new_tokens: usize,
    max_seq_len: usize,
    run_started: &Instant,
) -> Result<Vec<i64>> {
    debug!(
        max_seq_len,
        "using shared-buffer IO binding for KV cache"
    );

    let stop_ids = discover_stop_token_ids(tokenizer);
    let mut generated_ids: Vec<i64> = Vec::new();

    let mut binding = ort_result(session.create_binding())?;

    // Pre-allocate shared cache buffers (the same OrtValue is bound as both
    // input and output for each cache layer).
    let _cache_buffers = bind_shared_cache_buffers(&mut binding, layout, max_seq_len)?;

    // Bind logits output to CPU device; ORT will allocate the buffer.
    bind_logits_output(&mut binding, layout)?;

    // Also bind any non-logits, non-cache outputs so ORT doesn't choke on
    // missing output bindings.  We don't use them.
    // (the cache outputs are already bound by bind_shared_cache_buffers)

    // --- Prompt ingestion: feed one token at a time ---
    for (prompt_index, prompt_token) in context_ids.iter().copied().enumerate() {
        let step_started = Instant::now();
        let use_cache = prompt_index > 0;
        let step_input_ids = vec![prompt_token];
        let attention_len = prompt_index + 1;

        debug!(
            step = prompt_index + 1,
            use_cache,
            attention_len,
            "shared-buffer prompt-ingest step"
        );

        rebind_step_inputs(&mut binding, layout, &step_input_ids, attention_len, use_cache)?;

        let outputs = ort_result(session.run_binding(&binding))?;
        debug!(
            step = prompt_index + 1,
            elapsed_ms = step_started.elapsed().as_millis(),
            "shared-buffer prompt-ingest step completed"
        );

        // Verify logits are present (don't need the value yet).
        let _ = outputs
            .get(&layout.logits_output_name)
            .or_else(|| outputs.get("logits"))
            .or_else(|| outputs.get("lm_logits"))
            .ok_or_else(|| anyhow!("logits output missing during prompt ingestion"))?;
    }

    // --- Decode loop ---
    let mut all_ids = context_ids.to_vec();

    for step in 0..max_new_tokens.max(1) {
        let step_started = Instant::now();
        let last_token = *all_ids
            .last()
            .ok_or_else(|| anyhow!("empty context during decode"))?;
        let step_input_ids = vec![last_token];
        let attention_len = all_ids.len().max(1);

        debug!(
            step = step + 1,
            attention_len,
            "shared-buffer decode step"
        );

        rebind_step_inputs(&mut binding, layout, &step_input_ids, attention_len, true)?;

        let outputs = ort_result(session.run_binding(&binding))?;
        debug!(
            step = step + 1,
            elapsed_ms = step_started.elapsed().as_millis(),
            "shared-buffer decode step completed"
        );

        let logits_value = outputs
            .get(&layout.logits_output_name)
            .or_else(|| outputs.get("logits"))
            .or_else(|| outputs.get("lm_logits"))
            .ok_or_else(|| anyhow!("logits output missing during decode"))?;

        let next_token = select_next_token(logits_value)?;

        all_ids.push(next_token);
        generated_ids.push(next_token);

        if stop_ids.contains(&next_token) {
            break;
        }
    }

    debug!(
        generated_token_count = generated_ids.len(),
        elapsed_ms = run_started.elapsed().as_millis(),
        "shared-buffer decode completed"
    );

    Ok(generated_ids)
}

#[cfg(feature = "onnx")]
pub fn run_prompt(config: &ModelConfig, prompt: &str) -> Result<String> {
    let run_started = Instant::now();
    debug!(
        model = %config.model_path.display(),
        prompt_len = prompt.len(),
        max_new_tokens = config.max_new_tokens,
        "starting Vitis prompt run"
    );

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

    // Check if the model requires shared-buffer KV cache (RyzenAI hybrid GQO).
    let search_config = load_genai_search_config(config);
    let use_shared_buffer = search_config.past_present_share_buffer.unwrap_or(false)
        && !layout.cache_specs.is_empty();

    if use_shared_buffer {
        let context_ids = encode_prompt(&tokenizer, prompt)?;
        if context_ids.is_empty() {
            bail!("prompt encoding produced no token IDs");
        }
        let max_seq_len = search_config.max_length.unwrap_or(2048);
        let generated_ids = run_prompt_shared_buffer(
            &mut session,
            &layout,
            &tokenizer,
            &context_ids,
            config.max_new_tokens,
            max_seq_len,
            &run_started,
        )?;
        let mut generated_text = decode_generated(&tokenizer, &generated_ids)?;
        if generated_text.trim().is_empty() {
            generated_text = "(model produced no decodable continuation)".to_string();
        }
        debug!(
            generated_token_count = generated_ids.len(),
            generated_text_len = generated_text.len(),
            elapsed_ms = run_started.elapsed().as_millis(),
            "completed Vitis prompt run (shared-buffer)"
        );
        return Ok(format!("<final>{generated_text}</final>"));
    }

    let stop_ids = discover_stop_token_ids(&tokenizer);
    let mut context_ids = encode_prompt(&tokenizer, prompt)?;
    let mut generated_ids: Vec<i64> = Vec::new();

    debug!(
        prompt_token_count = context_ids.len(),
        stop_token_count = stop_ids.len(),
        elapsed_ms = run_started.elapsed().as_millis(),
        "prompt encoded and runtime layout ready"
    );

    let cache_disabled_for_live = env_var_truthy("WRAITHRUN_DISABLE_LIVE_KV_CACHE");
    let cache_enabled = !layout.cache_specs.is_empty() && !cache_disabled_for_live;
    // Always initialize cache tensors when the model has cache specs (GQO nodes
    // require them), even if we won't carry forward state between steps.
    let has_cache_specs = !layout.cache_specs.is_empty();
    let mut cache_state = if has_cache_specs {
        initialize_cache_state(&layout, 0)?
    } else {
        HashMap::new()
    };

    if context_ids.is_empty() {
        bail!("prompt encoding produced no token IDs");
    }

    if cache_enabled {
        for (prompt_index, prompt_token) in context_ids.iter().copied().enumerate() {
            let step_started = Instant::now();
            let decode_with_cache = prompt_index > 0;
            let step_input_ids = vec![prompt_token];
            let attention_len = prompt_index + 1;

            debug!(
                step = prompt_index + 1,
                decode_with_cache,
                step_input_len = step_input_ids.len(),
                attention_len,
                "running Vitis prompt-ingest forward pass"
            );

            let model_inputs = build_model_inputs(
                &layout,
                &step_input_ids,
                attention_len,
                decode_with_cache,
                &cache_state,
            )?;

            let mut outputs = ort_result(session.run(model_inputs))?;
            debug!(
                step = prompt_index + 1,
                elapsed_ms = step_started.elapsed().as_millis(),
                "Vitis prompt-ingest forward pass completed"
            );

            let _ = outputs
                .get(&layout.logits_output_name)
                .or_else(|| outputs.get("logits"))
                .or_else(|| outputs.get("lm_logits"))
                .unwrap_or(&outputs[0]);

            let mut next_cache = HashMap::new();
            for spec in &layout.cache_specs {
                let cache_value = outputs.remove(&spec.output_name).ok_or_else(|| {
                    anyhow!("cache output '{}' missing while ingesting prompt", spec.output_name)
                })?;
                next_cache.insert(spec.input.name.clone(), cache_value);
            }
            cache_state = next_cache;
        }
    }

    for step in 0..config.max_new_tokens.max(1) {
        let step_started = Instant::now();
        let (decode_with_cache, step_input_ids, attention_len) = if cache_enabled {
            (
                true,
                vec![*context_ids
                    .last()
                    .ok_or_else(|| anyhow!("empty context ids"))?],
                context_ids.len().max(1),
            )
        } else {
            let step_input_ids = context_ids.clone();
            let attention_len = step_input_ids.len().max(1);
            (false, step_input_ids, attention_len)
        };

        debug!(
            step = step + 1,
            decode_with_cache,
            step_input_len = step_input_ids.len(),
            attention_len,
            "running Vitis forward pass"
        );

        let model_inputs = build_model_inputs(
            &layout,
            &step_input_ids,
            attention_len,
            decode_with_cache,
            &cache_state,
        )?;

        let mut outputs = ort_result(session.run(model_inputs))?;
        debug!(
            step = step + 1,
            elapsed_ms = step_started.elapsed().as_millis(),
            "Vitis forward pass completed"
        );
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

    debug!(
        generated_token_count = generated_ids.len(),
        generated_text_len = generated_text.len(),
        elapsed_ms = run_started.elapsed().as_millis(),
        "completed Vitis prompt run"
    );

    Ok(format!("<final>{generated_text}</final>"))
}

#[cfg(not(feature = "onnx"))]
pub fn run_prompt(_config: &ModelConfig, _prompt: &str) -> Result<String> {
    bail!("ONNX inference is disabled. Rebuild with '--features inference_bridge/onnx' or '--features inference_bridge/vitis'.")
}

#[cfg(test)]
mod tests {
    #[cfg(not(feature = "onnx"))]
    use std::path::PathBuf;

    #[cfg(not(feature = "onnx"))]
    use crate::ModelConfig;

    use super::{cache_slot_key, is_cache_tensor_name, resolve_cache_output_name};

    #[cfg(not(feature = "onnx"))]
    use super::{inspect_runtime_compatibility, RuntimeCompatibilitySeverity};

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

    #[cfg(not(feature = "onnx"))]
    #[test]
    fn runtime_compatibility_reports_feature_disabled_warning_without_onnx() {
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
                && issue.reason_code == "onnx_feature_disabled"
        }));
    }
}
