#!/bin/sh
# Exponential CLI installer (EXP-403) — served at https://exponential.at/install.sh
#
#   curl -fsSL https://exponential.at/install.sh | sh
#   curl -fsSL https://exponential.at/install.sh | EXP_INSTANCE=https://issues.example.com sh
#
# ONE script for cloud and self-hosted: a self-hosted deployment ships only
# the web app (no marketing pages), so the script always lives on the cloud
# site and the target instance rides EXP_INSTANCE. EXP-1111: it installs the
# latest `cli-v*` release binary to ~/.local/bin/exponential
# (checksum-verified), turns CLI auto-update on, signs in (a terminal or
# not: the device code is printed and approved in any browser), then
# installs the daemon as a login/boot service so this machine shows up as a
# device, and ends with a summary.
#
# Environment:
#   EXP_INSTANCE        the instance URL (default: the cloud)
#   EXP_INSTALL_TOKEN   one-time expi_... token from the web's Add device
#                       dialog: signs in without any approval step
#   EXP_TOKEN           an existing expu_... API key (or session token)
#   EXP_DEVICE_LABEL    the name this machine gets in the Devices list
#   EXP_NO_DAEMON=1     skip the daemon service
#   EXP_NO_AUTOUPDATE=1 leave auto-update off
#   EXP_INSTALL_DIR     where the binary goes (default ~/.local/bin)

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
      x86_64)
        # A Rosetta-translated shell on an M-series Mac reports x86_64; macOS
        # runs arm64 binaries regardless of the invoking shell's translation
        # state, so only a genuine Intel Mac is unsupported.
        if [ "$(sysctl -n sysctl.proc_translated 2>/dev/null)" = "1" ]; then
          target="aarch64-apple-darwin"
        else
          fail "unsupported macOS architecture: $arch (Apple Silicon only)"
        fi
        ;;
      *) fail "unsupported macOS architecture: $arch (Apple Silicon only)" ;;
    esac
    ;;
  *) fail "unsupported OS: $os (Linux and macOS only)" ;;
esac

# --- Preflight: warn, don't block (the CLI's doctor owns enforcement) --------
command -v git >/dev/null 2>&1 || warn "git is not installed — coding sessions need it"
found_agent=""
for agent in claude codex; do
  if command -v "$agent" >/dev/null 2>&1; then
    found_agent="$agent"
    break
  fi
done
[ -n "$found_agent" ] || warn "no agent CLI found (claude or codex) — install one to run coding sessions"

# --- Resolve the latest cli-v* release ---------------------------------------
# per_page=100 (the GitHub max): the list is shared with the desktop/android/
# ios release trains, which must not bury the newest cli-v* entry.
say "Looking up the latest CLI release..."
# Capture curl separately so an unreachable/rate-limited API (403 from a
# shared-IP network is common unauthenticated) is not reported as "no release".
releases=$(curl -fsSL "https://api.github.com/repos/$REPO/releases?per_page=100") ||
  fail "could not reach api.github.com (rate limited or offline?) — try again later"
tag=$(printf '%s\n' "$releases" |
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

# --- Auto-update (default on) ----------------------------------------------
# Set BEFORE the first command that would otherwise ask about it. A CLI
# without `--auto` must not abort the install (`set -eu`): warn and go on.
bin="$INSTALL_DIR/$BIN_NAME"
if [ "${EXP_NO_AUTOUPDATE:-}" = "1" ]; then
  autoupdate="off"
else
  autoupdate="on"
fi
if ! "$bin" update --auto "$autoupdate" >/dev/null 2>&1; then
  warn "could not turn auto-update $autoupdate — later: $bin update --auto $autoupdate"
  autoupdate="not set"
fi

# --- Sign in ---------------------------------------------------------------
# EXP_INSTANCE / EXP_INSTALL_TOKEN / EXP_TOKEN flow through the environment
# (never argv, so no token shows up in `ps`). Under `curl | sh` stdin is the
# script pipe, so an interactive login attaches to the controlling terminal;
# with none (CI, cloud-init, a provisioning run) the login still happens:
# it prints the device code and waits for the approval. The tty probe
# actually OPENS /dev/tty — a permission test alone passes in CI/cron where
# the open would fail with "no such device or address".
has_tty=0
( : < /dev/tty ) 2>/dev/null && has_tty=1

# A re-run keeps an existing sign-in — for the SAME instance only. `whoami`
# prints one "<email> on <instance url>" line per account, the URL normalized
# like `exponential login` does it (no trailing slash, https:// when
# schemeless); unset EXP_INSTANCE = the cloud.
signed_in=0
instance="${EXP_INSTANCE:-https://app.exponential.at}"
while [ "${instance%/}" != "$instance" ]; do instance="${instance%/}"; done
case "$instance" in
  http://*|https://*) ;;
  *) instance="https://$instance" ;;
esac
if [ -z "${EXP_INSTALL_TOKEN:-}" ] && [ -z "${EXP_TOKEN:-}" ]; then
  if who=$("$bin" whoami 2>/dev/null); then
    while IFS= read -r line; do
      case "$line" in
        *" on $instance") signed_in=1 ;;
      esac
    done <<WHOAMI
$who
WHOAMI
  fi
fi

if [ "$signed_in" = "1" ]; then
  say "Already signed in."
elif [ -n "${EXP_INSTALL_TOKEN:-}" ] || [ -n "${EXP_TOKEN:-}" ]; then
  "$bin" login </dev/null || fail "sign-in failed"
elif [ "$has_tty" = "1" ]; then
  "$bin" login </dev/tty || fail "sign-in failed — rerun: $bin login"
else
  say "No terminal attached: approve this device code from any browser."
  "$bin" login --no-browser </dev/null || fail "sign-in failed — rerun: $bin login"
fi

# --- Daemon (default on) ---------------------------------------------------
# The service registers this machine as a device (remote starts from the
# web, mobile and other CLIs) and starts at login/boot; on Linux it also
# enables lingering, or prints the one sudo line that does.
if [ "${EXP_NO_DAEMON:-}" = "1" ]; then
  daemon="skipped (EXP_NO_DAEMON=1; run \`$bin daemon install\` later)"
else
  if [ -n "${EXP_DEVICE_LABEL:-}" ]; then
    "$bin" daemon install --label "$EXP_DEVICE_LABEL" </dev/null || warn "daemon install failed"
  else
    "$bin" daemon install </dev/null || warn "daemon install failed"
  fi
  if "$bin" daemon status >/dev/null 2>&1; then
    daemon="running"
    # Its first register lands a moment after the process is up.
    tries=0
    while [ "$tries" -lt 10 ] && ! "$bin" devices 2>/dev/null | grep -q '(this machine)'; do
      sleep 1
      tries=$((tries + 1))
    done
  else
    daemon="NOT running (see \`$bin daemon status\`)"
  fi
fi

# --- Summary ---------------------------------------------------------------
account=$("$bin" whoami 2>/dev/null | head -n 1) || account=""
device=$("$bin" status 2>/dev/null | sed -n 's/^Device    //p' | head -n 1) || device=""
say ""
say "Exponential CLI ${tag#cli-v} is set up."
say "  Account:      ${account:-not signed in}"
say "  Device:       ${device:-unknown}"
say "  Daemon:       $daemon"
say "  Auto-update:  $autoupdate"
say ""
say "Next: \`$BIN_NAME doctor\` checks the agent CLIs; \`$BIN_NAME devices\` lists your machines."
