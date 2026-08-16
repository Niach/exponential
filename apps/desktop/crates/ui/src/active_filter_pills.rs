//! Active-filter pills — web parity target
//! `apps/web/src/components/active-filter-pills.tsx` (masterplan-v3 §4.2
//! "Board view": "`ActiveFilterPills` (`Tag` chips with an ✕) below").
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
    h_flex, ActiveTheme as _, Icon, Sizable as _,
};

use domain::options::get_issue_priority_config;
use domain::rows::Label;
use domain::statuses::{status_key_matches, ResolvedStatus};
use domain::{empty_filters, IssueFilters, IssuePriority};

use crate::filter_popover::OnFiltersChange;
use crate::icons::{option_icon, registry, resolved_status_icon};
use crate::issue_list::parse_hex_color;

/// Compact pill height (web `h-6` = 24px, compact density).
const PILL_HEIGHT: f32 = 20.;

#[derive(IntoElement)]
pub struct ActiveFilterPills {
    filters: IssueFilters,
    labels: Vec<Label>,
    /// EXP-314: the scope team's status vocabulary — a pill renders the
    /// RESOLVED name/icon of its group key.
    statuses: Vec<ResolvedStatus>,
    on_filters_change: OnFiltersChange,
}

impl ActiveFilterPills {
    pub fn new(
        filters: IssueFilters,
        labels: Vec<Label>,
        statuses: Vec<ResolvedStatus>,
        on_filters_change: OnFiltersChange,
    ) -> Self {
        Self {
            filters,
            labels,
            statuses,
            on_filters_change,
        }
    }
}

impl RenderOnce for ActiveFilterPills {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        // Web: flex items-center gap-1.5 px-6 py-1.5 flex-wrap (the px-6 is
        // INSIDE the bar's own horizontal padding — copied as-is, compacted).
        let mut row = h_flex().flex_wrap().items_center().gap_1p5().px_4().py_1();

        // Pills read in the TEAM's status order (EXP-314), not in the order
        // the user happened to tick the boxes — the same vocabulary order the
        // popover lists them in. A key with no matching status (the row was
        // deleted) renders NOTHING, and toggling any pill prunes it.
        let picked = self.statuses.iter().filter(|status| {
            self.filters
                .status_keys
                .iter()
                .any(|token| status_key_matches(status, token))
        });
        for (ix, status) in picked.enumerate() {
            row = row.child(status_pill(
                ix,
                status,
                &self.statuses,
                self.filters.clone(),
                self.on_filters_change.clone(),
                cx,
            ));
        }
        let priorities = IssuePriority::DISPLAY_ORDER
            .iter()
            .copied()
            .filter(|priority| self.filters.priorities.contains(priority));
        for (ix, priority) in priorities.enumerate() {
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
                .ghost().cursor_pointer()
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
fn pill_base(id: impl Into<ElementId>, _cx: &App) -> gpui::Stateful<gpui::Div> {
    crate::surface::glass_pill(
        div()
            .id(id)
            .flex()
            .items_center()
            .h(px(PILL_HEIGHT))
            .gap_1()
            .px_2()
            .text_xs()
            .cursor_pointer(),
        false,
    )
    .hover(|style| style.bg(theme::tokens::glass::FILL_ACTIVE.to_hsla()))
}

fn pill_close_icon(cx: &App) -> impl IntoElement {
    Icon::new(registry::UI_CLOSE)
        .size_2p5()
        .text_color(cx.theme().muted_foreground)
}

fn status_pill(
    ix: usize,
    status: &ResolvedStatus,
    known: &[ResolvedStatus],
    filters: IssueFilters,
    on_change: OnFiltersChange,
    cx: &App,
) -> impl IntoElement {
    let this = status.clone();
    // Any key that no longer names a live status is pruned on the next toggle
    // (a deleted status must not linger as an invisible active filter). The
    // comparison is TOKEN matching, so a pre-sync `builtin:<key>` key that now
    // names a synced row is removed by its own pill, not pruned as dead.
    let live: Vec<ResolvedStatus> = known.to_vec();
    pill_base(("filter-pill-status", ix), cx)
        .child(resolved_status_icon(status, cx).size_3())
        .child(SharedString::from(status.name.clone()))
        .child(pill_close_icon(cx))
        .on_click(move |_, window, cx| {
            let mut next = filters.clone();
            next.status_keys.retain(|candidate| {
                !status_key_matches(&this, candidate)
                    && live
                        .iter()
                        .any(|status| status_key_matches(status, candidate))
            });
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
