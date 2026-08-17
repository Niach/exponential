//! Glass surface recipes (EXP-269) — the GlassTheme.swift / Glass.kt parity
//! layer for the desktop. gpui has no in-scene backdrop blur, so glass is the
//! Android approximation: white-alpha fills + hairline strokes over the page
//! gradient (`theme::background_gradient()`), radii from the token ladder
//! (row 10 / section 12 / card 16). Strokes are 1px on purpose — fractional
//! hairlines vanish on 1x-scale displays.

use gpui::{div, px, App, Div, InteractiveElement as _, StyleRefinement, Styled};
use gpui_component::{text::TextViewStyle, v_flex, ActiveTheme as _};
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

/// Rounded tab-chip base for the hand-rolled tab strips (EXP-277: center
/// screens + terminal dock). gpui-component's `TabVariant`s are either square
/// (`Tab`, plus a non-removable strip-wide bottom border) or hardwired to
/// opaque fills (`Segmented`), so the strips draw their own chips: soft
/// radius, transparent resting state, glass row fill on hover, glass active
/// fill when selected. Content, id, and handlers are the caller's job.
pub(crate) fn tab_chip(selected: bool, cx: &App) -> Div {
    let theme = cx.theme();
    let chip = div()
        // EXP-525: web tab-pill metrics (`h-7 rounded-full px-3 text-xs`
        // scale) — the old 24px/px_2 chips read too small next to the web.
        .h(px(26.))
        .px_2p5()
        .flex()
        .flex_none()
        .items_center()
        .gap_1p5()
        .rounded(theme.radius)
        .cursor_pointer()
        .text_sm();
    if selected {
        chip.bg(theme.tab_active).text_color(theme.tab_active_foreground)
    } else {
        chip.text_color(theme.tab_foreground)
            .hover(|style| style.bg(theme.list_hover))
    }
}

/// A NON-interactive chip (EXP-417): the glass fill + radius the interactive
/// `pickers::chip_button` triggers read as, sized to its content. The detail
/// headers' read-only properties (Origin, a single-board team's Board, an
/// action's repository/icon when it can't be changed) wear it.
pub(crate) fn glass_chip() -> Div {
    div()
        .flex()
        .items_center()
        .flex_shrink_0()
        .gap_1p5()
        .px_2()
        .py_1()
        .rounded(px(t::radius::SM))
        .bg(t::glass::FILL_CARD.to_hsla())
        .text_xs()
        .font_weight(gpui::FontWeight::MEDIUM)
}

/// Shared markdown `TextView` style (EXP-282): code blocks get a glass
/// section fill instead of the component default opaque `tokens.muted`
/// panel. Everything else stays at the component defaults the call sites
/// already rendered with.
pub(crate) fn markdown_style() -> TextViewStyle {
    let code_block = StyleRefinement::default()
        .bg(t::glass::FILL_SECTION.to_hsla())
        .rounded(px(t::radius::MD));
    TextViewStyle::default().code_block(code_block)
}

/// Markdown `TextView` style for the file viewer (EXP-282): the whole file is
/// fenced as one code block, so the block chrome disappears entirely — no
/// fill, no padding, no radius — and the code sits directly on the gradient.
pub(crate) fn bare_code_markdown_style() -> TextViewStyle {
    let code_block = StyleRefinement::default()
        .bg(gpui::transparent_black())
        .p_0()
        .rounded_none();
    TextViewStyle::default().code_block(code_block)
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
