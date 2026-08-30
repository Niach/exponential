//! Typed `codingSessions.*` tRPC helpers (masterplan-v3 §7.1 steps 6 + 8).
//! Shapes verified against `apps/web/src/lib/trpc/coding-sessions.ts`:
//!
//! - `codingSessions.start({issueId, deviceLabel?})` → `{session}` —
//!   **mutation**, called BEFORE the child spawns: the returned row id keys
//!   the terminal tab (§06) and the steer session room (§08). The server
//!   enforces the plan's concurrent-session capacity here
//!   (`assertWithinCodingSessionLimit` → `PRECONDITION_FAILED`/412 with an
//!   upgrade nudge on cloud; unlimited self-hosted) — the desktop never
//!   self-throttles (§7.6).
//! - `codingSessions.end({id})` → `{session}` — **mutation**, idempotent
//!   server-side (ending an already-ended session is a no-op), so firing it
//!   from the child-exit hook is safe even after a relay-side kill already
//!   ended the row.
//!
//! The row is a synced Electric shape: the "coding now" badge everywhere is
//! the synced row itself — no client fabricates it locally.

use serde::{Deserialize, Serialize};

use crate::error::ApiError;
use crate::trpc::TrpcClient;

/// The synced `coding_sessions` row as the start/end mutations return it.
/// Only the fields the launcher consumes are typed strictly; the rest are
/// tolerant options so server-side additions never break the decode.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CodingSession {
    pub id: String,
    /// NULL on batch-scoped (multi-issue) session rows.
    #[serde(default)]
    pub issue_id: Option<String>,
    #[serde(default)]
    pub team_id: Option<String>,
    #[serde(default)]
    pub user_id: Option<String>,
    #[serde(default)]
    pub device_label: Option<String>,
    /// Action-scoped rows (EXP-253): the `actions` row id + display-name
    /// snapshot; both NULL on issue/batch rows.
    #[serde(default)]
    pub action_id: Option<String>,
    #[serde(default)]
    pub action_name: Option<String>,
    /// EXP-583: the `automations` row that fired this run; NULL on every
    /// manual start.
    #[serde(default)]
    pub automation_id: Option<String>,
    /// EXP-445: the hosting account on shared-device runs (EXP-432) — the
    /// device owner whose daemon executes a teammate's session. Reaches us
    /// only via tRPC `get`/mutations (server-only column, never in the
    /// Electric shape); `None` on every self-hosted row. The CLI kill-poll
    /// needs it to recognize a hosted row's →ended flip as its own kill.
    #[serde(default)]
    pub host_user_id: Option<String>,
    /// `running` | `in_review` | `ended` (contract enum
    /// `coding_session_status`).
    #[serde(default)]
    pub status: Option<String>,
    /// EXP-484: the agent CLI running it (`claude`/`codex`/`pi`); `None` on
    /// rows written before the column existed.
    #[serde(default)]
    pub agent: Option<String>,
    /// EXP-637 close-out — who ended the run (`agent`/`user`/`client`/
    /// `merge`/`system`) and its one-paragraph `summary`. Both `None` on live
    /// rows and on rows written by pre-EXP-637 servers. EXP-686 dropped the
    /// self-reported `outcome` alongside its column; an unknown field on the
    /// wire is ignored, so old servers stay decodable.
    #[serde(default)]
    pub ended_by: Option<String>,
    #[serde(default)]
    pub summary: Option<String>,
    #[serde(default)]
    pub started_at: Option<String>,
    #[serde(default)]
    pub ended_at: Option<String>,
}

#[derive(Deserialize)]
struct SessionEnvelope {
    session: CodingSession,
}

/// EXP-432: requester attribution for a start that arrived over the relay
/// targeting a SHARED server device. `started_by_id` is the frame's
/// `startedBy` (the requesting teammate); `device_id` is THIS machine's
/// steer deviceId so the server can verify the share before attributing the
/// row. Both `None` (the [`Default`]) on local and own-device starts — the
/// wire stays byte-identical there.
#[derive(Clone, Copy, Debug, Default)]
pub struct Attribution<'a> {
    pub started_by_id: Option<&'a str>,
    pub device_id: Option<&'a str>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct StartInput<'a> {
    issue_id: &'a str,
    /// EXP-679: `agent` when another coding session started this run (the
    /// relay frame's `startedReason`) — it makes the row UNATTENDED, so the
    /// server registers `exponential_sessions_end` for it and that call ends
    /// it. Absent for every person-started run, so old servers never see the
    /// field.
    #[serde(skip_serializing_if = "Option::is_none")]
    started_reason: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    device_label: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    started_by_id: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    device_id: Option<&'a str>,
    /// EXP-662: the ended session this issue run resumes (the server has
    /// accepted `resumedFromId` on the issue branch since EXP-637). Behind
    /// `skip_serializing_if`, so a fresh start's wire is byte-identical.
    #[serde(skip_serializing_if = "Option::is_none")]
    resumed_from_id: Option<&'a str>,
    /// EXP-484: which agent CLI runs this session (contract `codingAgent`).
    /// Behind `skip_serializing_if`, so an agent-less start's wire is
    /// byte-identical and an older server simply strips the key.
    #[serde(skip_serializing_if = "Option::is_none")]
    agent: Option<&'a str>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct StartBatchInput<'a> {
    team_id: &'a str,
    /// EXP-679 — same as [`StartInput::started_reason`], on the batch branch.
    #[serde(skip_serializing_if = "Option::is_none")]
    started_reason: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    device_label: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    started_by_id: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    device_id: Option<&'a str>,
    /// EXP-662 — same as [`StartInput::resumed_from_id`], on the batch branch.
    #[serde(skip_serializing_if = "Option::is_none")]
    resumed_from_id: Option<&'a str>,
    /// EXP-484: which agent CLI runs this session (contract `codingAgent`).
    /// Behind `skip_serializing_if`, so an agent-less start's wire is
    /// byte-identical and an older server simply strips the key.
    #[serde(skip_serializing_if = "Option::is_none")]
    agent: Option<&'a str>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct StartActionInput<'a> {
    action_id: &'a str,
    /// Required by the server iff `action_id` is the builtin
    /// `builtin:create-action` literal (EXP-257 — the builtin has no DB row
    /// to resolve a team from); forbidden otherwise.
    #[serde(skip_serializing_if = "Option::is_none")]
    team_id: Option<&'a str>,
    /// EXP-530: `schedule`/`event` when an automation (not a person)
    /// started the run; absent for user starts — old servers never see the
    /// field.
    #[serde(skip_serializing_if = "Option::is_none")]
    started_reason: Option<&'a str>,
    /// EXP-583: the `automations` row that fired it. The server refuses it
    /// without a `started_reason`, and pairs it against the action.
    #[serde(skip_serializing_if = "Option::is_none")]
    automation_id: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    device_label: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    started_by_id: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    device_id: Option<&'a str>,
    /// EXP-637: the run branch this session works on (`exp/<slug>-<id8>` /
    /// `exp/chat-<id8>`). The server refuses it beside an `issueId`; absent
    /// on repo-less scratch runs, so old servers never see the field.
    #[serde(skip_serializing_if = "Option::is_none")]
    branch: Option<&'a str>,
    /// EXP-637: the ended session this run resumes.
    #[serde(skip_serializing_if = "Option::is_none")]
    resumed_from_id: Option<&'a str>,
    /// EXP-484: which agent CLI runs this session (contract `codingAgent`).
    /// Behind `skip_serializing_if`, so an agent-less start's wire is
    /// byte-identical and an older server simply strips the key.
    #[serde(skip_serializing_if = "Option::is_none")]
    agent: Option<&'a str>,
}

#[derive(Serialize)]
struct SessionIdInput<'a> {
    id: &'a str,
}

/// The row's original start scope, riding every heartbeat (EXP-105): a ping
/// that finds the row swept (laptop suspend outlived the server's staleness
/// window) re-creates it server-side under the SAME id, restoring the badge
/// and steerability. Exactly one of `issue_id`/`team_id` is set —
/// `start`'s own invariant. An ACTION scope (EXP-253) is `team_id` PLUS
/// `action_id`/`action_name`: the team rides along so a deleted action still
/// resurrects the row batch-shaped, and the name is the client-held display
/// snapshot the re-created row keeps.
/// No `device_label`: the server resolves the label from the `devices`
/// registry on a heartbeat (it only ever takes a sent label on `start`, the
/// fallback for the fire-and-forget `devices.register` race), so echoing one
/// here was dead wire (EXP-672).
#[derive(Clone, Debug)]
pub struct HeartbeatScope {
    pub issue_id: Option<String>,
    pub team_id: Option<String>,
    pub action_id: Option<String>,
    pub action_name: Option<String>,
    /// EXP-432: shared-device attribution echoed on every ping so a swept
    /// row resurrects requester-owned instead of flipping to the host. Both
    /// `None` for local/own-device sessions.
    pub started_by_id: Option<String>,
    pub device_id: Option<String>,
    /// EXP-530: the automation reason (`schedule`/`event`) echoed so a swept
    /// row resurrects inside the Automations run history instead of looking
    /// hand-started. `None` for every user-started run. EXP-679: an issue or
    /// batch scope can carry one too — `agent`, a run another coding session
    /// started.
    pub started_reason: Option<String>,
    /// EXP-583: the firing `automations` row, echoed for the same reason —
    /// a resurrected row keeps pointing at the automation that started it.
    pub automation_id: Option<String>,
    /// EXP-637: the run branch, echoed so a resurrected row keeps pointing
    /// at the worktree the agent is actually working in. `None` on issue
    /// scopes (the server refuses it there) and on repo-less runs.
    pub branch: Option<String>,
    /// EXP-484: the agent CLI running the session, echoed so a resurrected
    /// row still says which one it is.
    pub agent: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct HeartbeatInput<'a> {
    id: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    issue_id: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    team_id: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    action_id: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    action_name: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    started_by_id: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    device_id: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    started_reason: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    automation_id: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    branch: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    agent: Option<&'a str>,
}

#[derive(Deserialize)]
struct HeartbeatEnvelope {
    alive: bool,
}

#[derive(Deserialize)]
struct MaybeSessionEnvelope {
    #[serde(default)]
    session: Option<CodingSession>,
}

/// `codingSessions.get` — query (EXP-403): own-row status probe for the
/// headless CLI's kill-switch poll (no Electric sync there). `Ok(None)` =
/// row swept/foreign — like the desktop kill-watch's vanished-row rule this
/// must NOT read as a kill; only an explicit `status == "ended"` does.
pub fn get(trpc: &TrpcClient, id: &str) -> Result<Option<CodingSession>, ApiError> {
    let envelope: MaybeSessionEnvelope =
        trpc.query_with_input("codingSessions.get", &SessionIdInput { id })?;
    Ok(envelope.session)
}

/// A live session on an issue as `codingSessions.liveForIssue` reports it —
/// the CLI daemon's REV2-24 one-session-per-issue probe (the desktop reads
/// its synced collection for this instead).
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LiveSessionInfo {
    pub id: String,
    #[serde(default)]
    pub device_label: Option<String>,
    #[serde(default)]
    pub user_id: Option<String>,
}

#[derive(Deserialize)]
struct MaybeLiveEnvelope {
    #[serde(default)]
    session: Option<LiveSessionInfo>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct LiveForIssueInput<'a> {
    issue_id: &'a str,
}

/// `codingSessions.liveForIssue` — query. `Ok(None)` = no live session.
/// Best-effort at call sites: an older server without the procedure must
/// degrade to "no guard", never block the start.
pub fn live_for_issue(
    trpc: &TrpcClient,
    issue_id: &str,
) -> Result<Option<LiveSessionInfo>, ApiError> {
    let envelope: MaybeLiveEnvelope =
        trpc.query_with_input("codingSessions.liveForIssue", &LiveForIssueInput { issue_id })?;
    Ok(envelope.session)
}

/// `codingSessions.start` — mutation. A 412 (`PRECONDITION_FAILED`) is the
/// plan's concurrent-session cap; the launcher maps it to its `SessionLimit`
/// disabled state with the server's upgrade copy.
/// EXP-662: `resumed_from_id` links the new row to the ended session it
/// continues; `None` on every fresh start.
/// EXP-679: `started_reason` (`agent`) marks a run another coding session
/// started — the server treats such a row as unattended.
pub fn start(
    trpc: &TrpcClient,
    issue_id: &str,
    device_label: Option<&str>,
    attribution: Attribution,
    started_reason: Option<&str>,
    resumed_from_id: Option<&str>,
    agent: Option<&str>,
) -> Result<CodingSession, ApiError> {
    let envelope: SessionEnvelope = trpc.mutation(
        "codingSessions.start",
        &StartInput {
            issue_id,
            started_reason,
            device_label,
            started_by_id: attribution.started_by_id,
            device_id: attribution.device_id,
            resumed_from_id,
            agent,
        },
    )?;
    Ok(envelope.session)
}

/// `codingSessions.start` for a BATCH-scoped (multi-issue) session: the
/// server accepts exactly one of `issueId`/`teamId` and inserts a row
/// with `issue_id`/`board_id` NULL and the given `team_id`. Same 412
/// semantics as [`start`].
pub fn start_batch(
    trpc: &TrpcClient,
    team_id: &str,
    device_label: Option<&str>,
    attribution: Attribution,
    started_reason: Option<&str>,
    resumed_from_id: Option<&str>,
    agent: Option<&str>,
) -> Result<CodingSession, ApiError> {
    let envelope: SessionEnvelope = trpc.mutation(
        "codingSessions.start",
        &StartBatchInput {
            team_id,
            started_reason,
            device_label,
            started_by_id: attribution.started_by_id,
            device_id: attribution.device_id,
            resumed_from_id,
            agent,
        },
    )?;
    Ok(envelope.session)
}

/// `codingSessions.start` for an ACTION-scoped session (EXP-253): the
/// server resolves the action → team, snapshots `action_name`, and inserts
/// a batch-shaped row (`issue_id`/`board_id` NULL) carrying `action_id`.
/// EXP-257: the builtin `builtin:create-action` id must ride with `team_id`
/// (the server inserts that row with `action_id` NULL + the constant name);
/// real actions must NOT send one. Same 412 semantics as [`start`].
/// EXP-530: `started_reason` (`schedule`/`event`) marks an
/// automation-started run — `None` for every user start; EXP-583 pairs it
/// with the firing `automation_id` (the server refuses one without the
/// other).
/// EXP-637: the argument bag for [`start_action`] — the positional list
/// outgrew readability once the run branch and the resume link joined it.
#[derive(Clone, Copy, Debug, Default)]
pub struct ActionStart<'a> {
    pub action_id: &'a str,
    pub team_id: Option<&'a str>,
    pub started_reason: Option<&'a str>,
    pub automation_id: Option<&'a str>,
    pub device_label: Option<&'a str>,
    /// The run's dedicated branch (EXP-637 worktree-per-run); `None` on
    /// repo-less scratch runs.
    pub branch: Option<&'a str>,
    /// The ended session this run resumes; `None` on a fresh run.
    pub resumed_from_id: Option<&'a str>,
    /// EXP-484: the agent CLI executing the run (contract `codingAgent`).
    pub agent: Option<&'a str>,
    pub attribution: Attribution<'a>,
}

pub fn start_action(
    trpc: &TrpcClient,
    start: ActionStart<'_>,
) -> Result<CodingSession, ApiError> {
    let envelope: SessionEnvelope = trpc.mutation(
        "codingSessions.start",
        &StartActionInput {
            action_id: start.action_id,
            team_id: start.team_id,
            started_reason: start.started_reason,
            automation_id: start.automation_id,
            device_label: start.device_label,
            started_by_id: start.attribution.started_by_id,
            device_id: start.attribution.device_id,
            branch: start.branch,
            resumed_from_id: start.resumed_from_id,
            agent: start.agent,
        },
    )?;
    Ok(envelope.session)
}

/// `codingSessions.end` — mutation, idempotent server-side.
pub fn end(trpc: &TrpcClient, id: &str) -> Result<CodingSession, ApiError> {
    let envelope: SessionEnvelope = trpc.mutation("codingSessions.end", &SessionIdInput { id })?;
    Ok(envelope.session)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SetNeedsInputInput<'a> {
    id: &'a str,
    needs_input: bool,
}

#[derive(Deserialize)]
struct SetNeedsInputEnvelope {
    updated: bool,
}

/// `codingSessions.setNeedsInput` — mutation (EXP-214). Flips the synced
/// row's `needs_input` attention flag while the agent is parked on a
/// plan-approval / AskUserQuestion picker (the activity emitter's picker
/// watchers drive it). Fire-and-forget like heartbeat: `updated: false`
/// (row swept/ended) and transport errors are both ignorable — the flag
/// re-asserts on the next picker transition.
pub fn set_needs_input(trpc: &TrpcClient, id: &str, needs_input: bool) -> Result<bool, ApiError> {
    let envelope: SetNeedsInputEnvelope = trpc.mutation(
        "codingSessions.setNeedsInput",
        &SetNeedsInputInput { id, needs_input },
    )?;
    Ok(envelope.updated)
}

/// `codingSessions.heartbeat` — mutation. Advances the synced row's
/// `updated_at` while the claude child is alive so the server's staleness
/// sweep (which DELETES `running` rows whose liveness signal stopped) never
/// reaps a live session's badge — and, when `scope` rides along, re-creates
/// a row the sweep DID reap (suspend > staleness window) under the same id.
/// Fire-and-forget from the launcher's heartbeat thread: `alive: false`
/// (row ended, or resurrection impossible) and transport errors are both
/// ignorable — the next child-exit hook still ends the session normally.
pub fn heartbeat(
    trpc: &TrpcClient,
    id: &str,
    scope: Option<&HeartbeatScope>,
) -> Result<bool, ApiError> {
    let envelope: HeartbeatEnvelope = trpc.mutation(
        "codingSessions.heartbeat",
        &HeartbeatInput {
            id,
            issue_id: scope.and_then(|scope| scope.issue_id.as_deref()),
            team_id: scope.and_then(|scope| scope.team_id.as_deref()),
            action_id: scope.and_then(|scope| scope.action_id.as_deref()),
            action_name: scope.and_then(|scope| scope.action_name.as_deref()),
            started_by_id: scope.and_then(|scope| scope.started_by_id.as_deref()),
            device_id: scope.and_then(|scope| scope.device_id.as_deref()),
            started_reason: scope.and_then(|scope| scope.started_reason.as_deref()),
            automation_id: scope.and_then(|scope| scope.automation_id.as_deref()),
            branch: scope.and_then(|scope| scope.branch.as_deref()),
            agent: scope.and_then(|scope| scope.agent.as_deref()),
        },
    )?;
    Ok(envelope.alive)
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

    const SESSION_BODY: &str = r#"{"result":{"data":{"session":{
        "id":"sess-1","issueId":"issue-1","teamId":"ws-1","userId":"user-1",
        "deviceLabel":"testbox","status":"running",
        "startedAt":"2026-07-03T10:00:00.000Z","endedAt":null,
        "createdAt":"2026-07-03T10:00:00.000Z","updatedAt":"2026-07-03T10:00:00.000Z"}}}}"#;

    #[test]
    fn start_decodes_session_envelope_and_posts_device_label() {
        let (base, captured) = one_shot_server(200, SESSION_BODY);
        let session = start(&client(&base), "issue-1", Some("testbox"), Attribution::default(), None, None, None).unwrap();
        assert_eq!(session.id, "sess-1");
        assert_eq!(session.status.as_deref(), Some("running"));
        assert_eq!(session.device_label.as_deref(), Some("testbox"));
        assert_eq!(session.ended_at, None);
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(request.starts_with("POST /api/trpc/codingSessions.start HTTP/1.1"));
        assert!(request.ends_with(r#"{"issueId":"issue-1","deviceLabel":"testbox"}"#));
    }

    /// EXP-445: the tRPC row's `hostUserId` (shared-device runs) decodes —
    /// the CLI kill-poll's owner-or-host rule depends on it; absent = None.
    #[test]
    fn decodes_host_user_id_when_present() {
        let session: CodingSession = serde_json::from_str(
            r#"{"id":"sess-1","userId":"requester","hostUserId":"host-1","status":"running"}"#,
        )
        .unwrap();
        assert_eq!(session.host_user_id.as_deref(), Some("host-1"));

        let session: CodingSession = serde_json::from_str(r#"{"id":"sess-2"}"#).unwrap();
        assert_eq!(session.host_user_id, None);
    }

    #[test]
    fn start_omits_absent_device_label() {
        let (base, captured) = one_shot_server(200, SESSION_BODY);
        let _ = start(&client(&base), "issue-1", None, Attribution::default(), None, None, None).unwrap();
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(request.ends_with(r#"{"issueId":"issue-1"}"#));
    }

    #[test]
    fn start_batch_posts_team_id_and_decodes_the_issueless_row() {
        let (base, captured) = one_shot_server(
            200,
            r#"{"result":{"data":{"session":{
                "id":"sess-b","issueId":null,"teamId":"ws-1",
                "userId":"user-1","deviceLabel":"testbox","status":"running"}}}}"#,
        );
        let session = start_batch(&client(&base), "ws-1", Some("testbox"), Attribution::default(), None, None, None).unwrap();
        assert_eq!(session.id, "sess-b");
        assert_eq!(session.team_id.as_deref(), Some("ws-1"));
        assert_eq!(session.issue_id, None);
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(request.starts_with("POST /api/trpc/codingSessions.start HTTP/1.1"));
        assert!(request.ends_with(r#"{"teamId":"ws-1","deviceLabel":"testbox"}"#));
    }

    #[test]
    fn start_posts_the_resume_link() {
        // EXP-662: a resumed ISSUE session points back at the run it
        // continues. Behind skip_serializing_if — the two tests above lock
        // the byte-identical wire a fresh start still sends.
        let (base, captured) = one_shot_server(200, SESSION_BODY);
        let _ = start(
            &client(&base),
            "issue-1",
            Some("testbox"),
            Attribution::default(),
            None,
            Some("sess-old"),
            None,
        )
        .unwrap();
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(request.ends_with(
            r#"{"issueId":"issue-1","deviceLabel":"testbox","resumedFromId":"sess-old"}"#
        ));
    }

    #[test]
    fn start_batch_posts_the_resume_link() {
        let (base, captured) = one_shot_server(
            200,
            r#"{"result":{"data":{"session":{
                "id":"sess-b","issueId":null,"teamId":"ws-1",
                "userId":"user-1","status":"running"}}}}"#,
        );
        let _ = start_batch(
            &client(&base),
            "ws-1",
            Some("testbox"),
            Attribution::default(),
            None,
            Some("sess-old"),
            None,
        )
        .unwrap();
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(request.ends_with(
            r#"{"teamId":"ws-1","deviceLabel":"testbox","resumedFromId":"sess-old"}"#
        ));
    }

    #[test]
    fn start_posts_shared_device_attribution() {
        // EXP-432: a shared-device relay start echoes startedById + this
        // machine's deviceId so the server can verify the share.
        let (base, captured) = one_shot_server(200, SESSION_BODY);
        let attribution = Attribution {
            started_by_id: Some("user-2"),
            device_id: Some("dev-1"),
        };
        let _ = start(&client(&base), "issue-1", Some("testbox"), attribution, None, None, None).unwrap();
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(request.ends_with(
            r#"{"issueId":"issue-1","deviceLabel":"testbox","startedById":"user-2","deviceId":"dev-1"}"#
        ));
    }

    #[test]
    fn start_posts_device_id_without_started_by() {
        // EXP-549: every start now carries this machine's steer deviceId so
        // the server stamps `coding_sessions.device_id` and snapshots the
        // machine's CURRENT (renamed) label — no `startedById` on an
        // own-device start.
        let (base, captured) = one_shot_server(200, SESSION_BODY);
        let attribution = Attribution {
            started_by_id: None,
            device_id: Some("dev-1"),
        };
        let _ = start(&client(&base), "issue-1", Some("testbox"), attribution, None, None, None).unwrap();
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(request
            .ends_with(r#"{"issueId":"issue-1","deviceLabel":"testbox","deviceId":"dev-1"}"#));
    }

    #[test]
    fn heartbeat_posts_shared_device_attribution() {
        // EXP-432: the scope echoes attribution so a swept row resurrects
        // requester-owned.
        let (base, captured) = one_shot_server(200, r#"{"result":{"data":{"alive":true}}}"#);
        let scope = HeartbeatScope {
            issue_id: Some("issue-1".to_string()),
            team_id: None,
            action_id: None,
            action_name: None,
            started_by_id: Some("user-2".to_string()),
            device_id: Some("dev-1".to_string()),
            started_reason: None,
            automation_id: None,
            branch: None,
            agent: None,
        };
        assert!(heartbeat(&client(&base), "sess-1", Some(&scope)).unwrap());
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(request.ends_with(
            r#"{"id":"sess-1","issueId":"issue-1","startedById":"user-2","deviceId":"dev-1"}"#
        ));
    }

    #[test]
    fn session_limit_surfaces_as_412() {
        let (base, _captured) = one_shot_server(
            412,
            r#"{"error":{"message":"Concurrent coding session limit reached — upgrade to run more.","code":-32012,"data":{"code":"PRECONDITION_FAILED","httpStatus":412}}}"#,
        );
        match start(&client(&base), "issue-1", None, Attribution::default(), None, None, None) {
            Err(ApiError::Http { status, message }) => {
                assert_eq!(status, 412);
                assert!(message.contains("limit"));
            }
            other => panic!("expected 412 Http error, got {other:?}"),
        }
    }

    #[test]
    fn heartbeat_posts_id_and_decodes_alive() {
        let (base, captured) = one_shot_server(200, r#"{"result":{"data":{"alive":true}}}"#);
        let alive = heartbeat(&client(&base), "sess-1", None).unwrap();
        assert!(alive);
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(request.starts_with("POST /api/trpc/codingSessions.heartbeat HTTP/1.1"));
        assert!(request.ends_with(r#"{"id":"sess-1"}"#));
    }

    #[test]
    fn heartbeat_posts_resurrection_scope_when_provided() {
        let (base, captured) = one_shot_server(200, r#"{"result":{"data":{"alive":true}}}"#);
        let scope = HeartbeatScope {
            issue_id: Some("issue-1".to_string()),
            team_id: None,
            action_id: None,
            action_name: None,
            started_by_id: None,
            device_id: None,
            started_reason: None,
            automation_id: None,
            branch: None,
            agent: None,
        };
        assert!(heartbeat(&client(&base), "sess-1", Some(&scope)).unwrap());
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        // EXP-672: no `deviceLabel` on the heartbeat wire — the server
        // resolves the label from the devices registry there.
        assert!(request.ends_with(r#"{"id":"sess-1","issueId":"issue-1"}"#));
    }

    #[test]
    fn heartbeat_posts_batch_scope_team_id() {
        let (base, captured) = one_shot_server(200, r#"{"result":{"data":{"alive":true}}}"#);
        let scope = HeartbeatScope {
            issue_id: None,
            team_id: Some("ws-1".to_string()),
            action_id: None,
            action_name: None,
            started_by_id: None,
            device_id: None,
            started_reason: None,
            automation_id: None,
            branch: None,
            agent: None,
        };
        assert!(heartbeat(&client(&base), "sess-1", Some(&scope)).unwrap());
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(request.ends_with(r#"{"id":"sess-1","teamId":"ws-1"}"#));
    }

    #[test]
    fn start_action_posts_action_id_and_decodes_the_action_row() {
        let (base, captured) = one_shot_server(
            200,
            r#"{"result":{"data":{"session":{
                "id":"sess-a","issueId":null,"teamId":"ws-1",
                "actionId":"act-1","actionName":"Code review",
                "userId":"user-1","deviceLabel":"testbox","status":"running"}}}}"#,
        );
        let session = start_action(
            &client(&base),
            ActionStart {
                action_id: "act-1",
                device_label: Some("testbox"),
                ..ActionStart::default()
            },
        )
        .unwrap();
        assert_eq!(session.id, "sess-a");
        assert_eq!(session.action_id.as_deref(), Some("act-1"));
        assert_eq!(session.action_name.as_deref(), Some("Code review"));
        assert_eq!(session.issue_id, None);
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(request.starts_with("POST /api/trpc/codingSessions.start HTTP/1.1"));
        // A real action never sends teamId (the server resolves it).
        assert!(request.ends_with(r#"{"actionId":"act-1","deviceLabel":"testbox"}"#));
    }

    #[test]
    fn start_builtin_action_posts_the_literal_id_with_team_id() {
        // EXP-257: the builtin has no DB row — teamId must ride along, and
        // the server answers with actionId NULL + the constant name.
        let (base, captured) = one_shot_server(
            200,
            r#"{"result":{"data":{"session":{
                "id":"sess-c","issueId":null,"teamId":"ws-1",
                "actionId":null,"actionName":"Create action",
                "userId":"user-1","status":"running"}}}}"#,
        );
        let session = start_action(
            &client(&base),
            ActionStart {
                action_id: "builtin:create-action",
                team_id: Some("ws-1"),
                device_label: Some("testbox"),
                ..ActionStart::default()
            },
        )
        .unwrap();
        assert_eq!(session.id, "sess-c");
        assert_eq!(session.action_id, None);
        assert_eq!(session.action_name.as_deref(), Some("Create action"));
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(request.ends_with(
            r#"{"actionId":"builtin:create-action","teamId":"ws-1","deviceLabel":"testbox"}"#
        ));
    }

    /// EXP-679: an issue run another coding session started (the relay
    /// frame's `startedReason: "agent"`) stamps the reason on the ISSUE
    /// start too — the server needs it to register the close-out tool. A
    /// person's start omits the field entirely (locked by the two tests
    /// above), so old servers never see it.
    #[test]
    fn start_posts_the_agent_started_reason() {
        let (base, captured) = one_shot_server(200, SESSION_BODY);
        let _ = start(
            &client(&base),
            "issue-1",
            Some("testbox"),
            Attribution::default(),
            Some("agent"),
            None,
            None,
        )
        .unwrap();
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(request.ends_with(
            r#"{"issueId":"issue-1","startedReason":"agent","deviceLabel":"testbox"}"#
        ));
    }

    /// EXP-679, the batch branch of the test above.
    #[test]
    fn start_batch_posts_the_agent_started_reason() {
        let (base, captured) = one_shot_server(
            200,
            r#"{"result":{"data":{"session":{
                "id":"sess-b","issueId":null,"teamId":"ws-1",
                "userId":"user-1","status":"running"}}}}"#,
        );
        let _ = start_batch(
            &client(&base),
            "ws-1",
            Some("testbox"),
            Attribution::default(),
            Some("agent"),
            None,
            None,
        )
        .unwrap();
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(request
            .ends_with(r#"{"teamId":"ws-1","startedReason":"agent","deviceLabel":"testbox"}"#));
    }

    #[test]
    fn start_action_posts_started_reason_for_automation_runs() {
        // EXP-530: an automation-started run stamps startedReason; user
        // starts omit the field entirely (old-server compat rides on the
        // skip_serializing_if — locked by the two tests above).
        let (base, captured) = one_shot_server(
            200,
            r#"{"result":{"data":{"session":{
                "id":"sess-a","issueId":null,"teamId":"ws-1",
                "actionId":"act-1","actionName":"Code review",
                "userId":"user-1","status":"running"}}}}"#,
        );
        start_action(
            &client(&base),
            ActionStart {
                action_id: "act-1",
                started_reason: Some("schedule"),
                automation_id: Some("auto-1"),
                device_label: Some("testbox"),
                ..ActionStart::default()
            },
        )
        .unwrap();
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        // EXP-583: the firing automation rides beside the reason — the
        // server refuses an automationId without one.
        assert!(request.ends_with(
            r#"{"actionId":"act-1","startedReason":"schedule","automationId":"auto-1","deviceLabel":"testbox"}"#
        ));
    }

    #[test]
    fn heartbeat_posts_action_scope_with_team_and_snapshot() {
        // EXP-253: the action scope carries teamId (deleted-action degrade)
        // + the client-held name snapshot alongside actionId.
        let (base, captured) = one_shot_server(200, r#"{"result":{"data":{"alive":true}}}"#);
        let scope = HeartbeatScope {
            issue_id: None,
            team_id: Some("ws-1".to_string()),
            action_id: Some("act-1".to_string()),
            action_name: Some("Code review".to_string()),
            started_by_id: None,
            device_id: None,
            started_reason: None,
            automation_id: None,
            branch: None,
            agent: None,
        };
        assert!(heartbeat(&client(&base), "sess-a", Some(&scope)).unwrap());
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(request.ends_with(
            r#"{"id":"sess-a","teamId":"ws-1","actionId":"act-1","actionName":"Code review"}"#
        ));
    }

    #[test]
    fn heartbeat_posts_the_automation_reason() {
        // EXP-530: an automated run echoes its reason on every ping, so a
        // row the staleness sweep reaped resurrects with its "Automated"
        // badge instead of looking hand-started. User runs omit the field
        // entirely (the scope tests above lock that wire).
        let (base, captured) = one_shot_server(200, r#"{"result":{"data":{"alive":true}}}"#);
        let scope = HeartbeatScope {
            issue_id: None,
            team_id: Some("ws-1".to_string()),
            action_id: Some("act-1".to_string()),
            action_name: Some("Code review".to_string()),
            started_by_id: None,
            device_id: None,
            started_reason: Some("schedule".to_string()),
            automation_id: Some("auto-1".to_string()),
            branch: None,
            agent: None,
        };
        assert!(heartbeat(&client(&base), "sess-a", Some(&scope)).unwrap());
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(request.ends_with(
            r#"{"id":"sess-a","teamId":"ws-1","actionId":"act-1","actionName":"Code review","startedReason":"schedule","automationId":"auto-1"}"#
        ));
    }

    #[test]
    fn start_action_posts_the_run_branch_and_resume_link() {
        // EXP-637: worktree-per-run — the row records the branch the agent
        // works on, and a resumed run points back at the ended session it
        // continues. Both ride behind skip_serializing_if, so the wire above
        // stays byte-identical for old servers.
        let (base, captured) = one_shot_server(
            200,
            r#"{"result":{"data":{"session":{
                "id":"sess-a","issueId":null,"teamId":"ws-1",
                "actionId":"act-1","actionName":"Code review",
                "userId":"user-1","status":"running"}}}}"#,
        );
        start_action(
            &client(&base),
            ActionStart {
                action_id: "act-1",
                device_label: Some("testbox"),
                branch: Some("exp/code-review-1a2b3c4d"),
                resumed_from_id: Some("sess-old"),
                ..ActionStart::default()
            },
        )
        .unwrap();
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(request.ends_with(
            r#"{"actionId":"act-1","deviceLabel":"testbox","branch":"exp/code-review-1a2b3c4d","resumedFromId":"sess-old"}"#
        ));
    }

    #[test]
    fn heartbeat_posts_the_run_branch() {
        // EXP-637: a swept run row resurrects still pointing at its worktree.
        let (base, captured) = one_shot_server(200, r#"{"result":{"data":{"alive":true}}}"#);
        let scope = HeartbeatScope {
            issue_id: None,
            team_id: Some("ws-1".to_string()),
            action_id: Some("act-1".to_string()),
            action_name: Some("Code review".to_string()),
            started_by_id: None,
            device_id: None,
            started_reason: None,
            automation_id: None,
            branch: Some("exp/chat-1a2b3c4d".to_string()),
            agent: None,
        };
        assert!(heartbeat(&client(&base), "sess-a", Some(&scope)).unwrap());
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(request.ends_with(
            r#"{"id":"sess-a","teamId":"ws-1","actionId":"act-1","actionName":"Code review","branch":"exp/chat-1a2b3c4d"}"#
        ));
    }

    /// EXP-484: every start names the agent CLI that will run it — behind
    /// `skip_serializing_if`, so the agent-less wires above stay
    /// byte-identical.
    #[test]
    fn starts_post_the_agent() {
        let (base, captured) = one_shot_server(200, SESSION_BODY);
        let _ = start(
            &client(&base),
            "issue-1",
            Some("testbox"),
            Attribution::default(),
            None,
            None,
            Some("codex"),
        )
        .unwrap();
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(request
            .ends_with(r#"{"issueId":"issue-1","deviceLabel":"testbox","agent":"codex"}"#));

        let (base, captured) = one_shot_server(
            200,
            r#"{"result":{"data":{"session":{"id":"sess-b","teamId":"ws-1","status":"running"}}}}"#,
        );
        let _ = start_batch(
            &client(&base),
            "ws-1",
            None,
            Attribution::default(),
            None,
            None,
            Some("pi"),
        )
        .unwrap();
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(request.ends_with(r#"{"teamId":"ws-1","agent":"pi"}"#));

        let (base, captured) = one_shot_server(
            200,
            r#"{"result":{"data":{"session":{"id":"sess-a","teamId":"ws-1","status":"running"}}}}"#,
        );
        start_action(
            &client(&base),
            ActionStart {
                action_id: "act-1",
                agent: Some("claude"),
                ..ActionStart::default()
            },
        )
        .unwrap();
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(request.ends_with(r#"{"actionId":"act-1","agent":"claude"}"#));
    }

    /// And the heartbeat echoes it, so a resurrected row keeps naming its
    /// agent.
    #[test]
    fn heartbeat_posts_the_agent() {
        let (base, captured) = one_shot_server(200, r#"{"result":{"data":{"alive":true}}}"#);
        let scope = HeartbeatScope {
            issue_id: Some("issue-1".to_string()),
            team_id: None,
            action_id: None,
            action_name: None,
            started_by_id: None,
            device_id: None,
            started_reason: None,
            automation_id: None,
            branch: None,
            agent: Some("codex".to_string()),
        };
        assert!(heartbeat(&client(&base), "sess-1", Some(&scope)).unwrap());
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(request.ends_with(r#"{"id":"sess-1","issueId":"issue-1","agent":"codex"}"#));
    }

    /// The synced row decodes it (absent on pre-EXP-484 rows).
    #[test]
    fn decodes_the_session_agent() {
        let session: CodingSession =
            serde_json::from_str(r#"{"id":"sess-1","agent":"pi"}"#).unwrap();
        assert_eq!(session.agent.as_deref(), Some("pi"));
        let session: CodingSession = serde_json::from_str(r#"{"id":"sess-2"}"#).unwrap();
        assert_eq!(session.agent, None);
    }

    #[test]
    fn decodes_the_run_close_out() {
        // EXP-637: `exponential_sessions_end` writes these; absent = None.
        // The `outcome` key is deliberately still in this fixture (EXP-686
        // dropped the field): a server that predates the removal must stay
        // decodable, so an unknown key can never become an error.
        let session: CodingSession = serde_json::from_str(
            r#"{"id":"sess-1","status":"ended","endedBy":"agent","outcome":"done","summary":"Shipped it."}"#,
        )
        .unwrap();
        assert_eq!(session.ended_by.as_deref(), Some("agent"));
        assert_eq!(session.summary.as_deref(), Some("Shipped it."));

        let session: CodingSession = serde_json::from_str(r#"{"id":"sess-2"}"#).unwrap();
        assert_eq!(session.ended_by, None);
        assert_eq!(session.summary, None);
    }

    #[test]
    fn heartbeat_reports_a_dead_row_without_erroring() {
        let (base, _captured) = one_shot_server(200, r#"{"result":{"data":{"alive":false}}}"#);
        assert!(!heartbeat(&client(&base), "sess-1", None).unwrap());
    }

    #[test]
    fn set_needs_input_posts_flag_and_decodes_updated() {
        let (base, captured) = one_shot_server(200, r#"{"result":{"data":{"updated":true}}}"#);
        assert!(set_needs_input(&client(&base), "sess-1", true).unwrap());
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(request.starts_with("POST /api/trpc/codingSessions.setNeedsInput HTTP/1.1"));
        assert!(request.ends_with(r#"{"id":"sess-1","needsInput":true}"#));
    }

    #[test]
    fn end_posts_id_and_decodes_session() {
        let (base, captured) = one_shot_server(
            200,
            r#"{"result":{"data":{"session":{"id":"sess-1","status":"ended","endedAt":"2026-07-03T11:00:00.000Z"}}}}"#,
        );
        let session = end(&client(&base), "sess-1").unwrap();
        assert_eq!(session.id, "sess-1");
        assert_eq!(session.status.as_deref(), Some("ended"));
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(request.starts_with("POST /api/trpc/codingSessions.end HTTP/1.1"));
        assert!(request.ends_with(r#"{"id":"sess-1"}"#));
    }
}
