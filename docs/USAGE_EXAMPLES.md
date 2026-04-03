# Usage Examples

This page provides practical, copy-paste examples for running WraithRun yourself.

## Dry-Run Investigation (No Model Required)

Windows binary:

```powershell
.\wraithrun.exe --task "Investigate unauthorized SSH keys"
```

From source:

```powershell
cargo run -p wraithrun -- --task "Investigate unauthorized SSH keys"
```

From a task file:

```powershell
.\wraithrun.exe --task-file .\launch-assets\incident-task.txt --format summary
```

From stdin:

```powershell
Get-Content .\launch-assets\incident-task.txt | .\wraithrun.exe --task-stdin --format summary
```

Linux/macOS binary:

```bash
./wraithrun --task "Investigate unauthorized SSH keys"
```

Template-driven run:

```powershell
.\wraithrun.exe --task-template listener-risk
```

Hash template with custom target:

```powershell
.\wraithrun.exe --task-template hash-integrity --template-target C:/Temp/suspicious.exe --format summary
```

Syslog template with custom path and line count:

```powershell
.\wraithrun.exe --task-template syslog-summary --template-target C:/Logs/security.log --template-lines 50 --format summary
```

List available templates:

```powershell
.\wraithrun.exe --list-task-templates
```

List templates as JSON:

```powershell
.\wraithrun.exe --list-task-templates --introspection-format json
```

## Save Results to a File

PowerShell:

```powershell
.\wraithrun.exe --task "Check suspicious listener ports" --output-file .\launch-assets\network-report.json
```

Bash:

```bash
./wraithrun --task "Check suspicious listener ports" --output-file ./launch-assets/network-report.json
```

## Alternative Output Formats

Summary format:

```powershell
.\wraithrun.exe --task "Check suspicious listener ports" --format summary
```

Markdown format:

```powershell
.\wraithrun.exe --task "Investigate unauthorized SSH keys" --format markdown
```

Quiet mode (suppress runtime logs):

```powershell
.\wraithrun.exe --task "Check suspicious listener ports" --quiet
```

Verbose mode (debug logs):

```powershell
.\wraithrun.exe --task "Check suspicious listener ports" --verbose
```

## Use Profiles (Built-In)

Local lab profile:

```powershell
.\wraithrun.exe --task "Check suspicious listener ports" --profile local-lab
```

Production triage profile:

```powershell
.\wraithrun.exe --task "Investigate unauthorized SSH keys" --profile production-triage
```

Live model profile:

```powershell
.\wraithrun.exe --task "Investigate unauthorized SSH keys" --profile live-model
```

## Use a Config File

Use repository template directly:

```powershell
.\wraithrun.exe --task "Check suspicious listener ports" --config .\wraithrun.example.toml --profile production-triage
```

Auto-load local config (`./wraithrun.toml` if present):

```powershell
.\wraithrun.exe --task "Check suspicious listener ports" --profile local-lab
```

Select config with env var:

```powershell
$env:WRAITHRUN_CONFIG = ".\wraithrun.example.toml"
.\wraithrun.exe --task "Check suspicious listener ports" --profile production-triage
```

## Case Workflow Runbook (Collection, Verification, Retention)

Collection step 1: capture a reusable host baseline for later drift checks.

```powershell
.\wraithrun.exe --task "Capture host coverage baseline for persistence account and network" --case-id CASE-2026-IR-0100 --evidence-bundle-dir .\evidence\CASE-2026-IR-0100\baseline
```

Collection step 2: run investigation and export both directory bundle and deterministic archive.

```powershell
.\wraithrun.exe --task "Investigate unauthorized SSH keys" --case-id CASE-2026-IR-0100 --baseline-bundle .\evidence\CASE-2026-IR-0100\baseline --evidence-bundle-dir .\evidence\CASE-2026-IR-0100\run-01 --evidence-bundle-archive .\evidence\CASE-2026-IR-0100\run-01.tar
```

Verification step 1: verify using bundle directory path.

```powershell
.\wraithrun.exe --verify-bundle .\evidence\CASE-2026-IR-0100\run-01 --introspection-format json
```

Verification step 2: verify using direct checksum-manifest path (works for paths with spaces).

```powershell
.\wraithrun.exe --verify-bundle ".\evidence\CASE-2026-IR-0100\run 01\SHA256SUMS"
```

Retention step 1: store immutable archive, keep baseline bundle, and track integrity metadata.

```powershell
New-Item -ItemType Directory -Path .\retention\CASE-2026-IR-0100 -Force | Out-Null
Copy-Item .\evidence\CASE-2026-IR-0100\run-01.tar .\retention\CASE-2026-IR-0100\
Copy-Item .\evidence\CASE-2026-IR-0100\baseline\raw_observations.json .\retention\CASE-2026-IR-0100\baseline.raw_observations.json
Get-FileHash .\retention\CASE-2026-IR-0100\run-01.tar -Algorithm SHA256
```

Retention step 2: use case-scoped folder naming convention to simplify audit retrieval.

- `retention/<CASE-ID>/run-<NN>.tar`
- `retention/<CASE-ID>/baseline.raw_observations.json`
- `retention/<CASE-ID>/integrity-notes.txt`

## Resolution Order Example

This command chain demonstrates `CLI > env > config > defaults`.

```powershell
$env:WRAITHRUN_FORMAT = "summary"
.\wraithrun.exe --task "Check suspicious listener ports" --config .\wraithrun.example.toml --profile production-triage --format json
```

Expected result format: `json` (CLI wins over env and config).

To force dry-run over a live profile/config:

```powershell
.\wraithrun.exe --task "Check suspicious listener ports" --profile live-model --dry-run
```

## Run Doctor Checks

Quick diagnostics:

```powershell
.\wraithrun.exe --doctor
```

Check a specific profile and config combination:

```powershell
.\wraithrun.exe --doctor --config .\wraithrun.example.toml --profile live-model
```

If doctor reports failures, the command exits non-zero.

## Inspect Profile and Config Resolution

List built-in and config-defined profiles:

```powershell
.\wraithrun.exe --list-profiles --config .\wraithrun.example.toml
```

List profiles as JSON:

```powershell
.\wraithrun.exe --list-profiles --introspection-format json
```

Preview final merged runtime settings:

```powershell
.\wraithrun.exe --print-effective-config --profile production-triage --config .\wraithrun.example.toml
```

Show effective settings with source attribution:

```powershell
.\wraithrun.exe --explain-effective-config --profile production-triage --config .\wraithrun.example.toml
```

Inspect commands are mutually exclusive with `--doctor`.

## Initialize a Config File

Create `./wraithrun.toml`:

```powershell
.\wraithrun.exe --init-config
```

Create config in a custom folder:

```powershell
.\wraithrun.exe --init-config --init-config-path .\configs\team-wraithrun.toml
```

Overwrite an existing config file:

```powershell
.\wraithrun.exe --init-config --init-config-path .\configs\team-wraithrun.toml --force
```

## Pretty-Print or Parse JSON Output

PowerShell:

```powershell
Get-Content .\launch-assets\network-report.json | ConvertFrom-Json | ConvertTo-Json -Depth 20
```

Bash with jq:

```bash
cat ./launch-assets/network-report.json | jq .
```

Extract only the final answer (jq):

```bash
cat ./launch-assets/network-report.json | jq -r .final_answer
```

## Live ONNX/Vitis Inference

Validate build path:

```powershell
cargo check -p inference_bridge --features vitis
```

Run with live model:

```powershell
cargo run -p wraithrun --features inference_bridge/vitis -- --live --model C:/models/llm.onnx --tokenizer C:/models/tokenizer.json --task "Investigate unauthorized SSH keys"
```

Optional Vitis config knobs:

- `--vitis-config <path>`
- `--vitis-cache-dir <path>`
- `--vitis-cache-key <key>`

## Sandbox Policy Overrides

Windows PowerShell:

```powershell
$env:WRAITHRUN_ALLOWED_READ_ROOTS = "C:\Logs;C:\Temp"
$env:WRAITHRUN_DENIED_READ_ROOTS = "C:\Windows\System32\config"
$env:WRAITHRUN_COMMAND_ALLOWLIST = "whoami,netstat"
$env:WRAITHRUN_COMMAND_DENYLIST = "powershell,pwsh,cmd"
```

Linux/macOS shell:

```bash
export WRAITHRUN_ALLOWED_READ_ROOTS="/var/log:/tmp"
export WRAITHRUN_DENIED_READ_ROOTS="/root:/proc"
export WRAITHRUN_COMMAND_ALLOWLIST="id,ss,sudo"
export WRAITHRUN_COMMAND_DENYLIST="bash,sh,python,curl,wget"
```

## Task Prompt Ideas

- `Investigate unauthorized SSH keys`
- `Check suspicious listener ports and summarize risk`
- `Hash C:/Windows/System32/notepad.exe and report integrity context`
- `Review local privilege escalation indicators`
- `Read and summarize last 200 lines from C:/Logs/agent.log`

## Investigation Templates and Scope Validation

The agent resolves a declarative investigation template based on task keywords. Templates determine tool selection and execution order.

List investigation templates:

```powershell
.\wraithrun.exe --list-task-templates
```

Tasks outside supported scope (cloud, Kubernetes, email, SIEM) return an informational scoping finding:

```powershell
.\wraithrun.exe --task "Check our AWS S3 bucket permissions"
# Returns informational finding: task is outside host-level investigation scope
```

## Finding Confidence Labels and Relevance

Findings include a discrete `confidence_label` derived from the numeric score:

```powershell
.\wraithrun.exe --task "Investigate unauthorized SSH keys" --output-mode full
# Each finding includes: "confidence_label": "confirmed", "relevance": "primary"
```

In compact mode (default), supplementary findings from non-primary tools are separated:

```powershell
.\wraithrun.exe --task "Investigate unauthorized SSH keys"
# JSON contains "findings": [...] and "supplementary_findings": [...]
```

## Troubleshooting Quick Checks

- `Vitis inference is disabled`:
  - Add `--features inference_bridge/vitis` when running from source.
- `Unable to locate tokenizer.json`:
  - Provide `--tokenizer` or place `tokenizer.json` next to your model.
- `policy denied` errors:
  - Confirm your allowed/denied roots and command lists are correct.
- Need verbose logs:
  - Set `RUST_LOG=debug` before running.
