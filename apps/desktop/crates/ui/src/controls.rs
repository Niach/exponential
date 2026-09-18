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

use std::time::Duration;

use gpui::{
    anchored, bounce, deferred, div, point, prelude::FluentBuilder as _, px, Anchor, Animation,
    AnimationExt as _, AnyElement, App, Div, ElementId, Entity, Focusable as _, FontWeight,
    InteractiveElement as _, IntoElement, ParentElement as _, Pixels, SharedString, Stateful,
    Styled, Window,
};
use gpui_component::{
    input::{Input, InputState, TextareaState},
    menu::PopupMenuItem,
    v_flex, ActiveTheme as _, Icon, Sizable, Size,
};
use theme::tokens as t;

/// EXP-720: the ONE text-field recipe (styleguide `text-field`, web
/// `@exp/ui input.tsx`): card fill under the card stroke, and focus
/// swaps the STROKE to `glass::STROKE_ACTIVE` — no ring. gpui-component's
/// `Input` paints its focused state as `theme.ring` (the neutral RING token
/// the web keeps for BUTTON focus-visible halos) plus a halo child, which is
/// why an autofocused dialog field used to wear a bright grey outline no other
/// field carried. The theme has no hook for the focused input stroke alone
/// (`ring` also drives every other focus ring), so the swap happens here:
/// `focus_bordered(false)` mutes the component's own focused style and the
/// active stroke rides the caller refinement, which `Input` replays last.
/// Every `Input` goes through this — construct with it, never `Input::new`.
///
/// EXP-963: the corner is the FIELD rung, `radius::LG` (12) — the web
/// `rounded-lg` input, iOS/Android `GlassTextField`. gpui-component paints
/// every control at `theme.radius` (the row's 10), which left the desktop's
/// fields one step tighter than the other three clients'; the refinement is
/// replayed after the component's own `.rounded(theme.radius)`, so it wins
/// without forking the theme (buttons and rows keep their 10).
pub(crate) fn glass_input(state: &Entity<InputState>, window: &Window, cx: &App) -> Input {
    let focused = state.focus_handle(cx).is_focused(window);
    Input::new(state)
        .focus_bordered(false)
        .rounded(px(t::radius::LG))
        .when(focused, |input| {
            input.border_color(t::glass::STROKE_ACTIVE.to_hsla())
        })
}

/// The two rungs of the search field (EXP-963, web `SearchField size`):
/// `Md` is the stock 36px field, `Sm` the 28px one dense columns use (the
/// diff pane's file filter, a picker's own search row).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SearchFieldSize {
    Md,
    Sm,
}

/// The `Sm` search field's height: the compact ROW rung (web `h-7`, the
/// sidebar's 28px one-line row — [`crate::surface::flat_row_compact`]), not
/// a control rung: a filter sits over rows of that height and reads as one
/// of them.
pub(crate) const SEARCH_FIELD_SM_H: f32 = 28.;

/// EXP-963 — the ONE "filter this list" field, the desktop twin of the web
/// `SearchField` and the natives' `GlassSheetSearchField`: the text field
/// with the `nav-search` glyph INSIDE it and a ghost `ui-clear` circle that
/// appears only once there is something to clear — and puts the caret back
/// in the field, so typing continues. It is a [`glass_input`], not a new
/// box: every chrome decision (fill, hairline, focus stroke, radius) still
/// comes from there, and a host inside a popover still chains
/// `.appearance(false)` to go chrome-less.
///
/// The clear is OURS, not gpui-component's `cleanable(true)`: that one draws
/// an `X` from the component's own icon set, and the registry's clear glyph
/// (the circled cross every other client draws) is a different mark.
pub(crate) fn search_field(
    state: &Entity<InputState>,
    size: SearchFieldSize,
    window: &Window,
    cx: &App,
) -> Input {
    use gpui_component::button::{Button, ButtonVariants as _};
    let muted = cx.theme().muted_foreground;
    let glyph = match size {
        SearchFieldSize::Md => 16.,
        SearchFieldSize::Sm => 14.,
    };
    let has_text = !state.read(cx).value().is_empty();
    let input = glass_input(state, window, cx).prefix(
        Icon::new(crate::icons::registry::NAV_SEARCH)
            .size(px(glyph))
            .flex_shrink_0()
            .text_color(muted),
    );
    let input = match size {
        SearchFieldSize::Md => input.web_input(),
        // `Styled::h`, named: `Input` has an inherent `h` of its own that
        // only sizes a MULTI-line editor, and it would shadow the box height.
        SearchFieldSize::Sm => Styled::h(input.small(), px(SEARCH_FIELD_SM_H)),
    };
    if !has_text {
        return input;
    }
    let clear_target = state.clone();
    input.suffix(
        Button::new(("search-field-clear", state.entity_id()))
            .ghost()
            .cursor_pointer()
            .xsmall()
            .tab_stop(false)
            .icon(
                Icon::new(crate::icons::registry::UI_CLEAR)
                    .size(px(glyph))
                    .text_color(muted),
            )
            .tooltip("Clear search")
            .on_click(move |_, window, cx| {
                cx.stop_propagation();
                clear_target.update(cx, |input, cx| input.set_value("", window, cx));
                clear_target.read(cx).focus_handle(cx).focus(window, cx);
            }),
    )
}

/// Which edge a [`disclosure_header`]'s chevron sits on (web `chevron`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ChevronSide {
    /// Before the label, the way a tree folds (the default).
    Leading,
    /// At the far edge after a spacer — for a row whose siblings carry no
    /// chevron and must not indent out of line with them (the tool row's
    /// output fold).
    Trailing,
}

/// EXP-963 — the DISCLOSURE header, the web `DisclosureHeader` twin: the
/// one-line fold toggle a feed row, a subagent lane, a tool group or a
/// workflow agent opens and closes with. Muted at rest and brightening under
/// the pointer, a 12px chevron pointing right folded and down open, the
/// WHOLE line the target. `content` is the row's own text and glyphs (an
/// `h_flex` the caller builds — it keeps every caption and spinner it had);
/// the caller chains its `.on_click` and its type rung (`tool_text`) after.
///
/// It is NOT the group band (`surface::glass_section_band_fold`): that is a
/// filled strip heading a LIST; this is bare text heading a fold INSIDE a
/// row. And it may not contain another button — a fold's own action renders
/// beside it, never inside it.
pub(crate) fn disclosure_header(
    id: impl Into<gpui::ElementId>,
    open: bool,
    chevron: ChevronSide,
    content: impl gpui::IntoElement,
    cx: &App,
) -> gpui::Stateful<Div> {
    use gpui::InteractiveElement as _;
    let theme = cx.theme();
    let glyph = Icon::new(if open {
        crate::icons::registry::UI_CHEVRON_DOWN
    } else {
        crate::icons::registry::UI_CHEVRON_RIGHT
    })
    .xsmall()
    .flex_shrink_0();
    let foreground = theme.foreground;
    let row = div()
        .id(id)
        .flex()
        .flex_row()
        .w_full()
        .min_w_0()
        .gap_2()
        .items_center()
        .cursor_pointer()
        .text_color(theme.muted_foreground)
        .hover(move |style| style.text_color(foreground));
    match chevron {
        ChevronSide::Leading => row.child(glyph).child(content),
        ChevronSide::Trailing => row
            .child(content)
            .child(div().flex_1().min_w_0())
            .child(glyph),
    }
}

/// The two text-button variants (EXP-963, web `Button variant="text" |
/// "link"` at `size="inline"`): what happens when the words are pressed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TextButtonVariant {
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
pub(crate) fn text_button(
    id: impl Into<gpui::ElementId>,
    label: impl Into<SharedString>,
    variant: TextButtonVariant,
    cx: &App,
) -> gpui::Stateful<Div> {
    use gpui::InteractiveElement as _;
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

/// EXP-862 — the ONE GHOST icon button, the web `<Button variant="ghost"
/// size="icon-sm">` twin (iOS `GhostIconButton`, Android `CircleIconButton(
/// borderless = true)`).
///
/// A circle says "primary action" — play/start, send, the rail's New issue
/// and Search, the mobile FAB, an "+" add. Everything else that is a glyph
/// on a row (the ⋯ menu, close, the folder/file-list toggles, a fold
/// chevron, trash/remove, a refresh) is THIS: a 32px square, no fill and no
/// stroke at rest, the glyph at 70% foreground, and the flat row's own hover
/// wash (`list_hover` == `glass::FILL_ROW`, EXP-811) under the pointer.
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
pub(crate) fn ghost_icon_button(
    id: impl Into<gpui::ElementId>,
    icon: Icon,
    cx: &App,
) -> gpui_component::button::Button {
    use gpui_component::button::{ButtonCustomVariant, ButtonVariants as _};
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
        .rounded(px(t::radius::MD))
        .cursor_pointer()
        .icon(icon)
}

/// The size of a back glyph on every client (EXP-862): 16px, the `icon-sm`
/// rung's glyph, bare in a back ROW (EXP-863 retired the session header's
/// Back button; the sidebar carries navigation).
const BACK_GLYPH: f32 = 16.;

/// The BARE back glyph (EXP-862), for the back ROWS where the whole row is
/// the target (the settings nav's and the list nav's "‹ Boards"): a nested
/// button inside a clickable row is a second hit target for the same action,
/// so those rows take the glyph alone and keep their own click.
pub(crate) fn back_glyph() -> Icon {
    // EXP-870: the `ui-back` CONCEPT (an arrow), the web back rows' glyph.
    Icon::from(crate::icons::registry::UI_BACK)
        .size(px(BACK_GLYPH))
        .flex_shrink_0()
}

/// EXP-862 — the ONE switch: gpui-component's `Switch` with the web's pointer
/// cursor. gpui-component defaults every control to `cursor_default`; a toggle
/// the user clicks points on hover on all four clients, and a dozen call sites
/// each remembering to say so is how half of them forgot.
///
/// `Switch::new` is forbidden outside this module (see
/// `only_controls_constructs_switches`) — construct through this.
pub(crate) fn web_switch(id: impl Into<gpui::ElementId>) -> gpui_component::switch::Switch {
    gpui_component::switch::Switch::new(id).cursor_pointer()
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

/// Web segmented `TabsList` capsule (`@exp/ui tabs.tsx`): h-9 full-width
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

// ---------------------------------------------------------------------------
// The typeahead menu (EXP-970)
// ---------------------------------------------------------------------------

/// The menu's width band (web `TypeaheadMenu`: `w-72`, grown to the widest
/// row the desktop lists).
pub(crate) const TYPEAHEAD_MIN_W: f32 = 260.;
pub(crate) const TYPEAHEAD_MAX_W: f32 = 380.;
/// The tallest the menu gets on either side of the caret (web `MAX_HEIGHT`).
pub(crate) const TYPEAHEAD_MAX_H: f32 = 320.;
/// The shortest it is allowed to be capped to (web `MIN_HEIGHT`) — under
/// this the menu keeps its floor and the window snap slides it instead.
const TYPEAHEAD_MIN_H: f32 = 48.;
/// Below this much room under the caret the menu flips above, when the room
/// above is larger (web `FLIP_BELOW`).
const TYPEAHEAD_FLIP_BELOW: f32 = 200.;
/// The gap between the caret line and the menu edge (web `ANCHOR_GAP`).
const TYPEAHEAD_GAP: f32 = 4.;
/// The window margin the menu keeps clear (web `VIEWPORT_PAD`).
const TYPEAHEAD_PAD: f32 = 8.;

/// Which side of the caret a [`typeahead_menu`] opens on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TypeaheadPlacement {
    Below,
    Above,
}

/// The measured half of an anchored typeahead: the side and the height cap.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct TypeaheadPlan {
    pub(crate) placement: TypeaheadPlacement,
    pub(crate) max_h: f32,
}

/// The pure placement rule, the web `placeTypeaheadMenu` twin: below the
/// caret unless the room there is short AND the room above is larger, capped
/// to the room on the chosen side (never taller than [`TYPEAHEAD_MAX_H`],
/// never shorter than the floor). `caret_top`/`caret_bottom` are the caret
/// LINE's edges in window coordinates.
pub(crate) fn plan_typeahead(caret_top: f32, caret_bottom: f32, viewport_h: f32) -> TypeaheadPlan {
    let space_below = viewport_h - caret_bottom - TYPEAHEAD_PAD;
    let space_above = caret_top - TYPEAHEAD_PAD;
    let cap = |space: f32| (space - TYPEAHEAD_GAP).min(TYPEAHEAD_MAX_H).max(TYPEAHEAD_MIN_H);
    if space_below < TYPEAHEAD_FLIP_BELOW && space_above > space_below {
        TypeaheadPlan {
            placement: TypeaheadPlacement::Above,
            max_h: cap(space_above),
        }
    } else {
        TypeaheadPlan {
            placement: TypeaheadPlacement::Below,
            max_h: cap(space_below),
        }
    }
}

/// How a [`typeahead_menu`] sits in its host.
pub(crate) enum TypeaheadArm {
    /// Hangs off the caret: a deferred, window-anchored popup that flips
    /// above the caret when the room below runs out (the `@`/`#`/`:` menus
    /// of the editors and the mention composer).
    Anchored {
        /// The caret line's left edge, top and bottom, in window coordinates.
        caret_left: Pixels,
        caret_top: Pixels,
        caret_bottom: Pixels,
    },
    /// Sits in the flow of its host, full width (the steer composer's `/`
    /// menu, which lives inside the composer card above the textarea).
    Inline,
}

/// EXP-970 — the ONE typeahead menu, the web `TypeaheadMenu` twin: the menu
/// that follows what someone is TYPING (`@` mentions, `#` issue refs, `:`
/// emoji, `/` commands), as opposed to the combobox, which owns its own
/// field. The surface is the floating-menu recipe (`MENU_SURFACE_CLASS`):
/// the opaque popover fill under the card hairline on the row rung
/// (`radius::MD`), a 4px inset, rows 2px apart, capped and scrolling. The
/// rows are [`typeahead_row`]s the host builds (it owns their content, the
/// click that accepts and the hover that moves the selection).
///
/// Four hosts drew this box by hand before, each with its own width, radius,
/// corner and cap; the slash menu was a bare column. Anchoring is the
/// [`TypeaheadArm`]: the anchored arm measures the room itself
/// ([`plan_typeahead`]) and hands gpui the corner to hang from, so a long
/// list near the bottom of the window opens upward instead of being slid
/// over the caret.
pub(crate) fn typeahead_menu(
    id: impl Into<ElementId>,
    arm: TypeaheadArm,
    rows: Vec<AnyElement>,
    window: &Window,
    cx: &App,
) -> AnyElement {
    use gpui::StatefulInteractiveElement as _;
    let theme = cx.theme();
    let surface = v_flex()
        .id(id)
        .occlude()
        .p_1()
        .gap_0p5()
        .bg(theme.popover)
        .text_color(theme.popover_foreground)
        .border_1()
        .border_color(t::glass::STROKE_CARD.to_hsla())
        .rounded(px(t::radius::MD))
        .shadow_md()
        .overflow_y_scroll();
    match arm {
        TypeaheadArm::Inline => surface
            .w_full()
            .min_w_0()
            .max_h(px(TYPEAHEAD_MAX_H))
            .children(rows)
            .into_any_element(),
        TypeaheadArm::Anchored {
            caret_left,
            caret_top,
            caret_bottom,
        } => {
            let plan = plan_typeahead(
                f32::from(caret_top),
                f32::from(caret_bottom),
                f32::from(window.viewport_size().height),
            );
            let menu = surface
                .min_w(px(TYPEAHEAD_MIN_W))
                .max_w(px(TYPEAHEAD_MAX_W))
                .max_h(px(plan.max_h))
                .children(rows);
            let (corner, position) = match plan.placement {
                TypeaheadPlacement::Below => (
                    Anchor::TopLeft,
                    point(caret_left, caret_bottom + px(TYPEAHEAD_GAP)),
                ),
                TypeaheadPlacement::Above => (
                    Anchor::BottomLeft,
                    point(caret_left, caret_top - px(TYPEAHEAD_GAP)),
                ),
            };
            deferred(
                anchored()
                    .anchor(corner)
                    .position(position)
                    .snap_to_window_with_margin(px(TYPEAHEAD_PAD))
                    .child(menu),
            )
            .with_priority(200)
            .into_any_element()
        }
    }
}

/// One row of a [`typeahead_menu`] (web `TYPEAHEAD_ROW_CLASS`): full width,
/// `px_2 py_1`, the small rung's corner, and the list's active fill on the
/// SELECTED row only — hovering MOVES the selection (EXP-892) rather than
/// painting a second highlight, so the host chains `.on_hover` for that and
/// `.on_mouse_down` to accept; the content (`completion_row_content`, the
/// slash row's name/hint/description) is the host's child.
pub(crate) fn typeahead_row(id: impl Into<ElementId>, selected: bool, cx: &App) -> Stateful<Div> {
    let theme = cx.theme();
    div()
        .id(id)
        .flex()
        .flex_row()
        .w_full()
        .min_w_0()
        .gap_2()
        .items_center()
        .px_2()
        .py_1()
        .rounded(px(t::radius::SM))
        .cursor_pointer()
        .when(selected, |row| row.bg(theme.list_active))
}

// ---------------------------------------------------------------------------
// Skeleton, checkbox, alert (EXP-970)
// ---------------------------------------------------------------------------

/// The skeleton's pulse period — the web `animate-pulse` (2s). Not a motion
/// duration token: those are the transition rungs (120/180/280), and a
/// placeholder's breathing is a different kind of time; the CURVE is the
/// ladder's standard one.
pub(crate) const SKELETON_PULSE: Duration = Duration::from_secs(2);

/// EXP-970 — the skeleton, the web `Skeleton` twin: a block standing in for
/// text that is still loading, at the SHAPE of what will arrive. The row
/// rung's corner (`radius::MD`, web `rounded-md`), the theme's skeleton fill
/// (the opaque accent, `theme::exponential_dark`), breathing between full
/// and half opacity on the standard curve over [`SKELETON_PULSE`]. A
/// 16px-tall full-width bar by default; the caller sizes it like any
/// element (`.h_3p5().w_40()`, `.size_4().rounded_full()` for an avatar
/// stand-in, `.flex_1()` for a fill): a row's worth of bars, never a
/// spinner in a list.
///
/// gpui-component's own `Skeleton` is not used: its radius is the crate's
/// and its pulse is `bounce(ease_in_out)` over its own 2s, neither on the
/// ladder (`only_controls_constructs_skeletons` keeps it out). Construct
/// with [`skeleton`].
#[derive(IntoElement)]
pub(crate) struct Skeleton {
    style: gpui::StyleRefinement,
}

/// A [`Skeleton`] block.
pub(crate) fn skeleton() -> Skeleton {
    Skeleton {
        style: gpui::StyleRefinement::default(),
    }
}

impl Styled for Skeleton {
    fn style(&mut self) -> &mut gpui::StyleRefinement {
        &mut self.style
    }
}

impl gpui::RenderOnce for Skeleton {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        use gpui_component::StyledExt as _;
        div()
            .flex_shrink_0()
            .w_full()
            .h_4()
            .rounded(px(t::radius::MD))
            .bg(cx.theme().skeleton)
            .refine_style(&self.style)
            .with_animation(
                "skeleton-pulse",
                Animation::new(SKELETON_PULSE)
                    .repeat()
                    .with_easing(bounce(theme::motion::standard())),
                |block, delta| block.opacity(1. - 0.5 * delta),
            )
    }
}

/// What a [`checkbox`] holds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CheckState {
    Unchecked,
    Checked,
    /// A bulk selection that disagrees: the same box with a minus.
    Indeterminate,
}

impl From<bool> for CheckState {
    fn from(checked: bool) -> Self {
        if checked {
            Self::Checked
        } else {
            Self::Unchecked
        }
    }
}

/// The checkbox's box (web `size-4`).
pub(crate) const CHECKBOX_PX: f32 = 16.;

/// EXP-970 — the checkbox, the web `Checkbox` twin: the 16px square that
/// holds a TABLE's selection (the issue list's bulk-select column, the
/// rail's issue rows, a checklist item in the description) and nothing else
/// — a picker's multi-select rows draw the `ui-selected`/`ui-unselected`
/// circle pair instead. Glass tokens throughout: the row fill under the
/// STRONG stroke (a 16px unchecked square has to stay legible on the row it
/// sits on), the small rung's corner (`radius::SM`, web `rounded-sm`), and
/// checked = the primary fill under the primary foreground with the
/// registry's `ui-check` glyph (`ui-minus` for indeterminate). Disabled
/// halves the opacity and drops the pointer. The caller chains `.on_click`.
///
/// gpui-component's `Checkbox` is not used: its box is the theme's input
/// stroke at the crate's radius and its tick is the crate's own icon, none
/// of them the ladder's (`only_controls_constructs_checkboxes`).
pub(crate) fn checkbox(
    id: impl Into<ElementId>,
    state: CheckState,
    disabled: bool,
    cx: &App,
) -> Stateful<Div> {
    let theme = cx.theme();
    let marked = state != CheckState::Unchecked;
    let square = div()
        .id(id)
        .flex_shrink_0()
        .size(px(CHECKBOX_PX))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(t::radius::SM))
        .border_1()
        .when(!disabled, |square| square.cursor_pointer())
        .when(disabled, |square| square.opacity(0.5));
    let square = if marked {
        square.bg(theme.primary).border_color(theme.primary)
    } else {
        square
            .bg(t::glass::FILL_ROW.to_hsla())
            .border_color(t::glass::STROKE_STRONG.to_hsla())
    };
    let glyph = match state {
        CheckState::Unchecked => return square,
        CheckState::Checked => crate::icons::registry::UI_CHECK,
        CheckState::Indeterminate => crate::icons::registry::UI_MINUS,
    };
    square.child(
        Icon::new(glyph)
            .size(px(14.))
            .flex_shrink_0()
            .text_color(theme.primary_foreground),
    )
}

/// The two [`alert`] variants (web `Alert variant`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AlertVariant {
    /// A notice: the card fill under the card hairline, foreground text.
    Default,
    /// A failure: the danger tint at a tenth under its stroke at half,
    /// danger text.
    Destructive,
}

/// EXP-970 — the inline alert, the web `Alert` twin: a message that belongs
/// to the page it interrupts, not a toast and not a dialog. The row rung's
/// corner, `px_3 py_2`, `text_sm`; the leading glyph (16px, nudged 2px down
/// to sit on the first line) earns its column only when one is passed. The
/// caller appends its content — [`alert_title`] and/or a description — as
/// children; a banner that wants to wrap pills chains `.flex_wrap()`.
pub(crate) fn alert(variant: AlertVariant, glyph: Option<Icon>, cx: &App) -> Div {
    let theme = cx.theme();
    let (fill, stroke, text) = match variant {
        AlertVariant::Default => (
            t::glass::FILL_CARD.to_hsla(),
            t::glass::STROKE_CARD.to_hsla(),
            theme.foreground,
        ),
        AlertVariant::Destructive => (
            theme.danger.opacity(0.1),
            theme.danger.opacity(0.5),
            theme.danger,
        ),
    };
    div()
        .flex()
        .flex_row()
        .w_full()
        .min_w_0()
        .items_start()
        .gap_3()
        .px_3()
        .py_2()
        .rounded(px(t::radius::MD))
        .border_1()
        .border_color(stroke)
        .bg(fill)
        .text_sm()
        .text_color(text)
        .children(glyph.map(|glyph| {
            div()
                .flex_shrink_0()
                .pt_0p5()
                .child(glyph.size(px(16.)).flex_shrink_0())
        }))
}

/// An [`alert`]'s one-line title (web `AlertTitle`): medium weight, tight.
pub(crate) fn alert_title(title: impl Into<SharedString>) -> Div {
    div()
        .min_w_0()
        .font_weight(FontWeight::MEDIUM)
        .truncate()
        .child(title.into())
}

#[cfg(test)]
mod tests {
    /// EXP-862 — the structural half of [`super::web_switch`]: a `Switch`
    /// built anywhere else is a switch that does not point on hover, and no
    /// compiler can say so. A grep over the crate's own sources is the
    /// cheapest honest check there is (the `text_selection_guard` rule uses
    /// the same scan).
    ///
    /// This module is the one allowed constructor, so it is the one file the
    /// scan skips.
    #[test]
    fn only_controls_constructs_switches() {
        assert_only_controls_constructs("Switch::new(", "EXP-862", "controls::web_switch(id)");
    }

    /// EXP-970: the same rule for gpui-component's `Checkbox` — its box, its
    /// radius and its tick are the crate's, not the ladder's.
    #[test]
    fn only_controls_constructs_checkboxes() {
        assert_only_controls_constructs(
            "Checkbox::new(",
            "EXP-970",
            "controls::checkbox(id, state, disabled, cx)",
        );
    }

    /// EXP-970: and for its `Skeleton` — the crate's pulse and radius.
    #[test]
    fn only_controls_constructs_skeletons() {
        assert_only_controls_constructs(
            "Skeleton::new(",
            "EXP-970",
            "controls::skeleton()",
        );
    }

    /// The scan behind the three constructor rules: `needle` may appear in
    /// no source file of this crate but this module (the one allowed
    /// constructor, and the one file that spells the needle in its docs).
    fn assert_only_controls_constructs(needle: &str, issue: &str, replacement: &str) {
        let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut offenders: Vec<String> = Vec::new();
        let mut stack = vec![src.clone()];
        while let Some(dir) = stack.pop() {
            for entry in std::fs::read_dir(&dir).expect("the crate's own sources are readable") {
                let path = entry.expect("a readable dir entry").path();
                if path.is_dir() {
                    stack.push(path);
                    continue;
                }
                if path.extension().and_then(|ext| ext.to_str()) != Some("rs") {
                    continue;
                }
                if path.file_name().and_then(|name| name.to_str()) == Some("controls.rs") {
                    continue;
                }
                let text = std::fs::read_to_string(&path).expect("a readable source file");
                let hits = text.matches(needle).count();
                if hits > 0 {
                    let name = path
                        .strip_prefix(&src)
                        .unwrap_or(&path)
                        .to_string_lossy()
                        .to_string();
                    offenders.push(format!("{name} ({hits})"));
                }
            }
        }
        offenders.sort();
        assert!(
            offenders.is_empty(),
            "{issue}: construct through `{replacement}` — `{needle}` still appears in: {}",
            offenders.join(", ")
        );
    }

    /// EXP-970 — the placement rule is the web `placeTypeaheadMenu`, number
    /// for number (`typeahead.test.tsx`).
    #[test]
    fn the_typeahead_opens_below_and_flips_above_only_when_below_is_short() {
        use super::{plan_typeahead, TypeaheadPlacement, TYPEAHEAD_MAX_H};
        // Plenty of room below: below, capped at the maximum.
        let plan = plan_typeahead(100., 120., 800.);
        assert_eq!(plan.placement, TypeaheadPlacement::Below);
        assert_eq!(plan.max_h, TYPEAHEAD_MAX_H);
        // Room below shrinks under the flip line while above is larger: above,
        // capped to the room above minus the pad and the gap.
        let plan = plan_typeahead(500., 520., 700.);
        assert_eq!(plan.placement, TypeaheadPlacement::Above);
        assert_eq!(plan.max_h, TYPEAHEAD_MAX_H);
        let plan = plan_typeahead(300., 320., 400.);
        assert_eq!(plan.placement, TypeaheadPlacement::Above);
        assert_eq!(plan.max_h, 300. - 8. - 4.);
        // Short below AND short above: stays below, capped to what is there.
        let plan = plan_typeahead(60., 80., 200.);
        assert_eq!(plan.placement, TypeaheadPlacement::Below);
        assert_eq!(plan.max_h, 200. - 80. - 8. - 4.);
        // The floor: a menu never plans shorter than its minimum height.
        let plan = plan_typeahead(20., 40., 60.);
        assert_eq!(plan.placement, TypeaheadPlacement::Below);
        assert_eq!(plan.max_h, 48.);
    }

    #[test]
    fn a_bool_is_a_two_state_check() {
        use super::CheckState;
        assert_eq!(CheckState::from(true), CheckState::Checked);
        assert_eq!(CheckState::from(false), CheckState::Unchecked);
    }
}
