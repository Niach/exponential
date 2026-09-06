#!/bin/sh
# EXP-746 (lane E4) — a fake `pi --mode rpc` for `tests/pi_adapter.rs`.
#
# It answers every rpc command from the fixture files beside it, so the
# adapter is exercised over a REAL child process (the spawn recipe, the
# LF-only line framing, the writer mutex), not a mocked channel:
#
#   <command>.json          the `data` of that command's response (ONE line)
#   <command>.events.jsonl  events to emit after that response
#   after_confirm.jsonl     events to emit once the client answers a dialog
#   prompt.plan.events.jsonl  a plan turn's events, used when
#                             `$EXP_FAKE_PI_PLAN` is set (EXP-752)
#   prompt.hang.events.jsonl  a turn that STREAMS AND NEVER SETTLES, used when
#                             `$EXP_FAKE_PI_HANG` is set — the mid-turn state
#                             the plan switch refuses (EXP-752)
#
# `$EXP_FAKE_PI_EDIT`, when set, is rewritten as the answered dialog's tool
# actually edits it — that is what gives the adapter a before/after diff.
# `$EXP_FAKE_PI_STDIN_LOG`, when set, collects every line we were sent, so a
# test can assert what the adapter actually WROTE (the `/exp-plan` switch).
#
# The rpc flags (`--mode rpc`, `--model`, …) are deliberately ignored: the
# argv is asserted separately, in `adapters::pi`'s own unit test.

dir="$EXP_FAKE_PI_DIR"

while IFS= read -r line; do
	if [ -n "$EXP_FAKE_PI_STDIN_LOG" ]; then
		printf '%s\n' "$line" >>"$EXP_FAKE_PI_STDIN_LOG"
	fi
	# Our own writer emits `{"id":…,"type":…}` for a command and
	# `{"type":"extension_ui_response",…}` for a dialog answer, in that key
	# order, so both are readable with one anchored expression each.
	id=$(printf '%s' "$line" | sed -n 's/^{"id":"\([^"]*\)".*/\1/p')
	if [ -n "$id" ]; then
		command=$(printf '%s' "$line" | sed -n 's/^{"id":"[^"]*","type":"\([^"]*\)".*/\1/p')
	else
		command=$(printf '%s' "$line" | sed -n 's/^{"type":"\([^"]*\)".*/\1/p')
	fi

	case "$command" in
	extension_ui_response)
		if [ -n "$EXP_FAKE_PI_EDIT" ]; then
			printf 'after\n' >"$EXP_FAKE_PI_EDIT"
		fi
		if [ -f "$dir/after_confirm.jsonl" ]; then
			cat "$dir/after_confirm.jsonl"
		fi
		;;
	"")
		# Unparseable input: pi would ignore it, and so do we.
		;;
	*)
		if [ -f "$dir/$command.json" ]; then
			printf '{"id":"%s","type":"response","command":"%s","success":true,"data":%s}\n' \
				"$id" "$command" "$(cat "$dir/$command.json")"
		else
			printf '{"id":"%s","type":"response","command":"%s","success":true}\n' \
				"$id" "$command"
		fi
		# EXP-752: a prompt carrying an extension command runs the command
		# and starts NO turn — pi answers it and stays idle.
		case "$line" in
		*'"message":"/exp-plan'*)
			continue
			;;
		esac
		events="$dir/$command.events.jsonl"
		if [ "$command" = "prompt" ] && [ -n "$EXP_FAKE_PI_PLAN" ]; then
			events="$dir/prompt.plan.events.jsonl"
		fi
		if [ "$command" = "prompt" ] && [ -n "$EXP_FAKE_PI_HANG" ]; then
			events="$dir/prompt.hang.events.jsonl"
		fi
		if [ -f "$events" ]; then
			cat "$events"
		fi
		;;
	esac
done
