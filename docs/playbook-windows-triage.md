# Playbook: Triage a Potentially Compromised Windows Workstation

## Goal

Perform initial triage of a Windows workstation suspected of compromise. Identify active threats, persistence mechanisms, and lateral movement indicators.

## Prerequisites

- WraithRun installed on the target workstation or a forensic copy accessible locally.
- The host runs Windows 10/11 or Windows Server 2016+.
- You have administrator-level read access.

## Steps

### 1. Capture a coverage baseline

Start with a comprehensive snapshot of the system's current state including running processes, network connections, services, and scheduled tasks.

```bash
wraithrun run "capture a full coverage baseline of this Windows workstation"
```

### 2. Inspect persistence mechanisms

Check Run keys, scheduled tasks, services, startup folders, and WMI subscriptions for unauthorized entries.

```bash
wraithrun run "inspect all persistence locations for suspicious autostart entries or recently modified scheduled tasks"
```

### 3. Correlate processes with network activity

Identify processes making outbound connections to unusual destinations, high-entropy domain names, or known C2 ports.

```bash
wraithrun run "correlate running processes with their network connections and flag any suspicious outbound activity"
```

### 4. Check for privilege escalation vectors

Determine if the attacker has escalated privileges or left paths open for future escalation.

```bash
wraithrun run "check for privilege escalation vectors including unquoted service paths and writable service binaries"
```

### 5. Audit account changes

Look for new local accounts, group membership changes, or password resets that occurred during the suspected compromise window.

```bash
wraithrun run "audit local account changes for new accounts or group membership modifications"
```

## Expected Output Walkthrough

- **Critical — Suspicious scheduled task `WindowsUpdateHelper`**: Task created 3 days ago, runs a PowerShell-encoded command every 4 hours. Points to T1053.005.
- **High — Process `svchost_helper.exe` connecting to external IP on port 443**: Unknown binary with network activity to a non-Microsoft IP.
- **Medium — Unquoted service path for `BackupAgent` service**: Could be exploited for privilege escalation via path interception.
- **Low — Local account `admin2` created recently**: No corresponding IT change ticket found.

## Next Steps

- Isolate the workstation from the network if active C2 is confirmed.
- Collect volatile evidence (memory dump) before remediation.
- Remove malicious persistence entries.
- Hash suspicious binaries with `hash_binary` and check against threat intelligence feeds.
- Create a formal investigation case: `POST /api/v1/cases` with findings linked.
- Generate a narrative report: `wraithrun run --format narrative`.
