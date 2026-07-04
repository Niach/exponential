//! The per-window Workspace — a gpui-component `DockArea` (masterplan-v3
//! §3.3 / §3.6 / §3.10).
//!
//! Layout: **left dock** = the non-collapsible sidebar (EXP-1 #8, a
//! chrome-less `DockItem::Panel` so no tab/title bar renders over it),
//! **center** = an empty `TabPanel` area (Phase 3 fills it with issue
//! list/detail/diff tabs), **bottom dock** = the terminal dock, collapsed by
//! default (Phase 4 fills it with real terminal tabs).
//!
//! Every window gets its own `Root → Workspace → DockArea`, but they all read
//! the same global `Store` (§3.6 multi-window) — the sidebar's window counter
//! is the Phase-1 proof.
//!
//! Persistence (§3.3): each window persists its `DockAreaState` (sizes, which
//! panels are open) to a per-window JSON file and cold-restores panel identity
//! by name via the `ui::init` panel registry. v1 scope: layout only — no
//! per-panel inner-state round-trip.

use std::{path::PathBuf, sync::Arc, time::Duration};

use anyhow::{anyhow, bail, Context as _, Result};
use gpui::{
    div, px, AppContext as _, Edges, Entity, IntoElement, ParentElement, Pixels, Render, Styled,
    Task, WeakEntity, Window,
};
use gpui_component::{
    dock::{DockArea, DockAreaState, DockEvent, DockItem, PanelView},
    h_flex, v_flex, ActiveTheme as _, Root,
};
use sync::{SessionPhase, Store};

use crate::{
    debug_board::DebugBoardPanel, git_bar::GitBar, login::LoginView, navigation,
    run_bar::RunBar, screens::ScreensPanel, sidebar::SidebarPanel,
    terminal_dock::TerminalDockPanel,
};

/// Bump when the default layout shape changes so stale persisted layouts are
/// discarded and rebuilt (mirrors the gpui-component dock example's version
/// check; we silently reset instead of prompting).
/// v2: the Phase-2 debug board landed in the center tabs.
/// v3: the Phase-3 screens panel replaced the debug board as the default
///     center (debug board only behind `EXP_DEV_BOARD=1`).
/// v4: the center is a chrome-less `DockItem::panel` (no redundant "Workspace"
///     TabPanel title bar) — rebuilt fresh each launch in `install_fixed_chrome`
///     (a persisted panel rehydrates wrapped in a TabPanel, growing the bar
///     back), exactly like the sidebar.
const LAYOUT_VERSION: usize = 4;

const DOCK_AREA_ID: &str = "exp-workspace";

/// Default sidebar width — web parity (the web sidebar column).
const SIDEBAR_WIDTH: Pixels = px(240.);

/// Default (closed) terminal-dock height when first opened.
const TERMINAL_DOCK_HEIGHT: Pixels = px(240.);

/// Debounce for persisting layout changes (`DockEvent::LayoutChanged` fires on
/// every drag tick).
const SAVE_DEBOUNCE: Duration = Duration::from_secs(2);

pub struct Workspace {
    dock_area: Entity<DockArea>,
    /// The JetBrains-style top-right run bar (§7.5, EXP-2d/e): a strip above
    /// the dock carrying the active project's run-config dropdown + play/stop.
    /// It follows this window's navigation and self-hides (renders nothing)
    /// off a project scope; it resolves this window's bottom terminal dock
    /// through `dock_area` for the play→`TabKind::Run` launch (§7.4).
    run_bar: Entity<RunBar>,
    /// The trunk git bar (v4 §4.3) — branch chip, clone/sync progress,
    /// behind/ahead counts, Pull/Push, and the amber conflict chip. Sits left
    /// of the run bar on the same top strip; self-hides off a project scope.
    git_bar: Entity<GitBar>,
    /// The functional Phase-2 login surface — rendered INSTEAD of the dock
    /// whenever the session machine is not `Synced` (§5: a dead token routes
    /// to login, never an empty board).
    login: Entity<LoginView>,
    /// Which per-window layout slot this window persists to (window 0 = the
    /// main window; further windows get the next ordinal at open time).
    ordinal: usize,
    last_saved: Option<DockAreaState>,
    _save_task: Option<Task<()>>,
}

impl Workspace {
    /// The window's `DockArea` — the §7 coding flow resolves this window's
    /// bottom terminal dock through it (`coding_flow::window_terminal_manager`).
    pub(crate) fn dock_area(&self) -> &Entity<DockArea> {
        &self.dock_area
    }

    pub fn new(ordinal: usize, window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        let dock_area =
            cx.new(|cx| DockArea::new(DOCK_AREA_ID, Some(LAYOUT_VERSION), window, cx));
        let login = cx.new(|cx| LoginView::new(window, cx));

        // Per-window navigation state (§4.2): create it before any panel so
        // every `nav_for_window` lookup (sidebar, screens, cold-restored
        // panels) resolves to the same entity; the registry entry dies with
        // the window (release hook below).
        let _ = navigation::nav_for_window(window, cx);
        let window_id = window.window_handle().window_id();

        // ---- Shared-store window accounting (§3.10 multi-window gate) ------
        // Also drives the login-vs-board switch: re-render on session-phase
        // changes.
        let shared = Store::global(cx).state();
        cx.observe(&shared, |_, _, cx| cx.notify()).detach();
        shared.update(cx, |state, cx| {
            state.windows_open += 1;
            cx.notify();
        });
        cx.on_release({
            let shared = shared.clone();
            move |_, cx| {
                navigation::remove_window(window_id, cx);
                crate::repo_resolver::remove_window(window_id, cx);
                shared.update(cx, |state, cx| {
                    state.windows_open = state.windows_open.saturating_sub(1);
                    cx.notify();
                });
                // Non-macOS: the app quits when the last window closes.
                // macOS keeps running (standard platform behavior; the dock
                // icon / File ▸ New Window reopens a workspace).
                #[cfg(not(target_os = "macos"))]
                if shared.read(cx).windows_open == 0 {
                    cx.quit();
                }
            }
        })
        .detach();

        // ---- Layout: restore or build the default -------------------------
        if let Err(err) = Self::load_layout(&dock_area, ordinal, window, cx) {
            // First run / stale version / unreadable file → default layout.
            log_layout(&format!("window {ordinal}: building default layout ({err:#})"));
            Self::reset_default_layout(&dock_area, window, cx);
        }
        Self::install_fixed_chrome(&dock_area, window, cx);

        // ---- Persistence wiring (§3.3) -------------------------------------
        cx.subscribe_in(&dock_area, window, |this, dock_area, event: &DockEvent, window, cx| {
            if let DockEvent::LayoutChanged = event {
                this.queue_save_layout(dock_area.clone(), window, cx);
            }
        })
        .detach();

        cx.on_app_quit({
            let dock_area = dock_area.clone();
            move |_, cx| {
                let state = dock_area.read(cx).dump(cx);
                cx.background_executor().spawn(async move {
                    if let Err(err) = save_layout_state(ordinal, &state) {
                        log_layout(&format!("window {ordinal}: save on quit failed ({err:#})"));
                    }
                })
            }
        })
        .detach();

        // The §7.5 run bar owns a weak handle to this window's dock so its
        // play button can resolve the bottom terminal dock and launch a
        // `TabKind::Run` tab; built after the fixed chrome so the terminal
        // dock already exists.
        let run_bar = cx.new(|cx| RunBar::new(dock_area.downgrade(), window, cx));
        // The v4 §4.3 git bar shares the top strip, left of the run bar.
        let git_bar = cx.new(|cx| GitBar::new(window, cx));

        Self {
            dock_area,
            run_bar,
            git_bar,
            login,
            ordinal,
            last_saved: None,
            _save_task: None,
        }
    }

    /// Restore a persisted `DockAreaState` for this window slot.
    fn load_layout(
        dock_area: &Entity<DockArea>,
        ordinal: usize,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> Result<()> {
        let path = layout_file(ordinal).ok_or_else(|| anyhow!("no data dir"))?;
        let json = std::fs::read_to_string(&path)
            .with_context(|| format!("read {}", path.display()))?;
        let state: DockAreaState = serde_json::from_str(&json).context("parse layout state")?;
        if state.version != Some(LAYOUT_VERSION) {
            bail!("layout version changed");
        }
        dock_area.update(cx, |dock_area, cx| dock_area.load(state, window, cx))
    }

    /// The default layout marker. The center + docks are (re)built fresh in
    /// [`Self::install_fixed_chrome`] on BOTH the fresh and restore paths, so
    /// this only needs to stamp the version onto a first-run dock (§4.2 — the
    /// screens panel is the center, swapped on the per-window navigation).
    fn reset_default_layout(
        dock_area: &Entity<DockArea>,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        dock_area.update(cx, |dock_area, cx| {
            dock_area.set_version(LAYOUT_VERSION, window, cx);
        });
    }

    /// (Re)install the fixed chrome on BOTH the restore and default paths:
    ///
    /// - The **center** is always rebuilt fresh as a chrome-less
    ///   `DockItem::Panel` holding the screens panel — a `DockItem::Tabs` would
    ///   grow the redundant "Workspace" `TabPanel` title bar (+ zoom control),
    ///   and a persisted panel rehydrates wrapped in a `TabPanel` all the same,
    ///   so we never restore the center from disk (same rule as the sidebar).
    ///   `EXP_DEV_BOARD=1` keeps a tab strip so the second (debug-board) tab
    ///   stays reachable.
    /// - The **sidebar** is always rebuilt fresh as a chrome-less
    ///   `DockItem::Panel` — the serialized form would rehydrate wrapped in a
    ///   `TabPanel` (growing a title bar), and the sidebar carries no inner
    ///   state worth restoring; only the persisted dock **width** is kept.
    /// - The **bottom terminal dock** is added if the restored state lacked
    ///   it; its open/closed state and height persist across restarts.
    /// - Collapsibility (EXP-1 #8): left dock NOT collapsible (which also
    ///   forces it open), bottom dock collapsible (the terminal toggle).
    fn install_fixed_chrome(
        dock_area: &Entity<DockArea>,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let saved_width = dock_area
            .read(cx)
            .left_dock()
            .map(|dock| dock.read(cx).size());
        let sidebar: Arc<dyn PanelView> =
            Arc::new(cx.new(|cx| SidebarPanel::new(window, cx)));
        let weak: WeakEntity<DockArea> = dock_area.downgrade();

        // Fresh chrome-less center (see the doc note): a single `DockItem::Panel`
        // renders raw (no title bar); the dev-board escape hatch keeps tabs.
        let screens: Arc<dyn PanelView> = Arc::new(cx.new(|cx| ScreensPanel::new(window, cx)));
        let center = if std::env::var("EXP_DEV_BOARD").as_deref() == Ok("1") {
            let debug: Arc<dyn PanelView> = Arc::new(cx.new(|cx| DebugBoardPanel::new(window, cx)));
            DockItem::tabs(vec![screens, debug], &weak, window, cx)
        } else {
            DockItem::panel(screens)
        };

        dock_area.update(cx, |dock_area, cx| {
            dock_area.set_center(center, window, cx);

            dock_area.set_left_dock(
                DockItem::panel(sidebar),
                Some(saved_width.unwrap_or(SIDEBAR_WIDTH)),
                true,
                window,
                cx,
            );

            if dock_area.bottom_dock().is_none() {
                let terminal: Arc<dyn PanelView> =
                    Arc::new(cx.new(|cx| TerminalDockPanel::new(weak.clone(), window, cx)));
                let bottom = DockItem::tabs(vec![terminal], &weak, window, cx);
                // Collapsed by default — the Dock keeps a 29px toggle strip.
                dock_area.set_bottom_dock(bottom, Some(TERMINAL_DOCK_HEIGHT), false, window, cx);
            }

            dock_area.set_dock_collapsible(
                Edges {
                    bottom: true,
                    ..Default::default()
                },
                window,
                cx,
            );
        });
    }

    /// Debounced persist on `DockEvent::LayoutChanged` (which fires per drag
    /// tick — mirror of the sanctioned dock-example pattern).
    fn queue_save_layout(
        &mut self,
        dock_area: Entity<DockArea>,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let ordinal = self.ordinal;
        self._save_task = Some(cx.spawn_in(window, async move |this, window| {
            window
                .background_executor()
                .timer(SAVE_DEBOUNCE)
                .await;

            _ = this.update_in(window, move |this, _, cx| {
                let state = dock_area.read(cx).dump(cx);
                if this.last_saved.as_ref() == Some(&state) {
                    return;
                }
                match save_layout_state(ordinal, &state) {
                    Ok(()) => this.last_saved = Some(state),
                    Err(err) => {
                        log_layout(&format!("window {ordinal}: save failed ({err:#})"))
                    }
                }
            });
        }));
    }
}

impl Render for Workspace {
    fn render(
        &mut self,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> impl IntoElement {
        // §3.9 gotcha: never mount a zero-size surface — guard layout during
        // rapid resize; skip the tree entirely when the viewport is degenerate.
        let viewport = window.viewport_size();
        if viewport.width < px(1.) || viewport.height < px(1.) {
            return div().into_any_element();
        }

        // The §5 session switch: anything but `Synced` renders the login
        // surface — including `AuthExpired`, the EXP-1 #13(b) dead-token
        // routing (login screen, never an empty board).
        let session = Store::global(cx).session(cx);
        let content: gpui::AnyElement = match session {
            // §7.5: the run bar rides a compact top strip above the dock (it
            // renders nothing off a project scope, so non-project screens keep
            // the full height); the dock fills the rest.
            SessionPhase::Synced { .. } => v_flex()
                .size_full()
                // §4.3: one top strip — git bar (left) + run bar (right). The
                // strip owns the chrome (constant height, border, title-bar bg)
                // so navigation off a project scope never shifts the layout;
                // both children render inner content and self-hide.
                .child(
                    h_flex()
                        .h(px(30.))
                        .flex_shrink_0()
                        .items_center()
                        .border_b_1()
                        .border_color(cx.theme().border)
                        .bg(cx.theme().title_bar)
                        .child(self.git_bar.clone())
                        .child(div().flex_1())
                        .child(self.run_bar.clone()),
                )
                .child(div().flex_1().min_h_0().child(self.dock_area.clone()))
                .into_any_element(),
            _ => self.login.clone().into_any_element(),
        };

        // Root overlay layers (the sanctioned gpui-component pattern — story
        // lib.rs `StoryRoot::render`): the app's root view must compose the
        // sheet/dialog/notification layers or `window.open_dialog` /
        // `push_notification` silently never paint (§3.3 "Root MUST be the
        // first view" is necessary but not sufficient).
        let sheet_layer = Root::render_sheet_layer(window, cx);
        let dialog_layer = Root::render_dialog_layer(window, cx);
        let notification_layer = Root::render_notification_layer(window, cx);

        div()
            .size_full()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .child(content)
            .children(sheet_layer)
            .children(dialog_layer)
            .children(notification_layer)
            .into_any_element()
    }
}

/// Per-window layout state file (§3.3 "Each window persists its
/// DockAreaState"). macOS: `~/Library/Application Support/Exponential/…`;
/// Linux: `~/.local/share/exponential/…`.
fn layout_file(ordinal: usize) -> Option<PathBuf> {
    let dir = dirs::data_local_dir()?
        .join(if cfg!(target_os = "macos") {
            "Exponential"
        } else {
            "exponential"
        })
        .join("layouts");
    Some(dir.join(format!("window-{ordinal}.json")))
}

fn save_layout_state(ordinal: usize, state: &DockAreaState) -> Result<()> {
    let path = layout_file(ordinal).ok_or_else(|| anyhow!("no data dir"))?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(state)?;
    std::fs::write(&path, json)?;
    Ok(())
}

/// Layout persistence is best-effort — failures must never take the shell
/// down, they just cost the user a restored layout.
fn log_layout(message: &str) {
    eprintln!("[exp-desktop] layout: {message}");
}
