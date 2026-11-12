#!/usr/bin/env bash
# The Mint's weekly Docker prune (.github/runners/README.md): images nothing
# used for a week, build cache down to 20 GB (2026-11-12: `--filter until=168h`
# reclaimed 0 B of 107 GB months-old cache; the disk keeper also caps it). Never volumes: the runners' caches and every
# local stack's data live in them. Installed as a user crontab line:
#   ( crontab -l 2>/dev/null; echo "17 4 * * 0 $HOME/exp-runners/prune.sh >> $HOME/exp-runners/prune.log 2>&1" ) | crontab -
set -euo pipefail
echo "== $(date -Is)"
docker image prune --all --force --filter until=168h | tail -1
docker builder prune --all --force --max-used-space 20GB | tail -1
df -h /var/lib/docker | tail -1
