//! The team sidebar (masterplan-v3 §4.2, reworked as a JetBrains-style
//! tool-window rail).
//!
//! Two cooperating views share per-window state through [`RailShared`]:
//!
//! - [`RailView`] — the [`crate::shell::LEFT_COLUMN_WIDTH`]-wide sidebar
//!   owned by the `Shell` and
//!   rendered OUTSIDE the `DockArea`, full window height. EXP-723 made it the
//!   web sidebar's twin and removed the collapse entirely (no icon strip, no
//!   toggle, no logo). Top: the team switcher + Search + New issue header
//!   ([`render_left_column_header`], rendered FIXED by the `Shell` since
//!   EXP-863). Middle (scrolling): the tool-window
//!   selectors — **Inbox / Support / Devices / Actions / Automations /
//!   Reviews / Agent**, the team's boards, the **Sessions** section (EXP-791:
//!   one row per open session tab or live run of the caller's — the rail is
//!   the ONE navigation for coding sessions; hidden while empty), then
//!   **This device**: **Files / Source Control** (Source Control carries an
//!   amber badge while the trunk needs attention — a paused conflict, local
//!   commits, or a dirty tree, EXP-346 — and opens the changes screen
//!   immediately). Glyphs stay WHITE selected or not (EXP-635); the row
//!   fill IS the selection, and (EXP-851) an entry reads selected while ITS
//!   screen is up. Bottom: the "What's new" card, the muted Getting-started row,
//!   the sync spinner, then the account button with the new-terminal button
//!   and the settings gear on its right. The account dropdown is the web's
//!   exactly: What's new, About, Sign out (team switching lives in the
//!   header).
//! - [`ListPanel`] — the team's LIST surfaces (a board, the Inbox, Support).
//!   EXP-851: it renders EITHER as the full-width main view
//!   ([`ListMode::Screen`], the list screen a rail entry navigates to) or as
//!   the `ListNav` in the left column ([`ListMode::Nav`], the
//!   simplified list beside an open detail). One type either way, so the
//!   Support poll, the board query and the inbox grouping exist once.
//!
//! Every affordance dispatches a typed action (§3.6) or navigates directly;
//! menus render in the Root overlay, outside this element tree.

use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use gpui::{
    div, prelude::FluentBuilder as _, px, size, App, AppContext as _, ClickEvent, Entity,
    FontWeight, Hsla, InteractiveElement as _, IntoElement, ParentElement, Pixels, Render,
    ScrollHandle, ScrollStrategy, SharedString, Size, StatefulInteractiveElement as _, Styled,
    Subscription, Window, WindowId,
};
use gpui_component::{
    button::{Button, ButtonVariants as _},
    h_flex,
    menu::{ContextMenuExt as _, DropdownMenu as _},
    scroll::ScrollableElement as _,
    spinner::Spinner,
    v_flex, v_virtual_list, ActiveTheme as _, Icon, Selectable as _, Sizable as _,
    VirtualListScrollHandle,
};
use sync::Store;

use domain::rows::Issue;
use domain::statuses::ResolvedStatus;


// EXP-282: `OpenSettings` is gone from this file — the rail gear navigates
// directly (EXP-17) and the account dropdown no longer duplicates it.
use crate::actions::{CreateTeam, JoinTeam, OpenAbout, OpenWhatsNew, SignOut, SwitchTeam};
use crate::board::BoardView;
use crate::coding_flow;
use crate::controls::WebControl as _;
use crate::trunk_sync::TrunkSync;
use crate::icons::{self, registry, ExpIcon};
use crate::issue_list::{
    build_row_context_menu, control_cell, render_bulk_bar, row_id, selection_range,
    status_dropdown, BulkSelectionHost, IssueQuery,
};
use crate::navigation::{
    active_board_id, active_team_id, nav_for_window, navigate, resolved_screen, switch_team,
    GettingStartedTab, Navigation, Screen, TabOrigin,
};
use crate::issue_header::parse_hex_color;
use crate::queries;

/// EXP-851: which LIST a detail was opened from — the left column's
/// `ListNav` occupant, and the kind half of [`crate::navigation::TabOrigin`].
///
/// It used to name the rail's active TOOL WINDOW (a docked column beside the
/// centre). There is no tool column anymore: every one of these is a
/// full-width SCREEN ([`Screen::BoardIssues`], [`Screen::Inbox`],
/// [`Screen::Support`], [`Screen::Files`], [`Screen::SourceControl`],
/// [`Screen::Chat`], [`Screen::Reviews`]), and the enum survives only as the
/// origin vocabulary — `origin_screen` maps each back to its screen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ToolWindow {
    /// The merged personal tool window (EXP-186): an Inbox tab (notification
    /// groups; rows open the issue detail) + a My Issues tab (issues assigned
    /// to me across the team) — ONE rail entry, mirroring mobile's segmented
    /// My Work screen. The active tab is [`RailShared::inbox_tab`].
    Inbox,
    /// The active board's issue list (mini list) — the default tool.
    /// Selected via the rail's Projects board icons (no icon of its own).
    BoardIssues,
    /// Support tickets of the active team (EXP-180 — server-only tRPC data,
    /// polled). The rail icon renders only while the active team's synced
    /// `helpdesk_enabled` flag is on.
    Support,
    /// The trunk file tree at full panel height.
    Files,
    /// The trunk's local branches; activating also opens the changes screen.
    SourceControl,
    /// EXP-851: the Reviews page's rows — a PR diff opened from there keeps
    /// the queue beside it.
    Reviews,
    /// EXP-862: the Automations page's "Recent automated runs" log — opening
    /// a finished automated run keeps that log beside it, and its Back goes
    /// there rather than to the Agent page (an unattended run has no row on
    /// the Agent page's lists).
    Automations,
    /// EXP-981: the Workflows page's list — opening a workflow keeps it in
    /// the left column, and the detail's Back goes there.
    Workflows,
}

impl ToolWindow {
    /// EXP-851: the SCREEN this list is — what its rail entry navigates to,
    /// what a `ListNav` back row hops out to, and what the legacy
    /// `activate_tool` callers mean. `board` supplies the window's active
    /// board for the board list (the only kind that needs an argument);
    /// `None` there yields the dev sentinel, resolved at render time.
    pub(crate) fn origin_screen(self, board: Option<String>) -> Screen {
        match self {
            ToolWindow::Inbox => Screen::Inbox {
                tab: InboxTab::Inbox,
            },
            ToolWindow::BoardIssues => Screen::BoardIssues {
                board_id: board.unwrap_or_default(),
            },
            ToolWindow::Support => Screen::Support,
            ToolWindow::Files => Screen::Files,
            ToolWindow::SourceControl => Screen::SourceControl,
            ToolWindow::Reviews => Screen::Reviews,
            ToolWindow::Automations => Screen::Automations,
            ToolWindow::Workflows => Screen::Workflows,
        }
    }

    /// EXP-851: the `ListNav` back row's label — the list the detail came
    /// from, named the way its rail entry is. A board names ITSELF (the
    /// caller passes the synced board name), so it degrades generically here.
    pub(crate) fn list_label(self) -> &'static str {
        match self {
            ToolWindow::Inbox => "Inbox",
            ToolWindow::BoardIssues => "Board",
            ToolWindow::Support => "Support",
            ToolWindow::Files => "Files",
            ToolWindow::SourceControl => "Source Control",
            ToolWindow::Reviews => "Reviews",
            ToolWindow::Automations => "Automations",
            ToolWindow::Workflows => domain::workflow_view::WORKFLOWS_TITLE,
        }
    }
}

/// The Inbox screen's active tab (EXP-186). EXP-851: it rides
/// [`Screen::Inbox`] and the tab origin, so a go-back and a tab restore land
/// on the tab the user was on.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum InboxTab {
    /// The notification stream.
    Inbox,
    /// The My Issues board (assignee == me across the team).
    MyIssues,
}

/// Per-window state both rail and tool-window panel read: which tool window
/// is active plus the shared repo-backed entities. Lives in a window-keyed
/// registry (same pattern as `navigation::nav_for_window`) because the views
/// are constructed on different paths.
pub(crate) struct RailShared {
    /// The headless trunk-sync engine (EXP-253 — nothing renders it; the
    /// rail paints a status badge off its state). Driven every rail render
    /// so the §4.1 auto-clone lifecycle and the rail's sync/conflict badge
    /// stay live regardless of the visible screen.
    git_bar: Entity<TrunkSync>,
    file_tree: Entity<crate::file_tree::FileTreeView>,
    /// The Board Issues tool window's board (filter bar + grouped list,
    /// scoped to the active board). Shared here — not on `ListPanel` —
    /// so the issue detail's prev/next switcher (EXP-48) can read the same
    /// query + filter state the visible list applies.
    board_active: Entity<BoardView>,
    /// The "My Issues" board (assignee == me across the team).
    board_my: Entity<BoardView>,
    /// What the Source Control screen's diff pane shows — the sidebar
    /// history list selects it (EXP-253; EXP-509 added the working tree).
    sc_selection: ScSelection,
    /// EXP-288: the trunk-relative file the center file viewer shows — the
    /// Files tree selects it (files are NOT tabs anymore; the viewer is the
    /// Files tool's center content, like the SC diff follows
    /// `sc_selected_commit`). `None` = nothing selected.
    selected_file: Option<String>,
    /// EXP-282: the settings nav's selected section. Lives here (not on
    /// `SettingsView`) because the nav column now renders OUTSIDE the settings
    /// screen — it replaces the tool column while a settings screen is up, so
    /// the column and the detail view must read one selection.
    settings_section: crate::settings::SettingsSection,
}

impl RailShared {
    /// The shared headless trunk-sync engine.
    pub(crate) fn trunk_sync(&self) -> &Entity<TrunkSync> {
        &self.git_bar
    }

    /// The window's trunk file tree (EXP-851: the Files SCREEN renders it).
    pub(crate) fn file_tree(&self) -> Entity<crate::file_tree::FileTreeView> {
        self.file_tree.clone()
    }

    /// The sidebar history list's selection (EXP-253/EXP-509).
    pub(crate) fn sc_selection(&self) -> &ScSelection {
        &self.sc_selection
    }

    /// The selected commit hash, when the selection IS a commit (the history
    /// row highlight).
    pub(crate) fn sc_selected_commit(&self) -> Option<&str> {
        match &self.sc_selection {
            ScSelection::Commit(hash) => Some(hash),
            _ => None,
        }
    }

    /// Drop the selection (a Source Control scope change invalidated it).
    pub(crate) fn clear_sc_selection(&mut self, cx: &mut gpui::Context<Self>) {
        if self.sc_selection != ScSelection::None {
            self.sc_selection = ScSelection::None;
            cx.notify();
        }
    }

    /// The Files tree's file selection (EXP-288 — drives the center file
    /// viewer while the Files tool is active).
    pub(crate) fn selected_file(&self) -> Option<&str> {
        self.selected_file.as_deref()
    }

    /// Drop the file selection (a board/team scope change invalidated the
    /// trunk-relative path).
    pub(crate) fn clear_selected_file(&mut self, cx: &mut gpui::Context<Self>) {
        if self.selected_file.is_some() {
            self.selected_file = None;
            cx.notify();
        }
    }

    /// EXP-282: the settings nav's selected section (raw — callers clamp it
    /// through `settings::effective_selection`).
    pub(crate) fn settings_section(&self) -> crate::settings::SettingsSection {
        self.settings_section.clone()
    }
}

/// Select `section` in the settings nav (EXP-282 — the nav column lives
/// outside the settings screen now, so the selection is window state).
pub(crate) fn select_settings_section(
    window: &mut Window,
    cx: &mut App,
    section: crate::settings::SettingsSection,
) {
    let shared = rail_shared_for_window(window, cx);
    shared.update(cx, |shared, cx| {
        if shared.settings_section != section {
            shared.settings_section = section;
            cx.notify();
        }
    });
}

/// What the Source Control screen's diff pane shows (EXP-253 commit
/// selection; EXP-509 added the uncommitted working tree).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) enum ScSelection {
    /// Nothing picked (the pane's placeholder).
    #[default]
    None,
    /// One history commit's diff (`git show <hash>`).
    Commit(String),
    /// The uncommitted working tree (`git diff HEAD` + untracked files) —
    /// the history list's synthetic top row (EXP-509).
    WorkingTree,
}

/// Point the Source Control screen's diff pane at `selection` (the sidebar
/// history list's click target).
pub(crate) fn set_sc_selection(window: &mut Window, cx: &mut App, selection: ScSelection) {
    let shared = rail_shared_for_window(window, cx);
    shared.update(cx, |shared, cx| {
        if shared.sc_selection != selection {
            shared.sc_selection = selection;
            cx.notify();
        }
    });
}

/// Point the center file viewer at a trunk-relative `path` (EXP-288 — the
/// Files tree's click target); `None` clears the selection.
pub(crate) fn select_file(window: &mut Window, cx: &mut App, path: Option<String>) {
    let shared = rail_shared_for_window(window, cx);
    shared.update(cx, |shared, cx| {
        if shared.selected_file != path {
            shared.selected_file = path;
            cx.notify();
        }
    });
}

/// DEV-ONLY `EXP_DEV_SETTINGS` values: the [`crate::settings::SettingsSection`]
/// variants in kebab form, plus `board:<uuid>` for one board's pane. The
/// settings screen clamps a section the signed-in user cannot see
/// (`settings::effective_selection`), so an owner-only value on a member
/// account still lands on the fallback pane rather than an empty one.
fn parse_dev_settings_section(spec: &str) -> Option<crate::settings::SettingsSection> {
    use crate::settings::SettingsSection as S;
    match spec {
        "general" => Some(S::General),
        "members" => Some(S::Members),
        "labels" => Some(S::Labels),
        "statuses" => Some(S::Statuses),
        "storage" => Some(S::Storage),
        "archived-boards" => Some(S::ArchivedBoards),
        "repositories" => Some(S::Repositories),
        "tools" => Some(S::Tools),
        "agents" => Some(S::Agents),
        "local-repos" => Some(S::LocalRepos),
        "sessions" => Some(S::Sessions),
        "account" => Some(S::Account),
        "notifications" => Some(S::Notifications),
        "api-keys" => Some(S::ApiKeys),
        "mcp-servers" => Some(S::McpServers),
        "about" => Some(S::About),
        _ => spec.strip_prefix("board:").map(|id| S::Board(id.to_string())),
    }
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
    let git_bar = cx.new(|cx| TrunkSync::new(window, cx));
    let file_tree = cx.new(|cx| crate::file_tree::FileTreeView::new(window, cx));
    let board_active = cx.new(|cx| BoardView::new(window, cx));
    let board_my = cx.new(|cx| BoardView::new(window, cx));
    // DEV-ONLY (§11.4 headless verification, same family as
    // EXP_DEV_SERVER/EXP_DEV_SCREEN): pre-select the settings section so a
    // capture run lands on one pane without synthetic input. EXP-851 moved
    // the screen/list seeds (`EXP_DEV_TOOL`, `EXP_DEV_INBOX_TAB`) onto the
    // navigation, where the screens they name live. Never document for users.
    let shared = cx.new(|_| RailShared {
        git_bar,
        file_tree,
        board_active,
        board_my,
        sc_selection: ScSelection::None,
        selected_file: None,
        settings_section: std::env::var("EXP_DEV_SETTINGS")
            .ok()
            .as_deref()
            .map(str::trim)
            .and_then(parse_dev_settings_section)
            .unwrap_or(crate::settings::SettingsSection::General),
    });
    cx.default_global::<RailRegistry>()
        .by_window
        .insert(window_id, shared.clone());
    shared
}

/// Read-only lookup of what a window is LOOKING AT, in the tool vocabulary
/// (EXP-638: the OS-notification redundancy check runs from an App-level task
/// with no `&mut Window` in hand). EXP-851: derived from the active SCREEN —
/// the list screens map onto themselves, a detail onto the list it names, and
/// everything else onto the board default. `None` for windows without a
/// navigation — dialogs, undocked terminals, the login surface.
pub(crate) fn rail_tool_for_window_id(
    window_id: WindowId,
    cx: &App,
) -> Option<(ToolWindow, InboxTab)> {
    let nav = crate::navigation::nav_for_window_id(window_id, cx)?;
    let screen = nav.read(cx).screen().cloned();
    Some(focused_list(screen.as_ref()))
}

/// EXP-851: [`rail_tool_for_window_id`]'s pure rule — which LIST a screen
/// reads as. A list screen is itself; a support thread reads as Support (its
/// rows are the tickets); everything else falls back to the board list, which
/// is the redundancy check's "shows no notification stream" answer.
pub(crate) fn focused_list(screen: Option<&Screen>) -> (ToolWindow, InboxTab) {
    match screen {
        Some(Screen::Inbox { tab }) => (ToolWindow::Inbox, *tab),
        Some(Screen::Support) | Some(Screen::SupportThread { .. }) => {
            (ToolWindow::Support, InboxTab::Inbox)
        }
        Some(Screen::Files) => (ToolWindow::Files, InboxTab::Inbox),
        Some(Screen::SourceControl) => (ToolWindow::SourceControl, InboxTab::Inbox),
        Some(Screen::Reviews) => (ToolWindow::Reviews, InboxTab::Inbox),
        Some(Screen::Automations) => (ToolWindow::Automations, InboxTab::Inbox),
        Some(Screen::Workflows) | Some(Screen::Workflow { .. }) => {
            (ToolWindow::Workflows, InboxTab::Inbox)
        }
        _ => (ToolWindow::BoardIssues, InboxTab::Inbox),
    }
}

/// EXP-862 — the LIST a [`ListPanel`] row pins on the detail it opens.
///
/// The bug it fixes: every row used to navigate plainly and let the breadcrumb
/// rule ([`crate::navigation::derive_origin`]) work it out from the screen
/// that was up. Beside a detail opened from the RAIL that rule derives
/// NOTHING — a rail-opened detail carries no list — so clicking a row in the
/// left column blanked the column and threw the reader back to the rail,
/// mid-click, in the one place a list was plainly on screen.
///
/// * [`ListMode::Screen`]: the full-width list screen IS the list, so its rows
///   hand on their own screen's origin.
/// * [`ListMode::Nav`]: the left column's list stays put — the detail that
///   replaces the open one is pinned to the very list it was picked from.
///
/// Pure, so both modes are a unit test.
pub(crate) fn row_origin_for(
    mode: ListMode,
    nav_origin: Option<&crate::navigation::TabOrigin>,
    screen: Option<&Screen>,
) -> Option<crate::navigation::TabOrigin> {
    match mode {
        ListMode::Screen => screen.and_then(Screen::list_origin),
        ListMode::Nav => nav_origin.cloned(),
    }
}

/// Drop a closed window's entry (called from the `Shell` release hook,
/// mirroring `navigation::remove_window`).
pub fn remove_window(window_id: WindowId, cx: &mut App) {
    if let Some(registry) = cx.try_global::<RailRegistry>() {
        if registry.by_window.contains_key(&window_id) {
            cx.global_mut::<RailRegistry>().by_window.remove(&window_id);
        }
    }
}

/// EXP-851: open the list `tool` names — the legacy `activate_tool` seam,
/// kept because half a dozen surfaces (the create-board dialog, Source
/// Control's own buttons, an OS notification, the search palette, the
/// helpdesk settings pane) speak this vocabulary. A board list takes the
/// window's active board.
pub(crate) fn activate_tool(window: &mut Window, cx: &mut App, tool: ToolWindow) {
    let board = (tool == ToolWindow::BoardIssues)
        .then(|| {
            let nav = crate::navigation::nav_for_window(window, cx);
            active_board_id(&nav, cx)
        })
        .flatten();
    navigate(window, cx, tool.origin_screen(board));
}

/// Open the Inbox screen ON a specific tab (the `OpenInbox` / `OpenMyIssues`
/// actions and the OS-notification routes).
pub(crate) fn open_inbox_tab(window: &mut Window, cx: &mut App, tab: InboxTab) {
    navigate(window, cx, Screen::Inbox { tab });
}

/// EXP-851: keep the window's active BOARD in step with what it shows — the
/// scope every repo-backed surface resolves through (files, git, the `+`
/// shell cwd, the board picker). Called on go-back / go-forward and on tab
/// activation, where the screen changes without a fresh navigation.
pub(crate) fn apply_origin(window: &Window, cx: &mut App, origin: &crate::navigation::TabOrigin) {
    if origin.tool != ToolWindow::BoardIssues {
        return;
    }
    if let Some(board_id) = origin.board_id.clone() {
        crate::navigation::set_active_board(window, cx, board_id);
    }
}

/// Whether the ACTIVE team's synced row has the helpdesk flag on — the gate
/// for the Support rail icon + tool window (EXP-180). Rows synced before the
/// column existed hydrate `None` → disabled.
fn helpdesk_enabled(nav: &Entity<Navigation>, cx: &App) -> bool {
    active_team_id(nav, cx)
        .and_then(|id| {
            Store::global(cx)
                .collections()
                .teams
                .read(cx)
                .get(&id)
                .and_then(|team| team.helpdesk_enabled)
        })
        == Some(true)
}

/// The Support tool window's open/resolved filter (the server's
/// `helpdesk.listThreads` filter enum).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SupportFilter {
    Open,
    Resolved,
}

impl SupportFilter {
    fn as_str(self) -> &'static str {
        match self {
            SupportFilter::Open => "open",
            SupportFilter::Resolved => "resolved",
        }
    }
}

/// The fetch key of one Support list: `(team_id, filter)`.
type SupportKey = (String, SupportFilter);


/// FEED-3: hover-reveal group name for the expanded rail's board rows — the
/// gear that jumps to the board's settings page shows only under the cursor.
const BOARD_ROW_GROUP: &str = "rail-board-row";
/// EXP-863: the Pinned rows' hover group — reveals the trailing Unpin button
/// (the board rows' gear recipe).
const PIN_ROW_GROUP: &str = "rail-pin-row";
/// EXP-863: the `ListNav` issue rows' hover group — reveals the leading
/// bulk-select checkbox (the big list's `issue-row` group).
const NAV_ROW_GROUP: &str = "list-nav-issue-row";

/// EXP-915: the `ListNav` issue lists' row heights — the group band and the
/// `rail_row_lead` row — plus the 2px the old `gap_0p5` column put between
/// them, folded into each virtual-list item so the fixed sizes stay exact.
const NAV_HEADER_HEIGHT: f32 = 24.;
const NAV_ISSUE_ROW_HEIGHT: f32 = 28.;
const NAV_ROW_GAP: f32 = 2.;
/// EXP-980: the `ListNav` row's own left padding (`flat_row_compact`'s
/// `px_2`) — the base the sub-issue indent is measured off.
const NAV_ROW_PAD: f32 = 8.;
/// EXP-998: the gutters start under the STATUS glyph — past the 16px select
/// cell and its 4px gap, plus the 5 that put a 14px gutter's centre (7) under
/// a 24px cell's glyph (12).
const NAV_GUIDE_INSET: f32 = 16. + 4. + 5.;

/// EXP-923/EXP-965: the space between two rows of the rail's Running section
/// — the rail's own `gap_1`, as a number, because the connector has to BRIDGE
/// it (`tree_guides::guide_layer`) and a literal in one of the two places
/// would drift the moment the other moved.
const RUNNING_ROW_GAP: f32 = 0.25 * theme::FONT_SIZE_PX;

/// One flattened `ListNav` virtual-list row (EXP-915) — the big list's
/// `ListRow` at the column's density: the issue rides behind the memoized
/// query's `Rc`, so rebuilding the vector per frame clones handles, never
/// payloads.
enum NavRow {
    Header {
        status: Box<ResolvedStatus>,
        count: usize,
        collapsed: bool,
        /// EXP-998: the blocks rail's lanes crossing this band.
        rail: domain::issue_rail::RailRow,
    },
    Issue {
        /// The issue's ordinal across the WHOLE list, folded groups included
        /// — the row's element id, so folding a group never renumbers the
        /// rows below it into each other's element state.
        index: usize,
        issue: Rc<Issue>,
        /// EXP-980: the sub-issue connector (the big list's `ListRow::Issue`
        /// carries the same), off the group's visible depth sequence.
        guides: domain::tree_guides::Guides,
        /// EXP-998: the row's slice of the blocks rail — its node (labelled
        /// by the query's `blocks` numbers) and lanes.
        rail: domain::issue_rail::RailRow,
    },
}

impl NavRow {
    /// The virtual-list item height: the row plus its trailing gap.
    fn height(&self) -> Pixels {
        px(match self {
            NavRow::Header { .. } => NAV_HEADER_HEIGHT + NAV_ROW_GAP,
            NavRow::Issue { .. } => NAV_ISSUE_ROW_HEIGHT + NAV_ROW_GAP,
        })
    }
}

/// The `ListNav` board query's memo key (EXP-915): the scope plus every
/// collection input ([`queries::BoardDataKey`]).
#[derive(PartialEq, Eq)]
struct NavBoardKey {
    query: IssueQuery,
    base: queries::BoardDataKey,
}

/// EXP-282: one row of the EXPANDED rail — icon + label, left-aligned, glass
/// row fills. Hand-rolled on purpose: gpui-component's `Button` centers its
/// inner layout with no `Styled` reach into it (the settings-nav rows set the
/// same precedent), and a centered label would defeat the whole point of the
/// labelled rail. Handlers are the caller's job.
/// EXP-533: the rail spinner's label and tooltip.
const SYNCING_LABEL: &str = "Syncing\u{2026}";

/// A rail entry's status badge (EXP-509 — the Source Control entry outgrew
/// the plain dot): the classic colored dot (EXP-699: Inbox/Support primary,
/// Reviews green, Devices green/amber — the mobile tab-bar palette), a
/// small status ICON (SC attention triangle / error cross), or the spinning
/// refresh glyph while a sync is pulling.
#[derive(Clone)]
pub(crate) enum RailBadge {
    Dot(Hsla),
    Icon(ExpIcon, Hsla),
    Syncing,
    /// EXP-963: a COUNT — the web rail's `Badge` (EXP-962): the Drafts
    /// entry's pile, muted. Zero renders nothing (the badge's own rule).
    Count(usize, crate::surface::BadgeTone),
}

/// One badge element at `glyph_px` (dots keep their fixed 6px regardless;
/// a count is the 16px capsule).
fn rail_badge_element(badge: RailBadge, glyph_px: f32, cx: &App) -> gpui::AnyElement {
    match badge {
        RailBadge::Dot(color) => div()
            .size_1p5()
            .flex_shrink_0()
            .rounded_full()
            .bg(color)
            .into_any_element(),
        RailBadge::Count(count, tone) => div()
            .flex_shrink_0()
            .children(crate::surface::count_badge(count, tone, cx))
            .into_any_element(),
        RailBadge::Icon(icon, color) => Icon::from(icon)
            .with_size(px(glyph_px))
            .text_color(color)
            .flex_shrink_0()
            .into_any_element(),
        RailBadge::Syncing => div()
            .flex_shrink_0()
            .child(
                Spinner::new()
                    .icon(registry::UI_REFRESH)
                    .with_size(px(glyph_px))
                    .color(cx.theme().muted_foreground),
            )
            .into_any_element(),
    }
}

/// EXP-870: the COMPACT rail's entry — a 32px square holding the row's lead
/// glyph, the label demoted to a tooltip and the badge riding the top-right
/// corner. The expanded [`rail_row_lead`]'s twin: same fills, same hover, so
/// the icon column reads as the rail with its labels folded away.
fn rail_compact_button(
    id: impl Into<gpui::ElementId>,
    lead: gpui::AnyElement,
    label: impl Into<SharedString>,
    active: bool,
    badge: Option<RailBadge>,
    cx: &App,
) -> gpui::Stateful<gpui::Div> {
    let label: SharedString = label.into();
    div()
        .id(id)
        .relative()
        .size(px(32.))
        .flex_shrink_0()
        .flex()
        .items_center()
        .justify_center()
        .rounded(cx.theme().radius)
        .cursor_pointer()
        .when(active, |this| {
            this.bg(theme::tokens::glass::FILL_ACTIVE.to_hsla())
        })
        .hover(|this| this.bg(theme::tokens::glass::FILL_ROW.to_hsla()))
        .child(lead)
        .when_some(badge, |this, badge| {
            // A count capsule overhangs the glyph's corner (web `-top-0.5
            // -right-0.5`); the dots and glyphs sit inside it.
            let (top, right) = match badge {
                RailBadge::Count(..) => (-2., -2.),
                _ => (3., 3.),
            };
            this.child(
                div()
                    .absolute()
                    .top(px(top))
                    .right(px(right))
                    .child(rail_badge_element(badge, 10., cx)),
            )
        })
        .tooltip(move |window, cx| {
            gpui_component::tooltip::Tooltip::new(label.clone()).build(window, cx)
        })
}

fn rail_row(
    id: impl Into<gpui::ElementId>,
    icon: Icon,
    label: impl Into<SharedString>,
    active: bool,
    badge: Option<RailBadge>,
    cx: &App,
) -> gpui::Stateful<gpui::Div> {
    rail_row_lead(
        id,
        icon.with_size(gpui_component::Size::Medium).flex_shrink_0().into_any_element(),
        label,
        active,
        None,
        badge,
        cx,
    )
}

/// [`rail_row`] with an arbitrary LEAD element (EXP-818: a Sessions row leads
/// with a state dot, and a parent row with its collapse chevron).
///
/// `note` is the muted caption between the title and the badge — EXP-827: a
/// Sessions row's host MACHINE goes there, so the row reads
/// `● title · macbook ◌` and the spinner stays the row's LAST element (it used
/// to be appended by the caller, which put the machine name to the RIGHT of a
/// spinner that then jumped as the name's width changed).
fn rail_row_lead(
    id: impl Into<gpui::ElementId>,
    lead: gpui::AnyElement,
    label: impl Into<SharedString>,
    active: bool,
    note: Option<SharedString>,
    badge: Option<RailBadge>,
    cx: &App,
) -> gpui::Stateful<gpui::Div> {
    // EXP-635: selection never recolors the glyph — a tool icon stays white
    // whether or not its entry is active (a board row keeps the board tint
    // the caller already applied). The glass active fill carries the
    // selection (no 2px marker bar — the row fill IS the marker at this
    // width).
    // EXP-963: the COMPACT list row (web `SidebarMenuButton density=
    // "compact"` / `ListRow density="compact"`): 28 tall, 8px sides, 8px
    // between the glyph and the text.
    crate::surface::flat_row_compact()
        .id(id)
        .w_full()
        .flex_shrink_0()
        .cursor_pointer()
        .when(active, |this| {
            this.bg(theme::tokens::glass::FILL_ACTIVE.to_hsla())
        })
        .hover(|this| this.bg(theme::tokens::glass::FILL_ROW.to_hsla()))
        .child(lead)
        .child(div().flex_1().min_w_0().truncate().child(label.into()))
        .when_some(note, |this, note| {
            this.child(
                div()
                    .flex_shrink_0()
                    .max_w(px(96.))
                    .truncate()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(note),
            )
        })
        .when_some(badge, |this, badge| {
            this.child(rail_badge_element(badge, 12., cx))
        })
}

// ---------------------------------------------------------------------------
// RailView — the icon strip left of the dock area
// ---------------------------------------------------------------------------

/// The tool-window rail: the expanded sidebar, or (EXP-870) its 48px icon
/// column beside a list. Owned and rendered by the `Shell` OUTSIDE the
/// `DockArea`. (No terminal entry — terminals are session-bar tabs, EXP-769;
/// no Sessions section either — live runs are top tabs, EXP-870.)
pub struct RailView {
    nav: Entity<Navigation>,
    shared: Entity<RailShared>,
    /// The branch as of the last render — a checkout refreshes the file tree.
    last_branch: Option<String>,
    /// Scroll position of the rail's middle zone (tools + board icons) —
    /// small windows with many boards must not push Settings/Account off.
    rail_scroll: ScrollHandle,
    /// EXP-870: whether the rail renders as the 48px ICON column — true
    /// while a list or the settings nav sits beside it (the rail never
    /// leaves the window any more; it folds its labels away instead).
    /// Re-derived at the top of every render from the window's occupant.
    compact: bool,
    /// EXP-923: the parents of the Running section whose sub-runs are folded
    /// away (the session lists' `collapsed`). Per window, never persisted.
    collapsed_runs: HashSet<String>,
    /// EXP-923: the Running rows carry a liveness that expires with
    /// `last_seen_at` and produces no collection delta — the session lists'
    /// 5s re-derive, here.
    _tick: gpui::Task<()>,
    _subscriptions: Vec<Subscription>,
}

/// EXP-923: the Running section folds its nested runs like every other
/// session list ([`crate::sessions_section::fold_for`]).
impl crate::sessions_section::Collapsible for RailView {
    fn collapsed_mut(&mut self) -> &mut HashSet<String> {
        &mut self.collapsed_runs
    }
}

impl RailView {
    pub fn new(window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        let nav = nav_for_window(window, cx);
        let shared = rail_shared_for_window(window, cx);
        let git_bar = shared.read(cx).git_bar.clone();
        let collections = Store::global(cx).collections().clone();
        let avatar_cache = crate::user_avatar::AvatarCache::global(cx);
        let getting_started = crate::getting_started::GettingStartedProgress::global(cx);
        let coding_hub = coding_flow::CodingHub::global(cx);
        let local_sessions = coding_flow::LocalSessions::global(cx);
        let subscriptions = vec![
            cx.observe(&shared, |_, _, cx| cx.notify()),
            cx.observe(&nav, |_, _, cx| cx.notify()),
            // Sync/conflict badge follows the trunk engine's state.
            cx.observe(&git_bar, |_, _, cx| cx.notify()),
            // The Reviews dot is a live read over issues ⨝ boards.
            cx.observe(&collections.issues, |_, _, cx| cx.notify()),
            // Board icons + the Reviews dot follow the boards collection.
            cx.observe(&collections.boards, |_, _, cx| cx.notify()),
            // The Support icon gates on the team row's helpdesk_enabled flag.
            cx.observe(&collections.teams, |_, _, cx| cx.notify()),
            // The Inbox and Support dots are live reads over unread rows.
            cx.observe(&collections.notifications, |_, _, cx| cx.notify()),
            // EXP-778: the Pinned section — its rows, and the action names
            // it resolves (issues/sessions are observed already).
            cx.observe(&collections.pins, |_, _, cx| cx.notify()),
            // EXP-878: the conditional Drafts entry and its count.
            cx.observe(&collections.issue_drafts, |_, _, cx| cx.notify()),
            cx.observe(&collections.actions, |_, _, cx| cx.notify()),
            // The Running section (EXP-923) and the pinned session rows are
            // live reads over my coding_sessions rows.
            cx.observe(&collections.coding_sessions, |_, _, cx| cx.notify()),
            // The pinned session rows' dots read the runs THIS process hosts
            // (and their paused edge the devices rows below).
            cx.observe(&local_sessions, |_, _, cx| cx.notify()),
            cx.observe(&collections.devices, |_, _, cx| cx.notify()),
            // EXP-311: the account button's avatar rides the users shape
            // (profile image URL) plus the async avatar-byte cache.
            cx.observe(&collections.users, |_, _, cx| cx.notify()),
            cx.observe(&avatar_cache, |_, _, cx| cx.notify()),
            // EXP-548: the Getting-started entry hides once every checklist
            // entry is done — the shared progress entity re-notifies on the
            // synced collections it reads AND on its tRPC one-shots.
            cx.observe(&getting_started, |_, _, cx| cx.notify()),
            // EXP-533: the footer's sync spinner rides the sync engine's
            // shared state (health + the post-restart catch-up stamp), which
            // the rail did not observe before.
            cx.observe(&Store::global(cx).state(), |_, _, cx| cx.notify()),
            // EXP-723: the footer's What's-new card reads `changelogSeenId`
            // off the coding hub's settings, so dismissing it (or opening the
            // dialog, which also marks it seen) must repaint the rail.
            cx.observe(&coding_hub, |_, _, cx| cx.notify()),
        ];
        Self {
            nav,
            shared,
            last_branch: None,
            rail_scroll: ScrollHandle::new(),
            compact: false,
            collapsed_runs: HashSet::new(),
            _tick: crate::sessions_section::tick(cx, |_: &mut Self, cx| cx.notify()),
            _subscriptions: subscriptions,
        }
    }

    /// EXP-791: a muted section label ("Boards", "Sessions", "This device").
    fn section_label(&self, text: &'static str, cx: &App) -> gpui::Div {
        h_flex()
            .w_full()
            .h(px(24.))
            .pl_1p5()
            .pr_0p5()
            .items_center()
            .child(
                div()
                    .flex_1()
                    .text_xs()
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(cx.theme().sidebar_foreground.opacity(0.5))
                    .child(text),
            )
    }

    /// EXP-778: the Pinned section's rows — the caller's pins in the ACTIVE
    /// team, in `sort_order`, each resolved against its sibling collection
    /// and HIDDEN when the target is gone (a trashed board's issue, a session
    /// the shape no longer carries, a deleted action). An issue row leads
    /// with its mono identifier and opens the detail scoped on its board
    /// (the Issues list's path); a session row wears the Sessions row's
    /// state dot and opens the run the way every entry point does; an
    /// action row leads with the action's glyph and opens the Agent
    /// composer with that action picked (the Actions page's ▶ Run seam).
    /// EXP-862: an action row is ACTIVE while the composer is seeded with it
    /// (`active_chat_action`) — the pinned row is where that run is being
    /// started from, so it is the row that reads as current, and the Agent
    /// entry above it does not (web does the same off `?action=`).
    ///
    /// Empty when nothing is pinned — the caller hides the section.
    fn render_pinned_rows(
        &mut self,
        active_chat_action: Option<&str>,
        cx: &mut gpui::Context<Self>,
    ) -> Vec<gpui::AnyElement> {
        let Some(team_id) = active_team_id(&self.nav, cx) else {
            return Vec::new();
        };
        let pins = crate::pins::pins_in_team(&team_id, cx);
        if pins.is_empty() {
            return Vec::new();
        }
        let active_screen = resolved_screen(&self.nav, cx);
        let collections = Store::global(cx).collections().clone();
        let muted = cx.theme().muted_foreground;
        let mut out = Vec::with_capacity(pins.len());
        for (index, pin) in pins.iter().enumerate() {
            let Some((kind, target_id)) = crate::pins::pin_target(pin) else {
                continue;
            };
            let row_el = match kind {
                domain::contract::PIN_KIND_ISSUE => {
                    let Some(issue) = collections.issues.read(cx).get(target_id).cloned() else {
                        continue;
                    };
                    let screen = Screen::IssueDetail {
                        issue_id: issue.id.clone(),
                    };
                    let active = active_screen.as_ref() == Some(&screen);
                    // EXP-870: the identifier does not fit the compact
                    // square — the issue glyph stands in, the tooltip names it.
                    let lead = if self.compact {
                        Icon::new(registry::NAV_ISSUES)
                            .with_size(gpui_component::Size::Medium)
                            .flex_shrink_0()
                            .text_color(muted)
                            .into_any_element()
                    } else {
                        div()
                            .flex_shrink_0()
                            .text_xs()
                            .text_color(muted)
                            .font_family(theme::terminal::FONT_FAMILY)
                            .child(SharedString::from(issue.identifier.clone()))
                            .into_any_element()
                    };
                    let title = if self.compact {
                        format!("{} {}", issue.identifier, issue.title)
                    } else {
                        issue.title.clone()
                    };
                    let issue_id = issue.id.clone();
                    let board_id = issue.board_id.clone();
                    self.entry(("rail-pin", index), lead, title, active, None, cx)
                        .on_click(cx.listener(move |_, _: &ClickEvent, window, cx| {
                            // EXP-851: a RAIL row opens a detail with no list
                            // beside it — the rail stays. The board still
                            // becomes the window's scope (files, git, `+`).
                            crate::navigation::set_active_board(window, cx, board_id.clone());
                            crate::navigation::navigate_from_rail(
                                window,
                                cx,
                                Screen::IssueDetail {
                                    issue_id: issue_id.clone(),
                                },
                            );
                        }))
                }
                domain::contract::PIN_KIND_ACTION => {
                    let Some(action) = collections.actions.read(cx).get(target_id).cloned() else {
                        continue;
                    };
                    let name = action
                        .name
                        .clone()
                        .unwrap_or_else(|| "Untitled action".to_string());
                    let lead = crate::icons::action_icon(action.icon.as_deref())
                        .with_size(gpui_component::Size::Medium)
                        .flex_shrink_0()
                        .into_any_element();
                    let action_id = action.id.clone();
                    let active = active_chat_action == Some(action.id.as_str());
                    self.entry(("rail-pin", index), lead, name, active, None, cx)
                        .on_click(cx.listener(move |_, _: &ClickEvent, window, cx| {
                            // `ActionsView::run`: no agent CLI, nothing to
                            // run — the composer would refuse anyway.
                            if crate::coding_flow::no_agent_reason(cx).is_some() {
                                return;
                            }
                            crate::navigation::navigate_to_chat_from_rail(
                                window,
                                cx,
                                crate::navigation::ChatSeed::action(action_id.clone()),
                            );
                        }))
                }
                _ => continue,
            };
            // EXP-863: hover reveals Unpin on the row's right — the board
            // rows' gear recipe. The click stops at the button, so the row
            // underneath never navigates.
            let unpin = {
                let team_id = team_id.clone();
                let target_id = target_id.to_string();
                div()
                    .invisible()
                    .group_hover(PIN_ROW_GROUP, |style| style.visible())
                    .flex_shrink_0()
                    .child(
                        Button::new(("rail-pin-unpin", index))
                            .ghost()
                            .cursor_pointer()
                            .xsmall()
                            .icon(Icon::from(registry::UI_UNPIN))
                            .tooltip("Unpin")
                            .on_click(move |_: &ClickEvent, _window, cx| {
                                cx.stop_propagation();
                                crate::pins::toggle_pin(
                                    team_id.clone(),
                                    kind,
                                    target_id.clone(),
                                    cx,
                                );
                            }),
                    )
            };
            if self.compact {
                out.push(row_el.into_any_element());
            } else {
                out.push(row_el.group(PIN_ROW_GROUP).child(unpin).into_any_element());
            }
        }
        out
    }

    /// EXP-923 — the rail's **Running** section: MY live runs, here and on
    /// every other machine, between the boards and "This device".
    ///
    /// This is where a live run LIVES now. It used to own a top tab (EXP-870)
    /// and, before that, a dock chip: both said "a run is a document you have
    /// open", which is wrong — a run is a place, it comes and goes on its own
    /// and a strip that fills up with unclosable chips is the strip telling
    /// you so. The section HIDES entirely while nothing runs: a sidebar group
    /// that is empty most of the day is noise, and the empty copy belongs to
    /// the surfaces that promise an answer (mobile's band).
    ///
    /// A row leads with the AGENT's brand mark rather than a state dot —
    /// which agent is on it is the thing you cannot get anywhere else, and
    /// the one state that needs you (`needs_input`) rides the mark's corner
    /// as the rail's own yellow badge. The HOST machine is the trailing
    /// glyph, fixed: it never truncates, because "which machine" is the
    /// question a remote run exists to raise.
    ///
    /// Returns `None` with nothing running (the caller renders neither the
    /// rule nor the label).
    fn render_running_section(&mut self, cx: &mut gpui::Context<Self>) -> Option<gpui::AnyElement> {
        let rows = crate::sessions_section::rail_running_rows(cx);
        let rows = crate::sessions_section::drop_collapsed(
            rows,
            &self.collapsed_runs,
            |row| row.session_id.as_str(),
            |row| row.depth,
        );
        if rows.is_empty() {
            return None;
        }
        // EXP-965: the connector, off the VISIBLE depth sequence.
        let guides = domain::tree_guides::guides_for(
            &rows.iter().map(|row| row.depth).collect::<Vec<_>>(),
        );
        let screen = resolved_screen(&self.nav, cx);
        let elements: Vec<gpui::AnyElement> = rows
            .iter()
            .enumerate()
            .map(|(index, row)| {
                // EXP-870: the row stays current across the Issue | Run
                // toggle — both faces are the same piece of work.
                let active = match &screen {
                    Some(Screen::Session { session_id }) => session_id == &row.session_id,
                    Some(Screen::IssueDetail { issue_id }) => {
                        row.issue_id.as_deref() == Some(issue_id.as_str())
                    }
                    _ => false,
                };
                self.rail_running_row(
                    index,
                    row,
                    guides.get(index).cloned().unwrap_or_default(),
                    active,
                    cx,
                )
            })
            .collect();
        Some(
            v_flex()
                .w_full()
                .gap(px(RUNNING_ROW_GAP))
                .when(self.compact, |section| section.items_center())
                .child(self.divider(cx))
                .when(!self.compact, |section| {
                    section.child(self.section_label("Running", cx))
                })
                .children(elements)
                .into_any_element(),
        )
    }

    /// ONE Running row ([`Self::render_running_section`]) in the rail's
    /// current shape — the labelled row, or the icon column's 32px mark.
    fn rail_running_row(
        &self,
        index: usize,
        row: &crate::sessions_section::RailRunRow,
        guides: domain::tree_guides::Guides,
        active: bool,
        cx: &mut gpui::Context<Self>,
    ) -> gpui::AnyElement {
        let muted = cx.theme().muted_foreground;
        let session_id = row.session_id.clone();
        let issue_id = row.issue_id.clone();
        // EXP-923: the lead is the agent's brand mark, with the attention
        // badge on its corner — the compact column's badge rule, on a glyph.
        let mark = crate::coding_selects::agent_mark(row.agent).with_size(px(16.));
        let lead = div()
            .relative()
            .flex_shrink_0()
            .child(mark)
            .when(row.attention, |this| {
                this.child(
                    div()
                        .absolute()
                        .top(px(-2.))
                        .right(px(-2.))
                        .size(px(6.))
                        .rounded_full()
                        .bg(theme::tokens::YELLOW.to_hsla())
                        // The hairline keeps the dot readable over the glyph
                        // it overlaps (the rail's own ground).
                        .border_1()
                        .border_color(theme::tokens::BACKGROUND.to_hsla()),
                )
            })
            .into_any_element();
        // The compact square has no room for two texts: the tooltip is the
        // whole label the expanded row splits into identifier + title.
        let label: SharedString = match &row.identifier {
            Some(identifier) => format!("{identifier} {}", row.title).into(),
            None => row.title.clone(),
        };
        let open = {
            let session_id = session_id.clone();
            move |window: &mut Window, cx: &mut App| {
                // EXP-923: no tab — a live run is reachable from this row for
                // as long as it runs.
                crate::navigation::navigate_from_live_rail(
                    window,
                    cx,
                    Screen::Session {
                        session_id: session_id.clone(),
                    },
                );
            }
        };
        if self.compact {
            // EXP-923: nesting is not drawn in the icon column — 14px of
            // indent inside a 32px square would only clip the mark.
            return rail_compact_button(
                ("rail-running", index),
                lead,
                label,
                active,
                None,
                cx,
            )
            .on_click(cx.listener(move |_, _: &ClickEvent, window, cx| open(window, cx)))
            .into_any_element();
        }
        let fold = row.has_children.then(|| {
            let collapsed = self.collapsed_runs.contains(&row.session_id);
            let fold_id = row.session_id.clone();
            div()
                .id(("rail-running-fold", index))
                .flex_shrink_0()
                .cursor_pointer()
                .child(
                    Icon::from(if collapsed {
                        registry::UI_CHEVRON_RIGHT
                    } else {
                        registry::UI_CHEVRON_DOWN
                    })
                    .xsmall()
                    .text_color(muted),
                )
                .on_click(cx.listener(move |this: &mut Self, _, _window, cx| {
                    // The row itself opens the run — folding must not.
                    cx.stop_propagation();
                    if !this.collapsed_runs.insert(fold_id.clone()) {
                        this.collapsed_runs.remove(&fold_id);
                    }
                    cx.notify();
                }))
        });
        let device_label = row.device_label.clone();
        let device = div()
            .id(("rail-running-device", index))
            .flex_shrink_0()
            .child(Icon::from(row.device_icon.clone()).xsmall().text_color(muted))
            .when_some(device_label, |this, label| {
                this.tooltip(move |window, cx| {
                    gpui_component::tooltip::Tooltip::new(label.clone()).build(window, cx)
                })
            });
        let kill = (!row.paused).then(|| {
            (
                row.local.clone(),
                row.device_label.clone().map(|label| label.to_string()),
                session_id.clone(),
            )
        });
        let _ = issue_id;
        let row_el = crate::surface::flat_row_compact()
            .id(("rail-running", index))
            .w_full()
            .flex_shrink_0()
            .relative()
            .cursor_pointer()
            .pl(px(8. + crate::tree_guides::LEVEL_PITCH * guides.depth() as f32))
            .when(active, |this| {
                this.bg(theme::tokens::glass::FILL_ACTIVE.to_hsla())
            })
            .hover(|this| this.bg(theme::tokens::glass::FILL_ROW.to_hsla()))
            .children(crate::tree_guides::guide_layer(
                &guides,
                8.,
                RUNNING_ROW_GAP,
            ))
            .children(fold)
            .child(lead)
            .children(row.identifier.clone().map(|identifier| {
                div()
                    .flex_shrink_0()
                    .text_xs()
                    .text_color(muted)
                    .font_family(theme::terminal::FONT_FAMILY)
                    .child(identifier)
            }))
            .child(div().flex_1().min_w_0().truncate().child(row.title.clone()))
            .child(device)
            .on_click(cx.listener(move |_, _: &ClickEvent, window, cx| open(window, cx)));
        // EXP-874: the kill rides the row's right-click menu, not a trailing
        // button — the session lists' own grammar.
        match kill {
            None => row_el.into_any_element(),
            Some((local, device_label, session_id)) => row_el
                .context_menu(move |menu, _window, cx| {
                    let local = local.clone();
                    let device_label = device_label.clone();
                    let session_id = session_id.clone();
                    menu.item(
                        crate::controls::danger_menu_item(
                            // EXP-849: one verb, wherever the run is hosted.
                            SharedString::from("Stop session"),
                            Icon::from(registry::CODING_STOP),
                            cx,
                        )
                        .on_click(move |_, window, cx| {
                            crate::session_bar::prompt_kill_session(
                                local.clone(),
                                device_label.clone(),
                                session_id.clone(),
                                window,
                                cx,
                            );
                        }),
                    )
                })
                .into_any_element(),
        }
    }

    /// EXP-533: the rail footer's "still catching up" spinner. It answers
    /// the "did the app notice I was asleep?" question the offline banner
    /// cannot: the banner means "can't reach the server", this means "we ARE
    /// talking to the server and the boards are still filling in". Nothing
    /// renders when everything is at head — a permanently visible sync glyph
    /// would say nothing at all.
    ///
    /// Deliberately NOT [`RailBadge::Syncing`]: that badge is the Source
    /// Control tool's vocabulary (a git pull), a different thing entirely.
    fn render_sync_indicator(&self, cx: &mut gpui::Context<Self>) -> Option<gpui::AnyElement> {
        if !Store::global(cx).sync_status(cx).catching_up {
            return None;
        }
        let spinner = Spinner::new()
            .icon(registry::UI_REFRESH)
            .with_size(px(12.))
            .color(cx.theme().muted_foreground);
        let row = h_flex()
            .id("rail-sync-indicator")
            .w_full()
            .h(px(24.))
            .flex_shrink_0()
            .items_center()
            .gap_2()
            .px_2()
            .child(spinner)
            .child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(SYNCING_LABEL),
            )
            .tooltip(move |window, cx| {
                gpui_component::tooltip::Tooltip::new(SYNCING_LABEL).build(window, cx)
            });
        Some(row.into_any_element())
    }

    /// One tool-window icon: a ghost icon button, `selected` while its tool
    /// window is active — the glyph stays white either way (EXP-635) and the
    /// board accent rides the marker bar alone; `badge` paints an
    /// attention dot in the given color (EXP-214: review green for open PRs,
    /// amber for support/conflicts), `None` for no dot. EXP-723: one labelled
    /// row, always — the icon-only variant went with the collapse.
    ///
    /// `tooltip` is `Some` only where the row has something the label cannot
    /// say — Source Control's "synced 3m ago" / failure reason. Everywhere
    /// else a tooltip would just repeat the visible label.
    /// EXP-870: one rail entry in the rail's CURRENT shape — the labelled
    /// 28px row, or the compact column's 32px icon square with the label as
    /// its tooltip. Every nav/board/pinned entry goes through here so the two
    /// shapes can never list different destinations.
    fn entry(
        &self,
        id: impl Into<gpui::ElementId>,
        lead: gpui::AnyElement,
        label: impl Into<SharedString>,
        active: bool,
        badge: Option<RailBadge>,
        cx: &App,
    ) -> gpui::Stateful<gpui::Div> {
        if self.compact {
            rail_compact_button(id, lead, label, active, badge, cx)
        } else {
            rail_row_lead(id, lead, label, active, None, badge, cx)
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn rail_tool_icon(
        &self,
        id: &'static str,
        icon: Icon,
        tool: ToolWindow,
        label: &'static str,
        tooltip: Option<SharedString>,
        badge: Option<RailBadge>,
        cx: &mut gpui::Context<Self>,
    ) -> gpui::AnyElement {
        // EXP-851: a list is a SCREEN — the entry highlights while that
        // screen is up, exactly like every other rail row.
        let screen = tool.origin_screen(None);
        let active = match (&screen, resolved_screen(&self.nav, cx)) {
            // A board list highlights through its BOARD row, never here.
            (Screen::BoardIssues { .. }, _) => false,
            (screen, Some(current)) => *screen == current,
            _ => false,
        };
        // EXP-870: the compact square already wears its label as a tooltip;
        // a richer caller tooltip (Source Control's sync stamp) replaces it.
        let lead = icon.with_size(gpui_component::Size::Medium).flex_shrink_0().into_any_element();
        let label: SharedString = match (&tooltip, self.compact) {
            (Some(text), true) => text.clone(),
            _ => label.into(),
        };
        let compact = self.compact;
        self.entry(id, lead, label, active, badge, cx)
            .when(!compact, |row| {
                row.when_some(tooltip, |row, text| {
                    row.tooltip(move |window, cx| {
                        gpui_component::tooltip::Tooltip::new(text.clone()).build(window, cx)
                    })
                })
            })
            .on_click(cx.listener(move |_, _: &ClickEvent, window, cx| {
                activate_tool(window, cx, tool);
            }))
            .into_any_element()
    }

    /// A rail entry that navigates STRAIGHT to a tab-less full-page screen
    /// instead of activating a tool window (EXP-467's Actions entry,
    /// generalized by EXP-686 for Devices / Actions / Automations, and by
    /// EXP-706 for Reviews): the settings gear's direct-navigation shape in
    /// the tool-icon slot. While its screen is up this entry is the ONE
    /// highlighted row.
    fn rail_screen_entry(
        &self,
        id: &'static str,
        icon: Icon,
        label: &'static str,
        screen: Screen,
        badge: Option<RailBadge>,
        cx: &mut gpui::Context<Self>,
    ) -> gpui::AnyElement {
        let active = resolved_screen(&self.nav, cx).as_ref() == Some(&screen);
        self.rail_screen_entry_active(id, icon, label, screen, badge, active, cx)
    }

    /// [`Self::rail_screen_entry`] with the highlight decided by the CALLER —
    /// EXP-862's Agent entry, which is not the active row while the composer
    /// it opens is seeded with a pinned action (that action's row is).
    #[allow(clippy::too_many_arguments)]
    fn rail_screen_entry_active(
        &self,
        id: &'static str,
        icon: Icon,
        label: &'static str,
        screen: Screen,
        badge: Option<RailBadge>,
        active: bool,
        cx: &mut gpui::Context<Self>,
    ) -> gpui::AnyElement {
        let lead = icon.with_size(gpui_component::Size::Medium).flex_shrink_0().into_any_element();
        self.entry(id, lead, label, active, badge, cx)
            .on_click(cx.listener(move |_, _: &ClickEvent, window, cx| {
                // EXP-851: the rail is not a list — an entry never lends one
                // (the Agent page especially: reached from here it shows its
                // OWN sessions, not whatever the main view had up).
                crate::navigation::navigate_from_rail(window, cx, screen.clone());
            }))
            .into_any_element()
    }

    /// EXP-818/EXP-851: the Agent entry — the Chat page, which stacks the
    /// composer over the Running/Past session rows.
    ///
    /// EXP-862: it is the active row only while the composer has NO action
    /// seeded. With one seeded, the pinned action row that seeded it is the
    /// current row and two highlights would be a lie about where you are.
    fn rail_agent_entry(
        &self,
        active_chat_action: Option<&str>,
        cx: &mut gpui::Context<Self>,
    ) -> gpui::AnyElement {
        let active = matches!(resolved_screen(&self.nav, cx), Some(Screen::Chat))
            && active_chat_action.is_none();
        // EXP-1001: no badge — the Running section below IS the live signal.
        self.rail_screen_entry_active(
            "rail-agent",
            Icon::from(registry::ACTION_CHAT),
            "Agent",
            Screen::Chat,
            None,
            active,
            cx,
        )
    }

    /// EXP-862: the action the Agent page's composer is seeded with, for the
    /// rows that mark themselves. The chat screen answers once it is mounted
    /// ([`crate::screens::chat_action_id`]); until then the pending seed does
    /// (a play button writes it a beat before the page exists, and the row
    /// must light up on the click).
    fn active_chat_action(&self, window: &Window, cx: &App) -> Option<String> {
        crate::screens::chat_action_id(window, cx)
            .or_else(|| crate::navigation::pending_chat_action_id(&self.nav, cx))
    }

    /// The Getting-started entry (EXP-470): the desktop mirror of the web
    /// sidebar's re-entry point — navigates to the tab-less
    /// [`Screen::GettingStarted`] page, the [`Self::rail_screen_entry`] shape.
    /// EXP-548: it sits at the BOTTOM of the rail, under the What's-new card
    /// and above the account row (the web sidebar-footer position), and is
    /// rendered only while the checklist is incomplete — no dismissal.
    /// EXP-723: MUTED, like the web footer's — it is a re-entry point, not a
    /// destination competing with the boards above it.
    fn rail_getting_started_entry(&self, cx: &mut gpui::Context<Self>) -> gpui::AnyElement {
        // EXP-686: the page's tab rides the screen, so ANY tab highlights.
        let active = matches!(
            resolved_screen(&self.nav, cx),
            Some(Screen::GettingStarted { .. })
        );
        let icon = Icon::from(icons::registry::NAV_GETTING_STARTED);
        rail_row(
            "rail-getting-started",
            icon,
            "Getting started",
            active,
            None,
            cx,
        )
        .text_color(cx.theme().muted_foreground)
        .on_click(cx.listener(|_, _: &ClickEvent, window, cx| {
            navigate(window, cx, Screen::GettingStarted {
                tab: GettingStartedTab::FirstSteps,
            });
        }))
        .into_any_element()
    }

    /// The account button — the rail's bottom-left element, with the settings
    /// gear on its right.
    ///
    /// EXP-723: its dropdown is the WEB account menu, exactly — What's new,
    /// About, a separator, Sign out. Team switching moved UP into the header's
    /// team switcher (where the web keeps it), and there is deliberately no
    /// Admin entry: the admin console is web-only.
    fn render_account_button(&self, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let account = crate::queries::active_account(cx);
        // EXP-311, web sidebar parity: the profile image (or hue-hashed
        // initials) plus the FIRST name only — full name + email live in
        // settings → Account. Name-less accounts (Apple sign-in) fall back
        // to the email for the label/initials.
        let full_name: SharedString = account
            .as_ref()
            .and_then(|account| account.name.clone())
            .filter(|name| !name.trim().is_empty())
            .or_else(|| account.as_ref().map(|account| account.email.clone()))
            .map(SharedString::from)
            .unwrap_or_else(|| "Not signed in".into());
        let short_name: SharedString =
            SharedString::from(crate::user_avatar::first_name(&full_name).to_string());
        // The avatar URL rides the synced users row, never accounts.json.
        let image_url = crate::queries::active_user(cx).and_then(|user| user.image);
        let avatar_image = crate::user_avatar::cached_avatar_image(cx, image_url.as_deref());
        let make_avatar = {
            let full_name = full_name.clone();
            // EXP-698: the hue key is the USER ID, so the rail's disc matches
            // the one every member list draws for the same person.
            let user_id = account
                .as_ref()
                .map(|account| account.user_id.clone())
                .unwrap_or_default();
            let signed_in = account.is_some();
            move |size: gpui_component::Size, image: Option<std::sync::Arc<gpui::Image>>| {
                // Signed out keeps the generic person placeholder instead of
                // "NS" initials for "Not signed in".
                if !signed_in {
                    return gpui_component::avatar::Avatar::new()
                        .with_size(size)
                        .into_any_element();
                }
                crate::user_avatar::avatar_element(&user_id, &full_name, image, size)
            }
        };

        // EXP-870: the compact rail's trigger is the avatar alone.
        let compact = self.compact;
        Button::new("rail-account")
            .ghost().cursor_pointer()
            .small()
            .when(compact, |button| {
                button.size(px(32.)).child(make_avatar(
                    gpui_component::Size::Small,
                    avatar_image.clone(),
                ))
            })
            // The trigger is a full-width row — the Button's own inner layout
            // is centered and unreachable, so the row is a `w_full` child that
            // left-aligns inside it.
            .when(!compact, |button| {
                button.w_full().h(px(36.)).px_1p5().child(
                h_flex()
                    .w_full()
                    .gap_2()
                    .items_center()
                    .child(make_avatar(
                        gpui_component::Size::Small,
                        avatar_image.clone(),
                    ))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_sm()
                            .truncate()
                            .child(short_name.clone()),
                    )
                    .child(
                        Icon::new(registry::NAV_TEAM_SWITCHER)
                            .xsmall()
                            .flex_shrink_0()
                            .text_color(cx.theme().muted_foreground),
                    ),
                )
            })
            .tooltip(full_name.clone())
            .dropdown_menu_with_anchor(gpui::Anchor::BottomLeft, move |menu, _window, _cx| {
                // EXP-723: the WEB account menu, item for item. No "Settings"
                // (the gear beside this button is the single settings entry),
                // no "Account" (it is a settings section), no identity header
                // (the trigger already names the person) and no team switching
                // — that lives in the header's team switcher now. There is no
                // Admin entry either: the admin console is web-only.
                //
                // The separator is the ONE exception to the EXP-697 "no
                // dividers in menus" rule, because the web menu has it here:
                // Sign out is destructive and must not sit flush against a
                // navigation item.
                menu.menu_with_icon(
                    "What's new",
                    registry::NAV_CHANGELOG,
                    Box::new(OpenWhatsNew),
                )
                .menu_with_icon("About", registry::SETTINGS_ABOUT, Box::new(OpenAbout))
                .separator()
                .menu_with_icon("Sign out", registry::NAV_SIGN_OUT, Box::new(SignOut))
            })
    }

    /// One Projects board icon: the board's glyph tinted with its color,
    /// selected while it is the active board AND the Board Issues tool is
    /// up. Click = set the active board + activate Board Issues — a DIRECT
    /// listener call (EXP-17: rail buttons dispatching App-global actions
    /// from inside the window update silently no-op).
    fn rail_board_icon(
        &self,
        index: usize,
        board: &domain::rows::Board,
        cx: &mut gpui::Context<Self>,
    ) -> gpui::AnyElement {
        // EXP-851: a board row highlights while ITS list screen is up (the
        // dev sentinel names no board, so it falls back to the active one).
        let active = match resolved_screen(&self.nav, cx) {
            Some(Screen::BoardIssues { board_id }) if board_id == board.id => true,
            Some(Screen::BoardIssues { board_id }) if board_id.is_empty() => {
                active_board_id(&self.nav, cx).as_deref() == Some(board.id.as_str())
            }
            _ => false,
        };
        let tint = board
            .color
            .as_deref()
            .and_then(parse_hex_color)
            .unwrap_or_else(|| cx.theme().muted_foreground);
        let icon = crate::icons::board_icon(board).text_color(tint);
        let board_id = board.id.clone();
        {
            // FEED-3: hover gear → this board's settings page. Owner-gated
            // like the settings nav's Boards group, so the selection never
            // clamps away underneath a non-owner.
            let owner = active_team_id(&self.nav, cx)
                .map(|team_id| crate::settings::is_owner(cx, &team_id))
                .unwrap_or(false);
            let settings_board_id = board.id.clone();
            // EXP-282: the board's own color tints the glyph whether the
            // row is active or not — `active` only adds the row fill.
            let compact = self.compact;
            self.entry(
                ("rail-board", index),
                icon.with_size(gpui_component::Size::Medium).flex_shrink_0().into_any_element(),
                SharedString::from(board.name.clone()),
                active,
                None,
                cx,
            )
            .group(BOARD_ROW_GROUP)
            .on_click(cx.listener(move |_, _: &ClickEvent, window, cx| {
                crate::navigation::set_active_board(window, cx, board_id.clone());
                navigate(
                    window,
                    cx,
                    Screen::BoardIssues {
                        board_id: board_id.clone(),
                    },
                );
            }))
            .when(owner && !compact, |row| {
                row.child(
                    div()
                        .invisible()
                        .group_hover(BOARD_ROW_GROUP, |style| style.visible())
                        .flex_shrink_0()
                        .child(
                            Button::new(("rail-board-settings", index))
                                .ghost().cursor_pointer()
                                .xsmall()
                                .icon(Icon::from(registry::NAV_SETTINGS))
                                .tooltip("Board settings")
                                .on_click(cx.listener(move |_, _: &ClickEvent, window, cx| {
                                    cx.stop_propagation();
                                    select_settings_section(
                                        window,
                                        cx,
                                        crate::settings::SettingsSection::Board(
                                            settings_board_id.clone(),
                                        ),
                                    );
                                    navigate(window, cx, Screen::Settings);
                                })),
                        ),
                )
            })
            .into_any_element()
        }
    }

    /// The rail's section rule — full width, like the web sidebar's.
    fn divider(&self, cx: &mut gpui::Context<Self>) -> gpui::AnyElement {
        left_column_divider(cx)
    }
}

/// The left column's section rule — full width, like the web sidebar's. The
/// rail's own dividers and the one under the FIXED header (`Shell`) are the
/// same line.
pub(crate) fn left_column_divider(cx: &App) -> gpui::AnyElement {
    div()
        .w_full()
        .h(px(1.))
        .my_1()
        .flex_shrink_0()
        .bg(cx.theme().sidebar_border)
        .into_any_element()
}

/// EXP-723: the sidebar's header row, the desktop mirror of the web
/// `SidebarHeader` (`apps/web/src/components/team/sidebar.tsx`): the team
/// switcher taking the width, then icon-only Search and New issue.
///
/// EXP-863: it is the LEFT COLUMN's header, not the rail's — the `Shell`
/// renders it ONCE, fixed, above whichever occupant (rail / settings nav /
/// `ListNav`) is sliding underneath, so the switcher, Search and New issue
/// never move. Hence a free fn over the window's navigation and no listener
/// on any view: both buttons call their openers directly with the `window`
/// the click hands them.
///
/// Both icon buttons call their openers DIRECTLY rather than dispatching
/// `OpenSearch`/`NewIssue`: a sidebar button that dispatches an App-global
/// action fires from inside the window's own update, and the handler's
/// re-entrant active-window lookup makes the click silently no-op (EXP-17 —
/// the gear was dead for exactly this). The ⌘K / keymap bindings still route
/// through the actions.
pub(crate) fn render_left_column_header(
    nav: &Entity<Navigation>,
    cx: &mut App,
) -> impl IntoElement {
    let active_team = active_team_id(nav, cx);
    let teams: Vec<(String, String, bool)> = Store::global(cx)
        .collections()
        .teams_sorted(cx)
        .into_iter()
        .map(|team| {
            let active = Some(team.id.as_str()) == active_team.as_deref();
            (team.id, team.name, active)
        })
        .collect();
    // The trigger names the ACTIVE team; nothing synced yet degrades to
    // the app letter (`team_avatar`'s own fallback) and an empty label.
    let team_name: SharedString = teams
        .iter()
        .find(|(_, _, active)| *active)
        .map(|(_, name, _)| SharedString::from(name.clone()))
        .unwrap_or_default();
    // EXP-449: `active_board_id` falls back to the team's first board, so
    // this is `None` only when nothing is in scope at all.
    let board_id = active_board_id(nav, cx);

    let switcher = Button::new("rail-team-switcher")
        .ghost().cursor_pointer()
        .small()
        .w_full()
        .h(px(40.))
        .px_1p5()
        .child(
            h_flex()
                .w_full()
                .gap_2()
                .items_center()
                .child(crate::user_avatar::team_avatar(&team_name, 28.))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .text_sm()
                        .font_weight(FontWeight::SEMIBOLD)
                        .truncate()
                        .child(team_name.clone()),
                )
                .child(
                    Icon::new(registry::NAV_TEAM_SWITCHER)
                        .xsmall()
                        .flex_shrink_0()
                        .text_color(cx.theme().muted_foreground),
                ),
        )
        .dropdown_menu_with_anchor(gpui::Anchor::TopLeft, move |menu, _window, _cx| {
            // Flat checked rows (the menu builder has no submenus); always
            // shown, even with a single team (EXP-434: no teams=1 special
            // case anywhere).
            let mut menu = menu;
            for (id, name, active) in &teams {
                menu = menu.menu_with_check(
                    SharedString::from(name.clone()),
                    *active,
                    Box::new(SwitchTeam {
                        team_id: id.clone(),
                    }),
                );
            }
            menu.separator()
                .menu_with_icon("New team", registry::UI_ADD, Box::new(CreateTeam))
                .menu_with_icon("Join team", registry::UI_INVITE, Box::new(JoinTeam))
        });

    h_flex()
        .w_full()
        .gap_1()
        .items_center()
        .child(div().flex_1().min_w_0().child(switcher))
        .child(
            // EXP-771: an icon-only ACTION button is a CIRCLE on every
            // client (an icon PICKER trigger or a swatch cell stays a
            // rounded square at the theme radius — `board_form`) — the
            // shared `web_icon_sm` 32px capsule, not a hand-sized box.
            Button::new("rail-search")
                .ghost()
                .web_icon_sm()
                .flex_shrink_0()
                .icon(registry::NAV_SEARCH)
                .tooltip("Search")
                .on_click(|_: &ClickEvent, window, cx| {
                    crate::search_sheet::open_search(window, cx)
                }),
        )
        .when_some(board_id, |row, board_id| {
            row.child(
                // EXP-771: the primary twin of the Search circle above.
                Button::new("rail-new-issue")
                    .primary()
                    .web_icon_sm()
                    .flex_shrink_0()
                    .icon(registry::NAV_CREATE_ISSUE)
                    .tooltip("New issue")
                    .on_click(move |_: &ClickEvent, window, cx| {
                        crate::create_issue_dialog::open(window, cx, board_id.clone());
                    }),
            )
        })
}

impl RailView {
    /// EXP-723: the footer's "What's new" card — the desktop mirror of the
    /// web `WhatsNewCard`. Renders while the per-install `changelogSeenId`
    /// differs from [`crate::changelog::LATEST`]'s id; the ✕ marks it seen
    /// without opening anything, a click on the card opens the dialog (which
    /// marks it seen too).
    fn render_whats_new_card(&self, cx: &mut gpui::Context<Self>) -> Option<gpui::AnyElement> {
        let seen = coding_flow::CodingHub::global(cx)
            .read(cx)
            .settings
            .changelog_seen_id
            .clone();
        if !crate::changelog::whats_new_visible(seen.as_deref()) {
            return None;
        }
        Some(
            div()
                .id("rail-whats-new")
                .w_full()
                .flex_shrink_0()
                .rounded(px(theme::tokens::radius::LG))
                .border_1()
                .border_color(theme::tokens::glass::STROKE_CARD.to_hsla())
                .bg(theme::tokens::glass::FILL_CARD.to_hsla())
                .p_3()
                .cursor_pointer()
                .hover(|this| this.bg(theme::tokens::glass::FILL_ACTIVE.to_hsla()))
                .on_click(cx.listener(|_, _: &ClickEvent, window, cx| {
                    crate::changelog::open_whats_new(window, cx);
                }))
                .child(
                    h_flex()
                        .w_full()
                        .gap_2()
                        .items_center()
                        .child(
                            Icon::new(registry::NAV_CHANGELOG)
                                .small()
                                .flex_shrink_0()
                                .text_color(cx.theme().muted_foreground),
                        )
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .text_sm()
                                .font_weight(FontWeight::MEDIUM)
                                .truncate()
                                .child("What's new"),
                        )
                        .child(
                            Button::new("rail-whats-new-dismiss")
                                .ghost().cursor_pointer()
                                .xsmall()
                                .flex_shrink_0()
                                .icon(registry::UI_CLOSE)
                                .tooltip("Dismiss")
                                .on_click(cx.listener(|_, _: &ClickEvent, _window, cx| {
                                    // Without this the dismissal ALSO opens
                                    // the dialog the card's own click handler
                                    // owns.
                                    cx.stop_propagation();
                                    crate::changelog::mark_seen(cx);
                                })),
                        ),
                )
                .child(
                    div()
                        .mt_1()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .truncate()
                        .child(crate::changelog::LATEST.summary),
                )
                .into_any_element(),
        )
    }
}

impl Render for RailView {
    fn render(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        // EXP-870: a list or the settings nav beside the rail folds it into
        // the icon column — strictly derived, never a user toggle.
        self.compact =
            crate::shell::window_left_occupant(window, cx) != crate::shell::LeftOccupant::Rail;
        // Keep the git lifecycle live regardless of which tool window is
        // open: auto-clone on board open + the attention badge both ride the
        // GitBar's load gate.
        let git_bar = self.shared.read(cx).git_bar.clone();
        git_bar.update(cx, |bar, cx| bar.ensure_loaded(window, cx));
        // Trunk anomalies (paused conflict / local commits / dirty tree —
        // each parks the autopull, EXP-346) paint the amber badge, EXCEPT
        // while a live Action run is legitimately dirtying this clone.
        let mut sc_attention = git_bar.read(cx).attention();
        if sc_attention.is_some() && git_bar.read(cx).repo_tasks_alive(window, cx) {
            sc_attention = None;
        }

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

        // Reviews badge: any open issue-linked PR in the active team — plus
        // (EXP-734) any agent run holding a chore PR of its OWN, which no
        // issue row can account for.
        let has_reviews = active_team_id(&self.nav, cx)
            .map(|id| {
                queries::has_review_issues(cx, &id)
                    || !queries::review_runs(cx, &id).is_empty()
            })
            .unwrap_or(false);
        // Inbox badge (EXP-699): any unread renderable notification — the
        // primary-tinted dot the mobile tab bars show.
        let inbox_badge = queries::inbox_unread(cx)
            .then(|| RailBadge::Dot(theme::tokens::PRIMARY.to_hsla()));
        // EXP-1001: the Agent entry carries NO live dot. My live runs are
        // rows in the rail's Running section (EXP-923) right below, each
        // with its own state dot — a second signal on the entry above them
        // was noise. (It was the EXP-699 Devices dot, moved by EXP-818.)
        // Support tool (EXP-180): rendered ONLY while the active team's
        // synced row carries helpdesk_enabled = true. The badge lights on
        // unread helpdesk activity in that team (EXP-182); primary dot like
        // Inbox (EXP-699 — unread, not a warning).
        let support_icon = helpdesk_enabled(&self.nav, cx).then(|| {
            let support_unread = active_team_id(&self.nav, cx)
                .map(|id| queries::support_unread(cx, &id))
                .unwrap_or(false);
            let support_badge =
                support_unread.then(|| RailBadge::Dot(theme::tokens::PRIMARY.to_hsla()));
            self.rail_tool_icon(
                "rail-support",
                Icon::from(icons::registry::NAV_SUPPORT),
                ToolWindow::Support,
                "Support",
                None,
                support_badge,
                cx,
            )
        });
        // EXP-878: Drafts — a CONDITIONAL entry directly under Inbox, shown
        // only while this user has drafts in the active team (or is standing
        // on the page itself, so the rail never yanks the row out from under
        // the screen you are looking at). EXP-963: it carries the pile's
        // COUNT as the muted badge the web rail wears (EXP-962) — a count
        // you parked, not an alert, which is what the muted tone says.
        let draft_count = active_team_id(&self.nav, cx)
            .map(|id| crate::drafts::drafts_in_team(&id, cx).len())
            .unwrap_or(0);
        let on_drafts = matches!(resolved_screen(&self.nav, cx), Some(Screen::Drafts));
        let drafts_entry = (draft_count > 0 || on_drafts).then(|| {
            self.rail_screen_entry(
                "rail-drafts",
                Icon::from(icons::registry::NAV_DRAFTS),
                "Drafts",
                Screen::Drafts,
                Some(RailBadge::Count(
                    draft_count,
                    crate::surface::BadgeTone::Muted,
                )),
                cx,
            )
        });
        // Getting-started entry (EXP-470/548): pinned to the rail's bottom
        // (below), rendered until every checklist entry is done.
        let getting_started_icon = crate::getting_started::getting_started_visible(&self.nav, cx)
            .then(|| self.rail_getting_started_entry(cx));

        // Projects section (EXP-253 — the top-bar board picker flattened into
        // the rail): the ACTIVE team's boards as tinted icons + "+".
        let active_team = active_team_id(&self.nav, cx);
        let boards = active_team
            .as_deref()
            .map(|team_id| Store::global(cx).collections().boards_in_team(team_id, cx))
            .unwrap_or_default();
        let board_icons: Vec<gpui::AnyElement> = boards
            .iter()
            .enumerate()
            .map(|(index, board)| self.rail_board_icon(index, board, cx))
            .collect();
        // EXP-525: the section reads like the web sidebar — a "Boards" group
        // label with a trailing `+`.
        let boards_header: Option<gpui::AnyElement> =
            active_team.clone().filter(|_| !self.compact).map(|team_id| {
                self.section_label("Boards", cx)
                    .child(
                        Button::new("rail-new-board")
                            .ghost().cursor_pointer()
                            .xsmall()
                            .icon(registry::UI_ADD)
                            .tooltip("Create board")
                            // Direct call (EXP-17): rail buttons must not
                            // dispatch App-global actions.
                            .on_click(cx.listener(move |_, _: &ClickEvent, window, cx| {
                                crate::create_board_dialog::open(window, cx, team_id.clone());
                            })),
                    )
                    .into_any_element()
            });
        // EXP-862: the action the composer is being seeded with, if any —
        // the pinned row that seeded it is active, and the Agent entry is not.
        let active_chat_action = self.active_chat_action(window, cx);
        // EXP-923: MY live runs — between the boards and "This device",
        // hidden while nothing is running.
        let running_section = self.render_running_section(cx);
        // EXP-778: the Pinned section — hidden while nothing is pinned.
        let pinned_rows = self.render_pinned_rows(active_chat_action.as_deref(), cx);
        let pinned_section: Option<gpui::AnyElement> = (!pinned_rows.is_empty()).then(|| {
            v_flex()
                .w_full()
                .gap_1()
                .when(self.compact, |section| section.items_center())
                .child(self.divider(cx))
                .when(!self.compact, |section| {
                    section.child(self.section_label("Pinned", cx))
                })
                .children(pinned_rows)
                .into_any_element()
        });

        // Source Control badge (EXP-253 — the git bar is headless now, this
        // badge is its whole rail presence): attention (conflict / local
        // commits / dirty tree) beats sticky error beats syncing; the
        // tooltip carries the attention reason or the "synced Xm ago" stamp.
        // EXP-509: real glyphs instead of colored dots — a yellow warning
        // triangle for attention, a red cross for the sticky error, and a
        // SPINNING refresh while a pull/clone runs.
        let sc_badge = if sc_attention.is_some() {
            Some(RailBadge::Icon(
                registry::UI_WARNING,
                cx.theme().warning,
            ))
        } else if git_bar.read(cx).sync_error().is_some() {
            Some(RailBadge::Icon(registry::UI_ERROR, cx.theme().danger))
        } else if git_bar.read(cx).is_syncing() {
            Some(RailBadge::Syncing)
        } else {
            None
        };
        let sc_tooltip: SharedString = if let Some(reason) = sc_attention {
            format!("Source Control: {reason}").into()
        } else if let Some(error) = git_bar.read(cx).sync_error() {
            // EXP-366: the red dot's WHY — a failed clone ("git not found on
            // PATH") used to color the dot and say nothing anywhere.
            format!("Source Control sync failed: {error}").into()
        } else if let Some(percent) = git_bar.read(cx).clone_progress() {
            format!("Source Control: cloning… {percent}%").into()
        } else {
            match git_bar.read(cx).last_synced() {
                Some(at) => {
                    let (short, _) = crate::trunk_sync::synced_ago_labels(at.elapsed());
                    if short.as_ref() == "now" {
                        "Source Control: synced just now".into()
                    } else {
                        format!("Source Control: synced {short} ago").into()
                    }
                }
                None => "Source Control".into(),
            }
        };

        // Settings gear — the SINGLE settings entry point (EXP-282 dropped
        // the duplicate account-menu item). Navigates directly for the same
        // EXP-17 reason as the search button above; the keymap still
        // dispatches `OpenSettings`. EXP-340: icon-only in BOTH rail states —
        // expanded it sits in the account row, hugging the rail's right edge,
        // instead of taking a full-width row of its own.
        let settings_entry = Button::new("rail-settings")
            .ghost().cursor_pointer()
            .small()
            .icon(registry::NAV_SETTINGS)
            .selected(matches!(
                resolved_screen(&self.nav, cx),
                Some(Screen::Settings)
            ))
            .tooltip("Settings")
            .on_click(cx.listener(|_, _: &ClickEvent, window, cx| {
                navigate(window, cx, Screen::Settings)
            }));
        // EXP-791: a new terminal, from the footer — the session bar's `+`
        // moved here, since the bar is gone while no terminal is open. A
        // direct call (EXP-17), the same shell tab cmd-t opens.
        let terminal_entry = Button::new("rail-new-terminal")
            .ghost().cursor_pointer()
            .small()
            .icon(registry::NAV_TERMINAL)
            .tooltip("New terminal")
            .on_click(cx.listener(|_, _: &ClickEvent, window, cx| {
                crate::session_bar::open_new_shell(window, cx);
            }));

        if self.compact {
            // EXP-870: the ICON column. Same destinations in the same order
            // as the expanded rail below (Inbox, the conditional Drafts entry,
            // Support, …), minus everything that needs a
            // label to mean anything (section labels, the What's-new card,
            // Getting started, the sync caption, Files/Source Control's
            // "This device" heading); the footer stacks vertically.
            return v_flex()
                .w(px(crate::shell::COMPACT_RAIL_WIDTH))
                .flex_shrink_0()
                .h_full()
                .pb_2()
                .gap_1()
                .items_center()
                .text_color(cx.theme().sidebar_foreground)
                .child(crate::scroll_pane::v_scroll_pane(
                    "rail-scroll-compact",
                    &self.rail_scroll,
                    v_flex()
                        .w_full()
                        .px_2()
                        .gap_1()
                        .items_center()
                        .child(self.rail_tool_icon(
                            "rail-inbox",
                            Icon::new(registry::NAV_INBOX),
                            ToolWindow::Inbox,
                            "Inbox",
                            None,
                            inbox_badge,
                            cx,
                        ))
                        // EXP-878: Drafts sits directly under Inbox — the
                        // personal pile before the team surfaces.
                        .children(drafts_entry)
                        .children(support_icon)
                        .child(self.rail_screen_entry(
                            "rail-devices",
                            Icon::from(icons::registry::NAV_DEVICES),
                            "Devices",
                            Screen::Devices,
                            None,
                            cx,
                        ))
                        .child(self.rail_screen_entry(
                            "rail-actions",
                            Icon::from(icons::registry::NAV_ACTIONS),
                            "Actions",
                            Screen::Actions,
                            None,
                            cx,
                        ))
                        .child(self.rail_screen_entry(
                            "rail-automations",
                            Icon::from(icons::registry::NAV_AUTOMATIONS),
                            "Automations",
                            Screen::Automations,
                            None,
                            cx,
                        ))
                        // EXP-981: Workflows sits directly after Automations,
                        // everywhere Automations appears.
                        .child(self.rail_screen_entry(
                            "rail-workflows",
                            Icon::from(icons::registry::NAV_WORKFLOWS),
                            domain::workflow_view::WORKFLOWS_TITLE,
                            Screen::Workflows,
                            None,
                            cx,
                        ))
                        .child(self.rail_screen_entry(
                            "rail-reviews",
                            Icon::from(ExpIcon::GitPullRequest),
                            "Reviews",
                            Screen::Reviews,
                            has_reviews.then(|| RailBadge::Dot(theme::tokens::GREEN.to_hsla())),
                            cx,
                        ))
                        .child(self.rail_agent_entry(active_chat_action.as_deref(), cx))
                        .children(pinned_section)
                        .child(self.divider(cx))
                        .children(board_icons)
                        .children(running_section)
                        .child(self.divider(cx))
                        .child(self.rail_tool_icon(
                            "rail-files",
                            Icon::new(registry::NAV_FILES),
                            ToolWindow::Files,
                            "Files",
                            None,
                            None,
                            cx,
                        ))
                        .child(self.rail_tool_icon(
                            "rail-source-control",
                            Icon::from(ExpIcon::GitMerge),
                            ToolWindow::SourceControl,
                            "Source Control",
                            Some(sc_tooltip),
                            sc_badge,
                            cx,
                        )),
                ))
                .child(self.render_account_button(cx))
                .child(terminal_entry)
                .child(settings_entry)
                .into_any_element();
        }

        // EXP-863: the rail starts UNDER the left column's fixed header —
        // the titlebar strip, the team switcher row and the rule beneath
        // them are the `Shell`'s (`render_left_column`), shared with the
        // settings nav and the `ListNav`, so the rail neither pads its top
        // nor renders a header of its own.
        v_flex()
            .w(px(crate::shell::LEFT_COLUMN_WIDTH))
            .flex_shrink_0()
            .h_full()
            .pb_2()
            .px_2()
            .gap_1()
            // EXP-285/EXP-293 history: the rail used to be the app's ONE
            // lighter column, painting a `FILL_SECTION` wash over the Shell's
            // sidebar-alpha ramp. EXP-723 took the wash away and gave the
            // brightness step to the CONTENT side instead: the content
            // floats a rounded `FILL_PANEL` card over the ground, so a wash
            // here would make the rail read brighter than the panel and undo
            // the cutout. EXP-767 then dropped the column's own glassier ramp
            // too. The rail paints NOTHING — it sits implicitly on the ONE
            // ground the Shell ROOT paints under the whole window, with no
            // edge where it meets the content.
            .text_color(cx.theme().sidebar_foreground)
            // Middle zone — scrollable so many boards never push the pinned
            // Settings/Account off small windows. Rail order (EXP-699, the
            // mobile tab-bar order; EXP-791 added Agent and Sessions;
            // EXP-878 the conditional Drafts entry under Inbox):
            // [Inbox, Drafts?, Support, Devices, Actions, Automations,
            //  Reviews, Agent]
            // / Pinned (EXP-778) / boards + "+" / Running (EXP-923) / This
            // device: [Files, Source Control].
            .child(crate::scroll_pane::v_scroll_pane(
                "rail-scroll",
                &self.rail_scroll,
                v_flex()
                    .w_full()
                    .gap_1()
                    .child(self.rail_tool_icon(
                        "rail-inbox",
                        Icon::new(registry::NAV_INBOX),
                        ToolWindow::Inbox,
                        "Inbox",
                        None,
                        inbox_badge,
                        cx,
                    ))
                    // EXP-878: Drafts sits directly under Inbox — the
                    // personal pile before the team surfaces.
                    .children(drafts_entry)
                    .children(support_icon)
                    // EXP-686: Devices · Actions · Automations, the three
                    // surfaces the old Agents entry bundled.
                    .child(self.rail_screen_entry(
                        "rail-devices",
                        Icon::from(icons::registry::NAV_DEVICES),
                        "Devices",
                        Screen::Devices,
                        None,
                        cx,
                    ))
                    .child(self.rail_screen_entry(
                        "rail-actions",
                        Icon::from(icons::registry::NAV_ACTIONS),
                        "Actions",
                        Screen::Actions,
                        None,
                        cx,
                    ))
                    .child(self.rail_screen_entry(
                        "rail-automations",
                        Icon::from(icons::registry::NAV_AUTOMATIONS),
                        "Automations",
                        Screen::Automations,
                        None,
                        cx,
                    ))
                    // EXP-981: Workflows sits directly after Automations,
                    // everywhere Automations appears.
                    .child(self.rail_screen_entry(
                        "rail-workflows",
                        Icon::from(icons::registry::NAV_WORKFLOWS),
                        domain::workflow_view::WORKFLOWS_TITLE,
                        Screen::Workflows,
                        None,
                        cx,
                    ))
                    // EXP-706: Reviews is a full-page screen like the three
                    // above it, not a tool window with a docked list.
                    .child(self.rail_screen_entry(
                        "rail-reviews",
                        Icon::from(ExpIcon::GitPullRequest),
                        "Reviews",
                        Screen::Reviews,
                        // Review green (EXP-214): open PRs are "stuff to do",
                        // colored like the in_review issue status.
                        has_reviews.then(|| RailBadge::Dot(theme::tokens::GREEN.to_hsla())),
                        cx,
                    ))
                    // EXP-791: "Agent", the web sidebar's word for the Chat
                    // page (EXP-772). EXP-818: it is a TOOL now — the sessions
                    // list on the left, the Chat prompt in the center until a
                    // row is clicked (the Support master-detail shape).
                    .child(self.rail_agent_entry(active_chat_action.as_deref(), cx))
                    // EXP-778: Pinned sits between the nav entries and the
                    // boards (rail order: entries / Pinned / boards / Sessions).
                    .children(pinned_section)
                    .child(self.divider(cx))
                    .children(boards_header)
                    .children(board_icons)
                    // EXP-923: Running — MY live runs, the live half of the
                    // retired top-tab group.
                    .children(running_section)
                    .child(self.divider(cx))
                    // Repo tool windows — this machine's trunk clone.
                    .child(self.section_label("This device", cx))
                    .child(self.rail_tool_icon(
                        "rail-files",
                        Icon::new(registry::NAV_FILES),
                        ToolWindow::Files,
                        "Files",
                        None,
                        None,
                        cx,
                    ))
                    .child(self.rail_tool_icon(
                        "rail-source-control",
                        Icon::from(ExpIcon::GitMerge),
                        ToolWindow::SourceControl,
                        "Source Control",
                        Some(sc_tooltip),
                        sc_badge,
                        cx,
                    )),
            ))
            // EXP-723: the web sidebar footer's order, top to bottom —
            // What's-new card, the muted Getting-started re-entry point, the
            // sync spinner, then the account row with the settings gear.
            .children(self.render_whats_new_card(cx))
            .children(getting_started_icon)
            .children(self.render_sync_indicator(cx))
            // EXP-340: one bottom row — the account button fills the width,
            // the terminal button and the gear ride its right edge.
            .child(
                h_flex()
                    .w_full()
                    .gap_1()
                    .items_center()
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .child(self.render_account_button(cx)),
                    )
                    .child(terminal_entry)
                    .child(settings_entry),
            )
            .into_any_element()
    }
}

// ---------------------------------------------------------------------------
// ListPanel — the list surfaces, full width or as the left column's ListNav
// ---------------------------------------------------------------------------

/// EXP-851: WHERE a [`ListPanel`] renders, which is the only thing that
/// differs between its two instances per window.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ListMode {
    /// The main view — the full list screen (`Screen::BoardIssues` /
    /// `Screen::Inbox` / `Screen::Support`) with its filter bar and tab strip.
    Screen,
    /// The left column's `ListNav` — a back row over the SIMPLIFIED list the
    /// open detail was picked from.
    Nav,
}

/// EXP-851: the team's list surfaces in one view. It used to be the
/// `SidebarPanel` tool column beside the centre; the centre split is gone,
/// so the same rows render EITHER as the full-width main view
/// ([`ListMode::Screen`]) or as the `ListNav` beside an open detail
/// ([`ListMode::Nav`]). One type, so the Support poll, the board query and
/// the inbox grouping exist once.
pub struct ListPanel {
    mode: ListMode,
    nav: Entity<Navigation>,
    /// [`ListMode::Nav`] only: the Inbox tab the LIST shows. It rides the
    /// SCREEN in the other mode; beside a detail there is no screen to carry
    /// it, and flipping it must not navigate.
    nav_inbox_tab: InboxTab,
    /// The origin rendered last, so the outgoing `ListNav` keeps its rows
    /// through the ~200ms left-column swap (the live origin is already gone
    /// by then).
    last_origin: Option<crate::navigation::TabOrigin>,
    /// The Board Issues tool window — the full board (filter bar with
    /// All/Active/Backlog tabs + New Issue + the grouped virtualized list
    /// with inline status/priority menus), scoped to the active board.
    /// Lives in [`RailShared`] (EXP-48 — the detail switcher reads it too).
    board_active: Entity<BoardView>,
    /// The "My Issues" tool window — same board pinned to assignee == me
    /// (also shared via [`RailShared`]).
    board_my: Entity<BoardView>,
    /// The Support tool window's open/resolved filter (EXP-180).
    support_filter: SupportFilter,
    /// Fetched `helpdesk.listThreads` result, tagged with its
    /// `(team_id, filter)` key so another team's/filter's rows never render.
    support_threads: Option<(SupportKey, Vec<api::helpdesk::SupportThreadSummary>)>,
    /// The key the current fetch + 30s poll belong to. Cleared whenever the
    /// Support tool window is inactive (like `open_pulls_key`), which also
    /// ends the poll loop on its next tick.
    support_key: Option<SupportKey>,
    /// Bumped per list fetch — a stale response checks it before landing.
    support_seq: u64,
    /// Bumped per poll spawn — at most ONE Support poll loop is ever live.
    support_poll_seq: u64,
    /// [`ListMode::Nav`] only (EXP-862): the status groups folded away in the
    /// issue lists, by `group_key` — the big list's own `collapsed` set. Per
    /// panel, never persisted.
    nav_collapsed: HashSet<String>,
    /// [`ListMode::Nav`] only (EXP-863): the issue rows' bulk selection — the
    /// big list's `selected` + `select_anchor`, with the same click grammar
    /// (Cmd/Ctrl toggles, Shift extends, the hover checkbox toggles). Cleared
    /// when the column's origin changes; pruned each render to rows that
    /// still exist.
    nav_selected: HashSet<String>,
    nav_select_anchor: Option<String>,
    /// One bulk mutation in flight at a time (the bar's buttons disable).
    nav_bulk_busy: bool,
    /// The issue ids the CURRENT render listed, in list order — every row
    /// (`nav_issue_ids`, the bulk bar's universe: a folded group's rows stay
    /// selected) and the unfolded ones (`nav_visible_ids`, the Shift-range
    /// universe). Empty while the column shows a list without issue rows.
    nav_issue_ids: Vec<String>,
    nav_visible_ids: Vec<String>,
    /// EXP-915: the `ListNav` issue lists' memoized board query — the big
    /// list's REV-39 cache. gpui re-renders this panel on every window
    /// refresh (each scroll tick, each hover flip, every Electric batch);
    /// before this the column re-ran the full clone+sort+group pipeline AND
    /// rebuilt every row element per frame, which is what made scrolling a
    /// long board's column stutter.
    nav_data: queries::Memo<NavBoardKey, queries::BoardData>,
    /// EXP-915: the scope team's resolved status vocabulary, re-derived only
    /// when the team or the `issue_statuses` shape changes.
    nav_statuses: queries::Memo<(Option<String>, u64), Vec<ResolvedStatus>>,
    /// EXP-915: the flattened rows of the CURRENT render, read by the virtual
    /// list's range closure afterwards (the big list's `rows`).
    nav_rows: Rc<Vec<NavRow>>,
    /// EXP-998: the `ListNav` issue list's blocks rail — the column width
    /// (0 = none), the open edges its lanes name, the row indices whose
    /// rail strip is hovered (any = the arrows show) and the hovered node.
    nav_rail_width: f32,
    nav_block_edges: Rc<Vec<domain::issue_rail::BlockEdge>>,
    nav_rail_hovered: HashSet<usize>,
    nav_rail_hot: Option<String>,
    /// Per-render snapshots the row builders read instead of re-resolving
    /// navigation once per row: the vocabulary, the open detail's issue and
    /// the origin a row pins.
    nav_row_statuses: Rc<Vec<ResolvedStatus>>,
    nav_active_issue_id: Option<String>,
    nav_row_origin: Option<TabOrigin>,
    /// The issue lists' virtual-list scroll — reset to the top when the
    /// column's origin (or the Inbox tab) changes, since both lists share it.
    nav_list_scroll: VirtualListScrollHandle,
    /// EXP-915: the other sidebar lists' memoized queries — the inbox
    /// grouping, the Reviews queue and the Automations log's row facts.
    inbox_data: queries::Memo<queries::InboxDataKey, queries::InboxData>,
    reviews_data: queries::Memo<queries::ReviewGroupsKey, Vec<queries::ReviewGroup>>,
    automation_facts:
        queries::Memo<queries::AutomatedRunsKey, Vec<crate::run_rows::RunListFacts>>,
    /// EXP-981: the workflows list's rows — derived when the two workflow
    /// shapes move, never per repaint.
    workflows_data: queries::Memo<queries::WorkflowDataKey, Vec<domain::rows::WorkflowRow>>,
    _subscriptions: Vec<Subscription>,
}

impl BulkSelectionHost for ListPanel {
    fn set_bulk_busy(&mut self, busy: bool) {
        self.nav_bulk_busy = busy;
    }

    fn clear_selection(&mut self, cx: &mut gpui::Context<Self>) {
        self.nav_clear_selection(cx);
    }
}


/// Fire-and-forget `notifications.markRead` over a group's unread rows (the
/// web `markGroupRead`) — the Electric echo clears the dots.
fn mark_group_read(unread_ids: &[String], cx: &mut App) {
    if unread_ids.is_empty() {
        return;
    }
    let Some(trpc) = queries::trpc_client(cx) else {
        return;
    };
    let ids = unread_ids.to_vec();
    cx.background_executor()
        .spawn(async move {
            for id in ids {
                if let Err(err) = api::notifications::notifications_mark_read(&trpc, &id) {
                    log::warn!("[ui] notifications.markRead({id}) failed: {err}");
                }
            }
        })
        .detach();
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
        // EXP-180: the helpdesk fan-out — the Support rail tool's glyph.
        Some(domain::contract::NOTIFICATION_TYPE_SUPPORT_REPLY) => {
            Icon::from(ExpIcon::MessageSquare)
        }
        // EXP-801: an agent's message — the registry's bot glyph.
        Some(domain::contract::NOTIFICATION_TYPE_AGENT_MESSAGE) => {
            Icon::new(registry::NOTIFICATION_AGENT_MESSAGE)
        }
        // EXP-980: a run that hit a rate limit — the registry's waiting glyph.
        Some(domain::contract::NOTIFICATION_TYPE_SESSION_BLOCKED) => {
            Icon::new(registry::NOTIFICATION_SESSION_BLOCKED)
        }
        _ => Icon::new(registry::NAV_NOTIFICATIONS),
    }
}

impl ListPanel {
    pub fn new(mode: ListMode, window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        let nav = nav_for_window(window, cx);
        let shared = rail_shared_for_window(window, cx);
        let git_bar = shared.read(cx).git_bar.clone();
        let board_active = shared.read(cx).board_active.clone();
        let board_my = shared.read(cx).board_my.clone();
        let collections = Store::global(cx).collections().clone();
        let local_sessions = coding_flow::LocalSessions::global(cx);
        let merge_state = crate::pr_merge::MergeState::global(cx);
        let subscriptions = vec![
            // Rail toggles swap the tool window.
            cx.observe(&shared, |_, _, cx| cx.notify()),
            // Session phase — the shared state.
            cx.observe(&Store::global(cx).state(), |_, _, cx| cx.notify()),
            // Query scoping + inbox list are live collection reads.
            cx.observe(&collections.teams, |_, _, cx| cx.notify()),
            cx.observe(&collections.boards, |_, _, cx| cx.notify()),
            cx.observe(&collections.issues, |_, _, cx| cx.notify()),
            // EXP-915: the column's lists GROUP by the team's status rows and
            // memoize on `issue_statuses.revision()` — without this observer a
            // renamed/added status only landed on the next unrelated repaint.
            cx.observe(&collections.issue_statuses, |_, _, cx| cx.notify()),
            cx.observe(&collections.notifications, |_, _, cx| cx.notify()),
            // The coding badges ride the coding_sessions shape; the local
            // Start↔Stop flip rides the process-global LocalSessions registry.
            cx.observe(&collections.coding_sessions, |_, _, cx| cx.notify()),
            cx.observe(&local_sessions, |_, _, cx| cx.notify()),
            // Sync state rides the shared trunk-sync engine.
            cx.observe(&git_bar, |_, _, cx| cx.notify()),
            // Active-row highlight follows navigation.
            cx.observe(&nav, |_, _, cx| cx.notify()),
            // EXP-874: the automated-run rows show the device (paused/label)
            // and paint the shared two-click Merge state.
            cx.observe(&collections.devices, |_, _, cx| cx.notify()),
            cx.observe(&merge_state, |_, _, cx| cx.notify()),
        ];

        Self {
            mode,
            nav,
            nav_inbox_tab: InboxTab::Inbox,
            last_origin: None,
            board_active,
            board_my,
            support_filter: SupportFilter::Open,
            support_threads: None,
            support_key: None,
            support_seq: 0,
            support_poll_seq: 0,
            nav_collapsed: HashSet::new(),
            nav_selected: HashSet::new(),
            nav_select_anchor: None,
            nav_bulk_busy: false,
            nav_issue_ids: Vec::new(),
            nav_visible_ids: Vec::new(),
            nav_data: queries::Memo::default(),
            nav_statuses: queries::Memo::default(),
            nav_rows: Rc::new(Vec::new()),
            nav_rail_width: 0.,
            nav_block_edges: Rc::new(Vec::new()),
            nav_rail_hovered: HashSet::new(),
            nav_rail_hot: None,
            nav_row_statuses: Rc::new(Vec::new()),
            nav_active_issue_id: None,
            nav_row_origin: None,
            nav_list_scroll: VirtualListScrollHandle::new(),
            inbox_data: queries::Memo::default(),
            reviews_data: queries::Memo::default(),
            automation_facts: queries::Memo::default(),
            workflows_data: queries::Memo::default(),
            _subscriptions: subscriptions,
        }
    }

    // -- shared chrome -------------------------------------------------------

    /// EXP-282: the icon-tab strip that REPLACED the icon+title header on the
    /// two tabbed tool windows (Inbox, Support). EXP-525: chips sit LEFT
    /// (web parity — the inbox/support pills are left-aligned rows there);
    /// trailing controls ride the strip's right edge absolutely. Same height
    /// as [`Self::tool_header`] so the lists below don't shift between tools.
    /// EXP-818: the strip is the SEGMENTED capsule the start-coding dialog's
    /// Issues | Actions | Chat wears (`controls::segmented`) — the tabs fill
    /// it equally — with the strip's trailing control (Filter, Mark all read)
    /// inline on its right, never floating over the capsule.
    fn tool_tab_strip(
        &self,
        tabs: Vec<gpui::AnyElement>,
        trailing: Option<gpui::AnyElement>,
        cx: &App,
    ) -> gpui::Div {
        h_flex()
            .flex_shrink_0()
            .w_full()
            .px_2()
            .py_1p5()
            .gap_1p5()
            .items_center()
            .child(
                crate::controls::segmented(cx)
                    .flex_1()
                    .min_w_0()
                    .h(px(32.))
                    .children(tabs),
            )
            .children(trailing)
    }

    /// One segment of [`Self::tool_tab_strip`] — EXP-818: a
    /// `controls::segmented_item`, the same segment every tab strip wears.
    fn tool_tab(
        &self,
        id: &'static str,
        icon: Icon,
        label: &'static str,
        selected: bool,
        cx: &mut gpui::Context<Self>,
    ) -> gpui::Stateful<gpui::Div> {
        crate::controls::segmented_item(selected, cx)
            .id(id)
            .child(icon.with_size(px(14.)))
            .child(label)
    }

    fn list_skeleton(&self, _cx: &mut gpui::Context<Self>) -> gpui::AnyElement {
        v_flex()
            .p_3()
            .gap_2()
            .child(crate::controls::skeleton().h_3p5().w_40())
            .child(crate::controls::skeleton().h_3p5().w_48())
            .child(crate::controls::skeleton().h_3p5().w_32())
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

    /// EXP-862: the origin THIS panel's rows pin ([`row_origin_for`]). In
    /// `Nav` mode that is the list the column is rendering — `last_origin`,
    /// which the render pass has already reconciled with the live one.
    fn row_origin(&self, cx: &App) -> Option<crate::navigation::TabOrigin> {
        let screen = resolved_screen(&self.nav, cx);
        row_origin_for(self.mode, self.last_origin.as_ref(), screen.as_ref())
    }

    /// EXP-862: open `screen` FROM this list — explicitly, so the left column
    /// keeps showing the rows the click came from. Every row of every mode
    /// goes through here.
    fn open_from_list(&self, screen: Screen, window: &Window, cx: &mut App) {
        match self.row_origin(cx) {
            Some(origin) => crate::navigation::navigate_from(window, cx, screen, origin),
            None => navigate(window, cx, screen),
        }
    }

    // -- issue tool windows ---------------------------------------------------

    /// *Inbox* tool window (EXP-186): the merged personal surface — an Inbox
    /// tab (notification stream) + a My Issues tab (the full board pinned to
    /// assignee == me across the team), switched by header tab buttons (the
    /// Support Open/Resolved pattern), mirroring mobile's segmented My Work
    /// screen.
    fn render_inbox_tool(&mut self, cx: &mut gpui::Context<Self>) -> gpui::AnyElement {
        let tab = self.inbox_tab(cx);
        // EXP-282: centered icon tabs instead of the icon+title header.
        let inbox_tab = self
            .tool_tab(
                "inbox-tab-inbox",
                Icon::new(registry::NAV_INBOX),
                "Inbox",
                tab == InboxTab::Inbox,
                cx,
            )
            .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                this.set_inbox_tab(InboxTab::Inbox, window, cx);
            }))
            .into_any_element();
        let mine_tab = self
            .tool_tab(
                "inbox-tab-my-issues",
                Icon::new(registry::UI_ASSIGNEE),
                "My Issues",
                tab == InboxTab::MyIssues,
                cx,
            )
            .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                this.set_inbox_tab(InboxTab::MyIssues, window, cx);
            }))
            .into_any_element();
        if tab == InboxTab::MyIssues {
            let header = self.tool_tab_strip(vec![inbox_tab, mine_tab], None, cx);
            let body = match self.mode {
                ListMode::Screen => self.my_issues_body(cx),
                ListMode::Nav => self.render_my_issues_nav(cx),
            };
            return v_flex()
                .flex_1()
                .min_h_0()
                .min_w_0()
                .child(header)
                .child(body)
                .into_any_element();
        }

        // EXP-915: the grouping runs only when a collection it reads moved —
        // it used to clone every notification on every repaint.
        let data = {
            let app: &App = cx;
            self.inbox_data
                .get_or_insert_with(queries::inbox_data_key(app), || queries::inbox(app))
        };
        // "Mark all read" is the strip's trailing control (EXP-862: a
        // borderless 32px GHOST icon button — a quiet refresh-class glyph, not
        // a primary action), only while there is something to mark.
        let mark_all_read = (data.total_unread > 0).then(|| {
            crate::controls::ghost_icon_button(
                "inbox-mark-all-read",
                Icon::from(registry::NOTIFICATION_MARK_READ),
                cx,
            )
            .tooltip("Mark all read")
            .on_click(cx.listener(|_, _: &ClickEvent, _, cx| {
                if let Some(trpc) = queries::trpc_client(cx) {
                    cx.background_executor()
                        .spawn(async move {
                            if let Err(err) =
                                api::notifications::notifications_mark_all_read(&trpc)
                            {
                                log::warn!("[ui] notifications.markAllRead failed: {err}");
                            }
                        })
                        .detach();
                }
            }))
            .into_any_element()
        });
        let header = self.tool_tab_strip(vec![inbox_tab, mine_tab], mark_all_read, cx);

        // Single Linear-style activity stream: one row per issue group, the
        // LATEST notification's type icon + sentence. (The old trailing
        // "Needs your review" section moved to the Reviews page.)
        let body: gpui::AnyElement = if !data.is_ready {
            self.list_skeleton(cx)
        } else if data.groups.is_empty() {
            self.list_note("All caught up.", cx)
        } else {
            let rows: Vec<gpui::AnyElement> = data
                .groups
                .iter()
                .map(|entry| match entry {
                    queries::InboxEntry::Issue(group) => self.inbox_issue_row(group, cx),
                    queries::InboxEntry::Support(group) => self.inbox_support_row(group, cx),
                    queries::InboxEntry::Message(entry) => self.inbox_message_row(entry, cx),
                    queries::InboxEntry::Session(entry) => self.inbox_session_row(entry, cx),
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

    /// One issue-group inbox row: the latest notification's type icon +
    /// sentence; click marks the group read and opens the issue detail.
    fn inbox_issue_row(
        &self,
        group: &queries::InboxGroup,
        cx: &mut gpui::Context<Self>,
    ) -> gpui::AnyElement {
        let theme = cx.theme();
        let theme_radius = theme.radius;
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
        let type_icon = notification_type_icon(latest.and_then(|n| n.kind.as_deref()));
        h_flex()
            .id(SharedString::from(format!("mini-inbox-{}", group.issue.id)))
            .w_full()
            .items_start()
            .gap_2()
            .px_2()
            .py_1p5()
            .rounded(theme_radius)
            .when(selected, |this| this.bg(theme.list_active))
            .hover(|this| this.bg(theme.list_hover))
            .cursor_pointer()
            .on_click(cx.listener(move |this, _, window, cx| {
                // Web `markGroupRead`: clear the group's unreads
                // (the Electric echo removes the dot), then open.
                mark_group_read(&unread_ids, cx);
                this.open_from_list(
                    Screen::IssueDetail {
                        issue_id: issue_id.clone(),
                    },
                    window,
                    cx,
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
    }

    /// One synthetic Support inbox row (EXP-180): the group's latest
    /// `support_reply` sentence under a plain "Support" label (+ the team
    /// name when the ticket team is synced — web parity). Click marks the
    /// group read and opens that team's Support tool, switching the active
    /// team first when it differs; the generic NULL-team group opens
    /// Support for the current team.
    fn inbox_support_row(
        &self,
        group: &queries::SupportInboxGroup,
        cx: &mut gpui::Context<Self>,
    ) -> gpui::AnyElement {
        let theme = cx.theme();
        let theme_radius = theme.radius;
        let unread = group.unread > 0;
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
        // Notification titles are full human sentences ("A reporter replied
        // to …") — shown verbatim.
        let sentence: SharedString = latest
            .and_then(|n| n.title.clone())
            .unwrap_or_default()
            .into();
        let team_name: Option<SharedString> = group.team_name.clone().map(Into::into);
        let target_team = group.team_id.clone();
        let type_icon =
            notification_type_icon(Some(domain::contract::NOTIFICATION_TYPE_SUPPORT_REPLY));
        h_flex()
            .id(SharedString::from(format!(
                "mini-inbox-support-{}",
                group.team_id.as_deref().unwrap_or("unknown")
            )))
            .w_full()
            .items_start()
            .gap_2()
            .px_2()
            .py_1p5()
            .rounded(theme_radius)
            .hover(|this| this.bg(theme.list_hover))
            .cursor_pointer()
            .on_click(cx.listener(move |this, _, window, cx| {
                // Web `markGroupRead`, then open the ticket team's Support
                // inbox (a cross-team group switches the window's team; the
                // NULL-team legacy group stays on the current one).
                mark_group_read(&unread_ids, cx);
                if let Some(team_id) = target_team.clone() {
                    if active_team_id(&this.nav, cx).as_deref() != Some(team_id.as_str()) {
                        switch_team(window, cx, team_id);
                    }
                }
                activate_tool(window, cx, ToolWindow::Support);
            }))
            // Leading circular type badge — the Support glyph.
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
                                    .when(unread, |this| {
                                        this.font_weight(FontWeight::MEDIUM)
                                    })
                                    // Read groups render dimmed.
                                    .text_color(if unread {
                                        theme.foreground
                                    } else {
                                        theme.muted_foreground
                                    })
                                    .child("Support"),
                            )
                            .when_some(team_name, |this, name| {
                                this.child(
                                    div()
                                        .flex_1()
                                        .min_w_0()
                                        .text_xs()
                                        .truncate()
                                        .text_color(theme.muted_foreground)
                                        .child(name),
                                )
                            }),
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
    }

    /// One agent message row (EXP-801): the bot badge, the sentence ("Ada's
    /// agent: Build finished") as the headline, the team name when synced,
    /// the body underneath. Click marks the row read — the row IS the
    /// content, there is nowhere to go.
    fn inbox_message_row(
        &self,
        entry: &queries::MessageInboxEntry,
        cx: &mut gpui::Context<Self>,
    ) -> gpui::AnyElement {
        let theme = cx.theme();
        let theme_radius = theme.radius;
        let unread = entry.unread() > 0;
        let unread_ids: Vec<String> = if unread {
            vec![entry.item.id.clone()]
        } else {
            Vec::new()
        };
        let time: SharedString = entry
            .item
            .created_at
            .as_deref()
            .map(crate::inbox::relative_time)
            .unwrap_or_default()
            .into();
        let sentence: SharedString = entry.item.title.clone().unwrap_or_default().into();
        let body: Option<SharedString> = entry
            .item
            .body
            .clone()
            .filter(|body| !body.trim().is_empty())
            .map(Into::into);
        let team_name: Option<SharedString> = entry.team_name.clone().map(Into::into);
        let type_icon =
            notification_type_icon(Some(domain::contract::NOTIFICATION_TYPE_AGENT_MESSAGE));
        h_flex()
            .id(SharedString::from(format!("mini-inbox-message-{}", entry.item.id)))
            .w_full()
            .items_start()
            .gap_2()
            .px_2()
            .py_1p5()
            .rounded(theme_radius)
            .hover(|this| this.bg(theme.list_hover))
            .cursor_pointer()
            .on_click(cx.listener(move |_, _, _, cx| {
                mark_group_read(&unread_ids, cx);
            }))
            // EXP-862: the compact inbox drops the avatar circle (web parity):
            // the type glyph alone leads the row.
            .child(
                h_flex()
                    .size_4()
                    .flex_shrink_0()
                    .items_center()
                    .justify_center()
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
                                    .flex_1()
                                    .min_w_0()
                                    .text_xs()
                                    .truncate()
                                    .when(unread, |this| this.font_weight(FontWeight::MEDIUM))
                                    .text_color(if unread {
                                        theme.foreground
                                    } else {
                                        theme.muted_foreground
                                    })
                                    .child(sentence),
                            )
                            .when_some(team_name, |this, name| {
                                this.child(
                                    div()
                                        .flex_shrink_0()
                                        .text_xs()
                                        .text_color(theme.muted_foreground)
                                        .child(name),
                                )
                            }),
                    )
                    .when_some(body, |this, body| {
                        this.child(
                            div()
                                .w_full()
                                .text_xs()
                                .truncate()
                                .text_color(theme.muted_foreground)
                                .child(body),
                        )
                    }),
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
    }

    /// One blocked-run row (EXP-980): the waiting badge, the title ("EXP-12
    /// hit a rate limit"), the team name when synced and the reason
    /// underneath. Click marks it read and opens the run — a pruned run
    /// (`session_id` NULL) only marks read.
    fn inbox_session_row(
        &self,
        entry: &queries::SessionInboxEntry,
        cx: &mut gpui::Context<Self>,
    ) -> gpui::AnyElement {
        let theme = cx.theme();
        let theme_radius = theme.radius;
        let unread = entry.unread() > 0;
        let unread_ids: Vec<String> = if unread {
            vec![entry.item.id.clone()]
        } else {
            Vec::new()
        };
        let session_id = entry.session_id().map(str::to_string);
        let target_team = entry.item.team_id.clone();
        let time: SharedString = entry
            .item
            .created_at
            .as_deref()
            .map(crate::inbox::relative_time)
            .unwrap_or_default()
            .into();
        let sentence: SharedString = entry.item.title.clone().unwrap_or_default().into();
        let body: Option<SharedString> = entry
            .item
            .body
            .clone()
            .filter(|body| !body.trim().is_empty())
            .map(Into::into);
        let team_name: Option<SharedString> = entry.team_name.clone().map(Into::into);
        let type_icon =
            notification_type_icon(Some(domain::contract::NOTIFICATION_TYPE_SESSION_BLOCKED));
        h_flex()
            .id(SharedString::from(format!("mini-inbox-session-{}", entry.item.id)))
            .w_full()
            .items_start()
            .gap_2()
            .px_2()
            .py_1p5()
            .rounded(theme_radius)
            .hover(|this| this.bg(theme.list_hover))
            .cursor_pointer()
            .on_click(cx.listener(move |this, _, window, cx| {
                // Web `markGroupRead`, then open the run itself (a
                // cross-team row switches the window's team first, like the
                // Support group). A pruned run leads nowhere.
                mark_group_read(&unread_ids, cx);
                let Some(session_id) = session_id.clone() else {
                    return;
                };
                if let Some(team_id) = target_team.clone() {
                    if active_team_id(&this.nav, cx).as_deref() != Some(team_id.as_str()) {
                        switch_team(window, cx, team_id);
                    }
                }
                crate::session_screen::open_session(&session_id, window, cx);
            }))
            .child(
                h_flex()
                    .size_4()
                    .flex_shrink_0()
                    .items_center()
                    .justify_center()
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
                                    .flex_1()
                                    .min_w_0()
                                    .text_xs()
                                    .truncate()
                                    .when(unread, |this| this.font_weight(FontWeight::MEDIUM))
                                    .text_color(if unread {
                                        theme.foreground
                                    } else {
                                        theme.muted_foreground
                                    })
                                    .child(sentence),
                            )
                            .when_some(team_name, |this, name| {
                                this.child(
                                    div()
                                        .flex_shrink_0()
                                        .text_xs()
                                        .text_color(theme.muted_foreground)
                                        .child(name),
                                )
                            }),
                    )
                    .when_some(body, |this, body| {
                        this.child(
                            div()
                                .w_full()
                                .text_xs()
                                .truncate()
                                .text_color(theme.muted_foreground)
                                .child(body),
                        )
                    }),
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
    }

    /// The Inbox tab this panel shows: the SCREEN's in [`ListMode::Screen`],
    /// the panel's own beside a detail (EXP-851).
    fn inbox_tab(&self, cx: &App) -> InboxTab {
        match (self.mode, resolved_screen(&self.nav, cx)) {
            (ListMode::Screen, Some(Screen::Inbox { tab })) => tab,
            (ListMode::Screen, _) => InboxTab::Inbox,
            (ListMode::Nav, _) => self.nav_inbox_tab,
        }
    }

    /// Switch the Inbox tab (EXP-186). As the main view that IS the screen —
    /// `set_screen`, not a navigation, so a tab flip never stacks history;
    /// in the `ListNav` it is local state and the open detail stays put.
    fn set_inbox_tab(&mut self, tab: InboxTab, window: &mut Window, cx: &mut gpui::Context<Self>) {
        if self.mode == ListMode::Nav {
            if self.nav_inbox_tab != tab {
                self.nav_inbox_tab = tab;
                self.nav_list_scroll.scroll_to_item(0, ScrollStrategy::Top);
                cx.notify();
            }
            return;
        }
        crate::navigation::set_screen(window, cx, Some(Screen::Inbox { tab }));
        cx.notify();
    }

    /// The Inbox tool window's *My Issues* tab body: the full board pinned to
    /// assignee == me across the team (its bar renders the tabs and filter).
    fn my_issues_body(&mut self, cx: &mut gpui::Context<Self>) -> gpui::AnyElement {
        let query = match (
            active_team_id(&self.nav, cx),
            queries::active_account(cx),
        ) {
            (Some(team_id), Some(account)) => IssueQuery::MyIssues {
                team_id,
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

    /// *Board Issues* tool window: the board view, relocated — filter bar
    /// (All/Active/Backlog tabs, filter popover, New Issue) + the grouped
    /// virtualized list with inline status/priority menus.
    fn render_board_issues_tool(
        &mut self,
        board_id: Option<String>,
        cx: &mut gpui::Context<Self>,
    ) -> gpui::AnyElement {
        let query = match board_id {
            Some(board_id) => IssueQuery::Board { board_id },
            None => IssueQuery::None,
        };
        self.board_active.update(cx, |board, cx| board.set_query(query, cx));
        div()
            .flex_1()
            .min_h_0()
            .min_w_0()
            .child(self.board_active.clone())
            .into_any_element()
    }

    // -- Support tool window ----------------------------------------------------

    /// *Support* tool window (EXP-180): the active team's support tickets,
    /// filtered open/resolved. Threads are server-only tRPC data — a
    /// seq-guarded background fetch keyed on `(team_id, filter)` (the
    /// `reviews_view::ReviewsView::ensure_open_pulls` pattern) plus a 30s poll
    /// that lives only while
    /// this tool window is active (`support_key` clears on tool switch, which
    /// ends the loop). Rows open the thread's center tab.
    fn render_support_tool(&mut self, cx: &mut gpui::Context<Self>) -> gpui::AnyElement {
        let team_id = active_team_id(&self.nav, cx);
        let enabled = helpdesk_enabled(&self.nav, cx);
        if enabled {
            if let Some(id) = team_id.as_deref() {
                self.ensure_support_threads(id, cx);
            }
        }
        let filter = self.support_filter;

        // EXP-282: the open/resolved filter IS the header now — icon tabs
        // (same strip as the Inbox tool), no icon+title line. EXP-525: the
        // glyphs ride the shared support-open/support-resolved concepts.
        let open_tab = self
            .tool_tab(
                "support-filter-open",
                Icon::new(registry::SUPPORT_OPEN),
                "Open",
                filter == SupportFilter::Open,
                cx,
            )
            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                this.set_support_filter(SupportFilter::Open, cx);
            }))
            .into_any_element();
        let resolved_tab = self
            .tool_tab(
                "support-filter-resolved",
                Icon::new(registry::SUPPORT_RESOLVED),
                "Resolved",
                filter == SupportFilter::Resolved,
                cx,
            )
            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                this.set_support_filter(SupportFilter::Resolved, cx);
            }))
            .into_any_element();
        let header = self.tool_tab_strip(vec![open_tab, resolved_tab], None, cx);

        let key = team_id.map(|id| (id, filter));
        let threads: Option<Vec<api::helpdesk::SupportThreadSummary>> = self
            .support_threads
            .as_ref()
            .filter(|(tagged, _)| Some(tagged) == key.as_ref())
            .map(|(_, threads)| threads.clone());

        let body: gpui::AnyElement = if !enabled {
            // The rail icon is gated on the flag, but the tool can stay
            // active across a team switch — degrade instead of a dead panel.
            self.list_note("Support is not enabled for this team.", cx)
        } else {
            match threads {
                None => self.list_skeleton(cx),
                Some(threads) if threads.is_empty() => {
                    // EXP-525: the web list empty state (LifeBuoy + wording).
                    v_flex()
                        .items_center()
                        .gap_2()
                        .px_4()
                        .py_10()
                        .text_center()
                        .child(
                            Icon::from(ExpIcon::LifeBuoy)
                                .size_6()
                                .text_color(cx.theme().muted_foreground),
                        )
                        .child(
                            div()
                                .text_sm()
                                .text_color(cx.theme().muted_foreground)
                                .child(match filter {
                                    SupportFilter::Open => "No open conversations.",
                                    SupportFilter::Resolved => "No resolved conversations yet.",
                                }),
                        )
                        .into_any_element()
                }
                Some(threads) => {
                    let rows: Vec<gpui::AnyElement> = threads
                        .iter()
                        .map(|thread| self.support_row(thread, cx))
                        .collect();
                    div()
                        .id("support-scroll")
                        .flex_1()
                        .min_h_0()
                        .overflow_y_scrollbar()
                        // EXP-818: flat rows, no gap (web `ListRow` parity).
                        .child(v_flex().p_2().children(rows))
                        .into_any_element()
                }
            }
        };

        v_flex()
            .flex_1()
            .min_h_0()
            .min_w_0()
            .child(header)
            .child(body)
            .into_any_element()
    }

    /// One Support row (EXP-715): the ticket SUBJECT leads (the iOS/Android
    /// row), reporter + latest-message preview under, relative time and an
    /// unread dot on the subject line. Click opens the thread screen.
    fn support_row(
        &self,
        thread: &api::helpdesk::SupportThreadSummary,
        cx: &mut gpui::Context<Self>,
    ) -> gpui::AnyElement {
        let theme = cx.theme();
        let fg = theme.foreground;
        let muted = theme.muted_foreground;
        // EXP-277/642: rows use the glass list fills (EXP-269 list_* tokens);
        // hover is the web `GlassRow`'s `hover:bg-glass-active/50`.
        let row_active = theme.list_active;
        let row_hover = row_active.opacity(0.5);
        // The unread dot is the white primary (web `bg-primary`).
        let unread_dot = theme::tokens::PRIMARY.to_hsla();

        let selected = matches!(
            resolved_screen(&self.nav, cx),
            Some(Screen::SupportThread { thread_id }) if thread_id == thread.id
        );
        let unread = thread.unread;
        let title: SharedString = thread.title.clone().into();
        let reporter: String = thread
            .reporter_name
            .clone()
            .filter(|name| !name.trim().is_empty())
            .or_else(|| thread.reporter_email.clone())
            .unwrap_or_else(|| "Reporter".to_string());
        let time: SharedString = thread
            .updated_at
            .as_deref()
            .map(crate::inbox::relative_time)
            .unwrap_or_default()
            .into();
        // One-line latest-PUBLIC-message preview, prefixed by the reporter
        // (the Android `reporter · body` line); newlines collapse so
        // `truncate` sees a single line. A blank body leaves the reporter
        // alone — the subject already sits on line one.
        let preview: SharedString = thread
            .last_message
            .as_ref()
            .and_then(|message| message.body.as_deref())
            .map(|body| body.split_whitespace().collect::<Vec<_>>().join(" "))
            .filter(|body| !body.is_empty())
            .map(|body| format!("{reporter} · {body}"))
            .unwrap_or(reporter)
            .into();
        let nav_id = thread.id.clone();
        let nav_title = thread.title.clone();

        crate::surface::flat_row()
            .id(SharedString::from(format!("support-{}", thread.id)))
            .flex()
            .flex_col()
            .w_full()
            .px_3()
            .py_2p5()
            .gap_0p5()
            .when(selected, |this| this.bg(row_active))
            .hover(move |this| this.bg(row_hover))
            .cursor_pointer()
            .on_click(cx.listener(move |this, _, window, cx| {
                // Seed the tab label — thread titles are tRPC-only.
                crate::support_thread::remember_title(cx, &nav_id, &nav_title);
                this.open_from_list(
                    Screen::SupportThread {
                        thread_id: nav_id.clone(),
                    },
                    window,
                    cx,
                );
            }))
            .child(
                h_flex()
                    .w_full()
                    .items_center()
                    .gap_1p5()
                    // `flex_1` + `min_w_0` — without the flex basis the
                    // truncating div collapses and renders ONLY the "…"
                    // (the EXP-175 definite-width chain, again).
                    // The subject keeps full contrast whether read or not
                    // (iOS); unread adds weight, like the mobile rows.
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_sm()
                            .truncate()
                            .when(unread, |this| this.font_weight(FontWeight::MEDIUM))
                            .text_color(fg)
                            .child(title),
                    )
                    .child(
                        div()
                            .flex_shrink_0()
                            .text_xs()
                            .text_color(muted)
                            .child(time),
                    )
                    .when(unread, |this| {
                        this.child(div().size_2().flex_shrink_0().rounded_full().bg(unread_dot))
                    }),
            )
            .child(
                div()
                    .w_full()
                    .text_xs()
                    .truncate()
                    .text_color(muted)
                    .child(preview),
            )
            .into_any_element()
    }

    /// Flip the open/resolved filter — drops the fetch key so the next
    /// render refetches (and the stale-filter rows never show: the rendered
    /// list is key-tagged).
    fn set_support_filter(&mut self, filter: SupportFilter, cx: &mut gpui::Context<Self>) {
        if self.support_filter == filter {
            return;
        }
        self.support_filter = filter;
        self.support_key = None;
        cx.notify();
    }

    /// Kick the `helpdesk.listThreads` fetch when the Support tool window is
    /// shown or the team/filter changes, and start the 30s poll for that key
    /// (the `ensure_open_pulls` pattern plus polling — tickets arrive
    /// server-side with no Electric echo).
    fn ensure_support_threads(&mut self, team_id: &str, cx: &mut gpui::Context<Self>) {
        let key: SupportKey = (team_id.to_string(), self.support_filter);
        if self.support_key.as_ref() == Some(&key) {
            return;
        }
        self.support_key = Some(key.clone());
        // Rows from another key are dropped immediately; a re-open on the
        // same key keeps rendering the previous result while refreshing.
        if self
            .support_threads
            .as_ref()
            .is_some_and(|(tagged, _)| *tagged != key)
        {
            self.support_threads = None;
        }
        self.fetch_support_threads(cx);
        self.spawn_support_poll(key, cx);
    }

    /// One seq-guarded list fetch for the CURRENT `support_key`.
    fn fetch_support_threads(&mut self, cx: &mut gpui::Context<Self>) {
        let Some(key) = self.support_key.clone() else {
            return;
        };
        let Some(trpc) = queries::trpc_client(cx) else {
            return;
        };
        self.support_seq += 1;
        let seq = self.support_seq;
        cx.spawn(async move |this, cx| {
            let (team_id, filter) = key.clone();
            let result = cx
                .background_executor()
                .spawn(async move {
                    api::helpdesk::helpdesk_list_threads(&trpc, &team_id, filter.as_str())
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.support_seq != seq || this.support_key.as_ref() != Some(&key) {
                    return;
                }
                match result {
                    Ok(threads) => {
                        this.support_threads = Some((key, threads));
                        cx.notify();
                    }
                    Err(err) => {
                        // Keep whatever rendered; the next poll retries.
                        log::warn!("[ui] helpdesk.listThreads failed: {err}");
                    }
                }
            });
        })
        .detach();
    }

    /// The 30s Support poll: entity-weak, superseded by `support_poll_seq`
    /// (at most one loop live), and self-terminating once `support_key` no
    /// longer matches — i.e. the tool window was left or re-keyed.
    fn spawn_support_poll(&mut self, key: SupportKey, cx: &mut gpui::Context<Self>) {
        self.support_poll_seq += 1;
        let generation = self.support_poll_seq;
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(std::time::Duration::from_secs(30))
                    .await;
                let keep_going = this.update(cx, |this, cx| {
                    if this.support_poll_seq != generation
                        || this.support_key.as_ref() != Some(&key)
                    {
                        return false;
                    }
                    this.fetch_support_threads(cx);
                    true
                });
                if !matches!(keep_going, Ok(true)) {
                    break;
                }
            }
        })
        .detach();
    }

    // -- ListNav bulk selection (EXP-863) -------------------------------------

    /// Toggle one row (checkbox / Cmd/Ctrl-click) and re-anchor on it.
    fn nav_toggle_selected(&mut self, issue_id: String, cx: &mut gpui::Context<Self>) {
        if !self.nav_selected.remove(&issue_id) {
            self.nav_selected.insert(issue_id.clone());
        }
        self.nav_select_anchor = Some(issue_id);
        cx.notify();
    }

    /// Shift-click: ADD the contiguous visible slice between the anchor and
    /// the target — the anchor stays put for further extensions (the big
    /// list's rule). Without a usable anchor it degrades to a plain toggle.
    fn nav_extend_selection_to(&mut self, issue_id: String, cx: &mut gpui::Context<Self>) {
        let Some((from, to)) = selection_range(
            &self.nav_visible_ids,
            self.nav_select_anchor.as_deref(),
            &issue_id,
        ) else {
            return self.nav_toggle_selected(issue_id, cx);
        };
        for id in &self.nav_visible_ids[from..=to] {
            self.nav_selected.insert(id.clone());
        }
        cx.notify();
    }

    fn nav_clear_selection(&mut self, cx: &mut gpui::Context<Self>) {
        if self.nav_selected.is_empty() && self.nav_select_anchor.is_none() {
            return;
        }
        self.nav_selected.clear();
        self.nav_select_anchor = None;
        cx.notify();
    }

    /// The bulk bar over the `ListNav`'s selection, or `None` while nothing
    /// is selected (or the column lists no issue rows / no team is in
    /// scope). The ids come off the CURRENT render's rows, in list order and
    /// pruned to rows that still exist — the big list's `bulk_bar` rule.
    fn nav_bulk_bar(&mut self, cx: &mut gpui::Context<Self>) -> Option<gpui::AnyElement> {
        if self.nav_selected.is_empty() || self.nav_issue_ids.is_empty() {
            return None;
        }
        let ids: Vec<String> = self
            .nav_issue_ids
            .iter()
            .filter(|id| self.nav_selected.contains(id.as_str()))
            .cloned()
            .collect();
        if ids.is_empty() {
            return None;
        }
        let team_id = active_team_id(&self.nav, cx)?;
        // Icon-only and allowed to fold: the column is 264px wide.
        Some(render_bulk_bar(team_id, ids, self.nav_bulk_busy, false, true, cx))
    }

    /// EXP-863: the scope team's resolved status vocabulary, ONCE per render
    /// — every row's inline status menu and context menu read this snapshot
    /// (the big list's `team_statuses`). EXP-915: memoized on the team and
    /// the `issue_statuses` revision.
    fn nav_team_statuses(&mut self, cx: &App) -> Rc<Vec<ResolvedStatus>> {
        let team_id = active_team_id(&self.nav, cx);
        let revision = Store::global(cx)
            .collections()
            .issue_statuses
            .read(cx)
            .revision();
        let key = (team_id.clone(), revision);
        self.nav_statuses.get_or_insert_with(key, || {
            team_id
                .as_deref()
                .map(|team_id| queries::team_status_options(cx, team_id))
                .unwrap_or_else(domain::statuses::default_resolved_statuses)
        })
    }

    // -- ListNav bodies (EXP-851) --------------------------------------------

    /// The `ListNav`'s back row — the SETTINGS nav's row, shared
    /// (`settings::nav_back_row`), labelled with the list the detail came
    /// from and hopping one layer outward: the main view becomes that list
    /// screen and the left column becomes the rail.
    fn nav_back_row(
        &self,
        origin: &crate::navigation::TabOrigin,
        cx: &mut gpui::Context<Self>,
    ) -> gpui::AnyElement {
        let board_name = origin.board_id.as_deref().and_then(|board_id| {
            Store::global(cx)
                .collections()
                .boards
                .read(cx)
                .get(board_id)
                .map(|board| board.name.clone())
        });
        let label: SharedString = board_name
            .map(SharedString::from)
            .unwrap_or_else(|| origin.tool.list_label().into());
        let target = origin.tool.origin_screen(origin.board_id.clone());
        crate::settings::nav_back_row("list-nav-back", label, cx)
            .on_click(cx.listener(move |_, _: &ClickEvent, window, cx| {
                crate::navigation::go_back_to(window, cx, target.clone());
            }))
            .into_any_element()
    }

    /// The board `ListNav` body: the board's issues as plain rows — status
    /// glyph, identifier, title — with the open detail highlighted.
    fn render_board_nav(
        &mut self,
        board_id: Option<String>,
        cx: &mut gpui::Context<Self>,
    ) -> gpui::AnyElement {
        let Some(board_id) = board_id else {
            return self.list_note("No board selected.", cx);
        };
        self.render_nav_issue_list(
            IssueQuery::Board { board_id },
            ("list-nav-board-scroll", "list-nav-board-rows"),
            "No issues yet.",
            cx,
        )
    }

    /// The My Issues `ListNav` body — the same plain rows over the team-wide
    /// assignee query.
    fn render_my_issues_nav(&mut self, cx: &mut gpui::Context<Self>) -> gpui::AnyElement {
        let (Some(team_id), Some(account)) =
            (active_team_id(&self.nav, cx), queries::active_account(cx))
        else {
            return self.list_note("Nothing assigned.", cx);
        };
        self.render_nav_issue_list(
            IssueQuery::MyIssues {
                team_id,
                user_id: account.user_id,
            },
            ("list-nav-mine-scroll", "list-nav-mine-rows"),
            "Nothing assigned to you.",
            cx,
        )
    }

    /// EXP-915: the shared body of the two `ListNav` issue lists — the
    /// memoized board query ([`Self::nav_data`], keyed by scope + every
    /// collection input), flattened into [`NavRow`]s and drawn by a
    /// `v_virtual_list` (the big list's element), so a scroll tick lays out
    /// the visible rows only. It used to run the query and build EVERY row
    /// per frame.
    fn render_nav_issue_list(
        &mut self,
        query: IssueQuery,
        (scroll_id, rows_id): (&'static str, &'static str),
        empty_copy: &'static str,
        cx: &mut gpui::Context<Self>,
    ) -> gpui::AnyElement {
        let data = {
            let app: &App = cx;
            let key = NavBoardKey {
                query: query.clone(),
                base: queries::board_data_key(app),
            };
            self.nav_data
                .get_or_insert_with(key, || query.board_data(app))
        };
        if !data.is_ready {
            return self.list_skeleton(cx);
        }
        self.nav_prepare_rows(&data, cx);
        if self.nav_rows.is_empty() {
            return self.list_note(empty_copy, cx);
        }
        let sizes: Rc<Vec<Size<Pixels>>> = Rc::new(
            self.nav_rows
                .iter()
                .map(|row| size(px(0.), row.height()))
                .collect(),
        );
        div()
            .flex_1()
            .min_h_0()
            .min_w_0()
            .child(
                v_flex()
                    .id(scroll_id)
                    .relative()
                    .size_full()
                    .child(
                        v_virtual_list(
                            cx.entity().clone(),
                            rows_id,
                            sizes,
                            |this, visible_range, window, cx| {
                                visible_range
                                    .map(|ix| this.nav_render_row(ix, window, cx))
                                    .collect()
                            },
                        )
                        .track_scroll(&self.nav_list_scroll),
                    )
                    .scrollbar(
                        &self.nav_list_scroll,
                        gpui_component::scroll::ScrollbarAxis::Vertical,
                    ),
            )
            .into_any_element()
    }

    /// One virtual-list item of the `ListNav` issue lists (EXP-915): the row
    /// at `ix` in the column's old `px_2` gutter, with its trailing gap.
    fn nav_render_row(
        &mut self,
        ix: usize,
        _window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> gpui::AnyElement {
        let rows = self.nav_rows.clone();
        let Some(row) = rows.get(ix) else {
            return div().into_any_element();
        };
        let inner = match row {
            NavRow::Header {
                status,
                count,
                collapsed,
                rail,
            } => self.nav_group_header(status, *count, *collapsed, rail, cx),
            NavRow::Issue {
                index,
                issue,
                guides,
                rail,
            } => {
                let statuses = self.nav_row_statuses.clone();
                let any_selected = !self.nav_selected.is_empty();
                let guides = guides.clone();
                self.nav_issue_row(*index, ix, issue, &statuses, any_selected, &guides, rail, cx)
            }
        };
        div()
            .h(row.height())
            .w_full()
            .min_w_0()
            .px_2()
            .pb(px(NAV_ROW_GAP))
            .child(inner)
            .into_any_element()
    }

    /// EXP-862: the `ListNav`'s issue rows, grouped by STATUS exactly like the
    /// full-width list — a group band (glyph, name, count over the status'
    /// own tint) that folds its rows away when clicked. The left column used
    /// to be an undifferentiated run of issues, which is the one thing the
    /// big list never was; web's `board-issue-list-pane.tsx` got the same
    /// header in this wave.
    fn nav_prepare_rows(&mut self, data: &queries::BoardData, cx: &mut gpui::Context<Self>) {
        let groups = &data.groups;
        let counts = &data.block_counts;
        // EXP-863: the selection's universe for this render — every listed
        // id (the bulk bar's, folded groups included) and the unfolded ones
        // (the Shift-range's); a selected id whose row left the data set
        // (deleted elsewhere, moved board) drops out, the big list's prune.
        self.nav_issue_ids = groups
            .iter()
            .flat_map(|group| group.issues.iter().map(|issue| issue.id.clone()))
            .collect();
        self.nav_visible_ids = groups
            .iter()
            .filter(|group| !self.nav_collapsed.contains(&group.status.group_key))
            .flat_map(|group| group.issues.iter().map(|issue| issue.id.clone()))
            .collect();
        if !self.nav_selected.is_empty() {
            let present: HashSet<&str> = self.nav_issue_ids.iter().map(String::as_str).collect();
            self.nav_selected.retain(|id| present.contains(id.as_str()));
        }
        // EXP-915: the per-render snapshots the row builders read.
        self.nav_row_statuses = self.nav_team_statuses(cx);
        self.nav_active_issue_id = match resolved_screen(&self.nav, cx) {
            Some(Screen::IssueDetail { issue_id }) => Some(issue_id),
            _ => None,
        };
        self.nav_row_origin = self.row_origin(cx);

        let mut rows: Vec<NavRow> = Vec::new();
        // The row ids number the ISSUES, so folding a group never renumbers
        // the rows below it into each other's element state.
        let mut index = 0usize;
        for group in groups {
            // Empty groups are already hidden by the query (web parity).
            if group.issues.is_empty() {
                continue;
            }
            let collapsed = self.nav_collapsed.contains(&group.status.group_key);
            rows.push(NavRow::Header {
                status: Box::new(group.status.clone()),
                count: group.issues.len(),
                collapsed,
                rail: domain::issue_rail::RailRow::default(),
            });
            if collapsed {
                index += group.issues.len();
                continue;
            }
            // EXP-980: the connector off this group's own depth sequence.
            let guides = domain::tree_guides::guides_for(&group.depths);
            for (position, issue) in group.issues.iter().enumerate() {
                rows.push(NavRow::Issue {
                    index,
                    issue: issue.clone(),
                    guides: guides.get(position).cloned().unwrap_or_default(),
                    rail: domain::issue_rail::RailRow::default(),
                });
                index += 1;
            }
        }
        // EXP-998: the blocks rail over the visible entries (the big list's
        // `render` does the same).
        let entries: Vec<domain::issue_rail::RailEntry<'_>> = rows
            .iter()
            .map(|row| match row {
                NavRow::Header { .. } => domain::issue_rail::RailEntry::Gap,
                NavRow::Issue { issue, .. } => domain::issue_rail::RailEntry::Row(issue.id.as_str()),
            })
            .collect();
        let rail = domain::issue_rail::issue_rail(&entries, &data.block_edges, counts);
        self.nav_rail_width = rail.width();
        for (row, slice) in rows.iter_mut().zip(rail.entries) {
            match row {
                NavRow::Header { rail, .. } | NavRow::Issue { rail, .. } => *rail = slice,
            }
        }
        self.nav_block_edges = data.block_edges.clone();
        self.nav_rail_hovered.retain(|&ix| ix < rows.len());
        // DEV-ONLY: `EXP_DEV_RAIL_HOT=<issue uuid>` photographs the rail open
        // (the big list's hook, for the column) — pinned every render.
        if let Ok(hot) = std::env::var("EXP_DEV_RAIL_HOT") {
            if rows.iter().any(|row| matches!(row, NavRow::Issue { issue, .. } if issue.id == hot)) {
                self.nav_rail_hovered.insert(usize::MAX);
                self.nav_rail_hot = Some(hot);
            }
        }
        self.nav_rows = Rc::new(rows);
    }

    /// EXP-998: one `ListNav` entry's lanes of the rail, painted only while
    /// the pointer is on the rail.
    fn nav_rail_lanes(
        &self,
        rail: &domain::issue_rail::RailRow,
        cx: &App,
    ) -> Option<gpui::AnyElement> {
        crate::issue_rail::rail_lanes_layer(
            &crate::issue_rail::RailPaint {
                row: rail,
                edges: &self.nav_block_edges,
                width: self.nav_rail_width,
                right_pad: NAV_ROW_PAD,
                gap: NAV_ROW_GAP,
                open: !self.nav_rail_hovered.is_empty(),
                hot: self.nav_rail_hot.as_deref(),
            },
            cx,
        )
    }

    /// One `ListNav` status band — the big list's group header
    /// (`issue_list::render_group_header`) at the narrow column's density: a
    /// bare chevron, the status glyph, its name and the count, over a wash in
    /// the status' own hue. The WHOLE band folds the group (EXP-862 ×4 — the
    /// chevron is a disclosure marker, not a separate target).
    fn nav_group_header(
        &self,
        status: &domain::statuses::ResolvedStatus,
        count: usize,
        collapsed: bool,
        rail: &domain::issue_rail::RailRow,
        cx: &mut gpui::Context<Self>,
    ) -> gpui::AnyElement {
        let group_key = status.group_key.clone();
        let muted = cx.theme().muted_foreground;
        h_flex()
            .id(SharedString::from(format!("list-nav-group-{group_key}")))
            .w_full()
            .h(px(24.))
            .px_1p5()
            // EXP-998: the rail's lanes cross the band (the band's own 6px
            // side padding is what the layer sits inside).
            .relative()
            .children({
                let paint = crate::issue_rail::RailPaint {
                    row: rail,
                    edges: &self.nav_block_edges,
                    width: self.nav_rail_width,
                    right_pad: 6.,
                    gap: NAV_ROW_GAP,
                    open: !self.nav_rail_hovered.is_empty(),
                    hot: self.nav_rail_hot.as_deref(),
                };
                crate::issue_rail::rail_lanes_layer(&paint, cx)
            })
            .gap_1p5()
            .items_center()
            .rounded(cx.theme().radius)
            .cursor_pointer()
            // EXP-293's `statusHeaderBg`: a wash in the status' hue so the
            // band reads as a divider and not as one more issue row.
            .bg(crate::icons::status_tint_color(&status.tint, cx).opacity(0.07))
            .child(
                Icon::new(if collapsed {
                    registry::UI_CHEVRON_RIGHT
                } else {
                    registry::UI_CHEVRON_DOWN
                })
                .with_size(px(12.))
                .flex_shrink_0()
                .text_color(muted),
            )
            .child(crate::icons::resolved_status_icon(status, cx).xsmall())
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_xs()
                    .font_weight(FontWeight::MEDIUM)
                    .child(SharedString::from(status.name.clone())),
            )
            .child(
                div()
                    .flex_shrink_0()
                    .text_xs()
                    .text_color(muted)
                    .child(SharedString::from(count.to_string())),
            )
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                if !this.nav_collapsed.remove(&group_key) {
                    this.nav_collapsed.insert(group_key.clone());
                }
                cx.notify();
            }))
            .into_any_element()
    }

    /// One `ListNav` issue row (spec C: status glyph, identifier, title; the
    /// open detail highlighted). A plain click navigates to that detail —
    /// the `ListNav` stays, because the origin is inherited from the detail
    /// already open (`navigation::derive_origin`).
    ///
    /// EXP-863: the big list's row grammar at the column's density — a
    /// hover-revealed bulk-select checkbox (pinned visible while ANY row is
    /// selected), the status glyph as the INLINE status menu
    /// (`issue_list::status_dropdown` in its click-swallowing cell, so
    /// picking a status never navigates), Cmd/Ctrl-click toggles, Shift-click
    /// extends, and the right-click menu is the main list's
    /// (`build_row_context_menu`).
    #[allow(clippy::too_many_arguments)]
    fn nav_issue_row(
        &self,
        index: usize,
        ix: usize,
        issue: &Rc<Issue>,
        statuses: &Rc<Vec<ResolvedStatus>>,
        any_selected: bool,
        guides: &domain::tree_guides::Guides,
        rail: &domain::issue_rail::RailRow,
        cx: &mut gpui::Context<Self>,
    ) -> gpui::AnyElement {
        let screen = Screen::IssueDetail {
            issue_id: issue.id.clone(),
        };
        // EXP-915: off the render's snapshot, not a per-row navigation read.
        let active = self.nav_active_issue_id.as_deref() == Some(issue.id.as_str());
        let selected = self.nav_selected.contains(&issue.id);
        let toggle_id = issue.id.clone();
        let lead = h_flex()
            .flex_shrink_0()
            .gap_1()
            .items_center()
            .child(
                control_cell(row_id("nav-select-cell", &issue.id))
                    .w_4()
                    .when(!any_selected, |cell| {
                        cell.invisible().group_hover(NAV_ROW_GROUP, |style| style.visible())
                    })
                    .child(
                        crate::controls::checkbox(
                            row_id("nav-select", &issue.id),
                            selected.into(),
                            false,
                            cx,
                        )
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.nav_toggle_selected(toggle_id.clone(), cx);
                        })),
                    ),
            )
            .child(
                control_cell(row_id("nav-status-cell", &issue.id))
                    .w_6()
                    .child(status_dropdown(issue, statuses, cx)),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .font_family(theme::terminal::FONT_FAMILY)
                    .child(SharedString::from(issue.identifier.clone())),
            )
            .into_any_element();
        let title = issue.title.trim();
        let title = if title.is_empty() {
            SharedString::from("Untitled")
        } else {
            SharedString::from(title.to_string())
        };
        let click_id = issue.id.clone();
        let menu_issue = issue.clone();
        let menu_statuses = statuses.clone();
        let menu_origin = self.nav_row_origin.clone();
        // A bulk-selected row wears the same active fill as the open one —
        // both mean "this row is where you are" (EXP-426).
        rail_row_lead(
            ("list-nav-issue", index),
            lead,
            title,
            active || selected,
            None,
            None,
            cx,
        )
        // EXP-980: the sub-issue indent + EXP-965's elbow, at the narrow
        // column's 8px gutter (the rail's running rows' geometry).
        .relative()
        .pl(px(NAV_ROW_PAD + crate::tree_guides::LEVEL_PITCH * guides.depth() as f32))
        .children(crate::tree_guides::guide_layer(
            guides,
            NAV_ROW_PAD + NAV_GUIDE_INSET,
            NAV_ROW_GAP,
        ))
        // EXP-998: the blocks rail (the pill's successor) — the reserved
        // column, the hover strip, the lanes and the node.
        .when(self.nav_rail_width > 0., |row| {
            row.child(div().flex_shrink_0().w(px(self.nav_rail_width)))
        })
        .children(crate::issue_rail::rail_strip(
            row_id("nav-rail-strip", &issue.id),
            self.nav_rail_width,
            NAV_ROW_PAD,
            cx.listener(move |this, hovered: &bool, _, cx| {
                let changed = if *hovered {
                    this.nav_rail_hovered.insert(ix)
                } else {
                    this.nav_rail_hovered.remove(&ix)
                };
                if changed {
                    cx.notify();
                }
            }),
        ))
        .children(self.nav_rail_lanes(rail, cx))
        .children({
            let hot_id = issue.id.clone();
            crate::issue_rail::rail_node(
                format!("nav-rail-node-{}", issue.id),
                &issue.id,
                rail,
                NAV_ROW_PAD,
                cx.listener(move |this, hovered: &bool, _, cx| {
                    let next = hovered.then(|| hot_id.clone());
                    if this.nav_rail_hot != next {
                        this.nav_rail_hot = next;
                        cx.notify();
                    }
                }),
                cx,
            )
        })
        .group(NAV_ROW_GROUP)
        .on_click(cx.listener(move |this, event: &ClickEvent, window, cx| {
            let modifiers = event.modifiers();
            if modifiers.secondary() {
                this.nav_toggle_selected(click_id.clone(), cx);
                return;
            }
            if modifiers.shift {
                this.nav_extend_selection_to(click_id.clone(), cx);
                return;
            }
            this.open_from_list(screen.clone(), window, cx);
        }))
        .context_menu(move |menu, window, cx| {
            build_row_context_menu(
                menu,
                &menu_issue,
                &menu_statuses,
                menu_origin.clone(),
                window,
                cx,
            )
        })
        .into_any_element()
    }

    /// The Reviews `ListNav` body: the open-PR queue's rows, each opening its
    /// diff (the Reviews page's own click target).
    fn render_reviews_nav(&mut self, cx: &mut gpui::Context<Self>) -> gpui::AnyElement {
        let Some(team_id) = active_team_id(&self.nav, cx) else {
            return self.list_note("No team selected.", cx);
        };
        // EXP-915: the queue is grouped only when issues/boards moved.
        let groups = {
            let app: &App = cx;
            self.reviews_data
                .get_or_insert_with(queries::review_groups_key(app, &team_id), || {
                    queries::review_groups(app, &team_id)
                })
        };
        let open_issue = match resolved_screen(&self.nav, cx) {
            Some(Screen::PrDiff { issue_id }) => Some(issue_id),
            _ => None,
        };
        let rows: Vec<gpui::AnyElement> = groups
            .iter()
            .flat_map(|group| group.entries.iter())
            .enumerate()
            .map(|(index, entry)| {
                let issue = entry.representative();
                let screen = Screen::PrDiff {
                    issue_id: issue.id.clone(),
                };
                let active = open_issue.as_deref() == Some(issue.id.as_str());
                let lead = div()
                    .flex_shrink_0()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .font_family(theme::terminal::FONT_FAMILY)
                    .child(SharedString::from(issue.identifier.clone()))
                    .into_any_element();
                rail_row_lead(
                    ("list-nav-review", index),
                    lead,
                    SharedString::from(issue.title.clone()),
                    active,
                    None,
                    None,
                    cx,
                )
                .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                    this.open_from_list(screen.clone(), window, cx);
                }))
                .into_any_element()
            })
            .collect();
        if rows.is_empty() {
            return self.list_note("No open pull requests.", cx);
        }
        self.nav_scroll("list-nav-reviews-scroll", rows, cx)
    }

    /// The Automations `ListNav` body (EXP-862): this team's automated runs,
    /// newest first — the same rows and the same projection
    /// ([`queries::automated_runs`]) the Automations page's "Recent automated
    /// runs" log draws, with the open run selected. Opening a finished
    /// automated run is the one path that lands here, and its Back goes to
    /// the Automations page.
    fn render_automations_nav(&mut self, cx: &mut gpui::Context<Self>) -> gpui::AnyElement {
        let team_id = active_team_id(&self.nav, cx);
        let now_secs = chrono::Utc::now().timestamp();
        // EXP-915: the log's rows are derived when a collection they read
        // moves or the 5s clock ticks — never per repaint (every run row was
        // cloned and re-captioned on each scrolled pixel).
        let facts = {
            let app: &App = cx;
            let key = queries::automated_runs_key(app, team_id.as_deref(), now_secs);
            self.automation_facts.get_or_insert_with(key, || {
                queries::automated_runs(app, team_id.as_deref())
                    .iter()
                    .map(|session| {
                        crate::run_rows::RunListFacts::derive(session, now_secs, app)
                    })
                    .collect()
            })
        };
        if facts.is_empty() {
            return self.list_note("Nothing has fired yet.", cx);
        }
        let open_session = match resolved_screen(&self.nav, cx) {
            Some(Screen::Session { session_id }) => Some(session_id),
            _ => None,
        };
        let origin = self.row_origin(cx);
        let mut rows: Vec<gpui::AnyElement> = Vec::with_capacity(facts.len());
        for (index, facts) in facts.iter().cloned().enumerate() {
            let open_id = facts.session_id().to_string();
            let active = open_session.as_deref() == Some(open_id.as_str());
            let origin = origin.clone();
            // EXP-874: the shared run rows (live → running row, ended → past
            // row); automated runs are flat HERE (depth 0, no fold) — the
            // Automations page is where the run tree nests (EXP-897).
            rows.push(crate::run_rows::render_run_list_row(
                "list-nav-automation",
                index,
                domain::tree_guides::Guides::default(),
                None,
                facts,
                active,
                Box::new(move |_, window, cx| {
                    crate::session_screen::open_session_with_origin(
                        &open_id,
                        origin.clone(),
                        window,
                        cx,
                    );
                }),
                cx,
            ));
        }
        self.nav_scroll("list-nav-automations-scroll", rows, cx)
    }

    /// The shared `ListNav` scroll body.
    /// The Workflows `ListNav` body (EXP-981): this team's workflows in the
    /// same three bands the page draws, with the open one selected. Opening
    /// a workflow from the page is the one path that lands here, and its
    /// Back goes to the Workflows page.
    fn render_workflows_nav(&mut self, cx: &mut gpui::Context<Self>) -> gpui::AnyElement {
        use domain::workflow_view::{workflow_band, workflow_shape_line, WORKFLOW_BANDS};

        let Some(team_id) = active_team_id(&self.nav, cx) else {
            return self.list_note("No team selected.", cx);
        };
        // EXP-915's rule: derived when a collection the rows read moves,
        // never per repaint (the nav re-renders on every scrolled pixel).
        let workflows = {
            let app: &App = cx;
            let key = queries::workflow_data_key(app, Some(team_id.as_str()), None);
            self.workflows_data.get_or_insert_with(key, || {
                queries::team_workflows(app, &team_id).0
            })
        };
        if workflows.is_empty() {
            return self.list_note("No workflows yet.", cx);
        }
        let open_workflow = match resolved_screen(&self.nav, cx) {
            Some(Screen::Workflow { workflow_id }) => Some(workflow_id),
            _ => None,
        };
        let theme = cx.theme();
        let (row_hover, row_active, muted, danger) = (
            theme.list_hover,
            theme.list_active,
            theme.muted_foreground,
            theme.danger,
        );
        let mut rows: Vec<gpui::AnyElement> = Vec::with_capacity(workflows.len());
        for band in WORKFLOW_BANDS {
            let banded: Vec<&domain::rows::WorkflowRow> = workflows
                .iter()
                .filter(|row| workflow_band(row.status_wire()) == band)
                .collect();
            // An empty band is hidden, never an empty heading.
            if banded.is_empty() {
                continue;
            }
            rows.push(
                crate::surface::glass_section_band(None, band.title(), None, cx).into_any_element(),
            );
            for (index, row) in banded.iter().enumerate() {
                let shape = row.shape();
                let active = open_workflow.as_deref() == Some(row.id.as_str());
                let name = SharedString::from(row.name.clone().unwrap_or_default());
                let line = SharedString::from(workflow_shape_line(&shape));
                let cycles = !shape.cycles.is_empty();
                let open_id = row.id.clone();
                rows.push(
                    crate::surface::flat_row()
                        .id((crate::workflows_view::band_row_id(band), index))
                        .flex()
                        .w_full()
                        .min_w_0()
                        .items_center()
                        .gap_2()
                        .px_2()
                        .py_1p5()
                        .cursor_pointer()
                        .when(active, |this| this.bg(row_active))
                        .hover(move |style| style.bg(row_hover))
                        .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                            this.open_from_list(
                                Screen::Workflow {
                                    workflow_id: open_id.clone(),
                                },
                                window,
                                cx,
                            );
                        }))
                        .child(
                            div().flex_shrink_0().child(
                                Icon::from(icons::registry::NAV_WORKFLOWS)
                                    .xsmall()
                                    .text_color(muted),
                            ),
                        )
                        .child(
                            v_flex()
                                .flex_1()
                                .min_w_0()
                                .child(div().w_full().min_w_0().truncate().text_sm().child(name))
                                .child(
                                    div()
                                        .w_full()
                                        .min_w_0()
                                        .truncate()
                                        .text_xs()
                                        .text_color(muted)
                                        .child(line),
                                ),
                        )
                        .when(cycles, |this| {
                            this.child(div().flex_shrink_0().child(
                                Icon::from(icons::registry::UI_WARNING).xsmall().text_color(danger),
                            ))
                        })
                        .into_any_element(),
                );
            }
        }
        self.nav_scroll("list-nav-workflows-scroll", rows, cx)
    }

    fn nav_scroll(
        &self,
        id: &'static str,
        rows: Vec<gpui::AnyElement>,
        _cx: &mut gpui::Context<Self>,
    ) -> gpui::AnyElement {
        div()
            .id(id)
            .flex_1()
            .min_h_0()
            .min_w_0()
            .overflow_y_scrollbar()
            .child(v_flex().w_full().min_w_0().px_2().gap_0p5().children(rows))
            .into_any_element()
    }

    /// The `ListNav`'s body for `origin` (spec C's table).
    fn render_nav_body(
        &mut self,
        origin: &crate::navigation::TabOrigin,
        cx: &mut gpui::Context<Self>,
    ) -> gpui::AnyElement {
        match origin.tool {
            ToolWindow::BoardIssues => {
                let board_id = origin
                    .board_id
                    .clone()
                    .or_else(|| active_board_id(&self.nav, cx));
                self.render_board_nav(board_id, cx)
            }
            // Strip + rows for both tabs (`render_inbox_tool` picks the
            // simplified body in Nav mode).
            ToolWindow::Inbox => self.render_inbox_tool(cx),
            ToolWindow::Support => self.render_support_tool(cx),
            ToolWindow::Reviews => self.render_reviews_nav(cx),
            ToolWindow::Automations => self.render_automations_nav(cx),
            ToolWindow::Workflows => self.render_workflows_nav(cx),
            // Files / Source Control are not list ORIGINS (`Screen::list_origin`).
            ToolWindow::Files | ToolWindow::SourceControl => div().into_any_element(),
        }
    }
}

impl Render for ListPanel {
    fn render(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let screen = resolved_screen(&self.nav, cx);
        // Leaving the Support list drops its fetch key — the next open
        // refetches, and the 30s poll loop dies on its next tick.
        let shows_support = match self.mode {
            ListMode::Screen => matches!(screen, Some(Screen::Support)),
            ListMode::Nav => matches!(
                crate::shell::list_nav_origin(window, cx).map(|origin| origin.tool),
                Some(ToolWindow::Support)
            ),
        };
        if !shows_support {
            self.support_key = None;
        }
        let body = match self.mode {
            ListMode::Screen => match screen {
                // The tab strip lives inside the view; it reads the screen.
                Some(Screen::Inbox { .. }) => self.render_inbox_tool(cx),
                Some(Screen::BoardIssues { board_id }) => {
                    let board_id = (!board_id.is_empty())
                        .then_some(board_id)
                        .or_else(|| active_board_id(&self.nav, cx));
                    self.render_board_issues_tool(board_id, cx)
                }
                Some(Screen::Support) => self.render_support_tool(cx),
                // The panel is only mounted for the three list screens.
                _ => div().into_any_element(),
            },
            ListMode::Nav => {
                // The live origin is gone the moment the swap starts, so the
                // outgoing column keeps rendering the one it had.
                let origin = crate::shell::list_nav_origin(window, cx);
                if let Some(origin) = origin.clone() {
                    if self.last_origin.as_ref() != Some(&origin) {
                        // A NEW origin reseeds the local Inbox tab (the tab
                        // the detail was picked from); flipping it afterwards
                        // is the reader's business, not the origin's.
                        if let Some(tab) = origin.inbox_tab {
                            self.nav_inbox_tab = tab;
                        }
                        self.last_origin = Some(origin);
                        // EXP-863: the selection is per list, like the big
                        // list's (`set_query` clears it on a scope change).
                        self.nav_selected.clear();
                        self.nav_select_anchor = None;
                        // EXP-915: a new list starts at its top (the two
                        // issue lists share one virtual-list scroll).
                        self.nav_list_scroll.scroll_to_item(0, ScrollStrategy::Top);
                        // …and the outgoing list's memoized rows go with it:
                        // the slots are keyed, so keeping them only pinned an
                        // `Rc` of a query nothing will ask for again.
                        self.nav_data.clear();
                        self.nav_statuses.clear();
                        self.inbox_data.clear();
                        self.reviews_data.clear();
                        self.automation_facts.clear();
                        self.workflows_data.clear();
                    }
                }
                // EXP-863: the issue bodies refill these; any other list
                // leaves them empty, which hides the bulk bar.
                self.nav_issue_ids.clear();
                self.nav_visible_ids.clear();
                match origin.or_else(|| self.last_origin.clone()) {
                    Some(origin) => {
                        // EXP-863: the column's titlebar strip and header are
                        // the `Shell`'s, fixed above this pane — the ListNav
                        // starts at its back row.
                        let back = self.nav_back_row(&origin, cx);
                        let body = self.render_nav_body(&origin, cx);
                        // EXP-863: the bulk bar FLOATS over the bottom of the
                        // rows while a selection exists (EXP-289's no-jump
                        // rule — the rows never move for it).
                        let bulk_bar = self.nav_bulk_bar(cx);
                        v_flex()
                            .flex_1()
                            .min_h_0()
                            .min_w_0()
                            .child(back)
                            .child(crate::settings::nav_back_rule(cx))
                            .child(
                                v_flex()
                                    .flex_1()
                                    .min_h_0()
                                    .min_w_0()
                                    .relative()
                                    .child(body)
                                    .children(bulk_bar.map(|bar| {
                                        h_flex()
                                            .absolute()
                                            .bottom_2()
                                            .left_0()
                                            .right_0()
                                            .justify_center()
                                            .px_2()
                                            .child(bar)
                                    })),
                            )
                            .into_any_element()
                    }
                    None => div().into_any_element(),
                }
            }
        };
        v_flex()
            .size_full()
            .min_w_0()
            .overflow_hidden()
            .text_color(cx.theme().sidebar_foreground)
            .child(body)
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::{
        focused_list, row_origin_for, InboxTab, ListMode, ToolWindow,
    };
    use crate::navigation::{Screen, TabOrigin};

    /// EXP-851: every list in the origin vocabulary maps onto exactly the
    /// SCREEN it became — what a rail entry opens, what a ListNav back row
    /// hops out to, and what the legacy `activate_tool` callers mean.
    #[test]
    fn every_list_names_its_screen_and_its_label() {
        assert_eq!(
            ToolWindow::BoardIssues.origin_screen(Some("b1".into())),
            Screen::BoardIssues {
                board_id: "b1".into()
            }
        );
        // No board in hand: the sentinel the render resolves to the active one.
        assert_eq!(
            ToolWindow::BoardIssues.origin_screen(None),
            Screen::BoardIssues {
                board_id: String::new()
            }
        );
        assert_eq!(
            ToolWindow::Inbox.origin_screen(None),
            Screen::Inbox {
                tab: InboxTab::Inbox
            }
        );
        assert_eq!(ToolWindow::Support.origin_screen(None), Screen::Support);
        assert_eq!(ToolWindow::Files.origin_screen(None), Screen::Files);
        assert_eq!(
            ToolWindow::SourceControl.origin_screen(None),
            Screen::SourceControl
        );
        assert_eq!(ToolWindow::Reviews.origin_screen(None), Screen::Reviews);
        assert_eq!(
            ToolWindow::Automations.origin_screen(None),
            Screen::Automations
        );
        // The back row's words — a board overrides with its own name.
        assert_eq!(ToolWindow::Inbox.list_label(), "Inbox");
        assert_eq!(ToolWindow::Support.list_label(), "Support");
        assert_eq!(ToolWindow::Reviews.list_label(), "Reviews");
        assert_eq!(ToolWindow::Automations.list_label(), "Automations");
        // EXP-981.
        assert_eq!(ToolWindow::Workflows.origin_screen(None), Screen::Workflows);
        assert_eq!(ToolWindow::Workflows.list_label(), "Workflows");
    }

    /// EXP-862: which list a row click pins on the detail it opens. The bug
    /// this rule fixes is the `Nav` line: beside a RAIL-opened detail the
    /// breadcrumb rule derives nothing, so a click in the left column used to
    /// blank the column and throw the reader back to the rail.
    #[test]
    fn a_row_pins_its_own_list() {
        let board = TabOrigin {
            tool: ToolWindow::BoardIssues,
            board_id: Some("b1".into()),
            inbox_tab: None,
        };
        let issue = Screen::IssueDetail {
            issue_id: "i1".into(),
        };
        // The left column: whatever the column is rendering comes along, and
        // the screen that is up is irrelevant (here: a rail-opened detail).
        assert_eq!(
            row_origin_for(ListMode::Nav, Some(&board), Some(&issue)),
            Some(board.clone())
        );
        assert_eq!(row_origin_for(ListMode::Nav, None, Some(&issue)), None);
        // The full-width list screen IS the list.
        assert_eq!(
            row_origin_for(
                ListMode::Screen,
                None,
                Some(&Screen::BoardIssues {
                    board_id: "b1".into()
                })
            ),
            Some(board)
        );
        assert_eq!(
            row_origin_for(ListMode::Screen, None, Some(&Screen::Support))
                .map(|origin| origin.tool),
            Some(ToolWindow::Support)
        );
        // A screen that is no list pins nothing (the panel is only mounted
        // for the list screens, so this is the defensive arm).
        assert_eq!(row_origin_for(ListMode::Screen, None, Some(&issue)), None);
        assert_eq!(row_origin_for(ListMode::Screen, None, None), None);
    }

    /// EXP-851: what a window READS as, for the OS-notification redundancy
    /// check — the list screens are themselves, a ticket reads as Support,
    /// a session as the Agent page, and everything else falls back to the
    /// board list (which is "not the notification stream").
    #[test]
    fn the_focused_list_follows_the_screen() {
        assert_eq!(
            focused_list(Some(&Screen::Inbox {
                tab: InboxTab::MyIssues
            })),
            (ToolWindow::Inbox, InboxTab::MyIssues)
        );
        assert_eq!(
            focused_list(Some(&Screen::Support)).0,
            ToolWindow::Support
        );
        assert_eq!(
            focused_list(Some(&Screen::SupportThread {
                thread_id: "t1".into()
            }))
            .0,
            ToolWindow::Support
        );
        // EXP-923: a run is no longer "the Agent list" — nothing lists it,
        // so the redundancy check falls back to the board list.
        assert_eq!(
            focused_list(Some(&Screen::Session {
                session_id: "s1".into()
            }))
            .0,
            ToolWindow::BoardIssues
        );
        assert_eq!(focused_list(Some(&Screen::Files)).0, ToolWindow::Files);
        assert_eq!(
            focused_list(Some(&Screen::Reviews)).0,
            ToolWindow::Reviews
        );
        assert_eq!(
            focused_list(Some(&Screen::Automations)).0,
            ToolWindow::Automations
        );
        // EXP-981: the list and its detail share one focused list.
        assert_eq!(
            focused_list(Some(&Screen::Workflows)).0,
            ToolWindow::Workflows
        );
        assert_eq!(
            focused_list(Some(&Screen::Workflow { workflow_id: "wf-1".into() })).0,
            ToolWindow::Workflows
        );
        // An issue detail, Settings, nothing at all: never the inbox stream.
        for screen in [
            None,
            Some(Screen::Settings),
            Some(Screen::IssueDetail {
                issue_id: "i1".into(),
            }),
        ] {
            assert_eq!(
                focused_list(screen.as_ref()),
                (ToolWindow::BoardIssues, InboxTab::Inbox)
            );
        }
    }

}
