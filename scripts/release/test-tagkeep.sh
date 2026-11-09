#!/bin/bash
# Dry-run check (CI: test.yml `release-scripts`): a pinned prod stays pinned through an
# advert, an unpinned prod goes back to `latest`, and a deploy keeps a pin on the new sha.
set -u; R=$(cd "$(dirname "$0")" && pwd); fail=0
pass() { echo "PASS $1"; }; bad() { echo "FAIL $1"; echo "$2"; fail=1; }
out=$(DRY_RUN=1 FAKE_TAG=sha-7166e3f FAKE_RUNNING=sha-7166e3f "$R/advert.sh" CLIENT_LATEST_VERSION_DESKTOP=9.9.9 2>&1)
echo "$out" | grep -q "tag prod-app: sha-7166e3f -> sha-7166e3f" && pass "advert: pinned prod stays pinned" || bad "advert pinned" "$out"
echo "$out" | grep -q -- "--docker-tag latest" && bad "advert: pinned prod was set to latest" "$out"
out=$(DRY_RUN=1 FAKE_TAG=latest FAKE_RUNNING=sha-1234567 "$R/advert.sh" CLIENT_LATEST_VERSION_DESKTOP=9.9.9 2>&1)
echo "$out" | grep -q "tag prod-app: latest -> latest" && pass "advert: unpinned prod back to latest" || bad "advert unpinned" "$out"
out=$(DRY_RUN=1 bash -c ". '$R/tagkeep.sh'; settle_tag stagingapp sha-7166e3f sha-3b3fad2" 2>&1)
echo "$out" | grep -q "tag staginga: sha-7166e3f -> sha-3b3fad2" && pass "deploy: a pin stays a pin, on the deployed sha" || bad "deploy settle" "$out"
for f in "$R"/*.sh; do bash -n "$f" || bad "syntax $f" ""; done; [ $fail = 0 ] && pass "syntax of every script"
exit $fail
