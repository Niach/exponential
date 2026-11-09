#!/bin/bash
# usage: eui-tag.sh <version> <PR>: once "(#PR)" is on master, dispatch the full release suite
# there; tag eui-v<version> at that commit only if every `exponential-ui / *` leg is green,
# else print the red legs' error lines. PR_MAC_LEGS=off while the suite runs (the Macs).
. "$(dirname "$0")/env.sh"; V=$1; P=$2; cd "$REPO"
until git fetch -q origin 2>/dev/null; git log origin/master --oneline --grep="(#$P)" -1 | grep -q .; do sleep 60; done
git ls-remote --exit-code origin refs/tags/eui-v$V >/dev/null 2>&1 && { echo "eui-v$V already tagged"; exit 0; }
M=$(git rev-parse origin/master); echo "#$P on master; dispatching at $M"
gh variable set PR_MAC_LEGS --body off >/dev/null 2>&1
gh workflow run release-suite.yml --ref master >/dev/null 2>&1; sleep 40
while :; do sid=$(gh run list --workflow release-suite.yml -L 10 --json databaseId,event,displayTitle -q '[.[]|select(.event=="workflow_dispatch" and .displayTitle=="release-suite")][0].databaseId'); h=$(gh run view $sid --json headSha -q .headSha 2>/dev/null); [ -n "$h" ] && git merge-base --is-ancestor $M $h 2>/dev/null && break; sleep 30; git fetch -q origin 2>/dev/null; done; M=$h; echo "suite $sid at $M"
until [[ $(gh run view $sid --json status -q .status) == completed ]]; do sleep 300; done
gh variable delete PR_MAC_LEGS >/dev/null 2>&1 && echo "PR_MAC_LEGS unset"
red=$(gh run view $sid --json jobs -q '[.jobs[]|select(.name|startswith("exponential-ui / "))|select(.conclusion!="success" and .conclusion!="skipped")|.name+"="+.conclusion]|join(", ")'); echo "eUI reds: ${red:-none}"
repo=$(gh repo view --json nameWithOwner -q .nameWithOwner)
for n in $(gh run view $sid --json jobs -q '.jobs[]|select(.name|startswith("exponential-ui / "))|select(.conclusion=="failure")|.databaseId'); do echo "--- job $n"; gh api repos/$repo/actions/jobs/$n/logs 2>/dev/null | grep -E "##\[error\]|OVER BUDGET|> budget|< budget|panicked|\(fail\)" | grep -v "36;1m" | head -8 | cut -c30-260; done
[ -z "$red" ] || exit 1
git tag -f eui-v$V $M >/dev/null; git push -q origin refs/tags/eui-v$V && echo "pushed eui-v$V at $M"
