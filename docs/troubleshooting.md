# Troubleshooting

## Run doctor first

Symptom:

- Runtime setup is unclear or mixed config/env/profile values are hard to debug.

Fix:

```powershell
wraithrun --doctor
```

Doctor validates config parsing, profile resolution, environment variable parsing, and effective runtime settings.

## Need a starter config quickly

Symptom:

- You want to start using profiles/configs but do not have a local TOML file.

Fix:

```powershell
wraithrun --init-config
```

## Not sure which layer set a value

Symptom:

- Effective runtime values are unexpected and you need to know whether CLI/env/config/profile/default won.

Fix:

```powershell
wraithrun --explain-effective-config --profile local-lab
```

## Unsure how to phrase an investigation task

Symptom:

- You want a valid task prompt quickly.

Fix:

```powershell
wraithrun --list-task-templates
```

Run one directly:

```powershell
wraithrun --task-template listener-risk
```

If you need path or line overrides, use templates that support them:

- `hash-integrity` supports `--template-target`.
- `syslog-summary` supports `--template-target` and `--template-lines`.

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
