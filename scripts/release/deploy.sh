#!/bin/bash
# usage: deploy.sh <coolify-app-uuid> <https-url>: deploy, wait, print tag/status, revision + health.
# Exits 1 unless Coolify's deployment ends `finished`. "finished" alone is not proof the new image
# runs (#EXP-1838): callers that deploy an image check the revision (verify.sh require_rev).
. "$(dirname "$0")/env.sh"; need EXP_SSH
u=$1; url=$2; cd /tmp
coolify deploy uuid $u 2>&1 | tail -1
sleep 10
for i in $(seq 1 80); do
  s=$(coolify app deployments list $u --format json 2>/dev/null | python3 -c 'import json,sys;d=json.load(sys.stdin);print(d[0]["status"])')
  [ "$s" != queued ] && [ "$s" != in_progress ] && break; sleep 15
done
echo "deployment: ${s:-unknown}"; sleep 20
coolify app get $u --format json 2>/dev/null | python3 -c 'import json,sys;d=json.load(sys.stdin);print(d["docker_registry_image_tag"],d["status"])'
on_host "for c in \$(docker ps --format '{{.Names}}' | grep ^${u:0:8}); do echo \$c \$(docker inspect -f '{{index .Config.Labels \"org.opencontainers.image.revision\"}} {{.State.Health.Status}} {{.Config.Image}}' \$c); done"
curl -s -m 20 $url/api/health; echo
[ "$s" = finished ] || { echo "DEPLOYMENT ${s:-unknown} (not finished)"; exit 1; }
