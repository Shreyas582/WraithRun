# Playbook: Investigate Suspicious SSH Keys on a Linux Host

## Goal

Determine whether unauthorized SSH keys have been planted on a Linux host, identify which accounts are affected, and assess whether the keys are actively being used for access.

## Prerequisites

- WraithRun installed and available on the target host (or a forensic copy of the filesystem mounted locally).
- The host runs Linux with OpenSSH configured.
- You have read access to `/home/`, `/root/`, and `/var/log/`.

## Steps

### 1. Run a persistence check

Inspect known persistence locations including SSH `authorized_keys` files, cron jobs, and systemd units that may reference planted keys.

```bash
wraithrun run "inspect persistence locations for planted SSH keys or suspicious authorized_keys entries"
```

### 2. Audit account changes

Look for recently created or modified accounts that an attacker might have added to host their SSH key.

```bash
wraithrun run "audit account changes for recently created or modified users"
```

### 3. Correlate network activity

Check if any active SSH sessions or connections originate from unexpected source IPs.

```bash
wraithrun run "correlate processes and network connections looking for unexpected SSH sessions"
```

### 4. Review syslog

Search authentication logs for key-based logins, failed attempts, or `sshd` configuration reloads.

```bash
wraithrun run "read syslog entries related to SSH authentication and key-based logins"
```

## Expected Output Walkthrough

A completed run with the above tasks should produce findings such as:

- **High — Unauthorized SSH key in `/home/deploy/.ssh/authorized_keys`**: A key not matching any known team member was added after the last authorized change window.
- **Medium — New user account `svc-backup` created recently**: Account created 2 days ago with no corresponding change request.
- **Low — SSH connections from internal subnet only**: No external SSH connections detected at this time.

## Next Steps

- Remove unauthorized keys from `authorized_keys` files.
- Disable or lock suspicious user accounts.
- Rotate all SSH host keys if compromise is confirmed.
- Review SSH configuration (`/etc/ssh/sshd_config`) for weakened settings (e.g., `PermitRootLogin yes`).
- Create an investigation case to track follow-up: `wraithrun serve` + `POST /api/v1/cases`.
