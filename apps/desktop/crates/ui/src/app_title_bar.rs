//! In-app window titlebar (EXP-269).
//!
//! All three platforms draw their own chrome: gpui-component's [`TitleBar`]
//! provides the 34px strip — drag zone, macOS traffic-light gutter (the
//! native lights float over it at the `title_bar_options()` position),
//! Windows/Linux min–max–close controls, double-click zoom, and the Linux
//! right-click window menu. The bar's background/border come from the theme
//! (`title_bar` is transparent glass over the page gradient). This module
//! wraps it with the center tab strip and the guard for the one case where
//! native chrome remains.

use gpui::{
    prelude::FluentBuilder as _, App, InteractiveElement as _, IntoElement, MouseButton,
    ParentElement as _, Styled as _, Window,
};
use gpui_component::h_flex;

// EXP-269: the vendored TitleBar (rounded window controls — see
// `crate::title_bar`), not gpui-component's, whose close-button hover fill is
// a square that lands in the window's rounded top-right corner.
use crate::title_bar::TitleBar;


/// EXP-877: the height of the main window's tab band — the web's
/// `WORK_TABS_BAND_CLASS` (`md:h-[44px]`), with the panel starting flush
/// beneath it (`shell::PANEL_MARGIN_TOP` is 0).
pub(crate) const WORK_TABS_BAND_H: f32 = 44.;

/// #EXP-1726: the band above the content card: drag and window controls, no tabs.
pub(crate) const WINDOW_BAND_H: f32 = 34.;

/// True when this window paints its own chrome. False only on Linux when gpui
/// fell back to server-side decorations (X11 without a compositor forces
/// `Decorations::Server`), where the WM already draws a titlebar — rendering
/// ours would double both the bar and the controls.
pub(crate) fn client_chrome(window: &Window) -> bool {
    !cfg!(target_os = "linux")
        || matches!(window.window_decorations(), gpui::Decorations::Client { .. })
}

/// EXP-326: are the macOS traffic lights sitting in the left column's
/// titlebar strip? That is the ONE case where the strip has no room of its
/// own — windowed macOS under client chrome.
///
/// EXP-760: this is also the ONE condition the strip is rendered AT ALL. Since
/// EXP-723 emptied it (the brand and the expand toggle are gone), its only
/// remaining job is holding room for the traffic lights, so anywhere they do
/// not land — Windows, Linux, macOS fullscreen — those 34px are a dead gap at
/// the top of the rail. Drag and zoom survive on the right-hand
/// [`AppTitleBar`] band, which spans the rest of the decoration row.
pub(crate) fn macos_lights_in_strip(window: &Window) -> bool {
    cfg!(target_os = "macos") && client_chrome(window) && !window.is_fullscreen()
}

/// EXP-862 — the LEFT COLUMN's titlebar strip, one recipe for both of
/// its occupants (the rail, the settings nav). It used to be copied into
/// each of them, which is how they ended up with slightly different padding.
///
/// The strip is EMPTY chrome: since EXP-723 its only job is the 34px the
/// macOS traffic lights float over, plus the drag/zoom wiring the vendored
/// [`TitleBar`] gives the band to its right. [`macos_lights_in_strip`] is
/// therefore the whole gate — `None` everywhere else (Windows, Linux, macOS
/// fullscreen), where those 34px would be a bare gap above the column's first
/// row and the caller takes its own top inset instead.
///
/// `should_move` is the caller's drag latch (`&mut` through a field
/// accessor, since the listeners outlive this call).
pub(crate) fn left_column_top_strip<V: 'static>(
    id: &'static str,
    should_move: fn(&mut V) -> &mut bool,
    window: &Window,
    cx: &mut gpui::Context<V>,
) -> Option<gpui::AnyElement> {
    use gpui::WindowControlArea;
    use gpui_component::InteractiveElementExt as _;

    if !macos_lights_in_strip(window) {
        return None;
    }
    let client_chrome = client_chrome(window);
    Some(
        h_flex()
            .id(id)
            .w_full()
            .h(gpui_component::TITLE_BAR_HEIGHT)
            .flex_shrink_0()
            .items_center()
            // The columns differ in whether their ROOT pads horizontally, so
            // the strip pads itself and the roots stay as they are.
            .px_2()
            .when(client_chrome, |strip| {
                strip
                    .window_control_area(WindowControlArea::Drag)
                    .map(|strip| {
                        if cfg!(target_os = "macos") {
                            strip.on_double_click(|_, window, _| window.titlebar_double_click())
                        } else if cfg!(target_os = "linux") {
                            strip.on_double_click(|_, window, _| window.zoom_window())
                        } else {
                            strip
                        }
                    })
                    .on_mouse_down_out(cx.listener(move |this, _, _, _| *should_move(this) = false))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _, _, _| *should_move(this) = true),
                    )
                    .on_mouse_up(
                        MouseButton::Left,
                        cx.listener(move |this, _, _, _| *should_move(this) = false),
                    )
                    .on_mouse_move(cx.listener(move |this, _, window, _| {
                        if *should_move(this) {
                            *should_move(this) = false;
                            window.start_window_move();
                        }
                    }))
            })
            .into_any_element(),
    )
}

/// Wrap interactive titlebar content so pressing it can't start a window
/// drag — the story-app pattern: swallow the bar's own mouse-down listener.
/// (Windows forwards non-client clicks through gpui first, so a handled
/// click never falls through to a caption drag there either.)
///
/// EXP-294: the RIGHT button is swallowed for the same reason. On Linux
/// client decorations the bar pops the WM window menu (minimize/maximize/…)
/// for ANY right press that reaches it — see [`crate::title_bar`] — and that
/// menu takes a pointer grab, so it buried the tab strip's own context menu.
/// Suppression rides paint order: gpui registers a parent's mouse listeners
/// before painting its children and dispatches the bubble phase in REVERSE
/// paint order, so this wrapper runs before the bar's window-menu overlay
/// (which paints first) and after everything nested inside it — including
/// the handler `ContextMenuExt` installs, which it registers *after* painting
/// the element it wraps. Nested menus still open; only the WM menu is lost,
/// and only where interactive content actually sits (the bar's remaining dead
/// space keeps drag, double-click zoom, and the window menu).
pub(crate) fn interactive(children: impl IntoElement) -> impl IntoElement {
    h_flex()
        .items_center()
        .gap_2()
        .min_w_0()
        .on_mouse_down(MouseButton::Left, |_, _, cx: &mut App| cx.stop_propagation())
        .on_mouse_down(MouseButton::Right, |_, _, cx: &mut App| cx.stop_propagation())
        .child(children)
}

/// The chrome-only titlebar: the same 34px drag strip and window controls as
/// [`AppTitleBar`], with no tab strip and no hairline beneath it.
///
/// EXP-364: the Shell's full-window surfaces — the login screen, the EXP-367
/// first-run wizard, and the EXP-104 update gate — replace the rail+dock
/// entirely, so the tab row above them pointed at screens that surface can't
/// reach and made a whole-window surface read as a panel. They still need the
/// strip itself: wherever we paint our own chrome, dropping it leaves the
/// window undraggable (and unclosable on Windows/Linux).
pub(crate) fn chrome_only_title_bar() -> impl IntoElement {
    // No hairline: with nothing in the strip, the vendored bottom border is
    // just a rule across the top of a surface that should read as one page.
    TitleBar::new().border_b_0()
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::rc::Rc;

    use gpui::{
        div, point, px, Context, InteractiveElement as _, IntoElement, MouseButton,
        ParentElement as _, Render, Styled as _, TestApp, Window,
    };

    /// Mirrors the shape of the Linux client-decoration branch in
    /// [`crate::title_bar`]: an absolutely-positioned overlay painted BEFORE
    /// the bar's content, whose right-press handler is the one that calls
    /// `Window::show_window_menu` in the real bar (the test platform's
    /// `show_window_menu` is `unimplemented!()`, so a flag stands in for it).
    struct Bar {
        window_menu: Rc<Cell<bool>>,
        content_menu: Rc<Cell<bool>>,
        /// Whether the content goes through [`super::interactive`] — the
        /// wrapper every interactive titlebar element (incl. the EXP-277 tab
        /// strip) is mounted with.
        wrapped: bool,
    }

    impl Render for Bar {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            let _span = tracing::trace_span!("render", view = "Bar").entered();
            let window_menu = self.window_menu.clone();
            let content_menu = self.content_menu.clone();
            // Stands in for a tab chip's `.context_menu()`: a right-press
            // listener nested inside the bar's content.
            let content = div()
                .w(px(120.))
                .h(px(34.))
                .on_mouse_down(MouseButton::Right, move |_, _, _| content_menu.set(true));

            div()
                .relative()
                .w(px(400.))
                .h(px(34.))
                .child(
                    div()
                        .absolute()
                        .top_0()
                        .left_0()
                        .size_full()
                        .on_mouse_down(MouseButton::Right, move |_, _, _| window_menu.set(true)),
                )
                .child(if self.wrapped {
                    super::interactive(content).into_any_element()
                } else {
                    content.into_any_element()
                })
        }
    }

    fn right_press_on_content(wrapped: bool) -> (bool, bool) {
        let window_menu = Rc::new(Cell::new(false));
        let content_menu = Rc::new(Cell::new(false));
        let mut app = TestApp::new();
        let mut window = app.open_window({
            let window_menu = window_menu.clone();
            let content_menu = content_menu.clone();
            move |_, _| Bar {
                window_menu,
                content_menu,
                wrapped,
            }
        });

        window.simulate_mouse_down(point(px(20.), px(10.)), MouseButton::Right);
        (window_menu.get(), content_menu.get())
    }

    /// EXP-294: a right press on titlebar content must reach the content's
    /// own menu and NOT the Linux window menu — the WM menu grabs the
    /// pointer, so both firing means ours is buried.
    #[test]
    fn interactive_content_keeps_its_menu_and_suppresses_the_window_menu() {
        let (window_menu, content_menu) = right_press_on_content(true);
        assert!(content_menu, "the content's own right-press listener must run");
        assert!(!window_menu, "the Linux window menu must not be popped");
    }

    /// The counterfactual: bare content (what the tab strip effectively was
    /// for the right button before EXP-294) lets the press through to the
    /// overlay, which is exactly the reported bug. Also proves the harness
    /// wires the overlay the way the real bar does.
    #[test]
    fn bare_content_still_falls_through_to_the_window_menu() {
        let (window_menu, content_menu) = right_press_on_content(false);
        assert!(content_menu, "the content's own right-press listener must run");
        assert!(window_menu, "the overlay must fire without the wrapper");
    }
}
