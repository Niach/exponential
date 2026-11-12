#!/bin/bash
# hetzner-1's disk keeper (#ERR-15): a root systemd timer (every 15 minutes,
# install-disk-keeper.sh) that caps what can grow without anyone noticing.
# 2026-11-12: the 75 GB disk hit 100% from GlitchTip debug-file uploads and
# broke the package repo publish; prod web + DB live on the same disk.
#
#   disk-keeper.sh            cap the build cache, drop dangling images; first aid at DISK_KEEPER_EMERGENCY_PCT
#   disk-keeper.sh --dry-run  print the usage and what first aid would run, delete nothing
#
# Every run (regenerable only, never volumes, never containers):
#   1. Docker build cache above DISK_KEEPER_BUILD_CACHE (5GB)
#   2. dangling images
# At DISK_KEEPER_EMERGENCY_PCT (90) used, first aid, before the box fills:
#   3. GlitchTip's unreferenced upload chunks now (the hourly Coolify task's job)
#   4. every image no container uses that is older than a day (Coolify's
#      rollback images included: losing a rollback beats a full prod disk)
#   5. journald down to 100 MB
# The ALERT is Coolify's (server disk-usage notification at 80%, checked every
# 10 minutes, mailed to the team); this logs a warning to the journal at 80%
# (`journalctl -t exp-disk-keeper`).
set -uo pipefail

BUILD_CACHE="${DISK_KEEPER_BUILD_CACHE:-5GB}"
WARN_PCT="${DISK_KEEPER_WARN_PCT:-80}"
EMERGENCY_PCT="${DISK_KEEPER_EMERGENCY_PCT:-90}"
GLITCHTIP_WORKER="${DISK_KEEPER_GLITCHTIP_WORKER:-worker-ngxw7jcbbzgooaq225d1ffqt}"
dry_run=""
[ "${1:-}" = "--dry-run" ] && dry_run=1

log() { [ -n "$dry_run" ] || logger -t exp-disk-keeper -p "user.${2:-info}" "$1"; echo "$1"; }
used_pct() { df --output=pcent / | tail -1 | tr -dc '0-9'; }
run() { if [ -n "$dry_run" ]; then echo "would run: $*"; else "$@" >/dev/null 2>&1 || log "failed: $*" warning; fi; }

before=$(used_pct)
run docker builder prune -f --keep-storage "$BUILD_CACHE"
run docker image prune -f
if [ "$before" -ge "$EMERGENCY_PCT" ]; then
  log "disk ${before}% used (>= ${EMERGENCY_PCT}%): first aid" warning
  run docker exec "$GLITCHTIP_WORKER" python /ops/glitchtip_maintenance.py chunks
  run docker image prune -af --filter until=24h
  run journalctl --vacuum-size=100M
fi
after=$(used_pct)
if [ "$after" -ge "$WARN_PCT" ]; then
  log "disk ${after}% used (was ${before}%): above ${WARN_PCT}%, see .github/hetzner/README.md" warning
else
  log "disk ${after}% used (was ${before}%)"
fi
