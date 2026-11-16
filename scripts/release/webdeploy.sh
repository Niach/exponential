#!/bin/bash
# usage: webdeploy.sh <7-char sha>: device-shares, prune, STAGING (deploy, revision, health, boot log),
# then PROD (pinned only now), revision on BOTH, settle tags. Prod is never touched (not even its
# tag) unless staging verifiably runs <sha> and is healthy (#EXP-1838).
#   STAGING_ONLY=1   stop after staging (a soak); prod's tag is not touched
#   PRE_PROD_HOOK=…  a shell command that must pass before prod
# exit 0 = both run <sha> · 1 = duplicates / staging unhealthy / deploy failed · 2 = shares check
# could not run · 3 = REVISION MISMATCH (Coolify said done, the old image runs)
. "$(dirname "$0")/env.sh"; . "$REL/tagkeep.sh"; . "$REL/verify.sh"; need EXP_SSH EXP_APP_PROD EXP_APP_STAGING
SHA=$1; U=$EXP_APP_PROD; S=$EXP_APP_STAGING
[[ $SHA =~ ^[0-9a-f]{7,40}$ ]] || { echo "usage: webdeploy.sh <sha7>"; exit 1; }
"$REL/shares.sh"; rc=$?
case $rc in 0) ;; 1) echo "SHARES FAILED: duplicates found, nothing deployed"; exit 1;;
  *) echo "SHARES CHECK COULD NOT RUN (exit $rc): no result, nothing deployed; rerun"; exit 2;; esac
cd /tmp; PREV_U=$(tag_of $U); PREV_S=$(tag_of $S); echo "tags before: prod=$PREV_U staging=$PREV_S"
on_host 'docker image prune -af 2>&1 | tail -1'
coolify app update $S --docker-tag sha-$SHA >/dev/null 2>&1
deploy_app $S $EXP_STAGING_URL || { echo "STAGING DEPLOY FAILED: prod held, untouched"; exit 1; }
sleep 90
require_rev $S $SHA staging || { echo "prod held, untouched"; exit 3; }
c=$(ctr $S); echo "--- staging boot log ($c):"
on_host "docker logs --since 10m $c 2>&1 | grep -iE 'error|fatal|migrat|listening|ready' | tail -25; docker inspect -f '{{.State.Health.Status}} restarts={{.RestartCount}}' $c"
require_healthy $EXP_STAGING_URL staging || { echo "prod held, untouched; staging left on sha-$SHA"; exit 1; }
n=$(on_host "docker logs --since 10m $c 2>&1 | grep -ciE '\\b(error|fatal)\\b'")
if [[ ${n:-1} != 0 ]]; then echo "STAGING BOOT NOT CLEAN (${n:-?} error lines): prod held, untouched; staging left on sha-$SHA"; exit 1; fi
if [ -n "$PRE_PROD_HOOK" ]; then echo "--- pre-prod hook"; bash -c "$PRE_PROD_HOOK" || { echo "PRE-PROD HOOK FAILED: prod held"; exit 1; }; fi
if [ -n "$STAGING_ONLY" ]; then echo "STAGING ONLY: staging pinned sha-$SHA, prod untouched ($PREV_U)"; exit 0; fi
coolify app update $U --docker-tag sha-$SHA >/dev/null 2>&1
deploy_app $U $EXP_PROD_URL || { echo "PROD DEPLOY FAILED (prod tag is sha-$SHA, was $PREV_U)"; exit 1; }
bad=0; require_rev $U $SHA prod || bad=3; require_rev $S $SHA staging || bad=3
[ $bad = 0 ] || { echo "DEPLOY NOT DONE: Coolify reported success but the old image runs; tags left as set (prod was $PREV_U)"; exit 3; }
settle_tag $U "$PREV_U" sha-$SHA; settle_tag $S "$PREV_S" sha-$SHA
require_healthy $EXP_PROD_URL prod || exit 1
echo "DEPLOYED sha-$SHA: prod + staging verified"
