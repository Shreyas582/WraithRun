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
