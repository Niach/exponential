//! Glass surface recipes (EXP-269) — the GlassTheme.swift / Glass.kt parity
//! layer for the desktop. gpui has no in-scene backdrop blur, so glass is the
//! Android approximation: white-alpha fills + hairline strokes over the page
//! gradient (`theme::background_gradient()`), radii from the token ladder
//! (row 10 / section 12 / card 16). Strokes are 1px on purpose — fractional
//! hairlines vanish on 1x-scale displays.

use gpui::{
    div, px, AnyElement, App, Div, FontWeight, InteractiveElement as _, ParentElement as _,
    SharedString, StyleRefinement, Styled,
};
use gpui_component::input::Input;
use gpui_component::searchable_list::{SearchableListDelegate, SearchableListItem};
use gpui_component::select::Select;
use gpui_component::{h_flex, text::TextViewStyle, v_flex, ActiveTheme as _};
use theme::tokens as t;

/// EXP-698 — the web `GlassSectionHeader` (`components/ui/glass-rows.tsx`,
/// EXP-616): a PLAIN-TEXT heading over a glass list — no band, no fill, no
/// border — `px_1 pt_1 pb_2`, the label `text_sm` MEDIUM at 70% foreground,
/// an optional `count` trailing it in `text_xs` at 50%, then a spacer and the
/// optional trailing control. Labels are SENTENCE CASE, never uppercase.
///
/// Lived in `actions_view` until EXP-698 moved it here beside the other
/// glass recipes; every page section (Actions, Automations, Devices,
/// Getting started, the support rail) carries this one header design.
///
/// The `pb_2` IS the gap to the list below it — a section wrapper that adds
/// its own `gap_2` doubles it (EXP-697); keep the rows in a nested
/// `v_flex().gap_2()` instead.
pub(crate) fn glass_section_header(
    label: impl Into<SharedString>,
    count: Option<usize>,
    trailing: Option<AnyElement>,
    cx: &App,
) -> Div {
    let foreground = cx.theme().foreground;
    h_flex()
        .w_full()
        .min_w_0()
        .items_center()
        .gap_1p5()
        .px_1()
        .pt_1()
        .pb_2()
        .child(
            div()
                .text_sm()
                .font_weight(FontWeight::MEDIUM)
                .text_color(foreground.opacity(0.7))
                .child(label.into()),
        )
        .children(count.map(|count| {
            div()
                .text_xs()
                .text_color(foreground.opacity(0.5))
                .child(SharedString::from(count.to_string()))
        }))
        .child(div().flex_1())
        .children(trailing)
}

/// Card surface: radius 16, white 6% fill, white 10% hairline (mobile
/// `GlassCard`). Layout (width/padding/gap) is the caller's job.
pub(crate) fn glass_card() -> Div {
    v_flex()
        .rounded(px(t::radius::XL))
        .border_1()
        .border_color(t::glass::STROKE_CARD.to_hsla())
        .bg(t::glass::FILL_CARD.to_hsla())
}

/// EXP-642: ONE row of a carded list — the web `GlassRow`
/// (`components/ui/glass-rows.tsx`: `rounded-md border border-glass-stroke
/// bg-glass-row`). Unlike [`glass_card`] these stack with a GAP instead of
/// fusing into one bordered block, which is what the reviews/support/actions
/// lists and the machines section wear since the glass-row ladder (EXP-616)
/// landed on the web. Layout (padding, gap, hover, id) is the caller's job.
pub(crate) fn glass_row_card() -> Div {
    div()
        .rounded(px(t::radius::MD))
        .border_1()
        .border_color(t::glass::STROKE_ROW.to_hsla())
        .bg(t::glass::FILL_ROW.to_hsla())
}

/// EXP-694 — the inset-grouped card STACK, the reference look on every client
/// (Apple's "inset grouped list"; the Android `OptionGroup` / iOS
/// `glassFormRow` / web `GlassGroup` twin): ONE clipped radius-12 block filled
/// `FILL_ROW` with NO outer stroke, whose rows fuse into it and are separated
/// by white-6% hairlines instead of gaps.
///
/// This is the other half of the row ladder next to [`glass_row_card`]: that
/// one is for rows that are separate OBJECTS (list items), this one for rows
/// that are FIELDS of one form. Pair with [`glass_group_rows`] so the dividers
/// land automatically; groups stack with an 8px gap.
pub(crate) fn glass_group() -> Div {
    v_flex()
        .w_full()
        .rounded(px(t::radius::LG))
        .bg(t::glass::FILL_ROW.to_hsla())
        .overflow_hidden()
}

/// A [`glass_group`] filled with `rows` in order, hairline-divided: every row
/// but the first draws the divider as its OWN top border, so the group stays
/// one clipped block and the hairlines are full-bleed (the web
/// `divide-y divide-glass-stroke`).
pub(crate) fn glass_group_rows(rows: Vec<Div>) -> Div {
    rows.into_iter()
        .enumerate()
        .fold(glass_group(), |group, (ix, row)| {
            group.child(if ix == 0 { row } else { glass_row_divider(row) })
        })
}

/// The hairline a [`glass_group`] row draws above itself — for the rare caller
/// that assembles a group by hand instead of through [`glass_group_rows`].
pub(crate) fn glass_row_divider<T: Styled>(row: T) -> T {
    row.border_t_1()
        .border_color(t::glass::STROKE_ROW.to_hsla())
}

/// The row RHYTHM of a [`glass_group`]: 16 horizontal / 12 vertical padding,
/// vertically centered, leading label + trailing value. Every grouped row
/// ([`glass_picker_row`], [`glass_toggle_row`], the hand-built ones) starts
/// here so the stack keeps one baseline.
///
/// The toggle rows keep the same 12 even though the mobile twins drop to ~4
/// there: those platforms' switches carry their own inset padding, while
/// gpui-component's is a bare 20px pill — same 12 here, same ~44px row on
/// every client.
pub(crate) fn glass_row_shell() -> Div {
    h_flex().w_full().items_center().gap_3().px_4().py_3()
}

/// A picker row: the label leading at full foreground (with an optional muted
/// second line), the value trailing at 70% with its own chevron, and NO field
/// chrome — the group IS the field. Pass the trailing control through
/// [`glass_picker_select`] (a [`Select`]) or build it as a `dropdown_caret`
/// button; either way it must arrive stripped of background/border.
pub(crate) fn glass_picker_row(
    label: impl Into<SharedString>,
    description: Option<SharedString>,
    control: AnyElement,
    cx: &App,
) -> Div {
    let foreground = cx.theme().foreground;
    glass_row_shell()
        .child(
            v_flex()
                .flex_shrink_0()
                .gap_0p5()
                .text_sm()
                .text_color(foreground)
                .child(div().child(label.into()))
                .children(description.map(|description| {
                    div()
                        .text_xs()
                        .text_color(foreground.opacity(0.5))
                        .child(description)
                })),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .justify_end()
                .text_sm()
                .text_color(foreground.opacity(0.7))
                .child(control),
        )
}

/// How wide a picker row's trailing VALUE may grow before it ellipsises
/// (EXP-697). It has to be a PIXEL cap, not a percentage: the trigger sits
/// inside gpui-component's own `Popup` wrapper div, which we cannot style, and
/// a flex item's automatic minimum size is only clamped by a DEFINITE
/// `max_size` (taffy `flexbox.rs`, "4.5. Automatic Minimum Size of Flex
/// Items"). Without it the wrapper is sized to the label's min-content width
/// and a long name wraps onto a second line instead of truncating. 240 clears
/// the label column in every dialog that carries a picker row.
const PICKER_VALUE_MAX_W: f32 = 240.;

/// The trailing label of a `dropdown_menu` picker trigger, capped and
/// ellipsised (EXP-697). Pass it to [`gpui::ParentElement::child`] on the
/// trigger `Button` INSTEAD of `Button::label`: upstream renders `label` in a
/// `flex_none` box that neither shrinks nor truncates, so a long name wraps to
/// two lines and blows the row's height.
pub(crate) fn picker_value_label(label: impl Into<SharedString>) -> Div {
    div()
        .max_w(px(PICKER_VALUE_MAX_W))
        .truncate()
        .child(label.into())
}

/// Strip a [`Select`]'s field chrome so it reads as the trailing VALUE of a
/// [`glass_picker_row`]: no fill, no border, no focus ring box
/// (`appearance(false)`), no box padding or height of its own (the row's
/// 16/12 is the padding), and the title right-aligned against the caret the
/// component already draws. The web twin is the `GLASS_PICKER_ROW` trigger
/// (`bg-transparent border-0 h-auto`).
pub(crate) fn glass_picker_select<D>(select: Select<D>) -> Select<D>
where
    D: SearchableListDelegate + 'static,
    <D::Item as SearchableListItem>::Value: PartialEq + Clone,
{
    select
        .appearance(false)
        .h_auto()
        .px_0()
        .py_0()
        .text_right()
}

/// A TEXT-FIELD row: the label leading, the value typed trailing, and no
/// field chrome — the web `GlassInputRow` twin (the Name row of the device
/// editor, the CLI-path row of Settings → Agents). Pass the field through
/// [`glass_row_input`] so it arrives stripped.
pub(crate) fn glass_input_row(
    label: impl Into<SharedString>,
    input: AnyElement,
    cx: &App,
) -> Div {
    glass_row_shell()
        .child(
            div()
                .flex_shrink_0()
                .text_sm()
                .text_color(cx.theme().foreground)
                .child(label.into()),
        )
        .child(div().flex_1().min_w_0().child(input))
}

/// Strip an [`Input`]'s field chrome so it reads as the trailing VALUE of a
/// [`glass_input_row`]: no fill, no border, no focus ring, no box padding or
/// height of its own, and the text right-aligned like a picker's value. The
/// web twin is `GlassInputRow`'s `border-0 bg-transparent p-0 text-right`.
pub(crate) fn glass_row_input(input: Input) -> Input {
    input.appearance(false).h_auto().px_0().py_0().text_right()
}

/// A toggle row: the label (plus an optional muted description line) leading,
/// a [`gpui_component::switch::Switch`] trailing. The switch is the caller's —
/// it owns the id and the click listener — this only places it on the group's
/// row rhythm. Switches, never checkboxes: EXP-694 standardized the grouped
/// stacks on one control.
pub(crate) fn glass_toggle_row(
    label: impl Into<SharedString>,
    description: Option<SharedString>,
    switch: AnyElement,
    cx: &App,
) -> Div {
    let foreground = cx.theme().foreground;
    glass_row_shell()
        .child(
            v_flex()
                .flex_1()
                .min_w_0()
                .gap_0p5()
                .child(div().text_sm().text_color(foreground).child(label.into()))
                .children(description.map(|description| {
                    div()
                        .text_xs()
                        .text_color(foreground.opacity(0.5))
                        .child(description)
                })),
        )
        .child(switch)
}

/// EXP-694 — the EMBEDDED tab row (S3): a segmented strip stops being a
/// free-floating capsule above the card and becomes the group's FIRST ROW —
/// full width, no fill or border of its own, 8px of padding on every side, and
/// the hairline underneath comes from [`glass_group_rows`]. Fill it with
/// [`glass_tab_item`] segments, NOT with [`crate::controls::segmented`]'s
/// capsule container.
pub(crate) fn glass_tabs_row() -> Div {
    h_flex().w_full().items_center().gap_1().p_2()
}

/// One segment of a [`glass_tabs_row`]: [`crate::controls::segmented_item`]
/// carrying its own 7px vertical padding, since an embedded row has no fixed
/// capsule height for the segment to stretch into.
pub(crate) fn glass_tab_item(active: bool, cx: &App) -> Div {
    // `h_auto` undoes the capsule segment's `h_full`: there is no 36px
    // container height here, the segment's own padding sets the row height.
    crate::controls::segmented_item(active, cx)
        .h_auto()
        .py(px(7.))
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

/// EXP-568: a horizontal glass TRAY — the card recipe at row scale, holding a
/// WRAPPING group of chips as one object. The issue header's property cluster
/// wears it so the properties read as a unit beside the leading Start-coding
/// launcher.
///
/// The fill is `FILL_SECTION`, not the card's `FILL_CARD`: most chips inside
/// are ghost buttons (transparent at rest), but the static ones ([`glass_chip`]
/// — Origin, a single-board team's Board) already carry `FILL_CARD`, and
/// stacking that on itself composites near-opaque and reads as a different
/// material. The `STROKE_CARD` hairline keeps the tray's edge card-crisp.
pub(crate) fn glass_tray() -> Div {
    gpui_component::h_flex()
        .flex_wrap()
        .items_center()
        .gap_1()
        .px_1p5()
        .py_1()
        .rounded(px(t::radius::LG))
        .border_1()
        .border_color(t::glass::STROKE_CARD.to_hsla())
        .bg(t::glass::FILL_SECTION.to_hsla())
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
