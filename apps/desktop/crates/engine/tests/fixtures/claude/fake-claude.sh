#!/bin/sh
# EXP-746 E2 — a fake `claude` in stream-json mode.
#
# It is deliberately dumb: it replays RECORDED frames (scrubbed captures from
# the phase-1 spike, one directory per scenario) and answers the control
# requests the adapter sends, so `tests/claude_adapter.rs` exercises the real
# adapter against the real wire without a real agent, a network or an account.
#
# The state machine is the protocol's own, in the order a session hits it:
#   control_request initialize  -> the recorded initialize response
#   control_request interrupt   -> a bare success + the turn's recorded trailer
#   control_request <other>     -> a bare success (set_permission_mode, …)
#   user message                -> the next turn's frames (turn1, turn2, …)
#   control_response            -> the frames recorded AFTER that answer
#
# Env:
#   EXP_FAKE_CLAUDE_DIR    the scenario directory (required)
#   EXP_FAKE_CLAUDE_ARGV   file to dump argv + the env facts into (optional)
#   EXP_FAKE_CLAUDE_STDIN  file to append every stdin line to (optional)
set -u

dir="${EXP_FAKE_CLAUDE_DIR:?EXP_FAKE_CLAUDE_DIR is required}"

if [ -n "${EXP_FAKE_CLAUDE_ARGV:-}" ]; then
    : > "$EXP_FAKE_CLAUDE_ARGV"
    for arg in "$@"; do
        printf '%s\n' "$arg" >> "$EXP_FAKE_CLAUDE_ARGV"
    done
    # The env half of the launch contract: the session-state events are on,
    # the entrypoint label is NOT set, and the MCP key reaches the child.
    printf 'env:CLAUDE_CODE_EMIT_SESSION_STATE_EVENTS=%s\n' \
        "${CLAUDE_CODE_EMIT_SESSION_STATE_EVENTS:-}" >> "$EXP_FAKE_CLAUDE_ARGV"
    printf 'env:CLAUDE_CODE_ENTRYPOINT=%s\n' "${CLAUDE_CODE_ENTRYPOINT:-}" >> "$EXP_FAKE_CLAUDE_ARGV"
    printf 'env:EXP_MCP_TOKEN=%s\n' "${EXP_MCP_TOKEN:-}" >> "$EXP_FAKE_CLAUDE_ARGV"
fi

request_id() {
    printf '%s' "$1" | sed -n 's/.*"request_id":"\([^"]*\)".*/\1/p'
}

success() {
    printf '{"type":"control_response","response":{"subtype":"success","request_id":"%s","response":{}}}\n' "$1"
}

replay() {
    [ -f "$1" ] && cat "$1"
}

turn=0
answers=0
while IFS= read -r line; do
    if [ -n "${EXP_FAKE_CLAUDE_STDIN:-}" ]; then
        printf '%s\n' "$line" >> "$EXP_FAKE_CLAUDE_STDIN"
    fi
    case "$line" in
        *'"subtype":"initialize"'*)
            id=$(request_id "$line")
            if [ -f "$dir/initialize.jsonl" ]; then
                sed "s/@@REQUEST_ID@@/$id/g" "$dir/initialize.jsonl"
            else
                sed "s/@@REQUEST_ID@@/$id/g" "$dir/../initialize.jsonl"
            fi
            ;;
        *'"subtype":"interrupt"'*)
            success "$(request_id "$line")"
            # An interrupt ends the running turn: the CLI's own trailer.
            replay "$dir/after-interrupt.jsonl"
            ;;
        *'"type":"control_request"'*)
            success "$(request_id "$line")"
            ;;
        *'"type":"control_response"'*)
            answers=$((answers + 1))
            replay "$dir/after-answer.jsonl"
            ;;
        *'"type":"user"'*)
            turn=$((turn + 1))
            if [ -f "$dir/turn$turn.jsonl" ]; then
                cat "$dir/turn$turn.jsonl"
            else
                replay "$dir/turn.jsonl"
            fi
            ;;
    esac
done
