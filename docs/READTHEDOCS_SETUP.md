# Read the Docs Setup

This project is configured for Read the Docs using:

- `.readthedocs.yaml`
- `mkdocs.yml`
- `docs/requirements.txt`

## One-time setup

1. Sign in at https://readthedocs.org/ using your GitHub account.
2. Import repository: `Shreyas582/WraithRun`.
3. Confirm the default branch is `main`.
4. Ensure configuration file detection is enabled (Read the Docs auto-detects `.readthedocs.yaml`).
5. Trigger the first build.

## Versioned docs

Read the Docs can build docs for:

- `latest` from `main`
- tagged releases (for example `v0.2.1`)

After creating a new release tag, enable that version in the Read the Docs admin panel if needed.

## Local docs preview

Install docs dependencies:

```powershell
python -m pip install -r docs/requirements.txt
```

Run local docs server:

```powershell
mkdocs serve
```

Build static docs locally:

```powershell
mkdocs build
```
