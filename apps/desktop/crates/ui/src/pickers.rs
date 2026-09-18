//! Shared issue pickers (EXP-288): ONE set of status / priority / assignee /
//! label / due-date picker components used by every host — the issue detail's
//! header chip row and the create-issue dialog. Unification rules:
//!
//! - status/priority/assignee are single-pick `PopupMenu`s (close on pick);
//!   the trigger is the shared [`chip_button`], only the content differs.
//! - labels are a SEARCHABLE multi-toggle popover (filter input + checkbox
//!   rows that toggle without closing) — the properties-panel recipe,
//!   everywhere.
//! - the due date is a single-card popover: the `Calendar` is de-carded
//!   (EXP-282 — it paints its own border/radius/padding inside the popover's
//!   card otherwise, the "double card" bug) with a host-specific `extra`
//!   row below (detail: Clear; dialog: the time inputs).

use std::rc::Rc;

use gpui::{
    div, prelude::FluentBuilder as _, px, App, ElementId, Entity, Focusable as _, IntoElement,
    ParentElement, SharedString, Styled, Window,
};
use gpui_component::{
    button::Button,
    calendar::{Calendar, CalendarState},
    input::InputState,
    menu::{PopupMenu, PopupMenuItem},
    popover::Popover,
    v_flex, ActiveTheme as _, Icon, Sizable as _, Side,
};
use theme::tokens as t;


use domain::options::ISSUE_PRIORITY_OPTIONS;
use domain::rows::{Board, Label, User};
use domain::statuses::{IssueStatusCategory, ResolvedStatus};
use domain::{IssuePriority, IssueStatus};

use crate::icons::{option_icon, registry, resolved_status_icon};
use crate::settings::parse_hex_color;

/// A pick callback (the host owns the mutation — tRPC write vs local draft).
pub(crate) type OnPick<V> = Rc<dyn Fn(V, &mut Window, &mut App)>;

/// A label toggle callback: `(label_id, was_selected)`.
pub(crate) type OnToggleLabel = Rc<dyn Fn(&str, bool, &mut Window, &mut App)>;

// ---------------------------------------------------------------------------
// Row/trigger primitives (moved out of the properties panel — EXP-288)
// ---------------------------------------------------------------------------

/// Minimum width of a picker's menu/popover (EXP-417). The triggers are
/// content-sized chips now, so their menus need a floor of their own — this
/// replaced the sidebar-inner-width pins the full-width triggers matched.
pub(crate) const PICKER_MENU_MIN_WIDTH: f32 = 168.;

/// Width of a SEARCHABLE picker popover (labels / move-to-board): wide enough
/// for the filter input plus a checkbox + color dot + name row.
pub(crate) const PICKER_SEARCH_WIDTH: f32 = 260.;

/// The shared chip trigger (EXP-417): `Button variant="ghost" size="xs"`,
/// content-sized. Every picker host — the create-issue dialog's chip row and
/// the issue/action headers — triggers through this ONE shape.
pub(crate) fn chip_button(id: impl Into<ElementId>, cx: &App) -> Button {
    // EXP-698: a picker chip is a small ACTION pill — FILLED, like every
    // other client's property chips — not a ghost outline. It stays a
    // `Button` because `DropdownMenu`/`Popover` triggers must be one; the
    // paint is the shared pill's.
    crate::surface::glass_pill_button(id, crate::surface::PillSize::Sm, cx)
}

/// Cap on a chip's label before it ellipsizes (EXP-424): wide enough for a
/// couple of joined label names, narrow enough that one chip can never paint
/// past the reading column at the undocked window's 480px minimum.
pub(crate) const CHIP_LABEL_MAX_W: f32 = 260.;

/// A chip trigger's text, passed to [`chip_button`] as a CHILD instead of
/// `.label()` — Button's own label div is `flex_none` and can never shrink,
/// so an over-long value (joined label names, a long board name) would paint
/// past the reading column. `muted` renders unset placeholders ("Assignee",
/// "Labels", "Due date") in `muted_foreground` so they read as placeholders,
/// not values.
pub(crate) fn chip_label(
    text: impl Into<SharedString>,
    muted: bool,
    cx: &App,
) -> gpui::Div {
    div()
        .max_w(px(CHIP_LABEL_MAX_W))
        .overflow_hidden()
        .whitespace_nowrap()
        .text_ellipsis()
        .when(muted, |this| this.text_color(cx.theme().muted_foreground))
        .child(text.into())
}

/// A shadcn `CommandItem`-style popover row: px-2 py-1 text-sm, glass row
/// fill on hover (also the board filter popover's row shape).
pub(crate) fn picker_row(id: impl Into<ElementId>, cx: &App) -> gpui::Stateful<gpui::Div> {
    use gpui::InteractiveElement as _;
    div()
        .id(id)
        .flex()
        .items_center()
        .w_full()
        .px_2()
        .py_1()
        .gap_2()
        .rounded(cx.theme().radius)
        .text_sm()
        .cursor_pointer()
        .hover(|style| style.bg(t::glass::FILL_ROW.to_hsla()))
}

/// The `CommandEmpty` fallback of a searchable picker.
pub(crate) fn empty_picker_row(message: &'static str, cx: &App) -> impl IntoElement {
    div()
        .py_4()
        .w_full()
        .text_sm()
        .text_color(cx.theme().muted_foreground)
        .text_center()
        .child(message)
}

/// One option row (same as the board's): icon + label + right-side check.
///
/// EXP-314 took this OWNED (`SharedString` + `Icon` instead of a
/// `&'static IssueOption<V>`): the status vocabulary is per-team data now, so
/// the rows cannot come from a compile-time table.
pub(crate) fn option_item(
    label: SharedString,
    icon: Icon,
    checked: bool,
    on_select: impl Fn(&mut Window, &mut App) + 'static,
) -> PopupMenuItem {
    PopupMenuItem::new(label)
        .icon(icon)
        .checked(checked)
        .on_click(move |_, window, cx| on_select(window, cx))
}

// ---------------------------------------------------------------------------
// Single-pick menus (status / priority / assignee)
// ---------------------------------------------------------------------------

/// What a status picker emits (EXP-314). `status_id` is `Some` for a SYNCED
/// row (the write sends `statusId`); `None` for a constructed
/// `builtin:<key>` fallback, where the write degrades to the enum `anchor`.
/// The two are mutually exclusive server-side.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct StatusPick {
    pub status_id: Option<String>,
    pub anchor: IssueStatus,
    pub category: IssueStatusCategory,
}

impl StatusPick {
    pub(crate) fn from_resolved(status: &ResolvedStatus) -> Self {
        Self {
            status_id: status.row_id.clone(),
            anchor: status.anchor(),
            category: status.category,
        }
    }

    /// Stamp this pick onto an `issues.update` input — exactly one of
    /// `statusId` / `status`.
    pub(crate) fn apply_to_update(&self, input: &mut api::issues::IssuesUpdateInput) {
        match &self.status_id {
            Some(id) => input.status_id = Some(id.clone()),
            None => input.status = Some(self.anchor),
        }
    }

    /// Stamp this pick onto an `issues.bulkUpdate` input.
    pub(crate) fn apply_to_bulk(&self, input: &mut api::issues::IssuesBulkUpdateInput) {
        match &self.status_id {
            Some(id) => input.status_id = Some(id.clone()),
            None => input.status = Some(self.anchor),
        }
    }

    /// Stamp this pick onto an `issues.create` input.
    pub(crate) fn apply_to_create(&self, input: &mut api::issues::IssuesCreateInput) {
        match &self.status_id {
            Some(id) => input.status_id = Some(id.clone()),
            None => input.status = Some(self.anchor),
        }
    }
}

/// Which slice of the team vocabulary a status menu offers (EXP-314 / L27).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum StatusMenuScope {
    /// Single-issue surfaces (row dropdown, row context submenu, the detail
    /// header's status chip): the FULL vocabulary, `duplicate`-category rows
    /// INCLUDED — picking one is intercepted by `apply_status_selection` into
    /// the duplicate-canonical picker, which is the only way to mark a
    /// duplicate on desktop.
    SingleIssue,
    /// Bulk bars + create dialogs: `duplicate`-category rows are excluded —
    /// there is no canonical-issue picker on those paths, and a bare
    /// `status='duplicate'` without `duplicate_of_id` breaks the pairing
    /// invariant.
    Assignable,
}

impl StatusMenuScope {
    fn allows(&self, status: &ResolvedStatus) -> bool {
        match self {
            StatusMenuScope::SingleIssue => true,
            StatusMenuScope::Assignable => status.category != IssueStatusCategory::Duplicate,
        }
    }
}

/// The rows a status menu of `scope` offers — the pure half of
/// [`status_menu`], so the duplicate-visibility contract is unit-testable.
///
/// EXP-448: no picker-only re-sort — `statuses` already arrives in the ONE
/// order (Backlog leads) that the settings pane and the list groups
/// use, so a picker can never contradict either.
pub(crate) fn status_menu_options<'a>(
    statuses: &'a [ResolvedStatus],
    scope: StatusMenuScope,
) -> Vec<&'a ResolvedStatus> {
    statuses
        .iter()
        .filter(|status| scope.allows(status))
        .collect()
}

/// The team's statuses as menu items (EXP-314). The caller pre-configures the
/// menu (min_w etc.) and owns the trigger; `current_key` is the picked
/// issue's resolved group key, and `scope` decides whether the
/// `duplicate`-category row is offered (see [`StatusMenuScope`]).
pub(crate) fn status_menu(
    menu: PopupMenu,
    statuses: &[ResolvedStatus],
    current_key: &str,
    scope: StatusMenuScope,
    on_pick: OnPick<StatusPick>,
    cx: &App,
) -> PopupMenu {
    let mut menu = menu.check_side(Side::Right);
    for status in status_menu_options(statuses, scope) {
        let on_pick = on_pick.clone();
        let pick = StatusPick::from_resolved(status);
        menu = menu.item(option_item(
            SharedString::from(status.name.clone()),
            resolved_status_icon(status, cx),
            status.group_key == current_key,
            move |window, cx| on_pick(pick.clone(), window, cx),
        ));
    }
    menu
}

/// The priority options as menu items.
pub(crate) fn priority_menu(
    menu: PopupMenu,
    current: IssuePriority,
    on_pick: OnPick<IssuePriority>,
    cx: &App,
) -> PopupMenu {
    let mut menu = menu.check_side(Side::Right);
    for option in &ISSUE_PRIORITY_OPTIONS {
        let on_pick = on_pick.clone();
        let value = option.value;
        menu = menu.item(option_item(
            SharedString::from(option.label),
            option_icon(option, cx),
            option.value == current,
            move |window, cx| on_pick(value, window, cx),
        ));
    }
    menu
}

/// The assignee options as menu items: an Unassign row (only while someone
/// is assigned — picked as `None`) + every team member. Scroll-capped
/// (member lists grow with the team, EXP-46a).
pub(crate) fn assignee_menu(
    menu: PopupMenu,
    users: &[User],
    current: Option<&str>,
    on_pick: OnPick<Option<String>>,
) -> PopupMenu {
    let mut menu = menu
        .check_side(Side::Right)
        .scrollable(true)
        .max_h(px(320.));
    if current.is_some() {
        let on_pick = on_pick.clone();
        menu = menu.item(PopupMenuItem::new("Unassign").on_click(move |_, window, cx| {
            on_pick(None, window, cx);
        }));
    }
    for user in users {
        let name = crate::comments::author_label(Some(user));
        let checked = current == Some(user.id.as_str());
        let on_pick = on_pick.clone();
        let user_id = user.id.clone();
        menu = menu.item(user_menu_item(
            user.id.clone(),
            name,
            user.image.clone(),
            checked,
            move |window, cx| {
                on_pick(Some(user_id.clone()), window, cx);
            },
        ));
    }
    menu
}

/// One member row for an assignee menu (EXP-426): the REAL avatar + name via
/// [`crate::user_avatar::user_row`] on the `PopupMenuItem::element` escape
/// hatch — a plain item's `icon` slot can only carry a glyph. Shared by the
/// detail-header picker and the issue list's row/context assignee menus.
pub(crate) fn user_menu_item(
    user_id: String,
    name: String,
    image_url: Option<String>,
    checked: bool,
    on_select: impl Fn(&mut Window, &mut App) + 'static,
) -> PopupMenuItem {
    PopupMenuItem::element(move |_, cx| {
        crate::user_avatar::user_row(&user_id, &name, image_url.as_deref(), cx)
    })
    .checked(checked)
    .on_click(move |_, window, cx| on_select(window, cx))
}

// ---------------------------------------------------------------------------
// EXP-963 — the ONE searchable picker (web `Combobox`, iOS/Android picker
// sheets): a search field over rows, with the selection glyphs every client
// draws
// ---------------------------------------------------------------------------

/// A row's leading glyph, built at render (a board's tinted icon, a label's
/// colour dot).
pub(crate) type PickerLead = Rc<dyn Fn(&App) -> gpui::AnyElement>;

/// One option of a [`searchable_picker`] — the web `PickerOption`: `value`
/// is the IDENTITY (two boards may share a name), `keywords` the search
/// text (defaults to the label), `lead` the glyph before the label.
pub(crate) struct PickerOption {
    pub value: String,
    pub label: SharedString,
    pub keywords: Vec<String>,
    pub lead: Option<PickerLead>,
    /// An inert row — the CURRENT board of a move picker: it renders marked
    /// and never fires.
    pub disabled: bool,
}

impl PickerOption {
    pub(crate) fn new(value: impl Into<String>, label: impl Into<SharedString>) -> Self {
        let label = label.into();
        Self {
            value: value.into(),
            keywords: vec![label.to_string()],
            label,
            lead: None,
            disabled: false,
        }
    }

    pub(crate) fn lead(mut self, lead: impl Fn(&App) -> gpui::AnyElement + 'static) -> Self {
        self.lead = Some(Rc::new(lead));
        self
    }

    pub(crate) fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    fn matches(&self, filter: &str) -> bool {
        filter.is_empty()
            || self
                .keywords
                .iter()
                .any(|keyword| keyword.to_lowercase().contains(filter))
    }
}

/// How a picker marks its rows — the arity decides the glyph (web
/// `SelectionGlyph`, iOS `AgentIssuePickerSheet`, Android the same):
/// SINGLE marks the picked row with a trailing `ui-check` and closes on
/// pick; MULTI marks EVERY row with the leading `ui-selected` /
/// `ui-unselected` circle pair (never a checkbox) and stays open, because a
/// batch is several picks. `indeterminate` rows (a bulk edit over rows that
/// disagree) wear `ui-indeterminate`.
pub(crate) enum PickerSelection {
    Single { current: Option<String> },
    Multi {
        selected: Vec<String>,
        indeterminate: Vec<String>,
    },
}

/// One row's mark.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SelectionState {
    Selected,
    Unselected,
    Indeterminate,
}

impl PickerSelection {
    fn is_multi(&self) -> bool {
        matches!(self, PickerSelection::Multi { .. })
    }

    /// The mark a row with `value` wears — the pure half of the row
    /// renderer, unit-tested below.
    pub(crate) fn state_of(&self, value: &str) -> SelectionState {
        match self {
            PickerSelection::Single { current } => {
                if current.as_deref() == Some(value) {
                    SelectionState::Selected
                } else {
                    SelectionState::Unselected
                }
            }
            PickerSelection::Multi {
                selected,
                indeterminate,
            } => {
                if selected.iter().any(|id| id == value) {
                    SelectionState::Selected
                } else if indeterminate.iter().any(|id| id == value) {
                    SelectionState::Indeterminate
                } else {
                    SelectionState::Unselected
                }
            }
        }
    }
}

/// A pick: `(value, was_selected)`. A single picker also closes itself; a
/// multi one toggles without closing.
pub(crate) type OnPickOption = Rc<dyn Fn(&str, bool, &mut Window, &mut App)>;

pub(crate) struct SearchablePickerParams {
    pub options: Vec<PickerOption>,
    pub selection: PickerSelection,
    /// HOST-owned search input state (reset + focused on every open).
    pub query: Entity<InputState>,
    pub on_pick: OnPickOption,
    /// The `CommandEmpty` copy once the filter matches nothing.
    pub empty_text: &'static str,
    /// The copy for a picker with NO options at all (`None` = the plain
    /// empty text).
    pub no_options_text: Option<&'static str>,
    /// Popover width; `None` = intrinsic.
    pub width: Option<gpui::Pixels>,
}

/// The web `SelectionGlyph`: the mark a picker row wears, by arity.
/// `None` for an unpicked single row — that one carries nothing.
pub(crate) fn selection_glyph(
    multi: bool,
    state: SelectionState,
    cx: &App,
) -> Option<Icon> {
    let theme = cx.theme();
    if !multi {
        return (state == SelectionState::Selected).then(|| {
            Icon::new(registry::UI_CHECK)
                .size(px(14.))
                .flex_shrink_0()
                .text_color(theme.muted_foreground)
        });
    }
    let (glyph, ink) = match state {
        SelectionState::Selected => (registry::UI_SELECTED, theme.foreground),
        SelectionState::Indeterminate => (registry::UI_INDETERMINATE, theme.foreground),
        SelectionState::Unselected => (registry::UI_UNSELECTED, theme.muted_foreground),
    };
    Some(Icon::new(glyph).size(px(16.)).flex_shrink_0().text_color(ink))
}

/// EXP-963 — THE searchable picker: `trigger` opens a popover holding the
/// small [`crate::controls::search_field`] (fresh and focused on every
/// open, like the web `CommandInput`), a hairline, and the option rows that
/// match, scroll-capped. Every per-subject popover (labels, move-to-board,
/// the branch picker) is this one with different options.
pub(crate) fn searchable_picker(
    id: impl Into<ElementId>,
    trigger: Button,
    params: SearchablePickerParams,
) -> Popover {
    let id = id.into();
    let SearchablePickerParams {
        options,
        selection,
        query,
        on_pick,
        empty_text,
        no_options_text,
        width,
    } = params;
    let rows_id = ElementId::Name(SharedString::from(format!("{id:?}-rows")));
    let query_for_open = query.clone();
    let mut popover = Popover::new(id).p_1();
    if let Some(width) = width {
        popover = popover.w(width);
    }
    let multi = selection.is_multi();
    popover
        .trigger(trigger)
        .on_open_change(move |open, window, cx| {
            query_for_open.update(cx, |input, cx| input.set_value("", window, cx));
            if *open {
                query_for_open.read(cx).focus_handle(cx).focus(window, cx);
            }
        })
        .content(move |_, window, cx| {
            let popover_state = cx.entity();
            let filter = query.read(cx).value().trim().to_lowercase();
            let mut column = v_flex().w_full().child(
                crate::controls::search_field(
                    &query,
                    crate::controls::SearchFieldSize::Sm,
                    window,
                    cx,
                )
                .appearance(false),
            );
            if options.is_empty() {
                if let Some(text) = no_options_text {
                    return column.child(empty_picker_row(text, cx));
                }
            }
            column = column.child(
                div()
                    .h(px(1.))
                    .w_full()
                    .my_1()
                    .bg(cx.theme().border.opacity(0.5)),
            );
            let visible: Vec<&PickerOption> = options
                .iter()
                .filter(|option| option.matches(&filter))
                .collect();
            if visible.is_empty() {
                return column.child(empty_picker_row(empty_text, cx));
            }

            use gpui::{InteractiveElement as _, StatefulInteractiveElement as _};
            let mut rows = v_flex()
                .id(rows_id.clone())
                .w_full()
                .max_h(px(240.))
                .overflow_y_scroll();
            for option in visible {
                let state = selection.state_of(&option.value);
                let selected = state == SelectionState::Selected;
                let value = option.value.clone();
                let on_pick = on_pick.clone();
                let popover_state = popover_state.clone();
                let mut row = picker_row(
                    ElementId::Name(SharedString::from(format!("picker-option-{value}"))),
                    cx,
                )
                // EXP-963: a multi row that is picked wears the active fill
                // (web `bg-glass-active`), so the batch reads at a glance.
                .when(multi && selected, |row| {
                    row.bg(t::glass::FILL_ACTIVE.to_hsla())
                });
                if multi {
                    row = row.children(selection_glyph(true, state, cx));
                }
                if let Some(lead) = &option.lead {
                    row = row.child(lead(cx));
                }
                row = row.child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .whitespace_nowrap()
                        .overflow_hidden()
                        .text_ellipsis()
                        .child(option.label.clone()),
                );
                if !multi {
                    row = row.children(selection_glyph(false, state, cx));
                }
                if option.disabled {
                    row = row.cursor_default();
                } else {
                    row = row.on_click(move |_, window, cx| {
                        on_pick(&value, selected, window, cx);
                        if !multi {
                            popover_state.update(cx, |state, cx| state.dismiss(window, cx));
                        }
                    });
                }
                rows = rows.child(row);
            }
            column.child(rows)
        })
}

// ---------------------------------------------------------------------------
// The per-subject pickers, each ONE [`searchable_picker`] with its options
// ---------------------------------------------------------------------------

pub(crate) struct LabelPickerParams {
    pub labels: Vec<Label>,
    pub selected_ids: Vec<String>,
    /// HOST-owned search input state (reset + focused on every open).
    pub query: Entity<InputState>,
    /// `(label_id, was_selected)` — toggles WITHOUT closing the popover.
    pub on_toggle: OnToggleLabel,
    /// Popover width; `None` = intrinsic.
    pub width: Option<gpui::Pixels>,
}

/// A label's colour dot, the lead of its picker row.
fn label_dot(color: Option<&str>) -> gpui::AnyElement {
    let dot_color = color
        .and_then(parse_hex_color)
        .unwrap_or(gpui::opaque_grey(0.5, 1.0));
    div()
        .size_2p5()
        .rounded_full()
        .flex_shrink_0()
        .bg(dot_color)
        .into_any_element()
}

/// Web `LabelPicker` (EXP-282 recipe, shared since EXP-288): the MULTI
/// [`searchable_picker`] over the team's labels — colour-dot rows behind the
/// `ui-selected` / `ui-unselected` pair, toggling without closing.
pub(crate) fn label_picker_popover(
    id: impl Into<ElementId>,
    trigger: Button,
    params: LabelPickerParams,
) -> Popover {
    let LabelPickerParams {
        labels,
        selected_ids,
        query,
        on_toggle,
        width,
    } = params;
    let options = labels
        .iter()
        .map(|label| {
            let color = label.color.clone();
            PickerOption::new(label.id.clone(), label.name.clone())
                .lead(move |_| label_dot(color.as_deref()))
        })
        .collect();
    searchable_picker(
        id,
        trigger,
        SearchablePickerParams {
            options,
            selection: PickerSelection::Multi {
                selected: selected_ids,
                indeterminate: Vec::new(),
            },
            query,
            on_pick: Rc::new(move |label_id, was_selected, window, cx| {
                on_toggle(label_id, was_selected, window, cx);
            }),
            empty_text: "No labels found.",
            no_options_text: Some("No labels in this team"),
            width,
        },
    )
}

pub(crate) struct BoardPickerParams {
    /// The team's boards, current one included (it renders checked+inert).
    pub boards: Vec<Board>,
    pub current_board_id: String,
    /// HOST-owned search input state (reset + focused on every open).
    pub query: Entity<InputState>,
    /// Picked a DIFFERENT board (the current row never fires).
    pub on_pick: OnPick<String>,
    /// Popover width; `None` = intrinsic.
    pub width: Option<gpui::Pixels>,
}

/// Web `BoardPicker` ("Move to board..." command popover): the SINGLE
/// [`searchable_picker`] over the team's boards — board-glyph rows, the
/// current board checked and inert; picking another fires `on_pick` and
/// closes.
pub(crate) fn board_picker_popover(
    id: impl Into<ElementId>,
    trigger: Button,
    params: BoardPickerParams,
) -> Popover {
    let BoardPickerParams {
        boards,
        current_board_id,
        query,
        on_pick,
        width,
    } = params;
    let options = boards
        .iter()
        .map(|board| {
            let board = board.clone();
            let is_current = board.id == current_board_id;
            PickerOption::new(board.id.clone(), board.name.clone())
                .disabled(is_current)
                .lead(move |cx| {
                    let tint = board
                        .color
                        .as_deref()
                        .and_then(parse_hex_color)
                        .unwrap_or(cx.theme().muted_foreground);
                    crate::icons::board_icon(&board)
                        .xsmall()
                        .text_color(tint)
                        .flex_shrink_0()
                        .into_any_element()
                })
        })
        .collect();
    searchable_picker(
        id,
        trigger,
        SearchablePickerParams {
            options,
            selection: PickerSelection::Single {
                current: Some(current_board_id),
            },
            query,
            on_pick: Rc::new(move |board_id, _was_selected, window, cx| {
                on_pick(board_id.to_string(), window, cx);
            }),
            empty_text: "No boards found.",
            no_options_text: None,
            width,
        },
    )
}

// ---------------------------------------------------------------------------
// Due-date popover (single card — EXP-282 de-carding, everywhere)
// ---------------------------------------------------------------------------

/// Extra content a host renders under the calendar (detail: Clear button;
/// dialog: the time row).
pub(crate) type DueExtra = Rc<dyn Fn(&mut Window, &mut App) -> gpui::AnyElement>;

pub(crate) fn due_date_popover(
    id: impl Into<ElementId>,
    trigger: Button,
    calendar: Entity<CalendarState>,
    width: Option<gpui::Pixels>,
    extra: Option<DueExtra>,
) -> Popover {
    Popover::new(id)
        .trigger(trigger)
        .content(move |_, window, cx| {
            // EXP-282: the Calendar paints its OWN card (border + radius +
            // p_3) inside the popover's card — de-card it (its `Styled`
            // refinement runs after its defaults) so the picker sits in ONE
            // card, in EVERY host (the create dialog used to double-card).
            let mut content = v_flex().gap_1();
            if let Some(width) = width {
                content = content.w(width);
            }
            content = content.child(Calendar::new(&calendar).border_0().rounded_none().p_0());
            if let Some(extra) = &extra {
                content = content.child(extra(window, cx));
            }
            content
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use domain::statuses::default_resolved_statuses;

    /// L27: the single-issue surfaces MUST offer the duplicate row (their
    /// `apply_status_selection` intercepts it into the canonical picker — it
    /// is desktop's only path to marking a duplicate); bulk bars and the
    /// create dialog must not.
    #[test]
    fn duplicate_row_is_single_issue_only() {
        let statuses = default_resolved_statuses();
        let names = |scope| -> Vec<String> {
            status_menu_options(&statuses, scope)
                .into_iter()
                .map(|status| status.name.clone())
                .collect()
        };

        let single = names(StatusMenuScope::SingleIssue);
        assert_eq!(single.len(), statuses.len());
        assert!(single.iter().any(|name| name == "Duplicate"));

        let assignable = names(StatusMenuScope::Assignable);
        assert_eq!(assignable.len(), statuses.len() - 1);
        assert!(!assignable.iter().any(|name| name == "Duplicate"));

        // Everything else is untouched, in vocabulary order.
        let expected: Vec<String> = single
            .iter()
            .filter(|name| name.as_str() != "Duplicate")
            .cloned()
            .collect();
        assert_eq!(assignable, expected);
    }

    /// EXP-448: pickers lead with Backlog because the ONE category order
    /// does — the same order the board groups and the settings pane render,
    /// with the started rows' pie-clock positions intact.
    #[test]
    fn picker_options_lead_with_backlog() {
        let statuses = default_resolved_statuses();
        let names: Vec<String> = status_menu_options(&statuses, StatusMenuScope::SingleIssue)
            .into_iter()
            .map(|status| status.name.clone())
            .collect();
        assert_eq!(
            names,
            [
                "Backlog",
                "In Progress",
                "In Review",
                "Done",
                "Cancelled",
                "Duplicate"
            ]
        );
    }

    /// EXP-963: the ONE selection rule — a single picker marks only the
    /// current row, a multi one every row, and a disagreeing bulk row is
    /// indeterminate before it is unselected.
    #[test]
    fn selection_state_follows_the_arity() {
        let single = PickerSelection::Single {
            current: Some("b".into()),
        };
        assert_eq!(single.state_of("a"), SelectionState::Unselected);
        assert_eq!(single.state_of("b"), SelectionState::Selected);
        let none = PickerSelection::Single { current: None };
        assert_eq!(none.state_of("b"), SelectionState::Unselected);

        let multi = PickerSelection::Multi {
            selected: vec!["a".into()],
            indeterminate: vec!["a".into(), "c".into()],
        };
        assert_eq!(multi.state_of("a"), SelectionState::Selected);
        assert_eq!(multi.state_of("b"), SelectionState::Unselected);
        assert_eq!(multi.state_of("c"), SelectionState::Indeterminate);
    }

    /// The search text is the keywords, case-insensitive, and an empty
    /// filter keeps every row.
    #[test]
    fn options_filter_on_keywords() {
        let option = PickerOption::new("id", "Mobile Bugs");
        assert!(option.matches(""));
        assert!(option.matches("bug"));
        assert!(option.matches("mobile b"));
        assert!(!option.matches("web"));
    }

    /// The pick a single-issue duplicate row emits must carry the duplicate
    /// CATEGORY — that is what `apply_status_selection` intercepts on.
    #[test]
    fn duplicate_pick_carries_the_duplicate_category() {
        let statuses = default_resolved_statuses();
        let duplicate = status_menu_options(&statuses, StatusMenuScope::SingleIssue)
            .into_iter()
            .find(|status| status.category == IssueStatusCategory::Duplicate)
            .expect("duplicate row is offered on single-issue surfaces");
        let pick = StatusPick::from_resolved(duplicate);
        assert_eq!(pick.category, IssueStatusCategory::Duplicate);
        assert_eq!(pick.anchor, IssueStatus::Duplicate);
    }
}
