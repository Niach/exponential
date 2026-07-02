//! The ONLY gpui-aware module in this crate (masterplan-v3 §3.1 / §5.8).
//!
//! Phase-1 placeholder: the global [`Store`] exists so the §3.6 shell can wire
//! dependency injection and prove multi-window shared state (§3.10 gate: "a
//! second window opens sharing the global `Store`") before the Phase-2 sync
//! engine lands. Phase 2 replaces [`SharedState`] with one reactive
//! `Entity<Collection<T>>` per synced shape (§3.5 "per-shape collection
//! Entities") and gives [`Store::open`] its real job: open the rusqlite/WAL
//! store and spawn the 14 shape threads.

use gpui::{App, AppContext as _, Entity, Global};

/// Cross-window shared state, held in a single `Entity` so every window's
/// views can `cx.observe` it and re-render on change.
///
/// Phase 1 carries only enough state to *demonstrate* the sharing: a live
/// count of open workspace windows (each `ui::Workspace` increments on open
/// and decrements on release; every sidebar renders the count, so opening
/// window 2 visibly updates window 1). Phase 2 replaces this with the
/// per-shape collections.
pub struct SharedState {
    /// Number of workspace windows currently open across the app.
    pub windows_open: usize,
}

/// The sync store — a gpui [`Global`] (§3.2: `Store` and `Theme` are globals;
/// views `cx.observe` the specific collections they read).
pub struct Store {
    state: Entity<SharedState>,
}

impl Global for Store {}

impl Store {
    /// Phase-2 signature parity with §3.6 (`sync::Store::open(cx)`): will open
    /// the per-account SQLite store and spawn the Electric shape threads. In
    /// Phase 1 it only creates the shared-state entity.
    pub fn open(cx: &mut App) -> Self {
        let state = cx.new(|_| SharedState { windows_open: 0 });
        Self { state }
    }

    /// Read the global store (panics if the shell has not installed it — the
    /// §3.6 bootstrap sets it before any window opens).
    pub fn global(cx: &App) -> &Self {
        cx.global::<Store>()
    }

    /// The shared cross-window state entity. Observe it for re-renders.
    pub fn state(&self) -> Entity<SharedState> {
        self.state.clone()
    }
}
