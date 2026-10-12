//! Typed `threads.*` tRPC helpers (EXP-1284 threads). Mirrors
//! `apps/web/src/lib/trpc/threads.ts`; every procedure goes through
//! `resolveTeamAccess`:
//!
//! - `threads.get({id})` → `{thread, items}` — a member read (the one-shot
//!   fetch a view takes before the synced rows land; lists read the
//!   `threads`/`thread_items` collections, never this).
//! - `threads.create({teamId, title?, issueIds?})` → `{thread, txId}`.
//! - `threads.update({id, title})` → `{thread, txId}` — `title` `null` (or
//!   blank) CLEARS it back to the derived title, so the key is always sent.
//! - `threads.link({id, issueId? | sessionId?})` → `{added, txId}` — exactly
//!   one target ([`ThreadTarget`]). Linking a RUN moves it here (a run sits
//!   in exactly one thread); only its owner or its host may.
//! - `threads.unlink({id, issueId? | sessionId?})` → `{removed, txId,
//!   newThreadId?}` — unlinking a RUN moves it into a fresh thread of its
//!   own, whose id `newThreadId` names.
//!
//! The rows re-stream over the two shapes, so callers fire-and-forget and
//! let the collections settle. `txId` also decodes as `txid`.

use serde::{Deserialize, Serialize};

use crate::error::ApiError;
use crate::trpc::TrpcClient;

/// What a link/unlink names: an issue or a run (`coding_sessions` row) —
/// the server refuses both or neither, so the type rules that out.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ThreadTarget<'a> {
    Issue(&'a str),
    Run(&'a str),
}

/// A `threads` row as tRPC returns it (camelCase). Decoded loosely: only the
/// id is required, so a server-side column addition never fails a call.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Thread {
    pub id: String,
    #[serde(default)]
    pub team_id: Option<String>,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub created_by: Option<String>,
    /// THE thread Guide (`CodingSessionResult[]`); read it through
    /// `domain::session_results::parse_session_results`.
    #[serde(default)]
    pub results: Option<serde_json::Value>,
    #[serde(default)]
    pub created_at: Option<String>,
    #[serde(default)]
    pub updated_at: Option<String>,
}

/// A `thread_items` row as tRPC returns it (camelCase): `kind` issue | run
/// with exactly one of `issue_id`/`session_id` set.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThreadItem {
    pub id: String,
    #[serde(default)]
    pub thread_id: Option<String>,
    #[serde(default)]
    pub kind: Option<String>,
    #[serde(default)]
    pub issue_id: Option<String>,
    #[serde(default)]
    pub session_id: Option<String>,
    #[serde(default)]
    pub team_id: Option<String>,
    #[serde(default)]
    pub board_id: Option<String>,
    #[serde(default)]
    pub added_by: Option<String>,
    #[serde(default)]
    pub added_by_session_id: Option<String>,
    #[serde(default)]
    pub source: Option<String>,
    #[serde(default)]
    pub sort_order: Option<f64>,
    #[serde(default)]
    pub created_at: Option<String>,
}

/// `threads.get` output.
#[derive(Clone, Debug, Deserialize)]
pub struct ThreadDetail {
    pub thread: Thread,
    #[serde(default)]
    pub items: Vec<ThreadItem>,
}

/// `threads.create` / `threads.update` output.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThreadWriteOutput {
    pub thread: Thread,
    #[serde(default, alias = "txid")]
    pub tx_id: Option<i64>,
}

/// `threads.link` output.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThreadLinkOutput {
    /// `false` when the target already sat in this thread (never an error).
    #[serde(default)]
    pub added: bool,
    #[serde(default, alias = "txid")]
    pub tx_id: Option<i64>,
}

/// `threads.unlink` output.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThreadUnlinkOutput {
    /// `false` when the target was not in this thread.
    #[serde(default)]
    pub removed: bool,
    #[serde(default, alias = "txid")]
    pub tx_id: Option<i64>,
    /// An unlinked RUN's fresh thread of its own; `None` for an issue.
    #[serde(default)]
    pub new_thread_id: Option<String>,
}

/// The shared `{id, issueId? | sessionId?}` input of link/unlink.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct TargetInput<'a> {
    id: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    issue_id: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    session_id: Option<&'a str>,
}

impl<'a> TargetInput<'a> {
    fn new(id: &'a str, target: ThreadTarget<'a>) -> Self {
        let (issue_id, session_id) = match target {
            ThreadTarget::Issue(issue_id) => (Some(issue_id), None),
            ThreadTarget::Run(session_id) => (None, Some(session_id)),
        };
        Self {
            id,
            issue_id,
            session_id,
        }
    }
}

/// `threads.get` — query. Blocking; background executor only (§3.5).
pub fn threads_get(trpc: &TrpcClient, id: &str) -> Result<ThreadDetail, ApiError> {
    #[derive(Serialize)]
    struct Input<'a> {
        id: &'a str,
    }
    trpc.query_with_input("threads.get", &Input { id })
}

/// `threads.create` — mutation. `title` `None` = derived; `issue_ids` empty
/// = an empty thread (the key is then omitted).
pub fn threads_create(
    trpc: &TrpcClient,
    team_id: &str,
    title: Option<&str>,
    issue_ids: &[String],
) -> Result<ThreadWriteOutput, ApiError> {
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Input<'a> {
        team_id: &'a str,
        #[serde(skip_serializing_if = "Option::is_none")]
        title: Option<&'a str>,
        #[serde(skip_serializing_if = "<[String]>::is_empty")]
        issue_ids: &'a [String],
    }
    trpc.mutation(
        "threads.create",
        &Input {
            team_id,
            title,
            issue_ids,
        },
    )
}

/// `threads.update` — mutation (rename). `title` `None` sends an explicit
/// `null`: the thread goes back to its derived title.
pub fn threads_update(
    trpc: &TrpcClient,
    id: &str,
    title: Option<&str>,
) -> Result<ThreadWriteOutput, ApiError> {
    #[derive(Serialize)]
    struct Input<'a> {
        id: &'a str,
        title: Option<&'a str>,
    }
    trpc.mutation("threads.update", &Input { id, title })
}

/// `threads.link` — mutation. A run target MOVES the run into this thread.
pub fn threads_link(
    trpc: &TrpcClient,
    id: &str,
    target: ThreadTarget<'_>,
) -> Result<ThreadLinkOutput, ApiError> {
    trpc.mutation("threads.link", &TargetInput::new(id, target))
}

/// `threads.unlink` — mutation. A run target moves the run into a fresh
/// thread of its own ([`ThreadUnlinkOutput::new_thread_id`]).
pub fn threads_unlink(
    trpc: &TrpcClient,
    id: &str,
    target: ThreadTarget<'_>,
) -> Result<ThreadUnlinkOutput, ApiError> {
    trpc.mutation("threads.unlink", &TargetInput::new(id, target))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::trpc::tests::one_shot_server;
    use crate::StaticToken;
    use std::sync::Arc;
    use std::time::Duration;

    fn client(base: &str) -> TrpcClient {
        TrpcClient::new(base, Arc::new(StaticToken("tok".to_string())))
    }

    #[test]
    fn get_queries_the_id_and_decodes_thread_and_items() {
        let (base, captured) = one_shot_server(
            200,
            r#"{"result":{"data":{"thread":{"id":"th-1","teamId":"t-1","title":null,
                "results":[{"topic":"Summary","text":"Done."}],"futureField":1},
              "items":[
                {"id":"ti-1","threadId":"th-1","kind":"issue","issueId":"i-1","sessionId":null,"source":"user","sortOrder":1},
                {"id":"ti-2","threadId":"th-1","kind":"run","issueId":null,"sessionId":"s-1","source":"agent","addedBySessionId":"s-0","sortOrder":2}
              ]}}}"#,
        );
        let detail = threads_get(&client(&base), "th-1").unwrap();
        assert_eq!(detail.thread.id, "th-1");
        assert_eq!(detail.thread.team_id.as_deref(), Some("t-1"));
        assert_eq!(detail.thread.title, None);
        assert!(detail.thread.results.is_some());
        assert_eq!(detail.items.len(), 2);
        assert_eq!(detail.items[0].issue_id.as_deref(), Some("i-1"));
        assert_eq!(detail.items[1].session_id.as_deref(), Some("s-1"));
        assert_eq!(detail.items[1].added_by_session_id.as_deref(), Some("s-0"));
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(
            request.starts_with(
                "GET /api/trpc/threads.get?input=%7B%22id%22%3A%22th-1%22%7D HTTP/1.1"
            ),
            "{request}"
        );
    }

    #[test]
    fn create_omits_unset_title_and_empty_issue_ids() {
        let (base, captured) =
            one_shot_server(200, r#"{"result":{"data":{"thread":{"id":"th-1"},"txId":7}}}"#);
        let out = threads_create(&client(&base), "t-1", None, &[]).unwrap();
        assert_eq!(out.thread.id, "th-1");
        assert_eq!(out.tx_id, Some(7));
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(request.starts_with("POST /api/trpc/threads.create HTTP/1.1"));
        assert!(request.ends_with(r#"{"teamId":"t-1"}"#), "{request}");
    }

    #[test]
    fn create_posts_title_and_issue_ids() {
        let (base, captured) =
            one_shot_server(200, r#"{"result":{"data":{"thread":{"id":"th-1"},"txid":8}}}"#);
        let out = threads_create(
            &client(&base),
            "t-1",
            Some("Ship it"),
            &["i-1".to_string(), "i-2".to_string()],
        )
        .unwrap();
        // The lower-case spelling decodes too.
        assert_eq!(out.tx_id, Some(8));
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(
            request.ends_with(r#"{"teamId":"t-1","title":"Ship it","issueIds":["i-1","i-2"]}"#),
            "{request}"
        );
    }

    #[test]
    fn update_always_sends_the_title_key() {
        let (base, captured) =
            one_shot_server(200, r#"{"result":{"data":{"thread":{"id":"th-1","title":"New"},"txId":9}}}"#);
        let out = threads_update(&client(&base), "th-1", Some("New")).unwrap();
        assert_eq!(out.thread.title.as_deref(), Some("New"));
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(request.starts_with("POST /api/trpc/threads.update HTTP/1.1"));
        assert!(request.ends_with(r#"{"id":"th-1","title":"New"}"#), "{request}");

        // Clearing = an explicit null, never an omitted key.
        let (base, captured) =
            one_shot_server(200, r#"{"result":{"data":{"thread":{"id":"th-1"},"txId":10}}}"#);
        threads_update(&client(&base), "th-1", None).unwrap();
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(request.ends_with(r#"{"id":"th-1","title":null}"#), "{request}");
    }

    #[test]
    fn link_sends_exactly_one_target() {
        let (base, captured) =
            one_shot_server(200, r#"{"result":{"data":{"added":true,"txId":11}}}"#);
        let out = threads_link(&client(&base), "th-1", ThreadTarget::Issue("i-1")).unwrap();
        assert!(out.added);
        assert_eq!(out.tx_id, Some(11));
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(request.starts_with("POST /api/trpc/threads.link HTTP/1.1"));
        assert!(request.ends_with(r#"{"id":"th-1","issueId":"i-1"}"#), "{request}");

        let (base, captured) =
            one_shot_server(200, r#"{"result":{"data":{"added":false,"txId":12}}}"#);
        let out = threads_link(&client(&base), "th-1", ThreadTarget::Run("s-1")).unwrap();
        assert!(!out.added);
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(request.ends_with(r#"{"id":"th-1","sessionId":"s-1"}"#), "{request}");
    }

    #[test]
    fn unlink_decodes_the_runs_new_thread() {
        let (base, captured) = one_shot_server(
            200,
            r#"{"result":{"data":{"removed":true,"txId":13,"newThreadId":"th-2"}}}"#,
        );
        let out = threads_unlink(&client(&base), "th-1", ThreadTarget::Run("s-1")).unwrap();
        assert!(out.removed);
        assert_eq!(out.new_thread_id.as_deref(), Some("th-2"));
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(request.starts_with("POST /api/trpc/threads.unlink HTTP/1.1"));
        assert!(request.ends_with(r#"{"id":"th-1","sessionId":"s-1"}"#), "{request}");

        // An issue unlink: no new thread.
        let (base, captured) =
            one_shot_server(200, r#"{"result":{"data":{"removed":true,"txId":14}}}"#);
        let out = threads_unlink(&client(&base), "th-1", ThreadTarget::Issue("i-1")).unwrap();
        assert_eq!(out.new_thread_id, None);
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(request.ends_with(r#"{"id":"th-1","issueId":"i-1"}"#), "{request}");
    }
}
