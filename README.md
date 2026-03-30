# WraithRun

Local-First Agentic Cyber Operations Runtime (Rust + NPU)

WraithRun is an execution runtime for autonomous security agents that keeps telemetry and evidence on the local machine by default.

## Project Status

Early-stage, functional prototype.

What is implemented now:

- Rust workspace with modular crates for engine, tools, inference bridge, and CLI.
- Bounded ReAct loop with tool-call parsing and observation feedback.
- Sandboxed local tools that return structured JSON output.
- Feature-gated ONNX Runtime + Vitis execution provider session initialization path.
- Dry-run mode for local end-to-end testing without model deployment.

What is intentionally pending:

- Tokenizer integration and token-by-token decode loop for live text generation.
- Expanded policy-based tool sandboxing.
- Integration and regression test suite.

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
cargo run -p agentic-cyber-cli --features inference_bridge/vitis -- --live --model C:/models/llm.onnx --task "Investigate unauthorized SSH keys"
```

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

## Roadmap

1. Complete live generation path (tokenizer + decode loop).
2. Harden sandboxing policies and capability controls.
3. Add test coverage for engine state transitions and tool contracts.
4. Expand platform-aware host telemetry tools.

## License

Released under the MIT License. See [LICENSE](LICENSE).
