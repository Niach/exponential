# Sourced. Pin handling for the Coolify web apps: a deploy or advert must never move a
# PINNED app (sha-*) onto the floating `latest` (a migration soak keeps prod pinned).
#   prev=$(tag_of <uuid>)            the app's image tag before we touch it
#   settle_tag <uuid> <prev> <now>   after deploying tag <now>:
#       prev == latest  -> back to latest (normal: prod tracks master)
#       prev is a pin   -> stay pinned, on <now> (the image now running)
# DRY_RUN=1: coolify calls are printed, not run; tag_of answers $FAKE_TAG.
if [ -n "$DRY_RUN" ]; then
  coolify() { if [ "$1 $2" = "app get" ]; then echo "{\"docker_registry_image_tag\":\"${FAKE_TAG:-latest}\"}"; else echo "DRY coolify $*" >&2; fi; }
fi
tag_of() { coolify app get "$1" --format json 2>/dev/null | python3 -c 'import json,sys; print(json.load(sys.stdin)["docker_registry_image_tag"])'; }
settle_tag() { local u=$1 prev=$2 now=$3 t; if [ "$prev" = latest ]; then t=latest; else t=$now; fi
  coolify app update "$u" --docker-tag "$t" >/dev/null 2>&1; echo "tag ${u:0:8}: $prev -> $t"; }
