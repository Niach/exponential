#!/bin/bash
# Prod Postgres PITR: weekly WAL-G base backup + prune to the 14-day window. See README.md ("Prod Postgres PITR").
# Lives at /data/walg/pitr-cron.sh on hetzner-1; root crontab: 0 1 * * 0 /data/walg/pitr-cron.sh base
set -euo pipefail
LOG=/var/log/walg-pitr.log; exec >>"$LOG" 2>&1; echo "== $(date -u +%FT%TZ) $1"
PG=$(docker ps --format "{{.Names}}" | grep ^hqc1ofbam3x5kyxjexwj1oio | head -1 || true)
walg() { docker exec -u postgres "$PG" /walg/walg-env "$@"; }
case "$1" in
  base)
    walg backup-push /var/lib/postgresql/data
    walg delete before FIND_FULL "$(date -u -d "14 days ago" +%Y-%m-%dT%H:%M:%SZ)" --confirm
    walg backup-list ;;
  *) echo "usage: $0 base"; exit 2 ;;
esac
echo "== done"
