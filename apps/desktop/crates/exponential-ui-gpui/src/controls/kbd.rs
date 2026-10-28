//! S-3: THE keycap outside a surface — the catalog's `Kbd` (the web
//! `KBD_CLASSES`, `@exponential_app/ui/web/primitives/kbd`): one key, or
//! [`kbd_group`] for a chord. Inside a surface the painter draws the `Kbd`
//! macro; IDE chrome (Settings → Voice's push-to-talk binding) draws this.

use gpui::{div, px, App, Div, FontWeight, IntoElement, ParentElement as _, SharedString, Styled};
use gpui_component::ActiveTheme as _;

use crate::chrome::Chrome;

/// `h-5` / `min-w-5`: the catalog's `$control.badge` rung.
pub const KBD_H: f32 = 20.;
/// `px-1`: `$spacing.xs`.
pub const KBD_PAD_X: f32 = 4.;
/// `gap-1`: between the caps of a [`kbd_group`].
pub const KBD_GAP: f32 = 4.;

/// One keycap: 20 px tall, at least 20 px wide, 4 px side padding, the
/// `radius.sm` corner, the muted fill, 12 px medium muted text.
pub fn kbd(label: impl Into<SharedString>, cx: &App) -> Div {
    let theme = cx.theme();
    div()
        .flex()
        .flex_row()
        .flex_shrink_0()
        .items_center()
        .justify_center()
        .gap(px(KBD_GAP))
        .h(px(KBD_H))
        .min_w(px(KBD_H))
        .px(px(KBD_PAD_X))
        .rounded(px(Chrome::global(cx).radius_sm))
        .bg(theme.muted)
        .text_size(px(12.))
        .line_height(px(16.))
        .font_weight(FontWeight::MEDIUM)
        .text_color(theme.muted_foreground)
        .child(label.into())
}

/// `KbdGroup`: the caps in a row, 4 px apart.
pub fn kbd_group(caps: impl IntoIterator<Item = impl IntoElement>) -> Div {
    div().flex().flex_row().items_center().gap(px(KBD_GAP)).children(caps)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The cap IS the catalog's `Kbd` macro: `$control.badge` tall,
    /// `$spacing.xs` side padding (the exponential theme's values).
    #[test]
    fn the_keycap_is_the_catalog_kbd() {
        let theme: serde_json::Value =
            serde_json::from_str(include_str!("../../../../../../packages/exponential-ui/themes/exponential.theme.json"))
                .expect("the theme parses");
        let tokens = &theme["tokens"];
        assert_eq!(tokens["control"]["badge"].as_f64(), Some(KBD_H as f64));
        assert_eq!(tokens["spacing"]["xs"].as_f64(), Some(KBD_PAD_X as f64));
        assert_eq!(tokens["spacing"]["xs"].as_f64(), Some(KBD_GAP as f64));
        assert_eq!(tokens["radius"]["sm"].as_f64(), Some(Chrome::default().radius_sm as f64));
    }
}
