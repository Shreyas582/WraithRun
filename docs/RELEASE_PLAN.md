# Release Plan

## Goals

- Ship predictable, stable releases with auditable changes.
- Keep release notes contributor-friendly and operations-focused.
- Ensure reproducible binaries are generated from tagged commits.

## Versioning Strategy

This project follows Semantic Versioning principles.

- Major (`X.0.0`): breaking changes to public behavior, tool contracts, or CLI interfaces.
- Minor (`0.Y.0` while pre-1.0): new features, non-breaking architecture improvements.
- Patch (`0.0.Z`): bug fixes, docs updates, tests, and low-risk maintenance.

Until `1.0.0`, minor version bumps may include limited breaking changes if clearly documented.

## Branching and Tagging

- `main` is the release branch.
- Pull requests merge into `main` only after CI passes.
- Releases are tagged from `main` with `v*.*.*`.

Examples:

- `v0.3.0`
- `v0.3.1`
- `v0.4.0`

## Cadence

- Patch releases: target at least one release every 14 days when `Unreleased` contains user-visible changes.
- Patch releases: cut immediately for critical fixes.
- Minor releases: approximately every 2 to 6 weeks, depending on feature readiness.

## Release Checklist

1. Confirm all required CI jobs are green on `main`.
2. Ensure high-priority security findings are triaged.
3. Update release docs for shipped behavior:
   - `CHANGELOG.md`
   - `docs/upgrades.md`
   - `README.md`
   - `docs/cli-reference.md`
   - `docs/tool-reference.md`
   Then verify release notes labels on merged PRs.
4. Validate core commands locally:
   - `cargo check`
   - `cargo test -p core_engine`
   - `cargo check -p inference_bridge --features vitis`
5. Create and push an annotated tag: `git tag -a vX.Y.Z -m "Release vX.Y.Z"` then `git push origin vX.Y.Z`.
6. Wait for release workflow completion.
7. Verify generated GitHub release assets and notes.
8. Post-release sanity check by running released CLI binary.

## Go/No-Go Criteria

Release can proceed when:

- CI passes on latest `main` commit.
- No unresolved critical vulnerabilities in direct dependencies.
- Release notes and docs (`CHANGELOG.md` + `docs/upgrades.md` + user-facing CLI docs) are coherent and accurate.

Release should be blocked when:

- Regressions are detected in core runtime loop behavior.
- Security policy checks fail for tooling or dependency chain.
- Release artifacts are missing or corrupted.

## Post-Release Actions

- Open follow-up issues for deferred work.
- Refresh roadmap in `README.md` if priorities changed.
- Close the completed milestone tracker issue and milestone.
- Ensure the next milestone tracking issue is open and linked in release notes.

## Roadmap Milestones (v0.9.0-v1.4.0)

- `v0.9.0` Live-Mode Convenience First (completed):
   Delivered one-command live setup, actionable doctor remediation, model-pack lifecycle commands, cross-platform packaging, and live reliability/latency instrumentation.
- `v0.10.0` Runtime Compatibility and E2E Test Coverage (completed):
   KV-cache live inference (#37), runtime compatibility checks with ~30 deterministic reason codes (#38), E2E live-success test lane (#39), zero-guess setup with structured remediation metadata (#44), and release gate closure (#36). Tracker: #45.
- `v1.0.0` Local API and Web UI MVP:
   Add local API server endpoints plus baseline web UI workflows with secure local operation.
- `v1.1.0` Workflow Depth and Live Quality:
   Add policy lifecycle controls, suppressions, and live inference quality controls.
- `v1.2.0` Integrations and Team Mode:
   Add connector framework plus team-mode scheduling/backup foundations.
- `v1.3.0` Multi-Backend Inference Abstraction (milestone #16, tracking: #46):
   Define `ExecutionProviderBackend` trait, provider registry, provider-agnostic config, extract Vitis/CPU backends, provider-aware doctor, CLI `--backend` flag, and multi-backend test harness.
- `v1.4.0` Concrete Hardware Backends (milestone #17, tracking: #55):
   DirectML (Windows GPU), CoreML (macOS/Apple Silicon), CUDA/TensorRT (NVIDIA), QNN (Qualcomm Hexagon), non-ONNX formats (GGUF/SafeTensors), and quantization-aware loading.
- `v1.5.0` Concrete Hardware Backends (completed).
- `v1.6.0` Agentic Investigation Engine (completed): ReAct agent loop, task-aware LLM synthesis, temperature-scaled sampling, EP-aware debug logs, session caching, KV-cache prefix reuse.
- `v1.7.0` Live Evaluation Hardening (completed): per-tool timing, LLM reasoning capture, evidence-derived confidence, task-specific synthesis, expanded privilege/persistence checks, tokenizer discovery.
- `v1.7.1` Dependency Bumps (completed): toml 1.1, thiserror 2.0, sha2 0.11, CI actions v6–v8.
- `v1.8.0` Live Evaluation Fixes (completed): KV-cache attention mask fix, ReAct garbage fallback, quantization-aware param estimation, severity recalibration, findings detail, template/tool fixes, EP reporting, syslog-analysis template, enumerate_ssh_keys tool.

## Immediate Next Steps

Use this runbook to execute the active next milestone end-to-end.

1. Create a tracking issue from the Release Checklist template.
2. Apply labels `release` and the target milestone label.
3. Verify quality gates locally:
   - `cargo check`
   - `cargo test --workspace`
   - `cargo clippy --all-targets -- -D warnings`
   - `cargo check -p inference_bridge --features vitis`
4. Verify GitHub Actions CI is green on latest `main`.
5. Tag and publish:
   - `git tag -a vX.Y.Z -m "Release vX.Y.Z"`
   - `git push origin vX.Y.Z`
6. Confirm `Release` workflow completed and assets are attached.
7. Close the milestone and open a follow-on milestone.
8. Open planning issue for the next milestone scope.

## Labels and Milestones

- Source of truth for labels: `.github/labels.yml`
- Label sync workflow: `.github/workflows/labels.yml`
- Milestone creation workflow: `.github/workflows/milestones.yml`

If labels drift, run the `Labels` workflow manually.
