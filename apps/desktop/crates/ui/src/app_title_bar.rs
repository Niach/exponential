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
    div, prelude::FluentBuilder as _, px, App, Entity, InteractiveElement as _, IntoElement,
    MouseButton, ParentElement as _, Render, StatefulInteractiveElement as _, Styled as _,
    Subscription, Window,
};
use gpui_component::{h_flex, ActiveTheme as _, Icon, Sizable as _};

use crate::icons::{registry, ExpIcon};

// EXP-269: the vendored TitleBar (rounded window controls — see
// `crate::title_bar`), not gpui-component's, whose close-button hover fill is
// a square that lands in the window's rounded top-right corner.
use crate::title_bar::TitleBar;

use crate::screens::ScreensPanel;

/// The app's display name. EXP-326: rendered by the RAIL, not by this bar —
/// the titlebar band is the tab row and gives every pixel to the strip.
#[cfg(not(feature = "staging"))]
pub(crate) const APP_TITLE: &str = "Exponential";
#[cfg(feature = "staging")]
pub(crate) const APP_TITLE: &str = "Exponential (staging)";

/// The bar's own breathing room, left of the strip and right of it before the
/// window controls. One constant so the rendered padding and the strip's width
/// budget can never drift apart.
const BAR_INSET: f32 = 8.;

/// EXP-449: the fixed width of the labelled "New Issue" button — glyph + gap
/// + the `text_xs` label inside `indigo_button`'s `px_2p5`, with a little
/// slack absorbed by its `justify_center`. Fixed (not content-sized) so the
/// strip's reserve below can never drift from the rendered width.
const NEW_ISSUE_BUTTON_W: f32 = 96.;

/// The width the New Issue button takes out of the strip's budget — the
/// button box plus the [`BAR_INSET`] margin holding it off the window
/// controls. Same one-constant rule as [`BAR_INSET`].
const NEW_ISSUE_RESERVE: f32 = NEW_ISSUE_BUTTON_W + BAR_INSET;

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
/// own — windowed macOS under client chrome. Keep in step with
/// `shell::traffic_tongue_visible`, which draws the tongue for the collapsed
/// half of it.
pub(crate) fn macos_lights_in_strip(window: &Window) -> bool {
    cfg!(target_os = "macos") && client_chrome(window) && !window.is_fullscreen()
}

/// EXP-326: the app brand — logo glyph + [`APP_TITLE`] — rendered in the left
/// column's titlebar strip (the expanded rail, and since EXP-464 the settings
/// nav too) wherever [`macos_lights_in_strip`] leaves it room.
pub(crate) fn brand(cx: &App) -> impl IntoElement {
    h_flex()
        .flex_1()
        .min_w_0()
        .items_center()
        .gap_2()
        .child(
            Icon::from(ExpIcon::Logo)
                .small()
                .text_color(cx.theme().muted_foreground),
        )
        .child(
            div()
                .min_w_0()
                .truncate()
                .text_sm()
                .text_color(cx.theme().foreground.opacity(0.7))
                .child(APP_TITLE),
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

/// The main window's titlebar content: the center tab strip plus the EXP-449
/// New Issue button at the far right (EXP-277 — the decoration band doubles as
/// the tab row so the content area gains its height back). EXP-326: the brand
/// and the collapsed-rail expand toggle both moved into the rail column, so
/// the strip runs from the rail's right edge to the New Issue button.
/// Search/update/account affordances stay in the rail.
///
/// EXP-364: mounted ONLY by the Shell's dock branch — every other surface
/// takes [`chrome_only_title_bar`] — so the rail is always to our left here
/// and the layout math below can count on it.
pub struct AppTitleBar {
    /// This window's center screens panel, resolved lazily from the
    /// per-window registry — the panel is built after the titlebar during
    /// `Shell::new`, so the first render(s) may not find it yet.
    screens: Option<Entity<ScreensPanel>>,
    /// Repaints the bar when tabs open/close/retitle.
    _observe_screens: Option<Subscription>,
    /// Repaints the bar when the rail expands/collapses (EXP-285 — the
    /// left padding and the collapsed-state expand toggle depend on it).
    _observe_rail: Option<Subscription>,
    /// Repaints the bar when the window's scope moves (EXP-449 — the New
    /// Issue button is enabled only while a board is in scope).
    _observe_nav: Option<Subscription>,
}

impl AppTitleBar {
    pub fn new() -> Self {
        Self {
            screens: None,
            _observe_screens: None,
            _observe_rail: None,
            _observe_nav: None,
        }
    }
}

impl Render for AppTitleBar {
    fn render(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        if self.screens.is_none() {
            if let Some(panel) = crate::screens::screens_for_window(window, cx) {
                self._observe_screens = Some(cx.observe(&panel, |_, _, cx| cx.notify()));
                self.screens = Some(panel);
            }
        }
        if self._observe_rail.is_none() {
            let shared = crate::sidebar::rail_shared_for_window(window, cx);
            self._observe_rail = Some(cx.observe(&shared, |_, _, cx| cx.notify()));
        }
        let nav = crate::navigation::nav_for_window(window, cx);
        if self._observe_nav.is_none() {
            self._observe_nav = Some(cx.observe(&nav, |_, _, cx| cx.notify()));
        }
        // EXP-449: `active_board_id` falls back to the team's first board, so
        // this is false only when nothing is in scope at all (no team, or a
        // team whose boards haven't synced) — exactly when the `NewIssue`
        // handler would bail.
        let can_create = crate::navigation::active_board_id(&nav, cx).is_some();

        // EXP-326: the strip's width budget, computed instead of guessed.
        // Everything left and right of it is fixed-width chrome whose widths
        // are constants here, so the old 240/150 "rough estimates" (which cost
        // the strip ~100px and collapsed tabs into "+N" with visible room to
        // spare) have exact replacements:
        //
        //   left  = the rail column + the tongue when it is drawn + this
        //           bar's own inset (`pl` below), plus the vendored fullscreen
        //           `pl_3`.
        //   right = the EXP-449 New Issue button, plus the window controls:
        //           none on macOS (the lights are over the rail), otherwise
        //           three `TITLE_BAR_HEIGHT`-wide buttons — `TitleBar` draws
        //           min + max + close here.
        let strip_available = {
            // EXP-285/EXP-456: the full-height LEFT COLUMN (rail, or the
            // settings nav while Settings is up) sits left of this bar — its
            // target width is the budget's first term. macOS lights float
            // over that column: the expanded rail and the settings nav (both
            // 164px) clear the cluster entirely; the collapsed rail (44px)
            // gets the Shell's tongue. Fullscreen hides the lights, so the
            // reserve is reclaimed outright.
            let rail_w = crate::shell::left_column_target_width(window, cx);
            let tongue_w = if crate::shell::traffic_tongue_visible(window, cx) {
                crate::shell::TRAFFIC_TONGUE_TOTAL_W
            } else {
                0.
            };
            let left_inset = BAR_INSET;
            let fullscreen_inset = if window.is_fullscreen() { 12. } else { 0. };
            let right_reserve = NEW_ISSUE_RESERVE
                + if cfg!(target_os = "macos") {
                    BAR_INSET
                } else {
                    3. * f32::from(crate::title_bar::TITLE_BAR_HEIGHT) + BAR_INSET
                };
            let taken = rail_w + tongue_w + left_inset + fullscreen_inset + right_reserve;
            // EXP-343: on Linux CSD the viewport includes the rounded frame's
            // shadow + border, which are NOT content space — without
            // subtracting them the budget runs ~26px long and the strip's
            // tail lands under the window controls.
            let frame_chrome = crate::window_frame::frame_horizontal_chrome(window);
            (window.viewport_size().width - frame_chrome - px(taken)).max(px(160.))
        };
        // Building the strip via `update` on the panel entity is safe here —
        // the titlebar renders outside the panel's own render pass.
        let strip = self.screens.as_ref().map(|panel| {
            panel.update(cx, |panel, cx| {
                panel.render_tab_strip(strip_available, window, cx)
            })
        });

        let bar = TitleBar::new()
            // EXP-288: a hairline under the tab row — the chips' vertical
            // strokes used to end into nothing.
            .border_b_1()
            .border_color(theme::tokens::glass::STROKE_ROW.to_hsla())
            // EXP-303: with the rail present the vendored 80px macOS
            // traffic-light reserve is wrong in every state — expanded, the
            // rail (164px) clears the cluster; collapsed, the Shell renders
            // the sidebar-material tongue segment LEFT of this bar
            // (`shell::traffic_tongue`), which consumes the reserve. Either
            // way the bar itself only needs its normal inset.
            .pl(px(BAR_INSET));

        // EXP-449: the New Issue button moved out of the board filter bar and
        // into the window chrome, so it is reachable from every screen (and
        // from My Issues, which the filter bar's own gate already allowed).
        // Keeps its glyph + label, exactly as it read in the filter bar.
        let new_issue =
            crate::create_issue_dialog::indigo_button("titlebar-new-issue", !can_create, cx)
                .w(px(NEW_ISSUE_BUTTON_W))
                .child(Icon::new(registry::UI_ADD).small())
                .child("New Issue")
                // `indigo_button`'s disabled contract: callers skip `on_click`.
                .when(can_create, |button| {
                    button.on_click(|_, window, cx| {
                        window.dispatch_action(Box::new(crate::actions::NewIssue), cx)
                    })
                });

        bar.child(
            // The strip content swallows its own mouse-downs (interactive
            // wrapper) so tab presses never start a window drag; the empty
            // remainder of the flex_1 container stays a drag/zoom zone.
            // EXP-282: this wrapper must be a FLEX container — a plain
            // `div()` is display:block, which stretched the interactive
            // strip to 100% width and swallowed every drag on the bar.
            h_flex()
                .flex_1()
                .min_w_0()
                .items_center()
                .when_some(strip, |bar, strip| bar.child(interactive(strip))),
        )
        .child(div().mr(px(BAR_INSET)).child(interactive(new_issue)))
    }
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
