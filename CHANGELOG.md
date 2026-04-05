# Changelog

All notable changes to this project will be documented in this file.

The format is inspired by Keep a Changelog and this project follows Semantic Versioning principles.

## Unreleased

### Added

- (none yet)

### Changed

- (none yet)

### Fixed

- (none yet)

## 1.6.0 - 2026-04-05

### Added

- **ReAct agent loop** (#92): Moderate/Strong investigation tiers now use an LLM-guided ReAct (Reason + Act) loop with dynamic tool dispatch. The agent reasons about which tool to call next based on observations so far. Basic tier retains fast template-driven execution.
- **Task-aware LLM synthesis** (#93): synthesis prompts include verbatim task context and structured output sections (Summary, Key Findings, Risk Assessment, Recommendations). Evidence budget increased from 1,500 to 3,000 chars per observation.
- **Temperature-scaled sampling** (#66): configurable temperature parameter for LLM token generation. Softmax probability sampling when temperature > 0; greedy decoding when temperature ≤ 0.
- **EP-aware debug logs** (#67): all inference debug messages now include the active execution provider (DirectML, CoreML, CUDA, TensorRT, QNN, CPU).
- **ONNX session caching** (#64): `SessionCache` struct lazily initializes and reuses the ONNX session and tokenizer across investigation steps, eliminating per-step session rebuild overhead.
- **KV-cache prefix reuse** (#65): prefix detection framework compares current prompt tokens against previous invocation with hit/miss metrics. Scaffolded for full KV-state reuse pending upstream `DynValue` clonability.
- **Model pack download** (#94): `--model-download <NAME>` CLI command with curated model manifest, SHA-256 checksum verification, and skip-if-exists logic.
- 7 new tests covering ReAct parsing, tier dispatch, unknown tool handling, and prompt formatting (281 total, up from 274).

## 1.5.0 - 2026-04-05

### Added

- **DirectML backend** (#56): Windows GPU inference via DirectX 12, priority 100, feature-gated behind `directml`.
- **CoreML backend** (#57): macOS/Apple Silicon inference, priority 100, feature-gated behind `coreml`.
- **CUDA backend** (#58): NVIDIA GPU inference, priority 200, feature-gated behind `cuda`.
- **TensorRT backend** (#58): NVIDIA TensorRT optimized inference, priority 250, feature-gated behind `tensorrt`.
- **QNN backend** (#59): Qualcomm Hexagon NPU inference, priority 280, feature-gated behind `qnn`.
- **`ModelFormat` enum** (#60): `Onnx`, `Gguf`, `SafeTensors` with auto-detection from file extension via `ModelFormat::from_path()`.
- **`QuantFormat` enum** (#61): `Fp32`, `Fp16`, `Int8`, `Int4`, `BlockQuantized(String)`, `Unknown` with filename-based detection via `QuantFormat::detect_from_path()`.
- `ExecutionProviderBackend::supported_formats()` — default `[Onnx]`, overridable per backend.
- `ExecutionProviderBackend::supported_quant_formats()` — each backend declares supported quantization formats.
- Backend conformance tests expanded with `supported_formats_is_non_empty` and `supported_quant_formats_is_non_empty`.
- 15 new unit tests for format/quant detection, display, and serialization.

- (none yet)

## 1.4.0 - 2026-04-12

### Added

- **Provider-aware doctor diagnostics** (#52): `wraithrun doctor` now enumerates all registered inference backends, reports their availability and priority, and surfaces per-backend diagnostic entries. The JSON doctor report includes a new `backends` array with structured diagnostic data.
- **CLI `--backend` flag and auto-select** (#53): new `--backend <NAME>` flag (and `WRAITHRUN_BACKEND` env var / `[inference] backend` TOML key) lets users explicitly choose an inference backend. When omitted or set to `"auto"`, the engine picks the highest-priority available backend. Includes helpful error messages listing available backends if an invalid name is given.
- **Integration test harness for multi-backend conformance** (#54): `backend_contract_tests!` macro generates 9 contract tests per backend (name, priority, availability, config keys, diagnostics, dry-run session). Five registry-level tests verify discovery, ordering, and fallback behavior. CPU conformance always runs; Vitis conformance is feature-gated.

### Changed

- `RunReport` now includes an optional `backend` field recording which inference backend was used.
- `run-report.schema.json` and `doctor-introspection.schema.json` updated to reflect new fields.

## 1.3.1 - 2026-04-12

### Added

- (none yet)

### Changed

- **Provider-agnostic `ModelConfig`** (#49): replaced `vitis_config: Option<VitisEpConfig>` with generic `backend_override: Option<String>` and `backend_config: HashMap<String, String>`. `VitisEpConfig` is retained as a CLI-level helper with `into_backend_config()` / `from_backend_config()` conversion methods.
- **Vitis EP reads via `backend_config` map** (#50): `onnx_vitis` functions (`discover_ort_dylib_path`, `build_base_session_builder_with_provider`, `build_session_with_vitis_cascade`) now read config values from the generic `backend_config` map instead of the Vitis-specific struct.
- **CPU EP unblocked by config refactor** (#51): `CpuBackend` and all non-Vitis callers now use `backend_override: None, backend_config: Default::default()`, removing any coupling to Vitis types.

### Fixed

- (none yet)

## 1.3.0 - 2026-04-12

### Added

- **CI/CD pipeline integration** (#103): first-party GitHub composite Action (`Shreyas582/wraithrun-action@v1`) with version resolution, binary caching, cross-platform install, scan execution, and JSON finding extraction. Also ships GitLab CI template, generic shell script for Jenkins/CircleCI, and an example GitHub Actions workflow.
- **CI integration guide** (`docs/ci-integration.md`): step-by-step docs for GitHub Actions, GitLab CI, and generic shell usage, covering exit code policy, output formats, scheduled scanning, and interpreting results.
- **`ExecutionProviderBackend` trait** (#47): hardware-agnostic backend abstraction in `inference_bridge::backend` with `name()`, `is_available()`, `priority()`, `build_session()`, and `diagnose()` methods. Includes `DiagnosticEntry` type for doctor integration and `InferenceSession` trait for provider-created sessions.
- **`ProviderRegistry`** (#48): runtime registry with `discover()`, `best_available()`, `get()`, `list()`, and `build_session_with_fallback()`. Auto-selects highest-priority available backend with cascading fallback on session init failure.
- **Built-in CPU backend**: always-available CPU execution provider (priority 0) with dry-run support and ONNX Runtime CPU session bridging.
- **Built-in Vitis backend** (cfg-gated): AMD Vitis AI NPU provider (priority 300, `vitis` feature) with environment-based availability detection and diagnostic checks.
- 12 new unit tests for backend trait, registry, and session functionality (245 total).

### Changed

- `inference_bridge` crate now exports `pub mod backend` alongside `pub mod onnx_vitis`.

### Added

- **Dashboard UX overhaul** (#99): 5-tab layout (Runs, Findings, Cases, Compare, Health) with SVG donut severity charts, clickable evidence chains, run comparison diff view, JSON/CSV export, real-time progress spinners, and case management panel.
- **Tool plugin API** (#102): extend WraithRun with external tool plugins via `tool.toml` manifests and subprocess JSON I/O.
  - `--tools-dir` and `--allowed-plugins` CLI flags.
  - Automatic plugin discovery, platform filtering, sandbox policy enforcement, and timeout support.
  - Plugin tools appear in `--doctor` output and `/api/v1/runtime/status` endpoint.
  - Example plugin in `examples/tools/hello_world/`.
  - Full documentation in `docs/plugin-api.md`.
- **Security professional documentation** (#100):
  - Four investigation playbooks: SSH key compromise, Windows triage, credential leak audit, persistence sweep.
  - MITRE ATT&CK mapping for all 8 built-in tools.
  - Threat model with attack surface, trust boundaries, and security controls.
  - Two anonymized sample investigation reports.
- Added `io-util` feature to workspace tokio dependency for plugin subprocess I/O.

### Changed

- (none yet)

### Fixed

- (none yet)

## 1.1.0 - 2026-04-04

### Added

- Structured JSON audit logging module (`audit.rs`) with 12 event types covering authentication, run lifecycle, case operations, and server lifecycle (#98).
  - File sink (JSON lines) and in-memory ring buffer for recent events.
  - `GET /api/v1/audit/events?limit=N` endpoint to query recent audit events.
  - `--audit-log <PATH>` CLI flag to enable file-based audit trail.
  - Events emitted for: `AuthSuccess`, `AuthFailure`, `RunCreated`, `RunCompleted`, `RunFailed`, `RunCancelled`, `CaseCreated`, `CaseUpdated`, `ToolExecuted`, `ToolPolicyDenied`, `ServerStarted`, `ServerStopped`.
- Case management API for grouping related investigation runs (#97).
  - `POST /api/v1/cases` — create a new investigation case with title and optional description.
  - `GET /api/v1/cases` — list all cases with run count aggregates.
  - `GET /api/v1/cases/{id}` — retrieve a single case with linked run statistics.
  - `PATCH /api/v1/cases/{id}` — update case title, description, or status (open/investigating/closed).
  - `GET /api/v1/cases/{id}/runs` — list runs linked to a case.
  - `case_id` field on `POST /api/v1/runs` request body to associate runs with cases.
  - SQLite schema v2 migration: `cases` table and `case_id` column on `runs` (auto-migrated).
- Evidence-backed narrative report format via `--format narrative` (#96).
  - Executive Summary with task, case reference, finding count, max severity, and duration.
  - Risk Assessment severity distribution table.
  - Investigation Timeline with step-by-step tool execution log.
  - Detailed Findings with confidence level, evidence chain, and recommended action.
  - Supplementary Findings and Conclusion sections.
  - Report metadata footer (model tier, inference mode, live metrics).

## 1.0.0 - 2026-04-06

### Added

- Local API server via `wraithrun serve --port 8080` with v1 REST endpoints (#23).
  - `GET /api/v1/health` — unauthenticated liveness check returning status, version, uptime.
  - `GET /api/v1/ready` — readiness check returning available tool count.
  - `POST /api/v1/runs` — start a new investigation run (accepts `task` and optional `max_steps`).
  - `GET /api/v1/runs` — list all runs sorted by creation time.
  - `GET /api/v1/runs/{id}` — retrieve a single run with full report.
  - `POST /api/v1/runs/{id}/cancel` — cancel a queued or running investigation.
  - `GET /api/v1/runtime/status` — runtime introspection (mode, tools, concurrency config).
- `wraithrun serve` subcommand alias (equivalent to `--serve`).
- Bearer token authentication on all API endpoints except `/health` (#25).
  - Auto-generated UUID token printed at startup; override with `--api-token <TOKEN>`.
  - Invalid/missing tokens return 401 Unauthorized with audit log warning.
- Request body size limit (1 MiB default) via `tower-http` `RequestBodyLimitLayer` (#25).
- Bind address locked to `127.0.0.1` for local-only access (#25).
- Concurrency limiter: configurable max concurrent runs (default 4) with 429 Too Many Requests response.
- SQLite-backed data store for persistent run and findings storage (#26).
  - Schema: `runs`, `findings`, `schema_version` tables with WAL journal mode.
  - Versioned migration framework (current schema v1) with idempotent migration.
  - `--database <PATH>` flag to enable persistent storage; in-memory by default.
  - `DataStore` API: `insert_run`, `update_run`, `get_run`, `list_runs`, `backup`, `export_json`.
  - Runs auto-persisted on creation and completion when database is configured.
- Embedded web dashboard at `GET /` for browser-based investigation management (#24).
  - Run list with real-time polling (5-second refresh).
  - Run detail slide panel with findings, severity badges, and final answer.
  - Findings explorer with severity filter buttons (Critical/High/Medium/Low/Info).
  - Live health panel: server status, version, uptime, tools, mode, concurrent runs.
  - New investigation form with Enter-key submission.
  - Run cancellation from the dashboard.
  - Token-gated access with local storage persistence.
  - Dark theme matching WraithRun visual identity.
- New `api_server` workspace crate containing server, routes, data store, and embedded dashboard.
- 19 new tests: 13 API route tests (health, auth reject, auth wrong token, ready, CRUD, cancel, runtime status, concurrency), 6 data store tests (insert/retrieve, update, list ordering, nonexistent, export, migration idempotency).

### Changed

- Workspace dependencies updated: added `axum 0.8`, `tower-http 0.6` (cors, trace, limit), `uuid 1.11` (v4, serde), `rusqlite 0.35` (bundled, backup).
- `tokio` workspace feature set expanded with `net` for TCP listener support.

## 0.13.0 - 2026-04-05

### Added

- Discrete confidence label model (`FindingConfidence` enum: Informational, Possible, Likely, Confirmed) derived from continuous confidence float using calibrated thresholds (#85).
- `confidence_label` field on all findings, auto-derived from confidence float via `confidence_to_label()`.
- `Finding::new()` constructor that auto-populates confidence label; `with_derived_label()` backfill method for existing findings.
- Declarative investigation templates replacing hardcoded keyword matching in `investigation_plan()` (#84).
- Six built-in investigation templates: broad-host-triage, ssh-key-investigation, persistence-analysis, network-exposure-audit, privilege-escalation-check, file-integrity-check.
- `resolve_investigation_template()` scores templates by keyword match count and falls back to broad-host-triage.
- Investigation templates shown in `--list-task-templates` output with tool lists.
- Task scope validation: detects out-of-scope tasks (cloud/AWS/Azure/GCP/Kubernetes/etc.) and emits an info finding instead of misleading host data (#83).
- `FindingRelevance` enum (Primary/Supplementary) for tagging finding relevance to the user's task (#86).
- `relevance` field on all findings; template-primary tools produce Primary findings, others produce Supplementary.
- `supplementary_findings` array in run report for findings separated in compact mode.
- Compact output mode now moves supplementary findings to `supplementary_findings` array; full mode shows all with relevance tags.
- 22 new tests covering confidence labels, template resolution, scope validation, and finding filtering.

### Changed

- `investigation_plan()` replaced by template-based `resolve_investigation_template()` — tool selection is now declarative and extensible.
- Run report JSON schema updated with `confidence_label`, `relevance`, and `supplementary_findings` fields.
- Integration tests updated to find tool turns by tool name rather than assuming fixed order.

## 0.12.0 - 2026-04-05

### Added

- Model capability probe (`ModelCapabilityProbe`) that estimates parameter count from ONNX file size, detects execution provider, measures smoke-inference latency, and reads tokenizer vocab size.
- `ModelCapabilityTier` enum (`Basic`, `Moderate`, `Strong`) with const-threshold classification based on estimated params and observed latency.
- Agent Phase 2 adapts behavior to capability tier: `Basic` skips LLM and uses deterministic summary, `Moderate` reduces evidence window (top-5 findings), `Strong` runs full synthesis.
- `basic_tier_summary()` for structured SUMMARY / FINDINGS / RISK / ACTIONS output when a Basic-tier model is detected.
- `model_capability` object in JSON run report including tier, estimated params, execution provider, latency, vocab size, and override flag.
- `--capability-override <basic|moderate|strong>` CLI flag to manually set capability tier, bypassing automatic probe classification.
- 18 new unit tests covering probe, classification, tier summary, report serialisation, and CLI override.

### Changed

- Run report JSON schema updated with `model_capability` object definition.
- Agent Phase 2 no longer assumes full model capability; tier-aware branching is now the default path.

## 0.11.0 - 2026-04-04

### Added

- Findings deduplication across overlapping tools using authority-based ranking (`deduplicate_findings`).
- Findings sorted by severity descending with confidence tiebreaker (`sort_findings`).
- `max_severity` field in JSON run report reflecting the highest severity across all findings.
- Compact output mode (default): omits `turns` array from JSON output to reduce payload size.
- `--output-mode <compact|full>` CLI flag to select between compact and full JSON output.
- Deterministic executive summary fallback when LLM output is low quality or empty (`quality_checked_final_answer`).
- Confidence scores rounded to two decimal places for consistency.
- Tool precondition checking to skip tools with known-failing prerequisites (e.g., `read_syslog` when log file does not exist).

### Changed

- Default JSON output is now compact mode (turns array omitted). Use `--output-mode full` to restore previous behavior.
- Agent Phase 2 now applies deduplication, sorting, quality checking, and max-severity derivation before emitting results.

## 0.10.0 - 2026-04-03

### Added

- Structured `remediation` field on doctor and model-pack validation checks, providing actionable fix guidance for every `reason_code`.
- `remediation_for_reason_code()` mapper covering ~30 reason codes across model path, tokenizer, runtime/session init, IO signature, and feature-gate scenarios.
- `validate_live_setup_report` now gates on `live-runtime-compatibility` FAIL checks in addition to required file passes—setup rejects incompatible models before writing config.
- Doctor text output now renders "Fix:" lines with remediation guidance after each non-PASS check.
- Integration tests for incompatible-model detection (onnx-gated), missing-model remediation, missing-tokenizer remediation, and setup rejection for corrupt models.
- KV-cache (past/present) live inference with shared-buffer IO binding for GQO models (#37).
- Runtime compatibility inspector (`inspect_runtime_compatibility`) with deterministic reason codes and `RuntimeCompatibilityReport` (#38).
- Feature-gated live-success E2E integration test lane (#39).
- Cross-platform inference split: `onnx` feature (CPU EP) and `vitis` feature (AMD RyzenAI EP).
- Deterministic two-phase agent architecture: Phase 1 runs keyword-matched tools without LLM; Phase 2 feeds gathered evidence to the LLM for structured synthesis.
- Batch prefill prompt ingestion replacing token-by-token loop (~4× first-token-latency improvement).

### Changed

- Doctor JSON schema now includes an optional `remediation` field on check items.
- `live-runtime-compatibility` check is now a hard gate in `live setup`—previously it was informational only.
- Agent loop replaced: the previous multi-turn ReAct loop is now a single deterministic investigation pass followed by one LLM synthesis call.
- Pre-existing integration tests updated to tolerate runtime-compatibility results across onnx/non-onnx builds.
- Test suite now covers 149 tests (no features) and 148 tests (onnx feature) with 0 failures.

### Fixed

- `validate_live_setup_report` previously only checked file presence; now also rejects models that fail ONNX session initialization.
- Agent investigation plan now respects `max_steps` limit; previously all planned tools ran regardless of the configured cap.
- CI live e2e artifact upload path corrected for package-rooted test output.
- CI live e2e lane switched from `inference_bridge/vitis` to `inference_bridge/onnx` with CPU-compatible model for reliable self-hosted execution.

## 0.9.1 - 2026-04-01

### Added

- Live runtime preflight regression tests covering missing model path, missing explicit tokenizer path, and valid model/tokenizer success flow.

### Changed

- README now follows a live-first onboarding flow with a practical quick start, audience-fit guidance, and advanced capability highlights.

### Fixed

- Live mode now fails fast in runtime when required assets are missing by validating model/tokenizer availability before engine/session startup.
- Release packaging workflow fixes for Linux package smoke-check path handling and Windows WiX x64 metadata alignment.

## 0.9.0 - 2026-04-01

### Added

- `wraithrun live setup` bootstrap flow to discover local model/tokenizer defaults and write a ready-to-run live profile.
- `--doctor --live --fix` remediation flow for common live-mode misconfigurations (path discovery, fallback-policy hardening, and operator guidance).
- Structured `reason_code` values on doctor checks to support machine-actionable failure/warning classification.
- Model-pack manager modes: `wraithrun models list`, `wraithrun models validate`, and `wraithrun models benchmark`.
- New built-in live presets: `live-fast`, `live-balanced`, and `live-deep`.
- Cross-platform release packaging pipeline producing Windows (`.zip`, `.msi`), Linux (`.tar.gz`, `.deb`, `.rpm`), and macOS (`.tar.gz`, `.pkg`) artifacts.
- Release assets now include `SBOM.spdx.json` and `SHA256SUMS` manifests.
- Live-mode telemetry fields: `run_timing` and `live_run_metrics` in run JSON output, including `first_token_latency_ms`, `total_run_duration_ms`, and reliability counters/rates.
- Findings adapter (`findings-v1`) summary now includes optional `live_run_metrics` for machine-consumable CI/SIEM scoring.
- CI live-metrics benchmark gate enforcing regression thresholds for `first_token_latency_ms` and `total_run_duration_ms` on every run.

### Changed

- `live_fallback_decision` metadata now includes required `reason_code` when fallback is triggered.
- Summary and markdown output now render fallback reason code alongside fallback reason.
- Summary and markdown output now render run timing and live reliability metrics when available.
- CLI/README reference examples now include live setup plus model-pack discovery, validation, and benchmark workflows.
- Release workflow now runs post-install smoke checks for native installer and archive artifacts before publishing.

### Fixed

- Live-mode integration coverage now validates fallback reason-code presence and doctor fix-handler behavior.
- Live fallback integration tests now validate `live_run_metrics` presence and top failure-reason propagation.

## 0.8.0 - 2026-03-31

### Added

- Model-pack doctor checks for live-mode readiness (`live-model-format`, `live-model-size`, `live-tokenizer-size`, `live-tokenizer-json`, `live-tokenizer-shape`).
- Configurable live fallback policy via `--live-fallback-policy <none|dry-run-on-error>`.
- New live-mode operations guide for deployment readiness and troubleshooting (`docs/live-mode-operations.md`).

### Changed

- Run report and findings adapter outputs now include optional `live_fallback_decision` metadata when fallback is triggered.
- Added integration-test coverage for live-mode fallback success/failure behavior and adapter fallback visibility.

### Fixed

- (none yet)

## 0.7.0 - 2026-03-31

### Added

- Operator case-workflow runbook examples covering collection, verification, and retention sequences.
- Added top-level `contract_version` field (`1.0.0`) to run and introspection JSON outputs.
- Published machine-readable JSON schemas and example payloads for run report plus doctor/tool/profile/task-template introspection outputs.
- Added `--automation-adapter findings-v1` for findings-only normalized automation output.
- Added severity-threshold exit policy controls (`--exit-policy`, `--exit-threshold`) for CI/SIEM gating.
- Published automation adapter schema and example payload files.

### Changed

- Expanded CLI examples with direct `SHA256SUMS` verification and baseline import via `raw_observations.json` path.
- JSON schema guidance now requires automation parsers to validate `contract_version` before strict parsing.
- Added practical CI/SIEM automation workflow docs for adapter output and severity-based exit behavior.

### Fixed

- Added regression coverage for evidence-bundle path edge cases (paths with spaces and direct checksum-manifest inputs).

## 0.6.2 - 2026-03-31

### Added

- Added `--evidence-bundle-archive <PATH>` for deterministic single-file tar export of evidence bundle artifacts.

### Changed

- (none yet)

### Fixed

- (none yet)

## 0.6.1 - 2026-03-31

### Added

- `--baseline-bundle <PATH>` runtime option to import baseline arrays from prior evidence bundles and auto-populate drift-aware tool arguments.
- `--verify-bundle <PATH>` mode for validating evidence bundle integrity against the recorded `SHA256SUMS` manifest.

### Changed

- `--introspection-format json` now supports `--verify-bundle` output for automation-friendly integrity checks.

### Fixed

- (none yet)

## 0.6.0 - 2026-03-31

### Added

- Optional case workflow fields via `--case-id` and `--evidence-bundle-dir`.
- Evidence bundle export artifacts: `report.json`, `raw_observations.json`, and `SHA256SUMS`.

### Changed

- Run report JSON now includes optional `case_id` when provided.
- Release workflow preflight now validates versioned changelog + upgrade docs before publishing tags.

### Fixed

- (none yet)

## 0.5.0 - 2026-03-31

### Added

- `--list-tools --tool-filter <QUERY>` now supports multi-term query matching for faster triage lookups.
- First-class `findings` report layer with severity, confidence, evidence pointer, and recommended action fields.
- New host coverage tools: `inspect_persistence_locations`, `audit_account_changes`, and `correlate_process_network`.
- Baseline-aware coverage arguments for persistence, account, and process-network tools (baseline/allowlist and expected-process inputs).
- New `capture_coverage_baseline` tool for generating reusable baseline arrays used by drift-aware coverage checks.

### Changed

- `--tool-filter` matching now normalizes separators (spaces, hyphens, underscores, punctuation) and applies terms across tool names and descriptions.
- Summary/markdown output now renders actionable findings before raw turn-by-turn observations.
- Dry-run task routing now maps persistence/account-change/process-network prompts to specialized coverage tools.
- Findings derivation now surfaces baseline drift and network risk-score signals from expanded coverage tool observations.
- Dry-run task routing now maps baseline capture prompts to `capture_coverage_baseline`.
- Findings derivation now emits a baseline-capture info finding when baseline snapshot observations are returned.

### Fixed

- (none yet)

## 0.4.1 - 2026-03-30

### Added

- `--describe-tool <NAME>` now accepts unique partial and hyphenated tool queries for faster operator lookups.

### Changed

- `--describe-tool` now returns an explicit ambiguous-query error when a partial query matches multiple tools.

### Fixed

- (none)

## 0.4.0 - 2026-03-30

### Added

- Tool discovery filtering via `--tool-filter <QUERY>` for `--list-tools` output.

### Changed

- (none)

### Fixed

- (none)

## 0.3.3 - 2026-03-30

### Added

- CLI tool catalog mode via `--list-tools` with text/JSON introspection output.
- CLI single-tool introspection via `--describe-tool <NAME>` with text/JSON output.

### Changed

- Release runbook now sets a 14-day patch cadence target whenever `Unreleased` has user-visible changes.

### Fixed

- CI follow-up fixes for strict lint/format gates after new introspection test coverage.

## 0.3.2 - 2026-03-30

### Added

- Dedicated CI job to run stdin integration tests on Linux and Windows.
- Release workflow checksum manifest generation (`SHA256SUMS`) for published artifacts.

### Changed

- Release planning docs now advance to `v0.4.0` as the next milestone target.
- Label source-of-truth now includes `milestone:v0.3.1` for repository sync consistency.

### Fixed

- UTF-16 BOM encoded task files are now accepted by `--task-file`.
- Clippy lint compatibility fixed for Rust 1.92 (`manual_is_multiple_of`).

## 0.3.1 - 2026-03-30

### Added

- CLI integration tests for stdin-based task input (`--task-stdin` and `--task-file -`).
- Introspection JSON schema reference in CLI documentation for automation users.

### Changed

- Git ignore rules now exclude generated runtime artifacts under `launch-assets`.

### Fixed

- (none)

## 0.3.0 - 2026-03-30

### Added

- CLI output format controls: `--format json|summary|markdown`.
- CLI export control: `--output-file` with automatic parent directory creation.
- CLI configuration controls: `--config`, `--profile`, and `--dry-run`.
- TOML configuration support with optional auto-load from `./wraithrun.toml`.
- Built-in execution profiles: `local-lab`, `production-triage`, and `live-model`.
- Environment-variable overrides for runtime settings (model, generation, output, logging, and Vitis knobs).
- Repository config template: `wraithrun.example.toml`.
- Doctor diagnostics mode via `--doctor` to validate config/profile/env/runtime readiness.
- Profile discovery mode via `--list-profiles`.
- Effective runtime preview mode via `--print-effective-config`.
- Source-attributed runtime explanation mode via `--explain-effective-config`.
- Config bootstrap mode via `--init-config` with `--init-config-path` and `--force` support.
- Built-in investigation task templates via `--task-template` and discovery mode `--list-task-templates`.
- Template parameter support via `--template-target` and `--template-lines` for path/line-sensitive templates.
- Task prompt file input via `--task-file` for reusable long-form investigations.
- Task prompt stdin input via `--task-stdin` (plus `--task-file -` shortcut).
- JSON introspection output for `--doctor`, `--list-task-templates`, and `--list-profiles` via `--introspection-format json`.

### Changed

- Default runtime logging now avoids polluting standard output, making report piping safer.
- Dry-run task routing now maps hash, network, log, and privilege prompts to expected tools more reliably.
- Runtime settings now resolve deterministically with precedence: CLI > env > config > defaults.
- Release runbook milestone steps now target `v0.3.0`.

### Fixed

- Incorrect tool selection in dry-run mode for hash-focused tasks.

## 0.2.2 - 2026-03-30

### Added

- Read the Docs integration files (`.readthedocs.yaml`, `mkdocs.yml`, docs requirements).
- Structured documentation set for public users (getting started, CLI/tool reference, sandbox, troubleshooting, upgrade notes).
- Docs CI workflow for strict MkDocs validation.

### Changed

- README introduction rewritten to clearly explain user value and practical use cases.

### Fixed

- Quality Gates CI stabilized by pinning Rust toolchain and aligning rustfmt behavior across environments.

## 0.2.1 - 2026-03-29

### Added

- Public usage examples guide for self-serve adoption.

### Changed

- README rewritten with user-first onboarding, binary usage, and practical CLI guidance.
- CI/CD and release docs updated for annotated tag flow and latest workflow behavior.
- CLI package and executable name changed from `agentic-cyber-cli` to `wraithrun`.

### Fixed

- Dependency review workflow now auto-detects dependency graph support and skips with warning when unavailable.
- Linux-target clippy dead-code failure in cross-platform CI.

## 0.2.0 - 2026-03-29

### Added

- Tokenizer-backed greedy decode loop for live ONNX/Vitis inference.
- Path and command policy enforcement for local tool sandboxing.
- Core ReAct integration tests using mocked inference responses.
- GitHub Actions automation for CI, release drafting, security checks, label sync, milestone bootstrap, and tagged releases.

### Changed

- Release workflow now runs preflight checks before publishing and includes Linux, macOS, and Windows artifacts.
- Release drafting configuration now uses resolved semantic versioning based on labels.
- Project docs expanded with release planning and CI/CD guidance.

## 0.1.0 - 2026-03-29

### Added

- Initial Rust workspace scaffold with modular crates.
- Core ReAct orchestration loop and tool-call parsing.
- Local cyber tool registry and host probing primitives.
- Dry-run inference behavior and feature-gated Vitis session bridge.
- CLI entrypoint for local runtime execution.
- Open-source governance docs (license, code of conduct, security, contribution guide).
