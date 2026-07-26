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
//!   an amber badge in conflict mode and opens the changes
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
    v_flex, ActiveTheme as _, Disableable as _, Icon, IconName, InteractiveElementExt as _,
    Selectable as _, Sizable as _,
};
use sync::Store;


// EXP-282: `OpenSettings` is gone from this file — the rail gear navigates
// directly (EXP-17) and the account dropdown no longer duplicates it.
use crate::actions::{CreateTeam, JoinTeam, SignOut, SwitchTeam};
use crate::board::BoardView;
use crate::coding_flow;
use crate::trunk_sync::TrunkSync;
use crate::icons::ExpIcon;
use crate::issue_list::IssueQuery;
use crate::navigation::{
    active_board_id, active_team_id, nav_for_window, navigate, resolved_screen, switch_team,
    Navigation, Screen,
};
use crate::properties_panel::parse_hex_color;
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
    /// The team's reusable action prompts (EXP-253 — server-only tRPC data,
    /// fetched on open; runs are trust-gated claude sessions).
    Actions,
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
    /// The commit the Source Control screen's diff pane shows — the sidebar
    /// history list selects it (EXP-253; `None` = nothing selected).
    sc_selected_commit: Option<String>,
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

    /// The sidebar history list's commit selection (EXP-253).
    pub(crate) fn sc_selected_commit(&self) -> Option<&str> {
        self.sc_selected_commit.as_deref()
    }

    /// Drop the commit selection (a Source Control scope change invalidated
    /// it).
    pub(crate) fn clear_sc_selected_commit(&mut self, cx: &mut gpui::Context<Self>) {
        if self.sc_selected_commit.is_some() {
            self.sc_selected_commit = None;
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

    /// The issue list whose ordering the detail's prev/next switcher follows
    /// (EXP-48): the My Issues board while the Inbox tool window shows its
    /// My Issues tab, the active board's list otherwise (it is the window's
    /// persistent issue list).
    pub(crate) fn active_issue_board(&self) -> &Entity<BoardView> {
        match (self.tool, self.inbox_tab) {
            (ToolWindow::Inbox, InboxTab::MyIssues) => &self.board_my,
            _ => &self.board_active,
        }
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
/// `AppTitleBar` (traffic-light padding compensation + the collapsed-state
/// expand toggle live in the main titlebar now).
pub(crate) fn rail_expanded(window: &mut Window, cx: &mut App) -> bool {
    rail_shared_for_window(window, cx).read(cx).rail_expanded
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

/// Point the Source Control screen's diff pane at `commit` (EXP-253 — the
/// sidebar history list's click target); `None` clears the selection.
pub(crate) fn select_sc_commit(window: &mut Window, cx: &mut App, commit: Option<String>) {
    let shared = rail_shared_for_window(window, cx);
    shared.update(cx, |shared, cx| {
        if shared.sc_selected_commit != commit {
            shared.sc_selected_commit = commit;
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
    // EXP-282: the persisted rail state. Read through the coding hub — it
    // owns `settings.json`. EXP-285: absent = EXPANDED (the labelled rail is
    // the default look now); an explicit user collapse still sticks.
    let rail_expanded = coding_flow::CodingHub::global(cx)
        .read(cx)
        .settings
        .rail_expanded
        .unwrap_or(true);
    let shared = cx.new(|_| RailShared {
        // Issues-first default: the active board's issue list.
        tool: ToolWindow::BoardIssues,
        inbox_tab: InboxTab::Inbox,
        git_bar,
        file_tree,
        board_active,
        board_my,
        sc_selected_commit: None,
        selected_file: None,
        rail_expanded,
        settings_section: crate::settings::SettingsSection::General,
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

/// EXP-282: one row of the EXPANDED rail — icon + label, left-aligned, glass
/// row fills. Hand-rolled on purpose: gpui-component's `Button` centers its
/// inner layout with no `Styled` reach into it (the settings-nav rows set the
/// same precedent), and a centered label would defeat the whole point of the
/// labelled rail. Handlers are the caller's job.
fn rail_row(
    id: impl Into<gpui::ElementId>,
    icon: Icon,
    label: impl Into<SharedString>,
    active: bool,
    accent: Hsla,
    badge: Option<Hsla>,
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
        .when_some(badge, |this, color| {
            this.child(
                div()
                    .size_1p5()
                    .flex_shrink_0()
                    .rounded_full()
                    .bg(color),
            )
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
        badge: Option<Hsla>,
        accent: Hsla,
        expanded: bool,
        cx: &mut gpui::Context<Self>,
    ) -> gpui::AnyElement {
        let active = self.shared.read(cx).tool == tool;
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
            .when_some(badge, |this, color| {
                this.child(
                    div()
                        .absolute()
                        .top_0()
                        .right_0()
                        .size_1p5()
                        .rounded_full()
                        .bg(color),
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
        let who: SharedString = account
            .as_ref()
            .map(|account| SharedString::from(account.email.clone()))
            .unwrap_or_else(|| "Not signed in".into());
        let label = who.clone();
        // EXP-282: the expanded rail names the signed-in person (name over
        // email, email alone when the account carries no name).
        let display_name: SharedString = account
            .as_ref()
            .and_then(|account| account.name.clone())
            .filter(|name| !name.trim().is_empty())
            .map(SharedString::from)
            .unwrap_or_else(|| who.clone());
        let sub_label = (display_name != who).then(|| who.clone());

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
            .ghost()
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
                                .child(Icon::new(IconName::CircleUser).xsmall().flex_shrink_0())
                                .child(
                                    v_flex()
                                        .flex_1()
                                        .min_w_0()
                                        .gap_0()
                                        .child(
                                            div()
                                                .text_sm()
                                                .truncate()
                                                .child(display_name.clone()),
                                        )
                                        .children(sub_label.clone().map(|email| {
                                            div()
                                                .text_xs()
                                                .text_color(cx.theme().muted_foreground)
                                                .truncate()
                                                .child(email)
                                        })),
                                ),
                        )
                } else {
                    button.icon(IconName::CircleUser).tooltip(who.clone())
                }
            })
            .dropdown_menu_with_anchor(gpui::Anchor::BottomLeft, move |menu, _window, _cx| {
                // EXP-282: no "Settings" item — the rail's gear is the single
                // settings entry. EXP-288: no "Account" item either — Account
                // lives only in the settings nav's Personal group; this menu
                // is team switching + session actions.
                let mut menu = menu.label(label.clone());
                // "Switch team" section — flat checked rows (the menu builder
                // has no submenus); shown only with somewhere to switch to.
                if teams.len() > 1 {
                    menu = menu.separator().label("Switch team");
                    for (id, name, active) in &teams {
                        menu = menu.menu_with_check(
                            SharedString::from(name.clone()),
                            *active,
                            Box::new(SwitchTeam {
                                team_id: id.clone(),
                            }),
                        );
                    }
                }
                menu.separator()
                    .menu_with_icon("New team", IconName::Plus, Box::new(CreateTeam))
                    .menu_with_icon("Join team", IconName::User, Box::new(JoinTeam))
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
        let active = shared.tool == ToolWindow::BoardIssues
            && active_board.as_deref() == Some(board.id.as_str());
        let tint = board
            .color
            .as_deref()
            .and_then(parse_hex_color)
            .unwrap_or_else(|| cx.theme().muted_foreground);
        let icon = crate::icons::board_icon(board).text_color(tint);
        let board_id = board.id.clone();
        if expanded {
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
            .on_click(cx.listener(move |_, _: &ClickEvent, window, cx| {
                crate::navigation::set_active_board(window, cx, board_id.clone());
                activate_tool(window, cx, ToolWindow::BoardIssues);
            }))
            .into_any_element();
        }
        div()
            .relative()
            .child(
                Button::new(("rail-board", index))
                    .ghost()
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
        // open: auto-clone on board open + the conflict badge both ride the
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
            let support_badge = support_unread.then(|| cx.theme().warning);
            self.rail_tool_icon(
                "rail-support",
                Icon::from(ExpIcon::MessageSquare),
                ToolWindow::Support,
                "Support",
                "Support",
                support_badge,
                accent,
                expanded,
                cx,
            )
        });

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
        let new_board: Option<gpui::AnyElement> = active_team.clone().map(|team_id| {
            if expanded {
                return rail_row(
                    "rail-new-board",
                    Icon::new(IconName::Plus),
                    "New board",
                    false,
                    accent,
                    None,
                    cx,
                )
                .text_color(cx.theme().muted_foreground)
                .on_click(cx.listener(move |_, _: &ClickEvent, window, cx| {
                    crate::create_board_dialog::open(window, cx, team_id.clone());
                }))
                .into_any_element();
            }
            Button::new("rail-new-board")
                .ghost()
                .small()
                .icon(IconName::Plus)
                .tooltip("New board")
                // Direct call (EXP-17): rail buttons must not dispatch
                // App-global actions.
                .on_click(cx.listener(move |_, _: &ClickEvent, window, cx| {
                    crate::create_board_dialog::open(window, cx, team_id.clone());
                }))
                .into_any_element()
        });

        // Source Control badge (EXP-253 — the git bar is headless now, this
        // badge is its whole rail presence): conflict beats sticky error
        // beats syncing; the tooltip carries the "synced Xm ago" stamp.
        let sc_badge = if conflict {
            Some(cx.theme().warning)
        } else if git_bar.read(cx).sync_error().is_some() {
            Some(cx.theme().danger)
        } else if git_bar.read(cx).is_syncing() {
            Some(cx.theme().muted_foreground)
        } else {
            None
        };
        let sc_tooltip: SharedString = if let Some(percent) =
            git_bar.read(cx).clone_progress()
        {
            format!("Source Control — cloning… {percent}%").into()
        } else {
            match git_bar.read(cx).last_synced() {
                Some(at) => {
                    let (short, _) = crate::trunk_sync::synced_ago_labels(at.elapsed());
                    if short.as_ref() == "now" {
                        "Source Control — synced just now".into()
                    } else {
                        format!("Source Control — synced {short} ago").into()
                    }
                }
                None => "Source Control".into(),
            }
        };

        // EXP-282: the expand/collapse toggle. EXP-285: it sits in the rail's
        // titlebar strip while expanded (the Cursor position); collapsed under
        // client chrome it moves into the main titlebar (`AppTitleBar`) —
        // the 44px strip is narrower than the macOS traffic-light cluster.
        let toggle = Button::new("rail-toggle")
            .ghost()
            .small()
            .icon(if expanded {
                IconName::PanelLeftClose
            } else {
                IconName::PanelLeftOpen
            })
            .tooltip(if expanded {
                "Collapse sidebar"
            } else {
                "Expand sidebar"
            })
            // Direct call (EXP-17): rail buttons must not dispatch
            // App-global actions.
            .on_click(cx.listener(|_, _: &ClickEvent, window, cx| {
                toggle_rail_expanded(window, cx);
            }));

        // EXP-285: the rail spans the full window height — its top 34px sit
        // in the window-decoration band as a drag/zoom region (the vendored
        // `TitleBar` `should_move` pattern; on macOS the native traffic
        // lights float over the strip's left).
        let client_chrome = crate::app_title_bar::client_chrome(window);
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
                    strip
                        .justify_end()
                        .child(crate::app_title_bar::interactive(toggle))
                } else if !client_chrome {
                    // Linux SSD: no in-app titlebar to host the expand
                    // toggle — keep it centered in the strip.
                    strip
                        .justify_center()
                        .child(crate::app_title_bar::interactive(toggle))
                } else {
                    // Collapsed under client chrome: the strip stays empty
                    // (traffic lights float here on macOS); the expand
                    // toggle lives in the main titlebar.
                    strip
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
                Icon::new(IconName::Search),
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
                .ghost()
                .small()
                .icon(IconName::Search)
                .tooltip("Search")
                .on_click(cx.listener(|_, _: &ClickEvent, window, cx| {
                    crate::search_sheet::open_search(window, cx)
                }))
                .into_any_element()
        };

        // Settings gear — the SINGLE settings entry point (EXP-282 dropped
        // the duplicate account-menu item). Navigates directly for the same
        // EXP-17 reason as the search button above; the keymap still
        // dispatches `OpenSettings`.
        let settings_entry: gpui::AnyElement = if expanded {
            rail_row(
                "rail-settings",
                Icon::new(IconName::Settings),
                "Settings",
                matches!(
                    resolved_screen(&self.nav, cx),
                    Some(Screen::Settings) | Some(Screen::Account)
                ),
                accent,
                None,
                cx,
            )
            .on_click(cx.listener(|_, _: &ClickEvent, window, cx| {
                navigate(window, cx, Screen::Settings)
            }))
            .into_any_element()
        } else {
            Button::new("rail-settings")
                .ghost()
                .small()
                .icon(IconName::Settings)
                .tooltip("Settings")
                .on_click(cx.listener(|_, _: &ClickEvent, window, cx| {
                    navigate(window, cx, Screen::Settings)
                }))
                .into_any_element()
        };

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
                        Icon::new(IconName::Inbox),
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
                        has_reviews.then(|| theme::tokens::GREEN.to_hsla()),
                        accent,
                        expanded,
                        cx,
                    ))
                    .children(support_icon)
                    .child(self.rail_tool_icon(
                        "rail-actions",
                        Icon::from(ExpIcon::Zap),
                        ToolWindow::Actions,
                        "Actions",
                        "Actions",
                        None,
                        accent,
                        expanded,
                        cx,
                    ))
                    .child(self.divider(expanded, cx))
                    .children(board_icons)
                    .children(new_board)
                    .child(self.divider(expanded, cx))
                    // Repo tool windows.
                    .child(self.rail_tool_icon(
                        "rail-files",
                        Icon::new(IconName::Folder),
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
            .child(settings_entry)
            .child(self.render_account_button(expanded, cx))
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
    /// The Actions tool window (EXP-253).
    actions_panel: Entity<crate::actions_panel::ActionsPanel>,
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
        _ => Icon::new(IconName::Bell),
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
        let actions_panel = cx.new(|cx| crate::actions_panel::ActionsPanel::new(window, cx));
        let collections = Store::global(cx).collections().clone();
        let local_sessions = coding_flow::LocalSessions::global(cx);
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
            review_arm: None,
            review_arm_seq: 0,
            review_merging: HashSet::new(),
            review_error: None,
            open_pulls: None,
            open_pulls_key: None,
            open_pulls_seq: 0,
            support_filter: SupportFilter::Open,
            support_threads: None,
            support_key: None,
            support_seq: 0,
            support_poll_seq: 0,
            history,
            actions_panel,
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

    /// EXP-282: the centered icon-tab strip that REPLACED the icon+title
    /// header on the two tabbed tool windows (Inbox, Support). The title was
    /// pure duplication next to the tabs; centering the chips matches the
    /// EXP-277 center/terminal strips. Same height as [`Self::tool_header`]
    /// so the lists below don't shift between tools.
    fn tool_tab_strip(&self, tabs: Vec<gpui::AnyElement>) -> gpui::Div {
        h_flex()
            .relative()
            .flex_shrink_0()
            .w_full()
            .h(px(30.))
            .px_2()
            .items_center()
            .justify_center()
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
                Icon::new(IconName::Inbox),
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
                Icon::new(IconName::CircleUser),
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
                            .ghost()
                            .xsmall()
                            .icon(Icon::from(ExpIcon::ListChecks))
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

        // Rows that merged/closed (or left the team scope) drop their
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
        // EXP-259: a failed merge (typically "not mergeable" — conflicts)
        // offers the builtin "Fix merge conflicts" action run right on the
        // row. Needs the PR's recorded branch (the run rebases it); while a
        // local run is already working that branch the button parks.
        let fixing = issue.branch.as_deref().is_some_and(|branch| {
            crate::coding_flow::LocalSessions::global_ref(cx)
                .is_some_and(|sessions| sessions.read(cx).is_branch_live(branch))
        });
        let fix_button = error.as_ref().filter(|_| issue.branch.is_some()).map(|_| {
            let mut button =
                Button::new(SharedString::from(format!("review-fix-{}", issue.id)))
                    .xsmall()
                    .outline();
            if fixing {
                button = button.label("Fixing…").disabled(true);
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
            .when(selected, |this| this.bg(row_active))
            .hover(|this| this.bg(row_hover))
            .cursor_pointer()
            .on_click(cx.listener(move |this, _, window, cx| {
                // Any click outside the armed button disarms the confirm.
                if this.review_arm.is_some() {
                    this.review_arm = None;
                    this.review_arm_seq += 1;
                    cx.notify();
                }
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

    /// The review row's "Fix conflicts" button (EXP-259): start the builtin
    /// "Fix merge conflicts" action run targeting this PR — rebase onto the
    /// default branch in the PR's worktree, resolve, force-push, then merge
    /// via `exponential_pr_merge`. The runner resolves the `pr` input
    /// against the synced store and surfaces failures as notifications.
    fn on_fix_conflicts_click(
        &mut self,
        issue_id: String,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(team_id) = active_team_id(&self.nav, cx) else {
            return;
        };
        let settings = coding_flow::CodingHub::global(cx).read(cx).settings.clone();
        // The stale merge error would linger under the running fix.
        if self
            .review_error
            .as_ref()
            .is_some_and(|(id, _)| *id == issue_id)
        {
            self.review_error = None;
            cx.notify();
        }
        crate::action_run::start_action_run(
            crate::action_run::StartActionArgs {
                action_id: api::actions::BUILTIN_FIX_CONFLICTS_ID.to_string(),
                team_id,
                repo: crate::action_run::ActionRepo::Resolve,
                options: coding::LaunchOptions::defaults(&settings),
                origin: coding::LaunchOrigin::Local,
                inputs: vec![coding::ActionInputValue {
                    key: "pr".to_string(),
                    label: "Pull request".to_string(),
                    input_type: "pr".to_string(),
                    value: issue_id,
                    display: None,
                }],
                target: Some(window.window_handle()),
                activate_app: false,
            },
            cx,
        );
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
            .hover(|this| this.bg(row_hover))
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

        // EXP-282: the open/resolved filter IS the header now — centered icon
        // tabs (same strip as the Inbox tool), no icon+title line.
        let open_tab = self
            .tool_tab(
                "support-filter-open",
                Icon::from(ExpIcon::CircleDot),
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
                Icon::from(ExpIcon::CircleCheck),
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
                Some(threads) if threads.is_empty() => self.list_note(
                    match filter {
                        SupportFilter::Open => "No open tickets.",
                        SupportFilter::Resolved => "No resolved tickets.",
                    },
                    cx,
                ),
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
        // The unread dot's indigo — the blue accent token (token-locked, not
        // loose hex).
        let unread_dot = theme::tokens::BLUE.to_hsla();

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
        // Absent/blank bodies render nothing.
        let preview: Option<SharedString> = thread
            .last_message
            .as_ref()
            .and_then(|message| message.body.as_deref())
            .map(|body| body.split_whitespace().collect::<Vec<_>>().join(" "))
            .filter(|body| !body.is_empty())
            .map(Into::into);
        let nav_id = thread.id.clone();
        let nav_title = thread.title.clone();

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
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_xs()
                            .truncate()
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(if unread { fg } else { muted })
                            .child(SharedString::from(thread.title.clone())),
                    )
                    .child(
                        div()
                            .size_2()
                            .flex_shrink_0()
                            .rounded_full()
                            .when(unread, |this| this.bg(unread_dot)),
                    ),
            )
            .when_some(preview, |this, preview| {
                this.child(
                    div()
                        .w_full()
                        .text_xs()
                        .truncate()
                        .text_color(muted)
                        .child(preview),
                )
            })
            .child(
                h_flex()
                    .w_full()
                    .gap_1()
                    .text_xs()
                    .text_color(muted)
                    // `flex_1` + `min_w_0` — without the flex basis the
                    // truncating div collapses and renders ONLY the "…"
                    // (the EXP-175 definite-width chain, again).
                    .child(div().flex_1().min_w_0().truncate().child(reporter))
                    .child(
                        div()
                            .flex_shrink_0()
                            .child(SharedString::from(format!("\u{00B7} {time}"))),
                    ),
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

    // -- Actions tool window ----------------------------------------------------

    /// *Actions* tool window (EXP-253): the team's reusable prompts —
    /// [`crate::actions_panel::ActionsPanel`] owns the list + creators.
    fn render_actions_tool(&mut self, cx: &mut gpui::Context<Self>) -> gpui::AnyElement {
        let header = self.tool_header(Icon::from(ExpIcon::Zap), "Actions", cx);
        v_flex()
            .flex_1()
            .min_h_0()
            .min_w_0()
            .child(header)
            // Sized wrapper — the flex-child rule for entity children.
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .w_full()
                    .flex()
                    .flex_col()
                    .child(self.actions_panel.clone()),
            )
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
                    .ghost()
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
                ToolWindow::Actions => self.render_actions_tool(cx),
                ToolWindow::Files => self.render_files_tool(cx),
                ToolWindow::SourceControl => self.render_source_control_tool(cx),
            })
            .into_any_element()
    }
}
