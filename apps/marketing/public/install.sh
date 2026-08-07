#!/bin/sh
# Exponential CLI installer (EXP-403) — served at https://exponential.at/install.sh
#
#   curl -fsSL https://exponential.at/install.sh | sh
#   curl -fsSL https://exponential.at/install.sh | EXP_INSTANCE=https://issues.example.com sh
#
# ONE script for cloud and self-hosted: a self-hosted deployment ships only
# the web app (no marketing pages), so the script always lives on the cloud
# site and the target instance rides EXP_INSTANCE. Installs the latest
# `cli-v*` release binary to ~/.local/bin/exponential (checksum-verified),
# then signs in via the device-code flow unless non-interactive.

set -eu

REPO="Niach/exponential"
INSTALL_DIR="${EXP_INSTALL_DIR:-$HOME/.local/bin}"
BIN_NAME="exponential"

say() { printf '%s\n' "$*"; }
warn() { printf 'warning: %s\n' "$*" >&2; }
fail() { printf 'error: %s\n' "$*" >&2; exit 1; }

command -v curl >/dev/null 2>&1 || fail "curl is required"

# --- Target detection (linux x86_64/aarch64, darwin arm64) -------------------
os=$(uname -s)
arch=$(uname -m)
case "$os" in
  Linux)
    case "$arch" in
      x86_64) target="x86_64-unknown-linux-gnu" ;;
      aarch64 | arm64) target="aarch64-unknown-linux-gnu" ;;
      *) fail "unsupported Linux architecture: $arch" ;;
    esac
    ;;
  Darwin)
    case "$arch" in
      arm64) target="aarch64-apple-darwin" ;;
      *) fail "unsupported macOS architecture: $arch (Apple Silicon only)" ;;
    esac
    ;;
  *) fail "unsupported OS: $os (Linux and macOS only)" ;;
esac

# --- Preflight: warn, don't block (the CLI's doctor owns enforcement) --------
command -v git >/dev/null 2>&1 || warn "git is not installed — coding sessions need it"
found_agent=""
for agent in claude codex pi; do
  if command -v "$agent" >/dev/null 2>&1; then
    found_agent="$agent"
    break
  fi
done
[ -n "$found_agent" ] || warn "no agent CLI found (claude, codex or pi) — install one to run coding sessions"

# --- Resolve the latest cli-v* release ---------------------------------------
# per_page=100 (the GitHub max): the list is shared with the desktop/android/
# ios release trains, which must not bury the newest cli-v* entry.
say "Looking up the latest CLI release..."
tag=$(curl -fsSL "https://api.github.com/repos/$REPO/releases?per_page=100" |
  grep -o '"tag_name": *"cli-v[^"]*"' |
  head -n 1 |
  sed 's/.*"\(cli-v[^"]*\)".*/\1/')
[ -n "$tag" ] || fail "no cli-v* release found on github.com/$REPO"
say "Installing exponential ${tag#cli-v} for $target"

base="https://github.com/$REPO/releases/download/$tag"
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

curl -fSL --progress-bar "$base/$BIN_NAME-$target" -o "$tmp/$BIN_NAME-$target" ||
  fail "download failed: $base/$BIN_NAME-$target"
curl -fsSL "$base/SHA256SUMS.txt" -o "$tmp/SHA256SUMS.txt" ||
  fail "download failed: $base/SHA256SUMS.txt"

# --- Verify ------------------------------------------------------------------
if command -v sha256sum >/dev/null 2>&1; then
  (cd "$tmp" && grep " $BIN_NAME-$target\$" SHA256SUMS.txt | sha256sum -c - >/dev/null) ||
    fail "checksum verification failed"
elif command -v shasum >/dev/null 2>&1; then
  (cd "$tmp" && grep " $BIN_NAME-$target\$" SHA256SUMS.txt | shasum -a 256 -c - >/dev/null) ||
    fail "checksum verification failed"
else
  warn "no sha256 tool found — skipping checksum verification"
fi

# --- Install -----------------------------------------------------------------
mkdir -p "$INSTALL_DIR"
install -m 755 "$tmp/$BIN_NAME-$target" "$INSTALL_DIR/$BIN_NAME"
say "Installed $INSTALL_DIR/$BIN_NAME"

case ":$PATH:" in
  *":$INSTALL_DIR:"*) ;;
  *)
    warn "$INSTALL_DIR is not on your PATH"
    say "  add it with: export PATH=\"$INSTALL_DIR:\$PATH\""
    ;;
esac

# --- Sign in (interactive only) ----------------------------------------------
# EXP_INSTANCE flows through to `exponential login`. Under `curl | sh` stdin
# is the script pipe, so the login attaches to the controlling terminal;
# EXP_TOKEN (an API key from Settings -> API keys) or no controlling
# terminal = scripted setup. The probe actually OPENS /dev/tty — a
# permission test alone passes in CI/cron where the open would fail with
# "no such device or address".
if [ -n "${EXP_TOKEN:-}" ] || ! ( : < /dev/tty ) 2>/dev/null; then
  say "Done. Sign in with: $INSTALL_DIR/$BIN_NAME login"
  [ -n "${EXP_INSTANCE:-}" ] && say "  (EXP_INSTANCE=$EXP_INSTANCE)"
  exit 0
fi
exec "$INSTALL_DIR/$BIN_NAME" login < /dev/tty
