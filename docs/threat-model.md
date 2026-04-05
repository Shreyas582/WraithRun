# WraithRun Threat Model

This document describes WraithRun's own attack surface, trust boundaries, and security controls. It is intended for security professionals evaluating WraithRun before deploying it in their environment.

## System Overview

WraithRun is a local-first cyber investigation runtime. It runs entirely on the local host, with no cloud dependencies. In API server mode, it binds to `127.0.0.1` by default.

### Components

| Component | Role | Trust Level |
|-----------|------|-------------|
| CLI (`wraithrun`) | User interface, config parsing, report rendering | Trusted — user-controlled |
| Core Engine | Agent loop, finding derivation, report generation | Trusted — internal logic |
| Inference Bridge | ONNX Runtime model loading and inference | Partially trusted — processes model files |
| Cyber Tools | 8 built-in investigation tools | Trusted — sandboxed execution |
| API Server | Local REST API and dashboard | Trusted — localhost-only by default |
| SQLite Database | Persistent storage for runs and cases | Trusted — local file |
| Audit Log | JSON-lines event trail | Trusted — append-only local file |

## Trust Boundaries

### Boundary 1: User Input → CLI

- **Threat**: Malicious task descriptions, config file injection, path traversal in arguments.
- **Controls**: Task strings are treated as opaque text, never interpolated into shell commands. File paths are validated against the sandbox policy before use. Config files are parsed with TOML (no code execution).

### Boundary 2: CLI → Tool Execution

- **Threat**: A crafted task could cause the agent to invoke tools with dangerous arguments.
- **Controls**: The `SandboxPolicy` enforces:
  - **Path allowlist/denylist**: Tools can only read from explicitly allowed directory trees. Sensitive paths (`/etc/shadow`, `/proc`, `C:\Windows\System32\config`) are denied by default.
  - **Command allowlist/denylist**: Only specific system commands are allowed (`ss`, `id`, `netstat`, `whoami`, etc.). Shells (`bash`, `powershell`, `cmd`) are denied.
  - **No write operations**: Tools only read and inspect — they never modify the target system.

### Boundary 3: API Server → Network

- **Threat**: Unauthorized access to the API, cross-origin attacks, denial of service.
- **Controls**:
  - **Localhost binding**: The server binds to `127.0.0.1` by default. External access requires explicit `--bind` override.
  - **Bearer token authentication**: All mutating and data-reading endpoints require a valid bearer token. The token is auto-generated (UUID v4) on startup and displayed in the terminal. Users can override with `--api-token`.
  - **Request body size limit**: Default 1 MiB limit prevents oversized payloads.
  - **No CORS headers**: The dashboard is served from the same origin. No cross-origin API access is permitted.

### Boundary 4: Model Files → Inference Engine

- **Threat**: Malicious ONNX model files could exploit vulnerabilities in the ONNX Runtime.
- **Controls**:
  - Models are loaded from a user-specified local path — WraithRun does not download models from the internet.
  - In dry-run mode (default), no model is loaded or executed at all.
  - The ONNX Runtime is a well-maintained dependency with ongoing security updates.
  - Users should verify model file integrity (SHA-256) before use.

### Boundary 5: Dashboard → Browser

- **Threat**: Cross-site scripting (XSS) via finding titles, task descriptions, or other user-supplied text.
- **Controls**:
  - All dynamic text rendered in the dashboard HTML is escaped via a DOM-based `textContent` escaping function (`esc()`). No `innerHTML` is used with raw user input.
  - The API token is stored in `localStorage` and transmitted via `Authorization` header — not in URL parameters or cookies.
  - No external scripts, stylesheets, or CDNs are referenced. The dashboard is a self-contained HTML file.

## Attack Surface Summary

| Surface | Risk Level | Mitigation |
|---------|-----------|------------|
| CLI argument parsing | Low | Validated by clap; no shell interpolation |
| Task description text | Low | Treated as opaque; escaped in HTML output |
| Tool subprocess execution | Medium | Sandbox policy (path + command allowlists) |
| Local API server | Medium | Localhost-only + bearer token auth |
| Model file loading | Medium | User-controlled path; dry-run by default |
| SQLite database | Low | Local file; no SQL injection (parameterized queries) |
| Audit log file | Low | Append-only; structured JSON |
| Dashboard HTML | Low | Self-contained; all output escaped |

## Data Flow

```
User → CLI → Core Engine → Tool Registry → Sandbox Policy → System Commands
                ↓                                              ↓
         Inference Bridge                               Tool Observations
          (optional)                                         ↓
                ↓                                    Finding Derivation
           LLM Output                                       ↓
                ↓                                      Run Report
         Agent Decisions                                    ↓
                                                   API / Dashboard / File
```

## Recommendations for Deployers

1. **Run in dry-run mode** for initial evaluation. No model loading, no inference, deterministic output.
2. **Review sandbox policy** before first use. Adjust `WRAITHRUN_ALLOWED_READ_ROOTS` and `WRAITHRUN_COMMAND_ALLOWLIST` environment variables for your environment.
3. **Protect the API token**. Treat it like a password. Do not embed it in scripts stored in version control.
4. **Keep the server on localhost**. If you must expose the API to the network, add a reverse proxy with TLS and additional authentication.
5. **Verify model files**. If using live inference mode, only load models from trusted sources. Check SHA-256 hashes before deployment.
6. **Enable audit logging**. Use `--audit-log <path>` to maintain a tamper-evident record of all API operations.
7. **Use database persistence**. Use `--database <path>` to ensure investigation history survives server restarts.
