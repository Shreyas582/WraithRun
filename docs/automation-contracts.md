# Automation Contracts

WraithRun publishes machine-readable JSON schemas and matching examples for automation pipelines.

Contract version:

- `1.0.0`

Validate the top-level `contract_version` field before enforcing strict field-level parsing.

## Schema Files

- Run report: `docs/schemas/run-report.schema.json`
- Doctor introspection: `docs/schemas/doctor-introspection.schema.json`
- Tool list introspection: `docs/schemas/tool-list-introspection.schema.json`
- Profile list introspection: `docs/schemas/profile-list-introspection.schema.json`
- Task-template list introspection: `docs/schemas/task-template-list-introspection.schema.json`

## Example Payloads

- Run report example: `docs/schemas/examples/run-report.example.json`
- Doctor introspection example: `docs/schemas/examples/doctor-introspection.example.json`
- Tool list introspection example: `docs/schemas/examples/tool-list-introspection.example.json`
- Profile list introspection example: `docs/schemas/examples/profile-list-introspection.example.json`
- Task-template list introspection example: `docs/schemas/examples/task-template-list-introspection.example.json`

## Pipeline Guidance

1. Parse JSON payload.
2. Validate `contract_version` against expected value.
3. Validate payload shape using the matching schema.
4. Ignore unknown fields for forward compatibility unless policy requires strict rejection.
