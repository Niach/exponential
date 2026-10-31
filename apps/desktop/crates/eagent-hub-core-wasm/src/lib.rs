//! The wasm-bindgen facade over `eagent-hub-core` (#EXP-1509): the pure
//! orchestration reducer for JavaScript hosts. JSON strings in, JSON strings
//! out, the same shapes as `packages/eagent/src/model.ts`.
//!
//! ```js
//! const { hydrate, reduce } = require("./eagent_hub_core_wasm.js");
//! let state = hydrate(JSON.stringify(given));            // a fixture's `given` → HubState JSON
//! const outcome = JSON.parse(reduce(state, JSON.stringify(envelope)));
//! // { state, effects, result } | { state, effects: [], failure }
//! ```
//!
//! A malformed state or envelope throws; a refused event is an outcome with
//! `failure` (an eCore `Failure`).

use wasm_bindgen::prelude::*;

/// One event: `stateJson` a HubState, `eventJson` an eCore envelope → the outcome JSON.
#[wasm_bindgen]
pub fn reduce(state_json: &str, event_json: &str) -> Result<String, JsError> {
    eagent_hub_core::reduce_json(state_json, event_json).map_err(|e| JsError::new(&e))
}

/// A fixture's `given` with rules.json `defaults` filled in → the HubState JSON.
#[wasm_bindgen]
pub fn hydrate(given_json: &str) -> Result<String, JsError> {
    eagent_hub_core::hydrate_json(given_json).map_err(|e| JsError::new(&e))
}
