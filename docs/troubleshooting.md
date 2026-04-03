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

## Doctor reports live-runtime-compatibility FAIL

Symptom:

- `--doctor --live` reports `live-runtime-compatibility` with status `fail` and the model cannot be used.

Fix:

- Check the `reason_code` and `remediation` fields in the doctor JSON output for specific guidance.
- Common causes:
  - **runtime_session_init_failed**: Model file is not a valid ONNX model. Re-download or re-export the model.
  - **runtime_external_data_file_missing**: External data file referenced by the model is missing. Place it beside the model.
  - **runtime_vitis_provider_missing**: Vitis AI provider DLL not found. Install RyzenAI SDK or set `ORT_DYLIB_PATH`.
  - **runtime_ort_dylib_missing**: ONNX Runtime library not found. Set `ORT_DYLIB_PATH` to the runtime location.
  - **runtime_custom_ops_unavailable**: Custom operator library not found. Install it beside the runtime or model.
  - **onnx_feature_disabled**: Build does not include ONNX inference. Rebuild with `--features inference_bridge/onnx` or `--features inference_bridge/vitis`.

## Live setup command failed

Symptom:

- `wraithrun live setup --model <PATH>` exits with an error mentioning `Live setup validation failed`.

Fix:

- Setup validates model compatibility before writing config. If the model fails ONNX session initialization, setup rejects it.
- Run `--doctor --live --model <PATH> --introspection-format json` to see the full check results with `remediation` guidance.
- If `live-runtime-compatibility` shows `onnx_feature_disabled`, rebuild with `--features inference_bridge/onnx`.
- If the model is corrupt or incompatible, obtain a valid ONNX model file.
