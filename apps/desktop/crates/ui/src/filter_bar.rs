//! The board filter bar — web parity target
//! `apps/web/src/components/issue-filter-bar.tsx` (masterplan-v3 §4.2, at
//! compact density; the All/Active/Backlog tab presets were removed in
//! EXP-251 — the filter popover is the only status filter entry point).
//!
//! Structure mirrors the web component 1:1:
//!
//! 1. title row — `title` left, right-aligned [`IssueFilterPopover`] trigger
//!    and (when `can_create`) the indigo **New Issue** button,
//! 2. [`ActiveFilterPills`] (only when filters are active — web renders null).

use gpui::{
    div, App, Entity, FontWeight, IntoElement, ParentElement, RenderOnce, SharedString,
    StatefulInteractiveElement as _, Styled, Window,
};
use gpui_component::{h_flex, input::InputState, v_flex, Icon, IconName, Sizable as _};

use domain::rows::Label;
use domain::{has_active_filters, IssueFilters};

use crate::actions::NewIssue;
use crate::active_filter_pills::ActiveFilterPills;
use crate::create_issue_dialog::indigo_button;
use crate::filter_popover::{FilterView, IssueFilterPopover, OnFiltersChange, OnViewChange};

#[derive(IntoElement)]
pub struct IssueFilterBar {
    title: SharedString,
    filters: IssueFilters,
    labels: Vec<Label>,
    popover_view: FilterView,
    label_query: Entity<InputState>,
    on_filters_change: OnFiltersChange,
    on_view_change: OnViewChange,
    can_create: bool,
}

impl IssueFilterBar {
    #[allow(clippy::too_many_arguments)] // mirrors the web component's props
    pub fn new(
        title: impl Into<SharedString>,
        filters: IssueFilters,
        labels: Vec<Label>,
        popover_view: FilterView,
        label_query: Entity<InputState>,
        on_filters_change: OnFiltersChange,
        on_view_change: OnViewChange,
        can_create: bool,
    ) -> Self {
        Self {
            title: title.into(),
            filters,
            labels,
            popover_view,
            label_query,
            on_filters_change,
            on_view_change,
            can_create,
        }
    }
}

impl RenderOnce for IssueFilterBar {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        // Title row (web: flex items-center justify-between py-3, compacted).
        let title_row = h_flex()
            .py_2()
            .items_center()
            .justify_between()
            .child(
                div()
                    .text_sm()
                    .font_weight(FontWeight::MEDIUM)
                    .child(self.title.clone()),
            )
            .child(
                h_flex()
                    .gap_1()
                    .items_center()
                    .child(IssueFilterPopover::new(
                        self.filters.clone(),
                        self.labels.clone(),
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
                            // picks it up.
                            indigo_button("filter-bar-new-issue", false, cx)
                                .ml_1()
                                .child(Icon::new(IconName::Plus).xsmall())
                                .child("New Issue")
                                .on_click(|_, window, cx| {
                                    window.dispatch_action(Box::new(NewIssue), cx)
                                }),
                        )
                    }),
            );

        v_flex()
            .w_full()
            .flex_shrink_0()
            .px_4()
            .child(title_row)
            .when(has_active_filters(&self.filters), |bar| {
                bar.child(ActiveFilterPills::new(
                    self.filters.clone(),
                    self.labels.clone(),
                    self.on_filters_change.clone(),
                ))
            })
    }
}

use gpui::prelude::FluentBuilder as _;
