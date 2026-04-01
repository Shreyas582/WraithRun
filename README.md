# WraithRun

WraithRun helps you investigate suspicious host activity quickly, without sending your data to the cloud.

It is a local-first command-line tool for defenders and security engineers that:

- runs guided local checks (logs, network listeners, file hashes, privilege indicators),
- expands host coverage with persistence inventory, account/role snapshots, and process-network correlation,
- captures reusable coverage baselines so drift-aware checks can compare current host state against known-good snapshots,
- supports baseline-aware drift signals and process-network risk scoring for faster triage prioritization,
- supports case-tagged investigations and evidence bundle export (report, raw observations, checksums),
- verifies evidence bundle integrity against `SHA256SUMS` for auditable evidence handling,
- keeps evidence on your own machine by default,
- returns a structured JSON report you can archive, diff, or automate around.

If you need fast, repeatable endpoint triage with auditable output, WraithRun is built for that workflow.

## Documentation

- Hosted docs (Read the Docs): https://wraithrun.readthedocs.io/en/latest/
- Docs setup and publishing runbook: [docs/READTHEDOCS_SETUP.md](docs/READTHEDOCS_SETUP.md)

## Start Here

### Option A: Use release binaries (fastest)

1. Go to [Releases](https://github.com/Shreyas582/WraithRun/releases).
2. Download the asset for your OS:
    - `wraithrun-windows-x86_64.zip`
    - `wraithrun-linux-x86_64.tar.gz`
    - `wraithrun-macos-x86_64.tar.gz`
3. Extract and run a task (dry-run mode, no model required).

Windows:

```powershell
.\wraithrun.exe --task "Investigate unauthorized SSH keys"
```

Linux/macOS:

```bash
./wraithrun --task "Investigate unauthorized SSH keys"
```

### Option B: Run from source

Prerequisite: Rust stable toolchain.

```powershell
git clone https://github.com/Shreyas582/WraithRun.git
cd WraithRun
cargo run -p wraithrun -- --task "Investigate unauthorized SSH keys"
```

Need copy-paste scenarios for common investigations? See [docs/USAGE_EXAMPLES.md](docs/USAGE_EXAMPLES.md).

## What You Get Back

Each run prints a JSON report:

```json
{
    "contract_version": "1.0.0",
    "task": "Investigate unauthorized SSH keys",
    "findings": [
        {
            "title": "Privilege escalation indicators detected (1)",
            "severity": "medium",
            "confidence": 0.74,
            "evidence_pointer": {
                "turn": 1,
                "tool": "check_privilege_escalation_vectors",
                "field": "observation.indicator_count"
            },
            "recommended_action": "Review potential_vectors and verify whether elevated rights are expected."
        }
    ],
    "turns": [
        {
            "thought": "...",
            "tool_call": { "tool": "check_privilege_escalation_vectors", "args": {} },
            "observation": { "indicator_count": 0 }
        }
    ],
    "final_answer": "..."
}
```

Top-level fields:

- `contract_version`: machine-readable JSON contract version for automation compatibility checks.
- `task`: your input task string.
- `case_id`: optional investigation identifier when provided via CLI/config/env.
- `findings`: normalized actionable findings with severity, confidence, evidence pointer, and recommended action.
- `turns`: intermediate reasoning/tool interaction history.
- `final_answer`: the model/runtime conclusion.

Coverage-oriented observations commonly include drift and risk metrics such as:

- `baseline_version`, `baseline_entries_count`, and `baseline_exposed_binding_count` from baseline capture,
- `baseline_new_count` and `actionable_suspicious_count` from persistence checks,
- `newly_privileged_account_count` and `unapproved_privileged_account_count` from account snapshots,
- `network_risk_score` and `unknown_exposed_process_count` from process-network correlation.

## Use Live Model Inference (Optional)

By default, WraithRun runs in dry-run mode. To use your own ONNX model:

Prerequisites:

- ONNX model file (for example `llm.onnx`).
- Matching `tokenizer.json`.
- ONNX Runtime deployment with Vitis execution provider support.

Validate feature build:

```powershell
cargo check -p inference_bridge --features vitis
```

Run live mode:

```powershell
cargo run -p wraithrun --features inference_bridge/vitis -- --live --model C:/models/llm.onnx --tokenizer C:/models/tokenizer.json --task "Investigate unauthorized SSH keys"
```

Optional Vitis tuning flags:

- `--vitis-config <path>`
- `--vitis-cache-dir <path>`
- `--vitis-cache-key <key>`

## CLI Options

Run help:

```powershell
cargo run -p wraithrun -- --help
```

Common options:

- `--task <TASK>` investigation prompt (required unless `--task-stdin`, `--task-file`, `--task-template`, or a mode command is used).
- `--task-stdin` read investigation prompt text from stdin (pipe content into the command).
- `--task-file <PATH>` read investigation prompt text from a local file.
- `--task-template <NAME>` use a built-in investigation prompt template.
- `--template-target <PATH>` optional target path for supported templates (`hash-integrity`, `syslog-summary`).
- `--template-lines <N>` optional line count for `syslog-summary` template (default `200`).
- `--doctor` run runtime health checks and configuration diagnostics.
- `--list-task-templates` show available built-in investigation templates.
- `--list-tools` list available local investigation tools and argument schemas.
- `--tool-filter <QUERY>` filter `--list-tools` output by tool name or description terms (case-insensitive, punctuation-normalized, supports multi-word queries).
- `--describe-tool <NAME>` show details for one tool (name, description, argument schema). Accepts case-insensitive full names plus unique partial or hyphenated queries.
- `--list-profiles` list built-in and config-defined profiles.
- `--introspection-format <text|json>` output format for `--doctor`, `--list-task-templates`, `--list-tools`, `--describe-tool`, `--list-profiles`, and `--verify-bundle` (default `text`).
- `--print-effective-config` render the resolved runtime settings as JSON and exit.
- `--explain-effective-config` render resolved runtime settings plus per-field source attribution.
- `--init-config` write a starter TOML config file and exit.
- `--init-config-path <PATH>` output path used by `--init-config` (default `./wraithrun.toml`).
- `--force` overwrite an existing file when used with `--init-config`.
- `--config <CONFIG>` load settings from a TOML file (default auto-load: `./wraithrun.toml` when present).
- `--profile <PROFILE>` apply a named profile from built-ins or config file.
- `--live` enables model inference mode (default is dry-run).
- `--dry-run` forces dry-run mode (overrides profile/config live mode).
- `--model <MODEL>` model path (default `./models/llm.onnx`, unless overridden by config/env).
- `--tokenizer <TOKENIZER>` tokenizer path for live mode.
- `--max-steps <N>` max agent turns (default `8`).
- `--max-new-tokens <N>` generation cap per response (default `256`).
- `--temperature <F>` generation temperature (default `0.2`).
- `--format <json|summary|markdown>` output format (default `json`).
- `--output-file <PATH>` write rendered report to file and create directories if needed.
- `--case-id <CASE_ID>` attach a case identifier to the run report (`A-Z a-z 0-9 - _ . :`, max 128 chars).
- `--evidence-bundle-dir <PATH>` export `report.json`, `raw_observations.json`, and `SHA256SUMS` to a bundle directory.
- `--evidence-bundle-archive <PATH>` export a deterministic tar archive containing `report.json`, `raw_observations.json`, and `SHA256SUMS`.
- `--baseline-bundle <PATH>` import baseline arrays from a prior evidence bundle directory (or `raw_observations.json`) and auto-populate drift-aware tool arguments.
- `--verify-bundle <PATH>` verify an evidence bundle directory (or direct `SHA256SUMS` path) against recorded checksums.
- `--quiet` suppress runtime logs.
- `--verbose` enable debug-level runtime logs.

Example output controls:

```powershell
cargo run -p wraithrun -- --task "Check suspicious listener ports and summarize risk" --format summary
```

```powershell
cargo run -p wraithrun -- --task "Check suspicious listener ports and summarize risk" --output-file .\launch-assets\network-report.json
```

Run a task from a prompt file:

```powershell
cargo run -p wraithrun -- --task-file .\launch-assets\incident-task.txt --format summary
```

Run a task from stdin:

```powershell
Get-Content .\launch-assets\incident-task.txt | cargo run -p wraithrun -- --task-stdin --format summary
```

List built-in task templates:

```powershell
cargo run -p wraithrun -- --list-task-templates
```

List available tools:

```powershell
cargo run -p wraithrun -- --list-tools
```

List available tools as JSON:

```powershell
cargo run -p wraithrun -- --list-tools --introspection-format json
```

Filter tool list by keyword:

```powershell
cargo run -p wraithrun -- --list-tools --tool-filter hash
```

Filter tools with multi-word query terms:

```powershell
cargo run -p wraithrun -- --list-tools --tool-filter "priv esc"
```

Run persistence coverage check:

```powershell
cargo run -p wraithrun -- --task "Inspect persistence locations for suspicious autoruns" --format summary
```

Run account/privileged-role snapshot:

```powershell
cargo run -p wraithrun -- --task "Audit account change activity in admin group membership" --format summary
```

Run process-network correlation:

```powershell
cargo run -p wraithrun -- --task "Correlate process and network listener exposure" --format summary
```

Run a case-tagged investigation and export an evidence bundle:

```powershell
cargo run -p wraithrun -- --task "Investigate unauthorized SSH keys" --case-id CASE-2026-IR-0042 --evidence-bundle-dir .\evidence\CASE-2026-IR-0042
```

Export a deterministic single-file evidence bundle archive:

```powershell
cargo run -p wraithrun -- --task "Investigate unauthorized SSH keys" --case-id CASE-2026-IR-0042 --evidence-bundle-archive .\evidence\CASE-2026-IR-0042.tar
```

Capture reusable coverage baseline:

```powershell
cargo run -p wraithrun -- --task "Capture host coverage baseline for persistence account and network" --format summary
```

Run a drift check with baseline arrays imported from a prior bundle:

```powershell
cargo run -p wraithrun -- --task "Audit account change activity in admin group membership" --baseline-bundle .\evidence\CASE-2026-IR-0042
```

Verify evidence bundle integrity before sharing artifacts:

```powershell
cargo run -p wraithrun -- --verify-bundle .\evidence\CASE-2026-IR-0042 --introspection-format json
```

Describe one tool:

```powershell
cargo run -p wraithrun -- --describe-tool hash_binary
```

Describe one tool with a unique partial query:

```powershell
cargo run -p wraithrun -- --describe-tool privilege
```

Describe one tool as JSON:

```powershell
cargo run -p wraithrun -- --describe-tool hash_binary --introspection-format json
```

Run a task using a built-in template:

```powershell
cargo run -p wraithrun -- --task-template listener-risk --format summary
```

Run hash template with a custom target:

```powershell
cargo run -p wraithrun -- --task-template hash-integrity --template-target C:/Temp/suspicious.exe --format summary
```

Run syslog template with custom path and line count:

```powershell
cargo run -p wraithrun -- --task-template syslog-summary --template-target C:/Logs/security.log --template-lines 50 --format summary
```

Doctor checks:

```powershell
cargo run -p wraithrun -- --doctor
```

List available profiles:

```powershell
cargo run -p wraithrun -- --list-profiles
```

List profiles as JSON:

```powershell
cargo run -p wraithrun -- --list-profiles --introspection-format json
```

Preview resolved runtime config:

```powershell
cargo run -p wraithrun -- --print-effective-config --profile local-lab
```

Explain where each resolved value came from:

```powershell
cargo run -p wraithrun -- --explain-effective-config --profile local-lab
```

Initialize a local config file:

```powershell
cargo run -p wraithrun -- --init-config
```

Initialize at a custom path (overwrite if needed):

```powershell
cargo run -p wraithrun -- --init-config --init-config-path .\configs\team-wraithrun.toml --force
```

## Configuration Files and Profiles

WraithRun supports reusable runtime configuration through TOML files and named profiles.

Resolution order (highest to lowest):

1. CLI flags
2. Environment variables
3. Config file (base settings plus selected profile)
4. Built-in defaults

Default config auto-load behavior:

- If `./wraithrun.toml` exists, it is loaded automatically.
- Use `--config <path>` to load a specific file.
- Or set `WRAITHRUN_CONFIG` to point to a config file path.

Built-in profiles:

- `local-lab`: short dry-run loops with summary output.
- `production-triage`: longer dry-run loops with markdown output.
- `live-model`: live inference enabled with higher token budget.

Examples:

```powershell
cargo run -p wraithrun -- --task "Check suspicious listener ports" --profile local-lab
```

```powershell
cargo run -p wraithrun -- --task "Investigate unauthorized SSH keys" --config .\wraithrun.example.toml --profile production-triage
```

```powershell
$env:WRAITHRUN_FORMAT = "summary"
cargo run -p wraithrun -- --task "Check suspicious listener ports" --config .\wraithrun.example.toml --profile production-triage --format json
```

Reference config template:

- `wraithrun.example.toml`

## Built-in Tools

WraithRun currently ships with these local tools:

- `read_syslog`: tail a local log file with bounded line count.
- `scan_network`: list active local listening sockets.
- `hash_binary`: compute SHA-256 for a local file.
- `check_privilege_escalation_vectors`: collect local privilege-surface indicators.

The agent decides when to call tools during a run.

## Sandbox Controls

WraithRun enforces sandbox policy checks for tool paths and commands.

Environment variables:

- `WRAITHRUN_ALLOWED_READ_ROOTS`
- `WRAITHRUN_DENIED_READ_ROOTS`
- `WRAITHRUN_COMMAND_ALLOWLIST`
- `WRAITHRUN_COMMAND_DENYLIST`

Rules:

- Use `;` as path separator on Windows and `:` on Linux/macOS.
- Use comma-separated values for command allowlist/denylist.

Example overrides (Windows PowerShell):

```powershell
$env:WRAITHRUN_ALLOWED_READ_ROOTS = "C:\Logs;C:\Temp"
$env:WRAITHRUN_COMMAND_ALLOWLIST = "whoami,netstat"
```

Example overrides (Linux/macOS shell):

```bash
export WRAITHRUN_ALLOWED_READ_ROOTS="/var/log:/tmp"
export WRAITHRUN_COMMAND_ALLOWLIST="id,ss,sudo"
```

## Practical Task Prompts

You can start with prompts like:

- `Investigate unauthorized SSH keys`
- `Check suspicious listener ports and summarize risk`
- `Hash /usr/local/bin/custom-agent and report integrity context`
- `Review local privilege escalation indicators`

Equivalent built-in template names:

- `ssh-keys`
- `listener-risk`
- `hash-integrity`
- `priv-esc-review`
- `syslog-summary`

Template parameter support:

- `hash-integrity`: supports `--template-target`.
- `syslog-summary`: supports `--template-target` and `--template-lines`.

## Troubleshooting

- Error: `Vitis inference is disabled`:
    - Rebuild/run with `--features inference_bridge/vitis`.
- Error: tokenizer not found:
    - Pass `--tokenizer <path>` or place `tokenizer.json` next to the model.
- Error: policy denied for path/command:
    - Update sandbox environment variables to match your local policy.
- Need more runtime logs:
    - Set `RUST_LOG=debug` before running.
- Need a quick setup diagnostic:
    - Run `wraithrun --doctor` to validate config/profile/env resolution and effective runtime settings.

## Project Status

Early-stage but functional. Good for local experimentation and controlled defensive workflows.

Still in progress:

- KV-cache and streaming decode support.
- Broader end-to-end test coverage.
- Signed artifacts and SBOM publication.

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
