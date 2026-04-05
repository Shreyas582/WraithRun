#!/usr/bin/env bash
# Example WraithRun plugin tool.
# Reads JSON from stdin, extracts "target", and returns a JSON greeting.
set -euo pipefail

input=$(cat)
target=$(printf '%s' "$input" | python3 -c "import sys,json; print(json.load(sys.stdin).get('target','world'))" 2>/dev/null || echo "world")

printf '{"greeting":"Hello, %s!","echo":%s}\n' "$target" "$input"
