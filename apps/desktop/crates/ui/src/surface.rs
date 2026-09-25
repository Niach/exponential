//! Glass surface recipes (EXP-269) — the GlassTheme.swift / Glass.kt parity
//! layer for the desktop. gpui has no in-scene backdrop blur, so glass is the
//! Android approximation: white-alpha fills + hairline strokes over the page
//! gradient (`theme::background_gradient()`), radii from the token ladder
//! (row 10 / section 12 / card 16). Strokes are 1px on purpose — fractional
//! hairlines vanish on 1x-scale displays.

use gpui::{
    div, prelude::FluentBuilder as _, px, Animation, AnimationExt as _, AnyElement, App, Div,
    ElementId, FontWeight, Hsla, InteractiveElement as _, IntoElement as _, ParentElement as _,
    SharedString, Stateful, StyleRefinement, Styled,
};
use gpui_component::input::Input;
use gpui_component::searchable_list::{SearchableListDelegate, SearchableListItem};
use gpui_component::select::Select;
use gpui_component::{h_flex, text::TextViewStyle, v_flex, ActiveTheme as _};
use theme::tokens as t;

/// The settings panes' spelling of [`glass_section_band`] — it IS the band
/// (`glass_section_band(None, label, trailing, cx)`), not a header of its
/// own: the plain-text heading this name used to draw was retired by
/// EXP-818, when every list page moved to the filled group strip. Kept
/// because the panes read better naming the thing "the section's header",
/// and because a settings pane never carries a leading glyph on it.
///
/// Labels are SENTENCE CASE, never uppercase, and there is no count slot
/// (EXP-698 retired header counts on every client). What sits under it is
/// the hairline ladder, never a gapped stack of cards (EXP-1076).
pub(crate) fn glass_section_header(
    label: impl Into<SharedString>,
    trailing: Option<AnyElement>,
    cx: &App,
) -> Div {
    glass_section_band(None, label, trailing, cx)
}

/// EXP-818: the GROUP BAND — the Linear group header. A full-width strip
/// filled `FILL_SECTION` with the group's name in it (an optional leading
/// glyph, an optional trailing control), sitting directly over its flat rows
/// ([`flat_row`]) with a 4px gap. It replaced the plain-text header + gapped
/// card rows on every list page (Devices, Agent, Actions, Automations,
/// Reviews, the settings lists): rows read as a table under a highlighted
/// header, not as a stack of cards. Web `GlassSectionHeader` twin.
pub(crate) fn glass_section_band(
    leading: Option<AnyElement>,
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
        .px_3()
        .py_1p5()
        .mb_1()
        .rounded(px(t::radius::MD))
        .bg(t::glass::FILL_SECTION.to_hsla())
        .children(leading)
        .child(
            div()
                .min_w_0()
                .truncate()
                .text_sm()
                .font_weight(FontWeight::MEDIUM)
                .text_color(foreground.opacity(0.85))
                .child(label.into()),
        )
        .child(div().flex_1())
        .children(trailing)
}

/// EXP-862 — the FOLDABLE group band: [`glass_section_band`] with a leading
/// chevron and a trailing count, the whole strip a click target.
///
/// Returns the band WITHOUT a click handler — what folding means is the
/// caller's (it owns the collapsed flag).
pub(crate) fn glass_section_band_fold(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    count: usize,
    collapsed: bool,
    cx: &App,
) -> Stateful<Div> {
    use gpui::IntoElement as _;
    use gpui_component::Sizable as _;
    let foreground = cx.theme().foreground;
    let chevron = gpui_component::Icon::from(if collapsed {
        crate::icons::registry::UI_CHEVRON_RIGHT
    } else {
        crate::icons::registry::UI_CHEVRON_DOWN
    })
    .xsmall()
    .flex_shrink_0()
    .text_color(foreground.opacity(0.7))
    .into_any_element();
    let count = div()
        .flex_shrink_0()
        .text_xs()
        .text_color(foreground.opacity(0.5))
        .child(SharedString::from(count.to_string()))
        .into_any_element();
    glass_section_band(Some(chevron), label, Some(count), cx)
        .id(id)
        .cursor_pointer()
}

/// EXP-818: ONE flat list row — the web `ListRow`. No stroke, no fill of its
/// own: rows stack with NO gap under a [`glass_section_band`] and read as a
/// table; the `list_hover` wash is the only thing a hover paints, and a
/// selected row takes `list_active` (the caller applies both — a row is a
/// plain div so every list keeps its own id, padding and click). This is
/// the row every list wears since EXP-818; [`glass_row_card`] is left for
/// the few real cards (a transcript's tool output, a diff).
pub(crate) fn flat_row() -> Div {
    div().rounded(px(t::radius::MD))
}

/// EXP-963: the COMPACT density of [`flat_row`] — the web `ListRow
/// density="compact"` / `SidebarMenuButton density="compact"` twin: the
/// 28px one-line row the narrow column runs at (the rail's entries, the
/// `ListNav` issue rows), the list's own 14px type, 8px of side padding and
/// 8px between the glyph and the text. Same fills as the list row: the
/// caller still applies the hover wash and the active fill.
pub(crate) fn flat_row_compact() -> Div {
    flat_row()
        .flex()
        .flex_row()
        .h(px(FLAT_ROW_COMPACT_H))
        .px_2()
        .gap_2()
        .items_center()
        .text_sm()
}

/// The compact row's height (web `h-7`).
pub(crate) const FLAT_ROW_COMPACT_H: f32 = 28.;

/// The count badge's tone (EXP-963, web `Badge tone`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum BadgeTone {
    /// A count you parked (the rail's drafts): muted fill.
    Muted,
    /// A count that wants you (unread): the accent fill.
    Primary,
}

/// Past this a [`count_badge`] reads `99+` (web `Badge max`).
pub(crate) const COUNT_BADGE_MAX: usize = 99;

/// EXP-963 — the COUNT badge, the web `Badge` twin: the smallest chip there
/// is, a 16px capsule carrying a NUMBER and nothing else, 10px semibold so
/// a count can climb without the box twitching. Zero renders NOTHING (a
/// badge is a signal, and an empty signal is noise), past
/// [`COUNT_BADGE_MAX`] it reads `99+`. PLACEMENT stays at the call site — a
/// row's trailing edge, a rail glyph's corner — the badge owns only its
/// shape. A `Pill` `Sm` is 24 tall and carries a word; this carries a
/// quantity.
pub(crate) fn count_badge(count: usize, tone: BadgeTone, cx: &App) -> Option<Div> {
    if count == 0 {
        return None;
    }
    let theme = cx.theme();
    let (fill, ink) = match tone {
        BadgeTone::Muted => (theme.muted, theme.muted_foreground),
        BadgeTone::Primary => (theme.primary, theme.primary_foreground),
    };
    let label = if count > COUNT_BADGE_MAX {
        format!("{COUNT_BADGE_MAX}+")
    } else {
        count.to_string()
    };
    Some(
        div()
            .flex()
            .flex_row()
            .flex_shrink_0()
            .h(px(16.))
            .min_w(px(16.))
            .px(px(4.))
            .items_center()
            .justify_center()
            .rounded_full()
            .bg(fill)
            .text_color(ink)
            .text_size(px(10.))
            .line_height(px(10.))
            .font_weight(FontWeight::SEMIBOLD)
            .child(SharedString::from(label)),
    )
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
/// (`@exp/ui glass-rows.tsx`: `rounded-md border border-glass-stroke
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

/// EXP-994 — the same hairline-divided ladder with NO group of its own: no
/// fill, no radius, no clip. For rows that already sit ON a surface (a
/// popover, a sheet), where a [`glass_group`] would draw a card inside a
/// card — the surface IS the edge, the hairlines are all the structure the
/// rows need.
pub(crate) fn glass_group_rows_bare(rows: Vec<Div>) -> Div {
    rows.into_iter()
        .enumerate()
        .fold(v_flex().w_full(), |group, (ix, row)| {
            group.child(if ix == 0 { row } else { glass_row_divider(row) })
        })
}

/// The row rhythm of a [`glass_group_rows_bare`] ladder: tighter than
/// [`glass_row_shell`] because a popover is not a settings pane — leading
/// label, trailing control, one line.
pub(crate) fn bare_row_shell() -> Div {
    h_flex()
        .w_full()
        .min_w_0()
        .items_center()
        .justify_between()
        .gap_3()
        .px_2()
        .py_1p5()
}

/// The hairline a [`glass_group`] row draws above itself — for the rare caller
/// that assembles a group by hand instead of through [`glass_group_rows`].
pub(crate) fn glass_row_divider<T: Styled>(row: T) -> T {
    row.border_t_1()
        .border_color(t::glass::STROKE_ROW.to_hsla())
}

/// EXP-994 — the same hairline for a LIST whose rows are built one by one
/// (the settings ladders, the machines list): every row but the FIRST draws
/// it, so a grouped list reads as one table instead of a stack of
/// free-floating rows — and nothing draws an outer card border around them.
/// A caller that already knows its index passes it; `ix == 0` is a no-op.
pub(crate) fn list_row_divider<T: Styled>(row: T, ix: usize) -> T {
    if ix == 0 {
        row
    } else {
        glass_row_divider(row)
    }
}

/// [`list_row_divider`] for a row the caller hands over as an opaque element
/// (most list rows return `impl IntoElement`): the hairline rides a
/// full-width wrapper instead of the row itself.
pub(crate) fn list_row(row: impl gpui::IntoElement, ix: usize) -> Div {
    list_row_divider(div().w_full().min_w_0(), ix).child(row)
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

/// How narrow a picker row's trailing CONTROL column may get (EXP-830). The
/// label column sizes to its content (the description up to
/// [`ROW_DESCRIPTION_MAX_W`]) and SHRINKS when the row cannot hold both, so a
/// long hint wraps onto a second line instead of crowding the picker: a
/// [`Select`]'s trigger IS its menu width (`menu_width: Auto`), so a trigger
/// squeezed to a few glyphs opened a menu that clipped every option to its
/// first letters, and the device editor's "Shared with" row did exactly that
/// in a narrow settings pane. 200 seats the longest builtin value with its
/// caret and keeps the menu wide enough to read a team name.
const PICKER_CONTROL_MIN_W: f32 = 200.;

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
                // `min_w_0`: gpui measures min-content text UNWRAPPED, so
                // without it the column could never shrink below the hint's
                // single-line width and nothing would ever wrap.
                .min_w_0()
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
                .min_w(px(PICKER_CONTROL_MIN_W))
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
///
/// Insets are the web tray's (`issue-properties-panel.tsx`: `px-3 py-2
/// gap-1.5`) — 12 / 8 / 6 px, as `px()` literals because gpui's rem is 14.
pub(crate) fn glass_tray() -> Div {
    gpui_component::h_flex()
        .flex_wrap()
        .items_center()
        .gap(px(6.))
        .px(px(12.))
        .py(px(8.))
        .rounded(px(t::radius::LG))
        .border_1()
        .border_color(t::glass::STROKE_CARD.to_hsla())
        .bg(t::glass::FILL_SECTION.to_hsla())
}

/// EXP-698 round 5 — the ONE bulk-action bar chrome, shared with web
/// (`bg-glass-card-opaque` + `border-glass-stroke-strong`, radius 24) and the
/// two mobile bars: a single OPAQUE capsule that floats over the list it acts
/// on. Opaque is the point — a translucent bar would show the rows it covers
/// sliding underneath it (and gpui has no in-scene backdrop blur), so the fill
/// is `theme.popover`, the shared glass-menu fill (`FILL_CARD` composited over
/// `POPOVER`, byte-equal to Android's `GlassTokens.OpaqueCardFill`).
///
/// It does NOT wrap: the bar is a single fixed-height capsule, one row on
/// every surface. When the labeled row does not fit, the CALLER collapses its
/// buttons to icon-only with tooltips instead of flowing them onto a second
/// line (`issue_list::render_bulk_bar` and its `bulk_bar_label_min_width` gate) —
/// a two-line bar would change the host row's height under the list.
pub(crate) fn glass_bar(cx: &App) -> Div {
    h_flex()
        .items_center()
        .gap_1()
        .px_2p5()
        .py_2()
        .rounded(px(t::radius::XL3))
        .border_1()
        .border_color(t::glass::STROKE_STRONG.to_hsla())
        .bg(cx.theme().popover)
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
    /// EXP-926 — the toggle's own height (`CONTROL_LG`, the web `h-9`
    /// `TabsList`). The ONE place it is worn: a work-header action standing
    /// BESIDE the face toggle, where a shorter capsule read as a stray chip
    /// floating next to the control.
    Lg,
    Md,
    Sm,
}

impl PillSize {
    /// The capsule's height in px.
    pub(crate) fn height(self) -> f32 {
        match self {
            PillSize::Lg => t::size::CONTROL_LG,
            PillSize::Md => t::size::CONTROL_MD,
            PillSize::Sm => t::size::CONTROL_SM,
        }
    }

    /// The size a LEADING glyph renders at inside the capsule.
    pub(crate) fn glyph(self) -> f32 {
        match self {
            PillSize::Lg | PillSize::Md => 16.,
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
///
/// Orthogonal to all three is the PRIMARY paint flag (EXP-698, mirrored ×4 as
/// the web `<Pill primary>` / mobile `GlassPill(primary:)` prop): the ONE
/// emphasised capsule of a surface — solid `theme.primary`,
/// `primary_foreground` text, no stroke, a darker hover. It changes nothing
/// but paint: geometry, type and glyph size stay the pill's. A surface gets at
/// most one (the issue header's Start coding, the coding-now card's Watch);
/// everything else beside it stays the default glass fill, which is what makes
/// the primary one read as the action.
///
/// It rides [`glass_pill_button_primary`] rather than a `.primary()` chained
/// onto [`glass_pill_button`]: the name is already taken on `Button` by
/// gpui-component's `ButtonVariants::primary`, which every call site imports,
/// so a same-named extension method would only make every call ambiguous.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum PillMode {
    Action,
    Select { selected: bool },
    Readonly,
}

/// EXP-698 — the ONE capsule of the desktop, the twin of the web
/// `@exp/ui pill.tsx` and the mobile `GlassPill`s. Every chip, tag,
/// badge, filter pill, header button and picker trigger that used to be its
/// own recipe (`glass_chip`, the old two-arg `glass_pill`, `pickers::
/// chip_button`, `issue_list::label_chip`,
/// `settings/members::role_chip`, the two
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
        PillSize::Lg | PillSize::Md => (12., 6.),
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
        // EXP-926: `web_md` is the 36px control; the capsule shape is this
        // recipe's, so the radius comes back on.
        PillSize::Lg => button.web_md().rounded_full(),
    }
}

/// The PRIMARY paint of [`glass_pill_button`] (see [`PillMode`]): the same
/// capsule geometry, filled solid with `theme.primary` under
/// `primary_foreground` text and NO stroke.
///
/// The fill rides gpui-component's own `Primary` variant instead of a
/// [`custom_variant_fill`]ed `ButtonCustomVariant`: the variant already owns
/// the accent's hover/active pair (`button_primary_hover/_active`, a notch
/// darker) and paints its border in the fill colour, i.e. strokeless. That is
/// also why this is a separate recipe rather than a flag threaded through
/// [`glass_pill_button`] — that one has to override the border to get the
/// glass hairline, and an override cannot un-set itself.
pub(crate) fn glass_pill_button_primary(
    id: impl Into<ElementId>,
    size: PillSize,
) -> gpui_component::button::Button {
    use crate::controls::WebControl as _;
    use gpui_component::button::ButtonVariants as _;
    let button = gpui_component::button::Button::new(id).primary();
    match size {
        PillSize::Sm => button.web_xs(),
        PillSize::Md => button.web_sm(),
        PillSize::Lg => button.web_md().rounded_full(),
    }
}

/// The 6px colour dot a [`glass_pill`] carries instead of a glyph when the
/// thing it names IS a colour (an issue label, a custom status, a session's
/// liveness tone).
pub(crate) fn pill_dot(color: Hsla) -> Div {
    div().flex_shrink_0().size(px(6.)).rounded_full().bg(color)
}

/// The live dot's disc (web `size-2`).
pub(crate) const LIVE_DOT_PX: f32 = 8.;
/// One ripple of the ping halo — the web `animate-ping` period (1s).
const LIVE_DOT_PING: std::time::Duration = std::time::Duration::from_secs(1);

/// EXP-970 — the live dot, the web `LiveDot` twin (iOS `SessionStateDot`,
/// Android `LiveDot`): a session's state in one 8px disc, tinted by the ONE
/// tone table (`queries::session_dot_tone`), with the ATTENTION halo behind
/// it when `ping` is set. The halo is a second disc of the same tone that
/// grows from the dot to twice its size while fading from 60% to nothing,
/// once a second on the decelerate curve — the CSS `animate-ping` recipe
/// (`cubic-bezier(0, 0, 0.2, 1)` IS the ladder's `DECELERATE`), so the
/// desktop ripples exactly as the web does. gpui animates by repainting, so
/// a pinging dot is a live element: reserve it for something happening
/// right now (the agent mid-turn, `queries::session_agent_busy`), never for
/// `running` alone — a live-but-idle run draws the steady disc.
///
/// The container is exactly the disc's box; the halo is an absolute child,
/// so the ripple paints OVER the neighbours without moving them.
pub(crate) fn live_dot(tone: Hsla, ping: bool) -> AnyElement {
    let disc = div().size(px(LIVE_DOT_PX)).rounded_full().bg(tone);
    if !ping {
        return disc.flex_shrink_0().into_any_element();
    }
    div()
        .relative()
        .flex_shrink_0()
        .size(px(LIVE_DOT_PX))
        .child(
            div()
                .absolute()
                .rounded_full()
                .bg(tone)
                .with_animation(
                    "live-dot-ping",
                    Animation::new(LIVE_DOT_PING)
                        .repeat()
                        .with_easing(theme::motion::decelerate()),
                    |halo, delta| {
                        let grow = LIVE_DOT_PX * delta;
                        halo.left(px(-grow / 2.))
                            .top(px(-grow / 2.))
                            .size(px(LIVE_DOT_PX + grow))
                            .opacity(0.6 * (1. - delta))
                    },
                ),
        )
        .child(disc.absolute().left_0().top_0())
        .into_any_element()
}

// ---------------------------------------------------------------------------
// The ONE rich tab (EXP-698)
// ---------------------------------------------------------------------------

/// The leading marker of a [`rich_tab`].
pub(crate) enum RichTabStatus {
    /// A status/agent glyph, already coloured by the caller
    /// (`icons::resolved_status_icon`, `ChipLead::icon`).
    Glyph(gpui_component::Icon),
    /// A liveness tone dot (`queries::session_dot_tone`) — every run chip.
    Dot(Hsla),
    None,
}

/// How wide a [`rich_tab`] may grow before its title truncates — the chip's
/// whole `max-w`, web `DockTab`'s 240px (EXP-877).
pub(crate) const RICH_TAB_MAX_W: f32 = 240.;

/// How wide a [`rich_tab`]'s title may grow before it truncates. The chip cap
/// above bounds the row; this bounds the title inside it.
pub(crate) const RICH_TAB_TITLE_MAX_W: f32 = 180.;

/// The content of a [`rich_tab`]. Handlers stay the CALLER's: the two strips
/// differ on middle-click, context menus and the hover-revealed undock, and
/// folding those in here would make the builder a switchboard.
pub(crate) struct RichTab {
    pub(crate) id: ElementId,
    pub(crate) selected: bool,
    pub(crate) status: RichTabStatus,
    /// The mono shortcode ahead of the title (`EXP-698`), muted.
    pub(crate) identifier: Option<SharedString>,
    pub(crate) title: Option<SharedString>,
    /// A tinted exit-code badge (a terminal chip whose child exited).
    pub(crate) badge: Option<(SharedString, Hsla)>,
    /// EXP-905: whether the caller appends the trailing ghost × cluster. A
    /// chip WITHOUT one (a live run, EXP-877) pads its right side like its
    /// left ([`rich_tab_padding`]) instead of jamming the label against the
    /// border.
    pub(crate) closable: bool,
}

/// EXP-905 — a [`rich_tab`]'s `(left, right)` padding in px: `pl 8 / pr 4`
/// when the 24px ghost × trails the label (its own box supplies the air),
/// `pl 8 / pr 8` when nothing does. Byte-identical with the web `DockTab`
/// (`pl-2 pr-1` / `pl-2 pr-2`); the strip's width measurer reads the same
/// pair, so a live chip is measured exactly as wide as it paints.
pub(crate) fn rich_tab_padding(closable: bool) -> (f32, f32) {
    if closable { (8., 4.) } else { (8., 8.) }
}

impl RichTab {
    pub(crate) fn new(id: impl Into<ElementId>, selected: bool) -> Self {
        Self {
            id: id.into(),
            selected,
            status: RichTabStatus::None,
            identifier: None,
            title: None,
            badge: None,
            closable: true,
        }
    }
}

/// EXP-698/EXP-877 — the ONE RICH tab, now byte-identical with the web chip:
/// 32px tall, 6px radius, capped at [`RICH_TAB_MAX_W`], `pl 8 / pr 4` (the
/// short right side is the 24px ghost × the caller appends; `pr 8` on a chip
/// with no ×, EXP-905 [`rich_tab_padding`]), `gap 6`, a 14px
/// lead box holding a 14px glyph or an 8px dot, the mono `text_xs` identifier
/// and the `text_sm` truncating title.
///
/// Three states and no more: idle = transparent chrome + muted text, hover =
/// the glass ACTIVE fill + foreground, active = the same active fill inside a
/// card hairline + foreground (the panel fill read as "not selected" against
/// the bare ground). EXP-877 retired the ` · machine` caption (a tab is
/// chrome, and the machine is on the run's own header), the paused dimming
/// (a chip that dims reads as disabled) and the working spinner (the steady
/// liveness dot carries it; the spinner is the LIST's, EXP-848).
///
/// gpui's rem is 14px ([`theme::FONT_SIZE_PX`]), so every one of these is a
/// `px()` literal — the spacing helpers would resolve 8px as `px_2` only by
/// coincidence of the rem.
pub(crate) fn rich_tab(tab: RichTab, cx: &App) -> Stateful<Div> {
    let theme = cx.theme();
    let (pad_left, pad_right) = rich_tab_padding(tab.closable);
    let chip = div()
        .id(tab.id)
        .h(px(32.))
        .max_w(px(RICH_TAB_MAX_W))
        .pl(px(pad_left))
        .pr(px(pad_right))
        .flex()
        .flex_none()
        .items_center()
        .gap(px(6.))
        .rounded(px(6.))
        .border_1()
        .border_color(gpui::transparent_black())
        .cursor_pointer();
    let chip = if tab.selected {
        chip.border_color(t::glass::STROKE_CARD.to_hsla())
            .bg(t::glass::FILL_ACTIVE.to_hsla())
            .text_color(theme.foreground)
    } else {
        chip.text_color(theme.muted_foreground).hover(|style| {
            style
                .bg(t::glass::FILL_ACTIVE.to_hsla())
                .text_color(theme.foreground)
        })
    };
    // ONE 14px lead box, so a dot chip and a glyph chip line their titles up
    // (the strip's `lead_reserve_px` measures this box, not its content).
    chip.map(|chip| match tab.status {
        RichTabStatus::None => chip,
        status => chip.child(
            div()
                .flex_shrink_0()
                .size(px(14.))
                .flex()
                .items_center()
                .justify_center()
                .map(|slot| match status {
                    RichTabStatus::Glyph(icon) => {
                        slot.child(gpui_component::Sizable::with_size(icon, px(14.)))
                    }
                    // EXP-970: the shared disc; a chip never pings (EXP-877:
                    // the strip re-rendering on every turn edge was motion
                    // without information).
                    RichTabStatus::Dot(tone) => slot.child(live_dot(tone, false)),
                    RichTabStatus::None => slot,
                }),
        ),
    })
    .children(tab.identifier.map(|identifier| {
        div()
            .flex_shrink_0()
            .text_xs()
            .font_family(theme::terminal::FONT_FAMILY)
            .whitespace_nowrap()
            .child(identifier)
    }))
    .children(tab.title.map(|title| {
        div()
            .max_w(px(RICH_TAB_TITLE_MAX_W))
            .truncate()
            .text_sm()
            .font_weight(gpui::FontWeight::NORMAL)
            .child(title)
    }))
    .children(tab.badge.map(|(label, color)| {
        div()
            .flex_shrink_0()
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
    /// rather than a placeholder box (the bottom bar's terminal tabs carry no
    /// identifier, the top strip's chips no badge).
    #[test]
    fn rich_tab_builder_defaults_to_identity_only() {
        let tab = RichTab::new("t", true);
        assert!(tab.selected);
        assert!(matches!(tab.status, RichTabStatus::None));
        assert!(tab.identifier.is_none());
        assert!(tab.title.is_none());
        assert!(tab.badge.is_none());
        assert!(tab.closable, "a chip carries its × unless the caller says not");
    }

    /// EXP-905: a chip with no × (a live run) pads its right side like its
    /// left — the label used to touch the border — while a closable chip
    /// keeps the short right side its 24px × fills. Web `DockTab` twin:
    /// `pl-2 pr-2` / `pl-2 pr-1`.
    #[test]
    fn a_chip_without_a_close_button_pads_both_sides_equally() {
        assert_eq!(rich_tab_padding(false), (8., 8.));
        assert_eq!(rich_tab_padding(true), (8., 4.));
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
    fn rich_tab_caps_are_shared_with_the_strip_measurement() {
        // `screens::measure_chip_width` reads these for its overflow
        // computation; a divergence collapses tabs into "+N" too early.
        // EXP-877: 240px is the WEB chip's cap, byte-identical ×2.
        assert_eq!(RICH_TAB_MAX_W, 240.);
        assert_eq!(RICH_TAB_TITLE_MAX_W, 180.);
        assert!(RICH_TAB_TITLE_MAX_W < RICH_TAB_MAX_W);
    }
}
