// Clean reimplementation from the VT spec + alacritty_terminal (Apache-2.0). NOT derived from Zed's GPL terminal crates.
//! `TerminalManager` — the JetBrains-style multi-tab model for the bottom
//! dock (masterplan-v3 §6.13).
//!
//! The manager owns `Vec<TerminalTab>` + the active index and is the
//! run-bar's counterpart: §07 owns the run-config dropdown + play/stop
//! button, the manager owns the tabs those actions create. It is a gpui
//! entity (`Entity<TerminalManager>`, one per window/dock panel) but is
//! **window-free** — focus and dock-expansion are the dock panel's job,
//! driven by [`TerminalManagerEvent`].
//!
//! Behavior per §6.13:
//! - **"+"** → a `Shell` tab: the user's `$SHELL -l` in the given cwd
//!   (worktree root once Phase 5 provides one) or `$HOME`;
//! - **`Claude` / `Run(id)`** tabs are created by §07's launcher/run-bar via
//!   [`TerminalManager::open_tab`] — the same entry point the dock's `+`
//!   uses, so this is already the Phase-5 launch surface;
//! - add / close / switch; closing a tab kills its child (`Child::kill`) and
//!   joins its threads (`Terminal::shutdown`);
//! - tab titles follow OSC title events with a kind default; run tabs are
//!   named per run config by the Phase-5 caller;
//! - on child exit the tab stays OPEN (`TabStatus::Exited(code)` → the
//!   JetBrains "process finished with exit code N" strip in the dock panel),
//!   and the tab's one-shot [`ExitHook`] fires. **Phase-5 deferral:** ending
//!   the `coding_sessions` row on that edge is the launcher's job (§6.7 — the
//!   terminal crate has no api/tRPC); the hook is the seam it consumes.

use crate::element::{TerminalView, TerminalViewEvent};
use crate::pty::{ChildExit, SpawnSpec};
use crate::session::Terminal;
use crate::tab::{ExitHook, TabId, TabKind, TabStatus, TerminalTab};
use gpui::{AppContext as _, Context, Entity, EventEmitter, SharedString};
use std::path::PathBuf;

/// Spawn size before the first real layout; the grid element resizes the PTY
/// + emulator to the dock's true cell geometry on first paint (§6.10).
const DEFAULT_COLS: u16 = 80;
const DEFAULT_ROWS: u16 = 24;

/// Outward events the dock panel reacts to (expand the bottom dock, focus the
/// terminal, re-render the strip).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TerminalManagerEvent {
    /// A tab was created and made active — the §6.13 "panel expands when a
    /// tab is created" trigger.
    TabOpened(TabId),
    TabClosed(TabId),
    /// The tab's child exited (tab stays open with the exit code, §7.5).
    TabExited { id: TabId, code: i32 },
}

/// §6.13's `TerminalManager { tabs, active }` (fields kept private so the
/// active index can't be desynced from the vec by callers).
pub struct TerminalManager {
    tabs: Vec<TerminalTab>,
    active: usize,
}

impl EventEmitter<TerminalManagerEvent> for TerminalManager {}

impl TerminalManager {
    pub fn new() -> Self {
        Self { tabs: Vec::new(), active: 0 }
    }

    pub fn tabs(&self) -> &[TerminalTab] {
        &self.tabs
    }

    pub fn is_empty(&self) -> bool {
        self.tabs.is_empty()
    }

    pub fn len(&self) -> usize {
        self.tabs.len()
    }

    pub fn active_index(&self) -> Option<usize> {
        // `then` (lazy) — NOT `then_some`, whose argument evaluates eagerly:
        // `self.tabs.len() - 1` underflows (usize) when tabs is empty, which
        // the overflow check turns into a debug-build panic BEFORE the guard's
        // `None` can win (harmlessly wraps + is discarded in release).
        (!self.tabs.is_empty()).then(|| self.active.min(self.tabs.len() - 1))
    }

    pub fn active_tab(&self) -> Option<&TerminalTab> {
        self.active_index().map(|ix| &self.tabs[ix])
    }

    pub fn tab(&self, id: TabId) -> Option<&TerminalTab> {
        self.tabs.iter().find(|tab| tab.id == id)
    }

    fn tab_mut(&mut self, id: TabId) -> Option<&mut TerminalTab> {
        self.tabs.iter_mut().find(|tab| tab.id == id)
    }

    /// Switch the active tab (§6.13 "switch"). The dock panel focuses the
    /// newly active terminal view after calling this.
    pub fn activate(&mut self, index: usize, cx: &mut Context<Self>) {
        if index < self.tabs.len() && index != self.active {
            self.active = index;
            cx.notify();
        }
    }

    pub fn activate_next(&mut self, cx: &mut Context<Self>) {
        if let Some(current) = self.active_index() {
            self.activate(wrap_next(current, self.tabs.len()), cx);
        }
    }

    pub fn activate_prev(&mut self, cx: &mut Context<Self>) {
        if let Some(current) = self.active_index() {
            self.activate(wrap_prev(current, self.tabs.len()), cx);
        }
    }

    /// The "+" affordance (§6.13): a plain `Shell` tab running the user's
    /// `$SHELL -l` in `cwd` (workspace/worktree root when there is repo
    /// context — Phase 5) or `$HOME`.
    pub fn open_shell(
        &mut self,
        cwd: Option<PathBuf>,
        cx: &mut Context<Self>,
    ) -> anyhow::Result<TabId> {
        let shell = default_shell();
        let cwd = cwd.or_else(home_dir);
        // v4 §4.6: shell tab headers show the cwd directory name (the trunk
        // clone dir / `$HOME`), not the shell basename. OSC titles still
        // override at runtime.
        let title = cwd
            .as_deref()
            .and_then(dir_title)
            .unwrap_or_else(|| shell_title(&shell));
        // `-l` (login shell) is a unix-ism; PowerShell/cmd take no analog.
        #[cfg(unix)]
        let spec = SpawnSpec::new(&shell).arg("-l");
        #[cfg(not(unix))]
        let spec = SpawnSpec::new(&shell);
        let mut spec = spec;
        if let Some(cwd) = cwd {
            spec = spec.cwd(cwd);
        }
        self.open_tab(TabKind::Shell, title, &spec, None, cx)
    }

    /// The one general entry point every tab kind goes through — the dock's
    /// `+` today, §07's Start-coding launcher (`Claude`) and run-bar play
    /// (`Run`, argv-direct per §6.13/§7.3.5) in Phase 5.
    ///
    /// `on_exit` fires exactly once with the captured [`ChildExit`] (§6.7);
    /// the Phase-5 launcher ends the `coding_sessions` row from it.
    pub fn open_tab(
        &mut self,
        kind: TabKind,
        default_title: impl Into<SharedString>,
        spec: &SpawnSpec,
        on_exit: Option<ExitHook>,
        cx: &mut Context<Self>,
    ) -> anyhow::Result<TabId> {
        let session = Terminal::spawn(spec, DEFAULT_COLS, DEFAULT_ROWS)?;
        let view = cx.new(|cx| TerminalView::new(session, cx));
        let id = TabId::next();

        // Title + exit updates flow from the view's events (§6.6 drain →
        // TerminalViewEvent) into the tab strip.
        let subscription = cx.subscribe(&view, move |this, view, event, cx| match event {
            TerminalViewEvent::TitleChanged => {
                let title = view.read(cx).title().cloned();
                if let Some(tab) = this.tab_mut(id) {
                    tab.osc_title = title;
                    cx.notify();
                }
            }
            TerminalViewEvent::Bell => cx.notify(),
            TerminalViewEvent::Exited => this.handle_exit(id, &view, cx),
        });

        self.tabs.push(TerminalTab {
            id,
            kind,
            view,
            cwd: spec.cwd.clone(),
            status: TabStatus::Running,
            default_title: default_title.into(),
            osc_title: None,
            on_exit,
            _subscription: subscription,
        });
        self.active = self.tabs.len() - 1;
        cx.emit(TerminalManagerEvent::TabOpened(id));
        cx.notify();
        Ok(id)
    }

    /// Close a tab (§6.13): kill the child + join its threads, then drop the
    /// view. Works on both running and exited tabs — an exited tab stays
    /// open (keep-tab-open semantics) until closed here.
    pub fn close_tab(&mut self, id: TabId, cx: &mut Context<Self>) {
        let Some(ix) = self.tabs.iter().position(|tab| tab.id == id) else {
            return;
        };
        let tab = self.tabs.remove(ix);
        // Deterministic teardown before the entity is released: kill (no-op
        // if already exited) + join the read/wait threads.
        tab.view.update(cx, |view, _| view.session().borrow_mut().shutdown());
        self.active = active_after_close(self.active, ix, self.tabs.len());
        cx.emit(TerminalManagerEvent::TabClosed(id));
        cx.notify();
    }

    pub fn close_active(&mut self, cx: &mut Context<Self>) {
        if let Some(tab) = self.active_tab() {
            let id = tab.id;
            self.close_tab(id, cx);
        }
    }

    /// §6.7's foreground half: flip Running→Exited(code) exactly once, fire
    /// the one-shot exit hook (Phase 5 ends the `coding_sessions` row there),
    /// and surface the event for the dock's exit-code strip.
    fn handle_exit(&mut self, id: TabId, view: &Entity<TerminalView>, cx: &mut Context<Self>) {
        let exit = view
            .read(cx)
            .exit_status()
            .cloned()
            .unwrap_or(ChildExit { code: -1, success: false, signal: None });
        let (code, hook) = {
            let Some(tab) = self.tab_mut(id) else { return };
            if !tab.is_running() {
                return; // fire once (§6.7 — EOF and wait() both raise it)
            }
            tab.status = TabStatus::Exited(exit.code);
            (exit.code, tab.on_exit.take())
        };
        if let Some(hook) = hook {
            hook(id, &exit, cx);
        }
        cx.emit(TerminalManagerEvent::TabExited { id, code });
        cx.notify();
    }
}

impl Default for TerminalManager {
    fn default() -> Self {
        Self::new()
    }
}

/// The user's shell for `+` tabs (§6.13's `$SHELL`), with a platform default.
fn default_shell() -> String {
    #[cfg(windows)]
    {
        // PowerShell is present on every supported Windows; COMSPEC (cmd.exe)
        // is the fallback of last resort. $SHELL is not a Windows convention.
        if which_on_path("pwsh.exe") {
            return "pwsh.exe".into();
        }
        if which_on_path("powershell.exe") {
            return "powershell.exe".into();
        }
        return std::env::var("COMSPEC")
            .ok()
            .filter(|shell| !shell.trim().is_empty())
            .unwrap_or_else(|| "cmd.exe".into());
    }
    #[cfg(not(windows))]
    std::env::var("SHELL")
        .ok()
        .filter(|shell| !shell.trim().is_empty())
        .unwrap_or_else(|| {
            if cfg!(target_os = "macos") {
                "/bin/zsh".into()
            } else {
                "/bin/bash".into()
            }
        })
}

/// PATH probe for the Windows shell pick — plain existence check per PATH
/// entry, no spawning.
#[cfg(windows)]
fn which_on_path(exe: &str) -> bool {
    let Some(path) = std::env::var_os("PATH") else {
        return false;
    };
    std::env::split_paths(&path).any(|dir| dir.join(exe).is_file())
}

/// Default shell-tab title: the shell's basename (OSC titles override it).
fn shell_title(shell: &str) -> String {
    std::path::Path::new(shell)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("shell")
        .to_owned()
}

/// The cwd directory name for a shell tab header (v4 §4.6). `None` for a
/// root-only path with no final component (falls back to the shell basename).
fn dir_title(cwd: &std::path::Path) -> Option<String> {
    cwd.file_name()
        .and_then(|name| name.to_str())
        .map(str::to_owned)
}

fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .filter(|home| !home.is_empty())
        .map(PathBuf::from)
}

/// Active index after removing `closed` from a strip now `new_len` long:
/// stay on the same tab when possible, else the nearest remaining one.
fn active_after_close(active: usize, closed: usize, new_len: usize) -> usize {
    if new_len == 0 {
        0
    } else if closed < active {
        active - 1
    } else {
        active.min(new_len - 1)
    }
}

fn wrap_next(current: usize, len: usize) -> usize {
    debug_assert!(len > 0);
    (current + 1) % len
}

fn wrap_prev(current: usize, len: usize) -> usize {
    debug_assert!(len > 0);
    (current + len - 1) % len
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn active_index_survives_closing_before_it() {
        // tabs: [a, b, c], active = 2 (c); close ix 0 → [b, c], c is ix 1.
        assert_eq!(active_after_close(2, 0, 2), 1);
    }

    #[test]
    fn active_index_clamps_when_closing_the_active_last_tab() {
        // tabs: [a, b, c], active = 2 (c); close ix 2 → [a, b], active b.
        assert_eq!(active_after_close(2, 2, 2), 1);
    }

    #[test]
    fn active_index_stays_when_closing_after_it() {
        // tabs: [a, b, c], active = 0; close ix 2 → active still a.
        assert_eq!(active_after_close(0, 2, 2), 0);
    }

    #[test]
    fn active_index_zero_when_strip_empties() {
        assert_eq!(active_after_close(0, 0, 0), 0);
    }

    #[test]
    fn active_index_is_none_on_empty_without_underflow() {
        // Regression: an empty manager must return `None`, never underflow
        // `tabs.len() - 1` (the `then_some` eager-eval bug that panicked debug
        // builds when the terminal dock rendered with zero tabs).
        assert_eq!(TerminalManager::new().active_index(), None);
    }

    #[test]
    fn tab_switch_wraps_both_ways() {
        assert_eq!(wrap_next(2, 3), 0);
        assert_eq!(wrap_next(0, 3), 1);
        assert_eq!(wrap_prev(0, 3), 2);
        assert_eq!(wrap_prev(2, 3), 1);
        assert_eq!(wrap_next(0, 1), 0);
        assert_eq!(wrap_prev(0, 1), 0);
    }

    #[test]
    fn shell_title_is_the_basename() {
        assert_eq!(shell_title("/bin/zsh"), "zsh");
        assert_eq!(shell_title("/usr/local/bin/fish"), "fish");
        assert_eq!(shell_title(""), "shell");
    }

    #[test]
    fn dir_title_is_the_cwd_directory_name() {
        // v4 §4.6: the trunk clone dir name / `$HOME` name heads the tab.
        assert_eq!(
            dir_title(std::path::Path::new("/home/u/Exponential/repos/acme/web")),
            Some("web".to_string())
        );
        assert_eq!(
            dir_title(std::path::Path::new("/Users/tester")),
            Some("tester".to_string())
        );
        // A root path has no final component → fall back to the shell basename.
        assert_eq!(dir_title(std::path::Path::new("/")), None);
    }

    #[test]
    fn default_shell_is_never_empty() {
        assert!(!default_shell().is_empty());
    }
}
