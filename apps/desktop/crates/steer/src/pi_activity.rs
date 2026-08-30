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

use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde_json::Value;

use crate::activity::{
    secrets_from_worktree, truncate, DiffSnapshots, EmitterConfig, NeedsInputForwarder, Redactor,
    RemoteAnswer, ID_MAX, NARRATION_MAX, POLL_INTERVAL, QUESTION_TEXT_MAX, TOOL_DETAIL_MAX,
    TOOL_NAME_MAX,
};
use crate::frames::{ActivityEvent, QuestionOption};
use crate::pi_observer::PiEvent;
use crate::publisher::ActivitySender;

/// How long a remote plan answer parks before its keystroke lands — pi's
/// confirm dialog needs at least one paint after `plan_pending` was posted.
/// There is no grid detector for pi (unlike claude's `plan_picker`), so a
/// fixed paint budget is the mitigation.
const PLAN_DIALOG_PAINT: Duration = Duration::from_millis(200);

/// The pi plan gate (EXP-441): at most ONE plan approval is pending at a
/// time (the extension blocks inside the confirm dialog), keyed by the
/// extension's `toolCallId`-derived id.
#[derive(Default)]
pub(crate) struct PiPlanState {
    pending: Option<PendingPiPlan>,
    /// Ids whose keystroke already landed — injecting twice is never safe,
    /// but re-acking is required (EXP-374: a rejoined viewer re-taps).
    answered: HashSet<String>,
}

pub(crate) struct PendingPiPlan {
    id: String,
    shown_at: Instant,
}

/// Map one observer event to zero-or-one publishable events, tracking the
/// idle edge in `idle` and the plan gate in `plan`.
pub(crate) fn map_event(
    event: PiEvent,
    idle: &mut bool,
    plan: &mut PiPlanState,
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
        // EXP-441: the plan gate. `plan_pending` becomes the same
        // planMode-marked Question every client already renders as a "Plan
        // ready" card for claude; `plan_resolved` retires it (the dialog was
        // answered locally or by our injected keystroke).
        PiEvent::PlanPending { id, plan: text } => {
            let id = truncate(&id, ID_MAX);
            plan.pending = Some(PendingPiPlan {
                id: id.clone(),
                shown_at: Instant::now(),
            });
            let text = truncate(&redactor.redact(&text), QUESTION_TEXT_MAX);
            Some(ActivityEvent::Question {
                text: if text.trim().is_empty() {
                    "Plan ready for approval.".to_string()
                } else {
                    text
                },
                options: vec![
                    QuestionOption::new("Approve plan", "1"),
                    QuestionOption::new("Reject — keep planning", "2"),
                ],
                multi_select: None,
                plan_mode: Some(true),
                id: Some(id),
                ask_id: None,
                index: None,
                total: None,
                header: None,
                at: None,
            })
        }
        PiEvent::PlanResolved { id, approved } => {
            let id = truncate(&id, ID_MAX);
            if plan.pending.as_ref().is_some_and(|pending| pending.id == id) {
                plan.pending = None;
            }
            Some(ActivityEvent::QuestionResolved {
                id: Some(id),
                ask_id: None,
                answers: approved.then(|| vec!["Approved".to_string()]),
                dismissed: (!approved).then_some(true),
                at: None,
            })
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

    sender.send(ActivityEvent::narration(crate::activity::launch_narration(
        config.bypass_permissions,
        config.plan_mode,
    )));

    let mut idle = false;
    let mut plan = PiPlanState::default();
    let mut diffs = DiffSnapshots::new();
    let mut needs_input = NeedsInputForwarder::new();

    while active.load(Ordering::SeqCst) {
        if let Some(events) = &config.pi_events {
            while let Ok(event) = events.try_recv() {
                if let Some(event) = map_event(event, &mut idle, &mut plan, &redactor) {
                    sender.send(event);
                }
            }
        }
        // EXP-441: remote plan answers. The publisher forwards `answer`
        // frames through the same semantic seam claude uses; the only
        // answerable pi question is the plan gate, resolved by keystroke
        // into pi's confirm dialog (Enter selects the highlighted "Yes"
        // row, Esc cancels).
        if let Some(steering) = &config.steering {
            while let Ok(answer) = steering.answers.try_recv() {
                handle_plan_answer(&answer, &mut plan, &steering.write_input, &sender);
            }
        }
        // A pending plan parks pi on a dialog — that IS "needs input" for
        // the synced badge, even though the agent never settled.
        needs_input.tick(idle || plan.pending.is_some(), &config.on_needs_input);
        // EXP-637: `agent_settled` is pi's true between-turns edge — the
        // graceful stop waits on it.
        if let Some(signal) = &config.turn_signal {
            signal.set_idle(idle);
        }
        diffs.tick(
            &config.worktree,
            config.base_ref.as_deref(),
            &sender,
            &redactor,
        );
        std::thread::sleep(POLL_INTERVAL);
    }

    needs_input.clear_on_teardown(&config.on_needs_input);
}

/// Resolve one remote answer against the plan gate: key `"1"` approves
/// (`\r` — Enter activates the confirm dialog's default "Yes" row), key
/// `"2"` rejects (`\x1b` — Esc cancels). Unknown ids and keys drop silently;
/// an already-injected id re-acks without injecting (EXP-374 parity —
/// injecting twice is never safe, but a rejoined viewer's re-tap must not
/// leave its card timing out).
fn handle_plan_answer(
    answer: &RemoteAnswer,
    plan: &mut PiPlanState,
    write_input: &crate::publisher::InputHook,
    sender: &ActivitySender,
) {
    if plan.answered.contains(&answer.question_id) {
        sender.send(ActivityEvent::AnswerAck {
            id: answer.question_id.clone(),
            ask_id: answer.ask_id.clone(),
            at: None,
        });
        return;
    }
    let Some(pending) = &plan.pending else { return };
    if pending.id != answer.question_id {
        return;
    }
    let bytes: &[u8] = match answer.keys.first().map(String::as_str) {
        Some("1") => b"\r",
        Some("2") => b"\x1b",
        _ => return,
    };
    // The dialog needs at least one paint after `plan_pending` — park out
    // the remainder (emitter thread; blocking is fine here).
    let elapsed = pending.shown_at.elapsed();
    if elapsed < PLAN_DIALOG_PAINT {
        std::thread::sleep(PLAN_DIALOG_PAINT - elapsed);
    }
    write_input(bytes);
    plan.answered.insert(answer.question_id.clone());
    sender.send(ActivityEvent::AnswerAck {
        id: answer.question_id.clone(),
        ask_id: None,
        at: None,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn redactor() -> Redactor {
        Redactor::new(Vec::new())
    }

    fn plan() -> PiPlanState {
        PiPlanState::default()
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
            &mut plan(),
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
            &mut plan(),
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
            &mut plan(),
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
            &mut plan(),
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
            &mut plan(),
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
        assert!(map_event(PiEvent::AgentSettled, &mut idle, &mut plan(), &redactor()).is_none());
        assert!(idle);
        assert!(map_event(PiEvent::AgentStart, &mut idle, &mut plan(), &redactor()).is_none());
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
                &mut plan(),
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
                &mut plan(),
                &redactor()
            ),
            Some(ActivityEvent::Narration { ref text, .. })
                if text == "Checking how the launcher spawns codex"
        ));
    }

    // ── EXP-441: the pi plan gate ───────────────────────────────────────────

    #[test]
    fn plan_pending_publishes_the_plan_mode_question_and_resolved_retires_it() {
        let mut idle = false;
        let mut plan = plan();
        let event = map_event(
            PiEvent::PlanPending {
                id: "call-9".into(),
                plan: "## Plan\n1. fix".into(),
            },
            &mut idle,
            &mut plan,
            &redactor(),
        )
        .unwrap();
        match event {
            ActivityEvent::Question {
                text,
                options,
                plan_mode,
                id,
                ..
            } => {
                assert_eq!(text, "## Plan\n1. fix");
                assert_eq!(plan_mode, Some(true));
                assert_eq!(id.as_deref(), Some("call-9"));
                let pairs: Vec<(&str, &str)> = options
                    .iter()
                    .map(|option| (option.label.as_str(), option.key.as_str()))
                    .collect();
                assert_eq!(
                    pairs,
                    [("Approve plan", "1"), ("Reject — keep planning", "2")]
                );
            }
            other => panic!("expected question, got {other:?}"),
        }
        assert!(plan.pending.is_some());

        // An approval resolves with an answer; the pending gate clears.
        let event = map_event(
            PiEvent::PlanResolved {
                id: "call-9".into(),
                approved: true,
            },
            &mut idle,
            &mut plan,
            &redactor(),
        )
        .unwrap();
        assert!(matches!(
            event,
            ActivityEvent::QuestionResolved {
                ref id,
                ref answers,
                dismissed: None,
                ..
            } if id.as_deref() == Some("call-9")
                && answers.as_deref() == Some(&["Approved".to_string()][..])
        ));
        assert!(plan.pending.is_none());

        // A rejection retires the card as dismissed.
        map_event(
            PiEvent::PlanPending {
                id: "call-10".into(),
                plan: "".into(),
            },
            &mut idle,
            &mut plan,
            &redactor(),
        )
        .unwrap();
        let event = map_event(
            PiEvent::PlanResolved {
                id: "call-10".into(),
                approved: false,
            },
            &mut idle,
            &mut plan,
            &redactor(),
        )
        .unwrap();
        assert!(matches!(
            event,
            ActivityEvent::QuestionResolved {
                answers: None,
                dismissed: Some(true),
                ..
            }
        ));
    }

    #[test]
    fn empty_plan_text_still_shows_an_approval_card() {
        let event = map_event(
            PiEvent::PlanPending {
                id: "call-1".into(),
                plan: "   ".into(),
            },
            &mut false,
            &mut plan(),
            &redactor(),
        )
        .unwrap();
        assert!(matches!(
            event,
            ActivityEvent::Question { ref text, .. } if text == "Plan ready for approval."
        ));
    }

    /// A captured `write_input` plus a sender pair for answer tests.
    fn recording_input() -> (crate::publisher::InputHook, Arc<std::sync::Mutex<Vec<u8>>>) {
        let keys: Arc<std::sync::Mutex<Vec<u8>>> = Arc::new(std::sync::Mutex::new(Vec::new()));
        let sink = Arc::clone(&keys);
        let hook: crate::publisher::InputHook =
            Arc::new(move |bytes: &[u8]| sink.lock().unwrap().extend_from_slice(bytes));
        (hook, keys)
    }

    #[test]
    fn remote_answers_inject_enter_or_esc_exactly_once() {
        let (sender, published) = ActivitySender::test_pair();
        let (hook, keys) = recording_input();
        let mut plan = plan();
        let mut idle = false;
        map_event(
            PiEvent::PlanPending {
                id: "call-2".into(),
                plan: "p".into(),
            },
            &mut idle,
            &mut plan,
            &redactor(),
        );

        // Approve: Enter lands once, the answer acks, a duplicate re-acks
        // without injecting again.
        let answer = RemoteAnswer {
            question_id: "call-2".into(),
            ask_id: None,
            keys: vec!["1".into()],
            text: None,
        };
        handle_plan_answer(&answer, &mut plan, &hook, &sender);
        assert_eq!(keys.lock().unwrap().as_slice(), b"\r");
        handle_plan_answer(&answer, &mut plan, &hook, &sender);
        assert_eq!(keys.lock().unwrap().as_slice(), b"\r", "never twice");
        let acks = published
            .drain()
            .filter(|cmd| {
                matches!(
                    cmd,
                    crate::publisher::PublisherCmd::Activity(ActivityEvent::AnswerAck {
                        id, ..
                    }) if id == "call-2"
                )
            })
            .count();
        assert_eq!(acks, 2, "the duplicate re-acks (EXP-374)");

        // Reject on a fresh pending: Esc.
        keys.lock().unwrap().clear();
        map_event(
            PiEvent::PlanPending {
                id: "call-3".into(),
                plan: "p".into(),
            },
            &mut idle,
            &mut plan,
            &redactor(),
        );
        handle_plan_answer(
            &RemoteAnswer {
                question_id: "call-3".into(),
                ask_id: None,
                keys: vec!["2".into()],
                text: None,
            },
            &mut plan,
            &hook,
            &sender,
        );
        assert_eq!(keys.lock().unwrap().as_slice(), b"\x1b");

        // Unknown ids and unknown keys never inject.
        keys.lock().unwrap().clear();
        map_event(
            PiEvent::PlanPending {
                id: "call-4".into(),
                plan: "p".into(),
            },
            &mut idle,
            &mut plan,
            &redactor(),
        );
        handle_plan_answer(
            &RemoteAnswer {
                question_id: "other".into(),
                ask_id: None,
                keys: vec!["1".into()],
                text: None,
            },
            &mut plan,
            &hook,
            &sender,
        );
        handle_plan_answer(
            &RemoteAnswer {
                question_id: "call-4".into(),
                ask_id: None,
                keys: vec!["9".into()],
                text: None,
            },
            &mut plan,
            &hook,
            &sender,
        );
        assert!(keys.lock().unwrap().is_empty());
    }
}
