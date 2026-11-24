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
#   control_request generate_session_title -> the title `Fake chat name`
#   control_request <other>     -> a bare success (set_permission_mode, …)
#   user message                -> the next turn's frames (turn1, turn2, …)
#   control_response            -> the frames recorded AFTER that answer
#                                  (the n-th: `after-answer<n>.jsonl` when
#                                  the scenario has one)
#
# Env:
#   EXP_FAKE_CLAUDE_DIR    the scenario directory (required)
#   EXP_FAKE_CLAUDE_ARGV   file to dump argv + the env facts into (optional)
#   EXP_FAKE_CLAUDE_STDIN  file to append every stdin line to (optional)
#   EXP_FAKE_CLAUDE_EXIT   exit with this code once the turn has replayed —
#                          the crashed / OOM-killed / `kill -9`ed CLI
#   EXP_FAKE_CLAUDE_DIE_ON close stdout WITHOUT answering a control request of
#                          this subtype, then linger: the CLI whose stream
#                          ended with a control request still in flight
#                          (EXP-758). It lingers so the adapter's own answer
#                          has somewhere to land: a process that also exits
#                          closes the whole connection within microseconds and
#                          the client would see the transport go, not the
#                          reason.
#   EXP_FAKE_CLAUDE_HANGS  with EXP_FAKE_CLAUDE_SPAWNS (a counter file): the
#                          first N spawns answer NOTHING, not even
#                          `initialize` (#FEED-253: the CLI wedged at startup)
#   EXP_FAKE_CLAUDE_PRESSURE with EXP_CGROUP_ROOT (a fake cgroup2 root): a
#                          hanging spawn first builds, under that root, its
#                          OWN cgroup path (wherever it runs: a run scope of
#                          its own on a systemd host) with the parent slice
#                          over its memory.high and a sibling scope holding
#                          most of it, and writes the slice's name to
#                          EXP_FAKE_CLAUDE_PRESSURE (#FEED-306)
set -u

dir="${EXP_FAKE_CLAUDE_DIR:?EXP_FAKE_CLAUDE_DIR is required}"

if [ -n "${EXP_FAKE_CLAUDE_HANGS:-}" ] && [ -n "${EXP_FAKE_CLAUDE_SPAWNS:-}" ]; then
    echo spawn >> "$EXP_FAKE_CLAUDE_SPAWNS"
    if [ "$(wc -l < "$EXP_FAKE_CLAUDE_SPAWNS")" -le "$EXP_FAKE_CLAUDE_HANGS" ]; then
        if [ -n "${EXP_FAKE_CLAUDE_PRESSURE:-}" ] && [ -n "${EXP_CGROUP_ROOT:-}" ]; then
            own=$(sed -n 's/^0:://p' "/proc/$$/cgroup" 2>/dev/null)
            slice=${own%/*}
            if [ -n "$own" ] && [ -n "${slice#/}" ]; then
                calm='some avg10=0.00 avg60=0.00 avg300=0.00 total=0
full avg10=0.00 avg60=0.00 avg300=0.00 total=0'
                for entry in "$slice 17609365913 17179869184" "$own 52428800 max" "$slice/run-exp-hog.scope 13529146982 max"; do
                    set -- $entry
                    mkdir -p "$EXP_CGROUP_ROOT$1"
                    echo "$2" > "$EXP_CGROUP_ROOT$1/memory.current"
                    echo "$3" > "$EXP_CGROUP_ROOT$1/memory.high"
                    echo "$calm" > "$EXP_CGROUP_ROOT$1/memory.pressure"
                done
                echo "${slice##*/}" > "$EXP_FAKE_CLAUDE_PRESSURE"
            fi
        fi
        # Swallow stdin, say nothing, until stopped.
        while IFS= read -r _; do :; done
        sleep 30
        exit 0
    fi
fi

if [ -n "${EXP_FAKE_CLAUDE_ARGV:-}" ]; then
    : > "$EXP_FAKE_CLAUDE_ARGV"
    for arg in "$@"; do
        printf '%s\n' "$arg" >> "$EXP_FAKE_CLAUDE_ARGV"
    done
    # The env half of the launch contract: the session-state events are on,
    # the entrypoint label is NOT set, and the MCP key reaches the child.
    printf 'env:CLAUDE_CODE_EMIT_SESSION_STATE_EVENTS=%s\n' \
        "${CLAUDE_CODE_EMIT_SESSION_STATE_EVENTS:-}" >> "$EXP_FAKE_CLAUDE_ARGV"
    printf 'env:CLAUDE_CODE_TERMINAL_MCP_TOOLS=%s\n' \
        "${CLAUDE_CODE_TERMINAL_MCP_TOOLS:-}" >> "$EXP_FAKE_CLAUDE_ARGV"
    printf 'env:CLAUDE_CODE_SILENT_TURN_REMINDER=%s\n' \
        "${CLAUDE_CODE_SILENT_TURN_REMINDER:-}" >> "$EXP_FAKE_CLAUDE_ARGV"
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
    [ -f "$1" ] && play "$1"
}

# A recorded turn, frame by frame. A `@@SLEEP <seconds>` line is not a frame:
# it is the CLI's own latency at that point (EXP-1224: the gap between a
# background agent's notification and the continuation it wakes), so a test
# can tell an edge published IN that gap from one published after it.
play() {
    if grep -q '^@@SLEEP ' "$1"; then
        while IFS= read -r frame || [ -n "$frame" ]; do
            case "$frame" in
                '@@SLEEP '*) sleep "${frame#@@SLEEP }" ;;
                *) printf '%s\n' "$frame" ;;
            esac
        done < "$1"
    else
        cat "$1"
    fi
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
        *'"subtype":"generate_session_title"'*)
            printf '{"type":"control_response","response":{"subtype":"success","request_id":"%s","response":{"title":"Fake chat name"}}}\n' \
                "$(request_id "$line")"
            ;;
        *'"type":"control_request"'*)
            case "$line" in
            *"\"subtype\":\"${EXP_FAKE_CLAUDE_DIE_ON:-@@none@@}\""*)
                exec 1>&-
                sleep 30
                exit 9
                ;;
            esac
            success "$(request_id "$line")"
            ;;
        *'"type":"control_response"'*)
            answers=$((answers + 1))
            # A scenario with several calls answers each in turn
            # (`after-answer<n>.jsonl`), else every answer replays one file.
            if [ -f "$dir/after-answer$answers.jsonl" ]; then
                play "$dir/after-answer$answers.jsonl"
            else
                replay "$dir/after-answer.jsonl"
            fi
            ;;
        *'"type":"user"'*)
            turn=$((turn + 1))
            if [ -f "$dir/turn$turn.jsonl" ]; then
                play "$dir/turn$turn.jsonl"
            else
                replay "$dir/turn.jsonl"
            fi
            if [ -n "${EXP_FAKE_CLAUDE_EXIT:-}" ]; then
                exit "$EXP_FAKE_CLAUDE_EXIT"
            fi
            ;;
    esac
done
