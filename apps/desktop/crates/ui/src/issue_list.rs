//! The virtualized issue list — the §4.2/§4.6 board core (web parity target:
//! `apps/web/src/components/issue-list.tsx` at compact density).
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
//! Inline dropdowns: priority + status are shadcn-style
//! `DropdownMenu`s whose option rows carry the `domain` table icon + color;
//! selecting one fires the §4.1 **un-gated** tRPC mutation (`issues.update`)
//! on a background thread — the UI updates only via the Electric echo. The
//! dropdown cells `stop_propagation` so opening them never triggers the
//! row's click→detail navigation (§4.6's #1 bug source).

use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use gpui::{
    div, px, size, App, ClickEvent, ClipboardItem, ElementId, FocusHandle, FontWeight,
    InteractiveElement as _, IntoElement, KeyBinding, ParentElement, Pixels, Render, SharedString,
    Size, StatefulInteractiveElement as _, Styled, WeakEntity, Window,
};
use gpui_component::{
    avatar::Avatar,
    button::{Button, ButtonVariants as _},
    checkbox::Checkbox,
    h_flex,
    menu::{ContextMenuExt as _, DropdownMenu as _, PopupMenu, PopupMenuItem},
    scroll::ScrollableElement as _,
    skeleton::Skeleton,
    v_flex, v_virtual_list, ActiveTheme as _, Disableable as _, Icon, IconName, Side, Sizable as _,
    VirtualListScrollHandle,
};
use sync::Store;

use domain::board::format_short_date;
use domain::options::{
    get_issue_priority_config, get_issue_status_config, IssueOption, ISSUE_PRIORITY_OPTIONS,
    ISSUE_STATUS_OPTIONS,
};
use domain::rows::{Issue, Label, Project, User};
use domain::{IssueFilters, IssueStatus};

use crate::icons::{option_icon, ExpIcon};
use crate::issue_detail::{apply_status_selection, set_duplicate_of};
use crate::navigation::{navigate, Screen};
use crate::properties_panel::toggle_label;
use crate::queries::{self, BoardData};

/// Compact row height (§4.4: ~28px vs web's ~40px desktop row).
const ROW_HEIGHT: f32 = 28.;
/// Group header height (web py-1.5 + text-sm, compacted).
const HEADER_HEIGHT: f32 = 28.;
/// The row's hover group (web `group/row`) — reveals the bulk-select
/// checkbox. Reused per row: gpui resolves `group_hover` against the
/// innermost enclosing group with the name.
const ROW_GROUP: &str = "issue-row";
/// The list root's key context (the issue-detail pattern) — scopes the
/// select-all / clear-selection bindings to a focused issue list.
const KEY_CONTEXT: &str = "IssueList";
/// FIX F4: the bulk tRPC procedures cap inputs at 200 ids — clients chunk
/// and call sequentially.
const BULK_CHUNK: usize = 200;

gpui::actions!(
    issue_list,
    [
        /// Bulk select (cmd-a/ctrl-a): select every VISIBLE issue row —
        /// filtered, non-collapsed (Linear semantics, web parity).
        SelectAllIssues,
        /// Bulk select (escape): drop the selection.
        ClearIssueSelection,
    ]
);

/// Register the bulk-select bindings (call once from `ui::init`). The
/// predicate guards the keys against focused editables inside the list
/// subtree — the pinned gpui evaluates `!X` against the full focused
/// dispatch path (the issue-detail switcher's proven pattern).
pub(crate) fn init(cx: &mut App) {
    const BINDING_CONTEXT: &str =
        "IssueList && !Input && !MarkdownEditor && !MentionInput && !Terminal";
    #[cfg(target_os = "macos")]
    cx.bind_keys([KeyBinding::new("cmd-a", SelectAllIssues, Some(BINDING_CONTEXT))]);
    #[cfg(not(target_os = "macos"))]
    cx.bind_keys([KeyBinding::new("ctrl-a", SelectAllIssues, Some(BINDING_CONTEXT))]);
    cx.bind_keys([KeyBinding::new("escape", ClearIssueSelection, Some(BINDING_CONTEXT))]);
}

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

/// One flattened virtual-list row. The issue payload is boxed so the enum
/// stays small (clippy `large_enum_variant` — `Issue` is ~520 bytes vs the
/// header's ~10).
enum ListRow {
    Header {
        status: IssueStatus,
        count: usize,
        collapsed: bool,
    },
    Issue {
        issue: Box<Issue>,
        labels: Vec<Label>,
    },
}

pub struct IssueListView {
    query: IssueQuery,
    filters: IssueFilters,
    /// Collapsed status groups (web `collapsedGroups`).
    collapsed: HashSet<IssueStatus>,
    /// Bulk-selected issue ids (web `selectedIds`). Pruned in `render`
    /// against the current data set; collapsing a group hides rows but keeps
    /// them selected (web parity).
    selected: HashSet<String>,
    /// The shift-range anchor (web `anchorId`) — the last toggled row.
    select_anchor: Option<String>,
    /// One bulk mutation in flight at a time (the bar's buttons disable).
    bulk_busy: bool,
    /// Focus target of the [`KEY_CONTEXT`] bindings (terminal-dock pattern).
    focus_handle: FocusHandle,
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
            selected: HashSet::new(),
            select_anchor: None,
            bulk_busy: false,
            focus_handle: cx.focus_handle(),
            rows: Rc::new(Vec::new()),
            scroll_handle: VirtualListScrollHandle::new(),
            _subscriptions: subscriptions,
        }
    }

    /// Point the list at a new scope. Collapse + selection state resets —
    /// both are per-board, like the web component's local state.
    pub fn set_query(&mut self, query: IssueQuery, cx: &mut gpui::Context<Self>) {
        if self.query == query {
            return;
        }
        self.query = query;
        self.collapsed.clear();
        self.selected.clear();
        self.select_anchor = None;
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

    // -- bulk selection --------------------------------------------------------

    /// Toggle one row (checkbox / Cmd/Ctrl-click) and re-anchor on it.
    fn toggle_selected(&mut self, issue_id: String, cx: &mut gpui::Context<Self>) {
        if !self.selected.remove(&issue_id) {
            self.selected.insert(issue_id.clone());
        }
        self.select_anchor = Some(issue_id);
        cx.notify();
    }

    /// The flattened VISIBLE issue ids of the last render (collapse honored)
    /// — the range/select-all universe (web `visibleFlatIssues`).
    fn visible_issue_ids(&self) -> Vec<String> {
        self.rows
            .iter()
            .filter_map(|row| match row {
                ListRow::Issue { issue, .. } => Some(issue.id.clone()),
                ListRow::Header { .. } => None,
            })
            .collect()
    }

    /// Shift-click: ADD the contiguous visible slice between the anchor and
    /// the target — the anchor stays put for further extensions (web
    /// parity). Without a usable anchor it degrades to a plain toggle.
    fn extend_selection_to(&mut self, issue_id: String, cx: &mut gpui::Context<Self>) {
        let ids = self.visible_issue_ids();
        let anchor_ix = self
            .select_anchor
            .as_deref()
            .and_then(|anchor| ids.iter().position(|id| id == anchor));
        let target_ix = ids.iter().position(|id| *id == issue_id);
        let (Some(anchor_ix), Some(target_ix)) = (anchor_ix, target_ix) else {
            return self.toggle_selected(issue_id, cx);
        };
        let (from, to) = if anchor_ix <= target_ix {
            (anchor_ix, target_ix)
        } else {
            (target_ix, anchor_ix)
        };
        for id in &ids[from..=to] {
            self.selected.insert(id.clone());
        }
        cx.notify();
    }

    fn clear_selection(&mut self, cx: &mut gpui::Context<Self>) {
        if self.selected.is_empty() && self.select_anchor.is_none() {
            return;
        }
        self.selected.clear();
        self.select_anchor = None;
        cx.notify();
    }

    /// The workspace behind the current query — scopes the bulk bar's
    /// assignee/label pickers. `None` (join not synced yet) hides the bar.
    fn bulk_workspace_id(&self, cx: &App) -> Option<String> {
        let collections = Store::global(cx).collections();
        match &self.query {
            IssueQuery::None => None,
            IssueQuery::Project { project_id } => collections
                .projects
                .read(cx)
                .get(project_id)
                .map(|project| project.workspace_id.clone()),
            IssueQuery::MyIssues { workspace_id, .. } => Some(workspace_id.clone()),
        }
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
    /// title · labels · assignee · due (the 7-column
    /// `grid-cols-[1.5rem_4.5rem_1.5rem_1fr_auto_1.75rem_4.5rem]` template).
    fn render_issue_row(
        &self,
        issue: &Issue,
        labels: &[Label],
        cx: &mut gpui::Context<Self>,
    ) -> impl IntoElement {
        let issue_id = issue.id.clone();
        let menu_issue = issue.clone();
        let is_selected = self.selected.contains(&issue.id);
        let any_selected = !self.selected.is_empty();

        div()
            // Stable per-issue ElementId (§4.6): echo/refetch keeps row
            // identity, no scroll reset.
            .id(row_id("issue-row", &issue.id))
            .group(ROW_GROUP)
            .h(px(ROW_HEIGHT))
            .w_full()
            .px_3()
            .flex()
            .items_center()
            .cursor_pointer()
            .border_b_1()
            .border_color(cx.theme().border.opacity(0.3))
            .when(is_selected, |style| {
                style.bg(cx.theme().colors.list_active)
            })
            .hover(|style| style.bg(cx.theme().colors.list_hover))
            // Row click: Cmd/Ctrl toggles the selection, Shift extends the
            // range from the anchor, plain navigates to the detail. The
            // selection SURVIVES navigation (EXP-68): peeking into an issue
            // while composing a selection must not throw the picked set away
            // — it clears on scope change (`set_query`), Escape, or the bulk
            // bar's ✕.
            .on_click(cx.listener(move |this, event: &ClickEvent, window, cx| {
                // Modifier clicks drive selection; plain clicks navigate.
                let modifiers = event.modifiers();
                if modifiers.secondary() {
                    this.toggle_selected(issue_id.clone(), cx);
                    return;
                }
                if modifiers.shift {
                    this.extend_selection_to(issue_id.clone(), cx);
                    return;
                }
                navigate(
                    window,
                    cx,
                    Screen::IssueDetail {
                        issue_id: issue_id.clone(),
                    },
                );
            }))
            // Leading bulk-select checkbox: hover-revealed, pinned visible
            // while ANY selection exists (web `group-hover/row` parity).
            .child({
                let toggle_id = issue.id.clone();
                control_cell(row_id("select-cell", &issue.id))
                    .w_5()
                    .when(!any_selected, |cell| {
                        cell.invisible().group_hover(ROW_GROUP, |style| style.visible())
                    })
                    .child(
                        Checkbox::new(row_id("select", &issue.id))
                            .checked(is_selected)
                            .on_click(cx.listener(move |this, _: &bool, _, cx| {
                                this.toggle_selected(toggle_id.clone(), cx);
                            })),
                    )
            })
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
            // 28px assignee dropdown cell (web `AssigneeDropdown` — avatar or
            // dashed placeholder circle).
            .child(
                control_cell(row_id("assignee-cell", &issue.id))
                    .ml_3()
                    .child(assignee_dropdown(issue, cx)),
            )
            // auto due date: CalendarDays + short date (dimmed placeholder
            // glyph when unset — web parity; presets edit via the context
            // menu's "Set due date" submenu, mirroring `due-date-presets.tsx`).
            .child(due_cell(issue, cx))
            // Right-click context menu (web `IssueRowContextMenu`, §4.2/§4.6).
            .context_menu(move |menu, window, cx| {
                build_row_context_menu(menu, &menu_issue, window, cx)
            })
    }

    // -- bulk action bar -------------------------------------------------------

    /// The Linear-style floating bar (web `bulk-action-bar.tsx`): N selected ·
    /// Status · Priority · Assignee · Labels · Start coding · Delete
    /// (nested confirm) · clear. Buttons are icon-only with tooltips — every
    /// issue list renders inside the ~260px tool panel, where the web's
    /// labeled buttons cannot fit on one row. Property edits keep the
    /// selection alive — only delete clears it (FIX F3); every mutation
    /// chunks at 200 ids (FIX F4) and lands via the Electric echo.
    fn render_bulk_bar(
        &self,
        workspace_id: String,
        ids: Vec<String>,
        cx: &mut gpui::Context<Self>,
    ) -> gpui::AnyElement {
        let count = ids.len();
        let busy = self.bulk_busy;
        let list = cx.entity().downgrade();
        let border = cx.theme().border;
        let popover_bg = cx.theme().popover;
        let popover_fg = cx.theme().popover_foreground;
        let danger = cx.theme().danger;

        let status_menu = {
            let ids = ids.clone();
            let list = list.clone();
            Button::new("bulk-status")
                .ghost()
                .xsmall()
                .icon(Icon::from(ExpIcon::ListTodo))
                .tooltip("Status")
                .disabled(busy)
                .dropdown_menu_with_anchor(gpui::Anchor::BottomLeft, move |menu, _window, cx| {
                    let mut menu = menu.check_side(Side::Right);
                    // No Duplicate here: bulk marking has no canonical-issue
                    // picker, and status='duplicate' without duplicate_of_id
                    // breaks the pairing invariant (the single-issue path
                    // intercepts via apply_status_selection's picker).
                    for option in ISSUE_STATUS_OPTIONS
                        .iter()
                        .filter(|option| option.value != IssueStatus::Duplicate)
                    {
                        let ids = ids.clone();
                        let list = list.clone();
                        let value = option.value;
                        menu = menu.item(option_item(option, false, cx, move |_window, cx| {
                            spawn_bulk_op(
                                list.clone(),
                                cx,
                                ids.clone(),
                                false,
                                "issues.bulkUpdate",
                                move |trpc, chunk| {
                                    let mut input =
                                        api::issues::IssuesBulkUpdateInput::new(chunk.to_vec());
                                    input.status = Some(value);
                                    api::issues::issues_bulk_update(trpc, &input).map(|_| ())
                                },
                            );
                        }));
                    }
                    menu
                })
        };

        let priority_menu = {
            let ids = ids.clone();
            let list = list.clone();
            Button::new("bulk-priority")
                .ghost()
                .xsmall()
                .icon(Icon::from(ExpIcon::SignalHigh))
                .tooltip("Priority")
                .disabled(busy)
                .dropdown_menu_with_anchor(gpui::Anchor::BottomLeft, move |menu, _window, cx| {
                    let mut menu = menu.check_side(Side::Right);
                    for option in &ISSUE_PRIORITY_OPTIONS {
                        let ids = ids.clone();
                        let list = list.clone();
                        let value = option.value;
                        menu = menu.item(option_item(option, false, cx, move |_window, cx| {
                            spawn_bulk_op(
                                list.clone(),
                                cx,
                                ids.clone(),
                                false,
                                "issues.bulkUpdate",
                                move |trpc, chunk| {
                                    let mut input =
                                        api::issues::IssuesBulkUpdateInput::new(chunk.to_vec());
                                    input.priority = Some(value);
                                    api::issues::issues_bulk_update(trpc, &input).map(|_| ())
                                },
                            );
                        }));
                    }
                    menu
                })
        };

        let assignee_menu = {
            let ids = ids.clone();
            let list = list.clone();
            let workspace_id = workspace_id.clone();
            Button::new("bulk-assignee")
                .ghost()
                .xsmall()
                .icon(Icon::new(IconName::CircleUser))
                .tooltip("Assignee")
                .disabled(busy)
                .dropdown_menu_with_anchor(gpui::Anchor::BottomLeft, move |menu, _window, cx| {
                    let mut menu = menu.scrollable(true).max_h(px(320.));
                    menu = menu.item(
                        PopupMenuItem::new("Unassign")
                            .icon(Icon::new(IconName::Close))
                            .on_click({
                                let ids = ids.clone();
                                let list = list.clone();
                                move |_, _, cx| {
                                    spawn_bulk_op(
                                        list.clone(),
                                        cx,
                                        ids.clone(),
                                        false,
                                        "issues.bulkUpdate",
                                        |trpc, chunk| {
                                            let mut input =
                                                api::issues::IssuesBulkUpdateInput::new(
                                                    chunk.to_vec(),
                                                );
                                            input.assignee_id = api::Patch::Null;
                                            api::issues::issues_bulk_update(trpc, &input)
                                                .map(|_| ())
                                        },
                                    );
                                }
                            }),
                    );
                    for user in queries::workspace_users(cx, &workspace_id) {
                        let name = crate::comments::author_label(Some(&user));
                        let ids = ids.clone();
                        let list = list.clone();
                        let user_id = user.id.clone();
                        menu = menu.item(
                            PopupMenuItem::new(SharedString::from(name))
                                .icon(Icon::new(IconName::CircleUser))
                                .on_click(move |_, _, cx| {
                                    let user_id = user_id.clone();
                                    spawn_bulk_op(
                                        list.clone(),
                                        cx,
                                        ids.clone(),
                                        false,
                                        "issues.bulkUpdate",
                                        move |trpc, chunk| {
                                            let mut input =
                                                api::issues::IssuesBulkUpdateInput::new(
                                                    chunk.to_vec(),
                                                );
                                            input.assignee_id =
                                                api::Patch::Set(user_id.clone());
                                            api::issues::issues_bulk_update(trpc, &input)
                                                .map(|_| ())
                                        },
                                    );
                                }),
                        );
                    }
                    menu
                })
        };

        let labels_menu = {
            let ids = ids.clone();
            let list = list.clone();
            let workspace_id = workspace_id.clone();
            Button::new("bulk-labels")
                .ghost()
                .xsmall()
                .icon(Icon::from(ExpIcon::Tag))
                .tooltip("Labels")
                .disabled(busy)
                .dropdown_menu_with_anchor(gpui::Anchor::BottomLeft, move |menu, _window, cx| {
                    let mut menu = menu
                        .scrollable(true)
                        .max_h(px(320.))
                        .check_side(Side::Right);
                    let labels = queries::workspace_labels(cx, &workspace_id);
                    if labels.is_empty() {
                        return menu.item(PopupMenuItem::label("No labels in this team"));
                    }
                    // Tri-state per web: checked when the label is on ALL
                    // selected issues; toggling removes from all, else adds
                    // to all.
                    let selected_set: HashSet<&str> =
                        ids.iter().map(String::as_str).collect();
                    let mut counts: HashMap<String, usize> = HashMap::new();
                    for link in Store::global(cx).collections().issue_labels.read(cx).iter() {
                        if selected_set.contains(link.issue_id.as_str()) {
                            *counts.entry(link.label_id.clone()).or_default() += 1;
                        }
                    }
                    for label in labels {
                        let on_all =
                            counts.get(&label.id).copied().unwrap_or(0) == ids.len();
                        let dot = label
                            .color
                            .as_deref()
                            .and_then(parse_hex_color)
                            .unwrap_or(gpui::opaque_grey(0.5, 1.0));
                        let name = SharedString::from(label.name.clone());
                        let ids = ids.clone();
                        let list = list.clone();
                        let label_id = label.id.clone();
                        menu = menu.item(
                            PopupMenuItem::element(move |_, cx| {
                                h_flex()
                                    .gap_2()
                                    .items_center()
                                    .child(
                                        div().size_2().rounded_full().flex_shrink_0().bg(dot),
                                    )
                                    .child(
                                        div()
                                            .text_color(cx.theme().popover_foreground)
                                            .child(name.clone()),
                                    )
                            })
                            .checked(on_all)
                            .on_click(move |_, _, cx| {
                                let label_id = label_id.clone();
                                let op = if on_all {
                                    "issueLabels.bulkRemove"
                                } else {
                                    "issueLabels.bulkAdd"
                                };
                                spawn_bulk_op(
                                    list.clone(),
                                    cx,
                                    ids.clone(),
                                    false,
                                    op,
                                    move |trpc, chunk| {
                                        if on_all {
                                            api::labels::issue_labels_bulk_remove(
                                                trpc, &label_id, chunk,
                                            )
                                            .map(|_| ())
                                        } else {
                                            api::labels::issue_labels_bulk_add(
                                                trpc, &label_id, chunk,
                                            )
                                            .map(|_| ())
                                        }
                                    },
                                );
                            }),
                        );
                    }
                    menu
                })
        };

        // Bulk "Start coding": ONE batch coding session over the selection —
        // opens the unified Start-coding dialog with the selected issues
        // pre-checked (one repo per run is enforced there).
        let start_coding = {
            let ids = ids.clone();
            let workspace_id = workspace_id.clone();
            Button::new("bulk-start-coding")
                .ghost()
                .xsmall()
                .icon(Icon::new(IconName::Play))
                .tooltip("Start coding")
                .disabled(busy)
                .on_click(move |_, window, cx| {
                    crate::start_coding_dialog::open_for_selection(
                        window,
                        cx,
                        workspace_id.clone(),
                        ids.clone(),
                    );
                })
        };

        let delete_menu = {
            let ids = ids.clone();
            let list = list.clone();
            Button::new("bulk-delete")
                .ghost()
                .xsmall()
                .icon(Icon::new(IconName::Delete).text_color(danger))
                .tooltip("Delete selected")
                .disabled(busy)
                .dropdown_menu_with_anchor(gpui::Anchor::BottomLeft, move |menu, _window, _cx| {
                    // Nested confirm (destructive actions confirm first).
                    let label = if ids.len() == 1 {
                        "Confirm delete 1 issue".to_string()
                    } else {
                        format!("Confirm delete {} issues", ids.len())
                    };
                    let ids = ids.clone();
                    let list = list.clone();
                    menu.item(
                        PopupMenuItem::new(SharedString::from(label))
                            .icon(Icon::new(IconName::Delete))
                            .on_click(move |_, _, cx| {
                                spawn_bulk_op(
                                    list.clone(),
                                    cx,
                                    ids.clone(),
                                    true,
                                    "issues.bulkDelete",
                                    |trpc, chunk| {
                                        api::issues::issues_bulk_delete(trpc, chunk).map(|_| ())
                                    },
                                );
                            }),
                    )
                })
        };

        div()
            .absolute()
            .bottom_4()
            .left_0()
            .right_0()
            .flex()
            .justify_center()
            .child(
                h_flex()
                    .id("bulk-action-bar")
                    .occlude()
                    .gap_1()
                    .px_2()
                    .py_1()
                    .items_center()
                    .rounded(px(10.))
                    .border_1()
                    .border_color(border)
                    .bg(popover_bg)
                    .text_color(popover_fg)
                    .shadow_lg()
                    .child(
                        div()
                            .px_1()
                            .text_xs()
                            .font_weight(FontWeight::MEDIUM)
                            .whitespace_nowrap()
                            .child(SharedString::from(format!("{count} selected"))),
                    )
                    .child(
                        Button::new("bulk-clear")
                            .ghost()
                            .xsmall()
                            .icon(Icon::new(IconName::Close))
                            .tooltip("Clear selection")
                            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                                this.clear_selection(cx);
                            })),
                    )
                    .child(status_menu)
                    .child(priority_menu)
                    .child(assignee_menu)
                    .child(labels_menu)
                    .child(start_coding)
                    .child(delete_menu),
            )
            .into_any_element()
    }
}

// Fluent `when` helper (gpui's FluentBuilder) — imported via prelude below.
use gpui::prelude::FluentBuilder as _;

/// One bulk mutation run: chunk at [`BULK_CHUNK`] ids and call SEQUENTIALLY
/// on one background task (FIX F4 — Electric replays commits in order, so
/// the last echo landing implies every earlier chunk landed). `clear_on_ok`
/// is true ONLY for delete — property edits keep the selection alive
/// (FIX F3, Linear semantics). Failures log and stop the chunk loop; the
/// rows simply keep their old state (no echo), matching the list's silent
/// inline-mutation behavior.
fn spawn_bulk_op(
    list: WeakEntity<IssueListView>,
    cx: &mut App,
    ids: Vec<String>,
    clear_on_ok: bool,
    op: &'static str,
    call: impl Fn(&api::TrpcClient, &[String]) -> Result<(), api::ApiError> + Send + Sync + 'static,
) {
    if ids.is_empty() {
        return;
    }
    let Some(trpc) = queries::trpc_client(cx) else {
        log::warn!("[ui] {op} skipped: no signed-in account");
        return;
    };
    let _ = list.update(cx, |this, cx| {
        this.bulk_busy = true;
        cx.notify();
    });
    cx.spawn(async move |cx| {
        let result = cx
            .background_executor()
            .spawn(async move {
                for chunk in ids.chunks(BULK_CHUNK) {
                    call(&trpc, chunk)?;
                }
                Ok::<(), api::ApiError>(())
            })
            .await;
        let _ = list.update(cx, |this, cx| {
            this.bulk_busy = false;
            match result {
                Ok(()) => {
                    if clear_on_ok {
                        this.selected.clear();
                        this.select_anchor = None;
                    }
                }
                Err(err) => log::warn!("[ui] {op} failed: {err}"),
            }
            cx.notify();
        });
    })
    .detach();
}

impl Render for IssueListView {
    fn render(&mut self, _window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let data = self.board_data(cx);

        // Base surface: the REAL list token (web page background),
        // never a card color. Key context + tracked focus scope the
        // select-all/clear bindings here (terminal-dock pattern).
        let base = v_flex()
            .size_full()
            .bg(cx.theme().colors.list)
            .key_context(KEY_CONTEXT)
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(|this, _: &SelectAllIssues, _, cx| {
                let ids = this.visible_issue_ids();
                if ids.is_empty() {
                    return;
                }
                this.selected = ids.into_iter().collect();
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &ClearIssueSelection, _, cx| {
                if this.selected.is_empty() {
                    // Nothing to clear — let Escape reach outer surfaces.
                    cx.propagate();
                    return;
                }
                this.clear_selection(cx);
            }));

        let Some(data) = data else {
            self.rows = Rc::new(Vec::new());
            return base.child(list_skeleton(cx)).into_any_element();
        };

        // §4.1 load-bearing: while the first snapshot is in flight an empty
        // result is "still syncing" — skeleton, never an empty state.
        if data.groups.is_empty() {
            self.rows = Rc::new(Vec::new());
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

        // Prune selected ids whose rows left the data set (filter change,
        // delete elsewhere, sync) — web parity. Collapsed rows stay selected.
        if !self.selected.is_empty() {
            let present: HashSet<&str> = data
                .groups
                .iter()
                .flat_map(|group| group.issues.iter().map(|issue| issue.id.as_str()))
                .collect();
            self.selected.retain(|id| present.contains(id.as_str()));
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
                    issue: Box::new(issue.clone()),
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

        // The floating bulk action bar — selected ids snapshotted in visible
        // list order (workspace resolution can lag the issue rows; the bar
        // waits for it, the selection itself does not).
        let bulk_bar = if self.selected.is_empty() {
            None
        } else {
            self.bulk_workspace_id(cx).map(|workspace_id| {
                let ids: Vec<String> = data
                    .groups
                    .iter()
                    .flat_map(|group| group.issues.iter())
                    .filter(|issue| self.selected.contains(&issue.id))
                    .map(|issue| issue.id.clone())
                    .collect();
                self.render_bulk_bar(workspace_id, ids, cx)
            })
        };

        base.child(
            div()
                .flex_1()
                .min_h_0()
                .relative()
                .child(
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
                )
                .when_some(bulk_bar, |this, bar| this.child(bar)),
        )
        .into_any_element()
    }
}

// ---------------------------------------------------------------------------
// Inline dropdowns + cells
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
/// left-side checks would replace our icons, §4.6).
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
                    move |_window, cx| {
                        let mut input = api::issues::IssuesUpdateInput::new(issue_id.clone());
                        input.priority = Some(value);
                        spawn_issue_update(cx, input);
                    }
                }));
            }
            menu
        })
}

/// Status dropdown (web `StatusDropdown`). Selecting `duplicate` is intercepted
/// into the duplicate picker (L27), never a direct status write.
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
                    move |window, cx| apply_status_selection(issue_id.clone(), value, window, cx)
                }));
            }
            menu
        })
}

/// Assignee dropdown (web `AssigneeDropdown`): avatar trigger when assigned,
/// dashed placeholder circle otherwise; menu = Unassign + the workspace's
/// human members (current assignee first — web `orderedUsers`).
fn assignee_dropdown(issue: &Issue, cx: &App) -> impl IntoElement {
    let assignee = issue
        .assignee_id
        .as_deref()
        .and_then(|id| Store::global(cx).collections().users.read(cx).get(id).cloned());

    let trigger = match issue.assignee_id.as_deref() {
        // Assigned — the avatar seeds from the member's name, or `Member
        // <LAST4>` when the co-member's user row didn't sync.
        Some(id) => Button::new(row_id("assignee", &issue.id))
            .ghost()
            .xsmall()
            .child(
                Avatar::new()
                    .name(SharedString::from(crate::comments::user_label(
                        id,
                        assignee.as_ref(),
                    )))
                    .xsmall(),
            ),
        None => Button::new(row_id("assignee", &issue.id)).ghost().xsmall().child(
            div()
                .size_4()
                .rounded_full()
                .border_1()
                .border_dashed()
                .border_color(cx.theme().border)
                .flex()
                .items_center()
                .justify_center()
                .child(
                    Icon::new(IconName::User)
                        .size_2p5()
                        .text_color(cx.theme().muted_foreground.opacity(0.5)),
                ),
        ),
    };

    let issue_id = issue.id.clone();
    let project_id = issue.project_id.clone();
    let current = issue.assignee_id.clone();
    trigger.dropdown_menu(move |menu, _window, cx| {
        // Member lists grow with the workspace — cap + scroll (EXP-46a).
        // Scrollable ONLY on this top-level dropdown: the same body renders
        // as a context-menu SUBMENU below, where scrollable is unsupported
        // at the pinned gpui-component rev.
        let menu = menu.scrollable(true).max_h(px(320.));
        assignee_menu(menu, &issue_id, &project_id, current.as_deref(), cx)
    })
}

/// The shared assignee menu body (row dropdown + context-menu submenu).
fn assignee_menu(
    menu: PopupMenu,
    issue_id: &str,
    project_id: &str,
    current: Option<&str>,
    cx: &App,
) -> PopupMenu {
    let mut menu = menu.check_side(Side::Right);
    if current.is_some() {
        let issue_id = issue_id.to_string();
        menu = menu.item(
            PopupMenuItem::new("Unassign")
                .icon(Icon::new(IconName::Close))
                .on_click(move |_, _, cx| {
                    let mut input = api::issues::IssuesUpdateInput::new(issue_id.clone());
                    input.assignee_id = api::Patch::Null;
                    spawn_issue_update(cx, input);
                }),
        );
    }
    for user in assignable_users(project_id, current, cx) {
        let checked = current == Some(user.id.as_str());
        let name = crate::comments::author_label(Some(&user));
        let issue_id = issue_id.to_string();
        let user_id = user.id.clone();
        menu = menu.item(
            PopupMenuItem::new(SharedString::from(name))
                .icon(Icon::new(IconName::CircleUser))
                .checked(checked)
                .on_click(move |_, _, cx| {
                    let mut input = api::issues::IssuesUpdateInput::new(issue_id.clone());
                    input.assignee_id = api::Patch::Set(user_id.clone());
                    spawn_issue_update(cx, input);
                }),
        );
    }
    menu
}

/// The workspace's human members eligible as assignees (project → workspace →
/// members ⨝ users, agents hidden — web `people`), current assignee first
/// (web `orderedUsers`), then name-sorted.
fn assignable_users(project_id: &str, current: Option<&str>, cx: &App) -> Vec<User> {
    let collections = Store::global(cx).collections();
    let Some(project) = collections.projects.read(cx).get(project_id).cloned() else {
        return Vec::new();
    };
    let member_ids: HashSet<String> = collections
        .workspace_members
        .read(cx)
        .iter()
        .filter(|member| member.workspace_id == project.workspace_id)
        .map(|member| member.user_id.clone())
        .collect();
    let mut users: Vec<User> = collections
        .users
        .read(cx)
        .iter()
        .filter(|user| member_ids.contains(&user.id) && user.is_agent != Some(true))
        .cloned()
        .collect();
    users.sort_by_key(|user| {
        (
            current != Some(user.id.as_str()),
            crate::comments::author_label(Some(user)).to_lowercase(),
        )
    });
    users
}

// ---------------------------------------------------------------------------
// Row context menu (web `issue-row-menu/context-menu.tsx`)
// ---------------------------------------------------------------------------

/// Mirror of the web `IssueRowContextMenu`: header label, Open issue, Mark as
/// done / Move to todo, Copy issue ID, Mark as duplicate… / Unmark duplicate,
/// then Status / Assignee / Priority / Labels /
/// Move-to-project / Set-due-date submenus, then the Delete-issue confirm
/// submenu. Mutations are the §4.1 un-gated form.
fn build_row_context_menu(
    menu: PopupMenu,
    issue: &Issue,
    window: &mut Window,
    cx: &mut gpui::Context<PopupMenu>,
) -> PopupMenu {
    let mut menu = menu
        .check_side(Side::Right)
        .label(SharedString::from(issue.identifier.clone()));

    // Open issue.
    {
        let issue_id = issue.id.clone();
        menu = menu.item(
            PopupMenuItem::new("Open issue")
                .icon(Icon::from(ExpIcon::Pencil))
                .on_click(move |_, window, cx| {
                    navigate(
                        window,
                        cx,
                        Screen::IssueDetail {
                            issue_id: issue_id.clone(),
                        },
                    );
                }),
        );
    }

    // Mark as done / Move to todo (web toggles done ↔ todo).
    {
        let is_done = issue.status == IssueStatus::Done;
        let (label, icon) = if is_done {
            ("Move to todo", ExpIcon::ListTodo)
        } else {
            ("Mark as done", ExpIcon::CircleCheck)
        };
        let issue_id = issue.id.clone();
        menu = menu.item(PopupMenuItem::new(label).icon(Icon::from(icon)).on_click(
            move |_, _, cx| {
                let mut input = api::issues::IssuesUpdateInput::new(issue_id.clone());
                input.status = Some(if is_done {
                    IssueStatus::Todo
                } else {
                    IssueStatus::Done
                });
                spawn_issue_update(cx, input);
            },
        ));
    }

    // Copy issue ID.
    {
        let identifier = issue.identifier.clone();
        menu = menu.item(
            PopupMenuItem::new("Copy issue ID")
                .icon(Icon::from(ExpIcon::Copy))
                .on_click(move |_, _, cx| {
                    cx.write_to_clipboard(ClipboardItem::new_string(identifier.clone()));
                }),
        );
    }

    // Unmark duplicate only (L27 removed the standalone "Mark as duplicate…"
    // entry — marking now happens by choosing the `duplicate` status, which the
    // Status submenu below intercepts into the picker). The banner + this
    // un-mark affordance stay.
    if issue.duplicate_of_id.is_some() {
        let issue_id = issue.id.clone();
        menu = menu.item(
            PopupMenuItem::new("Unmark duplicate")
                .icon(Icon::new(IconName::Undo2))
                .on_click(move |_, _, cx| {
                    // Server restores the prior status and clears the link.
                    set_duplicate_of(issue_id.clone(), None, cx);
                }),
        );
    }

    menu = menu.separator();

    // Status submenu (option icons + right-side check). The trigger mirrors
    // the row's status icon: the CURRENT status, not a generic glyph (EXP-59).
    {
        let issue_id = issue.id.clone();
        let current = issue.status;
        let icon = option_icon(get_issue_status_config(current), cx);
        menu = menu.submenu_with_icon(Some(icon), "Status", window, cx, move |menu, _, cx| {
            let mut menu = menu.check_side(Side::Right);
            for option in &ISSUE_STATUS_OPTIONS {
                menu = menu.item(option_item(option, option.value == current, cx, {
                    let issue_id = issue_id.clone();
                    let value = option.value;
                    // L27: `duplicate` opens the picker; every other status writes.
                    move |window, cx| apply_status_selection(issue_id.clone(), value, window, cx)
                }));
            }
            menu
        });
    }

    // Assignee submenu (current member first, agents hidden). The trigger
    // reflects the current assignment (EXP-59) — the member glyph when
    // assigned, the unassigned placeholder otherwise (submenu triggers take
    // an `Icon`, so the web's avatar degrades to the assignee-row glyph).
    {
        let issue_id = issue.id.clone();
        let project_id = issue.project_id.clone();
        let current = issue.assignee_id.clone();
        let icon = if current.is_some() {
            Icon::new(IconName::CircleUser)
        } else {
            Icon::new(IconName::User)
        };
        menu = menu.submenu_with_icon(Some(icon), "Assignee", window, cx, move |menu, _, cx| {
            assignee_menu(menu, &issue_id, &project_id, current.as_deref(), cx)
        });
    }

    // Priority submenu. The trigger mirrors the row's priority icon: the
    // CURRENT priority, not a generic glyph (EXP-59).
    {
        let issue_id = issue.id.clone();
        let current = issue.priority;
        let icon = option_icon(get_issue_priority_config(current), cx);
        menu = menu.submenu_with_icon(Some(icon), "Priority", window, cx, move |menu, _, cx| {
            let mut menu = menu.check_side(Side::Right);
            for option in &ISSUE_PRIORITY_OPTIONS {
                menu = menu.item(option_item(option, option.value == current, cx, {
                    let issue_id = issue_id.clone();
                    let value = option.value;
                    move |_window, cx| {
                        let mut input = api::issues::IssuesUpdateInput::new(issue_id.clone());
                        input.priority = Some(value);
                        spawn_issue_update(cx, input);
                    }
                }));
            }
            menu
        });
    }

    // Labels submenu (colored dot + name + check, toggle membership).
    {
        let issue_id = issue.id.clone();
        let project_id = issue.project_id.clone();
        let icon = Icon::from(ExpIcon::Tag);
        menu = menu.submenu_with_icon(Some(icon), "Labels", window, cx, move |menu, _, cx| {
            let mut menu = menu.check_side(Side::Right);
            let collections = Store::global(cx).collections();
            let Some(project) = collections.projects.read(cx).get(&project_id).cloned() else {
                return menu;
            };
            let mut labels: Vec<Label> = collections
                .labels
                .read(cx)
                .iter()
                .filter(|label| label.workspace_id == project.workspace_id)
                .cloned()
                .collect();
            labels.sort_by(|a, b| {
                a.sort_order
                    .unwrap_or(f64::MAX)
                    .total_cmp(&b.sort_order.unwrap_or(f64::MAX))
                    .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
            });
            if labels.is_empty() {
                return menu.item(PopupMenuItem::label("No labels in this team"));
            }
            let selected: HashSet<String> = collections
                .issue_labels
                .read(cx)
                .iter()
                .filter(|link| link.issue_id == issue_id)
                .map(|link| link.label_id.clone())
                .collect();
            for label in labels {
                let checked = selected.contains(&label.id);
                let dot = label
                    .color
                    .as_deref()
                    .and_then(parse_hex_color)
                    .unwrap_or(gpui::opaque_grey(0.5, 1.0));
                let name = SharedString::from(label.name.clone());
                let issue_id = issue_id.clone();
                let label_id = label.id.clone();
                menu = menu.item(
                    PopupMenuItem::element(move |_, cx| {
                        h_flex()
                            .gap_2()
                            .items_center()
                            .child(div().size_2().rounded_full().flex_shrink_0().bg(dot))
                            .child(
                                div()
                                    .text_color(cx.theme().popover_foreground)
                                    .child(name.clone()),
                            )
                    })
                    .checked(checked)
                    .on_click(move |_, _, cx| {
                        toggle_label(cx, issue_id.clone(), label_id.clone(), checked);
                    }),
                );
            }
            menu
        });
    }

    // Move to project submenu (EXP-57, web `ProjectSubmenu`): the workspace's
    // projects with the current one disabled; hidden unless another project
    // exists. The server renumbers the issue in the target project
    // (EXP-42 → ABC-17); the row re-homes on the Electric echo.
    if !move_target_projects(cx, &issue.project_id).is_empty() {
        let issue_id = issue.id.clone();
        let project_id = issue.project_id.clone();
        menu = menu.submenu_with_icon(
            Some(Icon::from(ExpIcon::SquareKanban)),
            "Move to project",
            window,
            cx,
            move |menu, _, cx| move_to_project_menu(menu, &issue_id, &project_id, cx),
        );
    }

    // Set due date submenu (web `due-date-presets.tsx`: Tomorrow / End of this
    // week / In one week, plus Clear when set).
    {
        let issue_id = issue.id.clone();
        let due = issue.due_date.clone();
        let icon = Icon::from(ExpIcon::CalendarDays);
        menu = menu.submenu_with_icon(Some(icon), "Set due date", window, cx, move |menu, _, _| {
            let mut menu = menu.check_side(Side::Right);
            let today = chrono::Local::now().date_naive();
            for (label, date) in due_date_presets(today) {
                let formatted = date.format("%Y-%m-%d").to_string();
                let checked = due.as_deref() == Some(formatted.as_str());
                let issue_id = issue_id.clone();
                menu = menu.item(
                    PopupMenuItem::new(label)
                        .icon(Icon::from(ExpIcon::CalendarDays))
                        .checked(checked)
                        .on_click(move |_, _, cx| {
                            let mut input =
                                api::issues::IssuesUpdateInput::new(issue_id.clone());
                            input.due_date = api::Patch::Set(formatted.clone());
                            spawn_issue_update(cx, input);
                        }),
                );
            }
            if due.is_some() {
                let issue_id = issue_id.clone();
                menu = menu.separator().item(
                    PopupMenuItem::new("Clear due date")
                        .icon(Icon::new(IconName::Close))
                        .on_click(move |_, _, cx| {
                            let mut input =
                                api::issues::IssuesUpdateInput::new(issue_id.clone());
                            input.due_date = api::Patch::Null;
                            spawn_issue_update(cx, input);
                        }),
                );
            }
            menu
        });
    }

    // Delete issue → nested confirm (web's destructive submenu).
    {
        let issue_id = issue.id.clone();
        menu = menu.separator().submenu_with_icon(
            Some(Icon::new(IconName::Delete)),
            "Delete issue",
            window,
            cx,
            move |menu, _, _| {
                let issue_id = issue_id.clone();
                menu.item(
                    PopupMenuItem::new("Confirm delete")
                        .icon(Icon::new(IconName::Delete))
                        .on_click(move |_, _, cx| {
                            spawn_issue_delete(cx, issue_id.clone());
                        }),
                )
            },
        );
    }

    menu
}

/// Web `getDueDatePresets` (`lib/issue-due-date.ts`): Tomorrow, end of the
/// work week (next Friday, today included), in one week.
fn due_date_presets(today: chrono::NaiveDate) -> [(&'static str, chrono::NaiveDate); 3] {
    use chrono::Datelike as _;
    // web: (5 - day + 7) % 7 with Sunday=0 — "this week's Friday", today if
    // Friday.
    let days_until_friday = (5 + 7 - today.weekday().num_days_from_sunday() as i64) % 7;
    [
        ("Tomorrow", today + chrono::Duration::days(1)),
        (
            "End of this week",
            today + chrono::Duration::days(days_until_friday),
        ),
        ("In one week", today + chrono::Duration::days(7)),
    ]
}

/// The move-to-project submenu's target list (EXP-57): the issue's
/// workspace's projects in the shared sidebar order — empty (submenu hidden,
/// web `projects.length > 1` gate) unless a move target exists.
pub(crate) fn move_target_projects(cx: &App, project_id: &str) -> Vec<Project> {
    let collections = Store::global(cx).collections();
    let Some(workspace_id) = collections
        .projects
        .read(cx)
        .get(project_id)
        .map(|project| project.workspace_id.clone())
    else {
        return Vec::new();
    };
    let projects = collections.projects_in_workspace(&workspace_id, cx);
    if projects.len() < 2 {
        return Vec::new();
    }
    projects
}

/// The shared move-to-project menu body (row context submenu + the detail's
/// actions menu, EXP-57): colored dot + name per project (the Labels submenu
/// row pattern), current project checked + disabled; picking another fires
/// `issues.move`.
pub(crate) fn move_to_project_menu(
    menu: PopupMenu,
    issue_id: &str,
    project_id: &str,
    cx: &App,
) -> PopupMenu {
    let mut menu = menu.check_side(Side::Right);
    for project in move_target_projects(cx, project_id) {
        let is_current = project.id == project_id;
        let dot = project
            .color
            .as_deref()
            .and_then(parse_hex_color)
            .unwrap_or(gpui::opaque_grey(0.5, 1.0));
        let name = SharedString::from(project.name.clone());
        let issue_id = issue_id.to_string();
        let target_id = project.id.clone();
        menu = menu.item(
            PopupMenuItem::element(move |_, cx| {
                h_flex()
                    .gap_2()
                    .items_center()
                    .child(div().size_2().rounded_full().flex_shrink_0().bg(dot))
                    .child(
                        div()
                            .text_color(cx.theme().popover_foreground)
                            .child(name.clone()),
                    )
            })
            .checked(is_current)
            .disabled(is_current)
            .on_click(move |_, _, cx| {
                spawn_issue_move(cx, issue_id.clone(), target_id.clone());
            }),
        );
    }
    menu
}

/// §4.1 un-gated `issues.move` on a background thread (EXP-57): the issue
/// re-homes (and renumbers, EXP-42 → ABC-17) on the Electric echo.
pub(crate) fn spawn_issue_move(cx: &mut App, issue_id: String, project_id: String) {
    let Some(trpc) = queries::trpc_client(cx) else {
        log::warn!("[ui] issues.move skipped: no signed-in account");
        return;
    };
    cx.background_executor()
        .spawn(async move {
            if let Err(err) = api::issues::issues_move(&trpc, &issue_id, &project_id) {
                log::warn!("[ui] issues.move({issue_id} -> {project_id}) failed: {err}");
            }
        })
        .detach();
}

/// §4.1 un-gated `issues.delete` on a background thread (the row vanishes on
/// the Electric echo, web parity). `pub(crate)` — the detail's actions menu
/// (EXP-59) shares this with the row context menu's confirm submenu.
pub(crate) fn spawn_issue_delete(cx: &mut App, issue_id: String) {
    let Some(trpc) = queries::trpc_client(cx) else {
        log::warn!("[ui] issues.delete skipped: no signed-in account");
        return;
    };
    cx.background_executor()
        .spawn(async move {
            if let Err(err) = api::issues::issues_delete(&trpc, &issue_id) {
                log::warn!("[ui] issues.delete({issue_id}) failed: {err}");
            }
        })
        .detach();
}

/// One option row: `domain`-table icon (colored) + label + check when
/// current (never an iconless native menu).
fn option_item<V: Copy + 'static>(
    option: &'static IssueOption<V>,
    checked: bool,
    cx: &App,
    on_select: impl Fn(&mut Window, &mut App) + 'static,
) -> PopupMenuItem {
    PopupMenuItem::new(SharedString::from(option.label))
        .icon(option_icon(option, cx))
        .checked(checked)
        .on_click(move |_, window, cx| on_select(window, cx))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn due_date_presets_mirror_web_issue_due_date() {
        // Web getDueDatePresets: tomorrow, this week's Friday (today if
        // Friday), in one week.
        let wednesday = chrono::NaiveDate::from_ymd_opt(2026, 7, 1).unwrap();
        let [tomorrow, end_of_week, one_week] = due_date_presets(wednesday);
        assert_eq!(tomorrow.0, "Tomorrow");
        assert_eq!(tomorrow.1, chrono::NaiveDate::from_ymd_opt(2026, 7, 2).unwrap());
        assert_eq!(end_of_week.1, chrono::NaiveDate::from_ymd_opt(2026, 7, 3).unwrap()); // Friday
        assert_eq!(one_week.1, chrono::NaiveDate::from_ymd_opt(2026, 7, 8).unwrap());

        // On a Friday the end-of-week preset is today (web (5-day+7)%7 == 0).
        let friday = chrono::NaiveDate::from_ymd_opt(2026, 7, 3).unwrap();
        let [_, end_of_week, _] = due_date_presets(friday);
        assert_eq!(end_of_week.1, friday);

        // On a Saturday it wraps to NEXT Friday (web modulo behavior).
        let saturday = chrono::NaiveDate::from_ymd_opt(2026, 7, 4).unwrap();
        let [_, end_of_week, _] = due_date_presets(saturday);
        assert_eq!(end_of_week.1, chrono::NaiveDate::from_ymd_opt(2026, 7, 10).unwrap());
    }
}
