# CI/CD Overview

This repository uses GitHub Actions for quality gates, release planning, and artifact publication.

## Workflows

- `ci.yml`
  - Runs formatting checks, linting, tests, and cross-platform workspace compilation.
  - Validates feature-gated Vitis build path.
  - Runs live-mode metrics benchmark regression checks and validates machine-readable telemetry thresholds.
  - Includes an infrastructure-gated live-success e2e lane for Vitis environments (no fallback expected).
  - Cross-platform checks run on Linux, macOS, and Windows.

- `release-drafter.yml`
  - Maintains a draft release summary from merged pull requests and labels.

- `release.yml`
  - Performs preflight checks (`cargo check`, `cargo test --workspace`, Vitis feature check).
  - Builds cross-platform release packages on tag pushes (`v*.*.*`) and publishes GitHub Releases.
  - Publishes Windows (`.zip`, `.msi`), Linux (`.tar.gz`, `.deb`, `.rpm`), and macOS (`.tar.gz`, `.pkg`) artifacts.
  - Generates `SHA256SUMS` and `SBOM.spdx.json` release assets.
  - Runs post-install smoke checks for archive and installer packages before publishing.

- `dependency-review.yml`
  - Runs dependency review on pull requests.
  - If Dependency graph is disabled for the repository, the workflow skips review with a warning instead of failing.

- `security.yml`
  - Runs dependency audit on schedule and manual invocation.

- `labels.yml`
  - Synchronizes repository labels from `.github/labels.yml`.

- `milestones.yml`
  - Creates milestones from manual workflow dispatch input if they do not already exist.

- `docs.yml`
  - Builds MkDocs documentation in strict mode on docs/config changes.
  - Prevents publishing broken docs navigation or markdown references.

## CI Expectations for Pull Requests

Before merge, pull requests should satisfy:

- `cargo fmt --all -- --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test --workspace`
- `cargo check --workspace`
- `cargo check -p inference_bridge --features vitis`

When live-success e2e is enabled (self-hosted Vitis runner), the CI lane also runs:

- `cargo test -p wraithrun --features inference_bridge/vitis --test stdin_integration live_mode_e2e_success_without_fallback_when_fixture_is_configured -- --exact`

Live-success lane prerequisites (repository variables):

- `WRAITHRUN_LIVE_SUCCESS_E2E_ENABLED=true`
- `WRAITHRUN_LIVE_E2E_MODEL`
- `WRAITHRUN_LIVE_E2E_TOKENIZER`
- Optional Vitis tuning: `WRAITHRUN_LIVE_E2E_VITIS_CONFIG`, `WRAITHRUN_LIVE_E2E_VITIS_CACHE_DIR`, `WRAITHRUN_LIVE_E2E_VITIS_CACHE_KEY`

Runner labels expected for this lane:

- `self-hosted`
- `linux`
- `x64`
- `vitis`

## Release Notes and Labels

Release notes quality depends on pull request labels.

Recommended labels:

- `feature`
- `enhancement`
- `fix`
- `bug`
- `docs`
- `test`
- `chore`
- `ci`
- `dependencies`
- `release`
- `security`
- `breaking`

## Release Trigger

Create and push a semantic version tag:

```powershell
git tag -a v0.2.1 -m "Release v0.2.1"
git push origin v0.2.1
```

This triggers release build and publication workflow.

Manual dispatch is also supported, but the provided tag must match semantic version format (for example `v0.2.1`).

To enforce full dependency review behavior, enable Dependency graph in repository security analysis settings.

## Branch Protection Baseline (main)

Recommended branch protection settings for `main`:

- Require a pull request before merging.
- Require at least 1 approving review.
- Require status checks to pass before merging.
- Require branches to be up to date before merging.
- Restrict direct pushes to `main`.

Recommended required checks:

- `Quality Gates (ubuntu)`
- `Live metrics benchmark regression (ubuntu)`
- `Cross-platform compile (ubuntu-latest)`
- `Cross-platform compile (macos-latest)`
- `Cross-platform compile (windows-latest)`
- `dependency-review` (recommended once Dependency graph is enabled)

Optional checks (advisory, not required for every PR):

- `Dependency Vulnerability Audit` (scheduled/manual security workflow)
- `Release Drafter` (draft notes maintenance)
- `Live success e2e (self-hosted vitis)` (runs only when explicitly enabled)
