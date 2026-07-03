// Clean reimplementation from the VT spec + alacritty_terminal (Apache-2.0). NOT derived from Zed's GPL terminal crates.
//! One terminal tab (masterplan-v3 §6.13): `TerminalTab { id, kind, view,
//! title, status }` — the JetBrains-model unit the [`crate::manager`] owns.
//!
//! Deltas from the §6.13 sketch, both mechanical:
//! - the sketch's `terminal: Entity<Terminal>` is `view: Entity<TerminalView>`
//!   here — our foreground owner of a session *is* the `TerminalView` (it
//!   holds the `Terminal` in an `Rc<RefCell<_>>` and drains its wake channel);
//! - `title` is split into the kind-default (`claude · EXP-123`, run-config
//!   name, shell basename) and the live OSC title (`AlacTermEvent::Title`,
//!   §6.6) so a title *reset* falls back to the default instead of blanking.

use crate::element::TerminalView;
use crate::pty::ChildExit;
use gpui::{App, Entity, SharedString, Subscription};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

/// A DB `run_configs` row id (§7.3) — plain string here; the `terminal` crate
/// has no api/DB types (§6.1 dependency rule).
pub type RunConfigId = String;

/// Stable per-process tab identity (survives index shifts from close/reorder).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TabId(u64);

impl TabId {
    pub(crate) fn next() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        Self(NEXT.fetch_add(1, Ordering::Relaxed))
    }
}

/// §6.13's three tab kinds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TabKind {
    /// A Start-coding session (one per `coding_sessions` row, §07 opens it).
    Claude,
    /// A launched DB run-config (§07's play button; argv-direct, never a
    /// shell).
    Run(RunConfigId),
    /// A plain "+" terminal (`$SHELL -l`), like any IDE.
    Shell,
}

/// Running → play/stop "alive"; Exited(code) → the exit-code strip (§7.5:
/// green `0`, red non-zero; the tab stays open with final scrollback).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TabStatus {
    Running,
    Exited(i32),
}

/// Fired exactly once when the tab's child exits (§6.7): the `terminal` crate
/// only SIGNALS exit — it has no api/tRPC dep. Phase 5's launcher passes a
/// hook here that ends the `coding_sessions` row (`codingSessions.end`,
/// idempotent server-side); Phase 5 also flips the run-bar play→stop off the
/// same edge. **Deferred wiring:** until the `coding` crate lands, nothing
/// installs a hook — plain shell tabs pass `None`.
pub type ExitHook = Box<dyn FnOnce(TabId, &ChildExit, &mut App) + 'static>;

/// One tab in the bottom-dock strip (§6.13).
pub struct TerminalTab {
    pub id: TabId,
    pub kind: TabKind,
    /// The gpui view owning the live session (element focus + painting).
    pub view: Entity<TerminalView>,
    /// Spawn cwd — persisted for the §6.13 cold-shell restore
    /// (`{ kind, cwd, run_config_id }`, never scrollback).
    pub cwd: Option<PathBuf>,
    pub(crate) status: TabStatus,
    /// Kind-default title (shell basename / run-config name / `claude · ID`).
    pub(crate) default_title: SharedString,
    /// Live OSC 0/2 title (§6.6 `Title`/`ResetTitle`); `None` falls back to
    /// the default.
    pub(crate) osc_title: Option<SharedString>,
    pub(crate) on_exit: Option<ExitHook>,
    /// Keeps the manager's `TerminalViewEvent` subscription alive.
    pub(crate) _subscription: Subscription,
}

impl TerminalTab {
    /// Effective strip title: live OSC title, else the kind default.
    pub fn title(&self) -> &SharedString {
        self.osc_title.as_ref().unwrap_or(&self.default_title)
    }

    pub fn status(&self) -> TabStatus {
        self.status
    }

    pub fn is_running(&self) -> bool {
        matches!(self.status, TabStatus::Running)
    }

    /// Captured exit code, `None` while running (feeds the §7.5 exit badge).
    pub fn exit_code(&self) -> Option<i32> {
        match self.status {
            TabStatus::Running => None,
            TabStatus::Exited(code) => Some(code),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tab_ids_are_unique_and_monotonic() {
        let a = TabId::next();
        let b = TabId::next();
        assert_ne!(a, b);
        assert!(b.0 > a.0);
    }

    #[test]
    fn status_accessors() {
        assert!(matches!(TabStatus::Running, TabStatus::Running));
        let exited = TabStatus::Exited(2);
        assert_eq!(match exited {
            TabStatus::Exited(code) => Some(code),
            TabStatus::Running => None,
        }, Some(2));
    }
}
