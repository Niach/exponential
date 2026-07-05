//! The board filter bar — web parity target
//! `apps/web/src/components/issue-filter-bar.tsx` (masterplan-v3 §4.2:
//! tabs + filter button styled EXACTLY like web, at compact density).
//!
//! Structure mirrors the web component 1:1:
//!
//! 1. title row — `title` left, right-aligned [`IssueFilterPopover`] trigger
//!    and (when `can_create`) the indigo **New Issue** button,
//! 2. tabs row — **left-aligned** All Issues / Active / Backlog rounded-full
//!    ghost tabs; the active tab is `bg-accent text-foreground font-medium`,
//!    inactive `text-muted-foreground hover:text-foreground`,
//! 3. [`ActiveFilterPills`] (only when filters are active — web renders null).
//!
//! Tabs are presets over `filters.statuses` (`domain::tab_preset_statuses`,
//! the verbatim `filters.ts` port §4.7): clicking a tab replaces the status
//! set and `derive_active_tab` maps the current set back to the highlighted
//! tab — a drill-down status filter that no longer equals a preset highlights
//! "All Issues", exactly like web.

use gpui::{
    div, px, App, Entity, FontWeight, InteractiveElement as _, IntoElement, ParentElement,
    RenderOnce, SharedString, StatefulInteractiveElement as _, Styled, Window,
};
use gpui_component::{
    h_flex, input::InputState, v_flex, ActiveTheme as _, Icon, IconName, Sizable as _,
};

use domain::rows::Label;
use domain::{
    derive_active_tab, has_active_filters, tab_preset_statuses, IssueFilters, TabPreset,
};

use crate::actions::NewIssue;
use crate::active_filter_pills::ActiveFilterPills;
use crate::create_issue_dialog::indigo_button;
use crate::filter_popover::{FilterView, IssueFilterPopover, OnFiltersChange, OnViewChange};

/// Compact tab height (web `h-7` = 28px, compact density).
const TAB_HEIGHT: f32 = 24.;

/// Web tab list: `{ all: "All Issues", active: "Active", backlog: "Backlog" }`.
const TABS: [(TabPreset, &str); 3] = [
    (TabPreset::All, "All Issues"),
    (TabPreset::Active, "Active"),
    (TabPreset::Backlog, "Backlog"),
];

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

    fn render_tab(
        &self,
        ix: usize,
        preset: TabPreset,
        label: &'static str,
        active: bool,
        cx: &App,
    ) -> impl IntoElement {
        let on_change = self.on_filters_change.clone();
        let filters = self.filters.clone();
        div()
            .id(("filter-tab", ix))
            .flex()
            .flex_shrink_0()
            .items_center()
            .h(px(TAB_HEIGHT))
            .px_3()
            .rounded_full()
            .text_xs()
            .cursor_pointer()
            .map(|tab| {
                if active {
                    // Web: bg-accent text-foreground font-medium.
                    tab.bg(cx.theme().accent)
                        .text_color(cx.theme().foreground)
                        .font_weight(FontWeight::MEDIUM)
                } else {
                    // Web: ghost — text-muted-foreground hover:text-foreground
                    // (+ the shadcn ghost hover:bg-accent).
                    tab.text_color(cx.theme().muted_foreground).hover(|style| {
                        style
                            .bg(cx.theme().accent)
                            .text_color(cx.theme().foreground)
                    })
                }
            })
            .child(SharedString::from(label))
            .on_click(move |_, window, cx| {
                // Web handleTabClick: replace ONLY the status set.
                let mut next = filters.clone();
                next.statuses = tab_preset_statuses(preset).to_vec();
                on_change(next, window, cx);
            })
    }
}

impl RenderOnce for IssueFilterBar {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let active_tab = derive_active_tab(&self.filters.statuses);

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

        // Tabs row (web: flex items-center gap-1 pb-1, LEFT-aligned).
        let mut tabs_row = h_flex().gap_1().pb_1().items_center();
        for (ix, (preset, label)) in TABS.into_iter().enumerate() {
            tabs_row = tabs_row.child(self.render_tab(ix, preset, label, active_tab == preset, cx));
        }

        v_flex()
            .w_full()
            .flex_shrink_0()
            .px_4()
            .child(title_row)
            .child(tabs_row)
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
