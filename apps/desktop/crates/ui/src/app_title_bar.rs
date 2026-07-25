//! In-app window titlebar (EXP-269).
//!
//! All three platforms draw their own chrome: gpui-component's [`TitleBar`]
//! provides the 34px strip — drag zone, macOS traffic-light gutter (the
//! native lights float over it at the `title_bar_options()` position),
//! Windows/Linux min–max–close controls, double-click zoom, and the Linux
//! right-click window menu. The bar's background/border come from the theme
//! (`title_bar` is transparent glass over the page gradient). This module
//! wraps it with the app branding and the guard for the one case where
//! native chrome remains.

use gpui::{
    div, prelude::FluentBuilder as _, App, Entity, InteractiveElement as _, IntoElement,
    MouseButton, ParentElement as _, Render, Styled as _, Subscription, Window,
};
use gpui_component::{h_flex, ActiveTheme as _, Icon, Sizable as _, TitleBar};

use crate::icons::ExpIcon;
use crate::screens::ScreensPanel;

#[cfg(not(feature = "staging"))]
const APP_TITLE: &str = "Exponential";
#[cfg(feature = "staging")]
const APP_TITLE: &str = "Exponential (staging)";

/// True when this window paints its own chrome. False only on Linux when gpui
/// fell back to server-side decorations (X11 without a compositor forces
/// `Decorations::Server`), where the WM already draws a titlebar — rendering
/// ours would double both the bar and the controls.
pub(crate) fn client_chrome(window: &Window) -> bool {
    !cfg!(target_os = "linux")
        || matches!(window.window_decorations(), gpui::Decorations::Client { .. })
}

/// Wrap interactive titlebar content so pressing it can't start a window
/// drag — the story-app pattern: swallow the bar's own mouse-down listener.
/// (Windows forwards non-client clicks through gpui first, so a handled
/// click never falls through to a caption drag there either.)
pub(crate) fn interactive(children: impl IntoElement) -> impl IntoElement {
    h_flex()
        .items_center()
        .gap_2()
        .on_mouse_down(MouseButton::Left, |_, _, cx: &mut App| cx.stop_propagation())
        .child(children)
}

/// The main window's titlebar content: brand glyph + app name, then the
/// center tab strip (EXP-277 — the decoration band doubles as the tab row so
/// the content area gains its height back). Search/update/account affordances
/// stay in the rail.
pub struct AppTitleBar {
    /// This window's center screens panel, resolved lazily from the
    /// per-window registry — the panel is built after the titlebar during
    /// `Shell::new`, so the first render(s) may not find it yet.
    screens: Option<Entity<ScreensPanel>>,
    /// Repaints the bar when tabs open/close/retitle.
    _observe_screens: Option<Subscription>,
}

impl AppTitleBar {
    pub fn new() -> Self {
        Self { screens: None, _observe_screens: None }
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
        // Building the strip via `update` on the panel entity is safe here —
        // the titlebar renders outside the panel's own render pass.
        let strip = self
            .screens
            .as_ref()
            .map(|panel| panel.update(cx, |panel, cx| panel.render_tab_strip(cx)));

        TitleBar::new().child(
            h_flex()
                .flex_1()
                .min_w_0()
                .items_center()
                .gap_3()
                .child(
                    h_flex()
                        .flex_none()
                        .items_center()
                        .gap_2()
                        .child(
                            Icon::from(ExpIcon::Logo)
                                .small()
                                .text_color(cx.theme().muted_foreground),
                        )
                        .child(
                            div()
                                .text_sm()
                                .text_color(cx.theme().foreground.opacity(0.7))
                                .child(APP_TITLE),
                        ),
                )
                // The strip content swallows its own mouse-downs (interactive
                // wrapper) so tab presses never start a window drag; the empty
                // remainder of the flex_1 container stays a drag/zoom zone.
                .when_some(strip, |bar, strip| {
                    bar.child(div().flex_1().min_w_0().child(interactive(strip)))
                }),
        )
    }
}
