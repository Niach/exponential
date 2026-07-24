// Clean reimplementation from the VT spec + alacritty_terminal (Apache-2.0). NOT derived from Zed's GPL terminal crates.
//! One terminal tab (masterplan-v3 §6.13): `TerminalTab { id, kind, view,
//! title, status }` — the JetBrains-model unit the [`crate::manager`] owns.
//!
//! Deltas from the §6.13 sketch, both mechanical:
//! - the sketch's `terminal: Entity<Terminal>` is `view: Entity<TerminalView>`
//!   here — our foreground owner of a session *is* the `TerminalView` (it
//!   holds the `Terminal` in an `Rc<RefCell<_>>` and drains its wake channel);
//! - `title` is split into the kind-default (`claude · EXP-123`, action
//!   name, shell basename) and the live OSC title (`AlacTermEvent::Title`,
//!   §6.6) so a title *reset* falls back to the default instead of blanking.

use crate::element::TerminalView;
use crate::pty::ChildExit;
use gpui::{App, Entity, SharedString, Subscription};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

/// Stable per-process tab identity (survives index shifts from close/reorder).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TabId(u64);

impl TabId {
    pub(crate) fn next() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        Self(NEXT.fetch_add(1, Ordering::Relaxed))
    }
}

/// §6.13's tab kinds (v4 §4.9 adds [`TabKind::ClaudeTask`]; EXP-253 adds
/// [`TabKind::Action`] and removes the run-config `Run` kind).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TabKind {
    /// A Start-coding session (one per `coding_sessions` row, §07 opens it).
    Claude,
    /// A one-shot Claude task (masterplan v4 §4.9): interactive `claude` NOT
    /// bound to a `coding_sessions` row (no steer room, no plan charge, no
    /// worktree). Visually behaves like a [`TabKind::Shell`] tab. The
    /// `coding::claude_task` primitive builds its spawn spec.
    ClaudeTask,
    /// A running team action (EXP-253): an interactive claude session bound
    /// to a `coding_sessions` row (steerable like [`TabKind::Claude`]) but
    /// with no worktree/branch/PR — it runs on the trunk clone or a scratch
    /// dir. Carries the `actions` row id (plain string; the `terminal` crate
    /// has no api/DB types — §6.1 dependency rule).
    Action(String),
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
    /// (`{ kind, cwd }`, never scrollback).
    pub cwd: Option<PathBuf>,
    pub(crate) status: TabStatus,
    /// Kind-default title (shell basename / action name / `claude · ID`).
    pub(crate) default_title: SharedString,
    /// Live OSC 0/2 title (§6.6 `Title`/`ResetTitle`); `None` falls back to
    /// the default.
    pub(crate) osc_title: Option<SharedString>,
    /// Identity re-attached to live OSC titles (EXP-145): claude replaces the
    /// whole title with its task description, dropping the `EXP-42` the
    /// kind-default carried — a prefixed tab shows `EXP-42 · <osc title>`.
    pub(crate) title_prefix: Option<SharedString>,
    pub(crate) on_exit: Option<ExitHook>,
    /// Keeps the manager's `TerminalViewEvent` subscription alive.
    pub(crate) _subscription: Subscription,
}

impl TerminalTab {
    /// Effective strip title: live OSC title, else the kind default.
    pub fn title(&self) -> &SharedString {
        self.osc_title.as_ref().unwrap_or(&self.default_title)
    }

    /// Display form of a live OSC title (EXP-145): prepend [`Self::title_prefix`]
    /// (`EXP-42 · <osc title>`) — unless the emitted title already contains
    /// the prefix, so it never doubles up. The manager applies this when it
    /// stores the OSC title; a title *reset* still falls back to the
    /// kind-default, which carries the identity on its own.
    pub(crate) fn decorate_osc_title(&self, title: SharedString) -> SharedString {
        decorate_osc_title(self.title_prefix.as_deref(), title)
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

/// [`TerminalTab::decorate_osc_title`]'s pure core (testable without a view).
fn decorate_osc_title(prefix: Option<&str>, title: SharedString) -> SharedString {
    match prefix {
        Some(prefix) if !title.contains(prefix) => format!("{prefix} · {title}").into(),
        _ => title,
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
    fn osc_title_gets_the_issue_prefix() {
        // EXP-145: claude's OSC title is its task description — the tab must
        // keep showing which issue it belongs to.
        assert_eq!(
            decorate_osc_title(Some("EXP-109"), "Fix trashed notifications".into()),
            SharedString::from("EXP-109 · Fix trashed notifications")
        );
    }

    #[test]
    fn osc_title_prefix_never_doubles() {
        // An OSC title already naming the issue stays as-is.
        assert_eq!(
            decorate_osc_title(Some("EXP-109"), "EXP-109: fix notifications".into()),
            SharedString::from("EXP-109: fix notifications")
        );
    }

    #[test]
    fn osc_title_without_prefix_passes_through() {
        // Shell tabs carry no prefix — OSC titles land verbatim.
        assert_eq!(
            decorate_osc_title(None, "vim README.md".into()),
            SharedString::from("vim README.md")
        );
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
