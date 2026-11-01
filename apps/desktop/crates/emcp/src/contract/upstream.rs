//! One member's connection to one upstream
//! (`packages/emcp/fixtures/upstream.json`): the states, the transitions,
//! what is callable and the consent page's missing upstreams.

use std::collections::{BTreeSet, HashMap};

use serde::Deserialize;
use serde_json::Value;

use super::fixture;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum UpstreamState {
    Disconnected,
    Connecting,
    Connected,
    Expired,
    Reconnect,
    Failed,
}

impl UpstreamState {
    pub fn as_str(self) -> &'static str {
        match self {
            UpstreamState::Disconnected => "disconnected",
            UpstreamState::Connecting => "connecting",
            UpstreamState::Connected => "connected",
            UpstreamState::Expired => "expired",
            UpstreamState::Reconnect => "reconnect",
            UpstreamState::Failed => "failed",
        }
    }

    fn parse(text: &str) -> Option<Self> {
        serde_json::from_value(Value::String(text.to_string())).ok()
    }

    /// The words every surface shows (`labels`).
    pub fn label(self) -> &'static str {
        match self {
            UpstreamState::Disconnected => "Not connected",
            UpstreamState::Connecting => "Connecting…",
            UpstreamState::Connected | UpstreamState::Expired => "Connected",
            UpstreamState::Reconnect => "Reconnect needed",
            UpstreamState::Failed => "Connection failed",
        }
    }
}

/// The binding errors (`errors`).
pub const ERROR_DISCONNECTED: &str = "upstreamDisconnected";
pub const ERROR_MISSING: &str = "upstreamMissing";
pub const ERROR_FAILED: &str = "upstreamFailed";

/// The next state; an event a state does not list changes nothing (`*`
/// wins). Events are the fixture's words (`connect`, `callbackOk`, …).
pub fn upstream_step(state: UpstreamState, event: &str) -> UpstreamState {
    let transitions = &fixture::upstream()["transitions"];
    let next = transitions["*"][event].as_str().or_else(|| transitions[state.as_str()][event].as_str());
    next.and_then(UpstreamState::parse).unwrap_or(state)
}

/// May the broker call through it (an expired token is refreshed first)?
pub fn upstream_callable(state: UpstreamState) -> bool {
    matches!(state, UpstreamState::Connected | UpstreamState::Expired)
}

/// The servers the chosen apps declare that exist and are not callable for
/// this member (no state = disconnected), sorted, each once.
pub fn missing_upstreams(
    declared: &[Vec<String>],
    states: &HashMap<String, UpstreamState>,
    servers: &[String],
) -> Vec<String> {
    let names: BTreeSet<&String> = declared.iter().flatten().filter(|name| servers.contains(name)).collect();
    names
        .into_iter()
        .filter(|name| !upstream_callable(states.get(*name).copied().unwrap_or(UpstreamState::Disconnected)))
        .cloned()
        .collect()
}
