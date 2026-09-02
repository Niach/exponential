//! Glass surface recipes (EXP-269) — the GlassTheme.swift / Glass.kt parity
//! layer for the desktop. gpui has no in-scene backdrop blur, so glass is the
//! Android approximation: white-alpha fills + hairline strokes over the page
//! gradient (`theme::background_gradient()`), radii from the token ladder
//! (row 10 / section 12 / card 16). Strokes are 1px on purpose — fractional
//! hairlines vanish on 1x-scale displays.

use gpui::{
    div, prelude::FluentBuilder as _, px, AnyElement, App, Div, ElementId, FontWeight, Hsla,
    InteractiveElement as _, ParentElement as _, SharedString, Stateful, StyleRefinement, Styled,
};
use gpui_component::input::Input;
use gpui_component::searchable_list::{SearchableListDelegate, SearchableListItem};
use gpui_component::select::Select;
use gpui_component::{h_flex, text::TextViewStyle, v_flex, ActiveTheme as _};
use theme::tokens as t;

/// EXP-698 — the web `GlassSectionHeader` (`components/ui/glass-rows.tsx`,
/// EXP-616): a PLAIN-TEXT heading over a glass list — no band, no fill, no
/// border — `px_1 pt_1 pb_2`, the label `text_sm` MEDIUM at 70% foreground,
/// then a spacer and the optional trailing control. No count slot: EXP-698
/// retired header counts on every client. Labels are SENTENCE CASE, never
/// uppercase.
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

/// How wide a row's muted DESCRIPTION line may grow before it wraps. The
/// settings panes are ~550px, and a hint set full-bleed across one reads as a
/// paragraph rather than as a caption under its label — the deleted
/// `notifications_prefs::pref_row` capped it here, so the recipe does it for
/// every consumer.
pub(crate) const ROW_DESCRIPTION_MAX_W: f32 = 460.;

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
                        .max_w(px(ROW_DESCRIPTION_MAX_W))
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
                        .max_w(px(ROW_DESCRIPTION_MAX_W))
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
    // EXP-698: no inter-segment gap — the segments abut like the web
    // `TabsList`, and the capsule's own fill is what separates them.
    h_flex().w_full().items_center().p_2()
}

/// One segment of a [`glass_tabs_row`]: [`crate::controls::segmented_item`]
/// carrying its own 7px vertical padding, since an embedded row has no fixed
/// capsule height for the segment to stretch into.
pub(crate) fn glass_tab_item(active: bool, cx: &App) -> Div {
    // `h_auto` undoes the capsule segment's `h_full`: there is no 36px
    // container height here, the segment's own padding sets the row height.
    crate::controls::segmented_item(active, cx)
        .h_auto()
        .py(px(6.))
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


// ---------------------------------------------------------------------------
// The ONE pill (EXP-698)
// ---------------------------------------------------------------------------

/// The two pill rungs of the control ladder: `Md` is the 32px control box
/// (`size::CONTROL_MD`), `Sm` the 24px one (`size::CONTROL_SM`). There is no
/// third rung — a capsule smaller than 24 stops being a hit target.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum PillSize {
    Md,
    Sm,
}

impl PillSize {
    /// The capsule's height in px.
    pub(crate) fn height(self) -> f32 {
        match self {
            PillSize::Md => t::size::CONTROL_MD,
            PillSize::Sm => t::size::CONTROL_SM,
        }
    }

    /// The size a LEADING glyph renders at inside the capsule.
    pub(crate) fn glyph(self) -> f32 {
        match self {
            PillSize::Md => 16.,
            PillSize::Sm => 12.,
        }
    }
}

/// What a pill DOES, which is the only thing that varies its chrome:
///
/// - `Action` — it runs something on click (a header button, a picker
///   trigger, a filter pill's ✕). Hover lifts it to the active fill.
/// - `Select { selected }` — it is one option of a set: the sidebar's tool
///   tabs (Inbox / My issues, Open / Resolved) and the helpdesk composer's
///   Reply / Internal note modes. The selected one wears the active fill +
///   stroke. (The steer viewer has no tab strip to convert — subagent work
///   renders inline there; see its module doc.)
/// - `Readonly` — it only LABELS something (a role, a label, an attachment,
///   a count badge). No hover, no pointer cursor.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum PillMode {
    Action,
    Select { selected: bool },
    Readonly,
}

/// EXP-698 — the ONE capsule of the desktop, the twin of the web
/// `components/ui/pill.tsx` and the mobile `GlassPill`s. Every chip, tag,
/// badge, filter pill, header button and picker trigger that used to be its
/// own recipe (`glass_chip`, the old two-arg `glass_pill`, `pickers::
/// chip_button`, `active_filter_pills::pill_base`, `issue_list::label_chip`,
/// `settings/members::role_chip`, `filter_popover::count_badge`, the two
/// `file_chip`s, `pending_chip`) is this function with a different
/// [`PillSize`] / [`PillMode`].
///
/// Chrome: capsule, `FILL_CARD` over a 1px `STROKE_CARD` hairline, label at
/// 70% foreground. `Sm` is `text_xs` MEDIUM with 8px of side padding and a
/// 4px gap; `Md` is `text_sm` with 12/6. A leading glyph (sized
/// [`PillSize::glyph`]) or a [`pill_dot`] is the CALLER's child — the pill
/// only owns the box.
///
/// Returns a `Stateful<Div>` in every mode, `Readonly` included: the id is
/// free (gpui needs one for any element that may host a tooltip or a
/// context menu) and it keeps one return type, so a caller can flip a pill
/// between modes without rewriting the element chain. `Readonly` simply
/// carries no hover style and no pointer cursor.
pub(crate) fn glass_pill(
    id: impl Into<ElementId>,
    size: PillSize,
    mode: PillMode,
    cx: &App,
) -> Stateful<Div> {
    let foreground = cx.theme().foreground;
    let selected = matches!(mode, PillMode::Select { selected: true });
    let (fill, stroke) = if selected {
        (t::glass::FILL_ACTIVE, t::glass::STROKE_ACTIVE)
    } else {
        (t::glass::FILL_CARD, t::glass::STROKE_CARD)
    };
    let (px_pad, gap) = match size {
        PillSize::Md => (12., 6.),
        PillSize::Sm => (8., 4.),
    };
    let pill = div()
        .id(id)
        .flex()
        .flex_row()
        .flex_shrink_0()
        .items_center()
        .h(px(size.height()))
        .px(px(px_pad))
        .gap(px(gap))
        .rounded_full()
        .border_1()
        .border_color(stroke.to_hsla())
        .bg(fill.to_hsla())
        .whitespace_nowrap()
        .when(size == PillSize::Sm, |pill| {
            pill.text_xs().font_weight(FontWeight::MEDIUM)
        })
        .when(size == PillSize::Md, |pill| pill.text_sm());
    match mode {
        PillMode::Readonly => pill.text_color(foreground.opacity(0.7)),
        PillMode::Select { selected: true } => pill.cursor_pointer().text_color(foreground),
        PillMode::Select { selected: false } => pill
            .cursor_pointer()
            .text_color(foreground.opacity(0.7))
            .hover(|style| style.text_color(foreground)),
        PillMode::Action => pill
            .cursor_pointer()
            .text_color(foreground.opacity(0.7))
            .hover(|style| {
                style
                    .bg(t::glass::FILL_ACTIVE.to_hsla())
                    .text_color(foreground)
            }),
    }
}

/// How much of a `ButtonVariant::Custom` colour actually reaches the screen.
///
/// gpui-component paints a custom variant's rest/`outline` background as
/// `color.mix_oklab(transparent, 0.2)` (`button.rs`, `bg_color` /
/// `outline_background`), and that mix weights SELF by the factor —
/// `a = self.a * factor + other.a * (1 - factor)` — so a custom colour is
/// painted at a FIFTH of its alpha. Handing it `FILL_CARD` (white 6%)
/// directly paints white ~1.2%, i.e. a Button pill would be all but
/// invisible beside the `glass_pill` `Div` next to it.
const CUSTOM_VARIANT_ALPHA_FACTOR: f32 = 0.2;

/// Pre-divide a glass fill so [`CUSTOM_VARIANT_ALPHA_FACTOR`] mixes it back
/// to the token: a Button pill and a `Div` pill then paint the SAME surface.
/// Only the alpha moves — `mix_oklab` premultiplies in Oklab and
/// un-premultiplies by the result alpha, so mixing a colour with transparent
/// leaves hue/saturation/lightness untouched.
pub(crate) fn custom_variant_fill(fill: Hsla) -> Hsla {
    Hsla {
        a: (fill.a / CUSTOM_VARIANT_ALPHA_FACTOR).min(1.),
        ..fill
    }
}

/// The [`glass_pill`] chrome on a gpui-component `Button`.
///
/// Most capsules in the app are plain elements and take [`glass_pill`]. The
/// ones that are MENU or POPOVER triggers cannot: `DropdownMenu` is
/// implemented only for `Button` upstream (`Selectable + InteractiveElement`
/// bounds a `Stateful<Div>` does not satisfy), and every picker chip in the
/// issue header and the create dialog is such a trigger. So they stay
/// `Button`s wearing the pill's paint.
///
/// The paint rides a `ButtonCustomVariant`, not a `ghost` base plus a caller
/// refinement: the built-in variants paint their own hover fill from the
/// interactivity layer, which is applied AFTER `refine_style` and would win
/// over any glass tokens set here (and a second `.hover()` on the button
/// trips gpui's "hover style already set" assertion). The custom variant owns
/// bg/hover/active; only the stroke rides as a refinement. Same trick as
/// [`crate::controls::glass_icon_button`], which is this pill's icon-only
/// sibling.
pub(crate) fn glass_pill_button(
    id: impl Into<ElementId>,
    size: PillSize,
    cx: &App,
) -> gpui_component::button::Button {
    use crate::controls::WebControl as _;
    use gpui_component::button::{ButtonCustomVariant, ButtonVariants as _};
    let foreground = cx.theme().foreground;
    let variant = ButtonCustomVariant::new(cx)
        .color(custom_variant_fill(t::glass::FILL_CARD.to_hsla()))
        .hover(custom_variant_fill(t::glass::FILL_ACTIVE.to_hsla()))
        .active(custom_variant_fill(t::glass::FILL_ACTIVE.to_hsla()))
        .foreground(foreground.opacity(0.7));
    let button = gpui_component::button::Button::new(id)
        .custom(variant)
        .border_1()
        .border_color(t::glass::STROKE_CARD.to_hsla());
    match size {
        PillSize::Sm => button.web_xs(),
        PillSize::Md => button.web_sm(),
    }
}

/// The 6px colour dot a [`glass_pill`] carries instead of a glyph when the
/// thing it names IS a colour (an issue label, a custom status, a session's
/// liveness tone).
pub(crate) fn pill_dot(color: Hsla) -> Div {
    div().flex_shrink_0().size(px(6.)).rounded_full().bg(color)
}

// ---------------------------------------------------------------------------
// The ONE rich tab (EXP-698)
// ---------------------------------------------------------------------------

/// The leading marker of a [`rich_tab`].
pub(crate) enum RichTabStatus {
    /// A status/agent glyph, already coloured by the caller
    /// (`icons::resolved_status_icon`, `ChipLead::icon`).
    Glyph(gpui_component::Icon),
    /// A liveness tone dot (the remote session chips).
    Dot(Hsla),
    None,
}

/// How wide a [`rich_tab`]'s title may grow before it truncates.
pub(crate) const RICH_TAB_TITLE_MAX_W: f32 = 180.;

/// How wide a [`rich_tab`]'s trailing caption (` · machine`) may grow.
pub(crate) const RICH_TAB_CAPTION_MAX_W: f32 = 110.;

/// The content of a [`rich_tab`]. Handlers stay the CALLER's: the three
/// strips differ on middle-click, context menus, kill-confirms and the
/// hover-revealed undock, and folding those in here would make the builder a
/// switchboard.
pub(crate) struct RichTab {
    pub(crate) id: ElementId,
    pub(crate) selected: bool,
    /// A paused host's chip dims whole (EXP-696).
    pub(crate) paused: bool,
    pub(crate) status: RichTabStatus,
    /// The mono shortcode ahead of the title (`EXP-698`), 50% foreground.
    pub(crate) identifier: Option<SharedString>,
    pub(crate) title: Option<SharedString>,
    /// A trailing muted caption (` · machine`) after the title.
    pub(crate) caption: Option<SharedString>,
    /// A tinted exit-code badge.
    pub(crate) badge: Option<(SharedString, Hsla)>,
}

impl RichTab {
    pub(crate) fn new(id: impl Into<ElementId>, selected: bool) -> Self {
        Self {
            id: id.into(),
            selected,
            paused: false,
            status: RichTabStatus::None,
            identifier: None,
            title: None,
            caption: None,
            badge: None,
        }
    }
}

/// EXP-698 — the ONE RICH tab: the only tab shape left that is not a
/// [`glass_pill`], because it carries a whole row of content (status glyph,
/// mono identifier, truncating title, machine caption, exit badge, close/
/// undock buttons) rather than a word. It is worn by exactly two strips: the
/// window's top screen tabs (`screens::render_tab_strip`) and the terminal
/// dock's local + remote chips.
///
/// Chrome is the retired `tab_chip`'s, unchanged: 26px tall, radius MD,
/// `px_2p5 gap_1p5 text_sm`, transparent at rest with the glass row fill on
/// hover, `tab_active` (== `FILL_ACTIVE`) when selected.
///
/// The returned element already carries the standard children in order; the
/// caller appends its own trailing cluster (close, undock) and every handler.
pub(crate) fn rich_tab(tab: RichTab, cx: &App) -> Stateful<Div> {
    let theme = cx.theme();
    let foreground = theme.foreground;
    let chip = div()
        .id(tab.id)
        .h(px(26.))
        .px_2p5()
        .flex()
        .flex_none()
        .items_center()
        .gap_1p5()
        .rounded(theme.radius)
        .cursor_pointer()
        .text_sm()
        .when(tab.paused, |chip| chip.opacity(0.6));
    let chip = if tab.selected {
        chip.bg(theme.tab_active)
            .text_color(theme.tab_active_foreground)
    } else {
        chip.text_color(theme.tab_foreground)
            .hover(|style| style.bg(theme.list_hover))
    };
    chip.map(|chip| match tab.status {
        RichTabStatus::Glyph(icon) => {
            chip.child(gpui_component::Sizable::xsmall(icon))
        }
        RichTabStatus::Dot(tone) => chip.child(
            div()
                .flex_shrink_0()
                .size_1p5()
                .rounded_full()
                .bg(tone),
        ),
        RichTabStatus::None => chip,
    })
    .children(tab.identifier.map(|identifier| {
        div()
            .text_xs()
            .text_color(foreground.opacity(0.5))
            .font_family(theme::terminal::FONT_FAMILY)
            .whitespace_nowrap()
            .child(identifier)
    }))
    .children(tab.title.map(|title| {
        div()
            .max_w(px(RICH_TAB_TITLE_MAX_W))
            .truncate()
            .child(title)
    }))
    .children(tab.caption.map(|caption| {
        div()
            .max_w(px(RICH_TAB_CAPTION_MAX_W))
            .truncate()
            .text_xs()
            .text_color(foreground.opacity(0.5))
            .child(caption)
    }))
    .children(tab.badge.map(|(label, color)| {
        div()
            .text_xs()
            .px_1()
            .rounded(px(3.))
            .bg(color.opacity(0.15))
            .text_color(color)
            .child(label)
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// EXP-698: the two pill rungs ARE the token ladder's control rungs — no
    /// hand-typed 20/26/28 heights, which is how the app ended up with six
    /// chip shapes before this sweep.
    #[test]
    fn pill_sizes_are_the_token_control_rungs() {
        assert_eq!(PillSize::Md.height(), t::size::CONTROL_MD);
        assert_eq!(PillSize::Sm.height(), t::size::CONTROL_SM);
        assert_eq!(PillSize::Md.height(), 32.);
        assert_eq!(PillSize::Sm.height(), 24.);
        // A leading glyph is half the capsule's height, both rungs.
        assert_eq!(PillSize::Md.glyph(), 16.);
        assert_eq!(PillSize::Sm.glyph(), 12.);
        assert!(PillSize::Sm.glyph() < PillSize::Md.glyph());
    }

    /// `Select` is the only mode whose chrome depends on state; `Action` and
    /// `Readonly` are single-valued. (The chrome itself needs a `Window` to
    /// render, so this pins the discriminants the builder switches on.)
    #[test]
    fn pill_select_mode_carries_its_selection() {
        assert_ne!(
            PillMode::Select { selected: true },
            PillMode::Select { selected: false }
        );
        assert_ne!(PillMode::Action, PillMode::Readonly);
        assert_ne!(PillMode::Action, PillMode::Select { selected: false });
    }

    /// The rich-tab builder starts EMPTY apart from its identity: every strip
    /// fills only the slots it has, and an unset slot must render nothing
    /// rather than a placeholder box (the terminal dock's plain terminal tabs
    /// carry no identifier, the center tabs carry no caption or badge).
    #[test]
    fn rich_tab_builder_defaults_to_identity_only() {
        let tab = RichTab::new("t", true);
        assert!(tab.selected);
        assert!(!tab.paused);
        assert!(matches!(tab.status, RichTabStatus::None));
        assert!(tab.identifier.is_none());
        assert!(tab.title.is_none());
        assert!(tab.caption.is_none());
        assert!(tab.badge.is_none());
    }

    /// EXP-698: a Button pill and a `Div` pill must paint the SAME surface.
    /// gpui-component runs a custom variant's colour through
    /// `mix_oklab(transparent, 0.2)` before painting it, so the pre-division
    /// in [`custom_variant_fill`] has to mix back to the token exactly —
    /// asserted with the crate's OWN mix, so an upstream change to either the
    /// factor or the mix semantics fails here instead of on screen.
    #[test]
    fn custom_variant_fills_mix_back_to_the_glass_tokens() {
        use gpui_component::theme::Colorize as _;
        let transparent = gpui::transparent_black();
        for token in [t::glass::FILL_CARD, t::glass::FILL_ACTIVE] {
            let want = token.to_hsla();
            let painted = custom_variant_fill(want).mix_oklab(transparent, CUSTOM_VARIANT_ALPHA_FACTOR);
            assert!(
                (painted.a - want.a).abs() < 0.001,
                "compensated fill must paint at the token alpha: painted {:?} vs token {:?}",
                painted,
                want,
            );
            assert!(
                (painted.l - want.l).abs() < 0.01,
                "the mix must leave lightness alone: painted {:?} vs token {:?}",
                painted,
                want,
            );
        }
        // The naive (uncompensated) hand-off is what this guards against.
        let naive = t::glass::FILL_CARD
            .to_hsla()
            .mix_oklab(transparent, CUSTOM_VARIANT_ALPHA_FACTOR);
        assert!(
            naive.a < t::glass::FILL_CARD.to_hsla().a * 0.5,
            "sanity: passing the token straight through paints it far too faint ({naive:?})"
        );
    }

    #[test]
    fn rich_tab_title_cap_is_shared_with_the_strip_measurement() {
        // `screens::measure_chip_width` reads this constant for its overflow
        // computation; a divergence collapses tabs into "+N" too early.
        assert_eq!(RICH_TAB_TITLE_MAX_W, 180.);
        assert_eq!(RICH_TAB_CAPTION_MAX_W, 110.);
    }
}
