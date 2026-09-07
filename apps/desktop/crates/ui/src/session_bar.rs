//! The bottom **session bar** (EXP-769) — the desktop twin of the web
//! `AgentDock` (`apps/web/src/components/agent-dock/agent-dock.tsx`): ONE
//! fixed strip under the working panel carrying the tabs of the user's coding
//! sessions and PTY terminals, then the Chat and `+` buttons. Nothing else.
//!
//! What it replaced: the JetBrains-style sliding bottom terminal dock — its
//! open/close slide, its header row (undock / collapsed-form switch / hide),
//! the floating "bubble" (EXP-742), the launch-card empty state (EXP-369 —
//! one card per installed agent), the agent items in the `+` menu and the
//! "Terminal" label the empty strip wore. Session tabs used to sit in the TOP
//! strip beside the issue tabs, where the two were hard to tell apart; they
//! are back at the bottom now, and a terminal no longer slides up over the
//! content: it renders FULLSCREEN in the center as [`Screen::Terminal`],
//! exactly like a session renders as `Screen::Session`.
//!
//! Who owns what:
//! - [`SessionBar`] (one per shell window, registered by window like
//!   `screens::ScreensPanel`) owns the window's [`TerminalManager`] — the
//!   launcher, the actions panel, the file tree and the agent-login flow all
//!   open their tabs through it (`coding_flow::window_terminal_manager`) —
//!   plus the terminal-scoped key bindings (cmd-t / cmd-w / ctrl-tab), the
//!   Latest-changes poll for the active terminal, and the two buttons.
//! - The TABS are the `ScreensPanel`'s: a session or terminal screen is an
//!   ordinary detail tab of its one tab list, and the bar is the second VIEW
//!   of that list (`Screen::is_dock_tab`), rendered by
//!   `ScreensPanel::render_session_bar_tabs`. Web parity adds the caller's
//!   live runs that have no tab yet; clicking one opens it.
//! - A terminal's CONTENT (grid, exit strip, Latest changes) renders in the
//!   center through [`SessionBar::render_terminal_screen`], so the key
//!   context wraps the grid wherever it paints.
//!
//! Behavior that carried over unchanged: a tab OPENING navigates to its
//! screen and focuses the grid (the §6.13 expand-on-create edge, now a
//! navigation); a dead tab stays open with its scrollback and the JetBrains
//! "Process finished with exit code N" strip + a badge on its chip (§7.5);
//! `+` / cmd-t opens a plain shell on the active board's trunk clone
//! (v4 §4.6); NOTHING terminal-side is persisted (EXP-301) and nothing ever
//! spawns on its own — a terminal only appears from an explicit action.
//! Chat (EXP-739) is a promptless, repo-less chat run on the device's default
//! agent; agent-anchored launches stay on the Start-coding dialog.
//!
//! **Phase-5 deferral (§6.7):** "child exit ends the `coding_sessions` row"
//! is the launcher's wiring — it passes an `ExitHook` into `open_tab`; the
//! bar/manager only surface the exit edge.

use gpui::{
    actions, div, prelude::FluentBuilder as _, px, AnyElement, App, AppContext as _, Bounds,
    ClickEvent, Entity, Focusable as _, InteractiveElement, IntoElement, KeyBinding,
    ParentElement, Pixels, Render, SharedString, Styled, Subscription, Task, WeakEntity, Window,
    WindowId,
};
use gpui_component::{
    button::{Button, ButtonVariant, ButtonVariants as _},
    h_flex, notification::Notification, v_flex, ActiveTheme as _, Icon, Sizable as _,
    WindowExt as _,
};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::time::Duration;
use terminal::{TabId, TabKind, TerminalManager, TerminalManagerEvent};

use crate::changes_bar;
use crate::coding_flow::{CodingHub, LocalSessionHost, LocalSessions, TokenRefreshers};
use crate::controls::WebControl as _;
use crate::icons::registry;
use crate::native_dialog::{self, AlertSpec};
use crate::navigation::{self, Screen};
use crate::queries;
use crate::repo_resolver::{repo_resolver_for_window, RepoLookup, RepoResolver};

/// The bar's height — the web strip's `h-9`.
pub(crate) const SESSION_BAR_H: f32 = 36.;

/// EXP-688: how often the Latest-changes bar re-reads the branch diff. The
/// same 3s cadence the steer emitter's `DiffSnapshots` publishes on. While
/// there is nothing to read (no terminal on screen, a plain shell) the loop
/// idles on the shorter beat instead — it runs no git at all there, and the
/// bar then appears within a blink of the terminal coming up rather than 3s
/// later.
const CHANGES_POLL: Duration = Duration::from_secs(3);
const CHANGES_IDLE_POLL: Duration = Duration::from_millis(500);

/// Keymap scope for the terminal-screen bindings — an ancestor of the focused
/// terminal view in the dispatch path, so the chords work while typing in
/// the terminal (bindings match before raw key-down listeners).
const KEY_CONTEXT: &str = "TerminalScreen";

actions!(
    exp,
    [
        /// New plain shell tab (§6.13 "+").
        NewTerminalTab,
        /// Close the active terminal tab (kills its child, §6.13).
        CloseTerminalTab,
        /// Switch to the next terminal tab.
        NextTerminalTab,
        /// Switch to the previous terminal tab.
        PrevTerminalTab,
    ]
);

/// Per-window handle to the [`SessionBar`] and its manager (mirrors
/// `screens::ScreensRegistry`): the coding flow resolves a window's terminal
/// manager through it, the screens panel renders a terminal screen through
/// it, and `screen_title` finds a tab's title across windows.
///
/// The MANAGER is registered beside the bar on purpose: a chip's content is
/// resolved while the bar itself is mid-render (leased), so anything the
/// render path reads must go to the manager entity, never back through the
/// bar.
#[derive(Default)]
struct SessionBarRegistry {
    by_window: HashMap<WindowId, (Entity<SessionBar>, Entity<TerminalManager>)>,
}

impl gpui::Global for SessionBarRegistry {}

/// This window's session bar, if the shell has built one (a signed-out or
/// undocked window has none).
pub(crate) fn host_for_window(window: &Window, cx: &App) -> Option<Entity<SessionBar>> {
    host_for_window_id(window.window_handle().window_id(), cx)
}

/// [`host_for_window`] by id.
pub(crate) fn host_for_window_id(window_id: WindowId, cx: &App) -> Option<Entity<SessionBar>> {
    cx.try_global::<SessionBarRegistry>()
        .and_then(|registry| registry.by_window.get(&window_id))
        .map(|(bar, _)| bar.clone())
}

/// The manager that owns `tab` — a tab id is process-unique, so at most one
/// window answers. Safe from inside the bar's own render (see the registry).
pub(crate) fn manager_for_tab(tab: TabId, cx: &App) -> Option<Entity<TerminalManager>> {
    let registry = cx.try_global::<SessionBarRegistry>()?;
    registry
        .by_window
        .values()
        .map(|(_, manager)| manager)
        .find(|manager| manager.read(cx).tab(tab).is_some())
        .cloned()
}

/// A terminal tab's live title (OSC-updated), for `navigation::screen_title`.
pub(crate) fn terminal_tab_title(tab: TabId, cx: &App) -> Option<SharedString> {
    let manager = manager_for_tab(tab, cx)?;
    let manager = manager.read(cx);
    manager.tab(tab).map(|tab| tab.title().clone())
}

/// Drop a closed window's entry (called from the `Shell` release hook).
pub(crate) fn remove_window(window_id: WindowId, cx: &mut App) {
    if let Some(registry) = cx.try_global::<SessionBarRegistry>() {
        if registry.by_window.contains_key(&window_id) {
            cx.global_mut::<SessionBarRegistry>()
                .by_window
                .remove(&window_id);
        }
    }
}

/// Bind the terminal-scoped keys. Called once from [`crate::init`].
pub(crate) fn init(cx: &mut App) {
    // Resolve the §6.12 login PATH off-thread now, so the first spawn's
    // `build_command` finds the OnceLock already filled instead of running
    // `$SHELL -lic` on the gpui foreground.
    terminal::prewarm_login_path();

    #[cfg(target_os = "macos")]
    cx.bind_keys([
        KeyBinding::new("cmd-t", NewTerminalTab, Some(KEY_CONTEXT)),
        KeyBinding::new("cmd-w", CloseTerminalTab, Some(KEY_CONTEXT)),
    ]);
    #[cfg(not(target_os = "macos"))]
    cx.bind_keys([
        KeyBinding::new("ctrl-shift-t", NewTerminalTab, Some(KEY_CONTEXT)),
        KeyBinding::new("ctrl-shift-w", CloseTerminalTab, Some(KEY_CONTEXT)),
    ]);
    cx.bind_keys([
        KeyBinding::new("ctrl-tab", NextTerminalTab, Some(KEY_CONTEXT)),
        KeyBinding::new("ctrl-shift-tab", PrevTerminalTab, Some(KEY_CONTEXT)),
    ]);
}

/// The per-window session bar: owner of that window's [`TerminalManager`]
/// (multi-window = independent tab sets over the same global store, §7.6),
/// the bar's own chrome and the terminal screens' content.
pub struct SessionBar {
    manager: Entity<TerminalManager>,
    /// EXP-325: the promptless agent-shell tabs' P9 token-refresher holds
    /// (tab id → the retained trunk clone), released on `TabClosed`. (A
    /// window closed with live holds leaks its refresh loops until quit —
    /// bounded and rare; sessions normally end by tab close.)
    agent_shell_holds: HashMap<TabId, PathBuf>,
    /// EXP-497: the painted width of the bar's chip slot (the `flex_1`
    /// container the chips + the two buttons render into), recorded by an
    /// `on_children_prepainted` listener on the bar. Unlike the center strip,
    /// this width is NOT derivable from window chrome — the bar sits under a
    /// user-resizable split. `None` until the first paint (renders every
    /// chip; `overflow_x_hidden` covers that one frame).
    chips_slot_width: Option<f32>,
    /// EXP-372/EXP-739: a launch in flight (label of what is starting) — the
    /// Chat button reads it so two quick clicks cannot buy two agents.
    pending_launch: Option<SharedString>,
    /// EXP-688: the Latest-changes snapshot for the active terminal, when it
    /// is a local coding session.
    changes: Option<ChangesState>,
    /// The expanded Latest-changes view (the shared side-by-side diff).
    changes_diff: Entity<crate::diff::DiffView>,
    /// This window — the screens panel is poked by id (see
    /// [`Self::poke_screens`]).
    window_id: WindowId,
    /// Repaints the bar when the screens panel's tabs open/close/retitle
    /// (resolved lazily on the first render, like `AppTitleBar`). ONE-WAY on
    /// purpose: the panel never observes the bar — two entities observing
    /// each other would ping-pong notifies forever — so the bar pokes it
    /// explicitly when its OWN state changes what the terminal screen shows.
    _observe_screens: Option<Subscription>,
    _changes_poll: Task<()>,
    _subscription: Subscription,
}

impl SessionBar {
    /// This window's tab-strip model — §07's Start-coding launcher / actions
    /// panel open their `Claude`/`Action` tabs through it (the §6.13 "same
    /// entry point" rule; resolved per window via `coding_flow`).
    pub(crate) fn manager(&self) -> &Entity<TerminalManager> {
        &self.manager
    }

    /// The only constructor — every bar starts with zero tabs (EXP-301:
    /// launching the app must never spawn a terminal).
    pub fn new(window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        let manager = cx.new(|_| TerminalManager::new());

        let subscription = cx.subscribe_in(
            &manager,
            window,
            |this, _, event: &TerminalManagerEvent, window, cx| {
                match event {
                    // §6.13: a tab opening shows it — the dock used to expand;
                    // the terminal screen is navigated to now. Also the path
                    // Phase 5's play button / remote start rides.
                    TerminalManagerEvent::TabOpened(id) => {
                        // EXP-703: the chat-run launch has no success
                        // callback of its own — the tab landing IS the
                        // success, so the EXP-372 progress state ends here
                        // (failures end it via the runner's failure hook).
                        this.set_pending_launch(None, cx);
                        navigation::navigate(window, cx, Screen::Terminal { tab: *id });
                        this.focus_active_terminal(window, cx);
                        this.poke_screens(cx);
                    }
                    TerminalManagerEvent::TabClosed(id) => {
                        // EXP-325: a promptless agent-shell tab releases its
                        // token-refresher hold with the tab.
                        if let Some(clone) = this.agent_shell_holds.remove(id) {
                            TokenRefreshers::release(&clone, cx);
                        }
                        // The screen tab goes with the terminal (idempotent:
                        // a close that STARTED at the tab already removed it).
                        if let Some(screens) = crate::screens::screens_for_window(window, cx) {
                            let screen = Screen::Terminal { tab: *id };
                            screens.update(cx, |screens, cx| {
                                screens.close_screen_tab(&screen, window, cx);
                            });
                        }
                        this.focus_active_terminal(window, cx);
                        this.poke_screens(cx);
                    }
                    // Exit strip/badge render on notify; ending the
                    // coding_sessions row is Phase 5's ExitHook (§6.7). The
                    // strip is the terminal SCREEN's — the panel repaints.
                    TerminalManagerEvent::TabExited { .. } => this.poke_screens(cx),
                }
                cx.notify();
            },
        );

        // Dev hook: EXP_DEV_OPEN_SHELL=1 opens one plain shell tab at startup
        // so the §11.4 terminal smoke (tab chip + rendered prompt) is
        // demonstrable headlessly/in CI without synthesizing a `+` click.
        // Dev-only — never document for users. Runs AFTER the subscription
        // so TabOpened navigates to the terminal screen.
        //
        // `EXP_DEV_SHELL_CWD` overrides the shell's cwd (EXP-651): the default
        // is `$HOME`, and a shell tab is TITLED after its cwd's directory name
        // — which put the capturing developer's account name in the committed
        // `terminal` shot. The capture lane points it at the same username-free
        // repos root the Tools and Worktrees panes already render verbatim.
        if std::env::var("EXP_DEV_OPEN_SHELL").is_ok_and(|value| value == "1") {
            let shell_override = crate::coding_flow::terminal_shell_override(cx);
            let cwd = std::env::var_os("EXP_DEV_SHELL_CWD")
                .filter(|dir| !dir.is_empty())
                .map(std::path::PathBuf::from);
            manager.update(cx, |manager, cx| {
                if let Err(error) = manager.open_shell(cwd, shell_override, cx) {
                    log::warn!("session bar: EXP_DEV_OPEN_SHELL spawn failed: {error:#}");
                }
            });
        }

        // EXP-65: undocked tabs render elsewhere — repaint when the undock
        // registry changes (tab popped out / reattached).
        if let Some(undock_state) = crate::undock::state(cx) {
            cx.observe(&undock_state, |_, _, cx| cx.notify()).detach();
        }

        // The chips follow the synced rows (title/status/pr_state — boards
        // for the status resolution's team lookup; devices for the caption
        // and the paused edge), the local session registry, and the shared
        // merge state. (`try_global`: tests build bars without a store.)
        let collections =
            sync::Store::try_global(cx).map(|store| store.collections().clone());
        if let Some(collections) = collections {
            cx.observe(&collections.issues, |_, _, cx| cx.notify()).detach();
            cx.observe(&collections.issue_statuses, |_, _, cx| cx.notify())
                .detach();
            cx.observe(&collections.boards, |_, _, cx| cx.notify()).detach();
            cx.observe(&collections.coding_sessions, |_, _, cx| cx.notify())
                .detach();
            cx.observe(&collections.devices, |_, _, cx| cx.notify()).detach();
        }
        let local_sessions = LocalSessions::global(cx);
        cx.observe(&local_sessions, |_, _, cx| cx.notify()).detach();
        let merge_state = crate::pr_merge::MergeState::global(cx);
        cx.observe(&merge_state, |_, _, cx| cx.notify()).detach();
        // The Chat button's agent comes from the doctor report — repaint when
        // it lands. Guarded like the collections above.
        if sync::Store::try_global(cx).is_some() {
            let hub = CodingHub::global(cx);
            cx.observe(&hub, |_, _, cx| cx.notify()).detach();
        }

        // EXP-688: the Latest-changes poll. One timer for the bar's life — it
        // resolves the on-screen terminal itself and does no git work at all
        // while no terminal is showing.
        let changes_poll = cx.spawn_in(window, async move |this, window| loop {
            let Ok(job) = this.update_in(window, |this, window, cx| this.changes_job(window, cx))
            else {
                return; // bar gone with its window
            };
            let beat = match job {
                ChangesJob::Idle => CHANGES_IDLE_POLL,
                ChangesJob::Poll {
                    tab,
                    worktree,
                    base_ref,
                } => {
                    let files = window
                        .background_executor()
                        .spawn(async move {
                            coding::scm::branch_diff(&worktree, base_ref.as_deref()).ok()
                        })
                        .await;
                    if this
                        .update_in(window, |this, _, cx| this.apply_changes(tab, files, cx))
                        .is_err()
                    {
                        return;
                    }
                    CHANGES_POLL
                }
            };
            window.background_executor().timer(beat).await;
        });

        // Publish this window's bar (insert overwrites — a rebuilt shell wins).
        let window_id = window.window_handle().window_id();
        let entity = cx.entity();
        cx.default_global::<SessionBarRegistry>()
            .by_window
            .insert(window_id, (entity, manager.clone()));

        Self {
            manager,
            agent_shell_holds: HashMap::new(),
            chips_slot_width: None,
            pending_launch: None,
            changes: None,
            changes_diff: cx.new(|cx| crate::diff::DiffView::new(window, cx)),
            window_id,
            _observe_screens: None,
            _changes_poll: changes_poll,
            _subscription: subscription,
        }
    }

    /// Repaint this window's screens panel — the terminal SCREEN is rendered
    /// by it off this bar's state (exit strip, Latest changes), and the panel
    /// does not observe the bar (see `_observe_screens`).
    fn poke_screens(&self, cx: &mut App) {
        if let Some(screens) = crate::screens::screens_for_window_id(self.window_id, cx) {
            screens.update(cx, |_, cx| cx.notify());
        }
    }

    // -- tab activation ----------------------------------------------------

    /// Focus follows the active tab (§6.13 "each tab hosting the terminal
    /// element focused"). Undocked tabs render in their own window — never
    /// steal this window's focus for them (EXP-65). Only when the terminal
    /// is what the center shows: focusing a grid behind an issue tab would
    /// eat the keyboard invisibly.
    fn focus_active_terminal(&self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        let Some((id, view)) = self
            .manager
            .read(cx)
            .active_tab()
            .map(|tab| (tab.id, tab.view.clone()))
        else {
            return;
        };
        if crate::undock::is_terminal_tab_undocked(id, cx) {
            return;
        }
        let nav = navigation::nav_for_window(window, cx);
        let showing = matches!(
            navigation::resolved_screen(&nav, cx),
            Some(Screen::Terminal { tab: shown }) if shown == id
        );
        if !showing {
            return;
        }
        let handle = view.focus_handle(cx);
        window.focus(&handle, cx);
    }

    /// Manager indices of the tabs this window still shows (EXP-65: undocked
    /// tabs render in their own windows and are hidden here).
    fn visible_indices(&self, cx: &App) -> Vec<usize> {
        self.manager
            .read(cx)
            .tabs()
            .iter()
            .enumerate()
            .filter(|(_, tab)| !crate::undock::is_terminal_tab_undocked(tab.id, cx))
            .map(|(ix, _)| ix)
            .collect()
    }

    /// Ctrl-tab / ctrl-shift-tab step over VISIBLE tabs only (an undocked
    /// tab must not flash through while cycling).
    fn activate_visible_step(
        &mut self,
        forward: bool,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let visible = self.visible_indices(cx);
        let len = visible.len();
        if len == 0 {
            return;
        }
        let current_pos = self
            .manager
            .read(cx)
            .active_index()
            .and_then(|active| visible.iter().position(|ix| *ix == active));
        let next_pos = match current_pos {
            Some(pos) if forward => (pos + 1) % len,
            Some(pos) => (pos + len - 1) % len,
            None => 0,
        };
        let Some(id) = self
            .manager
            .read(cx)
            .tabs()
            .get(visible[next_pos])
            .map(|tab| tab.id)
        else {
            return;
        };
        self.show_tab(id, window, cx);
    }

    /// Show `tab`'s screen — the one path every activation takes (a chip
    /// click lands here through the screens panel; ctrl-tab and a re-dock
    /// directly). A tab activation, not a navigation: no back-stack push.
    fn show_tab(&mut self, tab: TabId, window: &mut Window, cx: &mut gpui::Context<Self>) {
        navigation::set_screen(window, cx, Some(Screen::Terminal { tab }));
        self.activate_tab_by_id(tab, window, cx);
    }

    /// Make `tab` the manager's active one and focus its grid — what the
    /// screens panel calls when a terminal screen becomes the center
    /// (`sync_tabs`), so cmd-w and the changes poll follow the screen.
    pub(crate) fn activate_tab_by_id(
        &mut self,
        tab: TabId,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let ix = self
            .manager
            .read(cx)
            .tabs()
            .iter()
            .position(|candidate| candidate.id == tab);
        if let Some(ix) = ix {
            self.manager
                .update(cx, |manager, cx| manager.activate(ix, cx));
        }
        self.focus_active_terminal(window, cx);
    }

    /// Close `tab` — kill its child and drop it (the screens panel's close
    /// for a terminal tab; `TabClosed` then echoes back as a no-op).
    pub(crate) fn close_terminal(&mut self, tab: TabId, cx: &mut gpui::Context<Self>) {
        self.manager
            .update(cx, |manager, cx| manager.close_tab(tab, cx));
    }

    /// Pop the tab out into its own native window (EXP-65). The tab stays in
    /// the manager (exit hooks / stop button / steer wiring untouched) — the
    /// registry hides it here and the new window renders its view. If it was
    /// the active tab, activate the nearest still-visible neighbor first so
    /// the manager never points at a hidden tab.
    pub(crate) fn undock_tab(
        &mut self,
        id: TabId,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let neighbor = {
            let manager = self.manager.read(cx);
            if manager.active_tab().map(|tab| tab.id) == Some(id) {
                let current = manager.active_index().unwrap_or(0);
                let visible: Vec<usize> = manager
                    .tabs()
                    .iter()
                    .enumerate()
                    .filter(|(_, tab)| {
                        tab.id != id && !crate::undock::is_terminal_tab_undocked(tab.id, cx)
                    })
                    .map(|(ix, _)| ix)
                    .collect();
                visible
                    .iter()
                    .copied()
                    .find(|ix| *ix > current)
                    .or_else(|| visible.last().copied())
            } else {
                None
            }
        };
        if let Some(ix) = neighbor {
            self.manager.update(cx, |manager, cx| manager.activate(ix, cx));
        }
        crate::undock::open_undocked_terminal_tab(
            self.manager.clone(),
            id,
            window.window_handle(),
            cx,
        );
        cx.notify();
    }

    // -- launches ------------------------------------------------------------

    /// The `+` shell tab (v4 §4.6): cwd = the **trunk** clone root of this
    /// window's active board; `$HOME` only off a board screen or while the
    /// clone doesn't exist yet. The repo→trunk-root resolution needs a
    /// (tRPC-only, never synced) `repositories.list` lookup, so the resolve
    /// runs off the foreground and the tab opens once the cwd is known; a
    /// non-board screen (or missing session/board) opens at `$HOME`
    /// immediately (`open_shell(None)`).
    fn new_shell_tab(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        self.set_pending_launch(Some("New terminal".into()), cx);
        let Some((resolver, board_id, settings)) = self.shell_scope(window, cx) else {
            self.open_shell_cwd(None, cx);
            return;
        };
        // The repo comes from the shared window resolver (the run/git bars keep
        // it warm); a still-loading / unlinked repo just opens at `$HOME`.
        resolver.update(cx, |resolver, cx| resolver.ensure_loaded(cx));
        let full_name = match resolver.read(cx).lookup_board(&board_id) {
            RepoLookup::Found(repo) => repo.full_name,
            _ => {
                self.open_shell_cwd(None, cx);
                return;
            }
        };
        cx.spawn(async move |this, cx| {
            let cwd = cx
                .background_executor()
                .spawn(async move {
                    let root = coding::clone_path(&settings.repos_root_path(), &full_name);
                    // `$HOME` (None) until the clone actually exists on disk.
                    coding::shell_cwd(Some(root))
                })
                .await;
            let _ = this.update(cx, |this, cx| this.open_shell_cwd(cwd, cx));
        })
        .detach();
    }

    /// Spawn a shell tab at `cwd` (`None` → `$HOME`, resolved by the manager).
    fn open_shell_cwd(&mut self, cwd: Option<PathBuf>, cx: &mut gpui::Context<Self>) {
        // The launch is over either way — a spawn failure must not leave the
        // Chat button dimmed.
        self.set_pending_launch(None, cx);
        let shell_override = crate::coding_flow::terminal_shell_override(cx);
        let result = self
            .manager
            .update(cx, |manager, cx| manager.open_shell(cwd, shell_override, cx));
        if let Err(error) = result {
            log::error!("session bar: shell spawn failed: {error:#}");
        }
    }

    /// EXP-372: flip the in-flight-launch state and repaint.
    fn set_pending_launch(&mut self, label: Option<SharedString>, cx: &mut gpui::Context<Self>) {
        if self.pending_launch != label {
            self.pending_launch = label;
            cx.notify();
        }
    }

    /// The sync-resolvable inputs for the `+` shell cwd: the shared window repo
    /// resolver, the window's active board (screen scope with the
    /// last-board fallback), and the coding settings (repos root). `None`
    /// with no resolvable board — the caller then opens the shell at
    /// `$HOME`.
    fn shell_scope(
        &self,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> Option<(Entity<RepoResolver>, String, coding::Settings)> {
        let nav = navigation::nav_for_window(window, cx);
        let board_id = navigation::active_board_id(&nav, cx)?;
        let resolver = repo_resolver_for_window(window, cx);
        let settings = CodingHub::global(cx).read(cx).settings.clone();
        Some((resolver, board_id, settings))
    }

    fn on_new_tab(&mut self, _: &NewTerminalTab, window: &mut Window, cx: &mut gpui::Context<Self>) {
        self.new_shell_tab(window, cx);
    }

    fn on_close_tab(
        &mut self,
        _: &CloseTerminalTab,
        _window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        self.manager.update(cx, |manager, cx| manager.close_active(cx));
    }

    fn on_next_tab(
        &mut self,
        _: &NextTerminalTab,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        self.activate_visible_step(true, window, cx);
    }

    fn on_prev_tab(
        &mut self,
        _: &PrevTerminalTab,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        self.activate_visible_step(false, window, cx);
    }

    /// EXP-739: the agent a one-click chat launches — the device's configured
    /// `default_agent` when the doctor found it INSTALLED, else the first
    /// installed one (a default pointing at an agent this machine does not
    /// have must not disable the button). `None` while no report exists yet,
    /// or when nothing is runnable at all.
    fn default_chat_agent(cx: &gpui::App) -> Option<coding::CodingAgent> {
        let hub = CodingHub::global_ref(cx)?;
        let hub = hub.read(cx);
        let installed = hub.doctor.report.as_ref()?.installed_agents();
        let preferred = hub.settings.default_agent;
        if installed.contains(&preferred) {
            return Some(preferred);
        }
        installed.first().copied()
    }

    /// EXP-739: the bar's one-click "Chat" — a promptless, repo-LESS chat
    /// run on the device's default agent ([`Self::launch_chat_run`] with no
    /// repo), for the "just talk to the agent about the tracker" shape. The
    /// web strip's trailing Chat glyph, in the same slot.
    fn chat_button(&self, cx: &gpui::Context<Self>) -> AnyElement {
        let bar = cx.entity().downgrade();
        let agent = Self::default_chat_agent(cx);
        // A launch is already in flight (this button's or cmd-t's): two quick
        // clicks must not buy two session rows, two scratch dirs and two
        // agents.
        let pending = self.pending_launch.clone();
        let tooltip: SharedString = match (&pending, agent) {
            (Some(label), _) => format!("Starting {label}…").into(),
            (None, Some(agent)) => format!("Chat with {}", agent.label()).into(),
            (None, None) => crate::coding_flow::NO_AGENT_COPY.into(),
        };
        // NEVER `.disabled(true)` on a strip surface (gpui-disabled-button
        // dead zone): dim the glyph and decide in the handler.
        let inert = pending.is_some() || agent.is_none();
        Button::new("session-bar-chat")
            .ghost()
            .web_icon_xs()
            .icon(Icon::new(registry::ACTION_CHAT))
            .tooltip(tooltip)
            .when(inert, |this| this.opacity(0.4))
            .on_click(move |_, window, cx| {
                cx.stop_propagation();
                let Some(agent) = agent else {
                    return;
                };
                if pending.is_some() {
                    return;
                }
                let Some(bar) = bar.upgrade() else {
                    return;
                };
                bar.update(cx, |bar, cx| {
                    bar.launch_chat_run(agent, None, window, cx);
                });
            })
            .into_any_element()
    }

    /// The `+`: a plain shell, straight away (cmd-t's click twin). The
    /// agent launches the old dropdown carried belong to the Start-coding
    /// dialog and the Chat button now.
    fn new_terminal_button(&self, cx: &gpui::Context<Self>) -> AnyElement {
        Button::new("session-bar-new-terminal")
            .ghost()
            .web_icon_xs()
            .icon(Icon::new(registry::UI_ADD))
            .tooltip("New terminal")
            .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                cx.stop_propagation();
                this.new_shell_tab(window, cx);
            }))
            .into_any_element()
    }

    /// EXP-703: the Chat button's agent launch — a promptless CHAT run over
    /// the builtin action rails ([`crate::action_run`] with the hidden
    /// `builtin:chat` action and, when one is given, its `repo` input
    /// filled). The run gets a `coding_sessions` row, a steer channel and the
    /// MCP session header — visible and steerable from web and mobile, and a
    /// child it starts via `exponential_sessions_start` gets parent linkage
    /// (EXP-700).
    ///
    /// EXP-739: `repo` is `Some((repository_id, full_name))` for a run
    /// anchored to a repository — its OWN worktree on `exp/chat-<id8>`, never
    /// the trunk clone — and `None` for a repo-LESS chat, which the launcher
    /// runs worktree-less in a scratch dir: a conversation with the tracker
    /// over MCP, no code checked out. The agent spawns with NO initial prompt
    /// and waits for input either way (the attended promptless shape
    /// `coding::prepare_action` allows since EXP-703).
    pub(crate) fn launch_chat_run(
        &mut self,
        agent: coding::CodingAgent,
        repo: Option<(String, String)>,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(deps) = crate::coding_flow::build_action_deps(cx) else {
            log::warn!("session bar: chat launch ignored — not signed in");
            return;
        };
        let nav = navigation::nav_for_window(window, cx);
        let Some(team_id) = navigation::active_team_id(&nav, cx) else {
            log::warn!("session bar: chat launch ignored — no active team");
            return;
        };
        let options = coding::LaunchOptions::defaults_for(&deps.settings, agent);
        let action = api::actions::builtin_chat_action(&team_id);
        // At most the `repo` input rides — deliberately NO `prompt`: the
        // person is sitting at the terminal and types the first message
        // themselves. EXP-739: a repo-less chat emits no `repo` key at all.
        let inputs: Vec<coding::ActionInputValue> = match &repo {
            Some((repository_id, full_name)) => action
                .inputs
                .iter()
                .filter(|input| input.key == "repo")
                .map(|input| coding::ActionInputValue {
                    key: input.key.clone(),
                    label: input.label.clone(),
                    input_type: input.input_type.clone(),
                    value: repository_id.clone(),
                    display: Some(full_name.clone()),
                })
                .collect(),
            None => Vec::new(),
        };
        // EXP-739: the SETTLED hook clears the pending state whichever way the
        // start ends — the TabOpened edge alone would strand it forever on an
        // ACP chat, which opens a center-panel session and no terminal tab.
        self.set_pending_launch(Some(agent.label().into()), cx);
        let bar = cx.entity().downgrade();
        let on_settled: crate::action_run::ActionSettledHook = Box::new(move |cx, _started| {
            if let Some(bar) = bar.upgrade() {
                bar.update(cx, |bar, cx| bar.set_pending_launch(None, cx));
            }
        });
        crate::action_run::start_action_run(
            crate::action_run::StartActionArgs {
                action_id: action.id,
                team_id,
                repo: crate::action_run::ActionRepo::Resolve,
                options,
                origin: coding::LaunchOrigin::Local,
                inputs,
                target: Some(window.window_handle()),
                activate_app: false,
                reservation: None,
                trigger: None,
                automation_id: None,
                on_settled: Some(on_settled),
            },
            cx,
        );
    }

    /// The EXP-325 promptless agent launch: background
    /// [`coding::prepare_agent_shell`] (doctor → token → clone/autopull →
    /// MCP wiring → promptless argv with the agent's settings defaults) →
    /// foreground [`TabKind::AgentShell`] tab in THIS window. No
    /// `coding_sessions` row / heartbeat / exit hook — the session has no
    /// issue/batch/action subject; the P9 token-refresher hold keeps `git
    /// push` working past the token TTL, released on tab close.
    ///
    /// This path serves the EXP-369 worktree-PINNED terminals (the settings
    /// pane's per-worktree button and the file tree's "Open agent here"),
    /// which must run in an EXISTING worktree — the chat rails always cut a
    /// fresh one. `cwd_override` pins the run to one of the clone's
    /// worktrees; `None` runs on the trunk clone root.
    pub(crate) fn launch_agent_shell(
        &mut self,
        agent: coding::CodingAgent,
        repository_id: String,
        full_name: String,
        cwd_override: Option<PathBuf>,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(deps) = crate::coding_flow::build_action_deps(cx) else {
            log::warn!("session bar: agent shell ignored — not signed in");
            return;
        };
        let options = coding::LaunchOptions::defaults_for(&deps.settings, agent);
        let request = coding::AgentShellRequest {
            options,
            repository_id,
            full_name,
            cwd_override,
        };
        self.set_pending_launch(Some(agent.label().into()), cx);
        cx.spawn_in(window, async move |this, cx| {
            let prepared = cx
                .background_executor()
                .spawn(async move { coding::prepare_agent_shell(&request, &deps) })
                .await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.set_pending_launch(None, cx);
                let launch = match prepared {
                    Ok(coding::PreparedAgentShell::Ready(launch)) => launch,
                    Ok(coding::PreparedAgentShell::Disabled(reason)) => {
                        window.push_notification(
                            Notification::error(SharedString::from(reason.message())),
                            cx,
                        );
                        return;
                    }
                    Err(err) => {
                        log::warn!("session bar: agent shell prepare failed: {err}");
                        window.push_notification(
                            Notification::error(SharedString::from(err.to_string())),
                            cx,
                        );
                        return;
                    }
                };
                let opened = this.manager.update(cx, |manager, cx| {
                    manager.open_tab(
                        TabKind::AgentShell,
                        launch.tab_title.clone(),
                        Some(launch.tab_title_prefix.clone().into()),
                        &launch.spawn,
                        None,
                        cx,
                    )
                });
                match opened {
                    Ok(tab_id) => {
                        TokenRefreshers::retain(&launch.clone, &launch.repository_id, cx);
                        this.agent_shell_holds.insert(tab_id, launch.clone.clone());
                    }
                    Err(error) => {
                        log::warn!("session bar: agent shell spawn failed: {error:#}");
                        window.push_notification(
                            Notification::error(SharedString::from(error.to_string())),
                            cx,
                        );
                    }
                }
            });
        })
        .detach();
    }

    /// EXP-484 (C1): open one agent-LOGIN tab — `claude auth login
    /// --claudeai`, `codex login --device-auth`, or pi's bare TUI with
    /// `/login` typed at its prompt.
    ///
    /// A plain [`TerminalManager::open_tab`], deliberately NOT the
    /// [`Self::launch_agent_shell`] path: `coding::prepare_agent_shell`
    /// refuses a signed-out agent, which is exactly who needs to sign in.
    /// No `coding_sessions` row, no token hold, no MCP wiring — just the
    /// CLI's own login command in a visible tab. The caller (
    /// [`crate::agent_login`]) owns the logout-first switch, the pi typing
    /// and the exit hook.
    pub(crate) fn launch_agent_login(
        &mut self,
        agent: coding::CodingAgent,
        plan: &coding::LoginPlan,
        on_exit: Option<terminal::tab::ExitHook>,
        cx: &mut gpui::Context<Self>,
    ) -> anyhow::Result<TabId> {
        self.manager.update(cx, |manager, cx| {
            manager.open_tab(
                TabKind::AgentLogin(agent.id().to_string()),
                SharedString::from(plan.title.clone()),
                Some(SharedString::from(agent.label().to_string())),
                &plan.spawn,
                on_exit,
                cx,
            )
        })
    }

    // -- Latest changes --------------------------------------------------------

    /// What the next Latest-changes poll should do. Clears the snapshot (and
    /// skips git entirely) whenever there is nothing to show: no terminal on
    /// screen, an undocked one, or a tab that is not a local coding session
    /// (a plain shell has no branch to diff).
    fn changes_job(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) -> ChangesJob {
        let idle = |this: &mut Self, cx: &mut gpui::Context<Self>| {
            if this.changes.take().is_some() {
                this.poke_screens(cx);
                cx.notify();
            }
            ChangesJob::Idle
        };
        let nav = navigation::nav_for_window(window, cx);
        let Some(Screen::Terminal { tab }) = navigation::resolved_screen(&nav, cx) else {
            return idle(self, cx);
        };
        if crate::undock::is_terminal_tab_undocked(tab, cx)
            || self.manager.read(cx).tab(tab).is_none()
        {
            return idle(self, cx);
        }
        let Some(sessions) = LocalSessions::global_ref(cx) else {
            return idle(self, cx);
        };
        let scope = {
            let sessions = sessions.read(cx);
            sessions
                .session_for_tab(tab)
                .map(|session| (session.worktree.clone(), session.base_ref.clone()))
        };
        let Some((worktree, base_ref)) = scope else {
            return idle(self, cx);
        };
        ChangesJob::Poll {
            tab,
            worktree,
            base_ref,
        }
    }

    /// Install a poll's answer. A FAILED poll (`None`) keeps the previous
    /// snapshot (`merge_changes_snapshot`): a diff momentarily unreadable —
    /// mid-rebase, mid-checkout — must not blank the bar and strand the Merge
    /// pill alone. A real empty answer (the branch was reset) clears it.
    fn apply_changes(
        &mut self,
        tab: TabId,
        files: Option<Vec<coding::scm::DiffFile>>,
        cx: &mut gpui::Context<Self>,
    ) {
        let previous = self
            .changes
            .take()
            .filter(|changes| changes.tab == tab);
        let expanded = previous.as_ref().is_some_and(|changes| changes.expanded);
        let generation = previous.as_ref().map_or(0, |changes| changes.generation);
        let previous_files = previous.map(|changes| changes.files).unwrap_or_default();
        let changed = files.as_ref().is_some_and(|files| *files != previous_files);
        let files = changes_bar::merge_changes_snapshot(previous_files, files);
        let (additions, deletions) = changes_bar::changes_totals(&files);
        self.changes = Some(ChangesState {
            tab,
            files,
            additions,
            deletions,
            expanded,
            generation: generation + u64::from(changed),
        });
        if changed && expanded {
            self.rebuild_changes_diff(cx);
        }
        self.poke_screens(cx);
        cx.notify();
    }

    /// Rebuild the expanded side-by-side view from the current snapshot.
    fn rebuild_changes_diff(&mut self, cx: &mut gpui::Context<Self>) {
        let Some(changes) = self.changes.as_ref() else {
            return;
        };
        let prepared = crate::diff::build_scm_diff(&changes.files, &cx.theme().highlight_theme);
        self.changes_diff
            .update(cx, |diff, cx| diff.set_prepared(prepared, cx));
    }

    /// EXP-678/EXP-688: "Latest changes" — the branch's diff (everything the
    /// PR carries, committed work included) plus the Merge button, in one
    /// row under the terminal grid. It renders when there IS a diff or an
    /// open PR to merge, so the Merge pill never stands alone. The CHROME
    /// lives in [`crate::changes_bar`] (EXP-746), where the session screen's
    /// rail wears it too.
    fn render_changes_bar(&self, tab: TabId, cx: &mut gpui::Context<Self>) -> Option<AnyElement> {
        let merge = merge_tab_meta(tab, cx);
        let changes = self
            .changes
            .as_ref()
            .filter(|changes| changes.tab == tab && !changes.files.is_empty());
        if !changes_bar::changes_bar_visible(changes.is_some(), merge.is_some()) {
            return None;
        }
        let expanded = changes.is_some_and(|changes| changes.expanded);
        let totals = changes.map(|changes| (changes.additions, changes.deletions));
        Some(changes_bar::render(
            changes_bar::ChangesSpec {
                toggle_id: "terminal-changes-toggle",
                placement: changes_bar::ChangesPlacement::Bar,
                totals,
                expanded,
                merge,
                diff_view: self.changes_diff.clone(),
                on_toggle: Box::new(|this: &mut Self, cx| this.toggle_changes_expanded(cx)),
                on_merged: Some(self.tab_close_on_merge(tab)),
            },
            cx,
        ))
    }

    /// Flip the Latest-changes bar open/shut, building the diff rows the
    /// first time it opens (they are only worth rendering when visible).
    fn toggle_changes_expanded(&mut self, cx: &mut gpui::Context<Self>) {
        let Some(changes) = self.changes.as_mut() else {
            return;
        };
        changes.expanded = !changes.expanded;
        if changes.expanded {
            self.rebuild_changes_diff(cx);
        }
        self.poke_screens(cx);
        cx.notify();
    }

    /// The local tab close a merge triggers: the tab closes the moment the
    /// merge call fires (the `TabClosed` watcher fires the idempotent
    /// `codingSessions.end`), so a merge that fails on conflicts never leaves
    /// a live session holding the branch — the Reviews page's "Fix conflicts"
    /// recovery starts immediately instead of parking behind a busy worktree.
    /// The server ends the user's live sessions on OTHER devices after the
    /// merge.
    fn tab_close_on_merge(&self, tab: TabId) -> changes_bar::OnMerged {
        let manager = self.manager.downgrade();
        std::rc::Rc::new(move |cx: &mut App| {
            if let Some(manager) = manager.upgrade() {
                manager.update(cx, |manager, cx| manager.close_tab(tab, cx));
            }
        })
    }

    // -- rendering -----------------------------------------------------------

    /// EXP-769: the terminal SCREEN — `tab`'s grid filling the center, its
    /// exit strip under it and the Latest-changes bar under that. The root
    /// carries the terminal key context and the cmd-t/cmd-w/ctrl-tab
    /// handlers, so the chords keep working while typing in the grid
    /// wherever it paints. Rendered by `ScreensPanel` through this entity so
    /// the listeners bind here.
    pub(crate) fn render_terminal_screen(
        &mut self,
        tab: TabId,
        _window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let (view, exit_code) = match self.manager.read(cx).tab(tab) {
            Some(entry) => (Some(entry.view.clone()), entry.exit_code()),
            None => (None, None),
        };
        let root = div()
            .id("terminal-screen")
            .key_context(KEY_CONTEXT)
            .on_action(cx.listener(Self::on_new_tab))
            .on_action(cx.listener(Self::on_close_tab))
            .on_action(cx.listener(Self::on_next_tab))
            .on_action(cx.listener(Self::on_prev_tab))
            .size_full()
            .flex()
            .flex_col()
            .overflow_hidden();
        let Some(view) = view else {
            // The manager already dropped it — the tab is closing this frame.
            return root
                .child(hint(registry::NAV_TERMINAL, "This terminal was closed.", cx))
                .into_any_element();
        };
        if crate::undock::is_terminal_tab_undocked(tab, cx) {
            // EXP-65: exactly one window paints a view at a time.
            return root
                .child(
                    v_flex()
                        .flex_1()
                        .min_h_0()
                        .items_center()
                        .justify_center()
                        .gap_2()
                        .child(hint(
                            registry::NAV_TERMINAL,
                            "This terminal is open in a separate window.",
                            cx,
                        ))
                        .child(
                            Button::new("terminal-screen-show-window")
                                .ghost()
                                .cursor_pointer()
                                .web_xs()
                                .label("Show window")
                                .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                                    crate::undock::reveal_terminal_tab(
                                        tab,
                                        this.manager.clone(),
                                        window.window_handle(),
                                        cx,
                                    );
                                })),
                        ),
                )
                .into_any_element();
        }
        root
            // min_h(0) so the flex child can shrink; the grid element itself
            // guards the 0-height case (§6.9).
            .child(div().flex_1().min_h_0().child(view))
            .when_some(exit_code, |this, code| this.child(exit_strip(code, cx)))
            .children(self.render_changes_bar(tab, cx))
            .into_any_element()
    }
}

/// A centered muted one-liner with a glyph over it (the terminal screen's
/// "gone" / "elsewhere" states).
fn hint(glyph: crate::icons::ExpIcon, copy: &'static str, cx: &App) -> AnyElement {
    v_flex()
        .items_center()
        .gap_1()
        .text_xs()
        .text_color(cx.theme().muted_foreground)
        .child(Icon::new(glyph).small())
        .child(copy)
        .into_any_element()
}

impl Render for SessionBar {
    /// The bar: the screens panel's session/terminal tabs with the Chat and
    /// `+` buttons riding right after the last one, on a `border_t` hairline
    /// inside the cutout panel — the web strip's `h-9 border-t px-2 gap-1`.
    /// Always rendered, even with nothing open: Chat and `+` stay one click
    /// away.
    fn render(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        if self._observe_screens.is_none() {
            if let Some(screens) = crate::screens::screens_for_window(window, cx) {
                self._observe_screens = Some(cx.observe(&screens, |_, _, cx| cx.notify()));
            }
        }
        // EXP-497: the two buttons ride INSIDE the slot after the chips — two
        // 24px circles plus their gaps come off the partition budget.
        let button_reserve =
            2. * (theme::tokens::size::CONTROL_SM + crate::screens::chip_gap(window));
        let available = self
            .chips_slot_width
            .map_or(f32::MAX, |slot| (slot - button_reserve).max(0.));
        let trailing = vec![self.chat_button(cx), self.new_terminal_button(cx)];
        let tabs = crate::screens::screens_for_window(window, cx).map(|screens| {
            screens.update(cx, |screens, cx| {
                screens.render_session_bar_tabs(available, trailing, window, cx)
            })
        });
        h_flex()
            // EXP-497: record the chip slot's painted width (the `flex_1`
            // child below, a pure-stretch flex item the chips cannot
            // inflate) so the partition budgets against the real layout.
            // Change-gated: only a real width change repaints. (On the bare
            // `Div` — the method is not exposed on `Stateful`, so it rides
            // ahead of `.id()`.)
            .on_children_prepainted({
                let bar = cx.entity().downgrade();
                move |bounds: Vec<Bounds<Pixels>>, _window, cx| {
                    let Some(slot) = bounds.first() else {
                        return;
                    };
                    let width = f32::from(slot.size.width);
                    let _ = bar.update(cx, |this, cx| {
                        if this
                            .chips_slot_width
                            .is_none_or(|prev| (prev - width).abs() > 0.5)
                        {
                            this.chips_slot_width = Some(width);
                            cx.notify();
                        }
                    });
                }
            })
            .id("session-bar")
            .h(px(SESSION_BAR_H))
            .w_full()
            .flex_shrink_0()
            .px_2()
            .gap_1()
            .items_center()
            .border_t_1()
            .border_color(theme::tokens::glass::STROKE_ROW.to_hsla())
            .child(
                // Chips never scroll — non-fitting tabs fold into the "+N"
                // dropdown; `overflow_x_hidden` covers the one unmeasured
                // first frame, which renders every chip.
                div()
                    .id("session-bar-slot")
                    .min_w_0()
                    .flex_1()
                    .overflow_x_hidden()
                    .children(tabs),
            )
    }
}

/// The JetBrains "process finished" strip under a dead tab's final
/// scrollback (§7.5 exit-code strip; the tab stays open). Free function so
/// the EXP-65 undocked terminal window renders the identical strip.
pub(crate) fn exit_strip(code: i32, cx: &App) -> impl IntoElement {
    let color = if code == 0 {
        cx.theme().success
    } else {
        cx.theme().danger
    };
    h_flex()
        .gap_2()
        .px_3()
        .py_1()
        .items_center()
        .border_t_1()
        .border_color(cx.theme().border)
        .text_xs()
        .text_color(cx.theme().muted_foreground)
        .child(div().size(px(6.)).rounded_full().bg(color))
        .child(SharedString::from(format!(
            "Process finished with exit code {code}"
        )))
}

/// EXP-688: the Latest-changes snapshot for ONE terminal tab.
struct ChangesState {
    tab: TabId,
    files: Vec<coding::scm::DiffFile>,
    additions: u32,
    deletions: u32,
    /// Whether the bar is showing its side-by-side diff.
    expanded: bool,
    /// Bumped on every CHANGED snapshot — the expanded view rebuilds off it.
    generation: u64,
}

/// What one Latest-changes tick has to do.
enum ChangesJob {
    /// Nothing to show (no terminal on screen / not a session) — no git.
    Idle,
    Poll {
        tab: TabId,
        worktree: PathBuf,
        base_ref: Option<String>,
    },
}

/// The issue-chip snapshot of one issue-session terminal tab (EXP-325): the
/// chip renders the center issue-tab treatment (status glyph + mono
/// identifier + synced title) instead of the plain terminal title.
pub(crate) struct IssueTabMeta {
    pub(crate) status: domain::statuses::ResolvedStatus,
    pub(crate) identifier: SharedString,
    /// `None` for a blank issue title — the identifier already labels the
    /// chip (the EXP-310 center-tab rule).
    pub(crate) title: Option<SharedString>,
}

/// Resolve a tab back to its issue chip, when it is an issue session over a
/// synced issue row (mirrors `screens::chip_content`). Batch/action/shell
/// tabs (and unsynced issues) answer `None`.
pub(crate) fn issue_tab_meta(tab_id: TabId, cx: &App) -> Option<IssueTabMeta> {
    let sessions = LocalSessions::global_ref(cx)?;
    let sessions = sessions.read(cx);
    let crate::coding_flow::SessionSubject::Issue(issue_id) = sessions.subject_for_tab(tab_id)?
    else {
        return None;
    };
    let store = sync::Store::try_global(cx)?;
    let issue = store.collections().issues.read(cx).get(issue_id)?;
    let title = issue.title.trim();
    Some(IssueTabMeta {
        status: queries::resolve_issue_status(cx, issue),
        identifier: SharedString::from(issue.identifier.clone()),
        title: (!title.is_empty()).then(|| SharedString::from(title.to_string())),
    })
}

/// Resolve a tab's merge affordance: the [`changes_bar::merge_target_for_run`]
/// rules over the LOCAL session behind the tab, with its synced
/// `coding_sessions` row looked up by the row id the launcher recorded
/// (EXP-734 — an action or chat run's own chore PR lives there and nowhere
/// else).
fn merge_tab_meta(tab_id: TabId, cx: &App) -> Option<changes_bar::MergeTarget> {
    let sessions = LocalSessions::global_ref(cx)?;
    let sessions = sessions.read(cx);
    let session = sessions.session_for_tab(tab_id)?;
    let store = sync::Store::try_global(cx)?;
    let collections = store.collections();
    let issues = collections.issues.read(cx);
    let rows = collections.coding_sessions.read(cx);
    let issue_id = match &session.subject {
        crate::coding_flow::SessionSubject::Issue(issue_id) => Some(issue_id.as_str()),
        // Batch and action/chat runs carry no issue of their own — the branch
        // and the session row answer for them.
        crate::coding_flow::SessionSubject::Batch(_)
        | crate::coding_flow::SessionSubject::Action(_) => None,
    };
    changes_bar::merge_target_for_run(
        issue_id,
        &session.branch,
        rows.get(&session.session_id),
        issues.iter(),
    )
}

/// EXP-769: the caller's LIVE runs the session bar lists whether or not a
/// tab is open for them (the web `useAgentsData(...).running`): every live
/// row of the caller's on OTHER machines (`queries::remote_session_rows`,
/// when a relay exists to open them through) plus the ACP runs THIS process
/// hosts. Newest start first. A run hosted on this process's PTY path is
/// its terminal tab and never listed here.
pub(crate) fn running_session_ids(cx: &mut App) -> Vec<String> {
    let Some(me) = queries::active_account(cx).map(|account| account.user_id) else {
        return Vec::new();
    };
    let own_device_id = queries::own_device_id(cx);
    let relay = queries::remote_start_enabled(cx);
    let local_sessions = LocalSessions::global_ref(cx);
    let Some(store) = sync::Store::try_global(cx) else {
        return Vec::new();
    };
    let collections = store.collections().clone();
    let now = chrono::Utc::now().timestamp();

    let (local_ids, pty_ids): (HashSet<String>, HashSet<String>) = local_sessions
        .as_ref()
        .map(|sessions| {
            let sessions = sessions.read(cx);
            let ids = sessions.session_ids();
            let pty = ids
                .iter()
                .filter(|id| {
                    sessions
                        .session_by_id(id)
                        .is_some_and(|session| session.host.tab().is_some())
                })
                .cloned()
                .collect();
            (ids.into_iter().collect(), pty)
        })
        .unwrap_or_default();

    let sessions = collections.coding_sessions.read(cx);
    let mut rows: Vec<&domain::rows::CodingSession> = if relay {
        queries::remote_session_rows(sessions.iter(), &me, &own_device_id, &local_ids, now)
    } else {
        Vec::new()
    };
    rows.extend(
        sessions
            .iter()
            .filter(|session| local_ids.contains(&session.id) && !pty_ids.contains(&session.id))
            .filter(|session| queries::coding_session_is_live(session, now)),
    );
    rows.sort_by(|a, b| b.started_at.cmp(&a.started_at).then_with(|| b.id.cmp(&a.id)));
    rows.into_iter().map(|session| session.id.clone()).collect()
}

/// EXP-769: a live run the session bar's × can kill (web `useKillSession`'s
/// `canKill`): the caller's, still live, its host not paused. `None` means
/// the × closes the tab instead.
#[derive(Clone)]
pub(crate) struct KillTarget {
    session_id: String,
    /// `Some` while THIS process hosts the run — the kill goes straight to
    /// the host instead of out through the relay.
    local: Option<LocalSessionHost>,
    /// The machine's name, for the confirm copy ("… on Studio").
    device_label: Option<String>,
}

pub(crate) fn kill_target(session_id: &str, cx: &App) -> Option<KillTarget> {
    let me = queries::active_account(cx)?.user_id;
    let store = sync::Store::try_global(cx)?;
    let collections = store.collections();
    let sessions = collections.coding_sessions.read(cx);
    let row = sessions.get(session_id)?;
    let now = chrono::Utc::now().timestamp();
    if row.user_id.as_deref() != Some(me.as_str()) || !queries::coding_session_is_live(row, now) {
        return None;
    }
    let presentation =
        queries::session_device_presentation(row, collections.devices.read(cx).iter(), now * 1_000);
    let issue_pr_state = row
        .issue_id
        .as_deref()
        .and_then(|issue_id| collections.issues.read(cx).get(issue_id).cloned())
        .and_then(|issue| issue.pr_state);
    let display = queries::coding_session_display(
        row,
        issue_pr_state.as_deref().or(row.pr_state.as_deref()),
    );
    // A paused host is never killed (it resumes when the lid opens).
    if queries::session_is_paused(display, &presentation) {
        return None;
    }
    let local = LocalSessions::global_ref(cx)
        .and_then(|sessions| sessions.read(cx).session_by_id(session_id).map(|s| s.host.clone()));
    Some(KillTarget {
        session_id: session_id.to_string(),
        local,
        device_label: presentation.label,
    })
}

/// The confirm before a live run is ended. A run this process hosts stops
/// through its [`LocalSessionHost`] (the same path the issue header's stop
/// takes); anything else goes out as `steer.killSession`. Shared with the
/// Devices screen's Running rows.
pub(crate) fn prompt_kill(target: KillTarget, window: &mut Window, cx: &mut App) {
    prompt_kill_session(target.local, target.device_label, target.session_id, window, cx);
}

/// [`prompt_kill`] on its parts.
pub(crate) fn prompt_kill_session(
    local: Option<LocalSessionHost>,
    device_label: Option<String>,
    session_id: String,
    window: &mut Window,
    cx: &mut App,
) {
    let spec = match local {
        Some(host) => {
            // EXP-746: the copy names what actually happens — a run without a
            // terminal must not be promised one.
            let detail = if host.tab().is_some() {
                "The agent stops immediately and the terminal tab closes. \
                 Uncommitted work in the worktree is kept."
            } else {
                "The agent stops immediately. Uncommitted work in the worktree is kept."
            };
            AlertSpec::new("Stop this coding session?", detail, "Stop session").on_ok(
                move |_, cx| {
                    host.stop(cx);
                    true
                },
            )
        }
        None => AlertSpec::new(
            "Kill this coding session?",
            crate::steer_viewer::kill_description(device_label.as_deref()),
            "Kill session",
        )
        .ok_variant(ButtonVariant::Danger)
        .on_ok(move |_, cx| {
            crate::steer_viewer::kill_session(&session_id, cx);
            true
        }),
    };
    native_dialog::open_alert(window, cx, spec);
}

/// EXP-746: show the terminal a PTY-hosted run occupies — the one seam
/// [`crate::session_screen::open_session`] takes for a run that IS a
/// terminal here. A session whose terminal lives in ANOTHER window's manager
/// is left alone (that window owns it, and this one has nothing to show).
pub(crate) fn reveal_pty_tab(
    tab: TabId,
    manager: &WeakEntity<TerminalManager>,
    window: &mut Window,
    cx: &mut App,
) {
    let Some(host) = host_for_window(window, cx) else {
        return;
    };
    if host.read(cx).manager.entity_id() != manager.entity_id() {
        return;
    }
    if host.read(cx).manager.read(cx).tab(tab).is_none() {
        return;
    }
    navigation::navigate(window, cx, Screen::Terminal { tab });
    host.update(cx, |host, cx| host.activate_tab_by_id(tab, window, cx));
}

#[cfg(test)]
mod tests {
    use super::*;

    /// EXP-769: the bar is the web strip's height, and the button reserve it
    /// takes off the chip budget is two `size-6` circles — the pill ladder's
    /// small rung, not a hand-typed 24.
    #[test]
    fn bar_geometry_is_the_web_strips() {
        assert_eq!(SESSION_BAR_H, 36.);
        assert_eq!(theme::tokens::size::CONTROL_SM, 24.);
    }
}
