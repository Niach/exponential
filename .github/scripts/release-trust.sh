#!/usr/bin/env bash
# #EXP-2151: the release trust boundary (workflows/README.md), as a gate.
# Release workflows never reach a self-hosted runner or a shared cache, and
# every job that reads a release secret runs in the `release` (or `train`)
# environment, which only tags and master may use.
set -euo pipefail
cd "$(dirname "$0")/../workflows"
bad=0
fail() { echo "::error::$1"; bad=1; }
releases=(build-android.yml build-cli.yml build-desktop.yml build-ios.yml build-ede.yml desktop-build.yml glitchtip-symbols.yml release-e*.yml)
for f in "${releases[@]}"; do
  if grep -nE 'pick-runner|self-hosted|exp-(linux|mac)|sccache|RUSTC_WRAPPER' <(sed 's/#.*//' "$f"); then
    fail "$f: a release workflow runs GitHub-hosted only"
  fi
  # A cache step is fine only when a pull request alone runs it.
  awk -v f="$f" '
    /^      - / { if (hit && !pr) { print f ": " hit; e = 1 } hit = ""; pr = 0 }
    /uses: (actions\/cache|Swatinem\/rust-cache|gradle\/actions)|^ +cache: / { hit = $0 }
    /if: github.event_name == .pull_request./ { pr = 1 }
    /uses: \.\/\.github\/actions\/cargo-fetch/ { hit = $0 }
    /restore: (.false.|\$\{\{ github.event_name == .pull_request. \}\})|dir: ..$/ { pr = 1 }
    END { if (hit && !pr) { print f ": " hit; e = 1 } exit e }' "$f" || fail "$f reads a shared cache"
done
for f in *.yml; do
  awk -v f="$f" '
    function check() { if (secret && !env) { print f ": job " job " reads " secret " outside an environment"; e = 1 } }
    /^jobs:/ { jobs = 1; next }
    !jobs { next }
    /^  [A-Za-z0-9_-]+:$/ { check(); job = $1; secret = ""; env = 0 }
    /^    environment: / { env = 1 }
    { line = $0; gsub(/secrets\.(RUNNER_STATUS_TOKEN|GITHUB_TOKEN)/, "", line); if (match(line, /secrets\.[A-Z_]+/)) secret = substr(line, RSTART, RLENGTH) }
    END { check(); exit e }' "$f" || fail "$f: a release secret without its environment"
  # `inherit` hands a called workflow every secret: build-desktop's legs only.
  if [ "$f" != build-desktop.yml ] && grep -qE '^ +secrets: inherit' "$f"; then fail "$f: pass RUNNER_STATUS_TOKEN, never secrets: inherit"; fi
done
[ "$bad" = 0 ] && echo "release trust boundary: ok"
exit "$bad"
