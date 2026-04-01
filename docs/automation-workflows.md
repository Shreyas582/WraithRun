# Automation Workflows

WraithRun supports CI and SIEM integrations through two controls:

- `--automation-adapter findings-v1`: emits a findings-only JSON envelope for pipeline ingestion.
- `--exit-policy severity-threshold --exit-threshold <severity>`: enforces deterministic non-zero exits when findings meet the selected severity threshold.
- `--live-fallback-policy dry-run-on-error`: keeps live-mode runs deterministic by retrying in dry-run mode when live inference fails.

## CI Gating Example

Fail a CI job when any `high` or `critical` finding is present:

```yaml
name: WraithRun Gate

on:
  workflow_dispatch:

jobs:
  triage-gate:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - name: Build
        run: cargo build -p wraithrun
      - name: Run triage gate
        run: |
          ./target/debug/wraithrun \
            --task "Investigate unauthorized SSH keys" \
            --live \
            --model ./models/llm.onnx \
            --live-fallback-policy dry-run-on-error \
            --automation-adapter findings-v1 \
            --exit-policy severity-threshold \
            --exit-threshold high \
            --output-file ./launch-assets/adapter-findings.json
      - name: Upload adapter payload
        uses: actions/upload-artifact@v4
        with:
          name: wraithrun-adapter-findings
          path: ./launch-assets/adapter-findings.json
```

Behavior:

- exit code `0`: no findings at or above threshold.
- non-zero exit code: at least one finding met/exceeded threshold.

## SIEM Forwarding Example

Generate normalized findings and forward to a collector:

```powershell
wraithrun --task "Correlate process and network listener exposure" --case-id CASE-2026-IR-0042 --automation-adapter findings-v1 --output-file .\launch-assets\findings-v1.json
Get-Content .\launch-assets\findings-v1.json
```

Example adapter payload fields for forwarding:

- `contract_version`
- `adapter`
- `summary.finding_count`
- `summary.highest_severity`
- `findings[].finding_id`
- `findings[].severity`
- `findings[].recommended_action`
- `findings[].evidence_pointer`
- `summary.live_fallback_decision` (when fallback is triggered)

## Migration Notes

For existing integrations consuming run-report JSON:

1. Keep current parser for `--format json` run reports.
2. Add a new parser path for `--automation-adapter findings-v1` payloads.
3. Validate `contract_version` first, then enforce schema.
4. Configure threshold policy by environment for pipeline consistency:

```powershell
$env:WRAITHRUN_EXIT_POLICY="severity-threshold"
$env:WRAITHRUN_EXIT_THRESHOLD="high"
```

5. Treat unknown future fields as forward-compatible unless strict policy requires rejection.
