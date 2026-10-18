//! Typed `sessions.*` DEVICE procedures (EXP-1292, Threads v2) — the half of
//! the session router the machine that RUNS a session calls. Shapes verified
//! against `apps/web/src/lib/trpc/sessions.ts`:
//!
//! - `sessions.brief({sessionId})` → `{brief, teamPrompt, agentInstructions,
//!   parentBranch}` — **query**: the server-composed first prompt plus the
//!   system-append layers (runbook + team prompt + agent instructions).
//! - `sessions.claim({sessionId, branch?})` — **mutation**, queued → running
//!   (idempotent for the claiming device). Called BEFORE the launcher
//!   prepares anything.
//! - `sessions.report({sessionId, status?, busy?, needsInput?, blocked?,
//!   branch?, title?})` — **mutation**, the device-written state: running ⇄
//!   idle, busy, needs_input, the usage wall, the branch the launcher chose,
//!   the CLI's auto-title; `ended` when the agent process is gone.
//! - `sessions.reply({sessionId, body, card?})` → `{messageId}` — the agent's
//!   side of the Conversation: a turn's final reply text and/or a card.
//! - `sessions.consume({sessionId, messageIds})` — the device delivered these
//!   person messages into the agent (≤100 ids per call).
//! - `sessions.send({sessionId, text})` → `{sessionId, messageId}` — here ONLY
//!   as [`post_message`], the device posting a trigger firing into its own
//!   agent's channel (the people-side `sessions.*` wrappers live elsewhere).
//!
//! The database is the bus: a start is a `queued` sessions row naming this
//! device and a message is a `messages` row, both picked up off the Electric
//! shapes — there is no relay start path and no heartbeat anymore. Every
//! mutation returns a `txid` the device never awaits (it reads its own writes
//! back off the shape), so the outputs are decoded tolerantly.

use serde::{Deserialize, Serialize};

use crate::error::ApiError;
use crate::trpc::TrpcClient;

/// The server's `consume` cap (`messageIds` `.max(100)`).
pub const CONSUME_CHUNK: usize = 100;

/// `sessions.brief` output.
#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SessionBrief {
    /// The seed prompt (fixtures/session-brief.json): the subject, the
    /// action's rendered prompt, the starting message.
    #[serde(default)]
    pub brief: String,
    /// The team's agent prompt (owner-edited, EXP-1025); `None` = none set.
    #[serde(default)]
    pub team_prompt: Option<String>,
    /// A team agent's instructions (kind `agent` roots and their trees).
    #[serde(default)]
    pub agent_instructions: Option<String>,
    /// A child's parent branch: its worktree bases on `origin/<parent>`.
    #[serde(default)]
    pub parent_branch: Option<String>,
    /// EXP-1252: the session MOVED here from another device and its handoff
    /// package waits on the server ([`download_handoff`]); `None` = an
    /// ordinary launch.
    #[serde(default)]
    pub handoff: Option<BriefHandoff>,
}

/// EXP-1252: `sessions.brief`'s `handoff` — the package the source device
/// uploaded before the move.
#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct BriefHandoff {
    #[serde(default)]
    pub size_bytes: u64,
    #[serde(default)]
    pub from_device_id: String,
    #[serde(default)]
    pub from_device_name: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SessionIdInput<'a> {
    session_id: &'a str,
}

/// `sessions.brief` — query.
pub fn brief(trpc: &TrpcClient, session_id: &str) -> Result<SessionBrief, ApiError> {
    trpc.query_with_input("sessions.brief", &SessionIdInput { session_id })
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct McpPicksOutput {
    #[serde(default)]
    mcp_server_ids: Vec<String>,
}

/// EXP-1334: `sessions.mcpPicks` — query, the session's server-only team MCP
/// pick (sorted ids, empty = none). The device compares it, between turns,
/// with the pick its run launched with (`sessions.configure` may move it).
pub fn mcp_picks(trpc: &TrpcClient, session_id: &str) -> Result<Vec<String>, ApiError> {
    let output: McpPicksOutput = trpc.query_with_input("sessions.mcpPicks", &SessionIdInput { session_id })?;
    let mut ids = output.mcp_server_ids;
    ids.sort_unstable();
    ids.dedup();
    Ok(ids)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ClaimInput<'a> {
    session_id: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    device_id: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    branch: Option<&'a str>,
}

/// `sessions.claim` — mutation, queued → running for THIS device. The host
/// calls it before preparing the launch (normally `branch: None`; the
/// launcher reports the branch it chose via [`report`] once the worktree
/// exists). A refusal (another device claimed it, the row ended) is an
/// error the host turns into "skip this row".
///
/// EXP-1252: `device_id` names the claiming device — the server refuses
/// (FORBIDDEN) a claim from a device the row no longer names (a moved
/// session), so one row never runs in two places.
pub fn claim(
    trpc: &TrpcClient,
    session_id: &str,
    device_id: Option<&str>,
    branch: Option<&str>,
) -> Result<(), ApiError> {
    let device_id = device_id.filter(|id| !id.is_empty());
    let _: serde::de::IgnoredAny =
        trpc.mutation("sessions.claim", &ClaimInput { session_id, device_id, branch })?;
    Ok(())
}

/// EXP-1252: how a source device's handoff of a moving session ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum HandoffOutcome {
    /// The package is uploaded: the row moves to the target device.
    Moved,
    /// The handoff could not be made: the move is cleared, the session
    /// stays here untouched.
    Failed,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct HandoffInput<'a> {
    session_id: &'a str,
    device_id: &'a str,
    outcome: HandoffOutcome,
    #[serde(skip_serializing_if = "Option::is_none")]
    reason: Option<&'a str>,
}

/// EXP-1252: `sessions.handoff` — mutation, the SOURCE device's verdict on a
/// pending move. `Moved` (after [`upload_handoff`]) re-stamps the row onto
/// the target (status `queued`); `Failed` clears the move and posts
/// `reason`.
pub fn handoff(
    trpc: &TrpcClient,
    session_id: &str,
    device_id: &str,
    outcome: HandoffOutcome,
    reason: Option<&str>,
) -> Result<(), ApiError> {
    let _: serde::de::IgnoredAny = trpc.mutation(
        "sessions.handoff",
        &HandoffInput { session_id, device_id, outcome, reason },
    )?;
    Ok(())
}

/// EXP-1252: `sessions.handoffDone` — mutation, the NEW host restored the
/// package: the server deletes the blob.
pub fn handoff_done(trpc: &TrpcClient, session_id: &str) -> Result<(), ApiError> {
    let _: serde::de::IgnoredAny =
        trpc.mutation("sessions.handoffDone", &SessionIdInput { session_id })?;
    Ok(())
}

/// EXP-1252: the server's cap on a handoff package (64 MiB; larger = 413).
pub const HANDOFF_MAX_BYTES: u64 = 64 * 1024 * 1024;

/// EXP-1252: the blob route of a session's handoff package.
pub fn handoff_path(session_id: &str) -> String {
    format!("/api/sessions/{}/handoff", crate::encode::percent_encode(session_id))
}

/// EXP-1252: `PUT /api/sessions/<id>/handoff` — the source device uploads
/// the gzip package (≤ [`HANDOFF_MAX_BYTES`]) while the move is pending.
pub fn upload_handoff(trpc: &TrpcClient, session_id: &str, package: Vec<u8>) -> Result<(), ApiError> {
    trpc.put_bytes(&handoff_path(session_id), package, "application/gzip")
}

/// EXP-1252: `GET /api/sessions/<id>/handoff` — the new host fetches the
/// package (404 = none).
pub fn download_handoff(trpc: &TrpcClient, session_id: &str) -> Result<Vec<u8>, ApiError> {
    trpc.get_bytes_within(&handoff_path(session_id), crate::trpc::TRANSFER_TIMEOUT)
        .map(|(bytes, _)| bytes)
}

/// The status a device may report (`sessionStatusSchema` minus `queued`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ReportStatus {
    Running,
    Idle,
    Ended,
}

/// The usage wall a device saw (EXP-804) — the server's
/// `codingSessionBlockedSchema` wire shape. EXP-1292: owned (it now rides
/// [`SessionReport`], which hosts keep across ticks).
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BlockedInput {
    pub kind: String,
    pub agent: String,
    pub window: String,
    pub resets_at: Option<String>,
    pub since: String,
    /// EXP-1005: `true` = this device handles the wall itself (rotates the
    /// run to another account, or waits the reset out), so the server sends
    /// the owner no rate-limit notification. Absent = the server notifies.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub handled: Option<bool>,
}

/// `sessions.report` input. Every `None` stays off the wire (unchanged);
/// `blocked: Some(None)` sends an explicit `null` (the wall cleared).
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionReport {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<ReportStatus>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub busy: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub needs_input: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blocked: Option<Option<BlockedInput>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// EXP-849: the login (profile id) the run ACTUALLY runs on — a start
    /// pick or a rotation may move it off the row's `account`. An older
    /// server strips the key (its zod object is not strict).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub account: Option<String>,
    /// EXP-1331: the live status line (`Editing ProductCard.tsx`, <=80
    /// chars). `Some(None)` sends an explicit `null` (cleared at turn end
    /// and session end).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub caption: Option<Option<String>>,
    /// EXP-1334: the row's `plan_mode` as the run is in it. EXP-1280: no
    /// device writes it any more (plan mode is Exponential's: a person's
    /// `sessions.configure` or the server's approval moves the row); kept
    /// for the wire.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plan_mode: Option<bool>,
    /// EXP-1334: the agent's model as the run is on it (a `/model` switch
    /// mid-session), so a resume launches on it. The server ignores one
    /// outside the agent's vocabulary.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// EXP-1252: the reporting device. A report from a device the row no
    /// longer names (a moved session's late write) is a no-op server-side.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub device_id: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ReportInput<'a> {
    session_id: &'a str,
    #[serde(flatten)]
    report: &'a SessionReport,
}

/// `sessions.report` — mutation. One write for every device-owned field
/// (replaces the old per-field `codingSessions.set*` setters and the
/// heartbeat). Fire-and-forget for the tickers: they retry an unlanded
/// write on their next edge.
pub fn report(trpc: &TrpcClient, session_id: &str, report: &SessionReport) -> Result<(), ApiError> {
    let _: serde::de::IgnoredAny =
        trpc.mutation("sessions.report", &ReportInput { session_id, report })?;
    Ok(())
}

/// A card on an agent reply — serializes to the server's `MessageCard` JSON
/// (`kind` tag, camelCase keys). `questionId` rides the tolerant passthrough
/// card schema so an answered card maps back to the engine's pending
/// question without device-side state.
///
/// Serialized by hand (not derived): a question card's options go out as
/// `options: string[]` (labels) plus the EXP-1322 `optionDescriptions`
/// sidecar, see [`option_descriptions`].
#[derive(Clone, Debug, PartialEq)]
pub enum ReplyCard {
    Question {
        question: String,
        options: Vec<ReplyOption>,
        question_id: String,
    },
    Subagent {
        title: String,
        status: String,
        summary: Option<String>,
    },
}

impl serde::Serialize for ReplyCard {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeMap;
        match self {
            Self::Question {
                question,
                options,
                question_id,
            } => {
                // EXP-1322: options post as label strings; descriptions ride
                // the `optionDescriptions` sidecar, never inside `options`.
                let labels: Vec<&str> = options.iter().map(ReplyOption::label).collect();
                let descriptions = option_descriptions(options);
                let mut map =
                    serializer.serialize_map(Some(4 + usize::from(descriptions.is_some())))?;
                map.serialize_entry("kind", "question")?;
                map.serialize_entry("question", question)?;
                map.serialize_entry("options", &labels)?;
                if let Some(descriptions) = &descriptions {
                    map.serialize_entry("optionDescriptions", descriptions)?;
                }
                map.serialize_entry("questionId", question_id)?;
                map.end()
            }
            Self::Subagent {
                title,
                status,
                summary,
            } => {
                let mut map = serializer.serialize_map(Some(3 + usize::from(summary.is_some())))?;
                map.serialize_entry("kind", "subagent")?;
                map.serialize_entry("title", title)?;
                map.serialize_entry("status", status)?;
                if let Some(summary) = summary {
                    map.serialize_entry("summary", summary)?;
                }
                map.end()
            }
        }
    }
}

/// EXP-1322: one option of a question card: its label and, when the feed
/// option carries one, its description (the thread card shows it like the
/// session view's card). The answer names the option by its label.
///
/// On the wire the card posts `options` as plain label strings and the
/// descriptions in the parallel `optionDescriptions` sidecar
/// ([`option_descriptions`]); the enum itself never goes on the wire.
#[derive(Clone, Debug, PartialEq)]
pub enum ReplyOption {
    Label(String),
    Described { label: String, description: String },
}

impl ReplyOption {
    /// A feed option: described only when the description carries text.
    pub fn new(label: impl Into<String>, description: Option<&str>) -> Self {
        let label = label.into();
        match description.map(str::trim).filter(|text| !text.is_empty()) {
            Some(description) => Self::Described { label, description: description.to_string() },
            None => Self::Label(label),
        }
    }

    pub fn label(&self) -> &str {
        match self {
            Self::Label(label) | Self::Described { label, .. } => label,
        }
    }

    /// The description, `None` for a plain label.
    pub fn description(&self) -> Option<&str> {
        match self {
            Self::Label(_) => None,
            Self::Described { description, .. } => Some(description),
        }
    }
}

/// The EXP-1322 `optionDescriptions` sidecar of a question card: parallel to
/// `options` (same length), `None` = that option has no description; `None`
/// altogether (the key is omitted) when no option carries one.
pub fn option_descriptions(options: &[ReplyOption]) -> Option<Vec<Option<&str>>> {
    let descriptions: Vec<Option<&str>> = options.iter().map(ReplyOption::description).collect();
    descriptions.iter().any(Option::is_some).then_some(descriptions)
}

impl From<&str> for ReplyOption {
    fn from(label: &str) -> Self {
        Self::Label(label.to_string())
    }
}

impl From<String> for ReplyOption {
    fn from(label: String) -> Self {
        Self::Label(label)
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ReplyInput<'a> {
    session_id: &'a str,
    body: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    card: Option<&'a ReplyCard>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct MessageIdOutput {
    message_id: String,
}

/// `sessions.reply` — mutation: the agent's reply text (a turn's FINAL
/// message; may be empty beside a card) and/or a card. Returns the new
/// `messages` row id.
pub fn reply(
    trpc: &TrpcClient,
    session_id: &str,
    body: &str,
    card: Option<&ReplyCard>,
) -> Result<String, ApiError> {
    let output: MessageIdOutput = trpc.mutation(
        "sessions.reply",
        &ReplyInput {
            session_id,
            body,
            card,
        },
    )?;
    Ok(output.message_id)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ConsumeInput<'a> {
    session_id: &'a str,
    message_ids: &'a [String],
}

/// `sessions.consume` — mutation: these person messages reached the agent.
/// Chunked to [`CONSUME_CHUNK`] ids per call; an empty slice is a no-op (the
/// server refuses `[]`). Stops at the first failing chunk.
pub fn consume(trpc: &TrpcClient, session_id: &str, message_ids: &[String]) -> Result<(), ApiError> {
    for chunk in message_ids.chunks(CONSUME_CHUNK) {
        let _: serde::de::IgnoredAny = trpc.mutation(
            "sessions.consume",
            &ConsumeInput {
                session_id,
                message_ids: chunk,
            },
        )?;
    }
    Ok(())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PostMessageInput<'a> {
    session_id: &'a str,
    text: &'a str,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SendOutput {
    #[serde(default)]
    message_id: Option<String>,
}

/// `sessions.send({sessionId, text})` — a trigger firing posted into the
/// agent's own channel (the agent wakes on it like on any message). Returns
/// the `messages` row id.
pub fn post_message(trpc: &TrpcClient, session_id: &str, text: &str) -> Result<String, ApiError> {
    let output: SendOutput =
        trpc.mutation("sessions.send", &PostMessageInput { session_id, text })?;
    output
        .message_id
        .ok_or_else(|| ApiError::Decode("sessions.send: no messageId".to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::trpc::tests::one_shot_server;
    use crate::StaticToken;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::sync::Arc;
    use std::time::Duration;

    const SESSION: &str = "11111111-1111-4111-8111-111111111111";

    fn client(base: &str) -> TrpcClient {
        TrpcClient::new(base, Arc::new(StaticToken("tok".to_string())))
    }

    fn body_of(request: &str) -> &str {
        request.split("\r\n\r\n").nth(1).unwrap_or_default()
    }

    #[test]
    fn brief_queries_with_the_session_id_and_decodes_the_layers() {
        let (base, captured) = one_shot_server(
            200,
            r#"{"result":{"data":{"brief":"Work on EXP-1.","teamPrompt":"Be terse.","agentInstructions":null,"parentBranch":"exp/EXP-0"}}}"#,
        );
        let brief = brief(&client(&base), SESSION).unwrap();
        assert_eq!(
            brief,
            SessionBrief {
                brief: "Work on EXP-1.".to_string(),
                team_prompt: Some("Be terse.".to_string()),
                agent_instructions: None,
                parent_branch: Some("exp/EXP-0".to_string()),
                handoff: None,
            }
        );
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        // `{"sessionId":"…"}` percent-encoded into the query string.
        assert!(
            request.starts_with(&format!(
                "GET /api/trpc/sessions.brief?input=%7B%22sessionId%22%3A%22{SESSION}%22%7D HTTP/1.1"
            )),
            "{request}"
        );
    }

    #[test]
    fn claim_posts_the_session_and_omits_an_absent_branch() {
        let (base, captured) = one_shot_server(200, r#"{"result":{"data":{"txid":12}}}"#);
        claim(&client(&base), SESSION, None, None).unwrap();
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(request.starts_with("POST /api/trpc/sessions.claim HTTP/1.1"));
        assert_eq!(body_of(&request), format!(r#"{{"sessionId":"{SESSION}"}}"#));

        let (base, captured) = one_shot_server(200, r#"{"result":{"data":{"txid":13}}}"#);
        claim(&client(&base), SESSION, Some("dev-1"), Some("exp/EXP-1")).unwrap();
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        assert_eq!(
            body_of(&request),
            format!(r#"{{"sessionId":"{SESSION}","deviceId":"dev-1","branch":"exp/EXP-1"}}"#)
        );
    }

    #[test]
    fn handoff_posts_the_source_verdict() {
        let (base, captured) = one_shot_server(200, r#"{"result":{"data":{"txid":3}}}"#);
        handoff(&client(&base), SESSION, "dev-1", HandoffOutcome::Moved, None).unwrap();
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(request.starts_with("POST /api/trpc/sessions.handoff HTTP/1.1"));
        assert_eq!(
            body_of(&request),
            format!(r#"{{"sessionId":"{SESSION}","deviceId":"dev-1","outcome":"moved"}}"#)
        );

        let (base, captured) = one_shot_server(200, r#"{"result":{"data":{"txid":4}}}"#);
        handoff(&client(&base), SESSION, "dev-1", HandoffOutcome::Failed, Some("Too big")).unwrap();
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        assert_eq!(
            body_of(&request),
            format!(r#"{{"sessionId":"{SESSION}","deviceId":"dev-1","outcome":"failed","reason":"Too big"}}"#)
        );

        let (base, captured) = one_shot_server(200, r#"{"result":{"data":{"txid":5}}}"#);
        handoff_done(&client(&base), SESSION).unwrap();
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(request.starts_with("POST /api/trpc/sessions.handoffDone HTTP/1.1"));
        assert_eq!(body_of(&request), format!(r#"{{"sessionId":"{SESSION}"}}"#));
    }

    #[test]
    fn the_handoff_package_puts_and_gets_on_the_blob_route() {
        let (base, captured) = one_shot_server(200, "{}");
        upload_handoff(&client(&base), SESSION, b"\x1f\x8bpkg".to_vec()).unwrap();
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(
            request.starts_with(&format!("PUT /api/sessions/{SESSION}/handoff HTTP/1.1")),
            "{request}"
        );
        assert!(request.to_ascii_lowercase().contains("content-type: application/gzip"), "{request}");
        assert!(request.contains("Bearer tok"), "{request}");

        let (base, _captured) = one_shot_server(413, r#"{"error":"too large"}"#);
        assert!(matches!(
            upload_handoff(&client(&base), SESSION, vec![0; 4]),
            Err(ApiError::Http { status: 413, .. })
        ));

        let (base, captured) =
            crate::trpc::tests::one_shot_server_typed(200, "application/gzip", "PKG");
        assert_eq!(download_handoff(&client(&base), SESSION).unwrap(), b"PKG".to_vec());
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(request.starts_with(&format!("GET /api/sessions/{SESSION}/handoff HTTP/1.1")));
    }

    #[test]
    fn the_brief_decodes_a_handoff() {
        let brief: SessionBrief = serde_json::from_str(
            r#"{"brief":"x","handoff":{"sizeBytes":12,"fromDeviceId":"dev-a","fromDeviceName":"Laptop"}}"#,
        )
        .unwrap();
        assert_eq!(
            brief.handoff,
            Some(BriefHandoff {
                size_bytes: 12,
                from_device_id: "dev-a".to_string(),
                from_device_name: "Laptop".to_string(),
            })
        );
        let plain: SessionBrief = serde_json::from_str(r#"{"brief":"x","handoff":null}"#).unwrap();
        assert_eq!(plain.handoff, None);
    }

    #[test]
    fn a_refused_claim_is_an_error() {
        let (base, _captured) = one_shot_server(
            409,
            r#"{"error":{"message":"Session already claimed","code":-32009,"data":{"code":"CONFLICT","httpStatus":409}}}"#,
        );
        assert!(claim(&client(&base), SESSION, None, None).is_err());
    }

    #[test]
    fn report_sends_only_the_set_fields() {
        let (base, captured) = one_shot_server(200, r#"{"result":{"data":{"txid":1}}}"#);
        report(
            &client(&base),
            SESSION,
            &SessionReport {
                status: Some(ReportStatus::Idle),
                busy: Some(false),
                ..Default::default()
            },
        )
        .unwrap();
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(request.starts_with("POST /api/trpc/sessions.report HTTP/1.1"));
        assert_eq!(
            body_of(&request),
            format!(r#"{{"sessionId":"{SESSION}","status":"idle","busy":false}}"#)
        );
    }

    #[test]
    fn report_serializes_every_field_and_the_blocked_tristate() {
        let wall = SessionReport {
            status: Some(ReportStatus::Running),
            busy: None,
            needs_input: Some(true),
            blocked: Some(Some(BlockedInput {
                kind: "rate_limit".to_string(),
                agent: "claude".to_string(),
                window: "session".to_string(),
                resets_at: Some("2026-10-13T12:00:00Z".to_string()),
                since: "2026-10-13T10:00:00Z".to_string(),
                handled: Some(true),
            })),
            branch: Some("exp/EXP-1".to_string()),
            title: Some("Fix the login".to_string()),
            account: Some("0a1b2c3d".to_string()),
            caption: Some(Some("Editing cart.rs".to_string())),
            plan_mode: Some(false),
            model: Some("opus".to_string()),
            device_id: Some("dev-1".to_string()),
        };
        let input = ReportInput {
            session_id: "s1",
            report: &wall,
        };
        assert_eq!(
            serde_json::to_string(&input).unwrap(),
            r#"{"sessionId":"s1","status":"running","needsInput":true,"blocked":{"kind":"rate_limit","agent":"claude","window":"session","resetsAt":"2026-10-13T12:00:00Z","since":"2026-10-13T10:00:00Z","handled":true},"branch":"exp/EXP-1","title":"Fix the login","account":"0a1b2c3d","caption":"Editing cart.rs","planMode":false,"model":"opus","deviceId":"dev-1"}"#
        );

        // Some(None) = an explicit null (the wall cleared); a no-reset wall
        // keeps `resetsAt: null` and drops an unknown `handled`.
        let cleared = SessionReport {
            blocked: Some(None),
            ..Default::default()
        };
        assert_eq!(
            serde_json::to_string(&ReportInput { session_id: "s1", report: &cleared }).unwrap(),
            r#"{"sessionId":"s1","blocked":null}"#
        );
        // EXP-1331: the caption's clear is an explicit null too.
        let uncaptioned = SessionReport {
            caption: Some(None),
            ..Default::default()
        };
        assert_eq!(
            serde_json::to_string(&ReportInput { session_id: "s1", report: &uncaptioned }).unwrap(),
            r#"{"sessionId":"s1","caption":null}"#
        );
        let open_ended = BlockedInput {
            kind: "rate_limit".to_string(),
            agent: "codex".to_string(),
            window: "weekly".to_string(),
            resets_at: None,
            since: "t".to_string(),
            handled: None,
        };
        assert_eq!(
            serde_json::to_string(&open_ended).unwrap(),
            r#"{"kind":"rate_limit","agent":"codex","window":"weekly","resetsAt":null,"since":"t"}"#
        );
        assert_eq!(
            serde_json::to_string(&ReportStatus::Ended).unwrap(),
            r#""ended""#
        );
    }

    #[test]
    fn reply_posts_body_and_card_and_returns_the_message_id() {
        let (base, captured) = one_shot_server(
            200,
            r#"{"result":{"data":{"messageId":"m-1","txid":4}}}"#,
        );
        let card = ReplyCard::Question {
            question: "Which DB?".to_string(),
            options: vec![
                ReplyOption::new("Postgres", Some("Managed, shared")),
                ReplyOption::new("SQLite", None),
            ],
            question_id: "q-1".to_string(),
        };
        let id = reply(&client(&base), SESSION, "", Some(&card)).unwrap();
        assert_eq!(id, "m-1");
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(request.starts_with("POST /api/trpc/sessions.reply HTTP/1.1"));
        assert_eq!(
            body_of(&request),
            format!(
                r#"{{"sessionId":"{SESSION}","body":"","card":{{"kind":"question","question":"Which DB?","options":["Postgres","SQLite"],"optionDescriptions":["Managed, shared",null],"questionId":"q-1"}}}}"#
            )
        );

        let (base, captured) = one_shot_server(
            200,
            r#"{"result":{"data":{"messageId":"m-2","txid":5}}}"#,
        );
        assert_eq!(reply(&client(&base), SESSION, "Done.", None).unwrap(), "m-2");
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        assert_eq!(
            body_of(&request),
            format!(r#"{{"sessionId":"{SESSION}","body":"Done."}}"#)
        );
    }

    #[test]
    fn reply_cards_match_the_message_card_wire() {
        let json = |card: &ReplyCard| serde_json::to_string(card).unwrap();
        // EXP-1322 sidecar: options post as labels; descriptions ride the
        // parallel `optionDescriptions` (null = none), omitted when no option
        // carries one.
        assert_eq!(
            json(&ReplyCard::Question {
                question: "Ship?".to_string(),
                options: vec![
                    ReplyOption::new("Now", Some("Behind a flag")),
                    ReplyOption::new("Later", Some("  ")),
                    ReplyOption::new("Never", None),
                ],
                question_id: "q-3".to_string(),
            }),
            r#"{"kind":"question","question":"Ship?","options":["Now","Later","Never"],"optionDescriptions":["Behind a flag",null,null],"questionId":"q-3"}"#
        );
        assert_eq!(
            json(&ReplyCard::Question {
                question: "Ship?".to_string(),
                options: vec!["Now".into(), ReplyOption::new("Never", Some(" "))],
                question_id: "q-4".to_string(),
            }),
            r#"{"kind":"question","question":"Ship?","options":["Now","Never"],"questionId":"q-4"}"#
        );
        assert_eq!(
            json(&ReplyCard::Question {
                question: "Anything?".to_string(),
                options: vec![],
                question_id: "q-5".to_string(),
            }),
            r#"{"kind":"question","question":"Anything?","options":[],"questionId":"q-5"}"#
        );
        assert_eq!(ReplyOption::new("Now", Some("x")).label(), "Now");
        assert_eq!(ReplyOption::new("Now", Some("x")).description(), Some("x"));
        assert_eq!(ReplyOption::new("Now", None).description(), None);
        assert_eq!(option_descriptions(&["A".into(), "B".into()]), None);
        assert_eq!(
            json(&ReplyCard::Subagent {
                title: "Review".to_string(),
                status: "completed".to_string(),
                summary: None,
            }),
            r#"{"kind":"subagent","title":"Review","status":"completed"}"#
        );
        assert_eq!(
            json(&ReplyCard::Subagent {
                title: "Review".to_string(),
                status: "failed".to_string(),
                summary: Some("No access".to_string()),
            }),
            r#"{"kind":"subagent","title":"Review","status":"failed","summary":"No access"}"#
        );
        for card in [
            ReplyCard::Question { question: String::new(), options: vec![], question_id: String::new() },
            ReplyCard::Subagent { title: String::new(), status: String::new(), summary: None },
        ] {
            let value = serde_json::to_value(&card).unwrap();
            let kind = value["kind"].as_str().unwrap();
            assert!(domain::contract::MESSAGE_CARD_KIND_VALUES.contains(&kind), "{kind}");
        }
    }

    /// A server answering `n` sequential requests (`Connection: close`, so
    /// every call dials anew), capturing each request.
    fn serve_n(n: usize, body: &'static str) -> (String, flume::Receiver<String>) {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        let (tx, rx) = flume::unbounded::<String>();
        std::thread::spawn(move || {
            for _ in 0..n {
                let (mut stream, _) = listener.accept().unwrap();
                stream.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
                let mut captured = Vec::new();
                let mut buf = [0u8; 8192];
                while let Ok(read) = stream.read(&mut buf) {
                    if read == 0 {
                        break;
                    }
                    captured.extend_from_slice(&buf[..read]);
                    let text = String::from_utf8_lossy(&captured);
                    if let Some((head, rest)) = text.split_once("\r\n\r\n") {
                        let length = head
                            .lines()
                            .find_map(|line| {
                                let (name, value) = line.split_once(':')?;
                                name.eq_ignore_ascii_case("content-length")
                                    .then(|| value.trim().parse::<usize>().ok())?
                            })
                            .unwrap_or(0);
                        if rest.len() >= length {
                            break;
                        }
                    }
                }
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                stream.write_all(response.as_bytes()).unwrap();
                let _ = tx.send(String::from_utf8_lossy(&captured).into_owned());
            }
        });
        (format!("http://127.0.0.1:{port}"), rx)
    }

    #[test]
    fn consume_chunks_at_one_hundred_ids() {
        let ids: Vec<String> = (0..150).map(|i| format!("m-{i}")).collect();
        let (base, captured) = serve_n(2, r#"{"result":{"data":{"txid":9}}}"#);
        consume(&client(&base), SESSION, &ids).unwrap();
        let mut sizes = Vec::new();
        for _ in 0..2 {
            let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
            assert!(request.starts_with("POST /api/trpc/sessions.consume HTTP/1.1"));
            let body: serde_json::Value = serde_json::from_str(body_of(&request)).unwrap();
            assert_eq!(body["sessionId"], SESSION);
            sizes.push(body["messageIds"].as_array().unwrap().len());
        }
        assert_eq!(sizes, vec![100, 50]);
        // An empty slice never reaches the server (it refuses `[]`).
        consume(&client("http://127.0.0.1:9"), SESSION, &[]).unwrap();
    }

    #[test]
    fn consume_posts_the_ids_in_order() {
        let (base, captured) = one_shot_server(200, r#"{"result":{"data":{"txid":9}}}"#);
        consume(
            &client(&base),
            SESSION,
            &["m-1".to_string(), "m-2".to_string()],
        )
        .unwrap();
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        assert_eq!(
            body_of(&request),
            format!(r#"{{"sessionId":"{SESSION}","messageIds":["m-1","m-2"]}}"#)
        );
    }

    #[test]
    fn post_message_sends_the_firing_into_the_agent() {
        let (base, captured) = one_shot_server(
            200,
            r#"{"result":{"data":{"sessionId":"agent-1","messageId":"m-7","txid":3}}}"#,
        );
        let id = post_message(&client(&base), "agent-1", "## Trigger\nEvery hour").unwrap();
        assert_eq!(id, "m-7");
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(request.starts_with("POST /api/trpc/sessions.send HTTP/1.1"));
        assert_eq!(
            body_of(&request),
            r###"{"sessionId":"agent-1","text":"## Trigger\nEvery hour"}"###
        );

        // A send that made no message (never expected for a text send) is a
        // decode error, not a silent empty id.
        let (base, _captured) = one_shot_server(
            200,
            r#"{"result":{"data":{"sessionId":"agent-1","messageId":null,"txid":3}}}"#,
        );
        assert!(matches!(
            post_message(&client(&base), "agent-1", "x"),
            Err(ApiError::Decode(_))
        ));
    }
}
