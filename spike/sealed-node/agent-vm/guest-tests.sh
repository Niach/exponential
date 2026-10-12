#!/bin/bash
# In-guest tests (agent VM). Output goes to the serial console; the host
# collects it as evidence. The guest never holds the real token: claude gets
# a DUMMY OAuth credential, the gateway swaps in the real one.
DUMMY=sk-ant-oat01-DUMMY-guest-credential-not-a-real-token
export ANTHROPIC_BASE_URL=http://127.0.0.1:8080
export CLAUDE_CODE_OAUTH_TOKEN=$DUMMY
export CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC=1
export CLAUDE_CONFIG_DIR=/root/.claude
GW=http://127.0.0.1:8080

section() { echo; echo "=== $* ==="; }
expect() { # <label> <expected-substring> <command...>
  local label=$1 want=$2; shift 2
  local out; out=$("$@" 2>&1 | head -c 400 | tr '\n' ' ')
  if [[ $out == *"$want"* ]]; then echo "PASS  $label"; else echo "FAIL  $label"; fi
  echo "      -> ${out:0:300}"
}

must_fail() { # <label> <command...>: PASS when the command fails
  local label=$1; shift
  local out; out=$("$@" </dev/null 2>&1 | head -c 300 | tr '\n' ' ')
  if "$@" </dev/null >/dev/null 2>&1; then echo "FAIL  $label"; else echo "PASS  $label"; fi
  echo "      -> ${out:0:300}"
}

section "Q3 environment"
echo "kernel: $(uname -r)  mem: $(free -m | awk '/Mem:/{print $2}')MiB  cpus: $(nproc)"
echo "net devices: $(ls /sys/class/net | tr '\n' ' ')"
echo "routes (/proc/net/route):"; cat /proc/net/route
echo "claude $(claude --version 2>&1 | head -1)  git $(git --version | cut -d' ' -f3)"

section "Q3 claude turn with tool calls (bash + git in the guest)"
mkdir -p /root/work && cd /root/work
start=$(cut -d' ' -f1 /proc/uptime)
claude -p "Use the Bash tool to run exactly this one command, then answer with only the number it printed last:
git init -q . && echo hello > a.txt && git add a.txt && git -c user.email=agent@sealed -c user.name=agent commit -qm 'first' && echo SCAN-WINDOW-OPEN > /dev/console && sleep 15 && git log --oneline | wc -l" \
  --allowedTools Bash --output-format stream-json --verbose --include-partial-messages \
  </dev/null > /tmp/claude-turn.jsonl 2> /tmp/claude-turn.err
rc=$?
end=$(cut -d' ' -f1 /proc/uptime)
echo "exit=$rc  wall=$(awk "BEGIN{print $end-$start}")s"
echo "stream events: $(grep -c '"type":"stream_event"' /tmp/claude-turn.jsonl)  deltas: $(grep -c 'content_block_delta' /tmp/claude-turn.jsonl)  tool_use blocks: $(grep -o '"type":"tool_use"' /tmp/claude-turn.jsonl | wc -l)"
grep '"type":"result"' /tmp/claude-turn.jsonl | grep -oE '"(subtype|is_error|num_turns|result|duration_ms)":("[^"]*"|[^,}]*)' | tr '\n' ' '; echo
echo "git log in guest: $(git log --oneline 2>&1)"
head -c 300 /tmp/claude-turn.err

section "Q4a no route out except the gateway"
expect "no NIC besides lo" "lo" ls /sys/class/net
must_fail "direct IP egress fails (1.1.1.1:80)" curl -sS -m 5 http://1.1.1.1/
must_fail "direct IP egress fails (api.anthropic.com IP 160.79.104.10:443)" curl -sS -m 5 -k https://160.79.104.10/
expect "DNS / direct HTTPS fails" "resolve" curl -sS -m 5 https://api.anthropic.com/
must_fail "gateway CID 4 not reachable directly over vsock" socat -T2 - VSOCK-CONNECT:4:8080
must_fail "gateway token port not reachable (CID 4:9000)" socat -T2 - VSOCK-CONNECT:4:9000
must_fail "no host vsock service on 9000" socat -T2 - VSOCK-CONNECT:2:9000
must_fail "no host vsock service on 22" socat -T2 - VSOCK-CONNECT:2:22

section "Q4b the gateway refuses non-allowlisted destinations"
expect "Host: example.com -> 403" "host is not allowlisted" curl -sS -m 10 -H 'Host: example.com' $GW/v1/messages
expect "absolute-form proxy request -> 403" "origin-form" curl -sS -m 10 -x $GW http://example.com/
expect "CONNECT tunnel -> 405" "405" curl -sS -m 10 -x $GW https://example.com/
expect "non-allowlisted path /v1/files -> 403" "path is not allowlisted" curl -sS -m 10 $GW/v1/files
expect "non-allowlisted path /api/oauth/profile -> 403" "path is not allowlisted" curl -sS -m 10 $GW/api/oauth/profile
expect "chunked request body -> 411" "chunked" curl -sS -m 10 -H 'Transfer-Encoding: chunked' --data-binary x $GW/v1/messages
expect "bare CR inside a header value -> 400" "control characters" curl -sS -m 10 -H $'X-Foo: a\rAuthorization: Bearer evil' $GW/v1/messages
expect "allowlisted route still works (count_tokens)" "input_tokens" curl -sS -m 20 -H 'content-type: application/json' -H 'anthropic-version: 2023-06-01' \
  --data '{"model":"claude-haiku-5-5","messages":[{"role":"user","content":"hi"}]}' $GW/v1/messages/count_tokens

section "Q4c the token never appears in the guest"
echo "anthropic-looking credentials in any process environment:"
cat /proc/[0-9]*/environ 2>/dev/null | tr '\0' '\n' | grep -oE 'sk-ant-[A-Za-z0-9_-]+' | sort | uniq -c
echo "anthropic-looking credentials anywhere in the guest filesystem (incl. claude transcripts = every response body)."
echo "positive control: a planted copy of the DUMMY in /tmp/control.txt must show up:"
echo "$DUMMY" > /tmp/control.txt
find / \( -path /proc -o -path /sys -o -path /dev \) -prune -o -type f -print0 2>/dev/null \
  | xargs -0 grep -HoaE 'sk-ant-[A-Za-z0-9_-]{20,}' 2>/dev/null | sort | uniq -c | head
echo "files scanned: $(find / \( -path /proc -o -path /sys -o -path /dev \) -prune -o -type f -print 2>/dev/null | wc -l)"
echo "files under ~/.claude: $(find /root/.claude -type f | wc -l)  bytes: $(du -sb /root/.claude | cut -f1)"
echo "credentials file present: $(ls /root/.claude/.credentials.json 2>/dev/null || echo none)"
