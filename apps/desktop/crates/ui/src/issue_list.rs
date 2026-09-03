//! The virtualized issue list — the §4.2/§4.6 board core (web parity target:
//! `apps/web/src/components/issue-list.tsx` at compact density).
//!
//! Structure: the board query (`queries::board_board` / `my_issues`, §4.1)
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

use std::cell::Cell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use gpui::{
    canvas, div, px, relative, size, App, ClickEvent, ClipboardItem, ElementId, FocusHandle,
    FontWeight, InteractiveElement as _, IntoElement, KeyBinding, ParentElement, Pixels, Render,
    ScrollHandle, SharedString, Size, StatefulInteractiveElement as _, Styled, WeakEntity, Window,
};
use gpui_component::{
    button::{Button, ButtonVariants as _},
    checkbox::Checkbox,
    h_flex,
    menu::{ContextMenuExt as _, DropdownMenu as _, PopupMenu, PopupMenuItem},
    scroll::ScrollableElement as _,
    skeleton::Skeleton,
    v_flex, v_virtual_list, ActiveTheme as _, Disableable as _, Icon, Side, Sizable as _,
    VirtualListScrollHandle,
};
use sync::Store;
use theme::tokens as t;

use domain::board::format_short_date;
use domain::options::{get_issue_priority_config, ColorToken, ISSUE_PRIORITY_OPTIONS};
use domain::rows::{Issue, Label, Board, User};
use domain::statuses::{ResolvedStatus, StatusTint};
use domain::{IssueFilters, IssueStatus};

use crate::controls::WebControl as _;
use crate::icons::{option_icon, registry, resolved_status_icon, ExpIcon};
use crate::issue_detail::{apply_status_selection, set_duplicate_of};
use crate::pickers::{option_item, status_menu, StatusMenuScope, StatusPick};
use crate::navigation::{nav_for_window, navigate, resolved_screen, Navigation, Screen};
use crate::issue_header::toggle_label;
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
/// EXP-439: below this measured list width the label chips collapse to bare
/// color dots. Full chips at panel width squeezed whatever the row could
/// shrink, and because flex pressure is PER ROW, only labeled rows gave way —
/// the status column walked out of line. The compact flag is global (one
/// canvas measure per frame), so every row renders the same grid.
const COMPACT_LIST_WIDTH: f32 = 440.;

/// EXP-698 round 5: at or above this measured panel width the bulk bar's
/// buttons carry their text labels; below it they collapse to icon-only with
/// their tooltips. The bar itself never wraps (`surface::glass_bar`) — a
/// second line would move the list rows under it — so the threshold is the
/// width the whole LABELED control row needs on ONE line:
///
/// ```text
///   20  bar padding (px_2p5 ×2)
/// + 32  the ✕ (web_icon_sm)
/// + 16  the count
/// + 82  Status    ┐ ghost web_sm, icon + label
/// + 88  Priority  │
/// + 98  Assignee  │
/// + 78  Labels    ┘
/// +128  Start coding (Md primary pill, icon + label)
/// +  9  the separator (1px + mx_1)
/// + 84  Delete
/// + 32  8px gaps ×4
/// + 32  the filter row's own px_4
/// + 90  the Filter trigger beside the bar
/// = 789 → 780, the nearest round number below it
/// ```
///
/// It is measured against the LIST's probe, not the bar's own box: the bar
/// and the filter row share the panel's width, and only the panel width is
/// known before the bar is built. gpui has no cheap "lay this out and tell me
/// how wide it came out" pass, hence the sum above rather than a measurement.
const BULK_BAR_LABEL_MIN_W: f32 = 780.;

gpui::actions!(
    issue_list,
    [
        /// Bulk select (cmd-a/ctrl-a): select every VISIBLE issue row —
        /// filtered, non-collapsed (Linear semantics, web parity).
        SelectAll,
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
    cx.bind_keys([KeyBinding::new("cmd-a", SelectAll, Some(BINDING_CONTEXT))]);
    #[cfg(not(target_os = "macos"))]
    cx.bind_keys([KeyBinding::new("ctrl-a", SelectAll, Some(BINDING_CONTEXT))]);
    cx.bind_keys([KeyBinding::new("escape", ClearIssueSelection, Some(BINDING_CONTEXT))]);
}

/// What this list shows (set by the screens panel on navigation).
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub enum IssueQuery {
    /// Nothing selected yet (renders the syncing skeleton).
    #[default]
    None,
    /// One board's view (`use-board-view-data`).
    Board { board_id: String },
    /// My Issues (`use-my-issues-data`): assignee == me in the team.
    MyIssues {
        team_id: String,
        user_id: String,
    },
}

/// One flattened virtual-list row. The issue payload sits behind a pointer so
/// the enum stays small (clippy `large_enum_variant` — `Issue` is ~520 bytes
/// vs the header's ~10); it is an `Rc` into the memoized [`BoardData`], so
/// rebuilding the row vector each frame clones handles, never row payloads
/// (REV-39 — an `Issue` carries the full markdown description).
enum ListRow {
    Header {
        status: Box<ResolvedStatus>,
        count: usize,
        collapsed: bool,
    },
    Issue {
        issue: Rc<Issue>,
        labels: Rc<Vec<Label>>,
    },
}

/// Every input the memoized [`BoardData`] derives from (REV-39). Revisions
/// alone are not enough: an empty up-to-date batch flips a collection's
/// readiness phase WITHOUT bumping its revision, so the combined `ready` bit
/// rides along; `today` covers the local-midnight overdue boundary. Query and
/// filter changes invalidate eagerly in [`IssueListView::set_query`] /
/// [`IssueListView::set_filters`].
#[derive(PartialEq, Eq)]
struct BoardDataKey {
    issues: u64,
    issue_labels: u64,
    labels: u64,
    boards: u64,
    issue_statuses: u64,
    ready: bool,
    today: String,
}

fn board_data_key(cx: &App) -> BoardDataKey {
    let collections = Store::global(cx).collections();
    BoardDataKey {
        issues: collections.issues.read(cx).revision(),
        issue_labels: collections.issue_labels.read(cx).revision(),
        labels: collections.labels.read(cx).revision(),
        boards: collections.boards.read(cx).revision(),
        issue_statuses: collections.issue_statuses.read(cx).revision(),
        ready: collections.issues.read(cx).is_ready()
            && collections.boards.read(cx).is_ready()
            && collections.issue_labels.read(cx).is_ready()
            && collections.labels.read(cx).is_ready(),
        today: queries::today_local(),
    }
}

pub struct IssueListView {
    query: IssueQuery,
    filters: IssueFilters,
    /// Collapsed status groups, keyed by resolved group key (web
    /// `collapsedGroups`).
    collapsed: HashSet<String>,
    /// Bulk-selected issue ids (web `selectedIds`). Pruned in `render`
    /// against the current data set; collapsing a group hides rows but keeps
    /// them selected (web parity).
    selected: HashSet<String>,
    /// The shift-range anchor (web `anchorId`) — the last toggled row.
    select_anchor: Option<String>,
    /// One bulk mutation in flight at a time (the bar's buttons disable).
    bulk_busy: bool,
    /// DEV-ONLY one-shot latch for [`Self::apply_dev_selection`].
    dev_selection_applied: bool,
    /// Whether the current scope's team has a single member — computed once
    /// per frame in `render` and read by the rows to drop the assignee cell
    /// (a solo team can only self-assign, so the affordance is noise).
    solo_team: bool,
    /// The list's content width as of the last prepaint (the mention-input
    /// canvas-measure pattern) — `render` classifies [`Self::compact`] from
    /// it, the canvas notifies only when the classification flips.
    measured_width: Rc<Cell<Pixels>>,
    /// EXP-439: measured width below [`COMPACT_LIST_WIDTH`] — rows collapse
    /// their label chips to bare color dots. Global per frame, so every row
    /// keeps the same grid.
    compact: bool,
    /// EXP-698: measured width at or above [`BULK_BAR_LABEL_MIN_W`] — the
    /// bulk bar shows its button labels. Kept as a field only so the width
    /// probe can notify when the classification flips.
    wide: bool,
    /// Focus target of the [`KEY_CONTEXT`] bindings (terminal-dock pattern).
    focus_handle: FocusHandle,
    /// This window's navigation — the rows highlight the issue whose detail
    /// is open (EXP-426, the inbox rows' treatment).
    nav: gpui::Entity<Navigation>,
    /// The open detail's issue id for the CURRENT render (from `nav`).
    active_issue_id: Option<String>,
    /// Rows of the CURRENT render — rebuilt in `render`, read by the
    /// virtual-list range closure afterwards.
    rows: Rc<Vec<ListRow>>,
    /// EXP-314: the scope team's resolved status vocabulary for the CURRENT
    /// render — the row dropdowns and the context menu read it instead of
    /// re-querying the collections once per row.
    team_statuses: Rc<Vec<ResolvedStatus>>,
    /// REV-39: the memoized board query. `render` re-runs on every
    /// `cx.notify` (selection toggles, group collapses, nav highlights,
    /// width flips, every Electric batch), and rebuilding the full
    /// clone+filter+group+sort pipeline each frame froze large boards —
    /// recompute only when [`BoardDataKey`] says an input actually changed.
    /// Shared with [`Self::bulk_bar`], which used to run one extra board
    /// query per frame while a selection was alive.
    data: Option<(BoardDataKey, Rc<BoardData>)>,
    scroll_handle: VirtualListScrollHandle,
    /// EXP-698 round 5: the empty-board branch scrolls (empty state +
    /// Getting-started cards), so it needs a handle of its own — the virtual
    /// list's is not rendered on that path.
    empty_scroll: ScrollHandle,
    _subscriptions: Vec<gpui::Subscription>,
}

impl IssueListView {
    pub fn new(window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        // §4.1 reactivity: observe exactly the collections the board query
        // reads; an Electric echo re-renders the list automatically.
        let collections = Store::global(cx).collections().clone();
        let nav = nav_for_window(window, cx);
        let getting_started = crate::getting_started::GettingStartedProgress::global(cx);
        let subscriptions = vec![
            cx.observe(&collections.issues, |_, _, cx| cx.notify()),
            cx.observe(&collections.issue_labels, |_, _, cx| cx.notify()),
            cx.observe(&collections.labels, |_, _, cx| cx.notify()),
            cx.observe(&collections.boards, |_, _, cx| cx.notify()),
            // EXP-314: the group vocabulary itself is synced data now.
            cx.observe(&collections.issue_statuses, |_, _, cx| cx.notify()),
            // EXP-426: the active-row highlight follows navigation.
            cx.observe(&nav, |_, _, cx| cx.notify()),
            // EXP-698 round 5: the empty-board branch renders the
            // Getting-started cards, whose signals include tRPC one-shots
            // (GitHub, widgets, MCP) no collection observer covers — without
            // this the block stays stuck on its loading state until some
            // unrelated echo happens to re-render the list.
            cx.observe(&getting_started, |_, _, cx| cx.notify()),
        ];

        Self {
            nav,
            active_issue_id: None,
            query: IssueQuery::None,
            filters: IssueFilters::empty(),
            collapsed: HashSet::new(),
            selected: HashSet::new(),
            select_anchor: None,
            bulk_busy: false,
            dev_selection_applied: false,
            solo_team: false,
            measured_width: Rc::new(Cell::new(px(0.))),
            compact: false,
            wide: false,
            focus_handle: cx.focus_handle(),
            rows: Rc::new(Vec::new()),
            team_statuses: Rc::new(Vec::new()),
            data: None,
            scroll_handle: VirtualListScrollHandle::new(),
            empty_scroll: ScrollHandle::new(),
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
        self.data = None;
        cx.notify();
    }

    /// Replace the active filters (driven by the §4.2 `BoardView` — tabs,
    /// filter popover and pills all funnel through it).
    pub fn set_filters(&mut self, filters: IssueFilters, cx: &mut gpui::Context<Self>) {
        if self.filters == filters {
            return;
        }
        self.filters = filters;
        self.data = None;
        cx.notify();
    }

    /// The board query behind [`Self::data`] — a cache hit is a handle clone,
    /// a miss reruns the full pipeline and re-keys the cache (REV-39).
    fn board_data(&mut self, cx: &App) -> Option<Rc<BoardData>> {
        if self.query == IssueQuery::None {
            return None;
        }
        let key = board_data_key(cx);
        if let Some((cached_key, cached)) = &self.data {
            if *cached_key == key {
                return Some(cached.clone());
            }
        }
        let data = Rc::new(match &self.query {
            IssueQuery::None => return None,
            IssueQuery::Board { board_id } => queries::board_board(cx, board_id, &self.filters),
            IssueQuery::MyIssues {
                team_id,
                user_id,
            } => queries::my_issues(cx, team_id, user_id, &self.filters),
        });
        self.data = Some((key, data.clone()));
        Some(data)
    }

    fn toggle_group(&mut self, group_key: String, cx: &mut gpui::Context<Self>) {
        if !self.collapsed.remove(&group_key) {
            self.collapsed.insert(group_key);
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
    /// DEV-ONLY (§11.4 headless verification, the EXP_DEV_* family):
    /// `EXP_DEV_SELECT=APP-11,APP-13,APP-10` pre-selects those issues so a
    /// capture run lands on the bulk-action bar without synthetic input.
    /// Runs from `render` (not `set_query`, which CLEARS the selection) and
    /// only once every named identifier has synced — a partial match would
    /// photograph a half-built selection. Unset in normal runs. Never
    /// document for users.
    fn apply_dev_selection(&mut self, cx: &mut gpui::Context<Self>) {
        if self.dev_selection_applied {
            return;
        }
        let Ok(spec) = std::env::var("EXP_DEV_SELECT") else {
            self.dev_selection_applied = true;
            return;
        };
        let wanted: Vec<&str> = spec
            .split(',')
            .map(str::trim)
            .filter(|id| !id.is_empty())
            .collect();
        if wanted.is_empty() {
            self.dev_selection_applied = true;
            return;
        }
        let mut ids = Vec::new();
        for identifier in &wanted {
            let found = self.rows.iter().find_map(|row| match row {
                ListRow::Issue { issue, .. } if issue.identifier == *identifier => {
                    Some(issue.id.clone())
                }
                _ => None,
            });
            // Not every row has synced yet — try again on the next render.
            let Some(id) = found else { return };
            ids.push(id);
        }
        self.dev_selection_applied = true;
        self.select_anchor = ids.last().cloned();
        self.selected = ids.into_iter().collect();
        cx.notify();
    }

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

    /// The team behind the current query — scopes the bulk bar's
    /// assignee/label pickers. `None` (join not synced yet) hides the bar.
    fn bulk_team_id(&self, cx: &App) -> Option<String> {
        let collections = Store::global(cx).collections();
        match &self.query {
            IssueQuery::None => None,
            IssueQuery::Board { board_id } => collections
                .boards
                .read(cx)
                .get(board_id)
                .map(|board| board.team_id.clone()),
            IssueQuery::MyIssues { team_id, .. } => Some(team_id.clone()),
        }
    }

    /// Whether the current scope's team has exactly one member — a solo team
    /// where the assignee affordance is noise (only self-assign is possible).
    /// Exactly one, not `<= 1`: a count of 0 means team_members has not
    /// synced yet, and treating that as solo made the affordance vanish and
    /// reappear on a genuine multi-member team. Same contract as
    /// issue_header's solo_team and the web clients.
    fn is_solo_team(&self, cx: &App) -> bool {
        let Some(team_id) = self.bulk_team_id(cx) else {
            return false;
        };
        Store::global(cx)
            .collections()
            .team_members
            .read(cx)
            .iter()
            .filter(|member| member.team_id == team_id)
            .count()
            == 1
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
                .render_group_header(status, *count, *collapsed, cx)
                .into_any_element(),
            ListRow::Issue { issue, labels } => {
                self.render_issue_row(issue, labels, cx).into_any_element()
            }
        }) else {
            return div().into_any_element();
        };
        row
    }

    /// Web group header: chevron trigger + status icon + label + count over a
    /// wash in the status' own hue (EXP-293 — restores the web
    /// `statusHeaderBg` tint that EXP-282/EXP-285 flattened away; see
    /// [`status_header_tint`] for why it came back).
    fn render_group_header(
        &self,
        status: &ResolvedStatus,
        count: usize,
        collapsed: bool,
        cx: &mut gpui::Context<Self>,
    ) -> impl IntoElement {
        let chevron = if collapsed {
            registry::UI_CHEVRON_RIGHT
        } else {
            registry::UI_CHEVRON_DOWN
        };
        let group_key = status.group_key.clone();

        h_flex()
            .h(px(HEADER_HEIGHT))
            .w_full()
            .px_3()
            .gap_1p5()
            .items_center()
            // EXP-293: a wash in the status' hue so the header reads as a group
            // divider and not as one more issue row (the hairline alone was too
            // little). Web parity — `statusHeaderBg` in issue-list.tsx.
            .bg(status_header_tint(&status.tint))
            .border_b_1()
            .border_color(cx.theme().border.opacity(0.5))
            .child(
                // EXP-698 round 5: a BARE chevron — a group header is
                // structure, not an action, so the disclosure carries no
                // glass circle on any client (web renders a ghost button,
                // the steer viewer's section headers the same bare glyph).
                // The 24px box is the hit target.
                div()
                    .id(header_id("collapse", &status.group_key))
                    .cursor_pointer()
                    .size(px(24.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(
                        Icon::new(chevron)
                            .xsmall()
                            .text_color(cx.theme().muted_foreground),
                    )
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.toggle_group(group_key.clone(), cx);
                    })),
            )
            .child(resolved_status_icon(status, cx).xsmall())
            .child(
                div()
                    .text_sm()
                    .font_weight(FontWeight::MEDIUM)
                    .child(SharedString::from(status.name.clone())),
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
        let menu_statuses = self.team_statuses.clone();
        let is_selected = self.selected.contains(&issue.id);
        // The open detail's row keeps the same active fill as a bulk-selected
        // one — both mean "this row is where you are" (EXP-426).
        let is_active = self.active_issue_id.as_deref() == Some(issue.id.as_str());
        let any_selected = !self.selected.is_empty();
        let solo_team = self.solo_team;
        let compact = self.compact;

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
            // Bounded clip at the extreme-narrow floor — cells carry min
            // widths, so past their sum the row clips instead of overflowing.
            .overflow_hidden()
            .cursor_pointer()
            .border_b_1()
            .border_color(cx.theme().border.opacity(0.3))
            .when(is_selected || is_active, |style| {
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
            // 72px identifier — FIXED again (EXP-439): EXP-426 let it shrink
            // to 44px under narrow-panel pressure, but flex pressure is per
            // ROW, so only rows with trailing chips gave way and the status
            // column drifted out of line. Narrow panels reclaim the space by
            // collapsing the chips instead (the `compact` flag).
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
                    .child(status_dropdown(issue, &self.team_statuses, cx)),
            )
            // 1fr title (truncating; the 88px floor hands further shrink
            // pressure to the identifier column — EXP-426).
            .child(
                h_flex()
                    .flex_1()
                    .min_w(px(88.))
                    .ml_2()
                    .gap_1p5()
                    .items_center()
                    .child(
                        div()
                            .text_sm()
                            .whitespace_nowrap()
                            .overflow_hidden()
                            .text_ellipsis()
                            .child(SharedString::from(issue.title.clone())),
                    ),
            )
            // auto labels — the web rounded-full chips, collapsed to bare
            // color dots when the panel is narrow (EXP-439: full chips ate
            // the title and forced per-row shrinking).
            .child(
                h_flex()
                    .ml_2()
                    .gap_1p5()
                    .flex_shrink_0()
                    .when(compact, |cell| cell.gap_1())
                    .children(labels.iter().map(|label| {
                        if compact {
                            label_dot(label, cx).into_any_element()
                        } else {
                            label_chip(label, cx).into_any_element()
                        }
                    })),
            )
            // 28px assignee dropdown cell (web `AssigneeDropdown` — avatar or
            // dashed placeholder circle). Dropped entirely on a solo team.
            .when(!solo_team, |row| {
                row.child(
                    control_cell(row_id("assignee-cell", &issue.id))
                        .ml_3()
                        .child(assignee_dropdown(issue, cx)),
                )
            })
            // auto due date: CalendarDays + short date, only when set — web
            // parity; presets edit via the context menu's "Set due date"
            // submenu, mirroring `due-date-presets.tsx`.
            .child(due_cell(issue, cx))
            // Right-click context menu (web `IssueRowContextMenu`, §4.2/§4.6).
            .context_menu(move |menu, window, cx| {
                build_row_context_menu(menu, &menu_issue, &menu_statuses, window, cx)
            })
    }

    // -- bulk action bar -------------------------------------------------------

    /// The bulk action bar for the current selection, or `None` when nothing
    /// is selected (or the team hasn't synced yet — the bar waits for it, the
    /// selection itself does not).
    ///
    /// EXP-289: the bar is NOT rendered by this view anymore. Entering
    /// multiselect used to prepend an in-flow row, which pushed the whole list
    /// down by its height — the list "jumped". [`crate::board::BoardView`]
    /// now hands the element this returns to the filter bar, which swaps its
    /// own fixed-height control row for it (EXP-426 made the swap in-flow —
    /// the rows still never move). `BoardView` observes this entity, so a
    /// selection change re-renders it; the ids are recomputed here off the
    /// CURRENT query data (never a snapshot taken during this view's own
    /// render, which runs AFTER its parent's and would lag a frame), in
    /// visible list order and pruned to rows that still exist — the same
    /// projection `render` prunes `selected` with. Since REV-39 that read is
    /// the memoized [`Self::data`], so agreeing with the rows underneath no
    /// longer costs an extra board query per frame.
    pub(crate) fn bulk_bar(&mut self, cx: &mut gpui::Context<Self>) -> Option<gpui::AnyElement> {
        if self.selected.is_empty() {
            return None;
        }
        let ids: Vec<String> = self
            .board_data(cx)?
            .groups
            .iter()
            .flat_map(|group| group.issues.iter())
            .filter(|issue| self.selected.contains(&issue.id))
            .map(|issue| issue.id.clone())
            .collect();
        if ids.is_empty() {
            return None;
        }
        let team_id = self.bulk_team_id(cx)?;
        Some(self.render_bulk_bar(team_id, ids, cx))
    }

    /// The bulk action bar's element (web `bulk-action-bar.tsx`): N selected ·
    /// clear · Status · Priority · Assignee · Labels · Start coding · Delete
    /// (nested confirm). Property edits keep the selection alive — only delete
    /// clears it (FIX F3); every mutation chunks at 200 ids (FIX F4) and lands
    /// via the Electric echo.
    ///
    /// EXP-698 round 5: the ONE bar shape of every client — an opaque
    /// [`crate::surface::glass_bar`] capsule with `icon + label` buttons, the
    /// web bar's labels included. The capsule is ONE fixed-height line and
    /// never wraps (a second line would move the list rows under it), so a
    /// panel narrower than [`BULK_BAR_LABEL_MIN_W`] — the default ~650px tool
    /// panel among them — collapses the buttons to icon-only and leans on the
    /// tooltips they all carry.
    fn render_bulk_bar(
        &self,
        team_id: String,
        ids: Vec<String>,
        cx: &mut gpui::Context<Self>,
    ) -> gpui::AnyElement {
        let count = ids.len();
        let busy = self.bulk_busy;
        let list = cx.entity().downgrade();
        let danger = cx.theme().danger;
        // Read straight off the probe, not off `self.wide`: the parent asks
        // for this element BEFORE this view's own render runs, so the field
        // would be one frame stale on the first selection.
        let labels = self.measured_width.get() >= px(BULK_BAR_LABEL_MIN_W);
        // `.label()` takes a value, so the collapse is a small helper rather
        // than a `when` chain on six buttons.
        let with_label = move |button: Button, label: &'static str| {
            if labels {
                button.label(label)
            } else {
                button
            }
        };

        let status_menu = {
            let ids = ids.clone();
            let list = list.clone();
            let team_id = team_id.clone();
            with_label(
                Button::new("bulk-status")
                    .ghost()
                    .web_sm()
                    .icon(Icon::from(ExpIcon::ListTodo)),
                "Status",
            )
                .tooltip("Status")
                .disabled(busy)
                .dropdown_menu(move |menu, _window, cx| {
                    // The team's own vocabulary (EXP-314), minus the
                    // duplicate-category rows (`StatusMenuScope::Assignable`):
                    // bulk marking has no canonical-issue picker, and
                    // status='duplicate' without duplicate_of_id breaks the
                    // pairing invariant (the single-issue path intercepts via
                    // apply_status_selection's picker).
                    let statuses = queries::team_status_options(cx, &team_id);
                    let ids = ids.clone();
                    let list = list.clone();
                    status_menu(
                        menu,
                        &statuses,
                        "",
                        StatusMenuScope::Assignable,
                        Rc::new(move |pick: StatusPick, _window, cx| {
                            let pick = pick.clone();
                            spawn_bulk_op(
                                list.clone(),
                                cx,
                                ids.clone(),
                                false,
                                "issues.bulkUpdate",
                                move |trpc, chunk| {
                                    let mut input =
                                        api::issues::IssuesBulkUpdateInput::new(chunk.to_vec());
                                    pick.apply_to_bulk(&mut input);
                                    api::issues::issues_bulk_update(trpc, &input).map(|_| ())
                                },
                            );
                        }),
                        cx,
                    )
                })
        };

        let priority_menu = {
            let ids = ids.clone();
            let list = list.clone();
            with_label(
                Button::new("bulk-priority")
                    .ghost()
                    .web_sm()
                    .icon(Icon::from(ExpIcon::SignalHigh)),
                "Priority",
            )
                .tooltip("Priority")
                .disabled(busy)
                .dropdown_menu(move |menu, _window, cx| {
                    let mut menu = menu.check_side(Side::Right);
                    for option in &ISSUE_PRIORITY_OPTIONS {
                        let ids = ids.clone();
                        let list = list.clone();
                        let value = option.value;
                        menu = menu.item(option_item(
                            SharedString::from(option.label),
                            option_icon(option, cx),
                            false,
                            move |_window, cx| {
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
                            },
                        ));
                    }
                    menu
                })
        };

        // Hidden on a solo team — one member can only self-assign, so the
        // affordance is noise. The fetched list feeds the menu directly (no
        // second query on open).
        let assignee_menu = {
            let ids = ids.clone();
            let list = list.clone();
            let users = queries::team_users(cx, &team_id);
            (users.len() > 1).then(|| {
                with_label(
                    Button::new("bulk-assignee")
                        .ghost()
                        .web_sm()
                        .icon(Icon::new(registry::UI_ASSIGNEE)),
                    "Assignee",
                )
                    .tooltip("Assignee")
                    .disabled(busy)
                    .dropdown_menu(move |menu, _window, _cx| {
                        let mut menu = menu.scrollable(true).max_h(px(320.));
                        menu = menu.item(
                            PopupMenuItem::new("Unassign")
                                .icon(Icon::new(registry::UI_CLOSE))
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
                        for user in &users {
                            let name = crate::comments::author_label(Some(user));
                            let ids = ids.clone();
                            let list = list.clone();
                            let user_id = user.id.clone();
                            menu = menu.item(
                                PopupMenuItem::new(SharedString::from(name))
                                    .icon(Icon::new(registry::UI_ASSIGNEE))
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
            })
        };

        let labels_menu = {
            let ids = ids.clone();
            let list = list.clone();
            let team_id = team_id.clone();
            with_label(
                Button::new("bulk-labels")
                    .ghost()
                    .web_sm()
                    .icon(Icon::from(ExpIcon::Tag)),
                "Labels",
            )
                .tooltip("Labels")
                .disabled(busy)
                .dropdown_menu(move |menu, _window, cx| {
                    let mut menu = menu
                        .scrollable(true)
                        .max_h(px(320.))
                        .check_side(Side::Right);
                    let labels = queries::team_labels(cx, &team_id);
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
            let list = list.clone();
            let team_id = team_id.clone();
            // EXP-367: no agent CLI installed → disabled with the reason,
            // never hidden (same copy as every Start-coding affordance).
            let no_agent = crate::coding_flow::no_agent_reason(cx);
            // The ONE emphasised control of the bar (web `Start coding` is
            // the primary button there too).
            with_label(
                crate::surface::glass_pill_button_primary(
                    "bulk-start-coding",
                    // `Md` == the web `size="sm"` control box (32) the ghost
                    // buttons beside it wear; `Sm` would sit 8px shorter than
                    // its own row.
                    crate::surface::PillSize::Md,
                )
                .icon(Icon::new(registry::ACTION_RUN)),
                "Start coding",
            )
                .tooltip(no_agent.clone().unwrap_or_else(|| "Start coding".into()))
                .disabled(busy || no_agent.is_some())
                .on_click(move |_, window, cx| {
                    // EXP-439: a successful launch ends the multiselect — the
                    // session took the batch over, so the dialog's launch
                    // callback unselects everything.
                    let list = list.clone();
                    crate::start_coding_dialog::open_for_selection(
                        window,
                        cx,
                        team_id.clone(),
                        ids.clone(),
                        Some(Rc::new(move |cx: &mut App| {
                            let _ = list.update(cx, |this, cx| this.clear_selection(cx));
                        })),
                        None,
                    );
                })
        };

        let delete_menu = {
            let ids = ids.clone();
            let list = list.clone();
            with_label(
                Button::new("bulk-delete")
                    .ghost()
                    .web_sm()
                    .icon(Icon::new(registry::UI_DELETE).text_color(danger))
                    // Destructive: the whole control reads danger, web parity.
                    .text_color(danger),
                "Delete",
            )
                .tooltip("Delete selected")
                .disabled(busy)
                .dropdown_menu(move |menu, _window, _cx| {
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
                            .icon(Icon::new(registry::UI_DELETE))
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

        // EXP-426: an INLINE row — the filter bar swaps its Filter cluster
        // for this element while a selection exists (its row keeps a fixed
        // min-height, so the list rows still never move — the EXP-289
        // no-jump invariant, without the old floating-overlay masking).
        // EXP-439: content-sized (no `w_full`) so the Filter trigger keeps
        // its right-hand slot on the same row.
        // EXP-698 round 5: the cluster wears the OPAQUE bar capsule (EXP-642
        // had it on the translucent tray) so it reads as the one object
        // acting on the selection, left of the Filter trigger — the web
        // `BulkActionBar` card and the two mobile bars, same shape. It stays
        // `flex_shrink_0`: the capsule is one line by construction, and the
        // label gate above — not a squeeze — is what makes it fit.
        crate::surface::glass_bar(cx)
            .id("bulk-action-bar")
            .flex_shrink_0()
            .child(
                Button::new("bulk-clear")
                    .ghost()
                    .web_icon_sm()
                    .icon(Icon::new(registry::UI_CLOSE))
                    .tooltip("Clear selection")
                    .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                        this.clear_selection(cx);
                    })),
            )
            .child(
                // The bare COUNT, like web/iOS/Android — the ✕ beside it and
                // the actions after it already say what it counts.
                div()
                    .px_1()
                    .text_sm()
                    .font_weight(FontWeight::SEMIBOLD)
                    .whitespace_nowrap()
                    .child(SharedString::from(count.to_string())),
            )
            .child(status_menu)
            .child(priority_menu)
            .when_some(assignee_menu, |this, btn| this.child(btn))
            .child(labels_menu)
            .child(start_coding)
            // The hairline before the destructive action (web's separator).
            .child(
                div()
                    .w_px()
                    .h(px(20.))
                    .mx_1()
                    .flex_shrink_0()
                    .bg(theme::tokens::glass::STROKE_CARD.to_hsla()),
            )
            .child(delete_menu)
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
        // Solo teams drop the per-row assignee cell — resolved once per frame
        // (the whole list is one team) so no per-row membership query runs.
        self.solo_team = self.is_solo_team(cx);
        // EXP-426: the row whose detail is open highlights (inbox parity) —
        // resolved once per frame off this window's navigation.
        self.active_issue_id = match resolved_screen(&self.nav, cx) {
            Some(Screen::IssueDetail { issue_id }) => Some(issue_id),
            _ => None,
        };
        // EXP-314: same once-per-frame treatment for the status vocabulary —
        // the whole list is one team, and every row dropdown + the context
        // menu read this snapshot.
        self.team_statuses = Rc::new(
            self.bulk_team_id(cx)
                .map(|team_id| queries::team_status_options(cx, &team_id))
                .unwrap_or_else(domain::statuses::default_resolved_statuses),
        );

        // EXP-439: classify the chip treatment off the LAST frame's measured
        // width (0 until the first prepaint — default to full chips, web
        // parity); the canvas below notifies when the classification flips,
        // so a resize across the threshold re-renders exactly once.
        let measured = self.measured_width.get();
        self.compact = measured > px(0.) && measured < px(COMPACT_LIST_WIDTH);
        self.wide = measured >= px(BULK_BAR_LABEL_MIN_W);

        // Base surface: NONE — the list sits directly on the window's page
        // gradient (EXP-282; `colors.list` has been transparent since the
        // EXP-269 glass pass, so the old `.bg()` was a dead no-op). Key
        // context + tracked focus scope the select-all/clear bindings here
        // (terminal-dock pattern).
        let rendered_compact = self.compact;
        let rendered_wide = self.wide;
        let measured_width = self.measured_width.clone();
        let entity = cx.entity().downgrade();
        let base = v_flex()
            .size_full()
            .relative()
            .child(
                // Width probe (the mention-input canvas idiom): record the
                // list's width at prepaint; re-render only on a compact flip.
                // The notify is DEFERRED — this view is leased while its own
                // element tree prepaints, so an inline update would panic.
                canvas(
                    move |bounds, _, cx| {
                        measured_width.set(bounds.size.width);
                        let compact = bounds.size.width > px(0.)
                            && bounds.size.width < px(COMPACT_LIST_WIDTH);
                        // EXP-698: the bulk bar's label threshold rides the
                        // same probe — a resize across EITHER line re-renders
                        // exactly once.
                        let wide = bounds.size.width >= px(BULK_BAR_LABEL_MIN_W);
                        if compact != rendered_compact || wide != rendered_wide {
                            let entity = entity.clone();
                            cx.defer(move |cx| {
                                if let Some(list) = entity.upgrade() {
                                    list.update(cx, |_, cx| cx.notify());
                                }
                            });
                        }
                    },
                    |_, _, _, _| {},
                )
                .absolute()
                .size_full(),
            )
            .key_context(KEY_CONTEXT)
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(|this, _: &SelectAll, _, cx| {
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
                    .child(
                        v_flex().size_full().items_center().justify_center().child(
                            crate::controls::empty_state(
                                Icon::from(ExpIcon::SearchX),
                                "No issues match your filters",
                                "Try removing some filters to see more issues.",
                                cx,
                            ),
                        ),
                    )

                    .into_any_element();
            }
            // EXP-698 round 5: a genuinely empty board is where the
            // Getting-started checklist belongs — the same cards the page
            // renders, under the empty state, on every client. The column
            // scrolls: ten cards are taller than a short panel.
            let cards = self
                .bulk_team_id(cx)
                .and_then(|team_id| crate::getting_started::inline_cards(&team_id, cx));
            return base
                .child(crate::scroll_pane::v_scroll_pane(
                    "issue-list-empty-scroll",
                    &self.empty_scroll,
                    v_flex()
                        .w_full()
                        .min_w_0()
                        .px_4()
                        .pb_4()
                        .gap_4()
                        .child(
                            // Without the cards under it the empty state
                            // would hug the top of the scroll column; the
                            // 60% band keeps it optically centred either way.
                            v_flex()
                                .w_full()
                                .min_h(relative(0.6))
                                .items_center()
                                .justify_center()
                                .child(crate::controls::empty_state(
                                    Icon::from(ExpIcon::ListTodo),
                                    "No issues yet",
                                    "Create an issue to start tracking work.",
                                    cx,
                                )),
                        )
                        .children(cards),
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
            let collapsed = self.collapsed.contains(&group.status.group_key);
            rows.push(ListRow::Header {
                status: Box::new(group.status.clone()),
                count: group.issues.len(),
                collapsed,
            });
            if collapsed {
                continue;
            }
            for issue in &group.issues {
                let labels = data
                    .labels_by_issue
                    .get(issue.id.as_str())
                    .cloned()
                    .unwrap_or_default();
                rows.push(ListRow::Issue {
                    // Rc handles into the memoized data — no row payload
                    // copies per frame (REV-39).
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
        // DEV-ONLY: seed the bulk selection once the named rows are all
        // present (see [`Self::apply_dev_selection`]).
        self.apply_dev_selection(cx);

        // EXP-289: no bulk-action row here anymore — [`Self::bulk_bar`] hands
        // it to `BoardView`, which floats it over the title row. The list's
        // geometry is therefore identical selected or not: no jump.
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
        .ghost().cursor_pointer()
        .xsmall()
        .icon(option_icon(config, cx))
        .dropdown_menu(move |menu, _window, cx| {
            let mut menu = menu.check_side(Side::Right);
            for option in &ISSUE_PRIORITY_OPTIONS {
                let issue_id = issue_id.clone();
                let value = option.value;
                menu = menu.item(option_item(
                    SharedString::from(option.label),
                    option_icon(option, cx),
                    option.value == current,
                    move |_window, cx| {
                        let mut input = api::issues::IssuesUpdateInput::new(issue_id.clone());
                        input.priority = Some(value);
                        spawn_issue_update(cx, input);
                    },
                ));
            }
            menu
        })
}

/// Status dropdown (web `StatusDropdown`). EXP-314: the trigger renders the
/// issue's RESOLVED status and the menu lists the team's own vocabulary;
/// picking a duplicate-category status is intercepted into the duplicate
/// picker (L27), never a direct status write.
fn status_dropdown(issue: &Issue, statuses: &[ResolvedStatus], cx: &App) -> impl IntoElement {
    let resolved = resolve_in(issue, statuses);
    let current_key = resolved.group_key.clone();
    let statuses = statuses.to_vec();
    let issue_id = issue.id.clone();

    Button::new(row_id("status", &issue.id))
        .ghost().cursor_pointer()
        .xsmall()
        .icon(resolved_status_icon(&resolved, cx))
        .dropdown_menu(move |menu, _window, cx| {
            let issue_id = issue_id.clone();
            status_menu(
                menu,
                &statuses,
                &current_key,
                // Single-issue surface: the duplicate row IS offered, and
                // `apply_status_selection` intercepts it into the canonical
                // picker (L27).
                StatusMenuScope::SingleIssue,
                Rc::new(move |pick, window, cx| {
                    apply_status_selection(issue_id.clone(), pick, window, cx)
                }),
                cx,
            )
        })
}

/// The issue's status within a PRE-RESOLVED team vocabulary (the frame's
/// `team_statuses`): a straight group-key hit, else the resolution chain over
/// the collections. Keeps the row icon in step with its group header.
fn resolve_in(issue: &Issue, statuses: &[ResolvedStatus]) -> ResolvedStatus {
    if let Some(status_id) = issue.status_id.as_deref() {
        if let Some(found) = statuses
            .iter()
            .find(|status| status.row_id.as_deref() == Some(status_id))
        {
            return found.clone();
        }
    }
    // An unknown forward-compat anchor normalizes to backlog BEFORE the row
    // lookup (the cross-platform rule), so the row icon agrees with the group
    // the board built for it.
    let anchor = domain::statuses::normalized_anchor(issue.status);
    if let Some(wire) = anchor.as_wire() {
        if let Some(found) = statuses
            .iter()
            .find(|status| status.builtin_key.as_deref() == Some(wire))
        {
            return found.clone();
        }
    }
    domain::statuses::constructed_default(anchor)
}

/// Assignee dropdown (web `AssigneeDropdown`): avatar trigger when assigned,
/// dashed placeholder circle otherwise; menu = Unassign + the team's
/// human members (current assignee first — web `orderedUsers`).
fn assignee_dropdown(issue: &Issue, cx: &mut App) -> impl IntoElement {
    let assignee = issue
        .assignee_id
        .as_deref()
        .and_then(|id| Store::global(cx).collections().users.read(cx).get(id).cloned());

    let trigger = match issue.assignee_id.as_deref() {
        // Assigned — the shared avatar: the member's profile picture when it
        // has landed, hue-hashed initials from the name otherwise (or `Member
        // <LAST4>` when the co-member's user row didn't sync). EXP-547: the
        // row used to draw initials ONLY, so two members sharing initials
        // (Danny/Dennis S.) rendered the same "DS" glyph and the row looked
        // assigned to the wrong person — web's `AssigneeDropdown` shows the
        // picture, so does the picker menu below.
        Some(id) => Button::new(row_id("assignee", &issue.id))
            .ghost().cursor_pointer()
            .xsmall()
            .child(crate::user_avatar::user_avatar(
                id,
                &crate::comments::user_label(id, assignee.as_ref()),
                assignee.as_ref().and_then(|user| user.image.as_deref()),
                gpui_component::Size::XSmall,
                cx,
            )),
        None => Button::new(row_id("assignee", &issue.id)).ghost().cursor_pointer().xsmall().child(
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
                    Icon::new(registry::UI_AVATAR_PLACEHOLDER)
                        .size_2p5()
                        .text_color(cx.theme().muted_foreground.opacity(0.5)),
                ),
        ),
    };

    let issue_id = issue.id.clone();
    let board_id = issue.board_id.clone();
    let current = issue.assignee_id.clone();
    trigger.dropdown_menu(move |menu, _window, cx| {
        // Member lists grow with the team — cap + scroll (EXP-46a).
        // Scrollable ONLY on this top-level dropdown: the same body renders
        // as a context-menu SUBMENU below, where scrollable is unsupported
        // at the pinned gpui-component rev.
        let menu = menu.scrollable(true).max_h(px(320.));
        assignee_menu(menu, &issue_id, &board_id, current.as_deref(), cx)
    })
}

/// The shared assignee menu body (row dropdown + context-menu submenu).
fn assignee_menu(
    menu: PopupMenu,
    issue_id: &str,
    board_id: &str,
    current: Option<&str>,
    cx: &App,
) -> PopupMenu {
    let mut menu = menu.check_side(Side::Right);
    if current.is_some() {
        let issue_id = issue_id.to_string();
        menu = menu.item(
            PopupMenuItem::new("Unassign")
                .icon(Icon::new(registry::UI_CLOSE))
                .on_click(move |_, _, cx| {
                    let mut input = api::issues::IssuesUpdateInput::new(issue_id.clone());
                    input.assignee_id = api::Patch::Null;
                    spawn_issue_update(cx, input);
                }),
        );
    }
    for user in assignable_users(board_id, current, cx) {
        let checked = current == Some(user.id.as_str());
        let name = crate::comments::author_label(Some(&user));
        let issue_id = issue_id.to_string();
        let user_id = user.id.clone();
        // EXP-426: the real avatar replaced the generic person glyph — the
        // shared row shape.
        menu = menu.item(crate::pickers::user_menu_item(
            user.id.clone(),
            name,
            user.image.clone(),
            checked,
            move |_window, cx| {
                let mut input = api::issues::IssuesUpdateInput::new(issue_id.clone());
                input.assignee_id = api::Patch::Set(user_id.clone());
                spawn_issue_update(cx, input);
            },
        ));
    }
    menu
}

/// The team's human members eligible as assignees (board → team →
/// members ⨝ users, agents hidden — web `people`), current assignee first
/// (web `orderedUsers`), then name-sorted.
fn assignable_users(board_id: &str, current: Option<&str>, cx: &App) -> Vec<User> {
    let collections = Store::global(cx).collections();
    let Some(board) = collections.boards.read(cx).get(board_id).cloned() else {
        return Vec::new();
    };
    let member_ids: HashSet<String> = collections
        .team_members
        .read(cx)
        .iter()
        .filter(|member| member.team_id == board.team_id)
        .map(|member| member.user_id.clone())
        .collect();
    let mut users: Vec<User> = collections
        .users
        .read(cx)
        .iter()
        .filter(|user| member_ids.contains(&user.id))
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
/// done / Move to backlog, Copy issue ID, Mark as duplicate… / Unmark duplicate,
/// then Status / Assignee / Priority / Labels /
/// Move-to-board / Set-due-date submenus, then the Delete-issue confirm
/// submenu. Mutations are the §4.1 un-gated form.
fn build_row_context_menu(
    menu: PopupMenu,
    issue: &Issue,
    statuses: &Rc<Vec<ResolvedStatus>>,
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

    // Mark as done / Move to backlog (web toggles done ↔ backlog; EXP-685
    // retired Todo). EXP-314: this convenience toggle stays an ENUM write —
    // the server's trigger derives `status_id` from it, so it lands on the
    // team's BUILTIN Done/Backlog rows (an issue in a custom status leaves
    // it, by design).
    {
        let is_done = issue.status == IssueStatus::Done;
        let (label, icon) = if is_done {
            ("Move to backlog", ExpIcon::ListTodo)
        } else {
            ("Mark as done", ExpIcon::CircleCheck)
        };
        let issue_id = issue.id.clone();
        menu = menu.item(PopupMenuItem::new(label).icon(Icon::from(icon)).on_click(
            move |_, _, cx| {
                let mut input = api::issues::IssuesUpdateInput::new(issue_id.clone());
                input.status = Some(if is_done {
                    IssueStatus::Backlog
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
                .icon(Icon::new(registry::UI_UNDO))
                .on_click(move |_, _, cx| {
                    // Server restores the prior status and clears the link.
                    set_duplicate_of(issue_id.clone(), None, cx);
                }),
        );
    }

    // Status submenu (option icons + right-side check). The trigger mirrors
    // the row's status icon: the CURRENT status, not a generic glyph (EXP-59).
    {
        let issue_id = issue.id.clone();
        let resolved = resolve_in(issue, statuses);
        let current_key = resolved.group_key.clone();
        let statuses = statuses.clone();
        let icon = resolved_status_icon(&resolved, cx);
        menu = menu.submenu_with_icon(Some(icon), "Status", window, cx, move |menu, _, cx| {
            let issue_id = issue_id.clone();
            status_menu(
                menu,
                &statuses,
                &current_key,
                // L27: a duplicate-category pick opens the picker; every other
                // status writes.
                StatusMenuScope::SingleIssue,
                Rc::new(move |pick, window, cx| {
                    apply_status_selection(issue_id.clone(), pick, window, cx)
                }),
                cx,
            )
        });
    }

    // Assignee submenu (current member first, agents hidden). The trigger
    // reflects the current assignment (EXP-59) — the member glyph when
    // assigned, the unassigned placeholder otherwise (submenu triggers take
    // an `Icon`, so the web's avatar degrades to the assignee-row glyph).
    {
        let issue_id = issue.id.clone();
        let board_id = issue.board_id.clone();
        let current = issue.assignee_id.clone();
        let icon = if current.is_some() {
            Icon::new(registry::UI_ASSIGNEE)
        } else {
            Icon::new(registry::UI_UNASSIGNED)
        };
        menu = menu.submenu_with_icon(Some(icon), "Assignee", window, cx, move |menu, _, cx| {
            assignee_menu(menu, &issue_id, &board_id, current.as_deref(), cx)
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
                let issue_id = issue_id.clone();
                let value = option.value;
                menu = menu.item(option_item(
                    SharedString::from(option.label),
                    option_icon(option, cx),
                    option.value == current,
                    move |_window, cx| {
                        let mut input = api::issues::IssuesUpdateInput::new(issue_id.clone());
                        input.priority = Some(value);
                        spawn_issue_update(cx, input);
                    },
                ));
            }
            menu
        });
    }

    // Labels submenu (colored dot + name + check, toggle membership).
    {
        let issue_id = issue.id.clone();
        let board_id = issue.board_id.clone();
        let icon = Icon::from(ExpIcon::Tag);
        menu = menu.submenu_with_icon(Some(icon), "Labels", window, cx, move |menu, _, cx| {
            let mut menu = menu.check_side(Side::Right);
            let collections = Store::global(cx).collections();
            let Some(board) = collections.boards.read(cx).get(&board_id).cloned() else {
                return menu;
            };
            let mut labels: Vec<Label> = collections
                .labels
                .read(cx)
                .iter()
                .filter(|label| label.team_id == board.team_id)
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

    // Move to board submenu (EXP-57, web `BoardSubmenu`): the team's
    // boards with the current one disabled; hidden unless another board
    // exists. The server renumbers the issue in the target board
    // (EXP-42 → ABC-17); the row re-homes on the Electric echo.
    if !move_target_boards(cx, &issue.board_id).is_empty() {
        let issue_id = issue.id.clone();
        let identifier = issue.identifier.clone();
        let board_id = issue.board_id.clone();
        menu = menu.submenu_with_icon(
            Some(Icon::from(ExpIcon::SquareKanban)),
            "Move to board",
            window,
            cx,
            move |menu, _, cx| move_to_board_menu(menu, &issue_id, &identifier, &board_id, cx),
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
                // EXP-697: no dividers in menus.
                menu = menu.item(
                    PopupMenuItem::new("Clear due date")
                        .icon(Icon::new(registry::UI_CLOSE))
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

    // Delete issue (EXP-697): ONE red item, not a nested confirm submenu —
    // a submenu label cannot be colored, and every other client confirms a
    // destructive action through a dialog instead.
    {
        let issue_id = issue.id.clone();
        let identifier = issue.identifier.clone();
        menu = menu.item(
            crate::controls::danger_menu_item(
                "Delete issue",
                Icon::new(registry::UI_DELETE),
                cx,
            )
            .on_click(move |_, window, cx| {
                prompt_issue_delete(issue_id.clone(), identifier.clone(), window, cx);
            }),
        );
    }

    menu
}

/// The destructive confirm behind every "Delete issue" affordance (EXP-697) —
/// the shared alert window the machines/actions removes already use.
pub(crate) fn prompt_issue_delete(
    issue_id: String,
    identifier: String,
    window: &mut Window,
    cx: &mut App,
) {
    let spec = crate::native_dialog::AlertSpec::new(
        "Delete issue",
        format!("Delete {identifier}? This cannot be undone."),
        "Delete",
    )
    .ok_variant(gpui_component::button::ButtonVariant::Danger)
    .on_ok(move |_, cx| {
        spawn_issue_delete(cx, issue_id.clone());
        true
    });
    crate::native_dialog::open_alert(window, cx, spec);
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

/// The move-to-board submenu's target list (EXP-57): the issue's
/// team's boards in the shared sidebar order — empty (submenu hidden,
/// web `boards.length > 1` gate) unless a move target exists.
pub(crate) fn move_target_boards(cx: &App, board_id: &str) -> Vec<Board> {
    let collections = Store::global(cx).collections();
    let Some(team_id) = collections
        .boards
        .read(cx)
        .get(board_id)
        .map(|board| board.team_id.clone())
    else {
        return Vec::new();
    };
    let boards = collections.boards_in_team(&team_id, cx);
    if boards.len() < 2 {
        return Vec::new();
    }
    boards
}

/// The row context menu's move-to-board submenu body (EXP-57): the board's
/// own glyph tinted with its color + name (EXP-282 — same treatment as the
/// rail's board icons; it replaced the anonymous color dot), current board
/// checked + disabled; picking another confirms, then fires `issues.move`
/// (REV-35 — EXP-428's "confirm everywhere").
pub(crate) fn move_to_board_menu(
    menu: PopupMenu,
    issue_id: &str,
    identifier: &str,
    board_id: &str,
    cx: &App,
) -> PopupMenu {
    let mut menu = menu.check_side(Side::Right);
    for board in move_target_boards(cx, board_id) {
        let is_current = board.id == board_id;
        let tint = board
            .color
            .as_deref()
            .and_then(parse_hex_color)
            .unwrap_or(gpui::opaque_grey(0.5, 1.0));
        let icon = crate::icons::board_icon(&board).text_color(tint);
        let name = SharedString::from(board.name.clone());
        let issue_id = issue_id.to_string();
        let identifier = identifier.to_string();
        let target_id = board.id.clone();
        let target_name = board.name.clone();
        menu = menu.item(
            PopupMenuItem::element(move |_, cx| {
                h_flex()
                    .gap_2()
                    .items_center()
                    .child(icon.clone().xsmall())
                    .child(
                        div()
                            .text_color(cx.theme().popover_foreground)
                            .child(name.clone()),
                    )
            })
            .checked(is_current)
            .disabled(is_current)
            .on_click(move |_, window, cx| {
                confirm_issue_move(
                    window,
                    cx,
                    issue_id.clone(),
                    identifier.clone(),
                    target_id.clone(),
                    target_name.clone(),
                );
            }),
        );
    }
    menu
}

/// EXP-428 "confirm everywhere": every board-move entry point asks first —
/// the move renumbers the issue (EXP-42), breaking `#IDENT` references — with
/// the canonical cross-client wording (web/iOS/Android share it). Shared by
/// the row context submenu and the detail header's Board chip (EXP-426).
pub(crate) fn confirm_issue_move(
    window: &mut Window,
    cx: &mut App,
    issue_id: String,
    identifier: String,
    target_id: String,
    target_name: String,
) {
    let spec = crate::native_dialog::AlertSpec::new(
        "Move issue",
        format!(
            "Move {identifier} to \"{target_name}\"? The issue will get a new \
             identifier in that board."
        ),
        "Move",
    )
    .on_ok(move |_, cx| {
        spawn_issue_move(cx, issue_id.clone(), target_id.clone());
        true
    });
    crate::native_dialog::open_alert(window, cx, spec);
}

/// §4.1 un-gated `issues.move` on a background thread (EXP-57): the issue
/// re-homes (and renumbers, EXP-42 → ABC-17) on the Electric echo.
pub(crate) fn spawn_issue_move(cx: &mut App, issue_id: String, board_id: String) {
    let Some(trpc) = queries::trpc_client(cx) else {
        log::warn!("[ui] issues.move skipped: no signed-in account");
        return;
    };
    cx.background_executor()
        .spawn(async move {
            if let Err(err) = api::issues::issues_move(&trpc, &issue_id, &board_id) {
                log::warn!("[ui] issues.move({issue_id} -> {board_id}) failed: {err}");
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

/// `#rrggbb` → Hsla (label/board colors are stored as hex strings; shared
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

/// EXP-439 compact chip: the label's color dot alone — what a narrow list
/// keeps of [`label_chip`] (name and border dropped, hue preserved).
fn label_dot(label: &Label, cx: &App) -> impl IntoElement {
    let color = label
        .color
        .as_deref()
        .and_then(parse_hex_color)
        .unwrap_or(cx.theme().muted_foreground);
    div().size_1p5().rounded_full().flex_shrink_0().bg(color)
}

/// EXP-698: a label chip IS the shared small READONLY pill with the label's
/// colour dot leading — the same capsule the active-filter pills, the role
/// badges and the attachment chips wear.
fn label_chip(label: &Label, cx: &App) -> impl IntoElement {
    use crate::surface::{pill_dot, PillMode, PillSize};
    let color = label
        .color
        .as_deref()
        .and_then(parse_hex_color)
        .unwrap_or(cx.theme().muted_foreground);
    crate::surface::glass_pill(
        gpui::ElementId::Name(SharedString::from(format!("label-chip-{}", label.id))),
        PillSize::Sm,
        PillMode::Readonly,
        cx,
    )
    .child(pill_dot(color))
    .child(SharedString::from(label.name.clone()))
}

/// Due-date urgency (REV2-48) — the mobile rule ported verbatim (iOS
/// `dueDateColor`, Android `StatusColors.dueDateColor`): due TODAY wins over
/// overdue, so a date that is already past local midnight still reads orange.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DueTone {
    Overdue,
    Today,
    Upcoming,
}

/// `today` is the device-LOCAL `YYYY-MM-DD` boundary the EXP-38 overdue sort
/// already uses (`queries::today_local`), so the color explains the ordering.
fn due_tone(due: &str, today: &str) -> DueTone {
    if due == today {
        DueTone::Today
    } else if due < today {
        DueTone::Overdue
    } else {
        DueTone::Upcoming
    }
}

/// Web due cell: `CalendarDays` + "Jul 3" when due; an empty slot when not
/// (the cell stays so the row layout holds; the date Popover is a later step).
fn due_cell(issue: &Issue, cx: &App) -> impl IntoElement {
    let cell = h_flex().ml_3().gap_1().items_center().flex_shrink_0();
    match issue.due_date.as_deref() {
        Some(due) => {
            let color = match due_tone(due, &queries::today_local()) {
                DueTone::Overdue => t::RED.to_hsla(),
                DueTone::Today => t::ORANGE.to_hsla(),
                DueTone::Upcoming => cx.theme().muted_foreground,
            };
            cell.child(Icon::from(ExpIcon::CalendarDays).xsmall().text_color(color))
                .child(
                    div()
                        .text_xs()
                        .text_color(color)
                        .whitespace_nowrap()
                        .child(SharedString::from(format_short_date(due))),
                )
        }
        None => cell,
    }
}

// ---------------------------------------------------------------------------
// Chrome bits
// ---------------------------------------------------------------------------

/// Group-header tint strength. Web parity: `statusHeaderBg`'s `/10` washes in
/// `apps/web/src/components/issue-list.tsx`. Deliberately between the glass
/// ladder's row fill (white 5% — the hover state) and its active fill (white
/// 15% — the selected state), so a tinted header can never be misread as a
/// hovered or selected issue row.
const HEADER_TINT_ALPHA: f32 = 0.10;

/// The group header's background wash, in the status' own hue (EXP-293).
///
/// History: EXP-282 deleted the original `status_header_bg` (verbatim web rgba
/// literals) and EXP-285 flattened the remaining glass band, leaving headers
/// bare on the page gradient with only a hairline — at 28px row height that
/// made them near-indistinguishable from issue rows. The tint is back, but
/// token-locked this time: for a BUILTIN status the hue comes from its own
/// [`ColorToken`] (`domain::options` — "status/priority accents are
/// token-locked, never loose hex in Rust"), so header, icon and every other
/// status affordance can never drift apart.
///
/// EXP-314: a CUSTOM status washes with its stored hex (the same 20-swatch
/// palette the labels use — all mid-tone, so the same alpha reads correctly),
/// degrading to the neutral accent when the hex does not parse.
fn status_header_tint(tint: &StatusTint) -> gpui::Hsla {
    let base = match tint {
        StatusTint::Token(token) => status_tint_base(*token).to_hsla(),
        StatusTint::Hex(hex) => {
            crate::settings::parse_hex_color(hex).unwrap_or_else(|| t::NEUTRAL.to_hsla())
        }
    };
    base.opacity(HEADER_TINT_ALPHA)
}

/// Hue source for [`status_header_tint`] — the generated design token behind a
/// [`ColorToken`], deliberately as a pure fn (no `cx`) so the mapping is unit
/// testable.
///
/// The five accents are exactly what [`crate::icons::token_color`] resolves
/// them to. The two FOREGROUND roles cannot be: `theme.foreground` and
/// `theme.muted_foreground` are near-white and near-grey, so a wash of them
/// would be indistinguishable from the white-alpha glass fills the app already
/// paints everywhere (hover, active, section). They map to the NEUTRAL accent
/// instead — the same role web's `zinc-500/10` plays for backlog / cancelled /
/// duplicate.
fn status_tint_base(token: ColorToken) -> theme::Srgb8 {
    match token {
        ColorToken::Foreground | ColorToken::MutedForeground => t::NEUTRAL,
        ColorToken::Yellow => t::YELLOW,
        ColorToken::Green => t::GREEN,
        ColorToken::Red => t::RED,
        ColorToken::Orange => t::ORANGE,
        ColorToken::Blue => t::BLUE,
    }
}

/// Web `IssueListSkeleton`: one header row + five body rows of placeholders.
fn list_skeleton(cx: &App) -> impl IntoElement {
    let header = h_flex()
        .h(px(HEADER_HEIGHT))
        .w_full()
        .px_3()
        .gap_2()
        .items_center()
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

/// Stable per-issue element id: `{kind}-{issue_id}`.
fn row_id(kind: &str, issue_id: &str) -> ElementId {
    ElementId::Name(SharedString::from(format!("{kind}-{issue_id}")))
}

/// Stable per-group element id: `{kind}-{group_key}` (EXP-314 — a status ROW
/// id, or `builtin:<key>` before the statuses shape syncs).
fn header_id(kind: &str, group_key: &str) -> ElementId {
    ElementId::Name(SharedString::from(format!("{kind}-{group_key}")))
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

    #[test]
    fn due_tone_mirrors_the_mobile_rule() {
        // REV2-48: due today is its own tone and WINS over overdue; anything
        // later is muted. Same boundary the overdue-first sort uses.
        assert_eq!(due_tone("2026-07-27", "2026-07-27"), DueTone::Today);
        assert_eq!(due_tone("2026-07-26", "2026-07-27"), DueTone::Overdue);
        assert_eq!(due_tone("2026-07-28", "2026-07-27"), DueTone::Upcoming);
        // Year/month rollovers compare correctly as ISO strings.
        assert_eq!(due_tone("2025-12-31", "2026-01-01"), DueTone::Overdue);
    }

    #[test]
    fn group_header_tints_follow_the_status_accent_tokens() {
        // EXP-293 web parity (`statusHeaderBg`): in_progress yellow, in_review
        // green, done blue; every grey status shares the neutral accent. Locked
        // to the GENERATED tokens so the header wash can never drift from the
        // status icon beside it. EXP-314 keeps this BYTE-identical for the 6
        // builtins (their tint is still a ColorToken, never the synced hex).
        for (builtin_key, want) in [
            ("in_progress", t::YELLOW),
            ("in_review", t::GREEN),
            ("done", t::BLUE),
            ("backlog", t::NEUTRAL),
            ("cancelled", t::NEUTRAL),
            ("duplicate", t::NEUTRAL),
            // Forward-compat: an unknown builtin key falls back to muted →
            // the neutral accent, so it still gets a tint.
            ("brand_new", t::NEUTRAL),
        ] {
            let tint = status_header_tint(&StatusTint::Token(
                domain::statuses::builtin_color_token(builtin_key),
            ));
            let want = want.to_hsla();
            assert!(
                (tint.h - want.h).abs() < 0.001 && (tint.s - want.s).abs() < 0.001,
                "{builtin_key} tint hue/sat {tint:?} != token {want:?}"
            );
            assert!(
                (tint.a - HEADER_TINT_ALPHA).abs() < 0.001,
                "{builtin_key} tint alpha: {tint:?}"
            );
        }
    }

    #[test]
    fn custom_status_headers_wash_with_their_stored_hex() {
        // EXP-314: a CUSTOM row has no token — its hex washes at the same
        // alpha, and an unparseable value degrades to the neutral accent.
        let tint = status_header_tint(&StatusTint::Hex("#22c55e".to_string()));
        let want: gpui::Hsla = gpui::Rgba {
            r: 0x22 as f32 / 255.,
            g: 0xc5 as f32 / 255.,
            b: 0x5e as f32 / 255.,
            a: 1.0,
        }
        .into();
        assert!(
            (tint.h - want.h).abs() < 0.001 && (tint.s - want.s).abs() < 0.001,
            "custom tint {tint:?} != hex {want:?}"
        );
        assert!((tint.a - HEADER_TINT_ALPHA).abs() < 0.001);

        for bad in ["", "nope", "22c55e"] {
            let tint = status_header_tint(&StatusTint::Hex(bad.to_string()));
            let neutral = t::NEUTRAL.to_hsla();
            assert!(
                (tint.h - neutral.h).abs() < 0.001 && (tint.s - neutral.s).abs() < 0.001,
                "{bad:?} should degrade to neutral, got {tint:?}"
            );
        }
    }

    #[test]
    fn header_tint_sits_between_the_row_hover_and_active_fills() {
        // The header must never be mistaken for a hovered (glass FILL_ROW) or
        // selected (FILL_ACTIVE) issue row — those are the two white washes
        // painted on the rows right below it.
        let hover = t::glass::FILL_ROW.a as f32 / 255.;
        let active = t::glass::FILL_ACTIVE.a as f32 / 255.;
        assert!(
            hover < HEADER_TINT_ALPHA && HEADER_TINT_ALPHA < active,
            "hover {hover} < header {HEADER_TINT_ALPHA} < active {active}"
        );
    }
}
