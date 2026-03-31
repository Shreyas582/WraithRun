# Tool Reference

WraithRun includes a local tool registry used by the agent during the ReAct loop.

## read_syslog

Purpose:

- Reads local log file tail lines in a bounded format.

Arguments:

- `path` (string, optional): log path. Default: `./agent.log`.
- `max_lines` (integer, optional): number of lines, range 1-1000. Default behavior targets a bounded tail.

Output fields:

- `path`
- `line_count`
- `lines`

## scan_network

Purpose:

- Lists active local listening sockets.

Arguments:

- `limit` (integer, optional): max entries, range 1-512.

Output fields:

- `listener_count`
- `listeners`

## hash_binary

Purpose:

- Computes SHA-256 hash for a local file.

Arguments:

- `path` (string, required)

Output fields:

- `path`
- `sha256`

## check_privilege_escalation_vectors

Purpose:

- Collects local privilege-surface indicators.

Arguments:

- none

Output fields:

- `indicator_count`
- `potential_vectors`
- `sample`

## inspect_persistence_locations

Purpose:

- Inventories common persistence locations and highlights suspicious entries.

Arguments:

- `limit` (integer, optional): max entries, range 1-512.
- `baseline_entries` (string array, optional): known-good persistence entry names for drift comparison.
- `allowlist_terms` (string array, optional): terms that suppress known-benign suspicious matches.

Output fields:

- `entry_count`
- `suspicious_entry_count`
- `actionable_suspicious_count`
- `baseline_new_count`
- `baseline_new_entries`
- `entries`

## audit_account_changes

Purpose:

- Captures privileged account state and highlights drift or unapproved memberships.

Arguments:

- `baseline_privileged_accounts` (string array, optional): previous privileged-account snapshot.
- `approved_privileged_accounts` (string array, optional): approved privileged-account allowlist.

Output fields:

- `privileged_account_count`
- `non_default_privileged_account_count`
- `newly_privileged_account_count`
- `removed_privileged_account_count`
- `unapproved_privileged_account_count`
- `privileged_accounts`
- `evidence`

## correlate_process_network

Purpose:

- Correlates listening sockets with process ownership and scores exposure risk.

Arguments:

- `limit` (integer, optional): max entries, range 1-512.
- `baseline_exposed_bindings` (string array, optional): known externally exposed listener bindings.
- `expected_processes` (string array, optional): approved process names for exposed listeners.

Output fields:

- `listener_count`
- `externally_exposed_count`
- `high_risk_exposed_count`
- `unknown_exposed_process_count`
- `new_exposed_binding_count`
- `network_risk_score`
- `network_risk_level`
- `records`

## capture_coverage_baseline

Purpose:

- Captures reusable baseline arrays for persistence, privileged accounts, and exposed process-network bindings.

Arguments:

- `persistence_limit` (integer, optional): max persistence entries, range 1-512.
- `listener_limit` (integer, optional): max listener records, range 1-512.

Output fields:

- `baseline_version`
- `captured_epoch_seconds`
- `baseline_entries_count`
- `baseline_privileged_account_count`
- `baseline_exposed_binding_count`
- `persistence.baseline_entries`
- `accounts.baseline_privileged_accounts`
- `accounts.approved_privileged_accounts`
- `network.baseline_exposed_bindings`
- `network.expected_processes`
