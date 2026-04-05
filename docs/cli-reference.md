# CLI Reference

Command name:

- wraithrun

Basic usage:

```text
wraithrun [OPTIONS] --task <TASK>
wraithrun [OPTIONS] --task-stdin
wraithrun [OPTIONS] --task-file <PATH>
wraithrun [OPTIONS] --task-template <TASK_TEMPLATE>
wraithrun serve [--port <PORT>] [--api-token <TOKEN>] [--database <PATH>]
wraithrun --doctor [OPTIONS]
wraithrun --list-task-templates
wraithrun --list-tools [OPTIONS]
wraithrun --describe-tool <NAME> [OPTIONS]
wraithrun --list-profiles [OPTIONS]
wraithrun --verify-bundle <PATH> [OPTIONS]
wraithrun --print-effective-config [OPTIONS]
wraithrun --explain-effective-config [OPTIONS]
wraithrun --init-config [--init-config-path <PATH>] [--force]
wraithrun models list [OPTIONS]
wraithrun models validate [OPTIONS]
wraithrun models benchmark [OPTIONS]
```

## Options

- `--task <TASK>`: investigation prompt.
- `--task-stdin`: read investigation prompt text from stdin.
- `--task-file <PATH>`: read investigation prompt text from a local file.
- `--task-template <TASK_TEMPLATE>`: built-in investigation prompt template.
- `--template-target <TEMPLATE_TARGET>`: optional target path for supported task templates.
- `--template-lines <TEMPLATE_LINES>`: optional line count for `syslog-summary` template.
- `--doctor`: run configuration/runtime diagnostics and exit.
- `--fix`: with `--doctor`, apply safe remediation handlers (path discovery, fallback-policy hardening, and permission/model-pack guidance).
- `--list-task-templates`: list built-in investigation templates and exit.
- `--list-tools`: list built-in local investigation tools and exit.
- `--tool-filter <QUERY>`: filter `--list-tools` results by name/description terms (case-insensitive, punctuation-normalized, multi-term support).
- `--describe-tool <NAME>`: render details for one tool and exit. Accepts case-insensitive full names plus unique partial or hyphenated queries.
- `--list-profiles`: list built-in and config-defined profiles, then exit.
- `--models-list`: list discovered live model packs and preset tuning (`wraithrun models list`).
- `--models-validate`: run live model-pack readiness checks for discovered packs (`wraithrun models validate`).
- `--models-benchmark`: rank discovered live packs by estimated responsiveness (`wraithrun models benchmark`).
- `--serve`: start the local API server and web dashboard. Alias: `wraithrun serve`.
- `--port <PORT>`: port for the API server. Default: `8080`. Requires `--serve`.
- `--api-token <TOKEN>`: bearer token for API authentication. Auto-generated if omitted. Requires `--serve`.
- `--database <PATH>`: SQLite database file for persistent run storage. In-memory if omitted. Requires `--serve`.
- `--verify-bundle <PATH>`: verify evidence bundle file integrity from a bundle directory or direct `SHA256SUMS` path.
- `--introspection-format <INTROSPECTION_FORMAT>`: format for introspection modes. Values: `text`, `json`. Default: `text`.
- `--print-effective-config`: print resolved runtime settings as JSON and exit.
- `--explain-effective-config`: print resolved runtime settings and per-field source attribution as JSON.
- `--init-config`: write a starter TOML config file and exit.
- `--init-config-path <INIT_CONFIG_PATH>`: output path for `--init-config`. Default: `./wraithrun.toml`.
- `--force`: allow overwrite of an existing file when `--init-config` is used.
- `--config <CONFIG>`: explicit TOML config file path.
- `--profile <PROFILE>`: named profile from built-ins or config file.
- `--model <MODEL>`: model path for live mode. Default fallback: `./models/llm.onnx`.
- `--tokenizer <TOKENIZER>`: tokenizer path used in live mode.
- `--max-steps <MAX_STEPS>`: max tools executed in the investigation plan. Default fallback: `8`.
- `--max-new-tokens <MAX_NEW_TOKENS>`: generation cap per model response. Default fallback: `256`.
- `--temperature <TEMPERATURE>`: generation temperature. Default fallback: `0.2`. Use `0` for greedy (deterministic) decoding; values above `0` enable softmax sampling (e.g., `0.1`–`0.3` for careful reasoning, `0.5`+ for creative exploration).
- `--model-download <NAME>`: download a curated model pack. Use `--model-download list` to see available packs. Downloads to `./models/`, verifies SHA-256, and skips if already present.
- `--live`: enable ONNX/Vitis live inference mode.
- `--dry-run`: force dry-run mode.
- `--live-fallback-policy <LIVE_FALLBACK_POLICY>`: live-mode fallback behavior. Values: `none`, `dry-run-on-error`. Default: `none`.
- `--format <FORMAT>`: output format. Values: `json`, `summary`, `markdown`. Default: `json`.
- `--automation-adapter <AUTOMATION_ADAPTER>`: automation ingestion envelope. Values: `findings-v1`.
- `--exit-policy <EXIT_POLICY>`: run exit behavior. Values: `none`, `severity-threshold`. Default: `none`.
- `--exit-threshold <EXIT_THRESHOLD>`: severity threshold for `severity-threshold` policy. Values: `info`, `low`, `medium`, `high`, `critical`. Default when policy is set: `medium`.
- `--output-file <OUTPUT_FILE>`: write rendered output to file.
- `--output-mode <OUTPUT_MODE>`: JSON output verbosity. Values: `compact`, `full`. Default: `compact`. Compact mode omits the `turns` array to reduce payload size.
- `--capability-override <CAPABILITY_OVERRIDE>`: manually set model capability tier, bypassing automatic probe classification. Values: `basic`, `moderate`, `strong`.
- `--case-id <CASE_ID>`: optional investigation case identifier. Allowed chars: alphanumeric plus `- _ . :`.
- `--evidence-bundle-dir <PATH>`: optional bundle export directory for `report.json`, `raw_observations.json`, and `SHA256SUMS`.
- `--evidence-bundle-archive <PATH>`: optional deterministic tar archive export path containing `report.json`, `raw_observations.json`, and `SHA256SUMS`.
- `--baseline-bundle <PATH>`: optional path to a prior evidence bundle directory (or `raw_observations.json`) used to import drift baseline arrays.
- `--quiet`: suppress runtime logs.
- `--verbose`: enable debug runtime logs.
- `--vitis-config <VITIS_CONFIG>`: Vitis provider config file path.
- `--vitis-cache-dir <VITIS_CACHE_DIR>`: Vitis cache directory.
- `--vitis-cache-key <VITIS_CACHE_KEY>`: Vitis cache key.
- `-h, --help`: print help.

When Vitis cache settings are omitted, WraithRun attempts to auto-discover them from model-adjacent artifacts (`dd_metastate_*`, `.cache`, `cache`, and `*_meta.json`).

## Resolution Order

Settings are resolved in this order:

1. CLI flags
2. Environment variables
3. Config file (`--config`, `WRAITHRUN_CONFIG`, or `./wraithrun.toml` if present)
4. Built-in defaults

When a profile is selected, built-in profile defaults apply before config file values.

## Doctor Mode

Run `--doctor` to validate:

- profile selection and availability,
- config file discovery/parsing,
- environment variable parsing,
- final effective runtime resolution,
- live-mode model-pack readiness checks:
	- model path exists,
	- model file is readable by the current operator account,
	- model extension is `.onnx`,
	- model file size is non-zero,
	- tokenizer path exists,
	- tokenizer file size is non-zero,
	- tokenizer JSON parses and includes top-level `model` key.

Use `--doctor --live --fix` to apply remediation handlers before checks execute. Current fix handlers cover:

- model/tokenizer path auto-discovery when values are not explicitly provided,
- fallback-policy hardening (`none` -> `dry-run-on-error`) for live-mode safety,
- direct guidance for permission and malformed model-pack issues with structured `reason_code` values.

Behavior:

- Exit code `0`: no failures (warnings may still be present).
- Non-zero exit code: one or more failures.

## Introspection Modes

`--introspection-format json` applies to:

- `--doctor`
- `--list-task-templates`
- `--list-tools`
- `--describe-tool`
- `--list-profiles`
- `--models-list`
- `--models-validate`
- `--models-benchmark`
- `--verify-bundle`

`--list-task-templates` output includes built-in task template names and their prompt text.

`--list-tools` output includes tool names, descriptions, and JSON argument schemas.

Current built-in coverage includes log tailing, listener inventory, file hashing, privilege vectors, persistence inventory, account-role snapshots, process-network correlation, and baseline capture for drift workflows.

Coverage tool argument highlights:

- `inspect_persistence_locations`: supports `limit`, optional `baseline_entries[]`, and optional `allowlist_terms[]`.
- `audit_account_changes`: supports optional `baseline_privileged_accounts[]` and `approved_privileged_accounts[]`.
- `correlate_process_network`: supports `limit`, optional `baseline_exposed_bindings[]`, and optional `expected_processes[]`.
- `capture_coverage_baseline`: supports optional `persistence_limit` and `listener_limit` and emits reusable baseline arrays.

When `--tool-filter` is used with `--list-tools`, only tools matching all query terms are returned.

`--tool-filter` matching behavior:

- case-insensitive,
- normalizes separators (spaces, hyphens, underscores, punctuation),
- supports multi-term queries such as `priv esc`.

`--describe-tool` output includes one matching tool object by name.

`--describe-tool` query resolution order:

- case-insensitive full-name match,
- normalized full-name match (hyphen and underscore treated equivalently),
- unique partial-name match,
- otherwise an error (`unknown` or `ambiguous`).

`--list-profiles` output includes:

- built-in profile names and purpose,
- config file path detection status,
- config-defined profile names,
- selected profile source (`built-in`, `config`, `built-in+config`, or `missing`) when `--profile` is set.

`wraithrun models list` output includes:

- live presets (`live-fast`, `live-balanced`, `live-deep`),
- additional live-capable config profiles,
- resolved model/tokenizer paths,
- readiness signal (`PASS`, `WARN`, `FAIL`) with warn/fail counts.

`wraithrun models validate` output includes:

- per-pack doctor-style check results,
- check-level reason codes when available,
- non-zero exit when one or more packs have failures.

`wraithrun models benchmark` output includes:

- ranked pack list,
- estimated token budget (`max_steps * max_new_tokens`),
- latency tier and benchmark score,
- recommended profile for fastest safe starting point.

`--print-effective-config` output includes the final merged runtime settings after applying precedence rules.

`--explain-effective-config` output includes:

- the same effective settings as `--print-effective-config`,
- a `sources` object showing where each field came from (default, profile, config, env, or CLI),
- selected profile and loaded config path context.

`--init-config` output includes the target path and suggested follow-up commands.

These modes are mutually exclusive with each other and with task execution modes.

### Introspection JSON Schema

When `--introspection-format json` is used, the output shape is stable per mode.

All introspection JSON payloads include a top-level `contract_version` string.

`--doctor --introspection-format json`:

```json
{
	"summary": {
		"pass": 0,
		"warn": 0,
		"fail": 0
	},
	"checks": [
		{
			"status": "pass",
			"name": "config-file",
			"detail": "Loaded config: ./wraithrun.toml"
		},
		{
			"status": "warn",
			"name": "live-tokenizer-path",
			"detail": "No tokenizer path resolved for live mode. The runtime will only work if tokenizer discovery succeeds.",
			"reason_code": "tokenizer_path_missing"
		}
	]
}
```

`--list-task-templates --introspection-format json`:

```json
{
	"templates": [
		{
			"name": "syslog-summary",
			"prompt": "Read and summarize last 200 lines from C:/Logs/agent.log",
			"supports_template_target": true,
			"supports_template_lines": true,
			"default_target": "C:/Logs/agent.log",
			"default_lines": 200
		}
	]
}
```

`--list-profiles --introspection-format json`:

```json
{
	"built_in_profiles": [
		{
			"name": "local-lab",
			"description": "dry-run, compact step/token budget, summary output"
		}
	],
	"config_path": "./wraithrun.toml",
	"config_profiles": ["team-default"],
	"selected_profile": {
		"name": "local-lab",
		"source": "built-in"
	}
}
```

`--list-tools --introspection-format json`:

```json
{
	"tools": [
		{
			"name": "hash_binary",
			"description": "Computes SHA-256 hash of a file for local integrity triage.",
			"args_schema": {
				"type": "object",
				"properties": {
					"path": {
						"type": "string"
					}
				},
				"required": ["path"]
			}
		}
	]
}
```

`--describe-tool hash_binary --introspection-format json`:

```json
{
	"tool": {
		"name": "hash_binary",
		"description": "Computes SHA-256 hash of a file for local integrity triage.",
		"args_schema": {
			"type": "object",
			"properties": {
				"path": {
					"type": "string"
				}
			},
			"required": ["path"]
		}
	}
}
```

`--verify-bundle ./evidence/CASE-2026-IR-0042 --introspection-format json`:

```json
{
	"bundle_dir": "./evidence/CASE-2026-IR-0042",
	"checksums_path": "./evidence/CASE-2026-IR-0042/SHA256SUMS",
	"summary": {
		"pass": 2,
		"fail": 0
	},
	"entries": [
		{
			"file": "report.json",
			"expected_sha256": "...",
			"actual_sha256": "...",
			"status": "pass"
		}
	]
}
```

`selected_profile.source` values:

- `built-in`
- `config`
- `built-in+config`
- `missing`

Schema compatibility policy:

- `contract_version` identifies the JSON contract family and version (current value: `1.0.0`).
- Automation should validate `contract_version` before enforcing strict field-level parsing.
- Patch releases (`0.x.Z`) keep existing JSON keys and meanings stable.
- Minor releases (`0.Y.0`) may add new JSON fields, but existing documented fields are not removed or renamed without an explicit changelog note.
- Automation should ignore unknown extra fields to remain forward-compatible.

Canonical schema and example files for automation validation live under `docs/schemas/` and are indexed in `docs/automation-contracts.md`.

## Run Report JSON Fields

Default run output (`--format json`) includes:

- `contract_version`: machine-readable JSON contract version.
- `task`: original task text.
- `case_id`: optional case identifier when set via runtime settings.
- `live_fallback_decision`: optional fallback metadata when live mode fails and configured policy reroutes execution.
	- includes `reason_code` for machine-actionable fallback classification.
- `run_timing`: optional run-level latency timing (`first_token_latency_ms`, `total_run_duration_ms`).
- `live_run_metrics`: optional live-mode reliability and latency metrics when `--live` is enabled.
	- includes `live_success_rate`, `fallback_rate`, and `top_failure_reasons` for automation scoring.
- `findings`: actionable finding list synthesized from collected evidence.
- `turns`: tool-thought-observation trace.
- `final_answer`: model/runtime conclusion string.

When `--evidence-bundle-dir` is set, the CLI also writes:

- `report.json`: full run report JSON.
- `raw_observations.json`: extracted turn-level observations for evidence sharing.
- `SHA256SUMS`: SHA-256 checksum manifest for bundle file integrity verification.

When `--evidence-bundle-archive` is set, the CLI writes a deterministic tar archive that contains the same three files in fixed order: `report.json`, `raw_observations.json`, then `SHA256SUMS`.

When `--verify-bundle` is set, the CLI validates `SHA256SUMS` entries against current bundle files and exits non-zero if any mismatches or missing files are detected.

When `--automation-adapter findings-v1` is set, run output switches to a findings-only automation envelope:

- `adapter`: fixed value `findings-v1`.
- `summary`: task/case context plus severity counts, optional `live_fallback_decision`, and optional `live_run_metrics`.
- `findings[]`: normalized finding entries with deterministic `finding_id` values (`F-0001`, `F-0002`, ...).

When `--exit-policy severity-threshold` is set, run exit behavior becomes severity-aware:

- exit `0` when no findings meet/exceed threshold,
- non-zero exit when at least one finding meets/exceeds threshold.

If `--exit-threshold` is omitted for `severity-threshold`, threshold defaults to `medium`.

When `--live-fallback-policy dry-run-on-error` is set and live inference fails, the runtime retries once in dry-run mode and records fallback details under `live_fallback_decision`.

Fallback metadata fields:

- `policy`: active live fallback policy.
- `reason`: human-readable fallback summary.
- `reason_code`: machine-readable classification (`model_path_missing`, `tokenizer_path_missing`, `tokenizer_json_invalid`, `permission_denied`, `live_runtime_error`, `unknown_live_error`).
- `live_error`: raw error text from live execution failure.
- `fallback_mode`: applied fallback runtime mode.

Live metrics fields (`live_run_metrics`):

- `first_token_latency_ms`: elapsed milliseconds before first model output in the overall live workflow.
- `total_run_duration_ms`: elapsed milliseconds for the complete live workflow (including fallback, when used).
- `live_attempt_duration_ms`: elapsed milliseconds spent in the initial live attempt.
- `live_attempt_count`: count of live attempts for the run (currently `1`).
- `live_success_count`: count of successful live attempts.
- `fallback_count`: count of fallback activations.
- `live_success_rate`: ratio of `live_success_count / live_attempt_count`.
- `fallback_rate`: ratio of `fallback_count / live_attempt_count`.
- `top_failure_reasons`: list of machine-readable reason-code/count pairs for live failures.

Coverage-oriented observations may also expose drift/risk metrics including `baseline_version`, `baseline_entries_count`, `baseline_new_count`, `newly_privileged_account_count`, `unknown_exposed_process_count`, and `network_risk_score` when those tools are used.

When `--baseline-bundle` is set, the runtime imports the latest `capture_coverage_baseline` observation from the referenced bundle and auto-injects arrays into drift-aware tool calls:

- `inspect_persistence_locations`: `baseline_entries`
- `audit_account_changes`: `baseline_privileged_accounts`, `approved_privileged_accounts`
- `correlate_process_network`: `baseline_exposed_bindings`, `expected_processes`

`findings[]` object fields:

- `title`: concise finding summary.
- `severity`: one of `info`, `low`, `medium`, `high`, `critical`.
- `confidence`: numeric confidence score (`0.00` to `1.00`).
- `confidence_label`: discrete confidence tier derived from the numeric score. One of `informational` (< 0.55), `possible` (≥ 0.55), `likely` (≥ 0.72), `confirmed` (≥ 0.90).
- `relevance`: finding relevance to the resolved investigation template. One of `primary` (from template-selected tools) or `supplementary` (from non-primary tools). Default: `primary`.
- `evidence_pointer`: pointer back to supporting evidence.
- `recommended_action`: analyst-facing next action.

In compact output mode, supplementary findings are separated into a `supplementary_findings` array. In full mode, all findings remain in the main `findings` array with their `relevance` tag.

`evidence_pointer` fields:

- `turn`: 1-based turn index when evidence comes from a tool observation (`null` when sourced from `final_answer`).
- `tool`: tool name associated with the evidence (`null` for non-tool evidence).
- `field`: JSON field path for supporting evidence (for example `observation.indicator_count`).

## Task Templates

Built-in template values for `--task-template`:

- `ssh-keys`
- `listener-risk`
- `hash-integrity`
- `priv-esc-review`
- `syslog-summary`

Template parameter support:

- `hash-integrity`: supports `--template-target`.
- `syslog-summary`: supports `--template-target` and `--template-lines`.
- `ssh-keys`, `listener-risk`, `priv-esc-review`: no template parameters.

## Investigation Templates

When a free-text `--task` is provided, the agent resolves a declarative investigation template by scoring keywords in the task description. The matched template determines which tools run and in what order.

Built-in investigation templates:

- **broad-host-triage**: default fallback. Runs all host-level tools.
- **ssh-key-investigation**: SSH key and account audit focus.
- **persistence-analysis**: autorun and persistence mechanism checks.
- **network-exposure-audit**: listener and network binding analysis.
- **privilege-escalation-check**: privilege escalation indicator checks.
- **file-integrity-check**: hash verification and file integrity analysis.

List investigation templates via `--list-task-templates`.

## Task Scope Validation

The agent validates that the task description falls within its supported scope (host-level cyber investigation). Tasks that reference out-of-scope domains (cloud infrastructure, container orchestration, email/phishing, SIEM) return an informational finding explaining the scope boundary instead of running the investigation.

## Built-In Profiles

- `local-lab`: dry-run, compact step/token budget, summary output.
- `production-triage`: dry-run, deeper loop budget, markdown output.
- `live-model`: enables live inference and a larger token budget.
- `live-fast`: live preset optimized for responsiveness.
- `live-balanced`: live preset balancing speed and depth.
- `live-deep`: live preset optimized for deeper iterative analysis.

## Examples

Dry-run mode:

```powershell
wraithrun --task "Check suspicious listener ports"
```

Persistence coverage:

```powershell
wraithrun --task "Inspect persistence locations for suspicious autoruns" --format summary
```

Account/role coverage:

```powershell
wraithrun --task "Audit account change activity in admin group membership" --format summary
```

Verify bundle integrity:

```powershell
wraithrun --verify-bundle .\evidence\CASE-2026-IR-0042 --introspection-format json
```

Verify using a direct checksum-manifest path (including paths with spaces):

```powershell
wraithrun --verify-bundle ".\evidence\CASE-2026-IR-0042\run 01\SHA256SUMS"
```

Import baseline arrays from a direct raw-observations file path:

```powershell
wraithrun --task "Audit account change activity in admin group membership" --baseline-bundle ".\evidence\CASE-2026-IR-0042\baseline\raw_observations.json"
```

Emit normalized findings adapter payload:

```powershell
wraithrun --task "Investigate unauthorized SSH keys" --automation-adapter findings-v1 --output-file .\launch-assets\findings-v1.json
```

Use severity-threshold exit policy for CI gates:

```powershell
wraithrun --task "Investigate unauthorized SSH keys" --automation-adapter findings-v1 --exit-policy severity-threshold --exit-threshold high
```

Case-tagged bundle export:

```powershell
wraithrun --task "Investigate unauthorized SSH keys" --case-id CASE-2026-IR-0042 --evidence-bundle-dir .\evidence\CASE-2026-IR-0042
```

Case-tagged deterministic archive export:

```powershell
wraithrun --task "Investigate unauthorized SSH keys" --case-id CASE-2026-IR-0042 --evidence-bundle-archive .\evidence\CASE-2026-IR-0042.tar
```

Process-network correlation:

```powershell
wraithrun --task "Correlate process and network listener exposure" --format summary
```

Capture reusable coverage baseline:

```powershell
wraithrun --task "Capture host coverage baseline for persistence account and network" --format summary
```

Task from stdin:

```powershell
Get-Content .\launch-assets\incident-task.txt | wraithrun --task-stdin --format summary
```

Task from file:

```powershell
wraithrun --task-file .\launch-assets\incident-task.txt --format summary
```

Template-driven dry-run mode:

```powershell
wraithrun --task-template listener-risk
```

Template-driven hash with custom target:

```powershell
wraithrun --task-template hash-integrity --template-target C:/Temp/suspicious.exe --format summary
```

Template-driven syslog summary with custom path and line count:

```powershell
wraithrun --task-template syslog-summary --template-target C:/Logs/security.log --template-lines 50 --format summary
```

List task templates:

```powershell
wraithrun --list-task-templates
```

List tools:

```powershell
wraithrun --list-tools
```

List tools as JSON:

```powershell
wraithrun --list-tools --introspection-format json
```

Filter tools by keyword:

```powershell
wraithrun --list-tools --tool-filter hash
```

Describe one tool:

```powershell
wraithrun --describe-tool hash_binary
```

Describe one tool as JSON:

```powershell
wraithrun --describe-tool hash_binary --introspection-format json
```

Use built-in profile:

```powershell
wraithrun --task "Check suspicious listener ports" --profile local-lab
```

Use config + profile:

```powershell
wraithrun --task "Investigate unauthorized SSH keys" --config .\wraithrun.example.toml --profile production-triage
```

Run doctor:

```powershell
wraithrun --doctor
```

Run doctor for specific profile/config combination:

```powershell
wraithrun --doctor --config .\wraithrun.example.toml --profile live-model
```

List profiles:

```powershell
wraithrun --list-profiles --config .\wraithrun.example.toml
```

List profiles as JSON:

```powershell
wraithrun --list-profiles --introspection-format json
```

Doctor report as JSON:

```powershell
wraithrun --doctor --introspection-format json
```

Print effective config:

```powershell
wraithrun --print-effective-config --profile production-triage --config .\wraithrun.example.toml
```

Explain effective config sources:

```powershell
wraithrun --explain-effective-config --profile production-triage --config .\wraithrun.example.toml
```

Initialize config at default path:

```powershell
wraithrun --init-config
```

Initialize config at custom path and overwrite existing file:

```powershell
wraithrun --init-config --init-config-path .\configs\team.toml --force
```

Live mode:

```powershell
wraithrun --live --model C:/models/llm.onnx --tokenizer C:/models/tokenizer.json --task "Investigate unauthorized SSH keys"
```

Live mode with deterministic fallback:

```powershell
wraithrun --live --model C:/models/llm.onnx --tokenizer C:/models/tokenizer.json --live-fallback-policy dry-run-on-error --task "Investigate unauthorized SSH keys"
```

List model packs and readiness:

```powershell
wraithrun models list
```

Validate all discovered model packs (non-zero exit on failures):

```powershell
wraithrun models validate --introspection-format json
```

Benchmark presets and choose a starting profile:

```powershell
wraithrun models benchmark --introspection-format json
```

Summary output with file export:

```powershell
wraithrun --task "Check suspicious listener ports" --format summary --output-file .\launch-assets\network-summary.txt
```

## Environment Variables

Runtime control variables:

- `WRAITHRUN_CONFIG`
- `WRAITHRUN_PROFILE`
- `WRAITHRUN_MODEL`
- `WRAITHRUN_TOKENIZER`
- `WRAITHRUN_MAX_STEPS`
- `WRAITHRUN_MAX_NEW_TOKENS`
- `WRAITHRUN_TEMPERATURE`
- `WRAITHRUN_LIVE`
- `WRAITHRUN_LIVE_FALLBACK_POLICY`
- `WRAITHRUN_FORMAT`
- `WRAITHRUN_AUTOMATION_ADAPTER`
- `WRAITHRUN_EXIT_POLICY`
- `WRAITHRUN_EXIT_THRESHOLD`
- `WRAITHRUN_OUTPUT_FILE`
- `WRAITHRUN_LOG` (`quiet`, `normal`, `verbose`)
- `WRAITHRUN_QUIET` (`true/false`, optional legacy override)
- `WRAITHRUN_VERBOSE` (`true/false`, optional legacy override)
- `WRAITHRUN_VITIS_CONFIG`
- `WRAITHRUN_VITIS_CACHE_DIR`
- `WRAITHRUN_VITIS_CACHE_KEY`
- `WRAITHRUN_ORT_DYLIB_PATH`

Example:

```powershell
$env:WRAITHRUN_PROFILE = "production-triage"
$env:WRAITHRUN_FORMAT = "summary"
wraithrun --task "Check suspicious listener ports" --format json
```

In the example above, CLI `--format json` wins over `WRAITHRUN_FORMAT=summary`.

## Config File Schema

Config file format is TOML. Top-level keys mirror runtime options; profiles are nested under `[profiles.<name>]`.

```toml
model = "./models/llm.onnx"
max_steps = 8
max_new_tokens = 256
temperature = 0.2
live_fallback_policy = "none"
format = "json"
log = "normal"

[profiles.local-lab]
max_steps = 6
format = "summary"
live = false

[profiles.production-triage]
max_steps = 12
format = "markdown"
live = false

[profiles.live-model]
live = true
format = "json"
```

Template file included in repository root:

- `wraithrun.example.toml`
