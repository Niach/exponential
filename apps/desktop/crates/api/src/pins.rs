//! Typed `pins.*` tRPC helper (EXP-778 personal pins). Verified against
//! `apps/web/src/lib/trpc/pins.ts`:
//!
//! - `pins.toggle({teamId, kind, targetId})` → `{txId, pinned}` — pins the
//!   target when absent, unpins it when present. The ONLY pins endpoint; the
//!   row itself re-streams over the per-user `pins` shape, so callers
//!   fire-and-forget and let the collection settle the toggle's look.

use serde::{Deserialize, Serialize};

use crate::error::ApiError;
use crate::trpc::TrpcClient;

/// `pins.toggle` output.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PinToggleOutput {
    #[serde(default)]
    pub tx_id: Option<i64>,
    /// The state AFTER the toggle.
    #[serde(default)]
    pub pinned: bool,
}

/// `pins.toggle` — mutation. `kind` is a contract `pinKind` value
/// (`domain::contract::PIN_KIND_*`); `target_id` the issue / session / action
/// id that kind names.
pub fn pins_toggle(
    trpc: &TrpcClient,
    team_id: &str,
    kind: &str,
    target_id: &str,
) -> Result<PinToggleOutput, ApiError> {
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Input<'a> {
        team_id: &'a str,
        kind: &'a str,
        target_id: &'a str,
    }
    trpc.mutation(
        "pins.toggle",
        &Input {
            team_id,
            kind,
            target_id,
        },
    )
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
    fn toggle_posts_camel_case_input_and_decodes_pinned() {
        let (base, captured) =
            one_shot_server(200, r#"{"result":{"data":{"txId":41,"pinned":true}}}"#);
        let out = pins_toggle(&client(&base), "t-1", "issue", "i-1").unwrap();
        assert_eq!(out.tx_id, Some(41));
        assert!(out.pinned);
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(request.starts_with("POST /api/trpc/pins.toggle HTTP/1.1"));
        assert!(request.ends_with(r#"{"teamId":"t-1","kind":"issue","targetId":"i-1"}"#));
    }

    #[test]
    fn toggle_decodes_an_unpin() {
        let (base, _captured) =
            one_shot_server(200, r#"{"result":{"data":{"txId":42,"pinned":false}}}"#);
        let out = pins_toggle(&client(&base), "t-1", "session", "s-1").unwrap();
        assert!(!out.pinned);
    }
}
