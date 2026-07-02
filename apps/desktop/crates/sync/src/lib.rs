//! `sync` — the from-scratch Electric client (masterplan-v3 §3.1 / §05).
//!
//! Phase 2 lands: `protocol.rs`, `client.rs`, `store.rs` (rusqlite/WAL),
//! `manager.rs` — all gpui-free and fixture-tested against
//! `packages/electric-protocol`; `collections.rs` is the thin gpui glue that
//! projects the store into reactive `Entity`-backed collections.
//!
//! Dependency rule (§3.1): depends only on `domain`; `protocol`/`client`/
//! `store`/`manager` must never depend on gpui — **only `sync::collections`
//! links gpui** (it is the single seam between the headless engine and the
//! view tree).
//!
//! Phase-1 state: only [`collections::Store`] exists — the gpui global the
//! §3.6 shell installs so multi-window shared state is proven before the
//! engine lands underneath it.

pub mod collections;

pub use collections::{SharedState, Store};
