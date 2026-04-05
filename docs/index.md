# WraithRun Documentation

WraithRun is a local-first cyber investigation runtime designed for host triage workflows.

Use this documentation to install, run, and operate WraithRun in your own environment.

## Start Here

- [Getting Started](getting-started.md): install and run your first task.
- [Usage Examples](USAGE_EXAMPLES.md): copy-paste commands for common workflows.
- [CLI Reference](cli-reference.md): all command-line options.
- [Tool Reference](tool-reference.md): built-in tool behavior and expected outputs.
- [Security and Sandbox](security-sandbox.md): policy controls and environment variables.
- [CI/CD Integration](ci-integration.md): run WraithRun in GitHub Actions, GitLab CI, Jenkins.
- [Troubleshooting](troubleshooting.md): common errors and fixes.

## Investigation Playbooks

Step-by-step guides for common security investigation scenarios.

- [Investigate Suspicious SSH Keys](playbook-ssh-keys.md)
- [Triage a Compromised Windows Workstation](playbook-windows-triage.md)
- [Audit Privileged Accounts After a Credential Leak](playbook-credential-leak.md)
- [Check for Persistence Mechanisms Post-Breach](playbook-persistence-sweep.md)

## Reference

- [Plugin API](plugin-api.md): extend WraithRun with external tool plugins.
- [MITRE ATT&CK Mapping](mitre-attack-mapping.md): tools mapped to ATT&CK techniques.
- [Threat Model](threat-model.md): WraithRun's attack surface, trust boundaries, and security controls.
- [Sample Report: Linux Persistence](sample-report-linux-persistence.md)
- [Sample Report: Windows Triage](sample-report-windows-triage.md)

## Operations and Releases

- [Release Plan](RELEASE_PLAN.md)
- [CI/CD Overview](CI_CD.md)
- [Upgrade Notes](upgrades.md)

## Source and Releases

- Source repository: https://github.com/Shreyas582/WraithRun
- Releases: https://github.com/Shreyas582/WraithRun/releases
