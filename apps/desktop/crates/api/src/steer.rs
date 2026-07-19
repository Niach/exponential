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
//! - `steer.myDevices` — **query**, no input → `{devices: [...]}` (the
//!   phone-side "Start on my desktop" picker; mirrored here for parity and
//!   diagnostics).
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
    Publisher {
        #[serde(rename = "codingSessionId")]
        coding_session_id: &'a str,
    },
    Viewer {
        #[serde(rename = "codingSessionId")]
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

/// `steer.mintTicket({kind: "publisher", codingSessionId})` — the per-session
/// PTY publisher ticket (§8.4). The server checks `session.userId === caller`
/// (only the owner's desktop may publish).
pub fn mint_publisher_ticket(
    trpc: &TrpcClient,
    coding_session_id: &str,
) -> Result<MintTicketResult, ApiError> {
    mint(trpc, &MintInput::Publisher { coding_session_id })
}

/// `steer.mintTicket({kind: "viewer", codingSessionId})` — watch/steer a
/// session from this client (team members; owners get perm `steer`).
pub fn mint_viewer_ticket(
    trpc: &TrpcClient,
    coding_session_id: &str,
) -> Result<MintTicketResult, ApiError> {
    mint(trpc, &MintInput::Viewer { coding_session_id })
}

/// One online desktop from `steer.myDevices` (relay in-memory presence).
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SteerDevice {
    pub device_id: String,
    pub device_label: String,
    /// Unix millis (relay `Date.now()` at `online`).
    #[serde(default)]
    pub connected_at: Option<i64>,
}

#[derive(Deserialize)]
struct DevicesWire {
    devices: Vec<SteerDevice>,
}

/// `steer.myDevices` — query, no input. Empty when the relay is disabled.
pub fn my_devices(trpc: &TrpcClient) -> Result<Vec<SteerDevice>, ApiError> {
    let wire: DevicesWire = trpc.query("steer.myDevices")?;
    Ok(wire.devices)
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
            r#"{"kind":"publisher","codingSessionId":"3f0f5a2e-1d4b-4c1e-9f6a-000000000001"}"#
        ));
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
        assert!(request.ends_with(r#"{"kind":"viewer","codingSessionId":"sess-1"}"#));
    }

    #[test]
    fn my_devices_decodes_list() {
        let (base, captured) = one_shot_server(
            200,
            r#"{"result":{"data":{"devices":[{"deviceId":"dev-1","deviceLabel":"MacBook","connectedAt":1751500000000}]}}}"#,
        );
        let devices = my_devices(&client(&base)).unwrap();
        assert_eq!(devices.len(), 1);
        assert_eq!(devices[0].device_id, "dev-1");
        assert_eq!(devices[0].device_label, "MacBook");
        assert_eq!(devices[0].connected_at, Some(1751500000000));
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(request.starts_with("GET /api/trpc/steer.myDevices HTTP/1.1"));
    }
}
