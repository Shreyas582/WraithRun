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

Expected warning that should be reviewed but may still run:

- `live-model-format` warning if file extension is not `.onnx`

## 2. Configure Predictable Fallback

Use fallback policy when live inference must not block triage completion:

```powershell
wraithrun --task "Investigate unauthorized SSH keys" --live --model C:/models/llm.onnx --live-fallback-policy dry-run-on-error
```

Policy behavior:

- `none`: live inference error returns non-zero immediately.
- `dry-run-on-error`: runtime retries once in dry-run mode and records `live_fallback_decision`.

When fallback is triggered, `live_fallback_decision.reason_code` provides structured classification for automation and alert routing.

## 3. Pipeline Gating Pattern

For automation pipelines, combine fallback and exit policy:

```powershell
wraithrun --task "Investigate unauthorized SSH keys" --live --model C:/models/llm.onnx --live-fallback-policy dry-run-on-error --automation-adapter findings-v1 --exit-policy severity-threshold --exit-threshold high
```

This keeps ingestion deterministic while preserving incident signaling:

- adapter output stays machine-consumable,
- severity threshold still controls process exit code,
- fallback details are preserved in output for auditability.

## 4. Troubleshooting Checklist

If live mode repeatedly falls back:

1. Verify `--doctor --live` failures first.
2. Confirm model path points to local readable storage.
3. Confirm tokenizer JSON parses and includes top-level `model`.
4. Confirm Vitis paths (`--vitis-config`, `--vitis-cache-dir`) are valid when used.
5. Review `live_fallback_decision.reason_code` and `live_fallback_decision.live_error` in run output and capture both in incident notes.

## 5. Operator Recording Guidance

When fallback is triggered during an active case:

- keep the run output with `live_fallback_decision`,
- preserve evidence bundle artifacts,
- record whether fallback affected analyst confidence or timeline.
