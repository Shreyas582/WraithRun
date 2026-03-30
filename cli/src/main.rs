use std::path::PathBuf;

use anyhow::Result;
use clap::Parser;
use core_engine::agent::Agent;
use cyber_tools::ToolRegistry;
use inference_bridge::{ModelConfig, OnnxVitisEngine, VitisEpConfig};
use tracing_subscriber::EnvFilter;

#[derive(Debug, Parser)]
#[command(
    name = "agentic-cyber-runtime",
    about = "Local-first agentic cyber operations runtime"
)]
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

    #[arg(long)]
    vitis_config: Option<String>,

    #[arg(long)]
    vitis_cache_dir: Option<String>,

    #[arg(long)]
    vitis_cache_key: Option<String>,
}

#[tokio::main]
async fn main() -> Result<()> {
    init_tracing();

    let cli = Cli::parse();
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
    println!("{}", serde_json::to_string_pretty(&report)?);

    Ok(())
}

fn build_vitis_config(cli: &Cli) -> Option<VitisEpConfig> {
    if cli.vitis_config.is_none() && cli.vitis_cache_dir.is_none() && cli.vitis_cache_key.is_none() {
        return None;
    }

    Some(VitisEpConfig {
        config_file: cli.vitis_config.clone(),
        cache_dir: cli.vitis_cache_dir.clone(),
        cache_key: cli.vitis_cache_key.clone(),
    })
}

fn init_tracing() {
    let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    let _ = tracing_subscriber::fmt()
        .with_env_filter(env_filter)
        .with_target(false)
        .try_init();
}
