//! Edge fades (EXP-1162/EXP-1191): the paint-only strips a scrolling body
//! wears so content slides away under a bar instead of being cut at a
//! hairline. gpui cannot blur what is behind a view, so these are the scrim
//! fade alone. The FILL is the host's (the IDE samples its window ramp under
//! the panel wash); each strip is absolute, so the caller's box must be
//! `relative()`, and has no hitbox, so clicks and the wheel reach the
//! content under it.

use gpui::{div, px, Div, Hsla, ParentElement as _, Styled};

/// The TOP edge strip (contract `detail-chrome.json` `edgeTop`): overlaid on
/// the top of a scrolling body, it fades from `fill` (opaque) to nothing
/// over `height` px.
pub fn edge_fade_top(fill: Hsla, height: f32) -> Div {
    div()
        .absolute()
        .top_0()
        .left_0()
        .right_0()
        .h(px(height))
        .bg(gpui::linear_gradient(
            180.,
            gpui::linear_color_stop(fill.opacity(1.), 0.),
            gpui::linear_color_stop(fill.opacity(0.), 1.),
        ))
}

/// The BOTTOM edge strip (`edgeBottom`): [`edge_fade_top`] mirrored, fading
/// upwards from the box's bottom edge to `fill`.
pub fn edge_fade_bottom(fill: Hsla, height: f32) -> Div {
    div()
        .absolute()
        .bottom_0()
        .left_0()
        .right_0()
        .h(px(height))
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
/// while the top stays a whisper.
pub fn edge_fade_bottom_soft(fill: Hsla, height: f32) -> Div {
    div()
        .absolute()
        .bottom_0()
        .left_0()
        .right_0()
        .h(px(height))
        .bg(gpui::linear_gradient(
            180.,
            gpui::linear_color_stop(fill.opacity(0.), 0.),
            gpui::linear_color_stop(fill.opacity(0.7), 1.),
        ))
        .child(
            div()
                .absolute()
                .bottom_0()
                .left_0()
                .right_0()
                .h(px(height / 2.))
                .bg(gpui::linear_gradient(
                    180.,
                    gpui::linear_color_stop(fill.opacity(0.), 0.),
                    gpui::linear_color_stop(fill, 1.),
                )),
        )
}
