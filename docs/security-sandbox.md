# Security and Sandbox

WraithRun is local-first and applies sandbox policy checks for file paths and command execution.

## Policy Controls

Environment variables:

- `WRAITHRUN_ALLOWED_READ_ROOTS`
- `WRAITHRUN_DENIED_READ_ROOTS`
- `WRAITHRUN_COMMAND_ALLOWLIST`
- `WRAITHRUN_COMMAND_DENYLIST`

Path separator rules:

- Windows: use `;`.
- Linux/macOS: use `:`.

Command list rules:

- Use comma-separated command names.

## Example Overrides

Windows PowerShell:

```powershell
$env:WRAITHRUN_ALLOWED_READ_ROOTS = "C:\Logs;C:\Temp"
$env:WRAITHRUN_DENIED_READ_ROOTS = "C:\Windows\System32\config"
$env:WRAITHRUN_COMMAND_ALLOWLIST = "whoami,netstat"
$env:WRAITHRUN_COMMAND_DENYLIST = "powershell,pwsh,cmd"
```

Linux/macOS shell:

```bash
export WRAITHRUN_ALLOWED_READ_ROOTS="/var/log:/tmp"
export WRAITHRUN_DENIED_READ_ROOTS="/root:/proc"
export WRAITHRUN_COMMAND_ALLOWLIST="id,ss,sudo"
export WRAITHRUN_COMMAND_DENYLIST="bash,sh,python,curl,wget"
```

## Operational Guidance

- Keep allowlists narrow.
- Deny risky shell execution paths for automated runs.
- Prefer read-only investigation flows on sensitive hosts.
- Log all runs and preserve outputs for auditability.
