# Getting Started

## Prerequisites

For binary use:

- No language toolchain required.

For source builds:

- Rust stable toolchain.

## Option A: Run Release Binary

1. Download your OS artifacts from GitHub Releases.
2. Install using a native package (`.msi`, `.deb`, `.rpm`, `.pkg`) or extract archive (`.zip`, `.tar.gz`).
3. Run a smoke check with `wraithrun --help`.
4. Run a dry-run investigation task.

Windows (MSI):

```powershell
msiexec /i .\wraithrun-windows-x86_64.msi /qn
wraithrun --help
```

Windows (ZIP):

```powershell
.\wraithrun.exe --task "Investigate unauthorized SSH keys"
```

Linux (DEB/RPM):

```bash
sudo dpkg -i ./wraithrun-linux-x86_64.deb
wraithrun --help
```

```bash
sudo dnf install ./wraithrun-linux-x86_64.rpm
wraithrun --help
```

Linux/macOS (tar.gz):

```bash
./wraithrun --task "Investigate unauthorized SSH keys"
```

macOS (PKG):

```bash
sudo installer -pkg ./wraithrun-macos-x86_64.pkg -target /
/usr/local/bin/wraithrun --help
```

## Option B: Run From Source

```powershell
git clone https://github.com/Shreyas582/WraithRun.git
cd WraithRun
cargo run -p wraithrun -- --task "Investigate unauthorized SSH keys"
```

Template-based run:

```powershell
cargo run -p wraithrun -- --task-template listener-risk
```

Task file run:

```powershell
cargo run -p wraithrun -- --task-file .\launch-assets\incident-task.txt --format summary
```

Task stdin run:

```powershell
Get-Content .\launch-assets\incident-task.txt | cargo run -p wraithrun -- --task-stdin --format summary
```

Template-based run with target overrides:

```powershell
cargo run -p wraithrun -- --task-template syslog-summary --template-target C:/Logs/security.log --template-lines 50
```

## Live Inference Mode (Optional)

Live inference requires:

- A compatible ONNX model.
- A matching tokenizer.json.
- ONNX Runtime (bundled or via `ORT_DYLIB_PATH`).

Two feature flags are available for source builds:

- `inference_bridge/onnx`: CPU execution provider (works on any platform with ONNX Runtime).
- `inference_bridge/vitis`: AMD RyzenAI Vitis execution provider (requires RyzenAI SDK).

Feature check:

```powershell
cargo check -p inference_bridge --features onnx
cargo check -p inference_bridge --features vitis
```

Live run (CPU):

```powershell
cargo run -p wraithrun --features inference_bridge/onnx -- --live --model C:/models/llm.onnx --tokenizer C:/models/tokenizer.json --task "Investigate unauthorized SSH keys"
```

Live run (RyzenAI NPU):

```powershell
cargo run -p wraithrun --features inference_bridge/vitis -- --live --model C:/models/llm.onnx --tokenizer C:/models/tokenizer.json --task "Investigate unauthorized SSH keys"
```

One-command live setup bootstrap (validates model compatibility before writing config):

```powershell
cargo run -p wraithrun -- live setup --model C:/models/llm.onnx --config .\wraithrun.toml
```

Model-pack lifecycle checks:

```powershell
cargo run -p wraithrun -- models list --introspection-format json
cargo run -p wraithrun -- models validate --introspection-format json
cargo run -p wraithrun -- models benchmark --introspection-format json
```

## Output Format

WraithRun prints a JSON report with:

- contract_version: machine-readable contract version marker.
- task: your original request.
- findings: normalized actionable findings.
- run_timing: optional latency fields (`first_token_latency_ms`, `total_run_duration_ms`).
- live_run_metrics: optional live reliability/latency fields for live-mode runs.
- turns: intermediate reasoning and tool observations.
- final_answer: final response text.

## Configuration and Profiles

WraithRun supports config-driven runs through TOML files and named profiles.

- Auto-loads `./wraithrun.toml` when present.
- Explicit file path via `--config` or `WRAITHRUN_CONFIG`.
- Profile selection via `--profile` or `WRAITHRUN_PROFILE`.

Built-in profile names:

- `local-lab`
- `production-triage`
- `live-model`
- `live-fast`
- `live-balanced`
- `live-deep`

Example:

```powershell
cargo run -p wraithrun -- --task "Check suspicious listener ports" --config .\wraithrun.example.toml --profile production-triage
```

Quick diagnostics:

```powershell
cargo run -p wraithrun -- --doctor
```

Quick diagnostics as JSON:

```powershell
cargo run -p wraithrun -- --doctor --introspection-format json
```

List profiles:

```powershell
cargo run -p wraithrun -- --list-profiles
```

Preview effective config:

```powershell
cargo run -p wraithrun -- --print-effective-config --profile local-lab
```

Explain source of each resolved value:

```powershell
cargo run -p wraithrun -- --explain-effective-config --profile local-lab
```

Generate starter config:

```powershell
cargo run -p wraithrun -- --init-config
```
