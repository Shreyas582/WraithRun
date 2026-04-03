# Automation Contracts

WraithRun publishes machine-readable JSON schemas and matching examples for automation pipelines.

Contract version:

- `1.0.0`

Validate the top-level `contract_version` field before enforcing strict field-level parsing.

Live-mode note:

- Run report includes optional `live_fallback_decision` when policy-driven fallback is triggered.
- Findings adapter summary includes optional `live_fallback_decision` for pipeline audit visibility.
- `live_fallback_decision.reason_code` provides machine-readable fallback classification.
- Run report includes optional `run_timing` and `live_run_metrics` for latency and reliability telemetry.
- Findings adapter summary includes optional `live_run_metrics` for downstream alerting and scoring.
- Doctor introspection checks now include an optional `remediation` field with actionable fix guidance for each `reason_code`.
- Run report findings now include `confidence_label` (discrete tier) and `relevance` (primary/supplementary) fields.
- Run report includes an optional `supplementary_findings` array for lower-relevance findings (compact output mode).

## Schema Files

- Run report: `docs/schemas/run-report.schema.json`
- Automation adapter (findings-v1): `docs/schemas/automation-adapter-findings-v1.schema.json`
- Doctor introspection: `docs/schemas/doctor-introspection.schema.json`
- Tool list introspection: `docs/schemas/tool-list-introspection.schema.json`
- Profile list introspection: `docs/schemas/profile-list-introspection.schema.json`
- Task-template list introspection: `docs/schemas/task-template-list-introspection.schema.json`

## Example Payloads

- Run report example: `docs/schemas/examples/run-report.example.json`
- Automation adapter example: `docs/schemas/examples/automation-adapter-findings-v1.example.json`
- Doctor introspection example: `docs/schemas/examples/doctor-introspection.example.json`
- Tool list introspection example: `docs/schemas/examples/tool-list-introspection.example.json`
- Profile list introspection example: `docs/schemas/examples/profile-list-introspection.example.json`
- Task-template list introspection example: `docs/schemas/examples/task-template-list-introspection.example.json`

## Pipeline Guidance

1. Parse JSON payload.
2. Validate `contract_version` against expected value.
3. Validate payload shape using the matching schema.
4. Ignore unknown fields for forward compatibility unless policy requires strict rejection.

For end-to-end CI/SIEM usage patterns, see `docs/automation-workflows.md`.
