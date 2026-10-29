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

use super::activity::RemoteAnswer;
use super::frames::ActivityEvent;

/// One relay answer as a card write-back: a question's `answer` (the typed
/// text, else the picked labels, `, `-joined) with a multi-select's picked
/// labels beside it as `answers` (what the closed card highlights), or a
/// plan's `decision` (`approved` | `rejected`) + the reject's note as
/// `feedback`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CardResolution {
    pub question_id: String,
    pub answer: Option<String>,
    /// A multi-select's picked labels (`None` for a single pick or typed
    /// text: the `answer` names the option).
    pub answers: Option<Vec<String>>,
    pub decision: Option<String>,
    /// Never set: the claude adapter's ExitPlanMode is a plain allow (no
    /// fresh-context option on the wire any more); a fresh approval is the
    /// thread card's own `sessions.answer`. Kept for the taps' write-back
    /// shape.
    pub fresh: Option<bool>,
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
/// picked options' labels (`, `-joined; a multi-select's also as
/// `answers`), else the raw keys.
pub fn card_resolution(answer: &RemoteAnswer, question: Option<&ActivityEvent>) -> CardResolution {
    let (options, plan, multi) = match question {
        Some(ActivityEvent::Question { options, plan_mode, multi_select, .. }) => {
            (options.as_slice(), plan_mode.unwrap_or(false), multi_select.unwrap_or(false))
        }
        _ => (&[][..], false, false),
    };
    let mut resolution = CardResolution { question_id: answer.question_id.clone(), ..CardResolution::default() };
    if plan {
        let reject = options.last().map(|option| option.key.as_str());
        let rejected = reject.is_some_and(|reject| answer.keys.iter().any(|key| key == reject));
        resolution.decision = Some(if rejected { "rejected" } else { "approved" }.to_string());
        resolution.feedback = non_blank(answer.text.as_deref()).filter(|_| rejected);
        return resolution;
    }
    if let Some(text) = non_blank(answer.text.as_deref()) {
        resolution.answer = Some(text);
        return resolution;
    }
    let labels: Vec<String> = answer
        .keys
        .iter()
        .map(|key| {
            options
                .iter()
                .find(|option| &option.key == key && !option.free_text)
                .map_or(key.as_str(), |option| option.label.as_str())
        })
        .filter(|label| !label.trim().is_empty())
        .map(str::to_string)
        .collect();
    resolution.answer = non_blank(Some(&labels.join(", ")));
    // EXP-1280: the closed card highlights its picks off `answers`; the
    // joined string names no option once there are two.
    if multi && resolution.answer.is_some() {
        resolution.answers = Some(labels);
    }
    resolution
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::frames::QuestionOption;

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
        assert_eq!(picked.answers, None, "a single pick names its option in `answer`");
        assert_eq!(picked.decision, None);
        let typed = card_resolution(&answer(&["3"], Some(" MySQL ")), Some(&card));
        assert_eq!(typed.answer.as_deref(), Some("MySQL"));
        assert_eq!(typed.answers, None);
        // Not in the journal: the raw keys still close the card.
        assert_eq!(card_resolution(&answer(&["1"], None), None).answer.as_deref(), Some("1"));
    }

    /// EXP-1280: a multi-select's picks ride `answers` too — the closed
    /// card highlights off them, and the joined `answer` names no option.
    #[test]
    fn a_multi_select_answers_with_its_labels_as_answers_too() {
        let mut card = question(false);
        if let ActivityEvent::Question { multi_select, .. } = &mut card {
            *multi_select = Some(true);
        }
        let picked = card_resolution(&answer(&["1", "2"], None), Some(&card));
        assert_eq!(picked.answer.as_deref(), Some("Postgres, SQLite"));
        assert_eq!(picked.answers, Some(vec!["Postgres".to_string(), "SQLite".to_string()]));
        // Typed text is the answer alone, picks or not.
        let typed = card_resolution(&answer(&["1", "3"], Some("MySQL")), Some(&card));
        assert_eq!(typed.answer.as_deref(), Some("MySQL"));
        assert_eq!(typed.answers, None);
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
        assert_eq!(approved.fresh, None);
    }

    /// The engine's plan picker has no fresh-context option (an
    /// ExitPlanMode is a plain allow): no approve key resolves `fresh`.
    #[test]
    fn no_plan_pick_resolves_fresh() {
        let mut plan = question(true);
        if let ActivityEvent::Question { options, .. } = &mut plan {
            *options = vec![
                QuestionOption::new("Yes", "exit-plan-bypass"),
                QuestionOption::new("No, keep planning", "reject"),
            ];
        }
        let plain = card_resolution(&answer(&["exit-plan-bypass"], None), Some(&plan));
        assert_eq!(plain.decision.as_deref(), Some("approved"));
        assert_eq!(plain.fresh, None);
        assert_eq!(plain.answers, None);
    }
}
