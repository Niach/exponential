//! Edge fades (EXP-1162/EXP-1191): the paint-only strips a scrolling body
//! wears so content slides away under a bar instead of being cut at a
//! hairline. gpui cannot blur what is behind a view, so these are the scrim
//! fade alone. The FILL is the host's (the IDE samples its window ramp under
//! the panel wash); each strip is absolute, so the caller's box must be
//! `relative()`, and has no hitbox, so clicks and the wheel reach the
//! content under it.
//!
//! EXP-1256: a strip stays INSIDE the card its body lives in. `inset` keeps
//! it off the card's side hairlines (a strip painted over the border cut a
//! gap into it), and `radius` rounds the strip's OUTER corners (the top
//! strip's top, a bottom strip's bottom) to the card's INNER radius where the
//! strip runs flush to the card's edge, so no square corner pokes over the
//! card's rounded one. A strip that ends mid-card (under a bar, above a
//! composer) passes `0.`. gpui clips to rectangles only, so every layer
//! rounds itself rather than leaning on an `overflow_hidden` parent.

use gpui::{div, px, Div, Hsla, ParentElement as _, Styled};

/// The shared box of every strip: absolute, `height` tall, `inset` off both
/// sides.
fn strip(height: f32, inset: f32) -> Div {
    div()
        .absolute()
        .left(px(inset))
        .right(px(inset))
        .h(px(height))
}

/// The TOP edge strip (contract `detail-chrome.json` `edgeTop`): overlaid on
/// the top of a scrolling body, it fades from `fill` (opaque) to nothing
/// over `height` px. `inset` / `radius`: the card clip (module docs).
pub fn edge_fade_top(fill: Hsla, height: f32, inset: f32, radius: f32) -> Div {
    strip(height, inset)
        .top_0()
        .rounded_tl(px(radius))
        .rounded_tr(px(radius))
        .bg(gpui::linear_gradient(
            180.,
            gpui::linear_color_stop(fill.opacity(1.), 0.),
            gpui::linear_color_stop(fill.opacity(0.), 1.),
        ))
}

/// The BOTTOM edge strip (`edgeBottom`): [`edge_fade_top`] mirrored, fading
/// upwards from the box's bottom edge to `fill`.
pub fn edge_fade_bottom(fill: Hsla, height: f32, inset: f32, radius: f32) -> Div {
    strip(height, inset)
        .bottom_0()
        .rounded_bl(px(radius))
        .rounded_br(px(radius))
        .bg(gpui::linear_gradient(
            180.,
            gpui::linear_color_stop(fill.opacity(0.), 0.),
            gpui::linear_color_stop(fill, 1.),
        ))
}

/// EXP-1191 — the SOFT bottom edge (the IDE's run-transcript composer edge):
/// the last lines fade softly into `fill`. A clean vertical ramp from
/// transparent, NO hairline, band or shadow. gpui's gradients take two
/// stops, so the soft (eased) curve is two layers: the full-height ramp to
/// 70 %, and a second over its lower half that lands the bottom fully opaque
/// while the top stays a whisper. Both layers wear the card clip.
pub fn edge_fade_bottom_soft(fill: Hsla, height: f32, inset: f32, radius: f32) -> Div {
    strip(height, inset)
        .bottom_0()
        .rounded_bl(px(radius))
        .rounded_br(px(radius))
        .bg(gpui::linear_gradient(
            180.,
            gpui::linear_color_stop(fill.opacity(0.), 0.),
            gpui::linear_color_stop(fill.opacity(0.7), 1.),
        ))
        .child(
            // Inside the outer strip, which already sits `inset` in.
            strip(height / 2., 0.)
                .bottom_0()
                .rounded_bl(px(radius))
                .rounded_br(px(radius))
                .bg(gpui::linear_gradient(
                    180.,
                    gpui::linear_color_stop(fill.opacity(0.), 0.),
                    gpui::linear_color_stop(fill, 1.),
                )),
        )
}
