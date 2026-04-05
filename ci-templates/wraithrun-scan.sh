#!/usr/bin/env bash
# WraithRun CI Scanner — generic shell script for Jenkins, CircleCI, etc.
#
# Usage:
#   WRAITHRUN_TASK="Investigate host" ./ci-templates/wraithrun-scan.sh
#
# Environment variables:
#   WRAITHRUN_VERSION       Version to install (default: latest)
#   WRAITHRUN_TASK          Investigation task (required)
#   WRAITHRUN_FORMAT        Output format: json|summary|markdown|narrative (default: json)
#   WRAITHRUN_MAX_STEPS     Max investigation steps (default: 10)
#   WRAITHRUN_FAIL_SEVERITY Fail threshold: none|info|low|medium|high|critical (default: none)
#   WRAITHRUN_EXTRA_ARGS    Additional CLI arguments
#   WRAITHRUN_REPORT_PATH   Output report path (default: ./wraithrun-report.json)

set -euo pipefail

: "${WRAITHRUN_TASK:?WRAITHRUN_TASK is required}"
: "${WRAITHRUN_VERSION:=latest}"
: "${WRAITHRUN_FORMAT:=json}"
: "${WRAITHRUN_MAX_STEPS:=10}"
: "${WRAITHRUN_FAIL_SEVERITY:=none}"
: "${WRAITHRUN_EXTRA_ARGS:=}"
: "${WRAITHRUN_REPORT_PATH:=./wraithrun-report.json}"

INSTALL_DIR="${HOME}/.wraithrun-bin"

# --- Install WraithRun ---------------------------------------------------

install_wraithrun() {
  local version="$1"
  if [ "${version}" = "latest" ]; then
    version=$(curl -sS https://api.github.com/repos/Shreyas582/WraithRun/releases/latest \
      | grep '"tag_name"' | head -1 | sed 's/.*"v\(.*\)".*/\1/')
  fi
  echo "Installing WraithRun v${version}..."

  mkdir -p "${INSTALL_DIR}"
  local os
  os=$(uname -s)
  case "${os}" in
    Linux)  asset="wraithrun-${version}-x86_64-unknown-linux-gnu.tar.gz" ;;
    Darwin) asset="wraithrun-${version}-x86_64-apple-darwin.tar.gz" ;;
    *)      echo "Unsupported OS: ${os}"; exit 1 ;;
  esac

  curl -sSL "https://github.com/Shreyas582/WraithRun/releases/download/v${version}/${asset}" \
    -o /tmp/wraithrun.tar.gz
  tar -xzf /tmp/wraithrun.tar.gz -C "${INSTALL_DIR}"
  rm -f /tmp/wraithrun.tar.gz
  export PATH="${INSTALL_DIR}:${PATH}"
  echo "Installed: $(wraithrun --version)"
}

# --- Run Scan -------------------------------------------------------------

if ! command -v wraithrun &>/dev/null; then
  install_wraithrun "${WRAITHRUN_VERSION}"
fi

ARGS=(--task "${WRAITHRUN_TASK}" --format "${WRAITHRUN_FORMAT}" --max-steps "${WRAITHRUN_MAX_STEPS}")

if [ "${WRAITHRUN_FAIL_SEVERITY}" != "none" ]; then
  ARGS+=(--exit-policy severity-threshold --exit-threshold "${WRAITHRUN_FAIL_SEVERITY}")
fi

# shellcheck disable=SC2206
if [ -n "${WRAITHRUN_EXTRA_ARGS}" ]; then
  ARGS+=(${WRAITHRUN_EXTRA_ARGS})
fi

EXIT_CODE=0
wraithrun "${ARGS[@]}" > "${WRAITHRUN_REPORT_PATH}" 2>&1 || EXIT_CODE=$?

echo ""
echo "=== WraithRun Scan Complete ==="
echo "Report: ${WRAITHRUN_REPORT_PATH}"
echo "Exit code: ${EXIT_CODE}"

if [ "${EXIT_CODE}" -ne 0 ] && [ "${WRAITHRUN_FAIL_SEVERITY}" != "none" ]; then
  echo "FAILED: Findings at or above '${WRAITHRUN_FAIL_SEVERITY}' severity detected."
  exit "${EXIT_CODE}"
fi

exit 0
