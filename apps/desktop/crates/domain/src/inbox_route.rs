//! #EXP-1456: where an inbox row (and its toast) opens ×4. An issue row opens
//! its issue; an issue-less row naming a session opens that session (an
//! agent's question or plan, a mention, a wall); [`session_target`] is the
//! session entity ref's rule. Byte-locked by
//! `packages/domain-contract/fixtures/inbox-route.json`; mirrors TS
//! `src/inbox-route.ts`, iOS `InboxRoute.swift`, Android `InboxRoute.kt`.

use crate::contract::{NOTIFICATION_TYPE_AGENT_MESSAGE, NOTIFICATION_TYPE_SESSION_BLOCKED};

/// Which face an issue row opens on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IssueFace {
    /// EXP-933: an agent's message about an issue opens its Results face.
    Results,
}

/// Where an inbox row opens.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InboxRowTarget {
    Issue { issue_id: String, face: Option<IssueFace> },
    Session { session_id: String },
    /// Nowhere: the tap only marks the row read.
    None,
}

fn non_empty(value: Option<&str>) -> Option<&str> {
    value.filter(|id| !id.is_empty())
}

/// The row's target, read off its `type`, `issue_id` and `session_id`.
pub fn inbox_row_target(kind: Option<&str>, issue_id: Option<&str>, session_id: Option<&str>) -> InboxRowTarget {
    let agent_message = kind == Some(NOTIFICATION_TYPE_AGENT_MESSAGE);
    if let Some(issue_id) = non_empty(issue_id) {
        return InboxRowTarget::Issue {
            issue_id: issue_id.to_string(),
            face: agent_message.then_some(IssueFace::Results),
        };
    }
    let session_row = agent_message || kind == Some(NOTIFICATION_TYPE_SESSION_BLOCKED);
    match non_empty(session_id) {
        Some(session_id) if session_row => InboxRowTarget::Session {
            session_id: session_id.to_string(),
        },
        _ => InboxRowTarget::None,
    }
}

/// Where a session opens (the S-3 inter-session rule, web `lib/session-back.ts`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SessionTarget {
    /// The viewer's OWN agent conversation root: the agent's page (its channel).
    Agent { agent_id: String },
    /// A thread root (`session_id` None) or a thread's child, inside the thread.
    Thread { thread_id: String, session_id: Option<String> },
    /// Anything else under an agent (a teammate's conversation, a child of a
    /// conversation): the session on its own screen.
    Session { session_id: String },
}

/// The session's target for `viewer_id`. `agent_id` and `user_id` are the
/// ROOT's (a child carries no agent: it resolves through its root); a NULL
/// `root_id` = the row is its own root.
pub fn session_target(
    id: &str,
    root_id: Option<&str>,
    agent_id: Option<&str>,
    user_id: Option<&str>,
    viewer_id: Option<&str>,
) -> SessionTarget {
    let root_id = non_empty(root_id).unwrap_or(id);
    let is_root = root_id == id;
    if let Some(agent_id) = non_empty(agent_id) {
        let own = user_id.is_some() && user_id == viewer_id;
        return if is_root && own {
            SessionTarget::Agent {
                agent_id: agent_id.to_string(),
            }
        } else {
            SessionTarget::Session {
                session_id: id.to_string(),
            }
        };
    }
    SessionTarget::Thread {
        thread_id: root_id.to_string(),
        session_id: (!is_root).then(|| id.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    const FIXTURE: &str =
        include_str!("../../../../../packages/domain-contract/fixtures/inbox-route.json");

    fn expected_row(value: &Value) -> InboxRowTarget {
        let s = |key: &str| value[key].as_str().unwrap().to_string();
        match value["kind"].as_str().unwrap() {
            "issue" => InboxRowTarget::Issue {
                issue_id: s("issueId"),
                face: match value["face"].as_str() {
                    Some("results") => Some(IssueFace::Results),
                    None => None,
                    Some(other) => panic!("unknown face {other}"),
                },
            },
            "session" => InboxRowTarget::Session { session_id: s("sessionId") },
            "none" => InboxRowTarget::None,
            other => panic!("unknown kind {other}"),
        }
    }

    fn expected_session(value: &Value) -> SessionTarget {
        let s = |key: &str| value[key].as_str().unwrap().to_string();
        match value["kind"].as_str().unwrap() {
            "agent" => SessionTarget::Agent { agent_id: s("agentId") },
            "thread" => SessionTarget::Thread {
                thread_id: s("threadId"),
                session_id: value["sessionId"].as_str().map(str::to_string),
            },
            "session" => SessionTarget::Session { session_id: s("sessionId") },
            other => panic!("unknown kind {other}"),
        }
    }

    #[test]
    fn every_row_case_matches() {
        let fixture: Value = serde_json::from_str(FIXTURE).unwrap();
        let cases = fixture["cases"].as_array().unwrap();
        assert!(!cases.is_empty());
        for case in cases {
            let row = &case["row"];
            let got = inbox_row_target(
                row["type"].as_str(),
                row["issueId"].as_str(),
                row["sessionId"].as_str(),
            );
            assert_eq!(got, expected_row(&case["expected"]), "{}", case["name"]);
        }
    }

    #[test]
    fn every_session_case_matches() {
        let fixture: Value = serde_json::from_str(FIXTURE).unwrap();
        let cases = fixture["sessionCases"].as_array().unwrap();
        assert!(!cases.is_empty());
        for case in cases {
            let session = &case["session"];
            let got = session_target(
                session["id"].as_str().unwrap(),
                session["rootId"].as_str(),
                session["agentId"].as_str(),
                session["userId"].as_str(),
                case["viewerId"].as_str(),
            );
            assert_eq!(got, expected_session(&case["expected"]), "{}", case["name"]);
        }
    }
}
