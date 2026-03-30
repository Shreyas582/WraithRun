# Troubleshooting

## Vitis inference is disabled

Symptom:

- Runtime reports Vitis inference is disabled.

Fix:

```powershell
cargo run -p wraithrun --features inference_bridge/vitis -- --live --model C:/models/llm.onnx --tokenizer C:/models/tokenizer.json --task "Investigate unauthorized SSH keys"
```

## tokenizer.json not found

Symptom:

- Runtime cannot locate tokenizer.json.

Fix:

- Provide `--tokenizer <path>`.
- Or place tokenizer.json next to the ONNX model file.

## policy denied for path or command

Symptom:

- Tool execution denied by sandbox policy.

Fix:

- Adjust allowlist/denylist environment variables to match your authorized local policy.

## Need more logs

Set runtime logging level:

Windows PowerShell:

```powershell
$env:RUST_LOG = "debug"
wraithrun --task "Check suspicious listener ports"
```

Linux/macOS shell:

```bash
RUST_LOG=debug ./wraithrun --task "Check suspicious listener ports"
```
