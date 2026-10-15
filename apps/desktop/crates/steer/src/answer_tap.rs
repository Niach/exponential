//! EXP-1316 — the answer TAP: a question/plan card answered in the SESSION
//! view reaches the engine as a `RemoteAnswer` (over the relay's
//! `ServerFrame::Answer`, the IDE's local card, or the CLI's own TUI),
//! never through the synced `messages` row the device posted for it, so the
//! thread's card would stay open on every other client. The engine host
//! hands each such answer (EXP-1317: from its `answer_ask`, the one choke
//! point every path shares — the publisher used to, and only saw the
//! relay's), translated into `sessions.resolveCard` terms
//! ([`card_resolution`]), to every tap a host installed
//! ([`add_answer_tap`]); the host maps the question id onto the card's
//! `messages.id` (`coding::pickup::LiveRegistry::resolve_card_input`) and
//! writes it back off-thread. Fire-and-forget: a tap never blocks the
//! engine, and the answer is submitted to it either way.

use std::sync::{Arc, RwLock};

use crate::activity::RemoteAnswer;
use crate::frames::ActivityEvent;

/// One relay answer as a card write-back: a question's `answer` (the typed
/// text, else the picked labels), or a plan's `decision` (`approved` |
/// `rejected`) + the reject's note as `feedback`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CardResolution {
    pub question_id: String,
    pub answer: Option<String>,
    pub decision: Option<String>,
    pub feedback: Option<String>,
}

/// A host's tap: `(session_id, resolution)`.
pub type AnswerTap = Arc<dyn Fn(&str, &CardResolution) + Send + Sync>;

static TAPS: RwLock<Vec<AnswerTap>> = RwLock::new(Vec::new());

/// Install a tap for the life of the process (each pickup host adds one;
/// a tap ignores sessions it does not run).
pub fn add_answer_tap(tap: AnswerTap) {
    if let Ok(mut taps) = TAPS.write() {
        taps.push(tap);
    }
}

/// Hand `resolution` to every installed tap (the engine's `answer_ask`,
/// before the mapper resolves the answer: the card is still open in the
/// outbox the tap maps it through).
pub fn notify(session_id: &str, resolution: &CardResolution) {
    let taps: Vec<AnswerTap> = match TAPS.read() {
        Ok(taps) => taps.clone(),
        Err(_) => return,
    };
    for tap in taps {
        tap(session_id, resolution);
    }
}

fn non_blank(text: Option<&str>) -> Option<String> {
    text.map(str::trim).filter(|text| !text.is_empty()).map(str::to_string)
}

/// Translate an answer against the question it answers (the engine's held
/// `question` event; `None` = unknown or evicted): a
/// plan card's LAST option is its reject (the session view's rule), any
/// other key approves; a question answers with its typed text, else the
/// picked options' labels (`, `-joined), else the raw keys.
pub fn card_resolution(answer: &RemoteAnswer, question: Option<&ActivityEvent>) -> CardResolution {
    let (options, plan) = match question {
        Some(ActivityEvent::Question { options, plan_mode, .. }) => {
            (options.as_slice(), plan_mode.unwrap_or(false))
        }
        _ => (&[][..], false),
    };
    let mut resolution = CardResolution {
        question_id: answer.question_id.clone(),
        ..CardResolution::default()
    };
    if plan {
        let reject = options.last().map(|option| option.key.as_str());
        let rejected = reject.is_some_and(|reject| answer.keys.iter().any(|key| key == reject));
        resolution.decision = Some(if rejected { "rejected" } else { "approved" }.to_string());
        resolution.feedback = non_blank(answer.text.as_deref()).filter(|_| rejected);
        return resolution;
    }
    resolution.answer = non_blank(answer.text.as_deref()).or_else(|| {
        let labels: Vec<&str> = answer
            .keys
            .iter()
            .map(|key| {
                options
                    .iter()
                    .find(|option| &option.key == key && !option.free_text)
                    .map_or(key.as_str(), |option| option.label.as_str())
            })
            .collect();
        non_blank(Some(&labels.join(", ")))
    });
    resolution
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frames::QuestionOption;

    fn question(plan: bool) -> ActivityEvent {
        let mut free = QuestionOption::new("Type something.", "3");
        free.free_text = true;
        ActivityEvent::Question {
            text: "Which DB?".into(),
            options: vec![QuestionOption::new("Postgres", "1"), QuestionOption::new("SQLite", "2"), free],
            multi_select: None,
            plan_mode: plan.then_some(true),
            id: "q-1".into(),
            ask_id: None,
            index: None,
            total: None,
            header: None,
            at: None,
        }
    }

    fn answer(keys: &[&str], text: Option<&str>) -> RemoteAnswer {
        RemoteAnswer {
            question_id: "q-1".into(),
            ask_id: None,
            keys: keys.iter().map(|key| key.to_string()).collect(),
            text: text.map(str::to_string),
        }
    }

    #[test]
    fn a_question_answers_with_the_label_or_the_typed_text() {
        let card = question(false);
        let picked = card_resolution(&answer(&["2"], None), Some(&card));
        assert_eq!(picked.answer.as_deref(), Some("SQLite"));
        assert_eq!(picked.decision, None);
        let typed = card_resolution(&answer(&["3"], Some(" MySQL ")), Some(&card));
        assert_eq!(typed.answer.as_deref(), Some("MySQL"));
        // Not in the journal: the raw keys still close the card.
        assert_eq!(card_resolution(&answer(&["1"], None), None).answer.as_deref(), Some("1"));
    }

    #[test]
    fn a_plan_decides_off_its_last_option() {
        let plan = question(true);
        let approved = card_resolution(&answer(&["1"], Some("ignored")), Some(&plan));
        assert_eq!(approved.decision.as_deref(), Some("approved"));
        assert_eq!(approved.feedback, None);
        let rejected = card_resolution(&answer(&["3"], Some("Smaller")), Some(&plan));
        assert_eq!(rejected.decision.as_deref(), Some("rejected"));
        assert_eq!(rejected.feedback.as_deref(), Some("Smaller"));
        assert_eq!(rejected.answer, None);
    }
}
