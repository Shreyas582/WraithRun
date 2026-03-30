# WraithRun

Local-first cyber investigation runtime you can run on your own machine.

WraithRun is a Rust CLI for host-focused security triage. It runs an agent loop with sandboxed local tools, keeps telemetry local, and returns a structured JSON report that you can store, diff, or feed into your own pipelines.

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
    "task": "Investigate unauthorized SSH keys",
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

- `task`: your input task string.
- `turns`: intermediate reasoning/tool interaction history.
- `final_answer`: the model/runtime conclusion.

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

- `--task <TASK>` required.
- `--live` enables model inference mode (default is dry-run).
- `--model <MODEL>` model path (default `./models/llm.onnx`).
- `--tokenizer <TOKENIZER>` tokenizer path for live mode.
- `--max-steps <N>` max agent turns (default `8`).
- `--max-new-tokens <N>` generation cap per response (default `256`).
- `--temperature <F>` generation temperature (default `0.2`).

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

## Troubleshooting

- Error: `Vitis inference is disabled`:
    - Rebuild/run with `--features inference_bridge/vitis`.
- Error: tokenizer not found:
    - Pass `--tokenizer <path>` or place `tokenizer.json` next to the model.
- Error: policy denied for path/command:
    - Update sandbox environment variables to match your local policy.
- Need more runtime logs:
    - Set `RUST_LOG=debug` before running.

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
