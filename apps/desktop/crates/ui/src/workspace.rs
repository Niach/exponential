//! The per-window Workspace — a gpui-component `DockArea` (masterplan-v3
//! §3.3 / §3.6 / §3.10).
//!
//! Shell layout (JetBrains new UI): a full-width **top bar** (project picker
//! + breadcrumbs + run/git widgets) above everything, the 44px **icon rail**
//! left of the dock area, and the dock area filling the rest. The dock
//! area's **center** is [`CenterPanel`] — a resizable split of the tool
//! window column (sidebar) and the screens panel — and its **bottom dock**
//! is the terminal dock. Because the sidebar lives inside the center (not a
//! left dock), the bottom terminal dock spans the full width right of the
//! rail, running beneath the sidebar.
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
    div, px, AnyElement, App, AppContext as _, ClickEvent, Edges, Entity, FocusHandle, Focusable,
    IntoElement, ParentElement, Pixels, Render, SharedString, Styled, Task, WeakEntity, Window,
};
use gpui_component::{
    button::{Button, ButtonVariants as _},
    dock::{DockArea, DockAreaState, DockEvent, DockItem, Panel, PanelControl, PanelEvent, PanelView},
    h_flex,
    resizable::{h_resizable, resizable_panel},
    v_flex, ActiveTheme as _, Icon, IconName, Root, Sizable as _,
};
use sync::{SessionPhase, Store};

use crate::{
    debug_board::DebugBoardPanel, login::LoginView, navigation, screens::ScreensPanel,
    sidebar::{RailView, SidebarPanel}, terminal_dock::TerminalDockPanel, top_bar::TopBar,
    update::{self, UpdatePhase, UpdateState},
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
/// v5: the sidebar grew the JetBrains-style tool-window rail — persisted
///     240px left docks would leave a cramped tool window next to the rail.
/// v6: the left dock is GONE — the sidebar moved inside the center panel's
///     resizable split so the bottom terminal dock spans beneath it; a
///     persisted left dock would render a second, dead sidebar.
const LAYOUT_VERSION: usize = 6;

const DOCK_AREA_ID: &str = "exp-workspace";

/// Default tool-window width inside the center split — web parity. The 44px
/// icon rail renders OUTSIDE the dock area (`Workspace::render`).
const SIDEBAR_WIDTH: Pixels = px(crate::sidebar::DEFAULT_DOCK_WIDTH);

/// Default (closed) terminal-dock height when first opened.
const TERMINAL_DOCK_HEIGHT: Pixels = px(240.);

/// Debounce for persisting layout changes (`DockEvent::LayoutChanged` fires on
/// every drag tick).
const SAVE_DEBOUNCE: Duration = Duration::from_secs(2);

pub struct Workspace {
    dock_area: Entity<DockArea>,
    /// The JetBrains-style tool-window rail — rendered LEFT of the dock area
    /// (so the bottom terminal dock spans everything right of it and lines
    /// up with the rail's terminal toggle).
    rail: Entity<RailView>,
    /// The full-width header above rail + dock area: project picker,
    /// breadcrumbs, run widget, git cluster.
    top_bar: Entity<TopBar>,
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

        // Re-render when the launch-time update check flips the "update
        // available" flag (§11.2) so the dismissible banner appears without a
        // navigation.
        let update_state = UpdateState::global(cx);
        cx.observe(&update_state, |_, _, cx| cx.notify()).detach();
        shared.update(cx, |state, cx| {
            state.windows_open += 1;
            cx.notify();
        });
        cx.on_release({
            let shared = shared.clone();
            move |_, cx| {
                navigation::remove_window(window_id, cx);
                crate::repo_resolver::remove_window(window_id, cx);
                crate::sidebar::remove_window(window_id, cx);
                shared.update(cx, |state, cx| {
                    state.windows_open = state.windows_open.saturating_sub(1);
                    cx.notify();
                });
                // EXP-65: close this window's undocked terminal windows (their
                // manager died with the dock panel), and ALL undocked windows
                // once no workspace window remains — nothing left to reattach
                // to, and non-macOS is about to quit.
                crate::undock::on_workspace_released(window_id, cx);
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

        // The rail and top bar live OUTSIDE the dock area; the top bar's run
        // widget drives launches through this weak handle. Built after the
        // fixed chrome so the dock already exists.
        let rail = cx.new(|cx| RailView::new(window, cx));
        let top_bar = cx.new(|cx| TopBar::new(dock_area.downgrade(), window, cx));

        Self {
            dock_area,
            rail,
            top_bar,
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
    ///   `DockItem::Panel` holding [`CenterPanel`] (the sidebar/screens
    ///   resizable split) — a `DockItem::Tabs` would grow the redundant
    ///   "Workspace" `TabPanel` title bar (+ zoom control), and a persisted
    ///   panel rehydrates wrapped in a `TabPanel` all the same, so we never
    ///   restore the center from disk. `EXP_DEV_BOARD=1` keeps a tab strip so
    ///   the second (debug-board) tab stays reachable.
    /// - There is deliberately **no left dock** (v6): the sidebar lives
    ///   inside the center split so the bottom terminal dock spans beneath it.
    /// - The **bottom terminal dock** is added if the restored state lacked
    ///   it; its open/closed state and height persist across restarts.
    /// - Collapsibility: bottom dock collapsible (the terminal toggle).
    fn install_fixed_chrome(
        dock_area: &Entity<DockArea>,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let weak: WeakEntity<DockArea> = dock_area.downgrade();

        // Fresh chrome-less center (see the doc note): a single `DockItem::Panel`
        // renders raw (no title bar); the dev-board escape hatch keeps tabs.
        let center_panel: Arc<dyn PanelView> =
            Arc::new(cx.new(|cx| CenterPanel::new(window, cx)));
        let center = if std::env::var("EXP_DEV_BOARD").as_deref() == Ok("1") {
            let debug: Arc<dyn PanelView> = Arc::new(cx.new(|cx| DebugBoardPanel::new(window, cx)));
            DockItem::tabs(vec![center_panel, debug], &weak, window, cx)
        } else {
            DockItem::panel(center_panel)
        };

        dock_area.update(cx, |dock_area, cx| {
            dock_area.set_center(center, window, cx);

            match dock_area.bottom_dock().cloned() {
                None => {
                    let terminal: Arc<dyn PanelView> =
                        Arc::new(cx.new(|cx| TerminalDockPanel::new(weak.clone(), window, cx)));
                    // Chrome-less like the center (§8.8c): a single `DockItem::Panel`
                    // renders raw with no `TabPanel` wrapper, so there is no zoom
                    // control over the terminal — the panel's own tab strip is the
                    // only chrome, and the Dock toggle handles collapse.
                    let bottom = DockItem::panel(terminal);
                    // Collapsed by default — the Dock keeps a 29px toggle strip.
                    dock_area.set_bottom_dock(
                        bottom,
                        Some(TERMINAL_DOCK_HEIGHT),
                        false,
                        window,
                        cx,
                    );
                }
                Some(dock) => {
                    // A RESTORED bottom dock rehydrates `PanelInfo::Panel`
                    // wrapped in a `TabPanel` (title row + zoom/menu chrome —
                    // the same "growing the bar back" problem the center
                    // solves by rebuilding fresh). Re-wrap the SAME restored
                    // panel chrome-less; the dock's persisted open state and
                    // height are untouched, and the restored shell tabs live
                    // in the panel entity we keep.
                    let restored = terminal_panel_view(dock.read(cx).panel(), cx)
                        .unwrap_or_else(|| {
                            Arc::new(
                                cx.new(|cx| TerminalDockPanel::new(weak.clone(), window, cx)),
                            )
                        });
                    dock.update(cx, |dock, cx| {
                        dock.set_panel(DockItem::panel(restored), window, cx);
                    });
                }
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
        // surface — including `AuthExpired`, the dead-token
        // routing (login screen, never an empty board).
        let session = Store::global(cx).session(cx);
        // Synced shell = full-width top bar above everything, then rail +
        // dock area — the bottom terminal dock spans the full width right of
        // the rail (beneath the sidebar, which lives inside the center split).
        let content: gpui::AnyElement = match session {
            // `.h_full()` on the dock wrapper is load-bearing: without a
            // definite height the dock area collapses (same flex-child rule
            // as the source-control diff pane).
            SessionPhase::Synced { .. } => v_flex()
                .size_full()
                // The explicit w_full wrapper pins the bar to the window
                // width — an entity child alone can end up content-sized
                // (the same flex-child rule as the dock wrapper below).
                .child(
                    div()
                        .w_full()
                        .flex_shrink_0()
                        .child(self.top_bar.clone()),
                )
                .child(
                    h_flex()
                        .w_full()
                        .flex_1()
                        .min_h_0()
                        .child(self.rail.clone())
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .h_full()
                                .child(self.dock_area.clone()),
                        ),
                )
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

        // The update banner (§11.2) rides above the whole shell (login OR
        // board) as a fixed-height strip; the content fills the rest. A column
        // wrapper keeps the `size_full` content from overlapping the banner.
        let body = v_flex()
            .size_full()
            .children(self.render_update_banner(cx))
            .child(div().flex_1().min_h_0().child(content));

        div()
            .size_full()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .child(body)
            .children(sheet_layer)
            .children(dialog_layer)
            .children(notification_layer)
            .into_any_element()
    }
}

impl Workspace {
    /// The §11.2 update banner: a thin strip above everything else, shown once
    /// the launch-time check found a newer `desktop-v*` release. Self-update
    /// capable installs get an in-app "Update" pipeline (download progress →
    /// "Restart to update"); everything else gets the original "Download"
    /// browser link. The × dismisses it for the session (a fresh launch that
    /// still sees a newer release shows it again); it is hidden while the
    /// pipeline is actively downloading/installing.
    fn render_update_banner(&self, cx: &mut gpui::Context<Self>) -> Option<AnyElement> {
        let model = UpdateState::global_ref(cx)?;
        let (info, phase) = {
            let state = model.read(cx);
            let (info, phase) = state.banner()?;
            (info.clone(), phase.clone())
        };
        let version = info.version.clone();
        let url = info.url.clone();

        let label: SharedString = match &phase {
            UpdatePhase::Available => {
                format!("Update available — Exponential {version} is out.").into()
            }
            UpdatePhase::Downloading { received, total } => {
                format!("Downloading Exponential {version}… {}", format_progress(*received, *total))
                    .into()
            }
            UpdatePhase::Installing => format!("Installing Exponential {version}…").into(),
            UpdatePhase::ReadyToRestart { .. } => {
                format!("Exponential {version} is ready — restart to finish updating.").into()
            }
            UpdatePhase::Failed { message } => {
                format!("Update to Exponential {version} failed: {message}").into()
            }
        };

        let download_button = |id: &'static str, label: &'static str, url: String| {
            Button::new(id).small().label(label).on_click(move |_: &ClickEvent, _, _| {
                if let Err(err) = api::opener::open_in_browser(&url) {
                    log::warn!("[ui] update: open release page failed: {err}");
                }
            })
        };

        let mut banner = h_flex()
            .h(px(34.))
            .w_full()
            .flex_shrink_0()
            .items_center()
            .gap_2()
            .px_3()
            .border_b_1()
            .border_color(cx.theme().border)
            .bg(cx.theme().info.opacity(0.14))
            .child(Icon::new(IconName::Info).size_4().text_color(cx.theme().info))
            .child(div().flex_1().text_sm().child(label));

        match &phase {
            UpdatePhase::Available if info.plan.is_some() => {
                banner = banner.child(
                    Button::new("update-install").primary().small().label("Update").on_click(
                        move |_: &ClickEvent, _, cx| {
                            update::start_update(cx);
                        },
                    ),
                );
            }
            UpdatePhase::Available => {
                banner =
                    banner.child(download_button("update-download", "Download", url).primary());
            }
            UpdatePhase::Downloading { .. } | UpdatePhase::Installing => {}
            UpdatePhase::ReadyToRestart { restart_path } => {
                let restart_path = restart_path.clone();
                banner = banner.child(
                    Button::new("update-restart")
                        .primary()
                        .small()
                        .label("Restart to update")
                        .on_click(move |_: &ClickEvent, _, cx| {
                            if let Some(path) = restart_path.clone() {
                                cx.set_restart_path(path);
                            }
                            cx.restart();
                        }),
                );
            }
            UpdatePhase::Failed { .. } => {
                if info.plan.is_some() {
                    banner = banner.child(
                        Button::new("update-retry").primary().small().label("Retry").on_click(
                            move |_: &ClickEvent, _, cx| {
                                update::start_update(cx);
                            },
                        ),
                    );
                }
                banner = banner.child(download_button("update-download", "Download", url));
            }
        }

        // Dismissible except mid-pipeline: a hidden banner with a running
        // download would leave the user no signal of what's happening.
        if !matches!(phase, UpdatePhase::Downloading { .. } | UpdatePhase::Installing) {
            banner = banner.child(
                Button::new("update-dismiss")
                    .ghost()
                    .small()
                    .icon(IconName::Close)
                    .on_click(cx.listener(|_, _: &ClickEvent, _, cx| {
                        if let Some(model) = UpdateState::global_ref(cx) {
                            model.update(cx, |state, cx| {
                                state.dismiss();
                                cx.notify();
                            });
                        }
                    })),
            );
        }

        Some(banner.into_any_element())
    }
}

/// Find the restored [`TerminalDockPanel`] view inside a rehydrated bottom
/// dock item (the registry re-created it with its persisted shell tabs —
/// keep THAT entity, only strip the `TabPanel` wrapper around it).
fn terminal_panel_view(item: &DockItem, cx: &App) -> Option<Arc<dyn PanelView>> {
    let is_terminal =
        |view: &Arc<dyn PanelView>| view.panel_name(cx) == crate::terminal_dock::PANEL_NAME;
    match item {
        DockItem::Panel { view, .. } => is_terminal(view).then(|| view.clone()),
        DockItem::Tabs { items, .. } => items.iter().find(|view| is_terminal(view)).cloned(),
        DockItem::Split { items, .. } => {
            items.iter().find_map(|item| terminal_panel_view(item, cx))
        }
        _ => None,
    }
}

/// Human progress: percent when the size is known, transferred MB otherwise.
fn format_progress(received: u64, total: Option<u64>) -> String {
    match total {
        Some(total) if total > 0 => format!("{}%", received * 100 / total),
        _ => format!("{:.1} MB", received as f64 / (1024.0 * 1024.0)),
    }
}

// ---------------------------------------------------------------------------
// CenterPanel — sidebar + screens as a resizable split
// ---------------------------------------------------------------------------

/// The dock area's center: the tool-window column (sidebar) and the screens
/// panel in a draggable horizontal split. Lives INSIDE the dock area so the
/// bottom terminal dock spans beneath both. Implements [`Panel`] because
/// everything docked must (§3.3), but renders chrome-less via
/// `DockItem::Panel`; it is rebuilt fresh each launch, never restored.
pub struct CenterPanel {
    focus_handle: FocusHandle,
    sidebar: Entity<SidebarPanel>,
    screens: Entity<ScreensPanel>,
}

/// Stable serialization name (§3.3) — present in dumps even though the center
/// is always rebuilt fresh.
pub(crate) const CENTER_PANEL_NAME: &str = "Center";

impl CenterPanel {
    pub fn new(window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        Self {
            focus_handle: cx.focus_handle(),
            sidebar: cx.new(|cx| SidebarPanel::new(window, cx)),
            screens: cx.new(|cx| ScreensPanel::new(window, cx)),
        }
    }
}

impl Panel for CenterPanel {
    fn panel_name(&self) -> &'static str {
        CENTER_PANEL_NAME
    }

    fn title(&mut self, _window: &mut Window, _cx: &mut gpui::Context<Self>) -> impl IntoElement {
        "Workspace"
    }

    /// The split IS the center — closing it would leave an empty center
    /// baked into the persisted layout.
    fn closable(&self, _cx: &App) -> bool {
        false
    }

    fn zoomable(&self, _cx: &App) -> Option<PanelControl> {
        None
    }
}

impl gpui::EventEmitter<PanelEvent> for CenterPanel {}

impl Focusable for CenterPanel {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for CenterPanel {
    fn render(&mut self, _window: &mut Window, _cx: &mut gpui::Context<Self>) -> impl IntoElement {
        div().size_full().child(
            h_resizable("center-split")
                .child(
                    resizable_panel()
                        .size(SIDEBAR_WIDTH)
                        .size_range(px(180.)..px(520.))
                        .child(self.sidebar.clone()),
                )
                .child(resizable_panel().child(self.screens.clone())),
        )
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
