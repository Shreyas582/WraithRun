# Plugin Tool API

WraithRun supports loading external tools from a plugin directory. Each plugin
is a self-contained directory with a `tool.toml` manifest and an executable
command. Plugins communicate with the agent via JSON on stdin/stdout.

## Quick start

```bash
# 1. Create a plugin directory
mkdir -p ~/.config/wraithrun/tools/my_tool

# 2. Write a tool.toml manifest
cat > ~/.config/wraithrun/tools/my_tool/tool.toml <<'EOF'
name = "my_tool"
description = "Short description of what the tool does."
version = "0.1.0"
command = "my_tool.sh"
platforms = ["linux", "macos"]

[parameters.target]
type = "string"
required = true
description = "Host or path to operate on."
EOF

# 3. Write the executable
cat > ~/.config/wraithrun/tools/my_tool/my_tool.sh <<'SCRIPT'
#!/usr/bin/env bash
set -euo pipefail
input=$(cat)
echo '{"status":"ok","message":"tool ran successfully"}'
SCRIPT
chmod +x ~/.config/wraithrun/tools/my_tool/my_tool.sh

# 4. Run WraithRun with the plugin enabled
wraithrun --allowed-plugins my_tool "Investigate host 10.0.0.1"
```

## `tool.toml` reference

| Field          | Type       | Required | Description                                          |
|----------------|------------|----------|------------------------------------------------------|
| `name`         | string     | yes      | Unique tool name (must match directory name by convention). |
| `description`  | string     | yes      | One-line description shown to the LLM agent.         |
| `version`      | string     | no       | Semver version of the plugin.                        |
| `command`      | string     | yes      | Path to the executable, relative to the plugin dir.  |
| `platforms`    | string[]   | no       | Supported platforms (`linux`, `macos`, `windows`). Empty = all. |
| `timeout_secs` | integer    | no       | Max seconds the process may run. Default: 30.        |

### Parameters

Parameters are defined as TOML tables under `[parameters.<name>]`:

```toml
[parameters.target]
type = "string"
required = true
description = "The host to scan."

[parameters.port_range]
type = "string"
required = false
description = "Port range, e.g. 1-1024."
```

These are exposed to the LLM agent as the tool's JSON Schema arguments.

## JSON stdin/stdout contract

When the agent calls the plugin, WraithRun:

1. Spawns the plugin `command` with the plugin directory as the working directory.
2. Writes a JSON object to stdin containing the parameter values chosen by the agent.
3. Closes stdin.
4. Waits for the process to exit (subject to `timeout_secs`).
5. Reads stdout as a single JSON object (max 1 MiB).

### Input (stdin)

```json
{
  "target": "10.0.0.1",
  "port_range": "22-443"
}
```

### Output (stdout)

Any valid JSON object. The agent sees the full object as the tool result.

```json
{
  "open_ports": [22, 80, 443],
  "scan_time_ms": 1250
}
```

### Errors

If the process exits with a non-zero status, WraithRun captures up to 500
characters of stderr and reports a tool execution error to the agent.

## Security model

Plugins are subject to the same **sandbox policy** as built-in tools:

- The plugin command name is checked against the global command denylist
  (e.g. `rm`, `mkfs`, `dd`, `shutdown`). Denied commands are never executed.
- Plugins must be **explicitly allow-listed** via `--allowed-plugins`. Plugins
  not in the list are silently skipped during discovery.
- The plugin process runs with the same user permissions as WraithRun itself.
  It does **not** run in a container or seccomp sandbox.
- In dry-run mode (default), the agent proposes tool calls but does not
  execute them. Plugins only run when `--live` is active.

### Recommendations

- Review plugin code before adding it to the allow-list.
- Use the `platforms` field to prevent accidental execution on unsupported OSes.
- Set conservative `timeout_secs` values to prevent runaway processes.
- Place plugin directories on a read-only filesystem when possible.

## CLI flags

| Flag                | Description                                              |
|---------------------|----------------------------------------------------------|
| `--tools-dir PATH`  | Override the plugin directory (default: `~/.config/wraithrun/tools/`). |
| `--allowed-plugins` | Comma-separated list of plugin names to load.            |

## Diagnostics

- `--doctor` reports discovered plugin tools under the `plugin-tools` check.
- The `/api/v1/runtime/status` endpoint includes a `plugin_tools` array when
  plugins are loaded via `--serve`.

## Example plugin

See [`examples/tools/hello_world/`](../examples/tools/hello_world/) for a
minimal working plugin that echoes its input with a greeting.
