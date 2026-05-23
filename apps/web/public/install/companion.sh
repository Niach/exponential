#!/usr/bin/env bash
set -euo pipefail

SERVER=""
SETUP_TOKEN=""

while [[ $# -gt 0 ]]; do
  case "$1" in
    --server)
      SERVER="${2:-}"
      shift 2
      ;;
    --setup-token)
      SETUP_TOKEN="${2:-}"
      shift 2
      ;;
    *)
      echo "Unknown argument: $1" >&2
      exit 1
      ;;
  esac
done

if [[ "$(uname -s)" != "Linux" ]]; then
  echo "The Exponential companion installer currently supports Linux only." >&2
  exit 1
fi

CONFIG_PATH="$HOME/.config/exponential-companion/config.toml"
if [[ -f "$CONFIG_PATH" ]]; then
  ALREADY_CONFIGURED="1"
else
  ALREADY_CONFIGURED=""
fi

# Setup token is only required for first-time installs. Re-running the
# script with an existing config just pulls latest source + restarts the
# service (i.e., it acts as an updater).
if [[ -z "$ALREADY_CONFIGURED" && ( -z "$SERVER" || -z "$SETUP_TOKEN" ) ]]; then
  echo "Usage: companion.sh --server https://app.exponential.at --setup-token expc_..." >&2
  exit 1
fi

require_command() {
  if ! command -v "$1" >/dev/null 2>&1; then
    echo "Missing required command: $1" >&2
    exit 1
  fi
}

require_command curl
require_command git
require_command systemctl

if ! command -v bun >/dev/null 2>&1; then
  echo "Installing Bun..."
  curl -fsSL https://bun.sh/install | bash
  export PATH="$HOME/.bun/bin:$PATH"
fi

require_command bun

REPO_URL="${EXPONENTIAL_COMPANION_REPO:-https://github.com/niach/exponential.git}"
INSTALL_DIR="${EXPONENTIAL_COMPANION_DIR:-$HOME/.local/share/exponential-companion/source}"

mkdir -p "$(dirname "$INSTALL_DIR")"

if [[ -d "$INSTALL_DIR/.git" ]]; then
  echo "Updating companion source in $INSTALL_DIR..."
  git -C "$INSTALL_DIR" pull --ff-only
else
  if [[ -e "$INSTALL_DIR" ]]; then
    echo "$INSTALL_DIR exists but is not a git checkout. Move it aside and retry." >&2
    exit 1
  fi
  echo "Cloning companion source into $INSTALL_DIR..."
  git clone --depth 1 "$REPO_URL" "$INSTALL_DIR"
fi

cd "$INSTALL_DIR"

echo "Installing dependencies..."
bun install --frozen-lockfile

if [[ -n "$SETUP_TOKEN" ]]; then
  # If the caller bothered to pass a fresh setup-token, always (re-)claim. The
  # old apikeys row may have been deleted server-side (regenerate / revoke),
  # leaving the local bot.token stale; the only way out is to mint a new key.
  if [[ -z "$SERVER" ]]; then
    echo "Missing --server (required alongside --setup-token)." >&2
    exit 1
  fi
  echo "Configuring companion..."
  bun apps/companion/src/cli.ts setup --server "$SERVER" --setup-token "$SETUP_TOKEN"
else
  echo "No --setup-token provided; keeping existing config at $CONFIG_PATH."
fi

echo "Installing systemd user service..."
bun apps/companion/src/cli.ts install-service
systemctl --user daemon-reload
if [[ -n "$ALREADY_CONFIGURED" ]]; then
  # Force a restart so the freshly-pulled code is what's running.
  systemctl --user restart exponential-companion
else
  systemctl --user enable --now exponential-companion
fi

cat <<EOF

Exponential companion is installed and running.

Status:
  bun $INSTALL_DIR/apps/companion/src/cli.ts status

Logs:
  journalctl --user -u exponential-companion -f

To keep it running after logout:
  sudo loginctl enable-linger $USER
EOF
