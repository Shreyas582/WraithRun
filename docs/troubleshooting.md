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

## JSON output is missing the turns array

Symptom:

- JSON output does not contain intermediate reasoning steps (the `turns` array is absent).

Fix:

- This is expected. Since v0.11.0, the default output mode is `compact`, which omits `turns` to reduce payload size.
- Use `--output-mode full` to restore the complete output including all intermediate turns.

## Model classified as wrong capability tier

Symptom:

- Agent behavior does not match what you expect for your model (e.g., skipping LLM synthesis when the model is capable).

Fix:

- WraithRun automatically probes model size and latency to classify capability as Basic, Moderate, or Strong.
- Override automatic classification with `--capability-override`:

```powershell
wraithrun --task "Investigate ..." --live --model C:/models/llm.onnx --tokenizer C:/models/tokenizer.json --capability-override strong
```

- Tier thresholds: Basic ≤2B params or ≥200ms latency; Strong ≥10B params and ≤50ms latency; Moderate is everything in between.
- Since v1.8.0, parameter estimation is quantization-aware. Q4 models use 0.55 bytes/param, Q8 uses 1.1, FP16 uses 2.2, FP32 uses 4.4. This may reclassify models that were previously under-estimated (e.g., a Q4 model that reported 0.5B may now correctly report ~2B and shift from Basic to Moderate).

## Final answer looks generic or templated

Symptom:

- The executive summary in `final_answer` is a structured SUMMARY/FINDINGS/RISK/ACTIONS block instead of natural language.

Fix:

- This happens when the model is classified as Basic tier (deterministic summary) or when LLM output quality is detected as low.
- Since v1.6.0, Moderate/Strong tiers use a ReAct loop that typically produces richer output. If output is still generic, try `--capability-override strong` or increase `--temperature` slightly (e.g., `0.1`).
- Since v1.8.0, the quality guard also catches hallucinated `<call>` tags and `[observation]` markers inside the final answer. When detected, the agent replaces the garbage with a deterministic summary built from real findings. This means even Moderate/Strong tier runs may show a structured summary if the model hallucinates.

## Agent not calling expected tools

Symptom:

- The agent finishes quickly without calling tools you expected, or calls fewer tools than anticipated.

Fix:

- Moderate/Strong tiers use a ReAct loop where the LLM decides which tools to call. The model may not choose the same tools as the template-driven Basic tier.
- Increase `--max-steps` if the agent is exhausting its step budget before reaching all relevant tools.
- If the model is too small, it may produce a `<final>` answer immediately. Try `--capability-override strong` to allow full iterative reasoning.
- Since v1.8.0, if the model produces `<final>` at step 0 without calling any tools, the agent automatically falls back to template-driven execution so that real host data is still collected.
- Check `RUST_LOG=debug` output for `react_step` entries showing the agent's reasoning at each step.

## Task returned a scope-boundary finding instead of running

Symptom:

- The agent returns a single informational finding about the task being outside host-level investigation scope, without executing any tools.

Fix:

- WraithRun validates that tasks fall within its supported domain (host-level cyber investigation). Tasks referencing cloud infrastructure (AWS, Azure, GCP), container orchestration (Kubernetes), email/phishing, or SIEM are rejected.
- Rephrase your task to focus on host-level analysis: accounts, processes, persistence, network listeners, file integrity, or logs.

## Some findings appear in supplementary_findings instead of findings

Symptom:

- In compact JSON output, some findings are in a `supplementary_findings` array instead of the main `findings` array.

Fix:

- This is expected. Since v0.13.0, the agent tags findings by relevance to the resolved investigation template. Findings from non-primary tools are classified as `supplementary` and separated in compact mode.
- Use `--output-mode full` to keep all findings in the main `findings` array with their `relevance` tag.
- If your model is capable, use `--capability-override moderate` or `--capability-override strong` to force LLM synthesis.
