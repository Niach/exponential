//! The bottom terminal dock (masterplan-v3 §6.13 / §7.5) — the
//! JetBrains-style multi-tab terminal panel.
//!
//! One [`TerminalDockPanel`] per window lives inside the bottom `Dock`'s
//! `TabPanel` (§3.3); *inside* it, a gpui-component `Tab`/`TabBar` strip
//! lists the [`terminal::TerminalManager`]'s sessions — **not** Zed's GPL
//! `Pane`/`Dock` (§6.13's licensing rule). Behavior:
//!
//! - **"+"** → a dropdown (EXP-325): one item per doctor-installed agent
//!   CLI, launching an empty promptless agent session on the current
//!   board's trunk repo (a repo submenu when the team has several), plus
//!   "New shell" — the plain `Shell` tab (`$SHELL -l`, cwd = the active
//!   board's **trunk** clone root, v4 §4.6; `$HOME` only off a board screen
//!   or before the clone exists), which cmd-t / ctrl-shift-t inside the
//!   dock still opens directly. This is also the launch surface: the
//!   Start-coding launcher and the actions panel call the same
//!   `TerminalManager::open_tab`.
//! - close buttons per tab (and cmd-w / ctrl-shift-w), ctrl-tab /
//!   ctrl-shift-tab to switch;
//! - no empty state: expanding an empty dock spawns a shell right away
//!   (`render` defers into `new_shell_tab`);
//! - the dock **expands when a tab is created** (`TabOpened` →
//!   `Dock::set_open`, §4's dock open/close) and the new tab's terminal is
//!   focused; the grid element resizes with the dock (§6.10);
//! - a dead tab **stays open** with its final scrollback and shows the
//!   JetBrains "Process finished with exit code N" strip + a green-0 /
//!   red-non-zero badge on the tab (§7.5's exit-code strip);
//! - persistence (EXP-301): **nothing terminal-side is persisted**. A launch
//!   never opens a terminal in the user's face — no tab is respawned, and the
//!   bottom dock always comes up collapsed (see `Shell::install_fixed_chrome`).
//!   Terminals only ever appear from an explicit user action: the "+" / cmd-t,
//!   expanding the dock, a Start-coding run, or an action run.
//!
//! **Phase-5 deferral (§6.7):** "child exit ends the `coding_sessions` row"
//! is the launcher's wiring — it passes an `ExitHook` into `open_tab`; the
//! dock/manager only surface the exit edge.

use gpui::{
    actions, div, prelude::FluentBuilder as _, px, App, AppContext as _, ClickEvent, Entity,
    FocusHandle, Focusable, InteractiveElement, IntoElement, KeyBinding, ParentElement, Render,
    SharedString, StatefulInteractiveElement as _, Styled, Subscription, WeakEntity, Window,
};
use gpui_component::{
    button::{Button, ButtonVariants as _},
    dock::{register_panel, DockArea, Panel, PanelControl, PanelEvent, PanelState},
    h_flex,
    menu::{DropdownMenu as _, PopupMenuItem},
    notification::Notification,
    v_flex, ActiveTheme as _, Disableable as _, Icon, Sizable as _, WindowExt as _,
};
use std::collections::HashMap;
use std::path::PathBuf;
use terminal::{TabId, TabKind, TerminalManager, TerminalManagerEvent, TerminalView};

use crate::coding_flow::{CodingHub, TokenRefreshers};
use crate::icons::{registry, ExpIcon};
use crate::navigation;
use crate::repo_resolver::{repo_resolver_for_window, RepoLookup, RepoResolver};

/// Stable serialization name for the panel registry (§3.3: never change it).
pub const PANEL_NAME: &str = "TerminalDock";

/// Per-tab hover group (EXP-65): reveals the undock button, mirroring the
/// center tabs' `TAB_GROUP` idiom.
const TAB_GROUP: &str = "terminal-tab";

/// Keymap scope for the dock-local bindings — an ancestor of the focused
/// terminal view in the dispatch path, so the chords work while typing in
/// the terminal (bindings match before raw key-down listeners).
const KEY_CONTEXT: &str = "TerminalDock";

actions!(
    exp,
    [
        /// New plain shell tab in the terminal dock (§6.13 "+").
        NewTerminalTab,
        /// Close the active terminal tab (kills its child, §6.13).
        CloseTerminalTab,
        /// Switch to the next terminal tab.
        NextTerminalTab,
        /// Switch to the previous terminal tab.
        PrevTerminalTab,
    ]
);

/// Register the panel + bind the dock-scoped keys. Called once from
/// [`crate::init`].
pub(crate) fn init(cx: &mut App) {
    // Resolve the §6.12 login PATH off-thread now, so the first spawn's
    // `build_command` finds the OnceLock already filled instead of running
    // `$SHELL -lic` on the gpui foreground.
    terminal::prewarm_login_path();

    // EXP-301: the rehydration path builds an EMPTY panel — a restored layout
    // never brings terminals back (nothing tab-side is persisted anymore).
    register_panel(cx, PANEL_NAME, |dock_area, _state, _info, window, cx| {
        Box::new(cx.new(|cx| TerminalDockPanel::new(dock_area, window, cx)))
    });

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

/// The bottom-dock terminal panel: one per window, owning that window's
/// [`TerminalManager`] (multi-window = independent tab strips over the same
/// global store, §7.6).
pub struct TerminalDockPanel {
    focus_handle: FocusHandle,
    manager: Entity<TerminalManager>,
    dock_area: WeakEntity<DockArea>,
    /// Debounces the expanded-and-empty auto-shell (one deferred spawn per
    /// render burst).
    spawn_queued: bool,
    /// EXP-325: the promptless agent-shell tabs' P9 token-refresher holds
    /// (tab id → the retained trunk clone), released on `TabClosed`. (A
    /// window closed with live holds leaks its refresh loops until quit —
    /// bounded and rare; sessions normally end by tab close.)
    agent_shell_holds: HashMap<TabId, PathBuf>,
    _subscription: Subscription,
}

impl TerminalDockPanel {
    /// This window's tab-strip model — §07's Start-coding launcher / actions
    /// panel open their `Claude`/`Action` tabs through it (the §6.13 "same
    /// entry point" rule; resolved per window via `coding_flow`).
    pub(crate) fn manager(&self) -> &Entity<TerminalManager> {
        &self.manager
    }

    /// The only constructor — fresh AND rehydrated panels start with zero
    /// tabs (EXP-301: launching the app must never spawn a terminal).
    pub fn new(
        dock_area: WeakEntity<DockArea>,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> Self {
        let manager = cx.new(|_| TerminalManager::new());

        let subscription = cx.subscribe_in(
            &manager,
            window,
            |this, _, event: &TerminalManagerEvent, window, cx| {
                match event {
                    // §6.13: the panel expands when a tab is created — also
                    // the path Phase 5's play button / remote start rides.
                    TerminalManagerEvent::TabOpened(_) => {
                        this.expand_dock(window, cx);
                        this.focus_active_terminal(window, cx);
                    }
                    TerminalManagerEvent::TabClosed(id) => {
                        // EXP-325: a promptless agent-shell tab releases its
                        // token-refresher hold with the tab.
                        if let Some(clone) = this.agent_shell_holds.remove(id) {
                            TokenRefreshers::release(&clone, cx);
                        }
                        // §8.8b: closing the last tab collapses the bottom dock
                        // (mirror of the TabOpened expand); otherwise focus the
                        // tab that took over.
                        if this.manager.read(cx).is_empty() {
                            this.collapse_dock(window, cx);
                        } else {
                            this.focus_active_terminal(window, cx);
                        }
                    }
                    // Exit strip/badge render on notify; ending the
                    // coding_sessions row is Phase 5's ExitHook (§6.7).
                    TerminalManagerEvent::TabExited { .. } => {}
                }
                cx.notify();
            },
        );

        // Dev hook: EXP_DEV_OPEN_SHELL=1 opens one plain shell tab at startup
        // so the §11.4 terminal-dock smoke (tab strip + rendered prompt +
        // expanded dock) is demonstrable headlessly/in CI without
        // synthesizing a `+` click. Dev-only — never document for users.
        // Runs AFTER the subscription so TabOpened expands the dock.
        if std::env::var("EXP_DEV_OPEN_SHELL").is_ok_and(|value| value == "1") {
            let shell_override = crate::coding_flow::terminal_shell_override(cx);
            manager.update(cx, |manager, cx| {
                if let Err(error) = manager.open_shell(None, shell_override, cx) {
                    log::warn!("terminal dock: EXP_DEV_OPEN_SHELL spawn failed: {error:#}");
                }
            });
        }

        // EXP-65: undocked tabs are hidden from the strip — repaint when the
        // undock registry changes (tab popped out / reattached).
        if let Some(undock_state) = crate::undock::state(cx) {
            cx.observe(&undock_state, |_, _, cx| cx.notify()).detach();
        }

        // EXP-325: the issue-styled chips follow the synced issue rows
        // (title/status/pr_state — boards for the status resolution's team
        // lookup), the local session registry, and the shared merge state.
        // (`try_global`: the rehydrate test builds panels without a store.)
        let collections =
            sync::Store::try_global(cx).map(|store| store.collections().clone());
        if let Some(collections) = collections {
            cx.observe(&collections.issues, |_, _, cx| cx.notify()).detach();
            cx.observe(&collections.issue_statuses, |_, _, cx| cx.notify())
                .detach();
            cx.observe(&collections.boards, |_, _, cx| cx.notify()).detach();
        }
        let local_sessions = crate::coding_flow::LocalSessions::global(cx);
        cx.observe(&local_sessions, |_, _, cx| cx.notify()).detach();
        let merge_state = crate::pr_merge::MergeState::global(cx);
        cx.observe(&merge_state, |_, _, cx| cx.notify()).detach();

        Self {
            focus_handle: cx.focus_handle(),
            manager,
            dock_area,
            spawn_queued: false,
            agent_shell_holds: HashMap::new(),
            _subscription: subscription,
        }
    }

    /// Whether the bottom dock is collapsed to its 29px strip. A chrome-less
    /// `DockItem::Panel` keeps rendering its full content inside that strip
    /// (the Dock only shrinks the container), so the panel must render the
    /// compact strip itself when collapsed ("bottom bar cut off").
    fn dock_collapsed(&self, cx: &App) -> bool {
        self.dock_area
            .upgrade()
            .and_then(|dock_area| dock_area.read(cx).bottom_dock().cloned())
            .is_some_and(|dock| !dock.read(cx).is_open())
    }

    /// Open the bottom dock if it is collapsed (§4 dock open/close).
    fn expand_dock(&self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        let Some(dock_area) = self.dock_area.upgrade() else {
            return;
        };
        let Some(dock) = dock_area.read(cx).bottom_dock().cloned() else {
            return;
        };
        if !dock.read(cx).is_open() {
            dock.update(cx, |dock, cx| dock.set_open(true, window, cx));
        }
    }

    /// Collapse the bottom dock if it is open (§8.8b: the last tab closed) —
    /// the Dock keeps its 29px toggle strip so the user can re-open it.
    fn collapse_dock(&self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        let Some(dock_area) = self.dock_area.upgrade() else {
            return;
        };
        let Some(dock) = dock_area.read(cx).bottom_dock().cloned() else {
            return;
        };
        if dock.read(cx).is_open() {
            dock.update(cx, |dock, cx| dock.set_open(false, window, cx));
        }
    }

    /// Focus follows the active tab (§6.13 "each tab hosting the terminal
    /// element focused"). Undocked tabs render in their own window — never
    /// steal this window's focus for them (EXP-65).
    fn focus_active_terminal(&self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        if let Some(tab) = self.manager.read(cx).active_tab() {
            if crate::undock::is_terminal_tab_undocked(tab.id, cx) {
                return;
            }
            let handle = tab.view.focus_handle(cx);
            window.focus(&handle, cx);
        }
    }

    /// Manager indices of the tabs the dock still shows (EXP-65: undocked
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
    /// tab must not flash through the dock while cycling).
    fn activate_visible_step(
        &mut self,
        forward: bool,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let visible = self.visible_indices(cx);
        if visible.is_empty() {
            return;
        }
        let current_pos = self
            .manager
            .read(cx)
            .active_index()
            .and_then(|active| visible.iter().position(|ix| *ix == active));
        let next_pos = match current_pos {
            Some(pos) if forward => (pos + 1) % visible.len(),
            Some(pos) => (pos + visible.len() - 1) % visible.len(),
            None => 0,
        };
        self.manager
            .update(cx, |manager, cx| manager.activate(visible[next_pos], cx));
        self.focus_active_terminal(window, cx);
    }

    /// Pop the tab out into its own native window (EXP-65). The tab stays in
    /// the manager (exit hooks / stop button / persistence untouched) — the
    /// registry hides it here and the new window renders its view. If it was
    /// the active tab, activate the nearest still-visible neighbor first so
    /// the dock never points at a hidden tab.
    fn undock_tab(&mut self, id: TabId, window: &mut Window, cx: &mut gpui::Context<Self>) {
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
        self.focus_active_terminal(window, cx);
        cx.notify();
    }

    /// The `+` shell tab (v4 §4.6): cwd = the **trunk** clone root of this
    /// window's active board; `$HOME` only off a board screen or while the
    /// clone doesn't exist yet. The repo→trunk-root resolution needs a
    /// (tRPC-only, never synced) `repositories.list` lookup, so the resolve
    /// runs off the foreground and the tab opens once the cwd is known; a
    /// non-board screen (or missing session/board) opens at `$HOME`
    /// immediately (`open_shell(None)`).
    fn new_shell_tab(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
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
        let shell_override = crate::coding_flow::terminal_shell_override(cx);
        let result = self
            .manager
            .update(cx, |manager, cx| manager.open_shell(cwd, shell_override, cx));
        if let Err(error) = result {
            log::error!("terminal dock: shell spawn failed: {error:#}");
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

    /// The session tab strip: one `Tab` per VISIBLE session (title + exit
    /// badge + hover-revealed undock + close button; EXP-65 undocked tabs
    /// are hidden — they render in their own windows), the `+` right after
    /// the last tab, and the collapse chevron at the far right (§6.13).
    /// Clicking the bar's empty space collapses the dock — the whole strip is
    /// the toggle, mirroring the collapsed strip's whole-bar expand
    /// (tab/button handlers stop propagation so their clicks never fall
    /// through to the collapse).
    fn render_tab_bar(
        &self,
        metas: &[TabMeta],
        selected_ix: usize,
        cx: &mut gpui::Context<Self>,
    ) -> impl IntoElement {
        // EXP-277: hand-rolled rounded chips (crate::surface::tab_chip), same
        // treatment as the center tab strip — gpui-component's TabBar is
        // square with a strip-wide bottom border.
        // EXP-325: the shared merge state drives the hover merge button —
        // materialized here (needs `&mut App`), read inside the chip closure.
        let merge_state = crate::pr_merge::MergeState::global(cx);
        let chips = metas.iter().enumerate().map(|(ix, meta)| {
            let id = meta.id;
            let manager_ix = meta.manager_ix;
            let merge_button = meta
                .issue
                .as_ref()
                .filter(|issue| issue.pr_open)
                .map(|issue| self.tab_merge_button(ix, issue, &merge_state, cx));
            let chip = crate::surface::tab_chip(ix == selected_ix, cx)
                .id(("terminal-tab", ix))
                .group(TAB_GROUP)
                .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                    cx.stop_propagation();
                    this.manager
                        .update(cx, |manager, cx| manager.activate(manager_ix, cx));
                    this.focus_active_terminal(window, cx);
                }));
            // EXP-325: an issue-session tab renders the center issue-tab
            // treatment (status glyph + mono identifier + synced title,
            // mirroring `screens::render_tab_strip`); everything else keeps
            // the plain terminal title.
            let chip = match &meta.issue {
                Some(issue) => chip
                    .child(crate::icons::resolved_status_icon(&issue.status, cx).xsmall())
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .font_family(theme::terminal::FONT_FAMILY)
                            .whitespace_nowrap()
                            .child(issue.identifier.clone()),
                    )
                    .when_some(issue.title.clone(), |chip, title| {
                        chip.child(div().max_w(px(180.)).truncate().child(title))
                    }),
                None => chip.child(div().max_w(px(180.)).truncate().child(meta.title.clone())),
            };
            chip.child(
                    h_flex()
                        .gap_0p5()
                        .items_center()
                        .when_some(meta.exit_code, |this, code| {
                            let color = if code == 0 {
                                cx.theme().success
                            } else {
                                cx.theme().danger
                            };
                            this.child(
                                div()
                                    .text_xs()
                                    .px_1()
                                    .rounded(px(3.))
                                    .bg(color.opacity(0.15))
                                    .text_color(color)
                                    .child(SharedString::from(code.to_string())),
                            )
                        })
                        // EXP-325: quick merge for an in-review issue session
                        // (hover-revealed while idle; armed/merging stay
                        // visible so the confirm never vanishes mid-click).
                        .when_some(merge_button, |this, button| this.child(button))
                        // Hover-revealed undock (EXP-65) — same treatment as
                        // the center tabs; `invisible` keeps the layout slot.
                        .child(
                            div()
                                .invisible()
                                .group_hover(TAB_GROUP, |style| style.visible())
                                .child(
                                    Button::new(("undock-terminal-tab", ix))
                                        .ghost()
                                        .xsmall()
                                        .icon(ExpIcon::ExternalLink)
                                        .tooltip("Open in new window")
                                        .on_click(cx.listener(
                                            move |this, _: &ClickEvent, window, cx| {
                                                cx.stop_propagation();
                                                this.undock_tab(id, window, cx);
                                            },
                                        )),
                                ),
                        )
                        .child(
                            Button::new(("close-terminal-tab", ix))
                                .ghost()
                                .xsmall()
                                .icon(registry::UI_CLOSE)
                                .on_click(cx.listener(move |this, _: &ClickEvent, _window, cx| {
                                    cx.stop_propagation();
                                    this.manager
                                        .update(cx, |manager, cx| manager.close_tab(id, cx));
                                })),
                        ),
                )
        });
        // Clicking the strip's empty space collapses the dock — the whole
        // strip is the toggle (chip/button handlers stop propagation).
        h_flex()
            .id("terminal-tab-strip")
            .w_full()
            .px_1()
            .py_0p5()
            .gap_1()
            .items_center()
            .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                this.collapse_dock(window, cx);
            }))
            // The chips scroll as a group (the center strip does the same) —
            // the chips are `flex_none`, so without this a handful of live
            // sessions would push the `+` and the collapse chevron out of the
            // window and leave the overflowing tabs unclickable.
            .child(
                h_flex()
                    .id("terminal-tab-chips")
                    .min_w_0()
                    .flex_1()
                    .overflow_x_scroll()
                    .gap_1()
                    .items_center()
                    .children(chips)
                    // The `+` rides the slot right AFTER the last tab
                    // (JetBrains placement), not the far-right suffix.
                    // EXP-325: a dropdown — the doctor-installed agent CLIs
                    // (immediate empty session on the current board's trunk
                    // repo; a repo submenu when the team has several) plus
                    // the plain shell (cmd-t unchanged).
                    .child(self.new_tab_menu(cx)),
            )
            .child(
                Button::new("collapse-terminal-dock")
                    .ghost()
                    .xsmall()
                    .icon(registry::UI_CHEVRON_DOWN)
                    .tooltip("Hide terminal")
                    .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                        cx.stop_propagation();
                        this.collapse_dock(window, cx);
                    })),
            )
    }

    /// The EXP-325 merge button on an in-review issue-session tab: the
    /// shared two-click confirm (`pr_merge`), hover-revealed while idle
    /// (armed/merging stay visible so the ~5s confirm window survives
    /// pointer drift). A failed merge (typically conflicts) jumps to the
    /// Reviews tool window, where the shared error caption + Fix-conflicts
    /// button render exactly as a Reviews-originated failure.
    fn tab_merge_button(
        &self,
        ix: usize,
        issue: &IssueTabMeta,
        merge_state: &Entity<crate::pr_merge::MergeState>,
        cx: &gpui::Context<Self>,
    ) -> gpui::AnyElement {
        let (armed, merging) = {
            let state = merge_state.read(cx);
            (state.armed(&issue.issue_id), state.merging(&issue.issue_id))
        };
        let mut button = Button::new(("merge-terminal-tab", ix)).xsmall();
        if merging {
            button = button
                .outline()
                .label("Merging…")
                .loading(true)
                .disabled(true);
        } else if armed {
            button = button.outline().label("Confirm merge").danger();
        } else {
            button = button
                .ghost()
                .icon(ExpIcon::GitMerge)
                .tooltip("Merge PR — completes every linked issue and ends its coding session");
        }
        let issue_id = issue.issue_id.clone();
        let button = button.on_click(cx.listener(move |_, _: &ClickEvent, window, cx| {
            cx.stop_propagation();
            let handle = window.window_handle();
            crate::pr_merge::two_click(
                crate::pr_merge::MergeOp::MergeIssuePr {
                    issue_id: issue_id.clone(),
                },
                Some(Box::new(move |cx: &mut App| {
                    let _ = handle.update(cx, |_, window, cx| {
                        crate::sidebar::activate_tool(
                            window,
                            cx,
                            crate::sidebar::ToolWindow::Reviews,
                        );
                    });
                })),
                None,
                cx,
            );
        }));
        if armed || merging {
            button.into_any_element()
        } else {
            div()
                .invisible()
                .group_hover(TAB_GROUP, |style| style.visible())
                .child(button)
                .into_any_element()
        }
    }

    /// The "+" dropdown (EXP-325): one item per doctor-INSTALLED agent CLI —
    /// clicking immediately launches an empty promptless session of that
    /// agent on the current team's trunk repo (several board-backed repos →
    /// a repo picker submenu; resolver still loading / no repo → disabled) —
    /// plus the plain "New shell" (the pre-EXP-325 `+` behavior; cmd-t
    /// unchanged). Installed agents and repos resolve fresh at OPEN time
    /// (the closure outlives renders); no doctor report yet → only the
    /// shell item.
    fn new_tab_menu(&self, cx: &gpui::Context<Self>) -> impl IntoElement {
        let panel = cx.entity().downgrade();
        Button::new("new-terminal-tab")
            .ghost()
            .xsmall()
            .icon(registry::UI_ADD)
            .tooltip("New session")
            .dropdown_menu(move |mut menu, window, cx| {
                let hub = CodingHub::global(cx);
                let installed = hub
                    .read(cx)
                    .doctor
                    .report
                    .as_ref()
                    .map(|report| report.installed_agents())
                    .unwrap_or_default();
                let resolver = repo_resolver_for_window(window, cx);
                resolver.update(cx, |resolver, cx| resolver.ensure_loaded(cx));
                let repos = resolver.read(cx).board_backed_repos();
                for agent in installed {
                    let icon = Icon::from(crate::coding_selects::agent_icon(agent));
                    let label = agent.label();
                    match repos.as_deref() {
                        Some([repo]) => {
                            // One repo — launch directly, no submenu.
                            let panel = panel.clone();
                            let repository_id = repo.repository_id.clone();
                            let full_name = repo.full_name.clone();
                            menu = menu.item(PopupMenuItem::new(label).icon(icon).on_click(
                                move |_, window, cx| {
                                    let Some(panel) = panel.upgrade() else {
                                        return;
                                    };
                                    let repository_id = repository_id.clone();
                                    let full_name = full_name.clone();
                                    panel.update(cx, |panel, cx| {
                                        panel.launch_agent_shell(
                                            agent,
                                            repository_id,
                                            full_name,
                                            window,
                                            cx,
                                        );
                                    });
                                },
                            ));
                        }
                        Some(repos) if repos.len() > 1 => {
                            // Several distinct repos — pick one explicitly.
                            let panel = panel.clone();
                            let repos: Vec<(String, String)> = repos
                                .iter()
                                .map(|repo| {
                                    (repo.repository_id.clone(), repo.full_name.clone())
                                })
                                .collect();
                            menu = menu.submenu_with_icon(
                                Some(icon),
                                label,
                                window,
                                cx,
                                move |mut submenu, _window, _cx| {
                                    for (repository_id, full_name) in &repos {
                                        let panel = panel.clone();
                                        let repository_id = repository_id.clone();
                                        let full_name = full_name.clone();
                                        let item_label = SharedString::from(full_name.clone());
                                        submenu = submenu.item(
                                            PopupMenuItem::new(item_label).on_click(
                                                move |_, window, cx| {
                                                    let Some(panel) = panel.upgrade() else {
                                                        return;
                                                    };
                                                    let repository_id = repository_id.clone();
                                                    let full_name = full_name.clone();
                                                    panel.update(cx, |panel, cx| {
                                                        panel.launch_agent_shell(
                                                            agent,
                                                            repository_id,
                                                            full_name,
                                                            window,
                                                            cx,
                                                        );
                                                    });
                                                },
                                            ),
                                        );
                                    }
                                    submenu
                                },
                            );
                        }
                        // Resolver loading, fetch failed, or zero
                        // board-backed repos — the agent has no trunk to
                        // land on; keep the item visible but inert.
                        _ => {
                            menu = menu.item(PopupMenuItem::new(label).icon(icon).disabled(true));
                        }
                    }
                }
                // `separator` no-ops on an empty menu — no leading rule when
                // there are no agent items.
                let shell_panel = panel.clone();
                menu.separator().item(
                    PopupMenuItem::new("New shell")
                        .icon(Icon::new(registry::NAV_TERMINAL))
                        .on_click(move |_, window, cx| {
                            let Some(panel) = shell_panel.upgrade() else {
                                return;
                            };
                            panel.update(cx, |panel, cx| panel.new_shell_tab(window, cx));
                        }),
                )
            })
    }

    /// The EXP-325 promptless agent launch: background
    /// [`coding::prepare_agent_shell`] (doctor → token → clone/autopull →
    /// MCP wiring → promptless argv with the agent's settings defaults) →
    /// foreground [`TabKind::AgentShell`] tab in THIS dock. No
    /// `coding_sessions` row / heartbeat / exit hook — the session has no
    /// issue/batch/action subject; the P9 token-refresher hold keeps `git
    /// push` working past the token TTL, released on tab close.
    fn launch_agent_shell(
        &mut self,
        agent: coding::CodingAgent,
        repository_id: String,
        full_name: String,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(deps) = crate::coding_flow::build_action_deps(cx) else {
            log::warn!("terminal dock: agent shell ignored — not signed in");
            return;
        };
        let options = coding::LaunchOptions::defaults_for(&deps.settings, agent);
        let request = coding::AgentShellRequest {
            options,
            repository_id,
            full_name,
        };
        cx.spawn_in(window, async move |this, cx| {
            let prepared = cx
                .background_executor()
                .spawn(async move { coding::prepare_agent_shell(&request, &deps) })
                .await;
            let _ = this.update_in(cx, |this, window, cx| {
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
                        log::warn!("terminal dock: agent shell prepare failed: {err}");
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
                        log::warn!("terminal dock: agent shell spawn failed: {error:#}");
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

    /// The collapsed-dock strip: the bottom dock keeps a 29px band
    /// when closed, and a chrome-less panel renders its full content clipped
    /// into it — instead render this compact one-line strip. Clicking it (or
    /// the chevron) re-opens the dock.
    fn render_collapsed_strip(
        &self,
        tab_count: usize,
        cx: &mut gpui::Context<Self>,
    ) -> impl IntoElement {
        let label: SharedString = if tab_count > 0 {
            format!("Terminal ({tab_count})").into()
        } else {
            "Terminal".into()
        };
        h_flex()
            .id("terminal-collapsed-strip")
            .w_full()
            .h(px(29.))
            .px_3()
            .gap_2()
            .items_center()
            .flex_shrink_0()
            .border_t_1()
            .border_color(cx.theme().border)
            .bg(cx.theme().title_bar)
            .text_color(cx.theme().muted_foreground)
            .cursor_pointer()
            .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                this.expand_dock(window, cx);
                // Zero sessions: the expanded render auto-spawns a shell —
                // there is deliberately no empty state.
                if !this.manager.read(cx).is_empty() {
                    this.focus_active_terminal(window, cx);
                }
            }))
            .child(Icon::new(registry::NAV_TERMINAL).xsmall())
            .child(div().text_xs().child(label))
            .child(div().flex_1())
            .child(Icon::new(registry::UI_CHEVRON_UP).xsmall())
    }

    /// EXP-65: every visible tab popped out into its own window — the dock
    /// stays usable (the bar keeps the `+`), with a hint instead of a
    /// terminal. Deliberately NOT the auto-spawn path: the manager isn't
    /// empty, the tabs just live elsewhere.
    fn render_undocked_hint(&self, cx: &gpui::Context<Self>) -> impl IntoElement {
        v_flex()
            .flex_1()
            .min_h_0()
            .items_center()
            .justify_center()
            .gap_1()
            .text_xs()
            .text_color(cx.theme().muted_foreground)
            .child(Icon::new(registry::NAV_TERMINAL).small())
            .child("All terminal tabs are open in separate windows.")
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

/// Per-tab render snapshot (cloned out so the manager borrow ends before the
/// listeners borrow `cx`). One entry per VISIBLE tab — undocked tabs are
/// filtered out, so `manager_ix` keeps the strip position → manager index
/// mapping honest (EXP-65).
struct TabMeta {
    manager_ix: usize,
    id: TabId,
    title: SharedString,
    exit_code: Option<i32>,
    /// EXP-325: present when this tab is a LOCAL issue coding session whose
    /// issue row is synced — the chip then renders the center issue-tab
    /// treatment (status glyph + mono identifier + synced title + hover
    /// merge) instead of the plain terminal title. Batch/action/shell tabs
    /// (and unsynced issues) stay `None`.
    issue: Option<IssueTabMeta>,
}

/// The issue-chip snapshot of one issue-session terminal tab (EXP-325).
struct IssueTabMeta {
    issue_id: String,
    status: domain::statuses::ResolvedStatus,
    identifier: SharedString,
    /// `None` for a blank issue title — the identifier already labels the
    /// chip (the EXP-310 center-tab rule).
    title: Option<SharedString>,
    /// Whether the issue's PR is open — gates the hover merge button.
    pr_open: bool,
}

/// Resolve a tab back to its issue chip, when it is an issue session over a
/// synced issue row (mirrors `screens::chip_content`).
fn issue_tab_meta(tab_id: TabId, cx: &App) -> Option<IssueTabMeta> {
    let sessions = crate::coding_flow::LocalSessions::global_ref(cx)?;
    let sessions = sessions.read(cx);
    let crate::coding_flow::SessionSubject::Issue(issue_id) = sessions.subject_for_tab(tab_id)?
    else {
        return None;
    };
    let store = sync::Store::try_global(cx)?;
    let issue = store.collections().issues.read(cx).get(issue_id)?;
    let title = issue.title.trim();
    Some(IssueTabMeta {
        issue_id: issue.id.clone(),
        status: crate::queries::resolve_issue_status(cx, issue),
        identifier: SharedString::from(issue.identifier.clone()),
        title: (!title.is_empty()).then(|| SharedString::from(title.to_string())),
        pr_open: issue.pr_state.as_deref() == Some("open"),
    })
}

impl Panel for TerminalDockPanel {
    fn panel_name(&self) -> &'static str {
        PANEL_NAME
    }

    fn title(&mut self, _window: &mut Window, _cx: &mut gpui::Context<Self>) -> impl IntoElement {
        "Terminal"
    }

    /// Fixed chrome: the dock collapses via the Dock toggle, tabs close via
    /// their own close buttons — the panel itself is not closable.
    fn closable(&self, _cx: &App) -> bool {
        false
    }

    fn zoomable(&self, _cx: &App) -> Option<PanelControl> {
        None
    }

    /// Focus the active terminal when the dock panel becomes active (tab
    /// click on the outer `TabPanel` / dock re-open). Always notify: the Dock
    /// caches this panel's element, and collapse/expand arrives here (via
    /// `DockItem::set_collapsed`) — without the notify the collapsed strip /
    /// full content swap would not repaint.
    fn set_active(&mut self, active: bool, window: &mut Window, cx: &mut gpui::Context<Self>) {
        if active {
            self.focus_active_terminal(window, cx);
        }
        cx.notify();
    }

    /// EXP-301: the dock persists NOTHING about its tabs — a relaunch must
    /// never put a terminal in the user's face. `PanelState::new` still carries
    /// the registry name (+ the default `PanelInfo::Panel`), so a saved layout
    /// rehydrates an EMPTY dock panel in the right slot.
    fn dump(&self, _cx: &App) -> PanelState {
        PanelState::new(self)
    }
}

impl gpui::EventEmitter<PanelEvent> for TerminalDockPanel {}

impl Focusable for TerminalDockPanel {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for TerminalDockPanel {
    fn render(&mut self, _window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        // Snapshot the strip so the manager borrow ends before listeners.
        // EXP-65: undocked tabs render in their own windows — the strip
        // skips them, and the active view is only painted here when the
        // active tab is NOT undocked (one window paints a view at a time).
        let (metas, active_id, active_view, active_exit): (
            Vec<TabMeta>,
            Option<TabId>,
            Option<Entity<TerminalView>>,
            Option<i32>,
        ) = {
            let manager = self.manager.read(cx);
            let metas = manager
                .tabs()
                .iter()
                .enumerate()
                .filter(|(_, tab)| !crate::undock::is_terminal_tab_undocked(tab.id, cx))
                .map(|(manager_ix, tab)| TabMeta {
                    manager_ix,
                    id: tab.id,
                    title: tab.title().clone(),
                    exit_code: tab.exit_code(),
                    issue: issue_tab_meta(tab.id, cx),
                })
                .collect();
            let active = manager
                .active_tab()
                .filter(|tab| !crate::undock::is_terminal_tab_undocked(tab.id, cx));
            (
                metas,
                active.map(|tab| tab.id),
                active.map(|tab| tab.view.clone()),
                active.and_then(|tab| tab.exit_code()),
            )
        };
        let tab_count = self.manager.read(cx).len();

        let root = v_flex()
            .id("terminal-dock")
            .key_context(KEY_CONTEXT)
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(Self::on_new_tab))
            .on_action(cx.listener(Self::on_close_tab))
            .on_action(cx.listener(Self::on_next_tab))
            .on_action(cx.listener(Self::on_prev_tab))
            .size_full()
            .overflow_hidden();

        // Collapsed dock: only the compact strip — never the full
        // content squeezed/clipped into the 29px band.
        if self.dock_collapsed(cx) {
            return root.child(self.render_collapsed_strip(metas.len(), cx));
        }

        let Some(active_view) = active_view else {
            if tab_count > 0 {
                // Tabs exist but none is visible/active here — every one is
                // undocked (or the active tab just popped out mid-frame).
                // Keep the bar (the `+` stays reachable) over a hint.
                return root
                    .child(self.render_tab_bar(&metas, 0, cx))
                    .child(self.render_undocked_hint(cx));
            }
            // An expanded, empty dock never shows an empty state — it spawns
            // a shell immediately (deferred out of render). Every expand path
            // funnels here, so a stray `set_open(true)` still gets a shell.
            if !self.spawn_queued {
                self.spawn_queued = true;
                cx.defer_in(_window, |this, window, cx| {
                    this.spawn_queued = false;
                    if this.manager.read(cx).is_empty() && !this.dock_collapsed(cx) {
                        this.new_shell_tab(window, cx);
                    }
                });
            }
            return root;
        };

        let selected_ix = active_id
            .and_then(|id| metas.iter().position(|meta| meta.id == id))
            .unwrap_or(0);
        root.child(self.render_tab_bar(&metas, selected_ix, cx))
            // min_h(0) so the flex child can shrink with the dock; the grid
            // element itself guards the 0-height collapsed case (§6.9).
            .child(div().flex_1().min_h_0().child(active_view))
            .when_some(active_exit, |this, code| {
                this.child(exit_strip(code, cx))
            })
    }
}

#[cfg(test)]
mod tests {
    use gpui::TestAppContext;
    use gpui_component::dock::{DockAreaState, PanelInfo};

    use super::*;

    /// A real pre-EXP-301 `window-0.json`: an OPEN bottom dock whose panel
    /// info still carries a persisted login shell and a claude tab. Old files
    /// like this stay on disk after the upgrade, so the restore path has to
    /// stay immune to them — not merely stop writing them.
    const LEGACY_LAYOUT: &str = r#"{
      "version": 8,
      "center": { "panel_name": "Center", "children": [], "info": { "panel": null } },
      "bottom_dock": {
        "panel": {
          "panel_name": "TerminalDock",
          "children": [],
          "info": {
            "panel": {
              "tabs": [
                { "kind": "shell", "cwd": "/tmp" },
                { "kind": "claude", "cwd": "/tmp" }
              ],
              "active": 1
            }
          }
        },
        "placement": "bottom",
        "size": 547.9297,
        "open": true
      }
    }"#;

    /// EXP-301: opening the app must never put a terminal in the user's face.
    /// Rehydrating a saved layout builds an EMPTY dock panel — no PTY is
    /// spawned for a persisted `shell` tab (nor for the legacy `run` kind),
    /// and the panel re-dumps without any tab payload.
    #[gpui::test]
    async fn a_saved_layout_rehydrates_the_dock_with_zero_terminals(cx: &mut TestAppContext) {
        cx.update(|cx| {
            // Same order as `app::main` — the component/theme globals must
            // exist before a layout can rehydrate.
            gpui_component::init(cx);
            theme::init(cx);
            init(cx);
        });

        let state: DockAreaState = serde_json::from_str(LEGACY_LAYOUT).expect("parse layout");
        let window = cx.add_window(|window, cx| DockArea::new("test-dock", Some(8), window, cx));

        window
            .update(cx, |dock_area, window, cx| {
                dock_area.load(state, window, cx).expect("load layout");

                let dock = dock_area
                    .bottom_dock()
                    .cloned()
                    .expect("bottom dock restored");
                let item = dock.read(cx).panel().clone();
                let panel = crate::coding_flow::find_terminal_dock(&item)
                    .expect("terminal dock panel rehydrated");

                assert!(
                    panel.read(cx).manager().read(cx).is_empty(),
                    "a restored layout must not respawn terminal tabs"
                );

                // The dump carries the registry name only — nothing to
                // resurrect on the NEXT launch either.
                let dumped = panel.read(cx).dump(cx);
                assert_eq!(dumped.panel_name, PANEL_NAME);
                assert_eq!(dumped.info, PanelInfo::Panel(serde_json::Value::Null));
            })
            .expect("window update");
    }
}
