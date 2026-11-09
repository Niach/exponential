#!/bin/bash
# usage: land-deploy.sh <PR> <migrations 0|1> <relay 0|1>
# Waits for "(#PR)" on master (git fetch, no API), then deploys web staging-first
# (with a migrations-only release-suite run first when <migrations>=1), then the push
# relay when asked. One deploy at a time ($EXP_STATE/deploy.lock); skipped when prod
# already runs a commit containing the PR. WAIT_EXACT=1 deploys the PR's own image only.
. "$(dirname "$0")/env.sh"; . "$REL/gh-lib.sh"; need EXP_SSH EXP_APP_PROD
P=$1; MIG=$2; REL_=$3; cd "$REPO"
until git fetch -q origin master 2>/dev/null; M=$(git log origin/master --format=%H --grep="(#$P)" -1) && [ -n "$M" ]; do sleep 60; done
echo "#$P landed at ${M:0:9}"; exec 9>"$EXP_STATE/deploy.lock"; flock 9; echo "lock held"
cur=$(on_host "docker inspect -f '{{index .Config.Labels \"org.opencontainers.image.revision\"}}' \$(docker ps --format '{{.Names}}' | grep ^${EXP_APP_PROD:0:8} | head -1)"); git fetch -q origin master 2>/dev/null
if [ -n "$cur" ] && git merge-base --is-ancestor $M $cur 2>/dev/null; then echo "prod web ${cur:0:9} already has #$P"; MIG=skip; fi
if [ "$MIG" = skip ]; then :
elif [ "$MIG" = 1 ]; then gh workflow run release-suite.yml --ref master -f only=migrations && echo "dispatched only=migrations"; "$REL/hotfix-web.sh" || { echo "hotfix web FAILED"; exit 1; }
else wait_image $M || exit 1
  "$REL/webdeploy.sh" $IMG || { echo "web deploy FAILED"; exit 1; }; fi
echo "web live"
if [ "$REL_" = 1 ]; then need EXP_APP_PUSH_RELAY; timeout 1500 "$REL/deploy.sh" $EXP_APP_PUSH_RELAY $EXP_PUSH_URL 2>&1 | grep -E "^deployment:"; echo "relay healthz: $(curl -s -o /dev/null -w %{http_code} $EXP_PUSH_URL/healthz)"; fi
