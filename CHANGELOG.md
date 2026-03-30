# Changelog

All notable changes to this project will be documented in this file.

The format is inspired by Keep a Changelog and this project follows Semantic Versioning principles.

## Unreleased

### Added

- Public usage examples guide for self-serve adoption.

### Changed

- README rewritten with user-first onboarding, binary usage, and practical CLI guidance.
- CI/CD and release docs updated for annotated tag flow and latest workflow behavior.

### Fixed

- Dependency review workflow now auto-detects dependency graph support and skips with warning when unavailable.
- Linux-target clippy dead-code failure in cross-platform CI.

## 0.2.0 - 2026-03-29

### Added

- Tokenizer-backed greedy decode loop for live ONNX/Vitis inference.
- Path and command policy enforcement for local tool sandboxing.
- Core ReAct integration tests using mocked inference responses.
- GitHub Actions automation for CI, release drafting, security checks, label sync, milestone bootstrap, and tagged releases.

### Changed

- Release workflow now runs preflight checks before publishing and includes Linux, macOS, and Windows artifacts.
- Release drafting configuration now uses resolved semantic versioning based on labels.
- Project docs expanded with release planning and CI/CD guidance.

## 0.1.0 - 2026-03-29

### Added

- Initial Rust workspace scaffold with modular crates.
- Core ReAct orchestration loop and tool-call parsing.
- Local cyber tool registry and host probing primitives.
- Dry-run inference behavior and feature-gated Vitis session bridge.
- CLI entrypoint for local runtime execution.
- Open-source governance docs (license, code of conduct, security, contribution guide).
