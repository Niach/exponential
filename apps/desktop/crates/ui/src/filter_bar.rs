//! The board filter bar — web parity target
//! `apps/web/src/components/issue-filter-bar.tsx` (masterplan-v3 §4.2, at
//! compact density; the All/Active/Backlog tab presets were removed in
//! EXP-251 — the filter popover is the only status filter entry point; the
//! "All Issues"/"My Issues" title text was removed in EXP-426).
//!
//! Structure:
//!
//! 1. control row — the right-aligned [`IssueFilterPopover`] trigger, joined
//!    (while a selection exists) by the list's INLINE bulk-action tray on the
//!    LEFT of the same row (EXP-642 — the trigger no longer disappears; with
//!    an `external_trigger` host the tray is the whole row).
//!    (EXP-449 moved the **New Issue** button into the window titlebar, where
//!    it is reachable from every screen.) The row keeps a fixed min-height so
//!    the swap never moves the list rows (the EXP-289 no-jump invariant,
//!    in-flow since EXP-426), and the control cluster wraps instead of
//!    overflowing when the panel gets narrow.
//! 2. [`ActiveFilterPills`] (only when filters are active — web renders null).

use gpui::{px, AnyElement, App, Entity, IntoElement, ParentElement, RenderOnce, Styled, Window};
use gpui_component::{h_flex, input::InputState, v_flex};

use domain::rows::Label;
use domain::statuses::ResolvedStatus;
use domain::{has_active_filters, IssueFilters};

use crate::active_filter_pills::ActiveFilterPills;
use crate::filter_popover::{FilterView, IssueFilterPopover, OnFiltersChange, OnViewChange};

/// Height of the control row, and the EXP-289 no-jump invariant: swapping the
/// Filter trigger for the bulk bar must not move the list rows, so BOTH
/// branches have to measure the same. This is a floor, not a cap — the taller
/// branch wins if it disagrees, which is exactly the jump it is meant to
/// prevent.
///
/// - trigger branch: 16 (the row's own `py_2`) + 32 (`web_sm` control) = 48
/// - bulk branch:     0 (no row padding — the bar carries its own) + 50
///   (`surface::glass_bar`: `py_2` 16 + a 32px control + 2 border) = 50
///
/// So the floor is the bulk branch's 50, and the trigger branch's 48 grows
/// into it. EXP-698 round 5: it was 44 while the bulk cluster was a bare
/// tray; the opaque capsule is 6px taller.
const CONTROL_ROW_MIN_H: f32 = 50.;

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
    /// EXP-426: the list's inline bulk-action bar — replaces the Filter
    /// trigger while a selection exists.
    bulk: Option<AnyElement>,
    /// EXP-525: the host renders the Filter trigger elsewhere (the Inbox
    /// tool strip) — the control row only appears for the bulk bar.
    external_trigger: bool,
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
            bulk,
            external_trigger: false,
        }
    }

    /// EXP-525: suppress the trigger-only control row (the host renders the
    /// Filter trigger in its own strip); the bulk bar still gets its row.
    pub fn external_trigger(mut self, external: bool) -> Self {
        self.external_trigger = external;
        self
    }
}

impl RenderOnce for IssueFilterBar {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        // The control row: bulk bar (selection alive) XOR the Filter popover
        // trigger. Fixed min-height keeps the swap jump-free; both clusters
        // wrap (`flex_wrap`) so a narrow panel never overlaps anything.
        let control_row = match self.bulk {
            // EXP-642: the bulk tray sits LEFT and the Filter trigger keeps
            // its right-hand slot (web parity) — unless the host owns the
            // trigger, in which case the tray is the whole row.
            Some(bulk) => Some(
                h_flex()
                    // No `py_2` here: the bar is a padded capsule of its own
                    // (see [`CONTROL_ROW_MIN_H`]) and the row's padding on top
                    // of it would out-grow the trigger branch.
                    .min_h(px(CONTROL_ROW_MIN_H))
                    .items_center()
                    .justify_between()
                    .flex_wrap()
                    .gap_1()
                    .child(bulk)
                    .when(!self.external_trigger, |row| {
                        row.child(IssueFilterPopover::new(
                            self.filters.clone(),
                            self.labels.clone(),
                            self.statuses.clone(),
                            self.popover_view,
                            self.label_query.clone(),
                            self.on_filters_change.clone(),
                            self.on_view_change.clone(),
                        ))
                    }),
            ),
            // EXP-525: with an external trigger the row vanishes entirely —
            // the strip hosts the trigger, so an empty row is dead space.
            None if self.external_trigger => None,
            None => Some(
                h_flex()
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
                    )),
            ),
        };

        v_flex()
            .w_full()
            .flex_shrink_0()
            .px_4()
            .children(control_row)
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
