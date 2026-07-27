//! Shared issue pickers (EXP-288): ONE set of status / priority / assignee /
//! label / due-date picker components used by every host — the issue
//! detail's properties sidebar and the create-issue dialog. Unification
//! rules:
//!
//! - status/priority/assignee are single-pick `PopupMenu`s (close on pick);
//!   the TRIGGER stays caller-owned (sidebar row vs dialog chip), only the
//!   menu body is shared.
//! - labels are a SEARCHABLE multi-toggle popover (filter input + checkbox
//!   rows that toggle without closing) — the properties-panel recipe,
//!   everywhere.
//! - the due date is a single-card popover: the `Calendar` is de-carded
//!   (EXP-282 — it paints its own border/radius/padding inside the popover's
//!   card otherwise, the "double card" bug) with a host-specific `extra`
//!   row below (detail: Clear; dialog: the time inputs).

use std::rc::Rc;

use gpui::{
    div, px, App, ElementId, Entity, Focusable as _, IntoElement, ParentElement, SharedString,
    Styled, Window,
};
use gpui_component::{
    button::{Button, ButtonVariants as _},
    calendar::{Calendar, CalendarState},
    checkbox::Checkbox,
    h_flex,
    input::{Input, InputState},
    menu::{PopupMenu, PopupMenuItem},
    popover::Popover,
    v_flex, ActiveTheme as _, Icon, IconName, Sizable as _, Side,
};
use theme::tokens as t;

use domain::options::ISSUE_PRIORITY_OPTIONS;
use domain::rows::{Board, Label, User};
use domain::statuses::{IssueStatusCategory, ResolvedStatus};
use domain::{IssuePriority, IssueStatus};

use crate::icons::{option_icon, resolved_status_icon};
use crate::settings::parse_hex_color;

/// A pick callback (the host owns the mutation — tRPC write vs local draft).
pub(crate) type OnPick<V> = Rc<dyn Fn(V, &mut Window, &mut App)>;

/// A label toggle callback: `(label_id, was_selected)`.
pub(crate) type OnToggleLabel = Rc<dyn Fn(&str, bool, &mut Window, &mut App)>;

// ---------------------------------------------------------------------------
// Row/trigger primitives (moved out of properties_panel — EXP-288)
// ---------------------------------------------------------------------------

/// The properties sidebar's full-width ghost picker trigger.
pub(crate) fn picker_trigger(
    id: &'static str,
    icon: Option<Icon>,
    label: SharedString,
    muted: bool,
    cx: &App,
) -> Button {
    let text_color = if muted {
        cx.theme().muted_foreground
    } else {
        cx.theme().foreground
    };
    Button::new(id)
        .ghost()
        .xsmall()
        .w_full()
        // Release the xsmall 20px box — a full-width row reads better at the
        // token control height.
        .h(px(t::size::CONTROL_SM))
        .child(
            h_flex()
                .w_full()
                .min_w_0()
                .gap_1p5()
                .items_center()
                .children(icon.map(|icon| icon.xsmall().flex_shrink_0()))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .whitespace_nowrap()
                        .overflow_hidden()
                        .text_ellipsis()
                        .text_color(text_color)
                        .child(label),
                )
                .child(
                    Icon::new(IconName::ChevronDown)
                        .size_3()
                        .flex_shrink_0()
                        .text_color(cx.theme().muted_foreground),
                ),
        )
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

/// The team's statuses as menu items (EXP-314). The caller pre-configures the
/// menu (min_w etc.) and owns the trigger; `current_key` is the picked
/// issue's resolved group key.
///
/// Rows in the `duplicate` CATEGORY are excluded exactly where the enum
/// `duplicate` was before: bulk edits have no canonical-issue picker, and the
/// single-issue path intercepts duplicate into the picker anyway.
pub(crate) fn status_menu(
    menu: PopupMenu,
    statuses: &[ResolvedStatus],
    current_key: &str,
    on_pick: OnPick<StatusPick>,
    cx: &App,
) -> PopupMenu {
    let mut menu = menu.check_side(Side::Right);
    for status in statuses
        .iter()
        .filter(|status| status.category != IssueStatusCategory::Duplicate)
    {
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
        menu = menu.item(
            PopupMenuItem::new(SharedString::from(name))
                .checked(checked)
                .on_click(move |_, window, cx| {
                    on_pick(Some(user_id.clone()), window, cx);
                }),
        );
    }
    menu
}

// ---------------------------------------------------------------------------
// Searchable label picker
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

/// Web `LabelPicker` (EXP-282 recipe, shared since EXP-288): "Filter
/// labels…" input on top, live `contains()` filtering, checkbox + color-dot
/// rows, scroll-capped list, empty states.
pub(crate) fn label_picker_popover(
    id: impl Into<ElementId>,
    trigger: Button,
    params: LabelPickerParams,
) -> Popover {
    let id = id.into();
    let LabelPickerParams {
        labels,
        selected_ids,
        query,
        on_toggle,
        width,
    } = params;
    let rows_id = ElementId::Name(SharedString::from(format!("{id:?}-rows")));
    let query_for_open = query.clone();
    let mut popover = Popover::new(id).p_1();
    if let Some(width) = width {
        popover = popover.w(width);
    }
    popover
        .trigger(trigger)
        .on_open_change(move |open, window, cx| {
            // Fresh search per open; the input takes focus like the web
            // CommandInput.
            query_for_open.update(cx, |input, cx| input.set_value("", window, cx));
            if *open {
                query_for_open.read(cx).focus_handle(cx).focus(window, cx);
            }
        })
        .content(move |_, _, cx| {
            let filter = query.read(cx).value().trim().to_lowercase();
            let visible: Vec<&Label> = labels
                .iter()
                .filter(|label| filter.is_empty() || label.name.to_lowercase().contains(&filter))
                .collect();

            let mut column = v_flex()
                .w_full()
                .child(Input::new(&query).small().appearance(false).cleanable(true));
            if labels.is_empty() {
                return column.child(empty_picker_row("No labels in this team", cx));
            }
            column = column.child(
                div()
                    .h(px(1.))
                    .w_full()
                    .my_1()
                    .bg(cx.theme().border.opacity(0.5)),
            );
            if visible.is_empty() {
                return column.child(empty_picker_row("No labels found.", cx));
            }

            // Label lists grow with the team — cap + scroll (EXP-46a).
            use gpui::{InteractiveElement as _, StatefulInteractiveElement as _};
            let mut rows = v_flex()
                .id(rows_id.clone())
                .w_full()
                .max_h(px(240.))
                .overflow_y_scroll();
            for label in visible {
                let checked = selected_ids.contains(&label.id);
                let label_id = label.id.clone();
                let on_toggle = on_toggle.clone();
                let dot_color = label
                    .color
                    .as_deref()
                    .and_then(parse_hex_color)
                    .unwrap_or(gpui::opaque_grey(0.5, 1.0));
                rows = rows.child(
                    picker_row(
                        ElementId::Name(SharedString::from(format!("picker-label-{label_id}"))),
                        cx,
                    )
                    // The row owns the click — a handler on the checkbox too
                    // would double-toggle (filter_popover pattern).
                    .child(
                        Checkbox::new(ElementId::Name(SharedString::from(format!(
                            "picker-label-check-{label_id}"
                        ))))
                        .checked(checked),
                    )
                    .child(
                        div()
                            .size_2p5()
                            .rounded_full()
                            .flex_shrink_0()
                            .bg(dot_color),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .whitespace_nowrap()
                            .overflow_hidden()
                            .text_ellipsis()
                            .child(SharedString::from(label.name.clone())),
                    )
                    .on_click(move |_, window, cx| {
                        on_toggle(&label_id, checked, window, cx);
                    }),
                );
            }
            column.child(rows)
        })
}

// ---------------------------------------------------------------------------
// Searchable move-to-board picker (EXP-316 — web `BoardPicker` parity)
// ---------------------------------------------------------------------------

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

/// Web `BoardPicker` ("Move to board..." command popover): search input on
/// top, live `contains()` filtering, board-glyph rows with the current board
/// checked; picking another board fires `on_pick` and closes the popover.
pub(crate) fn board_picker_popover(
    id: impl Into<ElementId>,
    trigger: Button,
    params: BoardPickerParams,
) -> Popover {
    let id = id.into();
    let BoardPickerParams {
        boards,
        current_board_id,
        query,
        on_pick,
        width,
    } = params;
    let rows_id = ElementId::Name(SharedString::from(format!("{id:?}-rows")));
    let query_for_open = query.clone();
    let mut popover = Popover::new(id).p_1();
    if let Some(width) = width {
        popover = popover.w(width);
    }
    popover
        .trigger(trigger)
        .on_open_change(move |open, window, cx| {
            // Fresh search per open; the input takes focus like the web
            // CommandInput.
            query_for_open.update(cx, |input, cx| input.set_value("", window, cx));
            if *open {
                query_for_open.read(cx).focus_handle(cx).focus(window, cx);
            }
        })
        .content(move |_, _, cx| {
            let popover_state = cx.entity();
            let filter = query.read(cx).value().trim().to_lowercase();
            let visible: Vec<&Board> = boards
                .iter()
                .filter(|board| filter.is_empty() || board.name.to_lowercase().contains(&filter))
                .collect();

            let mut column = v_flex()
                .w_full()
                .child(Input::new(&query).small().appearance(false).cleanable(true));
            column = column.child(
                div()
                    .h(px(1.))
                    .w_full()
                    .my_1()
                    .bg(cx.theme().border.opacity(0.5)),
            );
            if visible.is_empty() {
                return column.child(empty_picker_row("No boards found.", cx));
            }

            use gpui::{InteractiveElement as _, StatefulInteractiveElement as _};
            let mut rows = v_flex()
                .id(rows_id.clone())
                .w_full()
                .max_h(px(240.))
                .overflow_y_scroll();
            for board in visible {
                let is_current = board.id == current_board_id;
                let board_id = board.id.clone();
                let on_pick = on_pick.clone();
                let popover_state = popover_state.clone();
                let tint = board
                    .color
                    .as_deref()
                    .and_then(parse_hex_color)
                    .unwrap_or(cx.theme().muted_foreground);
                let mut row = picker_row(
                    ElementId::Name(SharedString::from(format!("picker-board-{board_id}"))),
                    cx,
                )
                .child(
                    crate::icons::board_icon(board)
                        .xsmall()
                        .text_color(tint)
                        .flex_shrink_0(),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .whitespace_nowrap()
                        .overflow_hidden()
                        .text_ellipsis()
                        .child(SharedString::from(board.name.clone())),
                );
                if is_current {
                    row = row.child(
                        Icon::new(IconName::Check)
                            .size_3()
                            .flex_shrink_0()
                            .text_color(cx.theme().muted_foreground),
                    );
                } else {
                    row = row.on_click(move |_, window, cx| {
                        on_pick(board_id.clone(), window, cx);
                        popover_state.update(cx, |state, cx| state.dismiss(window, cx));
                    });
                }
                rows = rows.child(row);
            }
            column.child(rows)
        })
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
