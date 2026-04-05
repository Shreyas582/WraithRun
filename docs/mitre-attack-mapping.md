# MITRE ATT&CK Mapping

This document maps each WraithRun built-in tool to the MITRE ATT&CK techniques it helps detect or investigate. Technique IDs reference the [Enterprise ATT&CK Matrix](https://attack.mitre.org/matrices/enterprise/).

## Tool-to-Technique Reference

| Tool | ATT&CK Techniques | Description |
|------|-------------------|-------------|
| `read_syslog` | [T1070.002](https://attack.mitre.org/techniques/T1070/002/) — Clear Linux or Mac System Logs | Reads log entries to detect evidence of log tampering or gaps |
| | [T1078](https://attack.mitre.org/techniques/T1078/) — Valid Accounts | Identifies authentication events indicating credential reuse |
| | [T1110](https://attack.mitre.org/techniques/T1110/) — Brute Force | Detects patterns of failed login attempts |
| `scan_network` | [T1049](https://attack.mitre.org/techniques/T1049/) — System Network Connections Discovery | Lists active listening sockets and connections |
| | [T1071](https://attack.mitre.org/techniques/T1071/) — Application Layer Protocol | Identifies unexpected outbound connections on standard ports |
| | [T1572](https://attack.mitre.org/techniques/T1572/) — Protocol Tunneling | Detects unusual port usage that may indicate tunneling |
| `hash_binary` | [T1036](https://attack.mitre.org/techniques/T1036/) — Masquerading | Verifies file integrity to detect replaced or trojanized binaries |
| | [T1027](https://attack.mitre.org/techniques/T1027/) — Obfuscated Files or Information | Produces SHA-256 hashes for threat intelligence correlation |
| `check_privilege_escalation_vectors` | [T1548](https://attack.mitre.org/techniques/T1548/) — Abuse Elevation Control Mechanism | Checks for SUID/SGID binaries, sudo misconfigurations |
| | [T1574.009](https://attack.mitre.org/techniques/T1574/009/) — Path Interception by Unquoted Service Path | Detects unquoted Windows service paths |
| | [T1068](https://attack.mitre.org/techniques/T1068/) — Exploitation for Privilege Escalation | Identifies writable service binaries and weak permissions |
| `inspect_persistence_locations` | [T1053](https://attack.mitre.org/techniques/T1053/) — Scheduled Task/Job | Inspects cron jobs, systemd timers, and Windows scheduled tasks |
| | [T1547.001](https://attack.mitre.org/techniques/T1547/001/) — Registry Run Keys / Startup Folder | Checks Windows Run keys and startup directories |
| | [T1543](https://attack.mitre.org/techniques/T1543/) — Create or Modify System Process | Inspects systemd services and Windows services |
| | [T1546.003](https://attack.mitre.org/techniques/T1546/003/) — Windows Management Instrumentation Event Subscription | Checks for WMI persistence |
| `audit_account_changes` | [T1136](https://attack.mitre.org/techniques/T1136/) — Create Account | Detects recently created local or domain accounts |
| | [T1098](https://attack.mitre.org/techniques/T1098/) — Account Manipulation | Identifies group membership changes, especially to privileged groups |
| | [T1531](https://attack.mitre.org/techniques/T1531/) — Account Access Removal | Detects account deletions or lockouts that may indicate anti-forensics |
| `correlate_process_network` | [T1071](https://attack.mitre.org/techniques/T1071/) — Application Layer Protocol | Maps processes to their network connections for C2 detection |
| | [T1095](https://attack.mitre.org/techniques/T1095/) — Non-Application Layer Protocol | Identifies processes using raw sockets or unusual protocols |
| | [T1571](https://attack.mitre.org/techniques/T1571/) — Non-Standard Port | Detects processes communicating on unexpected ports |
| | [T1573](https://attack.mitre.org/techniques/T1573/) — Encrypted Channel | Flags encrypted connections to non-standard destinations |
| `capture_coverage_baseline` | [T1082](https://attack.mitre.org/techniques/T1082/) — System Information Discovery | Captures full system state for comparison and anomaly detection |
| | [T1057](https://attack.mitre.org/techniques/T1057/) — Process Discovery | Lists all running processes with metadata |
| | [T1007](https://attack.mitre.org/techniques/T1007/) — System Service Discovery | Enumerates installed and running services |

## Tactic Coverage Summary

| Tactic | Covered Techniques | Primary Tools |
|--------|-------------------|---------------|
| Initial Access | T1078 (Valid Accounts) | `read_syslog`, `audit_account_changes` |
| Execution | T1053 (Scheduled Task) | `inspect_persistence_locations` |
| Persistence | T1053, T1547, T1543, T1546 | `inspect_persistence_locations`, `capture_coverage_baseline` |
| Privilege Escalation | T1548, T1574, T1068 | `check_privilege_escalation_vectors` |
| Defense Evasion | T1036, T1027, T1070 | `hash_binary`, `read_syslog` |
| Credential Access | T1110 (Brute Force) | `read_syslog` |
| Discovery | T1049, T1082, T1057, T1007 | `scan_network`, `capture_coverage_baseline` |
| Lateral Movement | T1078 | `audit_account_changes`, `read_syslog` |
| Command and Control | T1071, T1095, T1571, T1572, T1573 | `scan_network`, `correlate_process_network` |
| Impact | T1531 | `audit_account_changes` |

## Notes

- WraithRun tools focus on **detection** and **investigation** — they do not perform exploitation or active response.
- The agent's finding derivation engine uses tool observations to generate findings that reference these techniques contextually.
- For best coverage, combine multiple tools in a single investigation task. The agent will select the appropriate tools based on the task description.
