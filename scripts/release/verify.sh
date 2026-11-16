# Sourced. What a web deploy checks before it believes Coolify (#EXP-1838: Coolify said
# "finished" while the old image kept running).
#   running_rev <uuid>          git revision (image label) of the app's newest running container
#   require_rev <uuid> <sha> <name>   1-line verdict; return 3 on a mismatch (or nothing running)
#   require_healthy <url> <name>      /api/health answers ok; return 1 otherwise
#   deploy_app <uuid> <url>     deploy.sh under a timeout; its exit code, not grep's
running_rev() { on_host "c=\$(docker ps --format '{{.Names}}' | grep ^${1:0:8} | head -1); [ -n \"\$c\" ] && docker inspect -f '{{index .Config.Labels \"org.opencontainers.image.revision\"}}' \$c"; }
require_rev() { local r; r=$(running_rev "$1"); r=${r//[[:space:]]/}
  if [ ${#2} -ge 7 ] && [ "${r:0:${#2}}" = "$2" ]; then echo "$3 runs ${r:0:12}"; return 0; fi
  echo "REVISION MISMATCH on $3: runs '${r:-nothing}', expected $2"; return 3; }
require_healthy() { local h; h=$(curl -s -m 20 "$1/api/health"); echo "$2 health $h"
  [[ $h == *'"ok":true'* ]] || { echo "$2 NOT HEALTHY"; return 1; }; }
deploy_app() { timeout 1300 "$REL/deploy.sh" "$1" "$2" 2>&1 | grep -v "new version\|└\|─"; return "${PIPESTATUS[0]}"; }
