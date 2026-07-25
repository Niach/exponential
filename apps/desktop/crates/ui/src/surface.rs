//! Glass surface recipes (EXP-269) — the GlassTheme.swift / Glass.kt parity
//! layer for the desktop. gpui has no in-scene backdrop blur, so glass is the
//! Android approximation: white-alpha fills + hairline strokes over the page
//! gradient (`theme::background_gradient()`), radii from the token ladder
//! (row 10 / section 12 / card 16). Strokes are 1px on purpose — fractional
//! hairlines vanish on 1x-scale displays.

use gpui::{px, Div, Styled};
use gpui_component::v_flex;
use theme::tokens as t;

/// Card surface: radius 16, white 6% fill, white 10% hairline (mobile
/// `GlassCard`). Layout (width/padding/gap) is the caller's job.
pub(crate) fn glass_card() -> Div {
    v_flex()
        .rounded(px(t::radius::XL))
        .border_1()
        .border_color(t::glass::STROKE_CARD.to_hsla())
        .bg(t::glass::FILL_CARD.to_hsla())
}

/// Capsule glass-button treatment for any element (mobile `GlassButton`):
/// full rounding, white 6% fill / 10% stroke — 15% / 20% when active.
pub(crate) fn glass_pill<T: Styled>(el: T, active: bool) -> T {
    let (fill, stroke) = if active {
        (t::glass::FILL_ACTIVE, t::glass::STROKE_ACTIVE)
    } else {
        (t::glass::FILL_CARD, t::glass::STROKE_CARD)
    };
    el.rounded_full()
        .border_1()
        .border_color(stroke.to_hsla())
        .bg(fill.to_hsla())
}
