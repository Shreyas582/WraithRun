fn run_live_setup(cli: &Cli) -> Result<String> {
    let mut setup_cli = cli.clone();
    setup_cli.live = true;
    setup_cli.dry_run = false;

    if let Some(config_path) = setup_cli.config.as_ref() {
        if !config_path.is_file() {
            // Allow `live setup --config <new-path>` to bootstrap a brand new config file.
            setup_cli.config = None;
        }
    }

    let task = resolve_task_for_mode(&setup_cli, "live-setup")?;
    let mut runtime = resolve_runtime_config_with_task(&setup_cli, task)?;

    if cli.model.is_none() && !runtime.model.is_file() {
        if let Some(discovered_model) = discover_model_path() {
            runtime.model = discovered_model;
        }
    }

    let tokenizer_missing = runtime
        .tokenizer
        .as_ref()
        .map(|path| !path.is_file())
        .unwrap_or(true);
    if cli.tokenizer.is_none() && tokenizer_missing {
        runtime.tokenizer = discover_tokenizer_path(&runtime.model);
    }

    let mut report = DoctorReport::default();
    run_model_pack_doctor_checks(&runtime, &mut report);
    validate_live_setup_report(&report)?;

    let config_path = cli
        .config
        .clone()
        .unwrap_or_else(|| PathBuf::from(DEFAULT_CONFIG_FILE));
    write_live_setup_profile(&config_path, &runtime)?;

    Ok(render_live_setup_summary(&runtime, &config_path))
}

fn discover_model_path() -> Option<PathBuf> {
    let default_model = PathBuf::from(DEFAULT_MODEL_PATH);
    if default_model.is_file() {
        return Some(default_model);
    }

    let models_dir = PathBuf::from("./models");
    let mut candidates: Vec<PathBuf> = fs::read_dir(models_dir)
        .ok()?
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| {
            path.is_file()
                && path
                    .extension()
                    .and_then(|ext| ext.to_str())
                    .map(|ext| ext.eq_ignore_ascii_case("onnx"))
                    .unwrap_or(false)
        })
        .collect();

    candidates.sort();
    candidates.into_iter().next()
}

fn discover_model_path_near(seed_path: &Path) -> Option<PathBuf> {
    let parent = seed_path.parent()?;
    let mut candidates: Vec<PathBuf> = fs::read_dir(parent)
        .ok()?
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| path.is_file() && is_onnx_path(path))
        .collect();

    candidates.sort();
    candidates.into_iter().next()
}

fn is_onnx_path(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.eq_ignore_ascii_case("onnx"))
        .unwrap_or(false)
}

fn discover_tokenizer_candidates(model_path: &Path) -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    if let Some(parent) = model_path.parent() {
        candidates.push(parent.join("tokenizer.json"));

        if let Ok(entries) = fs::read_dir(parent) {
            for entry in entries.flatten() {
                let path = entry.path();
                if !path.is_file() {
                    continue;
                }

                let is_json = path
                    .extension()
                    .and_then(|ext| ext.to_str())
                    .map(|ext| ext.eq_ignore_ascii_case("json"))
                    .unwrap_or(false);
                if !is_json {
                    continue;
                }

                let has_tokenizer_name = path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .map(|name| name.to_ascii_lowercase().contains("tokenizer"))
                    .unwrap_or(false);

                if has_tokenizer_name {
                    candidates.push(path);
                }
            }
        }
    }

    candidates.push(PathBuf::from("./models/tokenizer.json"));

    let mut unique = Vec::new();
    for candidate in candidates {
        if !unique.iter().any(|existing| existing == &candidate) {
            unique.push(candidate);
        }
    }

    unique
}

fn discover_tokenizer_path(model_path: &Path) -> Option<PathBuf> {
    discover_tokenizer_candidates(model_path)
        .into_iter()
        .find(|path| path.is_file())
}

fn cache_key_from_metastate_name(file_name: &str) -> Option<String> {
    let suffixes = [".state", ".fconst", ".ctrlpkt", ".super"];
    let key = file_name.strip_prefix("dd_metastate_")?;

    suffixes
        .iter()
        .find_map(|suffix| key.strip_suffix(suffix))
        .filter(|value| !value.is_empty())
        .map(|value| value.to_string())
}

fn discover_vitis_metastate_key(model_dir: &Path) -> Option<String> {
    let mut state_keys = Vec::new();
    let mut fallback_keys = Vec::new();

    for entry in fs::read_dir(model_dir).ok()?.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }

        let Some(file_name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };

        let Some(cache_key) = cache_key_from_metastate_name(file_name) else {
            continue;
        };

        if file_name.ends_with(".state") {
            state_keys.push(cache_key);
        } else {
            fallback_keys.push(cache_key);
        }
    }

    state_keys.sort();
    state_keys.dedup();
    if let Some(cache_key) = state_keys.into_iter().next() {
        return Some(cache_key);
    }

    fallback_keys.sort();
    fallback_keys.dedup();
    fallback_keys.into_iter().next()
}

fn discover_vitis_cache_key(model_path: &Path) -> Option<String> {
    let model_dir = model_path.parent()?;

    if let Some(cache_key) = discover_vitis_metastate_key(model_dir) {
        return Some(cache_key);
    }

    let cache_dirs = [model_dir.join(".cache"), model_dir.join("cache")];
    let mut keys = Vec::new();

    for cache_dir in cache_dirs {
        if !cache_dir.is_dir() {
            continue;
        }

        let Ok(entries) = fs::read_dir(cache_dir) else {
            continue;
        };

        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }

            let Some(file_name) = path.file_name().and_then(|name| name.to_str()) else {
                continue;
            };

            if let Some(cache_key) = file_name
                .strip_suffix("_meta.json")
                .filter(|value| !value.is_empty())
            {
                keys.push(cache_key.to_string());
            }
        }
    }

    keys.sort();
    keys.dedup();
    keys.into_iter().next()
}

fn discover_vitis_cache_dir(model_path: &Path) -> Option<String> {
    let model_dir = model_path.parent()?;

    if discover_vitis_metastate_key(model_dir).is_some() {
        return Some(model_dir.display().to_string());
    }

    let candidates = [model_dir.join(".cache"), model_dir.join("cache")];

    candidates
        .into_iter()
        .find(|path| path.is_dir())
        .map(|path| path.display().to_string())
}

fn tokenizer_json_health(path: &Path) -> Result<(), &'static str> {
    let bytes = fs::read(path).map_err(|err| {
        if err.kind() == std::io::ErrorKind::PermissionDenied {
            "tokenizer_permission_denied"
        } else {
            "tokenizer_read_failed"
        }
    })?;

    let parsed: Value = serde_json::from_slice(&bytes).map_err(|_| "tokenizer_json_invalid")?;
    let has_model_key = parsed
        .as_object()
        .map(|obj| obj.contains_key("model"))
        .unwrap_or(false);

    if !has_model_key {
        return Err("tokenizer_model_key_missing");
    }

    Ok(())
}

fn discover_tokenizer_path_with_validation(
    model_path: &Path,
    exclude: Option<&Path>,
) -> Option<PathBuf> {
    discover_tokenizer_candidates(model_path)
        .into_iter()
        .filter(|path| path.is_file())
        .filter(|path| {
            exclude
                .map(|blocked| blocked != path.as_path())
                .unwrap_or(true)
        })
        .find(|path| tokenizer_json_health(path).is_ok())
}

fn validate_live_runtime_preflight(runtime: &RuntimeConfig) -> Result<()> {
    if !runtime.live {
        return Ok(());
    }

    if !runtime.model.is_file() {
        bail!(
            "Live mode model file not found: {}. Run '--doctor --live --introspection-format json' (or '--doctor --live --fix') and provide a readable --model path.",
            runtime.model.display()
        );
    }

    fs::File::open(&runtime.model).with_context(|| {
        format!(
            "Live mode model file is not readable: {}",
            runtime.model.display()
        )
    })?;

    match runtime.tokenizer.as_deref() {
        Some(tokenizer_path) => {
            if !tokenizer_path.is_file() {
                bail!(
                    "Tokenizer file not found: {}. Provide a valid --tokenizer path or run '--doctor --live --fix'.",
                    tokenizer_path.display()
                );
            }

            tokenizer_json_health(tokenizer_path).map_err(|reason_code| {
                anyhow!(
                    "Tokenizer validation failed for '{}': {reason_code}. Provide a readable tokenizer JSON with a top-level model key.",
                    tokenizer_path.display()
                )
            })?;
        }
        None => {
            if discover_tokenizer_path_with_validation(&runtime.model, None).is_none() {
                bail!(
                    "No valid tokenizer JSON resolved for live mode. Provide --tokenizer <PATH> or place tokenizer.json beside the model."
                );
            }
        }
    }

    Ok(())
}

fn validate_live_setup_report(report: &DoctorReport) -> Result<()> {
    let required_passes = [
        "live-model-path",
        "live-model-size",
        "live-tokenizer-path",
        "live-tokenizer-size",
        "live-tokenizer-json",
    ];

    let missing = required_passes
        .iter()
        .copied()
        .filter(|name| {
            !report
                .checks
                .iter()
                .any(|check| check.name == *name && check.status == DoctorStatus::Pass)
        })
        .collect::<Vec<_>>();

    let has_runtime_failure = report.checks.iter().any(|check| {
        check.name == "live-runtime-compatibility" && check.status == DoctorStatus::Fail
    });

    if missing.is_empty() && !has_runtime_failure {
        return Ok(());
    }

    let mut details = String::new();
    for check in report
        .checks
        .iter()
        .filter(|check| check.status != DoctorStatus::Pass)
    {
        let _ = writeln!(
            details,
            "- [{}] {}: {}",
            check.status.label(),
            check.name,
            check.detail
        );
        if let Some(remediation) = check.remediation {
            let _ = writeln!(details, "  Fix: {remediation}");
        }
    }

    bail!(
        "Live setup validation failed. Missing passing checks: {}\n{}Resolve the checks above and rerun 'wraithrun live setup --model <PATH> --tokenizer <PATH>'.",
        if missing.is_empty() { "live-runtime-compatibility".to_string() } else { missing.join(", ") },
        details
    );
}

fn write_live_setup_profile(config_path: &Path, runtime: &RuntimeConfig) -> Result<()> {
    if let Some(parent) = config_path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)
                .with_context(|| format!("Failed creating directory {}", parent.display()))?;
        }
    }

    let mut root = if config_path.is_file() {
        let existing = fs::read_to_string(config_path)
            .with_context(|| format!("Failed reading config {}", config_path.display()))?;
        toml::from_str::<toml::Value>(&existing)
            .with_context(|| format!("Failed parsing config {}", config_path.display()))?
    } else {
        toml::Value::Table(toml::map::Map::new())
    };

    let root_table = root.as_table_mut().ok_or_else(|| {
        anyhow!(
            "Config root must be a TOML table in '{}'",
            config_path.display()
        )
    })?;

    let profiles_entry = root_table
        .entry("profiles".to_string())
        .or_insert_with(|| toml::Value::Table(toml::map::Map::new()));
    let profiles_table = profiles_entry.as_table_mut().ok_or_else(|| {
        anyhow!(
            "Config key 'profiles' must be a TOML table in '{}'",
            config_path.display()
        )
    })?;

    let mut profile = toml::map::Map::new();
    profile.insert("live".to_string(), toml::Value::Boolean(true));
    profile.insert(
        "model".to_string(),
        toml::Value::String(runtime.model.display().to_string()),
    );
    if let Some(tokenizer) = runtime.tokenizer.as_ref() {
        profile.insert(
            "tokenizer".to_string(),
            toml::Value::String(tokenizer.display().to_string()),
        );
    }
    profile.insert(
        "live_fallback_policy".to_string(),
        toml::Value::String("dry-run-on-error".to_string()),
    );
    profile.insert(
        "format".to_string(),
        toml::Value::String("json".to_string()),
    );

    profiles_table.insert(
        LIVE_SETUP_PROFILE_NAME.to_string(),
        toml::Value::Table(profile),
    );

    let rendered = toml::to_string_pretty(&root).context("Failed serializing updated config")?;
    fs::write(config_path, rendered)
        .with_context(|| format!("Failed writing config {}", config_path.display()))?;

    Ok(())
}

fn render_live_setup_summary(runtime: &RuntimeConfig, config_path: &Path) -> String {
    let tokenizer = runtime
        .tokenizer
        .as_ref()
        .map(|path| path.display().to_string())
        .unwrap_or_else(|| "(not set)".to_string());

    let mut output = String::new();
    let _ = writeln!(output, "Live setup complete");
    let _ = writeln!(output, "profile: {LIVE_SETUP_PROFILE_NAME}");
    let _ = writeln!(output, "config: {}", config_path.display());
    let _ = writeln!(output, "model: {}", runtime.model.display());
    let _ = writeln!(output, "tokenizer: {tokenizer}");
    let _ = writeln!(
        output,
        "next: wraithrun --profile {LIVE_SETUP_PROFILE_NAME} --live --task \"Investigate unauthorized SSH keys\""
    );

    output.trim_end().to_string()
}
use std::collections::HashMap;
use std::ffi::OsString;
use std::io::{Cursor, IsTerminal, Read};
use std::path::{Path, PathBuf};
use std::time::Instant;
use std::{fmt::Write as _, fs};

use anyhow::{anyhow, bail, Context, Result};
use clap::{Parser, ValueEnum};
use core_engine::agent::Agent;
use core_engine::{
    CoverageBaseline, EvidencePointer, Finding, FindingSeverity, LiveFailureReasonCount,
    LiveFallbackDecision, LiveRunMetrics, RunReport,
};
use cyber_tools::{ToolRegistry, ToolSpec};
use inference_bridge::onnx_vitis::{inspect_runtime_compatibility, RuntimeCompatibilitySeverity};
use inference_bridge::{ModelConfig, OnnxVitisEngine, VitisEpConfig};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use tracing_subscriber::EnvFilter;

const DEFAULT_CONFIG_FILE: &str = "wraithrun.toml";
const DEFAULT_MODEL_PATH: &str = "./models/llm.onnx";
const DEFAULT_MAX_STEPS: usize = 8;
const DEFAULT_MAX_NEW_TOKENS: usize = 256;
const DEFAULT_TEMPERATURE: f32 = 0.2;
const DEFAULT_CONFIG_TEMPLATE: &str = include_str!("../../wraithrun.example.toml");
const JSON_CONTRACT_VERSION: &str = "1.0.0";
const LIVE_SETUP_PROFILE_NAME: &str = "live-model-local";
const LIVE_PRESET_PROFILE_NAMES: [&str; 3] = ["live-fast", "live-balanced", "live-deep"];

#[derive(Debug, Clone, Copy, ValueEnum, Deserialize, Serialize, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
enum OutputFormat {
    #[default]
    Json,
    Summary,
    Markdown,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
enum LogMode {
    Quiet,
    #[default]
    Normal,
    Verbose,
}

#[derive(Debug, Clone, Copy, ValueEnum, Deserialize, Serialize, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
enum IntrospectionFormat {
    #[default]
    Text,
    Json,
}

#[derive(Debug, Clone, Copy, ValueEnum, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
enum AutomationAdapter {
    FindingsV1,
}

#[derive(Debug, Clone, Copy, ValueEnum, Deserialize, Serialize, PartialEq, Eq, Default)]
#[serde(rename_all = "kebab-case")]
enum ExitPolicy {
    #[default]
    None,
    SeverityThreshold,
}

#[derive(Debug, Clone, Copy, ValueEnum, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
enum ExitSeverityThreshold {
    Info,
    Low,
    Medium,
    High,
    Critical,
}

#[derive(Debug, Clone, Copy, ValueEnum, Deserialize, Serialize, PartialEq, Eq, Default)]
#[serde(rename_all = "kebab-case")]
enum LiveFallbackPolicy {
    #[default]
    None,
    DryRunOnError,
}

#[derive(Debug, Clone, Copy, ValueEnum, PartialEq, Eq)]
enum TaskTemplate {
    #[value(name = "ssh-keys")]
    SshKeys,
    #[value(name = "listener-risk")]
    ListenerRisk,
    #[value(name = "hash-integrity")]
    HashIntegrity,
    #[value(name = "priv-esc-review")]
    PrivEscReview,
    #[value(name = "syslog-summary")]
    SyslogSummary,
}

#[derive(Debug, Clone, Copy, ValueEnum, Deserialize, Serialize, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
enum OutputMode {
    #[default]
    Compact,
    Full,
}

#[derive(Debug, Parser, Clone)]
#[command(name = "wraithrun", about = "Local-first cyber investigation runtime")]
struct Cli {
    #[arg(long, required_unless_present_any = ["task_file", "task_stdin", "task_template", "doctor", "list_profiles", "list_tools", "describe_tool", "print_effective_config", "init_config", "explain_effective_config", "list_task_templates", "verify_bundle", "live_setup", "models_list", "models_validate", "models_benchmark"])]
    task: Option<String>,

    #[arg(long, value_name = "PATH", conflicts_with_all = ["task", "task_stdin", "task_template"])]
    task_file: Option<PathBuf>,

    #[arg(long, conflicts_with_all = ["task", "task_file", "task_template"])]
    task_stdin: bool,

    #[arg(long, value_enum, conflicts_with_all = ["task", "task_file", "task_stdin"])]
    task_template: Option<TaskTemplate>,

    #[arg(long, requires = "task_template")]
    template_target: Option<String>,

    #[arg(long, requires = "task_template")]
    template_lines: Option<usize>,

    #[arg(long)]
    doctor: bool,

    #[arg(long)]
    list_task_templates: bool,

    #[arg(long)]
    list_tools: bool,

    #[arg(long, value_name = "NAME")]
    describe_tool: Option<String>,

    #[arg(long, value_name = "QUERY", requires = "list_tools")]
    tool_filter: Option<String>,

    #[arg(long)]
    list_profiles: bool,

    #[arg(long, value_enum, default_value_t = IntrospectionFormat::Text)]
    introspection_format: IntrospectionFormat,

    #[arg(long)]
    print_effective_config: bool,

    #[arg(long)]
    explain_effective_config: bool,

    #[arg(long)]
    init_config: bool,

    #[arg(long, requires = "init_config")]
    init_config_path: Option<PathBuf>,

    #[arg(long, requires = "init_config")]
    force: bool,

    #[arg(long, requires = "doctor")]
    fix: bool,

    #[arg(long)]
    live_setup: bool,

    #[arg(long)]
    models_list: bool,

    #[arg(long)]
    models_validate: bool,

    #[arg(long)]
    models_benchmark: bool,

    #[arg(long)]
    config: Option<PathBuf>,

    #[arg(long)]
    profile: Option<String>,

    #[arg(long)]
    model: Option<PathBuf>,

    #[arg(long)]
    tokenizer: Option<PathBuf>,

    #[arg(long)]
    max_steps: Option<usize>,

    #[arg(long)]
    max_new_tokens: Option<usize>,

    #[arg(long)]
    temperature: Option<f32>,

    #[arg(long, conflicts_with = "dry_run")]
    live: bool,

    #[arg(long, conflicts_with = "live")]
    dry_run: bool,

    #[arg(long, value_enum)]
    live_fallback_policy: Option<LiveFallbackPolicy>,

    #[arg(long, value_enum)]
    format: Option<OutputFormat>,

    #[arg(long, value_enum)]
    output_mode: Option<OutputMode>,

    #[arg(long, value_enum)]
    automation_adapter: Option<AutomationAdapter>,

    #[arg(long, value_enum)]
    exit_policy: Option<ExitPolicy>,

    #[arg(long, value_enum, requires = "exit_policy")]
    exit_threshold: Option<ExitSeverityThreshold>,

    #[arg(long)]
    output_file: Option<PathBuf>,

    #[arg(long, value_name = "CASE_ID")]
    case_id: Option<String>,

    #[arg(long, value_name = "PATH")]
    evidence_bundle_dir: Option<PathBuf>,

    #[arg(long, value_name = "PATH")]
    evidence_bundle_archive: Option<PathBuf>,

    #[arg(long, value_name = "PATH")]
    baseline_bundle: Option<PathBuf>,

    #[arg(long, value_name = "PATH")]
    verify_bundle: Option<PathBuf>,

    #[arg(long, conflicts_with = "verbose")]
    quiet: bool,

    #[arg(long, short = 'v', conflicts_with = "quiet")]
    verbose: bool,

    #[arg(long)]
    vitis_config: Option<String>,

    #[arg(long)]
    vitis_cache_dir: Option<String>,

    #[arg(long)]
    vitis_cache_key: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct SettingsFragment {
    model: Option<PathBuf>,
    tokenizer: Option<PathBuf>,
    max_steps: Option<usize>,
    max_new_tokens: Option<usize>,
    temperature: Option<f32>,
    live: Option<bool>,
    live_fallback_policy: Option<LiveFallbackPolicy>,
    format: Option<OutputFormat>,
    automation_adapter: Option<AutomationAdapter>,
    exit_policy: Option<ExitPolicy>,
    exit_threshold: Option<ExitSeverityThreshold>,
    output_file: Option<PathBuf>,
    case_id: Option<String>,
    evidence_bundle_dir: Option<PathBuf>,
    evidence_bundle_archive: Option<PathBuf>,
    baseline_bundle: Option<PathBuf>,
    output_mode: Option<OutputMode>,
    log: Option<LogMode>,
    vitis_config: Option<String>,
    vitis_cache_dir: Option<String>,
    vitis_cache_key: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct FileConfig {
    #[serde(flatten)]
    defaults: SettingsFragment,
    profiles: HashMap<String, SettingsFragment>,
}

#[derive(Debug, Clone)]
struct RuntimeConfig {
    task: String,
    model: PathBuf,
    tokenizer: Option<PathBuf>,
    max_steps: usize,
    max_new_tokens: usize,
    temperature: f32,
    live: bool,
    live_fallback_policy: LiveFallbackPolicy,
    format: OutputFormat,
    output_mode: OutputMode,
    automation_adapter: Option<AutomationAdapter>,
    exit_policy: ExitPolicy,
    exit_threshold: Option<ExitSeverityThreshold>,
    output_file: Option<PathBuf>,
    case_id: Option<String>,
    evidence_bundle_dir: Option<PathBuf>,
    evidence_bundle_archive: Option<PathBuf>,
    baseline_bundle: Option<PathBuf>,
    log_mode: LogMode,
    vitis_config: Option<String>,
    vitis_cache_dir: Option<String>,
    vitis_cache_key: Option<String>,
}

#[derive(Debug, Serialize)]
struct RuntimeConfigView {
    task: String,
    mode: &'static str,
    live: bool,
    live_fallback_policy: LiveFallbackPolicy,
    model: String,
    tokenizer: Option<String>,
    max_steps: usize,
    max_new_tokens: usize,
    temperature: f32,
    format: OutputFormat,
    automation_adapter: Option<AutomationAdapter>,
    exit_policy: ExitPolicy,
    exit_threshold: Option<ExitSeverityThreshold>,
    output_file: Option<String>,
    case_id: Option<String>,
    evidence_bundle_dir: Option<String>,
    evidence_bundle_archive: Option<String>,
    baseline_bundle: Option<String>,
    log_mode: LogMode,
    vitis_config: Option<String>,
    vitis_cache_dir: Option<String>,
    vitis_cache_key: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
struct RuntimeConfigSources {
    task: String,
    live: String,
    live_fallback_policy: String,
    model: String,
    tokenizer: String,
    max_steps: String,
    max_new_tokens: String,
    temperature: String,
    format: String,
    output_mode: String,
    automation_adapter: String,
    exit_policy: String,
    exit_threshold: String,
    output_file: String,
    case_id: String,
    evidence_bundle_dir: String,
    evidence_bundle_archive: String,
    baseline_bundle: String,
    log_mode: String,
    vitis_config: String,
    vitis_cache_dir: String,
    vitis_cache_key: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct RawObservationsBundle {
    task: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    case_id: Option<String>,
    turns: Vec<RawObservationTurn>,
}

#[derive(Debug, Serialize, Deserialize)]
struct RawObservationTurn {
    turn: usize,
    tool: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    args: Option<Value>,
    observation: Value,
}

#[derive(Debug)]
struct EvidenceBundleArtifact {
    relative_path: &'static str,
    bytes: Vec<u8>,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
enum BundleVerificationStatus {
    Pass,
    Missing,
    Mismatch,
    Unreadable,
}

#[derive(Debug, Serialize)]
struct BundleVerificationEntry {
    file: String,
    expected_sha256: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    actual_sha256: Option<String>,
    status: BundleVerificationStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    detail: Option<String>,
}

#[derive(Debug, Serialize)]
struct BundleVerificationSummary {
    pass: usize,
    fail: usize,
}

#[derive(Debug, Serialize)]
struct BundleVerificationReport {
    bundle_dir: String,
    checksums_path: String,
    summary: BundleVerificationSummary,
    entries: Vec<BundleVerificationEntry>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    parse_errors: Vec<String>,
}

#[derive(Debug, Serialize)]
struct EffectiveConfigExplanationView {
    effective: RuntimeConfigView,
    sources: RuntimeConfigSources,
    selected_profile: Option<String>,
    config_path: Option<String>,
}

#[derive(Debug, Serialize)]
struct TaskTemplateDescriptor {
    name: &'static str,
    prompt: &'static str,
    supports_template_target: bool,
    supports_template_lines: bool,
    default_target: Option<&'static str>,
    default_lines: Option<usize>,
}

#[derive(Debug, Serialize)]
struct TaskTemplateListView {
    templates: Vec<TaskTemplateDescriptor>,
}

#[derive(Debug, Serialize)]
struct ToolListView {
    tools: Vec<ToolSpec>,
}

#[derive(Debug, Serialize)]
struct ToolDetailView {
    tool: ToolSpec,
}

#[derive(Debug, Serialize)]
struct ProfileSummaryView {
    name: &'static str,
    description: &'static str,
}

#[derive(Debug, Serialize)]
struct SelectedProfileView {
    name: String,
    source: &'static str,
}

#[derive(Debug, Serialize)]
struct ProfileListView {
    built_in_profiles: Vec<ProfileSummaryView>,
    config_path: Option<String>,
    config_profiles: Vec<String>,
    selected_profile: Option<SelectedProfileView>,
}

#[derive(Debug, Clone)]
struct ModelPackCandidate {
    name: String,
    source: String,
    runtime: RuntimeConfig,
}

#[derive(Debug, Clone, Serialize)]
struct ModelPackView {
    name: String,
    source: String,
    model: String,
    tokenizer: Option<String>,
    max_steps: usize,
    max_new_tokens: usize,
    temperature: f32,
    live_fallback_policy: LiveFallbackPolicy,
    readiness: DoctorStatus,
    warn_count: usize,
    fail_count: usize,
}

#[derive(Debug, Serialize)]
struct ModelPackListView {
    packs: Vec<ModelPackView>,
}

#[derive(Debug, Clone, Serialize)]
struct ModelPackValidationCheckView {
    status: DoctorStatus,
    name: &'static str,
    detail: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    reason_code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    remediation: Option<&'static str>,
}

#[derive(Debug, Clone, Serialize)]
struct ModelPackValidationPackView {
    pack: ModelPackView,
    summary: DoctorSummaryView,
    checks: Vec<ModelPackValidationCheckView>,
}

#[derive(Debug, Serialize)]
struct ModelPackValidationView {
    summary: DoctorSummaryView,
    packs: Vec<ModelPackValidationPackView>,
}

#[derive(Debug, Clone, Serialize)]
struct ModelPackBenchmarkEntry {
    pack: ModelPackView,
    estimated_token_budget: usize,
    latency_tier: &'static str,
    benchmark_score: f32,
}

#[derive(Debug, Serialize)]
struct ModelPackBenchmarkView {
    recommended_profile: String,
    rationale: String,
    packs: Vec<ModelPackBenchmarkEntry>,
}

#[derive(Debug)]
struct ModelPackValidationOutcome {
    rendered: String,
    has_failures: bool,
}

#[derive(Debug, Clone, Serialize)]
struct DoctorSummaryView {
    pass: usize,
    warn: usize,
    fail: usize,
}

#[derive(Debug, Serialize)]
struct DoctorReportView<'a> {
    summary: DoctorSummaryView,
    checks: &'a [DoctorCheck],
}

#[derive(Debug, Default, Serialize)]
struct AdapterSeverityCounts {
    info: usize,
    low: usize,
    medium: usize,
    high: usize,
    critical: usize,
}

#[derive(Debug, Serialize)]
struct FindingsAdapterSummary {
    task: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    case_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    live_fallback_decision: Option<LiveFallbackDecision>,
    #[serde(skip_serializing_if = "Option::is_none")]
    live_run_metrics: Option<LiveRunMetrics>,
    finding_count: usize,
    highest_severity: String,
    severity_counts: AdapterSeverityCounts,
}

#[derive(Debug, Serialize)]
struct FindingsAdapterEntry {
    finding_id: String,
    title: String,
    severity: FindingSeverity,
    confidence: f32,
    recommended_action: String,
    evidence_pointer: EvidencePointer,
}

#[derive(Debug, Serialize)]
struct FindingsAdapterView {
    adapter: &'static str,
    summary: FindingsAdapterSummary,
    findings: Vec<FindingsAdapterEntry>,
}

impl RuntimeConfig {
    fn new(task: String) -> Self {
        Self {
            task,
            model: PathBuf::from(DEFAULT_MODEL_PATH),
            tokenizer: None,
            max_steps: DEFAULT_MAX_STEPS,
            max_new_tokens: DEFAULT_MAX_NEW_TOKENS,
            temperature: DEFAULT_TEMPERATURE,
            live: false,
            live_fallback_policy: LiveFallbackPolicy::None,
            format: OutputFormat::Json,
            output_mode: OutputMode::Compact,
            automation_adapter: None,
            exit_policy: ExitPolicy::None,
            exit_threshold: None,
            output_file: None,
            case_id: None,
            evidence_bundle_dir: None,
            evidence_bundle_archive: None,
            baseline_bundle: None,
            log_mode: LogMode::Normal,
            vitis_config: None,
            vitis_cache_dir: None,
            vitis_cache_key: None,
        }
    }

    fn apply_fragment(&mut self, fragment: &SettingsFragment) {
        if let Some(model) = &fragment.model {
            self.model = model.clone();
        }
        if let Some(tokenizer) = &fragment.tokenizer {
            self.tokenizer = Some(tokenizer.clone());
        }
        if let Some(max_steps) = fragment.max_steps {
            self.max_steps = max_steps;
        }
        if let Some(max_new_tokens) = fragment.max_new_tokens {
            self.max_new_tokens = max_new_tokens;
        }
        if let Some(temperature) = fragment.temperature {
            self.temperature = temperature;
        }
        if let Some(live) = fragment.live {
            self.live = live;
        }
        if let Some(live_fallback_policy) = fragment.live_fallback_policy {
            self.live_fallback_policy = live_fallback_policy;
        }
        if let Some(format) = fragment.format {
            self.format = format;
        }
        if let Some(automation_adapter) = fragment.automation_adapter {
            self.automation_adapter = Some(automation_adapter);
        }
        if let Some(exit_policy) = fragment.exit_policy {
            self.exit_policy = exit_policy;
        }
        if let Some(exit_threshold) = fragment.exit_threshold {
            self.exit_threshold = Some(exit_threshold);
        }
        if let Some(output_file) = &fragment.output_file {
            self.output_file = Some(output_file.clone());
        }
        if let Some(case_id) = &fragment.case_id {
            self.case_id = Some(case_id.clone());
        }
        if let Some(evidence_bundle_dir) = &fragment.evidence_bundle_dir {
            self.evidence_bundle_dir = Some(evidence_bundle_dir.clone());
        }
        if let Some(evidence_bundle_archive) = &fragment.evidence_bundle_archive {
            self.evidence_bundle_archive = Some(evidence_bundle_archive.clone());
        }
        if let Some(baseline_bundle) = &fragment.baseline_bundle {
            self.baseline_bundle = Some(baseline_bundle.clone());
        }
        if let Some(log_mode) = fragment.log {
            self.log_mode = log_mode;
        }
        if let Some(vitis_config) = &fragment.vitis_config {
            self.vitis_config = Some(vitis_config.clone());
        }
        if let Some(vitis_cache_dir) = &fragment.vitis_cache_dir {
            self.vitis_cache_dir = Some(vitis_cache_dir.clone());
        }
        if let Some(vitis_cache_key) = &fragment.vitis_cache_key {
            self.vitis_cache_key = Some(vitis_cache_key.clone());
        }
    }
}

impl RuntimeConfigSources {
    fn with_defaults(task_source: &str) -> Self {
        Self {
            task: task_source.to_string(),
            live: "default".to_string(),
            live_fallback_policy: "default".to_string(),
            model: "default".to_string(),
            tokenizer: "default".to_string(),
            max_steps: "default".to_string(),
            max_new_tokens: "default".to_string(),
            temperature: "default".to_string(),
            format: "default".to_string(),
            output_mode: "default".to_string(),
            automation_adapter: "default".to_string(),
            exit_policy: "default".to_string(),
            exit_threshold: "default".to_string(),
            output_file: "default".to_string(),
            case_id: "default".to_string(),
            evidence_bundle_dir: "default".to_string(),
            evidence_bundle_archive: "default".to_string(),
            baseline_bundle: "default".to_string(),
            log_mode: "default".to_string(),
            vitis_config: "default".to_string(),
            vitis_cache_dir: "default".to_string(),
            vitis_cache_key: "default".to_string(),
        }
    }
}

#[derive(Debug)]
enum ConfigPathSelection {
    None,
    Optional(PathBuf),
    Required(PathBuf),
}

fn remediation_for_reason_code(reason_code: &str) -> Option<&'static str> {
    match reason_code {
        // Model path issues
        "model_path_missing" => Some("Place an ONNX model file under ./models/ or pass --model <PATH>."),
        "model_path_explicit_invalid" => Some("Update --model to point to a readable .onnx file."),
        "model_path_discovery_failed" => Some("Place a .onnx file under ./models/ or pass --model <PATH>."),
        "model_file_empty" => Some("The model file has zero bytes. Re-download or replace the model."),
        "model_permission_denied" => Some("Grant read permission on the model file to the account running WraithRun."),
        "model_format_non_onnx" | "model_format_explicit_non_onnx" => Some("Provide a model with the .onnx extension."),

        // Tokenizer issues
        "tokenizer_path_missing" => Some("Place tokenizer.json beside the model or pass --tokenizer <PATH>."),
        "tokenizer_path_explicit_invalid" => Some("Update --tokenizer to a readable JSON file with a top-level 'model' key."),
        "tokenizer_discovery_failed" => Some("Add tokenizer.json beside the model (or under ./models/) and retry."),
        "tokenizer_file_empty" => Some("The tokenizer file has zero bytes. Re-download or replace the tokenizer."),
        "tokenizer_json_invalid" => Some("The tokenizer file is not valid JSON. Replace it with a valid tokenizer.json."),
        "tokenizer_model_key_missing" => Some("The tokenizer JSON is missing the top-level 'model' key. Use a Hugging Face tokenizer.json."),
        "tokenizer_permission_denied" => Some("Grant read permission on the tokenizer file."),

        // Runtime / session init issues
        "runtime_session_init_failed" => Some("Verify the model file is a valid ONNX model. Re-download if corrupted."),
        "runtime_model_invalid" => Some("The model file is not a valid ONNX model. Re-download or convert to ONNX format."),
        "runtime_ort_dylib_missing" => Some("Set ORT_DYLIB_PATH to the onnxruntime shared library, or place it beside the model."),
        "runtime_vitis_provider_missing" => Some("Install the RyzenAI SDK or set ORT_DYLIB_PATH to a Vitis-enabled ONNX Runtime build."),
        "runtime_custom_ops_unavailable" => Some("Install the custom ops library from the RyzenAI SDK, or place it beside the ONNX Runtime DLL."),
        "runtime_external_data_file_missing" => Some("Ensure the external data file referenced by the model is present beside the .onnx file."),
        "runtime_external_initializer_unresolved" => Some("Check that all external data files and _ORT_MEM_ADDR_ directories are present beside the model."),
        "runtime_ep_assignment_failed" => Some("The execution provider could not be assigned. Check runtime and model compatibility."),

        // IO signature issues
        "runtime_input_ids_missing" => Some("The model does not expose an input_ids or tokens input. It may not be a text-generation model."),
        "runtime_logits_output_missing" => Some("The model does not expose logits output. It may not be a text-generation model."),
        "runtime_input_unsupported" => Some("The model requires inputs this runtime does not support. Check model compatibility."),
        "runtime_input_dtype_unsupported" => Some("The model uses unsupported tensor types. Convert the model to use int64/int32 sequence inputs."),
        "runtime_cache_dtype_unsupported" => Some("The model's KV-cache tensors use an unsupported dtype. Convert to float32/float16."),
        "runtime_cache_output_missing" => Some("The model has cache inputs but no matching cache outputs. Check model export settings."),
        "runtime_forward_smoke_failed" => Some("The model loads but fails a minimal forward pass. Check model integrity and runtime version."),

        // ONNX feature disabled
        "onnx_feature_disabled" => Some("Rebuild with '--features inference_bridge/onnx' or '--features inference_bridge/vitis' to enable inference."),

        _ => None,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
enum DoctorStatus {
    Pass,
    Warn,
    Fail,
}

impl DoctorStatus {
    fn label(self) -> &'static str {
        match self {
            Self::Pass => "PASS",
            Self::Warn => "WARN",
            Self::Fail => "FAIL",
        }
    }
}

#[derive(Debug, Serialize)]
struct DoctorCheck {
    status: DoctorStatus,
    name: &'static str,
    detail: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    reason_code: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    remediation: Option<&'static str>,
}

#[derive(Debug, Default, Serialize)]
struct DoctorReport {
    checks: Vec<DoctorCheck>,
}

impl DoctorReport {
    fn push(&mut self, status: DoctorStatus, name: &'static str, detail: impl Into<String>) {
        self.push_with_reason(status, name, detail, None);
    }

    fn push_with_reason(
        &mut self,
        status: DoctorStatus,
        name: &'static str,
        detail: impl Into<String>,
        reason_code: Option<&'static str>,
    ) {
        let remediation = reason_code.and_then(remediation_for_reason_code);
        self.checks.push(DoctorCheck {
            status,
            name,
            detail: detail.into(),
            reason_code,
            remediation,
        });
    }

    fn has_failures(&self) -> bool {
        self.checks
            .iter()
            .any(|check| check.status == DoctorStatus::Fail)
    }

    fn counts(&self) -> (usize, usize, usize) {
        let pass_count = self
            .checks
            .iter()
            .filter(|check| check.status == DoctorStatus::Pass)
            .count();
        let warn_count = self
            .checks
            .iter()
            .filter(|check| check.status == DoctorStatus::Warn)
            .count();
        let fail_count = self
            .checks
            .iter()
            .filter(|check| check.status == DoctorStatus::Fail)
            .count();
        (pass_count, warn_count, fail_count)
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli_args = normalize_models_alias(std::env::args_os());
    let cli_args = normalize_live_setup_alias(cli_args);
    let cli = Cli::parse_from(cli_args);
    ensure_exclusive_modes(&cli)?;
    ensure_introspection_format_usage(&cli)?;

    if cli.list_task_templates {
        let rendered = match cli.introspection_format {
            IntrospectionFormat::Text => render_task_template_list(),
            IntrospectionFormat::Json => render_task_template_list_json()?,
        };
        println!("{rendered}");
        return Ok(());
    }

    if cli.list_tools {
        let rendered = run_list_tools(cli.introspection_format, cli.tool_filter.as_deref())?;
        println!("{rendered}");
        return Ok(());
    }

    if let Some(tool_name) = cli.describe_tool.as_deref() {
        let rendered = run_describe_tool(tool_name, cli.introspection_format)?;
        println!("{rendered}");
        return Ok(());
    }

    if cli.models_list {
        let rendered = run_models_list(&cli, cli.introspection_format)?;
        println!("{rendered}");
        return Ok(());
    }

    if cli.models_validate {
        let outcome = run_models_validate(&cli, cli.introspection_format)?;
        println!("{}", outcome.rendered);
        if outcome.has_failures {
            bail!("model pack validation reported failures");
        }
        return Ok(());
    }

    if cli.models_benchmark {
        let rendered = run_models_benchmark(&cli, cli.introspection_format)?;
        println!("{rendered}");
        return Ok(());
    }

    if cli.live_setup {
        let message = run_live_setup(&cli)?;
        println!("{message}");
        return Ok(());
    }

    if cli.init_config {
        let message = run_init_config(&cli)?;
        println!("{message}");
        return Ok(());
    }

    if cli.list_profiles {
        let listing = run_list_profiles(&cli, cli.introspection_format)?;
        println!("{listing}");
        return Ok(());
    }

    if cli.print_effective_config {
        let runtime = resolve_runtime_config_for_preview(&cli)?;
        println!("{}", render_effective_config_json(&runtime)?);
        return Ok(());
    }

    if cli.explain_effective_config {
        let explanation = resolve_effective_config_explanation(&cli)?;
        println!(
            "{}",
            render_effective_config_explanation_json(&explanation)?
        );
        return Ok(());
    }

    if cli.doctor {
        let report = run_doctor(&cli);
        let rendered = match cli.introspection_format {
            IntrospectionFormat::Text => render_doctor_report(&report),
            IntrospectionFormat::Json => render_doctor_report_json(&report)?,
        };
        println!("{rendered}");
        if report.has_failures() {
            bail!("doctor checks reported failures");
        }
        return Ok(());
    }

    if let Some(path) = cli.verify_bundle.as_deref() {
        let report = verify_evidence_bundle(path)?;
        let rendered = match cli.introspection_format {
            IntrospectionFormat::Text => render_bundle_verification_report(&report),
            IntrospectionFormat::Json => render_bundle_verification_report_json(&report)?,
        };
        println!("{rendered}");

        if report.summary.fail > 0 {
            bail!("evidence bundle verification failed");
        }

        return Ok(());
    }

    let runtime = resolve_runtime_config(&cli)?;
    init_tracing(runtime.log_mode);

    let mut report = run_with_live_fallback(&runtime).await?;
    if let Some(case_id) = runtime.case_id.as_ref() {
        report.case_id = Some(case_id.trim().to_string());
    }

    if let Some(bundle_dir) = &runtime.evidence_bundle_dir {
        write_evidence_bundle(bundle_dir, &report)?;
    }

    if let Some(archive_path) = &runtime.evidence_bundle_archive {
        write_evidence_bundle_archive(archive_path, &report)?;
    }

    let rendered = render_report(&report, runtime.format, runtime.output_mode, runtime.automation_adapter)?;
    if let Some(path) = &runtime.output_file {
        write_report_file(path, &rendered)?;
    }
    println!("{rendered}");

    if let Some(message) =
        evaluate_exit_policy(&report, runtime.exit_policy, runtime.exit_threshold)
    {
        bail!("{message}");
    }

    Ok(())
}

fn normalize_models_alias(args: impl IntoIterator<Item = OsString>) -> Vec<OsString> {
    let args: Vec<OsString> = args.into_iter().collect();
    if args.len() < 3 {
        return args;
    }

    let is_models = args
        .get(1)
        .and_then(|value| value.to_str())
        .map(|value| value.eq_ignore_ascii_case("models"))
        .unwrap_or(false);
    if !is_models {
        return args;
    }

    let mapped = args
        .get(2)
        .and_then(|value| value.to_str())
        .map(|value| match value.to_ascii_lowercase().as_str() {
            "list" => Some("--models-list"),
            "validate" => Some("--models-validate"),
            "benchmark" => Some("--models-benchmark"),
            _ => None,
        })
        .unwrap_or(None);

    let Some(flag) = mapped else {
        return args;
    };

    let mut normalized = Vec::with_capacity(args.len());
    normalized.push(args[0].clone());
    normalized.push(OsString::from(flag));
    normalized.extend(args.into_iter().skip(3));
    normalized
}

fn normalize_live_setup_alias(args: impl IntoIterator<Item = OsString>) -> Vec<OsString> {
    let args: Vec<OsString> = args.into_iter().collect();
    if args.len() < 3 {
        return args;
    }

    let is_live = args
        .get(1)
        .and_then(|value| value.to_str())
        .map(|value| value.eq_ignore_ascii_case("live"))
        .unwrap_or(false);
    let is_setup = args
        .get(2)
        .and_then(|value| value.to_str())
        .map(|value| value.eq_ignore_ascii_case("setup"))
        .unwrap_or(false);

    if !is_live || !is_setup {
        return args;
    }

    let mut normalized = Vec::with_capacity(args.len());
    normalized.push(args[0].clone());
    normalized.push(OsString::from("--live-setup"));
    normalized.extend(args.into_iter().skip(3));
    normalized
}

fn resolve_runtime_config(cli: &Cli) -> Result<RuntimeConfig> {
    let task = resolve_task_for_run(cli)?;

    resolve_runtime_config_with_task(cli, task)
}

fn resolve_runtime_config_for_preview(cli: &Cli) -> Result<RuntimeConfig> {
    let task = resolve_task_for_mode(cli, "preview-effective-config")?;
    resolve_runtime_config_with_task(cli, task)
}

fn resolve_effective_config_explanation(cli: &Cli) -> Result<EffectiveConfigExplanationView> {
    let task = resolve_task_for_mode(cli, "explain-effective-config")?;

    let task_source = if cli.task.is_some() {
        "cli --task".to_string()
    } else if cli.task_stdin {
        "cli --task-stdin".to_string()
    } else if cli.task_file.is_some() {
        "cli --task-file".to_string()
    } else if let Some(template) = cli.task_template {
        format!("cli --task-template ({})", task_template_name(template))
    } else {
        "mode default".to_string()
    };

    let profile = resolve_profile_name(cli)?;
    let (file_config, file_config_path) = load_config_for_cli(cli)?;
    let env_overrides = env_settings_fragment()?;

    let (runtime, sources) = merge_sources_with_explanation(
        cli,
        task,
        &task_source,
        profile.clone(),
        file_config.as_ref(),
        file_config_path.as_deref(),
        &env_overrides,
    )?;

    Ok(EffectiveConfigExplanationView {
        effective: RuntimeConfigView::from_runtime(&runtime),
        sources,
        selected_profile: profile,
        config_path: file_config_path.map(|path| path.display().to_string()),
    })
}

fn resolve_runtime_config_with_task(cli: &Cli, task: String) -> Result<RuntimeConfig> {
    let profile = resolve_profile_name(cli)?;
    let (file_config, file_config_path) = load_config_for_cli(cli)?;
    let env_overrides = env_settings_fragment()?;

    merge_sources(
        cli,
        task,
        profile,
        file_config.as_ref(),
        file_config_path.as_deref(),
        &env_overrides,
    )
}

fn resolve_task_for_run(cli: &Cli) -> Result<String> {
    if let Some(task) = &cli.task {
        let trimmed = task.trim();
        if trimmed.is_empty() {
            bail!("--task cannot be empty");
        }
        return Ok(trimmed.to_string());
    }

    if cli.task_stdin {
        return load_task_from_stdin();
    }

    if let Some(task_file) = &cli.task_file {
        return load_task_from_file(task_file);
    }

    if let Some(template) = cli.task_template {
        return resolve_task_from_template(cli, template);
    }

    bail!(
        "Either --task, --task-stdin, --task-file, or --task-template is required unless one of --doctor, --list-task-templates, --list-tools, --describe-tool, --list-profiles, --print-effective-config, --explain-effective-config, --init-config, --live-setup, --models-list, --models-validate, or --models-benchmark is set"
    )
}

fn resolve_task_for_mode(cli: &Cli, fallback: &str) -> Result<String> {
    if let Some(task) = &cli.task {
        return Ok(task.trim().to_string());
    }
    if cli.task_stdin {
        return load_task_from_stdin();
    }
    if let Some(task_file) = &cli.task_file {
        return load_task_from_file(task_file);
    }
    if let Some(template) = cli.task_template {
        return resolve_task_from_template(cli, template);
    }
    Ok(fallback.to_string())
}

fn load_task_from_file(path: &Path) -> Result<String> {
    if path == Path::new("-") {
        return load_task_from_stdin();
    }

    if !path.is_file() {
        bail!("Task file not found: {}", path.display());
    }

    let task_bytes =
        fs::read(path).with_context(|| format!("Failed reading task file {}", path.display()))?;
    let task_text = decode_task_text(&task_bytes, path)?;

    let trimmed = task_text.trim();
    if trimmed.is_empty() {
        bail!("Task file '{}' is empty", path.display());
    }

    Ok(trimmed.to_string())
}

fn decode_task_text(bytes: &[u8], path: &Path) -> Result<String> {
    if let Some(rest) = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]) {
        return String::from_utf8(rest.to_vec())
            .with_context(|| format!("Task file '{}' is not valid UTF-8", path.display()));
    }

    if let Some(rest) = bytes.strip_prefix(&[0xFF, 0xFE]) {
        return decode_utf16_text(rest, true, path);
    }

    if let Some(rest) = bytes.strip_prefix(&[0xFE, 0xFF]) {
        return decode_utf16_text(rest, false, path);
    }

    String::from_utf8(bytes.to_vec())
        .with_context(|| format!("Task file '{}' is not valid UTF-8", path.display()))
}

fn decode_utf16_text(bytes: &[u8], little_endian: bool, path: &Path) -> Result<String> {
    if !bytes.len().is_multiple_of(2) {
        bail!(
            "Task file '{}' has invalid UTF-16 byte length",
            path.display()
        );
    }

    let mut units = Vec::with_capacity(bytes.len() / 2);
    for chunk in bytes.chunks_exact(2) {
        let unit = if little_endian {
            u16::from_le_bytes([chunk[0], chunk[1]])
        } else {
            u16::from_be_bytes([chunk[0], chunk[1]])
        };
        units.push(unit);
    }

    let mut text = String::new();
    for decoded in char::decode_utf16(units.into_iter()) {
        match decoded {
            Ok(ch) => text.push(ch),
            Err(_) => {
                bail!(
                    "Task file '{}' contains invalid UTF-16 data",
                    path.display()
                );
            }
        }
    }

    Ok(text)
}

fn load_task_from_stdin() -> Result<String> {
    let stdin = std::io::stdin();
    if stdin.is_terminal() {
        bail!("--task-stdin requires piped stdin input");
    }

    let mut task_text = String::new();
    stdin
        .lock()
        .read_to_string(&mut task_text)
        .context("Failed reading task text from stdin")?;

    let trimmed = task_text.trim();
    if trimmed.is_empty() {
        bail!("Stdin task input is empty");
    }

    Ok(trimmed.to_string())
}

fn task_template_name(template: TaskTemplate) -> &'static str {
    match template {
        TaskTemplate::SshKeys => "ssh-keys",
        TaskTemplate::ListenerRisk => "listener-risk",
        TaskTemplate::HashIntegrity => "hash-integrity",
        TaskTemplate::PrivEscReview => "priv-esc-review",
        TaskTemplate::SyslogSummary => "syslog-summary",
    }
}

fn task_template_prompt(template: TaskTemplate) -> &'static str {
    match template {
        TaskTemplate::SshKeys => "Investigate unauthorized SSH keys",
        TaskTemplate::ListenerRisk => "Check suspicious listener ports and summarize risk",
        TaskTemplate::HashIntegrity => {
            "Hash C:/Windows/System32/notepad.exe and report integrity context"
        }
        TaskTemplate::PrivEscReview => "Review local privilege escalation indicators",
        TaskTemplate::SyslogSummary => "Read and summarize last 200 lines from C:/Logs/agent.log",
    }
}

fn task_template_descriptors() -> Vec<TaskTemplateDescriptor> {
    vec![
        TaskTemplateDescriptor {
            name: "ssh-keys",
            prompt: task_template_prompt(TaskTemplate::SshKeys),
            supports_template_target: false,
            supports_template_lines: false,
            default_target: None,
            default_lines: None,
        },
        TaskTemplateDescriptor {
            name: "listener-risk",
            prompt: task_template_prompt(TaskTemplate::ListenerRisk),
            supports_template_target: false,
            supports_template_lines: false,
            default_target: None,
            default_lines: None,
        },
        TaskTemplateDescriptor {
            name: "hash-integrity",
            prompt: task_template_prompt(TaskTemplate::HashIntegrity),
            supports_template_target: true,
            supports_template_lines: false,
            default_target: Some("C:/Windows/System32/notepad.exe"),
            default_lines: None,
        },
        TaskTemplateDescriptor {
            name: "priv-esc-review",
            prompt: task_template_prompt(TaskTemplate::PrivEscReview),
            supports_template_target: false,
            supports_template_lines: false,
            default_target: None,
            default_lines: None,
        },
        TaskTemplateDescriptor {
            name: "syslog-summary",
            prompt: task_template_prompt(TaskTemplate::SyslogSummary),
            supports_template_target: true,
            supports_template_lines: true,
            default_target: Some("C:/Logs/agent.log"),
            default_lines: Some(200),
        },
    ]
}

fn render_task_template_list_json() -> Result<String> {
    let view = TaskTemplateListView {
        templates: task_template_descriptors(),
    };
    render_json_with_contract(&view)
}

fn run_list_tools(format: IntrospectionFormat, filter: Option<&str>) -> Result<String> {
    let registry = ToolRegistry::with_default_tools();
    let mut tools = registry.tool_specs();

    if let Some(raw_filter) = filter {
        let normalized = raw_filter.trim();
        let terms = parse_tool_filter_terms(normalized)?;
        tools.retain(|tool| tool_matches_filter(tool, &terms));

        if tools.is_empty() {
            bail!(
                "No tools matched filter '{}'. Use --list-tools without --tool-filter to show all tools.",
                normalized
            );
        }
    }

    match format {
        IntrospectionFormat::Text => Ok(render_tool_list(&tools)),
        IntrospectionFormat::Json => render_tool_list_json(tools),
    }
}

fn parse_tool_filter_terms(raw_filter: &str) -> Result<Vec<String>> {
    if raw_filter.is_empty() {
        bail!("--tool-filter cannot be empty");
    }

    let terms = normalize_search_terms(raw_filter);
    if terms.is_empty() {
        bail!("--tool-filter must include at least one alphanumeric term");
    }

    Ok(terms)
}

fn tool_matches_filter(tool: &ToolSpec, terms: &[String]) -> bool {
    let searchable = format!(
        "{} {}",
        normalize_search_text(&tool.name),
        normalize_search_text(&tool.description)
    );

    terms.iter().all(|term| searchable.contains(term))
}

fn normalize_search_terms(value: &str) -> Vec<String> {
    normalize_search_text(value)
        .split_whitespace()
        .map(str::to_string)
        .collect()
}

fn normalize_search_text(value: &str) -> String {
    let mut normalized = String::new();
    let mut pending_separator = false;

    for ch in value.chars() {
        if ch.is_ascii_alphanumeric() {
            if pending_separator && !normalized.is_empty() {
                normalized.push(' ');
            }
            normalized.push(ch.to_ascii_lowercase());
            pending_separator = false;
        } else {
            pending_separator = true;
        }
    }

    normalized
}

fn run_describe_tool(tool_name: &str, format: IntrospectionFormat) -> Result<String> {
    let selected = tool_name.trim();
    if selected.is_empty() {
        bail!("--describe-tool cannot be empty");
    }

    let registry = ToolRegistry::with_default_tools();
    let tools = registry.tool_specs();

    let tool = resolve_tool_query(&tools, selected)?;

    match format {
        IntrospectionFormat::Text => Ok(render_tool_detail(tool)),
        IntrospectionFormat::Json => render_tool_detail_json(tool.clone()),
    }
}

fn resolve_tool_query<'a>(tools: &'a [ToolSpec], query: &str) -> Result<&'a ToolSpec> {
    if let Some(tool) = tools
        .iter()
        .find(|candidate| candidate.name.eq_ignore_ascii_case(query))
    {
        return Ok(tool);
    }

    let normalized_query = normalize_tool_query(query);

    if let Some(tool) = find_unique_tool_match(tools, &normalized_query, |name, normalized| {
        name == normalized
    })? {
        return Ok(tool);
    }

    if let Some(tool) = find_unique_tool_match(tools, &normalized_query, |name, normalized| {
        name.contains(normalized)
    })? {
        return Ok(tool);
    }

    let available = tools
        .iter()
        .map(|candidate| candidate.name.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    bail!("Unknown tool '{query}'. Available tools: {available}");
}

fn find_unique_tool_match<'a, F>(
    tools: &'a [ToolSpec],
    normalized_query: &str,
    matcher: F,
) -> Result<Option<&'a ToolSpec>>
where
    F: Fn(&str, &str) -> bool,
{
    let mut matches = tools
        .iter()
        .filter(|candidate| matcher(&normalize_tool_query(&candidate.name), normalized_query));

    let first = matches.next();
    let second = matches.next();

    match (first, second) {
        (Some(tool), None) => Ok(Some(tool)),
        (Some(first_tool), Some(second_tool)) => {
            let mut names = vec![first_tool.name.clone(), second_tool.name.clone()];
            names.extend(matches.map(|candidate| candidate.name.clone()));
            bail!(
                "Ambiguous tool query '{}'. Matches: {}. Use the full tool name.",
                normalized_query,
                names.join(", ")
            );
        }
        _ => Ok(None),
    }
}

fn normalize_tool_query(value: &str) -> String {
    value.trim().to_ascii_lowercase().replace(['-', ' '], "_")
}

fn render_tool_list_json(tools: Vec<ToolSpec>) -> Result<String> {
    let view = ToolListView { tools };
    render_json_with_contract(&view)
}

fn render_tool_detail_json(tool: ToolSpec) -> Result<String> {
    let view = ToolDetailView { tool };
    render_json_with_contract(&view)
}

fn render_tool_list(tools: &[ToolSpec]) -> String {
    let mut output = String::new();

    let _ = writeln!(output, "WraithRun Tools");
    for tool in tools {
        let _ = writeln!(output, "- {}: {}", tool.name, tool.description);
        let _ = writeln!(output, "  args_schema: {}", compact_json(&tool.args_schema));
    }

    output.trim_end().to_string()
}

fn render_tool_detail(tool: &ToolSpec) -> String {
    let mut output = String::new();

    let _ = writeln!(output, "WraithRun Tool");
    let _ = writeln!(output, "name: {}", tool.name);
    let _ = writeln!(output, "description: {}", tool.description);
    let _ = writeln!(output, "args_schema:");
    for line in pretty_json(&tool.args_schema).lines() {
        let _ = writeln!(output, "  {line}");
    }

    output.trim_end().to_string()
}

fn resolve_task_from_template(cli: &Cli, template: TaskTemplate) -> Result<String> {
    if let Some(raw_target) = &cli.template_target {
        if raw_target.trim().is_empty() {
            bail!("--template-target cannot be empty when provided");
        }
    }

    match template {
        TaskTemplate::SshKeys | TaskTemplate::ListenerRisk | TaskTemplate::PrivEscReview => {
            if cli.template_target.is_some() || cli.template_lines.is_some() {
                bail!(
                    "--template-target and --template-lines are not supported for task template '{}'.",
                    task_template_name(template)
                );
            }
            Ok(task_template_prompt(template).to_string())
        }
        TaskTemplate::HashIntegrity => {
            if cli.template_lines.is_some() {
                bail!("--template-lines is not supported for task template 'hash-integrity'.");
            }

            let target = cli
                .template_target
                .as_deref()
                .unwrap_or("C:/Windows/System32/notepad.exe")
                .trim();
            Ok(format!("Hash {target} and report integrity context"))
        }
        TaskTemplate::SyslogSummary => {
            let target = cli
                .template_target
                .as_deref()
                .unwrap_or("C:/Logs/agent.log")
                .trim();
            let lines = cli.template_lines.unwrap_or(200);
            if lines == 0 {
                bail!("--template-lines must be at least 1 for task template 'syslog-summary'.");
            }

            Ok(format!(
                "Read and summarize last {lines} lines from {target}"
            ))
        }
    }
}

fn render_task_template_list() -> String {
    let mut output = String::new();
    let templates = task_template_descriptors();

    let _ = writeln!(output, "WraithRun Task Templates");
    for descriptor in templates {
        let _ = writeln!(output, "- {}: {}", descriptor.name, descriptor.prompt);
        if descriptor.supports_template_target && descriptor.supports_template_lines {
            let default_target = descriptor.default_target.unwrap_or("(none)");
            let default_lines = descriptor.default_lines.unwrap_or(0);
            let _ = writeln!(
                output,
                "  options: --template-target <PATH> (default {default_target}), --template-lines <N> (default {default_lines})"
            );
            continue;
        }

        if descriptor.supports_template_target {
            let default_target = descriptor.default_target.unwrap_or("(none)");
            let _ = writeln!(
                output,
                "  options: --template-target <PATH> (default {default_target})"
            );
        }
    }

    output.trim_end().to_string()
}

fn merge_sources(
    cli: &Cli,
    task: String,
    profile: Option<String>,
    file_config: Option<&FileConfig>,
    file_config_path: Option<&Path>,
    env_overrides: &SettingsFragment,
) -> Result<RuntimeConfig> {
    let mut resolved = RuntimeConfig::new(task);

    let built_in_profile = profile.as_deref().and_then(builtin_profile);
    if let Some(fragment) = built_in_profile.as_ref() {
        resolved.apply_fragment(fragment);
    }

    let mut matched_file_profile = false;
    if let Some(file_config) = file_config {
        resolved.apply_fragment(&file_config.defaults);

        if let Some(profile_name) = profile.as_deref() {
            if let Some(profile_settings) = lookup_profile(&file_config.profiles, profile_name) {
                resolved.apply_fragment(profile_settings);
                matched_file_profile = true;
            }
        }
    }

    if let Some(profile_name) = profile.as_deref() {
        if built_in_profile.is_none() && !matched_file_profile {
            let known_profiles = KNOWN_PROFILE_NAMES.join(", ");
            if let Some(path) = file_config_path {
                bail!(
                    "Profile '{profile_name}' was not found in built-in profiles ({known_profiles}) or in config '{}'.",
                    path.display()
                );
            }

            bail!(
                "Profile '{profile_name}' was not found in built-in profiles ({known_profiles}), and no config file was loaded."
            );
        }
    }

    resolved.apply_fragment(env_overrides);
    apply_cli_overrides(&mut resolved, cli);
    validate_runtime_config(&resolved)?;

    Ok(resolved)
}

fn merge_sources_with_explanation(
    cli: &Cli,
    task: String,
    task_source: &str,
    profile: Option<String>,
    file_config: Option<&FileConfig>,
    file_config_path: Option<&Path>,
    env_overrides: &SettingsFragment,
) -> Result<(RuntimeConfig, RuntimeConfigSources)> {
    let mut resolved = RuntimeConfig::new(task);
    let mut sources = RuntimeConfigSources::with_defaults(task_source);

    let built_in_profile = profile.as_deref().and_then(builtin_profile);
    if let Some(profile_name) = profile.as_deref() {
        if let Some(fragment) = built_in_profile.as_ref() {
            let source = format!("built-in profile '{profile_name}'");
            apply_fragment_with_source(&mut resolved, &mut sources, fragment, &source);
        }
    }

    let mut matched_file_profile = false;
    if let Some(file_config) = file_config {
        let source = file_config_path
            .map(|path| format!("config defaults ({})", path.display()))
            .unwrap_or_else(|| "config defaults".to_string());
        apply_fragment_with_source(&mut resolved, &mut sources, &file_config.defaults, &source);

        if let Some(profile_name) = profile.as_deref() {
            if let Some(profile_settings) = lookup_profile(&file_config.profiles, profile_name) {
                let source = file_config_path
                    .map(|path| format!("config profile '{profile_name}' ({})", path.display()))
                    .unwrap_or_else(|| format!("config profile '{profile_name}'"));
                apply_fragment_with_source(&mut resolved, &mut sources, profile_settings, &source);
                matched_file_profile = true;
            }
        }
    }

    if let Some(profile_name) = profile.as_deref() {
        if built_in_profile.is_none() && !matched_file_profile {
            let known_profiles = KNOWN_PROFILE_NAMES.join(", ");
            if let Some(path) = file_config_path {
                bail!(
                    "Profile '{profile_name}' was not found in built-in profiles ({known_profiles}) or in config '{}'.",
                    path.display()
                );
            }

            bail!(
                "Profile '{profile_name}' was not found in built-in profiles ({known_profiles}), and no config file was loaded."
            );
        }
    }

    apply_fragment_with_source(
        &mut resolved,
        &mut sources,
        env_overrides,
        "environment variables",
    );
    apply_cli_overrides_with_source(&mut resolved, &mut sources, cli);
    validate_runtime_config(&resolved)?;

    Ok((resolved, sources))
}

fn resolve_profile_name(cli: &Cli) -> Result<Option<String>> {
    if let Some(profile) = &cli.profile {
        return normalize_profile_name(profile, "--profile");
    }

    if let Some(profile) = read_env_string("WRAITHRUN_PROFILE")? {
        return normalize_profile_name(&profile, "WRAITHRUN_PROFILE");
    }

    Ok(None)
}

fn normalize_profile_name(profile: &str, source: &str) -> Result<Option<String>> {
    let trimmed = profile.trim();
    if trimmed.is_empty() {
        bail!("{source} cannot be empty");
    }

    Ok(Some(trimmed.to_string()))
}

fn load_config_for_cli(cli: &Cli) -> Result<(Option<FileConfig>, Option<PathBuf>)> {
    match select_config_path(cli)? {
        ConfigPathSelection::None => Ok((None, None)),
        ConfigPathSelection::Optional(path) => {
            let config = load_config_file(&path)?;
            Ok((Some(config), Some(path)))
        }
        ConfigPathSelection::Required(path) => {
            let config = load_config_file(&path)?;
            Ok((Some(config), Some(path)))
        }
    }
}

fn select_config_path(cli: &Cli) -> Result<ConfigPathSelection> {
    if let Some(path) = &cli.config {
        return Ok(ConfigPathSelection::Required(path.clone()));
    }

    if let Some(path) = read_env_path("WRAITHRUN_CONFIG")? {
        return Ok(ConfigPathSelection::Required(path));
    }

    let default_path = PathBuf::from(DEFAULT_CONFIG_FILE);
    if default_path.is_file() {
        return Ok(ConfigPathSelection::Optional(default_path));
    }

    Ok(ConfigPathSelection::None)
}

fn load_config_file(path: &Path) -> Result<FileConfig> {
    if !path.is_file() {
        bail!("Config file not found: {}", path.display());
    }

    let config_text = fs::read_to_string(path)
        .with_context(|| format!("Failed reading config {}", path.display()))?;

    toml::from_str(&config_text)
        .with_context(|| format!("Failed parsing config {}", path.display()))
}

fn lookup_profile<'a>(
    profiles: &'a HashMap<String, SettingsFragment>,
    name: &str,
) -> Option<&'a SettingsFragment> {
    profiles.get(name).or_else(|| {
        profiles
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case(name))
            .map(|(_, profile)| profile)
    })
}

const KNOWN_PROFILE_NAMES: [&str; 6] = [
    "local-lab",
    "production-triage",
    "live-model",
    "live-fast",
    "live-balanced",
    "live-deep",
];

fn builtin_profile(name: &str) -> Option<SettingsFragment> {
    match name.to_ascii_lowercase().as_str() {
        "local-lab" => Some(SettingsFragment {
            max_steps: Some(6),
            max_new_tokens: Some(192),
            temperature: Some(0.1),
            format: Some(OutputFormat::Summary),
            live: Some(false),
            ..SettingsFragment::default()
        }),
        "production-triage" => Some(SettingsFragment {
            max_steps: Some(12),
            max_new_tokens: Some(320),
            temperature: Some(0.15),
            format: Some(OutputFormat::Markdown),
            live: Some(false),
            ..SettingsFragment::default()
        }),
        "live-model" => Some(SettingsFragment {
            max_steps: Some(10),
            max_new_tokens: Some(512),
            temperature: Some(0.2),
            format: Some(OutputFormat::Json),
            live: Some(true),
            ..SettingsFragment::default()
        }),
        "live-fast" => Some(SettingsFragment {
            max_steps: Some(6),
            max_new_tokens: Some(256),
            temperature: Some(0.1),
            format: Some(OutputFormat::Json),
            live: Some(true),
            live_fallback_policy: Some(LiveFallbackPolicy::DryRunOnError),
            ..SettingsFragment::default()
        }),
        "live-balanced" => Some(SettingsFragment {
            max_steps: Some(10),
            max_new_tokens: Some(512),
            temperature: Some(0.2),
            format: Some(OutputFormat::Json),
            live: Some(true),
            live_fallback_policy: Some(LiveFallbackPolicy::DryRunOnError),
            ..SettingsFragment::default()
        }),
        "live-deep" => Some(SettingsFragment {
            max_steps: Some(16),
            max_new_tokens: Some(768),
            temperature: Some(0.25),
            format: Some(OutputFormat::Json),
            live: Some(true),
            live_fallback_policy: Some(LiveFallbackPolicy::DryRunOnError),
            ..SettingsFragment::default()
        }),
        _ => None,
    }
}

fn env_settings_fragment() -> Result<SettingsFragment> {
    Ok(SettingsFragment {
        model: read_env_path("WRAITHRUN_MODEL")?,
        tokenizer: read_env_path("WRAITHRUN_TOKENIZER")?,
        max_steps: read_env_parse("WRAITHRUN_MAX_STEPS")?,
        max_new_tokens: read_env_parse("WRAITHRUN_MAX_NEW_TOKENS")?,
        temperature: read_env_parse("WRAITHRUN_TEMPERATURE")?,
        live: read_env_bool("WRAITHRUN_LIVE")?,
        live_fallback_policy: read_env_live_fallback_policy("WRAITHRUN_LIVE_FALLBACK_POLICY")?,
        format: read_env_output_format("WRAITHRUN_FORMAT")?,
        output_mode: read_env_output_mode("WRAITHRUN_OUTPUT_MODE")?,
        automation_adapter: read_env_automation_adapter("WRAITHRUN_AUTOMATION_ADAPTER")?,
        exit_policy: read_env_exit_policy("WRAITHRUN_EXIT_POLICY")?,
        exit_threshold: read_env_exit_threshold("WRAITHRUN_EXIT_THRESHOLD")?,
        output_file: read_env_path("WRAITHRUN_OUTPUT_FILE")?,
        case_id: read_env_string("WRAITHRUN_CASE_ID")?,
        evidence_bundle_dir: read_env_path("WRAITHRUN_EVIDENCE_BUNDLE_DIR")?,
        evidence_bundle_archive: read_env_path("WRAITHRUN_EVIDENCE_BUNDLE_ARCHIVE")?,
        baseline_bundle: read_env_path("WRAITHRUN_BASELINE_BUNDLE")?,
        log: read_env_log_mode()?,
        vitis_config: read_env_string("WRAITHRUN_VITIS_CONFIG")?,
        vitis_cache_dir: read_env_string("WRAITHRUN_VITIS_CACHE_DIR")?,
        vitis_cache_key: read_env_string("WRAITHRUN_VITIS_CACHE_KEY")?,
    })
}

fn apply_cli_overrides(runtime: &mut RuntimeConfig, cli: &Cli) {
    if let Some(model) = &cli.model {
        runtime.model = model.clone();
    }
    if let Some(tokenizer) = &cli.tokenizer {
        runtime.tokenizer = Some(tokenizer.clone());
    }
    if let Some(max_steps) = cli.max_steps {
        runtime.max_steps = max_steps;
    }
    if let Some(max_new_tokens) = cli.max_new_tokens {
        runtime.max_new_tokens = max_new_tokens;
    }
    if let Some(temperature) = cli.temperature {
        runtime.temperature = temperature;
    }
    if cli.live {
        runtime.live = true;
    }
    if cli.dry_run {
        runtime.live = false;
    }
    if let Some(live_fallback_policy) = cli.live_fallback_policy {
        runtime.live_fallback_policy = live_fallback_policy;
    }
    if let Some(format) = cli.format {
        runtime.format = format;
    }
    if let Some(output_mode) = cli.output_mode {
        runtime.output_mode = output_mode;
    }
    if let Some(automation_adapter) = cli.automation_adapter {
        runtime.automation_adapter = Some(automation_adapter);
    }
    if let Some(exit_policy) = cli.exit_policy {
        runtime.exit_policy = exit_policy;
    }
    if let Some(exit_threshold) = cli.exit_threshold {
        runtime.exit_threshold = Some(exit_threshold);
    }
    if let Some(output_file) = &cli.output_file {
        runtime.output_file = Some(output_file.clone());
    }
    if let Some(case_id) = &cli.case_id {
        runtime.case_id = Some(case_id.clone());
    }
    if let Some(evidence_bundle_dir) = &cli.evidence_bundle_dir {
        runtime.evidence_bundle_dir = Some(evidence_bundle_dir.clone());
    }
    if let Some(evidence_bundle_archive) = &cli.evidence_bundle_archive {
        runtime.evidence_bundle_archive = Some(evidence_bundle_archive.clone());
    }
    if let Some(baseline_bundle) = &cli.baseline_bundle {
        runtime.baseline_bundle = Some(baseline_bundle.clone());
    }
    if cli.quiet {
        runtime.log_mode = LogMode::Quiet;
    }
    if cli.verbose {
        runtime.log_mode = LogMode::Verbose;
    }
    if let Some(vitis_config) = &cli.vitis_config {
        runtime.vitis_config = Some(vitis_config.clone());
    }
    if let Some(vitis_cache_dir) = &cli.vitis_cache_dir {
        runtime.vitis_cache_dir = Some(vitis_cache_dir.clone());
    }
    if let Some(vitis_cache_key) = &cli.vitis_cache_key {
        runtime.vitis_cache_key = Some(vitis_cache_key.clone());
    }
}

fn apply_fragment_with_source(
    runtime: &mut RuntimeConfig,
    sources: &mut RuntimeConfigSources,
    fragment: &SettingsFragment,
    source: &str,
) {
    if let Some(model) = &fragment.model {
        runtime.model = model.clone();
        sources.model = source.to_string();
    }
    if let Some(tokenizer) = &fragment.tokenizer {
        runtime.tokenizer = Some(tokenizer.clone());
        sources.tokenizer = source.to_string();
    }
    if let Some(max_steps) = fragment.max_steps {
        runtime.max_steps = max_steps;
        sources.max_steps = source.to_string();
    }
    if let Some(max_new_tokens) = fragment.max_new_tokens {
        runtime.max_new_tokens = max_new_tokens;
        sources.max_new_tokens = source.to_string();
    }
    if let Some(temperature) = fragment.temperature {
        runtime.temperature = temperature;
        sources.temperature = source.to_string();
    }
    if let Some(live) = fragment.live {
        runtime.live = live;
        sources.live = source.to_string();
    }
    if let Some(live_fallback_policy) = fragment.live_fallback_policy {
        runtime.live_fallback_policy = live_fallback_policy;
        sources.live_fallback_policy = source.to_string();
    }
    if let Some(format) = fragment.format {
        runtime.format = format;
        sources.format = source.to_string();
    }
    if let Some(output_mode) = fragment.output_mode {
        runtime.output_mode = output_mode;
        sources.output_mode = source.to_string();
    }
    if let Some(automation_adapter) = fragment.automation_adapter {
        runtime.automation_adapter = Some(automation_adapter);
        sources.automation_adapter = source.to_string();
    }
    if let Some(exit_policy) = fragment.exit_policy {
        runtime.exit_policy = exit_policy;
        sources.exit_policy = source.to_string();
    }
    if let Some(exit_threshold) = fragment.exit_threshold {
        runtime.exit_threshold = Some(exit_threshold);
        sources.exit_threshold = source.to_string();
    }
    if let Some(output_file) = &fragment.output_file {
        runtime.output_file = Some(output_file.clone());
        sources.output_file = source.to_string();
    }
    if let Some(case_id) = &fragment.case_id {
        runtime.case_id = Some(case_id.clone());
        sources.case_id = source.to_string();
    }
    if let Some(evidence_bundle_dir) = &fragment.evidence_bundle_dir {
        runtime.evidence_bundle_dir = Some(evidence_bundle_dir.clone());
        sources.evidence_bundle_dir = source.to_string();
    }
    if let Some(evidence_bundle_archive) = &fragment.evidence_bundle_archive {
        runtime.evidence_bundle_archive = Some(evidence_bundle_archive.clone());
        sources.evidence_bundle_archive = source.to_string();
    }
    if let Some(baseline_bundle) = &fragment.baseline_bundle {
        runtime.baseline_bundle = Some(baseline_bundle.clone());
        sources.baseline_bundle = source.to_string();
    }
    if let Some(log_mode) = fragment.log {
        runtime.log_mode = log_mode;
        sources.log_mode = source.to_string();
    }
    if let Some(vitis_config) = &fragment.vitis_config {
        runtime.vitis_config = Some(vitis_config.clone());
        sources.vitis_config = source.to_string();
    }
    if let Some(vitis_cache_dir) = &fragment.vitis_cache_dir {
        runtime.vitis_cache_dir = Some(vitis_cache_dir.clone());
        sources.vitis_cache_dir = source.to_string();
    }
    if let Some(vitis_cache_key) = &fragment.vitis_cache_key {
        runtime.vitis_cache_key = Some(vitis_cache_key.clone());
        sources.vitis_cache_key = source.to_string();
    }
}

fn apply_cli_overrides_with_source(
    runtime: &mut RuntimeConfig,
    sources: &mut RuntimeConfigSources,
    cli: &Cli,
) {
    if let Some(model) = &cli.model {
        runtime.model = model.clone();
        sources.model = "cli --model".to_string();
    }
    if let Some(tokenizer) = &cli.tokenizer {
        runtime.tokenizer = Some(tokenizer.clone());
        sources.tokenizer = "cli --tokenizer".to_string();
    }
    if let Some(max_steps) = cli.max_steps {
        runtime.max_steps = max_steps;
        sources.max_steps = "cli --max-steps".to_string();
    }
    if let Some(max_new_tokens) = cli.max_new_tokens {
        runtime.max_new_tokens = max_new_tokens;
        sources.max_new_tokens = "cli --max-new-tokens".to_string();
    }
    if let Some(temperature) = cli.temperature {
        runtime.temperature = temperature;
        sources.temperature = "cli --temperature".to_string();
    }
    if cli.live {
        runtime.live = true;
        sources.live = "cli --live".to_string();
    }
    if cli.dry_run {
        runtime.live = false;
        sources.live = "cli --dry-run".to_string();
    }
    if let Some(live_fallback_policy) = cli.live_fallback_policy {
        runtime.live_fallback_policy = live_fallback_policy;
        sources.live_fallback_policy = "cli --live-fallback-policy".to_string();
    }
    if let Some(format) = cli.format {
        runtime.format = format;
        sources.format = "cli --format".to_string();
    }
    if let Some(output_mode) = cli.output_mode {
        runtime.output_mode = output_mode;
        sources.output_mode = "cli --output-mode".to_string();
    }
    if let Some(automation_adapter) = cli.automation_adapter {
        runtime.automation_adapter = Some(automation_adapter);
        sources.automation_adapter = "cli --automation-adapter".to_string();
    }
    if let Some(exit_policy) = cli.exit_policy {
        runtime.exit_policy = exit_policy;
        sources.exit_policy = "cli --exit-policy".to_string();
    }
    if let Some(exit_threshold) = cli.exit_threshold {
        runtime.exit_threshold = Some(exit_threshold);
        sources.exit_threshold = "cli --exit-threshold".to_string();
    }
    if let Some(output_file) = &cli.output_file {
        runtime.output_file = Some(output_file.clone());
        sources.output_file = "cli --output-file".to_string();
    }
    if let Some(case_id) = &cli.case_id {
        runtime.case_id = Some(case_id.clone());
        sources.case_id = "cli --case-id".to_string();
    }
    if let Some(evidence_bundle_dir) = &cli.evidence_bundle_dir {
        runtime.evidence_bundle_dir = Some(evidence_bundle_dir.clone());
        sources.evidence_bundle_dir = "cli --evidence-bundle-dir".to_string();
    }
    if let Some(evidence_bundle_archive) = &cli.evidence_bundle_archive {
        runtime.evidence_bundle_archive = Some(evidence_bundle_archive.clone());
        sources.evidence_bundle_archive = "cli --evidence-bundle-archive".to_string();
    }
    if let Some(baseline_bundle) = &cli.baseline_bundle {
        runtime.baseline_bundle = Some(baseline_bundle.clone());
        sources.baseline_bundle = "cli --baseline-bundle".to_string();
    }
    if cli.quiet {
        runtime.log_mode = LogMode::Quiet;
        sources.log_mode = "cli --quiet".to_string();
    }
    if cli.verbose {
        runtime.log_mode = LogMode::Verbose;
        sources.log_mode = "cli --verbose".to_string();
    }
    if let Some(vitis_config) = &cli.vitis_config {
        runtime.vitis_config = Some(vitis_config.clone());
        sources.vitis_config = "cli --vitis-config".to_string();
    }
    if let Some(vitis_cache_dir) = &cli.vitis_cache_dir {
        runtime.vitis_cache_dir = Some(vitis_cache_dir.clone());
        sources.vitis_cache_dir = "cli --vitis-cache-dir".to_string();
    }
    if let Some(vitis_cache_key) = &cli.vitis_cache_key {
        runtime.vitis_cache_key = Some(vitis_cache_key.clone());
        sources.vitis_cache_key = "cli --vitis-cache-key".to_string();
    }
}

fn validate_runtime_config(config: &RuntimeConfig) -> Result<()> {
    if config.max_steps == 0 {
        bail!("max_steps must be at least 1");
    }
    if config.max_new_tokens == 0 {
        bail!("max_new_tokens must be at least 1");
    }
    if config.temperature.is_nan() || !(0.0..=2.0).contains(&config.temperature) {
        bail!("temperature must be between 0.0 and 2.0");
    }

    if config.automation_adapter.is_some() && config.format != OutputFormat::Json {
        bail!("automation_adapter requires JSON output format (--format json)");
    }

    if config.exit_policy == ExitPolicy::None && config.exit_threshold.is_some() {
        bail!("exit_threshold requires exit_policy=severity-threshold");
    }

    if let Some(case_id) = config.case_id.as_deref() {
        validate_case_id(case_id)?;
    }

    if let Some(path) = config.baseline_bundle.as_deref() {
        validate_baseline_bundle_path(path)?;
    }

    Ok(())
}

fn validate_case_id(case_id: &str) -> Result<()> {
    let trimmed = case_id.trim();
    if trimmed.is_empty() {
        bail!("case_id cannot be empty");
    }

    if trimmed.len() > 128 {
        bail!("case_id must be 128 characters or fewer");
    }

    if !trimmed
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.' | ':'))
    {
        bail!(
            "case_id may only contain ASCII letters, digits, and the characters '-', '_', '.', ':'"
        );
    }

    Ok(())
}

fn validate_baseline_bundle_path(path: &Path) -> Result<()> {
    if path.is_dir() {
        return Ok(());
    }

    if path.is_file() {
        let file_name = path
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("");
        if file_name.eq_ignore_ascii_case("raw_observations.json") {
            return Ok(());
        }
        bail!(
            "baseline_bundle file must be named raw_observations.json (got '{}')",
            path.display()
        );
    }

    bail!(
        "baseline_bundle path '{}' does not exist or is not accessible",
        path.display()
    )
}

fn read_env_string(name: &str) -> Result<Option<String>> {
    match std::env::var(name) {
        Ok(value) => {
            let trimmed = value.trim();
            if trimmed.is_empty() {
                bail!("{name} is set but empty");
            }
            Ok(Some(trimmed.to_string()))
        }
        Err(std::env::VarError::NotPresent) => Ok(None),
        Err(std::env::VarError::NotUnicode(_)) => {
            bail!("{name} contains non-Unicode data");
        }
    }
}

fn read_env_path(name: &str) -> Result<Option<PathBuf>> {
    read_env_string(name).map(|value| value.map(PathBuf::from))
}

fn read_env_parse<T>(name: &str) -> Result<Option<T>>
where
    T: std::str::FromStr,
    T::Err: std::fmt::Display,
{
    let Some(raw) = read_env_string(name)? else {
        return Ok(None);
    };

    let parsed = raw
        .parse::<T>()
        .map_err(|err| anyhow!("{name} has invalid value '{raw}': {err}"))?;
    Ok(Some(parsed))
}

fn read_env_bool(name: &str) -> Result<Option<bool>> {
    let Some(raw) = read_env_string(name)? else {
        return Ok(None);
    };

    parse_bool(&raw, name).map(Some)
}

fn parse_bool(raw: &str, source: &str) -> Result<bool> {
    match raw.to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Ok(true),
        "0" | "false" | "no" | "off" => Ok(false),
        _ => bail!("{source} must be one of 1/0, true/false, yes/no, on/off (got '{raw}')"),
    }
}

fn read_env_output_format(name: &str) -> Result<Option<OutputFormat>> {
    let Some(raw) = read_env_string(name)? else {
        return Ok(None);
    };

    parse_output_format(&raw, name).map(Some)
}

fn parse_output_format(raw: &str, source: &str) -> Result<OutputFormat> {
    match raw.to_ascii_lowercase().as_str() {
        "json" => Ok(OutputFormat::Json),
        "summary" => Ok(OutputFormat::Summary),
        "markdown" => Ok(OutputFormat::Markdown),
        _ => bail!("{source} must be one of: json, summary, markdown (got '{raw}')"),
    }
}

fn read_env_output_mode(name: &str) -> Result<Option<OutputMode>> {
    let Some(raw) = read_env_string(name)? else {
        return Ok(None);
    };

    parse_output_mode(&raw, name).map(Some)
}

fn parse_output_mode(raw: &str, source: &str) -> Result<OutputMode> {
    match raw.to_ascii_lowercase().as_str() {
        "compact" => Ok(OutputMode::Compact),
        "full" => Ok(OutputMode::Full),
        _ => bail!("{source} must be one of: compact, full (got '{raw}')"),
    }
}

fn read_env_automation_adapter(name: &str) -> Result<Option<AutomationAdapter>> {
    let Some(raw) = read_env_string(name)? else {
        return Ok(None);
    };

    parse_automation_adapter(&raw, name).map(Some)
}

fn parse_automation_adapter(raw: &str, source: &str) -> Result<AutomationAdapter> {
    match raw.to_ascii_lowercase().as_str() {
        "findings-v1" => Ok(AutomationAdapter::FindingsV1),
        _ => bail!("{source} must be 'findings-v1' (got '{raw}')"),
    }
}

fn read_env_live_fallback_policy(name: &str) -> Result<Option<LiveFallbackPolicy>> {
    let Some(raw) = read_env_string(name)? else {
        return Ok(None);
    };

    parse_live_fallback_policy(&raw, name).map(Some)
}

fn parse_live_fallback_policy(raw: &str, source: &str) -> Result<LiveFallbackPolicy> {
    match raw.to_ascii_lowercase().as_str() {
        "none" => Ok(LiveFallbackPolicy::None),
        "dry-run-on-error" => Ok(LiveFallbackPolicy::DryRunOnError),
        _ => bail!("{source} must be one of: none, dry-run-on-error (got '{raw}')"),
    }
}

fn read_env_exit_policy(name: &str) -> Result<Option<ExitPolicy>> {
    let Some(raw) = read_env_string(name)? else {
        return Ok(None);
    };

    parse_exit_policy(&raw, name).map(Some)
}

fn parse_exit_policy(raw: &str, source: &str) -> Result<ExitPolicy> {
    match raw.to_ascii_lowercase().as_str() {
        "none" => Ok(ExitPolicy::None),
        "severity-threshold" => Ok(ExitPolicy::SeverityThreshold),
        _ => bail!("{source} must be one of: none, severity-threshold (got '{raw}')"),
    }
}

fn read_env_exit_threshold(name: &str) -> Result<Option<ExitSeverityThreshold>> {
    let Some(raw) = read_env_string(name)? else {
        return Ok(None);
    };

    parse_exit_threshold(&raw, name).map(Some)
}

fn parse_exit_threshold(raw: &str, source: &str) -> Result<ExitSeverityThreshold> {
    match raw.to_ascii_lowercase().as_str() {
        "info" => Ok(ExitSeverityThreshold::Info),
        "low" => Ok(ExitSeverityThreshold::Low),
        "medium" => Ok(ExitSeverityThreshold::Medium),
        "high" => Ok(ExitSeverityThreshold::High),
        "critical" => Ok(ExitSeverityThreshold::Critical),
        _ => bail!("{source} must be one of: info, low, medium, high, critical (got '{raw}')"),
    }
}

fn read_env_log_mode() -> Result<Option<LogMode>> {
    if let Some(raw) = read_env_string("WRAITHRUN_LOG")? {
        return parse_log_mode(&raw, "WRAITHRUN_LOG").map(Some);
    }

    let quiet = read_env_bool("WRAITHRUN_QUIET")?;
    let verbose = read_env_bool("WRAITHRUN_VERBOSE")?;

    if quiet == Some(true) && verbose == Some(true) {
        bail!("WRAITHRUN_QUIET and WRAITHRUN_VERBOSE cannot both be true");
    }
    if quiet == Some(true) {
        return Ok(Some(LogMode::Quiet));
    }
    if verbose == Some(true) {
        return Ok(Some(LogMode::Verbose));
    }
    if quiet.is_some() || verbose.is_some() {
        return Ok(Some(LogMode::Normal));
    }

    Ok(None)
}

fn parse_log_mode(raw: &str, source: &str) -> Result<LogMode> {
    match raw.to_ascii_lowercase().as_str() {
        "quiet" => Ok(LogMode::Quiet),
        "normal" => Ok(LogMode::Normal),
        "verbose" => Ok(LogMode::Verbose),
        _ => bail!("{source} must be one of: quiet, normal, verbose (got '{raw}')"),
    }
}

fn ensure_exclusive_modes(cli: &Cli) -> Result<()> {
    let mut selected = Vec::new();
    if cli.doctor {
        selected.push("--doctor");
    }
    if cli.list_task_templates {
        selected.push("--list-task-templates");
    }
    if cli.list_tools {
        selected.push("--list-tools");
    }
    if cli.describe_tool.is_some() {
        selected.push("--describe-tool");
    }
    if cli.list_profiles {
        selected.push("--list-profiles");
    }
    if cli.print_effective_config {
        selected.push("--print-effective-config");
    }
    if cli.explain_effective_config {
        selected.push("--explain-effective-config");
    }
    if cli.init_config {
        selected.push("--init-config");
    }
    if cli.verify_bundle.is_some() {
        selected.push("--verify-bundle");
    }
    if cli.live_setup {
        selected.push("--live-setup");
    }
    if cli.models_list {
        selected.push("--models-list");
    }
    if cli.models_validate {
        selected.push("--models-validate");
    }
    if cli.models_benchmark {
        selected.push("--models-benchmark");
    }

    if selected.len() > 1 {
        bail!(
            "Options {} are mutually exclusive; choose only one mode.",
            selected.join(", ")
        );
    }

    Ok(())
}

fn ensure_introspection_format_usage(cli: &Cli) -> Result<()> {
    if cli.introspection_format == IntrospectionFormat::Json
        && !(cli.doctor
            || cli.list_task_templates
            || cli.list_tools
            || cli.describe_tool.is_some()
            || cli.list_profiles
            || cli.verify_bundle.is_some()
            || cli.models_list
            || cli.models_validate
            || cli.models_benchmark)
    {
        bail!(
            "--introspection-format only applies to --doctor, --list-task-templates, --list-tools, --describe-tool, --list-profiles, --verify-bundle, --models-list, --models-validate, or --models-benchmark"
        );
    }

    Ok(())
}

fn run_init_config(cli: &Cli) -> Result<String> {
    let target_path = resolve_init_config_path(cli);

    if target_path.exists() && !cli.force {
        bail!(
            "Config file already exists at '{}'. Use --force to overwrite.",
            target_path.display()
        );
    }

    if let Some(parent) = target_path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)
                .with_context(|| format!("Failed creating directory {}", parent.display()))?;
        }
    }

    fs::write(&target_path, DEFAULT_CONFIG_TEMPLATE.as_bytes())
        .with_context(|| format!("Failed writing config file {}", target_path.display()))?;

    Ok(format!(
        "Wrote starter config to {}\nNext: run 'wraithrun --list-profiles --config {}' or 'wraithrun --print-effective-config --config {}'.",
        target_path.display(),
        target_path.display(),
        target_path.display()
    ))
}

fn resolve_init_config_path(cli: &Cli) -> PathBuf {
    cli.init_config_path
        .clone()
        .unwrap_or_else(|| PathBuf::from(DEFAULT_CONFIG_FILE))
}

fn run_list_profiles(cli: &Cli, format: IntrospectionFormat) -> Result<String> {
    let selected_profile = resolve_profile_name(cli)?;
    let (file_config, config_path) = load_config_for_cli(cli)?;

    match format {
        IntrospectionFormat::Text => Ok(render_profile_list(
            selected_profile.as_deref(),
            config_path.as_deref(),
            file_config.as_ref(),
        )),
        IntrospectionFormat::Json => render_profile_list_json(
            selected_profile.as_deref(),
            config_path.as_deref(),
            file_config.as_ref(),
        ),
    }
}

fn run_models_list(cli: &Cli, format: IntrospectionFormat) -> Result<String> {
    let candidates = collect_model_pack_candidates(cli)?;
    let mut packs = Vec::with_capacity(candidates.len());

    for candidate in &candidates {
        let mut report = DoctorReport::default();
        run_model_pack_doctor_checks(&candidate.runtime, &mut report);
        packs.push(build_model_pack_view(candidate, &report));
    }

    match format {
        IntrospectionFormat::Text => Ok(render_model_pack_list(&packs)),
        IntrospectionFormat::Json => render_json_with_contract(&ModelPackListView { packs }),
    }
}

fn run_models_validate(
    cli: &Cli,
    format: IntrospectionFormat,
) -> Result<ModelPackValidationOutcome> {
    let candidates = collect_model_pack_candidates(cli)?;
    let mut packs = Vec::with_capacity(candidates.len());
    let mut total_pass = 0usize;
    let mut total_warn = 0usize;
    let mut total_fail = 0usize;
    let mut has_failures = false;

    for candidate in &candidates {
        let mut report = DoctorReport::default();
        run_model_pack_doctor_checks(&candidate.runtime, &mut report);
        let (pass_count, warn_count, fail_count) = report.counts();

        total_pass += pass_count;
        total_warn += warn_count;
        total_fail += fail_count;
        if fail_count > 0 {
            has_failures = true;
        }

        let pack = build_model_pack_view(candidate, &report);
        let checks = report
            .checks
            .iter()
            .map(|check| ModelPackValidationCheckView {
                status: check.status,
                name: check.name,
                detail: check.detail.clone(),
                reason_code: check.reason_code.map(|code| code.to_string()),
                remediation: check.remediation,
            })
            .collect();

        packs.push(ModelPackValidationPackView {
            pack,
            summary: DoctorSummaryView {
                pass: pass_count,
                warn: warn_count,
                fail: fail_count,
            },
            checks,
        });
    }

    let summary = DoctorSummaryView {
        pass: total_pass,
        warn: total_warn,
        fail: total_fail,
    };

    let rendered = match format {
        IntrospectionFormat::Text => render_model_pack_validation(&summary, &packs),
        IntrospectionFormat::Json => render_json_with_contract(&ModelPackValidationView {
            summary: summary.clone(),
            packs,
        })?,
    };

    Ok(ModelPackValidationOutcome {
        rendered,
        has_failures,
    })
}

fn run_models_benchmark(cli: &Cli, format: IntrospectionFormat) -> Result<String> {
    let candidates = collect_model_pack_candidates(cli)?;
    let mut packs = Vec::with_capacity(candidates.len());

    for candidate in &candidates {
        let mut report = DoctorReport::default();
        run_model_pack_doctor_checks(&candidate.runtime, &mut report);
        let pack = build_model_pack_view(candidate, &report);

        let estimated_token_budget = candidate
            .runtime
            .max_steps
            .saturating_mul(candidate.runtime.max_new_tokens);
        let latency_tier = benchmark_latency_tier(estimated_token_budget);
        let benchmark_score = benchmark_score(candidate, &pack, estimated_token_budget);

        packs.push(ModelPackBenchmarkEntry {
            pack,
            estimated_token_budget,
            latency_tier,
            benchmark_score,
        });
    }

    packs.sort_by(|left, right| {
        right
            .benchmark_score
            .partial_cmp(&left.benchmark_score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| {
                left.estimated_token_budget
                    .cmp(&right.estimated_token_budget)
            })
            .then_with(|| left.pack.name.cmp(&right.pack.name))
    });

    let recommended = packs
        .iter()
        .find(|entry| entry.pack.source == "preset")
        .unwrap_or_else(|| {
            packs
                .first()
                .expect("model benchmark requires at least one pack")
        });

    let rationale = format!(
        "'{}' ranked highest for estimated responsiveness with budget {} and readiness {:?}.",
        recommended.pack.name, recommended.estimated_token_budget, recommended.pack.readiness
    );

    let view = ModelPackBenchmarkView {
        recommended_profile: recommended.pack.name.clone(),
        rationale,
        packs,
    };

    match format {
        IntrospectionFormat::Text => Ok(render_model_pack_benchmark(&view)),
        IntrospectionFormat::Json => render_json_with_contract(&view),
    }
}

fn collect_model_pack_candidates(cli: &Cli) -> Result<Vec<ModelPackCandidate>> {
    let selected_profile = resolve_profile_name(cli)?;
    let (file_config, file_config_path) = load_config_for_cli(cli)?;
    let env_overrides = env_settings_fragment()?;
    let task = resolve_task_for_mode(cli, "models-mode")?;

    let mut names: Vec<String> = Vec::new();
    if let Some(profile_name) = selected_profile.as_ref() {
        names.push(profile_name.clone());
    } else {
        names.extend(
            LIVE_PRESET_PROFILE_NAMES
                .iter()
                .map(|name| (*name).to_string()),
        );
        names.push("live-model".to_string());

        if let Some(config) = file_config.as_ref() {
            let mut configured_names: Vec<String> = config.profiles.keys().cloned().collect();
            configured_names.sort_unstable();
            names.extend(configured_names);
        }
    }

    let mut deduped = Vec::new();
    for name in names {
        if !deduped
            .iter()
            .any(|existing: &String| existing.eq_ignore_ascii_case(&name))
        {
            deduped.push(name);
        }
    }

    let mut packs = Vec::new();
    for profile_name in deduped {
        let mut scoped_cli = cli.clone();
        scoped_cli.profile = Some(profile_name.clone());
        scoped_cli.live = false;
        scoped_cli.dry_run = false;

        let runtime = merge_sources(
            &scoped_cli,
            task.clone(),
            Some(profile_name.clone()),
            file_config.as_ref(),
            file_config_path.as_deref(),
            &env_overrides,
        )?;

        if !runtime.live {
            continue;
        }

        let source = if is_live_preset_profile(&profile_name) {
            "preset".to_string()
        } else {
            selected_profile_source(&profile_name, file_config.as_ref()).to_string()
        };

        packs.push(ModelPackCandidate {
            name: profile_name,
            source,
            runtime,
        });
    }

    if packs.is_empty() {
        if let Some(profile_name) = selected_profile {
            bail!(
                "Profile '{profile_name}' did not resolve to a live model pack. Choose a live profile or preset (live-fast, live-balanced, live-deep)."
            );
        }

        bail!(
            "No live model packs were discovered. Configure a live profile or use built-in presets: live-fast, live-balanced, live-deep."
        );
    }

    Ok(packs)
}

fn is_live_preset_profile(name: &str) -> bool {
    LIVE_PRESET_PROFILE_NAMES
        .iter()
        .any(|preset| preset.eq_ignore_ascii_case(name))
}

fn build_model_pack_view(candidate: &ModelPackCandidate, report: &DoctorReport) -> ModelPackView {
    let (_, warn_count, fail_count) = report.counts();
    let readiness = if fail_count > 0 {
        DoctorStatus::Fail
    } else if warn_count > 0 {
        DoctorStatus::Warn
    } else {
        DoctorStatus::Pass
    };

    ModelPackView {
        name: candidate.name.clone(),
        source: candidate.source.clone(),
        model: candidate.runtime.model.display().to_string(),
        tokenizer: candidate
            .runtime
            .tokenizer
            .as_ref()
            .map(|path| path.display().to_string()),
        max_steps: candidate.runtime.max_steps,
        max_new_tokens: candidate.runtime.max_new_tokens,
        temperature: candidate.runtime.temperature,
        live_fallback_policy: candidate.runtime.live_fallback_policy,
        readiness,
        warn_count,
        fail_count,
    }
}

fn render_model_pack_list(packs: &[ModelPackView]) -> String {
    let mut output = String::new();
    let _ = writeln!(output, "WraithRun Model Packs");

    for pack in packs {
        let tokenizer = pack.tokenizer.as_deref().unwrap_or("(auto-discovery)");
        let _ = writeln!(
            output,
            "- {} [{}]: readiness={} (warn={}, fail={}), steps={}, max_new_tokens={}, temp={:.2}, fallback={}, model={}, tokenizer={}",
            pack.name,
            pack.source,
            pack.readiness.label(),
            pack.warn_count,
            pack.fail_count,
            pack.max_steps,
            pack.max_new_tokens,
            pack.temperature,
            live_fallback_policy_token(pack.live_fallback_policy),
            pack.model,
            tokenizer
        );
    }

    output.trim_end().to_string()
}

fn render_model_pack_validation(
    summary: &DoctorSummaryView,
    packs: &[ModelPackValidationPackView],
) -> String {
    let mut output = String::new();
    let _ = writeln!(output, "WraithRun Model Pack Validation");
    let _ = writeln!(
        output,
        "Summary: {} pass, {} warn, {} fail",
        summary.pass, summary.warn, summary.fail
    );

    for pack in packs {
        let _ = writeln!(
            output,
            "\n{} [{}] => {} pass, {} warn, {} fail",
            pack.pack.name,
            pack.pack.source,
            pack.summary.pass,
            pack.summary.warn,
            pack.summary.fail
        );

        for check in &pack.checks {
            if let Some(reason_code) = check.reason_code.as_deref() {
                let _ = writeln!(
                    output,
                    "  - [{}] {} [{}]: {}",
                    check.status.label(),
                    check.name,
                    reason_code,
                    check.detail
                );
            } else {
                let _ = writeln!(
                    output,
                    "  - [{}] {}: {}",
                    check.status.label(),
                    check.name,
                    check.detail
                );
            }
        }
    }

    output.trim_end().to_string()
}

fn render_model_pack_benchmark(view: &ModelPackBenchmarkView) -> String {
    let mut output = String::new();
    let _ = writeln!(output, "WraithRun Model Pack Benchmark");
    let _ = writeln!(output, "Recommended preset: {}", view.recommended_profile);
    let _ = writeln!(output, "Rationale: {}", view.rationale);

    for (idx, entry) in view.packs.iter().enumerate() {
        let _ = writeln!(
            output,
            "{}. {} [{}] score={:.2}, tier={}, budget={}, readiness={}, steps={}, max_new_tokens={}, temp={:.2}",
            idx + 1,
            entry.pack.name,
            entry.pack.source,
            entry.benchmark_score,
            entry.latency_tier,
            entry.estimated_token_budget,
            entry.pack.readiness.label(),
            entry.pack.max_steps,
            entry.pack.max_new_tokens,
            entry.pack.temperature
        );
    }

    output.trim_end().to_string()
}

fn benchmark_latency_tier(estimated_token_budget: usize) -> &'static str {
    match estimated_token_budget {
        0..=2048 => "fast",
        2049..=8192 => "balanced",
        _ => "deep",
    }
}

fn benchmark_score(
    candidate: &ModelPackCandidate,
    pack: &ModelPackView,
    estimated_token_budget: usize,
) -> f32 {
    let budget = estimated_token_budget.max(1) as f32;
    let fallback_multiplier = match candidate.runtime.live_fallback_policy {
        LiveFallbackPolicy::None => 0.95,
        LiveFallbackPolicy::DryRunOnError => 1.05,
    };
    let readiness_multiplier = match pack.readiness {
        DoctorStatus::Pass => 1.0,
        DoctorStatus::Warn => 0.85,
        DoctorStatus::Fail => 0.65,
    };

    let score = (10_000.0 / budget) * fallback_multiplier * readiness_multiplier;
    (score * 100.0).round() / 100.0
}

fn builtin_profile_summaries() -> Vec<ProfileSummaryView> {
    vec![
        ProfileSummaryView {
            name: "local-lab",
            description: "dry-run, compact step/token budget, summary output",
        },
        ProfileSummaryView {
            name: "production-triage",
            description: "dry-run, deeper loops, markdown output",
        },
        ProfileSummaryView {
            name: "live-model",
            description: "live inference enabled, larger token budget",
        },
        ProfileSummaryView {
            name: "live-fast",
            description: "live preset focused on responsiveness and lower token budget",
        },
        ProfileSummaryView {
            name: "live-balanced",
            description: "live preset balancing response depth and latency",
        },
        ProfileSummaryView {
            name: "live-deep",
            description: "live preset for deeper investigations with larger budget",
        },
    ]
}

fn selected_profile_source(
    selected_profile: &str,
    file_config: Option<&FileConfig>,
) -> &'static str {
    let is_builtin = builtin_profile(selected_profile).is_some();
    let is_in_config = file_config
        .and_then(|config| lookup_profile(&config.profiles, selected_profile))
        .is_some();

    if is_builtin && is_in_config {
        "built-in+config"
    } else if is_builtin {
        "built-in"
    } else if is_in_config {
        "config"
    } else {
        "missing"
    }
}

fn render_profile_list(
    selected_profile: Option<&str>,
    config_path: Option<&Path>,
    file_config: Option<&FileConfig>,
) -> String {
    let mut output = String::new();

    let _ = writeln!(output, "WraithRun Profiles");
    let _ = writeln!(output, "Built-in profiles:");
    for summary in builtin_profile_summaries() {
        let _ = writeln!(output, "- {}: {}", summary.name, summary.description);
    }

    match config_path {
        Some(path) => {
            let _ = writeln!(output, "Config file: {}", path.display());
        }
        None => {
            let _ = writeln!(output, "Config file: none detected");
        }
    }

    let mut profile_names = file_config
        .map(|config| config.profiles.keys().cloned().collect::<Vec<_>>())
        .unwrap_or_default();
    profile_names.sort_unstable();

    let _ = writeln!(output, "Config-defined profiles:");
    if profile_names.is_empty() {
        let _ = writeln!(output, "- (none)");
    } else {
        for profile in &profile_names {
            let _ = writeln!(output, "- {profile}");
        }
    }

    if let Some(profile_name) = selected_profile {
        let _ = writeln!(output, "Selected profile: {profile_name}");
        match selected_profile_source(profile_name, file_config) {
            "built-in+config" => {
                let _ = writeln!(
                    output,
                    "Profile source: built-in and config (config overrides overlapping keys)"
                );
            }
            "built-in" => {
                let _ = writeln!(output, "Profile source: built-in");
            }
            "config" => {
                let _ = writeln!(output, "Profile source: config");
            }
            _ => {
                let _ = writeln!(
                    output,
                    "Profile source: missing (not found in built-ins or config)"
                );
            }
        }
    }

    output.trim_end().to_string()
}

fn render_profile_list_json(
    selected_profile: Option<&str>,
    config_path: Option<&Path>,
    file_config: Option<&FileConfig>,
) -> Result<String> {
    let mut profile_names = file_config
        .map(|config| config.profiles.keys().cloned().collect::<Vec<_>>())
        .unwrap_or_default();
    profile_names.sort_unstable();

    let selected = selected_profile.map(|name| SelectedProfileView {
        name: name.to_string(),
        source: selected_profile_source(name, file_config),
    });

    let view = ProfileListView {
        built_in_profiles: builtin_profile_summaries(),
        config_path: config_path.map(|path| path.display().to_string()),
        config_profiles: profile_names,
        selected_profile: selected,
    };

    render_json_with_contract(&view)
}

fn render_effective_config_json(runtime: &RuntimeConfig) -> Result<String> {
    render_json_with_contract(&RuntimeConfigView::from_runtime(runtime))
}

fn render_effective_config_explanation_json(
    explanation: &EffectiveConfigExplanationView,
) -> Result<String> {
    render_json_with_contract(explanation)
}

fn render_json_with_contract<T: Serialize>(view: &T) -> Result<String> {
    let mut value = serde_json::to_value(view).map_err(|err| anyhow!(err))?;
    let Some(object) = value.as_object_mut() else {
        bail!("JSON contract payload must serialize to an object");
    };

    object.insert(
        "contract_version".to_string(),
        Value::String(JSON_CONTRACT_VERSION.to_string()),
    );

    serde_json::to_string_pretty(&value).map_err(|err| anyhow!(err))
}

impl RuntimeConfigView {
    fn from_runtime(runtime: &RuntimeConfig) -> Self {
        Self {
            task: runtime.task.clone(),
            mode: if runtime.live { "live" } else { "dry-run" },
            live: runtime.live,
            live_fallback_policy: runtime.live_fallback_policy,
            model: runtime.model.display().to_string(),
            tokenizer: runtime
                .tokenizer
                .as_ref()
                .map(|path| path.display().to_string()),
            max_steps: runtime.max_steps,
            max_new_tokens: runtime.max_new_tokens,
            temperature: runtime.temperature,
            format: runtime.format,
            automation_adapter: runtime.automation_adapter,
            exit_policy: runtime.exit_policy,
            exit_threshold: runtime.exit_threshold,
            output_file: runtime
                .output_file
                .as_ref()
                .map(|path| path.display().to_string()),
            case_id: runtime.case_id.clone(),
            evidence_bundle_dir: runtime
                .evidence_bundle_dir
                .as_ref()
                .map(|path| path.display().to_string()),
            evidence_bundle_archive: runtime
                .evidence_bundle_archive
                .as_ref()
                .map(|path| path.display().to_string()),
            baseline_bundle: runtime
                .baseline_bundle
                .as_ref()
                .map(|path| path.display().to_string()),
            log_mode: runtime.log_mode,
            vitis_config: runtime.vitis_config.clone(),
            vitis_cache_dir: runtime.vitis_cache_dir.clone(),
            vitis_cache_key: runtime.vitis_cache_key.clone(),
        }
    }
}

fn run_doctor(cli: &Cli) -> DoctorReport {
    let mut report = DoctorReport::default();

    let profile = match resolve_profile_name(cli) {
        Ok(profile) => {
            if let Some(profile_name) = profile.as_deref() {
                report.push(
                    DoctorStatus::Pass,
                    "profile-selection",
                    format!("Selected profile: {profile_name}"),
                );
            } else {
                report.push(
                    DoctorStatus::Warn,
                    "profile-selection",
                    "No profile selected; defaults/config/env/CLI values will be used.",
                );
            }
            profile
        }
        Err(err) => {
            report.push(
                DoctorStatus::Fail,
                "profile-selection",
                format!("Unable to resolve profile: {err}"),
            );
            None
        }
    };

    let selection = match select_config_path(cli) {
        Ok(selection) => Some(selection),
        Err(err) => {
            report.push(
                DoctorStatus::Fail,
                "config-selection",
                format!("Unable to resolve config path: {err}"),
            );
            None
        }
    };

    let mut file_config: Option<FileConfig> = None;
    let mut file_config_path: Option<PathBuf> = None;

    if let Some(selection) = selection {
        match selection {
            ConfigPathSelection::None => {
                report.push(
                    DoctorStatus::Warn,
                    "config-file",
                    "No config file detected (checked --config, WRAITHRUN_CONFIG, and ./wraithrun.toml).",
                );
            }
            ConfigPathSelection::Optional(path) | ConfigPathSelection::Required(path) => {
                file_config_path = Some(path.clone());
                match load_config_file(&path) {
                    Ok(config) => {
                        report.push(
                            DoctorStatus::Pass,
                            "config-file",
                            format!("Loaded config: {}", path.display()),
                        );
                        file_config = Some(config);
                    }
                    Err(err) => {
                        report.push(
                            DoctorStatus::Fail,
                            "config-file",
                            format!("Failed to load config '{}': {err}", path.display()),
                        );
                    }
                }
            }
        }
    }

    if let Some(profile_name) = profile.as_deref() {
        let is_builtin = builtin_profile(profile_name).is_some();
        let is_in_file = file_config
            .as_ref()
            .and_then(|cfg| lookup_profile(&cfg.profiles, profile_name))
            .is_some();

        if is_builtin && is_in_file {
            report.push(
                DoctorStatus::Pass,
                "profile-availability",
                format!(
                    "Profile '{profile_name}' found in built-ins and config file; config profile overrides overlapping keys."
                ),
            );
        } else if is_builtin {
            report.push(
                DoctorStatus::Pass,
                "profile-availability",
                format!("Profile '{profile_name}' found in built-in profiles."),
            );
        } else if is_in_file {
            report.push(
                DoctorStatus::Pass,
                "profile-availability",
                format!("Profile '{profile_name}' found in config profiles."),
            );
        } else {
            report.push(
                DoctorStatus::Fail,
                "profile-availability",
                format!(
                    "Profile '{profile_name}' is not available in built-in profiles ({}) or loaded config profiles.",
                    KNOWN_PROFILE_NAMES.join(", ")
                ),
            );
        }
    }

    let env_overrides = match env_settings_fragment() {
        Ok(overrides) => {
            report.push(
                DoctorStatus::Pass,
                "environment-overrides",
                "Environment overrides parsed successfully.",
            );
            Some(overrides)
        }
        Err(err) => {
            report.push(
                DoctorStatus::Fail,
                "environment-overrides",
                format!("Invalid environment variable value: {err}"),
            );
            None
        }
    };

    if let Some(env_overrides) = env_overrides.as_ref() {
        let doctor_task = cli
            .task
            .clone()
            .unwrap_or_else(|| "doctor-self-check".to_string());
        match merge_sources(
            cli,
            doctor_task,
            profile,
            file_config.as_ref(),
            file_config_path.as_deref(),
            env_overrides,
        ) {
            Ok(mut runtime) => {
                if cli.fix {
                    apply_doctor_live_fix_handlers(cli, &mut runtime, &mut report);
                }

                let mode = if runtime.live { "live" } else { "dry-run" };
                report.push(
                    DoctorStatus::Pass,
                    "effective-runtime",
                    format!(
                        "Resolved mode={mode}, model='{}', max_steps={}, max_new_tokens={}, format={:?}.",
                        runtime.model.display(),
                        runtime.max_steps,
                        runtime.max_new_tokens,
                        runtime.format
                    ),
                );

                run_model_pack_doctor_checks(&runtime, &mut report);

                if let Some(vitis_config) = runtime.vitis_config.as_ref() {
                    let path = PathBuf::from(vitis_config);
                    if path.is_file() {
                        report.push(
                            DoctorStatus::Pass,
                            "vitis-config",
                            format!("Vitis config file found: {}", path.display()),
                        );
                    } else {
                        report.push(
                            DoctorStatus::Warn,
                            "vitis-config",
                            format!(
                                "Vitis config is set but file was not found: {}",
                                path.display()
                            ),
                        );
                    }
                }
            }
            Err(err) => {
                report.push(
                    DoctorStatus::Fail,
                    "effective-runtime",
                    format!("Unable to resolve effective runtime settings: {err}"),
                );
            }
        }
    }

    report
}

fn apply_doctor_live_fix_handlers(
    cli: &Cli,
    runtime: &mut RuntimeConfig,
    report: &mut DoctorReport,
) {
    if !runtime.live {
        report.push_with_reason(
            DoctorStatus::Warn,
            "doctor-fix",
            "--fix was requested, but live mode is disabled in effective settings. Re-run with --doctor --live --fix to apply live remediation handlers.",
            Some("fix_requires_live_mode"),
        );
        return;
    }

    if runtime.live_fallback_policy == LiveFallbackPolicy::None {
        runtime.live_fallback_policy = LiveFallbackPolicy::DryRunOnError;
        report.push_with_reason(
            DoctorStatus::Pass,
            "fix-live-fallback-policy",
            "Set live fallback policy to dry-run-on-error so live failures preserve operator workflow continuity.",
            Some("fallback_policy_auto_enabled"),
        );
    } else {
        report.push(
            DoctorStatus::Pass,
            "fix-live-fallback-policy",
            format!(
                "Live fallback policy already set to '{}'.",
                live_fallback_policy_token(runtime.live_fallback_policy)
            ),
        );
    }

    if !runtime.model.is_file() {
        if cli.model.is_some() {
            report.push_with_reason(
                DoctorStatus::Warn,
                "fix-live-model-path",
                format!(
                    "Explicit --model path was not found: {}. Update --model to a readable .onnx file.",
                    runtime.model.display()
                ),
                Some("model_path_explicit_invalid"),
            );
        } else {
            let discovered = discover_model_path_near(&runtime.model).or_else(discover_model_path);
            if let Some(path) = discovered {
                runtime.model = path.clone();
                report.push_with_reason(
                    DoctorStatus::Pass,
                    "fix-live-model-path",
                    format!("Auto-discovered model path: {}", path.display()),
                    Some("model_path_auto_discovered"),
                );
            } else {
                report.push_with_reason(
                    DoctorStatus::Warn,
                    "fix-live-model-path",
                    "No fallback model candidate was discovered. Place a .onnx file under ./models or pass --model <PATH>.",
                    Some("model_path_discovery_failed"),
                );
            }
        }
    } else if !is_onnx_path(&runtime.model) {
        if cli.model.is_some() {
            report.push_with_reason(
                DoctorStatus::Warn,
                "fix-live-model-path",
                format!(
                    "Explicit --model path is not an ONNX file: {}. Provide a .onnx model file.",
                    runtime.model.display()
                ),
                Some("model_format_explicit_non_onnx"),
            );
        } else if let Some(candidate) = discover_model_path_near(&runtime.model) {
            if candidate != runtime.model {
                runtime.model = candidate.clone();
                report.push_with_reason(
                    DoctorStatus::Pass,
                    "fix-live-model-path",
                    format!(
                        "Switched to nearby ONNX model candidate: {}",
                        candidate.display()
                    ),
                    Some("model_path_auto_corrected"),
                );
            }
        }
    }

    if runtime.model.is_file() {
        match fs::File::open(&runtime.model) {
            Ok(_) => {
                report.push(
                    DoctorStatus::Pass,
                    "fix-live-model-permissions",
                    format!("Model file is readable: {}", runtime.model.display()),
                );
            }
            Err(err) => {
                let reason_code = if err.kind() == std::io::ErrorKind::PermissionDenied {
                    "model_permission_denied"
                } else {
                    "model_read_failed"
                };
                report.push_with_reason(
                    DoctorStatus::Warn,
                    "fix-live-model-permissions",
                    format!(
                        "Unable to read model file '{}': {err}. Ensure the account running WraithRun has read permission.",
                        runtime.model.display()
                    ),
                    Some(reason_code),
                );
            }
        }
    }

    let tokenizer_state = runtime.tokenizer.as_ref().map(|path| {
        if !path.is_file() {
            Err("tokenizer_path_missing")
        } else {
            tokenizer_json_health(path)
        }
    });
    let tokenizer_needs_fix = runtime.tokenizer.is_none()
        || tokenizer_state
            .as_ref()
            .map(|state| state.is_err())
            .unwrap_or(false);

    if !tokenizer_needs_fix {
        if let Some(path) = runtime.tokenizer.as_ref() {
            report.push(
                DoctorStatus::Pass,
                "fix-live-tokenizer-path",
                format!("Tokenizer already valid: {}", path.display()),
            );
        }
        return;
    }

    if cli.tokenizer.is_some() {
        let reason_code = tokenizer_state
            .as_ref()
            .and_then(|state| state.as_ref().err().copied())
            .unwrap_or("tokenizer_path_explicit_invalid");
        let detail = match runtime.tokenizer.as_ref() {
            Some(path) => format!(
                "Explicit --tokenizer path requires manual correction: {}. Update --tokenizer to a readable tokenizer JSON with a top-level model key.",
                path.display()
            ),
            None => "Explicit --tokenizer value was provided but could not be resolved; pass a readable tokenizer JSON path.".to_string(),
        };
        report.push_with_reason(
            DoctorStatus::Warn,
            "fix-live-tokenizer-path",
            detail,
            Some(reason_code),
        );
        return;
    }

    let exclude = runtime.tokenizer.as_deref();
    if let Some(path) = discover_tokenizer_path_with_validation(&runtime.model, exclude) {
        runtime.tokenizer = Some(path.clone());
        report.push_with_reason(
            DoctorStatus::Pass,
            "fix-live-tokenizer-path",
            format!("Auto-discovered a valid tokenizer JSON: {}", path.display()),
            Some("tokenizer_path_auto_discovered"),
        );
        return;
    }

    report.push_with_reason(
        DoctorStatus::Warn,
        "fix-live-tokenizer-path",
        "No valid tokenizer candidate was discovered. Add tokenizer.json beside the model (or under ./models) and retry.",
        Some("tokenizer_discovery_failed"),
    );
}

fn run_model_pack_doctor_checks(runtime: &RuntimeConfig, report: &mut DoctorReport) {
    if !runtime.live {
        return;
    }

    let mut model_readable = false;

    if runtime.model.is_file() {
        report.push(
            DoctorStatus::Pass,
            "live-model-path",
            format!("Model file found: {}", runtime.model.display()),
        );

        if is_onnx_path(&runtime.model) {
            report.push(
                DoctorStatus::Pass,
                "live-model-format",
                format!("Model extension is .onnx: {}", runtime.model.display()),
            );
        } else {
            report.push_with_reason(
                DoctorStatus::Warn,
                "live-model-format",
                format!(
                    "Model file extension is not .onnx (path: {}).",
                    runtime.model.display()
                ),
                Some("model_format_non_onnx"),
            );
        }

        match fs::metadata(&runtime.model) {
            Ok(metadata) if metadata.len() > 0 => {
                report.push(
                    DoctorStatus::Pass,
                    "live-model-size",
                    format!("Model file size: {} bytes", metadata.len()),
                );
            }
            Ok(_) => {
                report.push_with_reason(
                    DoctorStatus::Fail,
                    "live-model-size",
                    format!("Model file is empty: {}", runtime.model.display()),
                    Some("model_file_empty"),
                );
            }
            Err(err) => {
                let reason_code = if err.kind() == std::io::ErrorKind::PermissionDenied {
                    "model_metadata_permission_denied"
                } else {
                    "model_metadata_unreadable"
                };
                report.push_with_reason(
                    DoctorStatus::Fail,
                    "live-model-size",
                    format!(
                        "Unable to read model metadata '{}': {err}",
                        runtime.model.display()
                    ),
                    Some(reason_code),
                );
            }
        }

        match fs::File::open(&runtime.model) {
            Ok(_) => {
                model_readable = true;
                report.push(
                    DoctorStatus::Pass,
                    "live-model-readable",
                    format!("Model file is readable: {}", runtime.model.display()),
                );
            }
            Err(err) => {
                let reason_code = if err.kind() == std::io::ErrorKind::PermissionDenied {
                    "model_permission_denied"
                } else {
                    "model_read_failed"
                };
                report.push_with_reason(
                    DoctorStatus::Fail,
                    "live-model-readable",
                    format!(
                        "Unable to open model file '{}': {err}",
                        runtime.model.display()
                    ),
                    Some(reason_code),
                );
            }
        }
    } else {
        report.push_with_reason(
            DoctorStatus::Fail,
            "live-model-path",
            format!(
                "Live mode is enabled but model file was not found at {}.",
                runtime.model.display()
            ),
            Some("model_path_missing"),
        );
    }

    match runtime.tokenizer.as_ref() {
        Some(tokenizer) if tokenizer.is_file() => {
            report.push(
                DoctorStatus::Pass,
                "live-tokenizer-path",
                format!("Tokenizer file found: {}", tokenizer.display()),
            );

            match fs::metadata(tokenizer) {
                Ok(metadata) if metadata.len() > 0 => {
                    report.push(
                        DoctorStatus::Pass,
                        "live-tokenizer-size",
                        format!("Tokenizer file size: {} bytes", metadata.len()),
                    );
                }
                Ok(_) => {
                    report.push_with_reason(
                        DoctorStatus::Fail,
                        "live-tokenizer-size",
                        format!("Tokenizer file is empty: {}", tokenizer.display()),
                        Some("tokenizer_file_empty"),
                    );
                    return;
                }
                Err(err) => {
                    let reason_code = if err.kind() == std::io::ErrorKind::PermissionDenied {
                        "tokenizer_metadata_permission_denied"
                    } else {
                        "tokenizer_metadata_unreadable"
                    };
                    report.push_with_reason(
                        DoctorStatus::Fail,
                        "live-tokenizer-size",
                        format!(
                            "Unable to read tokenizer metadata '{}': {err}",
                            tokenizer.display()
                        ),
                        Some(reason_code),
                    );
                    return;
                }
            }

            match fs::read(tokenizer) {
                Ok(bytes) => match serde_json::from_slice::<Value>(&bytes) {
                    Ok(json) => {
                        report.push(
                            DoctorStatus::Pass,
                            "live-tokenizer-json",
                            format!(
                                "Tokenizer JSON parsed successfully: {}",
                                tokenizer.display()
                            ),
                        );

                        if json
                            .as_object()
                            .map(|obj| obj.contains_key("model"))
                            .unwrap_or(false)
                        {
                            report.push(
                                DoctorStatus::Pass,
                                "live-tokenizer-shape",
                                "Tokenizer JSON contains top-level 'model' key.",
                            );
                        } else {
                            report.push_with_reason(
                                DoctorStatus::Fail,
                                "live-tokenizer-shape",
                                "Tokenizer JSON parsed but top-level 'model' key was not found.",
                                Some("tokenizer_model_key_missing"),
                            );
                        }
                    }
                    Err(err) => {
                        report.push_with_reason(
                            DoctorStatus::Fail,
                            "live-tokenizer-json",
                            format!(
                                "Tokenizer JSON parse failed for '{}': {err}",
                                tokenizer.display()
                            ),
                            Some("tokenizer_json_invalid"),
                        );
                    }
                },
                Err(err) => {
                    let reason_code = if err.kind() == std::io::ErrorKind::PermissionDenied {
                        "tokenizer_permission_denied"
                    } else {
                        "tokenizer_read_failed"
                    };
                    report.push_with_reason(
                        DoctorStatus::Fail,
                        "live-tokenizer-json",
                        format!(
                            "Unable to read tokenizer file '{}': {err}",
                            tokenizer.display()
                        ),
                        Some(reason_code),
                    );
                }
            }
        }
        Some(tokenizer) => {
            report.push_with_reason(
                DoctorStatus::Fail,
                "live-tokenizer-path",
                format!("Tokenizer file not found: {}", tokenizer.display()),
                Some("tokenizer_path_missing"),
            );
        }
        None => {
            report.push_with_reason(
                DoctorStatus::Fail,
                "live-tokenizer-path",
                "No tokenizer path resolved for live mode. The runtime will only work if tokenizer discovery succeeds.",
                Some("tokenizer_path_missing"),
            );
        }
    }

    if !model_readable {
        return;
    }

    let compatibility = inspect_runtime_compatibility(
        &ModelConfig {
            model_path: runtime.model.clone(),
            tokenizer_path: runtime.tokenizer.clone(),
            max_new_tokens: 1,
            temperature: runtime.temperature,
            dry_run: false,
            vitis_config: build_vitis_config(runtime),
        },
        true,
    );

    if compatibility.issues.is_empty() {
        report.push(
            DoctorStatus::Pass,
            "live-runtime-compatibility",
            format!(
                "Runtime compatibility checks passed (cache_inputs={}, cache_outputs={}, smoke_check={}).",
                compatibility.cache_input_count,
                compatibility.cache_output_count,
                compatibility.smoke_check_ran
            ),
        );
        return;
    }

    for issue in compatibility.issues {
        let status = match issue.severity {
            RuntimeCompatibilitySeverity::Warn => DoctorStatus::Warn,
            RuntimeCompatibilitySeverity::Fail => DoctorStatus::Fail,
        };

        report.push_with_reason(
            status,
            "live-runtime-compatibility",
            issue.detail,
            Some(issue.reason_code),
        );
    }
}

fn render_doctor_report(report: &DoctorReport) -> String {
    let mut output = String::new();
    let (pass_count, warn_count, fail_count) = report.counts();

    let _ = writeln!(output, "WraithRun Doctor");
    let _ = writeln!(
        output,
        "Summary: {pass_count} pass, {warn_count} warn, {fail_count} fail"
    );

    for check in &report.checks {
        if let Some(reason_code) = check.reason_code {
            let _ = writeln!(
                output,
                "[{}] {} [{}]: {}",
                check.status.label(),
                check.name,
                reason_code,
                check.detail
            );
        } else {
            let _ = writeln!(
                output,
                "[{}] {}: {}",
                check.status.label(),
                check.name,
                check.detail
            );
        }
        if let Some(remediation) = check.remediation {
            let _ = writeln!(output, "       Fix: {remediation}");
        }
    }

    output.trim_end().to_string()
}

fn render_doctor_report_json(report: &DoctorReport) -> Result<String> {
    let (pass_count, warn_count, fail_count) = report.counts();
    let view = DoctorReportView {
        summary: DoctorSummaryView {
            pass: pass_count,
            warn: warn_count,
            fail: fail_count,
        },
        checks: &report.checks,
    };
    render_json_with_contract(&view)
}

fn render_report(
    report: &RunReport,
    format: OutputFormat,
    output_mode: OutputMode,
    automation_adapter: Option<AutomationAdapter>,
) -> Result<String> {
    if let Some(adapter) = automation_adapter {
        return render_automation_adapter(report, adapter);
    }

    match format {
        OutputFormat::Json => {
            if output_mode == OutputMode::Compact {
                render_json_compact(report)
            } else {
                render_json_with_contract(report)
            }
        }
        OutputFormat::Summary => Ok(render_summary(report)),
        OutputFormat::Markdown => Ok(render_markdown(report)),
    }
}

fn render_json_compact(report: &RunReport) -> Result<String> {
    let mut value = serde_json::to_value(report).map_err(|err| anyhow!(err))?;
    let Some(object) = value.as_object_mut() else {
        bail!("JSON compact payload must serialize to an object");
    };

    object.remove("turns");

    object.insert(
        "contract_version".to_string(),
        Value::String(JSON_CONTRACT_VERSION.to_string()),
    );

    serde_json::to_string_pretty(&value).map_err(|err| anyhow!(err))
}

fn render_automation_adapter(report: &RunReport, adapter: AutomationAdapter) -> Result<String> {
    match adapter {
        AutomationAdapter::FindingsV1 => render_findings_adapter_v1(report),
    }
}

fn render_findings_adapter_v1(report: &RunReport) -> Result<String> {
    let mut severity_counts = AdapterSeverityCounts::default();
    let mut highest_rank = 0;
    let mut findings = Vec::with_capacity(report.findings.len());

    for (idx, finding) in report.findings.iter().enumerate() {
        let rank = finding_severity_rank(finding.severity);
        highest_rank = highest_rank.max(rank);
        increment_severity_count(&mut severity_counts, finding.severity);

        findings.push(FindingsAdapterEntry {
            finding_id: format!("F-{:04}", idx + 1),
            title: finding.title.clone(),
            severity: finding.severity,
            confidence: finding.confidence,
            recommended_action: finding.recommended_action.clone(),
            evidence_pointer: finding.evidence_pointer.clone(),
        });
    }

    let view = FindingsAdapterView {
        adapter: "findings-v1",
        summary: FindingsAdapterSummary {
            task: report.task.clone(),
            case_id: report.case_id.clone(),
            live_fallback_decision: report.live_fallback_decision.clone(),
            live_run_metrics: report.live_run_metrics.clone(),
            finding_count: findings.len(),
            highest_severity: if findings.is_empty() {
                "none".to_string()
            } else {
                finding_severity_token_from_rank(highest_rank).to_string()
            },
            severity_counts,
        },
        findings,
    };

    render_json_with_contract(&view)
}

fn increment_severity_count(counts: &mut AdapterSeverityCounts, severity: FindingSeverity) {
    match severity {
        FindingSeverity::Info => counts.info += 1,
        FindingSeverity::Low => counts.low += 1,
        FindingSeverity::Medium => counts.medium += 1,
        FindingSeverity::High => counts.high += 1,
        FindingSeverity::Critical => counts.critical += 1,
    }
}

fn evaluate_exit_policy(
    report: &RunReport,
    policy: ExitPolicy,
    threshold: Option<ExitSeverityThreshold>,
) -> Option<String> {
    match policy {
        ExitPolicy::None => None,
        ExitPolicy::SeverityThreshold => {
            let threshold = threshold.unwrap_or(ExitSeverityThreshold::Medium);
            let threshold_rank = exit_threshold_rank(threshold);

            report
                .findings
                .iter()
                .enumerate()
                .find(|(_, finding)| finding_severity_rank(finding.severity) >= threshold_rank)
                .map(|(idx, finding)| {
                    format!(
                        "exit policy triggered: finding severity '{}' met/exceeded threshold '{}' (finding {}: {})",
                        finding_severity_token(finding.severity),
                        exit_threshold_token(threshold),
                        idx + 1,
                        finding.title
                    )
                })
        }
    }
}

fn finding_severity_rank(severity: FindingSeverity) -> u8 {
    match severity {
        FindingSeverity::Info => 0,
        FindingSeverity::Low => 1,
        FindingSeverity::Medium => 2,
        FindingSeverity::High => 3,
        FindingSeverity::Critical => 4,
    }
}

fn exit_threshold_rank(threshold: ExitSeverityThreshold) -> u8 {
    match threshold {
        ExitSeverityThreshold::Info => 0,
        ExitSeverityThreshold::Low => 1,
        ExitSeverityThreshold::Medium => 2,
        ExitSeverityThreshold::High => 3,
        ExitSeverityThreshold::Critical => 4,
    }
}

fn finding_severity_token(severity: FindingSeverity) -> &'static str {
    match severity {
        FindingSeverity::Info => "info",
        FindingSeverity::Low => "low",
        FindingSeverity::Medium => "medium",
        FindingSeverity::High => "high",
        FindingSeverity::Critical => "critical",
    }
}

fn finding_severity_token_from_rank(rank: u8) -> &'static str {
    match rank {
        0 => "info",
        1 => "low",
        2 => "medium",
        3 => "high",
        _ => "critical",
    }
}

fn exit_threshold_token(threshold: ExitSeverityThreshold) -> &'static str {
    match threshold {
        ExitSeverityThreshold::Info => "info",
        ExitSeverityThreshold::Low => "low",
        ExitSeverityThreshold::Medium => "medium",
        ExitSeverityThreshold::High => "high",
        ExitSeverityThreshold::Critical => "critical",
    }
}

fn write_report_file(path: &Path, report: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)?;
        }
    }

    fs::write(path, report.as_bytes())?;
    Ok(())
}

fn write_evidence_bundle(bundle_dir: &Path, report: &RunReport) -> Result<()> {
    fs::create_dir_all(bundle_dir).with_context(|| {
        format!(
            "Failed creating evidence bundle directory {}",
            bundle_dir.display()
        )
    })?;

    let artifacts = build_evidence_bundle_artifacts(report)?;
    for artifact in artifacts {
        let artifact_path = bundle_dir.join(artifact.relative_path);
        fs::write(&artifact_path, &artifact.bytes)
            .with_context(|| format!("Failed writing {}", artifact_path.display()))?;
    }

    Ok(())
}

fn write_evidence_bundle_archive(archive_path: &Path, report: &RunReport) -> Result<()> {
    if let Some(parent) = archive_path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)
                .with_context(|| format!("Failed creating directory {}", parent.display()))?;
        }
    }

    let artifacts = build_evidence_bundle_artifacts(report)?;
    let archive_file = fs::File::create(archive_path)
        .with_context(|| format!("Failed creating archive {}", archive_path.display()))?;
    let mut builder = tar::Builder::new(archive_file);

    for artifact in &artifacts {
        append_deterministic_tar_entry(&mut builder, artifact.relative_path, &artifact.bytes)?;
    }

    builder
        .finish()
        .with_context(|| format!("Failed finalizing archive {}", archive_path.display()))?;

    Ok(())
}

fn build_evidence_bundle_artifacts(report: &RunReport) -> Result<Vec<EvidenceBundleArtifact>> {
    let report_json = render_json_with_contract(report)?;
    let raw_bundle = build_raw_observations_bundle(report);
    let raw_json = render_json_with_contract(&raw_bundle)?;

    let mut checksums = String::new();
    let _ = writeln!(
        checksums,
        "{}  report.json",
        sha256_hex(report_json.as_bytes())
    );
    let _ = writeln!(
        checksums,
        "{}  raw_observations.json",
        sha256_hex(raw_json.as_bytes())
    );

    Ok(vec![
        EvidenceBundleArtifact {
            relative_path: "report.json",
            bytes: report_json.into_bytes(),
        },
        EvidenceBundleArtifact {
            relative_path: "raw_observations.json",
            bytes: raw_json.into_bytes(),
        },
        EvidenceBundleArtifact {
            relative_path: "SHA256SUMS",
            bytes: checksums.into_bytes(),
        },
    ])
}

fn append_deterministic_tar_entry<W: std::io::Write>(
    builder: &mut tar::Builder<W>,
    relative_path: &str,
    bytes: &[u8],
) -> Result<()> {
    let mut header = tar::Header::new_gnu();
    header.set_size(bytes.len() as u64);
    header.set_mode(0o644);
    header.set_uid(0);
    header.set_gid(0);
    header.set_mtime(0);
    header.set_cksum();

    builder
        .append_data(&mut header, relative_path, Cursor::new(bytes))
        .with_context(|| format!("Failed appending archive entry {relative_path}"))?;

    Ok(())
}

fn build_raw_observations_bundle(report: &RunReport) -> RawObservationsBundle {
    let turns = report
        .turns
        .iter()
        .enumerate()
        .filter_map(|(idx, turn)| {
            let observation = turn.observation.clone()?;
            Some(RawObservationTurn {
                turn: idx + 1,
                tool: turn.tool_call.as_ref().map(|call| call.tool.clone()),
                args: turn.tool_call.as_ref().map(|call| call.args.clone()),
                observation,
            })
        })
        .collect();

    RawObservationsBundle {
        task: report.task.clone(),
        case_id: report.case_id.clone(),
        turns,
    }
}

fn load_coverage_baseline_from_bundle(path: &Path) -> Result<CoverageBaseline> {
    let raw_path = if path.is_dir() {
        path.join("raw_observations.json")
    } else {
        path.to_path_buf()
    };

    let raw_text = fs::read_to_string(&raw_path).with_context(|| {
        format!(
            "Failed reading raw observations bundle from {}",
            raw_path.display()
        )
    })?;
    let raw_bundle: RawObservationsBundle = serde_json::from_str(&raw_text).with_context(|| {
        format!(
            "Failed parsing raw observations bundle JSON from {}",
            raw_path.display()
        )
    })?;

    let baseline_observation = raw_bundle
        .turns
        .iter()
        .rev()
        .find_map(|turn| {
            if turn.tool.as_deref() == Some("capture_coverage_baseline") {
                Some(&turn.observation)
            } else {
                None
            }
        })
        .ok_or_else(|| {
            anyhow!(
                "No capture_coverage_baseline observation found in {}",
                raw_path.display()
            )
        })?;

    let coverage_baseline = extract_coverage_baseline_from_observation(baseline_observation);
    if coverage_baseline.is_empty() {
        bail!(
            "Coverage baseline observation in {} did not contain reusable baseline arrays",
            raw_path.display()
        );
    }

    Ok(coverage_baseline)
}

fn verify_evidence_bundle(path: &Path) -> Result<BundleVerificationReport> {
    let checksums_path = resolve_checksums_path(path)?;
    let bundle_dir = checksums_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .to_path_buf();

    let checksums_text = fs::read_to_string(&checksums_path).with_context(|| {
        format!(
            "Failed reading checksum manifest {}",
            checksums_path.display()
        )
    })?;

    let (entries, mut parse_errors) = verify_checksums_entries(&bundle_dir, &checksums_text);
    if entries.is_empty() && parse_errors.is_empty() {
        parse_errors.push("No checksum entries found in SHA256SUMS".to_string());
    }

    let pass_count = entries
        .iter()
        .filter(|entry| entry.status == BundleVerificationStatus::Pass)
        .count();
    let fail_count = entries
        .iter()
        .filter(|entry| entry.status != BundleVerificationStatus::Pass)
        .count()
        + parse_errors.len();

    Ok(BundleVerificationReport {
        bundle_dir: bundle_dir.display().to_string(),
        checksums_path: checksums_path.display().to_string(),
        summary: BundleVerificationSummary {
            pass: pass_count,
            fail: fail_count,
        },
        entries,
        parse_errors,
    })
}

fn resolve_checksums_path(path: &Path) -> Result<PathBuf> {
    if path.is_dir() {
        return Ok(path.join("SHA256SUMS"));
    }

    if path.is_file()
        && path
            .file_name()
            .and_then(|value| value.to_str())
            .map(|name| name.eq_ignore_ascii_case("SHA256SUMS"))
            .unwrap_or(false)
    {
        return Ok(path.to_path_buf());
    }

    bail!(
        "--verify-bundle must point to an evidence bundle directory or a SHA256SUMS file (got '{}')",
        path.display()
    )
}

fn verify_checksums_entries(
    bundle_dir: &Path,
    checksums_text: &str,
) -> (Vec<BundleVerificationEntry>, Vec<String>) {
    let mut entries = Vec::new();
    let mut parse_errors = Vec::new();

    for (idx, line) in checksums_text.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        let Some((expected_sha256, relative_path)) = parse_checksum_line(trimmed) else {
            parse_errors.push(format!(
                "Line {} is not a valid '<sha256>  <filename>' entry: {}",
                idx + 1,
                trimmed
            ));
            continue;
        };

        if !is_valid_sha256_hex(expected_sha256) {
            parse_errors.push(format!(
                "Line {} has invalid SHA-256 digest '{}': expected 64 hex characters",
                idx + 1,
                expected_sha256
            ));
            continue;
        }

        if relative_path.is_empty() {
            parse_errors.push(format!("Line {} is missing a filename", idx + 1));
            continue;
        }

        let file_path = bundle_dir.join(relative_path);
        if !file_path.is_file() {
            entries.push(BundleVerificationEntry {
                file: relative_path.to_string(),
                expected_sha256: expected_sha256.to_ascii_lowercase(),
                actual_sha256: None,
                status: BundleVerificationStatus::Missing,
                detail: Some(format!("Missing file: {}", file_path.display())),
            });
            continue;
        }

        match fs::read(&file_path) {
            Ok(bytes) => {
                let actual_sha256 = sha256_hex(&bytes);
                let status = if actual_sha256.eq_ignore_ascii_case(expected_sha256) {
                    BundleVerificationStatus::Pass
                } else {
                    BundleVerificationStatus::Mismatch
                };

                entries.push(BundleVerificationEntry {
                    file: relative_path.to_string(),
                    expected_sha256: expected_sha256.to_ascii_lowercase(),
                    actual_sha256: Some(actual_sha256),
                    status,
                    detail: None,
                });
            }
            Err(err) => {
                entries.push(BundleVerificationEntry {
                    file: relative_path.to_string(),
                    expected_sha256: expected_sha256.to_ascii_lowercase(),
                    actual_sha256: None,
                    status: BundleVerificationStatus::Unreadable,
                    detail: Some(err.to_string()),
                });
            }
        }
    }

    (entries, parse_errors)
}

fn parse_checksum_line(line: &str) -> Option<(&str, &str)> {
    let split_idx = line
        .char_indices()
        .find(|(_, ch)| ch.is_ascii_whitespace())
        .map(|(idx, _)| idx)?;

    let (expected, remainder) = line.split_at(split_idx);
    Some((expected, remainder.trim_start()))
}

fn is_valid_sha256_hex(value: &str) -> bool {
    value.len() == 64 && value.chars().all(|ch| ch.is_ascii_hexdigit())
}

fn render_bundle_verification_report_json(report: &BundleVerificationReport) -> Result<String> {
    render_json_with_contract(report)
}

fn render_bundle_verification_report(report: &BundleVerificationReport) -> String {
    let mut output = String::new();

    let _ = writeln!(output, "WraithRun Evidence Bundle Verification");
    let _ = writeln!(output, "Bundle: {}", report.bundle_dir);
    let _ = writeln!(output, "Checksums: {}", report.checksums_path);
    let _ = writeln!(
        output,
        "Summary: {} pass, {} fail",
        report.summary.pass, report.summary.fail
    );

    if report.entries.is_empty() {
        let _ = writeln!(output, "\nChecks: none");
    } else {
        let _ = writeln!(output, "\nChecks:");
        for entry in &report.entries {
            let status = match entry.status {
                BundleVerificationStatus::Pass => "PASS",
                BundleVerificationStatus::Missing => "FAIL",
                BundleVerificationStatus::Mismatch => "FAIL",
                BundleVerificationStatus::Unreadable => "FAIL",
            };

            let _ = writeln!(output, "- [{status}] {}", entry.file);
            if entry.status != BundleVerificationStatus::Pass {
                let _ = writeln!(output, "  expected: {}", entry.expected_sha256);
                let _ = writeln!(
                    output,
                    "  actual: {}",
                    entry.actual_sha256.as_deref().unwrap_or("(unavailable)")
                );
                if let Some(detail) = entry.detail.as_deref() {
                    let _ = writeln!(output, "  detail: {detail}");
                }
            }
        }
    }

    if !report.parse_errors.is_empty() {
        let _ = writeln!(output, "\nParse Errors:");
        for error in &report.parse_errors {
            let _ = writeln!(output, "- [FAIL] {error}");
        }
    }

    output.trim_end().to_string()
}

fn extract_coverage_baseline_from_observation(observation: &Value) -> CoverageBaseline {
    let baseline_entries = extract_string_array(
        observation
            .pointer("/persistence/baseline_entries")
            .or_else(|| observation.get("baseline_entries")),
        512,
        512,
    );

    let baseline_privileged_accounts = extract_string_array(
        observation
            .pointer("/accounts/baseline_privileged_accounts")
            .or_else(|| observation.get("baseline_privileged_accounts")),
        512,
        256,
    );

    let mut approved_privileged_accounts = extract_string_array(
        observation
            .pointer("/accounts/approved_privileged_accounts")
            .or_else(|| observation.get("approved_privileged_accounts")),
        512,
        256,
    );
    if approved_privileged_accounts.is_empty() {
        approved_privileged_accounts = baseline_privileged_accounts.clone();
    }

    let baseline_exposed_bindings = extract_string_array(
        observation
            .pointer("/network/baseline_exposed_bindings")
            .or_else(|| observation.get("baseline_exposed_bindings")),
        512,
        256,
    );

    let expected_processes = extract_string_array(
        observation
            .pointer("/network/expected_processes")
            .or_else(|| observation.get("expected_processes")),
        512,
        256,
    );

    CoverageBaseline {
        baseline_entries,
        baseline_privileged_accounts,
        approved_privileged_accounts,
        baseline_exposed_bindings,
        expected_processes,
    }
}

fn extract_string_array(value: Option<&Value>, max_items: usize, max_chars: usize) -> Vec<String> {
    let mut collected = Vec::new();
    let Some(entries) = value.and_then(Value::as_array) else {
        return collected;
    };

    for entry in entries.iter().take(max_items) {
        let Some(raw) = entry.as_str() else {
            continue;
        };

        let trimmed = raw.trim();
        if trimmed.is_empty() {
            continue;
        }

        collected.push(trimmed.chars().take(max_chars).collect());
    }

    collected.sort_by_cached_key(|entry| entry.to_ascii_lowercase());
    collected.dedup_by(|left, right| left.eq_ignore_ascii_case(right));
    collected
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

fn render_summary(report: &RunReport) -> String {
    let mut output = String::new();

    let _ = writeln!(output, "Task: {}", report.task);
    if let Some(case_id) = report.case_id.as_deref() {
        let _ = writeln!(output, "Case ID: {case_id}");
    }
    if let Some(decision) = report.live_fallback_decision.as_ref() {
        let _ = writeln!(output, "Live Fallback: {}", decision.fallback_mode);
        let _ = writeln!(output, "Fallback Policy: {}", decision.policy);
        let _ = writeln!(output, "Fallback Reason: {}", decision.reason);
        let _ = writeln!(output, "Fallback Reason Code: {}", decision.reason_code);
    }
    if let Some(run_timing) = report.run_timing.as_ref() {
        let _ = writeln!(
            output,
            "Run Timing: first_token_ms={}, total_ms={}",
            optional_latency_label(run_timing.first_token_latency_ms),
            run_timing.total_run_duration_ms
        );
    }
    if let Some(metrics) = report.live_run_metrics.as_ref() {
        let _ = writeln!(
            output,
            "Live Metrics: success_rate={:.2}, fallback_rate={:.2}, live_attempt_duration_ms={}, total_ms={}, first_token_ms={}",
            metrics.live_success_rate,
            metrics.fallback_rate,
            metrics.live_attempt_duration_ms,
            metrics.total_run_duration_ms,
            optional_latency_label(metrics.first_token_latency_ms)
        );
        if !metrics.top_failure_reasons.is_empty() {
            let _ = writeln!(
                output,
                "Live Failure Reasons: {}",
                format_live_failure_reasons(&metrics.top_failure_reasons)
            );
        }
    }
    let _ = writeln!(output, "Turns: {}", report.turns.len());
    let _ = writeln!(output, "Findings: {}", report.findings.len());
    let _ = writeln!(output, "Final Answer: {}", report.final_answer);

    if !report.findings.is_empty() {
        let _ = writeln!(output, "\nFindings:");
        for (idx, finding) in report.findings.iter().enumerate() {
            let _ = writeln!(
                output,
                "{}. [{}] {} (confidence {:.2})",
                idx + 1,
                finding_severity_label(finding.severity),
                finding.title,
                finding.confidence
            );
            let _ = writeln!(
                output,
                "   evidence: {}",
                render_evidence_pointer(&finding.evidence_pointer)
            );
            let _ = writeln!(
                output,
                "   recommended_action: {}",
                finding.recommended_action
            );
        }
    }

    if report.turns.is_empty() {
        return output.trim_end().to_string();
    }

    let _ = writeln!(output, "\nTurn Breakdown:");
    for (idx, turn) in report.turns.iter().enumerate() {
        let _ = writeln!(output, "{}.", idx + 1);

        if let Some(call) = &turn.tool_call {
            let _ = writeln!(output, "   tool: {}", call.tool);
            let _ = writeln!(output, "   args: {}", compact_json(&call.args));
        } else {
            let _ = writeln!(output, "   tool: none");
        }

        if let Some(observation) = &turn.observation {
            let _ = writeln!(
                output,
                "   observation: {}",
                summarize_observation(observation)
            );
        } else {
            let _ = writeln!(output, "   observation: none");
        }
    }

    output.trim_end().to_string()
}

fn render_markdown(report: &RunReport) -> String {
    let mut output = String::new();

    let _ = writeln!(output, "# WraithRun Report");
    let _ = writeln!(output);
    let _ = writeln!(output, "- Task: {}", report.task);
    if let Some(case_id) = report.case_id.as_deref() {
        let _ = writeln!(output, "- Case ID: {case_id}");
    }
    if let Some(decision) = report.live_fallback_decision.as_ref() {
        let _ = writeln!(output, "- Live Fallback: {}", decision.fallback_mode);
        let _ = writeln!(output, "- Fallback Policy: {}", decision.policy);
        let _ = writeln!(output, "- Fallback Reason: {}", decision.reason);
        let _ = writeln!(output, "- Fallback Reason Code: {}", decision.reason_code);
    }
    if let Some(run_timing) = report.run_timing.as_ref() {
        let _ = writeln!(
            output,
            "- Run Timing: first_token_ms={}, total_ms={}",
            optional_latency_label(run_timing.first_token_latency_ms),
            run_timing.total_run_duration_ms
        );
    }
    if let Some(metrics) = report.live_run_metrics.as_ref() {
        let _ = writeln!(
            output,
            "- Live Metrics: success_rate={:.2}, fallback_rate={:.2}, live_attempt_duration_ms={}, total_ms={}, first_token_ms={}",
            metrics.live_success_rate,
            metrics.fallback_rate,
            metrics.live_attempt_duration_ms,
            metrics.total_run_duration_ms,
            optional_latency_label(metrics.first_token_latency_ms)
        );
        if !metrics.top_failure_reasons.is_empty() {
            let _ = writeln!(
                output,
                "- Live Failure Reasons: {}",
                format_live_failure_reasons(&metrics.top_failure_reasons)
            );
        }
    }
    let _ = writeln!(output, "- Turns: {}", report.turns.len());
    let _ = writeln!(output, "- Findings: {}", report.findings.len());
    let _ = writeln!(output, "- Final Answer: {}", report.final_answer);

    if !report.findings.is_empty() {
        let _ = writeln!(output, "\n## Findings");
        for (idx, finding) in report.findings.iter().enumerate() {
            let _ = writeln!(output, "\n### Finding {}", idx + 1);
            let _ = writeln!(output, "- Title: {}", finding.title);
            let _ = writeln!(
                output,
                "- Severity: {}",
                finding_severity_label(finding.severity)
            );
            let _ = writeln!(output, "- Confidence: {:.2}", finding.confidence);
            let _ = writeln!(
                output,
                "- Evidence: {}",
                render_evidence_pointer(&finding.evidence_pointer)
            );
            let _ = writeln!(
                output,
                "- Recommended Action: {}",
                finding.recommended_action
            );
        }
    }

    if report.turns.is_empty() {
        return output.trim_end().to_string();
    }

    let _ = writeln!(output, "\n## Turns");
    for (idx, turn) in report.turns.iter().enumerate() {
        let _ = writeln!(output, "\n### Turn {}", idx + 1);

        if let Some(call) = &turn.tool_call {
            let _ = writeln!(output, "- Tool: {}", call.tool);
            let _ = writeln!(output, "- Args:");
            let _ = writeln!(output, "```json");
            let _ = writeln!(output, "{}", pretty_json(&call.args));
            let _ = writeln!(output, "```");
        } else {
            let _ = writeln!(output, "- Tool: none");
        }

        if let Some(observation) = &turn.observation {
            let _ = writeln!(output, "- Observation:");
            let _ = writeln!(output, "```json");
            let _ = writeln!(output, "{}", pretty_json(observation));
            let _ = writeln!(output, "```");
        } else {
            let _ = writeln!(output, "- Observation: none");
        }
    }

    output.trim_end().to_string()
}

fn pretty_json(value: &Value) -> String {
    serde_json::to_string_pretty(value).unwrap_or_else(|_| "{}".to_string())
}

fn compact_json(value: &Value) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "{}".to_string())
}

fn summarize_observation(value: &Value) -> String {
    if let Some(object) = value.as_object() {
        if let Some(error) = object.get("error").and_then(Value::as_str) {
            return format!("error={error}");
        }

        let mut keys: Vec<&str> = object.keys().map(String::as_str).collect();
        keys.sort_unstable();

        if keys.is_empty() {
            return "{}".to_string();
        }

        return format!("keys={}", keys.join(","));
    }

    if value.is_null() {
        return "null".to_string();
    }

    compact_json(value)
}

fn finding_severity_label(severity: FindingSeverity) -> &'static str {
    match severity {
        FindingSeverity::Info => "INFO",
        FindingSeverity::Low => "LOW",
        FindingSeverity::Medium => "MEDIUM",
        FindingSeverity::High => "HIGH",
        FindingSeverity::Critical => "CRITICAL",
    }
}

fn render_evidence_pointer(pointer: &EvidencePointer) -> String {
    let turn = pointer
        .turn
        .map(|turn| format!("turn {turn}"))
        .unwrap_or_else(|| "turn n/a".to_string());
    let tool = pointer.tool.as_deref().unwrap_or("tool n/a");
    format!("{turn}, {tool}, {}", pointer.field)
}

fn optional_latency_label(value: Option<u64>) -> String {
    value
        .map(|latency_ms| latency_ms.to_string())
        .unwrap_or_else(|| "null".to_string())
}

fn format_live_failure_reasons(reasons: &[LiveFailureReasonCount]) -> String {
    reasons
        .iter()
        .map(|entry| format!("{}:{}", entry.reason_code, entry.count))
        .collect::<Vec<_>>()
        .join(",")
}

async fn run_with_live_fallback(runtime: &RuntimeConfig) -> Result<RunReport> {
    if !runtime.live {
        return run_agent_once(runtime, true).await;
    }

    let run_started_at = Instant::now();
    let live_attempt_started_at = Instant::now();

    match run_agent_once(runtime, false).await {
        Ok(mut report) => {
            let live_attempt_duration_ms = elapsed_ms_since(live_attempt_started_at);
            let total_run_duration_ms = elapsed_ms_since(run_started_at);
            let first_token_latency_ms = report
                .run_timing
                .as_ref()
                .and_then(|timing| timing.first_token_latency_ms);

            report.live_run_metrics = Some(build_live_run_metrics(
                total_run_duration_ms,
                live_attempt_duration_ms,
                first_token_latency_ms,
                true,
                None,
            ));

            Ok(report)
        }
        Err(err) => {
            if runtime.live_fallback_policy != LiveFallbackPolicy::DryRunOnError {
                return Err(err);
            }

            let live_attempt_duration_ms = elapsed_ms_since(live_attempt_started_at);
            let live_error = format!("{err:#}");
            let reason_code = classify_live_error_reason_code(&live_error).to_string();
            let mut report = run_agent_once(runtime, true).await.with_context(|| {
                format!(
                    "live run failed and fallback dry-run also failed (live error: {live_error})"
                )
            })?;

            let decision = LiveFallbackDecision {
                policy: live_fallback_policy_token(runtime.live_fallback_policy).to_string(),
                reason: "live inference failed and runtime fell back to dry-run".to_string(),
                reason_code: reason_code.clone(),
                live_error,
                fallback_mode: "dry-run".to_string(),
            };

            append_live_fallback_finding(&mut report, &decision);
            report.live_fallback_decision = Some(decision);
            let fallback_first_token_latency_ms = report
                .run_timing
                .as_ref()
                .and_then(|timing| timing.first_token_latency_ms);
            let first_token_latency_ms = fallback_first_token_latency_ms
                .map(|value| live_attempt_duration_ms.saturating_add(value));

            report.live_run_metrics = Some(build_live_run_metrics(
                elapsed_ms_since(run_started_at),
                live_attempt_duration_ms,
                first_token_latency_ms,
                false,
                Some(reason_code.as_str()),
            ));

            Ok(report)
        }
    }
}

fn elapsed_ms_since(started_at: Instant) -> u64 {
    started_at
        .elapsed()
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX)
}

fn build_live_run_metrics(
    total_run_duration_ms: u64,
    live_attempt_duration_ms: u64,
    first_token_latency_ms: Option<u64>,
    live_success: bool,
    failure_reason_code: Option<&str>,
) -> LiveRunMetrics {
    let live_attempt_count = 1usize;
    let live_success_count = if live_success { 1 } else { 0 };
    let fallback_count = if live_success { 0 } else { 1 };
    let top_failure_reasons = failure_reason_code
        .map(|reason_code| {
            vec![LiveFailureReasonCount {
                reason_code: reason_code.to_string(),
                count: 1,
            }]
        })
        .unwrap_or_default();

    LiveRunMetrics {
        first_token_latency_ms,
        total_run_duration_ms,
        live_attempt_duration_ms,
        live_attempt_count,
        live_success_count,
        fallback_count,
        live_success_rate: ratio(live_success_count, live_attempt_count),
        fallback_rate: ratio(fallback_count, live_attempt_count),
        top_failure_reasons,
    }
}

fn ratio(count: usize, total: usize) -> f64 {
    if total == 0 {
        return 0.0;
    }

    count as f64 / total as f64
}

async fn run_agent_once(runtime: &RuntimeConfig, dry_run: bool) -> Result<RunReport> {
    if runtime.live && !dry_run {
        validate_live_runtime_preflight(runtime)?;
    }

    let vitis_config = build_vitis_config(runtime);
    let model_config = ModelConfig {
        model_path: runtime.model.clone(),
        tokenizer_path: runtime.tokenizer.clone(),
        max_new_tokens: runtime.max_new_tokens,
        temperature: runtime.temperature,
        dry_run,
        vitis_config,
    };

    let brain = OnnxVitisEngine::new(model_config);
    let tools = ToolRegistry::with_default_tools();
    let mut agent = Agent::new(brain, tools).with_max_steps(runtime.max_steps);

    if let Some(baseline_bundle) = runtime.baseline_bundle.as_deref() {
        let coverage_baseline = load_coverage_baseline_from_bundle(baseline_bundle)?;
        agent = agent.with_coverage_baseline(coverage_baseline);
    }

    agent.run(&runtime.task).await
}

fn append_live_fallback_finding(report: &mut RunReport, decision: &LiveFallbackDecision) {
    if report
        .findings
        .iter()
        .any(|finding| finding.evidence_pointer.field == "live_fallback_decision.live_error")
    {
        return;
    }

    report.findings.push(Finding {
        title: "Live mode fallback applied after inference failure".to_string(),
        severity: FindingSeverity::Info,
        confidence: 1.0,
        evidence_pointer: EvidencePointer {
            turn: None,
            tool: None,
            field: "live_fallback_decision.live_error".to_string(),
        },
        recommended_action: format!(
            "Review live inference error details and model-pack readiness, then rerun live mode after fixing root cause. Fallback reason: {} (code: {}).",
            decision.reason,
            decision.reason_code
        ),
    });
}

fn classify_live_error_reason_code(live_error: &str) -> &'static str {
    let normalized = live_error.to_ascii_lowercase();

    if normalized.contains("runtime compatibility")
        || normalized.contains("unsupported runtime inputs")
        || normalized.contains("cache output")
        || normalized.contains("cache input")
        || normalized.contains("runtime_forward_smoke_failed")
    {
        return "model_runtime_incompatible";
    }

    if normalized.contains("unable to locate tokenizer")
        || normalized.contains("tokenizer file not found")
        || normalized.contains("tokenizer path")
    {
        return "tokenizer_path_missing";
    }

    if normalized.contains("tokenizer")
        && (normalized.contains("parse")
            || normalized.contains("json")
            || normalized.contains("failed to load tokenizer"))
    {
        return "tokenizer_json_invalid";
    }

    if normalized.contains("no such file")
        || normalized.contains("path does not exist")
        || normalized.contains("could not read model")
        || normalized.contains("model file")
    {
        return "model_path_missing";
    }

    if normalized.contains("permission denied") {
        return "permission_denied";
    }

    if normalized.contains("vitis") || normalized.contains("onnx") || normalized.contains("ort") {
        return "live_runtime_error";
    }

    "unknown_live_error"
}

fn live_fallback_policy_token(policy: LiveFallbackPolicy) -> &'static str {
    match policy {
        LiveFallbackPolicy::None => "none",
        LiveFallbackPolicy::DryRunOnError => "dry-run-on-error",
    }
}

fn build_vitis_config(runtime: &RuntimeConfig) -> Option<VitisEpConfig> {
    let discovered_cache_dir = discover_vitis_cache_dir(&runtime.model);
    let discovered_cache_key = discover_vitis_cache_key(&runtime.model);
    let cache_dir = runtime.vitis_cache_dir.clone().or(discovered_cache_dir);
    let cache_key = runtime.vitis_cache_key.clone().or(discovered_cache_key);

    if runtime.vitis_config.is_none() && cache_dir.is_none() && cache_key.is_none() {
        return None;
    }

    Some(VitisEpConfig {
        config_file: runtime.vitis_config.clone(),
        cache_dir,
        cache_key,
    })
}

fn init_tracing(log_mode: LogMode) {
    if matches!(log_mode, LogMode::Quiet) {
        return;
    }

    let default_level = if matches!(log_mode, LogMode::Verbose) {
        "debug"
    } else {
        "warn"
    };
    let env_filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(default_level));
    let _ = tracing_subscriber::fmt()
        .with_env_filter(env_filter)
        .with_target(false)
        .with_writer(std::io::stderr)
        .try_init();
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::io::Read;
    use std::path::Path;
    use std::time::{SystemTime, UNIX_EPOCH};
    use std::{env, fs};

    use serde_json::json;

    use core_engine::{
        AgentTurn, EvidencePointer, Finding, FindingSeverity, LiveFailureReasonCount,
        LiveFallbackDecision, LiveRunMetrics, RunReport, RunTimingMetrics, ToolCall,
    };

    use super::{
        append_live_fallback_finding, build_live_run_metrics, ensure_introspection_format_usage,
        evaluate_exit_policy, load_coverage_baseline_from_bundle, merge_sources,
        render_bundle_verification_report, render_doctor_report, render_doctor_report_json,
        render_effective_config_explanation_json, render_effective_config_json,
        render_profile_list, render_profile_list_json, render_report, render_task_template_list,
        render_task_template_list_json, render_tool_detail, render_tool_detail_json,
        render_tool_list, render_tool_list_json, resolve_effective_config_explanation,
        resolve_init_config_path, resolve_task_for_mode, resolve_task_for_run, run_describe_tool,
        run_init_config, run_list_tools, run_model_pack_doctor_checks, run_models_benchmark,
        run_models_list, run_models_validate, validate_live_runtime_preflight,
        verify_evidence_bundle, write_evidence_bundle, write_evidence_bundle_archive,
        AutomationAdapter, Cli, DoctorReport, DoctorStatus, ExitPolicy, ExitSeverityThreshold,
        FileConfig, IntrospectionFormat, LiveFallbackPolicy, OutputFormat, OutputMode,
        RuntimeConfig, SettingsFragment, TaskTemplate, ToolRegistry,
    };

    fn base_cli() -> Cli {
        Cli {
            task: Some("Check suspicious listener ports and summarize risk".to_string()),
            task_file: None,
            task_stdin: false,
            task_template: None,
            template_target: None,
            template_lines: None,
            doctor: false,
            list_task_templates: false,
            list_tools: false,
            describe_tool: None,
            tool_filter: None,
            list_profiles: false,
            introspection_format: IntrospectionFormat::Text,
            print_effective_config: false,
            explain_effective_config: false,
            init_config: false,
            init_config_path: None,
            force: false,
            fix: false,
            live_setup: false,
            models_list: false,
            models_validate: false,
            models_benchmark: false,
            config: None,
            profile: None,
            model: None,
            tokenizer: None,
            max_steps: None,
            max_new_tokens: None,
            temperature: None,
            live: false,
            dry_run: false,
            live_fallback_policy: None,
            format: None,
            output_mode: None,
            automation_adapter: None,
            exit_policy: None,
            exit_threshold: None,
            output_file: None,
            case_id: None,
            evidence_bundle_dir: None,
            evidence_bundle_archive: None,
            baseline_bundle: None,
            verify_bundle: None,
            quiet: false,
            verbose: false,
            vitis_config: None,
            vitis_cache_dir: None,
            vitis_cache_key: None,
        }
    }

    fn sample_report() -> RunReport {
        RunReport {
            task: "Check suspicious listener ports and summarize risk".to_string(),
            case_id: Some("CASE-2026-0001".to_string()),
            max_severity: Some(FindingSeverity::Medium),
            live_fallback_decision: None,
            run_timing: None,
            live_run_metrics: None,
            turns: vec![AgentTurn {
                thought: "<call>{...}</call>".to_string(),
                tool_call: Some(ToolCall {
                    tool: "scan_network".to_string(),
                    args: json!({ "limit": 40 }),
                }),
                observation: Some(json!({ "listener_count": 3, "listeners": [] })),
            }],
            final_answer: "Dry-run cycle complete.".to_string(),
            findings: vec![Finding {
                title: "Active listening sockets observed (3)".to_string(),
                severity: FindingSeverity::Medium,
                confidence: 0.68,
                evidence_pointer: EvidencePointer {
                    turn: Some(1),
                    tool: Some("scan_network".to_string()),
                    field: "observation.listener_count".to_string(),
                },
                recommended_action: "Correlate listener PIDs and ports with expected services."
                    .to_string(),
            }],
        }
    }

    #[test]
    fn renders_json_output() {
        let report = sample_report();
        let rendered =
            render_report(&report, OutputFormat::Json, OutputMode::Full, None).expect("json render should work");
        assert!(rendered.contains("\"contract_version\": \"1.0.0\""));
        assert!(rendered.contains("\"task\""));
        assert!(rendered.contains("\"scan_network\""));
        assert!(rendered.contains("\"findings\""));
    }

    #[test]
    fn renders_json_compact_omits_turns() {
        let report = sample_report();
        let rendered =
            render_report(&report, OutputFormat::Json, OutputMode::Compact, None)
                .expect("compact render should work");
        assert!(rendered.contains("\"contract_version\": \"1.0.0\""));
        assert!(rendered.contains("\"task\""));
        assert!(rendered.contains("\"findings\""));
        assert!(!rendered.contains("\"turns\""));
    }

    #[test]
    fn renders_json_output_with_live_metrics() {
        let mut report = sample_report();
        report.run_timing = Some(RunTimingMetrics {
            first_token_latency_ms: Some(42),
            total_run_duration_ms: 210,
        });
        report.live_run_metrics = Some(LiveRunMetrics {
            first_token_latency_ms: Some(77),
            total_run_duration_ms: 512,
            live_attempt_duration_ms: 150,
            live_attempt_count: 1,
            live_success_count: 0,
            fallback_count: 1,
            live_success_rate: 0.0,
            fallback_rate: 1.0,
            top_failure_reasons: vec![LiveFailureReasonCount {
                reason_code: "live_runtime_error".to_string(),
                count: 1,
            }],
        });

        let rendered =
            render_report(&report, OutputFormat::Json, OutputMode::Full, None).expect("json render should work");

        assert!(rendered.contains("\"run_timing\""));
        assert!(rendered.contains("\"live_run_metrics\""));
        assert!(rendered.contains("\"first_token_latency_ms\": 77"));
        assert!(rendered.contains("\"top_failure_reasons\""));
    }

    #[test]
    fn renders_summary_output() {
        let report = sample_report();
        let rendered = render_report(&report, OutputFormat::Summary, OutputMode::Full, None)
            .expect("summary render should work");
        assert!(rendered.contains("Task:"));
        assert!(rendered.contains("Case ID: CASE-2026-0001"));
        assert!(rendered.contains("Findings:"));
        assert!(rendered.contains("tool: scan_network"));
        assert!(rendered.contains("Final Answer:"));
    }

    #[test]
    fn renders_markdown_output() {
        let report = sample_report();
        let rendered = render_report(&report, OutputFormat::Markdown, OutputMode::Full, None)
            .expect("markdown render should work");
        assert!(rendered.contains("# WraithRun Report"));
        assert!(rendered.contains("- Case ID: CASE-2026-0001"));
        assert!(rendered.contains("## Findings"));
        assert!(rendered.contains("## Turns"));
        assert!(rendered.contains("```json"));
    }

    #[test]
    fn renders_findings_adapter_output() {
        let report = sample_report();
        let rendered = render_report(
            &report,
            OutputFormat::Json,
            OutputMode::Full,
            Some(AutomationAdapter::FindingsV1),
        )
        .expect("adapter render should work");

        assert!(rendered.contains("\"contract_version\": \"1.0.0\""));
        assert!(rendered.contains("\"adapter\": \"findings-v1\""));
        assert!(rendered.contains("\"finding_id\": \"F-0001\""));
        assert!(rendered.contains("\"summary\""));
    }

    #[test]
    fn findings_adapter_includes_live_metrics_summary() {
        let mut report = sample_report();
        report.live_run_metrics = Some(LiveRunMetrics {
            first_token_latency_ms: Some(95),
            total_run_duration_ms: 540,
            live_attempt_duration_ms: 180,
            live_attempt_count: 1,
            live_success_count: 0,
            fallback_count: 1,
            live_success_rate: 0.0,
            fallback_rate: 1.0,
            top_failure_reasons: vec![LiveFailureReasonCount {
                reason_code: "model_path_missing".to_string(),
                count: 1,
            }],
        });

        let rendered = render_report(
            &report,
            OutputFormat::Json,
            OutputMode::Full,
            Some(AutomationAdapter::FindingsV1),
        )
        .expect("adapter render should work");

        assert!(rendered.contains("\"live_run_metrics\""));
        assert!(rendered.contains("\"fallback_rate\": 1.0"));
        assert!(rendered.contains("\"reason_code\": \"model_path_missing\""));
    }

    #[test]
    fn adapter_requires_json_output_format() {
        let mut cli = base_cli();
        cli.format = Some(OutputFormat::Summary);
        cli.automation_adapter = Some(AutomationAdapter::FindingsV1);

        let err = merge_sources(
            &cli,
            "test-task".to_string(),
            None,
            None,
            None,
            &SettingsFragment::default(),
        )
        .expect_err("adapter should require JSON output format");

        assert!(err
            .to_string()
            .contains("automation_adapter requires JSON output format"));
    }

    #[test]
    fn fallback_policy_from_cli_is_applied() {
        let mut cli = base_cli();
        cli.live_fallback_policy = Some(LiveFallbackPolicy::DryRunOnError);

        let resolved = merge_sources(
            &cli,
            "test-task".to_string(),
            None,
            None,
            None,
            &SettingsFragment::default(),
        )
        .expect("runtime resolution should succeed");

        assert_eq!(
            resolved.live_fallback_policy,
            LiveFallbackPolicy::DryRunOnError
        );
    }

    #[test]
    fn appends_live_fallback_finding_once() {
        let mut report = sample_report();
        let decision = LiveFallbackDecision {
            policy: "dry-run-on-error".to_string(),
            reason: "live inference failed and runtime fell back to dry-run".to_string(),
            reason_code: "live_runtime_error".to_string(),
            live_error: "session create failed".to_string(),
            fallback_mode: "dry-run".to_string(),
        };

        append_live_fallback_finding(&mut report, &decision);
        append_live_fallback_finding(&mut report, &decision);

        let fallback_findings = report
            .findings
            .iter()
            .filter(|finding| finding.evidence_pointer.field == "live_fallback_decision.live_error")
            .count();
        assert_eq!(fallback_findings, 1);
    }

    #[test]
    fn live_runtime_preflight_rejects_missing_model() {
        let temp_dir = unique_temp_dir("wraithrun-live-preflight-missing-model");
        fs::create_dir_all(&temp_dir).expect("temp dir should be created");

        let mut runtime = RuntimeConfig::new("live-preflight".to_string());
        runtime.live = true;
        runtime.model = temp_dir.join("missing.onnx");

        let err = validate_live_runtime_preflight(&runtime)
            .expect_err("missing model should fail preflight");

        assert!(err.to_string().contains("model file not found"));

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn live_runtime_preflight_rejects_missing_explicit_tokenizer() {
        let temp_dir = unique_temp_dir("wraithrun-live-preflight-missing-tokenizer");
        fs::create_dir_all(&temp_dir).expect("temp dir should be created");

        let model_path = temp_dir.join("model.onnx");
        fs::write(&model_path, b"onnx-model-bytes").expect("model fixture should be written");

        let mut runtime = RuntimeConfig::new("live-preflight".to_string());
        runtime.live = true;
        runtime.model = model_path;
        runtime.tokenizer = Some(temp_dir.join("missing-tokenizer.json"));

        let err = validate_live_runtime_preflight(&runtime)
            .expect_err("missing tokenizer should fail preflight");

        assert!(err.to_string().contains("Tokenizer file not found"));

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn live_runtime_preflight_accepts_valid_model_and_tokenizer() {
        let temp_dir = unique_temp_dir("wraithrun-live-preflight-valid");
        fs::create_dir_all(&temp_dir).expect("temp dir should be created");

        let model_path = temp_dir.join("model.onnx");
        let tokenizer_path = temp_dir.join("tokenizer.json");
        fs::write(&model_path, b"onnx-model-bytes").expect("model fixture should be written");
        fs::write(
            &tokenizer_path,
            r#"{"model":{"type":"WordPiece"},"version":"1.0"}"#,
        )
        .expect("tokenizer fixture should be written");

        let mut runtime = RuntimeConfig::new("live-preflight".to_string());
        runtime.live = true;
        runtime.model = model_path;
        runtime.tokenizer = Some(tokenizer_path);

        validate_live_runtime_preflight(&runtime)
            .expect("valid live model/tokenizer should pass preflight");

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn build_live_run_metrics_calculates_success_rates() {
        let metrics = build_live_run_metrics(400, 400, Some(120), true, None);

        assert_eq!(metrics.live_attempt_count, 1);
        assert_eq!(metrics.live_success_count, 1);
        assert_eq!(metrics.fallback_count, 0);
        assert_eq!(metrics.live_success_rate, 1.0);
        assert_eq!(metrics.fallback_rate, 0.0);
        assert!(metrics.top_failure_reasons.is_empty());
    }

    #[test]
    fn build_live_run_metrics_captures_fallback_reason() {
        let metrics =
            build_live_run_metrics(900, 200, Some(260), false, Some("live_runtime_error"));

        assert_eq!(metrics.live_attempt_count, 1);
        assert_eq!(metrics.live_success_count, 0);
        assert_eq!(metrics.fallback_count, 1);
        assert_eq!(metrics.live_success_rate, 0.0);
        assert_eq!(metrics.fallback_rate, 1.0);
        assert_eq!(metrics.top_failure_reasons.len(), 1);
        assert_eq!(
            metrics.top_failure_reasons[0].reason_code,
            "live_runtime_error"
        );
    }

    #[test]
    fn exit_threshold_requires_policy() {
        let mut cli = base_cli();
        cli.exit_threshold = Some(ExitSeverityThreshold::High);

        let err = merge_sources(
            &cli,
            "test-task".to_string(),
            None,
            None,
            None,
            &SettingsFragment::default(),
        )
        .expect_err("exit threshold without policy should fail");

        assert!(err
            .to_string()
            .contains("exit_threshold requires exit_policy=severity-threshold"));
    }

    #[test]
    fn exit_policy_triggers_when_threshold_is_met() {
        let report = sample_report();
        let message = evaluate_exit_policy(
            &report,
            ExitPolicy::SeverityThreshold,
            Some(ExitSeverityThreshold::Medium),
        );

        assert!(message.is_some());
        assert!(message
            .expect("message should exist")
            .contains("exit policy triggered"));
    }

    #[test]
    fn exit_policy_uses_medium_default_threshold() {
        let report = sample_report();
        let message = evaluate_exit_policy(&report, ExitPolicy::SeverityThreshold, None);

        assert!(message.is_some());
    }

    #[test]
    fn case_id_with_spaces_is_rejected() {
        let mut cli = base_cli();
        cli.case_id = Some("CASE 2026 INVALID".to_string());

        let err = merge_sources(
            &cli,
            "test-task".to_string(),
            None,
            None,
            None,
            &SettingsFragment::default(),
        )
        .expect_err("invalid case id should fail validation");

        assert!(err
            .to_string()
            .contains("case_id may only contain ASCII letters"));
    }

    #[test]
    fn writes_evidence_bundle_files() {
        let report = sample_report();
        let bundle_dir = unique_temp_dir("wraithrun-evidence-bundle");

        write_evidence_bundle(&bundle_dir, &report).expect("bundle write should succeed");

        let report_path = bundle_dir.join("report.json");
        let raw_path = bundle_dir.join("raw_observations.json");
        let sums_path = bundle_dir.join("SHA256SUMS");

        assert!(report_path.is_file(), "report.json should exist");
        assert!(raw_path.is_file(), "raw_observations.json should exist");
        assert!(sums_path.is_file(), "SHA256SUMS should exist");

        let sums = fs::read_to_string(&sums_path).expect("checksums should be readable");
        assert!(sums.contains("report.json"));
        assert!(sums.contains("raw_observations.json"));

        let _ = fs::remove_dir_all(&bundle_dir);
    }

    #[test]
    fn writes_deterministic_evidence_bundle_archive() {
        let report = sample_report();
        let archive_dir = unique_temp_dir("wraithrun-evidence-bundle-archive");
        fs::create_dir_all(&archive_dir).expect("archive directory should be created");

        let archive_a = archive_dir.join("bundle-a.tar");
        let archive_b = archive_dir.join("bundle-b.tar");

        write_evidence_bundle_archive(&archive_a, &report)
            .expect("first archive write should succeed");
        write_evidence_bundle_archive(&archive_b, &report)
            .expect("second archive write should succeed");

        let bytes_a = fs::read(&archive_a).expect("first archive should be readable");
        let bytes_b = fs::read(&archive_b).expect("second archive should be readable");
        assert_eq!(
            bytes_a, bytes_b,
            "archive bytes should be deterministic for identical report input"
        );

        let archive_file = fs::File::open(&archive_a).expect("archive should be openable");
        let mut archive = tar::Archive::new(archive_file);
        let mut entry_names = Vec::new();
        let mut checksums_text = String::new();

        for entry in archive
            .entries()
            .expect("archive entries should be readable")
        {
            let mut entry = entry.expect("archive entry should be readable");
            let entry_path = entry
                .path()
                .expect("archive entry path should be available")
                .to_string_lossy()
                .to_string();

            if entry_path == "SHA256SUMS" {
                entry
                    .read_to_string(&mut checksums_text)
                    .expect("checksums entry should be readable");
            }

            entry_names.push(entry_path);
        }

        assert_eq!(
            entry_names,
            vec![
                "report.json".to_string(),
                "raw_observations.json".to_string(),
                "SHA256SUMS".to_string(),
            ]
        );
        assert!(checksums_text.contains("report.json"));
        assert!(checksums_text.contains("raw_observations.json"));

        let _ = fs::remove_dir_all(&archive_dir);
    }

    #[test]
    fn loads_coverage_baseline_from_raw_bundle() {
        let bundle_dir = unique_temp_dir("wraithrun-baseline-import");
        fs::create_dir_all(&bundle_dir).expect("bundle directory should be created");

        let raw_content = r#"{
    "task": "Capture baseline",
    "case_id": "CASE-2026-BASE-1",
    "turns": [
        {
            "turn": 1,
            "tool": "capture_coverage_baseline",
            "args": {"persistence_limit": 128},
            "observation": {
                "persistence": {
                    "baseline_entries": ["A-entry", "a-entry", "B-entry"]
                },
                "accounts": {
                    "baseline_privileged_accounts": ["svc-admin"],
                    "approved_privileged_accounts": ["svc-admin"]
                },
                "network": {
                    "baseline_exposed_bindings": ["0.0.0.0:443"],
                    "expected_processes": ["nginx"]
                }
            }
        }
    ]
}"#;

        let raw_path = bundle_dir.join("raw_observations.json");
        fs::write(&raw_path, raw_content).expect("raw bundle fixture should be written");

        let baseline =
            load_coverage_baseline_from_bundle(&bundle_dir).expect("baseline import should work");

        assert_eq!(baseline.baseline_entries.len(), 2);
        assert!(baseline
            .baseline_entries
            .iter()
            .any(|entry| entry.eq_ignore_ascii_case("A-entry")));
        assert_eq!(
            baseline.baseline_privileged_accounts,
            vec!["svc-admin".to_string()]
        );
        assert_eq!(baseline.expected_processes, vec!["nginx".to_string()]);

        let _ = fs::remove_dir_all(&bundle_dir);
    }

    #[test]
    fn baseline_bundle_requires_raw_observations_filename_for_file_path() {
        let mut cli = base_cli();
        let invalid_path = unique_temp_file("wraithrun-invalid-baseline");
        fs::write(&invalid_path, "{}").expect("invalid baseline fixture should be created");
        cli.baseline_bundle = Some(invalid_path.clone());

        let err = merge_sources(
            &cli,
            "test-task".to_string(),
            None,
            None,
            None,
            &SettingsFragment::default(),
        )
        .expect_err("invalid baseline file name should fail validation");

        assert!(err
            .to_string()
            .contains("baseline_bundle file must be named raw_observations.json"));

        let _ = fs::remove_file(&invalid_path);
    }

    #[test]
    fn verifies_evidence_bundle_and_reports_mismatch() {
        let report = sample_report();
        let bundle_dir = unique_temp_dir("wraithrun-verify-bundle");

        write_evidence_bundle(&bundle_dir, &report).expect("bundle write should succeed");

        let report_path = bundle_dir.join("report.json");
        fs::write(&report_path, "{\"tampered\":true}\n").expect("tamper write should succeed");

        let verification = verify_evidence_bundle(&bundle_dir).expect("verification should run");
        assert_eq!(verification.summary.fail, 1);
        assert!(verification
            .entries
            .iter()
            .any(|entry| entry.file == "report.json"
                && entry.status == super::BundleVerificationStatus::Mismatch));

        let rendered = render_bundle_verification_report(&verification);
        assert!(rendered.contains("Summary: 1 pass, 1 fail"));
        assert!(rendered.contains("[FAIL] report.json"));

        let _ = fs::remove_dir_all(&bundle_dir);
    }

    #[test]
    fn precedence_is_cli_over_env_over_config_over_defaults() {
        let mut cli = base_cli();
        cli.profile = Some("production-triage".to_string());
        cli.max_steps = Some(16);

        let file_config = FileConfig {
            defaults: SettingsFragment {
                max_steps: Some(5),
                max_new_tokens: Some(300),
                ..SettingsFragment::default()
            },
            profiles: HashMap::from([(
                "production-triage".to_string(),
                SettingsFragment {
                    max_steps: Some(10),
                    ..SettingsFragment::default()
                },
            )]),
        };

        let env_overrides = SettingsFragment {
            max_steps: Some(12),
            ..SettingsFragment::default()
        };

        let resolved = merge_sources(
            &cli,
            "test-task".to_string(),
            Some("production-triage".to_string()),
            Some(&file_config),
            None,
            &env_overrides,
        )
        .expect("config merge should succeed");

        assert_eq!(resolved.max_steps, 16);
        assert_eq!(resolved.max_new_tokens, 300);
    }

    #[test]
    fn builtin_profile_applies_when_no_config_file() {
        let mut cli = base_cli();
        cli.profile = Some("local-lab".to_string());

        let resolved = merge_sources(
            &cli,
            "test-task".to_string(),
            Some("local-lab".to_string()),
            None,
            None,
            &SettingsFragment::default(),
        )
        .expect("builtin profile should resolve");

        assert_eq!(resolved.max_steps, 6);
        assert_eq!(resolved.format, OutputFormat::Summary);
        assert!(!resolved.live);
    }

    #[test]
    fn live_presets_resolve_to_live_mode() {
        for profile_name in ["live-fast", "live-balanced", "live-deep"] {
            let mut cli = base_cli();
            cli.profile = Some(profile_name.to_string());

            let resolved = merge_sources(
                &cli,
                "test-task".to_string(),
                Some(profile_name.to_string()),
                None,
                None,
                &SettingsFragment::default(),
            )
            .expect("live preset should resolve");

            assert!(resolved.live, "preset should enable live mode");
            assert_eq!(resolved.format, OutputFormat::Json);
            assert_eq!(
                resolved.live_fallback_policy,
                LiveFallbackPolicy::DryRunOnError
            );
        }
    }

    #[test]
    fn models_list_json_includes_live_presets() {
        let mut cli = base_cli();
        cli.task = None;

        let rendered =
            run_models_list(&cli, IntrospectionFormat::Json).expect("models-list should render");

        assert!(rendered.contains("\"contract_version\": \"1.0.0\""));
        assert!(rendered.contains("\"packs\""));
        assert!(rendered.contains("\"live-fast\""));
        assert!(rendered.contains("\"live-balanced\""));
        assert!(rendered.contains("\"live-deep\""));
    }

    #[test]
    fn models_validate_reports_failures_for_missing_pack_files() {
        let mut cli = base_cli();
        cli.task = None;
        cli.profile = Some("live-fast".to_string());

        let missing_model = unique_temp_file("wraithrun-missing-model-pack");
        let missing_tokenizer = missing_model.with_extension("json");
        let _ = fs::remove_file(&missing_model);
        let _ = fs::remove_file(&missing_tokenizer);
        cli.model = Some(missing_model);
        cli.tokenizer = Some(missing_tokenizer);

        let outcome = run_models_validate(&cli, IntrospectionFormat::Json)
            .expect("models-validate should render JSON");

        assert!(
            outcome.has_failures,
            "default environment should report failures"
        );
        assert!(outcome.rendered.contains("\"summary\""));
        assert!(outcome.rendered.contains("\"packs\""));
    }

    #[test]
    fn models_benchmark_json_reports_recommendation() {
        let mut cli = base_cli();
        cli.task = None;

        let rendered = run_models_benchmark(&cli, IntrospectionFormat::Json)
            .expect("models-benchmark should render");

        assert!(rendered.contains("\"recommended_profile\""));
        assert!(rendered.contains("\"benchmark_score\""));
        assert!(rendered.contains("\"live-fast\""));
    }

    #[test]
    fn unknown_profile_without_config_fails() {
        let mut cli = base_cli();
        cli.profile = Some("nonexistent".to_string());

        let error = merge_sources(
            &cli,
            "test-task".to_string(),
            Some("nonexistent".to_string()),
            None,
            None,
            &SettingsFragment::default(),
        )
        .expect_err("unknown profile should fail");

        assert!(error.to_string().contains("nonexistent"));
    }

    #[test]
    fn renders_doctor_report_summary() {
        let mut report = DoctorReport::default();
        report.push(DoctorStatus::Pass, "config-file", "loaded config");
        report.push(DoctorStatus::Warn, "live-model-path", "missing model");
        report.push(DoctorStatus::Fail, "effective-runtime", "invalid profile");

        let rendered = render_doctor_report(&report);

        assert!(rendered.contains("WraithRun Doctor"));
        assert!(rendered.contains("Summary: 1 pass, 1 warn, 1 fail"));
        assert!(rendered.contains("[FAIL] effective-runtime"));
    }

    #[test]
    fn renders_doctor_report_json() {
        let mut report = DoctorReport::default();
        report.push(DoctorStatus::Pass, "config-file", "loaded config");
        report.push(DoctorStatus::Warn, "live-model-path", "missing model");
        report.push(DoctorStatus::Fail, "effective-runtime", "invalid profile");

        let rendered = render_doctor_report_json(&report).expect("json doctor render works");

        assert!(rendered.contains("\"contract_version\": \"1.0.0\""));
        assert!(rendered.contains("\"summary\""));
        assert!(rendered.contains("\"pass\": 1"));
        assert!(rendered.contains("\"status\": \"fail\""));
    }

    #[test]
    fn model_pack_doctor_checks_detect_invalid_live_pack() {
        let temp_dir = unique_temp_dir("wraithrun-doctor-model-pack-invalid");
        fs::create_dir_all(&temp_dir).expect("temp dir should be created");

        let model_path = temp_dir.join("model.bin");
        let tokenizer_path = temp_dir.join("tokenizer.json");
        fs::write(&model_path, b"not an onnx payload").expect("model fixture should be written");
        fs::write(&tokenizer_path, b"").expect("tokenizer fixture should be written");

        let mut runtime = RuntimeConfig::new("doctor-task".to_string());
        runtime.live = true;
        runtime.model = model_path;
        runtime.tokenizer = Some(tokenizer_path);

        let mut report = DoctorReport::default();
        run_model_pack_doctor_checks(&runtime, &mut report);

        assert!(report
            .checks
            .iter()
            .any(|check| check.name == "live-model-format" && check.status == DoctorStatus::Warn));
        assert!(report.checks.iter().any(|check| {
            check.name == "live-tokenizer-size" && check.status == DoctorStatus::Fail
        }));

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn model_pack_doctor_checks_accept_valid_live_pack() {
        let temp_dir = unique_temp_dir("wraithrun-doctor-model-pack-valid");
        fs::create_dir_all(&temp_dir).expect("temp dir should be created");

        let model_path = temp_dir.join("model.onnx");
        let tokenizer_path = temp_dir.join("tokenizer.json");
        fs::write(&model_path, b"onnx-model-bytes").expect("model fixture should be written");
        fs::write(
            &tokenizer_path,
            r#"{"model":{"type":"WordPiece"},"version":"1.0"}"#,
        )
        .expect("tokenizer fixture should be written");

        let mut runtime = RuntimeConfig::new("doctor-task".to_string());
        runtime.live = true;
        runtime.model = model_path;
        runtime.tokenizer = Some(tokenizer_path);

        let mut report = DoctorReport::default();
        run_model_pack_doctor_checks(&runtime, &mut report);

        assert!(report
            .checks
            .iter()
            .any(|check| check.name == "live-model-format" && check.status == DoctorStatus::Pass));
        assert!(report.checks.iter().any(|check| {
            check.name == "live-tokenizer-json" && check.status == DoctorStatus::Pass
        }));
        assert!(report.checks.iter().any(|check| {
            check.name == "live-tokenizer-shape" && check.status == DoctorStatus::Pass
        }));

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn renders_profile_list_with_config_profiles() {
        let file_config = FileConfig {
            defaults: SettingsFragment::default(),
            profiles: HashMap::from([
                ("team-default".to_string(), SettingsFragment::default()),
                ("incident-hotfix".to_string(), SettingsFragment::default()),
            ]),
        };

        let rendered = render_profile_list(
            Some("local-lab"),
            Some(Path::new("./wraithrun.toml")),
            Some(&file_config),
        );

        assert!(rendered.contains("WraithRun Profiles"));
        assert!(rendered.contains("team-default"));
        assert!(rendered.contains("incident-hotfix"));
        assert!(rendered.contains("Profile source: built-in"));
    }

    #[test]
    fn renders_profile_list_json_with_selected_source() {
        let file_config = FileConfig {
            defaults: SettingsFragment::default(),
            profiles: HashMap::from([("incident-hotfix".to_string(), SettingsFragment::default())]),
        };

        let rendered = render_profile_list_json(
            Some("incident-hotfix"),
            Some(Path::new("./wraithrun.toml")),
            Some(&file_config),
        )
        .expect("json profile render works");

        assert!(rendered.contains("\"contract_version\": \"1.0.0\""));
        assert!(rendered.contains("\"built_in_profiles\""));
        assert!(rendered.contains("\"selected_profile\""));
        assert!(rendered.contains("\"source\": \"config\""));
    }

    #[test]
    fn renders_effective_config_json() {
        let cli = base_cli();
        let runtime = merge_sources(
            &cli,
            "test-task".to_string(),
            None,
            None,
            None,
            &SettingsFragment::default(),
        )
        .expect("runtime resolution should succeed");

        let rendered =
            render_effective_config_json(&runtime).expect("effective config rendering works");

        assert!(rendered.contains("\"contract_version\": \"1.0.0\""));
        assert!(rendered.contains("\"mode\": \"dry-run\""));
        assert!(rendered.contains("\"max_steps\": 8"));
        assert!(rendered.contains("\"model\""));
    }

    #[test]
    fn renders_effective_config_explanation_json() {
        let mut cli = base_cli();
        cli.task = None;
        cli.profile = Some("local-lab".to_string());
        let path = unique_temp_file("wraithrun-explain");
        fs::write(&path, "").expect("config fixture should be created");
        cli.config = Some(path.clone());

        let explanation =
            resolve_effective_config_explanation(&cli).expect("explanation should resolve");
        let rendered = render_effective_config_explanation_json(&explanation)
            .expect("explanation should serialize");

        assert!(rendered.contains("\"contract_version\": \"1.0.0\""));
        assert!(rendered.contains("\"sources\""));
        assert!(rendered.contains("\"selected_profile\": \"local-lab\""));
        assert!(rendered.contains("built-in profile 'local-lab'"));

        let _ = fs::remove_file(&path);
    }

    #[test]
    fn resolves_init_config_path() {
        let mut cli = base_cli();
        cli.init_config = true;
        cli.init_config_path = Some(Path::new("./custom.toml").to_path_buf());

        let path = resolve_init_config_path(&cli);
        assert!(path.ends_with("custom.toml"));
    }

    #[test]
    fn resolves_task_from_template() {
        let mut cli = base_cli();
        cli.task = None;
        cli.task_template = Some(TaskTemplate::ListenerRisk);

        let task = resolve_task_for_run(&cli).expect("template task should resolve");
        assert!(task.contains("listener ports"));
    }

    #[test]
    fn resolves_task_from_file() {
        let mut cli = base_cli();
        cli.task = None;
        let path = unique_temp_file("wraithrun-task-file");
        fs::write(&path, "  Investigate unauthorized SSH keys\n")
            .expect("task file fixture should be created");
        cli.task_file = Some(path.clone());

        let task = resolve_task_for_run(&cli).expect("task file should resolve");
        assert_eq!(task, "Investigate unauthorized SSH keys");

        let _ = fs::remove_file(&path);
    }

    #[test]
    fn resolves_task_from_utf16le_bom_file() {
        let mut cli = base_cli();
        cli.task = None;
        let path = unique_temp_file("wraithrun-task-file-utf16");

        let mut bytes = vec![0xFF, 0xFE];
        for unit in "Investigate unauthorized SSH keys".encode_utf16() {
            bytes.extend_from_slice(&unit.to_le_bytes());
        }
        fs::write(&path, bytes).expect("utf16 task file fixture should be created");

        cli.task_file = Some(path.clone());
        let task = resolve_task_for_run(&cli).expect("utf16 task file should resolve");
        assert_eq!(task, "Investigate unauthorized SSH keys");

        let _ = fs::remove_file(&path);
    }

    #[test]
    fn task_source_precedence_prefers_explicit_task() {
        let mut cli = base_cli();
        cli.task = Some("explicit task".to_string());
        let path = unique_temp_file("wraithrun-task-file-precedence");
        fs::write(&path, "task from file").expect("task file fixture should be created");
        cli.task_file = Some(path.clone());
        cli.task_template = Some(TaskTemplate::SshKeys);

        let task = resolve_task_for_run(&cli).expect("task should resolve");
        assert_eq!(task, "explicit task");

        let _ = fs::remove_file(&path);
    }

    #[test]
    fn task_source_precedence_prefers_task_file_over_template() {
        let mut cli = base_cli();
        cli.task = None;
        let path = unique_temp_file("wraithrun-task-file-over-template");
        fs::write(&path, "task from file").expect("task file fixture should be created");
        cli.task_file = Some(path.clone());
        cli.task_template = Some(TaskTemplate::SshKeys);

        let task = resolve_task_for_run(&cli).expect("task should resolve");
        assert_eq!(task, "task from file");

        let _ = fs::remove_file(&path);
    }

    #[test]
    fn resolve_task_for_mode_uses_fallback_without_sources() {
        let mut cli = base_cli();
        cli.task = None;

        let task = resolve_task_for_mode(&cli, "fallback-task").expect("fallback should resolve");
        assert_eq!(task, "fallback-task");
    }

    #[test]
    fn rejects_empty_task_file() {
        let mut cli = base_cli();
        cli.task = None;
        let path = unique_temp_file("wraithrun-empty-task-file");
        fs::write(&path, "\n   \n").expect("empty task file fixture should be created");
        cli.task_file = Some(path.clone());

        let err = resolve_task_for_run(&cli).expect_err("empty task file should fail");
        assert!(err.to_string().contains("is empty"));

        let _ = fs::remove_file(&path);
    }

    #[test]
    fn resolves_hash_template_with_custom_target() {
        let mut cli = base_cli();
        cli.task = None;
        cli.task_template = Some(TaskTemplate::HashIntegrity);
        cli.template_target = Some("C:/Temp/suspicious.exe".to_string());

        let task = resolve_task_for_run(&cli).expect("hash template should resolve");
        assert!(task.contains("C:/Temp/suspicious.exe"));
    }

    #[test]
    fn resolves_syslog_template_with_target_and_lines() {
        let mut cli = base_cli();
        cli.task = None;
        cli.task_template = Some(TaskTemplate::SyslogSummary);
        cli.template_target = Some("C:/Logs/security.log".to_string());
        cli.template_lines = Some(50);

        let task = resolve_task_for_run(&cli).expect("syslog template should resolve");
        assert!(task.contains("last 50 lines"));
        assert!(task.contains("C:/Logs/security.log"));
    }

    #[test]
    fn rejects_invalid_template_options() {
        let mut cli = base_cli();
        cli.task = None;
        cli.task_template = Some(TaskTemplate::ListenerRisk);
        cli.template_lines = Some(10);

        let err = resolve_task_for_run(&cli).expect_err("invalid template options should fail");
        assert!(err.to_string().contains("not supported"));
    }

    #[test]
    fn rejects_zero_syslog_lines() {
        let mut cli = base_cli();
        cli.task = None;
        cli.task_template = Some(TaskTemplate::SyslogSummary);
        cli.template_lines = Some(0);

        let err = resolve_task_for_run(&cli).expect_err("zero lines should fail");
        assert!(err.to_string().contains("at least 1"));
    }

    #[test]
    fn renders_task_template_list() {
        let rendered = render_task_template_list();
        assert!(rendered.contains("WraithRun Task Templates"));
        assert!(rendered.contains("ssh-keys"));
        assert!(rendered.contains("priv-esc-review"));
    }

    #[test]
    fn renders_task_template_list_json() {
        let rendered =
            render_task_template_list_json().expect("template list json rendering should work");
        assert!(rendered.contains("\"contract_version\": \"1.0.0\""));
        assert!(rendered.contains("\"templates\""));
        assert!(rendered.contains("\"syslog-summary\""));
        assert!(rendered.contains("\"supports_template_lines\": true"));
    }

    #[test]
    fn renders_tool_list() {
        let tools = ToolRegistry::with_default_tools().tool_specs();
        let rendered = render_tool_list(&tools);
        assert!(rendered.contains("WraithRun Tools"));
        assert!(rendered.contains("hash_binary"));
        assert!(rendered.contains("scan_network"));
    }

    #[test]
    fn renders_tool_list_json() {
        let tools = ToolRegistry::with_default_tools().tool_specs();
        let rendered = render_tool_list_json(tools).expect("tool list json rendering should work");
        assert!(rendered.contains("\"contract_version\": \"1.0.0\""));
        assert!(rendered.contains("\"tools\""));
        assert!(rendered.contains("\"check_privilege_escalation_vectors\""));
        assert!(rendered.contains("\"args_schema\""));
    }

    #[test]
    fn list_tools_filter_matches_expected_tool() {
        let rendered = run_list_tools(IntrospectionFormat::Json, Some("hash"))
            .expect("filtered list-tools should succeed");
        assert!(rendered.contains("\"hash_binary\""));
        assert!(!rendered.contains("\"scan_network\""));
    }

    #[test]
    fn list_tools_filter_matches_multi_term_query() {
        let rendered = run_list_tools(IntrospectionFormat::Json, Some("priv esc"))
            .expect("multi-term filtered list-tools should succeed");
        assert!(rendered.contains("\"check_privilege_escalation_vectors\""));
        assert!(!rendered.contains("\"scan_network\""));
    }

    #[test]
    fn list_tools_filter_matches_hyphenated_query() {
        let rendered = run_list_tools(IntrospectionFormat::Json, Some("hash-binary"))
            .expect("hyphenated filtered list-tools should succeed");
        assert!(rendered.contains("\"hash_binary\""));
        assert!(!rendered.contains("\"scan_network\""));
    }

    #[test]
    fn list_tools_filter_rejects_unknown_query() {
        let error = run_list_tools(IntrospectionFormat::Text, Some("definitely-not-a-tool"))
            .expect_err("unknown filter should fail");
        assert!(error.to_string().contains("No tools matched filter"));
    }

    #[test]
    fn list_tools_filter_rejects_empty_query() {
        let error = run_list_tools(IntrospectionFormat::Text, Some("   "))
            .expect_err("empty filter should fail");
        assert!(error.to_string().contains("--tool-filter cannot be empty"));
    }

    #[test]
    fn list_tools_filter_rejects_non_alphanumeric_query() {
        let error = run_list_tools(IntrospectionFormat::Text, Some("---"))
            .expect_err("separator-only filter should fail");
        assert!(error.to_string().contains("at least one alphanumeric term"));
    }

    #[test]
    fn renders_tool_detail() {
        let tool = ToolRegistry::with_default_tools()
            .tool_specs()
            .into_iter()
            .find(|candidate| candidate.name == "hash_binary")
            .expect("hash_binary should exist");

        let rendered = render_tool_detail(&tool);
        assert!(rendered.contains("WraithRun Tool"));
        assert!(rendered.contains("name: hash_binary"));
        assert!(rendered.contains("args_schema:"));
    }

    #[test]
    fn renders_tool_detail_json() {
        let tool = ToolRegistry::with_default_tools()
            .tool_specs()
            .into_iter()
            .find(|candidate| candidate.name == "hash_binary")
            .expect("hash_binary should exist");

        let rendered =
            render_tool_detail_json(tool).expect("tool detail json rendering should work");
        assert!(rendered.contains("\"contract_version\": \"1.0.0\""));
        assert!(rendered.contains("\"tool\""));
        assert!(rendered.contains("\"name\": \"hash_binary\""));
    }

    #[test]
    fn describes_tool_by_name() {
        let rendered = run_describe_tool("hash_binary", IntrospectionFormat::Json)
            .expect("describe-tool should work");
        assert!(rendered.contains("\"tool\""));
        assert!(rendered.contains("\"name\": \"hash_binary\""));
    }

    #[test]
    fn describes_tool_with_hyphenated_name_alias() {
        let rendered = run_describe_tool("hash-binary", IntrospectionFormat::Json)
            .expect("describe-tool should support hyphenated aliases");
        assert!(rendered.contains("\"tool\""));
        assert!(rendered.contains("\"name\": \"hash_binary\""));
    }

    #[test]
    fn describes_tool_with_unique_partial_query() {
        let rendered = run_describe_tool("privilege", IntrospectionFormat::Json)
            .expect("describe-tool should support unique partial matches");
        assert!(rendered.contains("\"tool\""));
        assert!(rendered.contains("\"name\": \"check_privilege_escalation_vectors\""));
    }

    #[test]
    fn describe_tool_rejects_ambiguous_partial_query() {
        let error = run_describe_tool("c", IntrospectionFormat::Text)
            .expect_err("ambiguous partial query should fail");
        assert!(error.to_string().contains("Ambiguous tool query 'c'"));
        assert!(error.to_string().contains("scan_network"));
        assert!(error
            .to_string()
            .contains("check_privilege_escalation_vectors"));
    }

    #[test]
    fn describe_tool_rejects_unknown_name() {
        let error = run_describe_tool("does-not-exist", IntrospectionFormat::Text)
            .expect_err("unknown tool should fail");
        assert!(error.to_string().contains("Unknown tool"));
        assert!(error.to_string().contains("hash_binary"));
    }

    #[test]
    fn rejects_json_introspection_format_without_mode() {
        let mut cli = base_cli();
        cli.introspection_format = IntrospectionFormat::Json;

        let err = ensure_introspection_format_usage(&cli)
            .expect_err("json introspection should require introspection mode");
        assert!(err
            .to_string()
            .contains("--introspection-format only applies"));
    }

    #[test]
    fn allows_json_introspection_format_with_mode() {
        let mut cli = base_cli();
        cli.list_profiles = true;
        cli.introspection_format = IntrospectionFormat::Json;

        ensure_introspection_format_usage(&cli)
            .expect("json introspection should be allowed for list-profiles");
    }

    #[test]
    fn allows_json_introspection_format_for_models_list_mode() {
        let mut cli = base_cli();
        cli.models_list = true;
        cli.introspection_format = IntrospectionFormat::Json;

        ensure_introspection_format_usage(&cli)
            .expect("json introspection should be allowed for models-list");
    }

    #[test]
    fn allows_json_introspection_format_for_list_tools_mode() {
        let mut cli = base_cli();
        cli.list_tools = true;
        cli.introspection_format = IntrospectionFormat::Json;

        ensure_introspection_format_usage(&cli)
            .expect("json introspection should be allowed for list-tools");
    }

    #[test]
    fn allows_json_introspection_format_for_describe_tool_mode() {
        let mut cli = base_cli();
        cli.describe_tool = Some("hash_binary".to_string());
        cli.introspection_format = IntrospectionFormat::Json;

        ensure_introspection_format_usage(&cli)
            .expect("json introspection should be allowed for describe-tool");
    }

    #[test]
    fn allows_json_introspection_format_for_verify_bundle_mode() {
        let mut cli = base_cli();
        cli.task = None;
        cli.verify_bundle = Some(Path::new(".").to_path_buf());
        cli.introspection_format = IntrospectionFormat::Json;

        ensure_introspection_format_usage(&cli)
            .expect("json introspection should be allowed for verify-bundle");
    }

    #[test]
    fn init_config_writes_file() {
        let mut cli = base_cli();
        cli.init_config = true;
        cli.task = None;
        let path = unique_temp_file("wraithrun-init-write");
        cli.init_config_path = Some(path.clone());

        let result = run_init_config(&cli).expect("init-config should write file");

        let content = fs::read_to_string(&path).expect("written file should be readable");
        assert!(result.contains("Wrote starter config"));
        assert!(content.contains("[profiles.local-lab]"));

        let _ = fs::remove_file(&path);
    }

    #[test]
    fn init_config_requires_force_to_overwrite() {
        let mut cli = base_cli();
        cli.init_config = true;
        cli.task = None;
        let path = unique_temp_file("wraithrun-init-overwrite");
        fs::write(&path, "model = 'old'\n").expect("pre-seed should succeed");
        cli.init_config_path = Some(path.clone());

        let error = run_init_config(&cli).expect_err("overwrite without force should fail");
        assert!(error.to_string().contains("Use --force to overwrite"));

        cli.force = true;
        run_init_config(&cli).expect("overwrite with force should succeed");
        let content = fs::read_to_string(&path).expect("overwritten file should be readable");
        assert!(content.contains("[profiles.production-triage]"));

        let _ = fs::remove_file(&path);
    }

    fn unique_temp_file(prefix: &str) -> std::path::PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time should be valid")
            .as_nanos();
        env::temp_dir().join(format!("{prefix}-{}-{stamp}.toml", std::process::id()))
    }

    fn unique_temp_dir(prefix: &str) -> std::path::PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time should be valid")
            .as_nanos();
        env::temp_dir().join(format!("{prefix}-{}-{stamp}", std::process::id()))
    }
}
