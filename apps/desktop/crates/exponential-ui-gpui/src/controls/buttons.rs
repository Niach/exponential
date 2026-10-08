//! Buttons: the round glass icon button, the ghost icon button, the text
//! button, the switch, and the custom-variant alpha compensation they share
//! with the pill.

use gpui::{div, px, App, Div, ElementId, Hsla, InteractiveElement as _, ParentElement as _};
use gpui::{SharedString, Stateful, Styled};
use gpui_component::{ActiveTheme as _, Icon, Sizable as _, Size};

use super::sizing::{WebControl as _, CTL_MD_H};
use crate::chrome::Chrome;

/// How much of a `ButtonVariant::Custom` colour actually reaches the screen.
///
/// gpui-component paints a custom variant's rest/`outline` background as
/// `color.mix_oklab(transparent, 0.2)` (`button.rs`, `bg_color` /
/// `outline_background`), and that mix weights SELF by the factor —
/// `a = self.a * factor + other.a * (1 - factor)` — so a custom colour is
/// painted at a FIFTH of its alpha. Handing it `fill_card` (white 6%)
/// directly paints white ~1.2%, i.e. a Button pill would be all but
/// invisible beside the `glass_pill` `Div` next to it.
pub const CUSTOM_VARIANT_ALPHA_FACTOR: f32 = 0.2;

/// Pre-divide a glass fill so [`CUSTOM_VARIANT_ALPHA_FACTOR`] mixes it back
/// to the token: a Button pill and a `Div` pill then paint the SAME surface.
/// Only the alpha moves — `mix_oklab` premultiplies in Oklab and
/// un-premultiplies by the result alpha, so mixing a colour with transparent
/// leaves hue/saturation/lightness untouched.
pub fn custom_variant_fill(fill: Hsla) -> Hsla {
    Hsla {
        a: (fill.a / CUSTOM_VARIANT_ALPHA_FACTOR).min(1.),
        ..fill
    }
}

/// EXP-686: the web's round glass play button — a 32px circle filled with the
/// glass card fill + stroke, its glyph at 70% foreground, hovering to the
/// active fill. The row actions that used to be `.web_sm().rounded(999)`
/// outline buttons (the action row's ▶ Run, the machine row's ▶ Start coding)
/// all take this shape, so both lists match the web/mobile play affordance.
///
/// It rides a `ButtonCustomVariant`, not a `ghost` base with a caller
/// refinement: the built-in variants paint their own hover fill from the
/// interactivity layer, which is applied AFTER `refine_style` and would win
/// over any glass tokens set here (and a second `.hover()` on the button
/// trips gpui's "hover style already set" assertion). The custom variant owns
/// bg/hover/active; only the stroke and the pill radius are refinements.
pub fn glass_icon_button(
    id: impl Into<ElementId>,
    icon: Icon,
    cx: &App,
) -> gpui_component::button::Button {
    use gpui_component::button::{ButtonCustomVariant, ButtonVariants as _};
    let chrome = Chrome::global(cx);
    let foreground = cx.theme().foreground;
    // Custom variants paint at a fifth of the handed-in alpha (EXP-698,
    // [`custom_variant_fill`]): pre-divide so the circle lands on the same
    // card/active fills as the pills beside it.
    let variant = ButtonCustomVariant::new(cx)
        .color(custom_variant_fill(chrome.fill_card))
        .hover(custom_variant_fill(chrome.fill_active))
        .active(custom_variant_fill(chrome.fill_active))
        .foreground(foreground.opacity(0.7));
    gpui_component::button::Button::new(id)
        .custom(variant)
        .web_icon_sm()
        .icon(icon)
        .border_1()
        .border_color(chrome.stroke_card)
}

/// EXP-862 — the ONE GHOST icon button, the web `<Button variant="ghost"
/// size="icon-sm">` twin (iOS `GhostIconButton`, Android `CircleIconButton(
/// borderless = true)`).
///
/// A circle says "primary action" — play/start, send, the rail's New issue
/// and Search, the mobile FAB, an "+" add. Everything else that is a glyph
/// on a row (the ⋯ menu, close, the folder/file-list toggles, a fold
/// chevron, trash/remove, a refresh) is THIS: a 32px square, no fill and no
/// stroke at rest, the glyph at 70% foreground, and the flat row's own hover
/// wash (`list_hover` == `glass.fillRow`, EXP-811) under the pointer.
///
/// Signature-identical to [`glass_icon_button`] on purpose — the sites that
/// stop being circles swap the ONE call and keep every chained
/// `.tooltip()` / `.on_click()` / `.dropdown_menu()` they already had.
///
/// Like its glass sibling the paint rides a `ButtonCustomVariant` rather than
/// a `ghost` base: the built-in variants paint their own hover fill after
/// `refine_style`, and a second `.hover()` trips gpui's "hover style already
/// set" assertion. A custom variant with a transparent colour also paints a
/// TRANSPARENT border upstream (`ButtonVariant::border_color`), which is the
/// borderless part of the recipe — nothing here needs to un-set a stroke.
pub fn ghost_icon_button(
    id: impl Into<ElementId>,
    icon: Icon,
    cx: &App,
) -> gpui_component::button::Button {
    use gpui_component::button::{ButtonCustomVariant, ButtonVariants as _};
    let radius = Chrome::global(cx).radius_md;
    let theme = cx.theme();
    let foreground = theme.foreground;
    // The hover colour is handed to the painter UNMIXED (`ButtonVariant::
    // hovered` reads `colors.hover` straight, unlike the rest-state colour
    // which goes through `mix_oklab(transparent, 0.2)`), so no
    // `custom_variant_fill` pre-division here: this IS the row wash.
    let variant = ButtonCustomVariant::new(cx)
        .color(gpui::transparent_black())
        .hover(theme.list_hover)
        .active(theme.list_hover)
        .foreground(foreground.opacity(0.7));
    gpui_component::button::Button::new(id)
        .custom(variant)
        .with_size(Size::Small)
        .size(px(CTL_MD_H))
        .rounded(px(radius))
        .cursor_pointer()
        .icon(icon)
}

/// The two text-button variants (EXP-963, web `Button variant="text" |
/// "link"` at `size="inline"`): what happens when the words are pressed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextButtonVariant {
    /// Toggles something IN PLACE (a fold's Show more / Show less): muted,
    /// brightens under the pointer, never underlines.
    Text,
    /// GOES somewhere (a session band's "Continues in a newer run"): the
    /// primary colour, underlined under the pointer.
    Link,
}

/// EXP-963 — a control made of WORDS: 12px, no box, no height of its own,
/// sitting in the run of muted text around it. The caller chains
/// `.on_click`. Anything that wants a box is the pill or a `Button`.
pub fn text_button(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    variant: TextButtonVariant,
    cx: &App,
) -> Stateful<Div> {
    let theme = cx.theme();
    let button = div()
        .id(id)
        .flex()
        .flex_row()
        .items_center()
        .gap_1()
        .text_xs()
        .cursor_pointer()
        .child(label.into());
    match variant {
        TextButtonVariant::Text => {
            let foreground = theme.foreground;
            button
                .text_color(theme.muted_foreground)
                .hover(move |style| style.text_color(foreground))
        }
        TextButtonVariant::Link => button
            .text_color(theme.primary)
            .hover(|style| style.text_decoration_1()),
    }
}

/// EXP-862 — the ONE switch: gpui-component's `Switch` with the web's pointer
/// cursor. gpui-component defaults every control to `cursor_default`; a toggle
/// the user clicks points on hover on all four clients, and a dozen call sites
/// each remembering to say so is how half of them forgot.
///
/// The IDE forbids constructing the upstream switch anywhere but here
/// (`only_controls_constructs_switches`) — construct through this.
pub fn web_switch(id: impl Into<ElementId>) -> gpui_component::switch::Switch {
    gpui_component::switch::Switch::new(id).cursor_pointer()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// EXP-698: a Button pill and a `Div` pill must paint the SAME surface.
    /// gpui-component runs a custom variant's colour through
    /// `mix_oklab(transparent, 0.2)` before painting it, so the pre-division
    /// in [`custom_variant_fill`] has to mix back to the token exactly —
    /// asserted with the crate's OWN mix, so an upstream change to either the
    /// factor or the mix semantics fails here instead of on screen.
    #[test]
    fn custom_variant_fills_mix_back_to_the_chrome_fills() {
        use gpui_component::theme::Colorize as _;
        let chrome = Chrome::default();
        let transparent = gpui::transparent_black();
        for want in [chrome.fill_card, chrome.fill_active] {
            let painted =
                custom_variant_fill(want).mix_oklab(transparent, CUSTOM_VARIANT_ALPHA_FACTOR);
            assert!((painted.a - want.a).abs() < 0.001, "{painted:?} vs {want:?}");
            assert!((painted.l - want.l).abs() < 0.01, "{painted:?} vs {want:?}");
        }
    }
}
