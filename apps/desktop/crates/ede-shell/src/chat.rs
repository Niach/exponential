//! The agent chat (#EDE-65 lane I): one conversation as the shared chat
//! document draws it (`apps/ede/ui/chat.json`, the model
//! `uicore_model::chat`). The domain builds the thread (session-thread.json:
//! the rows, their order, the cards' facts); this resolves the words the
//! other fixtures own (a divider's label, a plan's status, the locked
//! pill) and the clock, so the model only decides how the chat is drawn.

use std::collections::HashMap;

use domain::rows::MessageRow;
use domain::session_tree::SessionThreadRow;
use uicore_model::chat::{ChatOption, ChatRowInput};

/// `HH:MM` (local) of a wire timestamp; empty when unparseable.
pub fn clock(at: Option<&str>) -> String {
    at.and_then(domain::session_tree::iso_ms)
        .and_then(chrono::DateTime::from_timestamp_millis)
        .map(|at| at.with_timezone(&chrono::Local).format("%H:%M").to_string())
        .unwrap_or_default()
}

/// The domain's thread rows as the chat model's; `messages` = the rows'
/// messages by id (their time), `name` = a person's display name.
pub fn rows(rows: &[SessionThreadRow], messages: &HashMap<&str, &MessageRow>, name: impl Fn(Option<&str>) -> Option<String>) -> Vec<ChatRowInput> {
    rows.iter().filter_map(|row| chat_row(row, messages, &name)).collect()
}

fn chat_row(row: &SessionThreadRow, messages: &HashMap<&str, &MessageRow>, name: &impl Fn(Option<&str>) -> Option<String>) -> Option<ChatRowInput> {
    let at = |id: &str| clock(messages.get(id).and_then(|m| m.created_at.as_deref()));
    Some(match row {
        SessionThreadRow::Message { id, author, author_id, body, queued, .. } => ChatRowInput::Message {
            id: id.clone(),
            author: author.clone(),
            body: body.clone(),
            name: if author == "user" { name(author_id.as_deref()) } else { None },
            time: at(id),
            queued: *queued,
        },
        SessionThreadRow::Session { id, card } => ChatRowInput::Card {
            id: id.clone(),
            icon: card.icon.clone(),
            title: card.name.clone(),
            text: card.kind_word.clone(),
            target: Some(card.session_id.clone()),
        },
        SessionThreadRow::Report { id, session_id, text, .. } => {
            ChatRowInput::Card { id: id.clone(), icon: "session-tree".into(), title: text.clone(), text: String::new(), target: Some(session_id.clone()) }
        }
        SessionThreadRow::Suggestions { id, session_id, header, .. } => {
            ChatRowInput::Card { id: id.clone(), icon: "list-checks".into(), title: header.clone(), text: String::new(), target: Some(session_id.clone()) }
        }
        SessionThreadRow::Question { id, question, options, custom, multi, answer, answers, answered, dismissed, .. } => ChatRowInput::Question {
            id: id.clone(),
            question: question.clone(),
            options: options.iter().map(|o| ChatOption { label: o.label.clone(), description: o.description.clone() }).collect(),
            custom: *custom,
            multi: *multi,
            waiting: !*answered && *dismissed != Some(true),
            answer: answers.as_ref().filter(|a| !a.is_empty()).map(|a| a.join(", ")).or_else(|| answer.clone()),
        },
        // The plan row (thread_screen `plan_row`): the plan's label, its status word.
        SessionThreadRow::Plan { id, session_id, plan, decision, fresh, .. } => ChatRowInput::Card {
            id: id.clone(),
            icon: if decision.as_deref() == Some("approved") { "ui-check" } else { "coding-plan" }.into(),
            title: domain::plan_page::plan_row_label(plan),
            text: match decision {
                None => domain::plan_page::PLAN_PAGE.copy.needs_you.clone(),
                Some(decision) => domain::ask_dock::plan_decision_label(decision, *fresh),
            },
            target: Some(session_id.clone()),
        },
        SessionThreadRow::Subagent { id, title, status } => ChatRowInput::Line {
            id: id.clone(),
            icon: "coding-subagent".into(),
            text: [title.as_str(), status.as_str()].into_iter().filter(|p| !p.trim().is_empty()).collect::<Vec<_>>().join(" · "),
        },
        SessionThreadRow::Picture { id, attachment_id, caption, .. } => {
            ChatRowInput::Picture { id: id.clone(), attachment_id: attachment_id.clone(), caption: caption.clone() }
        }
        SessionThreadRow::Event { id, text, .. } => ChatRowInput::Line { id: id.clone(), icon: "ui-info".into(), text: text.clone() },
        SessionThreadRow::Divider { id, kind, at } => {
            let time = clock(Some(at));
            let label = if time.is_empty() { kind.label().to_string() } else { format!("{} · {time}", kind.label()) };
            ChatRowInput::Pill { id: id.clone(), icon: kind.icon().into(), label }
        }
        SessionThreadRow::Locked { id, reason, .. } => {
            let placeholder = &*domain::sealed::PLACEHOLDER;
            ChatRowInput::Pill { id: id.clone(), icon: placeholder.icon.clone(), label: placeholder.text(*reason).to_string() }
        }
        SessionThreadRow::Surface { id, session_id, title, .. } => ChatRowInput::Card {
            id: id.clone(),
            icon: "ui-widget".into(),
            title: title.clone().unwrap_or_else(|| domain::plan_card::PLAN_CARD.copy.surface.clone()),
            text: String::new(),
            target: Some(session_id.clone()),
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use domain::chats::DividerKind;

    fn message(id: &str, at: &str) -> MessageRow {
        MessageRow { id: id.into(), created_at: Some(at.into()), ..Default::default() }
    }

    #[test]
    fn a_person_s_message_carries_their_name_and_the_time() {
        let m = message("m1", "2026-11-08T09:14:00.000Z");
        let messages = HashMap::from([("m1", &m)]);
        let row = SessionThreadRow::Message { id: "m1".into(), author: "user".into(), author_id: Some("u1".into()), body: "hi".into(), queued: false, author_session_id: None, xref: None };
        let out = rows(&[row], &messages, |id| (id == Some("u1")).then(|| "Dana".to_string()));
        let ChatRowInput::Message { name, time, .. } = &out[0] else { panic!("{out:?}") };
        assert_eq!(name.as_deref(), Some("Dana"));
        assert_eq!(time, &clock(Some("2026-11-08T09:14:00.000Z")));
        assert_eq!(time.len(), 5, "HH:MM");
    }

    #[test]
    fn an_agent_has_no_name_and_a_divider_reads_its_label_and_time() {
        let agent = SessionThreadRow::Message { id: "m2".into(), author: "agent".into(), author_id: None, body: "done".into(), queued: false, author_session_id: None, xref: None };
        let divider = SessionThreadRow::Divider { id: "d1".into(), kind: DividerKind::Compact, at: "bad".into() };
        let out = rows(&[agent, divider], &HashMap::new(), |_| Some("never".into()));
        assert!(matches!(&out[0], ChatRowInput::Message { name: None, .. }));
        assert_eq!(out[1], ChatRowInput::Pill { id: "d1".into(), icon: DividerKind::Compact.icon().into(), label: DividerKind::Compact.label().into() });
    }

    #[test]
    fn a_question_waits_until_answered_or_dismissed_and_names_its_picks() {
        let q = |answered: bool, dismissed: Option<bool>, answers: Option<Vec<String>>| SessionThreadRow::Question {
            id: "q".into(),
            session_id: "s".into(),
            question: "Ship?".into(),
            options: vec![],
            custom: true,
            multi: true,
            surface: None,
            answer: Some("Yes".into()),
            answers,
            surface_data: None,
            answered,
            dismissed,
        };
        let waiting = |row| matches!(rows(&[row], &HashMap::new(), |_| None)[0], ChatRowInput::Question { waiting: true, .. });
        assert!(waiting(q(false, None, None)));
        assert!(!waiting(q(true, None, None)));
        assert!(!waiting(q(false, Some(true), None)));
        let ChatRowInput::Question { answer, .. } = &rows(&[q(true, None, Some(vec!["A".into(), "B".into()]))], &HashMap::new(), |_| None)[0] else { panic!() };
        assert_eq!(answer.as_deref(), Some("A, B"));
    }

    #[test]
    fn a_waiting_plan_needs_you_and_opens_its_session() {
        let plan = SessionThreadRow::Plan {
            id: "p".into(),
            session_id: "s".into(),
            plan: "# Lazy-load the editor\n\nsteps".into(),
            questions: vec![],
            surface: None,
            decision: None,
            fresh: false,
            answers: None,
            feedback: None,
            surface_data: None,
        };
        let ChatRowInput::Card { title, text, target, .. } = &rows(&[plan], &HashMap::new(), |_| None)[0] else { panic!() };
        assert_eq!(title, &domain::plan_page::plan_row_label("# Lazy-load the editor\n\nsteps"));
        assert_eq!(text, &domain::plan_page::PLAN_PAGE.copy.needs_you);
        assert_eq!(target.as_deref(), Some("s"));
    }
}
