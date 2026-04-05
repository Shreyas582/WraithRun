# Upgrade Notes

## v1.7.1

- Dependency-only release. No breaking API changes.
- `sha2` 0.11 changes the `Display`/`LowerHex` impl on digest types. If you format hashes via `format!("{:x}", digest)`, switch to iterating over `digest.as_slice()` with per-byte hex formatting.
- CI action versions bumped (checkout v6, upload-artifact v7, download-artifact v8, setup-python v6, release-drafter v7). No user-facing impact unless you pin these in your own workflows.

## v1.7.0

### Breaking/visible changes

- **`RunReport` gains new fields**: `elapsed_ms` per tool entry, `llm_reasoning` on Moderate/Strong tier results, and `confidence` score with corroboration metadata. Consumers parsing JSON reports should handle these new optional fields.
- **Basic tier now emits a summary**: `basic_tier_summary_for_task()` produces a short task-context summary even without LLM synthesis. Previously Basic tier returned raw tool output only.
- **Small model warning**: models with fewer than 100M estimated parameters emit a warning at load time. This is informational and does not block execution.

### Migration

- No TOML config changes required.
- If you parse `RunReport` JSON, add handling for the new optional `elapsed_ms`, `llm_reasoning`, and `confidence` fields.
- Model packs with placeholder (all-zero) SHA-256 checksums will now be rejected. Re-download any affected packs.

## v1.6.0

### Breaking/visible changes

- **Agent dispatch changed by tier**: `Agent::run()` now dispatches Moderate and Strong tiers through a ReAct loop instead of the template-driven Phase 1 pipeline. Basic tier behavior is unchanged.
- **`run_prompt` temperature parameter**: `run_prompt()` and `run_prompt_shared_buffer()` now take a `temperature: f32` parameter. Pass `0.0` for greedy (previous behavior).
- **`generate()` caches sessions**: `OnnxVitisEngine::generate()` now caches the ONNX session internally via `Arc<Mutex<...>>`. No API change, but the engine is no longer stateless between calls.
- **New CLI flag `--model-download`**: added `--model-download <NAME|list>` for downloading recommended model packs. Mutually exclusive with `--task`/`--live`/`--doctor`.

### Migration

- If you call `run_prompt()` directly, add a `temperature` argument (use `0.0` for identical behavior to v1.5.0).
- No TOML config changes required.

## v1.5.0

### Breaking/visible changes

- **New backend feature flags**: `directml`, `coreml`, `cuda`, `tensorrt`, `qnn` feature flags added. Default build (no flags) still uses CPU/Vitis only.
- **`ModelFormat` and `QuantFormat` enums**: model config now exposes format and quantization metadata. These are informational and auto-detected; no config changes needed.
- **Backend `supported_formats()` / `supported_quant_formats()`**: new trait methods on `ExecutionProviderBackend`. Existing custom backends must implement these (default returns `[Onnx]` and `[Fp32, Fp16]`).

### Migration

- No TOML or CLI changes required. New backends activate only when their feature flag is enabled at compile time.
- If you implement a custom `ExecutionProviderBackend`, add `supported_formats()` and `supported_quant_formats()` methods.

## v1.4.0

### Breaking/visible changes

- **Doctor output includes backends**: `wraithrun doctor --json` now includes a `backends` array with per-backend diagnostic data. Consumers parsing the JSON should handle the new field.
- **`RunReport.backend` field**: run reports now include an optional `backend` string recording which inference backend was used.
- **`--backend` CLI flag**: new flag `--backend <NAME>` (or `WRAITHRUN_BACKEND` env var / `[inference] backend` TOML key) lets users force a specific backend. Default is `"auto"`.

### Migration

- No breaking API changes. The new `backend` field in `RunReport` is optional and defaults to `None` for backward compatibility.
- To pin a backend, set `--backend vitis` or add `backend = "vitis"` under `[inference]` in TOML.

## v1.3.1

### Breaking/visible changes

- **`ModelConfig` struct changed** (#49): the `vitis_config: Option<VitisEpConfig>` field is replaced by two new fields:
  - `backend_override: Option<String>` — optional backend name hint (e.g. `"vitis"`)
  - `backend_config: HashMap<String, String>` — generic key-value config map

  Both fields default to empty via `#[serde(default)]`, so TOML/JSON deserialization is backward-compatible if you don't set them.

- **`VitisEpConfig` is still available** as a helper. Use `into_backend_config()` to convert to the new map and `from_backend_config()` to reconstruct from one.

### Migration

Before:
```rust
let config = ModelConfig {
    // ...
    vitis_config: Some(VitisEpConfig {
        config_file: Some("/path/to/vitis.json".into()),
        cache_dir: None,
        cache_key: None,
    }),
};
```

After:
```rust
use std::collections::HashMap;

let config = ModelConfig {
    // ...
    backend_override: Some("vitis".to_string()),
    backend_config: HashMap::from([
        ("config_file".to_string(), "/path/to/vitis.json".to_string()),
    ]),
};
```

Or using the helper:
```rust
let vitis = VitisEpConfig {
    config_file: Some("/path/to/vitis.json".into()),
    cache_dir: None,
    cache_key: None,
};
let config = ModelConfig {
    // ...
    backend_override: Some("vitis".to_string()),
    backend_config: vitis.into_backend_config(),
};
```

## v1.3.0

### Breaking/visible changes

- `inference_bridge` now exports a new `backend` module. This is additive and fully backward-compatible — the existing `InferenceEngine` trait and `OnnxVitisEngine` are unchanged.
- The `backend::InferenceSession` trait introduces a synchronous `generate()` method that parallels the existing async `InferenceEngine::generate()`. Downstream callers can adopt it incrementally.

### New infrastructure

- **GitHub composite Action** (`action.yml`): use `Shreyas582/wraithrun-action@v1` in your CI workflows to run WraithRun scans with binary caching and cross-platform support.
- **CI templates**: GitLab CI (`ci-templates/gitlab-ci.yml`) and generic shell (`ci-templates/wraithrun-scan.sh`) for Jenkins, CircleCI, and other platforms.
- **CI integration guide** (`docs/ci-integration.md`): comprehensive setup docs for all supported CI systems.

### New types in `inference_bridge::backend`

| Type | Purpose |
|------|---------|
| `ExecutionProviderBackend` trait | Hardware-agnostic backend abstraction |
| `InferenceSession` trait | Provider-created inference session |
| `ProviderRegistry` | Runtime discovery and selection |
| `DiagnosticEntry` / `DiagnosticSeverity` | Backend self-check diagnostics |
| `ProviderInfo` | Backend metadata for listing |
| `BackendOptions` | Provider-specific config passthrough |
| `CpuBackend` | Built-in CPU provider (always available) |
| `VitisBackend` | Built-in Vitis NPU provider (cfg-gated) |

### Migration examples

To use the new backend registry:

```rust
use inference_bridge::backend::{ProviderRegistry, BackendOptions};
use inference_bridge::ModelConfig;

let registry = ProviderRegistry::discover();

// List available backends
for info in registry.list() {
    println!("{}: available={}, priority={}", info.name, info.available, info.priority);
}

// Auto-select best backend and build a session
let config = ModelConfig { /* ... */ };
let (backend_name, session) = registry
    .build_session_with_fallback(&config, &BackendOptions::new(), None)
    .expect("no backend available");
println!("Using backend: {backend_name}");

let output = session.generate("Analyze this host", 512)?;
```

To integrate WraithRun into GitHub Actions CI:

```yaml
- uses: Shreyas582/wraithrun-action@v1
  with:
    task: "Quick triage of ${{ github.sha }}"
    fail-on-severity: high
```

## v1.2.0

### Breaking/visible changes

- Two new CLI flags: `--tools-dir <PATH>` and `--allowed-plugins <name1,name2,...>`. These are optional and have no effect unless explicitly used.
- The `/api/v1/runtime/status` response now includes a `plugin_tools` array (empty when no plugins are loaded). Clients that strictly validate the response schema may need updating.
- The web dashboard has been completely redesigned with a 5-tab layout. Bookmarks or scripts that scraped the old single-page layout may need adjustment.
- Workspace tokio dependency now includes the `io-util` feature. This is transparent to users but may slightly increase binary size.

### New documentation

- **Investigation playbooks**: 4 step-by-step guides for common security tasks (SSH keys, Windows triage, credential leak, persistence sweep).
- **Plugin API**: full reference for writing external tool plugins (`docs/plugin-api.md`).
- **MITRE ATT&CK mapping**: all 8 built-in tools mapped to ATT&CK techniques.
- **Threat model**: attack surface analysis and security control documentation.
- **Sample reports**: 2 anonymized investigation reports demonstrating output format.

### Migration examples

To load an external plugin tool:

```bash
# Create a plugin directory with a tool.toml manifest
mkdir -p ~/.config/wraithrun/tools/my_scanner
# ... add tool.toml and executable (see docs/plugin-api.md)

# Run with the plugin enabled
wraithrun --allowed-plugins my_scanner --task "Scan host 10.0.0.1"
```

To verify plugin discovery:

```bash
wraithrun --allowed-plugins my_scanner --doctor
```

## v1.1.0

### Breaking/visible changes

- SQLite database schema automatically migrates from v1 to v2 on first use. The migration adds a `cases` table and a `case_id` column to the `runs` table. Existing databases are upgraded in-place; no manual action is required.
- New `narrative` output format available via `--format narrative`. Existing formats (`json`, `summary`, `markdown`) are unchanged.
- New API endpoints added under `/api/v1/cases/*` and `/api/v1/audit/events`. Existing endpoints are unchanged.

### Migration examples

To enable audit logging, pass the new `--audit-log` flag:

```powershell
wraithrun serve --audit-log ./audit.jsonl
```

To create and use cases via the API:

```bash
# Create a case
curl -X POST http://127.0.0.1:8080/api/v1/cases \
  -H "Authorization: Bearer <TOKEN>" \
  -H "Content-Type: application/json" \
  -d '{"title": "Incident 2026-04-04", "description": "Suspicious SSH activity"}'

# Start a run linked to a case
curl -X POST http://127.0.0.1:8080/api/v1/runs \
  -H "Authorization: Bearer <TOKEN>" \
  -H "Content-Type: application/json" \
  -d '{"task": "Investigate SSH keys", "case_id": "<CASE-UUID>"}'
```

To generate a narrative report:

```powershell
wraithrun --task "Check suspicious ports" --format narrative
```

## v1.0.0

### Breaking/visible changes

- New `api_server` crate added to the workspace. This is an additive change; the CLI continues to work identically without `--serve`.
- When `--serve` is used, WraithRun starts an HTTP server on `127.0.0.1:8080` (configurable via `--port`) instead of running a single investigation and exiting.
- Bearer token authentication is now required for all API endpoints except `/api/v1/health`. A random token is printed at startup unless `--api-token` is provided.
- SQLite persistence is opt-in via `--database <PATH>`. Without it, runs are stored in memory only.

### Migration examples

Start the API server:

```powershell
wraithrun serve --port 8080 --database ./wraithrun.db
```

Use a fixed API token for automation:

```powershell
wraithrun serve --api-token my-secret-token
```

Existing CLI workflows (non-serve) are completely unchanged.

## v0.13.0

### Breaking/visible changes

- Findings now include a `confidence_label` field (one of `informational`, `possible`, `likely`, `confirmed`) derived from the numeric `confidence` score. Existing `confidence` field is unchanged.
- Findings now include a `relevance` field (`primary` or `supplementary`) indicating whether the finding came from a template-selected tool. Default: `primary`.
- In compact output mode, supplementary findings are separated into a new `supplementary_findings` array. Full mode keeps all findings in the main array with relevance tags.
- Free-text tasks are now matched against declarative investigation templates that determine tool selection order. Previously, tool selection used a hardcoded keyword mapping.
- Tasks referencing out-of-scope domains (cloud, Kubernetes, email/phishing, SIEM) now return an informational scope-boundary finding instead of running the investigation.

### Migration examples

The `confidence_label` field is additive — existing parsers that ignore unknown fields are unaffected:

```json
{
  "title": "Unauthorized SSH key detected",
  "severity": "high",
  "confidence": 0.92,
  "confidence_label": "confirmed",
  "relevance": "primary"
}
```

If your pipeline consumes compact JSON and filters on `findings[]`, check for a new `supplementary_findings` array containing lower-relevance findings:

```json
{
  "findings": [ ... ],
  "supplementary_findings": [ ... ]
}
```

Confidence label thresholds:

- `confirmed`: score ≥ 0.90
- `likely`: score ≥ 0.72
- `possible`: score ≥ 0.55
- `informational`: score < 0.55

## v0.12.0

### Breaking/visible changes

- Agent Phase 2 now adapts behavior based on model capability tier. Basic-tier models skip LLM synthesis entirely and produce deterministic structured output. Moderate-tier models receive a reduced evidence window (top-5 findings). This may change output format and content compared to previous versions where full synthesis was always attempted.
- JSON run report now includes a `model_capability` object with tier classification, estimated parameters, execution provider, smoke latency, vocab size, and override flag.

### Migration examples

Automatic capability tiering is enabled by default in live mode. No action required unless you want to override:

```powershell
wraithrun --task "Check suspicious processes" --live --model C:/models/llm.onnx --tokenizer C:/models/tokenizer.json --capability-override strong
```

The `model_capability` field appears in JSON output:

```json
{
  "model_capability": {
    "tier": "moderate",
    "estimated_params_b": 3.5,
    "execution_provider": "CpuExecutionProvider",
    "smoke_latency_ms": 120,
    "vocab_size": 32000,
    "override": false
  }
}
```

## v0.11.0

### Breaking/visible changes

- Default JSON output is now **compact mode**, which omits the `turns` array. Use `--output-mode full` to restore the previous behavior that included intermediate reasoning steps.
- Findings are now automatically deduplicated across overlapping tools and sorted by severity descending. Previously, duplicate findings from multiple tools could appear in results.
- `max_severity` field is now included in the JSON run report.
- Confidence scores are now rounded to two decimal places. Previously, raw floating-point values were emitted.
- The agent now skips tools whose preconditions are known to fail (e.g., `read_syslog` when the log file does not exist), which may change the set of tools executed compared to previous runs.
- When LLM output is low quality or empty, a deterministic executive summary is generated instead of passing through the raw text.

### Migration examples

Restore full output with turns:

```powershell
wraithrun --task "Investigate unauthorized SSH keys" --output-mode full
```

Compact output (default) omits turns:

```powershell
wraithrun --task "Investigate unauthorized SSH keys"
# JSON output will not contain "turns" array
```

## v0.10.0

### Breaking/visible changes

- `validate_live_setup_report` now gates on `live-runtime-compatibility` FAIL checks. Previously, `live setup` only checked file presence; now it also rejects models that fail ONNX session initialization. Existing scripts that relied on setup succeeding with invalid models will see failures.
- Doctor JSON output now includes an optional `remediation` field on check items, providing actionable fix guidance for every `reason_code`.
- Doctor text output now renders "Fix:" lines after each non-PASS check.
- Cross-platform inference features are now split: `onnx` (CPU execution provider) and `vitis` (AMD RyzenAI execution provider). Use `--features inference_bridge/onnx` for generic CPU inference or `--features inference_bridge/vitis` for RyzenAI NPU acceleration.
- Batch prefill prompt ingestion replaces the token-by-token loop, reducing first-token latency by ~4×.
- The agent loop has been replaced with a deterministic two-phase architecture: Phase 1 runs keyword-matched tools without LLM interaction; Phase 2 feeds gathered evidence to the LLM for structured synthesis. `--max-steps` now limits the number of tools executed in Phase 1.

### Migration examples

Bootstrap live setup with automatic model compatibility validation:

```powershell
.\wraithrun.exe live setup --model C:/models/llm.onnx --tokenizer C:/models/tokenizer.json --config .\wraithrun.toml
```

If setup fails with `live-runtime-compatibility`, the error output now includes remediation guidance:

```
Live setup validation failed. Missing passing checks: live-runtime-compatibility
- [FAIL] live-runtime-compatibility: Unable to initialize ONNX session: ...
  Fix: Verify the model file is a valid ONNX model. Re-download if corrupted.
```

Doctor JSON now includes remediation metadata for automation consumers:

```powershell
.\wraithrun.exe --doctor --live --model C:/models/llm.onnx --introspection-format json
```

Each check with a `reason_code` now also carries a `remediation` string:

```json
{
  "status": "fail",
  "name": "live-runtime-compatibility",
  "reason_code": "runtime_session_init_failed",
  "remediation": "Verify the model file is a valid ONNX model. Re-download if corrupted."
}
```

Build with CPU-only ONNX inference (no RyzenAI NPU):

```powershell
cargo run -p wraithrun --features inference_bridge/onnx -- --live --model C:/models/llm.onnx --task "Investigate ..."
```

### Recommended checks after upgrade

- Validate compatible models pass doctor checks: `wraithrun --doctor --live --model <PATH> --introspection-format json`.
- If automation parsers consume doctor JSON, update them to handle the optional `remediation` field.
- If CI/CD pipelines invoke `live setup`, verify they handle the new runtime-compatibility gating (non-zero exit when model is incompatible).
- Review the `--features inference_bridge/onnx` vs `--features inference_bridge/vitis` split if building from source.

## v0.9.1

### Breaking/visible changes

- Live mode now performs runtime preflight checks before engine startup and fails immediately when model/tokenizer assets are missing or invalid.
- Release documentation is reorganized for live-first onboarding, with clearer operator fit guidance and advanced feature mapping.
- Release packaging workflow reliability improved for Linux package smoke checks and Windows WiX x64 metadata.

### Migration examples

Validate live readiness before active triage:

```powershell
.\wraithrun.exe --doctor --live --model C:/models/llm.onnx --tokenizer C:/models/tokenizer.json --introspection-format json
```

Run live mode with deterministic fallback for resilient case execution:

```powershell
.\wraithrun.exe --task "Investigate unauthorized SSH keys" --live --model C:/models/llm.onnx --tokenizer C:/models/tokenizer.json --live-fallback-policy dry-run-on-error
```

### Recommended checks after upgrade

- Confirm automation or wrappers treat new fast-fail preflight errors as setup/configuration failures and route them to operator remediation.
- Keep `--doctor --live` in preflight runbooks to validate model and tokenizer readiness before operational runs.
- If you consume release artifacts in deployment workflows, verify package smoke-check and installer ingestion paths remain green.

## v0.9.0

### Breaking/visible changes

- Added `wraithrun live setup` for one-command local live profile bootstrapping.
- Added `--doctor --live --fix` remediation flow with reason-coded fix guidance.
- Added model-pack manager operations: `wraithrun models list`, `wraithrun models validate`, and `wraithrun models benchmark`.
- Added live-mode telemetry in machine-readable output: `run_timing` and `live_run_metrics`.
- Added cross-platform package builds and smoke checks for Windows (`.zip`, `.msi`), Linux (`.tar.gz`, `.deb`, `.rpm`), and macOS (`.tar.gz`, `.pkg`).

### Migration examples

Bootstrap a local live profile automatically:

```powershell
.\wraithrun.exe live setup --config .\wraithrun.toml
```

Run live mode with deterministic fallback and inspect telemetry:

```powershell
.\wraithrun.exe --task "Investigate unauthorized SSH keys" --live --live-fallback-policy dry-run-on-error --format json
```

Validate and benchmark available model packs:

```powershell
.\wraithrun.exe models validate --introspection-format json
.\wraithrun.exe models benchmark --introspection-format json
```

### Recommended checks after upgrade

- Confirm automation parsers tolerate optional `run_timing` and `live_run_metrics` in run and adapter payloads.
- Validate any CI/SIEM gates that consume `live_success_rate`, `fallback_rate`, or `top_failure_reasons`.
- Verify release artifact ingestion processes include installer formats (`.msi`, `.deb`, `.rpm`, `.pkg`) and generated checksum/SBOM assets.

## v0.8.0

### Breaking/visible changes

- Added live-mode model-pack doctor checks for model and tokenizer readiness (`live-model-format`, `live-model-size`, `live-tokenizer-size`, `live-tokenizer-json`, `live-tokenizer-shape`).
- Added `--live-fallback-policy <none|dry-run-on-error>` for deterministic live-mode fallback behavior.
- Run report and findings adapter outputs now include optional `live_fallback_decision` metadata when fallback is triggered.

### Migration examples

Run live mode with deterministic dry-run fallback:

```powershell
.\wraithrun.exe --task "Investigate unauthorized SSH keys" --live --model C:/models/llm.onnx --live-fallback-policy dry-run-on-error
```

Validate live model-pack readiness before deployment:

```powershell
.\wraithrun.exe --doctor --live --model C:/models/llm.onnx --tokenizer C:/models/tokenizer.json --introspection-format json
```

### Recommended checks after upgrade

- Confirm automation parsers tolerate optional `live_fallback_decision` in run and adapter payloads.
- Keep `--doctor --live` in preflight runbooks for model-pack readiness checks.
- Decide whether pipelines should use fallback (`dry-run-on-error`) or fail-fast (`none`) based on incident handling policy.

## v0.7.0

### Breaking/visible changes

- Run and introspection JSON outputs now include top-level `contract_version` (`1.0.0`) for automation compatibility checks.
- Added machine-readable JSON schema and example files for run report and core introspection outputs under `docs/schemas/`.
- Added `--automation-adapter findings-v1` output mode for findings-only normalized pipeline ingestion.
- Added severity-threshold exit policy controls (`--exit-policy`, `--exit-threshold`) for deterministic CI/SIEM gating.
- Added case-workflow runbook examples for evidence collection, integrity verification, and retention operations.
- Expanded evidence-bundle path handling coverage to include direct `SHA256SUMS` verification and path-with-spaces workflows.

### Migration examples

Verify bundle integrity via direct checksum-manifest path:

```powershell
.\wraithrun.exe --verify-bundle ".\evidence\CASE-2026-IR-0042\run 01\SHA256SUMS"
```

Import baseline arrays directly from a `raw_observations.json` file path:

```powershell
.\wraithrun.exe --task "Audit account change activity in admin group membership" --baseline-bundle ".\evidence\CASE-2026-IR-0042\baseline\raw_observations.json"
```

Run with findings adapter and severity gate:

```powershell
.\wraithrun.exe --task "Investigate unauthorized SSH keys" --automation-adapter findings-v1 --exit-policy severity-threshold --exit-threshold high
```

### Recommended checks after upgrade

- Validate automation parsers check `contract_version` before strict field-level validation.
- Validate contract checks in CI against the published schema set in `docs/automation-contracts.md`.
- Validate adapter parsers against `docs/schemas/automation-adapter-findings-v1.schema.json`.
- Confirm pipeline exit behavior for each threshold (`info` through `critical`) in both dry-run and live mode.
- Validate your incident-response runbook uses the documented collection, verify, and retention sequence for case workflows.
- Add test coverage in downstream wrappers for bundle paths that include spaces or direct checksum-manifest references.

## v0.6.2

### Breaking/visible changes

- Added optional `--evidence-bundle-archive` runtime output for deterministic single-file evidence bundle export (`.tar`).

### Migration examples

Export a deterministic evidence bundle archive during a case-tagged run:

```powershell
.\wraithrun.exe --task "Investigate unauthorized SSH keys" --case-id CASE-2026-IR-0042 --evidence-bundle-archive .\evidence\CASE-2026-IR-0042.tar
```

### Recommended checks after upgrade

- Confirm receiving workflows can ingest `.tar` evidence bundles and still validate contents with `--verify-bundle` after extraction.
- If you generate archives in CI, ensure archive paths are unique per case to avoid accidental overwrite.

## v0.6.1

### Breaking/visible changes

- Added optional `--baseline-bundle` runtime input to import drift baseline arrays from prior evidence bundles.
- Added `--verify-bundle` mode to validate evidence bundle file integrity against `SHA256SUMS`.

### Migration examples

Load prior baseline arrays while running account drift checks:

```powershell
.\wraithrun.exe --task "Audit account change activity in admin group membership" --baseline-bundle .\evidence\CASE-2026-IR-0042
```

Verify evidence bundle integrity before sharing:

```powershell
.\wraithrun.exe --verify-bundle .\evidence\CASE-2026-IR-0042 --introspection-format json
```

### Recommended checks after upgrade

- Ensure baseline bundles retained for drift workflows include `raw_observations.json` with a `capture_coverage_baseline` tool observation.
- Gate evidence sharing steps on successful `--verify-bundle` checks to avoid distributing tampered or incomplete bundles.

## v0.6.0

### Breaking/visible changes

- Added optional `--case-id` for case-tagged run reports.
- Added optional `--evidence-bundle-dir` to export auditable investigation artifacts (`report.json`, `raw_observations.json`, `SHA256SUMS`).
- Run report JSON now includes optional `case_id` when set.

### Migration examples

Run with case metadata and evidence export:

```powershell
.\wraithrun.exe --task "Investigate unauthorized SSH keys" --case-id CASE-2026-IR-0042 --evidence-bundle-dir .\evidence\CASE-2026-IR-0042
```

### Recommended checks after upgrade

- Validate downstream automation tolerates optional `case_id` in JSON output.
- Confirm evidence bundle storage permissions and retention controls for exported artifacts.
- Verify checksum verification procedure is documented in incident-response runbooks.

## v0.5.0

### Breaking/visible changes

- Run report JSON now includes a first-class `findings[]` layer with severity, confidence, evidence pointers, and recommended actions.
- Summary and markdown output now render findings before turn-by-turn evidence.
- `--list-tools --tool-filter <QUERY>` now supports multi-term, separator-normalized matching.
- Added host coverage tools for persistence inventory, account/role snapshots, and process-network correlation.
- Coverage tools now support optional baseline/allowlist argument sets and emit drift/risk metrics (`baseline_new_count`, `newly_privileged_account_count`, `network_risk_score`).
- Added `capture_coverage_baseline` tool to generate reusable baseline arrays for persistence, account, and network drift workflows.

### Migration examples

Run a standard task and inspect findings in JSON output:

```powershell
.\wraithrun.exe --task "Investigate unauthorized SSH keys"
```

Filter tools using multiple terms:

```powershell
.\wraithrun.exe --list-tools --tool-filter "priv esc"
```

Run process-network correlation task:

```powershell
.\wraithrun.exe --task "Correlate process and network listener exposure" --format summary
```

### Recommended checks after upgrade

- If automation consumes run output JSON, parse `findings[]` and ignore unknown future fields for forward compatibility.
- Validate analyst runbooks treat `evidence_pointer` as a jump target into `turns[]` observations.
- Confirm triage dashboards can display severity/confidence and recommended action from findings.
- Validate runbooks include the new persistence/account/process-network coverage tasks for baseline collection.
- If your automation compares host state over time, feed baseline arrays into tool calls and alert on the new drift counters.
- Capture and store baseline snapshots periodically so drift-aware tool arguments can be refreshed from recent known-good host states.

## v0.4.1

### Breaking/visible changes

- `--describe-tool <NAME>` now accepts unique partial and hyphenated tool queries.
- `--describe-tool` now fails fast with an explicit ambiguous-query error when multiple tools match.

### Migration examples

Describe a tool with a hyphenated alias:

```powershell
.\wraithrun.exe --describe-tool hash-binary
```

Describe a tool with a unique partial query:

```powershell
.\wraithrun.exe --describe-tool privilege
```

### Recommended checks after upgrade

- If automation drives `--describe-tool`, ensure query strings remain unique or switch to full tool names.
- Confirm operator runbooks handle ambiguous-query failures by retrying with exact tool names.

## v0.4.0

### Breaking/visible changes

- Added `--tool-filter <QUERY>` for filtered tool discovery in `--list-tools` mode.

### Migration examples

Filter tool list by keyword:

```powershell
.\wraithrun.exe --list-tools --tool-filter hash
```

Filter tool list as JSON:

```powershell
.\wraithrun.exe --list-tools --tool-filter network --introspection-format json
```

### Recommended checks after upgrade

- Validate tooling that consumes `--list-tools` handles filtered result sets.
- Validate automation handles no-match failures when a filter is too restrictive.

## v0.3.3

### Breaking/visible changes

- Added tool catalog introspection mode via `--list-tools`.
- Added single-tool introspection mode via `--describe-tool <NAME>`.
- Added JSON contract output support for `--describe-tool` with stable `tool` object shape.

### Migration examples

List all tools:

```powershell
.\wraithrun.exe --list-tools
```

Describe one tool as JSON:

```powershell
.\wraithrun.exe --describe-tool hash_binary --introspection-format json
```

### Recommended checks after upgrade

- If you automate against introspection data, validate parsers for both `tools[]` (`--list-tools`) and `tool` (`--describe-tool`).
- For operator runbooks, map critical workflows to specific tool names using `--describe-tool` output.

## v0.3.2

### Breaking/visible changes

- Stdin-based task entry is now covered by dedicated integration tests in CI on Linux and Windows.
- Release artifacts now include a checksum manifest (`SHA256SUMS`) for integrity verification.
- `--task-file` now supports UTF-16 BOM encoded files commonly produced by Windows editors.

### Migration examples

Task from stdin:

```powershell
Get-Content .\incident-task.txt | .\wraithrun.exe --task-stdin --format summary
```

Task file with UTF-16 content:

```powershell
.\wraithrun.exe --task-file .\incident-task-utf16.txt --format summary
```

Checksum verification (PowerShell):

```powershell
Get-FileHash .\wraithrun-windows-x86_64.zip -Algorithm SHA256
Get-Content .\SHA256SUMS
```

### Recommended checks after upgrade

- Validate automation that reads introspection JSON still works with the documented schema contract.
- Verify local wrappers/scripts can pass task input via stdin where desired.
- For release consumers, verify downloaded artifact hashes against `SHA256SUMS`.

## v0.3.1

### Breaking/visible changes

- Added integration-test coverage for stdin task entry paths.
- Added documented introspection JSON schema examples for automation consumers.

### Recommended checks after upgrade

- Re-run automation that consumes `--introspection-format json` output.
- Confirm local scripted runs using `--task-stdin` and `--task-file -` still behave as expected.

## v0.2.1

### Breaking/visible changes

- Primary executable name is now `wraithrun`.
- Release artifacts are now named with `wraithrun-*` prefixes.

### Migration examples

Old source command:

```powershell
cargo run -p agentic-cyber-cli -- --task "Investigate unauthorized SSH keys"
```

New source command:

```powershell
cargo run -p wraithrun -- --task "Investigate unauthorized SSH keys"
```

Old binary:

- `agentic-cyber-cli.exe`

New binary:

- `wraithrun.exe`

### Recommended checks after upgrade

- Re-run your automation scripts with new command names.
- Verify release asset download names in CI/CD or deployment scripts.
- Confirm expected JSON output shape in downstream parsers.
