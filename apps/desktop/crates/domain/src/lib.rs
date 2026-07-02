//! `domain` — canonical enum values + row structs (masterplan-v3 §3.1).
//!
//! `src/contract.generated.rs` (emitted from `@exp/domain-contract`, COMMITTED)
//! carries the canonical enum value lists and per-value consts. This crate
//! layers the row structs, the enums with icon/color metadata, the
//! status/priority option tables, and the tolerant `string → native` serde on
//! top (Phase 1+, [03-desktop-architecture]). gpui-free — headless-testable.

pub mod contract {
    include!("contract.generated.rs");
}
