//! eAgent's orchestration reducer (#EXP-1509): a pure port of
//! `packages/eagent/src/reduce.ts`. `reduce(state, event)` takes a
//! [`HubState`] and an eCore [`Envelope`](ecore::Envelope) and returns the
//! next state, the [`Effect`]s to apply and the step's result, or an eCore
//! [`Failure`](ecore::Failure) with the state unchanged. No I/O, no clock,
//! no async: the envelope's `at` is now. `tests/fixtures.rs` replays every
//! case of `packages/eagent/fixtures/*.json`.

pub mod copy;
mod js;
mod model;
mod reduce;

pub use model::*;
pub use reduce::{
    ask_hidden, awaits_caller, bus_listeners, clean_live_session_ids, hydrate, is_live, reduce, reuse_refusal, tree_agent_id,
    ReuseRoot,
};

/// [`reduce`] over JSON: `state` a [`HubState`], `event` an eCore envelope; the outcome as JSON.
pub fn reduce_json(state: &str, event: &str) -> Result<String, String> {
    let state: HubState = serde_json::from_str(state).map_err(|e| format!("state: {e}"))?;
    let event: HubEvent = serde_json::from_str(event).map_err(|e| format!("event: {e}"))?;
    serde_json::to_string(&reduce(&state, &event)).map_err(|e| e.to_string())
}

/// [`hydrate`] over JSON: a fixture's `given` → a [`HubState`].
pub fn hydrate_json(given: &str) -> Result<String, String> {
    let given: serde_json::Value = serde_json::from_str(given).map_err(|e| format!("given: {e}"))?;
    serde_json::to_string(&hydrate(&given)?).map_err(|e| e.to_string())
}
