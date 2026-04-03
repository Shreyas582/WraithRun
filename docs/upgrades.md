# Upgrade Notes

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
