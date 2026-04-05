# Playbook: Audit Privileged Account Changes After a Credential Leak

## Goal

After learning of a credential leak (e.g., password dump, phishing compromise, or insider threat), determine what account-level changes have been made and whether the compromised credentials were used.

## Prerequisites

- WraithRun installed on the target host.
- Access to local user databases and authentication logs.
- Known timeframe of the credential leak or suspected compromise window.

## Steps

### 1. Audit account changes

Check all local account modifications, creations, deletions, group membership changes, and password resets.

```bash
wraithrun run "audit all account changes looking for new accounts, group membership modifications, and password resets"
```

### 2. Review authentication logs

Search for login attempts — especially successful logins from unusual times, sources, or patterns.

```bash
wraithrun run "read syslog entries for authentication events, failed logins, and sudo usage during the past 72 hours"
```

### 3. Check for persistence

Attackers who obtain valid credentials often install persistence to survive password rotations.

```bash
wraithrun run "inspect persistence locations for anything added in the past week"
```

### 4. Correlate network and process activity

Look for sessions or processes running under the compromised account that show unusual behavior.

```bash
wraithrun run "correlate processes and network connections for processes running as the compromised user account"
```

## Expected Output Walkthrough

- **High — New account `svc-monitor` added to Administrators group**: Account created 12 hours after the credential leak timeline.
- **Medium — Successful SSH login from external IP for user `deploy`**: Login occurred outside normal business hours.
- **Medium — Cron job added under user `deploy`**: Runs a curl command to an external URL every 6 hours.
- **Info — No privilege escalation vectors detected**: System configuration is not vulnerable to common escalation paths.

## Next Steps

- Force password rotation for all potentially compromised accounts.
- Remove unauthorized accounts and group memberships.
- Revoke and rotate all API keys, tokens, and SSH keys for affected accounts.
- Disable persistence entries planted by the attacker.
- Notify affected users and initiate incident response procedures.
- Document findings in a case: `POST /api/v1/cases`.
