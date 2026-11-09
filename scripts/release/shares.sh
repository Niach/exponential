#!/bin/bash
# usage: shares.sh: check:device-shares (FEED-149) on prod, then staging, through ssh tunnels
. "$(dirname "$0")/env.sh"; need EXP_SSH EXP_APP_PROD EXP_APP_STAGING EXP_DB_PROD EXP_DB_STAGING
cd /tmp
for pair in "$EXP_APP_PROD:$EXP_DB_PROD:55461:prod" "$EXP_APP_STAGING:$EXP_DB_STAGING:55462:staging"; do
  IFS=: read u db port name <<<"$pair"
  # a DB container's IP changes when it restarts: look it up every time
  ip=$(on_host "docker inspect -f '{{range .NetworkSettings.Networks}}{{.IPAddress}}{{end}}' \$(docker ps --format '{{.Names}}' | grep ^${db:0:8} | head -1)")
  url=$(coolify app env list $u --show-sensitive --format json 2>/dev/null | python3 -c '
import json,sys
for e in json.load(sys.stdin):
  if e["key"]=="DATABASE_URL" and not e.get("is_preview"): print(e["value"])')
  url=$(echo "$url" | sed -E "s#@[^:/]+:5432#@127.0.0.1:$port#")
  ssh -o BatchMode=yes -o ExitOnForwardFailure=yes -o ControlMaster=no -f -N -L $port:$ip:5432 "$EXP_SSH"
  sleep 1
  echo "== $name"; (cd "$REPO/apps/web" && DATABASE_URL="$url" timeout 60 nice -n 19 bun run check:device-shares 2>&1 | tail -3; echo "exit=${PIPESTATUS[0]}")
  pkill -f "L $port:$ip:5432" 2>/dev/null
done
