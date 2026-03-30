# Usage Examples

This page provides practical, copy-paste examples for running WraithRun yourself.

## Dry-Run Investigation (No Model Required)

Windows binary:

```powershell
.\agentic-cyber-cli.exe --task "Investigate unauthorized SSH keys"
```

From source:

```powershell
cargo run -p agentic-cyber-cli -- --task "Investigate unauthorized SSH keys"
```

Linux/macOS binary:

```bash
./agentic-cyber-cli --task "Investigate unauthorized SSH keys"
```

## Save Results to a File

PowerShell:

```powershell
.\agentic-cyber-cli.exe --task "Check suspicious listener ports" | Out-File -Encoding utf8 report.json
```

Bash:

```bash
./agentic-cyber-cli --task "Check suspicious listener ports" > report.json
```

## Pretty-Print or Parse JSON Output

PowerShell:

```powershell
Get-Content .\report.json | ConvertFrom-Json | ConvertTo-Json -Depth 20
```

Bash with jq:

```bash
cat report.json | jq .
```

Extract only the final answer (jq):

```bash
cat report.json | jq -r .final_answer
```

## Live ONNX/Vitis Inference

Validate build path:

```powershell
cargo check -p inference_bridge --features vitis
```

Run with live model:

```powershell
cargo run -p agentic-cyber-cli --features inference_bridge/vitis -- --live --model C:/models/llm.onnx --tokenizer C:/models/tokenizer.json --task "Investigate unauthorized SSH keys"
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
