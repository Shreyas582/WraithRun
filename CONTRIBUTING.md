# Contributing to WraithRun

Thanks for your interest in contributing.

## Development Setup

1. Install Rust stable toolchain.
2. Clone the repository.
3. Run:

```powershell
cargo check
```

4. Run tests:

```powershell
cargo test -p core_engine
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
- Ensure GitHub Actions checks pass.

## Labels and Release Notes

Use labels to improve release note quality and version planning:

- `feature`: new user-facing capability.
- `enhancement`: extension of an existing capability.
- `fix`: bug fix.
- `bug`: alternative bug-fix label recognized by release drafting.
- `docs`: documentation-only changes.
- `test`: test-only changes.
- `chore`: maintenance with no behavior change.
- `breaking`: backward-incompatible behavior change.
- `ci`: continuous integration changes.
- `dependencies`: changes related to dependencies.
- `release`: changes related to release processes.
- `security`: changes that affect security.
- `priority:p0`: highest priority issues.
- `priority:p1`: medium priority issues.
- `priority:p2`: lower priority issues.
- `milestone:v0.2.1`: milestone for version 0.2.1.

Release notes are assembled automatically from merged pull requests.

## Release Process

1. Keep [CHANGELOG.md](CHANGELOG.md) up to date.
2. Confirm CI is green on `main`.
3. Create an annotated semantic version tag, for example `git tag -a v0.2.1 -m "Release v0.2.1"`.
4. Push the tag to trigger the release workflow.
5. Verify generated release notes and attached binaries.

## Documentation Workflow

WraithRun docs are configured for Read the Docs with MkDocs.

Relevant files:

- `.readthedocs.yaml`
- `mkdocs.yml`
- `docs/requirements.txt`

Local docs preview:

```powershell
python -m pip install -r docs/requirements.txt
mkdocs serve
```

Validation build:

```powershell
mkdocs build --strict
```

## Code Style

- Follow idiomatic Rust style.
- Prefer explicit error handling over panics.
- Keep security-sensitive logic easy to audit.
