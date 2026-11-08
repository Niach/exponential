#!/usr/bin/env bash
# The Mint's weekly Docker prune (.github/runners/README.md): images and build
# cache nothing used for a week. Never volumes: the runners' caches and every
# local stack's data live in them. Installed as a user crontab line:
#   ( crontab -l 2>/dev/null; echo "17 4 * * 0 $HOME/exp-runners/prune.sh >> $HOME/exp-runners/prune.log 2>&1" ) | crontab -
set -euo pipefail
echo "== $(date -Is)"
docker image prune --all --force --filter until=168h | tail -1
docker builder prune --all --force --filter until=168h | tail -1
df -h /var/lib/docker | tail -1
