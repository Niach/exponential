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
#
# `$EXP_FAKE_PI_EDIT`, when set, is rewritten as the answered dialog's tool
# actually edits it — that is what gives the adapter a before/after diff.
#
# The rpc flags (`--mode rpc`, `--model`, …) are deliberately ignored: the
# argv is asserted separately, in `adapters::pi`'s own unit test.

dir="$EXP_FAKE_PI_DIR"

while IFS= read -r line; do
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
		if [ -f "$dir/$command.events.jsonl" ]; then
			cat "$dir/$command.events.jsonl"
		fi
		;;
	esac
done
