//! `domain` — canonical enum values + row structs (masterplan-v3 §3.1).
//!
//! `src/contract.generated.rs` (emitted from `@exp/domain-contract`, COMMITTED)
//! carries the canonical enum value lists and per-value consts. This crate
//! layers on top (§5.1/§5.5):
//!
//! * [`enums`] — typed enums with tolerant-unknown deserialization + the
//!   board display orders (locked to the generated contract by test);
//! * [`rows`] — the 14 hand-written shape row structs mirroring
//!   `packages/db-schema`, hydrated from the sync store's snake_case JSON;
//! * [`hydrate`] — the tolerant `string → native` serde helpers (§5.5 — NOT
//!   `BoolFromInt`; Electric booleans surface in many forms).
//!
//! gpui-free — headless-testable. Icon/color option tables (§4.7) land with
//! the Phase-3 screens.

pub mod contract {
    include!("contract.generated.rs");
}

pub mod enums;
pub mod hydrate;
pub mod rows;

pub use enums::{IssuePriority, IssueStatus};
