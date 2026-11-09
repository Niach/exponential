#!/bin/bash
# usage: advert.sh KEY=VER [KEY=VER…]   e.g. CLIENT_LATEST_VERSION_DESKTOP=0.14.144
# Sets prod's client adverts, then redeploys prod's CURRENT image (pinned while it
# redeploys) and settles the tag: a pinned prod stays pinned, `latest` stays latest.
. "$(dirname "$0")/env.sh"; . "$REL/tagkeep.sh"; [ -z "$DRY_RUN" ] && need EXP_SSH EXP_APP_PROD
U=${EXP_APP_PROD:-prod-app}; cd /tmp
prev=$(tag_of $U)
S=$( [ -n "$DRY_RUN" ] && echo "${FAKE_RUNNING:-sha-0000000}" || on_host "docker inspect -f '{{.Config.Image}}' \$(docker ps --format '{{.Names}}' | grep ^${U:0:8} | head -1)" | sed 's/.*://')
echo "prod image $S (tag was $prev)"
for kv in "$@"; do coolify app env update $U ${kv%%=*} --value ${kv#*=} >/dev/null 2>&1; done
if [ -z "$DRY_RUN" ]; then "$REL/shares.sh" 2>&1 | grep -E "duplicate|exit="; fi
coolify app update $U --docker-tag $S >/dev/null 2>&1
[ -z "$DRY_RUN" ] && timeout 1300 "$REL/deploy.sh" $U $EXP_PROD_URL 2>&1 | grep -v "new version\|└\|─"
settle_tag $U "$prev" "$S"
[ -z "$DRY_RUN" ] && curl -s $EXP_PROD_URL/api/version; echo
