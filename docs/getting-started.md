# Getting Started

## Prerequisites

For binary use:

- No language toolchain required.

For source builds:

- Rust stable toolchain.

## Option A: Run Release Binary

1. Download your OS artifact from GitHub Releases.
2. Extract the archive.
3. Run a dry-run investigation task.

Windows:

```powershell
.\wraithrun.exe --task "Investigate unauthorized SSH keys"
```

Linux/macOS:

```bash
./wraithrun --task "Investigate unauthorized SSH keys"
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

## Live Inference Mode (Optional)

Live inference requires:

- A compatible ONNX model.
- A matching tokenizer.json.
- ONNX Runtime with Vitis execution provider support.

Feature check:

```powershell
cargo check -p inference_bridge --features vitis
```

Live run:

```powershell
cargo run -p wraithrun --features inference_bridge/vitis -- --live --model C:/models/llm.onnx --tokenizer C:/models/tokenizer.json --task "Investigate unauthorized SSH keys"
```

## Output Format

WraithRun prints a JSON report with:

- task: your original request.
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

Example:

```powershell
cargo run -p wraithrun -- --task "Check suspicious listener ports" --config .\wraithrun.example.toml --profile production-triage
```

Quick diagnostics:

```powershell
cargo run -p wraithrun -- --doctor
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
