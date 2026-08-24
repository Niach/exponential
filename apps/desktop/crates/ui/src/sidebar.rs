//! The team sidebar (masterplan-v3 §4.2, reworked as a JetBrains-style
//! tool-window rail).
//!
//! Two cooperating views share per-window state through [`RailShared`]:
//!
//! - [`RailView`] — a 44px icon-only strip owned by the `Shell` shell and
//!   rendered OUTSIDE the `DockArea`, full height below the top bar. Top: the
//!   Search action, then the tool-window selectors — **Inbox / My Issues /
//!   Board Issues / Reviews** (mini issue lists; Reviews carries a
//!   dot while open PRs exist) and **Files / Source Control** (Source Control carries
//!   an amber badge while the trunk needs attention — a paused conflict,
//!   local commits, or a dirty tree, EXP-346 — and opens the changes
//!   screen immediately). The active tool's icon is tinted with the active
//!   board's color. One tool is ALWAYS active — re-clicking never
//!   unselects. Bottom: terminal-dock toggle, settings gear, and the
//!   **account button as the very bottom element** — its dropdown holds the
//!   account-level actions only (EXP-69: team switching moved into the
//!   top bar's merged board picker).
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
    FontWeight, Hsla, InteractiveElement as _, IntoElement, MouseButton, ParentElement, Render,
    ScrollHandle, SharedString, StatefulInteractiveElement as _, Styled, Subscription, Window,
    WindowControlArea, WindowId,
};
use gpui_component::{
    button::{Button, ButtonVariants as _},
    h_flex,
    menu::DropdownMenu as _,
    scroll::ScrollableElement as _,
    skeleton::Skeleton,
    spinner::Spinner,
    v_flex, ActiveTheme as _, Disableable as _, Icon, InteractiveElementExt as _,
    Selectable as _, Sizable as _,
};
use sync::Store;


// EXP-282: `OpenSettings` is gone from this file — the rail gear navigates
// directly (EXP-17) and the account dropdown no longer duplicates it.
use crate::actions::{CreateTeam, JoinTeam, SignOut, SwitchTeam};
use crate::board::BoardView;
use crate::coding_flow;
use crate::trunk_sync::TrunkSync;
use crate::icons::{self, registry, ExpIcon};
use crate::issue_list::IssueQuery;
use crate::navigation::{
    active_board_id, active_team_id, nav_for_window, navigate, resolved_screen, switch_team,
    Navigation, Screen,
};
use crate::issue_header::parse_hex_color;
use crate::queries;

/// Width of the icon-only rail column (outside the dock area) — the
/// COLLAPSED rail.
pub(crate) const RAIL_W: f32 = 44.;

/// EXP-282: width of the EXPANDED rail (the Cursor-style labelled rail) —
/// wide enough for a board name at `text_sm` without eating the tool column.
/// EXP-285: trimmed 184 → 164 per feedback.
pub(crate) const RAIL_EXPANDED_W: f32 = 164.;

/// Default tool-window width (EXP-109: doubled from the original 260px web
/// parity — the issue lists inside the tool window were too cramped).
pub(crate) const DEFAULT_DOCK_WIDTH: f32 = 520.;

/// Minimum tool-window width (EXP-426): sized to the widest single-line
/// occupant — the issue list's inline bulk-action bar ("N selected" + clear
/// + 6 icon controls at `gap_2`, inside the bar's `px_4` inset). Keep in
/// step with `issue_list::render_bulk_bar`'s children.
pub(crate) const MIN_DOCK_WIDTH: f32 = 320.;

/// The rail's tool windows (JetBrains tool-window bar). One is ALWAYS active
/// — there is deliberately no unselected/collapsed state.
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
    /// Open pull requests across the team: issue-linked ones grouped by
    /// board, plus GitHub-listed PRs not linked to any issue grouped by
    /// repo — both with an inline squash-merge action (server-side via the
    /// GitHub App).
    Reviews,
    /// Support tickets of the active team (EXP-180 — server-only tRPC data,
    /// polled). The rail icon renders only while the active team's synced
    /// `helpdesk_enabled` flag is on.
    Support,
    /// The trunk file tree at full panel height.
    Files,
    /// The trunk's local branches; activating also opens the changes screen.
    SourceControl,
}

/// The Inbox tool window's active tab (EXP-186 — sticky across tool
/// switches, like mobile's persisted My Work segment).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum InboxTab {
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
    tool: ToolWindow,
    /// The Inbox tool window's active tab (EXP-186).
    inbox_tab: InboxTab,
    /// The headless trunk-sync engine (EXP-253 — nothing renders it; the
    /// rail paints a status badge off its state). Driven every rail render
    /// so the §4.1 auto-clone lifecycle and the rail's sync/conflict badge
    /// stay live regardless of the visible screen.
    git_bar: Entity<TrunkSync>,
    file_tree: Entity<crate::file_tree::FileTreeView>,
    /// The Board Issues tool window's board (filter bar + grouped list,
    /// scoped to the active board). Shared here — not on `SidebarPanel` —
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
    /// EXP-282: whether the rail renders EXPANDED (labelled rows) instead of
    /// the 44px icon strip. Per-window runtime state, seeded from — and
    /// persisted back to — the per-install `settings.json` (`railExpanded`).
    rail_expanded: bool,
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

    /// The active tool window (EXP-288 — the screens panel reads it for the
    /// tab-less center default and for tab-origin capture).
    pub(crate) fn tool(&self) -> ToolWindow {
        self.tool
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

    /// The issue list the CURRENT rail state shows: the My Issues board
    /// while the Inbox tool window shows its My Issues tab, the active
    /// board's list otherwise. Since EXP-426 the detail's prev/next switcher
    /// prefers the active TAB's remembered origin and uses this only as its
    /// fallback (undocked windows, origin-less tabs).
    pub(crate) fn active_issue_board(&self) -> &Entity<BoardView> {
        match (self.tool, self.inbox_tab) {
            (ToolWindow::Inbox, InboxTab::MyIssues) => &self.board_my,
            _ => &self.board_active,
        }
    }

    /// The Inbox tool window's active tab (EXP-426 — stamped into a new
    /// detail tab's [`crate::navigation::TabOrigin`]).
    pub(crate) fn inbox_tab(&self) -> InboxTab {
        self.inbox_tab
    }

    /// The two issue boards by role (EXP-426 — the switcher's origin
    /// resolution needs a specific one, not the rail-state pick).
    pub(crate) fn board_my(&self) -> &Entity<BoardView> {
        &self.board_my
    }

    pub(crate) fn board_active(&self) -> &Entity<BoardView> {
        &self.board_active
    }

    /// Both issue boards (the detail view observes them so the EXP-48
    /// counter re-renders on filter changes).
    pub(crate) fn issue_boards(&self) -> [&Entity<BoardView>; 2] {
        [&self.board_active, &self.board_my]
    }

    /// EXP-282: the settings nav's selected section (raw — callers clamp it
    /// through `settings::effective_selection`).
    pub(crate) fn settings_section(&self) -> crate::settings::SettingsSection {
        self.settings_section.clone()
    }
}

/// Toggle the rail between the icon strip and the labelled rail (EXP-282),
/// persisting the choice to the per-install `settings.json` so the next
/// launch opens the way this one closed.
pub(crate) fn toggle_rail_expanded(window: &mut Window, cx: &mut App) {
    let shared = rail_shared_for_window(window, cx);
    let expanded = shared.update(cx, |shared, cx| {
        shared.rail_expanded = !shared.rail_expanded;
        cx.notify();
        shared.rail_expanded
    });
    let hub = coding_flow::CodingHub::global(cx);
    let mut settings = hub.read(cx).settings.clone();
    settings.rail_expanded = Some(expanded);
    if let Err(err) = coding_flow::CodingHub::save_settings(&hub, settings, cx) {
        log::warn!("[ui] persisting the rail state failed: {err}");
    }
}

/// EXP-285: the rail's expanded state for this window — read by
/// `AppTitleBar` (the tab strip's width budget) and by the `Shell` (the
/// macOS traffic-light tongue).
pub(crate) fn rail_expanded(window: &mut Window, cx: &mut App) -> bool {
    rail_shared_for_window(window, cx).read(cx).rail_expanded
}

/// EXP-456: flip the rail's expanded state WITHOUT touching the persisted
/// preference. The Shell expands a collapsed rail while Settings is up (the
/// swap animation reads as expand-then-slide) and recollapses it on the way
/// back — a `settings.json` write here would overwrite what the user chose.
pub(crate) fn set_rail_expanded_transient(window: &mut Window, cx: &mut App, expanded: bool) {
    let shared = rail_shared_for_window(window, cx);
    shared.update(cx, |shared, cx| {
        if shared.rail_expanded != expanded {
            shared.rail_expanded = expanded;
            cx.notify();
        }
    });
}

/// The rail expand/collapse toggle (EXP-282). EXP-326: shared, because the
/// collapsed windowed-macOS case renders it in the `Shell`'s traffic-light
/// tongue instead of the rail's own strip — one recipe, two hosts.
///
/// Direct call (EXP-17): rail buttons must not dispatch App-global actions.
pub(crate) fn rail_toggle_button(id: &'static str, expanded: bool) -> Button {
    Button::new(id)
        .ghost().cursor_pointer()
        .small()
        .icon(if expanded {
            registry::NAV_RAIL_COLLAPSE
        } else {
            registry::NAV_RAIL_EXPAND
        })
        .tooltip(if expanded {
            "Collapse sidebar"
        } else {
            "Expand sidebar"
        })
        .on_click(|_: &ClickEvent, window, cx| toggle_rail_expanded(window, cx))
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

/// DEV-ONLY `EXP_DEV_TOOL` values: `inbox` | `my-issues` | `board` |
/// `reviews` | `support` | `files` | `source-control` (anything else = the
/// ordinary default). See [`rail_shared_for_window`].
fn parse_dev_tool(spec: &str) -> Option<ToolWindow> {
    match spec {
        // Both tabs live in the one Inbox tool window (EXP-186).
        "inbox" | "my-issues" => Some(ToolWindow::Inbox),
        "board" | "board-issues" | "issues" => Some(ToolWindow::BoardIssues),
        "reviews" => Some(ToolWindow::Reviews),
        "support" => Some(ToolWindow::Support),
        "files" => Some(ToolWindow::Files),
        "source-control" => Some(ToolWindow::SourceControl),
        _ => None,
    }
}

/// DEV-ONLY `EXP_DEV_INBOX_TAB` values: `inbox` | `my-issues`.
fn parse_dev_inbox_tab(spec: &str) -> Option<InboxTab> {
    match spec {
        "inbox" => Some(InboxTab::Inbox),
        "my-issues" => Some(InboxTab::MyIssues),
        _ => None,
    }
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
        "account" => Some(S::Account),
        "notifications" => Some(S::Notifications),
        "api-keys" => Some(S::ApiKeys),
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
    // EXP-525: My Issues hosts its Filter trigger in the Inbox tool strip.
    board_my.update(cx, |board, _| board.set_external_filter(true));
    // EXP-282: the persisted rail state. Read through the coding hub — it
    // owns `settings.json`. EXP-285: absent = EXPANDED (the labelled rail is
    // the default look now); an explicit user collapse still sticks.
    let rail_expanded = coding_flow::CodingHub::global(cx)
        .read(cx)
        .settings
        .rail_expanded
        .unwrap_or(true);
    // DEV-ONLY (§11.4 headless verification, same family as
    // EXP_DEV_SERVER/EXP_DEV_SCREEN): pre-select the rail tool, the Inbox
    // tab and the settings section so a capture run lands on one surface
    // without synthetic input. Unset/unknown = the ordinary defaults below.
    // Never document for users.
    let dev_tool = std::env::var("EXP_DEV_TOOL").ok();
    let dev_tool = dev_tool.as_deref().map(str::trim);
    let dev_inbox_tab = std::env::var("EXP_DEV_INBOX_TAB").ok();
    let shared = cx.new(|_| RailShared {
        // Issues-first default: the active board's issue list.
        tool: dev_tool.and_then(parse_dev_tool).unwrap_or(ToolWindow::BoardIssues),
        inbox_tab: dev_inbox_tab
            .as_deref()
            .map(str::trim)
            .and_then(parse_dev_inbox_tab)
            // `my-issues` names the Inbox tool's My Issues tab, so it seeds
            // the tab too (EXP_DEV_INBOX_TAB still wins).
            .unwrap_or(if dev_tool == Some("my-issues") {
                InboxTab::MyIssues
            } else {
                InboxTab::Inbox
            }),
        git_bar,
        file_tree,
        board_active,
        board_my,
        sc_selection: ScSelection::None,
        selected_file: None,
        rail_expanded,
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

/// Drop a closed window's entry (called from the `Shell` release hook,
/// mirroring `navigation::remove_window`).
pub fn remove_window(window_id: WindowId, cx: &mut App) {
    if let Some(registry) = cx.try_global::<RailRegistry>() {
        if registry.by_window.contains_key(&window_id) {
            cx.global_mut::<RailRegistry>().by_window.remove(&window_id);
        }
    }
}

/// Select `tool` in this window's rail AND deselect the active center tab
/// (EXP-288: a rail-entry click always shows the tool's own center content —
/// the SC diff, the file viewer, or the empty state — never a stale detail
/// tab from another context). Loop-safe: `set_screen` only notifies nav; the
/// screens panel's `sync_tabs` early-returns on `None` and never writes back
/// here.
pub(crate) fn activate_tool(window: &mut Window, cx: &mut App, tool: ToolWindow) {
    set_tool_inner(window, cx, tool);
    crate::navigation::set_screen(window, cx, None);
}

/// Select `tool` WITHOUT touching the center tab (EXP-288 — the tab-click
/// path: activating a tab re-selects its origin tool, then sets its screen).
pub(crate) fn select_tool_for_tab(window: &mut Window, cx: &mut App, tool: ToolWindow) {
    set_tool_inner(window, cx, tool);
}

/// Restore the Inbox tool window's tab WITHOUT touching the center tab
/// (EXP-426 — the tab-activation path; `activate_tool`/`open_inbox_tab`
/// would `set_screen(None)` and close the tab being activated).
pub(crate) fn select_inbox_tab_for_tab(window: &mut Window, cx: &mut App, tab: InboxTab) {
    let shared = rail_shared_for_window(window, cx);
    shared.update(cx, |shared, cx| {
        if shared.inbox_tab != tab {
            shared.inbox_tab = tab;
            cx.notify();
        }
    });
}

/// The shared tool switch (re-selecting the active tool is a no-op — a tool
/// window can never be unselected).
fn set_tool_inner(window: &mut Window, cx: &mut App, tool: ToolWindow) {
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
}

/// Activate the Inbox tool window ON a specific tab (the `OpenInbox` /
/// `OpenMyIssues` actions — plain rail clicks keep the sticky tab instead).
pub(crate) fn open_inbox_tab(window: &mut Window, cx: &mut App, tab: InboxTab) {
    let shared = rail_shared_for_window(window, cx);
    shared.update(cx, |shared, cx| {
        if shared.inbox_tab != tab {
            shared.inbox_tab = tab;
            cx.notify();
        }
    });
    activate_tool(window, cx, ToolWindow::Inbox);
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

/// The window's active-board accent color (rail selection tint, falls back
/// to the theme primary when the board has no color).
fn board_accent(nav: &Entity<Navigation>, cx: &App) -> Hsla {
    active_board_id(nav, cx)
        .and_then(|id| {
            Store::global(cx)
                .collections()
                .boards
                .read(cx)
                .get(&id)
                .and_then(|board| board.color.as_deref().and_then(parse_hex_color))
        })
        .unwrap_or_else(|| cx.theme().primary)
}

/// FEED-3: hover-reveal group name for the expanded rail's board rows — the
/// gear that jumps to the board's settings page shows only under the cursor.
const BOARD_ROW_GROUP: &str = "rail-board-row";

/// EXP-282: one row of the EXPANDED rail — icon + label, left-aligned, glass
/// row fills. Hand-rolled on purpose: gpui-component's `Button` centers its
/// inner layout with no `Styled` reach into it (the settings-nav rows set the
/// same precedent), and a centered label would defeat the whole point of the
/// labelled rail. Handlers are the caller's job.
/// A rail entry's status badge (EXP-509 — the Source Control entry outgrew
/// the plain dot): the classic colored dot (Reviews green, Support amber), a
/// small status ICON (SC attention triangle / error cross), or the spinning
/// refresh glyph while a sync is pulling.
#[derive(Clone)]
pub(crate) enum RailBadge {
    Dot(Hsla),
    Icon(ExpIcon, Hsla),
    Syncing,
}

/// One badge element at `glyph_px` (dots keep their fixed 6px regardless).
fn rail_badge_element(badge: RailBadge, glyph_px: f32, cx: &App) -> gpui::AnyElement {
    match badge {
        RailBadge::Dot(color) => div()
            .size_1p5()
            .flex_shrink_0()
            .rounded_full()
            .bg(color)
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
    accent: Hsla,
    badge: Option<RailBadge>,
    cx: &App,
) -> gpui::Stateful<gpui::Div> {
    // Active entries keep the collapsed rail's accent tint on the glyph; the
    // glass active fill carries the selection (no 2px marker bar — the row
    // fill IS the marker at this width).
    let icon = if active { icon.text_color(accent) } else { icon };
    h_flex()
        .id(id)
        .w_full()
        .h(px(28.))
        .px_1p5()
        .gap_2()
        .items_center()
        .flex_shrink_0()
        .rounded(cx.theme().radius)
        .cursor_pointer()
        .text_sm()
        .when(active, |this| {
            this.bg(theme::tokens::glass::FILL_ACTIVE.to_hsla())
        })
        .hover(|this| this.bg(theme::tokens::glass::FILL_ROW.to_hsla()))
        .child(icon.xsmall().flex_shrink_0())
        .child(div().flex_1().min_w_0().truncate().child(label.into()))
        .when_some(badge, |this, badge| {
            this.child(rail_badge_element(badge, 12., cx))
        })
}

// ---------------------------------------------------------------------------
// RailView — the icon strip left of the dock area
// ---------------------------------------------------------------------------

/// The 44px tool-window rail. Owned and rendered by the `Shell` shell
/// OUTSIDE the `DockArea`, below the full-width top bar. (No terminal
/// toggle — the bottom terminal strip is the single toggle affordance.)
pub struct RailView {
    nav: Entity<Navigation>,
    shared: Entity<RailShared>,
    /// The branch as of the last render — a checkout refreshes the file tree.
    last_branch: Option<String>,
    /// Scroll position of the rail's middle zone (tools + board icons) —
    /// small windows with many boards must not push Settings/Account off.
    rail_scroll: ScrollHandle,
    /// EXP-285: the rail spans the titlebar strip now — its top 34px are a
    /// window-drag region (the vendored `TitleBar` `should_move` pattern).
    should_move: bool,
    _subscriptions: Vec<Subscription>,
}

impl RailView {
    pub fn new(window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        let nav = nav_for_window(window, cx);
        let shared = rail_shared_for_window(window, cx);
        let git_bar = shared.read(cx).git_bar.clone();
        let collections = Store::global(cx).collections().clone();
        let avatar_cache = crate::user_avatar::AvatarCache::global(cx);
        let getting_started = crate::getting_started::GettingStartedProgress::global(cx);
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
            // The Support dot is a live read over unread support_reply rows.
            cx.observe(&collections.notifications, |_, _, cx| cx.notify()),
            // EXP-311: the account button's avatar rides the users shape
            // (profile image URL) plus the async avatar-byte cache.
            cx.observe(&collections.users, |_, _, cx| cx.notify()),
            cx.observe(&avatar_cache, |_, _, cx| cx.notify()),
            // EXP-548: the Getting-started entry hides once every checklist
            // entry is done — the shared progress entity re-notifies on the
            // synced collections it reads AND on its tRPC one-shots.
            cx.observe(&getting_started, |_, _, cx| cx.notify()),
        ];
        Self {
            nav,
            shared,
            last_branch: None,
            rail_scroll: ScrollHandle::new(),
            should_move: false,
            _subscriptions: subscriptions,
        }
    }

    /// One tool-window icon: a ghost icon button, `selected` + tinted with
    /// the board accent while its tool window is active; `badge` paints an
    /// attention dot in the given color (EXP-214: review green for open PRs,
    /// amber for support/conflicts), `None` for no dot. EXP-282: on the
    /// EXPANDED rail the same entry renders as a labelled row instead.
    #[allow(clippy::too_many_arguments)]
    fn rail_tool_icon(
        &self,
        id: &'static str,
        icon: Icon,
        tool: ToolWindow,
        label: &'static str,
        tooltip: impl Into<SharedString>,
        badge: Option<RailBadge>,
        accent: Hsla,
        expanded: bool,
        cx: &mut gpui::Context<Self>,
    ) -> gpui::AnyElement {
        // EXP-480: while the Actions (or Getting-started, EXP-470) full-page
        // mode is up the tool column is unmounted, so no tool entry may read
        // as selected — exactly one rail entry highlights, like a tool
        // switch.
        let active = self.shared.read(cx).tool == tool
            && !matches!(
                resolved_screen(&self.nav, cx),
                Some(Screen::Actions) | Some(Screen::GettingStarted)
            );
        if expanded {
            return rail_row(id, icon, label, active, accent, badge, cx)
                .on_click(cx.listener(move |_, _: &ClickEvent, window, cx| {
                    activate_tool(window, cx, tool);
                }))
                .into_any_element();
        }
        let icon = if active { icon.text_color(accent) } else { icon };
        div()
            .relative()
            .child(
                Button::new(id)
                    .ghost().cursor_pointer()
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
            .when_some(badge, |this, badge| {
                this.child(
                    div()
                        .absolute()
                        .top_0()
                        .right_0()
                        .child(rail_badge_element(badge, 10., cx)),
                )
            })
            .into_any_element()
    }

    /// The Actions entry (EXP-467): not a tool window anymore — it navigates
    /// to the [`Screen::Actions`] center page (the web agents page), the
    /// settings gear's direct-navigation shape in the tool-icon slot.
    /// EXP-480: the page is a tab-less full-page mode (no tool column, no
    /// tab chip), so while it is up this entry is the ONE highlighted row.
    fn rail_actions_entry(
        &self,
        accent: Hsla,
        expanded: bool,
        cx: &mut gpui::Context<Self>,
    ) -> gpui::AnyElement {
        let active = matches!(
            resolved_screen(&self.nav, cx),
            Some(Screen::Actions)
        );
        let icon = Icon::from(icons::registry::NAV_AGENTS);
        if expanded {
            return rail_row("rail-actions", icon, "Actions", active, accent, None, cx)
                .on_click(cx.listener(|_, _: &ClickEvent, window, cx| {
                    navigate(window, cx, Screen::Actions);
                }))
                .into_any_element();
        }
        let icon = if active { icon.text_color(accent) } else { icon };
        div()
            .relative()
            .child(
                Button::new("rail-actions")
                    .ghost().cursor_pointer()
                    .small()
                    .icon(icon)
                    .selected(active)
                    .tooltip("Actions")
                    .on_click(cx.listener(|_, _: &ClickEvent, window, cx| {
                        navigate(window, cx, Screen::Actions);
                    })),
            )
            .when(active, |this| {
                // The tool icons' JetBrains-style selection marker — this
                // entry sits among them, so it carries the same bar.
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
            .into_any_element()
    }

    /// The Getting-started entry (EXP-470): the desktop mirror of the web
    /// sidebar's re-entry point — navigates to the tab-less
    /// [`Screen::GettingStarted`] page, the `rail_actions_entry` shape.
    /// EXP-548: it sits at the BOTTOM of the rail, right above the
    /// Settings/Account row (the web sidebar-footer position), and is
    /// rendered only while the checklist is incomplete — no dismissal.
    fn rail_getting_started_entry(
        &self,
        accent: Hsla,
        expanded: bool,
        cx: &mut gpui::Context<Self>,
    ) -> gpui::AnyElement {
        let active = matches!(
            resolved_screen(&self.nav, cx),
            Some(Screen::GettingStarted)
        );
        let icon = Icon::from(icons::registry::NAV_GETTING_STARTED);
        if expanded {
            return rail_row(
                "rail-getting-started",
                icon,
                "Getting started",
                active,
                accent,
                None,
                cx,
            )
            .on_click(cx.listener(|_, _: &ClickEvent, window, cx| {
                navigate(window, cx, Screen::GettingStarted);
            }))
            .into_any_element();
        }
        let icon = if active { icon.text_color(accent) } else { icon };
        div()
            .relative()
            .child(
                Button::new("rail-getting-started")
                    .ghost().cursor_pointer()
                    .small()
                    .icon(icon)
                    .selected(active)
                    .tooltip("Getting started")
                    .on_click(cx.listener(|_, _: &ClickEvent, window, cx| {
                        navigate(window, cx, Screen::GettingStarted);
                    })),
            )
            .when(active, |this| {
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
            .into_any_element()
    }

    /// The account button — ALWAYS the rail's very bottom element. Its
    /// dropdown holds the account-level actions plus team switching
    /// (EXP-253: the top bar's merged board picker is gone — the rail shows
    /// only the ACTIVE team's boards, so other teams are reached here; a
    /// board-less team stays reachable too).
    fn render_account_button(
        &self,
        expanded: bool,
        cx: &mut gpui::Context<Self>,
    ) -> impl IntoElement {
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
            let signed_in = account.is_some();
            move |size: gpui_component::Size, image: Option<std::sync::Arc<gpui::Image>>| {
                // Signed out keeps the generic person placeholder instead of
                // "NS" initials for "Not signed in".
                let avatar = gpui_component::avatar::Avatar::new().with_size(size);
                let avatar = if signed_in {
                    avatar.name(full_name.clone())
                } else {
                    avatar
                };
                match image {
                    Some(image) => avatar.src(image),
                    None => avatar,
                }
            }
        };

        // Captured snapshot for the lazy menu builder (overlay renders must
        // not read `self`): every team, checked on the active one.
        let active_team = active_team_id(&self.nav, cx);
        let teams: Vec<(String, String, bool)> = Store::global(cx)
            .collections()
            .teams_sorted(cx)
            .into_iter()
            .map(|team| {
                let active = Some(team.id.as_str()) == active_team.as_deref();
                (team.id, team.name, active)
            })
            .collect();

        Button::new("rail-account")
            .ghost().cursor_pointer()
            .small()
            // EXP-282: expanded, the trigger becomes a full-width row — the
            // Button's own inner layout is centered and unreachable, so the
            // row is a `w_full` child that left-aligns inside it.
            .map(|button| {
                if expanded {
                    button
                        .w_full()
                        .h(px(36.))
                        .px_1p5()
                        .child(
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
                                ),
                        )
                } else {
                    button
                        .child(make_avatar(
                            gpui_component::Size::XSmall,
                            avatar_image.clone(),
                        ))
                        .tooltip(full_name.clone())
                }
            })
            .dropdown_menu_with_anchor(gpui::Anchor::BottomLeft, move |menu, _window, _cx| {
                // EXP-282: no "Settings" item — the rail's gear is the single
                // settings entry. EXP-288: no "Account" item either — Account
                // lives only in the settings nav's Personal group; this menu
                // is team switching + session actions. EXP-311: no identity
                // header either (web parity — the trigger already names the
                // person; email lives in settings → Account).
                let mut menu = menu;
                // "Switch team" section — flat checked rows (the menu builder
                // has no submenus); always shown, even with a single team
                // (EXP-434: no teams=1 special case anywhere).
                if !teams.is_empty() {
                    menu = menu.label("Switch team");
                    for (id, name, active) in &teams {
                        menu = menu.menu_with_check(
                            SharedString::from(name.clone()),
                            *active,
                            Box::new(SwitchTeam {
                                team_id: id.clone(),
                            }),
                        );
                    }
                    menu = menu.separator();
                }
                menu
                    .menu_with_icon("New team", registry::UI_ADD, Box::new(CreateTeam))
                    .menu_with_icon("Join team", registry::UI_INVITE, Box::new(JoinTeam))
                    .separator()
                    .menu("Sign out", Box::new(SignOut))
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
        expanded: bool,
        cx: &mut gpui::Context<Self>,
    ) -> gpui::AnyElement {
        let shared = self.shared.read(cx);
        let active_board = active_board_id(&self.nav, cx);
        // Same EXP-480 suppression as `rail_tool_icon`: a full-page mode
        // (Actions / Getting-started) owns the highlight while it is up.
        let active = shared.tool == ToolWindow::BoardIssues
            && active_board.as_deref() == Some(board.id.as_str())
            && !matches!(
                resolved_screen(&self.nav, cx),
                Some(Screen::Actions) | Some(Screen::GettingStarted)
            );
        let tint = board
            .color
            .as_deref()
            .and_then(parse_hex_color)
            .unwrap_or_else(|| cx.theme().muted_foreground);
        let icon = crate::icons::board_icon(board).text_color(tint);
        let board_id = board.id.clone();
        if expanded {
            // FEED-3: hover gear → this board's settings page. Owner-gated
            // like the settings nav's Boards group, so the selection never
            // clamps away underneath a non-owner.
            let owner = active_team_id(&self.nav, cx)
                .map(|team_id| crate::settings::is_owner(cx, &team_id))
                .unwrap_or(false);
            let settings_board_id = board.id.clone();
            // EXP-282: the board's own color stays the row's accent (the
            // glyph is always tinted — `active` only adds the row fill).
            return rail_row(
                ("rail-board", index),
                icon,
                SharedString::from(board.name.clone()),
                active,
                tint,
                None,
                cx,
            )
            .group(BOARD_ROW_GROUP)
            .on_click(cx.listener(move |_, _: &ClickEvent, window, cx| {
                crate::navigation::set_active_board(window, cx, board_id.clone());
                activate_tool(window, cx, ToolWindow::BoardIssues);
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
            .into_any_element();
        }
        div()
            .relative()
            .child(
                Button::new(("rail-board", index))
                    .ghost().cursor_pointer()
                    .small()
                    .icon(icon)
                    .selected(active)
                    .tooltip(SharedString::from(board.name.clone()))
                    .on_click(cx.listener(move |_, _: &ClickEvent, window, cx| {
                        crate::navigation::set_active_board(window, cx, board_id.clone());
                        activate_tool(window, cx, ToolWindow::BoardIssues);
                    })),
            )
            .when(active, |this| {
                this.child(
                    div()
                        .absolute()
                        .left(px(-6.))
                        .top_0()
                        .bottom_0()
                        .w(px(2.))
                        .rounded_full()
                        .bg(tint),
                )
            })
            .into_any_element()
    }

    /// EXP-282: the divider spans the labelled rail's full width and stays a
    /// short centered tick on the icon rail.
    fn divider(&self, expanded: bool, cx: &mut gpui::Context<Self>) -> gpui::AnyElement {
        div()
            .map(|this| if expanded { this.w_full() } else { this.w_6() })
            .h(px(1.))
            .my_1()
            .flex_shrink_0()
            .bg(cx.theme().sidebar_border)
            .into_any_element()
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

        // EXP-282: the labelled-rail switch (persisted per install).
        let expanded = self.shared.read(cx).rail_expanded;
        let accent = board_accent(&self.nav, cx);
        // Reviews badge: any open issue-linked PR in the active team.
        let has_reviews = active_team_id(&self.nav, cx)
            .map(|id| !queries::review_issues(cx, &id).is_empty())
            .unwrap_or(false);
        // Support tool (EXP-180): rendered ONLY while the active team's
        // synced row carries helpdesk_enabled = true. The badge lights on
        // unread helpdesk activity in that team (EXP-182).
        let support_icon = helpdesk_enabled(&self.nav, cx).then(|| {
            let support_unread = active_team_id(&self.nav, cx)
                .map(|id| queries::support_unread(cx, &id))
                .unwrap_or(false);
            let support_badge = support_unread.then(|| RailBadge::Dot(cx.theme().warning));
            self.rail_tool_icon(
                "rail-support",
                Icon::from(icons::registry::NAV_SUPPORT),
                ToolWindow::Support,
                "Support",
                "Support",
                support_badge,
                accent,
                expanded,
                cx,
            )
        });
        // Getting-started entry (EXP-470/548): pinned to the rail's bottom
        // (below), rendered until every checklist entry is done.
        let getting_started_icon = crate::getting_started::getting_started_visible(&self.nav, cx)
            .then(|| self.rail_getting_started_entry(accent, expanded, cx));

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
            .map(|(index, board)| self.rail_board_icon(index, board, expanded, cx))
            .collect();
        // EXP-525: expanded, the section reads like the web sidebar — a
        // "Boards" group label with a trailing `+` — instead of a "New
        // board" row; collapsed keeps the `+` icon below the board icons.
        let boards_header: Option<gpui::AnyElement> =
            active_team.clone().filter(|_| expanded).map(|team_id| {
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
                            .child("Boards"),
                    )
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
        let new_board: Option<gpui::AnyElement> =
            active_team.clone().filter(|_| !expanded).map(|team_id| {
                Button::new("rail-new-board")
                    .ghost().cursor_pointer()
                    .small()
                    .icon(registry::UI_ADD)
                    .tooltip("New board")
                    .on_click(cx.listener(move |_, _: &ClickEvent, window, cx| {
                        crate::create_board_dialog::open(window, cx, team_id.clone());
                    }))
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

        // EXP-282: the expand/collapse toggle. EXP-326: it lives in the rail's
        // own titlebar strip in EVERY state but one — collapsed on windowed
        // macOS, where the native traffic lights bury the 44px strip and the
        // toggle moves into the Shell's tongue instead (`shell::traffic_tongue`).
        let toggle = rail_toggle_button("rail-toggle", expanded);

        // EXP-285: the rail spans the full window height — its top 34px sit
        // in the window-decoration band as a drag/zoom region (the vendored
        // `TitleBar` `should_move` pattern; on macOS the native traffic
        // lights float over the strip's left).
        let client_chrome = crate::app_title_bar::client_chrome(window);
        let macos_lights = crate::app_title_bar::macos_lights_in_strip(window);
        // EXP-326: the app brand moved out of the titlebar (which is all tab
        // strip now) into the rail — but only where it fits: the expanded rail
        // minus the lights. Collapsed there is no room at 44px, and windowed
        // macOS spends the expanded strip's left half on the light cluster.
        let brand = (expanded && !macos_lights).then(|| crate::app_title_bar::brand(cx));
        let top_strip = h_flex()
            .id("rail-titlebar-strip")
            .w_full()
            .h(gpui_component::TITLE_BAR_HEIGHT)
            .flex_shrink_0()
            .items_center()
            .when(client_chrome, |strip| {
                strip
                    .window_control_area(WindowControlArea::Drag)
                    .map(|strip| {
                        if cfg!(target_os = "macos") {
                            strip.on_double_click(|_, window, _| window.titlebar_double_click())
                        } else if cfg!(target_os = "linux") {
                            strip.on_double_click(|_, window, _| window.zoom_window())
                        } else {
                            strip
                        }
                    })
                    .on_mouse_down_out(cx.listener(|this, _, _, _| this.should_move = false))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, _, _, _| this.should_move = true),
                    )
                    .on_mouse_up(
                        MouseButton::Left,
                        cx.listener(|this, _, _, _| this.should_move = false),
                    )
                    .on_mouse_move(cx.listener(|this, _, window, _| {
                        if this.should_move {
                            this.should_move = false;
                            window.start_window_move();
                        }
                    }))
            })
            .map(|strip| {
                if expanded {
                    // Brand left, toggle right. `justify_between` only does
                    // that with BOTH children present — on its own the toggle
                    // would land at flex-start, so the brand-less case keeps
                    // its EXP-285 `justify_end`.
                    strip
                        .map(|strip| {
                            if brand.is_some() {
                                strip.justify_between()
                            } else {
                                strip.justify_end()
                            }
                        })
                        .children(brand)
                        .child(crate::app_title_bar::interactive(toggle))
                } else if macos_lights {
                    // EXP-326: the traffic lights own this strip — the toggle
                    // renders in the Shell's tongue, just right of them.
                    strip
                } else {
                    // Collapsed with the strip to itself (macOS fullscreen,
                    // Windows/Linux either way): center the toggle in the
                    // 44px rail rather than leaking it into the titlebar.
                    strip
                        .justify_center()
                        .child(crate::app_title_bar::interactive(toggle))
                }
            });

        // Search — opens the ⌘K sheet. Call the opener directly via
        // cx.listener (like the rail tool icons below) rather than
        // dispatching OpenSearch: a rail button that dispatches to the
        // App-global handler fires from inside the window's own update, and
        // the handler's re-entrant active-window lookup makes the click
        // silently no-op — the gear next to it was dead for exactly this
        // reason (EXP-17). The ⌘K keybinding still routes through the action.
        let search: gpui::AnyElement = if expanded {
            rail_row(
                "rail-search",
                Icon::new(registry::NAV_SEARCH),
                "Search",
                false,
                accent,
                None,
                cx,
            )
            .on_click(cx.listener(|_, _: &ClickEvent, window, cx| {
                crate::search_sheet::open_search(window, cx)
            }))
            .into_any_element()
        } else {
            Button::new("rail-search")
                .ghost().cursor_pointer()
                .small()
                .icon(registry::NAV_SEARCH)
                .tooltip("Search")
                .on_click(cx.listener(|_, _: &ClickEvent, window, cx| {
                    crate::search_sheet::open_search(window, cx)
                }))
                .into_any_element()
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

        v_flex()
            .w(px(if expanded { RAIL_EXPANDED_W } else { RAIL_W }))
            .flex_shrink_0()
            .h_full()
            // EXP-285: top padding comes from the 34px titlebar strip — the
            // rail spans the full window height, flush at y=0.
            .pb_2()
            .gap_1()
            // EXP-285: the rail is the ONE lighter column of the app (Cursor
            // look) — section wash in BOTH states, full height, while every
            // other pane sits bare on the page gradient. EXP-293: it is also
            // the app's GLASS column — the wash is the only thing it paints, so
            // the Shell root's sidebar-alpha ramp (`theme::glass_sidebar_alpha`)
            // stays exposed here while the content column right of it tops up to
            // near-solid.
            .bg(theme::tokens::glass::FILL_SECTION.to_hsla())
            // EXP-269 corners: the wash runs flush into the window's LEFT
            // edge, top to bottom, so it must round its two left corners with
            // the frame (see `window_frame::frame_radii` — gpui's content
            // mask is rectangular and cannot clip it). The right corners stay
            // square: they sit in the middle of the window.
            .rounded_tl(crate::window_frame::frame_radii(window).top_left)
            .rounded_bl(crate::window_frame::frame_radii(window).bottom_left)
            .map(|this| {
                if expanded {
                    this.px_2()
                } else {
                    this.items_center()
                }
            })
            .text_color(cx.theme().sidebar_foreground)
            .child(top_strip)
            .child(search)
            .child(self.divider(expanded, cx))
            // Middle zone — scrollable so many boards never push the pinned
            // Settings/Account off small windows. Rail order (EXP-253):
            // [Inbox, Reviews, Support, Actions] / boards + "+" /
            // [Files, Source Control].
            .child(crate::scroll_pane::v_scroll_pane(
                "rail-scroll",
                &self.rail_scroll,
                v_flex()
                    .w_full()
                    .when(!expanded, |this| this.items_center())
                    .gap_1()
                    .child(self.rail_tool_icon(
                        "rail-inbox",
                        Icon::new(registry::NAV_INBOX),
                        ToolWindow::Inbox,
                        "Inbox",
                        "Inbox",
                        None,
                        accent,
                        expanded,
                        cx,
                    ))
                    .child(self.rail_tool_icon(
                        "rail-reviews",
                        Icon::from(ExpIcon::GitPullRequest),
                        ToolWindow::Reviews,
                        "Reviews",
                        "Reviews",
                        // Review green (EXP-214): open PRs are "stuff to do",
                        // colored like the in_review issue status.
                        has_reviews.then(|| RailBadge::Dot(theme::tokens::GREEN.to_hsla())),
                        accent,
                        expanded,
                        cx,
                    ))
                    // EXP-525: Actions above Support (web sidebar order is
                    // Inbox · Reviews · Agents · Support).
                    .child(self.rail_actions_entry(accent, expanded, cx))
                    .children(support_icon)
                    .child(self.divider(expanded, cx))
                    .children(boards_header)
                    .children(board_icons)
                    .children(new_board)
                    .child(self.divider(expanded, cx))
                    // Repo tool windows.
                    .child(self.rail_tool_icon(
                        "rail-files",
                        Icon::new(registry::NAV_FILES),
                        ToolWindow::Files,
                        "Files",
                        "Files",
                        None,
                        accent,
                        expanded,
                        cx,
                    ))
                    .child(self.rail_tool_icon(
                        "rail-source-control",
                        Icon::from(ExpIcon::GitMerge),
                        ToolWindow::SourceControl,
                        "Source Control",
                        sc_tooltip,
                        sc_badge,
                        accent,
                        expanded,
                        cx,
                    )),
            ))
            // EXP-548: the Getting-started entry lives down here with
            // Settings/Account, exactly where the web sidebar footer keeps
            // it — above the account row, outside the scrolling middle zone.
            .children(getting_started_icon)
            .map(|this| {
                if expanded {
                    // EXP-340: one bottom row — the account button fills the
                    // width, the gear rides its right edge.
                    this.child(
                        h_flex()
                            .w_full()
                            .gap_1()
                            .items_center()
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .child(self.render_account_button(true, cx)),
                            )
                            .child(settings_entry),
                    )
                } else {
                    this.child(settings_entry)
                        .child(self.render_account_button(false, cx))
                }
            })
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
    /// The Board Issues tool window — the full board (filter bar with
    /// All/Active/Backlog tabs + New Issue + the grouped virtualized list
    /// with inline status/priority menus), scoped to the active board.
    /// Lives in [`RailShared`] (EXP-48 — the detail switcher reads it too).
    board_active: Entity<BoardView>,
    /// The "My Issues" tool window — same board pinned to assignee == me
    /// (also shared via [`RailShared`]).
    board_my: Entity<BoardView>,
    /// The Source Control tool window's commit history (EXP-253 — it
    /// replaced the branch flow graph; master-only IDE).
    history: Entity<crate::source_control::HistoryList>,
    /// Fetched `repositories.openPulls` result: `(team_id, repos)` —
    /// open PRs with NO issue link (release PRs, manual branches, external
    /// contributors), listed straight from GitHub. Rendered below the board
    /// groups; a merged pull is removed locally (no Electric echo).
    open_pulls: Option<(String, Vec<api::repositories::OpenPullsRepo>)>,
    /// The team the current openPulls fetch belongs to. Cleared whenever
    /// the Reviews tool window is inactive, so re-opening refetches (the
    /// server caches ~60s; there is deliberately no polling).
    open_pulls_key: Option<String>,
    /// Bumped per fetch — a stale response checks it before landing.
    open_pulls_seq: u64,
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
    _subscriptions: Vec<Subscription>,
}

use crate::pr_merge::{close_pr_key, pull_merge_key, MergeOp, MergeState};

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
        _ => Icon::new(registry::NAV_NOTIFICATIONS),
    }
}

impl SidebarPanel {
    pub fn new(window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        let nav = nav_for_window(window, cx);
        let shared = rail_shared_for_window(window, cx);
        let git_bar = shared.read(cx).git_bar.clone();
        let board_active = shared.read(cx).board_active.clone();
        let board_my = shared.read(cx).board_my.clone();
        let history = cx.new(|cx| crate::source_control::HistoryList::new(window, cx));
        let collections = Store::global(cx).collections().clone();
        let local_sessions = coding_flow::LocalSessions::global(cx);
        let merge_state = MergeState::global(cx);
        let subscriptions = vec![
            // Rail toggles swap the tool window.
            cx.observe(&shared, |_, _, cx| cx.notify()),
            // Session phase — the shared state.
            cx.observe(&Store::global(cx).state(), |_, _, cx| cx.notify()),
            // Query scoping + inbox list are live collection reads.
            cx.observe(&collections.teams, |_, _, cx| cx.notify()),
            cx.observe(&collections.boards, |_, _, cx| cx.notify()),
            cx.observe(&collections.issues, |_, _, cx| cx.notify()),
            cx.observe(&collections.notifications, |_, _, cx| cx.notify()),
            // The coding badges ride the coding_sessions shape; the local
            // Start↔Stop flip rides the process-global LocalSessions registry.
            cx.observe(&collections.coding_sessions, |_, _, cx| cx.notify()),
            cx.observe(&local_sessions, |_, _, cx| cx.notify()),
            // EXP-325: the Reviews rows' merge arm/spinner/error live in the
            // shared app-global merge state (any surface can drive them).
            cx.observe(&merge_state, |_, _, cx| cx.notify()),
            // Sync state rides the shared trunk-sync engine.
            cx.observe(&git_bar, |_, _, cx| cx.notify()),
            // Active-row highlight follows navigation.
            cx.observe(&nav, |_, _, cx| cx.notify()),
        ];

        Self {
            nav,
            shared,
            board_active,
            board_my,
            open_pulls: None,
            open_pulls_key: None,
            open_pulls_seq: 0,
            support_filter: SupportFilter::Open,
            support_threads: None,
            support_key: None,
            support_seq: 0,
            support_poll_seq: 0,
            history,
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
        // EXP-277: no bottom hairline — typography + the strip's height
        // separate the header from the list (fewer chrome lines).
        h_flex()
            .flex_shrink_0()
            .w_full()
            .h(px(30.))
            .px_3()
            .gap_1p5()
            .items_center()
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

    /// EXP-282: the icon-tab strip that REPLACED the icon+title header on the
    /// two tabbed tool windows (Inbox, Support). EXP-525: chips sit LEFT
    /// (web parity — the inbox/support pills are left-aligned rows there);
    /// trailing controls ride the strip's right edge absolutely. Same height
    /// as [`Self::tool_header`] so the lists below don't shift between tools.
    fn tool_tab_strip(&self, tabs: Vec<gpui::AnyElement>) -> gpui::Div {
        h_flex()
            .relative()
            .flex_shrink_0()
            .w_full()
            .h(px(30.))
            .px_2()
            .items_center()
            .justify_start()
            .child(h_flex().gap_1().items_center().children(tabs))
    }

    /// One chip of [`Self::tool_tab_strip`] — the shared glass chip with a
    /// leading glyph (`surface::tab_chip` already carries the gap).
    fn tool_tab(
        &self,
        id: &'static str,
        icon: Icon,
        label: &'static str,
        selected: bool,
        cx: &mut gpui::Context<Self>,
    ) -> gpui::Stateful<gpui::Div> {
        crate::surface::tab_chip(selected, cx)
            .id(id)
            .child(icon.xsmall())
            .child(label)
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

    /// *Inbox* tool window (EXP-186): the merged personal surface — an Inbox
    /// tab (notification stream) + a My Issues tab (the full board pinned to
    /// assignee == me across the team), switched by header tab buttons (the
    /// Support Open/Resolved pattern), mirroring mobile's segmented My Work
    /// screen.
    fn render_inbox_tool(&mut self, cx: &mut gpui::Context<Self>) -> gpui::AnyElement {
        let tab = self.shared.read(cx).inbox_tab;
        // EXP-282: centered icon tabs instead of the icon+title header.
        let inbox_tab = self
            .tool_tab(
                "inbox-tab-inbox",
                Icon::new(registry::NAV_INBOX),
                "Inbox",
                tab == InboxTab::Inbox,
                cx,
            )
            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                this.set_inbox_tab(InboxTab::Inbox, cx);
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
            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                this.set_inbox_tab(InboxTab::MyIssues, cx);
            }))
            .into_any_element();
        let header = self.tool_tab_strip(vec![inbox_tab, mine_tab]);

        if tab == InboxTab::MyIssues {
            // EXP-525: the Filter trigger moved INTO the strip (web parity —
            // no dedicated filter row above the list).
            let trigger = self
                .board_my
                .update(cx, |board, cx| board.filter_trigger(cx));
            let header = header.child(
                div()
                    .absolute()
                    .right_2()
                    .top_0()
                    .bottom_0()
                    .flex()
                    .items_center()
                    .child(trigger),
            );
            return v_flex()
                .flex_1()
                .min_h_0()
                .min_w_0()
                .child(header)
                .child(self.my_issues_body(cx))
                .into_any_element();
        }

        let data = queries::inbox(cx);
        // "Mark all read" rides the strip's trailing edge absolutely so the
        // tab chips stay centered whether or not it is there (EXP-282).
        let header = header.when(data.total_unread > 0, |this| {
            this.child(
                div()
                    .absolute()
                    .right_2()
                    .top_0()
                    .bottom_0()
                    .flex()
                    .items_center()
                    .child(
                        Button::new("inbox-mark-all-read")
                            .ghost().cursor_pointer()
                            .xsmall()
                            .icon(Icon::from(registry::NOTIFICATION_MARK_READ))
                            .tooltip("Mark all read")
                            .on_click(cx.listener(|_, _: &ClickEvent, _, cx| {
                                if let Some(trpc) = queries::trpc_client(cx) {
                                    cx.background_executor()
                                        .spawn(async move {
                                            if let Err(err) =
                                                api::notifications::notifications_mark_all_read(
                                                    &trpc,
                                                )
                                            {
                                                log::warn!(
                                                    "[ui] notifications.markAllRead failed: {err}"
                                                );
                                            }
                                        })
                                        .detach();
                                }
                            })),
                    ),
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
            let rows: Vec<gpui::AnyElement> = data
                .groups
                .iter()
                .map(|entry| match entry {
                    queries::InboxEntry::Issue(group) => self.inbox_issue_row(group, cx),
                    queries::InboxEntry::Support(group) => self.inbox_support_row(group, cx),
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
            .on_click(cx.listener(move |_, _, window, cx| {
                // Web `markGroupRead`: clear the group's unreads
                // (the Electric echo removes the dot), then open.
                mark_group_read(&unread_ids, cx);
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

    /// Switch the Inbox tool window's active tab (EXP-186).
    fn set_inbox_tab(&mut self, tab: InboxTab, cx: &mut gpui::Context<Self>) {
        self.shared.update(cx, |shared, cx| {
            if shared.inbox_tab != tab {
                shared.inbox_tab = tab;
                cx.notify();
            }
        });
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
    fn render_board_issues_tool(&mut self, cx: &mut gpui::Context<Self>) -> gpui::AnyElement {
        let query = match active_board_id(&self.nav, cx) {
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

    // -- Reviews tool window ----------------------------------------------------

    /// *Reviews* tool window: open pull requests across the team, each
    /// mergeable row with a two-click inline merge confirm. Issue-linked PRs
    /// come from the synced issues shape, grouped by board; below them, PRs
    /// NOT linked to anything (manual branches, external contributors) come
    /// from a background `repositories.openPulls` fetch, grouped by repo —
    /// the synced lists never wait on GitHub. Merging goes through the server
    /// (`issues.mergePr` / `repositories.mergePull`, GitHub App squash) —
    /// never local git; synced rows leave the list via the Electric echo,
    /// unlinked pulls are removed locally.
    fn render_reviews_tool(&mut self, cx: &mut gpui::Context<Self>) -> gpui::AnyElement {
        let collections = Store::global(cx).collections().clone();
        let is_ready = collections.issues.read(cx).is_ready()
            && collections.boards.read(cx).is_ready();
        let team_id = active_team_id(&self.nav, cx);
        if let Some(id) = team_id.as_deref() {
            self.ensure_open_pulls(id, cx);
        }
        let groups = team_id
            .as_deref()
            .map(|id| queries::review_groups(cx, id))
            .unwrap_or_default();
        let pull_repos: Vec<api::repositories::OpenPullsRepo> = self
            .open_pulls
            .as_ref()
            .filter(|(ws, _)| Some(ws.as_str()) == team_id.as_deref())
            .map(|(_, repos)| queries::visible_pull_repos(repos))
            .unwrap_or_default();

        // Unlinked pulls have no Electric echo — a pull merged elsewhere
        // drops its transient merge state here against the fetched list.
        // (Issue rows are echo-settled by the shared state's own issues
        // observer, EXP-325.)
        {
            let live_keys: HashSet<String> = pull_repos
                .iter()
                .flat_map(|repo| {
                    repo.pulls
                        .iter()
                        .map(|pull| pull_merge_key(&repo.repository_id, pull.number))
                })
                .collect();
            MergeState::global(cx)
                .update(cx, |state, cx| state.retain_pull_keys(&live_keys, cx));
        }

        let header = self.tool_header(Icon::from(ExpIcon::GitPullRequest), "Reviews", cx);

        let body: gpui::AnyElement = if !is_ready {
            self.list_skeleton(cx)
        } else if groups.is_empty() && pull_repos.is_empty() {
            // EXP-525: the web `EmptyState` (icon disc + title + description).
            crate::controls::empty_state(
                Icon::from(ExpIcon::GitPullRequest),
                "No open pull requests",
                "Open pull requests in this team's repositories land here for review.",
                cx,
            )
            .into_any_element()
        } else {
            let muted = cx.theme().muted_foreground;
            let mut children: Vec<gpui::AnyElement> = Vec::new();
            for group in &groups {
                let dot = group
                    .board
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
                                .child(SharedString::from(group.board.name.clone())),
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
    /// every linked issue. Clicking the row opens the PR diff screen
    /// (EXP-181; its header links to the issue detail). The subtle ghost `×` left of
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
        let danger = theme.danger;
        // EXP-277: rows use the glass list fills (EXP-269 list_* tokens).
        let row_hover = theme.list_hover;
        let row_active = theme.list_active;
        // Open-PR green (the token the status/priority accents use).
        let pr_green = theme::tokens::GREEN.to_hsla();

        let selected = matches!(
            resolved_screen(&self.nav, cx),
            Some(Screen::PrDiff { issue_id }) if issue_id == issue.id
        );
        // EXP-325: the two-click arm/spinner/error live in the shared
        // app-global merge state — a merge driven from the issue-detail
        // sidebar or a terminal tab renders here identically.
        let close_key = close_pr_key(&issue.id);
        let (merging, armed, closing, close_armed, error, failed_op) = {
            let state = MergeState::global(cx);
            let state = state.read(cx);
            (
                state.merging(&issue.id),
                state.armed(&issue.id),
                state.merging(&close_key),
                state.armed(&close_key),
                state.error(&issue.id),
                state.failed_op(&issue.id),
            )
        };
        // EXP-259: a failed merge (typically "not mergeable" — conflicts)
        // offers the builtin "Fix merge conflicts" action run right on the
        // row. MERGE failures only — the run ends in a merge, the opposite of
        // what a failed close was asked to do (merge and close share this
        // row's caption). Needs the PR's recorded branch (the run rebases
        // it); "Fixing…" parks the button only while an ACTUAL fix run works
        // the branch — any other session still holding it is ended by the
        // fix-run launch itself.
        let fixing = issue.branch.as_deref().is_some_and(|branch| {
            crate::coding_flow::LocalSessions::global_ref(cx)
                .is_some_and(|sessions| sessions.read(cx).is_branch_fixing(branch))
        });
        let fix_button = error
            .as_ref()
            .filter(|_| failed_op == Some(crate::pr_merge::FailedOp::Merge))
            .filter(|_| issue.branch.is_some())
            .map(|_| {
                let mut button =
                    Button::new(SharedString::from(format!("review-fix-{}", issue.id)))
                        .xsmall()
                        .outline().cursor_pointer();
                if fixing {
                    button = button.label("Fixing…").disabled(true);
                } else if let Some(reason) = crate::coding_flow::no_agent_reason(cx) {
                    // EXP-367: no agent CLI → disabled with the reason.
                    button = button.label("Fix conflicts").tooltip(reason).disabled(true);
                } else {
                    button = button.label("Fix conflicts");
                }
                let click_id = issue.id.clone();
                button.on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                    cx.stop_propagation();
                    this.on_fix_conflicts_click(click_id.clone(), window, cx);
                }))
            });

        let sub: String = match (issue.pr_number, issue.branch.as_deref()) {
            (Some(number), Some(branch)) => format!("#{number} \u{00B7} {branch}"),
            (Some(number), None) => format!("#{number}"),
            (None, Some(branch)) => branch.to_string(),
            (None, None) => String::new(),
        };

        let merge_button = {
            let mut button = Button::new(SharedString::from(format!("review-merge-{}", issue.id)))
                .xsmall()
                .outline().cursor_pointer();
            if merging {
                button = button.label("Merging…").loading(true).disabled(true);
            } else if armed {
                button = button.label("Confirm merge").danger().cursor_pointer();
            } else {
                button = button.label("Merge").disabled(closing);
            }
            let click_id = issue.id.clone();
            button.on_click(cx.listener(move |_, _: &ClickEvent, _, cx| {
                cx.stop_propagation();
                crate::pr_merge::two_click(
                    MergeOp::MergeIssuePr {
                        issue_id: click_id.clone(),
                    },
                    None,
                    None,
                    cx,
                );
            }))
        };

        // The reject path — intentionally quiet next to Merge: a muted ghost
        // `×` that only grows into a labeled danger confirm once armed.
        let close_button = {
            let mut button = Button::new(SharedString::from(format!("review-close-{}", issue.id)))
                .xsmall()
                .ghost().cursor_pointer();
            if closing {
                button = button
                    .icon(Icon::new(registry::UI_CLOSE))
                    .loading(true)
                    .disabled(true);
            } else if close_armed {
                button = button.label("Close PR").danger().cursor_pointer();
            } else {
                button = button
                    .icon(Icon::new(registry::UI_CLOSE).text_color(muted))
                    .tooltip("Close PR without merging")
                    .disabled(merging);
            }
            let click_id = issue.id.clone();
            button.on_click(cx.listener(move |_, _: &ClickEvent, _, cx| {
                cx.stop_propagation();
                crate::pr_merge::two_click(
                    MergeOp::CloseIssuePr {
                        issue_id: click_id.clone(),
                    },
                    None,
                    None,
                    cx,
                );
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
            .when(selected, |this| this.bg(row_active))
            .hover(|this| this.bg(row_hover))
            .cursor_pointer()
            .on_click(cx.listener(move |_, _, window, cx| {
                // Any click outside the armed button disarms the confirm.
                MergeState::disarm(cx);
                // The PR diff (EXP-181): a review click is about the CODE —
                // the diff screen renders it, and its header links back to
                // the issue detail for the body.
                crate::navigation::navigate(
                    window,
                    cx,
                    crate::navigation::Screen::PrDiff {
                        issue_id: nav_id.clone(),
                    },
                );
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
                    h_flex()
                        .pl_5()
                        .gap_2()
                        .items_center()
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .text_xs()
                                .truncate()
                                .text_color(danger)
                                .child(SharedString::from(message)),
                        )
                        .children(fix_button),
                )
            })
            .into_any_element()
    }

    /// The review row's "Fix conflicts" button (EXP-259): open the Start
    /// coding dialog with the builtin "Fix merge conflicts" action and this
    /// PR preselected (EXP-313 — agent/model/effort stay choosable; the run
    /// only starts when the dialog confirms). The `review_error` caption
    /// stays — the PR really does still have conflicts until a fix lands.
    fn on_fix_conflicts_click(
        &mut self,
        issue_id: String,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(team_id) = active_team_id(&self.nav, cx) else {
            return;
        };
        crate::start_coding_dialog::open_for_fix_conflicts(window, cx, team_id, issue_id);
    }

    /// Kick the `repositories.openPulls` fetch when the Reviews tool window
    /// is shown or the team changes — never on a timer (the server
    /// caches ~60s). Data from another team is dropped immediately; a
    /// reopen in the same team keeps rendering the previous result while
    /// the refresh is in flight.
    fn ensure_open_pulls(&mut self, team_id: &str, cx: &mut gpui::Context<Self>) {
        if self.open_pulls_key.as_deref() == Some(team_id) {
            return;
        }
        self.open_pulls_key = Some(team_id.to_string());
        if self
            .open_pulls
            .as_ref()
            .is_some_and(|(ws, _)| ws != team_id)
        {
            self.open_pulls = None;
        }
        self.open_pulls_seq += 1;
        let seq = self.open_pulls_seq;
        let Some(trpc) = queries::trpc_client(cx) else {
            return;
        };
        let ws = team_id.to_string();
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
        let danger = theme.danger;
        // EXP-277: rows use the glass list fills (EXP-269 list_* tokens).
        let row_hover = theme.list_hover;
        let pr_green = theme::tokens::GREEN.to_hsla();

        let key = pull_merge_key(repository_id, pull.number);
        let (merging, armed, error) = {
            let state = MergeState::global(cx);
            let state = state.read(cx);
            (state.merging(&key), state.armed(&key), state.error(&key))
        };

        let sub = format!("{} \u{2192} {}", pull.branch, pull.base_branch);

        let merge_button = {
            let mut button = Button::new(SharedString::from(format!("pull-merge-{key}")))
                .xsmall()
                .outline().cursor_pointer();
            if merging {
                button = button.label("Merging…").loading(true).disabled(true);
            } else if pull.draft {
                button = button.label("Merge").disabled(true);
            } else if armed {
                button = button.label("Confirm merge").danger().cursor_pointer();
            } else {
                button = button.label("Merge");
            }
            let click_repo = repository_id.to_string();
            let number = pull.number;
            button.on_click(cx.listener(move |_, _: &ClickEvent, _, cx| {
                cx.stop_propagation();
                // There is no Electric echo for unlinked pulls — success
                // drops the row from this panel's fetched state.
                let panel = cx.entity().downgrade();
                let success_repo = click_repo.clone();
                crate::pr_merge::two_click(
                    MergeOp::MergePull {
                        repository_id: click_repo.clone(),
                        number,
                    },
                    None,
                    Some(Box::new(move |cx: &mut gpui::App| {
                        let _ = panel.update(cx, |this: &mut Self, cx| {
                            if let Some((_, repos)) = this.open_pulls.as_mut() {
                                queries::remove_merged_pull(repos, &success_repo, number);
                            }
                            cx.notify();
                        });
                    })),
                    cx,
                );
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
            .hover(|this| this.bg(row_hover))
            .cursor_pointer()
            .on_click(cx.listener(move |_, _, _, cx| {
                // Any click outside the armed button disarms the confirm.
                MergeState::disarm(cx);
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

    // -- Support tool window ----------------------------------------------------

    /// *Support* tool window (EXP-180): the active team's support tickets,
    /// filtered open/resolved. Threads are server-only tRPC data — a
    /// seq-guarded background fetch keyed on `(team_id, filter)` (the
    /// `ensure_open_pulls` pattern) plus a 30s poll that lives only while
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
        let header = self.tool_tab_strip(vec![open_tab, resolved_tab]);

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
                        .child(v_flex().p_1().gap_0p5().children(rows))
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

    /// One Support row: title, reporter + relative time, an unread dot while
    /// the reporter spoke last. Click opens the thread screen.
    fn support_row(
        &self,
        thread: &api::helpdesk::SupportThreadSummary,
        cx: &mut gpui::Context<Self>,
    ) -> gpui::AnyElement {
        let theme = cx.theme();
        let radius = theme.radius;
        let fg = theme.foreground;
        let muted = theme.muted_foreground;
        // EXP-277: rows use the glass list fills (EXP-269 list_* tokens).
        let row_hover = theme.list_hover;
        let row_active = theme.list_active;
        // The unread dot is the white primary (web `bg-primary`).
        let unread_dot = theme::tokens::PRIMARY.to_hsla();

        let selected = matches!(
            resolved_screen(&self.nav, cx),
            Some(Screen::SupportThread { thread_id }) if thread_id == thread.id
        );
        let unread = thread.unread;
        let reporter: SharedString = thread
            .reporter_name
            .clone()
            .filter(|name| !name.trim().is_empty())
            .or_else(|| thread.reporter_email.clone())
            .unwrap_or_else(|| "Reporter".to_string())
            .into();
        let time: SharedString = thread
            .updated_at
            .as_deref()
            .map(crate::inbox::relative_time)
            .unwrap_or_default()
            .into();
        // One-line latest-PUBLIC-message preview (web/iOS/Android row
        // parity); newlines collapse so `truncate` sees a single line.
        // Blank bodies fall back to the thread subject.
        let preview: SharedString = thread
            .last_message
            .as_ref()
            .and_then(|message| message.body.as_deref())
            .map(|body| body.split_whitespace().collect::<Vec<_>>().join(" "))
            .filter(|body| !body.is_empty())
            .unwrap_or_else(|| thread.title.clone())
            .into();
        let nav_id = thread.id.clone();
        let nav_title = thread.title.clone();

        // EXP-525: the web list row — reporter name + time + unread dot on
        // line one, preview under (`support-inbox.tsx`).
        v_flex()
            .id(SharedString::from(format!("support-{}", thread.id)))
            .w_full()
            .px_2()
            .py_1p5()
            .gap_0p5()
            .rounded(radius)
            .when(selected, |this| this.bg(row_active))
            .hover(|this| this.bg(row_hover))
            .cursor_pointer()
            .on_click(cx.listener(move |_, _, window, cx| {
                // Seed the tab label — thread titles are tRPC-only.
                crate::support_thread::remember_title(cx, &nav_id, &nav_title);
                navigate(
                    window,
                    cx,
                    Screen::SupportThread {
                        thread_id: nav_id.clone(),
                    },
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
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_sm()
                            .truncate()
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(fg)
                            .child(reporter),
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
                self.tool_header(Icon::new(registry::NAV_FILES), "Files", cx).child(
                    Button::new("files-refresh")
                        .ghost().cursor_pointer()
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

    /// *Source Control* tool window (EXP-253 master-only): the trunk's
    /// commit history ([`crate::source_control::HistoryList`] — it replaced
    /// the branch flow graph). Clicking a commit shows its diff in the
    /// changes screen; there is no branch switching anymore.
    fn render_source_control_tool(&mut self, cx: &mut gpui::Context<Self>) -> gpui::AnyElement {
        let trunk_sync = self.shared.read(cx).trunk_sync().clone();
        let header = self
            .tool_header(Icon::from(ExpIcon::GitMerge), "Source Control", cx)
            .child(
                Button::new("history-refresh")
                    .ghost().cursor_pointer()
                    .xsmall()
                    .icon(Icon::from(ExpIcon::Repeat))
                    .tooltip("Check for updates")
                    .on_click(move |_, window, cx| {
                        trunk_sync.update(cx, |engine, cx| engine.refresh(window, cx));
                    }),
            );

        v_flex()
            .flex_1()
            .min_h_0()
            .min_w_0()
            .child(header)
            // The explicit sized wrapper is load-bearing for entity children
            // (same flex-child rule as the shell's dock wrapper); flex column
            // so the list's own flex_1 scroll pane resolves to this height.
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .w_full()
                    .flex()
                    .flex_col()
                    .child(self.history.clone()),
            )
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
        // Leaving the Support tool drops its fetch key — the next open
        // refetches, and the 30s poll loop dies on its next tick.
        if tool != ToolWindow::Support {
            self.support_key = None;
        }
        v_flex()
            .size_full()
            .min_w_0()
            .overflow_hidden()
            // EXP-285: no section wash — every pane sits on the ONE page
            // gradient; only the icon rail keeps a lighter tint. A hairline
            // marks the boundary to the center.
            .border_r_1()
            .border_color(theme::tokens::glass::STROKE_ROW.to_hsla())
            .text_color(cx.theme().sidebar_foreground)
            .child(match tool {
                ToolWindow::Inbox => self.render_inbox_tool(cx),
                ToolWindow::BoardIssues => self.render_board_issues_tool(cx),
                ToolWindow::Reviews => self.render_reviews_tool(cx),
                ToolWindow::Support => self.render_support_tool(cx),
                ToolWindow::Files => self.render_files_tool(cx),
                ToolWindow::SourceControl => self.render_source_control_tool(cx),
            })
            .into_any_element()
    }
}
