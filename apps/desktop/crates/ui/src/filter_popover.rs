//! The board filter popover — web parity target
//! `apps/web/src/components/issue-filter-popover.tsx` (masterplan-v3 §4.2
//! "Board view").
//!
//! A `Popover` hosting the web component's two-level drill-down: a
//! **categories** view (Status / Priority / Labels rows with count badges and
//! a chevron) that navigates into per-category **option views** (back row +
//! checkbox option rows; the labels view adds the "Filter labels..." search
//! input). Toggling never closes the popover (web behavior); closing it
//! resets the drill-down back to categories (web `onOpenChange`).
//!
//! Stateless `RenderOnce`: the current [`FilterView`], the filters and the
//! label-search `InputState` live on the owning `BoardView` (§4.1 — views
//! render from state snapshots; mutations flow back through callbacks, the
//! Rust analog of the web component's `filters`/`onFiltersChange` props).

use std::rc::Rc;

use gpui::{
    div, px, App, ElementId, Entity, Focusable as _, FontWeight, InteractiveElement as _,
    IntoElement, ParentElement, RenderOnce, SharedString, StatefulInteractiveElement as _, Styled,
    Window,
};
use gpui_component::{
    button::{Button, ButtonVariants as _},
    checkbox::Checkbox,
    h_flex,
    input::{Input, InputState},
    popover::Popover,
    v_flex, ActiveTheme as _, Icon, IconName, Sizable as _,
};

use domain::options::{IssueOption, ISSUE_PRIORITY_OPTIONS, ISSUE_STATUS_OPTIONS};
use domain::rows::Label;
use domain::{active_filter_count, IssueFilters};

use crate::icons::{option_icon, ExpIcon};
use crate::issue_list::parse_hex_color;

/// Which pane the popover shows (web `type View`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FilterView {
    #[default]
    Categories,
    Status,
    Priority,
    Labels,
}

/// `onFiltersChange` — the owner replaces its whole `IssueFilters` (web prop
/// shape, kept 1:1 so the bar/pills/popover all share one callback).
pub type OnFiltersChange = Rc<dyn Fn(IssueFilters, &mut Window, &mut App)>;
/// Drill-down navigation (web `setView`).
pub type OnViewChange = Rc<dyn Fn(FilterView, &mut Window, &mut App)>;
/// Toggle one option value in/out of the active filter set.
type OnToggleValue<V> = Rc<dyn Fn(V, &mut Window, &mut App)>;
/// Toggle a fixed row (label rows capture their own id).
type OnToggleRow = Rc<dyn Fn(&mut Window, &mut App)>;

/// Web `bg-indigo-500/20` — the count-badge pill background (a literal
/// Tailwind color on web too, not a theme token).
pub(crate) fn indigo_badge_bg() -> gpui::Hsla {
    gpui::Rgba {
        r: 99. / 255.,
        g: 102. / 255.,
        b: 241. / 255.,
        a: 0.2,
    }
    .into()
}

/// Web `text-indigo-400`.
pub(crate) fn indigo_badge_fg() -> gpui::Hsla {
    gpui::Rgba {
        r: 129. / 255.,
        g: 140. / 255.,
        b: 248. / 255.,
        a: 1.,
    }
    .into()
}

/// The rounded indigo count pill (web `rounded-full bg-indigo-500/20
/// text-indigo-400 px-1.5 text-[0.625rem] font-medium`).
pub(crate) fn count_badge(count: usize) -> impl IntoElement {
    div()
        .rounded_full()
        .bg(indigo_badge_bg())
        .text_color(indigo_badge_fg())
        .px_1p5()
        .text_size(px(10.))
        .font_weight(FontWeight::MEDIUM)
        .child(SharedString::from(count.to_string()))
}

#[derive(IntoElement)]
pub struct IssueFilterPopover {
    filters: IssueFilters,
    labels: Vec<Label>,
    view: FilterView,
    label_query: Entity<InputState>,
    on_filters_change: OnFiltersChange,
    on_view_change: OnViewChange,
}

impl IssueFilterPopover {
    pub fn new(
        filters: IssueFilters,
        labels: Vec<Label>,
        view: FilterView,
        label_query: Entity<InputState>,
        on_filters_change: OnFiltersChange,
        on_view_change: OnViewChange,
    ) -> Self {
        Self {
            filters,
            labels,
            view,
            label_query,
            on_filters_change,
            on_view_change,
        }
    }
}

impl RenderOnce for IssueFilterPopover {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let count = active_filter_count(&self.filters);

        // Web trigger: ghost xs `text-muted-foreground` — ListFilter icon +
        // "Filter" + count badge.
        let trigger = Button::new("issue-filter-trigger")
            .ghost()
            .xsmall()
            .text_color(cx.theme().muted_foreground)
            .icon(
                Icon::from(ExpIcon::ListFilter)
                    .size_3()
                    .text_color(cx.theme().muted_foreground),
            )
            .label("Filter")
            .when(count > 0, |button| button.child(count_badge(count)));

        let content = match self.view {
            FilterView::Categories => categories_view(
                &self.filters,
                self.on_view_change.clone(),
                self.label_query.clone(),
                cx,
            )
            .into_any_element(),
            FilterView::Status => option_filter_view(
                "filter-status",
                "Status",
                &ISSUE_STATUS_OPTIONS,
                self.filters.statuses.clone(),
                {
                    let filters = self.filters.clone();
                    let on_change = self.on_filters_change.clone();
                    Rc::new(move |value, window, cx| {
                        let mut next = filters.clone();
                        toggle_value(&mut next.statuses, value);
                        on_change(next, window, cx);
                    })
                },
                self.on_view_change.clone(),
                cx,
            )
            .into_any_element(),
            FilterView::Priority => option_filter_view(
                "filter-priority",
                "Priority",
                &ISSUE_PRIORITY_OPTIONS,
                self.filters.priorities.clone(),
                {
                    let filters = self.filters.clone();
                    let on_change = self.on_filters_change.clone();
                    Rc::new(move |value, window, cx| {
                        let mut next = filters.clone();
                        toggle_value(&mut next.priorities, value);
                        on_change(next, window, cx);
                    })
                },
                self.on_view_change.clone(),
                cx,
            )
            .into_any_element(),
            FilterView::Labels => labels_view(
                &self.filters,
                &self.labels,
                &self.label_query,
                self.on_filters_change.clone(),
                self.on_view_change.clone(),
                cx,
            )
            .into_any_element(),
        };

        // Web popover: w-[14rem] p-0, align="start" (the default TopLeft
        // anchor). Content carries the shadcn Command p-1 inset.
        Popover::new("issue-filter-popover")
            .w(px(224.))
            .p_1()
            .trigger(trigger)
            .on_open_change({
                let on_view_change = self.on_view_change.clone();
                let label_query = self.label_query.clone();
                move |_open, window, cx| {
                    // Web resets the drill-down on close; resetting on both
                    // flips also guarantees a fresh label search per open.
                    on_view_change(FilterView::Categories, window, cx);
                    label_query.update(cx, |input, cx| input.set_value("", window, cx));
                }
            })
            .child(content)
    }
}

/// Add-or-remove `value` (web `includes ? filter : [...push]`).
fn toggle_value<V: PartialEq>(values: &mut Vec<V>, value: V) {
    if let Some(ix) = values.iter().position(|v| *v == value) {
        values.remove(ix);
    } else {
        values.push(value);
    }
}

/// A shadcn `CommandItem`-style row: px-2 py-1.5(compact: py-1) text-sm
/// rounded, hover accent.
fn command_item(id: impl Into<ElementId>, cx: &App) -> gpui::Stateful<gpui::Div> {
    div()
        .id(id)
        .flex()
        .items_center()
        .px_2()
        .py_1()
        .gap_2()
        .rounded(cx.theme().radius)
        .text_sm()
        .cursor_pointer()
        .hover(|style| style.bg(cx.theme().accent))
}

/// Web `CategoriesView`: Status / Priority / Labels rows, each with an
/// active-count badge and a chevron.
fn categories_view(
    filters: &IssueFilters,
    on_view_change: OnViewChange,
    label_query: Entity<InputState>,
    cx: &App,
) -> impl IntoElement {
    let categories: [(FilterView, &'static str, usize); 3] = [
        (FilterView::Status, "Status", filters.statuses.len()),
        (FilterView::Priority, "Priority", filters.priorities.len()),
        (FilterView::Labels, "Labels", filters.label_ids.len()),
    ];

    let mut list = v_flex().w_full();
    for (ix, (view, label, count)) in categories.into_iter().enumerate() {
        let on_view_change = on_view_change.clone();
        let label_query = label_query.clone();
        list = list.child(
            command_item(("filter-category", ix), cx)
                .justify_between()
                .child(SharedString::from(label))
                .child(
                    h_flex()
                        .gap_1()
                        .items_center()
                        .when(count > 0, |row| row.child(count_badge(count)))
                        .child(
                            Icon::new(IconName::ChevronRight)
                                .size_3p5()
                                .text_color(cx.theme().muted_foreground),
                        ),
                )
                .on_click(move |_, window, cx| {
                    on_view_change(view, window, cx);
                    if view == FilterView::Labels {
                        // Web's CommandInput autofocuses when the view mounts.
                        label_query.read(cx).focus_handle(cx).focus(window, cx);
                    }
                }),
        );
    }
    list
}

/// Web `IssueOptionFilterView`: back row (arrow + title) then one checkbox
/// row per option-table entry (colored icon + label).
fn option_filter_view<V: Copy + PartialEq + 'static>(
    id_prefix: &'static str,
    title: &'static str,
    options: &'static [IssueOption<V>],
    selected: Vec<V>,
    on_toggle: OnToggleValue<V>,
    on_view_change: OnViewChange,
    cx: &App,
) -> impl IntoElement {
    let mut list = v_flex().w_full().child(back_row(
        ("filter-back", 0usize),
        title,
        on_view_change,
        cx,
    ));

    for (ix, option) in options.iter().enumerate() {
        let checked = selected.contains(&option.value);
        let value = option.value;
        let on_toggle = on_toggle.clone();
        list = list.child(
            command_item((id_prefix, ix), cx)
                // No handler on the checkbox itself — clicks bubble to the
                // row (web `pointer-events-none`; a second handler here would
                // double-toggle).
                .child(Checkbox::new((id_prefix, ix)).checked(checked))
                .child(option_icon(option, cx).size_3p5())
                .child(SharedString::from(option.label))
                .on_click(move |_, window, cx| on_toggle(value, window, cx)),
        );
    }
    list
}

/// Web `LabelsView`: back button + "Filter labels..." input, then checkbox +
/// color-dot rows filtered by the query, with the `CommandEmpty` fallback.
fn labels_view(
    filters: &IssueFilters,
    labels: &[Label],
    label_query: &Entity<InputState>,
    on_filters_change: OnFiltersChange,
    on_view_change: OnViewChange,
    cx: &App,
) -> impl IntoElement {
    let query = label_query.read(cx).value().trim().to_lowercase();
    let visible: Vec<&Label> = labels
        .iter()
        .filter(|label| query.is_empty() || label.name.to_lowercase().contains(&query))
        .collect();

    let header = h_flex()
        .gap_2()
        .px_1()
        .py_1()
        .items_center()
        .child(
            Button::new("filter-labels-back")
                .ghost()
                .xsmall()
                .icon(Icon::new(IconName::ArrowLeft).size_3p5())
                .on_click({
                    let on_view_change = on_view_change.clone();
                    move |_, window, cx| on_view_change(FilterView::Categories, window, cx)
                }),
        )
        .child(
            Input::new(label_query)
                .small()
                .appearance(false)
                .cleanable(true),
        );

    let mut list = v_flex().w_full().child(header).child(
        div()
            .h(px(1.))
            .w_full()
            .bg(cx.theme().border.opacity(0.5)),
    );

    if visible.is_empty() {
        return list.child(
            div()
                .py_4()
                .text_sm()
                .text_color(cx.theme().muted_foreground)
                .text_center()
                .child("No labels found."),
        );
    }

    for label in visible {
        let checked = filters.label_ids.contains(&label.id);
        let color = label
            .color
            .as_deref()
            .and_then(parse_hex_color)
            .unwrap_or(cx.theme().muted_foreground);
        let row_id = SharedString::from(format!("filter-label-{}", label.id));
        let on_toggle: OnToggleRow = Rc::new({
            let filters = filters.clone();
            let on_change = on_filters_change.clone();
            let label_id = label.id.clone();
            move |window, cx| {
                let mut next = filters.clone();
                toggle_value(&mut next.label_ids, label_id.clone());
                on_change(next, window, cx);
            }
        });
        list = list.child(
            command_item(ElementId::Name(row_id.clone()), cx)
                // Row owns the click (see option_filter_view).
                .child(
                    Checkbox::new(ElementId::Name(SharedString::from(format!(
                        "filter-label-check-{}",
                        label.id
                    ))))
                    .checked(checked),
                )
                .child(div().size_2p5().rounded_full().flex_shrink_0().bg(color))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .text_sm()
                        .whitespace_nowrap()
                        .overflow_hidden()
                        .text_ellipsis()
                        .child(SharedString::from(label.name.clone())),
                )
                .on_click(move |_, window, cx| on_toggle(window, cx)),
        );
    }
    list
}

/// The drill-down back row (web: `CommandItem` with ArrowLeft + medium title).
fn back_row(
    id: impl Into<ElementId>,
    title: &'static str,
    on_view_change: OnViewChange,
    cx: &App,
) -> impl IntoElement {
    command_item(id, cx)
        .child(
            Icon::new(IconName::ArrowLeft)
                .size_3p5()
                .text_color(cx.theme().muted_foreground),
        )
        .child(
            div()
                .font_weight(FontWeight::MEDIUM)
                .child(SharedString::from(title)),
        )
        .on_click(move |_, window, cx| on_view_change(FilterView::Categories, window, cx))
}

// Fluent `when` helper.
use gpui::prelude::FluentBuilder as _;
