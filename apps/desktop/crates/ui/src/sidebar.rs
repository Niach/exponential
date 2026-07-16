//! The workspace sidebar (masterplan-v3 §4.2, reworked as a JetBrains-style
//! tool-window rail).
//!
//! Two cooperating views share per-window state through [`RailShared`]:
//!
//! - [`RailView`] — a 44px icon-only strip owned by the `Workspace` shell and
//!   rendered OUTSIDE the `DockArea`, full height below the top bar. Top: the
//!   Search action, then the tool-window selectors — **Inbox / My Issues /
//!   All Issues / Reviews** (mini issue lists; Reviews carries a
//!   dot while open PRs exist) and **Files / Source Control** (Source Control carries
//!   an amber badge in conflict mode and opens the changes
//!   screen immediately). The active tool's icon is tinted with the active
//!   project's color. One tool is ALWAYS active — re-clicking never
//!   unselects. Bottom: terminal-dock toggle, settings gear, and the
//!   **account button as the very bottom element** — its dropdown holds the
//!   account-level actions only (EXP-69: workspace switching moved into the
//!   top bar's merged project picker).
//! - [`SidebarPanel`] — the tool-window column right of the rail (a resizable
//!   pane INSIDE the dock-area center, so the bottom terminal dock runs
//!   beneath it): the active tool window's content. Issue tools are mini
//!   master lists whose rows open the full detail in the center pane; Source
//!   Control lists the trunk's local branches — rows VIEW that branch's
//!   history (never a checkout; checkout lives exclusively on the git bar's
//!   branch chip, the one dirty-switch dialog surface); Files is the trunk
//!   file tree.
//!
//! Every affordance dispatches a typed action (§3.6) or navigates directly;
//! menus render in the Root overlay, outside this element tree.

use std::collections::{HashMap, HashSet};

use gpui::{
    div, prelude::FluentBuilder as _, px, App, AppContext as _, ClickEvent, Entity,
    FontWeight, Hsla, InteractiveElement as _, IntoElement, ParentElement, Render, ScrollHandle,
    SharedString, StatefulInteractiveElement as _, Styled, Subscription, Window, WindowId,
};
use gpui_component::{
    button::{Button, ButtonVariants as _},
    h_flex,
    menu::DropdownMenu as _,
    scroll::ScrollableElement as _,
    skeleton::Skeleton,
    v_flex, ActiveTheme as _, Disableable as _, Icon, IconName, Selectable as _, Sizable as _,
};
use sync::Store;


use crate::actions::{CreateWorkspace, OpenSettings, SendFeedback, SignOut};
use crate::board::BoardView;
use crate::coding_flow;
use crate::git_bar::GitBar;
use crate::icons::ExpIcon;
use crate::issue_list::IssueQuery;
use crate::navigation::{
    active_project_id, active_workspace_id, nav_for_window, navigate, resolved_screen, Navigation,
    Screen,
};
use crate::properties_panel::parse_hex_color;
use crate::queries;

/// Width of the icon-only rail column (outside the dock area).
pub(crate) const RAIL_W: f32 = 44.;

/// Default tool-window width (EXP-109: doubled from the original 260px web
/// parity — the issue lists inside the tool window were too cramped).
pub(crate) const DEFAULT_DOCK_WIDTH: f32 = 520.;

/// The rail's tool windows (JetBrains tool-window bar). One is ALWAYS active
/// — there is deliberately no unselected/collapsed state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ToolWindow {
    /// Notification groups (mini inbox); rows open the issue detail.
    Inbox,
    /// Issues assigned to me across the workspace (mini list).
    MyIssues,
    /// Every issue in the workspace (mini list).
    AllIssues,
    /// Open pull requests across the workspace: issue-linked ones grouped by
    /// project, plus GitHub-listed PRs not linked to any issue grouped by
    /// repo — both with an inline squash-merge action (server-side via the
    /// GitHub App).
    Reviews,
    /// The trunk file tree at full panel height.
    Files,
    /// The trunk's local branches; activating also opens the changes screen.
    SourceControl,
}

/// Per-window state both rail and tool-window panel read: which tool window
/// is active plus the shared repo-backed entities. Lives in a window-keyed
/// registry (same pattern as `navigation::nav_for_window`) because the views
/// are constructed on different paths.
pub(crate) struct RailShared {
    tool: ToolWindow,
    /// The trunk git chrome (rendered by the top bar). Driven every rail
    /// render so the §4.1 auto-clone lifecycle and the rail's conflict badge
    /// stay live regardless of the visible screen.
    git_bar: Entity<GitBar>,
    file_tree: Entity<crate::file_tree::FileTreeView>,
    /// The "All Issues" tool window's board (filter bar + grouped list,
    /// scoped to the active project). Shared here — not on `SidebarPanel` —
    /// so the issue detail's prev/next switcher (EXP-48) can read the same
    /// query + filter state the visible list applies.
    board_all: Entity<BoardView>,
    /// The "My Issues" board (assignee == me across the workspace).
    board_my: Entity<BoardView>,
    /// The branch whose HISTORY the Source Control screen shows — a sidebar
    /// branch row selects it WITHOUT checking out (`None` = the checked-out
    /// branch, working tree included).
    view_branch: Option<String>,
}

impl RailShared {
    /// The shared trunk git chrome — the top bar renders it.
    pub(crate) fn git_bar(&self) -> &Entity<GitBar> {
        &self.git_bar
    }

    /// The Source Control screen's history scope (`None` = current branch).
    pub(crate) fn view_branch(&self) -> Option<&str> {
        self.view_branch.as_deref()
    }

    /// The issue list whose ordering the detail's prev/next switcher follows
    /// (EXP-48): the My Issues board while that tool window is active, the
    /// All Issues board otherwise (it is the window's persistent issue list).
    pub(crate) fn active_issue_board(&self) -> &Entity<BoardView> {
        match self.tool {
            ToolWindow::MyIssues => &self.board_my,
            _ => &self.board_all,
        }
    }

    /// Both issue boards (the detail view observes them so the EXP-48
    /// counter re-renders on filter changes).
    pub(crate) fn issue_boards(&self) -> [&Entity<BoardView>; 2] {
        [&self.board_all, &self.board_my]
    }
}

/// Point the Source Control screen at `branch`'s history (no checkout);
/// `None` returns to the checked-out branch.
pub(crate) fn set_view_branch(window: &mut Window, cx: &mut App, branch: Option<String>) {
    let shared = rail_shared_for_window(window, cx);
    shared.update(cx, |shared, cx| {
        if shared.view_branch != branch {
            shared.view_branch = branch;
            cx.notify();
        }
    });
}

#[derive(Default)]
struct RailRegistry {
    by_window: HashMap<WindowId, Entity<RailShared>>,
}

impl gpui::Global for RailRegistry {}

/// The window's shared rail state, created on first access.
pub(crate) fn rail_shared_for_window(
    window: &mut Window,
    cx: &mut App,
) -> Entity<RailShared> {
    let window_id = window.window_handle().window_id();
    if let Some(existing) = cx
        .try_global::<RailRegistry>()
        .and_then(|registry| registry.by_window.get(&window_id).cloned())
    {
        return existing;
    }
    let git_bar = cx.new(|cx| GitBar::new(window, cx));
    let file_tree = cx.new(|cx| crate::file_tree::FileTreeView::new(window, cx));
    let board_all = cx.new(|cx| BoardView::new(window, cx));
    let board_my = cx.new(|cx| BoardView::new(window, cx));
    let shared = cx.new(|_| RailShared {
        // Issues-first default: the All Issues list is the board.
        tool: ToolWindow::AllIssues,
        git_bar,
        file_tree,
        board_all,
        board_my,
        view_branch: None,
    });
    cx.default_global::<RailRegistry>()
        .by_window
        .insert(window_id, shared.clone());
    shared
}

/// Drop a closed window's entry (called from the `Workspace` release hook,
/// mirroring `navigation::remove_window`).
pub fn remove_window(window_id: WindowId, cx: &mut App) {
    if let Some(registry) = cx.try_global::<RailRegistry>() {
        if registry.by_window.contains_key(&window_id) {
            cx.global_mut::<RailRegistry>().by_window.remove(&window_id);
        }
    }
}

/// Select `tool` in this window's rail. Re-selecting the active tool is a
/// no-op (a tool window can never be unselected). Source Control additionally
/// navigates to the changes screen — the sidebar shows branches, the center
/// shows commits + diff, immediately.
pub(crate) fn activate_tool(window: &mut Window, cx: &mut App, tool: ToolWindow) {
    let shared = rail_shared_for_window(window, cx);
    if shared.read(cx).tool != tool {
        shared.update(cx, |shared, cx| {
            shared.tool = tool;
            if tool == ToolWindow::Files {
                // Activation kicks a git-status refresh so the tree's dots
                // reflect the trunk as of now.
                shared.file_tree.update(cx, |tree, cx| tree.refresh(cx));
            }
            cx.notify();
        });
    }
    if tool == ToolWindow::SourceControl {
        navigate(window, cx, Screen::SourceControl);
    }
}

/// The window's active-project accent color (rail selection tint, falls back
/// to the theme primary when the project has no color).
fn project_accent(nav: &Entity<Navigation>, cx: &App) -> Hsla {
    active_project_id(nav, cx)
        .and_then(|id| {
            Store::global(cx)
                .collections()
                .projects
                .read(cx)
                .get(&id)
                .and_then(|project| project.color.as_deref().and_then(parse_hex_color))
        })
        .unwrap_or_else(|| cx.theme().primary)
}

// ---------------------------------------------------------------------------
// RailView — the icon strip left of the dock area
// ---------------------------------------------------------------------------

/// The 44px tool-window rail. Owned and rendered by the `Workspace` shell
/// OUTSIDE the `DockArea`, below the full-width top bar. (No terminal
/// toggle — the bottom terminal strip is the single toggle affordance.)
pub struct RailView {
    nav: Entity<Navigation>,
    shared: Entity<RailShared>,
    /// The branch as of the last render — a checkout refreshes the file tree.
    last_branch: Option<String>,
    _subscriptions: Vec<Subscription>,
}

impl RailView {
    pub fn new(window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        let nav = nav_for_window(window, cx);
        let shared = rail_shared_for_window(window, cx);
        let git_bar = shared.read(cx).git_bar.clone();
        let collections = Store::global(cx).collections().clone();
        let subscriptions = vec![
            cx.observe(&shared, |_, _, cx| cx.notify()),
            cx.observe(&nav, |_, _, cx| cx.notify()),
            // Conflict badge follows the git bar's trunk state.
            cx.observe(&git_bar, |_, _, cx| cx.notify()),
            // The Reviews dot is a live read over issues ⨝ projects.
            cx.observe(&collections.issues, |_, _, cx| cx.notify()),
            cx.observe(&collections.projects, |_, _, cx| cx.notify()),
        ];
        Self {
            nav,
            shared,
            last_branch: None,
            _subscriptions: subscriptions,
        }
    }

    /// One tool-window icon: a ghost icon button, `selected` + tinted with
    /// the project accent while its tool window is active; `badge` paints the
    /// amber conflict dot.
    fn rail_tool_icon(
        &self,
        id: &'static str,
        icon: Icon,
        tool: ToolWindow,
        tooltip: &'static str,
        badge: bool,
        accent: Hsla,
        cx: &mut gpui::Context<Self>,
    ) -> gpui::AnyElement {
        let active = self.shared.read(cx).tool == tool;
        let icon = if active { icon.text_color(accent) } else { icon };
        div()
            .relative()
            .child(
                Button::new(id)
                    .ghost()
                    .small()
                    .icon(icon)
                    .selected(active)
                    .tooltip(tooltip)
                    .on_click(cx.listener(move |_, _: &ClickEvent, window, cx| {
                        activate_tool(window, cx, tool);
                    })),
            )
            .when(active, |this| {
                // JetBrains-style selection marker: a 2px accent bar hugging
                // the rail's left edge.
                this.child(
                    div()
                        .absolute()
                        .left(px(-6.))
                        .top_0()
                        .bottom_0()
                        .w(px(2.))
                        .rounded_full()
                        .bg(accent),
                )
            })
            .when(badge, |this| {
                this.child(
                    div()
                        .absolute()
                        .top_0()
                        .right_0()
                        .size_1p5()
                        .rounded_full()
                        .bg(cx.theme().warning),
                )
            })
            .into_any_element()
    }

    /// The account button — ALWAYS the rail's very bottom element. Its
    /// dropdown holds the account-level actions (EXP-69: workspace switching
    /// lives in the top bar's merged project picker now, and account
    /// deletion is web/mobile-only).
    fn render_account_button(&self, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let who: SharedString = crate::queries::active_account(cx)
            .map(|account| SharedString::from(account.email.clone()))
            .unwrap_or_else(|| "Not signed in".into());
        let label = who.clone();

        Button::new("rail-account")
            .ghost()
            .small()
            .icon(IconName::CircleUser)
            .tooltip(who)
            .dropdown_menu_with_anchor(gpui::Anchor::BottomLeft, move |menu, _window, _cx| {
                menu.label(label.clone())
                    .menu_with_icon("Settings", IconName::Settings, Box::new(OpenSettings))
                    .menu_with_icon(
                        "Notifications",
                        IconName::Bell,
                        Box::new(crate::actions::OpenAccount),
                    )
                    .menu_with_icon("Send Feedback", IconName::ThumbsUp, Box::new(SendFeedback))
                    .menu_with_icon("New team", IconName::Plus, Box::new(CreateWorkspace))
                    .separator()
                    .menu("Sign out", Box::new(SignOut))
            })
    }

    fn divider(&self, cx: &mut gpui::Context<Self>) -> gpui::AnyElement {
        div()
            .w_6()
            .h(px(1.))
            .my_1()
            .bg(cx.theme().sidebar_border)
            .into_any_element()
    }
}

impl Render for RailView {
    fn render(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        // Keep the git lifecycle live regardless of which tool window is
        // open: auto-clone on project open + the conflict badge both ride the
        // GitBar's load gate.
        let git_bar = self.shared.read(cx).git_bar.clone();
        git_bar.update(cx, |bar, cx| bar.ensure_loaded(window, cx));
        let conflict = git_bar.read(cx).has_conflict();

        // A branch checkout changes the working tree — refresh the file tree
        // the first render after the branch flips.
        let branch = git_bar.read(cx).branch().to_string();
        if !branch.is_empty() && self.last_branch.as_deref() != Some(branch.as_str()) {
            let refresh = self.last_branch.is_some();
            self.last_branch = Some(branch);
            if refresh {
                self.shared
                    .read(cx)
                    .file_tree
                    .clone()
                    .update(cx, |tree, cx| tree.refresh(cx));
            }
        }

        let accent = project_accent(&self.nav, cx);
        // Reviews badge: any open issue-linked PR in the active workspace.
        let has_reviews = active_workspace_id(&self.nav, cx)
            .map(|id| !queries::review_issues(cx, &id).is_empty())
            .unwrap_or(false);
        v_flex()
            .w(px(RAIL_W))
            .flex_shrink_0()
            .h_full()
            .items_center()
            .py_2()
            .gap_1()
            .bg(cx.theme().tokens.sidebar)
            .text_color(cx.theme().sidebar_foreground)
            .border_r_1()
            .border_color(cx.theme().sidebar_border)
            // Search — opens the ⌘K sheet. Call the opener directly via
            // cx.listener (like the rail tool icons below) rather than
            // dispatching OpenSearch: a rail button that dispatches to the
            // App-global handler fires from inside the window's own update, and
            // the handler's re-entrant active-window lookup makes the click
            // silently no-op — the gear next to it was dead for exactly this
            // reason (EXP-17). The ⌘K keybinding still routes through the action.
            .child(
                Button::new("rail-search")
                    .ghost()
                    .small()
                    .icon(IconName::Search)
                    .tooltip("Search")
                    .on_click(cx.listener(|_, _: &ClickEvent, window, cx| {
                        crate::search_sheet::open_search(window, cx)
                    })),
            )
            .child(self.divider(cx))
            // Issue tool windows — Inbox on top, then My Issues, All Issues.
            .child(self.rail_tool_icon(
                "rail-inbox",
                Icon::new(IconName::Inbox),
                ToolWindow::Inbox,
                "Inbox",
                false,
                accent,
                cx,
            ))
            .child(self.rail_tool_icon(
                "rail-my-issues",
                Icon::new(IconName::CircleUser),
                ToolWindow::MyIssues,
                "My Issues",
                false,
                accent,
                cx,
            ))
            .child(self.rail_tool_icon(
                "rail-all-issues",
                Icon::from(ExpIcon::ListTodo),
                ToolWindow::AllIssues,
                "All Issues",
                false,
                accent,
                cx,
            ))
            .child(self.rail_tool_icon(
                "rail-reviews",
                Icon::from(ExpIcon::GitPullRequest),
                ToolWindow::Reviews,
                "Reviews",
                has_reviews,
                accent,
                cx,
            ))
            .child(self.divider(cx))
            // Repo tool windows.
            .child(self.rail_tool_icon(
                "rail-files",
                Icon::new(IconName::Folder),
                ToolWindow::Files,
                "Files",
                false,
                accent,
                cx,
            ))
            .child(self.rail_tool_icon(
                "rail-source-control",
                Icon::from(ExpIcon::GitMerge),
                ToolWindow::SourceControl,
                "Source Control",
                conflict,
                accent,
                cx,
            ))
            .child(div().flex_1())
            .child(
                Button::new("rail-settings")
                    .ghost()
                    .small()
                    .icon(IconName::Settings)
                    .tooltip("Settings")
                    // Navigate directly (see the search button above): the
                    // dispatch → App-global OpenSettings handler no-ops when
                    // fired from a rail button, which is why the gear did
                    // nothing (EXP-17). The account-dropdown "Settings" item and
                    // the keymap still dispatch the action.
                    .on_click(cx.listener(|_, _: &ClickEvent, window, cx| {
                        navigate(window, cx, Screen::Settings)
                    })),
            )
            .child(self.render_account_button(cx))
    }
}

// ---------------------------------------------------------------------------
// SidebarPanel — the tool-window column
// ---------------------------------------------------------------------------

/// The tool-window column right of the rail. A plain view — it lives inside
/// the dock-area center's resizable split (NOT a dock), so the bottom
/// terminal dock spans beneath it.
pub struct SidebarPanel {
    nav: Entity<Navigation>,
    shared: Entity<RailShared>,
    /// The "All Issues" tool window — the full board (filter bar with
    /// All/Active/Backlog tabs + New Issue + the grouped virtualized list
    /// with inline status/priority menus), scoped to the active project.
    /// Lives in [`RailShared`] (EXP-48 — the detail switcher reads it too).
    board_all: Entity<BoardView>,
    /// The "My Issues" tool window — same board pinned to assignee == me
    /// (also shared via [`RailShared`]).
    board_my: Entity<BoardView>,
    /// The Source Control tool window's branch-flow graph (replaced the flat
    /// branch list — [`crate::flow_view`]).
    flow: Entity<crate::flow_view::FlowView>,
    /// Scroll position of the flow graph's lane list (EXP-67: the full
    /// uncapped list scrolls instead of collapsing behind "+N more").
    flow_scroll: ScrollHandle,
    /// Two-click merge/close confirm: the armed row's key — an issue id,
    /// `close:<issue id>` for the issue row's close-without-merge button
    /// (see [`close_pr_key`]), or `repo#number` for an unlinked pull (see
    /// [`pull_merge_key`]). Any other click or ~5s of inactivity disarms.
    review_arm: Option<String>,
    /// Bumped on every arm/disarm — a stale disarm timer checks it before
    /// clearing so it never cancels a newer arm.
    review_arm_seq: u64,
    /// Row keys with an in-flight merge/close call (`issues.mergePr`,
    /// `issues.closePr` under [`close_pr_key`], or `repositories.mergePull`).
    /// Issue rows keep the key until the Electric echo removes the row
    /// (render prunes it); pull rows clear on completion.
    review_merging: HashSet<String>,
    /// The last merge failure, `(row_key, message)` — a caption under the
    /// row, cleared on the next attempt.
    review_error: Option<(String, String)>,
    /// Fetched `repositories.openPulls` result: `(workspace_id, repos)` —
    /// open PRs with NO issue link (release PRs, manual branches, external
    /// contributors), listed straight from GitHub. Rendered below the project
    /// groups; a merged pull is removed locally (no Electric echo).
    open_pulls: Option<(String, Vec<api::repositories::OpenPullsRepo>)>,
    /// The workspace the current openPulls fetch belongs to. Cleared whenever
    /// the Reviews tool window is inactive, so re-opening refetches (the
    /// server caches ~60s; there is deliberately no polling).
    open_pulls_key: Option<String>,
    /// Bumped per fetch — a stale response checks it before landing.
    open_pulls_seq: u64,
    _subscriptions: Vec<Subscription>,
}

/// Merge-state key for an unlinked pull. `review_arm`/`review_merging`/
/// `review_error` share the namespace with issue rows, whose keys are issue
/// UUIDs — `repo-uuid#number` can never collide with those.
fn pull_merge_key(repository_id: &str, number: u64) -> String {
    format!("{repository_id}#{number}")
}

/// Arm/in-flight key for an issue row's close-without-merge action (EXP-100).
/// Shares the `review_arm`/`review_merging` namespace with the merge keys —
/// the `close:` prefix can never collide with an issue UUID or a
/// `repo-uuid#number` pull key.
fn close_pr_key(issue_id: &str) -> String {
    format!("close:{issue_id}")
}

/// Latest-notification kind → the inbox row's leading type-badge glyph (the
/// meaning table shared across all clients).
fn notification_type_icon(kind: Option<&str>) -> Icon {
    match kind {
        Some(domain::contract::NOTIFICATION_TYPE_ISSUE_ASSIGNED) => Icon::from(ExpIcon::UserPlus),
        Some(domain::contract::NOTIFICATION_TYPE_ISSUE_COMMENT)
        | Some(domain::contract::NOTIFICATION_TYPE_ISSUE_MENTION) => {
            Icon::from(ExpIcon::MessageSquare)
        }
        Some(domain::contract::NOTIFICATION_TYPE_ISSUE_STATUS_CHANGED) => {
            Icon::from(ExpIcon::CircleDot)
        }
        Some(domain::contract::NOTIFICATION_TYPE_PR_OPENED) => Icon::from(ExpIcon::GitPullRequest),
        Some(domain::contract::NOTIFICATION_TYPE_PR_MERGED) => Icon::from(ExpIcon::GitMerge),
        _ => Icon::new(IconName::Bell),
    }
}

impl SidebarPanel {
    pub fn new(window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        let nav = nav_for_window(window, cx);
        let shared = rail_shared_for_window(window, cx);
        let git_bar = shared.read(cx).git_bar.clone();
        let board_all = shared.read(cx).board_all.clone();
        let board_my = shared.read(cx).board_my.clone();
        let flow = cx.new(|cx| crate::flow_view::FlowView::new(window, cx));
        let collections = Store::global(cx).collections().clone();
        let local_sessions = coding_flow::LocalSessions::global(cx);
        let subscriptions = vec![
            // Rail toggles swap the tool window.
            cx.observe(&shared, |_, _, cx| cx.notify()),
            // Session phase — the shared state.
            cx.observe(&Store::global(cx).state(), |_, _, cx| cx.notify()),
            // Query scoping + inbox list are live collection reads.
            cx.observe(&collections.workspaces, |_, _, cx| cx.notify()),
            cx.observe(&collections.projects, |_, _, cx| cx.notify()),
            cx.observe(&collections.issues, |_, _, cx| cx.notify()),
            cx.observe(&collections.notifications, |_, _, cx| cx.notify()),
            // The coding badges ride the coding_sessions shape; the local
            // Start↔Stop flip rides the process-global LocalSessions registry.
            cx.observe(&collections.coding_sessions, |_, _, cx| cx.notify()),
            cx.observe(&local_sessions, |_, _, cx| cx.notify()),
            // Branch list + syncing state ride the shared git bar.
            cx.observe(&git_bar, |_, _, cx| cx.notify()),
            // Active-row highlight follows navigation.
            cx.observe(&nav, |_, _, cx| cx.notify()),
        ];

        Self {
            nav,
            shared,
            board_all,
            board_my,
            review_arm: None,
            review_arm_seq: 0,
            review_merging: HashSet::new(),
            review_error: None,
            open_pulls: None,
            open_pulls_key: None,
            open_pulls_seq: 0,
            flow,
            flow_scroll: ScrollHandle::new(),
            _subscriptions: subscriptions,
        }
    }

    // -- shared chrome -------------------------------------------------------

    /// Shared tool-window title strip (JetBrains tool-window header).
    fn tool_header(
        &self,
        icon: Icon,
        title: &'static str,
        cx: &mut gpui::Context<Self>,
    ) -> gpui::Div {
        h_flex()
            .flex_shrink_0()
            .w_full()
            .h(px(30.))
            .px_3()
            .gap_1p5()
            .items_center()
            .border_b_1()
            .border_color(cx.theme().sidebar_border)
            .text_color(cx.theme().sidebar_foreground.opacity(0.7))
            .child(icon.xsmall())
            .child(
                div()
                    .flex_1()
                    .text_xs()
                    .font_weight(FontWeight::MEDIUM)
                    .child(title),
            )
    }

    fn list_skeleton(&self, _cx: &mut gpui::Context<Self>) -> gpui::AnyElement {
        v_flex()
            .p_3()
            .gap_2()
            .child(Skeleton::new().h_3p5().w_40())
            .child(Skeleton::new().h_3p5().w_48())
            .child(Skeleton::new().h_3p5().w_32())
            .into_any_element()
    }

    fn list_note(
        &self,
        message: impl Into<SharedString>,
        cx: &mut gpui::Context<Self>,
    ) -> gpui::AnyElement {
        div()
            .p_3()
            .text_xs()
            .text_color(cx.theme().muted_foreground)
            .child(message.into())
            .into_any_element()
    }

    // -- issue tool windows ---------------------------------------------------

    fn render_inbox_tool(&mut self, cx: &mut gpui::Context<Self>) -> gpui::AnyElement {
        let data = queries::inbox(cx);
        let header = self
            .tool_header(Icon::new(IconName::Inbox), "Inbox", cx)
            .when(data.total_unread > 0, |this| {
                this.child(
                    Button::new("inbox-mark-all-read")
                        .ghost()
                        .xsmall()
                        .icon(Icon::from(ExpIcon::ListChecks))
                        .tooltip("Mark all read")
                        .on_click(cx.listener(|_, _: &ClickEvent, _, cx| {
                            if let Some(trpc) = queries::trpc_client(cx) {
                                cx.background_executor()
                                    .spawn(async move {
                                        if let Err(err) =
                                            api::notifications::notifications_mark_all_read(&trpc)
                                        {
                                            log::warn!(
                                                "[ui] notifications.markAllRead failed: {err}"
                                            );
                                        }
                                    })
                                    .detach();
                            }
                        })),
                )
            });

        // Single Linear-style activity stream: one row per issue group, the
        // LATEST notification's type icon + sentence. (The old trailing
        // "Needs your review" section moved to the Reviews tool window.)
        let body: gpui::AnyElement = if !data.is_ready {
            self.list_skeleton(cx)
        } else if data.groups.is_empty() {
            self.list_note("All caught up.", cx)
        } else {
            let theme_radius = cx.theme().radius;
            let rows: Vec<gpui::AnyElement> = data
                .groups
                .iter()
                .map(|group| {
                    let theme = cx.theme();
                    let unread = group.unread > 0;
                    let selected = matches!(
                        resolved_screen(&self.nav, cx),
                        Some(Screen::IssueDetail { issue_id }) if issue_id == group.issue.id
                    );
                    let issue_id = group.issue.id.clone();
                    let unread_ids: Vec<String> = group
                        .items
                        .iter()
                        .filter(|n| n.read_at.is_none())
                        .map(|n| n.id.clone())
                        .collect();
                    // Items are newest first — `first()` IS the latest.
                    let latest = group.items.first();
                    let time: SharedString = latest
                        .and_then(|n| n.created_at.as_deref())
                        .map(crate::inbox::relative_time)
                        .unwrap_or_default()
                        .into();
                    // Notification titles are full human sentences ("Danny
                    // merged the pull request for …") — shown verbatim.
                    let sentence: SharedString = latest
                        .and_then(|n| n.title.clone())
                        .unwrap_or_default()
                        .into();
                    let type_icon =
                        notification_type_icon(latest.and_then(|n| n.kind.as_deref()));
                    h_flex()
                        .id(SharedString::from(format!("mini-inbox-{}", group.issue.id)))
                        .w_full()
                        .items_start()
                        .gap_2()
                        .px_2()
                        .py_1p5()
                        .rounded(theme_radius)
                        .when(selected, |this| this.bg(theme.accent.opacity(0.6)))
                        .hover(|this| this.bg(theme.accent.opacity(0.3)))
                        .cursor_pointer()
                        .on_click(cx.listener(move |_, _, window, cx| {
                            // Web `markGroupRead`: clear the group's unreads
                            // (the Electric echo removes the dot), then open.
                            if !unread_ids.is_empty() {
                                if let Some(trpc) = queries::trpc_client(cx) {
                                    let ids = unread_ids.clone();
                                    cx.background_executor()
                                        .spawn(async move {
                                            for id in ids {
                                                if let Err(err) =
                                                    api::notifications::notifications_mark_read(
                                                        &trpc, &id,
                                                    )
                                                {
                                                    log::warn!(
                                                        "[ui] notifications.markRead({id}) failed: {err}"
                                                    );
                                                }
                                            }
                                        })
                                        .detach();
                                }
                            }
                            navigate(
                                window,
                                cx,
                                Screen::IssueDetail {
                                    issue_id: issue_id.clone(),
                                },
                            );
                        }))
                        // Leading circular type badge (the latest item's kind).
                        .child(
                            h_flex()
                                .size_6()
                                .flex_shrink_0()
                                .items_center()
                                .justify_center()
                                .rounded_full()
                                .bg(theme.muted)
                                .child(type_icon.xsmall().text_color(theme.muted_foreground)),
                        )
                        .child(
                            v_flex()
                                .flex_1()
                                .min_w_0()
                                .child(
                                    h_flex()
                                        .w_full()
                                        .items_center()
                                        .gap_1p5()
                                        .child(
                                            div()
                                                .flex_shrink_0()
                                                .text_xs()
                                                .text_color(theme.muted_foreground)
                                                .font_family(theme::terminal::FONT_FAMILY)
                                                .child(SharedString::from(
                                                    group.issue.identifier.clone(),
                                                )),
                                        )
                                        .child(
                                            div()
                                                .flex_1()
                                                .min_w_0()
                                                .text_xs()
                                                .truncate()
                                                .when(unread, |this| {
                                                    this.font_weight(FontWeight::MEDIUM)
                                                })
                                                // Read groups render dimmed.
                                                .text_color(if unread {
                                                    theme.foreground
                                                } else {
                                                    theme.muted_foreground
                                                })
                                                .child(SharedString::from(
                                                    group.issue.title.clone(),
                                                )),
                                        ),
                                )
                                .child(
                                    div()
                                        .w_full()
                                        .text_xs()
                                        .truncate()
                                        .text_color(theme.muted_foreground)
                                        .child(sentence),
                                ),
                        )
                        .child(
                            h_flex()
                                .flex_shrink_0()
                                .items_center()
                                .gap_1p5()
                                .pt_0p5()
                                .child(
                                    div()
                                        .text_xs()
                                        .text_color(theme.muted_foreground)
                                        .child(time),
                                )
                                .child(
                                    div()
                                        .size_2()
                                        .flex_shrink_0()
                                        .rounded_full()
                                        .when(unread, |this| this.bg(theme.primary)),
                                ),
                        )
                        .into_any_element()
                })
                .collect();
            div()
                .id("mini-inbox-scroll")
                .flex_1()
                .min_h_0()
                .overflow_y_scrollbar()
                .child(v_flex().p_1().gap_0p5().children(rows))
                .into_any_element()
        };

        v_flex()
            .flex_1()
            .min_h_0()
            .min_w_0()
            .child(header)
            .child(body)
            .into_any_element()
    }

    /// *My Issues* tool window: the full board pinned to assignee == me
    /// across the workspace (its bar renders the title, tabs and filter).
    fn render_my_issues_tool(&mut self, cx: &mut gpui::Context<Self>) -> gpui::AnyElement {
        let query = match (
            active_workspace_id(&self.nav, cx),
            queries::active_account(cx),
        ) {
            (Some(workspace_id), Some(account)) => IssueQuery::MyIssues {
                workspace_id,
                user_id: account.user_id,
            },
            _ => IssueQuery::None,
        };
        self.board_my.update(cx, |board, cx| board.set_query(query, cx));
        div()
            .flex_1()
            .min_h_0()
            .min_w_0()
            .child(self.board_my.clone())
            .into_any_element()
    }

    /// *All Issues* tool window: the project board, relocated — filter bar
    /// (All/Active/Backlog tabs, filter popover, New Issue) + the grouped
    /// virtualized list with inline status/priority menus.
    fn render_all_issues_tool(&mut self, cx: &mut gpui::Context<Self>) -> gpui::AnyElement {
        let query = match active_project_id(&self.nav, cx) {
            Some(project_id) => IssueQuery::Project { project_id },
            None => IssueQuery::None,
        };
        self.board_all.update(cx, |board, cx| board.set_query(query, cx));
        div()
            .flex_1()
            .min_h_0()
            .min_w_0()
            .child(self.board_all.clone())
            .into_any_element()
    }

    // -- Reviews tool window ----------------------------------------------------

    /// *Reviews* tool window: open pull requests across the workspace, each
    /// mergeable row with a two-click inline merge confirm. Issue-linked PRs
    /// come from the synced issues shape, grouped by project; below them, PRs
    /// NOT linked to anything (manual branches, external contributors) come
    /// from a background `repositories.openPulls` fetch, grouped by repo —
    /// the synced lists never wait on GitHub. Merging goes through the server
    /// (`issues.mergePr` / `repositories.mergePull`, GitHub App squash) —
    /// never local git; synced rows leave the list via the Electric echo,
    /// unlinked pulls are removed locally.
    fn render_reviews_tool(&mut self, cx: &mut gpui::Context<Self>) -> gpui::AnyElement {
        let collections = Store::global(cx).collections().clone();
        let is_ready = collections.issues.read(cx).is_ready()
            && collections.projects.read(cx).is_ready();
        let workspace_id = active_workspace_id(&self.nav, cx);
        if let Some(id) = workspace_id.as_deref() {
            self.ensure_open_pulls(id, cx);
        }
        let groups = workspace_id
            .as_deref()
            .map(|id| queries::review_groups(cx, id))
            .unwrap_or_default();
        let pull_repos: Vec<api::repositories::OpenPullsRepo> = self
            .open_pulls
            .as_ref()
            .filter(|(ws, _)| Some(ws.as_str()) == workspace_id.as_deref())
            .map(|(_, repos)| queries::visible_pull_repos(repos))
            .unwrap_or_default();

        // Rows that merged/closed (or left the workspace scope) drop their
        // transient merge state — this is also where a successful merge's
        // lingering "Merging…" id gets collected once the echo lands.
        {
            let mut live_ids: HashSet<String> = groups
                .iter()
                .flat_map(|group| group.entries.iter())
                .flat_map(|entry| {
                    // Merge/close target the representative id, but keep every
                    // linked issue's id live so no transient state is dropped
                    // while a batch PR is in flight.
                    entry
                        .issues
                        .iter()
                        .flat_map(|issue| [issue.id.clone(), close_pr_key(&issue.id)])
                })
                .collect();
            for repo in &pull_repos {
                for pull in &repo.pulls {
                    live_ids.insert(pull_merge_key(&repo.repository_id, pull.number));
                }
            }
            self.review_merging.retain(|id| live_ids.contains(id));
            if self
                .review_arm
                .as_deref()
                .is_some_and(|id| !live_ids.contains(id))
            {
                self.review_arm = None;
            }
            if self
                .review_error
                .as_ref()
                .is_some_and(|(id, _)| !live_ids.contains(id.as_str()))
            {
                self.review_error = None;
            }
        }

        let header = self.tool_header(Icon::from(ExpIcon::GitPullRequest), "Reviews", cx);

        let body: gpui::AnyElement = if !is_ready {
            self.list_skeleton(cx)
        } else if groups.is_empty() && pull_repos.is_empty() {
            self.list_note("No open pull requests.", cx)
        } else {
            let muted = cx.theme().muted_foreground;
            let mut children: Vec<gpui::AnyElement> = Vec::new();
            for group in &groups {
                let dot = group
                    .project
                    .color
                    .as_deref()
                    .and_then(parse_hex_color)
                    .unwrap_or(muted);
                children.push(
                    h_flex()
                        .px_2()
                        .pt_2()
                        .pb_0p5()
                        .gap_1p5()
                        .items_center()
                        .child(div().size_2().flex_shrink_0().rounded_full().bg(dot))
                        .child(
                            div()
                                .text_xs()
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(muted)
                                .child(SharedString::from(group.project.name.clone())),
                        )
                        .into_any_element(),
                );
                for entry in &group.entries {
                    children.push(self.review_row(entry, cx));
                }
            }
            for repo in &pull_repos {
                children.push(
                    h_flex()
                        .px_2()
                        .pt_2()
                        .pb_0p5()
                        .gap_1p5()
                        .items_center()
                        .child(
                            Icon::from(ExpIcon::GitPullRequest)
                                .xsmall()
                                .flex_shrink_0()
                                .text_color(muted),
                        )
                        .child(
                            div()
                                .min_w_0()
                                .text_xs()
                                .truncate()
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(muted)
                                .child(SharedString::from(repo.full_name.clone())),
                        )
                        .child(
                            div()
                                .flex_shrink_0()
                                .text_xs()
                                .text_color(muted.opacity(0.8))
                                .child(SharedString::from(format!(
                                    "not linked to an issue \u{00B7} {}",
                                    repo.pulls.len()
                                ))),
                        )
                        .into_any_element(),
                );
                for pull in &repo.pulls {
                    children.push(self.pull_row(&repo.repository_id, pull, cx));
                }
            }
            div()
                .id("reviews-scroll")
                .flex_1()
                .min_h_0()
                .overflow_y_scrollbar()
                .child(v_flex().p_1().gap_0p5().children(children))
                .into_any_element()
        };

        v_flex()
            .flex_1()
            .min_h_0()
            .min_w_0()
            .child(header)
            .child(body)
            .into_any_element()
    }

    /// One Reviews row for a PR entry: PR icon + identifier + title with a
    /// trailing Merge button, sub-line `#N · branch`, optional error caption.
    /// A single-issue entry shows the issue identifier + title; a BATCH entry
    /// (EXP-131: N issues on ONE PR) shows `#<pr_number>`, a "N issues" count,
    /// and the linked identifiers in place of the title. Merge/× act on the
    /// representative issue's id — the server merges the ONE PR and completes
    /// every linked issue. Clicking the row opens the representative issue's
    /// detail (its Changes tab shows the diff). The subtle ghost `×` left of
    /// Merge closes the PR WITHOUT merging (EXP-100: the reject path) — same
    /// two-click confirm, `issues.closePr`.
    fn review_row(
        &self,
        entry: &queries::ReviewEntry,
        cx: &mut gpui::Context<Self>,
    ) -> gpui::AnyElement {
        let issue = entry.representative();
        let is_batch = entry.is_batch();
        // Batch: `#<pr_number>` (all linked issues share one PR); single: the
        // issue identifier. Batch title = the linked identifiers; single =
        // the issue title. The "N issues" count renders only for batches.
        let identifier_text = if is_batch {
            match issue.pr_number {
                Some(number) => format!("#{number}"),
                None => issue.identifier.clone(),
            }
        } else {
            issue.identifier.clone()
        };
        let title_text = if is_batch {
            entry
                .issues
                .iter()
                .map(|i| i.identifier.clone())
                .collect::<Vec<_>>()
                .join(", ")
        } else {
            issue.title.clone()
        };
        let batch_count = is_batch.then(|| format!("{} issues", entry.issues.len()));

        let theme = cx.theme();
        let radius = theme.radius;
        let fg = theme.foreground;
        let muted = theme.muted_foreground;
        let accent = theme.accent;
        let danger = theme.danger;
        // Open-PR green (the token the status/priority accents use).
        let pr_green = theme::tokens::GREEN.to_hsla();

        let selected = matches!(
            resolved_screen(&self.nav, cx),
            Some(Screen::IssueDetail { issue_id }) if issue_id == issue.id
        );
        let merging = self.review_merging.contains(&issue.id);
        let armed = self.review_arm.as_deref() == Some(issue.id.as_str());
        let close_key = close_pr_key(&issue.id);
        let closing = self.review_merging.contains(&close_key);
        let close_armed = self.review_arm.as_deref() == Some(close_key.as_str());
        let error: Option<String> = self
            .review_error
            .as_ref()
            .filter(|(id, _)| *id == issue.id)
            .map(|(_, message)| message.clone());

        let sub: String = match (issue.pr_number, issue.branch.as_deref()) {
            (Some(number), Some(branch)) => format!("#{number} \u{00B7} {branch}"),
            (Some(number), None) => format!("#{number}"),
            (None, Some(branch)) => branch.to_string(),
            (None, None) => String::new(),
        };

        let merge_button = {
            let mut button = Button::new(SharedString::from(format!("review-merge-{}", issue.id)))
                .xsmall()
                .outline();
            if merging {
                button = button.label("Merging…").loading(true).disabled(true);
            } else if armed {
                button = button.label("Confirm merge").danger();
            } else {
                button = button.label("Merge").disabled(closing);
            }
            let click_id = issue.id.clone();
            button.on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                cx.stop_propagation();
                this.on_merge_click(click_id.clone(), cx);
            }))
        };

        // The reject path — intentionally quiet next to Merge: a muted ghost
        // `×` that only grows into a labeled danger confirm once armed.
        let close_button = {
            let mut button = Button::new(SharedString::from(format!("review-close-{}", issue.id)))
                .xsmall()
                .ghost();
            if closing {
                button = button
                    .icon(Icon::new(IconName::Close))
                    .loading(true)
                    .disabled(true);
            } else if close_armed {
                button = button.label("Close PR").danger();
            } else {
                button = button
                    .icon(Icon::new(IconName::Close).text_color(muted))
                    .tooltip("Close PR without merging")
                    .disabled(merging);
            }
            let click_id = issue.id.clone();
            button.on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                cx.stop_propagation();
                this.on_close_pr_click(click_id.clone(), cx);
            }))
        };

        let nav_id = issue.id.clone();
        v_flex()
            .id(SharedString::from(format!("review-{}", issue.id)))
            .w_full()
            .px_2()
            .py_1()
            .gap_0p5()
            .rounded(radius)
            .when(selected, |this| this.bg(accent.opacity(0.6)))
            .hover(|this| this.bg(accent.opacity(0.3)))
            .cursor_pointer()
            .on_click(cx.listener(move |this, _, window, cx| {
                // Any click outside the armed button disarms the confirm.
                if this.review_arm.is_some() {
                    this.review_arm = None;
                    this.review_arm_seq += 1;
                    cx.notify();
                }
                // A PR row's payload is the diff — land on the Changes tab.
                crate::navigation::navigate_issue_changes(window, cx, nav_id.clone());
            }))
            .child(
                h_flex()
                    .w_full()
                    .items_center()
                    .gap_1p5()
                    .child(
                        Icon::from(ExpIcon::GitPullRequest)
                            .xsmall()
                            .flex_shrink_0()
                            .text_color(pr_green),
                    )
                    .child(
                        div()
                            .flex_shrink_0()
                            .text_xs()
                            .text_color(muted)
                            .font_family(theme::terminal::FONT_FAMILY)
                            .child(SharedString::from(identifier_text)),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_xs()
                            .truncate()
                            .text_color(fg)
                            .child(SharedString::from(title_text)),
                    )
                    .when_some(batch_count, |this, count| {
                        this.child(
                            div()
                                .flex_shrink_0()
                                .text_xs()
                                .text_color(muted)
                                .child(SharedString::from(count)),
                        )
                    })
                    .child(close_button)
                    .child(merge_button),
            )
            .child(
                div()
                    .pl_5()
                    .text_xs()
                    .truncate()
                    .text_color(muted)
                    .child(SharedString::from(sub)),
            )
            .when_some(error, |this, message| {
                this.child(
                    div()
                        .pl_5()
                        .text_xs()
                        .truncate()
                        .text_color(danger)
                        .child(SharedString::from(message)),
                )
            })
            .into_any_element()
    }

    /// The Merge button's two-click flow: first click arms (auto-disarm after
    /// ~5s), second click fires `issues.mergePr` on the background executor.
    /// Failures come back to a caption under the row; success leaves the
    /// spinner until the Electric echo removes the row.
    fn on_merge_click(&mut self, issue_id: String, cx: &mut gpui::Context<Self>) {
        // Ignore while either action on this row is already in flight.
        if self.review_merging.contains(&issue_id)
            || self.review_merging.contains(&close_pr_key(&issue_id))
        {
            return;
        }
        if self.review_arm.as_deref() != Some(issue_id.as_str()) {
            self.arm_merge_confirm(issue_id, cx);
            return;
        }

        // Confirmed — fire the server-side squash merge.
        self.review_arm = None;
        self.review_arm_seq += 1;
        self.review_error = None;
        let Some(trpc) = queries::trpc_client(cx) else {
            log::warn!("[ui] issues.mergePr skipped: no active account");
            cx.notify();
            return;
        };
        self.review_merging.insert(issue_id.clone());
        cx.notify();
        cx.spawn(async move |this, cx| {
            let call_id = issue_id.clone();
            let result = cx
                .background_executor()
                .spawn(async move { api::issues::merge_pr(&trpc, &call_id) })
                .await;
            let _ = this.update(cx, |this, cx| {
                if let Err(err) = result {
                    log::warn!("[ui] issues.mergePr({issue_id}) failed: {err}");
                    // Show the server's user-facing message when there is
                    // one; transport-level errors keep the full rendering.
                    let message = match err {
                        api::ApiError::Http { message, .. } => message,
                        other => other.to_string(),
                    };
                    this.review_merging.remove(&issue_id);
                    this.review_error = Some((issue_id.clone(), message));
                    cx.notify();
                }
                // Success: nothing to do — the collection observer re-renders
                // when the echo flips `pr_state` and the row leaves the list.
            });
        })
        .detach();
    }

    /// The subtle Close-PR button's two-click flow (EXP-100): first click
    /// arms (auto-disarm after ~5s), second click fires `issues.closePr` on
    /// the background executor — closes the PR on GitHub WITHOUT merging.
    /// Failures caption the row; success leaves the spinner until the
    /// Electric echo flips `pr_state` to closed and the row leaves the list.
    fn on_close_pr_click(&mut self, issue_id: String, cx: &mut gpui::Context<Self>) {
        let key = close_pr_key(&issue_id);
        // Ignore while either action on this row is already in flight.
        if self.review_merging.contains(&key) || self.review_merging.contains(&issue_id) {
            return;
        }
        if self.review_arm.as_deref() != Some(key.as_str()) {
            self.arm_merge_confirm(key, cx);
            return;
        }

        // Confirmed — fire the server-side close.
        self.review_arm = None;
        self.review_arm_seq += 1;
        self.review_error = None;
        let Some(trpc) = queries::trpc_client(cx) else {
            log::warn!("[ui] issues.closePr skipped: no active account");
            cx.notify();
            return;
        };
        self.review_merging.insert(key.clone());
        cx.notify();
        cx.spawn(async move |this, cx| {
            let call_id = issue_id.clone();
            let result = cx
                .background_executor()
                .spawn(async move { api::issues::close_pr(&trpc, &call_id) })
                .await;
            let _ = this.update(cx, |this, cx| {
                if let Err(err) = result {
                    log::warn!("[ui] issues.closePr({issue_id}) failed: {err}");
                    // Show the server's user-facing message when there is
                    // one; transport-level errors keep the full rendering.
                    let message = match err {
                        api::ApiError::Http { message, .. } => message,
                        other => other.to_string(),
                    };
                    this.review_merging.remove(&key);
                    // Error captions key on the ROW (the issue id), not the
                    // close key — the caption renders under the row either way.
                    this.review_error = Some((issue_id.clone(), message));
                    cx.notify();
                }
                // Success: nothing to do — the collection observer re-renders
                // when the echo flips `pr_state` and the row leaves the list.
            });
        })
        .detach();
    }

    /// First click of the two-click merge confirm: arm `key` (an issue id or
    /// an unlinked-pull key) and start the ~5s auto-disarm timer.
    fn arm_merge_confirm(&mut self, key: String, cx: &mut gpui::Context<Self>) {
        self.review_arm = Some(key);
        self.review_arm_seq += 1;
        let seq = self.review_arm_seq;
        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(std::time::Duration::from_secs(5))
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.review_arm_seq == seq && this.review_arm.is_some() {
                    this.review_arm = None;
                    cx.notify();
                }
            });
        })
        .detach();
        cx.notify();
    }

    /// Kick the `repositories.openPulls` fetch when the Reviews tool window
    /// is shown or the workspace changes — never on a timer (the server
    /// caches ~60s). Data from another workspace is dropped immediately; a
    /// reopen in the same workspace keeps rendering the previous result while
    /// the refresh is in flight.
    fn ensure_open_pulls(&mut self, workspace_id: &str, cx: &mut gpui::Context<Self>) {
        if self.open_pulls_key.as_deref() == Some(workspace_id) {
            return;
        }
        self.open_pulls_key = Some(workspace_id.to_string());
        if self
            .open_pulls
            .as_ref()
            .is_some_and(|(ws, _)| ws != workspace_id)
        {
            self.open_pulls = None;
        }
        self.open_pulls_seq += 1;
        let seq = self.open_pulls_seq;
        let Some(trpc) = queries::trpc_client(cx) else {
            return;
        };
        let ws = workspace_id.to_string();
        cx.spawn(async move |this, cx| {
            let call_ws = ws.clone();
            let result = cx
                .background_executor()
                .spawn(async move { api::repositories::open_pulls(&trpc, &call_ws) })
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.open_pulls_seq != seq {
                    return;
                }
                match result {
                    Ok(repos) => {
                        this.open_pulls = Some((ws, repos));
                        cx.notify();
                    }
                    Err(err) => {
                        // The synced rows still render; the unlinked section
                        // just stays absent (same degradation as the web).
                        log::warn!("[ui] repositories.openPulls failed: {err}");
                    }
                }
            });
        })
        .detach();
    }

    /// One unlinked-PR row: `#N` + title with a trailing Merge button
    /// (disabled for drafts — GitHub refuses those), sub-line
    /// `branch → base`, optional Draft pill and error caption. Clicking the
    /// row opens the PR on GitHub — no local detail exists behind these.
    fn pull_row(
        &self,
        repository_id: &str,
        pull: &api::repositories::OpenPull,
        cx: &mut gpui::Context<Self>,
    ) -> gpui::AnyElement {
        let theme = cx.theme();
        let radius = theme.radius;
        let fg = theme.foreground;
        let muted = theme.muted_foreground;
        let accent = theme.accent;
        let danger = theme.danger;
        let pr_green = theme::tokens::GREEN.to_hsla();

        let key = pull_merge_key(repository_id, pull.number);
        let merging = self.review_merging.contains(&key);
        let armed = self.review_arm.as_deref() == Some(key.as_str());
        let error: Option<String> = self
            .review_error
            .as_ref()
            .filter(|(id, _)| *id == key)
            .map(|(_, message)| message.clone());

        let sub = format!("{} \u{2192} {}", pull.branch, pull.base_branch);

        let merge_button = {
            let mut button = Button::new(SharedString::from(format!("pull-merge-{key}")))
                .xsmall()
                .outline();
            if merging {
                button = button.label("Merging…").loading(true).disabled(true);
            } else if pull.draft {
                button = button.label("Merge").disabled(true);
            } else if armed {
                button = button.label("Confirm merge").danger();
            } else {
                button = button.label("Merge");
            }
            let click_repo = repository_id.to_string();
            let number = pull.number;
            button.on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                cx.stop_propagation();
                this.on_pull_merge_click(click_repo.clone(), number, cx);
            }))
        };

        let url = pull.url.clone();
        v_flex()
            .id(SharedString::from(format!("pull-{key}")))
            .w_full()
            .px_2()
            .py_1()
            .gap_0p5()
            .rounded(radius)
            .hover(|this| this.bg(accent.opacity(0.3)))
            .cursor_pointer()
            .on_click(cx.listener(move |this, _, _, cx| {
                // Any click outside the armed button disarms the confirm.
                if this.review_arm.is_some() {
                    this.review_arm = None;
                    this.review_arm_seq += 1;
                    cx.notify();
                }
                crate::settings::open_url(cx, url.clone());
            }))
            .child(
                h_flex()
                    .w_full()
                    .items_center()
                    .gap_1p5()
                    .child(
                        Icon::from(ExpIcon::GitPullRequest)
                            .xsmall()
                            .flex_shrink_0()
                            .text_color(pr_green),
                    )
                    .child(
                        div()
                            .flex_shrink_0()
                            .text_xs()
                            .text_color(muted)
                            .font_family(theme::terminal::FONT_FAMILY)
                            .child(SharedString::from(format!("#{}", pull.number))),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_xs()
                            .truncate()
                            .text_color(fg)
                            .child(SharedString::from(pull.title.clone())),
                    )
                    .when(pull.draft, |this| {
                        this.child(
                            div()
                                .flex_shrink_0()
                                .px_1()
                                .rounded(radius)
                                .bg(muted.opacity(0.15))
                                .text_xs()
                                .text_color(muted)
                                .child("Draft"),
                        )
                    })
                    .child(merge_button),
            )
            .child(
                div()
                    .pl_5()
                    .text_xs()
                    .truncate()
                    .text_color(muted)
                    .child(SharedString::from(sub)),
            )
            .when_some(error, |this, message| {
                this.child(
                    div()
                        .pl_5()
                        .text_xs()
                        .truncate()
                        .text_color(danger)
                        .child(SharedString::from(message)),
                )
            })
            .into_any_element()
    }

    /// The unlinked-pull Merge flow: same two-click confirm as issue rows,
    /// firing `repositories.mergePull`. There is no Electric echo — success
    /// removes the pull from the fetched state; failures caption the row.
    fn on_pull_merge_click(
        &mut self,
        repository_id: String,
        number: u64,
        cx: &mut gpui::Context<Self>,
    ) {
        let key = pull_merge_key(&repository_id, number);
        if self.review_merging.contains(&key) {
            return;
        }
        if self.review_arm.as_deref() != Some(key.as_str()) {
            self.arm_merge_confirm(key, cx);
            return;
        }

        // Confirmed — fire the server-side squash merge.
        self.review_arm = None;
        self.review_arm_seq += 1;
        self.review_error = None;
        let Some(trpc) = queries::trpc_client(cx) else {
            log::warn!("[ui] repositories.mergePull skipped: no active account");
            cx.notify();
            return;
        };
        self.review_merging.insert(key.clone());
        cx.notify();
        cx.spawn(async move |this, cx| {
            let call_repo = repository_id.clone();
            let result = cx
                .background_executor()
                .spawn(async move { api::repositories::merge_pull(&trpc, &call_repo, number) })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.review_merging.remove(&key);
                match result {
                    Ok(_) => {
                        if let Some((_, repos)) = this.open_pulls.as_mut() {
                            queries::remove_merged_pull(repos, &repository_id, number);
                        }
                    }
                    Err(err) => {
                        log::warn!(
                            "[ui] repositories.mergePull({repository_id}#{number}) failed: {err}"
                        );
                        // Show the server's user-facing message when there is
                        // one; transport-level errors keep the full rendering.
                        let message = match err {
                            api::ApiError::Http { message, .. } => message,
                            other => other.to_string(),
                        };
                        this.review_error = Some((key.clone(), message));
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    // -- Files tool window ----------------------------------------------------

    /// *Files* tool window: the trunk file tree at full panel height.
    fn render_files_tool(&mut self, cx: &mut gpui::Context<Self>) -> gpui::AnyElement {
        let file_tree = self.shared.read(cx).file_tree.clone();
        let refresh_tree = file_tree.clone();
        v_flex()
            .flex_1()
            .min_h_0()
            .min_w_0()
            .child(
                self.tool_header(Icon::new(IconName::Folder), "Files", cx).child(
                    Button::new("files-refresh")
                        .ghost()
                        .xsmall()
                        .icon(Icon::from(ExpIcon::Repeat))
                        .tooltip("Refresh")
                        .on_click(move |_, _, cx| {
                            refresh_tree.update(cx, |tree, cx| tree.refresh(cx));
                        }),
                ),
            )
            .child(div().flex_1().min_h_0().child(file_tree))
            .into_any_element()
    }

    // -- Source Control tool window --------------------------------------------

    /// *Source Control* tool window: the trunk's local branches (from the
    /// shared git bar — refreshed with every trunk read), current one
    /// checked. Clicking a row VIEWS that branch's history in the changes
    /// screen — never a checkout (that stays on the git bar's branch chip,
    /// the one dirty-switch dialog surface).
    fn render_source_control_tool(&mut self, cx: &mut gpui::Context<Self>) -> gpui::AnyElement {
        let git_bar = self.shared.read(cx).git_bar.clone();
        // The sweep button (EXP-93) only lights up when merged lanes exist —
        // the candidate probe is the same cheap pure join the flow renders
        // from (no git cost).
        let sweepable = self
            .flow
            .update(cx, |flow, cx| !flow.sweep_candidates(cx).is_empty());
        let sweep_busy = self.flow.read(cx).is_busy();
        let flow = self.flow.clone();
        let header = self
            .tool_header(Icon::from(ExpIcon::GitMerge), "Source Control", cx)
            .child(
                Button::new("branches-sweep")
                    .ghost()
                    .xsmall()
                    .icon(Icon::from(ExpIcon::BrushCleaning))
                    .tooltip(
                        "Sweep merged branches — delete their worktrees and local \
                         branches (worktrees with uncommitted changes are skipped)…",
                    )
                    .disabled(!sweepable || sweep_busy)
                    .on_click(move |_, window, cx| {
                        flow.update(cx, |flow, cx| flow.prompt_sweep_merged(window, cx));
                    }),
            )
            .child(
                Button::new("branches-refresh")
                    .ghost()
                    .xsmall()
                    .icon(Icon::from(ExpIcon::Repeat))
                    .tooltip("Refresh")
                    .on_click(move |_, _, cx| {
                        git_bar.update(cx, |bar, cx| bar.refresh(cx));
                    }),
            );

        // The branch-flow graph replaced the flat branch list — one surface
        // for "what hangs off what", with view-on-click and hover-delete
        // ([`crate::flow_view`]).
        v_flex()
            .flex_1()
            .min_h_0()
            .min_w_0()
            .child(header)
            .child(crate::scroll_pane::v_scroll_pane(
                "flow-scroll",
                &self.flow_scroll,
                self.flow.clone(),
            ))
            .into_any_element()
    }
}

impl Render for SidebarPanel {
    fn render(&mut self, _window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let tool = self.shared.read(cx).tool;
        // Leaving the Reviews tool drops the openPulls fetch key so the next
        // open refetches (the server cache keeps that cheap).
        if tool != ToolWindow::Reviews {
            self.open_pulls_key = None;
        }
        v_flex()
            .size_full()
            .min_w_0()
            .overflow_hidden()
            .bg(cx.theme().tokens.sidebar)
            .text_color(cx.theme().sidebar_foreground)
            .border_r_1()
            .border_color(cx.theme().sidebar_border)
            .child(match tool {
                ToolWindow::Inbox => self.render_inbox_tool(cx),
                ToolWindow::MyIssues => self.render_my_issues_tool(cx),
                ToolWindow::AllIssues => self.render_all_issues_tool(cx),
                ToolWindow::Reviews => self.render_reviews_tool(cx),
                ToolWindow::Files => self.render_files_tool(cx),
                ToolWindow::SourceControl => self.render_source_control_tool(cx),
            })
            .into_any_element()
    }
}
