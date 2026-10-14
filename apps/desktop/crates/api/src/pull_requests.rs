//! Typed `pullRequests.*` tRPC helpers (EXP-1291: the PR model,
//! `pull_requests` + `pull_request_issues`, synced). A person acts on a PR by
//! its row id; the old issue-keyed `issues.mergePr`/`closePr`/`prFiles` and
//! `repositories.mergePull` are gone. Shapes verified against
//! `apps/web/src/lib/trpc/pull-requests.ts`:
//!
//! - `merge({pullRequestId, stack?})` → `{merged, queued?, txid?}`; `stack`
//!   merges THROUGH this member of a native stack. Refusals throw (the code is
//!   the signal: `CONFLICT` = a diagnosed content conflict).
//! - `close({pullRequestId})` → `{txid?}`.
//! - `files({pullRequestId})` → `{files: [{path, additions, deletions, patch}]}`.

use serde::{Deserialize, Serialize};

use crate::error::ApiError;
use crate::trpc::TrpcClient;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PullRequestInput<'a> {
    pull_request_id: &'a str,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct MergeInput<'a> {
    pull_request_id: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    stack: Option<bool>,
}

/// `pullRequests.merge` output. A merge GitHub's merge queue only ENQUEUED
/// answers `merged: false, queued: true` (EXP-1165).
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
pub struct MergeResult {
    pub merged: bool,
    #[serde(default)]
    pub queued: bool,
}

impl MergeResult {
    /// The pull request is in: merged and not merely queued.
    pub fn landed(&self) -> bool {
        self.merged && !self.queued
    }
}

/// `pullRequests.merge` — squash-merge through the GitHub App. `stack` is
/// sent only when set.
pub fn merge(trpc: &TrpcClient, pull_request_id: &str, stack: bool) -> Result<MergeResult, ApiError> {
    trpc.mutation(
        "pullRequests.merge",
        &MergeInput {
            pull_request_id,
            stack: stack.then_some(true),
        },
    )
}

/// `pullRequests.close` — close without merging.
pub fn close(trpc: &TrpcClient, pull_request_id: &str) -> Result<(), ApiError> {
    let _: serde::de::IgnoredAny =
        trpc.mutation("pullRequests.close", &PullRequestInput { pull_request_id })?;
    Ok(())
}

/// One changed file of a PR.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct PullRequestFile {
    pub path: String,
    /// Signed and wide on purpose (EXP-912: the clamp lives in the renderer).
    pub additions: i64,
    pub deletions: i64,
    /// `None` for a binary or too-large file.
    #[serde(default)]
    pub patch: Option<String>,
}

#[derive(Deserialize)]
struct FilesOutput {
    #[serde(default)]
    files: Vec<PullRequestFile>,
}

/// `pullRequests.files` — query: the PR's changed files.
pub fn files(trpc: &TrpcClient, pull_request_id: &str) -> Result<Vec<PullRequestFile>, ApiError> {
    let out: FilesOutput =
        trpc.query_with_input("pullRequests.files", &PullRequestInput { pull_request_id })?;
    Ok(out.files)
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
    fn merge_sends_the_stack_flag_only_when_set() {
        let (base, captured) = one_shot_server(200, r#"{"result":{"data":{"merged":true,"txid":4}}}"#);
        assert!(merge(&client(&base), "pr-1", false).unwrap().landed());
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(request.starts_with("POST /api/trpc/pullRequests.merge HTTP/1.1"));
        assert!(request.ends_with(r#"{"pullRequestId":"pr-1"}"#), "{request}");

        let (base, captured) =
            one_shot_server(200, r#"{"result":{"data":{"merged":false,"queued":true}}}"#);
        let queued = merge(&client(&base), "pr-1", true).unwrap();
        assert!(!queued.landed());
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(request.ends_with(r#"{"pullRequestId":"pr-1","stack":true}"#), "{request}");
    }

    #[test]
    fn a_refused_merge_surfaces_the_server_message() {
        let (base, _captured) = one_shot_server(
            409,
            r#"{"error":{"message":"The pull request has conflicts","code":-32009,"data":{"code":"CONFLICT","httpStatus":409}}}"#,
        );
        match merge(&client(&base), "pr-1", false) {
            Err(ApiError::Http { status, message }) => {
                assert_eq!(status, 409);
                assert!(message.contains("conflicts"));
            }
            other => panic!("expected a 409, got {other:?}"),
        }
    }

    #[test]
    fn close_posts_the_id() {
        let (base, captured) = one_shot_server(200, r#"{"result":{"data":{}}}"#);
        close(&client(&base), "pr-2").unwrap();
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(request.starts_with("POST /api/trpc/pullRequests.close HTTP/1.1"));
        assert!(request.ends_with(r#"{"pullRequestId":"pr-2"}"#));
    }

    #[test]
    fn files_decodes_the_rows() {
        let (base, captured) = one_shot_server(
            200,
            r#"{"result":{"data":{"files":[{"path":"src/a.rs","additions":-3,"deletions":1,"patch":"@@ -1 +1 @@"},{"path":"logo.png","additions":0,"deletions":0,"patch":null}]}}}"#,
        );
        let files = files(&client(&base), "pr-3").unwrap();
        assert_eq!(files.len(), 2);
        assert_eq!(files[0].additions, -3);
        assert_eq!(files[1].patch, None);
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(request.starts_with("GET /api/trpc/pullRequests.files?input="), "{request}");
    }
}
