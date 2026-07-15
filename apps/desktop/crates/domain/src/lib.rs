//! `domain` — canonical enum values + row structs (masterplan-v3 §3.1).
//!
//! `src/contract.generated.rs` (emitted from `@exp/domain-contract`, COMMITTED)
//! carries the canonical enum value lists and per-value consts. This crate
//! layers on top (§5.1/§5.5):
//!
//! * [`enums`] — typed enums with tolerant-unknown deserialization + the
//!   board display orders (locked to the generated contract by test);
//! * [`rows`] — the 15 hand-written shape row structs mirroring
//!   `packages/db-schema`, hydrated from the sync store's snake_case JSON;
//! * [`hydrate`] — the tolerant `string → native` serde helpers (§5.5 — NOT
//!   `BoolFromInt`; Electric booleans surface in many forms);
//! * [`filters`] — VERBATIM port of `apps/web/src/lib/filters.ts` (§4.7:
//!   IssueFilters, tab presets, `matches_filters`);
//! * [`options`] — the status/priority icon+color option tables mirroring
//!   `apps/web/src/lib/domain.ts` (§4.7; presentation as data — glyph names +
//!   color tokens — so this crate stays gpui-free);
//! * [`board`] — `apps/web/src/lib/project-board.ts` grouping/sorting.
//!
//! gpui-free — headless-testable.

pub mod contract {
    include!("contract.generated.rs");
}

pub mod board;
pub mod client_version;
pub mod enums;
pub mod filters;
pub mod hydrate;
pub mod options;
pub mod rows;

pub use enums::{IssuePriority, IssueStatus};
pub use rows::member_fallback_label;
pub use filters::{
    active_filter_count, derive_active_tab, empty_filters, has_active_filters, matches_filters,
    tab_preset_statuses, IssueFilters, TabPreset,
};
