//! Shared issue-picker PIECES (EXP-288, narrowed by EXP-1021).
//!
//! THE picker is [`crate::picker`] now — one primitive, ten typed pickers,
//! every trigger-owning call site through it. What is left here is what the
//! primitive does NOT own:
//!
//! - the shared chip [`chip_button`] / [`chip_label`] every picker triggers
//!   through, and the row shell [`picker_row`] both surfaces draw;
//! - the menu ROW builders (`option_item`, `status_menu`, `user_menu_item`)
//!   — a context-menu SUBMENU composes rows into a menu the host owns, which
//!   a trigger-based picker cannot express, so the issue list's row and
//!   context menus still build through them;
//! - the due date, which is not one of the ten: a single-card popover whose
//!   `Calendar` is de-carded (EXP-282 — it paints its own
//!   border/radius/padding inside the popover's card otherwise, the "double
//!   card" bug) with a host-specific `extra` row below (detail: Clear;
//!   dialog: the time inputs).

use std::rc::Rc;

use gpui::{
    div, prelude::FluentBuilder as _, px, App, ElementId, Entity, IntoElement, ParentElement,
    SharedString, Styled, Window,
};
use gpui_component::{
    button::Button,
    calendar::{Calendar, CalendarState},
    menu::{PopupMenu, PopupMenuItem},
    popover::Popover,
    v_flex, ActiveTheme as _, Icon, Side,
};
use theme::tokens as t;

use domain::statuses::{IssueStatusCategory, ResolvedStatus};
use domain::IssueStatus;

use crate::icons::{registry, resolved_status_icon};

/// A pick callback (the host owns the mutation — tRPC write vs local draft).
pub(crate) type OnPick<V> = Rc<dyn Fn(V, &mut Window, &mut App)>;

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
// EXP-1021 — the row marks and the keyboard model the SHARED picker
// (`crate::picker`) rides. EXP-963's `searchable_picker` and the per-subject
// popovers over it (labels, move-to-board) are RETIRED: the primitive owns
// its query field and its keyboard cursor now, so no host holds picker state
// any more. What survives here is the pure half both surfaces need.
// ---------------------------------------------------------------------------

/// One row's mark. The THIRD state moved to the primitive with the bulk
/// edits that need it (`picker::PickerChecked`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SelectionState {
    Selected,
    Unselected,
}

/// The nearest pickable position at or after `selected` (wrapping to the
/// front), or `None` when no row can be picked — a narrowing query always
/// leaves a real row selected.
pub(crate) fn clamp_picker_selection(selected: usize, enabled: &[bool]) -> Option<usize> {
    if enabled.is_empty() {
        return None;
    }
    let start = selected.min(enabled.len() - 1);
    (0..enabled.len())
        .map(|step| (start + step) % enabled.len())
        .find(|&ix| enabled[ix])
}

/// The selection one step (`delta` = ±1) from `selected`: wraps at both ends
/// and skips rows that cannot be picked.
pub(crate) fn step_picker_selection(
    selected: usize,
    delta: isize,
    enabled: &[bool],
) -> Option<usize> {
    let len = enabled.len();
    if len == 0 || !enabled.iter().any(|on| *on) {
        return None;
    }
    let mut ix = selected.min(len - 1);
    loop {
        ix = (ix as isize + delta).rem_euclid(len as isize) as usize;
        if enabled[ix] {
            return Some(ix);
        }
    }
}

/// The web `SelectionGlyph`: the mark a picker row wears, by arity. `None`
/// for an unpicked single row — that one carries nothing, and a MULTI row
/// carries nothing either: EXP-1021 made its highlight the mark.
pub(crate) fn selection_glyph(multi: bool, state: SelectionState, cx: &App) -> Option<Icon> {
    if multi {
        return None;
    }
    (state == SelectionState::Selected).then(|| {
        Icon::new(registry::UI_CHECK)
            .size(px(14.))
            .flex_shrink_0()
            .text_color(cx.theme().muted_foreground)
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

#[cfg(test)]
mod cursor_tests {
    use super::{clamp_picker_selection, step_picker_selection};

    /// Release review R5: the clamp lands on a pickable row at or after the
    /// cursor (wrapping), and `None` only when nothing can be picked.
    #[test]
    fn the_clamp_lands_on_a_pickable_row() {
        assert_eq!(clamp_picker_selection(0, &[true, true]), Some(0));
        assert_eq!(clamp_picker_selection(5, &[true, true]), Some(1), "past the end: the last");
        assert_eq!(clamp_picker_selection(0, &[false, true, true]), Some(1), "skips an inert top row");
        assert_eq!(clamp_picker_selection(2, &[true, false, false]), Some(0), "wraps to the front");
        assert_eq!(clamp_picker_selection(0, &[false, false]), None);
        assert_eq!(clamp_picker_selection(0, &[]), None);
    }

    /// ↑/↓ wrap at both ends and skip rows that cannot be picked.
    #[test]
    fn a_step_wraps_and_skips_inert_rows() {
        let all = [true, true, true];
        assert_eq!(step_picker_selection(0, 1, &all), Some(1));
        assert_eq!(step_picker_selection(2, 1, &all), Some(0), "down off the end wraps");
        assert_eq!(step_picker_selection(0, -1, &all), Some(2), "up off the top wraps");
        let inert_middle = [true, false, true];
        assert_eq!(step_picker_selection(0, 1, &inert_middle), Some(2));
        assert_eq!(step_picker_selection(2, -1, &inert_middle), Some(0));
        assert_eq!(step_picker_selection(0, 1, &[true]), Some(0), "a lone row stays");
        assert_eq!(step_picker_selection(0, 1, &[false, false]), None);
        assert_eq!(step_picker_selection(0, 1, &[]), None);
    }
}
