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

- `findings`: severity-scored, actionable observations with evidence pointers.
- `live_fallback_decision`: machine-readable reason codes when fallback triggers.
- `run_timing` and `live_run_metrics`: latency and reliability telemetry for operations.
- `case_id` and evidence bundle artifacts for case tracking and auditability.

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

- Live preflight validation in runtime path to fail fast on missing model or tokenizer assets.
- Deterministic fallback controls with `--live-fallback-policy` and machine-readable fallback reason codes.
- Model-pack lifecycle operations: discover, validate, and benchmark candidate live packs.
- Evidence handling with bundle export, deterministic archive creation, and checksum verification.
- Automation contracts with `findings-v1` adapter output and severity-threshold exit policy.
- Baseline-aware drift workflows for persistence, account changes, and process-network risk scoring.
- Effective configuration introspection (`--print-effective-config` and `--explain-effective-config`).

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

Early-stage but production-minded for controlled defensive workflows.

Completed in v0.10.0:

- KV-cache and shared-buffer IO binding for live inference with GQO models
- Runtime compatibility checks with ~30 deterministic reason codes and structured remediation
- Zero-guess live setup with automatic model compatibility validation
- E2E live-success test lane (feature-gated)
- Cross-platform inference split: `onnx` (CPU EP) and `vitis` (AMD RyzenAI EP)

In progress:

- Code-signing and platform trust hardening for released binaries/installers
- Multi-backend inference abstraction (v1.3.0: DirectML, CoreML, CUDA/TensorRT, QNN)
- Local API server and web UI MVP (v1.0.0)

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
