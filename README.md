# WraithRun

Local-First Agentic Cyber Operations Runtime (Rust + NPU)

WraithRun is an execution runtime for autonomous security agents that keeps telemetry and evidence on the local machine by default.

## Project Status

Early-stage, functional prototype.

What is implemented now:

- Rust workspace with modular crates for engine, tools, inference bridge, and CLI.
- Bounded ReAct loop with tool-call parsing and observation feedback.
- Sandboxed local tools with path and command policy enforcement.
- Feature-gated ONNX Runtime + Vitis execution provider path with tokenizer-backed greedy decode loop.
- Dry-run mode for local end-to-end testing without model deployment.
- Integration tests for core ReAct transitions with mocked inference behavior.

What is intentionally pending:

- KV-cache and streaming decode support for larger context efficiency.
- Expanded end-to-end workspace test coverage beyond core engine transitions.
- Signed release artifacts and SBOM publication.

## Why Local-First

- Sensitive host telemetry stays local.
- Lower latency for command-and-observe workflows.
- Better operator control over model execution and artifacts.
- Practical path for edge/NPU deployments.

## Architecture

WraithRun follows a CPU/NPU split around a ReAct loop.

- Orchestrator (CPU / Rust): state machine, prompt assembly, tool execution, observation loop.
- Brain (NPU / ONNX Runtime + Vitis): model session setup and response generation path.
- Tools (CPU / Rust): explicit allowlisted capabilities exposed as structured calls.

## Workspace Layout

```text
WraithRun/
├── Cargo.toml
├── README.md
├── core_engine/
│   └── src/
│       ├── lib.rs
│       └── agent.rs
├── inference_bridge/
│   └── src/
│       ├── lib.rs
│       └── onnx_vitis.rs
├── cyber_tools/
│   └── src/
│       ├── lib.rs
│       ├── log_parser.rs
│       └── network_scanner.rs
└── cli/
    └── src/
        └── main.rs
```

## Quick Start

1. Validate compile:

```powershell
cargo check
```

2. Run default dry-run flow (no model required):

```powershell
cargo run -p agentic-cyber-cli -- --task "Investigate unauthorized SSH keys"
```

3. Validate Vitis feature build path:

```powershell
cargo check -p inference_bridge --features vitis
```

4. Run live mode with ONNX model (requires Vitis-capable ONNX Runtime deployment):

```powershell
cargo run -p agentic-cyber-cli --features inference_bridge/vitis -- --live --model C:/models/llm.onnx --tokenizer C:/models/tokenizer.json --task "Investigate unauthorized SSH keys"
```

## Tool Sandbox Policy

WraithRun enforces a default sandbox policy for tool execution.

- Read paths are restricted to allowlisted roots.
- Sensitive paths are denylisted.
- Command-based tools are constrained by command allowlist and denylist checks.

Policy overrides are supported via environment variables:

- `WRAITHRUN_ALLOWED_READ_ROOTS`
- `WRAITHRUN_DENIED_READ_ROOTS`
- `WRAITHRUN_COMMAND_ALLOWLIST`
- `WRAITHRUN_COMMAND_DENYLIST`

Use platform path-list separators (`;` on Windows, `:` on Unix-like systems) for path list variables.

## Dependencies

- ort: ONNX Runtime Rust bindings and execution provider wiring.
- serde and serde_json: structured tool contracts and observations.
- tokio: async orchestration and process execution.
- tracing and tracing-subscriber: structured diagnostics.

## Security Model

- Local-first by default, with no cloud telemetry requirement.
- Explicit tool registry and allowlisted execution surface.
- Structured error returns instead of panics for tool workflows.
- Bounded step count in the ReAct loop to prevent runaway behavior.

## Responsible Use

This project is intended for authorized defensive security operations, controlled research, and local system analysis.

Do not use WraithRun on systems or networks you do not own or have explicit permission to assess.

## Contributing

Public contributions are welcome.

- Contribution process: [CONTRIBUTING.md](CONTRIBUTING.md)
- Community expectations: [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md)
- Security disclosure: [SECURITY.md](SECURITY.md)

## Release Management

- Changelog: [CHANGELOG.md](CHANGELOG.md)
- Release strategy and checklist: [docs/RELEASE_PLAN.md](docs/RELEASE_PLAN.md)
- CI/CD workflow documentation: [docs/CI_CD.md](docs/CI_CD.md)

Releases are automated through GitHub Actions when tags matching `v*.*.*` are pushed.

## Roadmap

1. Complete live generation path (tokenizer + decode loop).
2. Harden sandboxing policies and capability controls.
3. Add test coverage for engine state transitions and tool contracts.
4. Expand platform-aware host telemetry tools.

## License

Released under the MIT License. See [LICENSE](LICENSE).
