# Playbook: Check for Persistence Mechanisms Post-Breach

## Goal

After confirming a breach, perform a comprehensive sweep of all known persistence mechanisms to identify attacker footholds that would survive reboots, password resets, or partial remediation.

## Prerequisites

- WraithRun installed on the target host (Linux or Windows).
- Administrator or root read access.
- An established timeline of the breach for filtering recent changes.

## Steps

### 1. Full persistence inspection

Run a comprehensive check of all registered persistence locations.

```bash
wraithrun run "inspect all persistence locations for entries added or modified in the past 30 days"
```

### 2. Capture a coverage baseline

Get a full system snapshot to identify anything that the persistence check alone might miss.

```bash
wraithrun run "capture a full coverage baseline including all running processes, services, scheduled tasks, and startup items"
```

### 3. Hash suspicious binaries

For any suspicious files discovered in persistence locations, compute their SHA-256 hashes for threat intelligence lookup.

```bash
wraithrun run "hash the binaries referenced by suspicious persistence entries"
```

### 4. Review logs for persistence activity

Check system logs for evidence of persistence being installed — task scheduler events, service creation, registry modifications.

```bash
wraithrun run "read syslog and event log entries related to scheduled task creation, service installation, and registry changes"
```

### 5. Network correlation for active backdoors

Check if any persistence mechanism is currently phoning home.

```bash
wraithrun run "correlate process-network activity for processes spawned by scheduled tasks or startup items"
```

## Expected Output Walkthrough

- **Critical — Malicious systemd service `update-helper.service`**: Service file created post-breach, runs an obfuscated binary on boot.
- **High — Scheduled task with encoded PowerShell command**: Task `SystemHealthCheck` runs every 2 hours, downloads and executes remote payload.
- **High — Unknown binary `/usr/local/bin/sysmond` with active C2 connection**: SHA-256 hash not found in known-good baselines.
- **Medium — Modified `.bashrc` for user `www-data`**: Appended command that runs on every shell login.
- **Low — Cron job for log rotation looks legitimate**: Entry matches expected system maintenance pattern.

## Next Steps

- Prioritize removal of Critical and High persistence entries.
- Compare binary hashes against threat intelligence (VirusTotal, MISP).
- Re-image the host if persistence is deeply embedded (rootkit, firmware level).
- Monitor for re-infection after remediation.
- Generate a narrative report for stakeholders: `wraithrun run --format narrative`.
- Track all findings in a case for audit trail: `POST /api/v1/cases`.
