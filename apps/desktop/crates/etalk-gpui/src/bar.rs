//! THE bar's parts (the bulk bar's glass capsule, `glass_bar`): its item,
//! its bold value and the box arithmetic that sizes a window to it before
//! the window exists. The voice overlay's capsule ([`crate::voice_overlay`])
//! and Exponential's computer-use banner both stand on them.

use gpui::{div, px, App, Div, Font, FontWeight, ParentElement as _, Pixels, SharedString, Styled as _, TextRun, WindowTextSystem};
use gpui_component::ActiveTheme as _;

/// The bar's boxes around its words. `glass_bar`'s are in rems (2.5, 2 and
/// 1 spacing steps: 0.625 rem sides, 0.5 rem top and bottom, 0.25 rem
/// gaps) inside a 1 px stroke; the rest in logical pixels ([`item`]: 32
/// tall, 8 px sides, 6 px gaps; the label 4 px sides).
pub const BAR_SIDE_REMS: f32 = 0.625;
pub const BAR_TOP_REMS: f32 = 0.5;
pub const BAR_GAP_REMS: f32 = 0.25;
pub const BAR_STROKE: f32 = 1.;
pub const ITEM_SIDE: f32 = 8.;
pub const LABEL_SIDE: f32 = 4.;

/// The air the window gives the bar beyond its sum: the toolkit snaps
/// every box to whole device pixels (fractional rem sizes, fractional
/// scale factors) and takes each text whole pixels up, so the drawn bar
/// can be a few device pixels wider than the sum of its parts. The bar
/// fills the window and the air lands inside its padding, unseen; without
/// it the window was 1 px narrower than the bar for some labels and
/// ellipsized them.
pub const BAR_SLACK: f32 = 4.;

/// The bar's rem: the theme's font size, as every `Root` window sets it.
pub fn rem_size(cx: &App) -> Pixels {
    cx.theme().font_size
}

/// One line's shaped width in the theme's font, the path the text element
/// takes, without a window (the bar's does not exist yet); whole pixels
/// up, as the text element takes it.
pub fn shaped_width(text: &str, size: Pixels, weight: FontWeight, cx: &App) -> f32 {
    if text.is_empty() {
        return 0.;
    }
    let font = Font { weight, ..gpui::font(cx.theme().font_family.clone()) };
    let run = TextRun { len: text.len(), font, color: gpui::black(), background_color: None, underline: None, strikethrough: None };
    let layout = WindowTextSystem::new(cx.text_system().clone()).layout_line(text, size, &[run], None);
    f32::from(layout.width).ceil()
}

/// One item of THE bar (the glyph and the words, muted; the bulk bar's: 32
/// tall, 8 px sides): also the voice overlay's line (`voice_overlay::capsule`,
/// NEW-77).
pub fn item(cx: &App) -> Div {
    div().flex().flex_shrink_0().items_center().gap(px(6.)).h(px(32.)).px(px(8.)).text_sm().text_color(cx.theme().muted_foreground)
}

/// THE bar's bold value (the session's label; the voice overlay's state).
pub fn value(text: impl Into<SharedString>, cx: &App) -> Div {
    div()
        .min_w_0()
        .px(px(LABEL_SIDE))
        .overflow_hidden()
        .whitespace_nowrap()
        .text_ellipsis()
        .text_sm()
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(cx.theme().foreground)
        .child(text.into())
}
