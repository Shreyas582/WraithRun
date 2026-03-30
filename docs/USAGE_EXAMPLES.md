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

Linux/macOS binary:

```bash
./wraithrun --task "Investigate unauthorized SSH keys"
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

Preview final merged runtime settings:

```powershell
.\wraithrun.exe --print-effective-config --profile production-triage --config .\wraithrun.example.toml
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

## Troubleshooting Quick Checks

- `Vitis inference is disabled`:
  - Add `--features inference_bridge/vitis` when running from source.
- `Unable to locate tokenizer.json`:
  - Provide `--tokenizer` or place `tokenizer.json` next to your model.
- `policy denied` errors:
  - Confirm your allowed/denied roots and command lists are correct.
- Need verbose logs:
  - Set `RUST_LOG=debug` before running.
