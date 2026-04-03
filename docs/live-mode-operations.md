# Live-Mode Operations Guide

This guide focuses on running live mode reliably outside lab conditions.

## 1. Validate Model Pack Readiness

Run doctor in live mode before operational use:

```powershell
wraithrun --doctor --live --model C:/models/llm.onnx --tokenizer C:/models/tokenizer.json --introspection-format json
```

For auto-remediation of common setup issues, run:

```powershell
wraithrun --doctor --live --fix --model C:/models/llm.onnx --introspection-format json
```

Required PASS checks for live readiness:

- `live-model-path`
- `live-model-size`
- `live-tokenizer-path`
- `live-tokenizer-size`
- `live-tokenizer-json`
- `live-runtime-compatibility` (requires `--features inference_bridge/onnx` or `vitis`)

Expected warning that should be reviewed but may still run:

- `live-model-format` warning if file extension is not `.onnx`
- `live-runtime-compatibility` warn when ONNX inference feature is not enabled in the build

When a check fails, the doctor output now includes a `remediation` field with actionable fix guidance. For example:

```json
{
  "status": "fail",
  "name": "live-runtime-compatibility",
  "reason_code": "runtime_session_init_failed",
  "remediation": "Verify the model file is a valid ONNX model. Re-download if corrupted."
}
```

### Common reason codes

| Reason Code | Meaning |
|---|---|
| `model_path_missing` | Model file does not exist at the configured path |
| `tokenizer_path_missing` | Tokenizer file does not exist at the configured path |
| `tokenizer_json_malformed` | Tokenizer file is not valid JSON |
| `tokenizer_json_missing_model_key` | Tokenizer JSON lacks required top-level `model` key |
| `runtime_session_init_failed` | ONNX session could not be initialized |
| `runtime_model_invalid` | Model file is not a valid ONNX model |
| `runtime_external_data_file_missing` | External data file referenced by model is missing |
| `runtime_external_initializer_unresolved` | External initializer tensors could not be resolved |
| `runtime_vitis_provider_missing` | Vitis AI execution provider library not found |
| `runtime_ort_dylib_missing` | ONNX Runtime shared library not found |
| `runtime_custom_ops_unavailable` | Custom operator library required by model not available |
| `runtime_ep_assignment_failed` | Model nodes could not be assigned to execution provider |
| `runtime_input_ids_missing` | Model does not expose an input_ids/tokens input |
| `runtime_input_unsupported` | Model requires inputs not supported by the runtime |
| `runtime_input_dtype_unsupported` | Model input uses an unsupported tensor element type |
| `runtime_logits_output_missing` | Model outputs do not include logits |
| `runtime_cache_output_missing` | Model has cache inputs but no matching cache outputs |
| `onnx_feature_disabled` | Build does not have ONNX inference support enabled |

## 2. Compare Presets and Packs

Use the model-pack manager to compare presets and live profiles before selecting one for active runs:

```powershell
wraithrun models list
wraithrun models benchmark --introspection-format json
```

Validate all discovered packs (or a specific one via `--profile`) before promotion:

```powershell
wraithrun models validate --introspection-format json
wraithrun models validate --profile live-balanced --introspection-format json
```

## 3. Configure Predictable Fallback

Use fallback policy when live inference must not block triage completion:

```powershell
wraithrun --task "Investigate unauthorized SSH keys" --live --model C:/models/llm.onnx --live-fallback-policy dry-run-on-error
```

Policy behavior:

- `none`: live inference error returns non-zero immediately.
- `dry-run-on-error`: runtime retries once in dry-run mode and records `live_fallback_decision`.

When fallback is triggered, `live_fallback_decision.reason_code` provides structured classification for automation and alert routing.

When `--live` is enabled, run output also includes `live_run_metrics` for operational telemetry (`first_token_latency_ms`, `total_run_duration_ms`, `live_success_rate`, `fallback_rate`, and `top_failure_reasons`).

## 4. Pipeline Gating Pattern

For automation pipelines, combine fallback and exit policy:

```powershell
wraithrun --task "Investigate unauthorized SSH keys" --live --model C:/models/llm.onnx --live-fallback-policy dry-run-on-error --automation-adapter findings-v1 --exit-policy severity-threshold --exit-threshold high
```

This keeps ingestion deterministic while preserving incident signaling:

- adapter output stays machine-consumable,
- severity threshold still controls process exit code,
- fallback details and live telemetry are preserved in output for auditability.

## 5. Troubleshooting Checklist

If live mode repeatedly falls back:

1. Verify `--doctor --live` failures first. Check `remediation` fields for actionable fix guidance.
2. Confirm model path points to local readable storage.
3. Confirm tokenizer JSON parses and includes top-level `model`.
4. Confirm Vitis paths (`--vitis-config`, `--vitis-cache-dir`) are valid when used.
5. Review `live_fallback_decision.reason_code` and `live_fallback_decision.live_error` in run output and capture both in incident notes.
6. If `live-runtime-compatibility` reports FAIL, verify the model is a valid ONNX file and matches the runtime (CPU vs Vitis).

## 6. Operator Recording Guidance

When fallback is triggered during an active case:

- keep the run output with `live_fallback_decision`,
- preserve evidence bundle artifacts,
- record whether fallback affected analyst confidence or timeline.
