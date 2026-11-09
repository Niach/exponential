#!/bin/bash
# usage: webdeploy.sh <7-char sha>: device-shares, prune, pin, STAGING, boot check, then PROD, settle tags
#   STAGING_ONLY=1   stop after staging (a soak); prod's tag is not touched
#   PRE_PROD_HOOK=…  a shell command that must pass before prod
. "$(dirname "$0")/env.sh"; . "$REL/tagkeep.sh"; need EXP_SSH EXP_APP_PROD EXP_APP_STAGING
SHA=$1; U=$EXP_APP_PROD; S=$EXP_APP_STAGING
out=$("$REL/shares.sh" 2>&1); echo "$out" | grep -E "^== |duplicate|exit="
echo "$out" | grep -q "exit=1" && { echo "SHARES FAILED"; exit 1; }
cd /tmp; PREV_U=$(tag_of $U); PREV_S=$(tag_of $S); echo "tags before: prod=$PREV_U staging=$PREV_S"
on_host 'docker image prune -af 2>&1 | tail -1'
for u in $( [ -n "$STAGING_ONLY" ] && echo $S || echo $U $S ); do coolify app update $u --docker-tag sha-$SHA >/dev/null 2>&1; done
timeout 1300 "$REL/deploy.sh" $S $EXP_STAGING_URL 2>&1 | grep -v "new version\|└\|─"
sleep 90
c=$(ctr $S); echo "--- staging boot log ($c):"
on_host "docker logs --since 10m $c 2>&1 | grep -iE 'error|fatal|migrat|listening|ready' | tail -25; docker inspect -f '{{.State.Health.Status}} restarts={{.RestartCount}}' $c"
h=$(curl -s $EXP_STAGING_URL/api/health); echo "staging health $h"
n=$(on_host "docker logs --since 10m $c 2>&1 | grep -ciE '\\b(error|fatal)\\b'")
if [[ $h != *'"ok":true'* || ${n:-1} -gt 0 ]]; then echo "STAGING BOOT NOT CLEAN ($n error lines): prod held; staging left on sha-$SHA"; exit 1; fi
if [ -n "$PRE_PROD_HOOK" ]; then echo "--- pre-prod hook"; bash -c "$PRE_PROD_HOOK" || { echo "PRE-PROD HOOK FAILED: prod held"; exit 1; }; fi
if [ -n "$STAGING_ONLY" ]; then echo "STAGING ONLY: staging pinned sha-$SHA, prod untouched ($PREV_U)"; exit 0; fi
timeout 1300 "$REL/deploy.sh" $U $EXP_PROD_URL 2>&1 | grep -v "new version\|└\|─"
settle_tag $U "$PREV_U" sha-$SHA; settle_tag $S "$PREV_S" sha-$SHA
curl -s $EXP_PROD_URL/api/version; echo
