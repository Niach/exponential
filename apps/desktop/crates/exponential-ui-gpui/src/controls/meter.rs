//! The meter (EXP-909) and the context ring (EXP-877).

use gpui::{div, px, App, Div, Hsla, ParentElement as _, SharedString, Styled};
use gpui_component::{
    button::{Button, ButtonVariants as _},
    progress::ProgressCircle,
};

use super::sizing::WebControl as _;
use crate::chrome::Chrome;

/// EXP-909 — THE bar. One primitive for every meter (the IDE's usage
/// windows, the context block, the mini line), so a tone or a radius
/// changes in one place; `height` is the only geometry a caller varies and
/// `fill` the tone (the IDE maps its usage severity onto it). The track is
/// the chrome's `stroke_strong` (EXP-698: not a dimmed chrome border). The
/// web twin is the `@exp/ui` `Meter`, iOS `AgentUsageTrack`, Android
/// `UsageTrack`.
pub fn meter(percent: u8, fill: Hsla, height: f32, cx: &App) -> Div {
    div()
        .w_full()
        .h(px(height))
        .rounded_full()
        .bg(Chrome::global(cx).stroke_strong)
        .child(
            div()
                .h_full()
                .rounded_full()
                .w(gpui::relative(percent as f32 / 100.))
                .bg(fill),
        )
}

/// EXP-877 — the CONTEXT RING: a 16px radial meter of how full a run's
/// context window is, in the caller's `tone`. It is the trigger for a usage
/// popover, so it is a ghost icon-xs `Button`, and it says the numbers in its
/// `tooltip` rather than beside itself: a percentage printed next to a meter
/// of the same percentage is the same fact twice.
///
/// A run with no window to report renders NO ring at all — that is the
/// caller's check.
pub fn context_ring(
    id: impl Into<gpui::ElementId>,
    percent: u8,
    tone: Hsla,
    tooltip: impl Into<SharedString>,
    _cx: &App,
) -> Button {
    let tooltip: SharedString = tooltip.into();
    Button::new(id)
        .ghost()
        .web_icon_xs()
        .icon(
            // `Size::Size(s)` renders at `s * 0.75`, so the ring is asked for
            // the size that lands ON 16px.
            gpui_component::Sizable::with_size(
                ProgressCircle::new("session-context-ring")
                    .value(f32::from(percent))
                    .color(tone),
                px(16. / 0.75),
            ),
        )
        .tooltip(tooltip)
}
