//! The team sidebar (masterplan-v3 §4.2, reworked as a JetBrains-style
//! tool-window rail).
//!
//! Two cooperating views share per-window state through [`RailShared`]:
//!
//! - [`RailView`] — the sidebar (EXP-1156: at the dragged `main` width,
//!   [`crate::resize_edge::panel_width`]) owned by the `Shell` and
//!   rendered OUTSIDE the `DockArea`, full window height. EXP-723 made it the
//!   web sidebar's twin and removed the collapse entirely (no icon strip, no
//!   toggle, no logo). Top: the team switcher + Search + New issue header
//!   ([`render_left_column_header`], rendered FIXED by the `Shell` since
//!   EXP-863). Middle (scrolling): the tool-window
//!   selectors — **Agent / Inbox / Devices / Reviews / Actions** (EXP-1192:
//!   Agent first, the landing page), the team's boards, the **Sessions** section (EXP-791:
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
//! - [`ListPanel`] — the team's LIST surfaces. EXP-851/EXP-1192: it renders
//!   EITHER as the full-width board list ([`ListMode::Screen`], what a board
//!   row navigates to) or as a SECOND SIDEBAR inside the content card
//!   ([`ListMode::Nav`]): the Inbox (always up on the Inbox screen and beside
//!   every detail opened from it, its Inbox | My Issues strip on top) or the
//!   Reviews queue beside a review it opened. The rail itself never folds or
//!   leaves.
//!
//! Every affordance dispatches a typed action (§3.6) or navigates directly;
//! menus render in the Root overlay, outside this element tree.

use std::collections::{BTreeMap, HashMap, HashSet};
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
    menu::{ContextMenuExt as _, DropdownMenu as _, PopupMenuItem},
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
    GettingStartedTab, Navigation, Screen, SecondSidebar, TabOrigin,
};
use crate::issue_header::parse_hex_color;
use crate::queries;

/// EXP-851: which LIST a detail was opened from — the kind half of
/// [`crate::navigation::TabOrigin`] (EXP-1192: only the Inbox and Reviews
/// still lend a second sidebar).
///
/// It used to name the rail's active TOOL WINDOW (a docked column beside the
/// centre). There is no tool column anymore: every one of these is a
/// full-width SCREEN ([`Screen::BoardIssues`], [`Screen::Inbox`],
/// [`Screen::Files`], [`Screen::SourceControl`],
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
    /// The trunk file tree at full panel height.
    Files,
    /// The trunk's local branches; activating also opens the changes screen.
    SourceControl,
    /// EXP-851: the Reviews page's rows — a PR diff opened from there keeps
    /// the queue beside it.
    Reviews,
}

impl ToolWindow {
    /// EXP-851: the SCREEN this list is — what its rail entry navigates to
    /// and what the legacy `activate_tool` callers mean. `board` supplies the window's active
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
            ToolWindow::Files => Screen::Files,
            ToolWindow::SourceControl => Screen::SourceControl,
            ToolWindow::Reviews => Screen::Reviews,
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
        "issues" => Some(S::Issues),
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
    let file_tree =
        cx.new(|cx| crate::file_tree::FileTreeView::new(git_bar.clone(), window, cx));
    let board_active = cx.new(|cx| BoardView::new(window, cx));
    // DEV-ONLY (§11.4 headless verification, same family as
    // EXP_DEV_SERVER/EXP_DEV_SCREEN): pre-select the settings section so a
    // capture run lands on one pane without synthetic input. EXP-851 moved
    // the screen/list seeds (`EXP_DEV_TOOL`, `EXP_DEV_INBOX_TAB`) onto the
    // navigation, where the screens they name live. Never document for users.
    let shared = cx.new(|_| RailShared {
        git_bar,
        file_tree,
        board_active,
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
/// reads as. A list screen is itself; everything else falls back to the
/// board list, which is the redundancy check's "shows no notification
/// stream" answer.
pub(crate) fn focused_list(screen: Option<&Screen>) -> (ToolWindow, InboxTab) {
    match screen {
        Some(Screen::Inbox { tab }) => (ToolWindow::Inbox, *tab),
        Some(Screen::Files) => (ToolWindow::Files, InboxTab::Inbox),
        Some(Screen::SourceControl) => (ToolWindow::SourceControl, InboxTab::Inbox),
        Some(Screen::Reviews) => (ToolWindow::Reviews, InboxTab::Inbox),
        _ => (ToolWindow::BoardIssues, InboxTab::Inbox),
    }
}

/// EXP-862 — the LIST a [`ListPanel`] row pins on the detail it opens.
///
/// The bug it fixes: every row used to navigate plainly and let the breadcrumb
/// rule ([`crate::navigation::derive_origin`]) work it out from the screen
/// that was up. Beside a detail opened from the RAIL that rule derives
/// NOTHING — a rail-opened detail carries no list — so clicking a row in the
/// second sidebar would have slid it away mid-click.
///
/// * [`ListMode::Screen`]: the full-width list screen's own origin — EXP-1192:
///   none for a board (its issues open with nothing beside them).
/// * [`ListMode::Nav`]: the second sidebar stays put — the detail that
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

/// EXP-1194: what the Reviews side list lights — the open issue's row, or
/// the open RUN's (an agent-run PR opens `Screen::Session`). `(issue, run)`.
fn open_review(screen: Option<&Screen>) -> (Option<&str>, Option<&str>) {
    match screen {
        Some(Screen::IssueDetail { issue_id }) => (Some(issue_id.as_str()), None),
        Some(Screen::Session { session_id }) => (None, Some(session_id.as_str())),
        _ => (None, None),
    }
}

/// EXP-1194: a Reviews side-list run row's `(lead, title)` — the Reviews
/// page's "Agent runs" row texts: `#N` and the run's name (a chat run reads
/// its auto-named title, else "Chat").
fn review_run_texts(run: &domain::rows::CodingSession) -> (String, String) {
    let number = run.pr_number.map(|number| format!("#{number}")).unwrap_or_default();
    let title = domain::batch_run::action_run_subject(run)
        .unwrap_or_else(|| domain::batch_run::CHAT_RUN_NAME.to_string());
    (number, title)
}

/// EXP-1192: what one [`ListPanel::set_side`] push changes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SidePush {
    /// The same kind and the same tab as the last push — nothing.
    Same,
    /// The same kind, a CHANGED pushed tab — the list takes it.
    NewTab,
    /// Another kind — the list resets.
    NewKind,
}

/// EXP-1192: [`ListPanel::set_side`]'s pure rule over the last push
/// (`side`, `last_tab`) and this one. A `None` tab never counts as a change
/// (only the Inbox kind carries one). `screen_list_tab` is the tab the LIST
/// shows while the Inbox SCREEN is up (`None` beside a detail): there the
/// tab is the screen's, so a push that disagrees with the list always wins —
/// a local flip made beside a detail must not outlive the walk back to the
/// Inbox screen, whose tab equals the last push.
fn side_push(
    side: Option<SecondSidebar>,
    last_tab: Option<InboxTab>,
    kind: SecondSidebar,
    tab: Option<InboxTab>,
    screen_list_tab: Option<InboxTab>,
) -> SidePush {
    if side != Some(kind) {
        SidePush::NewKind
    } else if tab.is_some()
        && (tab != last_tab || screen_list_tab.is_some_and(|shown| Some(shown) != tab))
    {
        SidePush::NewTab
    } else {
        SidePush::Same
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
/// Control's own buttons, an OS notification, the search palette) speak
/// this vocabulary. A board list takes the
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

/// EXP-1105: whether the ACTIVE team runs in yolo mode — Files and Source
/// Control leave the rail for every member (Reviews follows
/// `domain::reviews_queue::reviews_nav`). `None` = no active team;
/// pre-column rows hydrate `None` → off.
fn yolo_mode(nav: &Entity<Navigation>, cx: &App) -> Option<bool> {
    active_team_id(nav, cx).map(|id| {
        Store::global(cx)
            .collections()
            .teams
            .read(cx)
            .get(&id)
            .is_some_and(|team| team.yolo_mode())
    })
}

/// EXP-1105: which of the three yolo-hideable rail entries render. Off =
/// all three (today's rail). On = Files never; Source Control only while
/// the trunk needs attention or its sync failed. Reviews = EXP-1244's
/// `reviews_nav(..).shows` (yolo mode keeps it only while the queue holds a
/// PR — an open one there means its auto-merge FAILED), identical ×4.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct YoloRail {
    reviews: bool,
    files: bool,
    source_control: bool,
}

fn yolo_rail(yolo: bool, reviews_shows: bool, sc_failing: bool) -> YoloRail {
    YoloRail {
        reviews: reviews_shows,
        files: !yolo,
        source_control: !yolo || sc_failing,
    }
}

/// FEED-3: hover-reveal group name for the rail's board rows — the
/// gear that jumps to the board's settings page shows only under the cursor.
const BOARD_ROW_GROUP: &str = "rail-board-row";
/// EXP-863: the Pinned rows' hover group — reveals the trailing Unpin button
/// (the board rows' gear recipe).
const PIN_ROW_GROUP: &str = "rail-pin-row";
/// EXP-863: the side list's issue rows' hover group — reveals the leading
/// bulk-select checkbox (the big list's `issue-row` group).
const NAV_ROW_GROUP: &str = "list-nav-issue-row";

/// EXP-915: the side list's My Issues rows' heights — the group band and the
/// `rail_row_lead` row — plus the 2px the old `gap_0p5` column put between
/// them, folded into each virtual-list item so the fixed sizes stay exact.
const NAV_HEADER_HEIGHT: f32 = 24.;
const NAV_ISSUE_ROW_HEIGHT: f32 = 28.;
const NAV_ROW_GAP: f32 = 2.;
/// EXP-980: the side list row's own left padding (`flat_row_compact`'s
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

/// EXP-1022: the breathing room between two of the rail's sections
/// (nav entries / Pinned / Boards / Running / This device) — plain space, like
/// the web sidebar's group padding; the rules are gone.
const SECTION_GAP: f32 = 8.;

/// EXP-1022: the room the rail's scroll content keeps under its last row
/// while the What's-new card floats over the scroll area's bottom edge — the
/// card's height plus a gap, so the last row can still scroll clear of it.
/// The web sidebar reserves the same (`whats-new.tsx`
/// `WHATS_NEW_CLEARANCE_CLASS`).
const WHATS_NEW_CLEARANCE: f32 = 80.;

/// One flattened side-list virtual-list row (EXP-915) — the big list's
/// `ListRow` at the column's density: the issue rides behind the memoized
/// query's `Rc`, so rebuilding the vector per frame clones handles, never
/// payloads.
enum NavRow {
    Header {
        status: Box<ResolvedStatus>,
        count: usize,
        collapsed: bool,
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
        /// EXP-998: the row's slice of the blocks rail — its node, labelled
        /// by the query's `blocks` numbers.
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

/// The side list's issue query memo key (EXP-915): the scope plus every
/// collection input ([`queries::BoardDataKey`]).
#[derive(PartialEq, Eq)]
struct NavBoardKey {
    query: IssueQuery,
    base: queries::BoardDataKey,
}

/// EXP-282: one row of the rail — icon + label, left-aligned, glass
/// row fills. Hand-rolled on purpose: gpui-component's `Button` centers its
/// inner layout with no `Styled` reach into it (the settings-nav rows set the
/// same precedent), and a centered label would defeat the whole point of the
/// labelled rail. Handlers are the caller's job.
/// EXP-533: the rail spinner's label and tooltip.
const SYNCING_LABEL: &str = "Syncing\u{2026}";

/// A rail entry's status badge (EXP-509 — the Source Control entry outgrew
/// the plain dot): the classic colored dot (EXP-699: Inbox primary,
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
    /// EXP-923/EXP-996: the folded rows of the Running section, by
    /// [`domain::session_tree::session_tree_node_key`] — a parent run's id.
    /// Per window, never persisted.
    collapsed_runs: HashSet<String>,
    /// EXP-923: the Running rows carry a liveness that expires with
    /// `last_seen_at` and produces no collection delta — the session lists'
    /// 5s re-derive, here.
    _tick: gpui::Task<()>,
    /// EXP-1244: the Reviews dot's queue count, memoized on its inputs (the
    /// rail renders every frame, EXP-915).
    reviews_count: queries::Memo<queries::ReviewsCountKey, usize>,
    /// The team the rail last asked the openPulls store about — a team
    /// change refreshes a stale entry (the web nav's mount).
    reviews_team: Option<String>,
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
        let open_pulls = crate::open_pulls::OpenPulls::global(cx);
        let subscriptions = vec![
            cx.observe(&shared, |_, _, cx| cx.notify()),
            cx.observe(&nav, |_, _, cx| cx.notify()),
            // EXP-1244: the Reviews dot reads the app-wide openPulls store;
            // bringing the window forward refreshes a stale active team (the
            // web nav's focus refetch).
            cx.observe(&open_pulls, |_, _, cx| cx.notify()),
            cx.observe_window_activation(window, |this: &mut Self, window, cx| {
                if window.is_window_active() {
                    if let Some(id) = active_team_id(&this.nav, cx) {
                        crate::open_pulls::OpenPulls::refresh(&id, false, cx);
                    }
                }
            }),
            // Sync/conflict badge follows the trunk engine's state.
            cx.observe(&git_bar, |_, _, cx| cx.notify()),
            // The Reviews dot is a live read over issues ⨝ boards (+ runs).
            cx.observe(&collections.issues, |_, _, cx| cx.notify()),
            // Board icons + the Reviews dot follow the boards collection.
            cx.observe(&collections.boards, |_, _, cx| cx.notify()),
            // The team picker's rows follow the teams collection.
            cx.observe(&collections.teams, |_, _, cx| cx.notify()),
            // The Inbox dot is a live read over unread rows.
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
            collapsed_runs: HashSet::new(),
            _tick: crate::sessions_section::tick(cx, |_: &mut Self, cx| cx.notify()),
            reviews_count: queries::Memo::default(),
            reviews_team: None,
            _subscriptions: subscriptions,
        }
    }

    /// EXP-791: a muted section label ("Boards", "Pinned", "Running").
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
                    let lead = div()
                        .flex_shrink_0()
                        .text_xs()
                        .text_color(muted)
                        .font_family(theme::terminal::FONT_FAMILY)
                        .child(SharedString::from(issue.identifier.clone()))
                        .into_any_element();
                    let title = issue.title.clone();
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
            out.push(row_el.group(PIN_ROW_GROUP).child(unpin).into_any_element());
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
        // EXP-1075: the ACTIVE team's own live runs only — the other teams
        // are the switcher's dot now.
        let nav = self.nav.clone();
        let rows = crate::sessions_section::rail_running_rows(&nav, cx);
        let rows = crate::sessions_section::drop_collapsed(
            rows,
            &self.collapsed_runs,
            |row| row.key.as_str(),
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
                let guides = guides.get(index).cloned().unwrap_or_default();
                let run = &row.run;
                // EXP-870: the row stays current across the Issue | Run
                // toggle — both faces are the same piece of work.
                let active = match &screen {
                    Some(Screen::Session { session_id }) => session_id == &run.session_id,
                    Some(Screen::IssueDetail { issue_id }) => {
                        run.issue_id.as_deref() == Some(issue_id.as_str())
                    }
                    _ => false,
                };
                self.rail_running_row(index, row, run, guides, active, cx)
            })
            .collect();
        let rule = self.section_rule(cx);
        Some(
            v_flex()
                .w_full()
                .gap(px(RUNNING_ROW_GAP))
                .child(rule)
                .child(self.section_label("Running", cx))
                .children(elements)
                .into_any_element(),
        )
    }

    /// EXP-923 — the fold chevron a rail row with nested rows wears, keyed by
    /// the tree's node key.
    fn rail_run_fold(
        &self,
        index: usize,
        key: &str,
        has_children: bool,
        cx: &mut gpui::Context<Self>,
    ) -> Option<gpui::AnyElement> {
        if !has_children {
            return None;
        }
        let muted = cx.theme().muted_foreground;
        let collapsed = self.collapsed_runs.contains(key);
        let fold_id = key.to_string();
        Some(
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
                    // The row itself opens the subject — folding must not.
                    cx.stop_propagation();
                    if !this.collapsed_runs.insert(fold_id.clone()) {
                        this.collapsed_runs.remove(&fold_id);
                    }
                    cx.notify();
                }))
                .into_any_element(),
        )
    }

    /// ONE Running row ([`Self::render_running_section`]) — the labelled row.
    fn rail_running_row(
        &self,
        index: usize,
        row: &crate::sessions_section::SessionTreeRow<crate::sessions_section::RailRunRow>,
        run: &crate::sessions_section::RailRunRow,
        guides: domain::tree_guides::Guides,
        active: bool,
        cx: &mut gpui::Context<Self>,
    ) -> gpui::AnyElement {
        let muted = cx.theme().muted_foreground;
        let session_id = run.session_id.clone();
        let issue_id = run.issue_id.clone();
        // EXP-923/EXP-1184: the lead is the agent's mark — its working
        // spark mid-turn, else the brand mark with the run state's badge on
        // its corner (the rail's badge rule, on a glyph).
        let lead = crate::coding_selects::run_lead(Some(run.agent), 16., 6., run.state)
            .into_any_element();
        let title = run.title.clone();
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
        let fold = self.rail_run_fold(index, &row.key, row.has_children, cx);
        let device_label: Option<SharedString> = run.device_label.clone();
        let device = div()
            .id(("rail-running-device", index))
            .flex_shrink_0()
            .child(Icon::from(run.device_icon.clone()).xsmall().text_color(muted))
            .when_some(device_label, |this, label| {
                this.tooltip(move |window, cx| {
                    gpui_component::tooltip::Tooltip::new(label.clone()).build(window, cx)
                })
            });
        let kill = (!run.paused).then(|| {
            (
                run.local.clone(),
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
            .children(run.identifier.clone().map(|identifier| {
                div()
                    .flex_shrink_0()
                    .text_xs()
                    .text_color(muted)
                    .font_family(theme::terminal::FONT_FAMILY)
                    .child(identifier)
            }))
            .child(div().flex_1().min_w_0().truncate().child(title))
            .child(device)
            .on_click(cx.listener(move |_, _: &ClickEvent, window, cx| open(window, cx)));
        // EXP-874: the kill rides the row's right-click menu, not a trailing
        // button — the session lists' own grammar.
        match kill {
            None => row_el.into_any_element(),
            Some((local, session_id)) => row_el
                .context_menu(move |menu, _window, cx| {
                    let local = local.clone();
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
    /// EXP-870: one rail entry — the labelled 28px row (EXP-1192: the rail
    /// never folds to an icon column any more). Every nav/board/pinned entry
    /// goes through here.
    fn entry(
        &self,
        id: impl Into<gpui::ElementId>,
        lead: gpui::AnyElement,
        label: impl Into<SharedString>,
        active: bool,
        badge: Option<RailBadge>,
        cx: &App,
    ) -> gpui::Stateful<gpui::Div> {
        rail_row_lead(id, lead, label, active, None, badge, cx)
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
        // screen is up, exactly like every other rail row. EXP-1192: the
        // Inbox on EITHER tab (My Issues lives in its sidebar); a detail
        // opened beside it is not the Inbox screen and lights nothing.
        let screen = tool.origin_screen(None);
        let active = match (&screen, resolved_screen(&self.nav, cx)) {
            // A board list highlights through its BOARD row, never here.
            (Screen::BoardIssues { .. }, _) => false,
            (Screen::Inbox { .. }, Some(Screen::Inbox { .. })) => true,
            (screen, Some(current)) => *screen == current,
            _ => false,
        };
        let lead = icon.with_size(gpui_component::Size::Medium).flex_shrink_0().into_any_element();
        self.entry(id, lead, label, active, badge, cx)
            .when_some(tooltip, |row, text| {
                row.tooltip(move |window, cx| {
                    gpui_component::tooltip::Tooltip::new(text.clone()).build(window, cx)
                })
            })
            .on_click(cx.listener(move |_, _: &ClickEvent, window, cx| {
                activate_tool(window, cx, tool);
            }))
            .into_any_element()
    }

    /// A rail entry that navigates STRAIGHT to a tab-less full-page screen
    /// instead of activating a tool window (EXP-467's Actions entry,
    /// generalized by EXP-686 for Devices / Actions, and by
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

    /// The footer's Computer button — THIS machine's tools behind one menu
    /// on the menu surface: a new Terminal (the shell tab cmd-t opens,
    /// EXP-791), Files and Source Control (EXP-1105: yolo mode hides those
    /// two until a git failure needs a person). Source Control's attention /
    /// error badge and its reason ride the button, so a trunk failure stays
    /// visible with the menu closed. Lit while Files or Source Control is up.
    fn rail_computer_entry(
        &self,
        files: bool,
        source_control: bool,
        sc_tooltip: SharedString,
        sc_badge: Option<RailBadge>,
        cx: &mut gpui::Context<Self>,
    ) -> gpui::AnyElement {
        let active = matches!(
            resolved_screen(&self.nav, cx),
            Some(Screen::Files) | Some(Screen::SourceControl)
        );
        // Only a FAILURE rides the button (the attention triangle / error
        // cross); the syncing spinner stays on the menu row.
        let entry_badge = sc_badge
            .clone()
            .filter(|badge| source_control && matches!(badge, RailBadge::Icon(..)));
        let tooltip: SharedString = if entry_badge.is_some() {
            sc_tooltip
        } else {
            "Computer".into()
        };
        let menu_sc_badge = sc_badge.filter(|_| source_control);
        let menu = move |mut menu: gpui_component::menu::PopupMenu,
                         _window: &mut Window,
                         _cx: &mut gpui::Context<gpui_component::menu::PopupMenu>| {
            menu = menu.item(
                PopupMenuItem::new("Terminal")
                    .icon(Icon::from(registry::NAV_TERMINAL))
                    .on_click(|_, window, cx| crate::session_bar::open_new_shell(window, cx)),
            );
            if files {
                menu = menu.item(
                    PopupMenuItem::new("Files")
                        .icon(Icon::new(registry::NAV_FILES))
                        .on_click(|_, window, cx| activate_tool(window, cx, ToolWindow::Files)),
                );
            }
            if source_control {
                let badge = menu_sc_badge.clone();
                menu = menu.item(
                    PopupMenuItem::element(move |_, cx| {
                        h_flex()
                            .flex_1()
                            .gap_2()
                            .items_center()
                            .child(div().flex_1().min_w_0().truncate().child("Source Control"))
                            .children(badge.clone().map(|badge| rail_badge_element(badge, 12., cx)))
                    })
                    .icon(Icon::from(registry::NAV_SOURCE_CONTROL))
                    .on_click(|_, window, cx| {
                        activate_tool(window, cx, ToolWindow::SourceControl)
                    }),
                );
            }
            menu
        };

        // The footer's small ghost square (the settings gear's shape), the
        // failure badge in its top-right corner. It opens upward: the
        // button sits at the window's bottom edge.
        let button = Button::new("rail-computer")
            .ghost()
            .cursor_pointer()
            .small()
            .icon(registry::NAV_COMPUTER)
            .selected(active)
            .tooltip(tooltip)
            .dropdown_menu_with_anchor(gpui::Anchor::BottomLeft, menu);
        div()
            .relative()
            .flex_shrink_0()
            .child(button)
            .when_some(entry_badge, |this, badge| {
                this.child(
                    div()
                        .absolute()
                        .top(px(3.))
                        .right(px(3.))
                        .child(rail_badge_element(badge, 10., cx)),
                )
            })
            .into_any_element()
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

        Button::new("rail-account")
            .ghost().cursor_pointer()
            .small()
            // The trigger is a full-width row — the Button's own inner layout
            // is centered and unreachable, so the row is a `w_full` child that
            // left-aligns inside it.
            .map(|button| {
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
            .when(owner, |row| {
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

    /// EXP-1022: what separates two sections in the rail — plain
    /// [`SECTION_GAP`] space (the web sidebar draws no rules between its
    /// groups either).
    fn section_rule(&self, _cx: &mut gpui::Context<Self>) -> gpui::AnyElement {
        div()
            .w_full()
            .h(px(SECTION_GAP))
            .flex_shrink_0()
            .into_any_element()
    }
}

/// The left column's section rule — full width, like the web sidebar's. The
/// one under the FIXED header (`Shell`) and the back rows' rules are the same
/// line.
pub(crate) fn left_column_divider(cx: &App) -> gpui::AnyElement {
    div()
        .w_full()
        .h(px(1.))
        .my_1()
        .flex_shrink_0()
        .bg(cx.theme().sidebar_border)
        .into_any_element()
}

/// EXP-1075 — one team's live-run dot, or `None` when that team has none of
/// my runs going. Amber while any of them is parked on a question, else the
/// plain live green ([`session_dot_tone`]'s tokens — one palette, one
/// meaning). A DOT, never a count: the switcher answers "is something waiting
/// for me elsewhere", and the number is the rail's job once you are there.
fn team_run_dot(live: Option<&queries::TeamLiveRuns>) -> Option<Hsla> {
    let runs = live?;
    if runs.count == 0 {
        return None;
    }
    Some(if runs.needs_input {
        theme::tokens::YELLOW.to_hsla()
    } else {
        theme::tokens::GREEN.to_hsla()
    })
}

/// EXP-723: the sidebar's header row, the desktop mirror of the web
/// `SidebarHeader` (`apps/web/src/components/team/sidebar.tsx`): the team
/// switcher taking the width, then icon-only Search and New issue.
///
/// EXP-863: it is the LEFT COLUMN's header, not the rail's — the `Shell`
/// renders it ONCE, fixed, above whichever occupant (rail / settings nav) is
/// sliding underneath, so the switcher, Search and New issue
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
    live: &BTreeMap<String, queries::TeamLiveRuns>,
    cx: &mut App,
) -> impl IntoElement {
    let active_team = active_team_id(nav, cx);
    // EXP-1075: the rail's Running section is the ACTIVE team's runs now, so
    // the switcher carries the others — a dot per team, never a count.
    let teams: Vec<(String, String, bool, Option<Hsla>)> = Store::global(cx)
        .collections()
        .teams_sorted(cx)
        .into_iter()
        .map(|team| {
            let active = Some(team.id.as_str()) == active_team.as_deref();
            let dot = team_run_dot(live.get(&team.id));
            (team.id, team.name, active, dot)
        })
        .collect();
    // The trigger's own dot folds every OTHER team into one: amber wins over
    // green, because a run asking a question is the thing you must be told.
    let elsewhere = teams
        .iter()
        .filter(|(_, _, active, _)| !*active)
        .filter_map(|(id, _, _, _)| live.get(id))
        .fold(queries::TeamLiveRuns::default(), |mut acc, runs| {
            acc.count += runs.count;
            acc.needs_input |= runs.needs_input && runs.count > 0;
            acc
        });
    let elsewhere_tone = team_run_dot(Some(&elsewhere));
    // The trigger names the ACTIVE team; nothing synced yet degrades to
    // the app letter (`team_avatar`'s own fallback) and an empty label.
    let team_name: SharedString = teams
        .iter()
        .find(|(_, _, active, _)| *active)
        .map(|(_, name, _, _)| SharedString::from(name.clone()))
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
                    // EXP-1075: the chevron wears the other teams' live-run
                    // dot on its corner — the rail running row's badge, on
                    // the one glyph that means "somewhere else".
                    div()
                        .relative()
                        .flex_shrink_0()
                        .child(
                            Icon::new(registry::NAV_TEAM_SWITCHER)
                                .xsmall()
                                .flex_shrink_0()
                                .text_color(cx.theme().muted_foreground),
                        )
                        .when_some(elsewhere_tone, |this, tone| {
                            this.child(
                                div()
                                    .absolute()
                                    .top(px(-2.))
                                    .right(px(-2.))
                                    .size(px(6.))
                                    .rounded_full()
                                    .bg(tone)
                                    // The hairline keeps the dot readable
                                    // over the glyph it overlaps.
                                    .border_1()
                                    .border_color(theme::tokens::BACKGROUND.to_hsla()),
                            )
                        }),
                ),
        )
        .dropdown_menu_with_anchor(gpui::Anchor::TopLeft, move |menu, _window, _cx| {
            // Flat checked rows (the menu builder has no submenus); always
            // shown, even with a single team (EXP-434: no teams=1 special
            // case anywhere).
            let mut menu = menu;
            for (id, name, active, dot) in &teams {
                let label = SharedString::from(name.clone());
                let switch = Box::new(SwitchTeam {
                    team_id: id.clone(),
                });
                // EXP-1075: the ACTIVE team never wears a dot — its runs are
                // the rail's Running section, right there. Every other team
                // with live runs of mine gets the trailing 6px disc.
                match (*active, *dot) {
                    (false, Some(tone)) => {
                        menu = menu.menu_element_with_check(false, switch, move |_window, _cx| {
                            h_flex()
                                .flex_1()
                                .min_w_0()
                                .items_center()
                                .justify_between()
                                .gap_2()
                                .child(div().truncate().child(label.clone()))
                                .child(
                                    div()
                                        .size(px(6.))
                                        .rounded_full()
                                        .flex_shrink_0()
                                        .bg(tone),
                                )
                        });
                    }
                    _ => menu = menu.menu_with_check(label, *active, switch),
                }
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
                        // EXP-1170: the New issue PAGE, no list beside it.
                        crate::issue_draft_screen::open_new_from_rail(
                            window,
                            cx,
                            board_id.clone(),
                        );
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
        // EXP-1022: the card FLOATS over the rail's scroll area, so the
        // glass fill sits on an OPAQUE ground of its own (the window
        // gradient's bottom stop, where the card lives) — rows sliding
        // behind it must not show through — under a shadow that lifts it.
        Some(
            div()
                .w_full()
                .flex_shrink_0()
                .rounded(px(theme::tokens::radius::LG))
                .bg(theme::background_gradient_color_at(1.))
                .shadow_md()
                .child(
                    div()
                        .id("rail-whats-new")
                        .w_full()
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
                        ),
                )
                .into_any_element(),
        )
    }
}

impl Render for RailView {
    fn render(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
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

        // EXP-1244: the Reviews entry reads the page's OWN queue
        // (`domain::reviews_queue`) over the active team — open issue PRs,
        // runs' own PRs (EXP-734) and the unlinked pulls of the app-wide
        // openPulls store — then `reviews_nav` decides dot + visibility ×4.
        let reviews_team = active_team_id(&self.nav, cx);
        if reviews_team != self.reviews_team {
            if let Some(id) = reviews_team.as_deref() {
                crate::open_pulls::OpenPulls::refresh(id, false, cx);
            }
            self.reviews_team = reviews_team.clone();
        }
        let reviews_count = reviews_team.as_deref().map_or(0, |id| {
            let open_pulls = crate::open_pulls::OpenPulls::global(cx);
            let app: &App = cx;
            let store = open_pulls.read(app);
            *self.reviews_count.get_or_insert_with(
                queries::reviews_count_key(app, id, store.revision()),
                || queries::reviews_count(app, id, store.repos(id)),
            )
        });
        let team_yolo = yolo_mode(&self.nav, cx);
        let reviews_nav = domain::reviews_queue::reviews_nav(
            team_yolo.as_slice(),
            reviews_count,
        );
        let has_reviews = reviews_nav.dot;
        // Inbox badge (EXP-699): any unread renderable notification — the
        // primary-tinted dot the mobile tab bars show.
        let inbox_badge = queries::inbox_unread(cx)
            .then(|| RailBadge::Dot(theme::tokens::PRIMARY.to_hsla()));
        // EXP-1001: the Agent entry carries NO live dot. My live runs are
        // rows in the rail's Running section (EXP-923) right below, each
        // with its own state dot — a second signal on the entry above them
        // was noise. (It was the EXP-699 Devices dot, moved by EXP-818.)
        // EXP-878: the Drafts pile — a root entry with its muted count
        // (EXP-963) while any draft is parked (or the pile is on screen).
        // EXP-1170: the draft open on the New issue page is not "parked" —
        // it does not count.
        let open_draft = match resolved_screen(&self.nav, cx) {
            Some(Screen::IssueDraft { draft_id, .. }) => Some(draft_id),
            _ => None,
        };
        let draft_count = active_team_id(&self.nav, cx)
            .map(|id| {
                crate::drafts::drafts_in_team(&id, cx)
                    .iter()
                    .filter(|draft| Some(&draft.id) != open_draft.as_ref())
                    .count()
            })
            .unwrap_or(0);
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
            active_team.clone().map(|team_id| {
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
            let rule = self.section_rule(cx);
            v_flex()
                .w_full()
                .gap_1()
                .child(rule)
                .child(self.section_label("Pinned", cx))
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
        // EXP-1105: the same two failure inputs the badge reads (attention
        // or a sticky sync error) — a pull merely in flight is not one.
        let sc_failing = sc_attention.is_some() || git_bar.read(cx).sync_error().is_some();
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
        // dispatches `OpenSettings`. EXP-340: icon-only — it sits in the
        // account row, hugging the rail's right edge, instead of taking a
        // full-width row of its own.
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
        // EXP-1105: yolo mode hides Reviews / Files / Source Control unless
        // a git failure needs a person (see `yolo_rail`).
        let rail_gate = yolo_rail(team_yolo.unwrap_or(false), reviews_nav.shows, sc_failing);
        let reviews_entry = rail_gate.reviews.then(|| {
            self.rail_screen_entry(
                "rail-reviews",
                Icon::from(ExpIcon::GitPullRequest),
                "Reviews",
                Screen::Reviews,
                // Review green (EXP-214): open PRs are "stuff to do",
                // colored like the in_review issue status.
                has_reviews.then(|| RailBadge::Dot(theme::tokens::GREEN.to_hsla())),
                cx,
            )
        });
        // Actions is a root entry (lit on one action's page too, SLOP-2);
        // Drafts follows while any draft is parked.
        let actions_entry = self.rail_screen_entry_active(
            "rail-actions",
            Icon::from(registry::NAV_ACTIONS),
            "Actions",
            Screen::Actions,
            None,
            matches!(
                resolved_screen(&self.nav, cx),
                Some(Screen::Actions) | Some(Screen::Action { .. })
            ),
            cx,
        );
        let on_drafts = matches!(resolved_screen(&self.nav, cx), Some(Screen::Drafts));
        let drafts_entry = (draft_count > 0 || on_drafts).then(|| {
            self.rail_screen_entry(
                "rail-drafts",
                Icon::from(registry::NAV_DRAFTS),
                "Drafts",
                Screen::Drafts,
                Some(RailBadge::Count(
                    draft_count,
                    crate::surface::BadgeTone::Muted,
                )),
                cx,
            )
        });
        // The footer's Computer menu: Terminal (EXP-791's footer button),
        // Files and Source Control.
        let computer_entry = self.rail_computer_entry(
            rail_gate.files,
            rail_gate.source_control,
            sc_tooltip,
            sc_badge,
            cx,
        );

        // EXP-863: the rail starts UNDER the left column's fixed header —
        // the titlebar strip, the team switcher row and the rule beneath
        // them are the `Shell`'s (`render_left_column`), shared with the
        // settings nav, so the rail neither pads its top
        // nor renders a header of its own.
        // EXP-1022: the What's-new card FLOATS over the scroll area's bottom
        // edge (rows slide behind it) — rendered up front so the scroll
        // content knows whether to keep clearance under its last row.
        let whats_new_card = self.render_whats_new_card(cx);
        let whats_new_clearance = whats_new_card.is_some();
        // EXP-997: the column's 8px gutter belongs to the SCROLL CONTENT and
        // the footer rows, not the column — the scroll pane spans the full
        // column width. EXP-1095: the vendored thumb (6-8px, inset 4 in a
        // 16px track) still reached 2-4px into the rows, so the sidebar's
        // scrollers take `sidebar_scroll_pane`'s slim variant: a 3-4px thumb
        // inset 1px from the column's right edge in an 8px hit strip, i.e.
        // wholly inside that gutter, beside the rows instead of over them.
        // EXP-1156: the `main` width, read exactly as the Shell's column
        // reads it — a FIXED width rather than `w_full`, so while the column
        // animates into Settings the rows are clipped instead of reflowing
        // frame by frame.
        v_flex()
            .w(px(crate::resize_edge::panel_width(
                crate::resize_edge::SidebarPanel::Main,
                crate::shell::window_extent(window),
            )))
            .flex_shrink_0()
            .h_full()
            .pb_2()
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
            // Settings/Account off small windows. Rail order (SLOP-5, the
            // four nouns plus Reviews and Inbox; EXP-1192: Agent first, the
            // landing page): [Agent, Inbox, Devices, Reviews?, Actions,
            // Drafts?] / Pinned (EXP-778) / boards + "+" / Running (EXP-923).
            // Terminal, Files and Source Control are the footer's Computer
            // menu.
            .child(crate::scroll_pane::sidebar_scroll_pane(
                "rail-scroll",
                &self.rail_scroll,
                v_flex()
                    .w_full()
                    .px_2()
                    .gap_1()
                    .when(whats_new_clearance, |content| {
                        content.pb(px(WHATS_NEW_CLEARANCE))
                    })
                    // EXP-791: "Agent", the web sidebar's word for the Chat
                    // page (EXP-772). EXP-1192: the first entry — the page
                    // the window lands on.
                    .child(self.rail_agent_entry(active_chat_action.as_deref(), cx))
                    .child(self.rail_tool_icon(
                        "rail-inbox",
                        Icon::new(registry::NAV_INBOX),
                        ToolWindow::Inbox,
                        "Inbox",
                        None,
                        inbox_badge,
                        cx,
                    ))
                    // EXP-686: Devices, the machine list.
                    .child(self.rail_screen_entry(
                        "rail-devices",
                        Icon::from(icons::registry::NAV_DEVICES),
                        "Devices",
                        Screen::Devices,
                        None,
                        cx,
                    ))
                    // EXP-706: Reviews is a full-page screen like the three
                    // above it, not a tool window with a docked list.
                    // EXP-1105/EXP-1244: `reviews_nav(..).shows` — absent in
                    // yolo mode unless the queue holds a (failed-merge) PR.
                    .children(reviews_entry)
                    // Actions and Drafts (while any) close the nav entries.
                    .child(actions_entry)
                    .children(drafts_entry)
                    // EXP-778: Pinned sits between the nav entries and the
                    // boards (rail order: entries / Pinned / boards / Running).
                    .children(pinned_section)
                    .child(self.section_rule(cx))
                    .children(boards_header)
                    .children(board_icons)
                    // EXP-923: Running — MY live runs, the live half of the
                    // retired top-tab group.
                    .children(running_section),
            )
            // EXP-1022: the What's-new card, floating over the scroll area's
            // bottom edge in the column's gutter; the `sidebar_scroll_pane` shell
            // is `relative`, so the card anchors to the pane's bounds.
            .children(whats_new_card.map(|card| {
                div()
                    .absolute()
                    .bottom_0()
                    .left_0()
                    .right_0()
                    .px_2()
                    .child(card)
            })))
            // EXP-723: the web sidebar footer's order, top to bottom — the
            // muted Getting-started re-entry point, the sync spinner, then
            // the account row with the settings gear (the What's-new card
            // floats above, over the scroll area).
            .child(
                v_flex()
                    .w_full()
                    .px_2()
                    .gap_1()
                    .children(getting_started_icon)
                    .children(self.render_sync_indicator(cx))
                    // EXP-340: one bottom row — the account button fills the
                    // width, the Computer menu and the gear ride its right
                    // edge.
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
                            .child(computer_entry)
                            .child(settings_entry),
                    ),
            )
            .into_any_element()
    }
}

// ---------------------------------------------------------------------------
// ListPanel — the board list, or a second sidebar inside the content card
// ---------------------------------------------------------------------------

/// EXP-851: WHERE a [`ListPanel`] renders, which is the only thing that
/// differs between its two instances per window.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ListMode {
    /// The main view — the full board list (`Screen::BoardIssues`) with its
    /// filter bar. EXP-1192: the Inbox has no full-width body any more; its
    /// list IS the second sidebar.
    Screen,
    /// EXP-1192: the SECOND SIDEBAR inside the content card, pushed its kind
    /// by the host ([`ListPanel::set_side`]) — the Inbox (strip + rows) or
    /// the Reviews queue (a back row over its rows).
    Nav,
}

/// EXP-851: the team's list surfaces in one view — the full-width board list
/// ([`ListMode::Screen`]) or a second sidebar ([`ListMode::Nav`]). One type,
/// so the issue queries and the inbox grouping exist once.
pub struct ListPanel {
    mode: ListMode,
    nav: Entity<Navigation>,
    /// [`ListMode::Nav`] only: the Inbox tab the LIST shows. On the Inbox
    /// screen it follows the screen's tab (pushed through [`Self::set_side`]);
    /// beside a detail there is no screen to carry it, and flipping it must
    /// not navigate.
    nav_inbox_tab: InboxTab,
    /// [`ListMode::Nav`] only (EXP-1192): the second sidebar this panel
    /// shows, PUSHED by its host ([`Self::set_side`]) rather than read off
    /// the window — so an outgoing sidebar keeps its rows through its slide.
    side: Option<SecondSidebar>,
    /// The Inbox tab the host pushed LAST ([`Self::set_side`]) — only a
    /// CHANGE of it moves [`Self::nav_inbox_tab`], so a local flip beside a
    /// detail survives the host's per-render push.
    side_tab: Option<InboxTab>,
    /// The Board Issues tool window — the full board (filter bar with
    /// All/Active/Backlog tabs + New Issue + the grouped virtualized list
    /// with inline status/priority menus), scoped to the active board.
    /// Lives in [`RailShared`] (EXP-48 — the detail switcher reads it too).
    board_active: Entity<BoardView>,
    /// [`ListMode::Nav`] only (EXP-862): the status groups folded away in the
    /// issue lists, by `group_key` — the big list's own `collapsed` set. Per
    /// panel, never persisted.
    nav_collapsed: HashSet<String>,
    /// [`ListMode::Nav`] only (EXP-863): the issue rows' bulk selection — the
    /// big list's `selected` + `select_anchor`, with the same click grammar
    /// (Cmd/Ctrl toggles, Shift extends, the hover checkbox toggles). Cleared
    /// when the side list's kind changes; pruned each render to rows that
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
    /// EXP-915: the side list's memoized My Issues query — the big
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
    /// EXP-998: the side list's My Issues blocks rail column width (0 =
    /// none). EXP-1057: dots only; each dot owns its hover card.
    nav_rail_width: f32,
    /// Per-render snapshots the row builders read instead of re-resolving
    /// navigation once per row: the vocabulary, the open detail's issue and
    /// the origin a row pins.
    nav_row_statuses: Rc<Vec<ResolvedStatus>>,
    nav_active_issue_id: Option<String>,
    nav_row_origin: Option<TabOrigin>,
    /// The My Issues virtual-list scroll — reset to the top when the side
    /// list's kind (or the Inbox tab) changes.
    nav_list_scroll: VirtualListScrollHandle,
    /// EXP-915: the other sidebar lists' memoized queries — the inbox
    /// grouping and the Reviews queue.
    inbox_data: queries::Memo<queries::InboxDataKey, queries::InboxData>,
    reviews_data: queries::Memo<queries::ReviewGroupsKey, Vec<queries::ReviewGroup>>,
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
        // SLOP-4: a widget reporter answered on an issue — the registry's
        // reply glyph (`notification-reporter-reply` ×4).
        Some(domain::contract::NOTIFICATION_TYPE_REPORTER_REPLY) => {
            Icon::new(registry::NOTIFICATION_REPORTER_REPLY)
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
    /// EXP-1192: point the [`ListMode::Nav`] panel at a second sidebar —
    /// called by its host every render, with the Inbox screen's tab (or the
    /// open detail's `origin.inbox_tab`) for the Inbox kind. The same kind
    /// and the same tab as the LAST push = a no-op, no notify. A pushed tab
    /// that CHANGED becomes the list's tab (the Inbox screen flipped, or a
    /// detail picked from the other tab opened); an unchanged one leaves a
    /// local flip beside a detail alone — but ON the Inbox screen the pushed
    /// tab is the screen's and the list always shows it. A new kind resets
    /// the list.
    pub(crate) fn set_side(
        &mut self,
        kind: SecondSidebar,
        inbox_tab: Option<InboxTab>,
        cx: &mut gpui::Context<Self>,
    ) {
        let screen_list_tab = matches!(resolved_screen(&self.nav, cx), Some(Screen::Inbox { .. }))
            .then_some(self.nav_inbox_tab);
        match side_push(self.side, self.side_tab, kind, inbox_tab, screen_list_tab) {
            SidePush::Same => return,
            SidePush::NewKind => {}
            SidePush::NewTab => {
                self.side_tab = inbox_tab;
                if let Some(tab) = inbox_tab.filter(|tab| *tab != self.nav_inbox_tab) {
                    self.nav_inbox_tab = tab;
                    self.nav_list_scroll.scroll_to_item(0, ScrollStrategy::Top);
                    cx.notify();
                }
                return;
            }
        }
        self.side = Some(kind);
        self.side_tab = inbox_tab;
        if let Some(tab) = inbox_tab {
            self.nav_inbox_tab = tab;
        }
        // EXP-863: the selection is per list, like the big list's
        // (`set_query` clears it on a scope change).
        self.nav_selected.clear();
        self.nav_select_anchor = None;
        // EXP-915: a new list starts at its top (the issue lists share one
        // virtual-list scroll) …
        self.nav_list_scroll.scroll_to_item(0, ScrollStrategy::Top);
        // … and the outgoing list's memoized rows go with it: the slots are
        // keyed, so keeping them only pinned an `Rc` of a query nothing will
        // ask for again.
        self.nav_data.clear();
        self.nav_statuses.clear();
        self.inbox_data.clear();
        self.reviews_data.clear();
        cx.notify();
    }

    /// EXP-1192: the list [`Self::side`] names, as the origin its rows pin on
    /// the detail they open (Recent runs is not a `ListPanel` body).
    fn side_origin(&self) -> Option<crate::navigation::TabOrigin> {
        let (tool, inbox_tab) = match self.side? {
            SecondSidebar::Inbox => (ToolWindow::Inbox, Some(self.nav_inbox_tab)),
            SecondSidebar::Reviews => (ToolWindow::Reviews, None),
            SecondSidebar::RecentRuns => return None,
        };
        Some(crate::navigation::TabOrigin {
            tool,
            board_id: None,
            inbox_tab,
        })
    }

    pub fn new(mode: ListMode, window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        let nav = nav_for_window(window, cx);
        let shared = rail_shared_for_window(window, cx);
        let git_bar = shared.read(cx).git_bar.clone();
        let board_active = shared.read(cx).board_active.clone();
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
            side: None,
            side_tab: None,
            board_active,
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
            nav_row_statuses: Rc::new(Vec::new()),
            nav_active_issue_id: None,
            nav_row_origin: None,
            nav_list_scroll: VirtualListScrollHandle::new(),
            inbox_data: queries::Memo::default(),
            reviews_data: queries::Memo::default(),
            _subscriptions: subscriptions,
        }
    }

    // -- shared chrome -------------------------------------------------------

    /// EXP-282: the icon-tab strip that REPLACED the icon+title header on the
    /// tabbed Inbox tool window. EXP-525: chips sit LEFT
    /// (web parity — the inbox pills are left-aligned rows there);
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
    /// `Nav` mode that is the second sidebar the host pushed
    /// ([`Self::side_origin`], with the list's CURRENT Inbox tab).
    fn row_origin(&self, cx: &App) -> Option<crate::navigation::TabOrigin> {
        let screen = resolved_screen(&self.nav, cx);
        row_origin_for(self.mode, self.side_origin().as_ref(), screen.as_ref())
    }

    /// EXP-862: open `screen` FROM this list — explicitly, so the second
    /// sidebar keeps showing the rows the click came from. Every row of every
    /// mode goes through here.
    fn open_from_list(&self, screen: Screen, window: &Window, cx: &mut App) {
        match self.row_origin(cx) {
            Some(origin) => crate::navigation::navigate_from(window, cx, screen, origin),
            None => navigate(window, cx, screen),
        }
    }

    // -- issue tool windows ---------------------------------------------------

    /// The Inbox second sidebar (EXP-186/EXP-1192): the merged personal
    /// surface — an Inbox tab (notification stream) + a My Issues tab (issues
    /// assigned to me across the team), switched by the segmented strip on
    /// top (Mark all read on its right), mirroring mobile's My Work screen.
    /// No back row: the Inbox IS this list, the main pane beside it shows
    /// what a row opened.
    fn render_inbox_tool(&mut self, cx: &mut gpui::Context<Self>) -> gpui::AnyElement {
        let tab = self.nav_inbox_tab;
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
            let body = self.render_my_issues_nav(cx);
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
                    queries::InboxEntry::Message(entry) => self.inbox_message_row(entry, cx),
                    queries::InboxEntry::Session(entry) => self.inbox_session_row(entry, cx),
                })
                .collect();
            crate::scroll_pane::SidebarScrollArea::new(
                "mini-inbox-scroll",
                v_flex().p_1().gap_0p5().children(rows),
            )
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
        // EXP-933: an agent's message about this issue opens its Results.
        let opens_results = group.opens_results();
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
                let screen = Screen::IssueDetail {
                    issue_id: issue_id.clone(),
                };
                if opens_results {
                    let origin = this.row_origin(cx);
                    crate::work_header::open_issue_results(&issue_id, window, cx, move |window, cx| {
                        match origin {
                            Some(origin) => {
                                crate::navigation::navigate_from(window, cx, screen, origin)
                            }
                            None => navigate(window, cx, screen),
                        }
                    });
                    return;
                }
                this.open_from_list(screen, window, cx);
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
        // EXP-1192: the run this row opened reads selected beside it, like
        // an issue row beside its detail.
        let selected = session_id.as_deref().is_some_and(|id| {
            matches!(
                resolved_screen(&self.nav, cx),
                Some(Screen::Session { session_id }) if session_id == id
            )
        });
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
            .when(selected, |this| this.bg(theme.list_active))
            .hover(|this| this.bg(theme.list_hover))
            .cursor_pointer()
            .on_click(cx.listener(move |this, _, window, cx| {
                // Web `markGroupRead`, then open the run itself (a
                // cross-team row switches the window's team first), pinned
                // to this list so the Inbox stays beside it (EXP-1192). A
                // pruned run leads nowhere.
                mark_group_read(&unread_ids, cx);
                let Some(session_id) = session_id.clone() else {
                    return;
                };
                if let Some(team_id) = target_team.clone() {
                    if active_team_id(&this.nav, cx).as_deref() != Some(team_id.as_str()) {
                        switch_team(window, cx, team_id);
                    }
                }
                let origin = this.row_origin(cx);
                crate::session_screen::open_session_with_origin(
                    &session_id,
                    origin,
                    window,
                    cx,
                );
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

    /// Switch the Inbox tab (EXP-186). On the Inbox SCREEN the tab is the
    /// screen's — `set_screen`, not a navigation, so a flip never stacks
    /// history (the host pushes the new tab back through [`Self::set_side`]).
    /// Beside a detail it is local state and the open detail stays put.
    fn set_inbox_tab(&mut self, tab: InboxTab, window: &mut Window, cx: &mut gpui::Context<Self>) {
        if self.nav_inbox_tab != tab {
            self.nav_inbox_tab = tab;
            self.nav_list_scroll.scroll_to_item(0, ScrollStrategy::Top);
            cx.notify();
        }
        if matches!(resolved_screen(&self.nav, cx), Some(Screen::Inbox { .. })) {
            crate::navigation::set_screen(window, cx, Some(Screen::Inbox { tab }));
        }
    }

    /// The full-width board list ([`ListMode::Screen`]): the board view — filter bar
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

    // -- side list bulk selection (EXP-863) -----------------------------------

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

    /// The bulk bar over the side list's selection, or `None` while nothing
    /// is selected (or the list shows no issue rows / no team is in
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
        // Icon-only and allowed to fold: the side list is a narrow column.
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

    // -- second sidebar bodies (EXP-851/EXP-1192) -----------------------------

    /// The Reviews side list's back row — the SETTINGS nav's row, shared
    /// (`settings::nav_back_row`): "Reviews", hopping out to the full queue
    /// page. The Inbox side has none (the Inbox IS its list).
    fn nav_back_row(&self, cx: &mut gpui::Context<Self>) -> gpui::AnyElement {
        crate::settings::nav_back_row("list-nav-back", "Reviews", cx)
            .on_click(cx.listener(|_, _: &ClickEvent, window, cx| {
                crate::navigation::go_back_to(window, cx, Screen::Reviews);
            }))
            .into_any_element()
    }

    /// The Inbox side list's My Issues body: plain rows — status glyph,
    /// identifier, title, the open detail highlighted — over the team-wide
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

    /// EXP-915: the side list's virtualized issue body — the
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
                    .child(crate::scroll_pane::sidebar_scrollbar_layer(
                        (gpui::ElementId::from(scroll_id), "scrollbar"),
                        &self.nav_list_scroll,
                    )),
            )
            .into_any_element()
    }

    /// One virtual-list item of the side list's issue rows (EXP-915): the row
    /// at `ix` in the column's `px_2` gutter, with its trailing gap.
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
            } => self.nav_group_header(status, *count, *collapsed, cx),
            NavRow::Issue {
                index,
                issue,
                guides,
                rail,
            } => {
                let statuses = self.nav_row_statuses.clone();
                let any_selected = !self.nav_selected.is_empty();
                let guides = guides.clone();
                self.nav_issue_row(*index, issue, &statuses, any_selected, &guides, rail, cx)
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

    /// EXP-862: the side list's issue rows, grouped by STATUS exactly like the
    /// full-width list — a group band (glyph, name, count over the status'
    /// own tint) that folds its rows away when clicked.
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
        let rail = domain::issue_rail::issue_rail(&entries, counts);
        self.nav_rail_width = rail.width();
        for (row, slice) in rows.iter_mut().zip(rail.entries) {
            if let NavRow::Issue { rail, .. } = row {
                *rail = slice;
            }
        }
        self.nav_rows = Rc::new(rows);
    }

    /// One side-list status band — the big list's group header
    /// (`issue_list::render_group_header`) at the narrow column's density: a
    /// bare chevron, the status glyph, its name and the count, over a wash in
    /// the status' own hue. The WHOLE band folds the group (EXP-862 ×4 — the
    /// chevron is a disclosure marker, not a separate target).
    fn nav_group_header(
        &self,
        status: &domain::statuses::ResolvedStatus,
        count: usize,
        collapsed: bool,
        cx: &mut gpui::Context<Self>,
    ) -> gpui::AnyElement {
        let group_key = status.group_key.clone();
        let muted = cx.theme().muted_foreground;
        h_flex()
            .id(SharedString::from(format!("list-nav-group-{group_key}")))
            .w_full()
            .h(px(24.))
            .px_1p5()
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

    /// One side-list issue row (spec C: status glyph, identifier, title; the
    /// open detail highlighted). A plain click opens that detail pinned to
    /// this list ([`Self::open_from_list`]), so the sidebar stays beside it.
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
        // column and the node; EXP-1057: hovering the dot opens the graph.
        .when(self.nav_rail_width > 0., |row| {
            row.child(div().flex_shrink_0().w(px(self.nav_rail_width)))
        })
        .children(crate::issue_rail::rail_node(
            format!("nav-rail-node-{}", issue.id),
            &issue.id,
            rail,
            NAV_ROW_PAD,
            cx,
        ))
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

    /// The Reviews side list's body: the open-PR queue's rows, each opening
    /// its issue on the Changes face (the Reviews page's own click target),
    /// then the page's "Agent runs" (EXP-734/EXP-1194): the issue-less runs
    /// holding a PR of their own, each opening the RUN on its Changes face.
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
        let screen = resolved_screen(&self.nav, cx);
        let (open_issue, open_run) = open_review(screen.as_ref());
        let mut rows: Vec<gpui::AnyElement> = groups
            .iter()
            .flat_map(|group| group.entries.iter())
            .enumerate()
            .map(|(index, entry)| {
                let issue = entry.representative();
                // EXP-1154: the row opens the issue on its Changes face.
                let screen = Screen::IssueDetail {
                    issue_id: issue.id.clone(),
                };
                let changes_for = issue.id.clone();
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
                    crate::screens::request_issue_changes(&changes_for, None, window, cx);
                    this.open_from_list(screen.clone(), window, cx);
                }))
                .into_any_element()
            })
            .collect();
        // EXP-1194: the page's "Agent runs" — the same synced read, so the
        // run a row there opened is listed (and lit) beside it.
        for (index, run) in queries::review_runs(cx, &team_id).iter().enumerate() {
            let (number, title) = review_run_texts(run);
            let active = open_run == Some(run.id.as_str());
            let lead = div()
                .flex_shrink_0()
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .font_family(theme::terminal::FONT_FAMILY)
                .child(SharedString::from(number))
                .into_any_element();
            let run_id = run.id.clone();
            rows.push(
                rail_row_lead(
                    ("list-nav-review-run", index),
                    lead,
                    SharedString::from(title),
                    active,
                    None,
                    None,
                    cx,
                )
                .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                    // The page's own click (`ReviewsView::run_row`): the run
                    // on its Changes face, pinned to this list.
                    crate::session_screen::open_session_with_origin(
                        &run_id,
                        this.row_origin(cx),
                        window,
                        cx,
                    );
                    crate::screens::set_run_face(
                        &run_id,
                        crate::screens::RunFace::Diff,
                        window,
                        cx,
                    );
                }))
                .into_any_element(),
            );
        }
        if rows.is_empty() {
            return self.list_note("No open pull requests.", cx);
        }
        self.nav_scroll("list-nav-reviews-scroll", rows, cx)
    }

    /// The side list's plain scroll body.
    fn nav_scroll(
        &self,
        id: &'static str,
        rows: Vec<gpui::AnyElement>,
        _cx: &mut gpui::Context<Self>,
    ) -> gpui::AnyElement {
        crate::scroll_pane::SidebarScrollArea::new(
            id,
            v_flex().w_full().min_w_0().px_2().gap_0p5().children(rows),
        )
        .into_any_element()
    }

}

impl Render for ListPanel {
    fn render(&mut self, _window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let screen = resolved_screen(&self.nav, cx);
        let body = match self.mode {
            ListMode::Screen => match screen {
                Some(Screen::BoardIssues { board_id }) => {
                    let board_id = (!board_id.is_empty())
                        .then_some(board_id)
                        .or_else(|| active_board_id(&self.nav, cx));
                    self.render_board_issues_tool(board_id, cx)
                }
                // EXP-1192: the panel is only mounted for the board list
                // (the Inbox's list is the second sidebar).
                _ => div().into_any_element(),
            },
            ListMode::Nav => {
                // EXP-863: the issue bodies refill these; any other list
                // leaves them empty, which hides the bulk bar.
                self.nav_issue_ids.clear();
                self.nav_visible_ids.clear();
                // EXP-1192: the host draws the column's hairline and resize
                // edge; the list starts at its own top under the tab band —
                // the Inbox at its tab strip, Reviews at its back row.
                let header: Option<gpui::AnyElement> = match self.side {
                    Some(SecondSidebar::Reviews) => Some(
                        v_flex()
                            .flex_shrink_0()
                            .w_full()
                            .child(self.nav_back_row(cx))
                            .child(crate::settings::nav_back_rule(cx))
                            .into_any_element(),
                    ),
                    _ => None,
                };
                let body = match self.side {
                    Some(SecondSidebar::Inbox) => Some(self.render_inbox_tool(cx)),
                    Some(SecondSidebar::Reviews) => Some(self.render_reviews_nav(cx)),
                    // Recent runs is the host's own `RecentRunsNav`.
                    Some(SecondSidebar::RecentRuns) | None => None,
                };
                match body {
                    Some(body) => {
                        // EXP-863: the bulk bar FLOATS over the bottom of the
                        // rows while a selection exists (EXP-289's no-jump
                        // rule — the rows never move for it).
                        let bulk_bar = self.nav_bulk_bar(cx);
                        v_flex()
                            .flex_1()
                            .min_h_0()
                            .min_w_0()
                            .children(header)
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
        focused_list, row_origin_for, side_push, yolo_rail, InboxTab, ListMode, SidePush,
        ToolWindow, YoloRail,
    };
    use crate::navigation::{Screen, SecondSidebar, TabOrigin};

    /// EXP-1105: yolo mode hides Files / Source Control, but a git failure
    /// (trunk attention or a sync error) brings Source Control back; Reviews
    /// passes `reviews_nav(..).shows` through (EXP-1244, fixture-locked in
    /// `domain::reviews_queue`). Off = today's rail.
    #[test]
    fn yolo_rail_hides_entries_until_a_failure_surfaces() {
        for (reviews, sc) in [(false, false), (true, false), (false, true), (true, true)] {
            assert_eq!(
                yolo_rail(false, reviews, sc),
                YoloRail {
                    reviews,
                    files: true,
                    source_control: true,
                }
            );
        }
        assert_eq!(
            yolo_rail(true, false, false),
            YoloRail {
                reviews: false,
                files: false,
                source_control: false,
            }
        );
        assert_eq!(
            yolo_rail(true, true, true),
            YoloRail {
                reviews: true,
                files: false,
                source_control: true,
            }
        );
    }

    /// EXP-1244: the rail's Reviews entry = `reviews_nav` over the active
    /// team (none = shown): yolo hides it only while the queue is empty.
    #[test]
    fn reviews_entry_follows_reviews_nav() {
        use domain::reviews_queue::reviews_nav;
        assert!(reviews_nav(None::<bool>.as_slice(), 0).shows);
        assert!(reviews_nav(Some(false).as_slice(), 0).shows);
        assert!(!reviews_nav(Some(true).as_slice(), 0).shows);
        let failed = reviews_nav(Some(true).as_slice(), 1);
        assert!(failed.shows && failed.dot);
    }

    /// EXP-851: every list in the origin vocabulary maps onto exactly the
    /// SCREEN it became — what a rail entry opens and what the legacy
    /// `activate_tool` callers mean.
    #[test]
    fn every_list_names_its_screen() {
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
        assert_eq!(ToolWindow::Files.origin_screen(None), Screen::Files);
        assert_eq!(
            ToolWindow::SourceControl.origin_screen(None),
            Screen::SourceControl
        );
        assert_eq!(ToolWindow::Reviews.origin_screen(None), Screen::Reviews);
    }

    /// EXP-1192: the host pushes the second sidebar every render. Only a new
    /// kind resets the list; only a CHANGED pushed tab moves the Inbox tab
    /// (so a local flip beside a detail survives the next unchanged push).
    #[test]
    fn a_side_push_changes_only_what_moved() {
        use InboxTab::{Inbox, MyIssues};
        use SecondSidebar::{Inbox as InboxSide, Reviews};
        // First push, and a kind change: reset.
        assert_eq!(side_push(None, None, InboxSide, Some(Inbox), None), SidePush::NewKind);
        assert_eq!(
            side_push(Some(Reviews), None, InboxSide, Some(Inbox), None),
            SidePush::NewKind
        );
        assert_eq!(
            side_push(Some(InboxSide), Some(Inbox), Reviews, None, None),
            SidePush::NewKind
        );
        // The same push again: nothing (no notify).
        assert_eq!(
            side_push(Some(InboxSide), Some(Inbox), InboxSide, Some(Inbox), None),
            SidePush::Same
        );
        assert_eq!(side_push(Some(Reviews), None, Reviews, None, None), SidePush::Same);
        // The Inbox screen flipped tabs (or a detail from the other tab
        // opened): the list follows.
        assert_eq!(
            side_push(Some(InboxSide), Some(Inbox), InboxSide, Some(MyIssues), None),
            SidePush::NewTab
        );
        // On the Inbox SCREEN the pushed tab is the screen's: a list left on
        // the other tab by a local flip beside a detail follows it, even
        // though the push itself did not change …
        assert_eq!(
            side_push(Some(InboxSide), Some(Inbox), InboxSide, Some(Inbox), Some(MyIssues)),
            SidePush::NewTab
        );
        // … and an agreeing list is still a no-op.
        assert_eq!(
            side_push(Some(InboxSide), Some(Inbox), InboxSide, Some(Inbox), Some(Inbox)),
            SidePush::Same
        );
        // A missing tab never counts as a change.
        assert_eq!(
            side_push(Some(InboxSide), Some(MyIssues), InboxSide, None, None),
            SidePush::Same
        );
    }

    /// EXP-1194: the Reviews side list lights the open issue's row OR the
    /// open run's — an agent-run PR opens `Screen::Session`, never an issue.
    #[test]
    fn the_reviews_side_list_lights_the_open_issue_or_run() {
        let issue = Screen::IssueDetail {
            issue_id: "issue-1".into(),
        };
        let run = Screen::Session {
            session_id: "sess-1".into(),
        };
        assert_eq!(super::open_review(Some(&issue)), (Some("issue-1"), None));
        assert_eq!(super::open_review(Some(&run)), (None, Some("sess-1")));
        assert_eq!(super::open_review(Some(&Screen::Reviews)), (None, None));
        assert_eq!(super::open_review(None), (None, None));
    }

    /// EXP-1194: a run row reads like the page's "Agent runs" row — `#N` and
    /// the action's name; a chat run its agent title, else "Chat".
    #[test]
    fn a_review_run_row_reads_like_the_pages() {
        let run = |value: serde_json::Value| -> domain::rows::CodingSession {
            serde_json::from_value(value).unwrap()
        };
        let action = run(serde_json::json!({
            "id": "sess-1", "pr_number": 42, "action_id": "a1", "action_name": "Release notes",
        }));
        assert_eq!(
            super::review_run_texts(&action),
            ("#42".to_string(), "Release notes".to_string())
        );
        let chat = run(serde_json::json!({ "id": "sess-2" }));
        assert_eq!(
            super::review_run_texts(&chat),
            (String::new(), domain::batch_run::CHAT_RUN_NAME.to_string())
        );
    }

    /// EXP-862: which list a row click pins on the detail it opens. The bug
    /// this rule fixes is the `Nav` line: beside a RAIL-opened detail the
    /// breadcrumb rule derives nothing, so a click in the second sidebar
    /// would slide it away mid-click.
    #[test]
    fn a_row_pins_its_own_list() {
        let inbox = TabOrigin {
            tool: ToolWindow::Inbox,
            board_id: None,
            inbox_tab: Some(InboxTab::MyIssues),
        };
        let issue = Screen::IssueDetail {
            issue_id: "i1".into(),
        };
        // The second sidebar: whatever it is rendering comes along (with
        // its current tab), and the screen that is up is irrelevant (here: a
        // rail-opened detail).
        assert_eq!(
            row_origin_for(ListMode::Nav, Some(&inbox), Some(&issue)),
            Some(inbox.clone())
        );
        assert_eq!(row_origin_for(ListMode::Nav, None, Some(&issue)), None);
        // The full-width board list pins nothing — EXP-1192: its issues
        // open with nothing beside them.
        assert_eq!(
            row_origin_for(
                ListMode::Screen,
                None,
                Some(&Screen::BoardIssues {
                    board_id: "b1".into()
                })
            ),
            None
        );
        assert_eq!(
            row_origin_for(ListMode::Screen, None, Some(&Screen::Reviews))
                .map(|origin| origin.tool),
            Some(ToolWindow::Reviews)
        );
        // A screen that is no list pins nothing (the panel is only mounted
        // for the board list, so this is the defensive arm).
        assert_eq!(row_origin_for(ListMode::Screen, None, Some(&issue)), None);
        assert_eq!(row_origin_for(ListMode::Screen, None, None), None);
    }

    /// EXP-851: what a window READS as, for the OS-notification redundancy
    /// check — the list screens are themselves, and everything else (a
    /// session included) falls back to the board list (which is "not the
    /// notification stream").
    #[test]
    fn the_focused_list_follows_the_screen() {
        assert_eq!(
            focused_list(Some(&Screen::Inbox {
                tab: InboxTab::MyIssues
            })),
            (ToolWindow::Inbox, InboxTab::MyIssues)
        );
        assert_eq!(focused_list(Some(&Screen::Reviews)).0, ToolWindow::Reviews);
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
