//! EXP-1294: the Threads v2 entries' shared demo kit, not an entry — the
//! contract fixtures' camelCase rows read as the synced-shape (snake_case)
//! `domain::rows` the painters take, their ISO clock as epoch ms, and a
//! demo pull-request stack.

use domain::pr_stack::PrSet;
use domain::rows::{Issue, MessageRow, PullRequestIssueRow, PullRequestRow};
use serde::de::DeserializeOwned;
use serde_json::{json, Value};

pub(crate) const SESSION_CARD_JSON: &str =
    include_str!("../../../../../../../packages/domain-contract/fixtures/session-card.json");
pub(crate) const SESSION_THREAD_JSON: &str =
    include_str!("../../../../../../../packages/domain-contract/fixtures/session-thread.json");
pub(crate) const THREADS_LIST_JSON: &str =
    include_str!("../../../../../../../packages/domain-contract/fixtures/threads-list.json");

/// `teamId` → `team_id`.
fn snake(key: &str) -> String {
    let mut out = String::with_capacity(key.len() + 4);
    for ch in key.chars() {
        if ch.is_ascii_uppercase() {
            out.push('_');
            out.push(ch.to_ascii_lowercase());
        } else {
            out.push(ch);
        }
    }
    out
}

/// One fixture row as its synced-shape type: top-level keys snake-cased
/// (jsonb values such as `card` and `guide` keep their own keys), `id` =
/// `fallback_id` when the fixture row has none (session-issue links).
fn row_or(value: &Value, fallback_id: &str) -> Value {
    let mut object: serde_json::Map<String, Value> = value
        .as_object()
        .map(|object| object.iter().map(|(key, value)| (snake(key), value.clone())).collect())
        .unwrap_or_default();
    object.entry("id").or_insert_with(|| json!(fallback_id));
    Value::Object(object)
}

/// One fixture row as its synced-shape type ([`row_or`] with no fallback id).
pub(crate) fn row<T: DeserializeOwned>(value: &Value) -> T {
    serde_json::from_value(row_or(value, "")).expect("a fixture row deserializes")
}

/// Every row of a fixture array (an id-less row reads `row-<index>`).
pub(crate) fn rows<T: DeserializeOwned>(value: &Value) -> Vec<T> {
    value
        .as_array()
        .map(|items| {
            items
                .iter()
                .enumerate()
                .map(|(index, item)| {
                    serde_json::from_value(row_or(item, &format!("row-{index}")))
                        .expect("a fixture row deserializes")
                })
                .collect()
        })
        .unwrap_or_default()
}

/// NEW-8: one fixture `TreeMessage` (already KEYED: its `sessionId` is the
/// session its conversation belongs to) as a keyed `MessageRow`: a
/// synthesized conversation, the resolved session set after decode (the
/// row's own `session_id` is never decoded).
pub(crate) fn message(value: &Value, fallback_id: &str) -> Option<MessageRow> {
    let session_id = value["sessionId"].as_str().unwrap_or_default().to_string();
    let mut object = row_or(value, fallback_id);
    if let Some(object) = object.as_object_mut() {
        object.entry("conversation_id").or_insert_with(|| json!(format!("c-{session_id}")));
    }
    let message: MessageRow = serde_json::from_value(object).ok()?;
    Some(MessageRow { session_id, ..message })
}

/// Every fixture message ([`message`]; an id-less row reads `row-<index>`).
pub(crate) fn messages(value: &Value) -> Vec<MessageRow> {
    value
        .as_array()
        .map(|items| {
            items
                .iter()
                .enumerate()
                .map(|(index, item)| message(item, &format!("row-{index}")).expect("a fixture message deserializes"))
                .collect()
        })
        .unwrap_or_default()
}

/// A fixture `now` as epoch ms.
pub(crate) fn now_ms(value: &Value) -> i64 {
    value
        .as_str()
        .and_then(|iso| chrono::DateTime::parse_from_rfc3339(iso).ok())
        .map(|at| at.timestamp_millis())
        .unwrap_or(0)
}

/// A demo pull-request stack: synced-shape `pull_requests`, their issue
/// links and the issues, as one `domain::pr_stack::PrSet`.
pub(crate) struct PrDemo {
    pub(crate) pulls: Vec<PullRequestRow>,
    pub(crate) links: Vec<PullRequestIssueRow>,
    pub(crate) issues: Vec<Issue>,
}

impl PrDemo {
    /// One open pull request per `(identifier, title, number, base)`, on
    /// `exp/<identifier>` in one repository, closing the issue of that
    /// identifier (id `sg-<identifier>`; its pull request `sg-pr-<identifier>`).
    pub(crate) fn open(members: &[(&str, &str, i64, &str)]) -> Self {
        let mut demo = Self { pulls: Vec::new(), links: Vec::new(), issues: Vec::new() };
        for (identifier, title, number, base) in members {
            demo.issues.push(row(&json!({
                "id": format!("sg-{identifier}"),
                "boardId": "sg-board",
                "number": number,
                "identifier": identifier,
                "title": title,
                "status": "in_progress",
            })));
            demo.pulls.push(row(&json!({
                "id": format!("sg-pr-{identifier}"),
                "repositoryId": "sg-repo",
                "number": number,
                "url": format!("https://github.com/o/r/pull/{number}"),
                "title": title,
                "head": format!("exp/{identifier}"),
                "base": base,
                "state": "open",
            })));
            demo.links.push(row(&json!({
                "id": format!("sg-link-{identifier}"),
                "pullRequestId": format!("sg-pr-{identifier}"),
                "issueId": format!("sg-{identifier}"),
            })));
        }
        demo
    }

    /// The pull request of `identifier`.
    pub(crate) fn pull(&self, identifier: &str) -> Option<&PullRequestRow> {
        let id = format!("sg-pr-{identifier}");
        self.pulls.iter().find(|pull| pull.id == id)
    }

    /// `f` over the demo's `PrSet`.
    pub(crate) fn with_set<R>(&self, f: impl FnOnce(&PrSet<'_>) -> R) -> R {
        let pulls: Vec<&PullRequestRow> = self.pulls.iter().collect();
        let links: Vec<&PullRequestIssueRow> = self.links.iter().collect();
        let issues: Vec<&Issue> = self.issues.iter().collect();
        f(&PrSet { pulls: &pulls, links: &links, issues: &issues })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn camel_keys_read_as_snake_case_rows() {
        assert_eq!(snake("needsInput"), "needs_input");
        let session: domain::rows::SessionRow = row(&serde_json::json!({
            "id": "s1", "rootId": "s1", "needsInput": true, "startedAt": "2026-10-13T10:00:00.000Z",
        }));
        assert_eq!(session.root_id.as_deref(), Some("s1"));
        assert!(session.needs_input);
        assert_eq!(now_ms(&serde_json::json!("1970-01-01T00:00:01.000Z")), 1000);
    }
}
