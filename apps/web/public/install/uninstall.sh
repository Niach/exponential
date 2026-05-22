#!/usr/bin/env bash
set -euo pipefail

INSTALL_DIR="${EXPONENTIAL_COMPANION_DIR:-$HOME/.local/share/exponential-companion/source}"

if [[ "$(uname -s)" != "Linux" ]]; then
  echo "The Exponential companion installer currently supports Linux only." >&2
  exit 1
fi

KEEP_STATE=""
KEEP_AGENT=""
REMOVE_SOURCE="1"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --keep-state)
      KEEP_STATE="--keep-state"
      shift
      ;;
    --keep-agent)
      KEEP_AGENT="--keep-agent"
      shift
      ;;
    --keep-source)
      REMOVE_SOURCE=""
      shift
      ;;
    *)
      echo "Unknown argument: $1" >&2
      exit 1
      ;;
  esac
done

if [[ ! -d "$INSTALL_DIR" ]]; then
  echo "Companion source not found at $INSTALL_DIR — nothing to uninstall."
  exit 0
fi

if ! command -v bun >/dev/null 2>&1; then
  echo "bun is not on PATH; cannot run the companion CLI to uninstall cleanly." >&2
  exit 1
fi

echo "Running companion uninstall…"
bun "$INSTALL_DIR/apps/companion/src/cli.ts" uninstall ${KEEP_STATE} ${KEEP_AGENT}

if [[ -n "$REMOVE_SOURCE" ]]; then
  echo "Removing source checkout at $INSTALL_DIR"
  rm -rf "$INSTALL_DIR"
fi

echo "Done."
