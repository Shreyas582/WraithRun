# WraithRun

WraithRun is a live-first incident triage CLI for defenders.

Run investigations locally with your own ONNX model, keep evidence auditable, and avoid stalled workflows with deterministic fallback when live inference fails.

## Why WraithRun For Live Triage

- Practical live triage: bring your own model and tokenizer, run on your infrastructure.
- Operational reliability: preflight checks, doctor diagnostics, and `dry-run-on-error` fallback.
- Automation-ready outputs: structured JSON, findings adapter, evidence bundles, and checksum verification.
- Useful host coverage out of the box: logs, listeners, file hashes, privilege indicators, persistence drift, account drift, and process-network risk.

## Who This Is For / Not For

Who this is for:

- Incident response and SOC teams that need fast host-level triage with auditable outputs.
- Security engineering teams that need local execution and data control.
- Teams integrating triage results into SIEM/SOAR or CI workflows.

Who this is not for:

- Teams expecting autonomous remediation without analyst oversight.
- Environments that cannot provide a local model/tokenizer for live mode.
- Workflows focused on broad internet scanning instead of host-centric investigation.

## Live Mode Quick Start (Recommended)

These steps are the fastest path to evaluate real operational value.

### 1. Install

Download a release binary from [Releases](https://github.com/Shreyas582/WraithRun/releases).

- Windows: `.msi` or `.zip`
- Linux: `.deb`, `.rpm`, or `.tar.gz`
- macOS: `.pkg` or `.tar.gz`

If you run from source (Rust stable):

```powershell
git clone https://github.com/Shreyas582/WraithRun.git
cd WraithRun
```

### 2. Validate Live Readiness

```powershell
wraithrun --doctor --live --model C:/models/llm.onnx --tokenizer C:/models/tokenizer.json --introspection-format json
```

Optional remediation for common setup issues:

```powershell
wraithrun --doctor --live --fix --model C:/models/llm.onnx --introspection-format json
```

### 3. Run Your First Live Investigation

```powershell
wraithrun --task "Investigate unauthorized SSH keys" --live --model C:/models/llm.onnx --tokenizer C:/models/tokenizer.json --live-fallback-policy dry-run-on-error --automation-adapter findings-v1
```

### 4. Export and Verify Evidence

```powershell
wraithrun --task "Investigate unauthorized SSH keys" --case-id CASE-2026-IR-0042 --live --model C:/models/llm.onnx --tokenizer C:/models/tokenizer.json --evidence-bundle-dir .\evidence\CASE-2026-IR-0042
wraithrun --verify-bundle .\evidence\CASE-2026-IR-0042 --introspection-format json
```

If you run from source, replace `wraithrun ...` with:

```powershell
cargo run -p wraithrun -- ...
```

And for live inference support in source builds, enable the feature:

```powershell
cargo run -p wraithrun --features inference_bridge/vitis -- --task "Investigate unauthorized SSH keys" --live --model C:/models/llm.onnx --tokenizer C:/models/tokenizer.json
```

## What You Get Back

Each run emits structured output that is useful to both analysts and automation:

- `max_severity`: highest severity level across all deduplicated findings.
- `findings`: severity-scored, actionable observations with evidence pointers (deduplicated and sorted).
- `model_capability`: capability tier, estimated parameters, execution provider, latency, and vocab size (live mode).
- `live_fallback_decision`: machine-readable reason codes when fallback triggers.
- `run_timing` and `live_run_metrics`: latency and reliability telemetry for operations.
- `case_id` and evidence bundle artifacts for case tracking and auditability.

Default output uses compact mode (omits intermediate reasoning). Use `--output-mode full` for complete turn-by-turn output.

## Common Operational Commands

```powershell
wraithrun --list-tools
wraithrun --task-template listener-risk --format summary
wraithrun models list --introspection-format json
wraithrun models validate --introspection-format json
wraithrun models benchmark --introspection-format json
wraithrun --list-profiles
```

Need an offline-only path first? Use dry-run mode:

```powershell
wraithrun --task "Check suspicious listener ports and summarize risk" --dry-run --format summary
```

## Advanced Features

**Agentic investigation (v1.6.0)**

- ReAct agent loop: Moderate/Strong tiers reason iteratively, choosing which tool to call based on observations so far.
- Task-aware LLM synthesis with structured output (Summary, Key Findings, Risk Assessment, Recommendations).
- Session caching eliminates per-step ONNX session rebuild; KV-cache prefix reuse detects shared prompt prefixes.
- Temperature-scaled sampling for creative vs. deterministic output (`temperature` config key).

**Model management**

- `--model-download list` shows curated model packs; `--model-download <NAME>` fetches and verifies with SHA-256.
- Capability tiering: automatic probe classifies models as Basic/Moderate/Strong and adapts agent behavior.
- `--capability-override` to manually set tier classification.
- Model-pack lifecycle: discover, validate, and benchmark candidate packs.

**Multi-backend inference (v1.4.0–v1.5.0)**

- Pluggable backends: CPU, Vitis (AMD RyzenAI), DirectML, CoreML, CUDA, TensorRT, QNN.
- `--backend <NAME>` flag (or auto-select by priority). EP-aware debug logging.
- `ModelFormat` (ONNX/GGUF/SafeTensors) and `QuantFormat` (FP32/FP16/INT8/INT4) auto-detection.

**Operational reliability**

- Live preflight validation to fail fast on missing model or tokenizer assets.
- Deterministic fallback controls with `--live-fallback-policy` and machine-readable reason codes.
- Findings deduplication, severity sorting, and quality-checked summaries.
- Compact output (default) with `--output-mode full` for verbose turn-by-turn output.

**Evidence and automation**

- Evidence bundle export with deterministic archive creation and checksum verification.
- Automation contracts with `findings-v1` adapter and severity-threshold exit policy.
- Baseline-aware drift workflows for persistence, account changes, and process-network risk.
- Effective configuration introspection (`--print-effective-config` and `--explain-effective-config`).

**Local API server and web UI (v1.0.0)**

- `wraithrun serve` with 7 REST endpoints, embedded HTML dashboard, bearer token auth.
- Case management, structured audit logging, and SQLite-backed data model.

## Documentation Map

- Hosted docs: https://wraithrun.readthedocs.io/en/latest/
- Getting started: [docs/getting-started.md](docs/getting-started.md)
- Live-mode operations: [docs/live-mode-operations.md](docs/live-mode-operations.md)
- CLI reference: [docs/cli-reference.md](docs/cli-reference.md)
- Tool reference: [docs/tool-reference.md](docs/tool-reference.md)
- Usage examples: [docs/USAGE_EXAMPLES.md](docs/USAGE_EXAMPLES.md)
- Automation contracts and schemas: [docs/automation-contracts.md](docs/automation-contracts.md)
- Troubleshooting: [docs/troubleshooting.md](docs/troubleshooting.md)
- Security sandbox controls: [docs/security-sandbox.md](docs/security-sandbox.md)

## Project Status

**Current release: [v1.6.0](https://github.com/Shreyas582/WraithRun/releases/tag/v1.6.0)** — Agentic Investigation Engine

| Version | Milestone | Highlights |
|---------|-----------|------------|
| v1.6.0 | Agentic Investigation Engine | ReAct agent loop, task-aware synthesis, temperature sampling, session caching, model-pack download |
| v1.5.0 | Concrete Hardware Backends | DirectML, CoreML, CUDA/TensorRT, QNN, ModelFormat/QuantFormat auto-detection |
| v1.4.0 | Multi-Backend Abstraction | Provider registry, `--backend` flag, provider-aware doctor diagnostics |
| v1.3.0 | Backend Trait Extraction | `ExecutionProviderBackend` trait, CPU/Vitis trait impls, conformance harness |
| v1.2.0 | Integrations & Extensibility | Tool plugin API, CI/CD pipeline integration |
| v1.1.0 | Professional Workflow Depth | Narrative reports, case management API, structured audit logging |
| v1.0.0 | Local API Server & Web UI | REST API, embedded dashboard, bearer auth, SQLite data model |

See [CHANGELOG.md](CHANGELOG.md) for full release history.

## Responsible Use

Use only on systems and networks you own or are explicitly authorized to assess.

## Contributing and Governance

- Contribution guide: [CONTRIBUTING.md](CONTRIBUTING.md)
- Code of conduct: [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md)
- Security policy: [SECURITY.md](SECURITY.md)
- Changelog: [CHANGELOG.md](CHANGELOG.md)
- Release plan: [docs/RELEASE_PLAN.md](docs/RELEASE_PLAN.md)
- CI/CD details: [docs/CI_CD.md](docs/CI_CD.md)

## License

MIT. See [LICENSE](LICENSE).
