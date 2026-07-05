//! Active-filter pills — web parity target
//! `apps/web/src/components/active-filter-pills.tsx` (masterplan-v3 §4.2
//! "Project board": "`ActiveFilterPills` (`Tag` chips with an ✕) below").
//!
//! One removable outline pill per active filter value — status pills carry
//! the colored status glyph, priority pills the priority glyph, label pills
//! the label color dot — plus the trailing ghost "Clear all". Clicking a pill
//! removes exactly that value (web `removeStatus`/`removePriority`/
//! `removeLabel`); "Clear all" resets to `emptyFilters`.
//!
//! The owner only renders this when `has_active_filters` (web returns null) —
//! `IssueFilterBar` guards with `.when(...)`.

use gpui::{
    div, px, App, ElementId, InteractiveElement as _, IntoElement, ParentElement, RenderOnce,
    SharedString, StatefulInteractiveElement as _, Styled, Window,
};
use gpui_component::{
    button::{Button, ButtonVariants as _},
    h_flex, ActiveTheme as _, Icon, IconName, Sizable as _,
};

use domain::options::{get_issue_priority_config, get_issue_status_config};
use domain::rows::Label;
use domain::{empty_filters, IssueFilters, IssuePriority, IssueStatus};

use crate::filter_popover::OnFiltersChange;
use crate::icons::option_icon;
use crate::issue_list::parse_hex_color;

/// Compact pill height (web `h-6` = 24px, compact density).
const PILL_HEIGHT: f32 = 20.;

#[derive(IntoElement)]
pub struct ActiveFilterPills {
    filters: IssueFilters,
    labels: Vec<Label>,
    on_filters_change: OnFiltersChange,
}

impl ActiveFilterPills {
    pub fn new(
        filters: IssueFilters,
        labels: Vec<Label>,
        on_filters_change: OnFiltersChange,
    ) -> Self {
        Self {
            filters,
            labels,
            on_filters_change,
        }
    }
}

impl RenderOnce for ActiveFilterPills {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        // Web: flex items-center gap-1.5 px-6 py-1.5 flex-wrap (the px-6 is
        // INSIDE the bar's own horizontal padding — copied as-is, compacted).
        let mut row = h_flex().flex_wrap().items_center().gap_1p5().px_4().py_1();

        for (ix, status) in self.filters.statuses.iter().copied().enumerate() {
            row = row.child(status_pill(
                ix,
                status,
                self.filters.clone(),
                self.on_filters_change.clone(),
                cx,
            ));
        }
        for (ix, priority) in self.filters.priorities.iter().copied().enumerate() {
            row = row.child(priority_pill(
                ix,
                priority,
                self.filters.clone(),
                self.on_filters_change.clone(),
                cx,
            ));
        }
        for label_id in &self.filters.label_ids {
            // Web: unresolved label ids render nothing.
            let Some(label) = self.labels.iter().find(|label| &label.id == label_id) else {
                continue;
            };
            row = row.child(label_pill(
                label,
                self.filters.clone(),
                self.on_filters_change.clone(),
                cx,
            ));
        }

        let on_clear = self.on_filters_change.clone();
        row.child(
            Button::new("filter-pills-clear-all")
                .ghost()
                .xsmall()
                .text_color(cx.theme().muted_foreground)
                .label("Clear all")
                .on_click(move |_, window, cx| on_clear(empty_filters(), window, cx)),
        )
    }
}

/// The web pill skeleton: outline rounded-full h-6 gap-1 text-xs with the
/// trailing ✕ — a stateful div (Button's icon/label slots cannot express the
/// icon + text + suffix-✕ order).
fn pill_base(id: impl Into<ElementId>, cx: &App) -> gpui::Stateful<gpui::Div> {
    div()
        .id(id)
        .flex()
        .items_center()
        .h(px(PILL_HEIGHT))
        .gap_1()
        .px_2()
        .rounded_full()
        .border_1()
        .border_color(cx.theme().border)
        .text_xs()
        .cursor_pointer()
        .hover(|style| style.bg(cx.theme().accent))
}

fn pill_close_icon(cx: &App) -> impl IntoElement {
    Icon::new(IconName::Close)
        .size_2p5()
        .text_color(cx.theme().muted_foreground)
}

fn status_pill(
    ix: usize,
    status: IssueStatus,
    filters: IssueFilters,
    on_change: OnFiltersChange,
    cx: &App,
) -> impl IntoElement {
    let config = get_issue_status_config(status);
    pill_base(("filter-pill-status", ix), cx)
        .child(option_icon(config, cx).size_3())
        .child(SharedString::from(config.label))
        .child(pill_close_icon(cx))
        .on_click(move |_, window, cx| {
            let mut next = filters.clone();
            next.statuses.retain(|s| *s != status);
            on_change(next, window, cx);
        })
}

fn priority_pill(
    ix: usize,
    priority: IssuePriority,
    filters: IssueFilters,
    on_change: OnFiltersChange,
    cx: &App,
) -> impl IntoElement {
    let config = get_issue_priority_config(priority);
    pill_base(("filter-pill-priority", ix), cx)
        .child(option_icon(config, cx).size_3())
        .child(SharedString::from(config.label))
        .child(pill_close_icon(cx))
        .on_click(move |_, window, cx| {
            let mut next = filters.clone();
            next.priorities.retain(|p| *p != priority);
            on_change(next, window, cx);
        })
}

fn label_pill(
    label: &Label,
    filters: IssueFilters,
    on_change: OnFiltersChange,
    cx: &App,
) -> impl IntoElement {
    let color = label
        .color
        .as_deref()
        .and_then(parse_hex_color)
        .unwrap_or(cx.theme().muted_foreground);
    let label_id = label.id.clone();
    pill_base(
        ElementId::Name(SharedString::from(format!("filter-pill-label-{}", label.id))),
        cx,
    )
    .child(div().size_2().rounded_full().flex_shrink_0().bg(color))
    .child(SharedString::from(label.name.clone()))
    .child(pill_close_icon(cx))
    .on_click(move |_, window, cx| {
        let mut next = filters.clone();
        next.label_ids.retain(|id| *id != label_id);
        on_change(next, window, cx);
    })
}
