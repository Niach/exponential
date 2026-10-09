//! Glass surface recipes (EXP-269) — the GlassTheme.swift / Glass.kt parity
//! layer for the desktop: group bands, flat rows, cards, inset groups and
//! their hairlines, the form rows (picker, input, toggle, tabs), the
//! segmented capsule, the property row, the bulk bar and the disclosure
//! header. gpui has no in-scene backdrop blur, so glass is the Android
//! approximation: alpha fills + hairline strokes over the page, radii from
//! the chrome's ladder (row 10 / section 12 / card 16). Strokes are 1px on
//! purpose — fractional hairlines vanish on 1x-scale displays.

use gpui::{
    div, px, AnyElement, App, Div, ElementId, FontWeight, InteractiveElement as _,
    IntoElement, ParentElement as _, SharedString, Stateful, Styled,
};
use gpui_component::input::Input;
use gpui_component::searchable_list::{SearchableListDelegate, SearchableListItem};
use gpui_component::select::Select;
use gpui_component::{h_flex, v_flex, ActiveTheme as _, Icon, Sizable as _};

use super::sizing::CTL_LG_H;
use crate::chrome::Chrome;

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
pub fn glass_section_header(
    label: impl Into<SharedString>,
    trailing: Option<AnyElement>,
    cx: &App,
) -> Div {
    glass_section_band(None, label, trailing, cx)
}

/// EXP-818: the GROUP BAND — the Linear group header. A full-width strip
/// filled `fill_section` with the group's name in it (an optional leading
/// glyph, an optional trailing control), sitting directly over its flat rows
/// ([`flat_row`]) with a 4px gap. It replaced the plain-text header + gapped
/// card rows on every list page (Devices, Agent, Actions, an action's page,
/// Reviews, the settings lists): rows read as a table under a highlighted
/// header, not as a stack of cards. Web `GlassSectionHeader` twin.
pub fn glass_section_band(
    leading: Option<AnyElement>,
    label: impl Into<SharedString>,
    trailing: Option<AnyElement>,
    cx: &App,
) -> Div {
    let chrome = Chrome::global(cx);
    let foreground = cx.theme().foreground;
    h_flex()
        .w_full()
        .min_w_0()
        .items_center()
        .gap_1p5()
        .px_3()
        .py_1p5()
        .mb_1()
        .rounded(px(chrome.radius_md))
        .bg(chrome.fill_section)
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
/// chevron (the chrome's `icons.chevron_right` folded, `chevron_down` open)
/// and a trailing count, the whole strip a click target.
///
/// Returns the band WITHOUT a click handler — what folding means is the
/// caller's (it owns the collapsed flag).
pub fn glass_section_band_fold(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    count: usize,
    collapsed: bool,
    cx: &App,
) -> Stateful<Div> {
    let icons = &Chrome::global(cx).icons;
    let foreground = cx.theme().foreground;
    let chevron = Icon::empty()
        .path(if collapsed {
            icons.chevron_right.clone()
        } else {
            icons.chevron_down.clone()
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
pub fn flat_row(chrome: &Chrome) -> Div {
    div().rounded(px(chrome.radius_md))
}

/// EXP-963: the COMPACT density of [`flat_row`] — the web `ListRow
/// density="compact"` / `SidebarMenuButton density="compact"` twin: the
/// 28px one-line row the narrow column runs at (the rail's entries, the
/// side-list issue rows), the list's own 14px type, 8px of side padding and
/// 8px between the glyph and the text. Same fills as the list row: the
/// caller still applies the hover wash and the active fill.
pub fn flat_row_compact(chrome: &Chrome) -> Div {
    flat_row(chrome)
        .flex()
        .flex_row()
        .h(px(FLAT_ROW_COMPACT_H))
        .px_2()
        .gap_2()
        .items_center()
        .text_sm()
}

/// The compact row's height (web `h-7`).
pub const FLAT_ROW_COMPACT_H: f32 = 28.;

/// Card surface: the `radius_xl` (16) corner, `fill_card` under a
/// `stroke_card` hairline (mobile `GlassCard`). Layout (width/padding/gap) is
/// the caller's job.
pub fn glass_card(chrome: &Chrome) -> Div {
    v_flex()
        .rounded(px(chrome.radius_xl))
        .border_1()
        .border_color(chrome.stroke_card)
        .bg(chrome.fill_card)
}

/// EXP-642: ONE row of a carded list — the web `GlassRow`
/// (`@exp/ui glass-rows.tsx`: `rounded-md border border-glass-stroke
/// bg-glass-row`). Unlike [`glass_card`] these stack with a GAP instead of
/// fusing into one bordered block, which is what the reviews/support/actions
/// lists and the machines section wear since the glass-row ladder (EXP-616)
/// landed on the web. Layout (padding, gap, hover, id) is the caller's job.
pub fn glass_row_card(chrome: &Chrome) -> Div {
    div()
        .rounded(px(chrome.radius_md))
        .border_1()
        .border_color(chrome.stroke_row)
        .bg(chrome.fill_row)
}

/// EXP-694 — the inset-grouped card STACK, the reference look on every client
/// (Apple's "inset grouped list"; the Android `OptionGroup` / iOS
/// `glassFormRow` / web `GlassGroup` twin): ONE clipped radius-12 block filled
/// `fill_row` with NO outer stroke, whose rows fuse into it and are separated
/// by hairlines instead of gaps.
///
/// This is the other half of the row ladder next to [`glass_row_card`]: that
/// one is for rows that are separate OBJECTS (list items), this one for rows
/// that are FIELDS of one form. Pair with [`glass_group_rows`] so the dividers
/// land automatically; groups stack with an 8px gap.
pub fn glass_group(chrome: &Chrome) -> Div {
    v_flex()
        .w_full()
        .rounded(px(chrome.radius_lg))
        .bg(chrome.fill_row)
        .overflow_hidden()
}

/// A [`glass_group`] filled with `rows` in order, hairline-divided: every row
/// but the first draws the divider as its OWN top border, so the group stays
/// one clipped block and the hairlines are full-bleed (the web
/// `divide-y divide-glass-stroke`).
pub fn glass_group_rows(chrome: &Chrome, rows: Vec<Div>) -> Div {
    rows.into_iter()
        .enumerate()
        .fold(glass_group(chrome), |group, (ix, row)| {
            group.child(if ix == 0 {
                row
            } else {
                glass_row_divider(chrome, row)
            })
        })
}

/// EXP-994 — the same hairline-divided ladder with NO group of its own: no
/// fill, no radius, no clip. For rows that already sit ON a surface (a
/// popover, a sheet), where a [`glass_group`] would draw a card inside a
/// card — the surface IS the edge, the hairlines are all the structure the
/// rows need.
pub fn glass_group_rows_bare(chrome: &Chrome, rows: Vec<Div>) -> Div {
    rows.into_iter()
        .enumerate()
        .fold(v_flex().w_full(), |group, (ix, row)| {
            group.child(if ix == 0 {
                row
            } else {
                glass_row_divider(chrome, row)
            })
        })
}

/// The row rhythm of a [`glass_group_rows_bare`] ladder: tighter than
/// [`glass_row_shell`] because a popover is not a settings pane — leading
/// label, trailing control, one line.
pub fn bare_row_shell() -> Div {
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
pub fn glass_row_divider<T: Styled>(chrome: &Chrome, row: T) -> T {
    row.border_t_1().border_color(chrome.stroke_row)
}

/// EXP-994 — the same hairline for a LIST whose rows are built one by one
/// (the settings ladders, the machines list): every row but the FIRST draws
/// it, so a grouped list reads as one table instead of a stack of
/// free-floating rows — and nothing draws an outer card border around them.
/// A caller that already knows its index passes it; `ix == 0` is a no-op.
pub fn list_row_divider<T: Styled>(chrome: &Chrome, row: T, ix: usize) -> T {
    if ix == 0 {
        row
    } else {
        glass_row_divider(chrome, row)
    }
}

/// [`list_row_divider`] for a row the caller hands over as an opaque element
/// (most list rows return `impl IntoElement`): the hairline rides a
/// full-width wrapper instead of the row itself.
pub fn list_row(chrome: &Chrome, row: impl IntoElement, ix: usize) -> Div {
    list_row_divider(chrome, div().w_full().min_w_0(), ix).child(row)
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
pub fn glass_row_shell() -> Div {
    h_flex().w_full().items_center().gap_3().px_4().py_3()
}

/// How wide a row's muted DESCRIPTION line may grow before it wraps. The
/// settings panes are ~550px, and a hint set full-bleed across one reads as a
/// paragraph rather than as a caption under its label — the deleted
/// `notifications_prefs::pref_row` capped it here, so the recipe does it for
/// every consumer.
pub const ROW_DESCRIPTION_MAX_W: f32 = 460.;

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
/// [`glass_picker_select`] (a [`Select`]) or build it as a `dropdown_menu`
/// button ending in [`picker_row_chevron`]; either way it must arrive
/// stripped of background/border.
pub fn glass_picker_row(
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
pub fn picker_value_label(label: impl Into<SharedString>) -> Div {
    div()
        .max_w(px(PICKER_VALUE_MAX_W))
        .truncate()
        .child(label.into())
}

/// The trailing glyph of a grouped picker row: `ui-chevron-right` ×4, never
/// a dropdown caret (the polish round's picker rule). A `dropdown_menu`
/// trigger appends it as its LAST child instead of `dropdown_caret(true)`.
pub fn picker_row_chevron(cx: &App) -> Icon {
    Icon::empty()
        .path(Chrome::global(cx).icons.chevron_right.clone())
        .size_4()
}

/// Strip a [`Select`]'s field chrome so it reads as the trailing VALUE of a
/// [`glass_picker_row`]: no fill, no border, no focus ring box
/// (`appearance(false)`), no box padding or height of its own (the row's
/// 16/12 is the padding), the title right-aligned, and the component's caret
/// swapped for the row's [`picker_row_chevron`]. The web twin is the
/// `GLASS_PICKER_ROW` trigger (`bg-transparent border-0 h-auto`).
pub fn glass_picker_select<D>(select: Select<D>, cx: &App) -> Select<D>
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
        .icon(picker_row_chevron(cx))
}

/// A TEXT-FIELD row: the label leading, the value typed trailing, and no
/// field chrome — the web `GlassInputRow` twin (the Name row of the device
/// editor, the CLI-path row of Settings → Agents). Pass the field through
/// [`glass_row_input`] so it arrives stripped.
pub fn glass_input_row(label: impl Into<SharedString>, input: AnyElement, cx: &App) -> Div {
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
pub fn glass_row_input(input: Input) -> Input {
    input.appearance(false).h_auto().px_0().py_0().text_right()
}

/// A toggle row: the label (plus an optional muted description line) leading,
/// a [`gpui_component::switch::Switch`] trailing. The switch is the caller's —
/// it owns the id and the click listener — this only places it on the group's
/// row rhythm. Switches, never checkboxes: EXP-694 standardized the grouped
/// stacks on one control.
pub fn glass_toggle_row(
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

/// Web segmented `TabsList` capsule (`@exp/ui tabs.tsx`): h-9 full-width
/// capsule with a 3px inset. Pair with [`segmented_item`] children.
pub fn segmented(cx: &App) -> Div {
    let chrome = Chrome::global(cx);
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
        .border_color(chrome.stroke_section)
        .bg(chrome.fill_section)
        .p(px(3.))
        .text_color(theme.muted_foreground)
}

/// A web `TabsTrigger`: equal-width capsule segment, active = glass active
/// fill + stroke.
pub fn segmented_item(active: bool, cx: &App) -> Div {
    let chrome = Chrome::global(cx);
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
        item.bg(chrome.fill_active)
            .border_color(chrome.stroke_active)
            .text_color(theme.foreground)
    } else {
        item.hover(|style| style.text_color(theme.foreground))
    }
}

/// EXP-694 — the EMBEDDED tab row (S3): a segmented strip stops being a
/// free-floating capsule above the card and becomes the group's FIRST ROW —
/// full width, no fill or border of its own, 8px of padding on every side, and
/// the hairline underneath comes from [`glass_group_rows`]. Fill it with
/// [`glass_tab_item`] segments, NOT with [`segmented`]'s capsule container.
pub fn glass_tabs_row() -> Div {
    // EXP-698: no inter-segment gap — the segments abut like the web
    // `TabsList`, and the capsule's own fill is what separates them.
    h_flex().w_full().items_center().p_2()
}

/// One segment of a [`glass_tabs_row`]: [`segmented_item`] carrying its own
/// 7px vertical padding, since an embedded row has no fixed capsule height
/// for the segment to stretch into.
pub fn glass_tab_item(active: bool, cx: &App) -> Div {
    // `h_auto` undoes the capsule segment's `h_full`: there is no 36px
    // container height here, the segment's own padding sets the row height.
    segmented_item(active, cx).h_auto().py(px(6.))
}

/// EXP-568 / EXP-1191: the property ROW — a WRAPPING group of chips read as
/// one object by its spacing alone. EXP-1191 dropped the card chrome (fill,
/// hairline, radius, insets) it used to wear: the props sit plainly on the
/// page, the first chip on the title column's left edge (all four clients).
/// Gap = the web row's `gap-1.5` — 6 px, a `px()` literal because gpui's rem
/// is 14 in the IDE.
pub fn property_row() -> Div {
    h_flex().flex_wrap().items_center().gap(px(6.))
}

/// EXP-698 round 5 — the ONE bulk-action bar chrome, shared with web
/// (`bg-glass-card-opaque` + `border-glass-stroke-strong`, radius 24) and the
/// two mobile bars: a single OPAQUE capsule that floats over the list it acts
/// on. Opaque is the point — a translucent bar would show the rows it covers
/// sliding underneath it (and gpui has no in-scene backdrop blur), so the fill
/// is `theme.popover`, the shared glass-menu fill.
///
/// It does NOT wrap: the bar is a single fixed-height capsule, one row on
/// every surface. When the labeled row does not fit, the CALLER collapses its
/// buttons to icon-only with tooltips instead of flowing them onto a second
/// line (the IDE's `issue_list::render_bulk_bar`) — a two-line bar would
/// change the host row's height under the list.
pub fn glass_bar(cx: &App) -> Div {
    let chrome = Chrome::global(cx);
    h_flex()
        .items_center()
        .gap_1()
        .px_2p5()
        .py_2()
        .rounded(px(chrome.radius_xl3))
        .border_1()
        .border_color(chrome.stroke_strong)
        .bg(cx.theme().popover)
}

/// Which edge a [`disclosure_header`]'s chevron sits on (web `chevron`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChevronSide {
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
/// the pointer, a 12px chevron (the chrome's `icons.chevron_right` folded,
/// `chevron_down` open), the WHOLE line the target. `content` is the row's
/// own text and glyphs (an `h_flex` the caller builds — it keeps every
/// caption and spinner it had); the caller chains its `.on_click` and its
/// type rung after.
///
/// It is NOT the group band ([`glass_section_band_fold`]): that is a filled
/// strip heading a LIST; this is bare text heading a fold INSIDE a row. And
/// it may not contain another button — a fold's own action renders beside
/// it, never inside it.
pub fn disclosure_header(
    id: impl Into<ElementId>,
    open: bool,
    chevron: ChevronSide,
    content: impl IntoElement,
    cx: &App,
) -> Stateful<Div> {
    let icons = &Chrome::global(cx).icons;
    let theme = cx.theme();
    let glyph = Icon::empty()
        .path(if open {
            icons.chevron_down.clone()
        } else {
            icons.chevron_right.clone()
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
