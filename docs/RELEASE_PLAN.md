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

- `v0.2.0`
- `v0.2.1`

## Cadence

- Patch releases: as needed for critical fixes.
- Minor releases: approximately every 2 to 6 weeks, depending on feature readiness.

## Release Checklist

1. Confirm all required CI jobs are green on `main`.
2. Ensure high-priority security findings are triaged.
3. Update `CHANGELOG.md` and verify release notes labels on merged PRs.
4. Validate core commands locally:
   - `cargo check`
   - `cargo test -p core_engine`
   - `cargo check -p inference_bridge --features vitis`
5. Create and push tag: `git tag vX.Y.Z` then `git push origin vX.Y.Z`.
6. Wait for release workflow completion.
7. Verify generated GitHub release assets and notes.
8. Post-release sanity check by running released CLI binary.

## Go/No-Go Criteria

Release can proceed when:

- CI passes on latest `main` commit.
- No unresolved critical vulnerabilities in direct dependencies.
- Release notes and changelog are coherent and accurate.

Release should be blocked when:

- Regressions are detected in core runtime loop behavior.
- Security policy checks fail for tooling or dependency chain.
- Release artifacts are missing or corrupted.

## Post-Release Actions

- Open follow-up issues for deferred work.
- Refresh roadmap in `README.md` if priorities changed.
- Optionally create next milestone tracking issue.
