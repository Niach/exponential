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
    ActiveTheme as _,
};
use sync::Store;

use crate::{sidebar::SidebarPanel, terminal_dock::TerminalDockPanel};

/// Bump when the default layout shape changes so stale persisted layouts are
/// discarded and rebuilt (mirrors the gpui-component dock example's version
/// check; we silently reset instead of prompting).
const LAYOUT_VERSION: usize = 1;

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
    /// Which per-window layout slot this window persists to (window 0 = the
    /// main window; further windows get the next ordinal at open time).
    ordinal: usize,
    last_saved: Option<DockAreaState>,
    _save_task: Option<Task<()>>,
}

impl Workspace {
    pub fn new(ordinal: usize, window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        let dock_area =
            cx.new(|cx| DockArea::new(DOCK_AREA_ID, Some(LAYOUT_VERSION), window, cx));

        // ---- Shared-store window accounting (§3.10 multi-window gate) ------
        let shared = Store::global(cx).state();
        shared.update(cx, |state, cx| {
            state.windows_open += 1;
            cx.notify();
        });
        cx.on_release({
            let shared = shared.clone();
            move |_, cx| {
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

        Self {
            dock_area,
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

    /// The default layout: empty center tabs (issue tabs land in Phase 3).
    fn reset_default_layout(
        dock_area: &Entity<DockArea>,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let weak = dock_area.downgrade();
        let center = DockItem::tabs(vec![], &weak, window, cx);
        dock_area.update(cx, |dock_area, cx| {
            dock_area.set_version(LAYOUT_VERSION, window, cx);
            dock_area.set_center(center, window, cx);
        });
    }

    /// (Re)install the fixed chrome on BOTH the restore and default paths:
    ///
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

        dock_area.update(cx, |dock_area, cx| {
            dock_area.set_left_dock(
                DockItem::panel(sidebar),
                Some(saved_width.unwrap_or(SIDEBAR_WIDTH)),
                true,
                window,
                cx,
            );

            if dock_area.bottom_dock().is_none() {
                let terminal: Arc<dyn PanelView> =
                    Arc::new(cx.new(|cx| TerminalDockPanel::new(window, cx)));
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

        div()
            .size_full()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .child(self.dock_area.clone())
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
