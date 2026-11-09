# Sourced by every release script. Loads the operator's LOCAL config (never committed:
# hosts, Coolify app ids and DB container ids stay out of this public repo) and the
# helpers the scripts share. Config: $EXP_RELEASE_ENV, default ~/.config/exponential/release.env
# (see README.md for the keys). DRY_RUN=1 skips the config (tests).
REL=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
REPO=$(git -C "$REL" rev-parse --show-toplevel)
CONF=${EXP_RELEASE_ENV:-$HOME/.config/exponential/release.env}
if [ -z "$DRY_RUN" ]; then [ -f "$CONF" ] || { echo "release: no config at $CONF (see scripts/release/README.md)" >&2; exit 2; }; . "$CONF"; fi
: "${EXP_PROD_URL:=https://app.exponential.at}" "${EXP_STAGING_URL:=https://next.exponential.at}" "${EXP_PUSH_URL:=https://push.exponential.at}"
: "${EXP_WEB_IMAGE:=ghcr.io/niach/exponential-web}" "${EXP_STATE:=$HOME/.local/state/exponential-release}"
mkdir -p "$EXP_STATE"
need() { local v; for v; do [ -n "${!v}" ] || { echo "release: $v unset in $CONF" >&2; exit 2; }; done; }
# on the deploy host: the newest running container of a Coolify app / DB (its name starts with the id)
ctr() { ssh -o BatchMode=yes "$EXP_SSH" "docker ps --format '{{.Names}}' | grep ^${1:0:8} | head -1"; }
on_host() { ssh -o BatchMode=yes "$EXP_SSH" "$@"; }
