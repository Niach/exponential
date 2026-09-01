//! Typed `steer.*` tRPC mirrors (masterplan-v3 §8.2/§8.3) — the desktop's
//! ticket-CONSUMER surface. Shapes verified against
//! `apps/web/src/lib/trpc/steer.ts` (+ the pure core `apps/web/src/lib/steer.ts`):
//!
//! - `steer.config` — **query**, no input → `{enabled, relayUrl}`. Clients
//!   poll this before dialing anything; `enabled: false` is a normal state,
//!   never an error (an unconfigured instance generates no noise).
//! - `steer.mintTicket` — **mutation**, discriminated on `kind`
//!   (`control` / `publisher` / `viewer`) → `{disabled: true}` or
//!   `{ticket, url}` where `url` is the FULL ws(s) dial URL with
//!   `?ticket=…` already embedded (`steerTicketUrl`). Consumers use `url`
//!   **as-is** — never reconstruct it (the relay reads the ticket from the
//!   query string only).
//!
//! The desktop is NEVER a signer: it holds no `STEER_RELAY_SECRET` and never
//! touches `signSteerTicket`. All authorization is decided server-side at
//! mint time (§8.0).

use serde::{Deserialize, Serialize};

use crate::error::ApiError;
use crate::trpc::TrpcClient;

/// `steer.config` output — whether remote start + live steering is available
/// on this instance (enabled iff BOTH `STEER_RELAY_URL` and
/// `STEER_RELAY_SECRET` are set server-side).
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SteerConfig {
    pub enabled: bool,
    #[serde(default)]
    pub relay_url: Option<String>,
}

/// `steer.config` — query, no input.
pub fn config(trpc: &TrpcClient) -> Result<SteerConfig, ApiError> {
    trpc.query("steer.config")
}

/// A server-minted relay ticket + the full dial URL (60s connect window —
/// dial IMMEDIATELY, §8.7).
#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct MintedTicket {
    pub ticket: String,
    pub url: String,
}

/// `steer.mintTicket` output: relay-disabled is a *result*, not an error, so
/// pollers never treat an unconfigured instance as a failure.
#[derive(Clone, Debug, PartialEq)]
pub enum MintTicketResult {
    Disabled,
    Ticket(MintedTicket),
}

impl MintTicketResult {
    /// `Some(ticket)` when the relay is enabled.
    pub fn into_ticket(self) -> Option<MintedTicket> {
        match self {
            MintTicketResult::Ticket(ticket) => Some(ticket),
            MintTicketResult::Disabled => None,
        }
    }
}

#[derive(Deserialize)]
#[serde(untagged)]
enum MintWire {
    Ticket {
        ticket: String,
        url: String,
    },
    Disabled {
        /// Read only by serde's untagged matcher (`{"disabled": true}`).
        #[allow(dead_code)]
        disabled: bool,
    },
}

#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
enum MintInput<'a> {
    Control {
        #[serde(rename = "deviceLabel", skip_serializing_if = "Option::is_none")]
        device_label: Option<&'a str>,
    },
    // EXP-707: the wire name is `sessionId` (renamed from `codingSessionId`).
    Publisher {
        #[serde(rename = "sessionId")]
        coding_session_id: &'a str,
    },
    Viewer {
        #[serde(rename = "sessionId")]
        coding_session_id: &'a str,
    },
}

fn mint(trpc: &TrpcClient, input: &MintInput<'_>) -> Result<MintTicketResult, ApiError> {
    let wire: MintWire = trpc.mutation("steer.mintTicket", input)?;
    Ok(match wire {
        MintWire::Ticket { ticket, url } => MintTicketResult::Ticket(MintedTicket { ticket, url }),
        MintWire::Disabled { .. } => MintTicketResult::Disabled,
    })
}

/// `steer.mintTicket({kind: "control", deviceLabel?})` — the device-presence
/// socket ticket (§8.3). Any authed user may register presence for their own
/// account.
pub fn mint_control_ticket(
    trpc: &TrpcClient,
    device_label: Option<&str>,
) -> Result<MintTicketResult, ApiError> {
    mint(trpc, &MintInput::Control { device_label })
}

/// `steer.mintTicket({kind: "publisher", sessionId})` — the per-session
/// PTY publisher ticket (§8.4). The server checks `session.userId === caller`
/// (only the owner's desktop may publish).
pub fn mint_publisher_ticket(
    trpc: &TrpcClient,
    coding_session_id: &str,
) -> Result<MintTicketResult, ApiError> {
    mint(trpc, &MintInput::Publisher { coding_session_id })
}

/// `steer.mintTicket({kind: "viewer", sessionId})` — watch/steer a
/// session from this client (team members; owners get perm `steer`).
pub fn mint_viewer_ticket(
    trpc: &TrpcClient,
    coding_session_id: &str,
) -> Result<MintTicketResult, ApiError> {
    mint(trpc, &MintInput::Viewer { coding_session_id })
}

/// `steer.startSession` input (EXP-696) — the same remote-start payload web
/// and mobile send. Exactly one of `issue_id` / `issue_ids` / `action_id` /
/// `resume_session_id` must be set; `team_id` rides built-in action starts
/// only; `inputs` rides action starts only (BTreeMap for a deterministic
/// wire order). Absent options mean "target device's defaults".
#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StartSessionInput {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub issue_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub issue_ids: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub action_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub team_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub inputs: Option<std::collections::BTreeMap<String, String>>,
    pub device_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub effort: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ultracode: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plan_mode: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resume: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resume_session_id: Option<String>,
}

#[derive(Deserialize)]
struct OkWire {
    /// Read only by serde (the mutation returns `{"ok": true}`).
    #[allow(dead_code)]
    ok: bool,
}

/// `steer.startSession` — remote-start a run on another of the caller's
/// devices (or a shared server device). The server validates the subject,
/// the target's registered agents and the option vocabulary, then posts the
/// `start_session` frame to the device's control socket. `PRECONDITION_FAILED`
/// when the relay is off or the device is offline/unregistered.
pub fn start_session(trpc: &TrpcClient, input: &StartSessionInput) -> Result<(), ApiError> {
    let _: OkWire = trpc.mutation("steer.startSession", input)?;
    Ok(())
}

/// `steer.killSession` — end a live session (owner or hosting-device owner):
/// flips the row to `ended`/`ended_by: user` and best-effort kills the relay
/// room. Idempotent on already-ended rows. The synced row edge is the
/// authoritative confirmation; the returned row is ignored here.
pub fn kill_session(trpc: &TrpcClient, coding_session_id: &str) -> Result<(), ApiError> {
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Input<'a> {
        /// EXP-707: the wire name is `sessionId` (renamed from
        /// `codingSessionId`).
        session_id: &'a str,
    }
    let _: serde_json::Value = trpc.mutation(
        "steer.killSession",
        &Input { session_id: coding_session_id },
    )?;
    Ok(())
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
    fn config_decodes_enabled_and_uses_get() {
        let (base, captured) = one_shot_server(
            200,
            r#"{"result":{"data":{"enabled":true,"relayUrl":"http://relay.lan:4002"}}}"#,
        );
        let config = config(&client(&base)).unwrap();
        assert!(config.enabled);
        assert_eq!(config.relay_url.as_deref(), Some("http://relay.lan:4002"));
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(request.starts_with("GET /api/trpc/steer.config HTTP/1.1"));
    }

    #[test]
    fn config_decodes_disabled_null_url() {
        let (base, _captured) =
            one_shot_server(200, r#"{"result":{"data":{"enabled":false,"relayUrl":null}}}"#);
        let config = config(&client(&base)).unwrap();
        assert!(!config.enabled);
        assert_eq!(config.relay_url, None);
    }

    #[test]
    fn mint_control_posts_kind_and_decodes_ticket() {
        let (base, captured) = one_shot_server(
            200,
            r#"{"result":{"data":{"ticket":"abc.def","url":"ws://relay.lan:4002/ws?ticket=abc.def"}}}"#,
        );
        let result = mint_control_ticket(&client(&base), Some("MacBook")).unwrap();
        assert_eq!(
            result,
            MintTicketResult::Ticket(MintedTicket {
                ticket: "abc.def".to_string(),
                url: "ws://relay.lan:4002/ws?ticket=abc.def".to_string(),
            })
        );
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(request.starts_with("POST /api/trpc/steer.mintTicket HTTP/1.1"));
        assert!(request.ends_with(r#"{"kind":"control","deviceLabel":"MacBook"}"#));
    }

    #[test]
    fn mint_control_omits_absent_device_label() {
        let (base, captured) = one_shot_server(
            200,
            r#"{"result":{"data":{"ticket":"t","url":"ws://r/ws?ticket=t"}}}"#,
        );
        let _ = mint_control_ticket(&client(&base), None).unwrap();
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(request.ends_with(r#"{"kind":"control"}"#));
    }

    #[test]
    fn mint_publisher_posts_session_id_and_decodes_disabled() {
        let (base, captured) = one_shot_server(200, r#"{"result":{"data":{"disabled":true}}}"#);
        let result = mint_publisher_ticket(
            &client(&base),
            "3f0f5a2e-1d4b-4c1e-9f6a-000000000001",
        )
        .unwrap();
        assert_eq!(result, MintTicketResult::Disabled);
        assert_eq!(result.clone().into_ticket(), None);
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(request.ends_with(
            r#"{"kind":"publisher","sessionId":"3f0f5a2e-1d4b-4c1e-9f6a-000000000001"}"#
        ));
    }

    #[test]
    fn start_session_posts_action_subject_with_device() {
        let (base, captured) = one_shot_server(200, r#"{"result":{"data":{"ok":true}}}"#);
        let mut inputs = std::collections::BTreeMap::new();
        inputs.insert("prompt".to_string(), "do the thing".to_string());
        start_session(
            &client(&base),
            &StartSessionInput {
                action_id: Some("11111111-1111-4111-8111-111111111111".to_string()),
                inputs: Some(inputs),
                device_id: "dev-1".to_string(),
                agent: Some("claude".to_string()),
                plan_mode: Some(true),
                ..Default::default()
            },
        )
        .unwrap();
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(request.starts_with("POST /api/trpc/steer.startSession HTTP/1.1"));
        assert!(request.ends_with(
            r#"{"actionId":"11111111-1111-4111-8111-111111111111","inputs":{"prompt":"do the thing"},"deviceId":"dev-1","agent":"claude","planMode":true}"#
        ));
    }

    #[test]
    fn start_session_issue_subject_omits_absent_options() {
        let (base, captured) = one_shot_server(200, r#"{"result":{"data":{"ok":true}}}"#);
        start_session(
            &client(&base),
            &StartSessionInput {
                issue_id: Some("22222222-2222-4222-8222-222222222222".to_string()),
                device_id: "dev-2".to_string(),
                ..Default::default()
            },
        )
        .unwrap();
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(request.ends_with(
            r#"{"issueId":"22222222-2222-4222-8222-222222222222","deviceId":"dev-2"}"#
        ));
    }

    #[test]
    fn kill_session_posts_session_id() {
        let (base, captured) = one_shot_server(
            200,
            r#"{"result":{"data":{"session":{"id":"s-1","status":"ended"},"txId":7}}}"#,
        );
        kill_session(&client(&base), "s-1").unwrap();
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(request.starts_with("POST /api/trpc/steer.killSession HTTP/1.1"));
        assert!(request.ends_with(r#"{"sessionId":"s-1"}"#));
    }

    #[test]
    fn mint_viewer_posts_kind_viewer() {
        let (base, captured) = one_shot_server(
            200,
            r#"{"result":{"data":{"ticket":"t","url":"wss://steer.exponential.at/ws?ticket=t"}}}"#,
        );
        let result = mint_viewer_ticket(&client(&base), "sess-1").unwrap();
        assert!(matches!(result, MintTicketResult::Ticket(_)));
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(request.ends_with(r#"{"kind":"viewer","sessionId":"sess-1"}"#));
    }
}
