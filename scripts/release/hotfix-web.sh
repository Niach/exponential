#!/bin/bash
# usage: hotfix-web.sh: after `gh workflow run release-suite.yml -f only=migrations`, wait for its
# migrations job, then the image, then webdeploy.sh (staging first). Polls gh every 5 min.
. "$(dirname "$0")/env.sh"; . "$REL/gh-lib.sh"; cd "$REPO"
sleep 40; sid=$(gh run list --workflow release-suite.yml -L 5 --json databaseId,event,displayTitle -q '[.[]|select(.event=="workflow_dispatch" and (.displayTitle|contains("only migrations")))][0].databaseId')
M=$(gh run view $sid --json headSha -q .headSha); echo "suite $sid at $M"
until [[ $(gh run view $sid --json status -q .status) == completed ]]; do sleep 300; done
m=$(gh run view $sid --json jobs -q '.jobs[]|select(.name=="migrations")|.conclusion'); echo "migrations $m"; [[ $m == success ]] || exit 1
wait_image $M || exit 1
"$REL/webdeploy.sh" $IMG
