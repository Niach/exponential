//! Web-parity control metrics (EXP-525) — the shadcn sizing layer.
//!
//! gpui-component sizes its controls in rem off this app's root font size
//! ([`theme::FONT_SIZE_PX`], 14px since EXP-723), which still lands every
//! button/input under the web's shadcn boxes (web Button
//! default h-9/px-4, sm h-8/px-3 rounded-full, xs h-6/px-2 rounded-full;
//! inputs h-9). These helpers are pure `Styled` refinements — gpui-component
//! applies caller refinements after its own base styles (`refine_style` runs
//! last and is replayed inside the selected/disabled state closures), so they
//! win without forking the component. `cursor_pointer` rides the same
//! refinement: gpui-component buttons default to `cursor_default`, the web
//! (and every native toolkit convention we mirror) points on hover.

use gpui::{
    div, prelude::FluentBuilder as _, px, App, Div, Entity, Focusable as _, FontWeight,
    InteractiveElement as _, ParentElement as _, SharedString, Styled, Window,
};
use gpui_component::{
    input::{Input, InputState, TextareaState},
    menu::PopupMenuItem,
    v_flex, ActiveTheme as _, Icon, Sizable, Size,
};
use theme::tokens as t;

/// EXP-720: the ONE text-field recipe (styleguide `text-field`, web
/// `components/ui/input.tsx`): card fill under the card stroke, and focus
/// swaps the STROKE to `glass::STROKE_ACTIVE` — no ring. gpui-component's
/// `Input` paints its focused state as `theme.ring` (the neutral RING token
/// the web keeps for BUTTON focus-visible halos) plus a halo child, which is
/// why an autofocused dialog field used to wear a bright grey outline no other
/// field carried. The theme has no hook for the focused input stroke alone
/// (`ring` also drives every other focus ring), so the swap happens here:
/// `focus_bordered(false)` mutes the component's own focused style and the
/// active stroke rides the caller refinement, which `Input` replays last.
/// Every `Input` goes through this — construct with it, never `Input::new`.
pub(crate) fn glass_input(state: &Entity<InputState>, window: &Window, cx: &App) -> Input {
    let focused = state.focus_handle(cx).is_focused(window);
    Input::new(state)
        .focus_bordered(false)
        .when(focused, |input| {
            input.border_color(t::glass::STROKE_ACTIVE.to_hsla())
        })
}

// EXP-698: the rung names match the TOKEN ladder (`size::CONTROL_*`), which
// is the same ladder on all four clients — LG 36 / MD 32 / SM 24. They used
// to be shifted one notch (36 was "MD"), which made every cross-file read a
// translation step.
/// Web `h-9` (Button default / Input) — `size::INPUT_HEIGHT`.
pub(crate) const CTL_LG_H: f32 = t::size::CONTROL_LG;
/// Web `h-8` (Button sm / small inputs) — `size::CONTROL_MD`.
pub(crate) const CTL_MD_H: f32 = t::size::CONTROL_MD;
/// Web `h-6` (Button xs) — `size::CONTROL_SM`.
pub(crate) const CTL_SM_H: f32 = t::size::CONTROL_SM;

/// One import per file: `use crate::controls::WebControl as _;`
/// `with_size` keeps the component's own label/icon typography mapping; the
/// explicit height/padding overrides the too-small rem-derived boxes.
pub(crate) trait WebControl: Styled + Sizable + Sized {
    /// Web Button default: h-9 px-4, theme radius.
    fn web_md(self) -> Self {
        self.with_size(Size::Medium)
            .h(px(CTL_LG_H))
            .px(px(16.))
            .cursor_pointer()
    }

    /// Web Button `sm`: h-8 px-3, capsule.
    fn web_sm(self) -> Self {
        self.with_size(Size::Small)
            .h(px(CTL_MD_H))
            .px(px(12.))
            .rounded_full()
            .cursor_pointer()
    }

    /// Web Button `xs`: h-6 px-2, capsule.
    fn web_xs(self) -> Self {
        self.with_size(Size::XSmall)
            .h(px(CTL_SM_H))
            .px(px(8.))
            .rounded_full()
            .cursor_pointer()
    }

    /// Web Button icon-sm: size-8 circle.
    fn web_icon_sm(self) -> Self {
        self.with_size(Size::Small)
            .size(px(CTL_MD_H))
            .rounded_full()
            .cursor_pointer()
    }

    /// Web Button `icon-xs`: size-6 circle.
    fn web_icon_xs(self) -> Self {
        self.with_size(Size::XSmall)
            .size(px(CTL_SM_H))
            .rounded_full()
            .cursor_pointer()
    }

    /// Web Input: h-9 (radius stays the component's).
    fn web_input(self) -> Self {
        self.h(px(CTL_LG_H))
    }

    /// Web small input/select: h-8.
    fn web_input_sm(self) -> Self {
        self.h(px(CTL_MD_H))
    }
}

impl<T: Styled + Sizable> WebControl for T {}

/// EXP-698 — the web's 11px caption rung (`text-[11px]`), one step below
/// `text_xs` (12px). gpui has no rung there, and the steer feed needs two
/// caption levels: a tool row's mono argument, a permission's detail and
/// hint, a subagent's status line and the stepper counter all render at 11 on
/// the web, and rendering them at 12 flattens them into the labels above them.
///
/// Its own trait, not a [`WebControl`] method: that one is bounded on
/// `Sizable` (a gpui-component CONTROL), and these are plain `Div`s.
///
/// One import per file: `use crate::controls::WebText as _;`
pub(crate) trait WebText: Styled + Sized {
    fn text_2xs(self) -> Self {
        self.text_size(gpui::rems(0.6875))
    }
}

impl<T: Styled> WebText for T {}

/// Web `EmptyState` (`components/empty-state.tsx`): centered column, a 48px
/// primary-tinted icon disc, semibold title, muted description.
pub(crate) fn empty_state(
    icon: Icon,
    title: impl Into<SharedString>,
    description: impl Into<SharedString>,
    cx: &App,
) -> Div {
    let theme = cx.theme();
    v_flex()
        .w_full()
        .max_w(px(448.))
        .mx_auto()
        .items_center()
        .gap_3()
        .px_6()
        .py_12()
        .text_center()
        .child(
            div()
                .size(px(48.))
                .flex()
                .items_center()
                .justify_center()
                .rounded_full()
                .bg(theme.primary.opacity(0.1))
                .child(icon.size(px(24.)).text_color(theme.primary)),
        )
        .child(
            div()
                .text_lg()
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(theme.foreground)
                .child(title.into()),
        )
        .child(
            div()
                .text_sm()
                .text_color(theme.muted_foreground)
                .child(description.into()),
        )
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
pub(crate) fn glass_icon_button(
    id: impl Into<gpui::ElementId>,
    icon: Icon,
    cx: &App,
) -> gpui_component::button::Button {
    use gpui_component::button::{ButtonCustomVariant, ButtonVariants as _};
    let foreground = cx.theme().foreground;
    // Custom variants paint at a fifth of the handed-in alpha (EXP-698,
    // `surface::custom_variant_fill`): pre-divide so the circle lands on the
    // same card/active fills as the pills beside it.
    let variant = ButtonCustomVariant::new(cx)
        .color(crate::surface::custom_variant_fill(t::glass::FILL_CARD.to_hsla()))
        .hover(crate::surface::custom_variant_fill(t::glass::FILL_ACTIVE.to_hsla()))
        .active(crate::surface::custom_variant_fill(t::glass::FILL_ACTIVE.to_hsla()))
        .foreground(foreground.opacity(0.7));
    gpui_component::button::Button::new(id)
        .custom(variant)
        .web_icon_sm()
        .icon(icon)
        .border_1()
        .border_color(t::glass::STROKE_CARD.to_hsla())
}

/// A DESTRUCTIVE popup-menu item (EXP-697): label AND glyph in the theme's
/// danger red, matching the iOS/Android glass menus where delete/remove always
/// reads red. `PopupMenuItem` has no danger variant upstream, so the label
/// rides `PopupMenuItem::element` — the only escape hatch that lets a menu row
/// paint its own text color.
pub(crate) fn danger_menu_item(
    label: impl Into<SharedString>,
    icon: Icon,
    cx: &App,
) -> PopupMenuItem {
    let danger = cx.theme().danger;
    let label = label.into();
    PopupMenuItem::element(move |_, _| div().text_color(danger).child(label.clone()))
        .icon(icon.text_color(danger))
}

/// Web segmented `TabsList` capsule (`components/ui/tabs.tsx`): h-9 full-width
/// capsule with a 3px inset. Pair with [`segmented_item`] children.
pub(crate) fn segmented(cx: &App) -> Div {
    let theme = cx.theme();
    div()
        .flex()
        .flex_row()
        .w_full()
        .h(px(CTL_LG_H))
        .items_center()
        .justify_center()
        .rounded_full()
        .border_1()
        .border_color(t::glass::STROKE_SECTION.to_hsla())
        .bg(t::glass::FILL_SECTION.to_hsla())
        .p(px(3.))
        .text_color(theme.muted_foreground)
}

/// A web `TabsTrigger`: equal-width capsule segment, active = glass active
/// fill + stroke.
pub(crate) fn segmented_item(active: bool, cx: &App) -> Div {
    let theme = cx.theme();
    let item = div()
        .flex()
        .flex_row()
        .flex_1()
        .h_full()
        .items_center()
        .justify_center()
        // EXP-698: no MEDIUM weight and no 6px gap — a segment is a label,
        // not a heading, and the glyph sits tighter to it.
        .gap_1()
        .rounded_full()
        .border_1()
        .border_color(gpui::transparent_black())
        .px_2()
        .text_sm()
        .cursor_pointer();
    if active {
        item.bg(t::glass::FILL_ACTIVE.to_hsla())
            .border_color(t::glass::STROKE_ACTIVE.to_hsla())
            .text_color(theme.foreground)
    } else {
        item.hover(|style| style.text_color(theme.foreground))
    }
}

/// EXP-698 — the ONE textarea state: a multi-line field GROWS with its
/// content between `min_rows` and `max_rows` instead of standing at a
/// hard-coded pixel height. Every dialog textarea used to pick its own
/// `h(px(72.))` / `h(px(80.))` / `h(px(120.))` / `h(px(180.))`, which is four
/// different fields for one control; the row range is the web/mobile
/// contract (`min-h`/`max-h` in rows) and it is the same number on every
/// client.
///
/// The row range lives on the STATE, not on the element: gpui-component
/// carries the layout mode in `TextareaState` (`auto_grow`), so this is the
/// constructor half. The CHROME half is the theme's — `theme.input` is the
/// glass card stroke since EXP-698, so any `appearance(true)` field is
/// already a glass field and needs nothing here; a field inside a
/// [`crate::surface::glass_group`] row keeps `appearance(false)`, because
/// there the GROUP is the field.
pub(crate) fn web_textarea(
    min_rows: usize,
    max_rows: usize,
    window: &mut gpui::Window,
    cx: &mut gpui::Context<TextareaState>,
) -> TextareaState {
    TextareaState::new(window, cx).auto_grow(min_rows, max_rows)
}
