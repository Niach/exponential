//! The bottom terminal dock (masterplan-v3 §6.13 / §7.5) — the
//! JetBrains-style multi-tab terminal panel.
//!
//! One [`TerminalDockPanel`] per window lives inside the bottom `Dock`'s
//! `TabPanel` (§3.3); *inside* it, a gpui-component `Tab`/`TabBar` strip
//! lists the [`terminal::TerminalManager`]'s sessions — **not** Zed's GPL
//! `Pane`/`Dock` (§6.13's licensing rule). Behavior:
//!
//! - **"+"** (and cmd-t / ctrl-shift-t inside the dock) → a plain `Shell`
//!   tab (`$SHELL -l`, cwd = the active project's **trunk** clone root, v4
//!   §4.6; `$HOME` only off a project screen or before the clone exists).
//!   This is also the launch surface: the Start-coding launcher and run-bar
//!   play button call the same `TerminalManager::open_tab`.
//! - close buttons per tab (and cmd-w / ctrl-shift-w), ctrl-tab /
//!   ctrl-shift-tab to switch;
//! - empty state: "No terminal sessions" + a New-shell action;
//! - the dock **expands when a tab is created** (`TabOpened` →
//!   `Dock::set_open`, §4's dock open/close) and the new tab's terminal is
//!   focused; the grid element resizes with the dock (§6.10);
//! - a dead tab **stays open** with its final scrollback and shows the
//!   JetBrains "Process finished with exit code N" strip + a green-0 /
//!   red-non-zero badge on the tab (§7.5's exit-code strip);
//! - persistence (§6.13): `{ kind, cwd, run_config_id }` per tab — never
//!   scrollback. On restore, `Shell` tabs re-open cold; `Claude`/`Run` tabs
//!   are not respawned (a coding session is bound to a live worktree; §07
//!   decides resumability).
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
    dock::{register_panel, DockArea, Panel, PanelControl, PanelEvent, PanelInfo, PanelState},
    h_flex,
    tab::{Tab, TabBar},
    v_flex, ActiveTheme as _, Icon, IconName, Sizable as _, Size,
};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use terminal::{TabId, TabKind, TerminalManager, TerminalManagerEvent, TerminalView};

use crate::coding_flow::CodingHub;
use crate::navigation;
use crate::repo_resolver::{repo_resolver_for_window, RepoLookup, RepoResolver};

/// Stable serialization name for the panel registry (§3.3: never change it).
pub const PANEL_NAME: &str = "TerminalDock";

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

    register_panel(cx, PANEL_NAME, |dock_area, state, _info, window, cx| {
        Box::new(cx.new(|cx| TerminalDockPanel::from_state(dock_area, state, window, cx)))
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

/// §6.13 persistence unit: `{ kind, cwd, run_config_id }` — never scrollback.
#[derive(Debug, Serialize, Deserialize)]
struct PersistedTab {
    kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    cwd: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    run_config_id: Option<String>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct PersistedDock {
    #[serde(default)]
    tabs: Vec<PersistedTab>,
    #[serde(default)]
    active: usize,
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
    _subscription: Subscription,
    /// Repaints the §8.5 "Remote steering" banner when a `presence` frame
    /// changes the steerer. `None` when steer isn't installed (headless tests).
    _steer_subscription: Option<Subscription>,
}

impl TerminalDockPanel {
    /// This window's tab-strip model — §07's Start-coding launcher / run bar
    /// open their `Claude`/`Run` tabs through it (the §6.13 "same entry
    /// point" rule; resolved per window via `coding_flow`).
    pub(crate) fn manager(&self) -> &Entity<TerminalManager> {
        &self.manager
    }

    pub fn new(
        dock_area: WeakEntity<DockArea>,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> Self {
        Self::build(dock_area, PersistedDock::default(), window, cx)
    }

    /// Registry rehydration path (§3.3): restore `Shell` tabs cold from the
    /// persisted `{ kind, cwd }` list; never auto-respawn `Claude` sessions
    /// and leave exited `Run` tabs closed (§6.13).
    fn from_state(
        dock_area: WeakEntity<DockArea>,
        state: &PanelState,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> Self {
        let persisted = match &state.info {
            PanelInfo::Panel(value) => {
                serde_json::from_value::<PersistedDock>(value.clone()).unwrap_or_default()
            }
            _ => PersistedDock::default(),
        };
        Self::build(dock_area, persisted, window, cx)
    }

    fn build(
        dock_area: WeakEntity<DockArea>,
        persisted: PersistedDock,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> Self {
        let manager = cx.new(|_| TerminalManager::new());

        // Restore BEFORE subscribing so a cold restore neither force-expands
        // the dock (its open state is persisted separately) nor steals focus.
        manager.update(cx, |manager, cx| {
            for tab in persisted
                .tabs
                .iter()
                .filter(|tab| tab.kind == persisted_kind(&TabKind::Shell))
            {
                if let Err(error) = manager.open_shell(tab.cwd.clone(), cx) {
                    log::warn!("terminal dock: restoring shell tab failed: {error:#}");
                }
            }
            if !manager.is_empty() {
                manager.activate(persisted.active.min(manager.len() - 1), cx);
            }
        });

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
                    TerminalManagerEvent::TabClosed(_) => {
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
        // Runs AFTER the subscription so TabOpened expands the dock, and only
        // when nothing was restored (no doubling on rehydration).
        if manager.read(cx).is_empty()
            && std::env::var("EXP_DEV_OPEN_SHELL").is_ok_and(|value| value == "1")
        {
            manager.update(cx, |manager, cx| {
                if let Err(error) = manager.open_shell(None, cx) {
                    log::warn!("terminal dock: EXP_DEV_OPEN_SHELL spawn failed: {error:#}");
                }
            });
        }

        // §8.5: repaint when a `presence` frame flips the remote steerer, so the
        // banner shows/hides without waiting for an unrelated tab event.
        let steer_subscription =
            crate::steer_wiring::observe_steer_presence(cx, |_, cx| cx.notify());

        Self {
            focus_handle: cx.focus_handle(),
            manager,
            dock_area,
            spawn_queued: false,
            _subscription: subscription,
            _steer_subscription: steer_subscription,
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
    /// element focused").
    fn focus_active_terminal(&self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        if let Some(tab) = self.manager.read(cx).active_tab() {
            let handle = tab.view.focus_handle(cx);
            window.focus(&handle, cx);
        }
    }

    /// The `+` shell tab (v4 §4.6): cwd = the **trunk** clone root of this
    /// window's active project; `$HOME` only off a project screen or while the
    /// clone doesn't exist yet. The repo→trunk-root resolution needs a
    /// (tRPC-only, never synced) `repositories.list` lookup, so the resolve
    /// runs off the foreground and the tab opens once the cwd is known; a
    /// non-project screen (or missing session/project) opens at `$HOME`
    /// immediately (`open_shell(None)`).
    fn new_shell_tab(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        let Some((resolver, project_id, settings)) = self.shell_scope(window, cx) else {
            self.open_shell_cwd(None, cx);
            return;
        };
        // The repo comes from the shared window resolver (the run/git bars keep
        // it warm); a still-loading / unlinked repo just opens at `$HOME`.
        resolver.update(cx, |resolver, cx| resolver.ensure_loaded(cx));
        let full_name = match resolver.read(cx).lookup_project(&project_id) {
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
                    let root = coding::run_launch::run_root(&settings.repos_root_path(), &full_name);
                    // `$HOME` (None) until the clone actually exists on disk.
                    coding::run_launch::shell_cwd(Some(root))
                })
                .await;
            let _ = this.update(cx, |this, cx| this.open_shell_cwd(cwd, cx));
        })
        .detach();
    }

    /// Spawn a shell tab at `cwd` (`None` → `$HOME`, resolved by the manager).
    fn open_shell_cwd(&mut self, cwd: Option<PathBuf>, cx: &mut gpui::Context<Self>) {
        let result = self.manager.update(cx, |manager, cx| manager.open_shell(cwd, cx));
        if let Err(error) = result {
            log::error!("terminal dock: shell spawn failed: {error:#}");
        }
    }

    /// The sync-resolvable inputs for the `+` shell cwd: the shared window repo
    /// resolver, the window's active project (screen scope with the
    /// last-board fallback), and the coding settings (repos root). `None`
    /// with no resolvable project — the caller then opens the shell at
    /// `$HOME`.
    fn shell_scope(
        &self,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> Option<(Entity<RepoResolver>, String, coding::Settings)> {
        let nav = navigation::nav_for_window(window, cx);
        let project_id = navigation::active_project_id(&nav, cx)?;
        let resolver = repo_resolver_for_window(window, cx);
        let settings = CodingHub::global(cx).read(cx).settings.clone();
        Some((resolver, project_id, settings))
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
        self.manager.update(cx, |manager, cx| manager.activate_next(cx));
        self.focus_active_terminal(window, cx);
    }

    fn on_prev_tab(
        &mut self,
        _: &PrevTerminalTab,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        self.manager.update(cx, |manager, cx| manager.activate_prev(cx));
        self.focus_active_terminal(window, cx);
    }

    /// The session tab strip: one `Tab` per session (title + exit badge +
    /// close button), the `+` right after the last tab, and the collapse
    /// chevron at the far right (§6.13). Clicking the bar's empty space
    /// collapses the dock — the whole strip is the toggle, mirroring the
    /// collapsed strip's whole-bar expand (tab/button handlers stop
    /// propagation so their clicks never fall through to the collapse).
    fn render_tab_bar(
        &self,
        metas: &[TabMeta],
        active_ix: usize,
        cx: &mut gpui::Context<Self>,
    ) -> impl IntoElement {
        let tab_bar = TabBar::new("terminal-tabs")
            .with_size(Size::Small) // compact density
            .selected_index(active_ix)
            .on_click(cx.listener(|this, ix: &usize, window, cx| {
                cx.stop_propagation();
                this.manager.update(cx, |manager, cx| manager.activate(*ix, cx));
                this.focus_active_terminal(window, cx);
            }))
            .children(metas.iter().enumerate().map(|(ix, meta)| {
                let id = meta.id;
                Tab::new().label(meta.title.clone()).suffix(
                    h_flex()
                        .gap_1()
                        .pr_1()
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
                        .child(
                            Button::new(("close-terminal-tab", ix))
                                .ghost()
                                .xsmall()
                                .icon(IconName::Close)
                                .on_click(cx.listener(move |this, _: &ClickEvent, _window, cx| {
                                    cx.stop_propagation();
                                    this.manager
                                        .update(cx, |manager, cx| manager.close_tab(id, cx));
                                })),
                        ),
                )
            }))
            // The `+` rides the slot right AFTER the last tab (JetBrains
            // placement), not the far-right suffix.
            .last_empty_space(
                h_flex().px_0p5().child(
                    Button::new("new-terminal-tab")
                        .ghost()
                        .xsmall()
                        .icon(IconName::Plus)
                        .tooltip("New shell")
                        .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                            cx.stop_propagation();
                            this.new_shell_tab(window, cx);
                        })),
                ),
            )
            .suffix(
                h_flex().px_1().child(
                    Button::new("collapse-terminal-dock")
                        .ghost()
                        .xsmall()
                        .icon(IconName::ChevronDown)
                        .tooltip("Hide terminal")
                        .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                            cx.stop_propagation();
                            this.collapse_dock(window, cx);
                        })),
                ),
            );
        div()
            .id("terminal-tab-strip")
            .w_full()
            .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                this.collapse_dock(window, cx);
            }))
            .child(tab_bar)
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
            .child(Icon::new(IconName::SquareTerminal).xsmall())
            .child(div().text_xs().child(label))
            .child(div().flex_1())
            .child(Icon::new(IconName::ChevronUp).xsmall())
    }

    /// The §8.5 "Remote steering" banner: shown while a REMOTE viewer holds the
    /// steer claim on the active coding tab. The LOCAL user is never gated —
    /// this is purely informational plus a "Take over" affordance that revokes
    /// the remote steerer (publisher-sent `claim` → relay `publisherTakeover`).
    fn render_steer_banner(
        &self,
        session_id: String,
        steerer: String,
        cx: &gpui::Context<Self>,
    ) -> impl IntoElement {
        let accent = cx.theme().warning;
        h_flex()
            .gap_2()
            .px_3()
            .py_1()
            .items_center()
            .justify_between()
            .border_b_1()
            .border_color(cx.theme().border)
            .bg(accent.opacity(0.12))
            .text_xs()
            .child(
                h_flex()
                    .gap_1p5()
                    .items_center()
                    .child(Icon::new(IconName::Eye).xsmall().text_color(accent))
                    .child(
                        div()
                            .text_color(cx.theme().foreground)
                            .child(SharedString::from(format!("Remote steering — {steerer}"))),
                    ),
            )
            .child(
                Button::new("steer-take-over")
                    .outline()
                    .xsmall()
                    .label("Take over")
                    .tooltip("Revoke the remote steerer — your typing is never blocked.")
                    .on_click(cx.listener(move |_, _: &ClickEvent, _window, cx| {
                        crate::steer_wiring::take_over(&session_id, cx);
                    })),
            )
    }

    /// The JetBrains "process finished" strip under a dead tab's final
    /// scrollback (§7.5 exit-code strip; the tab stays open).
    fn render_exit_strip(&self, code: i32, cx: &gpui::Context<Self>) -> impl IntoElement {
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

}

/// Per-tab render snapshot (cloned out so the manager borrow ends before the
/// listeners borrow `cx`).
struct TabMeta {
    id: TabId,
    title: SharedString,
    exit_code: Option<i32>,
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

    /// §6.13 persistence: `{ kind, cwd, run_config_id }` per tab + the active
    /// index — never scrollback.
    fn dump(&self, cx: &App) -> PanelState {
        let manager = self.manager.read(cx);
        let persisted = PersistedDock {
            tabs: manager
                .tabs()
                .iter()
                .map(|tab| PersistedTab {
                    kind: persisted_kind(&tab.kind).to_owned(),
                    cwd: tab.cwd.clone(),
                    run_config_id: match &tab.kind {
                        TabKind::Run(id) => Some(id.clone()),
                        _ => None,
                    },
                })
                .collect(),
            active: manager.active_index().unwrap_or(0),
        };
        let mut state = PanelState::new(self);
        state.info = PanelInfo::panel(serde_json::to_value(persisted).unwrap_or_default());
        state
    }
}

fn persisted_kind(kind: &TabKind) -> &'static str {
    match kind {
        TabKind::Claude => "claude",
        // v4 §4.9: not issue-bound and not persisted-restorable as a session —
        // treated like a shell tab for cold-restore (a plain terminal), matching
        // its shell-like runtime behavior.
        TabKind::ClaudeTask => "shell",
        TabKind::Run(_) => "run",
        TabKind::Shell => "shell",
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
        let (metas, active_ix, active_view, active_exit): (
            Vec<TabMeta>,
            usize,
            Option<Entity<TerminalView>>,
            Option<i32>,
        ) = {
            let manager = self.manager.read(cx);
            let metas = manager
                .tabs()
                .iter()
                .map(|tab| TabMeta {
                    id: tab.id,
                    title: tab.title().clone(),
                    exit_code: tab.exit_code(),
                })
                .collect();
            let active_ix = manager.active_index().unwrap_or(0);
            let active = manager.active_tab();
            (
                metas,
                active_ix,
                active.map(|tab| tab.view.clone()),
                active.and_then(|tab| tab.exit_code()),
            )
        };

        // §8.5: is a REMOTE viewer steering the active coding tab right now?
        // (`metas[active_ix]` is the active tab — same order as `manager.tabs()`.)
        let steer_banner = metas
            .get(active_ix)
            .and_then(|meta| crate::steer_wiring::remote_steerer_for_tab(meta.id, cx));

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

        root.child(self.render_tab_bar(&metas, active_ix, cx))
            .when_some(steer_banner, |this, (session_id, steerer)| {
                this.child(self.render_steer_banner(session_id, steerer, cx))
            })
            // min_h(0) so the flex child can shrink with the dock; the grid
            // element itself guards the 0-height collapsed case (§6.9).
            .child(div().flex_1().min_h_0().child(active_view))
            .when_some(active_exit, |this, code| {
                this.child(self.render_exit_strip(code, cx))
            })
    }
}

