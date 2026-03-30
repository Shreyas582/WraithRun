# Changelog

All notable changes to this project will be documented in this file.

The format is inspired by Keep a Changelog and this project follows Semantic Versioning principles.

## Unreleased

### Added

- Tokenizer-backed greedy decode loop for live ONNX/Vitis inference.
- Path and command policy enforcement for local tool sandboxing.
- Core ReAct integration tests using mocked inference responses.
- Initial GitHub Actions automation for CI, release drafting, security checks, and tagged releases.

### Changed

- README and contribution docs expanded with release and CI/CD guidance.

## 0.1.0 - 2026-03-29

### Added

- Initial Rust workspace scaffold with modular crates.
- Core ReAct orchestration loop and tool-call parsing.
- Local cyber tool registry and host probing primitives.
- Dry-run inference behavior and feature-gated Vitis session bridge.
- CLI entrypoint for local runtime execution.
- Open-source governance docs (license, code of conduct, security, contribution guide).
