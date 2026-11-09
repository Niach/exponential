# Sourced. GitHub's API quota is shared with every session on the same login: no tight
# `gh` polling. Landing = `git fetch`; images = the registry, read on the deploy host.
# wait_image <full sha>: wait until a web image exists for <sha>, or (unless WAIT_EXACT=1)
# for a newer master commit containing it (build-web cancels superseded runs).
# Sets IMG=<7-char sha> of the image to deploy.
wait_image() { local i c
  for i in $(seq 1 90); do git -C "$REPO" fetch -q origin master 2>/dev/null
    for c in $( [ -n "$WAIT_EXACT" ] && echo $1 || git -C "$REPO" log origin/master --format=%H -10 $1^..origin/master 2>/dev/null); do
      git -C "$REPO" merge-base --is-ancestor $1 $c || continue
      on_host "docker manifest inspect $EXP_WEB_IMAGE:sha-${c:0:7} >/dev/null 2>&1" && { IMG=${c:0:7}; echo "image sha-$IMG ready (for ${1:0:7})"; return 0; }; done
    sleep 60; done; echo "no image for ${1:0:7} FAILED"; return 1; }
