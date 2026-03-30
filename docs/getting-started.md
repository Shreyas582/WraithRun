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
