//! The pi activity emitter (EXP-383): drains the structured event stream the
//! `.exp-pi-observer.ts` extension POSTs to the loopback observer sidecar
//! ([`crate::pi_observer`]) and maps it onto the same scrubbed
//! [`ActivityEvent`] stream the claude emitter publishes — narration, tool
//! headlines, the user's prompts, and the synced needs-input flag — plus the
//! shared debounced worktree diff.
//!
//! Mapping stance:
//!
//! * `input` (interactive OR extension source) → [`ActivityEvent::UserMessage`]
//!   — an extension-injected steer is a real user turn and OTHER viewers must
//!   see it (the sender's own client already dedupes its echo).
//! * completed assistant text blocks → narration; completed THINKING blocks
//!   → narration too (EXP-389). Pi models mostly think between tool calls
//!   and only write prose at the end of a turn, so a thinking-less feed is
//!   just a wall of tool headlines — the deliberate codex parity here is the
//!   `agent_reasoning` narration, not claude's prose-only stance.
//! * `tool_execution_start` → a tool headline with a DERIVED detail (path /
//!   pattern / description / bash first token — never a raw command string).
//! * `agent_start` / `agent_settled` → the synced needs-input flag.
//!   `agent_settled` is pi's true idle signal (`agent_end` can still be
//!   followed by auto-retry / queued follow-ups).

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use serde_json::Value;

use crate::activity::{
    secrets_from_worktree, truncate, DiffSnapshots, EmitterConfig, NeedsInputForwarder, Redactor,
    NARRATION_MAX, POLL_INTERVAL, TOOL_DETAIL_MAX, TOOL_NAME_MAX,
};
use crate::frames::ActivityEvent;
use crate::pi_observer::PiEvent;
use crate::publisher::ActivitySender;

/// Map one observer event to zero-or-one publishable events, tracking the
/// idle edge in `idle`.
pub(crate) fn map_event(
    event: PiEvent,
    idle: &mut bool,
    redactor: &Redactor,
) -> Option<ActivityEvent> {
    match event {
        PiEvent::Input { text, source } => {
            // `rpc` cannot occur in an embedded TUI session; skip it anyway
            // so a future pi mode never leaks unexpected input sources.
            if source == "rpc" {
                return None;
            }
            *idle = false;
            Some(ActivityEvent::user_message(truncate(
                &redactor.redact(&text),
                NARRATION_MAX,
            )))
        }
        PiEvent::AssistantText { text } | PiEvent::Thinking { text } => Some(
            ActivityEvent::narration(truncate(&redactor.redact(&text), NARRATION_MAX)),
        ),
        PiEvent::ToolStart { name, args, .. } => {
            let detail = tool_detail(&name, &args)
                .map(|detail| truncate(&redactor.redact(&detail), TOOL_DETAIL_MAX));
            Some(ActivityEvent::tool(truncate(&name, TOOL_NAME_MAX), detail))
        }
        PiEvent::ToolEnd { .. } => None,
        PiEvent::AgentStart => {
            *idle = false;
            None
        }
        PiEvent::AgentSettled => {
            *idle = true;
            None
        }
        PiEvent::SessionStart | PiEvent::SessionShutdown => None,
    }
}

/// The single primary argument worth publishing for a pi tool call — the
/// same stance as the claude tool headlines: a path/pattern/description,
/// never a raw command string (bash publishes its first token only).
fn tool_detail(name: &str, args: &Value) -> Option<String> {
    for key in ["path", "file_path", "filePath", "pattern", "description"] {
        if let Some(text) = args.get(key).and_then(Value::as_str) {
            if !text.trim().is_empty() {
                return Some(text.trim().to_string());
            }
        }
    }
    if name.eq_ignore_ascii_case("bash") {
        return args
            .get("command")
            .and_then(Value::as_str)
            .and_then(|command| command.split_whitespace().next())
            .map(str::to_string);
    }
    None
}

pub(crate) fn run_emitter(config: EmitterConfig, sender: ActivitySender, active: Arc<AtomicBool>) {
    let mut exact_secrets = secrets_from_worktree(&config.worktree);
    exact_secrets.extend(config.extra_secrets.iter().cloned());
    let redactor = Redactor::new(exact_secrets);

    sender.send(ActivityEvent::narration("Session started"));

    let mut idle = false;
    let mut diffs = DiffSnapshots::new();
    let mut needs_input = NeedsInputForwarder::new();

    while active.load(Ordering::SeqCst) {
        if let Some(events) = &config.pi_events {
            while let Ok(event) = events.try_recv() {
                if let Some(event) = map_event(event, &mut idle, &redactor) {
                    sender.send(event);
                }
            }
        }
        needs_input.tick(idle, &config.on_needs_input);
        diffs.tick(&config.worktree, &sender, &redactor);
        std::thread::sleep(POLL_INTERVAL);
    }

    needs_input.clear_on_teardown(&config.on_needs_input);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn redactor() -> Redactor {
        Redactor::new(Vec::new())
    }

    #[test]
    fn user_input_becomes_a_user_message_for_both_live_sources() {
        let mut idle = true;
        let event = map_event(
            PiEvent::Input {
                text: "try the other approach".into(),
                source: "interactive".into(),
            },
            &mut idle,
            &redactor(),
        )
        .unwrap();
        assert!(matches!(
            event,
            ActivityEvent::UserMessage { ref text, .. } if text == "try the other approach"
        ));
        assert!(!idle, "a prompt means pi is about to work");

        // A remotely-steered message re-enters as source "extension" — other
        // viewers must see it too.
        assert!(map_event(
            PiEvent::Input {
                text: "steered".into(),
                source: "extension".into(),
            },
            &mut idle,
            &redactor(),
        )
        .is_some());
        // A future rpc source is skipped.
        assert!(map_event(
            PiEvent::Input {
                text: "x".into(),
                source: "rpc".into(),
            },
            &mut idle,
            &redactor(),
        )
        .is_none());
    }

    #[test]
    fn tool_details_are_derived_never_raw_commands() {
        let mut idle = false;
        let event = map_event(
            PiEvent::ToolStart {
                id: "t1".into(),
                name: "bash".into(),
                args: serde_json::json!({ "command": "git push origin exp/EXP-1 --force" }),
            },
            &mut idle,
            &redactor(),
        )
        .unwrap();
        match event {
            ActivityEvent::Tool { name, detail, .. } => {
                assert_eq!(name, "bash");
                assert_eq!(detail.as_deref(), Some("git"));
            }
            other => panic!("expected tool, got {other:?}"),
        }
        let event = map_event(
            PiEvent::ToolStart {
                id: "t2".into(),
                name: "read".into(),
                args: serde_json::json!({ "path": "src/lib.rs" }),
            },
            &mut idle,
            &redactor(),
        )
        .unwrap();
        match event {
            ActivityEvent::Tool { detail, .. } => assert_eq!(detail.as_deref(), Some("src/lib.rs")),
            other => panic!("expected tool, got {other:?}"),
        }
    }

    #[test]
    fn settled_and_start_drive_the_idle_flag_without_publishing() {
        let mut idle = false;
        assert!(map_event(PiEvent::AgentSettled, &mut idle, &redactor()).is_none());
        assert!(idle);
        assert!(map_event(PiEvent::AgentStart, &mut idle, &redactor()).is_none());
        assert!(!idle);
    }

    #[test]
    fn assistant_text_and_thinking_both_become_narration() {
        // EXP-389: thinking is pi's only between-tool-calls signal — a feed
        // without it is just tool headlines.
        let mut idle = false;
        assert!(matches!(
            map_event(
                PiEvent::AssistantText { text: "Done.".into() },
                &mut idle,
                &redactor()
            ),
            Some(ActivityEvent::Narration { ref text, .. }) if text == "Done."
        ));
        assert!(matches!(
            map_event(
                PiEvent::Thinking {
                    text: "Checking how the launcher spawns codex".into()
                },
                &mut idle,
                &redactor()
            ),
            Some(ActivityEvent::Narration { ref text, .. })
                if text == "Checking how the launcher spawns codex"
        ));
    }
}
