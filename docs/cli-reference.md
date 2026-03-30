# CLI Reference

Command name:

- wraithrun

Basic usage:

```text
wraithrun [OPTIONS] --task <TASK>
wraithrun --doctor [OPTIONS]
wraithrun --list-profiles [OPTIONS]
wraithrun --print-effective-config [OPTIONS]
wraithrun --init-config [--init-config-path <PATH>] [--force]
```

## Options

- `--task <TASK>`: required investigation prompt.
- `--doctor`: run configuration/runtime diagnostics and exit.
- `--list-profiles`: list built-in and config-defined profiles, then exit.
- `--print-effective-config`: print resolved runtime settings as JSON and exit.
- `--init-config`: write a starter TOML config file and exit.
- `--init-config-path <INIT_CONFIG_PATH>`: output path for `--init-config`. Default: `./wraithrun.toml`.
- `--force`: allow overwrite of an existing file when `--init-config` is used.
- `--config <CONFIG>`: explicit TOML config file path.
- `--profile <PROFILE>`: named profile from built-ins or config file.
- `--model <MODEL>`: model path for live mode. Default fallback: `./models/llm.onnx`.
- `--tokenizer <TOKENIZER>`: tokenizer path used in live mode.
- `--max-steps <MAX_STEPS>`: max agent iterations. Default fallback: `8`.
- `--max-new-tokens <MAX_NEW_TOKENS>`: generation cap per model response. Default fallback: `256`.
- `--temperature <TEMPERATURE>`: generation temperature. Default fallback: `0.2`.
- `--live`: enable ONNX/Vitis live inference mode.
- `--dry-run`: force dry-run mode.
- `--format <FORMAT>`: output format. Values: `json`, `summary`, `markdown`. Default: `json`.
- `--output-file <OUTPUT_FILE>`: write rendered output to file.
- `--quiet`: suppress runtime logs.
- `--verbose`: enable debug runtime logs.
- `--vitis-config <VITIS_CONFIG>`: Vitis provider config file path.
- `--vitis-cache-dir <VITIS_CACHE_DIR>`: Vitis cache directory.
- `--vitis-cache-key <VITIS_CACHE_KEY>`: Vitis cache key.
- `-h, --help`: print help.

## Resolution Order

Settings are resolved in this order:

1. CLI flags
2. Environment variables
3. Config file (`--config`, `WRAITHRUN_CONFIG`, or `./wraithrun.toml` if present)
4. Built-in defaults

When a profile is selected, built-in profile defaults apply before config file values.

## Doctor Mode

Run `--doctor` to validate:

- profile selection and availability,
- config file discovery/parsing,
- environment variable parsing,
- final effective runtime resolution,
- live-mode file-path readiness checks.

Behavior:

- Exit code `0`: no failures (warnings may still be present).
- Non-zero exit code: one or more failures.

## Introspection Modes

`--list-profiles` output includes:

- built-in profile names and purpose,
- config file path detection status,
- config-defined profile names,
- selected profile source (built-in/config/missing) when `--profile` is set.

`--print-effective-config` output includes the final merged runtime settings after applying precedence rules.

`--init-config` output includes the target path and suggested follow-up commands.

These modes are mutually exclusive with each other and with `--doctor`.

## Built-In Profiles

- `local-lab`: dry-run, compact step/token budget, summary output.
- `production-triage`: dry-run, deeper loop budget, markdown output.
- `live-model`: enables live inference and a larger token budget.

## Examples

Dry-run mode:

```powershell
wraithrun --task "Check suspicious listener ports"
```

Use built-in profile:

```powershell
wraithrun --task "Check suspicious listener ports" --profile local-lab
```

Use config + profile:

```powershell
wraithrun --task "Investigate unauthorized SSH keys" --config .\wraithrun.example.toml --profile production-triage
```

Run doctor:

```powershell
wraithrun --doctor
```

Run doctor for specific profile/config combination:

```powershell
wraithrun --doctor --config .\wraithrun.example.toml --profile live-model
```

List profiles:

```powershell
wraithrun --list-profiles --config .\wraithrun.example.toml
```

Print effective config:

```powershell
wraithrun --print-effective-config --profile production-triage --config .\wraithrun.example.toml
```

Initialize config at default path:

```powershell
wraithrun --init-config
```

Initialize config at custom path and overwrite existing file:

```powershell
wraithrun --init-config --init-config-path .\configs\team.toml --force
```

Live mode:

```powershell
wraithrun --live --model C:/models/llm.onnx --tokenizer C:/models/tokenizer.json --task "Investigate unauthorized SSH keys"
```

Summary output with file export:

```powershell
wraithrun --task "Check suspicious listener ports" --format summary --output-file .\launch-assets\network-summary.txt
```

## Environment Variables

Runtime control variables:

- `WRAITHRUN_CONFIG`
- `WRAITHRUN_PROFILE`
- `WRAITHRUN_MODEL`
- `WRAITHRUN_TOKENIZER`
- `WRAITHRUN_MAX_STEPS`
- `WRAITHRUN_MAX_NEW_TOKENS`
- `WRAITHRUN_TEMPERATURE`
- `WRAITHRUN_LIVE`
- `WRAITHRUN_FORMAT`
- `WRAITHRUN_OUTPUT_FILE`
- `WRAITHRUN_LOG` (`quiet`, `normal`, `verbose`)
- `WRAITHRUN_QUIET` (`true/false`, optional legacy override)
- `WRAITHRUN_VERBOSE` (`true/false`, optional legacy override)
- `WRAITHRUN_VITIS_CONFIG`
- `WRAITHRUN_VITIS_CACHE_DIR`
- `WRAITHRUN_VITIS_CACHE_KEY`

Example:

```powershell
$env:WRAITHRUN_PROFILE = "production-triage"
$env:WRAITHRUN_FORMAT = "summary"
wraithrun --task "Check suspicious listener ports" --format json
```

In the example above, CLI `--format json` wins over `WRAITHRUN_FORMAT=summary`.

## Config File Schema

Config file format is TOML. Top-level keys mirror runtime options; profiles are nested under `[profiles.<name>]`.

```toml
model = "./models/llm.onnx"
max_steps = 8
max_new_tokens = 256
temperature = 0.2
format = "json"
log = "normal"

[profiles.local-lab]
max_steps = 6
format = "summary"
live = false

[profiles.production-triage]
max_steps = 12
format = "markdown"
live = false

[profiles.live-model]
live = true
format = "json"
```

Template file included in repository root:

- `wraithrun.example.toml`
