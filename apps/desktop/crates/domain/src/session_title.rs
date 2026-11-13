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

use std::borrow::Cow;
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

/// The composer's positional image markers (EXP-698), with the spaces and
/// tabs around a run of them: one space in a title line.
fn image_markers() -> &'static regex::Regex {
    static MARKERS: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    MARKERS.get_or_init(|| regex::Regex::new(r"[ \t]*(?:\[Image #[0-9]+\][ \t]*)+").unwrap())
}

/// One line as a title reads it: trimmed, `[Image #N]` markers removed.
pub fn title_line(line: &str) -> Cow<'_, str> {
    let line = line.trim();
    match image_markers().replace_all(line, " ") {
        Cow::Borrowed(line) => Cow::Borrowed(line),
        Cow::Owned(stripped) => Cow::Owned(stripped.trim().to_string()),
    }
}

/// The first usable line of a body: non-blank once its image markers are
/// gone, not an image embed.
pub fn first_line(body: &str) -> Option<Cow<'_, str>> {
    body.split('\n')
        .map(title_line)
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
        Some(line) => clip(&line, TITLE_MAX),
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
    let own: Vec<&SessionIssueRow> = links
        .iter()
        .copied()
        .filter(|link| link.session_id == session.id)
        .collect();
    resolve_links(own, &by_id)
}

fn resolve_links<'i>(mut own: Vec<&SessionIssueRow>, by_id: &HashMap<&str, &'i TreeIssue>) -> Vec<&'i TreeIssue> {
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

/// The first prompt of `session_id` among `messages`: the oldest usable
/// input (a person's, or another session's), never a card.
pub fn first_prompt<'m>(session_id: &str, messages: &[&'m MessageRow]) -> Option<Cow<'m, str>> {
    let inputs = messages.iter().copied().filter(|message| {
        message.session_id == session_id
            && no_card(message)
            && (message.author_kind.as_deref() == Some(MESSAGE_AUTHOR_KIND_USER)
                || message
                    .author_session_id
                    .as_deref()
                    .is_some_and(|author| !author.is_empty() && author != session_id))
    });
    let at = |message: &MessageRow| {
        message
            .created_at
            .as_deref()
            .and_then(parse_instant)
            .unwrap_or(0)
    };
    // #LOGS-7: the oldest usable one in one pass (each timestamp parsed
    // once), not a sort of every input.
    inputs
        .filter_map(|message| Some((at(message), message.id.as_str(), first_line(message.body_text())?)))
        .min_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(b.1)))
        .map(|(_, _, line)| line)
}

/// THE display title (fixture `session-title.json` rungs).
pub fn session_title(
    session: &SessionRow,
    links: &[&SessionIssueRow],
    issues: &[TreeIssue],
    messages: &[&MessageRow],
) -> String {
    TitleIndex::new(links, issues, messages).title(session)
}

/// Every given session's title, keyed by id (the tree's card names).
pub fn session_titles(
    sessions: &[&SessionRow],
    links: &[&SessionIssueRow],
    issues: &[TreeIssue],
    messages: &[&MessageRow],
) -> HashMap<String, String> {
    let index = TitleIndex::new(links, issues, messages);
    sessions
        .iter()
        .map(|session| (session.id.clone(), index.title(session)))
        .collect()
}

/// #LOGS-3: [`session_title`] for many sessions over the same rows: the
/// links, issues and messages grouped ONCE (a per-session scan of each made
/// a large team's titles O(sessions × rows), on every synced change).
pub struct TitleIndex<'a> {
    issues: HashMap<&'a str, &'a TreeIssue>,
    links: HashMap<&'a str, Vec<&'a SessionIssueRow>>,
    messages: HashMap<&'a str, Vec<&'a MessageRow>>,
}

impl<'a> TitleIndex<'a> {
    pub fn new(links: &[&'a SessionIssueRow], issues: &'a [TreeIssue], messages: &[&'a MessageRow]) -> Self {
        let mut by_session: HashMap<&'a str, Vec<&'a SessionIssueRow>> = HashMap::new();
        for link in links {
            by_session.entry(link.session_id.as_str()).or_default().push(link);
        }
        let mut own: HashMap<&'a str, Vec<&'a MessageRow>> = HashMap::new();
        for message in messages {
            own.entry(message.session_id.as_str()).or_default().push(message);
        }
        Self {
            issues: issues.iter().map(|issue| (issue.id.as_str(), issue)).collect(),
            links: by_session,
            messages: own,
        }
    }

    /// [`session_title`] of `session`.
    pub fn title(&self, session: &SessionRow) -> String {
        let own_links = self.links.get(session.id.as_str()).cloned().unwrap_or_default();
        let own: Vec<TitleIssue<'_>> = resolve_links(own_links, &self.issues)
            .into_iter()
            .map(|issue| TitleIssue { identifier: &issue.identifier, title: &issue.title })
            .collect();
        let needs_prompt = clean(session.title.as_deref()).is_none() && own.is_empty();
        let prompt = if needs_prompt {
            self.messages.get(session.id.as_str()).and_then(|messages| first_prompt(&session.id, messages))
        } else {
            None
        };
        title_from_parts(session.title.as_deref(), &own, prompt.as_deref())
    }
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
                    // NEW-8: a fixture `TitleMessage` is already keyed: its
                    // `sessionId` is the RESOLVED session.
                    let mut message: MessageRow = serde_json::from_value(json!({
                        "id": m["id"],
                        "conversation_id": format!("c-{}", m["sessionId"].as_str().unwrap_or_default()),
                        "author_kind": m["authorKind"],
                        "author_session_id": m["authorSessionId"],
                        "body": m["body"],
                        "card": m["card"],
                        "created_at": m["createdAt"],
                    }))
                    .unwrap();
                    message.session_id = m["sessionId"].as_str().unwrap_or_default().to_string();
                    message
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
