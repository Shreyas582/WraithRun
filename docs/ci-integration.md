# Integrating WraithRun in CI/CD

Run automated security investigations on every push, pull request, or schedule.

## GitHub Actions

Use the official WraithRun Action:

```yaml
- name: Run WraithRun scan
  uses: Shreyas582/wraithrun-action@v1
  with:
    task: 'Triage this host for persistence mechanisms'
    format: json
    max-steps: 10
    fail-on-severity: high
```

### Action inputs

| Input             | Required | Default  | Description                                                |
|-------------------|----------|----------|------------------------------------------------------------|
| `version`         | no       | `latest` | WraithRun version to install                               |
| `task`            | **yes**  | —        | Investigation task description                             |
| `profile`         | no       | —        | Named configuration profile                                |
| `max-steps`       | no       | `10`     | Maximum agent investigation steps                          |
| `format`          | no       | `json`   | Output format: `json`, `summary`, `markdown`, `narrative`  |
| `fail-on-severity`| no       | `none`   | Fail threshold: `none`, `info`, `low`, `medium`, `high`, `critical` |
| `extra-args`      | no       | —        | Additional CLI arguments                                   |

### Action outputs

| Output          | Description                             |
|-----------------|-----------------------------------------|
| `report-path`   | Path to the generated report file       |
| `finding-count` | Total number of findings                |
| `max-severity`  | Highest finding severity (or `"none"`)  |
| `exit-code`     | WraithRun process exit code             |

### Full workflow example

See [`.github/workflows/wraithrun-scan.example.yml`](https://github.com/Shreyas582/WraithRun/blob/main/.github/workflows/wraithrun-scan.example.yml) for a complete example with artifact upload, step summary, and scheduled nightly scans.

## GitLab CI

Include the template or copy it into your `.gitlab-ci.yml`:

```yaml
include:
  - remote: https://raw.githubusercontent.com/Shreyas582/WraithRun/main/ci-templates/gitlab-ci.yml
```

Override variables to customize:

```yaml
wraithrun-scan:
  variables:
    WRAITHRUN_TASK: "Check for unauthorized SSH keys"
    WRAITHRUN_FAIL_SEVERITY: "medium"
```

## Jenkins / CircleCI / Generic

Use the shell script in your pipeline:

```bash
export WRAITHRUN_TASK="Investigate host for persistence"
export WRAITHRUN_FAIL_SEVERITY="high"
bash ci-templates/wraithrun-scan.sh
```

Or install directly:

```bash
curl -sSL https://github.com/Shreyas582/WraithRun/releases/download/v1.2.0/wraithrun-1.2.0-x86_64-unknown-linux-gnu.tar.gz | tar -xz -C /usr/local/bin
wraithrun --task "Investigate host" --format json --exit-policy severity-threshold --exit-threshold high
```

## Exit code policy

WraithRun supports exit code policies for CI gate decisions:

| Flag               | Values                               | Description                              |
|--------------------|--------------------------------------|------------------------------------------|
| `--exit-policy`    | `none`, `severity-threshold`         | When to use a non-zero exit code         |
| `--exit-threshold` | `info`, `low`, `medium`, `high`, `critical` | Minimum severity to trigger failure |

When `--exit-policy severity-threshold` is set and any finding meets or exceeds the threshold, WraithRun exits with code 1. This maps to a failed step in all CI systems.

## Output formats

| Format      | Best for                        |
|-------------|---------------------------------|
| `json`      | Machine parsing, dashboards     |
| `summary`   | Quick terminal overview         |
| `markdown`  | PR comments, documentation      |
| `narrative` | Executive/stakeholder reporting |

The `json` format follows the schema in [`docs/schemas/run-report.schema.json`](schemas/run-report.schema.json). See [Automation Contracts](automation-contracts.md) for full contract details.

## Scheduled scanning

### Nightly host triage

```yaml
on:
  schedule:
    - cron: '0 2 * * *' # 02:00 UTC daily
```

### Weekly persistence check

```yaml
on:
  schedule:
    - cron: '0 6 * * 1' # 06:00 UTC every Monday
```

## Interpreting results

1. **Check exit code** — non-zero means findings exceeded your threshold.
2. **Parse JSON report** — `findings` array contains all discovered issues.
3. **Review severity** — each finding has a `severity` field: `critical`, `high`, `medium`, `low`, `info`.
4. **Check confidence** — `confidence_label` indicates how certain the tool is.
5. **Follow evidence** — each finding includes an `evidence` field linking to tool observations.

## Tips

- Start with `fail-on-severity: critical` and lower the threshold as you remediate findings.
- Use `--profile` to run pre-configured investigation templates.
- Upload reports as artifacts for audit trail.
- Post `--format markdown` output as PR comments for visibility.
