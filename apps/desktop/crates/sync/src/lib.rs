//! `sync` — the from-scratch Electric client (masterplan-v3 §3.1 / §05).
//!
//! Phase 2 lands: `protocol.rs`, `client.rs`, `store.rs` (rusqlite/WAL),
//! `manager.rs` — all gpui-free and fixture-tested against
//! `packages/electric-protocol`; `collections.rs` is the thin gpui glue that
//! boards the store into reactive `Entity`-backed collections.
//!
//! Dependency rule (§3.1): depends only on `domain`; `protocol`/`client`/
//! `store`/`manager` must never depend on gpui — **only `sync::collections`
//! links gpui** (it is the single seam between the headless engine and the
//! view tree).
//!
//! Phase-2 state: `protocol.rs` (wire protocol, fixture-locked against
//! `packages/electric-protocol`), `shapes.rs` (the 15 `ShapeSpec` entries),
//! `store.rs` (rusqlite/WAL generic upsert + the §5.6c atomic-refetch dance),
//! `client.rs` (the blocking reqwest long-poll engine, one thread per shape) and
//! `manager.rs` (per-account pipeline reconcile) are in — all gpui-free and
//! covered by `tests/{protocol,store,engine}.rs`. `collections.rs` is the
//! real §5.8 glue: the global [`Store`] with one reactive
//! `Entity<Collection<T>>` per shape, the single foreground delta drain, and
//! the §5 session state machine (SignedOut → SigningIn → Synced /
//! AuthExpired — a dead token routes to login, never an empty board).

pub mod client;
#[cfg(feature = "gpui")]
pub mod collections;
pub mod health;
pub mod kill_watch;
pub mod manager;
pub mod protocol;
pub mod shapes;
pub mod store;

pub use client::{
    ShapeClient, ShapeClientConfig, ShapeDelta, ShapeError, ShapeTransport, TokenFn,
    HttpTransport, TransportError, TransportResponse, UnauthorizedFn, UpgradeRequiredFn,
};
#[cfg(feature = "gpui")]
pub use collections::{
    cmp_identifiers, derive_active_health, ActiveSyncStatus, Collection, Collections, SessionPhase,
    ShapeRow, ShapeStatus, ShapeSyncPhase, SharedState, Store,
};
pub use health::{AccountHealth, SyncHealth, ERROR_STALENESS_WINDOW, FAILURE_STREAK_GRACE};
pub use kill_watch::{session_row_fires_kill, session_row_is_ended};
#[cfg(feature = "gpui")]
pub use kill_watch::{KillWatch, OnSessionEnded};
pub use manager::{AccountSyncConfig, SyncManager};
