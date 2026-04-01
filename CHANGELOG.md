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
