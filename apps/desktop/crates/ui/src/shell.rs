//! The per-window Shell — a gpui-component `DockArea` (masterplan-v3
//! §3.3 / §3.6 / §3.10).
//!
//! Shell layout (EXP-269 glass): the in-app **titlebar** (34px drag strip
//! with embedded window controls, `crate::app_title_bar`) above everything,
//! the 44px **icon rail** left of the dock area, and the dock area filling
//! the rest — all over the page gradient. The dock area's **center** is
//! [`CenterPanel`] — a resizable split of the tool window column (sidebar)
//! and the screens panel — and its **bottom dock** is the terminal dock.
//! Because the sidebar lives inside the center (not a left dock), the bottom
//! terminal dock spans the full width right of the rail, running beneath the
//! sidebar. (The old EXP-253 top bar is gone — boards live in the rail.)
//!
//! Every window gets its own `Root → Shell → DockArea`, but they all read
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
    div, prelude::FluentBuilder as _, px, AnyElement, App, AppContext as _, ClickEvent, Edges,
    Entity, FocusHandle, Focusable, FontWeight, IntoElement, ParentElement, Pixels, Render,
    SharedString, Size, Styled, Task, WeakEntity, Window,
};
use gpui_component::{
    button::{Button, ButtonVariants as _},
    dock::{DockArea, DockAreaState, DockEvent, DockItem, Panel, PanelControl, PanelEvent, PanelView},
    h_flex,
    resizable::{h_resizable, resizable_panel, ResizableState},
    v_flex, ActiveTheme as _, Icon, Root, Sizable as _,
};
use sync::{SessionPhase, Store};

use crate::{
    debug_board::DebugBoardPanel, icons::ExpIcon, login::LoginView, navigation,
    navigation::{nav_for_window, resolved_screen, Navigation, Screen},
    screens::ScreensPanel,
    settings::{SettingsNavPanel, SETTINGS_NAV_WIDTH},
    sidebar::{RailView, SidebarPanel},
    terminal_dock::TerminalDockPanel,
    update::{self, UpdatePhase, UpdateState},
    window_size::SizeFrame,
};
use crate::icons::registry;

/// Bump when the default layout shape changes so stale persisted layouts are
/// discarded and rebuilt (mirrors the gpui-component dock example's version
/// check; we silently reset instead of prompting).
/// v2: the Phase-2 debug board landed in the center tabs.
/// v3: the Phase-3 screens panel replaced the debug board as the default
///     center (debug board only behind `EXP_DEV_BOARD=1`).
/// v4: the center is a chrome-less `DockItem::panel` (no redundant "Shell"
///     TabPanel title bar) — rebuilt fresh each launch in `install_fixed_chrome`
///     (a persisted panel rehydrates wrapped in a TabPanel, growing the bar
///     back), exactly like the sidebar.
/// v5: the sidebar grew the JetBrains-style tool-window rail — persisted
///     240px left docks would leave a cramped tool window next to the rail.
/// v6: the left dock is GONE — the sidebar moved inside the center panel's
///     resizable split so the bottom terminal dock spans beneath it; a
///     persisted left dock would render a second, dead sidebar.
/// v7: the top bar is GONE (EXP-253) — boards moved into the rail as icon
///     entries and the git bar became the headless trunk-sync engine; the
///     freed header height changes every persisted pane size.
/// v8: the in-app titlebar (EXP-269) reserves 34px above the dock area on
///     every platform — persisted pane sizes shift again.
/// v9: EXP-723's cutout panel — the dock area is inset by a 10px margin on
///     every side (6px under the decoration band) and the terminal dock grew
///     its own 28px header row, so every persisted pane size shifts once more.
const LAYOUT_VERSION: usize = 9;

const DOCK_AREA_ID: &str = "exp-workspace";

/// Default tool-window width inside the center split — web parity. The 44px
/// icon rail renders OUTSIDE the dock area (`Shell::render`).
const SIDEBAR_WIDTH: Pixels = px(crate::sidebar::DEFAULT_DOCK_WIDTH);

/// Default (closed) terminal-dock height when first opened.
const TERMINAL_DOCK_HEIGHT: Pixels = px(240.);

/// EXP-723 cutout: the gap between the working panel and the window edges —
/// the same 10px the web shell uses (`app-shell.ts` `md:m-[10px]`).
const PANEL_MARGIN: f32 = 10.;

/// The panel's TOP gap under the 34px decoration band. Tighter than
/// [`PANEL_MARGIN`] because the band already supplies breathing room above
/// it; the full margin there reads as a hole. Windows without client chrome
/// (the Linux server-decoration fallback) have no band and take
/// [`PANEL_MARGIN`] on all four sides.
const PANEL_MARGIN_TOP: f32 = 6.;

/// EXP-456: whether this window is in the tab-less Settings mode — the left
/// column shows the settings nav instead of the rail.
pub(crate) fn window_in_settings(window: &Window, cx: &mut App) -> bool {
    let nav = nav_for_window(window, cx);
    matches!(resolved_screen(&nav, cx), Some(Screen::Settings))
}

/// EXP-456: the left column's TARGET width — `app_title_bar`'s strip budget
/// reads this instead of raw `RAIL_W` (during the swap animation the target
/// is the width the strip is about to have; a ~200ms transient under-budget
/// is invisible). EXP-723: the rail has ONE width now, so this only ever
/// distinguishes the settings nav from the rail.
pub(crate) fn left_column_target_width(window: &mut Window, cx: &mut App) -> f32 {
    if window_in_settings(window, cx) {
        SETTINGS_NAV_WIDTH
    } else {
        crate::sidebar::RAIL_W
    }
}

/// EXP-456: duration of the rail ⇄ settings-nav swap and (EXP-523) of the
/// rail's own expand/collapse. The shared `standard` motion token — every
/// client's default duration — rather than the 200ms it was hand-set to when
/// this was the app's only animation.
const LEFT_COL_ANIM_DURATION: Duration = theme::motion::STANDARD;

/// EXP-456: pure state machine for the rail ⇄ settings-nav swap — the
/// upstream `Sidebar` recipe (`gpui_component::sidebar`'s animation state)
/// as plain `Shell` fields: from/target width, whether both children stay
/// mounted, and an epoch guarding the unmount timer against retargets.
#[derive(Clone, Copy, Debug, PartialEq)]
struct LeftColumnAnim {
    from: f32,
    to: f32,
    /// The rail's width inside the swap strip, captured at retarget time —
    /// the slide distance (EXP-456 expands a collapsed rail transiently, so
    /// this is usually `RAIL_EXPANDED_W`).
    rail_w: f32,
    /// Target occupant: true = settings nav.
    settings: bool,
    /// Both children stay mounted while the swap transition runs.
    swapping: bool,
    /// EXP-523: a same-occupant WIDTH animation is in flight — the rail
    /// expanding or collapsing. Distinct from `swapping`, which mounts BOTH
    /// children; here the single child just rides a morphing clip.
    width_only: bool,
    /// Guards the unmount timer against retargets (upstream `hide_request`).
    epoch: u64,
}

impl LeftColumnAnim {
    fn new(settings: bool, width: f32, rail_w: f32) -> Self {
        Self {
            from: width,
            to: width,
            rail_w,
            settings,
            swapping: false,
            width_only: false,
            epoch: 0,
        }
    }

    /// Sync with the rendered state. Returns `Some(epoch)` when a swap
    /// STARTED and the caller must spawn the settle timer.
    fn retarget(&mut self, settings: bool, target: f32, rail_w: f32) -> Option<u64> {
        if self.settings == settings {
            // Same occupant: the rail expanding or collapsing (or no change).
            // EXP-523 animates it — the child keeps its TARGET width and the
            // column's `overflow_hidden` clip morphs around it, so the labels
            // are revealed by a wipe and nothing inside the rail re-layouts
            // mid-flight. A width change arriving while a settings SWAP is in
            // flight is still absorbed: retargeting there would restart the
            // swap from the wrong geometry.
            if self.swapping {
                self.to = target;
                return None;
            }
            if (target - self.to).abs() <= 0.5 {
                // No actual change (this runs on every render).
                self.from = target;
                self.to = target;
                self.rail_w = rail_w;
                return None;
            }
            self.from = if self.width_only { self.to } else { self.from };
            self.to = target;
            self.rail_w = rail_w;
            self.width_only = true;
            self.epoch += 1;
            return Some(self.epoch);
        }
        // Mid-flight reversal jumps from the previous TARGET (upstream has
        // the same limitation) — acceptable over 200ms.
        self.from = self.to;
        self.to = target;
        self.rail_w = rail_w;
        self.settings = settings;
        self.swapping = true;
        self.width_only = false;
        self.epoch += 1;
        Some(self.epoch)
    }

    /// Timer callback. True = the swap this epoch belongs to just settled.
    fn finish(&mut self, epoch: u64) -> bool {
        if (self.swapping || self.width_only) && self.epoch == epoch {
            self.swapping = false;
            self.width_only = false;
            self.from = self.to;
            true
        } else {
            false
        }
    }
}

/// EXP-456: animation id that encodes `from → to`, so a retargeted transition
/// gets a NEW id and restarts cleanly (upstream `sidebar_animation_id`).
fn left_anim_id(name: &'static str, from: f32, to: f32) -> gpui::ElementId {
    gpui::ElementId::NamedInteger(
        name.into(),
        ((from.to_bits() as u64) << 32) | to.to_bits() as u64,
    )
}

/// Debounce for persisting layout changes (`DockEvent::LayoutChanged` fires on
/// every drag tick).
const SAVE_DEBOUNCE: Duration = Duration::from_secs(2);

pub struct Shell {
    dock_area: Entity<DockArea>,
    /// The in-app titlebar (EXP-269) — the first row of the DOCK shell;
    /// hidden under the Linux server-decoration fallback (`client_chrome`).
    /// EXP-364: the full-window surfaces (login, wizard, update gate) mount
    /// `app_title_bar::chrome_only_title_bar()` instead — same strip, no tab
    /// row, since there is no dock behind them.
    title_bar: Entity<crate::app_title_bar::AppTitleBar>,
    /// The JetBrains-style tool-window rail — rendered LEFT of the dock area
    /// (so the bottom terminal dock spans everything right of it and lines
    /// up with the rail's terminal toggle).
    rail: Entity<RailView>,
    /// EXP-456: the settings navigation — it takes the RAIL's slot (the
    /// window's leftmost column) while `Screen::Settings` is up, sliding in
    /// as the rail slides out; the settings detail fills everything right of
    /// it (`CenterPanel` renders just the screens then).
    settings_nav: Entity<SettingsNavPanel>,
    /// EXP-456: the rail ⇄ settings-nav swap transition state.
    left_anim: LeftColumnAnim,
    _left_anim_task: Option<Task<()>>,
    /// The functional Phase-2 login surface — rendered INSTEAD of the dock
    /// whenever the session machine is not `Synced` (§5: a dead token routes
    /// to login, never an empty board).
    login: Entity<LoginView>,
    /// The EXP-367 first-run wizard — rendered INSTEAD of the dock while a
    /// step applies (team → board → tools). Long-lived like `login`; inert
    /// whenever `is_active` says no.
    onboarding: Entity<crate::onboarding::OnboardingView>,
    /// Which per-window layout slot this window persists to (window 0 = the
    /// main window; further windows get the next ordinal at open time).
    ordinal: usize,
    last_saved: Option<DockAreaState>,
    _save_task: Option<Task<()>>,
    /// EXP-210: the MAIN window's last observed size, pending persist —
    /// debounced to disk on resize, flushed on quit; the next launch opens
    /// at it (`window_size::load_last_size`). Always `None` on ordinal > 0.
    pending_window_size: Option<Size<Pixels>>,
    _size_save_task: Option<Task<()>>,
    /// EXP-263: this window's min-size clamp budget (see
    /// [`crate::window_size::MinSizeClamp`]).
    min_size_clamp: crate::window_size::MinSizeClamp,
    /// EXP-276/EXP-278: the launch ↔ persist round trip for the main
    /// window's size (see [`crate::window_size::WindowSizeTracker`]). Only
    /// ordinal 0 observes bounds, so it is inert on further windows.
    size_tracker: crate::window_size::WindowSizeTracker,
}

impl Shell {
    /// The window's `DockArea` — the §7 coding flow resolves this window's
    /// bottom terminal dock through it (`coding_flow::window_terminal_manager`).
    pub(crate) fn dock_area(&self) -> &Entity<DockArea> {
        &self.dock_area
    }

    pub fn new(ordinal: usize, window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        let dock_area =
            cx.new(|cx| DockArea::new(DOCK_AREA_ID, Some(LAYOUT_VERSION), window, cx));
        let login = cx.new(|cx| LoginView::new(window, cx));
        // EXP-367: the first-run wizard drives the Synced-arm switch below;
        // its own observers (collections, coding hub) notify it, we just
        // re-render when it does.
        let onboarding = cx.new(|cx| crate::onboarding::OnboardingView::new(window, cx));
        cx.observe(&onboarding, |_, _, cx| cx.notify()).detach();

        // Per-window navigation state (§4.2): create it before any panel so
        // every `nav_for_window` lookup (sidebar, screens, cold-restored
        // panels) resolves to the same entity; the registry entry dies with
        // the window (release hook below).
        let nav = navigation::nav_for_window(window, cx);
        // EXP-456: the left column swaps on the active screen (rail ⇄
        // settings nav) — the Shell must re-render when navigation moves.
        cx.observe(&nav, |_, _, cx| cx.notify()).detach();
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
                crate::screens::remove_window(window_id, cx);
                shared.update(cx, |state, cx| {
                    state.windows_open = state.windows_open.saturating_sub(1);
                    cx.notify();
                });
                // EXP-65: close this window's undocked terminal windows (their
                // manager died with the dock panel), and ALL undocked windows
                // once no shell window remains — nothing left to reattach
                // to, and non-macOS is about to quit.
                crate::undock::on_shell_released(window_id, cx);
                // EXP-287: same for the dialogs this window opened. A dialog
                // is an independent `WindowKind::Normal` window now — its own
                // taskbar button, no auto-dismissal — so nothing else would
                // take it down, and an orphan would linger pointing at a dead
                // opener (its `close_then` result path included).
                crate::native_dialog::on_opener_released(window_id, cx);
                // Non-macOS: the app quits when the last window closes.
                // macOS keeps running (standard platform behavior; the dock
                // icon / File ▸ New Window reopens a shell).
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
            move |this: &mut Self, cx| {
                let state = dock_area.read(cx).dump(cx);
                // EXP-210: flush a resize the debounce hasn't landed yet.
                let pending_size = this.pending_window_size.take();
                cx.background_executor().spawn(async move {
                    if let Err(err) = save_layout_state(ordinal, &state) {
                        log_layout(&format!("window {ordinal}: save on quit failed ({err:#})"));
                    }
                    if let Some(last) = pending_size {
                        if let Err(err) = crate::window_size::save_last_size(last) {
                            log_layout(&format!("window size save on quit failed ({err:#})"));
                        }
                    }
                })
            }
        })
        .detach();

        // ---- EXP-263: enforce the minimum window size ourselves ------------
        // macOS enforces `window_min_size` natively; on Linux the option is
        // only a hint the WM/compositor may ignore, so every shell window
        // clamps itself back to the floor (no-op where the OS enforces it).
        // A WM that refuses the request (tiling WMs answer with a synthetic
        // ConfigureNotify at the unchanged size) wins after a few tries
        // instead of spinning this observer at frame rate.
        cx.observe_window_bounds(window, |this, window, cx| {
            crate::window_size::enforce_min_size(&mut this.min_size_clamp, window, cx);
        })
        .detach();

        // ---- EXP-210: remember the MAIN window's last used size ------------
        // `load_last_size` feeds the next launch's `open_shell_window`. Local
        // file only — never synced.
        //
        // EXP-292: what survives a maximized session is the last WINDOWED
        // frame, because the tracker ignores every frame observed while
        // maximized/fullscreen. `window_bounds()` is NOT the restore size
        // there (this comment used to claim it was): X11 reports the current
        // maximized surface, and Wayland the pre-maximize OUTER bounds while
        // `window_paddings` reads zero (maximizing tiles every edge).
        //
        // EXP-269/EXP-276/EXP-278: what we persist is the VISIBLE frame size
        // (the outer surface minus the Linux CSD shadow), but the two Linux
        // backends disagree about what the number we hand back to
        // `WindowOptions.window_bounds` means — Wayland's compositor echo
        // re-adds the inset, X11's `ConfigureNotify` does not. So the decision
        // lives in `WindowSizeTracker`, which suppresses launch echoes under
        // EITHER convention and asks for one compensating resize when the
        // window opened 24px short. See its docs for the full mechanism.
        if ordinal == 0 {
            cx.observe_window_bounds(window, |this, window, cx| {
                let outer = window.window_bounds().get_bounds().size;
                let edges = gpui_component::window_paddings(window);
                let pad = Size {
                    width: edges.left + edges.right,
                    height: edges.top + edges.bottom,
                };
                let wm_sized = window.is_fullscreen() || window.is_maximized();
                match this.size_tracker.observe(outer, pad, wm_sized) {
                    SizeFrame::Ignore => {}
                    SizeFrame::Persist(last) => {
                        if this.pending_window_size != Some(last) {
                            this.pending_window_size = Some(last);
                            this.queue_save_window_size(cx);
                        }
                    }
                    SizeFrame::ResizeTo(target) => {
                        // Deferred for the same reason as `enforce_min_size`:
                        // resizing from inside a bounds observer re-enters the
                        // platform resize path mid-dispatch.
                        window.defer(cx, move |window, _cx| window.resize(target));
                    }
                }
            })
            .detach();
        }

        // EXP-533: bringing a window forward is exactly when stale data is
        // most visible — a lid closed for hours leaves every shape thread
        // parked on a dead connection. One debounced restart, only when the
        // last successful poll is genuinely old (`Store::kick_if_stale`);
        // mirrors the trunk-sync engine's own activation trigger.
        cx.observe_window_activation(window, |_, window, cx| {
            if window.is_window_active() {
                let store = Store::global(cx).clone();
                store.kick_if_stale(cx);
            }
        })
        .detach();

        // The rail lives OUTSIDE the dock area. Built after the fixed chrome
        // so the dock already exists.
        let rail = cx.new(|cx| RailView::new(window, cx));
        let title_bar = cx.new(|_| crate::app_title_bar::AppTitleBar::new());
        // EXP-456: the settings nav shares the rail's slot (see the struct
        // docs); building it here keeps its subscriptions alive across
        // settings round-trips.
        let settings_nav = cx.new(|cx| SettingsNavPanel::new(window, cx));

        // EXP-456: seed the swap state from the CURRENT screen so a window
        // that opens straight into settings (EXP_DEV_SCREEN=settings) shows
        // no spurious entry animation.
        let in_settings = window_in_settings(window, cx);
        let rail_w = crate::sidebar::RAIL_W;
        let left_anim = LeftColumnAnim::new(
            in_settings,
            if in_settings { SETTINGS_NAV_WIDTH } else { rail_w },
            rail_w,
        );

        Self {
            dock_area,
            title_bar,
            rail,
            settings_nav,
            left_anim,
            _left_anim_task: None,
            login,
            onboarding,
            ordinal,
            last_saved: None,
            _save_task: None,
            pending_window_size: None,
            _size_save_task: None,
            min_size_clamp: Default::default(),
            size_tracker: crate::window_size::WindowSizeTracker::new(
                crate::window_size::launch_size(),
            ),
        }
    }

    /// EXP-456: reconcile the left column's swap state with the active
    /// screen. Called at the top of every render — retargets the transition
    /// when settings open/close and schedules the settle timer that unmounts
    /// the outgoing child (the upstream `gpui_component::sidebar` recipe).
    fn sync_left_column(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        let in_settings = window_in_settings(window, cx);
        let target = left_column_target_width(window, cx);
        if let Some(epoch) =
            self.left_anim
                .retarget(in_settings, target, crate::sidebar::RAIL_W)
        {
            // Settle after the transition: unmount the outgoing child so its
            // controls leave the hit-test/tab order. Dropping a superseded
            // task cancels its timer; the epoch guards against a stale one
            // that already fired.
            self._left_anim_task = Some(cx.spawn_in(window, async move |this, window| {
                window
                    .background_executor()
                    .timer(LEFT_COL_ANIM_DURATION)
                    .await;
                _ = this.update_in(window, move |this, _window, cx| {
                    if this.left_anim.finish(epoch) {
                        cx.notify();
                    }
                });
            }));
        }
    }

    /// EXP-456: the window's leftmost column — the rail, the settings nav,
    /// or (mid-swap) both riding a sliding strip inside a width-morphing
    /// clip.
    ///
    /// EXP-303: the COLUMN paints the one sidebar-alpha ramp and the
    /// window's left frame corners. The Shell root paints no full-window base
    /// ramp, because a second translucent GRADIENT stacked on it never
    /// composited to the intended alpha — the content read fully opaque at
    /// any top-up value (the EXP-293 "not verified visually" gap).
    ///
    /// EXP-723: the children now add NOTHING on top. The rail dropped its
    /// `FILL_SECTION` wash so the brightness step belongs to the cutout panel
    /// on the content side; a wash here would make the sidebar read brighter
    /// than the panel and undo the cutout.
    fn render_left_column(
        &self,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let _ = cx;
        let radii = crate::window_frame::frame_radii(window);
        let column = div()
            .h_full()
            .flex_shrink_0()
            .relative()
            .overflow_hidden()
            .bg(theme::sidebar_background_gradient())
            // EXP-269 corners, left half — the ramp runs flush into the
            // window's left edge (the children round their washes the same
            // way).
            .rounded_tl(radii.top_left)
            .rounded_bl(radii.bottom_left);

        let anim = self.left_anim;
        if !anim.swapping {
            let child: AnyElement = if anim.settings {
                // The explicit sized wrapper is load-bearing for entity
                // children (the dock wrapper's flex-child rule).
                div()
                    .w(px(SETTINGS_NAV_WIDTH))
                    .h_full()
                    .child(self.settings_nav.clone())
                    .into_any_element()
            } else {
                self.rail.clone().into_any_element()
            };
            if !anim.width_only {
                return column.w(px(anim.to)).child(child).into_any_element();
            }
            // EXP-523: the rail expanding/collapsing. The child is pinned at
            // its TARGET width (`sidebar.rs` sizes it from the same flag) and
            // the column's clip morphs around it — a wipe, not a reflow, so
            // no label re-wraps or icon re-centers mid-flight.
            return gpui_component::animation::EffectTransition::new(LEFT_COL_ANIM_DURATION)
                .ease(theme::motion::standard())
                .width(px(anim.from), px(anim.to))
                .apply(
                    column
                        .child(
                            div()
                                .absolute()
                                .top_0()
                                .bottom_0()
                                .left_0()
                                .w(px(anim.to))
                                .child(child),
                        ),
                    left_anim_id("shell-leftcol-rail-width", anim.from, anim.to),
                )
                .into_any_element();
        }

        // Swap in flight: BOTH children ride an absolute [rail | nav] strip
        // that slides while the clip width morphs — the rail exits
        // stage-left as the settings nav takes its place, and the reverse on
        // the way back.
        let (left_from, left_to) = if anim.settings {
            (px(0.), px(-anim.rail_w))
        } else {
            (px(-anim.rail_w), px(0.))
        };
        let strip = h_flex()
            .absolute()
            .top_0()
            .bottom_0()
            .w(px(anim.rail_w + SETTINGS_NAV_WIDTH))
            .child(
                div()
                    .w(px(anim.rail_w))
                    .h_full()
                    .flex_shrink_0()
                    .child(self.rail.clone()),
            )
            .child(
                div()
                    .w(px(SETTINGS_NAV_WIDTH))
                    .h_full()
                    .flex_shrink_0()
                    .child(self.settings_nav.clone()),
            );
        let strip = gpui_component::animation::EffectTransition::new(LEFT_COL_ANIM_DURATION)
            .ease(theme::motion::standard())
            .slide_x(left_from, left_to)
            .apply(
                strip,
                left_anim_id(
                    "shell-leftcol-slide",
                    f32::from(left_from),
                    f32::from(left_to),
                ),
            );
        gpui_component::animation::EffectTransition::new(LEFT_COL_ANIM_DURATION)
            .ease(theme::motion::standard())
            .width(px(anim.from), px(anim.to))
            .apply(
                column.child(strip),
                left_anim_id("shell-leftcol-width", anim.from, anim.to),
            )
            .into_any_element()
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
    ///   "Shell" `TabPanel` title bar (+ zoom control), and a persisted
    ///   panel rehydrates wrapped in a `TabPanel` all the same, so we never
    ///   restore the center from disk. `EXP_DEV_BOARD=1` keeps a tab strip so
    ///   the second (debug-board) tab stays reachable.
    /// - There is deliberately **no left dock** (v6): the sidebar lives
    ///   inside the center split so the bottom terminal dock spans beneath it.
    /// - The **bottom terminal dock** is added if the restored state lacked
    ///   it; its height persists across restarts, but its open state does NOT
    ///   (EXP-301: every launch starts collapsed, with no terminal tabs).
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
                    // panel chrome-less; the dock's persisted height is
                    // untouched.
                    let restored = terminal_panel_view(dock.read(cx).panel(), cx)
                        .unwrap_or_else(|| {
                            Arc::new(
                                cx.new(|cx| TerminalDockPanel::new(weak.clone(), window, cx)),
                            )
                        });
                    dock.update(cx, |dock, cx| {
                        dock.set_panel(DockItem::panel(restored), window, cx);
                        // EXP-301: the OPEN state is deliberately NOT restored —
                        // a launch always comes up with the terminal collapsed
                        // to its 29px strip, so nothing terminal-shaped greets
                        // the user (the restored panel has zero tabs; an open
                        // dock would come up on EXP-369's empty-state cards).
                        dock.set_open(false, window, cx);
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

    /// EXP-210: debounced persist of the main window's last used size
    /// (`observe_window_bounds` fires per resize tick — same rationale as
    /// [`Self::queue_save_layout`]). `take()` so the quit-flush only writes
    /// what the debounce hasn't.
    fn queue_save_window_size(&mut self, cx: &mut gpui::Context<Self>) {
        self._size_save_task = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(SAVE_DEBOUNCE).await;
            _ = this.update(cx, |this, _| {
                if let Some(last) = this.pending_window_size.take() {
                    if let Err(err) = crate::window_size::save_last_size(last) {
                        log_layout(&format!("window size save failed ({err:#})"));
                    }
                }
            });
        }));
    }
}

impl Render for Shell {
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

        // EXP-104: the server 426'd this build — nothing is usable until it
        // updates. This wins over the session switch (login OR board): the
        // whole window becomes the blocking "Update required" surface.
        let client_chrome = crate::app_title_bar::client_chrome(window);
        let blocked = UpdateState::global_ref(cx).is_some_and(|m| m.read(cx).is_blocked());
        if blocked {
            let sheet_layer = Root::render_sheet_layer(window, cx);
            let dialog_layer = Root::render_dialog_layer(window, cx);
            let notification_layer = Root::render_notification_layer(window, cx);
            // The titlebar renders here too — without it this window would be
            // undraggable/unclosable on Windows/Linux (no native chrome) —
            // but as the CHROME-ONLY strip (EXP-364): the tab row belongs to
            // the dock, which this surface replaces, so tabs over it pointed
            // at screens nothing here can reach.
            return crate::window_frame::window_frame()
                .child(
                    // EXP-269 corners: see `window_frame::frame_radii` — the
                    // page gradient paints to the window edge, so it carries
                    // the frame's radii itself (a rectangular content mask
                    // cannot clip it).
                    crate::window_frame::round_to_frame(div(), window)
                        .size_full()
                        .bg(theme::background_gradient())
                        .text_color(cx.theme().foreground)
                        .child(
                            v_flex()
                                .size_full()
                                .when(client_chrome, |body| {
                                    body.child(crate::app_title_bar::chrome_only_title_bar())
                                })
                                .child(
                                    div()
                                        .flex_1()
                                        .min_h_0()
                                        .child(self.render_update_required(cx)),
                                ),
                        )
                        .children(sheet_layer)
                        .children(dialog_layer)
                        .children(notification_layer),
                )
                .into_any_element();
        }

        // The §5 session switch: anything but `Synced` renders the login
        // surface — including `AuthExpired`, the dead-token
        // routing (login screen, never an empty board).
        let session = Store::global(cx).session(cx);
        // EXP-456: keep the left column's rail ⇄ settings-nav swap in step
        // with the active screen (no-op outside the Synced arm, where no
        // left column renders).
        self.sync_left_column(window, cx);
        // Synced shell = rail + dock area, no header (EXP-253 removed the top
        // bar) — the bottom terminal dock spans the full width right of the
        // rail (beneath the sidebar, which lives inside the center split).
        // EXP-285: the rail spans the FULL window height — through the
        // titlebar strip (Cursor look); titlebar + update banner + dock stack
        // in the column to its right. The login/update surfaces keep the
        // titlebar-first layout (no rail there).
        let body: gpui::AnyElement = match session {
            // EXP-367: signed in but a first-run wizard step applies — the
            // wizard takes the whole window (login-surface layout, NO rail:
            // the rail drives trunk-sync, so no clone is attempted until the
            // toolchain step is past).
            SessionPhase::Synced { .. } if self.onboarding.read(cx).is_active(cx) => {
                crate::window_frame::round_to_frame(v_flex(), window)
                    .size_full()
                    .bg(theme::background_gradient())
                    // EXP-364: chrome only — the wizard replaces the dock, so
                    // the tab row has nothing to point at (same rule as the
                    // login and update-gate surfaces).
                    .when(client_chrome, |body| {
                        body.child(crate::app_title_bar::chrome_only_title_bar())
                    })
                    .children(self.render_update_banner(cx))
                    .children(self.render_offline_banner(cx))
                    .child(div().flex_1().min_h_0().child(self.onboarding.clone()))
                    .into_any_element()
            }
            // `.h_full()` on the dock wrapper is load-bearing: without a
            // definite height the dock area collapses (same flex-child rule
            // as the source-control diff pane).
            SessionPhase::Synced { .. } => h_flex()
                .size_full()
                .min_h_0()
                // EXP-456: the leftmost column — the rail, or the settings
                // nav while `Screen::Settings` is up (animated swap). The
                // EXP-303 material rules live on `render_left_column`.
                .child(self.render_left_column(window, cx))
                .child({
                    // EXP-723's cutout: the RIGHT column paints the content
                    // ground edge to edge (decoration band included) and the
                    // app's working surface floats on it as a rounded card
                    // inset by `PANEL_MARGIN`. The margin and the band are the
                    // only places the ground shows through.
                    let radii = crate::window_frame::frame_radii(window);
                    v_flex()
                        .flex_1()
                        .min_w_0()
                        .h_full()
                        // EXP-303/EXP-723: ONE gradient for the whole column
                        // — never a second translucent layer stacked on it
                        // (that never composited to the intended alpha).
                        .bg(theme::background_gradient())
                        // EXP-269 corners: this column reaches the window's
                        // right edge top and bottom (rectangular content mask
                        // — it must round itself).
                        .rounded_tr(radii.top_right)
                        .rounded_br(radii.bottom_right)
                        .when(client_chrome, |col| {
                            // EXP-525: pin the decoration band to a COMPUTED
                            // definite width (the same budget terms as
                            // `app_title_bar`'s `strip_available`). Under
                            // gpui's stray fit-content passes (the EXP-492
                            // class — see `screens::pinned_panel_root`) a
                            // `flex_1` band has no definite width and
                            // collapses to fit-content: the window controls
                            // hug the rail. The tab-less Actions page idles on
                            // exactly such a frame.
                            let band_w = (window.viewport_size().width
                                - crate::window_frame::frame_horizontal_chrome(window)
                                - px(left_column_target_width(window, cx)))
                            .max(px(160.));
                            col.child(
                                h_flex().flex_shrink_0().child(
                                    // EXP-723: NO fill of its own — the band
                                    // is bare ground above the panel, which is
                                    // what makes the panel read as a cutout.
                                    div().w(band_w).child(self.title_bar.clone()),
                                ),
                            )
                        })
                        .child(
                            // The panel: a solid-ish wash inside a hairline,
                            // holding the banners and the dock area.
                            v_flex()
                                .flex_1()
                                .min_h_0()
                                .min_w_0()
                                .mt(px(if client_chrome {
                                    PANEL_MARGIN_TOP
                                } else {
                                    PANEL_MARGIN
                                }))
                                .mx(px(PANEL_MARGIN))
                                .mb(px(PANEL_MARGIN))
                                .rounded(px(theme::tokens::radius::LG))
                                .border_1()
                                .border_color(theme::tokens::glass::STROKE_CARD.to_hsla())
                                .bg(theme::tokens::glass::FILL_PANEL.to_hsla())
                                .overflow_hidden()
                                // EXP-742: the bubble below positions
                                // against the panel, and the panel's own
                                // clip + radii keep it inside the card.
                                .relative()
                                .children(self.render_update_banner(cx))
                                .children(self.render_offline_banner(cx))
                                .child(div().flex_1().min_h_0().child(self.dock_area.clone()))
                                // LAST child: it must paint over the dock
                                // area's band.
                                .children(self.render_dock_bubble(cx)),
                        )
                })
                .into_any_element(),
            // No rail here, so the whole window is content: one content-alpha
            // gradient on all four corners (EXP-303 single-layer rule).
            _ => crate::window_frame::round_to_frame(v_flex(), window)
                .size_full()
                .bg(theme::background_gradient())
                // EXP-364: chrome only here too — a signed-out window kept
                // rendering the previous session's tabs above the login card.
                .when(client_chrome, |body| {
                    body.child(crate::app_title_bar::chrome_only_title_bar())
                })
                .children(self.render_update_banner(cx))
                .child(div().flex_1().min_h_0().child(self.login.clone()))
                .into_any_element(),
        };

        // Root overlay layers (the sanctioned gpui-component pattern — story
        // lib.rs `StoryRoot::render`): the app's root view must compose the
        // sheet/dialog/notification layers or `window.open_dialog` /
        // `push_notification` silently never paint (§3.3 "Root MUST be the
        // first view" is necessary but not sufficient).
        let sheet_layer = Root::render_sheet_layer(window, cx);
        let dialog_layer = Root::render_dialog_layer(window, cx);
        let notification_layer = Root::render_notification_layer(window, cx);

        // window_frame: Linux CSD shadow + rounded frame + resize zones
        // (pass-through elsewhere). The sheet/dialog/notification layers stay
        // INSIDE it so overlays clip to the visible window.
        crate::window_frame::window_frame()
            .child(
                // EXP-269 corners: the gradient paints to the window edge and
                // must carry the frame's radii — gpui clips children with a
                // RECTANGULAR content mask, so without this its square
                // corners fill the notch outside the frame's arc
                // (`window_frame::frame_radii`).
                crate::window_frame::round_to_frame(div(), window)
                    .size_full()
                    // EXP-303/EXP-723: the root paints NO page gradient, and
                    // no region ever stacks two. There are exactly three
                    // layers left, each painted once:
                    //   1. the RAIL column — the sidebar-alpha ramp
                    //      (`render_left_column`); the rail view itself paints
                    //      nothing on top of it.
                    //   2. the CONTENT column — the content-alpha ramp, the
                    //      "ground" the cutout floats on. It is visible in the
                    //      34px decoration band and in the panel's margins.
                    //   3. the PANEL — a SOLID wash (`glass::FILL_PANEL`)
                    //      inside a `strokeCard` hairline, holding the banners
                    //      and the dock.
                    // Stacking a second translucent gradient over a
                    // full-window base never composited to the intended alpha
                    // (the content stayed opaque at any top-up value), which
                    // is why the base+top-up layering EXP-293 introduced is
                    // gone and why the panel's wash is a flat fill, not a
                    // second ramp.
                    .text_color(cx.theme().foreground)
                    .child(body)
                    .children(sheet_layer)
                    .children(dialog_layer)
                    .children(notification_layer),
            )
            .into_any_element()
    }
}

impl Shell {
    /// EXP-742: the collapsed terminal dock's BUBBLE — the panel's own
    /// element (`TerminalDockPanel::render_bubble`, listeners and all),
    /// hosted HERE as the cutout panel's last child. The upstream `Dock`
    /// clips its closed band to a hard 29px, so the only place a card can
    /// float over the corner is above the dock area, and the shell is what
    /// owns that layer. `None` while the dock is open, mid-slide, or when
    /// this machine collapses to the strip.
    ///
    /// Reading the pick through `CodingHub::global` (not the read-only peek)
    /// is deliberate: it WARMS the hub on the shell's first frame, before
    /// the panel renders, so a launch lands straight on the persisted form.
    fn render_dock_bubble(&self, cx: &mut gpui::Context<Self>) -> Option<AnyElement> {
        if !crate::coding_flow::CodingHub::global(cx)
            .read(cx)
            .settings
            .terminal_dock_bubble
        {
            return None;
        }
        let dock = self.dock_area.read(cx).bottom_dock().cloned()?;
        let panel = crate::coding_flow::find_terminal_dock(dock.read(cx).panel())?;
        panel.update(cx, |panel, cx| panel.render_bubble(cx))
    }

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
                format!("Update available. Exponential {version} is out.").into()
            }
            UpdatePhase::Downloading { received, total } => {
                format!("Downloading Exponential {version}… {}", format_progress(*received, *total))
                    .into()
            }
            UpdatePhase::Installing => format!("Installing Exponential {version}…").into(),
            UpdatePhase::ReadyToRestart { .. } => {
                format!("Exponential {version} is ready. Restart to finish updating.").into()
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
            // EXP-723: a banner is the FIRST child of the cutout panel, so it
            // sits in the panel's two top corners — and gpui's content mask is
            // rectangular, so its fill has to round itself or it squares them
            // off inside the panel's own arc.
            .rounded_t(px(theme::tokens::radius::LG))
            .bg(cx.theme().info.opacity(0.14))
            .child(Icon::new(registry::UI_INFO).size_4().text_color(cx.theme().info))
            .child(div().flex_1().text_sm().child(label));

        match &phase {
            UpdatePhase::Available if info.plan.is_some() => {
                banner = banner.child(
                    Button::new("update-install").primary().cursor_pointer().small().label("Update").on_click(
                        move |_: &ClickEvent, _, cx| {
                            update::start_update(cx);
                        },
                    ),
                );
            }
            UpdatePhase::Available => {
                banner =
                    banner.child(download_button("update-download", "Download", url).primary().cursor_pointer());
            }
            UpdatePhase::Downloading { .. } | UpdatePhase::Installing => {}
            UpdatePhase::ReadyToRestart { restart_path } => {
                let restart_path = restart_path.clone();
                banner = banner.child(
                    Button::new("update-restart")
                        .primary().cursor_pointer()
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
                        Button::new("update-retry").primary().cursor_pointer().small().label("Retry").on_click(
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
                    .ghost().cursor_pointer()
                    .small()
                    .icon(registry::UI_CLOSE)
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

    /// EXP-501: the offline strip — the sync engine's failure streak outlived
    /// its grace window, so the boards are showing cached data. Amber (the
    /// update banner is info-blue), not dismissible: it clears itself on the
    /// first successful poll. iOS copy parity (`AppNavigator.healthBanner`).
    /// Only rendered in the Synced layouts — `Store::sync_status` returns Ok
    /// for every other session phase, and the 426 gate replaces the whole
    /// window before this could paint.
    fn render_offline_banner(&self, cx: &mut gpui::Context<Self>) -> Option<AnyElement> {
        let status = Store::global(cx).sync_status(cx);
        if status.health != sync::SyncHealth::Offline {
            return None;
        }
        let ago: Option<SharedString> = status.last_success_at.and_then(|t| {
            let elapsed = std::time::SystemTime::now().duration_since(t).ok()?;
            Some(crate::trunk_sync::synced_ago_labels(elapsed).1)
        });
        Some(
            h_flex()
                .h(px(34.))
                .w_full()
                .flex_shrink_0()
                .items_center()
                .gap_2()
                .px_3()
                .border_b_1()
                .border_color(cx.theme().border)
                // EXP-723: rounds the cutout panel's top corners itself — see
                // `render_update_banner` (the two stack, and whichever is
                // first is the one that shows).
                .rounded_t(px(theme::tokens::radius::LG))
                .bg(cx.theme().warning.opacity(0.14))
                .child(
                    Icon::new(registry::UI_OFFLINE)
                        .size_4()
                        .text_color(cx.theme().warning),
                )
                .child(
                    div()
                        .flex_1()
                        .text_sm()
                        .child("Can't reach the server, showing cached data"),
                )
                .when_some(ago, |banner, ago| {
                    banner.child(div().text_sm().text_color(cx.theme().muted_foreground).child(ago))
                })
                .child(
                    // EXP-533: Retry is not fire-and-forget — while the
                    // restarted pipeline works its way back to head the
                    // button says so, so a second click isn't the only way to
                    // learn the first one did something.
                    Button::new("sync-retry")
                        .ghost()
                        .cursor_pointer()
                        .small()
                        .label(if status.catching_up {
                            "Retrying\u{2026}"
                        } else {
                            "Retry"
                        })
                        .loading(status.catching_up)
                        .on_click(move |_: &ClickEvent, _, cx| {
                            // Restart the account pipeline: parked backoffs and
                            // held long-polls re-poll immediately (EXP-470 rails).
                            let store = Store::global(cx).clone();
                            store.resync_active(cx);
                        }),
                )
                .into_any_element(),
        )
    }

    /// The EXP-104 blocking "Update required" surface: a full-window centered
    /// card (styled like the login surface) shown when the server 426'd this
    /// build. It reuses the self-update pipeline — the same `UpdatePhase`
    /// progression and `start_update` / restart flow as
    /// [`Self::render_update_banner`] — but as a mandatory, non-dismissible
    /// gate. When no install plan is available (staging, dev, an assetless
    /// release) it degrades to a browser "Download update" link.
    fn render_update_required(&self, cx: &mut gpui::Context<Self>) -> AnyElement {
        let (info, phase) = UpdateState::global_ref(cx)
            .map(|model| {
                let state = model.read(cx);
                (state.available().cloned(), state.phase().clone())
            })
            .unwrap_or((None, UpdatePhase::Available));

        let has_plan = info.as_ref().and_then(|i| i.plan.as_ref()).is_some();
        // The release page (info's own html_url, else the generic latest page)
        // is the universal browser fallback.
        let fallback_url = info
            .as_ref()
            .map(|i| i.url.clone())
            .unwrap_or_else(|| update::releases_page_url().to_string());

        // EXP-316: when only the browser link can be offered, SAY WHY — a
        // capability gate ($APPIMAGE unset, moved file, unwritable folder,
        // staging build), a release without this platform's asset, or a
        // check that hasn't answered yet. All three used to render as the
        // same bare "Download update" button, which made Linux failures
        // undiagnosable from a screenshot.
        let no_plan_explainer: Option<SharedString> = (!has_plan)
            .then(|| match update::self_update_unavailable_reason() {
                Some(reason) => format!("In-app update isn't available: {reason}.").into(),
                None => match info.as_ref() {
                    Some(_) => SharedString::from(
                        "The release doesn't include this platform's update file yet. Checking again automatically.",
                    ),
                    None => SharedString::from(
                        "Looking for the release… if this persists, the update check can't reach GitHub.",
                    ),
                },
            });

        // A per-phase status line under the body copy (progress / installing /
        // failure reason) — `None` in the idle `Available` state.
        let status: Option<SharedString> = match &phase {
            UpdatePhase::Available => None,
            UpdatePhase::Downloading { received, total } => {
                Some(format!("Downloading… {}", format_progress(*received, *total)).into())
            }
            UpdatePhase::Installing => Some("Installing…".into()),
            UpdatePhase::ReadyToRestart { .. } => {
                Some("The update is installed. Restart to finish.".into())
            }
            UpdatePhase::Failed { message } => Some(format!("Update failed: {message}").into()),
        };

        let download_button = |id: &'static str, label: &'static str, url: String| {
            Button::new(id)
                .w_full()
                .label(label)
                .on_click(move |_: &ClickEvent, _, _| {
                    if let Err(err) = api::opener::open_in_browser(&url) {
                        log::warn!("[ui] update-required: open release page failed: {err}");
                    }
                })
        };

        // The primary action tracks the pipeline phase — mirrors the banner's
        // phase→button mapping, laid out as a full-width column of buttons.
        let mut actions = v_flex().w_full().gap_2();
        match &phase {
            UpdatePhase::Available if has_plan => {
                actions = actions.child(
                    Button::new("update-required-install")
                        .primary().cursor_pointer()
                        .w_full()
                        .label("Update now")
                        .on_click(move |_: &ClickEvent, _, cx| update::start_update(cx)),
                );
            }
            UpdatePhase::Available => {
                actions =
                    actions.child(download_button("update-required-download", "Download update", fallback_url.clone()).primary().cursor_pointer());
            }
            UpdatePhase::Downloading { .. } | UpdatePhase::Installing => {
                // No action — the status line shows progress; the pipeline is
                // running.
            }
            UpdatePhase::ReadyToRestart { restart_path } => {
                let restart_path = restart_path.clone();
                actions = actions.child(
                    Button::new("update-required-restart")
                        .primary().cursor_pointer()
                        .w_full()
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
                if has_plan {
                    actions = actions.child(
                        Button::new("update-required-retry")
                            .primary().cursor_pointer()
                            .w_full()
                            .label("Retry")
                            .on_click(move |_: &ClickEvent, _, cx| update::start_update(cx)),
                    );
                }
                actions = actions.child(download_button(
                    "update-required-download",
                    "Download update",
                    fallback_url.clone(),
                ));
            }
        }

        // Brand header above the card (login parity).
        let brand = h_flex()
            .gap_2()
            .items_center()
            .justify_center()
            .child(
                Icon::from(ExpIcon::Logo)
                    .with_size(px(32.))
                    .text_color(cx.theme().foreground),
            )
            .child(
                div()
                    .text_xl()
                    .font_weight(FontWeight::SEMIBOLD)
                    .child("Exponential"),
            );

        let card = v_flex()
            .w(px(380.))
            .gap_4()
            .p_6()
            .rounded(cx.theme().radius_lg)
            .border_1()
            .border_color(cx.theme().border)
            .bg(cx.theme().popover)
            .child(
                v_flex()
                    .gap_1()
                    .child(
                        div()
                            .text_xl()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child("Update required"),
                    )
                    .child(
                        div()
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .child(
                                "A newer version of Exponential is required to keep using this instance.",
                            ),
                    ),
            )
            .children(status.map(|text| {
                div()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child(text)
            }))
            .children(no_plan_explainer.map(|text| {
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(text)
            }))
            .child(actions);

        div()
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .child(v_flex().gap_6().items_center().child(brand).child(card))
            .into_any_element()
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
    /// The tool-column split's dragged width. Owned HERE, not by the element:
    /// `h_resizable`'s own state is per-frame element state (gpui drops what a
    /// frame never touched), and settings unmount the split entirely — without
    /// this entity a Settings round-trip would snap the column back to its
    /// default width.
    center_split: Entity<ResizableState>,
    nav: Entity<Navigation>,
    /// The window's shared rail state — the empty-board mode keys on its
    /// active tool (EXP-698).
    rail_shared: Entity<crate::sidebar::RailShared>,
    /// EXP-525/698: whether last render UNMOUNTED the center split (either
    /// full-page mode). A structural flip settles only on the NEXT frame, and
    /// the app must not idle on the flip frame (the EXP-492 stray-pass class).
    last_split_unmounted: Option<bool>,
    _subscriptions: Vec<gpui::Subscription>,
}

/// EXP-698 round 7: the empty-board rule as a truth table — an empty board
/// takes the WHOLE center (no detail pane), matching web
/// (`shots/board-empty/web.webp`). Pure so the combination is testable; the
/// render path feeds it live facts and short-circuits before the expensive
/// ones.
fn board_empty_full_center(
    screen_open: bool,
    board_issues_tool: bool,
    has_board: bool,
    issues_ready: bool,
    board_is_empty: bool,
) -> bool {
    !screen_open && board_issues_tool && has_board && issues_ready && board_is_empty
}

/// EXP-698 round 7: does the center drop its split and give the whole width
/// to the board? Only for the Board Issues tool on a board whose SYNCED issue
/// set is empty — filters are irrelevant (a board filtered to zero results
/// still has issues and keeps the split), and §4.1's `is_ready` keeps a
/// still-syncing collection from flashing the mode on every cold start.
/// Shared with the sidebar, which drops its right hairline in this mode.
pub(crate) fn board_empty_full(
    nav: &Entity<Navigation>,
    rail_shared: &Entity<crate::sidebar::RailShared>,
    cx: &App,
) -> bool {
    let screen_open = resolved_screen(nav, cx).is_some();
    let board_issues_tool = matches!(
        rail_shared.read(cx).tool(),
        crate::sidebar::ToolWindow::BoardIssues
    );
    // The cheap half gates the rest — this runs every frame.
    if screen_open || !board_issues_tool {
        return false;
    }
    let Some(store) = Store::try_global(cx) else {
        return false;
    };
    let board = navigation::active_board_id(nav, cx);
    let issues = store.collections().issues.read(cx);
    let issues_ready = issues.is_ready();
    // A membership probe, not `issues_in_board` — that clones and sorts the
    // whole board to answer "is there one".
    let board_is_empty = issues_ready
        && board
            .as_deref()
            .is_some_and(|board_id| !issues.iter().any(|issue| issue.board_id == board_id));
    board_empty_full_center(
        screen_open,
        board_issues_tool,
        board.is_some(),
        issues_ready,
        board_is_empty,
    )
}

/// Stable serialization name (§3.3) — present in dumps even though the center
/// is always rebuilt fresh.
pub(crate) const CENTER_PANEL_NAME: &str = "Center";

impl CenterPanel {
    pub fn new(window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        let nav = nav_for_window(window, cx);
        // The left column swaps on the ACTIVE SCREEN (EXP-282) — this panel
        // must re-render when navigation moves, not just its children.
        let mut subscriptions = vec![cx.observe(&nav, |_, _, cx| cx.notify())];
        // EXP-698: the empty-board full-width mode keys on the rail's active
        // tool and on the board's SYNCED issue rows — the split must re-mount
        // when the tool changes or the board's first issue arrives.
        let rail_shared = crate::sidebar::rail_shared_for_window(window, cx);
        subscriptions.push(cx.observe(&rail_shared, |_, _, cx| cx.notify()));
        // `try_global`: the rehydrate test builds panels without a store.
        if let Some(store) = Store::try_global(cx) {
            let issues = store.collections().issues.clone();
            subscriptions.push(cx.observe(&issues, |_, _, cx| cx.notify()));
        }
        Self {
            focus_handle: cx.focus_handle(),
            sidebar: cx.new(|cx| SidebarPanel::new(window, cx)),
            screens: cx.new(|cx| ScreensPanel::new(window, cx)),
            center_split: cx.new(|_| ResizableState::default()),
            nav,
            rail_shared,
            last_split_unmounted: None,
            _subscriptions: subscriptions,
        }
    }
}

impl Panel for CenterPanel {
    fn panel_name(&self) -> &'static str {
        CENTER_PANEL_NAME
    }

    fn title(&mut self, _window: &mut Window, _cx: &mut gpui::Context<Self>) -> impl IntoElement {
        "Team"
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
    fn render(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        // EXP-456: in settings the nav column lives in the Shell's LEFT
        // column (it replaces the rail), so the whole center is the settings
        // detail — no resizable split (the detail column caps itself), and
        // the tool column is unmounted. EXP-480: the Actions page is the
        // same tab-less full-page mode (the rail stays; any rail-tool click
        // leaves it via `activate_tool`'s `set_screen(None)`). EXP-698 round
        // 7: an EMPTY board is the second no-split mode — web renders it as
        // ONE full-width area (`shots/board-empty/web.webp`), so the "Nothing
        // open" detail pane must not sit beside a board that has nothing to
        // open. It is the mirror image of `full_page`: there the SCREEN takes
        // the center, here the BOARD does.
        let full_page = resolved_screen(&self.nav, cx).is_some_and(|screen| {
            matches!(screen, Screen::Settings) || screen.is_rail_full_page()
        });
        let board_empty_full = !full_page && board_empty_full(&self.nav, &self.rail_shared, cx);
        // EXP-525: a mount/unmount of the center split settles its layout on
        // the FOLLOWING frame (gpui's stray fit-content passes, EXP-492 —
        // the screens.rs layout tests draw twice per width for the same
        // reason). Force that second frame so a static page (Actions) can't
        // idle on the flip frame's collapsed decoration band. Both no-split
        // modes fold into ONE bool here: what matters is the STRUCTURAL flip,
        // not which mode caused it.
        let split_unmounted = full_page || board_empty_full;
        if self.last_split_unmounted != Some(split_unmounted) {
            self.last_split_unmounted = Some(split_unmounted);
            window.request_animation_frame();
        }
        if full_page {
            return div()
                .size_full()
                .child(self.screens.clone())
                .into_any_element();
        }
        if board_empty_full {
            return div()
                .size_full()
                .child(self.sidebar.clone())
                .into_any_element();
        }
        div()
            .size_full()
            .child(
                h_resizable("center-split")
                    // Panel-owned state: settings unmount this split, and
                    // element state dies with the frame that stops touching it.
                    .with_state(&self.center_split)
                    .child(
                        resizable_panel()
                            .size(SIDEBAR_WIDTH)
                            // Max must stay clear of the default (EXP-109: 520px)
                            // or the sidebar starts grow-locked at its own cap.
                            // Floor = the inline bulk bar's one-line width.
                            .size_range(px(crate::sidebar::MIN_DOCK_WIDTH)..px(880.))
                            .child(self.sidebar.clone()),
                    )
                    .child(resizable_panel().child(self.screens.clone())),
            )
            .into_any_element()
    }
}

/// Per-window layout state file (§3.3 "Each window persists its
/// DockAreaState"). macOS: `~/Library/Application Support/Exponential/…`;
/// Linux: `~/.local/share/exponential/…`.
fn layout_file(ordinal: usize) -> Option<PathBuf> {
    let dir = crate::window_size::app_data_dir()?.join("layouts");
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::SETTINGS_NAV_WIDTH;
    use crate::sidebar::RAIL_W;

    /// EXP-698 round 7: only a Board Issues tool, on a real board, whose
    /// SYNCED issue set is empty, takes the whole center. Every other fact
    /// keeps the split — a still-syncing collection especially (§4.1).
    #[test]
    fn empty_board_takes_the_center_only_when_every_fact_says_so() {
        assert!(board_empty_full_center(false, true, true, true, true));
        // A center tab is open — the detail pane has something to show.
        assert!(!board_empty_full_center(true, true, true, true, true));
        // Another tool (Files / Source Control / Inbox) keeps its detail pane.
        assert!(!board_empty_full_center(false, false, true, true, true));
        // No board resolved yet.
        assert!(!board_empty_full_center(false, true, false, true, true));
        // Empty because the shape has not caught up, not because it is empty.
        assert!(!board_empty_full_center(false, true, true, false, true));
        // The board HAS issues (a filter hiding them all is not our business).
        assert!(!board_empty_full_center(false, true, true, true, false));
    }

    #[test]
    fn retarget_into_settings_starts_swap_and_bumps_epoch() {
        let mut anim = LeftColumnAnim::new(false, RAIL_W, RAIL_W);
        let epoch = anim.retarget(true, SETTINGS_NAV_WIDTH, RAIL_W);
        assert_eq!(epoch, Some(1));
        assert!(anim.swapping);
        assert_eq!(anim.from, RAIL_W);
        assert_eq!(anim.to, SETTINGS_NAV_WIDTH);
        assert!(anim.settings);
    }

    /// EXP-523's same-occupant WIDTH animation. EXP-723 removed the rail
    /// collapse, so nothing in the app drives this path today — but the state
    /// machine still has to handle a width change without mounting both
    /// children, which is why the widths here are literals rather than the
    /// (now single) rail constant.
    #[test]
    fn same_occupant_animates_the_width_without_swapping() {
        let mut anim = LeftColumnAnim::new(false, 44., 44.);
        assert_eq!(anim.retarget(false, 208., 208.), Some(1));
        assert!(!anim.swapping, "same occupant must not mount both children");
        assert!(anim.width_only);
        assert_eq!(anim.from, 44., "it must start where the rail actually was");
        assert_eq!(anim.to, 208.);
        assert_eq!(anim.rail_w, 208.);

        // `sync_left_column` runs `retarget` on EVERY render — an unchanged
        // target must stay a no-op, or the animation restarts every frame and
        // the rail never arrives.
        assert_eq!(anim.retarget(false, 208., 208.), None);
        assert_eq!(anim.epoch, 1);

        assert!(anim.finish(1));
        assert!(!anim.width_only);
        assert_eq!(anim.from, anim.to);
    }

    #[test]
    fn finish_ignores_stale_epochs() {
        let mut anim = LeftColumnAnim::new(false, RAIL_W, RAIL_W);
        let first = anim.retarget(true, SETTINGS_NAV_WIDTH, RAIL_W).unwrap();
        // Mid-flight reversal: the new swap replaces the old one.
        let second = anim.retarget(false, RAIL_W, RAIL_W).unwrap();
        assert_ne!(first, second);
        // The superseded timer fires anyway (cancellation raced) — no-op.
        assert!(!anim.finish(first));
        assert!(anim.swapping);
        // The live one settles the swap.
        assert!(anim.finish(second));
        assert!(!anim.swapping);
        assert_eq!(anim.from, anim.to);
    }

    /// Upstream sidebar rule: a retarget restarts from the prior TARGET (the
    /// animation ids restart the transition, so the visual jump is bounded by
    /// one swap). Literal widths so the assertion stays meaningful now that
    /// the rail and the settings nav are the same width.
    #[test]
    fn midflight_reversal_jumps_from_the_previous_target() {
        let mut anim = LeftColumnAnim::new(false, 208., 208.);
        anim.retarget(true, 164., 208.).unwrap();
        anim.retarget(false, 208., 208.).unwrap();
        assert_eq!(anim.from, 164.);
        assert_eq!(anim.to, 208.);
        assert!(!anim.settings);
    }
}
