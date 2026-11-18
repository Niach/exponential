#!/bin/bash
# usage: shares.sh: the duplicate device-share check (FEED-149) on prod's and staging's DB, run ON
# the deploy host (`docker exec psql` in the DB container, so mint's load can't starve it; #EXP-1838).
# The query is shares.sql = lib/device-share-duplicates.ts (a web unit test keeps them equal).
#   exit 0 = clean · 1 = DUPLICATES FOUND · 2 = the CHECK COULD NOT RUN (timeout, ssh or psql
#   failure): never a result, never read as duplicates. SHARES_TIMEOUT (s, default 90) bounds each DB.
. "$(dirname "$0")/env.sh"; need EXP_SSH EXP_DB_PROD EXP_DB_STAGING
T=${SHARES_TIMEOUT:-90}; dup=0; norun=0; err=$(mktemp); trap 'rm -f "$err"' EXIT
for pair in "$EXP_DB_PROD:prod" "$EXP_DB_STAGING:staging"; do
  IFS=: read db name <<<"$pair"
  out=$(timeout "$T" ssh -o BatchMode=yes -o ConnectTimeout=20 "$EXP_SSH" \
    "c=\$(docker ps --format '{{.Names}}' | grep ^${db:0:8} | head -1); [ -n \"\$c\" ] || { echo 'no running DB container' >&2; exit 3; }; docker exec -i \$c sh -c 'psql -X -q -At -v ON_ERROR_STOP=1 -U \"\$POSTGRES_USER\" -d \"\${POSTGRES_DB:-\$POSTGRES_USER}\"'" \
    <"$REL/shares.sql" 2>"$err"); rc=$?
  if [ $rc -eq 124 ]; then echo "shares $name: CHECK COULD NOT RUN (timed out after ${T}s)"; norun=1
  elif [ $rc -ne 0 ]; then echo "shares $name: CHECK COULD NOT RUN (exit $rc: $(tail -1 "$err"))"; norun=1
  else n=$(grep -c . <<<"$out")
    if [ "$n" = 0 ]; then echo "shares $name: clean, 0 duplicate (machine, team) sharing pairs"
    else echo "shares $name: DUPLICATES FOUND, $n (machine, team) pair(s) shared twice:"; head -20 <<<"$out" | cut -c1-400; dup=1; fi
  fi
done
[ $dup = 1 ] && exit 1; [ $norun = 1 ] && exit 2; exit 0
