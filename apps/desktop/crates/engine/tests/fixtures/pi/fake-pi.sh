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
#   prompt.turn<n>.events.jsonl  one script per prompt, used when
#                             `$EXP_FAKE_PI_TURNS` is set (EXP-758: two turns
#                             whose costs have to add up)
#
# EXP-758 also adds two ways for a command to go wrong, because both are
# states a real pi reaches and neither can be provoked any other way:
# `$EXP_FAKE_PI_SILENT=<command>` never answers that command (the rpc timeout)
# and `$EXP_FAKE_PI_FAIL=<command>` answers it `success:false` (a rejected
# steer). `$EXP_FAKE_PI_NO_TURN=<name>` makes a prompt carrying `/<name>` run
# like the extension command it is: answered, with no turn behind it.
#
# `$EXP_FAKE_PI_EDIT`, when set, is rewritten as the answered dialog's tool
# actually edits it — that is what gives the adapter a before/after diff.
# `$EXP_FAKE_PI_STDIN_LOG`, when set, collects every line we were sent, so a
# test can assert what the adapter actually WROTE (the `/exp-plan` switch).
#
# The rpc flags (`--mode rpc`, `--model`, …) are deliberately ignored: the
# argv is asserted separately, in `adapters::pi`'s own unit test.

dir="$EXP_FAKE_PI_DIR"
turn=0

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
		# EXP-758: a command pi never answers. Its caller must give up on
		# its own deadline rather than park for the life of the run.
		if [ -n "$EXP_FAKE_PI_SILENT" ] && [ "$command" = "$EXP_FAKE_PI_SILENT" ]; then
			continue
		fi
		# EXP-758: a command pi REJECTS (`success:false`).
		if [ -n "$EXP_FAKE_PI_FAIL" ] && [ "$command" = "$EXP_FAKE_PI_FAIL" ]; then
			printf '{"id":"%s","type":"response","command":"%s","success":false,"data":"pi rejected %s"}\n' \
				"$id" "$command" "$command"
			continue
		fi
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
		# EXP-758: any other extension command behaves the same way.
		if [ -n "$EXP_FAKE_PI_NO_TURN" ]; then
			case "$line" in
			*"\"message\":\"/$EXP_FAKE_PI_NO_TURN"*)
				continue
				;;
			esac
		fi
		events="$dir/$command.events.jsonl"
		if [ "$command" = "prompt" ] && [ -n "$EXP_FAKE_PI_PLAN" ]; then
			events="$dir/prompt.plan.events.jsonl"
		fi
		if [ "$command" = "prompt" ] && [ -n "$EXP_FAKE_PI_HANG" ]; then
			events="$dir/prompt.hang.events.jsonl"
		fi
		# EXP-758: one script per prompt, so two turns can carry different
		# costs and the session total can be watched adding up.
		if [ "$command" = "prompt" ] && [ -n "$EXP_FAKE_PI_TURNS" ]; then
			turn=$((turn + 1))
			if [ -f "$dir/prompt.turn$turn.events.jsonl" ]; then
				events="$dir/prompt.turn$turn.events.jsonl"
			fi
		fi
		if [ -f "$events" ]; then
			cat "$events"
		fi
		;;
	esac
done
