#!/usr/bin/env bash
# Installs the MacBook's disk keeper (disk-keeper.sh) as a LaunchAgent that runs
# every 15 minutes. Idempotent; rerun to upgrade. The MacBook only, never the server.
#   bash .github/runners/mac/install-disk-keeper.sh              # install or upgrade
#   bash .github/runners/mac/install-disk-keeper.sh --uninstall  # remove the job
# The script is copied out of the checkout, so deleting a worktree never breaks the job.
# Picks up CARGO_TARGET_DIR (the shared target) from the installing shell.
set -euo pipefail

LABEL=at.exponential.disk-keeper
DEST="$HOME/Library/Application Support/exponential-disk-keeper"
PLIST="$HOME/Library/LaunchAgents/$LABEL.plist"
DOMAIN="gui/$(id -u)"

launchctl bootout "$DOMAIN/$LABEL" >/dev/null 2>&1 || true
if [ "${1:-}" = "--uninstall" ]; then
  rm -f "$PLIST"
  rm -rf "$DEST"
  echo "disk-keeper: uninstalled"
  exit 0
fi

mkdir -p "$DEST" "$HOME/Library/LaunchAgents" "$HOME/Library/Logs/exponential"
install -m 755 "$(dirname "$0")/disk-keeper.sh" "$DEST/disk-keeper.sh"

cat > "$PLIST" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key><string>$LABEL</string>
  <key>ProgramArguments</key>
  <array><string>/bin/bash</string><string>$DEST/disk-keeper.sh</string></array>
  <key>StartInterval</key><integer>900</integer>
  <key>RunAtLoad</key><true/>
  <key>ProcessType</key><string>Background</string>
  <key>Nice</key><integer>10</integer>
  <key>EnvironmentVariables</key>
  <dict>
    <key>PATH</key><string>/opt/homebrew/bin:/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin</string>
    <key>CARGO_TARGET_DIR</key><string>${CARGO_TARGET_DIR:-}</string>
  </dict>
  <key>StandardOutPath</key><string>$HOME/Library/Logs/exponential/disk-keeper.stdout.log</string>
  <key>StandardErrorPath</key><string>$HOME/Library/Logs/exponential/disk-keeper.stdout.log</string>
</dict>
</plist>
PLIST
plutil -lint "$PLIST" >/dev/null
launchctl bootstrap "$DOMAIN" "$PLIST"
echo "disk-keeper: installed ($LABEL, every 15 minutes); log: ~/Library/Logs/exponential/disk-keeper.log"
