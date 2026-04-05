# WraithRun

**Local-first AI-powered incident triage for defenders.**

WraithRun runs security investigations on your machine using your own model (ONNX, GGUF, or SafeTensors). Point it at a task, and it reasons through host-level evidence (logs, listeners, persistence, accounts, processes) then delivers severity-scored findings with full audit trails. No cloud APIs, no data exfiltration, no vendor lock-in.

```powershell
wraithrun --task "Investigate unauthorized SSH keys" --live --model ./models/llm.onnx --tokenizer ./models/tokenizer.json
```

## Key Features

- **AI-guided investigation.** An agentic ReAct loop reasons about which tools to run, collects evidence iteratively, and synthesizes structured findings (Summary, Key Findings, Risk Assessment, Recommendations).
- **Runs entirely on your hardware.** Bring your own model in ONNX, GGUF, or SafeTensors format. Supports CPU, DirectML, CoreML, CUDA, TensorRT, QNN, and AMD Vitis backends.
- **Deterministic fallback.** If live inference fails, the agent falls back to dry-run mode so triage never stalls. Machine-readable reason codes explain every fallback.
- **Auditable evidence.** Case IDs, evidence bundles with SHA-256 checksums, and structured JSON output for analyst review and automation ingestion.
- **Host coverage out of the box.** Logs, network listeners, file hashes, privilege indicators, persistence drift, account drift, and process-network risk correlation.

## Quick Start

### Install

Download from [Releases](https://github.com/Shreyas582/WraithRun/releases) (Windows `.msi`/`.zip`, Linux `.deb`/`.rpm`/`.tar.gz`, macOS `.pkg`/`.tar.gz`).

Or build from source (Rust stable):

```powershell
git clone https://github.com/Shreyas582/WraithRun.git
cd WraithRun
cargo build -p wraithrun --release
```

### Get a Model

```powershell
wraithrun --model-download list                        # see available packs
wraithrun --model-download tinyllama-1.1b-chat         # download + SHA-256 verify
```

### Validate Setup

```powershell
wraithrun --doctor --live --model ./models/llm.onnx --tokenizer ./models/tokenizer.json
```

### Run an Investigation

```powershell
wraithrun --task "Investigate unauthorized SSH keys" --live \
  --model ./models/llm.onnx --tokenizer ./models/tokenizer.json \
  --live-fallback-policy dry-run-on-error
```

### Export Evidence

```powershell
wraithrun --task "Investigate unauthorized SSH keys" --case-id CASE-2026-IR-0042 \
  --live --model ./models/llm.onnx --tokenizer ./models/tokenizer.json \
  --evidence-bundle-dir ./evidence/CASE-2026-IR-0042

wraithrun --verify-bundle ./evidence/CASE-2026-IR-0042
```

### Dry-Run (No Model Required)

```powershell
wraithrun --task "Check suspicious listener ports" --dry-run --format summary
```

## What You Get Back

Each run returns a JSON report containing:

- **`findings`**: severity-scored, deduplicated observations with evidence pointers and recommended actions.
- **`max_severity`**: highest severity across all findings for quick alert routing.
- **`model_capability`**: tier classification, execution provider, latency, and parameters (live mode).
- **`live_fallback_decision`**: why fallback triggered, if applicable.
- **`case_id` / evidence bundle**: for chain-of-custody tracking.

Use `--format summary` for human-readable output, `--automation-adapter findings-v1` for pipeline ingestion, or `--output-mode full` for complete turn-by-turn reasoning.

## Useful Commands

```powershell
wraithrun --list-tools                                 # available investigation tools
wraithrun --list-profiles                              # built-in config profiles
wraithrun --task-template listener-risk --format summary
wraithrun --doctor --live --fix --model ./models/llm.onnx  # auto-fix setup issues
wraithrun serve                                        # start local API server + dashboard
```

## Features

**Agentic investigation.** Moderate/Strong-tier models use a ReAct loop that iteratively selects tools, collects observations, and synthesizes findings. Basic-tier models use fast template-driven execution with deterministic summaries.

**Multi-backend inference.** Pluggable execution providers (CPU, DirectML, CoreML, CUDA, TensorRT, QNN, Vitis). Auto-selects the best available backend, or pin one with `--backend <NAME>`. Supports ONNX, GGUF, and SafeTensors model formats with automatic quantization detection.

**Model management.** Download curated model packs with `--model-download`, automatic capability tiering (Basic/Moderate/Strong) based on model size and latency, and `--capability-override` for manual control.

**Operational reliability.** Preflight doctor checks, live-mode fallback with `--live-fallback-policy`, deterministic executive summaries when LLM quality is low, and configurable temperature for greedy vs. sampling decoding.

**Evidence and automation.** Case ID tracking, deterministic evidence bundles with checksum verification, `findings-v1` automation adapter, severity-threshold exit policy for CI/CD gating, and baseline-aware drift detection.

**API server and dashboard.** `wraithrun serve` exposes REST endpoints with bearer token auth, an embedded HTML dashboard, case management, and structured audit logging backed by SQLite.

## Documentation

| Resource | Link |
|----------|------|
| Full docs | [wraithrun.readthedocs.io](https://wraithrun.readthedocs.io/en/latest/) |
| Getting started | [docs/getting-started.md](docs/getting-started.md) |
| CLI reference | [docs/cli-reference.md](docs/cli-reference.md) |
| Tool reference | [docs/tool-reference.md](docs/tool-reference.md) |
| Live-mode operations | [docs/live-mode-operations.md](docs/live-mode-operations.md) |
| Usage examples | [docs/USAGE_EXAMPLES.md](docs/USAGE_EXAMPLES.md) |
| Automation contracts | [docs/automation-contracts.md](docs/automation-contracts.md) |
| Troubleshooting | [docs/troubleshooting.md](docs/troubleshooting.md) |
| Security sandbox | [docs/security-sandbox.md](docs/security-sandbox.md) |

## Project Status

**Latest release: [v1.6.0](https://github.com/Shreyas582/WraithRun/releases/tag/v1.6.0)**

Active development. See [CHANGELOG.md](CHANGELOG.md) for release history.

## Responsible Use

Use only on systems and networks you own or are explicitly authorized to assess.

## Contributing

- [CONTRIBUTING.md](CONTRIBUTING.md): contribution guide
- [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md): code of conduct
- [SECURITY.md](SECURITY.md): security policy

## License

MIT. See [LICENSE](LICENSE).
