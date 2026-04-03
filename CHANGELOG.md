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
