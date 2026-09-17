//! EXP-876 — how a BATCH run is NAMED.
//!
//! Every batch row used to read "Batch run" — one string for every batch this
//! team ever ran, so two of them in a list (or one running beside last
//! night's) could not be told apart at all.
//!
//! A batch names itself after the issues it covers: the first one's identifier
//! with `+N` for the rest in the mono slot, that issue's title as the subject —
//! the same two-part row an issue run renders, so one layout keeps serving
//! every kind (EXP-874). The trailing control is still none: EXP-893 took the
//! open-issue circle off every session row on every client, and it was exactly
//! the control a multi-issue run could never answer.
//!
//! The covered set has two sources, in this order:
//!   1. `coding_sessions.batch_issue_ids` — written at start, so the name is
//!      right from the run's first second (the composer's order, preserved).
//!   2. the issues sharing the row's `branch` — what `pr_open` stamped on both
//!      sides (EXP-545), which names batches started before the column existed
//!      or by a client too old to send it, from the moment their PR opens.
//!
//! The twin of web `lib/batch-run.ts`, iOS `BatchRun` and Android
//! `BatchRun.kt`: same order, same `+N`, same fallback string, same test
//! names.

use serde_json::Value;

use crate::rows::{CodingSession, Issue};

/// The one string a batch with no knowable issues shows. Byte-identical ×4.
pub const BATCH_RUN_FALLBACK: &str = "Batch run";

/// The launcher's batch branch marker (`exp/batch-<id8>`), deliberately
/// lowercase so it can never parse as an issue branch.
pub const BATCH_BRANCH_PREFIX: &str = "exp/batch-";

/// Read `coding_sessions.batch_issue_ids`. Same tolerance as every other
/// jsonb column here (`session_results`): the store hands it over as TEXT, the
/// wire structured, anything that is not a list of non-empty strings is
/// dropped rather than blanking the name.
pub fn parse_batch_issue_ids(raw: Option<&Value>) -> Vec<String> {
    let Some(raw) = raw else {
        return Vec::new();
    };
    let reparsed;
    let items = match raw {
        Value::Array(items) => items,
        Value::String(text) => {
            reparsed = serde_json::from_str::<Value>(text).ok();
            match reparsed.as_ref() {
                Some(Value::Array(items)) => items,
                _ => return Vec::new(),
            }
        }
        _ => return Vec::new(),
    };
    items
        .iter()
        .filter_map(|item| item.as_str())
        .filter(|id| !id.is_empty())
        .map(str::to_string)
        .collect()
}

/// An issue-less, action-less run — the batch. (A chat run carries the
/// reserved `Chat` snapshot, an action run its own, EXP-615.)
pub fn is_batch_run(session: &CodingSession) -> bool {
    session.issue_id.is_none() && session.action_name.is_none()
}

/// The reserved action-name snapshot a CHAT run carries (EXP-615): the chat
/// builtin's own name, `api::actions::BUILTIN_CHAT_NAME`, which this crate
/// does not depend on.
pub const CHAT_RUN_NAME: &str = "Chat";

/// A chat run: issue-less, action-row-less, carrying the reserved `Chat`
/// snapshot (the server stamps it from the builtin, never client text).
pub fn is_chat_run(session: &CodingSession) -> bool {
    session.issue_id.is_none()
        && session.action_id.is_none()
        && session.action_name.as_deref().map(str::trim) == Some(CHAT_RUN_NAME)
}

/// EXP-908 — the subject of an issue-less ACTION-shaped run (chat or action):
/// a chat run reads the agent's auto-named `agent_title` when non-empty, else
/// `Chat`; an action run its name snapshot. `None` for an issue run, a batch,
/// or a blank snapshot. Byte-identical ×4 (web `pastRunTitle`).
pub fn action_run_subject(session: &CodingSession) -> Option<String> {
    if session.issue_id.is_some() {
        return None;
    }
    if is_chat_run(session) {
        let title = session.agent_title.as_deref().map(str::trim).unwrap_or("");
        return Some(if title.is_empty() {
            CHAT_RUN_NAME.to_string()
        } else {
            title.to_string()
        });
    }
    session
        .action_name
        .as_deref()
        .filter(|name| !name.trim().is_empty())
        .map(str::to_string)
}

/// The issues a batch run covers, in NAMING order: the stored order when the
/// row recorded it, else the branch-mates oldest first (a deterministic order
/// every client reaches the same way — `created_at` is on every issue row,
/// identifiers break the tie).
pub fn batch_run_issues<'a, I>(session: &CodingSession, issues: I) -> Vec<&'a Issue>
where
    I: IntoIterator<Item = &'a Issue>,
{
    if !is_batch_run(session) {
        return Vec::new();
    }
    let ids = parse_batch_issue_ids(session.batch_issue_ids.as_ref());
    let issues: Vec<&Issue> = issues.into_iter().collect();
    if !ids.is_empty() {
        return ids
            .iter()
            .filter_map(|id| issues.iter().copied().find(|issue| &issue.id == id))
            .collect();
    }
    let Some(branch) = session
        .branch
        .as_deref()
        .filter(|branch| branch.starts_with(BATCH_BRANCH_PREFIX))
    else {
        return Vec::new();
    };
    let mut covered: Vec<&Issue> = issues
        .into_iter()
        .filter(|issue| issue.branch.as_deref() == Some(branch))
        .collect();
    covered.sort_by(|a, b| {
        a.created_at
            .as_deref()
            .unwrap_or_default()
            .cmp(b.created_at.as_deref().unwrap_or_default())
            .then_with(|| a.identifier.cmp(&b.identifier))
    });
    covered
}

/// What a batch row shows: `EXP-874 +2` beside the first issue's title.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BatchRunName {
    /// The mono lead-in — `None` when no covered issue is known.
    pub identifier: Option<String>,
    pub subject: String,
}

/// Name a batch run. `issues` is whatever the caller has synced; only the
/// covered ones are read. A batch whose issues are all unknown (no stored ids,
/// no PR yet — or a row whose issues left the viewer's teams) keeps the old
/// generic label rather than inventing one.
pub fn batch_run_name<'a, I>(session: &CodingSession, issues: I) -> BatchRunName
where
    I: IntoIterator<Item = &'a Issue>,
{
    let covered = batch_run_issues(session, issues);
    let Some(first) = covered.first() else {
        return BatchRunName {
            identifier: None,
            subject: BATCH_RUN_FALLBACK.to_string(),
        };
    };
    // The STORED count wins over the resolved one: a batch of three whose
    // middle issue has not synced is still a batch of three, and "+1" would
    // quietly understate what the run is working on.
    let total = parse_batch_issue_ids(session.batch_issue_ids.as_ref())
        .len()
        .max(covered.len());
    let title = first.title.trim();
    BatchRunName {
        identifier: Some(if total > 1 {
            format!("{} +{}", first.identifier, total - 1)
        } else {
            first.identifier.clone()
        }),
        subject: if title.is_empty() {
            "Untitled issue".to_string()
        } else {
            title.to_string()
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// A `coding_sessions` row hydrated the way the store hands one over.
    fn session(over: Value) -> CodingSession {
        let mut row = json!({
            "id": "s-1",
            "team_id": "team-1",
            "user_id": "me",
            "status": "running",
        });
        let (Value::Object(base), Value::Object(extra)) = (&mut row, over) else {
            unreachable!("both are objects")
        };
        base.extend(extra);
        serde_json::from_value(row).unwrap()
    }

    fn issue(id: &str, identifier: &str, title: &str, created_at: &str) -> Issue {
        serde_json::from_value(json!({
            "id": id,
            "board_id": "board-1",
            "number": 1,
            "identifier": identifier,
            "title": title,
            "status": "in_progress",
            "branch": "exp/batch-1a2b3c4d",
            "created_at": created_at,
        }))
        .unwrap()
    }

    fn pool() -> Vec<Issue> {
        vec![
            issue(
                "i-3",
                "EXP-889",
                "Diff on the issue page",
                "2026-09-03T10:00:00Z",
            ),
            issue(
                "i-1",
                "EXP-874",
                "Session list fixes",
                "2026-09-01T10:00:00Z",
            ),
            issue("i-2", "EXP-876", "Batch run names", "2026-09-02T10:00:00Z"),
        ]
    }

    #[test]
    fn is_an_issue_less_action_less_row() {
        assert!(is_batch_run(&session(json!({}))));
        assert!(!is_batch_run(&session(json!({"issue_id": "i-1"}))));
        assert!(!is_batch_run(&session(json!({"action_name": "Chat"}))));
    }

    #[test]
    fn a_chat_run_is_named_after_its_agent_title() {
        let chat = |title: Value| session(json!({"action_name": "Chat", "agent_title": title}));
        assert_eq!(
            action_run_subject(&chat(json!("  Fix the login flow "))).as_deref(),
            Some("Fix the login flow")
        );
        assert_eq!(action_run_subject(&chat(json!("   "))).as_deref(), Some("Chat"));
        assert_eq!(action_run_subject(&chat(Value::Null)).as_deref(), Some("Chat"));
        // Action and issue runs never read the agent title.
        let action = session(json!({
            "action_id": "a-1", "action_name": "Daily digest", "agent_title": "Something"
        }));
        assert_eq!(action_run_subject(&action).as_deref(), Some("Daily digest"));
        let issue = session(json!({"issue_id": "i-1", "agent_title": "Something"}));
        assert_eq!(action_run_subject(&issue), None);
        // A batch has no action subject at all (it names itself after issues).
        assert_eq!(action_run_subject(&session(json!({"agent_title": "x"}))), None);
        assert!(is_chat_run(&session(json!({"action_name": "Chat"}))));
        assert!(!is_chat_run(&session(json!({"action_id": "a-1", "action_name": "Chat"}))));
    }

    #[test]
    fn batch_run_issues_keep_the_order_the_run_stored() {
        let row = session(json!({"batch_issue_ids": ["i-2", "i-1"]}));
        let issues = pool();
        let covered = batch_run_issues(&row, issues.iter());
        assert_eq!(
            covered.iter().map(|i| i.id.as_str()).collect::<Vec<_>>(),
            vec!["i-2", "i-1"]
        );
    }

    #[test]
    fn batch_run_issues_skip_an_id_that_has_not_synced() {
        let row = session(json!({"batch_issue_ids": ["gone", "i-1"]}));
        let issues = pool();
        let covered = batch_run_issues(&row, issues.iter());
        assert_eq!(
            covered.iter().map(|i| i.id.as_str()).collect::<Vec<_>>(),
            vec!["i-1"]
        );
    }

    #[test]
    fn batch_run_issues_fall_back_to_the_branch_oldest_first() {
        let row = session(json!({"branch": "exp/batch-1a2b3c4d"}));
        let issues = pool();
        let covered = batch_run_issues(&row, issues.iter());
        assert_eq!(
            covered.iter().map(|i| i.id.as_str()).collect::<Vec<_>>(),
            vec!["i-1", "i-2", "i-3"]
        );
    }

    #[test]
    fn batch_run_issues_never_match_a_branch_that_is_not_a_batchs() {
        let issues = pool();
        for branch in ["exp/chat-1a2b3c4d", "exp/EXP-874"] {
            let row = session(json!({"branch": branch}));
            assert!(batch_run_issues(&row, issues.iter()).is_empty());
        }
    }

    #[test]
    fn names_a_batch_after_its_covered_issues() {
        let issues = pool();
        let row = session(json!({"batch_issue_ids": ["i-1", "i-2", "i-3"]}));
        assert_eq!(
            batch_run_name(&row, issues.iter()),
            BatchRunName {
                identifier: Some("EXP-874 +2".into()),
                subject: "Session list fixes".into(),
            }
        );
    }

    #[test]
    fn drops_the_suffix_for_a_batch_of_one() {
        let issues = pool();
        let row = session(json!({"batch_issue_ids": ["i-2"]}));
        assert_eq!(
            batch_run_name(&row, issues.iter()),
            BatchRunName {
                identifier: Some("EXP-876".into()),
                subject: "Batch run names".into(),
            }
        );
    }

    #[test]
    fn counts_the_issues_the_run_stored_not_the_ones_that_synced() {
        let issues = pool();
        let row = session(json!({"batch_issue_ids": ["i-1", "gone", "i-3"]}));
        assert_eq!(
            batch_run_name(&row, issues.iter()).identifier,
            Some("EXP-874 +2".into())
        );
    }

    #[test]
    fn falls_back_to_the_branch() {
        let issues = pool();
        let row = session(json!({"branch": "exp/batch-1a2b3c4d"}));
        assert_eq!(
            batch_run_name(&row, issues.iter()),
            BatchRunName {
                identifier: Some("EXP-874 +2".into()),
                subject: "Session list fixes".into(),
            }
        );
    }

    #[test]
    fn falls_back_to_batch_run() {
        let issues = pool();
        assert_eq!(
            batch_run_name(&session(json!({})), issues.iter()),
            BatchRunName {
                identifier: None,
                subject: "Batch run".into(),
            }
        );
        let unknown = session(json!({"batch_issue_ids": ["gone"]}));
        assert_eq!(batch_run_name(&unknown, issues.iter()).identifier, None);
        let nothing_synced = session(json!({"batch_issue_ids": ["i-1"]}));
        assert_eq!(batch_run_name(&nothing_synced, []).identifier, None);
    }

    #[test]
    fn reads_the_column_however_the_store_hands_it_over() {
        // SQLite returns a jsonb column as TEXT; the wire delivers an array.
        assert_eq!(
            parse_batch_issue_ids(Some(&json!(r#"["i-1","i-2"]"#))),
            vec!["i-1".to_string(), "i-2".to_string()]
        );
        assert_eq!(
            parse_batch_issue_ids(Some(&json!(["i-1"]))),
            vec!["i-1".to_string()]
        );
        // Anything else names nothing rather than panicking.
        assert!(parse_batch_issue_ids(None).is_empty());
        assert!(parse_batch_issue_ids(Some(&json!(null))).is_empty());
        assert!(parse_batch_issue_ids(Some(&json!("not json"))).is_empty());
        assert!(parse_batch_issue_ids(Some(&json!([1, "", "i-1"]))) == vec!["i-1".to_string()]);
    }
}
