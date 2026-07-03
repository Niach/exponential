//! The virtualized issue list — the §4.2/§4.6 board core (web parity target:
//! `apps/web/src/components/issue-list.tsx` at compact density, EXP-2f).
//!
//! Structure: the board query (`queries::project_board` / `my_issues`, §4.1)
//! yields status groups; groups + rows flatten into ONE row vector backing a
//! `v_virtual_list` (§4.6 — the list can be long; virtualization is
//! mandatory). Status-group headers collapse (chevron, web parity), empty
//! groups are hidden, rows key by issue id (**stable ElementIds** — an
//! Electric echo or 409 refetch re-renders changed rows without a scroll
//! reset, §4.6).
//!
//! Row grid — the web template at compact density (§4.4 knob 4): `24px
//! priority · 72px identifier · 24px status · 1fr title · auto labels · auto
//! due`, ~28px row height (web ~40px). Fixed-width flex cells express the
//! grid template.
//!
//! Inline dropdowns (EXP-1 #5): priority + status are shadcn-style
//! `DropdownMenu`s whose option rows carry the `domain` table icon + color;
//! selecting one fires the §4.1 **un-gated** tRPC mutation (`issues.update`)
//! on a background thread — the UI updates only via the Electric echo. The
//! dropdown cells `stop_propagation` so opening them never triggers the
//! row's click→detail navigation (§4.6's #1 bug source).

use std::collections::HashSet;
use std::rc::Rc;

use gpui::{
    div, px, size, App, ElementId, FontWeight, InteractiveElement as _, IntoElement,
    ParentElement, Pixels, Render, SharedString, Size, StatefulInteractiveElement as _, Styled,
    Window,
};
use gpui_component::{
    button::{Button, ButtonVariants as _},
    h_flex,
    menu::{DropdownMenu as _, PopupMenuItem},
    scroll::ScrollableElement as _,
    skeleton::Skeleton,
    v_flex, v_virtual_list, ActiveTheme as _, Icon, IconName, Side, Sizable as _,
    VirtualListScrollHandle,
};
use sync::Store;

use domain::board::format_short_date;
use domain::options::{
    get_issue_priority_config, get_issue_status_config, IssueOption, ISSUE_PRIORITY_OPTIONS,
    ISSUE_STATUS_OPTIONS,
};
use domain::rows::{Issue, Label};
use domain::{IssueFilters, IssueStatus};

use crate::icons::{option_icon, ExpIcon};
use crate::navigation::{navigate, Screen};
use crate::queries::{self, BoardData};

/// Compact row height (§4.4: ~28px vs web's ~40px desktop row).
const ROW_HEIGHT: f32 = 28.;
/// Group header height (web py-1.5 + text-sm, compacted).
const HEADER_HEIGHT: f32 = 28.;

/// What this list shows (set by the screens panel on navigation).
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub enum IssueQuery {
    /// Nothing selected yet (renders the syncing skeleton).
    #[default]
    None,
    /// One project's board (`use-project-board-data`).
    Project { project_id: String },
    /// My Issues (`use-my-issues-data`): assignee == me in the workspace.
    MyIssues {
        workspace_id: String,
        user_id: String,
    },
}

/// One flattened virtual-list row.
enum ListRow {
    Header {
        status: IssueStatus,
        count: usize,
        collapsed: bool,
    },
    Issue {
        issue: Issue,
        labels: Vec<Label>,
    },
}

pub struct IssueListView {
    query: IssueQuery,
    filters: IssueFilters,
    /// Collapsed status groups (web `collapsedGroups`).
    collapsed: HashSet<IssueStatus>,
    /// Rows of the CURRENT render — rebuilt in `render`, read by the
    /// virtual-list range closure afterwards.
    rows: Rc<Vec<ListRow>>,
    scroll_handle: VirtualListScrollHandle,
    _subscriptions: Vec<gpui::Subscription>,
}

impl IssueListView {
    pub fn new(cx: &mut gpui::Context<Self>) -> Self {
        // §4.1 reactivity: observe exactly the collections the board query
        // reads; an Electric echo re-renders the list automatically.
        let collections = Store::global(cx).collections().clone();
        let subscriptions = vec![
            cx.observe(&collections.issues, |_, _, cx| cx.notify()),
            cx.observe(&collections.issue_labels, |_, _, cx| cx.notify()),
            cx.observe(&collections.labels, |_, _, cx| cx.notify()),
            cx.observe(&collections.projects, |_, _, cx| cx.notify()),
        ];

        Self {
            query: IssueQuery::None,
            filters: IssueFilters::empty(),
            collapsed: HashSet::new(),
            rows: Rc::new(Vec::new()),
            scroll_handle: VirtualListScrollHandle::new(),
            _subscriptions: subscriptions,
        }
    }

    /// Point the list at a new scope. Collapse state resets — it is
    /// per-board, like the web component's local state.
    pub fn set_query(&mut self, query: IssueQuery, cx: &mut gpui::Context<Self>) {
        if self.query == query {
            return;
        }
        self.query = query;
        self.collapsed.clear();
        cx.notify();
    }

    /// Replace the active filters (driven by the §4.2 `BoardView` — tabs,
    /// filter popover and pills all funnel through it).
    pub fn set_filters(&mut self, filters: IssueFilters, cx: &mut gpui::Context<Self>) {
        if self.filters == filters {
            return;
        }
        self.filters = filters;
        cx.notify();
    }

    fn board_data(&self, cx: &App) -> Option<BoardData> {
        match &self.query {
            IssueQuery::None => None,
            IssueQuery::Project { project_id } => {
                Some(queries::project_board(cx, project_id, &self.filters))
            }
            IssueQuery::MyIssues {
                workspace_id,
                user_id,
            } => Some(queries::my_issues(cx, workspace_id, user_id, &self.filters)),
        }
    }

    fn toggle_group(&mut self, status: IssueStatus, cx: &mut gpui::Context<Self>) {
        if !self.collapsed.remove(&status) {
            self.collapsed.insert(status);
        }
        cx.notify();
    }

    // -- row rendering -------------------------------------------------------

    fn render_row(
        &mut self,
        ix: usize,
        _window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> gpui::AnyElement {
        let Some(row) = self.rows.clone().get(ix).map(|row| match row {
            ListRow::Header {
                status,
                count,
                collapsed,
            } => self
                .render_group_header(*status, *count, *collapsed, cx)
                .into_any_element(),
            ListRow::Issue { issue, labels } => {
                self.render_issue_row(issue, labels, cx).into_any_element()
            }
        }) else {
            return div().into_any_element();
        };
        row
    }

    /// Web group header: chevron trigger + status icon + label + count on the
    /// per-status tinted background.
    fn render_group_header(
        &self,
        status: IssueStatus,
        count: usize,
        collapsed: bool,
        cx: &mut gpui::Context<Self>,
    ) -> impl IntoElement {
        let config = get_issue_status_config(status);
        let chevron = if collapsed {
            IconName::ChevronRight
        } else {
            IconName::ChevronDown
        };

        h_flex()
            .h(px(HEADER_HEIGHT))
            .w_full()
            .px_3()
            .gap_1p5()
            .items_center()
            .bg(status_header_bg(status))
            .border_b_1()
            .border_color(cx.theme().border.opacity(0.5))
            .child(
                Button::new(header_id("collapse", status))
                    .ghost()
                    .xsmall()
                    .icon(Icon::new(chevron).text_color(cx.theme().muted_foreground))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.toggle_group(status, cx);
                    })),
            )
            .child(option_icon(config, cx).xsmall())
            .child(
                div()
                    .text_sm()
                    .font_weight(FontWeight::MEDIUM)
                    .child(SharedString::from(config.label)),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(SharedString::from(count.to_string())),
            )
    }

    /// The web row grid at compact density: priority · identifier · status ·
    /// title · labels · due.
    fn render_issue_row(
        &self,
        issue: &Issue,
        labels: &[Label],
        cx: &mut gpui::Context<Self>,
    ) -> impl IntoElement {
        let issue_id = issue.id.clone();

        div()
            // Stable per-issue ElementId (§4.6): echo/refetch keeps row
            // identity, no scroll reset.
            .id(row_id("issue-row", &issue.id))
            .h(px(ROW_HEIGHT))
            .w_full()
            .px_3()
            .flex()
            .items_center()
            .cursor_pointer()
            .border_b_1()
            .border_color(cx.theme().border.opacity(0.3))
            .hover(|style| style.bg(cx.theme().colors.list_hover))
            // Whole row navigates to detail (web `onIssueClick`).
            .on_click(cx.listener(move |_, _, window, cx| {
                navigate(
                    window,
                    cx,
                    Screen::IssueDetail {
                        issue_id: issue_id.clone(),
                    },
                );
            }))
            // 24px priority dropdown cell (stop_propagation wrapper, §4.6).
            .child(
                control_cell(row_id("prio-cell", &issue.id))
                    .w_6()
                    .child(priority_dropdown(issue, cx)),
            )
            // 72px identifier.
            .child(
                div()
                    .w(px(72.))
                    .flex_shrink_0()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .font_family(theme::terminal::FONT_FAMILY)
                    .whitespace_nowrap()
                    .overflow_hidden()
                    .text_ellipsis()
                    .child(SharedString::from(issue.identifier.clone())),
            )
            // 24px status dropdown cell.
            .child(
                control_cell(row_id("status-cell", &issue.id))
                    .w_6()
                    .child(status_dropdown(issue, cx)),
            )
            // 1fr title (truncating), with the Repeat glyph for recurring
            // issues (web `issue.recurrenceInterval !== null`).
            .child(
                h_flex()
                    .flex_1()
                    .min_w_0()
                    .ml_2()
                    .gap_1p5()
                    .items_center()
                    .when(issue.recurrence_interval.is_some(), |row| {
                        row.child(
                            Icon::from(ExpIcon::Repeat)
                                .xsmall()
                                .text_color(cx.theme().muted_foreground),
                        )
                    })
                    .child(
                        div()
                            .text_sm()
                            .whitespace_nowrap()
                            .overflow_hidden()
                            .text_ellipsis()
                            .child(SharedString::from(issue.title.clone())),
                    ),
            )
            // auto labels (web rounded-full chips with the color dot).
            .child(
                h_flex()
                    .ml_4()
                    .gap_1p5()
                    .flex_shrink_0()
                    .children(labels.iter().map(|label| label_chip(label, cx))),
            )
            // auto due date: CalendarDays + short date (dimmed placeholder
            // glyph when unset — web parity).
            .child(due_cell(issue, cx))
    }
}

// Fluent `when` helper (gpui's FluentBuilder) — imported via prelude below.
use gpui::prelude::FluentBuilder as _;

impl Render for IssueListView {
    fn render(&mut self, _window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let data = self.board_data(cx);

        // Base surface: EXP-1 #4 — the REAL list token (web page background),
        // never a card color.
        let base = v_flex().size_full().bg(cx.theme().colors.list);

        let Some(data) = data else {
            return base.child(list_skeleton(cx)).into_any_element();
        };

        // §4.1 load-bearing: while the first snapshot is in flight an empty
        // result is "still syncing" — skeleton, never an empty state.
        if data.groups.is_empty() {
            if !data.is_ready {
                return base.child(list_skeleton(cx)).into_any_element();
            }
            if data.has_any_issues && domain::has_active_filters(&self.filters) {
                return base
                    .child(empty_state(
                        Icon::from(ExpIcon::SearchX),
                        "No issues match your filters",
                        "Try removing some filters to see more issues.",
                        cx,
                    ))
                    .into_any_element();
            }
            return base
                .child(empty_state(
                    Icon::from(ExpIcon::ListTodo),
                    "No issues yet",
                    "Create an issue to start tracking work.",
                    cx,
                ))
                .into_any_element();
        }

        // Flatten groups → virtual rows, honoring collapse; empty groups are
        // already hidden by the query (web parity).
        let mut rows: Vec<ListRow> = Vec::new();
        for group in &data.groups {
            if group.issues.is_empty() {
                // Status-filtered boards keep selected-but-empty groups in the
                // group list (web); render nothing for them in v1 like the
                // web's empty Collapsible body.
                continue;
            }
            let collapsed = self.collapsed.contains(&group.status);
            rows.push(ListRow::Header {
                status: group.status,
                count: group.issues.len(),
                collapsed,
            });
            if collapsed {
                continue;
            }
            for issue in &group.issues {
                let labels = data
                    .labels_by_issue
                    .get(&issue.id)
                    .cloned()
                    .unwrap_or_default();
                rows.push(ListRow::Issue {
                    issue: issue.clone(),
                    labels,
                });
            }
        }

        let sizes: Rc<Vec<Size<Pixels>>> = Rc::new(
            rows.iter()
                .map(|row| match row {
                    ListRow::Header { .. } => size(px(0.), px(HEADER_HEIGHT)),
                    ListRow::Issue { .. } => size(px(0.), px(ROW_HEIGHT)),
                })
                .collect(),
        );
        self.rows = Rc::new(rows);

        base.child(
            div().flex_1().min_h_0().child(
                v_flex()
                    .id("issue-list-scroll")
                    .relative()
                    .size_full()
                    .child(
                        v_virtual_list(
                            cx.entity().clone(),
                            "issue-list-rows",
                            sizes,
                            |this, visible_range, window, cx| {
                                visible_range
                                    .map(|ix| this.render_row(ix, window, cx))
                                    .collect()
                            },
                        )
                        .track_scroll(&self.scroll_handle),
                    )
                    .scrollbar(
                        &self.scroll_handle,
                        gpui_component::scroll::ScrollbarAxis::Vertical,
                    ),
            ),
        )
        .into_any_element()
    }
}

// ---------------------------------------------------------------------------
// Inline dropdowns (EXP-1 #5) + cells
// ---------------------------------------------------------------------------

/// A fixed control cell that swallows clicks so opening the control never
/// triggers the row navigation (§4.6 — the web wrapper's `stopPropagation`).
fn control_cell(id: ElementId) -> gpui::Stateful<gpui::Div> {
    div()
        .id(id)
        .flex_shrink_0()
        .flex()
        .items_center()
        .justify_center()
        .on_click(|_, _, cx| cx.stop_propagation())
}

/// Priority dropdown (web `PriorityDropdown`): xsmall ghost trigger with the
/// colored priority glyph; options carry icon + label + check (right side —
/// left-side checks would replace our icons, §4.6/EXP-1 #5).
fn priority_dropdown(issue: &Issue, cx: &App) -> impl IntoElement {
    let config = get_issue_priority_config(issue.priority);
    let current = issue.priority;
    let issue_id = issue.id.clone();

    Button::new(row_id("priority", &issue.id))
        .ghost()
        .xsmall()
        .icon(option_icon(config, cx))
        .dropdown_menu(move |menu, _window, cx| {
            let mut menu = menu.check_side(Side::Right);
            for option in &ISSUE_PRIORITY_OPTIONS {
                menu = menu.item(option_item(option, option.value == current, cx, {
                    let issue_id = issue_id.clone();
                    let value = option.value;
                    move |cx| {
                        let mut input = api::issues::IssuesUpdateInput::new(issue_id.clone());
                        input.priority = Some(value);
                        spawn_issue_update(cx, input);
                    }
                }));
            }
            menu
        })
}

/// Status dropdown (web `StatusDropdown`).
fn status_dropdown(issue: &Issue, cx: &App) -> impl IntoElement {
    let config = get_issue_status_config(issue.status);
    let current = issue.status;
    let issue_id = issue.id.clone();

    Button::new(row_id("status", &issue.id))
        .ghost()
        .xsmall()
        .icon(option_icon(config, cx))
        .dropdown_menu(move |menu, _window, cx| {
            let mut menu = menu.check_side(Side::Right);
            for option in &ISSUE_STATUS_OPTIONS {
                menu = menu.item(option_item(option, option.value == current, cx, {
                    let issue_id = issue_id.clone();
                    let value = option.value;
                    move |cx| {
                        let mut input = api::issues::IssuesUpdateInput::new(issue_id.clone());
                        input.status = Some(value);
                        spawn_issue_update(cx, input);
                    }
                }));
            }
            menu
        })
}

/// One option row: `domain`-table icon (colored) + label + check when
/// current (EXP-1 #5 — never an iconless native menu).
fn option_item<V: Copy + 'static>(
    option: &'static IssueOption<V>,
    checked: bool,
    cx: &App,
    on_select: impl Fn(&mut App) + 'static,
) -> PopupMenuItem {
    PopupMenuItem::new(SharedString::from(option.label))
        .icon(option_icon(option, cx))
        .checked(checked)
        .on_click(move |_, _, cx| on_select(cx))
}

/// §4.1 un-gated inline mutation: fire `issues.update` on a background
/// thread; the UI reflects the change when the Electric echo lands (observe →
/// re-render). Errors are logged — the row simply stays put (the echo never
/// arrives), matching the web's silent-toastless inline behavior.
fn spawn_issue_update(cx: &mut App, input: api::issues::IssuesUpdateInput) {
    let Some(trpc) = queries::trpc_client(cx) else {
        log::warn!("[ui] issues.update skipped: no signed-in account");
        return;
    };
    cx.background_executor()
        .spawn(async move {
            if let Err(err) = api::issues::issues_update(&trpc, &input) {
                log::warn!("[ui] issues.update({}) failed: {err}", input.id);
            }
        })
        .detach();
}

/// `#rrggbb` → Hsla (label/project colors are stored as hex strings; shared
/// by the board pills, filter popover and search rows).
pub(crate) fn parse_hex_color(hex: &str) -> Option<gpui::Hsla> {
    let hex = hex.trim().strip_prefix('#')?;
    if hex.len() != 6 || !hex.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
    let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
    let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
    Some(
        gpui::Rgba {
            r: r as f32 / 255.,
            g: g as f32 / 255.,
            b: b as f32 / 255.,
            a: 1.0,
        }
        .into(),
    )
}

/// Web label chip: rounded-full border, 1.5px color dot, label name.
fn label_chip(label: &Label, cx: &App) -> impl IntoElement {
    let color = label
        .color
        .as_deref()
        .and_then(parse_hex_color)
        .unwrap_or(cx.theme().muted_foreground);
    h_flex()
        .gap_1()
        .px_1p5()
        .border_1()
        .border_color(cx.theme().border.opacity(0.5))
        .rounded_full()
        .text_xs()
        .text_color(cx.theme().muted_foreground)
        .items_center()
        .child(div().size_1p5().rounded_full().flex_shrink_0().bg(color))
        .child(SharedString::from(label.name.clone()))
}

/// Web due cell: `CalendarDays` + "Jul 3" when due; a 30%-dimmed glyph when
/// not (the web renders the dimmed trigger; the date Popover is a later step).
fn due_cell(issue: &Issue, cx: &App) -> impl IntoElement {
    let cell = h_flex().ml_3().gap_1().items_center().flex_shrink_0();
    match issue.due_date.as_deref() {
        Some(due) => cell
            .child(
                Icon::from(ExpIcon::CalendarDays)
                    .xsmall()
                    .text_color(cx.theme().muted_foreground),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .whitespace_nowrap()
                    .child(SharedString::from(format_short_date(due))),
            ),
        None => cell.child(
            Icon::from(ExpIcon::CalendarDays)
                .xsmall()
                .text_color(cx.theme().muted_foreground.opacity(0.3)),
        ),
    }
}

// ---------------------------------------------------------------------------
// Chrome bits
// ---------------------------------------------------------------------------

/// Web `statusHeaderBg` — fixed rgba tints per status group (copied verbatim
/// from `issue-list.tsx`; they are literals there too, not theme tokens).
fn status_header_bg(status: IssueStatus) -> gpui::Hsla {
    let (r, g, b, a) = match status {
        IssueStatus::Todo => (212, 212, 216, 0.08),
        IssueStatus::InProgress => (234, 179, 8, 0.10),
        IssueStatus::Done => (34, 197, 94, 0.10),
        // backlog / cancelled / duplicate / unknown share the zinc tint.
        _ => (113, 113, 122, 0.08),
    };
    gpui::Rgba {
        r: r as f32 / 255.,
        g: g as f32 / 255.,
        b: b as f32 / 255.,
        a,
    }
    .into()
}

/// Web `IssueListSkeleton`: one header row + five body rows of placeholders.
fn list_skeleton(cx: &App) -> impl IntoElement {
    let header = h_flex()
        .h(px(HEADER_HEIGHT))
        .w_full()
        .px_3()
        .gap_2()
        .items_center()
        .bg(cx.theme().colors.list_head)
        .border_b_1()
        .border_color(cx.theme().border.opacity(0.5))
        .child(Skeleton::new().size_3p5().rounded_full())
        .child(Skeleton::new().h_3p5().w_24());

    let mut body = v_flex().w_full();
    for _ in 0..5 {
        body = body.child(
            h_flex()
                .h(px(ROW_HEIGHT))
                .w_full()
                .px_3()
                .gap_3()
                .items_center()
                .border_b_1()
                .border_color(cx.theme().border.opacity(0.3))
                .child(Skeleton::new().size_4().rounded_full())
                .child(Skeleton::new().h_3().w(px(56.)))
                .child(Skeleton::new().size_4().rounded_full())
                .child(Skeleton::new().h_3p5().flex_1().max_w(px(288.))),
        );
    }

    v_flex().w_full().child(header).child(body)
}

/// Web `EmptyState`: centered icon + title + description.
fn empty_state(
    icon: Icon,
    title: &'static str,
    description: &'static str,
    cx: &App,
) -> impl IntoElement {
    v_flex()
        .size_full()
        .items_center()
        .justify_center()
        .gap_2()
        .child(icon.size_6().text_color(cx.theme().muted_foreground))
        .child(div().text_sm().font_weight(FontWeight::MEDIUM).child(title))
        .child(
            div()
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child(description),
        )
}

/// Stable per-issue element id: `{kind}-{issue_id}`.
fn row_id(kind: &str, issue_id: &str) -> ElementId {
    ElementId::Name(SharedString::from(format!("{kind}-{issue_id}")))
}

fn header_id(kind: &str, status: IssueStatus) -> ElementId {
    ElementId::Name(SharedString::from(format!(
        "{kind}-{}",
        status.as_wire().unwrap_or("unknown")
    )))
}
