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

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ClaimInput<'a> {
    session_id: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    branch: Option<&'a str>,
}

/// `sessions.claim` — mutation, queued → running for THIS device. The host
/// calls it before preparing the launch (normally `branch: None`; the
/// launcher reports the branch it chose via [`report`] once the worktree
/// exists). A refusal (another device claimed it, the row ended) is an
/// error the host turns into "skip this row".
pub fn claim(trpc: &TrpcClient, session_id: &str, branch: Option<&str>) -> Result<(), ApiError> {
    let _: serde::de::IgnoredAny =
        trpc.mutation("sessions.claim", &ClaimInput { session_id, branch })?;
    Ok(())
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
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum ReplyCard {
    Question {
        question: String,
        options: Vec<String>,
        #[serde(rename = "questionId")]
        question_id: String,
    },
    Plan {
        plan: String,
        #[serde(rename = "questionId")]
        question_id: String,
    },
    Subagent {
        title: String,
        status: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        summary: Option<String>,
    },
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
        claim(&client(&base), SESSION, None).unwrap();
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(request.starts_with("POST /api/trpc/sessions.claim HTTP/1.1"));
        assert_eq!(body_of(&request), format!(r#"{{"sessionId":"{SESSION}"}}"#));

        let (base, captured) = one_shot_server(200, r#"{"result":{"data":{"txid":13}}}"#);
        claim(&client(&base), SESSION, Some("exp/EXP-1")).unwrap();
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        assert_eq!(
            body_of(&request),
            format!(r#"{{"sessionId":"{SESSION}","branch":"exp/EXP-1"}}"#)
        );
    }

    #[test]
    fn a_refused_claim_is_an_error() {
        let (base, _captured) = one_shot_server(
            409,
            r#"{"error":{"message":"Session already claimed","code":-32009,"data":{"code":"CONFLICT","httpStatus":409}}}"#,
        );
        assert!(claim(&client(&base), SESSION, None).is_err());
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
            ..Default::default()
        };
        let input = ReportInput {
            session_id: "s1",
            report: &wall,
        };
        assert_eq!(
            serde_json::to_string(&input).unwrap(),
            r#"{"sessionId":"s1","status":"running","needsInput":true,"blocked":{"kind":"rate_limit","agent":"claude","window":"session","resetsAt":"2026-10-13T12:00:00Z","since":"2026-10-13T10:00:00Z","handled":true},"branch":"exp/EXP-1","title":"Fix the login"}"#
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
            options: vec!["Postgres".to_string(), "SQLite".to_string()],
            question_id: "q-1".to_string(),
        };
        let id = reply(&client(&base), SESSION, "", Some(&card)).unwrap();
        assert_eq!(id, "m-1");
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(request.starts_with("POST /api/trpc/sessions.reply HTTP/1.1"));
        assert_eq!(
            body_of(&request),
            format!(
                r#"{{"sessionId":"{SESSION}","body":"","card":{{"kind":"question","question":"Which DB?","options":["Postgres","SQLite"],"questionId":"q-1"}}}}"#
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
        assert_eq!(
            json(&ReplyCard::Plan {
                plan: "1. Do it".to_string(),
                question_id: "q-2".to_string(),
            }),
            r#"{"kind":"plan","plan":"1. Do it","questionId":"q-2"}"#
        );
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
            ReplyCard::Plan { plan: String::new(), question_id: String::new() },
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
