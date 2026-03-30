use std::path::PathBuf;
use std::{fmt::Write as _, fs};

use anyhow::Result;
use clap::{Parser, ValueEnum};
use core_engine::agent::Agent;
use core_engine::RunReport;
use cyber_tools::ToolRegistry;
use inference_bridge::{ModelConfig, OnnxVitisEngine, VitisEpConfig};
use serde_json::Value;
use tracing_subscriber::EnvFilter;

#[derive(Debug, Clone, Copy, ValueEnum)]
enum OutputFormat {
    Json,
    Summary,
    Markdown,
}

#[derive(Debug, Parser)]
#[command(name = "wraithrun", about = "Local-first cyber investigation runtime")]
struct Cli {
    #[arg(long)]
    task: String,

    #[arg(long, default_value = "./models/llm.onnx")]
    model: PathBuf,

    #[arg(long)]
    tokenizer: Option<PathBuf>,

    #[arg(long, default_value_t = 8)]
    max_steps: usize,

    #[arg(long, default_value_t = 256)]
    max_new_tokens: usize,

    #[arg(long, default_value_t = 0.2)]
    temperature: f32,

    #[arg(long, default_value_t = false)]
    live: bool,

    #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
    format: OutputFormat,

    #[arg(long)]
    output_file: Option<PathBuf>,

    #[arg(long, default_value_t = false, conflicts_with = "verbose")]
    quiet: bool,

    #[arg(long, short = 'v', default_value_t = false, conflicts_with = "quiet")]
    verbose: bool,

    #[arg(long)]
    vitis_config: Option<String>,

    #[arg(long)]
    vitis_cache_dir: Option<String>,

    #[arg(long)]
    vitis_cache_key: Option<String>,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    init_tracing(cli.quiet, cli.verbose);

    let vitis_config = build_vitis_config(&cli);

    let model_config = ModelConfig {
        model_path: cli.model,
        tokenizer_path: cli.tokenizer,
        max_new_tokens: cli.max_new_tokens,
        temperature: cli.temperature,
        dry_run: !cli.live,
        vitis_config,
    };

    let brain = OnnxVitisEngine::new(model_config);
    let tools = ToolRegistry::with_default_tools();
    let agent = Agent::new(brain, tools).with_max_steps(cli.max_steps);

    let report = agent.run(&cli.task).await?;
    let rendered = render_report(&report, cli.format)?;
    if let Some(path) = &cli.output_file {
        write_report_file(path, &rendered)?;
    }
    println!("{rendered}");

    Ok(())
}

fn render_report(report: &RunReport, format: OutputFormat) -> Result<String> {
    match format {
        OutputFormat::Json => Ok(serde_json::to_string_pretty(report)?),
        OutputFormat::Summary => Ok(render_summary(report)),
        OutputFormat::Markdown => Ok(render_markdown(report)),
    }
}

fn write_report_file(path: &PathBuf, report: &str) -> Result<()> {
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

fn build_vitis_config(cli: &Cli) -> Option<VitisEpConfig> {
    if cli.vitis_config.is_none() && cli.vitis_cache_dir.is_none() && cli.vitis_cache_key.is_none()
    {
        return None;
    }

    Some(VitisEpConfig {
        config_file: cli.vitis_config.clone(),
        cache_dir: cli.vitis_cache_dir.clone(),
        cache_key: cli.vitis_cache_key.clone(),
    })
}

fn init_tracing(quiet: bool, verbose: bool) {
    if quiet {
        return;
    }

    let default_level = if verbose { "debug" } else { "warn" };
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
    use serde_json::json;

    use core_engine::{AgentTurn, RunReport, ToolCall};

    use super::{render_report, OutputFormat};

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
}
