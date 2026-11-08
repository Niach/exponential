//! Buttons: the round glass icon button, the ghost icon button, the text
//! button, the switch, and the custom-variant alpha compensation they share
//! with the pill.

use gpui::{div, px, App, Div, ElementId, Hsla, InteractiveElement as _, IntoElement as _, ParentElement as _};
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
    // a11y: named by the caller (the tooltip or `accessibility_label` it chains).
    gpui_component::button::Button::new(id)
        .custom(variant)
        .web_icon_sm()
        .icon(icon)
        .border_1()
        .border_color(chrome.stroke_card)
}

/// S-3 (voice-call.json, styleguide `call-controls`): what a round call
/// button says.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CallTone {
    /// Mute / the speaker at rest: the glass circle.
    Neutral,
    /// A toggled mute: the inverted neutral fill (foreground circle,
    /// background glyph).
    Toggled,
    /// End call / Cancel: the red `destructive` circle.
    Destructive,
}

/// S-3: the round call buttons' two sizes: the call panel's 56 px circle
/// with a 24 px glyph (web `icon-lg`), the floating pill's 32 px one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CallSize {
    Lg,
    Sm,
}

impl CallSize {
    /// The circle's edge.
    pub fn edge(self) -> f32 {
        match self {
            CallSize::Lg => 56.,
            CallSize::Sm => CTL_MD_H,
        }
    }
}

/// S-3 — the ROUND ICON-ONLY CALL BUTTON (`call-controls`, eUI
/// `CallButton`): [`glass_icon_button`]'s circle with a tone and a size.
/// Icon only: the caller chains the label as `.tooltip()` (Cancel's label
/// shows under it, drawn by the caller).
pub fn call_button(
    id: impl Into<ElementId>,
    icon: Icon,
    tone: CallTone,
    size: CallSize,
    cx: &App,
) -> gpui_component::button::Button {
    use gpui_component::button::{ButtonCustomVariant, ButtonVariants as _};
    use gpui_component::theme::Colorize as _;
    // A custom variant paints its rest colour at a fifth of its alpha
    // ([`CUSTOM_VARIANT_ALPHA_FACTOR`]) and the built-in `danger` is the
    // web's tint: the SOLID red circle rides a transparent custom variant
    // (its hover and press, handed over unmixed) under a refined rest fill.
    // Neutral = web's `bg-secondary`, toggled = the inverted neutral fill
    // (`bg-foreground`, EXP-1676: never the off-white `primary`).
    let solid = |id: ElementId, fill: gpui::Hsla, glyph: gpui::Hsla, icon: Icon, cx: &App| {
        let variant = ButtonCustomVariant::new(cx)
            .color(gpui::transparent_black())
            .hover(fill.opacity(0.9))
            .active(fill.opacity(0.8))
            .foreground(glyph);
        // a11y: named by the caller (the tooltip or `accessibility_label` it chains).
        gpui_component::button::Button::new(id).custom(variant).bg(fill).icon(icon)
    };
    let button = match tone {
        CallTone::Neutral => solid(id.into(), cx.theme().secondary, cx.theme().foreground, icon, cx),
        CallTone::Toggled => solid(id.into(), cx.theme().foreground, cx.theme().background, icon, cx),
        CallTone::Destructive => {
            let red = cx.theme().danger;
            let variant = ButtonCustomVariant::new(cx)
                .color(gpui::transparent_black())
                .hover(red.darken(0.08))
                .active(red.darken(0.16))
                .foreground(gpui::white());
            // a11y: named by the caller (the tooltip or `accessibility_label` it chains).
            gpui_component::button::Button::new(id).custom(variant).bg(red).icon(icon)
        }
    };
    let button = match size {
        CallSize::Lg => button.with_size(Size::Large),
        CallSize::Sm => button.with_size(Size::Small),
    };
    button.size(px(size.edge())).rounded_full().cursor_pointer()
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
    // a11y: named by the caller (the tooltip or `accessibility_label` it chains).
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
    use gpui::StatefulInteractiveElement as _;
    let theme = cx.theme();
    let label = label.into();
    // #EXP-1488: a Button (a Link when it goes somewhere) named by its words.
    let role = match variant {
        TextButtonVariant::Text => gpui::Role::Button,
        TextButtonVariant::Link => gpui::Role::Link,
    };
    let button = div()
        .id(id)
        .role(role)
        .aria_label(label.clone())
        .flex()
        .flex_row()
        .items_center()
        .gap_1()
        .text_xs()
        .cursor_pointer()
        .child(label);
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

/// EXP-862 — the ONE switch, with the web's pointer cursor. gpui-component
/// defaults every control to `cursor_default`; a toggle the user clicks
/// points on hover on all four clients, and a dozen call sites each
/// remembering to say so is how half of them forgot.
///
/// EXP-1557: it is [`WebSwitch`] now, not gpui-component's `Switch` — that
/// one paints ONE thumb token in both states, so the off switch wore a dark
/// thumb on a dark track where every other client draws a white one.
///
/// The IDE forbids constructing the upstream switch anywhere but here
/// (`only_controls_constructs_switches`) — construct through this.
pub fn web_switch(id: impl Into<ElementId>) -> WebSwitch {
    WebSwitch::new(id).cursor_pointer()
}

/// EXP-1557 — THE switch, the eUI `switch.ts` recipe (web `SWITCH_CLASSES`,
/// dark): ON = a `primary` track under a `primary-foreground` (dark) thumb,
/// OFF = a `glass-active` track under a `foreground` (white) thumb, disabled
/// = the whole control at 50%. The behaviour (click, keyboard, focus, the
/// accessibility role) is gpui-base's `Switch`, the same primitive
/// gpui-component's switch is built on; geometry and the 150 ms thumb slide
/// are gpui-component's (36×20 track, 16px thumb; 28×16 / 12px when small).
#[derive(gpui::IntoElement)]
pub struct WebSwitch {
    id: ElementId,
    style: gpui::StyleRefinement,
    checked: bool,
    disabled: bool,
    size: Size,
    on_click: Option<std::rc::Rc<dyn Fn(&bool, &mut gpui::Window, &mut App)>>,
}

impl WebSwitch {
    fn new(id: impl Into<ElementId>) -> Self {
        Self {
            id: id.into(),
            style: gpui::StyleRefinement::default(),
            checked: false,
            disabled: false,
            size: Size::Medium,
            on_click: None,
        }
    }

    /// The checked state.
    pub fn checked(mut self, checked: bool) -> Self {
        self.checked = checked;
        self
    }

    /// Called with the NEXT checked value when the user toggles it.
    pub fn on_click(
        mut self,
        handler: impl Fn(&bool, &mut gpui::Window, &mut App) + 'static,
    ) -> Self {
        self.on_click = Some(std::rc::Rc::new(handler));
        self
    }
}

impl Styled for WebSwitch {
    fn style(&mut self) -> &mut gpui::StyleRefinement {
        &mut self.style
    }
}

impl gpui_component::Sizable for WebSwitch {
    fn with_size(mut self, size: impl Into<Size>) -> Self {
        self.size = size.into();
        self
    }
}

impl gpui_component::Disableable for WebSwitch {
    fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
}

impl gpui::RenderOnce for WebSwitch {
    fn render(self, window: &mut gpui::Window, cx: &mut App) -> impl gpui::IntoElement {
        use gpui::prelude::FluentBuilder as _;
        use gpui::AnimationExt as _;
        use gpui_component::StyledExt as _;

        const SLIDE: std::time::Duration = std::time::Duration::from_millis(150);
        let checked = self.checked;
        // What the thumb last settled on: a change animates the slide.
        let settled = window.use_keyed_state(self.id.clone(), cx, |_, _| checked);
        let theme = cx.theme();
        let (track, thumb) = if checked {
            (theme.primary, theme.primary_foreground)
        } else {
            (Chrome::global(cx).fill_active, theme.foreground)
        };
        let (track_w, track_h, knob) = match self.size {
            Size::XSmall | Size::Small => (px(28.), px(16.), px(12.)),
            _ => (px(36.), px(20.), px(16.)),
        };
        let inset = px(2.);
        let max_x = track_w - knob - inset * 2.;
        let thumb_el = gpui_base::SwitchThumb::new(checked)
            .disabled(self.disabled)
            .rounded_full()
            .size(knob)
            .bg(thumb);
        let thumb_el = if !self.disabled && *settled.read(cx) != checked {
            let settle = settled.clone();
            cx.spawn(async move |cx| {
                cx.background_executor().timer(SLIDE).await;
                _ = settle.update(cx, |state, _| *state = checked);
            })
            .detach();
            thumb_el
                .with_animation(
                    ElementId::NamedInteger("move".into(), checked as u64),
                    gpui::Animation::new(SLIDE),
                    move |thumb, delta| {
                        thumb.left(if checked { max_x * delta } else { max_x - max_x * delta })
                    },
                )
                .into_any_element()
        } else {
            thumb_el
                .left(if checked { max_x } else { px(0.) })
                .into_any_element()
        };
        let on_click = self.on_click;
        div()
            .refine_style(&self.style)
            .when(self.disabled, |this| this.opacity(0.5))
            .child(
                gpui_base::Switch::new(self.id.clone())
                    .checked(checked)
                    .disabled(self.disabled)
                    .when_some(on_click, |this, on_click| {
                        this.on_change(move |next, _, window, cx| {
                            _ = settled.update(cx, |state, _| *state = checked);
                            on_click(&next, window, cx);
                        })
                    })
                    .h_flex()
                    .items_start()
                    .child(
                        gpui_base::SwitchTrack::new((self.id.clone(), "track"))
                            .checked(checked)
                            .disabled(self.disabled)
                            .w(track_w)
                            .h(track_h)
                            .rounded_full()
                            .flex()
                            .items_center()
                            .border(inset)
                            .border_color(gpui::transparent_black())
                            .bg(track)
                            .child(thumb_el),
                    ),
            )
    }
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
