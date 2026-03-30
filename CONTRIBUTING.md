# Contributing to WraithRun

Thanks for your interest in contributing.

## Development Setup

1. Install Rust stable toolchain.
2. Clone the repository.
3. Run:

```powershell
cargo check
```

## Contribution Workflow

1. Open an issue describing the change or bug.
2. Create a focused branch from `main`.
3. Keep commits atomic and descriptive.
4. Add tests for behavior changes when practical.
5. Ensure `cargo check` passes before opening a pull request.

## Pull Request Expectations

- Explain what changed and why.
- Link related issues.
- Note security impact for tooling or execution changes.
- Include local validation steps.

## Code Style

- Follow idiomatic Rust style.
- Prefer explicit error handling over panics.
- Keep security-sensitive logic easy to audit.
