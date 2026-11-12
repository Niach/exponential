#!/usr/bin/env bash
# Installs the Mint's disk keeper (disk-keeper.sh) as a systemd USER timer that
# runs every 15 minutes. Idempotent; rerun to upgrade. The Mint only.
#   bash .github/runners/linux/install-disk-keeper.sh              # install or upgrade
#   bash .github/runners/linux/install-disk-keeper.sh --uninstall  # remove the timer
# The script is copied out of the checkout, so deleting a worktree never breaks
# the job. Picks up CARGO_TARGET_DIR (the shared target) from the installing shell.
# Runs while Danny is logged in; `loginctl enable-linger` would keep it running
# logged out (not needed on a desktop that is always logged in).
set -euo pipefail

UNIT=exp-disk-keeper
DEST="${XDG_DATA_HOME:-$HOME/.local/share}/exponential-disk-keeper"
UNITS="${XDG_CONFIG_HOME:-$HOME/.config}/systemd/user"

systemctl --user disable --now "$UNIT.timer" >/dev/null 2>&1 || true
if [ "${1:-}" = "--uninstall" ]; then
  rm -f "$UNITS/$UNIT.service" "$UNITS/$UNIT.timer"
  rm -rf "$DEST"
  systemctl --user daemon-reload
  echo "disk-keeper: uninstalled"
  exit 0
fi

mkdir -p "$DEST" "$UNITS"
install -m 755 "$(dirname "$0")/disk-keeper.sh" "$DEST/disk-keeper.sh"

cat > "$UNITS/$UNIT.service" <<UNITFILE
[Unit]
Description=Exponential disk keeper (stale build output, .github/runners/linux/disk-keeper.sh)

[Service]
Type=oneshot
ExecStart=/bin/bash $DEST/disk-keeper.sh
Environment=CARGO_TARGET_DIR=${CARGO_TARGET_DIR:-}
Nice=10
IOSchedulingClass=idle
UNITFILE
# An empty CARGO_TARGET_DIR= would override the script's default.
[ -n "${CARGO_TARGET_DIR:-}" ] || sed -i '/^Environment=CARGO_TARGET_DIR=$/d' "$UNITS/$UNIT.service"

cat > "$UNITS/$UNIT.timer" <<UNITFILE
[Unit]
Description=Run the Exponential disk keeper every 15 minutes

[Timer]
OnBootSec=5min
OnUnitActiveSec=15min
AccuracySec=1min

[Install]
WantedBy=timers.target
UNITFILE

systemctl --user daemon-reload
systemctl --user enable --now "$UNIT.timer"
echo "disk-keeper: installed ($UNIT.timer, every 15 minutes); log: ${XDG_STATE_HOME:-$HOME/.local/state}/exponential/disk-keeper.log"
