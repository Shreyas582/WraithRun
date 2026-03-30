---
name: Release Checklist
about: Track readiness and publication tasks for a versioned release
title: "release: vX.Y.Z checklist"
labels: ["release", "chore"]
assignees: []
---

## Release Metadata

- Version: `vX.Y.Z`
- Target date:
- Release lead:

## Scope and Risk

- [ ] Scope is frozen and documented.
- [ ] High-risk items are explicitly called out.
- [ ] Open known issues are documented in release notes.

## Quality Gates

- [ ] CI on `main` is green.
- [ ] `cargo check` passes locally.
- [ ] `cargo test --workspace` passes locally.
- [ ] `cargo check -p inference_bridge --features vitis` passes.

## Security and Dependencies

- [ ] Scheduled or manual security audit has no untriaged critical findings.
- [ ] Dependency review concerns are resolved or accepted with rationale.

## Notes and Changelog

- [ ] `CHANGELOG.md` is updated.
- [ ] Release Drafter output reviewed for correctness.
- [ ] Breaking changes (if any) are clearly highlighted.

## Tag and Publish

- [ ] Tag is created from latest `main`: `vX.Y.Z`
- [ ] Tag pushed: `git push origin vX.Y.Z`
- [ ] Release workflow completed successfully.
- [ ] Linux, macOS, and Windows artifacts are present.

## Post-Release Validation

- [ ] Download and smoke-test at least one published artifact.
- [ ] Follow-up issues for deferred work created.
- [ ] Milestone closed and next milestone opened.
