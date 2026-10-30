//! EXP-1459: what this install could make of a sealed value (a message body,
//! a card, a session title; `packages/domain-contract/fixtures/sealed-content.json`).
//! `sync::sealed` opens rows at ingest and stamps this state; the renderers
//! branch on it (the locked row).

use serde::Deserialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SealState {
    /// Opened with a tree key this install holds: the row reads as plaintext.
    Opened,
    /// `noKey`: sealed for an epoch this install holds no key for yet; a
    /// holder wraps it automatically, the row re-opens then.
    Locked,
    /// `unsupported` / `malformed` / `refused`: never shown as text.
    Unreadable,
}

impl SealState {
    /// The contract's open status (`plain` is no state at all).
    pub fn from_status(status: &str) -> Option<Self> {
        match status {
            "plain" => None,
            "opened" => Some(Self::Opened),
            "noKey" => Some(Self::Locked),
            _ => Some(Self::Unreadable),
        }
    }

    /// The content is not available on this install.
    pub fn is_closed(self) -> bool {
        self != Self::Opened
    }
}

/// `true` when a row's sealed content could not be opened here.
pub fn is_closed(state: Option<SealState>) -> bool {
    state.is_some_and(SealState::is_closed)
}
