//! Typed `steer.*` tRPC mirrors (masterplan-v3 §8.2/§8.3) — the desktop's
//! ticket-CONSUMER surface. Shapes verified against
//! `apps/web/src/lib/trpc/steer.ts` (+ the pure core `apps/web/src/lib/steer.ts`):
//!
//! - `steer.config` — **query**, no input → `{enabled, relayUrl}`. Clients
//!   poll this before dialing anything; `enabled: false` is a normal state,
//!   never an error (an unconfigured instance generates no noise).
//! - `steer.mintTicket` — **mutation**, `{kind: "control"}` (VAPP-43: the
//!   desktop mints nothing else; sessions are served over eNet) →
//!   `{disabled: true}` or
//!   `{ticket, url}` where `url` is the FULL ws(s) dial URL with
//!   `?ticket=…` already embedded (`steerTicketUrl`). Consumers use `url`
//!   **as-is** — never reconstruct it (the relay reads the ticket from the
//!   query string only).
//!
//! The desktop is NEVER a signer: it holds no `STEER_RELAY_SECRET` and never
//! touches `signSteerTicket`. All authorization is decided server-side at
//! mint time (§8.0).
//!
//! EXP-1292: the start path (`steer.startSession` / `killSession` /
//! `reportStartFailure`) is gone — the database is the bus; the relay keeps
//! only the live layer these tickets open.

use serde::{Deserialize, Serialize};

use crate::error::ApiError;
use crate::trpc::TrpcClient;

/// `steer.config` output — whether live steering is available on this
/// instance (EXP-1292: starts no longer ride the relay — a start is a
/// `queued` sessions row the device picks up off the shape) (enabled iff BOTH `STEER_RELAY_URL` and
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
enum MintInput {
    // The mint carries the kind and nothing else: the device's label reaches
    // the server through `devices.register` (the persisted row the pickers
    // read), and the ticket never encoded it.
    Control,
}

fn mint(trpc: &TrpcClient, input: &MintInput) -> Result<MintTicketResult, ApiError> {
    let wire: MintWire = trpc.mutation("steer.mintTicket", input)?;
    Ok(match wire {
        MintWire::Ticket { ticket, url } => MintTicketResult::Ticket(MintedTicket { ticket, url }),
        MintWire::Disabled { .. } => MintTicketResult::Disabled,
    })
}

/// `steer.mintTicket({kind: "control"})` — the device-presence socket ticket
/// (§8.3). Any authed user may register presence for their own account.
pub fn mint_control_ticket(trpc: &TrpcClient) -> Result<MintTicketResult, ApiError> {
    mint(trpc, &MintInput::Control)
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
        let result = mint_control_ticket(&client(&base)).unwrap();
        assert_eq!(
            result,
            MintTicketResult::Ticket(MintedTicket {
                ticket: "abc.def".to_string(),
                url: "ws://relay.lan:4002/ws?ticket=abc.def".to_string(),
            })
        );
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(request.starts_with("POST /api/trpc/steer.mintTicket HTTP/1.1"));
        // The kind and nothing else — no `deviceLabel` rides the mint.
        assert!(request.ends_with(r#"{"kind":"control"}"#));
    }
}
