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
    div, prelude::FluentBuilder as _, px, App, ClickEvent, Entity, InteractiveElement as _,
    IntoElement, MouseButton, ParentElement as _, Render, Styled as _, Subscription, Window,
};
use gpui_component::{
    button::{Button, ButtonVariants as _},
    h_flex, ActiveTheme as _, Icon, IconName, Sizable as _,
};

// EXP-269: the vendored TitleBar (rounded window controls — see
// `crate::title_bar`), not gpui-component's, whose close-button hover fill is
// a square that lands in the window's rounded top-right corner.
use crate::title_bar::TitleBar;
use sync::{SessionPhase, Store};

use crate::icons::ExpIcon;
use crate::screens::ScreensPanel;
use crate::update::UpdateState;

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
        .min_w_0()
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
    /// Repaints the bar when the rail expands/collapses (EXP-285 — the
    /// left padding and the collapsed-state expand toggle depend on it).
    _observe_rail: Option<Subscription>,
    /// Repaints the bar when the session phase flips (EXP-285 — `rail_present`
    /// reads it, and the macOS traffic-light padding follows the rail). Without
    /// this a Synced↔login transition leaves the bar on the previous padding
    /// until some unrelated notify happens to repaint it.
    _observe_session: Option<Subscription>,
    /// Same, for the update-blocked gate — the other half of `rail_present`.
    _observe_update: Option<Subscription>,
}

impl AppTitleBar {
    pub fn new() -> Self {
        Self {
            screens: None,
            _observe_screens: None,
            _observe_rail: None,
            _observe_session: None,
            _observe_update: None,
        }
    }
}

/// EXP-285: whether this window renders the full-height rail next to the
/// titlebar (`Shell`'s Synced branch, not update-blocked). Mirrors the
/// branch conditions in `Shell::render` — keep the two in sync.
fn rail_present(cx: &App) -> bool {
    let blocked = UpdateState::global_ref(cx).is_some_and(|m| m.read(cx).is_blocked());
    !blocked && matches!(Store::global(cx).session(cx), SessionPhase::Synced { .. })
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
        // Both halves of `rail_present` live outside this view — observe them
        // so the rail-aware padding can't go stale (see the field docs).
        if self._observe_session.is_none() {
            if let Some(state) = Store::try_global(cx).map(|store| store.state()) {
                self._observe_session = Some(cx.observe(&state, |_, _, cx| cx.notify()));
            }
        }
        if self._observe_update.is_none() {
            if let Some(update) = UpdateState::global_ref(cx) {
                self._observe_update = Some(cx.observe(&update, |_, _, cx| cx.notify()));
            }
        }
        // Building the strip via `update` on the panel entity is safe here —
        // the titlebar renders outside the panel's own render pass.
        let strip = self
            .screens
            .as_ref()
            .map(|panel| panel.update(cx, |panel, cx| panel.render_tab_strip(cx)));

        // EXP-285: with the full-height rail to our left, the vendored 80px
        // macOS traffic-light reserve is wrong — the lights float over the
        // RAIL now. Expanded (164px) the rail clears the cluster entirely;
        // collapsed (44px) the bar still needs the remainder. Fullscreen
        // hides the lights, so the reserve is reclaimed outright.
        let rail = rail_present(cx);
        let expanded = rail && crate::sidebar::rail_expanded(window, cx);
        let bar = TitleBar::new().when(rail, |bar| {
            let pl = if cfg!(target_os = "macos") && !window.is_fullscreen() && !expanded {
                px(80. - crate::sidebar::RAIL_W)
            } else {
                px(8.)
            };
            bar.pl(pl)
        });

        // Collapsed rail: the 44px strip can't host the expand toggle (the
        // macOS traffic lights sit over it) — surface it here instead.
        let expand_toggle = (rail && !expanded).then(|| {
            interactive(
                Button::new("titlebar-rail-expand")
                    .ghost()
                    .small()
                    .icon(IconName::PanelLeftOpen)
                    .tooltip("Expand sidebar")
                    // Direct call (EXP-17): titlebar buttons must not
                    // dispatch App-global actions.
                    .on_click(cx.listener(|_, _: &ClickEvent, window, cx| {
                        crate::sidebar::toggle_rail_expanded(window, cx);
                    })),
            )
        });

        bar.child(
            h_flex()
                .flex_1()
                .min_w_0()
                .items_center()
                .gap_3()
                .children(expand_toggle)
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
                // EXP-282: this wrapper must be a FLEX container — a plain
                // `div()` is display:block, which stretched the interactive
                // strip to 100% width and swallowed every drag on the bar.
                .when_some(strip, |bar, strip| {
                    bar.child(h_flex().flex_1().min_w_0().child(interactive(strip)))
                }),
        )
    }
}
