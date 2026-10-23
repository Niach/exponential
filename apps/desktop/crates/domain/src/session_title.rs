//! EXP-1341/NEW-1: THE display title of any session ×4 (never `Session N`) — the
//! Rust twin of `packages/domain-contract/src/session-title.ts` (iOS ExpCore
//! `SessionTitle.swift`, Android `domain/SessionTitle.kt`), byte-locked by
//! `fixtures/session-title.json`. The first rung that yields text wins:
//!
//! 1. the explicit `title` (a name passed to `sessions_send`, else the
//!    agent CLI's own auto-title);
//! 2. the session's issues = its OWN rows (`session_id` = the holder, root
//!    or child alike, NEW-1), `#APP-1 Title` / `… + n`;
//! 3. the first prompt's first usable line (a person's or another
//!    session's input, never a card, never an image embed);
//! 4. `Untitled`.
//!
//! [`session_title`] reads the synced rows; [`title_from_parts`] is the same
//! ladder for callers that only hold the parts (the launcher's tab label).

use std::collections::HashMap;

use crate::activity_fold::parse_instant;
use crate::contract::MESSAGE_AUTHOR_KIND_USER;
use crate::rows::{MessageRow, SessionIssueRow, SessionRow};
use crate::session_tree::TreeIssue;

/// `session-title.json` copy (the constants test locks them).
pub mod copy {
    /// One issue: `{identifier}` / `{title}` slots.
    pub const ISSUE: &str = "#{identifier} {title}";
    /// Several issues: plus `{more}` = n - 1.
    pub const ISSUES: &str = "#{identifier} {title} + {more}";
    /// Nothing at all.
    pub const UNTITLED: &str = "Untitled";
}

/// A derived title (the {title} part of an issue label, a first prompt) is
/// cut to this many characters, ending in `…`.
pub const TITLE_MAX: usize = 80;

/// An issue as the title needs it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TitleIssue<'a> {
    pub identifier: &'a str,
    pub title: &'a str,
}

fn clean(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|value| !value.is_empty())
}

/// Cut to `max` characters, ending in `…`.
fn clip(text: &str, max: usize) -> String {
    if text.chars().count() > max {
        let head: String = text.chars().take(max.saturating_sub(1)).collect();
        format!("{}…", head.trim_end())
    } else {
        text.to_string()
    }
}

/// The first usable line of a body: non-blank after trimming, not an image
/// embed.
pub fn first_line(body: &str) -> Option<&str> {
    body.split('\n')
        .map(str::trim)
        .find(|part| !part.is_empty() && !part.starts_with("!["))
}

fn issue_label(issues: &[TitleIssue<'_>]) -> String {
    let first = issues[0];
    let template = if issues.len() > 1 { copy::ISSUES } else { copy::ISSUE }
        .replacen("{identifier}", first.identifier, 1)
        .replacen("{more}", &(issues.len() - 1).to_string(), 1);
    let room = TITLE_MAX.saturating_sub(template.replacen("{title}", "", 1).chars().count());
    template.replacen("{title}", &clip(first.title.trim(), room.max(1)), 1)
}

/// The ladder over its parts: `issues` = the session's own issues in link
/// order, `prompt` = its first prompt (any body; its first usable line is
/// taken).
pub fn title_from_parts(
    title: Option<&str>,
    issues: &[TitleIssue<'_>],
    prompt: Option<&str>,
) -> String {
    if let Some(named) = clean(title) {
        return named.to_string();
    }
    if !issues.is_empty() {
        return issue_label(issues);
    }
    match prompt.and_then(first_line) {
        Some(line) => clip(line, TITLE_MAX),
        None => copy::UNTITLED.to_string(),
    }
}

/// The session's OWN rows (`session_id == id`, root or child alike, NEW-1;
/// the legacy `assigned_session_id` is ignored), by sort order then issue
/// id, resolved through `issues` (unknown ids skipped).
pub fn session_issues<'i>(
    session: &SessionRow,
    links: &[&SessionIssueRow],
    issues: &'i [TreeIssue],
) -> Vec<&'i TreeIssue> {
    let by_id: HashMap<&str, &TreeIssue> =
        issues.iter().map(|issue| (issue.id.as_str(), issue)).collect();
    let mut own: Vec<&SessionIssueRow> = links
        .iter()
        .copied()
        .filter(|link| link.session_id == session.id)
        .collect();
    own.sort_by(|a, b| {
        a.sort_order
            .unwrap_or(0.0)
            .partial_cmp(&b.sort_order.unwrap_or(0.0))
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.issue_id.cmp(&b.issue_id))
    });
    own.into_iter()
        .filter_map(|link| by_id.get(link.issue_id.as_str()).copied())
        .collect()
}

fn no_card(message: &MessageRow) -> bool {
    matches!(message.card, None | Some(serde_json::Value::Null))
}

/// The first prompt's usable line: a person's text or another session's
/// input, oldest first (`created_at`, then `id`).
pub fn first_prompt<'m>(session_id: &str, messages: &[&'m MessageRow]) -> Option<&'m str> {
    let mut inputs: Vec<&MessageRow> = messages
        .iter()
        .copied()
        .filter(|message| {
            message.session_id == session_id
                && no_card(message)
                && (message.author_kind.as_deref() == Some(MESSAGE_AUTHOR_KIND_USER)
                    || message
                        .author_session_id
                        .as_deref()
                        .is_some_and(|author| !author.is_empty() && author != session_id))
        })
        .collect();
    let at = |message: &MessageRow| {
        message
            .created_at
            .as_deref()
            .and_then(parse_instant)
            .unwrap_or(0)
    };
    inputs.sort_by(|a, b| at(a).cmp(&at(b)).then_with(|| a.id.cmp(&b.id)));
    inputs.into_iter().find_map(|message| first_line(message.body_text()))
}

/// THE display title (fixture `session-title.json` rungs).
pub fn session_title(
    session: &SessionRow,
    links: &[&SessionIssueRow],
    issues: &[TreeIssue],
    messages: &[&MessageRow],
) -> String {
    let own: Vec<TitleIssue<'_>> = session_issues(session, links, issues)
        .into_iter()
        .map(|issue| TitleIssue { identifier: &issue.identifier, title: &issue.title })
        .collect();
    let needs_prompt = clean(session.title.as_deref()).is_none() && own.is_empty();
    let prompt = if needs_prompt { first_prompt(&session.id, messages) } else { None };
    title_from_parts(session.title.as_deref(), &own, prompt)
}

/// Every given session's title, keyed by id (the tree's card names).
pub fn session_titles(
    sessions: &[&SessionRow],
    links: &[&SessionIssueRow],
    issues: &[TreeIssue],
    messages: &[&MessageRow],
) -> HashMap<String, String> {
    sessions
        .iter()
        .map(|session| (session.id.clone(), session_title(session, links, issues, messages)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};

    const FIXTURE: &str =
        include_str!("../../../../../packages/domain-contract/fixtures/session-title.json");

    fn str_of(value: &Value, key: &str) -> Option<String> {
        value.get(key).and_then(Value::as_str).map(str::to_string)
    }

    #[test]
    fn every_fixture_case_matches() {
        let fixture: Value = serde_json::from_str(FIXTURE).unwrap();
        let cases = fixture["cases"].as_array().unwrap();
        assert!(!cases.is_empty());
        for case in cases {
            let name = case["name"].as_str().unwrap();
            let input = &case["input"];
            let s = &input["session"];
            let session = SessionRow {
                id: str_of(s, "id").unwrap(),
                root_id: str_of(s, "rootId"),
                title: str_of(s, "title"),
                ..Default::default()
            };
            let links: Vec<SessionIssueRow> = input["links"]
                .as_array()
                .unwrap()
                .iter()
                .enumerate()
                .map(|(index, link)| {
                    serde_json::from_value(json!({
                        "id": format!("link-{index}"),
                        "session_id": link["sessionId"],
                        "issue_id": link["issueId"],
                        "sort_order": link["sortOrder"],
                    }))
                    .unwrap()
                })
                .collect();
            let issues: Vec<TreeIssue> = serde_json::from_value(input["issues"].clone()).unwrap();
            let messages: Vec<MessageRow> = input["messages"]
                .as_array()
                .unwrap()
                .iter()
                .map(|m| {
                    serde_json::from_value(json!({
                        "id": m["id"],
                        "session_id": m["sessionId"],
                        "author_kind": m["authorKind"],
                        "author_session_id": m["authorSessionId"],
                        "body": m["body"],
                        "card": m["card"],
                        "created_at": m["createdAt"],
                    }))
                    .unwrap()
                })
                .collect();
            let link_refs: Vec<&SessionIssueRow> = links.iter().collect();
            let message_refs: Vec<&MessageRow> = messages.iter().collect();
            assert_eq!(
                session_title(&session, &link_refs, &issues, &message_refs),
                case["expected"].as_str().unwrap(),
                "{name}"
            );
        }
    }

    #[test]
    fn the_constants_match_the_fixture() {
        let fixture: Value = serde_json::from_str(FIXTURE).unwrap();
        assert_eq!(fixture["titles"]["issue"], copy::ISSUE);
        assert_eq!(fixture["titles"]["issues"], copy::ISSUES);
        assert!(fixture["titles"].get("action").is_none());
        assert_eq!(fixture["untitled"], copy::UNTITLED);
        assert_eq!(fixture["titleMax"].as_u64().unwrap() as usize, TITLE_MAX);
    }

    #[test]
    fn parts_follow_the_same_ladder() {
        let issue = TitleIssue { identifier: "APP-1", title: "Crash" };
        assert_eq!(title_from_parts(Some(" Bot "), &[issue], None), "Bot");
        assert_eq!(title_from_parts(None, &[issue], Some("hi")), "#APP-1 Crash");
        assert_eq!(title_from_parts(None, &[], Some("\n![a](b)\n do it ")), "do it");
        assert_eq!(title_from_parts(Some(" "), &[], None), "Untitled");
    }
}
