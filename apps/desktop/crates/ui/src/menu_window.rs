//! N-6 — THE menu window: an IDE menu drawn in its own pop-up window, so it
//! can overhang the window that opened it.
//!
//! Every IDE dialog is a real OS window (EXP-284) and every IDE menu is a
//! gpui-drawn element INSIDE the window that opened it — and an element can
//! never leave its window. The composer dialog opens 258px tall, its "+"
//! menu wants ~360px: no flip, slide or anchor can make that fit, so
//! `anchored().snap_to_window` slammed the menu against the title bar and
//! clipped the rest. Same for every dialog that hosts a menu.
//!
//! The fix is what every native toolkit does: the menu gets a borderless,
//! transparent pop-up window of its own — a "menu layer" — laid over the
//! host window and a [`LAYER_MARGIN`] past its edges, clamped to the display.
//! Inside it the SAME drawn menu (`PopupMenu`, the pointer layer, the enter
//! fade) renders with the same `anchored` / `snap_to_window` logic, against
//! the layer instead of the small host — flipping, submenus (deferred
//! anchored children of the same menu) and scroll caps keep working
//! untouched, and the look is identical to a menu in the main window.
//!
//! Platform basis (the pinned gpui rev):
//! - macOS: `WindowKind::PopUp` is a non-activating `NSPanel` at pop-up
//!   level that CAN become key, placed at absolute screen coordinates
//!   (`window.bounds()` is screen-space there and on Windows).
//! - Windows: `PopUp` is a topmost tool window, placed likewise.
//! - X11: `PopUp` is an override-redirect window — the one window kind
//!   whose origin survives the WM (EXP-308 lost it for `Normal` dialogs).
//!   The WM never focuses one and gpui ignores `focus` there, so the layer
//!   activates itself and hands the keyboard back on close (N-14).
//! - Wayland: toplevels cannot be positioned at all; gpui's own
//!   `WindowKind::AnchoredPopup` (an `xdg_popup`) is the only placed
//!   surface there and is the follow-up. Today the open fails there and the
//!   menu falls back to the in-window popover.
//! - gpui's `AnchoredPopup` is Wayland-only at this rev (macOS, Windows and
//!   X11 reject it with `PopupNotSupportedError`), hence `PopUp`.
//!
//! Semantics:
//! - **Who:** [`wanted`] says whether menus opened from a window go to a
//!   layer — today: the window is a dialog (the `native_dialog` registry
//!   knows). Menus in the main and undocked windows do not change at all.
//! - **Dismiss:** a press on the layer's transparent part is a mouse-down
//!   OUTSIDE the menu, which the menu answers with `DismissEvent` (the web's
//!   modal menu: an outside click only closes it, it does not fall through
//!   to the host). Escape, a pick, the layer losing key status (⌘-Tab, a
//!   click in another window), the host moving, resizing or closing also
//!   close it. ONE layer app-wide: opening a second closes the first.
//! - **Keys:** the menu takes focus inside the layer; arrows, Enter and
//!   Escape are the menu's own actions.
//! - **Handlers run in the HOST:** a row's `on_select` expects the window
//!   it was built for (it focuses the composer's input, opens the file
//!   picker over the dialog, resizes the dialog). [`run_in_host`] relays
//!   them from the layer into the host window, deferred, so every existing
//!   handler keeps its `window`.
//! - **Fallback:** when the OS refuses the window, the caller opens its
//!   in-window popover as before (`PointerMenu`'s `open_in_window`).

use std::cell::Cell;
use std::rc::Rc;

use gpui::{
    anchored, deferred, div, point, px, size, Anchor, AnyWindowHandle,
    App, AppContext as _, Bounds, Context, Entity, Focusable as _, Global, InteractiveElement as _,
    IntoElement, ParentElement as _, Pixels, Render, SharedString, Styled as _, Subscription,
    Window, WindowBounds, WindowId, WindowKind, WindowOptions,
};
use gpui_component::menu::PopupMenu;
use gpui_component::{v_flex, Root};

use crate::controls::{overlay_enter, with_pointer_layer};


/// How far past the host's edges the layer reaches on every side — the room
/// a menu (and its submenus) may overhang by. Clamped to the display.
pub(crate) const LAYER_MARGIN: f32 = 480.;

/// The gap a menu keeps to the layer's edge (the in-window popover's
/// `snap_to_window_with_margin(8)`).
const SNAP_MARGIN: f32 = 8.;

/// The menu's builder: the same shape `PointerMenu` and
/// `PointerContextMenu` take.
pub(crate) type MenuBuilder =
    Rc<dyn Fn(PopupMenu, &mut Window, &mut Context<PopupMenu>) -> PopupMenu>;

/// Where a menu hangs: `rect` in the HOST window's coordinates (the trigger,
/// or a zero-size rect at the pointer for a context menu) and the height
/// the menu wants, room permitting. The layer picks the side itself
/// ([`plan_side`]): it has room the host never had.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct MenuAnchor {
    pub rect: Bounds<Pixels>,
    pub wanted: Pixels,
}

/// The height an IDE menu is assumed to want when its caller does not say
/// (the composer's "+" menu is ~360px).
pub(crate) const DEFAULT_WANTED_HEIGHT: f32 = 360.;

/// The gap between the trigger and the menu (web `sideOffset`).
const SIDE_GAP: f32 = 4.;

/// Which side of `anchor` (layer coordinates) the menu opens on inside a
/// layer `layer_size` tall, and the point its corner sits at: BELOW the
/// anchor (`Anchor::TopLeft` at its bottom-left, the web's default side)
/// when `wanted` fits there or there is at least as much room as above;
/// else ABOVE (`Anchor::BottomLeft` at its top-left). The snap keeps a menu
/// taller than either side inside the layer.
pub(crate) fn plan_side(
    layer_size: gpui::Size<Pixels>,
    anchor: Bounds<Pixels>,
    wanted: Pixels,
) -> (Anchor, gpui::Point<Pixels>) {
    let gap = px(SIDE_GAP);
    let below = layer_size.height - anchor.bottom() - gap;
    let above = anchor.top() - gap;
    if below >= wanted || below >= above {
        (Anchor::TopLeft, point(anchor.left(), anchor.bottom() + gap))
    } else {
        (Anchor::BottomLeft, point(anchor.left(), anchor.top() - gap))
    }
}

/// Whether menus opened from `window` should go to a menu window: the
/// window is a dialog. A dialog window is small by design and its menus
/// are the ones that clip; everywhere else the in-window menu has room.
pub(crate) fn wanted(window: &Window, cx: &App) -> bool {
    crate::native_dialog::is_dialog_window(window, cx)
}

/// The layer's screen rect: the host's rect grown by `margin` on every side,
/// clamped to the display it is on (`None` = no display known: unclamped).
pub(crate) fn plan_layer(
    host: Bounds<Pixels>,
    display: Option<Bounds<Pixels>>,
    margin: Pixels,
) -> Bounds<Pixels> {
    let grown = Bounds {
        origin: point(host.origin.x - margin, host.origin.y - margin),
        size: size(host.size.width + margin * 2., host.size.height + margin * 2.),
    };
    match display {
        Some(display) => {
            let clamped = grown.intersect(&display);
            // A host entirely off its display (mid-drag between screens):
            // keep the grown rect rather than a degenerate one.
            if clamped.size.width <= px(0.) || clamped.size.height <= px(0.) {
                grown
            } else {
                clamped
            }
        }
        None => grown,
    }
}

/// A rect in the host's window coordinates, in the layer's: the host's
/// screen origin carries it over, the layer's takes it back.
pub(crate) fn into_layer(
    layer: Bounds<Pixels>,
    host: Bounds<Pixels>,
    rect_in_host: Bounds<Pixels>,
) -> Bounds<Pixels> {
    Bounds {
        origin: point(
            host.origin.x + rect_in_host.origin.x - layer.origin.x,
            host.origin.y + rect_in_host.origin.y - layer.origin.y,
        ),
        size: rect_in_host.size,
    }
}

/// The ONE open layer.
struct OpenLayer {
    layer: AnyWindowHandle,
    host: AnyWindowHandle,
    /// The opener's menu id — its trigger draws selected while this is up.
    id: SharedString,
    /// Lives while the layer does: the host's move/resize observer.
    _host_watch: Entity<HostWatch>,
}

#[derive(Default)]
struct MenuWindowState {
    open: Option<OpenLayer>,
}

struct MenuWindowGlobal(Entity<MenuWindowState>);

impl Global for MenuWindowGlobal {}

fn state(cx: &mut App) -> Entity<MenuWindowState> {
    if let Some(global) = cx.try_global::<MenuWindowGlobal>() {
        return global.0.clone();
    }
    let state = cx.new(|_| MenuWindowState::default());
    cx.set_global(MenuWindowGlobal(state.clone()));
    state
}

/// The open layer's window id and its host, if a layer is up.
fn open_layer(cx: &App) -> Option<(WindowId, AnyWindowHandle)> {
    let state = cx.try_global::<MenuWindowGlobal>()?.0.read(cx);
    state
        .open
        .as_ref()
        .map(|open| (open.layer.window_id(), open.host))
}

/// Whether the menu `id` is the one open in a layer (its trigger draws
/// selected).
pub(crate) fn is_open(id: &SharedString, cx: &App) -> bool {
    cx.try_global::<MenuWindowGlobal>()
        .is_some_and(|global| global.0.read(cx).open.as_ref().is_some_and(|open| open.id == *id))
}

/// Run `f` in the window a menu row was built for, NOW. From the menu layer
/// that is the HOST window (a synchronous update of another window — the
/// layer is the one mid-update, the host is free); from anywhere else it is
/// this window. Every IDE row handler goes through here
/// (`controls::host_bound`, the search list's pick), so a handler written
/// against its own window keeps working when its menu moved to a layer: it
/// focuses the dialog's input, resizes the dialog, opens the file picker
/// over it. A host that is gone (closed under the menu) runs `f` here.
pub(crate) fn in_host<R>(
    window: &mut Window,
    cx: &mut App,
    f: impl FnOnce(&mut Window, &mut App) -> R,
) -> R {
    match open_layer(cx) {
        Some((layer, host)) if layer == window.window_handle().window_id() => {
            let mut f = Some(f);
            let relayed = host.update(cx, |_, window, cx| (f.take().expect("runs once"))(window, cx));
            match relayed {
                Ok(result) => result,
                Err(_) => (f.take().expect("the host update did not run it"))(window, cx),
            }
        }
        _ => f(window, cx),
    }
}

/// [`in_host`] for a handler with nothing to return.
pub(crate) fn run_in_host(
    window: &mut Window,
    cx: &mut App,
    f: impl FnOnce(&mut Window, &mut App),
) {
    in_host(window, cx, f)
}

/// Close the open layer, if any (deferred — safe from inside any update).
/// The host repaints, so the trigger drops its selected look.
pub(crate) fn close(cx: &mut App) {
    let state = state(cx);
    let Some(open) = state.update(cx, |state, _| state.open.take()) else {
        return;
    };
    let layer = open.layer;
    let host = open.host;
    log::info!("[menu-window] closing the layer of {}", open.id);
    cx.defer(move |cx| {
        // N-14: a layer that held the keyboard hands it back to its host
        // (on X11 nothing else would: the focus would revert to the root).
        // One that had lost it (a click in another app) leaves it there.
        let had_key = layer
            .update(cx, |_, window, _| window.is_window_active())
            .unwrap_or(false);
        let _ = layer.update(cx, |_, window, _| window.remove_window());
        let _ = host.update(cx, |_, window, _| {
            if had_key && cfg!(target_os = "linux") {
                window.activate_window();
            }
            window.refresh();
        });
    });
}

/// A host window is going away (the dialog registry's release hook): its
/// layer goes with it.
pub(crate) fn on_host_released(host: WindowId, cx: &mut App) {
    if open_layer(cx).is_some_and(|(_, open_host)| open_host.window_id() == host) {
        close(cx);
    }
}

/// Open `builder`'s menu in a layer over `host` (the window the call comes
/// from), hung at `anchor`; `select_first` selects its first row once it is
/// up (EXP-1259's dev open, for a capture). The window opens DEFERRED (opening a window
/// from inside another window's update is the re-entrancy trap
/// `native_dialog` dodges the same way); when the OS refuses it,
/// `on_refused` runs in the host, and the caller opens its in-window menu
/// as before. The layer replaces any layer already up.
pub(crate) fn open(
    host: &mut Window,
    cx: &mut App,
    anchor: MenuAnchor,
    builder: MenuBuilder,
    id: SharedString,
    select_first: bool,
    on_refused: impl FnOnce(&mut Window, &mut App) + 'static,
) {
    close(cx);
    let host_handle = host.window_handle();
    let host_bounds = host.bounds();
    let display = host.display(cx);
    let display_bounds = display.as_ref().map(|display| display.bounds());
    let display_id = display.as_ref().map(|display| display.id());
    let layer_bounds = plan_layer(host_bounds, display_bounds, px(LAYER_MARGIN));
    let anchor_in_layer = into_layer(layer_bounds, host_bounds, anchor.rect);
    let wanted = anchor.wanted;
    let state = state(cx);
    // The host MOVING closes the layer: the menu would be left hanging where
    // the trigger no longer is. A resize keeps it: a dialog follows its
    // content downward with its top pinned (EXP-1155, the composer's note
    // landing under the card), which moves nothing above the growth, and
    // the layer's margin covers the grown host. gpui's bounds observer also
    // fires on changes that move nothing at all (another window of the app
    // coming up — the layer itself), so only an origin change counts.
    let host_watch = cx.new(|cx: &mut Context<HostWatch>| {
        let bounds = cx.observe_window_bounds(host, move |_, window, cx| {
            if window.bounds().origin == host_bounds.origin {
                return;
            }
            log::info!("[menu-window] the host moved to {:?}: closing", window.bounds());
            close(cx);
        });
        HostWatch { _subscriptions: vec![bounds] }
    });
    cx.defer(move |cx| {
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(layer_bounds)),
            titlebar: None,
            // Key, so the menu's arrows / Enter / Escape land in it; the
            // panel kind never activates the app (it already is).
            focus: true,
            show: true,
            kind: WindowKind::PopUp,
            is_movable: false,
            is_resizable: false,
            is_minimizable: false,
            display_id,
            window_background: gpui::WindowBackgroundAppearance::Transparent,
            app_id: Some(crate::undock::CHANNEL_APP_ID.to_string()),
            #[cfg(target_os = "linux")]
            window_decorations: Some(gpui::WindowDecorations::Client),
            ..Default::default()
        };
        let layer_id = id.clone();
        log::info!(
            "[menu-window] opening a layer for {id}: layer {layer_bounds:?} over host {host_bounds:?}, anchor {anchor_in_layer:?}"
        );
        let opened = cx.open_window(options, move |window, cx| {
            let menu = PopupMenu::build(window, cx, {
                let builder = builder.clone();
                move |menu, window, cx| builder(menu, window, cx)
            });
            let layer = cx.new(|cx| {
                MenuLayer::new(menu, anchor_in_layer, wanted, layer_id, select_first, window, cx)
            });
            // Root first (§3.3): it hosts the overlay layers a menu's own
            // popovers paint into. N-14: WITHOUT the Linux client-side
            // decoration wrapper — it framed the whole transparent layer in
            // a 1px border and a shadow (macOS never draws it).
            cx.new(|cx| {
                Root::new(layer, window, cx)
                    .bordered(false)
                    .bg(gpui::transparent_black())
            })
        });
        match opened {
            Ok(handle) => {
                log::info!("[menu-window] layer up for {id}");
                // N-14: X11 ignores `focus` and never gives an
                // override-redirect window the keyboard — the keys went on
                // to the dialog (Escape closed the DIALOG, the arrows and
                // the submenu's search typed nowhere). Take it explicitly.
                #[cfg(target_os = "linux")]
                let _ = handle.update(cx, |_, window, _| window.activate_window());
                state.update(cx, |state, _| {
                    state.open = Some(OpenLayer {
                        layer: handle.into(),
                        host: host_handle,
                        id,
                        _host_watch: host_watch,
                    });
                });
                let _ = host_handle.update(cx, |_, window, _| window.refresh());
            }
            Err(error) => {
                log::warn!("menu window: pop-up refused, the menu stays in-window: {error:#}");
                let _ = host_handle.update(cx, |_, window, cx| on_refused(window, cx));
            }
        }
    });
}

/// Holds the host-side subscriptions for the layer's lifetime.
struct HostWatch {
    _subscriptions: Vec<Subscription>,
}

/// The layer window's root view: the menu, hung at the anchor, over a
/// transparent full-window surface.
struct MenuLayer {
    menu: Entity<PopupMenu>,
    anchor: Bounds<Pixels>,
    wanted: Pixels,
    id: SharedString,
    _subscriptions: Vec<Subscription>,
}

impl MenuLayer {
    fn new(
        menu: Entity<PopupMenu>,
        anchor: Bounds<Pixels>,
        wanted: Pixels,
        id: SharedString,
        select_first: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let dismissed = cx.subscribe_in(&menu, window, |_, _, _: &gpui::DismissEvent, _, cx| {
            close(cx);
        });
        // The layer has been key once — before that a "not active" reading
        // is the window still coming up, not the user leaving.
        let was_active = Rc::new(Cell::new(false));
        let activation = cx.observe_window_activation(window, {
            let was_active = was_active.clone();
            move |_, window, cx| {
                if window.is_window_active() {
                    was_active.set(true);
                } else if was_active.get() {
                    // ⌘-Tab, a click in another window: the menu goes.
                    log::info!("[menu-window] the layer lost key: closing");
                    close(cx);
                }
            }
        });
        menu.focus_handle(cx).focus(window, cx);
        if select_first {
            // EXP-1259's dev open (a capture): the first frame that PAINTED
            // the menu owns its key context — Down from nothing selects row
            // 0, and a submenu row selected is its submenu open.
            let handle = menu.focus_handle(cx);
            window.on_next_frame(move |window, cx| {
                handle.dispatch_action(&gpui_base::actions::SelectDown, window, cx);
            });
        }
        Self {
            menu,
            anchor,
            wanted,
            id,
            _subscriptions: vec![dismissed, activation],
        }
    }
}

impl Render for MenuLayer {
    fn render(&mut self, window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let (corner, position) = plan_side(window.viewport_size(), self.anchor, self.wanted);
        let content = v_flex()
            .occlude()
            .child(overlay_enter(
                gpui::ElementId::Name(SharedString::from(format!("menu-window-enter:{}", self.id))),
                with_pointer_layer(self.menu.clone()),
            ));
        div()
            .id("menu-layer")
            .size_full()
            .relative()
            .on_scroll_wheel(|_, _, cx| cx.stop_propagation())
            .child(
                deferred(
                    anchored()
                        .anchor(corner)
                        .position(position)
                        .snap_to_window_with_margin(px(SNAP_MARGIN))
                        .child(content),
                )
                .with_priority(gpui_base::POPUP_PRIORITY),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bounds(x: f32, y: f32, w: f32, h: f32) -> Bounds<Pixels> {
        Bounds {
            origin: point(px(x), px(y)),
            size: size(px(w), px(h)),
        }
    }

    #[test]
    fn the_layer_grows_the_host_by_the_margin_on_every_side() {
        let layer = plan_layer(bounds(1000., 500., 640., 300.), None, px(480.));
        assert_eq!(layer, bounds(520., 20., 1600., 1260.));
    }

    #[test]
    fn the_layer_is_clamped_to_the_display() {
        let display = bounds(0., 0., 1920., 1080.);
        let layer = plan_layer(bounds(100., 50., 640., 300.), Some(display), px(480.));
        assert_eq!(layer, bounds(0., 0., 1220., 830.));
    }

    #[test]
    fn a_host_off_its_display_keeps_the_grown_rect() {
        let display = bounds(0., 0., 1920., 1080.);
        let layer = plan_layer(bounds(3000., 50., 640., 300.), Some(display), px(480.));
        assert_eq!(layer, bounds(2520., -430., 1600., 1260.));
    }

    #[test]
    fn a_host_rect_lands_in_layer_coordinates() {
        let host = bounds(1000., 500., 640., 300.);
        let layer = plan_layer(host, None, px(480.));
        let trigger = bounds(24., 240., 32., 32.);
        assert_eq!(into_layer(layer, host, trigger), bounds(504., 720., 32., 32.));
    }

    #[test]
    fn a_menu_opens_below_its_trigger_when_the_room_is_there() {
        let trigger = bounds(500., 300., 24., 24.);
        let (corner, at) = plan_side(size(px(1600.), px(900.)), trigger, px(360.));
        assert_eq!(corner, Anchor::TopLeft);
        assert_eq!(at, point(px(500.), px(328.)));
    }

    #[test]
    fn a_menu_opens_above_when_only_the_room_above_fits_it() {
        let trigger = bounds(500., 700., 24., 24.);
        let (corner, at) = plan_side(size(px(1600.), px(900.)), trigger, px(360.));
        assert_eq!(corner, Anchor::BottomLeft);
        assert_eq!(at, point(px(500.), px(696.)));
    }

    #[test]
    fn a_menu_too_tall_for_either_side_takes_the_bigger_one() {
        let trigger = bounds(500., 300., 24., 24.);
        let (corner, _) = plan_side(size(px(1600.), px(500.)), trigger, px(900.));
        assert_eq!(corner, Anchor::BottomLeft);
    }

    #[test]
    fn a_clamped_layer_still_maps_the_host_rect() {
        let host = bounds(100., 50., 640., 300.);
        let layer = plan_layer(host, Some(bounds(0., 0., 1920., 1080.)), px(480.));
        let trigger = bounds(24., 240., 32., 32.);
        // The layer starts at the display's origin: the host's own offset
        // carries the trigger over unchanged.
        assert_eq!(into_layer(layer, host, trigger), bounds(124., 290., 32., 32.));
    }
}
