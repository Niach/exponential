//! The board filter bar — web parity target
//! `apps/web/src/components/issue-filter-bar.tsx` (masterplan-v3 §4.2, at
//! compact density; the All/Active/Backlog tab presets were removed in
//! EXP-251 — the filter popover is the only status filter entry point; the
//! "All Issues"/"My Issues" title text was removed in EXP-426).
//!
//! Structure:
//!
//! 1. control row — either the list's INLINE bulk-action bar (while a
//!    selection exists) or the right-aligned [`IssueFilterPopover`] trigger
//!    and (when `can_create`) the indigo **New Issue** button. The row keeps
//!    a fixed min-height so the swap never moves the list rows (the EXP-289
//!    no-jump invariant, in-flow since EXP-426), and the control cluster
//!    wraps instead of overflowing when the panel gets narrow.
//! 2. [`ActiveFilterPills`] (only when filters are active — web renders null).

use gpui::{
    px, AnyElement, App, Entity, IntoElement, ParentElement, RenderOnce,
    StatefulInteractiveElement as _, Styled, Window,
};
use gpui_component::{h_flex, input::InputState, v_flex, Icon, Sizable as _};

use domain::rows::Label;
use domain::statuses::ResolvedStatus;
use domain::{has_active_filters, IssueFilters};

use crate::actions::NewIssue;
use crate::active_filter_pills::ActiveFilterPills;
use crate::create_issue_dialog::indigo_button;
use crate::filter_popover::{FilterView, IssueFilterPopover, OnFiltersChange, OnViewChange};
use crate::icons::registry;

/// Minimum height of the control row: `py_2` + the tallest control (44px
/// since EXP-289 bumped the two controls one step). Fixed so swapping the
/// Filter/New-Issue cluster for the bulk bar never moves the list rows.
const CONTROL_ROW_MIN_H: f32 = 44.;

#[derive(IntoElement)]
pub struct IssueFilterBar {
    filters: IssueFilters,
    labels: Vec<Label>,
    /// EXP-314: the scope team's status vocabulary — the popover's Status
    /// pane and the pills both render from it.
    statuses: Vec<ResolvedStatus>,
    popover_view: FilterView,
    label_query: Entity<InputState>,
    on_filters_change: OnFiltersChange,
    on_view_change: OnViewChange,
    can_create: bool,
    /// EXP-426: the list's inline bulk-action bar — replaces the
    /// Filter/New-Issue cluster while a selection exists.
    bulk: Option<AnyElement>,
}

impl IssueFilterBar {
    #[allow(clippy::too_many_arguments)] // mirrors the web component's props
    pub fn new(
        filters: IssueFilters,
        labels: Vec<Label>,
        statuses: Vec<ResolvedStatus>,
        popover_view: FilterView,
        label_query: Entity<InputState>,
        on_filters_change: OnFiltersChange,
        on_view_change: OnViewChange,
        can_create: bool,
        bulk: Option<AnyElement>,
    ) -> Self {
        Self {
            filters,
            labels,
            statuses,
            popover_view,
            label_query,
            on_filters_change,
            on_view_change,
            can_create,
            bulk,
        }
    }
}

impl RenderOnce for IssueFilterBar {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        // The control row: bulk bar (selection alive) XOR the Filter /
        // New-Issue cluster. Fixed min-height keeps the swap jump-free; the
        // cluster wraps (`flex_wrap`) so a narrow panel drops New Issue onto
        // a second line instead of overlapping anything.
        let control_row = match self.bulk {
            Some(bulk) => h_flex()
                .py_2()
                .min_h(px(CONTROL_ROW_MIN_H))
                .items_center()
                .child(bulk),
            None => h_flex()
                .py_2()
                .min_h(px(CONTROL_ROW_MIN_H))
                .items_center()
                .justify_end()
                .flex_wrap()
                .gap_1()
                .child(IssueFilterPopover::new(
                    self.filters.clone(),
                    self.labels.clone(),
                    self.statuses.clone(),
                    self.popover_view,
                    self.label_query.clone(),
                    self.on_filters_change.clone(),
                    self.on_view_change.clone(),
                ))
                .when(self.can_create, |row| {
                    row.child(
                        // Web: xs bg-indigo-600 hover:bg-indigo-700
                        // text-white ml-1, Plus + "New Issue" — SOLID
                        // indigo (the shared hand-rolled button; the
                        // pinned ButtonCustomVariant cannot render a
                        // solid fill). Dispatches the typed action
                        // (§3.6) — the create-issue dialog's handler
                        // picks it up. EXP-289: one step bigger here only
                        // (h_6 → h_7 + roomier padding) — the shared
                        // helper keeps its dialog-footer geometry.
                        indigo_button("filter-bar-new-issue", false, cx)
                            .ml_1()
                            .h_7()
                            .px_3()
                            .child(Icon::new(registry::UI_ADD).small())
                            .child("New Issue")
                            .on_click(|_, window, cx| {
                                window.dispatch_action(Box::new(NewIssue), cx)
                            }),
                    )
                }),
        };

        v_flex()
            .w_full()
            .flex_shrink_0()
            .px_4()
            .child(control_row)
            .when(has_active_filters(&self.filters), |bar| {
                bar.child(ActiveFilterPills::new(
                    self.filters.clone(),
                    self.labels.clone(),
                    self.statuses.clone(),
                    self.on_filters_change.clone(),
                ))
            })
    }
}

use gpui::prelude::FluentBuilder as _;
