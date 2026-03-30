use std::collections::HashMap;
use std::io::{IsTerminal, Read};
use std::path::{Path, PathBuf};
use std::{fmt::Write as _, fs};

use anyhow::{anyhow, bail, Context, Result};
use clap::{Parser, ValueEnum};
use core_engine::agent::Agent;
use core_engine::RunReport;
use cyber_tools::ToolRegistry;
use inference_bridge::{ModelConfig, OnnxVitisEngine, VitisEpConfig};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tracing_subscriber::EnvFilter;

const DEFAULT_CONFIG_FILE: &str = "wraithrun.toml";
const DEFAULT_MODEL_PATH: &str = "./models/llm.onnx";
const DEFAULT_MAX_STEPS: usize = 8;
const DEFAULT_MAX_NEW_TOKENS: usize = 256;
const DEFAULT_TEMPERATURE: f32 = 0.2;
const DEFAULT_CONFIG_TEMPLATE: &str = include_str!("../../wraithrun.example.toml");

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

#[derive(Debug, Parser, Clone)]
#[command(name = "wraithrun", about = "Local-first cyber investigation runtime")]
struct Cli {
    #[arg(long, required_unless_present_any = ["task_file", "task_stdin", "task_template", "doctor", "list_profiles", "print_effective_config", "init_config", "explain_effective_config", "list_task_templates"])]
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
    format: Option<OutputFormat>,

    #[arg(long)]
    output_file: Option<PathBuf>,

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
    format: Option<OutputFormat>,
    output_file: Option<PathBuf>,
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
    format: OutputFormat,
    output_file: Option<PathBuf>,
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
    model: String,
    tokenizer: Option<String>,
    max_steps: usize,
    max_new_tokens: usize,
    temperature: f32,
    format: OutputFormat,
    output_file: Option<String>,
    log_mode: LogMode,
    vitis_config: Option<String>,
    vitis_cache_dir: Option<String>,
    vitis_cache_key: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
struct RuntimeConfigSources {
    task: String,
    live: String,
    model: String,
    tokenizer: String,
    max_steps: String,
    max_new_tokens: String,
    temperature: String,
    format: String,
    output_file: String,
    log_mode: String,
    vitis_config: String,
    vitis_cache_dir: String,
    vitis_cache_key: String,
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

#[derive(Debug, Serialize)]
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
            format: OutputFormat::Json,
            output_file: None,
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
        if let Some(format) = fragment.format {
            self.format = format;
        }
        if let Some(output_file) = &fragment.output_file {
            self.output_file = Some(output_file.clone());
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
            model: "default".to_string(),
            tokenizer: "default".to_string(),
            max_steps: "default".to_string(),
            max_new_tokens: "default".to_string(),
            temperature: "default".to_string(),
            format: "default".to_string(),
            output_file: "default".to_string(),
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
}

#[derive(Debug, Default, Serialize)]
struct DoctorReport {
    checks: Vec<DoctorCheck>,
}

impl DoctorReport {
    fn push(&mut self, status: DoctorStatus, name: &'static str, detail: impl Into<String>) {
        self.checks.push(DoctorCheck {
            status,
            name,
            detail: detail.into(),
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
    let cli = Cli::parse();
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

    let runtime = resolve_runtime_config(&cli)?;
    init_tracing(runtime.log_mode);

    let vitis_config = build_vitis_config(&runtime);

    let model_config = ModelConfig {
        model_path: runtime.model,
        tokenizer_path: runtime.tokenizer,
        max_new_tokens: runtime.max_new_tokens,
        temperature: runtime.temperature,
        dry_run: !runtime.live,
        vitis_config,
    };

    let brain = OnnxVitisEngine::new(model_config);
    let tools = ToolRegistry::with_default_tools();
    let agent = Agent::new(brain, tools).with_max_steps(runtime.max_steps);

    let report = agent.run(&runtime.task).await?;
    let rendered = render_report(&report, runtime.format)?;
    if let Some(path) = &runtime.output_file {
        write_report_file(path, &rendered)?;
    }
    println!("{rendered}");

    Ok(())
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
        "Either --task, --task-stdin, --task-file, or --task-template is required unless one of --doctor, --list-task-templates, --list-profiles, --print-effective-config, --explain-effective-config, or --init-config is set"
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
    serde_json::to_string_pretty(&view).map_err(|err| anyhow!(err))
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

const KNOWN_PROFILE_NAMES: [&str; 3] = ["local-lab", "production-triage", "live-model"];

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
        format: read_env_output_format("WRAITHRUN_FORMAT")?,
        output_file: read_env_path("WRAITHRUN_OUTPUT_FILE")?,
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
    if let Some(format) = cli.format {
        runtime.format = format;
    }
    if let Some(output_file) = &cli.output_file {
        runtime.output_file = Some(output_file.clone());
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
    if let Some(format) = fragment.format {
        runtime.format = format;
        sources.format = source.to_string();
    }
    if let Some(output_file) = &fragment.output_file {
        runtime.output_file = Some(output_file.clone());
        sources.output_file = source.to_string();
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
    if let Some(format) = cli.format {
        runtime.format = format;
        sources.format = "cli --format".to_string();
    }
    if let Some(output_file) = &cli.output_file {
        runtime.output_file = Some(output_file.clone());
        sources.output_file = "cli --output-file".to_string();
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

    Ok(())
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
        && !(cli.doctor || cli.list_task_templates || cli.list_profiles)
    {
        bail!(
            "--introspection-format only applies to --doctor, --list-task-templates, or --list-profiles"
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

    serde_json::to_string_pretty(&view).map_err(|err| anyhow!(err))
}

fn render_effective_config_json(runtime: &RuntimeConfig) -> Result<String> {
    serde_json::to_string_pretty(&RuntimeConfigView::from_runtime(runtime))
        .map_err(|err| anyhow!(err))
}

fn render_effective_config_explanation_json(
    explanation: &EffectiveConfigExplanationView,
) -> Result<String> {
    serde_json::to_string_pretty(explanation).map_err(|err| anyhow!(err))
}

impl RuntimeConfigView {
    fn from_runtime(runtime: &RuntimeConfig) -> Self {
        Self {
            task: runtime.task.clone(),
            mode: if runtime.live { "live" } else { "dry-run" },
            live: runtime.live,
            model: runtime.model.display().to_string(),
            tokenizer: runtime
                .tokenizer
                .as_ref()
                .map(|path| path.display().to_string()),
            max_steps: runtime.max_steps,
            max_new_tokens: runtime.max_new_tokens,
            temperature: runtime.temperature,
            format: runtime.format,
            output_file: runtime
                .output_file
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
            Ok(runtime) => {
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

                if runtime.live {
                    if runtime.model.is_file() {
                        report.push(
                            DoctorStatus::Pass,
                            "live-model-path",
                            format!("Model file found: {}", runtime.model.display()),
                        );
                    } else {
                        report.push(
                            DoctorStatus::Warn,
                            "live-model-path",
                            format!(
                                "Live mode is enabled but model file was not found at {}.",
                                runtime.model.display()
                            ),
                        );
                    }

                    match runtime.tokenizer {
                        Some(tokenizer) if tokenizer.is_file() => {
                            report.push(
                                DoctorStatus::Pass,
                                "live-tokenizer-path",
                                format!("Tokenizer file found: {}", tokenizer.display()),
                            );
                        }
                        Some(tokenizer) => {
                            report.push(
                                DoctorStatus::Warn,
                                "live-tokenizer-path",
                                format!("Tokenizer file not found: {}", tokenizer.display()),
                            );
                        }
                        None => {
                            report.push(
                                DoctorStatus::Warn,
                                "live-tokenizer-path",
                                "No tokenizer path resolved for live mode. The runtime will only work if tokenizer discovery succeeds.",
                            );
                        }
                    }
                }

                if let Some(vitis_config) = runtime.vitis_config {
                    let path = PathBuf::from(&vitis_config);
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

fn render_doctor_report(report: &DoctorReport) -> String {
    let mut output = String::new();
    let (pass_count, warn_count, fail_count) = report.counts();

    let _ = writeln!(output, "WraithRun Doctor");
    let _ = writeln!(
        output,
        "Summary: {pass_count} pass, {warn_count} warn, {fail_count} fail"
    );

    for check in &report.checks {
        let _ = writeln!(
            output,
            "[{}] {}: {}",
            check.status.label(),
            check.name,
            check.detail
        );
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
    serde_json::to_string_pretty(&view).map_err(|err| anyhow!(err))
}

fn render_report(report: &RunReport, format: OutputFormat) -> Result<String> {
    match format {
        OutputFormat::Json => Ok(serde_json::to_string_pretty(report)?),
        OutputFormat::Summary => Ok(render_summary(report)),
        OutputFormat::Markdown => Ok(render_markdown(report)),
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

fn render_summary(report: &RunReport) -> String {
    let mut output = String::new();

    let _ = writeln!(output, "Task: {}", report.task);
    let _ = writeln!(output, "Turns: {}", report.turns.len());
    let _ = writeln!(output, "Final Answer: {}", report.final_answer);

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
    let _ = writeln!(output, "- Turns: {}", report.turns.len());
    let _ = writeln!(output, "- Final Answer: {}", report.final_answer);

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

fn build_vitis_config(runtime: &RuntimeConfig) -> Option<VitisEpConfig> {
    if runtime.vitis_config.is_none()
        && runtime.vitis_cache_dir.is_none()
        && runtime.vitis_cache_key.is_none()
    {
        return None;
    }

    Some(VitisEpConfig {
        config_file: runtime.vitis_config.clone(),
        cache_dir: runtime.vitis_cache_dir.clone(),
        cache_key: runtime.vitis_cache_key.clone(),
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
    use std::path::Path;
    use std::time::{SystemTime, UNIX_EPOCH};
    use std::{env, fs};

    use serde_json::json;

    use core_engine::{AgentTurn, RunReport, ToolCall};

    use super::{
        ensure_introspection_format_usage, merge_sources, render_doctor_report,
        render_doctor_report_json, render_effective_config_explanation_json,
        render_effective_config_json, render_profile_list, render_profile_list_json, render_report,
        render_task_template_list, render_task_template_list_json,
        resolve_effective_config_explanation, resolve_init_config_path, resolve_task_for_mode,
        resolve_task_for_run, run_init_config, Cli, DoctorReport, DoctorStatus, FileConfig,
        IntrospectionFormat, OutputFormat, SettingsFragment, TaskTemplate,
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
            list_profiles: false,
            introspection_format: IntrospectionFormat::Text,
            print_effective_config: false,
            explain_effective_config: false,
            init_config: false,
            init_config_path: None,
            force: false,
            config: None,
            profile: None,
            model: None,
            tokenizer: None,
            max_steps: None,
            max_new_tokens: None,
            temperature: None,
            live: false,
            dry_run: false,
            format: None,
            output_file: None,
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
            turns: vec![AgentTurn {
                thought: "<call>{...}</call>".to_string(),
                tool_call: Some(ToolCall {
                    tool: "scan_network".to_string(),
                    args: json!({ "limit": 40 }),
                }),
                observation: Some(json!({ "listener_count": 3, "listeners": [] })),
            }],
            final_answer: "Dry-run cycle complete.".to_string(),
        }
    }

    #[test]
    fn renders_json_output() {
        let report = sample_report();
        let rendered = render_report(&report, OutputFormat::Json).expect("json render should work");
        assert!(rendered.contains("\"task\""));
        assert!(rendered.contains("\"scan_network\""));
    }

    #[test]
    fn renders_summary_output() {
        let report = sample_report();
        let rendered =
            render_report(&report, OutputFormat::Summary).expect("summary render should work");
        assert!(rendered.contains("Task:"));
        assert!(rendered.contains("tool: scan_network"));
        assert!(rendered.contains("Final Answer:"));
    }

    #[test]
    fn renders_markdown_output() {
        let report = sample_report();
        let rendered =
            render_report(&report, OutputFormat::Markdown).expect("markdown render should work");
        assert!(rendered.contains("# WraithRun Report"));
        assert!(rendered.contains("## Turns"));
        assert!(rendered.contains("```json"));
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

        assert!(rendered.contains("\"summary\""));
        assert!(rendered.contains("\"pass\": 1"));
        assert!(rendered.contains("\"status\": \"fail\""));
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
        assert!(rendered.contains("\"templates\""));
        assert!(rendered.contains("\"syslog-summary\""));
        assert!(rendered.contains("\"supports_template_lines\": true"));
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
}
