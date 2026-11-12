#!/usr/bin/env bash
# Installs hetzner-1's disk keeper (disk-keeper.sh) as a root systemd timer
# (every 15 minutes) and caps journald at 200 MB. Idempotent; rerun to
# upgrade. Copy both files to the box and run it there:
#   scp .github/hetzner/{disk-keeper,install-disk-keeper}.sh root@<hetzner-1>:/tmp/
#   ssh root@<hetzner-1> bash /tmp/install-disk-keeper.sh               # install or upgrade
#   ssh root@<hetzner-1> bash /tmp/install-disk-keeper.sh --uninstall   # remove the timer
set -euo pipefail

UNIT=exp-disk-keeper
DEST=/usr/local/lib/exponential/disk-keeper.sh

systemctl disable --now "$UNIT.timer" >/dev/null 2>&1 || true
if [ "${1:-}" = "--uninstall" ]; then
  rm -f "/etc/systemd/system/$UNIT.service" "/etc/systemd/system/$UNIT.timer" "$DEST"
  systemctl daemon-reload
  echo "disk-keeper: uninstalled (the journald cap stays)"
  exit 0
fi

install -D -m 755 "$(dirname "$0")/disk-keeper.sh" "$DEST"

cat > "/etc/systemd/system/$UNIT.service" <<UNITFILE
[Unit]
Description=Exponential disk keeper (.github/hetzner/disk-keeper.sh)
After=docker.service

[Service]
Type=oneshot
ExecStart=/bin/bash $DEST
Nice=10
IOSchedulingClass=idle
UNITFILE

cat > "/etc/systemd/system/$UNIT.timer" <<UNITFILE
[Unit]
Description=Run the Exponential disk keeper every 15 minutes

[Timer]
OnBootSec=5min
OnUnitActiveSec=15min
AccuracySec=1min

[Install]
WantedBy=timers.target
UNITFILE

mkdir -p /etc/systemd/journald.conf.d
cat > /etc/systemd/journald.conf.d/exp-size.conf <<'CONF'
# #ERR-15: the journal never takes more than 200 MB of hetzner-1's 75 GB.
[Journal]
SystemMaxUse=200M
CONF
systemctl restart systemd-journald

systemctl daemon-reload
systemctl enable --now "$UNIT.timer"
echo "disk-keeper: installed ($UNIT.timer, every 15 minutes; journald capped at 200M); log: journalctl -t exp-disk-keeper"
