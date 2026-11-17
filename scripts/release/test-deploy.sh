#!/bin/bash
# #EXP-1838: webdeploy.sh / shares.sh / deploy.sh against stubbed ssh, coolify and curl (no
# network, no host): a shares timeout or psql failure is "could not run" (exit 2), never
# duplicates; staging must run <sha> and be healthy before prod is touched (even its tag);
# Coolify "finished" on the old image = REVISION MISMATCH (exit 3). Run by test-tagkeep.sh (CI).
set -u; R=$(cd "$(dirname "$0")" && pwd); fail=0
pass() { echo "PASS $1"; }; bad() { echo "FAIL $1"; echo "$2" | sed 's/^/    /'; fail=1; }
REAL_SLEEP=$(command -v sleep); TMPS=(); trap 'rm -rf "${TMPS[@]}"' EXIT
setup() { ST=$(mktemp -d); TMPS+=("$ST"); mkdir -p "$ST/bin" "$ST/state"
  cat >"$ST/release.env" <<EOF
EXP_SSH=deploy@stubhost
EXP_APP_PROD=prodapp1-uuid
EXP_APP_STAGING=stagapp1-uuid
EXP_DB_PROD=proddb01-uuid
EXP_DB_STAGING=stagdb01-uuid
EXP_PROD_URL=https://prod.test
EXP_STAGING_URL=https://staging.test
EXP_STATE=$ST/state
EOF
  for a in prodapp1-uuid stagapp1-uuid; do echo sha-aaaaaaa >"$ST/tag_$a"; echo sha-aaaaaaa >"$ST/run_$a"; done
  printf '#!/bin/bash\nexit 0\n' >"$ST/bin/sleep"
  cat >"$ST/bin/coolify" <<'EOF'
#!/bin/bash
S=$STUB; echo "coolify $*" >>$S/calls
case "$1 $2" in
  "deploy uuid") grep -qx "$3" $S/stuck 2>/dev/null || cp $S/tag_$3 $S/run_$3; echo queued;;
  "app update") echo "$5" >$S/tag_$3;;
  "app get") echo "{\"docker_registry_image_tag\":\"$(cat $S/tag_$3)\",\"status\":\"running:healthy\"}";;
  "app deployments") st=finished; grep -qx "$4" $S/deployfail 2>/dev/null && st=failed; echo "[{\"status\":\"$st\"}]";;
esac
EOF
  cat >"$ST/bin/ssh" <<'EOF'
#!/bin/bash
S=$STUB; cmd="${@: -1}"; echo "ssh $cmd" >>$S/calls
p=$(sed -nE 's/.*grep \^([A-Za-z0-9-]{1,8}).*/\1/p' <<<"$cmd" | head -1)
case "$cmd" in
  *psql*) cat >/dev/null; m=$(cat $S/shares_$p 2>/dev/null || echo clean)
    case $m in clean) exit 0;; dup) echo 'dev-1|team-1|acme|[{"id":"a"},{"id":"b"}]'; exit 0;;
      hang) exec "$REAL_SLEEP" 30;; fail) echo 'psql: error: connection refused' >&2; exit 1;; esac;;
  *image.revision*\$c) f=$(ls $S/run_$p* | head -1); t=$(cat $f); echo "${t#sha-}0123456789abcdef0123456789abcdef0";;
  *image.revision*) :;;
  *"docker manifest"*) exit 0;;
  *"grep -ciE"*) cat $S/errlines 2>/dev/null || echo 0;;
  *"docker logs"*) echo "Listening on: http://localhost:3000/";;
  *"image prune"*) echo "Total reclaimed space: 0B";;
  *"docker ps"*) echo "$p-20261116T000000";;
esac
EOF
  cat >"$ST/bin/curl" <<'EOF'
#!/bin/bash
u="${@: -1}"; for h in $(cat $STUB/unhealthy 2>/dev/null); do [[ $u == *$h* ]] && { echo '{"ok":false,"db":false}'; exit 0; }; done
echo '{"ok":true,"db":true,"electric":true}'
EOF
  chmod +x "$ST/bin/"*; }
run() { PATH="$ST/bin:$PATH" STUB=$ST REAL_SLEEP=$REAL_SLEEP EXP_RELEASE_ENV=$ST/release.env SHARES_TIMEOUT=${SHARES_TIMEOUT:-2} "$@" 2>&1; }
deployed() { grep -q "^coolify deploy uuid $1" "$ST/calls" 2>/dev/null; }
tagged() { grep -q "^coolify app update $1 --docker-tag sha-bbbbbbb" "$ST/calls" 2>/dev/null; }

# 1. the happy path: both verified on the new revision, tags settled
setup; out=$(run "$R/webdeploy.sh" bbbbbbb); rc=$?
[[ $rc == 0 && $out == *"prod runs bbbbbbb"* && $out == *"staging runs bbbbbbb"* && $out == *"DEPLOYED sha-bbbbbbb"* ]] \
  && pass "deploy: staging then prod, both verified on the new revision" || bad "happy path rc=$rc" "$out"
grep -n "deploy uuid" "$ST/calls" | head -1 | grep -q stagapp1 && pass "deploy: staging goes first" || bad "staging first" "$(cat $ST/calls)"

# 2. a shares timeout is "could not run", never duplicates; nothing is deployed
setup; echo hang >"$ST/shares_proddb01"; out=$(run "$R/shares.sh"); rc=$?
[[ $rc == 2 && $out == *"shares prod: CHECK COULD NOT RUN (timed out"* && $out != *DUPLICATE* ]] \
  && pass "shares: a timeout reports 'check could not run' (exit 2)" || bad "shares timeout rc=$rc" "$out"
out=$(run "$R/webdeploy.sh" bbbbbbb); rc=$?
[[ $rc == 2 && $out == *"SHARES CHECK COULD NOT RUN"* && $out != *"duplicates found"* ]] && ! deployed prodapp1 && ! deployed stagapp1 \
  && pass "webdeploy: a shares timeout holds the deploy as 'could not run', not as duplicates" || bad "webdeploy timeout rc=$rc" "$out"

# 3. a psql failure (exit 1 on the host) is "could not run" too, not duplicates
setup; echo fail >"$ST/shares_stagdb01"; out=$(run "$R/shares.sh"); rc=$?
[[ $rc == 2 && $out == *"shares staging: CHECK COULD NOT RUN (exit 1: psql: error"* && $out == *"shares prod: clean"* ]] \
  && pass "shares: a failing psql is 'check could not run', not a result" || bad "shares psql fail rc=$rc" "$out"

# 4. real duplicates stop the deploy as duplicates
setup; echo dup >"$ST/shares_proddb01"; out=$(run "$R/webdeploy.sh" bbbbbbb); rc=$?
[[ $rc == 1 && $out == *"shares prod: DUPLICATES FOUND, 1"* && $out == *"SHARES FAILED: duplicates found"* ]] && ! deployed stagapp1 \
  && pass "webdeploy: duplicates stop it before any deploy" || bad "duplicates rc=$rc" "$out"

# 5. Coolify says finished but staging still runs the old image: prod untouched, even its tag
setup; echo stagapp1-uuid >"$ST/stuck"; out=$(run "$R/webdeploy.sh" bbbbbbb); rc=$?
[[ $rc == 3 && $out == *"REVISION MISMATCH on staging"* ]] && ! deployed prodapp1 && ! tagged prodapp1 \
  && pass "webdeploy: staging on the old revision holds prod (and its tag)" || bad "staging stuck rc=$rc" "$out"$'\n'"$(cat $ST/calls)"

# 6. staging unhealthy on the new revision: prod untouched
setup; echo staging >"$ST/unhealthy"; out=$(run "$R/webdeploy.sh" bbbbbbb); rc=$?
[[ $rc == 1 && $out == *"staging NOT HEALTHY"* ]] && ! deployed prodapp1 && ! tagged prodapp1 \
  && pass "webdeploy: an unhealthy staging holds prod" || bad "staging unhealthy rc=$rc" "$out"

# 7. staging's Coolify deployment failed: prod untouched
setup; echo stagapp1-uuid >"$ST/deployfail"; out=$(run "$R/webdeploy.sh" bbbbbbb); rc=$?
[[ $rc == 1 && $out == *"DEPLOYMENT failed"* && $out == *"STAGING DEPLOY FAILED"* ]] && ! deployed prodapp1 \
  && pass "webdeploy: a failed staging deployment holds prod" || bad "staging deployfail rc=$rc" "$out"

# 8. prod "finished" on the old image: loud failure, tags not settled to latest
setup; echo prodapp1-uuid >"$ST/stuck"; out=$(run "$R/webdeploy.sh" bbbbbbb); rc=$?
[[ $rc == 3 && $out == *"REVISION MISMATCH on prod"* && $out == *"DEPLOY NOT DONE"* && $out != *DEPLOYED* ]] \
  && pass "webdeploy: prod on the old revision after 'finished' fails loudly" || bad "prod stuck rc=$rc" "$out"

# 9. boot errors on staging hold prod
setup; echo 3 >"$ST/errlines"; out=$(run "$R/webdeploy.sh" bbbbbbb); rc=$?
[[ $rc == 1 && $out == *"STAGING BOOT NOT CLEAN (3"* ]] && ! deployed prodapp1 \
  && pass "webdeploy: staging boot errors hold prod" || bad "boot errors rc=$rc" "$out"

# 10. the query run on the host is shares.sql
setup; run "$R/shares.sh" >/dev/null; grep -q "docker exec -i .*psql" "$ST/calls" && ! grep -q "bun run" "$ST/calls" \
  && pass "shares: runs psql in the DB container on the host, not bun on this machine" || bad "shares host-side" "$(cat $ST/calls)"
exit $fail
