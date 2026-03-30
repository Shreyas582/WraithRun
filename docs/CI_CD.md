# CI/CD Overview

This repository uses GitHub Actions for quality gates, release planning, and artifact publication.

## Workflows

- `ci.yml`
  - Runs formatting checks, linting, tests, and cross-platform workspace compilation.
  - Validates feature-gated Vitis build path.
  - Cross-platform checks run on Linux, macOS, and Windows.

- `release-drafter.yml`
  - Maintains a draft release summary from merged pull requests and labels.

- `release.yml`
  - Performs preflight checks (`cargo check`, `cargo test --workspace`, Vitis feature check).
  - Builds release binaries on tag pushes (`v*.*.*`) and publishes GitHub Releases.
  - Publishes Linux, macOS, and Windows CLI artifacts.

- `security.yml`
  - Runs dependency audit on schedule and manual invocation.

## CI Expectations for Pull Requests

Before merge, pull requests should satisfy:

- `cargo fmt --all -- --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test --workspace`
- `cargo check --workspace`
- `cargo check -p inference_bridge --features vitis`

## Release Notes and Labels

Release notes quality depends on pull request labels.

Recommended labels:

- `feature`
- `fix`
- `docs`
- `test`
- `chore`
- `breaking`

## Release Trigger

Create and push a semantic version tag:

```powershell
git tag v0.2.0
git push origin v0.2.0
```

This triggers release build and publication workflow.

Manual dispatch is also supported, but the provided tag must match semantic version format (for example `v0.2.0`).
