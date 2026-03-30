# CLI Reference

Command name:

- wraithrun

Basic usage:

```text
wraithrun [OPTIONS] --task <TASK>
```

## Options

- `--task <TASK>`: required investigation prompt.
- `--model <MODEL>`: model path for live mode. Default: `./models/llm.onnx`.
- `--tokenizer <TOKENIZER>`: tokenizer path used in live mode.
- `--max-steps <MAX_STEPS>`: max agent iterations. Default: `8`.
- `--max-new-tokens <MAX_NEW_TOKENS>`: generation cap per model response. Default: `256`.
- `--temperature <TEMPERATURE>`: generation temperature. Default: `0.2`.
- `--live`: enable ONNX/Vitis live inference mode.
- `--format <FORMAT>`: output format. Values: `json`, `summary`, `markdown`. Default: `json`.
- `--output-file <OUTPUT_FILE>`: write rendered output to file.
- `--quiet`: suppress runtime logs.
- `--verbose`: enable debug runtime logs.
- `--vitis-config <VITIS_CONFIG>`: Vitis provider config file path.
- `--vitis-cache-dir <VITIS_CACHE_DIR>`: Vitis cache directory.
- `--vitis-cache-key <VITIS_CACHE_KEY>`: Vitis cache key.
- `-h, --help`: print help.

## Examples

Dry-run mode:

```powershell
wraithrun --task "Check suspicious listener ports"
```

Live mode:

```powershell
wraithrun --live --model C:/models/llm.onnx --tokenizer C:/models/tokenizer.json --task "Investigate unauthorized SSH keys"
```

Summary output with file export:

```powershell
wraithrun --task "Check suspicious listener ports" --format summary --output-file .\launch-assets\network-summary.txt
```
