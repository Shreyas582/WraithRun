# Upgrade Notes

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
