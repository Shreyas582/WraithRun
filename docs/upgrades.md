# Upgrade Notes

## v0.7.0

### Breaking/visible changes

- Run and introspection JSON outputs now include top-level `contract_version` (`1.0.0`) for automation compatibility checks.
- Added machine-readable JSON schema and example files for run report and core introspection outputs under `docs/schemas/`.
- Added `--automation-adapter findings-v1` output mode for findings-only normalized pipeline ingestion.
- Added severity-threshold exit policy controls (`--exit-policy`, `--exit-threshold`) for deterministic CI/SIEM gating.
- Added case-workflow runbook examples for evidence collection, integrity verification, and retention operations.
- Expanded evidence-bundle path handling coverage to include direct `SHA256SUMS` verification and path-with-spaces workflows.

### Migration examples

Verify bundle integrity via direct checksum-manifest path:

```powershell
.\wraithrun.exe --verify-bundle ".\evidence\CASE-2026-IR-0042\run 01\SHA256SUMS"
```

Import baseline arrays directly from a `raw_observations.json` file path:

```powershell
.\wraithrun.exe --task "Audit account change activity in admin group membership" --baseline-bundle ".\evidence\CASE-2026-IR-0042\baseline\raw_observations.json"
```

Run with findings adapter and severity gate:

```powershell
.\wraithrun.exe --task "Investigate unauthorized SSH keys" --automation-adapter findings-v1 --exit-policy severity-threshold --exit-threshold high
```

### Recommended checks after upgrade

- Validate automation parsers check `contract_version` before strict field-level validation.
- Validate contract checks in CI against the published schema set in `docs/automation-contracts.md`.
- Validate adapter parsers against `docs/schemas/automation-adapter-findings-v1.schema.json`.
- Confirm pipeline exit behavior for each threshold (`info` through `critical`) in both dry-run and live mode.
- Validate your incident-response runbook uses the documented collection, verify, and retention sequence for case workflows.
- Add test coverage in downstream wrappers for bundle paths that include spaces or direct checksum-manifest references.

## v0.6.2

### Breaking/visible changes

- Added optional `--evidence-bundle-archive` runtime output for deterministic single-file evidence bundle export (`.tar`).

### Migration examples

Export a deterministic evidence bundle archive during a case-tagged run:

```powershell
.\wraithrun.exe --task "Investigate unauthorized SSH keys" --case-id CASE-2026-IR-0042 --evidence-bundle-archive .\evidence\CASE-2026-IR-0042.tar
```

### Recommended checks after upgrade

- Confirm receiving workflows can ingest `.tar` evidence bundles and still validate contents with `--verify-bundle` after extraction.
- If you generate archives in CI, ensure archive paths are unique per case to avoid accidental overwrite.

## v0.6.1

### Breaking/visible changes

- Added optional `--baseline-bundle` runtime input to import drift baseline arrays from prior evidence bundles.
- Added `--verify-bundle` mode to validate evidence bundle file integrity against `SHA256SUMS`.

### Migration examples

Load prior baseline arrays while running account drift checks:

```powershell
.\wraithrun.exe --task "Audit account change activity in admin group membership" --baseline-bundle .\evidence\CASE-2026-IR-0042
```

Verify evidence bundle integrity before sharing:

```powershell
.\wraithrun.exe --verify-bundle .\evidence\CASE-2026-IR-0042 --introspection-format json
```

### Recommended checks after upgrade

- Ensure baseline bundles retained for drift workflows include `raw_observations.json` with a `capture_coverage_baseline` tool observation.
- Gate evidence sharing steps on successful `--verify-bundle` checks to avoid distributing tampered or incomplete bundles.

## v0.6.0

### Breaking/visible changes

- Added optional `--case-id` for case-tagged run reports.
- Added optional `--evidence-bundle-dir` to export auditable investigation artifacts (`report.json`, `raw_observations.json`, `SHA256SUMS`).
- Run report JSON now includes optional `case_id` when set.

### Migration examples

Run with case metadata and evidence export:

```powershell
.\wraithrun.exe --task "Investigate unauthorized SSH keys" --case-id CASE-2026-IR-0042 --evidence-bundle-dir .\evidence\CASE-2026-IR-0042
```

### Recommended checks after upgrade

- Validate downstream automation tolerates optional `case_id` in JSON output.
- Confirm evidence bundle storage permissions and retention controls for exported artifacts.
- Verify checksum verification procedure is documented in incident-response runbooks.

## v0.5.0

### Breaking/visible changes

- Run report JSON now includes a first-class `findings[]` layer with severity, confidence, evidence pointers, and recommended actions.
- Summary and markdown output now render findings before turn-by-turn evidence.
- `--list-tools --tool-filter <QUERY>` now supports multi-term, separator-normalized matching.
- Added host coverage tools for persistence inventory, account/role snapshots, and process-network correlation.
- Coverage tools now support optional baseline/allowlist argument sets and emit drift/risk metrics (`baseline_new_count`, `newly_privileged_account_count`, `network_risk_score`).
- Added `capture_coverage_baseline` tool to generate reusable baseline arrays for persistence, account, and network drift workflows.

### Migration examples

Run a standard task and inspect findings in JSON output:

```powershell
.\wraithrun.exe --task "Investigate unauthorized SSH keys"
```

Filter tools using multiple terms:

```powershell
.\wraithrun.exe --list-tools --tool-filter "priv esc"
```

Run process-network correlation task:

```powershell
.\wraithrun.exe --task "Correlate process and network listener exposure" --format summary
```

### Recommended checks after upgrade

- If automation consumes run output JSON, parse `findings[]` and ignore unknown future fields for forward compatibility.
- Validate analyst runbooks treat `evidence_pointer` as a jump target into `turns[]` observations.
- Confirm triage dashboards can display severity/confidence and recommended action from findings.
- Validate runbooks include the new persistence/account/process-network coverage tasks for baseline collection.
- If your automation compares host state over time, feed baseline arrays into tool calls and alert on the new drift counters.
- Capture and store baseline snapshots periodically so drift-aware tool arguments can be refreshed from recent known-good host states.

## v0.4.1

### Breaking/visible changes

- `--describe-tool <NAME>` now accepts unique partial and hyphenated tool queries.
- `--describe-tool` now fails fast with an explicit ambiguous-query error when multiple tools match.

### Migration examples

Describe a tool with a hyphenated alias:

```powershell
.\wraithrun.exe --describe-tool hash-binary
```

Describe a tool with a unique partial query:

```powershell
.\wraithrun.exe --describe-tool privilege
```

### Recommended checks after upgrade

- If automation drives `--describe-tool`, ensure query strings remain unique or switch to full tool names.
- Confirm operator runbooks handle ambiguous-query failures by retrying with exact tool names.

## v0.4.0

### Breaking/visible changes

- Added `--tool-filter <QUERY>` for filtered tool discovery in `--list-tools` mode.

### Migration examples

Filter tool list by keyword:

```powershell
.\wraithrun.exe --list-tools --tool-filter hash
```

Filter tool list as JSON:

```powershell
.\wraithrun.exe --list-tools --tool-filter network --introspection-format json
```

### Recommended checks after upgrade

- Validate tooling that consumes `--list-tools` handles filtered result sets.
- Validate automation handles no-match failures when a filter is too restrictive.

## v0.3.3

### Breaking/visible changes

- Added tool catalog introspection mode via `--list-tools`.
- Added single-tool introspection mode via `--describe-tool <NAME>`.
- Added JSON contract output support for `--describe-tool` with stable `tool` object shape.

### Migration examples

List all tools:

```powershell
.\wraithrun.exe --list-tools
```

Describe one tool as JSON:

```powershell
.\wraithrun.exe --describe-tool hash_binary --introspection-format json
```

### Recommended checks after upgrade

- If you automate against introspection data, validate parsers for both `tools[]` (`--list-tools`) and `tool` (`--describe-tool`).
- For operator runbooks, map critical workflows to specific tool names using `--describe-tool` output.

## v0.3.2

### Breaking/visible changes

- Stdin-based task entry is now covered by dedicated integration tests in CI on Linux and Windows.
- Release artifacts now include a checksum manifest (`SHA256SUMS`) for integrity verification.
- `--task-file` now supports UTF-16 BOM encoded files commonly produced by Windows editors.

### Migration examples

Task from stdin:

```powershell
Get-Content .\incident-task.txt | .\wraithrun.exe --task-stdin --format summary
```

Task file with UTF-16 content:

```powershell
.\wraithrun.exe --task-file .\incident-task-utf16.txt --format summary
```

Checksum verification (PowerShell):

```powershell
Get-FileHash .\wraithrun-windows-x86_64.zip -Algorithm SHA256
Get-Content .\SHA256SUMS
```

### Recommended checks after upgrade

- Validate automation that reads introspection JSON still works with the documented schema contract.
- Verify local wrappers/scripts can pass task input via stdin where desired.
- For release consumers, verify downloaded artifact hashes against `SHA256SUMS`.

## v0.3.1

### Breaking/visible changes

- Added integration-test coverage for stdin task entry paths.
- Added documented introspection JSON schema examples for automation consumers.

### Recommended checks after upgrade

- Re-run automation that consumes `--introspection-format json` output.
- Confirm local scripted runs using `--task-stdin` and `--task-file -` still behave as expected.

## v0.2.1

### Breaking/visible changes

- Primary executable name is now `wraithrun`.
- Release artifacts are now named with `wraithrun-*` prefixes.

### Migration examples

Old source command:

```powershell
cargo run -p agentic-cyber-cli -- --task "Investigate unauthorized SSH keys"
```

New source command:

```powershell
cargo run -p wraithrun -- --task "Investigate unauthorized SSH keys"
```

Old binary:

- `agentic-cyber-cli.exe`

New binary:

- `wraithrun.exe`

### Recommended checks after upgrade

- Re-run your automation scripts with new command names.
- Verify release asset download names in CI/CD or deployment scripts.
- Confirm expected JSON output shape in downstream parsers.
