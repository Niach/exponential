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
//!   selectors — **Agent / Needs you / Chats / Devices / Drafts**
//!   (EXP-1192: Agent first, the landing page; EXP-1294: Chats replaced
//!   Reviews), Pinned, the team's **Agents** (EXP-1294), its boards, the
//!   **Running** section (my live chats, hidden while empty), then
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
//! - [`ListPanel`] — the full-width board list (what a board row navigates to).
//!   #FEED-191: no second sidebar any more; the Inbox is the header's popover
//!   ([`crate::inbox_popover`]), My issues a pill on the board. The rail itself
//!   never folds or leaves.
//!
//! Every affordance dispatches a typed action (§3.6) or navigates directly;
//! menus render in the Root overlay, outside this element tree.

use crate::controls::PointerContextMenuExt as _;
use std::collections::HashMap;

use gpui::{
    div, prelude::FluentBuilder as _, px, App, AppContext as _, ClickEvent, Entity,
    FontWeight, Hsla, InteractiveElement as _, IntoElement, ParentElement, Render,
    SharedString, StatefulInteractiveElement as _, Styled,
    Subscription, Window, WindowId,
};
use gpui_component::{
    button::{Button, ButtonVariants as _},
    h_flex,
    menu::PopupMenuItem,
    v_flex, ActiveTheme as _, Icon, Selectable as _, Sizable as _,
};
use crate::feedback::Spinner;
use sync::Store;



// EXP-282: `OpenSettings` is gone from this file — the rail gear navigates
// directly (EXP-17) and the account dropdown no longer duplicates it.
use crate::board::BoardView;
use crate::coding_flow;
use crate::trunk_sync::TrunkSync;
use crate::icons::{self, registry, ExpIcon};
use crate::navigation::{
    active_board_id, active_team_id, nav_for_window, navigate, resolved_screen,
    Navigation, Screen,
};

/// EXP-851: which LIST a detail was opened from — the kind half of
/// [`crate::navigation::TabOrigin`].
///
/// It used to name the rail's active TOOL WINDOW (a docked column beside the
/// centre). There is no tool column anymore: every one of these is a
/// full-width SCREEN ([`Screen::BoardIssues`], [`Screen::Files`],
/// [`Screen::SourceControl`]), and the enum survives only as the
/// origin vocabulary — `origin_screen` maps each back to its screen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ToolWindow {
    /// The active board's issue list (mini list) — the default tool.
    /// Selected via the rail's Projects board icons (no icon of its own).
    BoardIssues,
    /// The trunk file tree at full panel height.
    Files,
    /// The trunk's local branches; activating also opens the changes screen.
    SourceControl,
}

impl ToolWindow {
    /// EXP-851: the SCREEN this list is — what its rail entry navigates to
    /// and what the legacy `activate_tool` callers mean. `board` supplies the window's active
    /// board for the board list (the only kind that needs an argument);
    /// `None` there yields the dev sentinel, resolved at render time.
    pub(crate) fn origin_screen(self, board: Option<String>) -> Screen {
        match self {
            ToolWindow::BoardIssues => Screen::BoardIssues {
                board_id: board.unwrap_or_default(),
            },
            ToolWindow::Files => Screen::Files,
            ToolWindow::SourceControl => Screen::SourceControl,
        }
    }
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
/// variants in kebab form, plus `board:<uuid>` / `agent:<id>` /
/// `prompt:<id>` for one board's / agent's / prompt's pane, `agents` = the
/// Agents index and `agent` = the team's first agent (resolved by the
/// settings screen once synced). The
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
        "all-boards" => Some(S::AllBoards),
        "repositories" => Some(S::Repositories),
        // EXP-1558: the Widget pane (the `settings-widget` capture).
        "widget" => Some(S::Widget),
        "tools" => Some(S::Tools),
        "agent-clis" => Some(S::AgentClis),
        // NEW-6: the Agents index (web `settings/agents`: the agents + the
        // Events band); `agent` = the team's first agent's page.
        "agents" => Some(S::Agents),
        "agent" => Some(S::Agent(String::new())),
        "new-agent" => Some(S::NewAgent),
        // VAPP-110: one app's page (`app` = the team's first app,
        // `app:<slug or id>` that one). VAPP-176: no create page; VAPP-242:
        // no Apps index.
        "app" => Some(S::App(String::new())),
        spec if spec.starts_with("app:") => Some(S::App(spec["app:".len()..].to_string())),
        // NEW-6: Settings → Prompts, and its create page.
        "prompts" => Some(S::Prompts),
        "new-prompt" => Some(S::NewPrompt),
        "skills" => Some(S::Skills),
        "new-skill" => Some(S::NewSkill),
        "local-repos" => Some(S::LocalRepos),
        "sessions" => Some(S::Sessions),
        "account" => Some(S::Account),
        "notifications" => Some(S::Notifications),
        "voice" => Some(S::Voice),
        // S-3: the viewer's Assistant page (`Voice` resolves to it once
        // the agents synced: `first_agent_resolved`).
        "assistant" => Some(S::Voice),
        "api-keys" => Some(S::ApiKeys),
        "mcp" => Some(S::Mcp),
        "mcp-activity" => Some(S::McpActivity),
        "about" => Some(S::About),
        _ => spec
            .strip_prefix("board:")
            .map(|id| S::Board(id.to_string()))
            .or_else(|| spec.strip_prefix("agent:").map(|id| S::Agent(id.to_string())))
            .or_else(|| spec.strip_prefix("prompt:").map(|id| S::Prompt(id.to_string())))
            .or_else(|| spec.strip_prefix("skill:").map(|id| S::Skill(id.to_string()))),
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

/// #FEED-191: the active board on its My issues pill (the `OpenMyIssues`
/// action).
pub(crate) fn open_my_issues(window: &mut Window, cx: &mut App) {
    let nav = nav_for_window(window, cx);
    let board_id = active_board_id(&nav, cx).unwrap_or_default();
    let board = rail_shared_for_window(window, cx).read(cx).board_active.clone();
    board.update(cx, |board, cx| board.show_my_issues(cx));
    navigate(window, cx, Screen::BoardIssues { board_id });
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

/// EXP-282: one row of the rail — icon + label, left-aligned, glass
/// row fills. Hand-rolled on purpose: gpui-component's `Button` centers its
/// inner layout with no `Styled` reach into it (the settings-nav rows set the
/// same precedent), and a centered label would defeat the whole point of the
/// labelled rail. Handlers are the caller's job.
/// EXP-533: the rail spinner's label and tooltip.
const SYNCING_LABEL: &str = "Syncing\u{2026}";

/// EXP-1105 / #EXP-1704: whether the ACTIVE team auto-merges (merge mode
/// Automatic, which replaced yolo mode) — Files and Source Control leave the
/// rail for every member. `None` = no active team; only an explicit `auto`
/// hides them, so a pre-column row keeps the rail.
fn yolo_mode(nav: &Entity<Navigation>, cx: &App) -> Option<bool> {
    active_team_id(nav, cx).map(|id| {
        Store::global(cx)
            .collections()
            .teams
            .read(cx)
            .get(&id)
            .is_some_and(|team| team.merge_mode.as_deref() == Some(domain::contract::TEAM_MERGE_MODE_AUTO))
    })
}

/// EXP-1105: which of the two yolo-hideable Computer-menu entries render.
/// Off = both (today's rail). On = Files never; Source Control only while
/// the trunk needs attention or its sync failed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct YoloRail {
    files: bool,
    source_control: bool,
}

fn yolo_rail(yolo: bool, sc_failing: bool) -> YoloRail {
    YoloRail {
        files: !yolo,
        source_control: !yolo || sc_failing,
    }
}

/// #EXP-1741: what the Computer button shows (the rail computes it per render
/// from the trunk; the sidebar document's corner paints it).
#[derive(Clone, PartialEq)]
pub(crate) struct ComputerState {
    pub(crate) files: bool,
    pub(crate) source_control: bool,
    pub(crate) tooltip: SharedString,
    pub(crate) badge: Option<RailBadge>,
    pub(crate) active: bool,
}

/// The Computer button — THIS machine's tools behind one menu on the menu
/// surface: a new Terminal (the shell tab cmd-t opens, EXP-791), Files,
/// Source Control and Version history (EXP-1105: yolo mode hides Files and
/// Source Control until a git failure needs a person). Source Control's
/// attention / error badge and its reason ride the button, so a trunk
/// failure stays visible with the menu closed. Lit while Files or Source
/// Control is up. #EXP-1741: back beside the Assistant and the gear (the IDE
/// has no start menu).
pub(crate) fn computer_button(state: &ComputerState, cx: &App) -> gpui::AnyElement {
    let ComputerState { files, source_control, tooltip, badge, active } = state.clone();
    // Only a FAILURE rides the button (the attention triangle / error
    // cross); the syncing spinner stays on the menu row.
    let entry_badge = badge.clone().filter(|badge| source_control && matches!(badge, RailBadge::Icon(..)));
    let tooltip: SharedString = if entry_badge.is_some() { tooltip } else { "Computer".into() };
    let menu_sc_badge = badge.filter(|_| source_control);
    let menu = move |mut menu: gpui_component::menu::PopupMenu, _window: &mut Window, _cx: &mut gpui::Context<gpui_component::menu::PopupMenu>| {
        menu = menu.item(
            crate::controls::pointer_label_item("Terminal", false)
                .icon(Icon::from(registry::NAV_TERMINAL))
                .on_click(|_, window, cx| crate::session_bar::open_new_shell(window, cx)),
        );
        if files {
            menu = menu.item(
                crate::controls::pointer_label_item("Files", false)
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
                .on_click(|_, window, cx| activate_tool(window, cx, ToolWindow::SourceControl)),
            );
            menu = menu.item(
                crate::controls::pointer_label_item("Version history", false)
                    .icon(Icon::new(registry::UI_HISTORY))
                    .on_click(|_, window, cx| activate_tool(window, cx, ToolWindow::SourceControl)),
            );
        }
        menu
    };
    // The footer's small ghost square (the settings gear's shape), the
    // failure badge in its top-right corner. It opens upward: the button
    // sits at the window's bottom edge.
    let button = crate::controls::PointerMenu::for_button(
        Button::new("rail-computer").ghost().cursor_pointer().small().icon(registry::NAV_COMPUTER).selected(active).tooltip(tooltip),
        menu,
    )
    .anchor(gpui::Anchor::BottomLeft);
    div()
        .relative()
        .flex_shrink_0()
        .child(button)
        .when_some(entry_badge, |this, badge| this.child(div().absolute().top(px(3.)).right(px(3.)).child(rail_badge_element(badge, 10., cx))))
        .into_any_element()
}

/// A rail entry's status badge (EXP-509 — the Source Control entry outgrew
/// the plain dot): the classic colored dot (EXP-699: Inbox primary;
/// EXP-1294: Chats amber/green — the mobile tab-bar palette), a
/// small status ICON (SC attention triangle / error cross), or THE spinner's
/// arc (#EXP-1629, was the refresh glyph) while a sync is pulling.
#[derive(Clone)]
pub(crate) enum RailBadge {
    Dot(Hsla),
    Icon(ExpIcon, Hsla),
    Syncing,
    /// EXP-963: a COUNT — the web rail's `Badge` (EXP-962): the Drafts
    /// entry's pile, muted. Zero renders nothing (the badge's own rule).
    Count(usize, crate::surface::BadgeTone),
}

/// #LOGS-4: the rail repaints its document only when a badge changed (an
/// icon compares by its glyph).
impl PartialEq for RailBadge {
    fn eq(&self, other: &Self) -> bool {
        use gpui_component::IconNamed as _;
        match (self, other) {
            (Self::Dot(a), Self::Dot(b)) => a == b,
            (Self::Icon(a, ta), Self::Icon(b, tb)) => ta == tb && a.clone().path() == b.clone().path(),
            (Self::Syncing, Self::Syncing) => true,
            (Self::Count(a, ta), Self::Count(b, tb)) => a == b && ta == tb,
            _ => false,
        }
    }
}

/// #EXP-1480 — THE rail status dot (a nav row's, an agent's needs-you, a
/// team's live dot): the `sidebar.trailingDot` disc, web `SidebarMenuDot`.
pub(crate) fn rail_dot(color: Hsla) -> gpui::Div {
    // #EDE-65: eDE's rail paints the same dot.
    uicore_gpui::glyphs::dot(color)
}

/// One badge element at `glyph_px` (dots keep their fixed [`rail_dot`] size
/// regardless; a count is the 16px capsule).
pub(crate) fn rail_badge_element(badge: RailBadge, glyph_px: f32, cx: &App) -> gpui::AnyElement {
    match badge {
        RailBadge::Dot(color) => rail_dot(color).into_any_element(),
        RailBadge::Count(count, tone) => div()
            .flex_shrink_0()
            .children(crate::surface::count_badge(count, tone, cx))
            .into_any_element(),
        RailBadge::Syncing => div()
            .flex_shrink_0()
            .child(Spinner::new().icon(registry::UI_LOADING).with_size(px(glyph_px)).color(cx.theme().muted_foreground))
            .into_any_element(),
        RailBadge::Icon(icon, color) => Icon::from(icon)
            .with_size(px(glyph_px))
            .text_color(color)
            .flex_shrink_0()
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
        badge,
        cx,
    )
}

/// [`rail_row`] with an arbitrary LEAD element (an issue pin's mono
/// identifier, a tinted board or agent glyph). EXP-1294: no caption — a rail
/// row is its lead, its label and at most a badge.
pub(crate) fn rail_row_lead(
    id: impl Into<gpui::ElementId>,
    lead: gpui::AnyElement,
    label: impl Into<SharedString>,
    active: bool,
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
        .when_some(badge, |this, badge| {
            this.child(rail_trailing_slot().child(rail_badge_element(badge, 12., cx)))
        })
}

/// EXP-1477: THE trailing column of a rail row (web `SidebarMenuTrailing`):
/// its dot, count or status glyph centred in a 20px column 4px in from the
/// row's right edge — the column a row's hover gear and a section label's
/// `+` take too, so every trailing indicator of the rail lines up. The
/// negative margin eats half of the row's 8px side padding.
pub(crate) fn rail_trailing_slot() -> gpui::Div {
    div()
        .relative()
        .flex_shrink_0()
        .min_w(px(20.))
        .h(px(20.))
        .mr(px(-4.))
        .flex()
        .items_center()
        .justify_center()
}

/// A rail section's muted label row ("Agents", "Boards"…); a trailing `+`
/// child lands in the rows' trailing column.
pub(crate) fn rail_section_label(text: &'static str, cx: &App) -> gpui::Div {
    h_flex()
        .w_full()
        .h(px(24.))
        .pl_1p5()
        // EXP-1477: the `+` sits in the rows' trailing column.
        .pr_1()
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

/// [`rail_trailing_slot`] holding a row's hover gear over its badge: the
/// gear shows while `group` is hovered and the badge (if any) yields the
/// column to it — the web's `yieldsOnHover`.
pub(crate) fn rail_trailing_hover_action(
    group: &'static str,
    badge: Option<gpui::AnyElement>,
    action: impl IntoElement,
) -> gpui::Div {
    rail_trailing_slot()
        .when_some(badge, |slot, badge| {
            slot.child(div().group_hover(group, |style| style.invisible()).child(badge))
        })
        .child(
            div()
                .absolute()
                .inset_0()
                .flex()
                .items_center()
                .justify_center()
                .invisible()
                .group_hover(group, |style| style.visible())
                .child(action),
        )
}

// ---------------------------------------------------------------------------
// RailView — the icon strip left of the dock area
// ---------------------------------------------------------------------------

/// The tool-window rail: the expanded sidebar, or (EXP-870) its 48px icon
/// column beside a list. Owned and rendered by the `Shell` OUTSIDE the
/// `DockArea`. (No terminal entry — terminals are session-bar tabs, EXP-769.)
pub struct RailView {
    nav: Entity<Navigation>,
    shared: Entity<RailShared>,
    /// The branch as of the last render — a checkout refreshes the file tree.
    last_branch: Option<String>,
    /// #EXP-1725: the rows, the shared sidebar document.
    region: Entity<crate::shell_regions::SidebarRegion>,
    /// #LOGS-11: what the rows were last derived from; a render with the
    /// same inputs (a hover, a glyph's region, the Computer button) does
    /// not derive them again.
    derived: Option<RailInputs>,
    _subscriptions: Vec<Subscription>,
}

/// #LOGS-11: everything the rail's rows read
/// ([`crate::shell_regions::SidebarRegion::refresh`]): the synced store
/// (all but the transcripts and the event stream, which no row reads), the
/// window's screen, team and board, the account and the minute (Needs you's
/// stuck rows and the list bands move with the clock).
#[derive(PartialEq)]
struct RailInputs {
    revisions: Vec<u64>,
    screen: Option<Screen>,
    team: Option<String>,
    board: Option<String>,
    account: Option<String>,
    minute: i64,
}

impl RailInputs {
    fn read(nav: &Entity<Navigation>, cx: &App) -> Self {
        Self {
            revisions: Store::global(cx).collections().revisions_except(&["messages", "events"], cx),
            screen: resolved_screen(nav, cx),
            team: crate::navigation::active_team_id(nav, cx),
            board: crate::navigation::active_board_id(nav, cx),
            account: crate::queries::active_account(cx).map(|account| account.id),
            minute: crate::session_data::now_ms() / 60_000,
        }
    }
}

impl RailView {
    /// The rows, for the pictures.
    #[cfg(test)]
    pub(crate) fn region(&self) -> Entity<crate::shell_regions::SidebarRegion> {
        self.region.clone()
    }

    pub fn new(window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        let nav = nav_for_window(window, cx);
        let shared = rail_shared_for_window(window, cx);
        let git_bar = shared.read(cx).git_bar.clone();
        let collections = Store::global(cx).collections().clone();
        let avatar_cache = crate::user_avatar::AvatarCache::global(cx);
        let getting_started = crate::getting_started::GettingStartedProgress::global(cx);
        let coding_hub = coding_flow::CodingHub::global(cx);
        let mut subscriptions = vec![
            cx.observe(&shared, |_, _, cx| cx.notify()),
            cx.observe(&nav, |_, _, cx| cx.notify()),
            // Sync/conflict badge follows the trunk engine's state.
            cx.observe(&git_bar, |_, _, cx| cx.notify()),
            // Board icons follow the boards collection.
            cx.observe(&collections.boards, |_, _, cx| cx.notify()),
            // The team picker's rows follow the teams collection.
            cx.observe(&collections.teams, |_, _, cx| cx.notify()),
            // The Inbox dot is a live read over unread rows.
            cx.observe(&collections.notifications, |_, _, cx| cx.notify()),
            // EXP-778: the Pinned section's rows (issues are observed
            // below).
            cx.observe(&collections.pins, |_, _, cx| cx.notify()),
            // EXP-878: the conditional Drafts entry and its count.
            cx.observe(&collections.issue_drafts, |_, _, cx| cx.notify()),
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
        // EXP-1294: the Chats dot, the Agents rows and the Running section
        // are live reads over the session tree (+ the issues that title it).
        // #LOGS-11: never the transcripts — no row reads a message.
        subscriptions.extend(crate::session_data::observe_session_rows(cx));
        // #NEW-97: the footer's voice button follows the tray (its host, the
        // wake word's microphone, Mute and the switches).
        if let Some(tray) = etalk_gpui::voice_tray::entity(cx) {
            subscriptions.push(cx.observe(&tray, |_, _, cx| cx.notify()));
        }
        let region = crate::shell_regions::SidebarRegion::new(nav.clone(), window, cx);
        Self {
            nav,
            shared,
            last_branch: None,
            region,
            derived: None,
            _subscriptions: subscriptions,
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
            .icon(registry::UI_LOADING)
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

    /// #NEW-97: the footer's voice button while no tray host shows the tray
    /// (`etalk::voice::rail_voice`): the Computer button's ghost square with
    /// the `settings-voice` glyph, the tray icon's red dot while the wake
    /// word holds the microphone open, and the tray's rows (minus Open and
    /// Quit) on the pointer menu, each running the tray's own handler.
    fn rail_voice_entry(&self, cx: &App) -> Option<gpui::AnyElement> {
        let rail = etalk_gpui::voice_tray::rail(cx)?;
        let menu = |mut menu: gpui_component::menu::PopupMenu,
                    _window: &mut Window,
                    cx: &mut gpui::Context<gpui_component::menu::PopupMenu>| {
            // Built on open from the tray's current rows; the pointer menu
            // widths of tokens.json `menu.pointer` (a long row truncates).
            menu = menu
                .min_w(px(theme::tokens::menu::pointer::MIN_WIDTH))
                .max_w(px(theme::tokens::menu::pointer::MAX_WIDTH));
            let Some(rail) = etalk_gpui::voice_tray::rail(cx) else { return menu };
            for item in rail.menu {
                let row = match item {
                    etalk::voice::TrayItem::Check { id, label, checked, .. } => crate::controls::pointer_toggle_item(
                        format!("rail-voice-{id:?}"),
                        None,
                        label,
                        checked,
                        move |_, _, cx| etalk_gpui::voice_tray::click(id, cx),
                    ),
                    etalk::voice::TrayItem::Action { id, label, .. } => crate::controls::pointer_menu_item(
                        None,
                        label,
                        |_| None,
                        move |_, cx| etalk_gpui::voice_tray::click(id, cx),
                    ),
                    etalk::voice::TrayItem::Separator => continue,
                };
                menu = menu.item(row);
            }
            menu
        };
        let button = crate::controls::PointerMenu::for_button(
            Button::new("rail-voice")
                .ghost()
                .cursor_pointer()
                .small()
                .icon(registry::SETTINGS_VOICE)
                .tooltip(rail.tooltip),
            menu,
        )
        .anchor(gpui::Anchor::BottomLeft);
        Some(
            div()
                .relative()
                .flex_shrink_0()
                .child(button)
                .when(rail.badge, |this| {
                    this.child(div().absolute().top(px(3.)).right(px(3.)).child(rail_badge_element(
                        RailBadge::Dot(theme::tokens::DESTRUCTIVE.to_hsla()),
                        10.,
                        cx,
                    )))
                })
                .into_any_element(),
        )
    }

    /// The Getting-started entry (EXP-470): sits at the BOTTOM of the rail,
    /// under the What's-new card and above the account row, MUTED (EXP-723),
    /// and rendered only while the block is incomplete. EXP-1270: it opens
    /// the block in a centred dialog ([`crate::getting_started::open_dialog`]).
    fn rail_getting_started_entry(&self, cx: &mut gpui::Context<Self>) -> gpui::AnyElement {
        let icon = Icon::from(icons::registry::NAV_GETTING_STARTED);
        rail_row("getting-started-entry", icon, "Getting started", false, None, cx)
            .text_color(cx.theme().muted_foreground)
            .on_click(cx.listener(|_, _: &ClickEvent, window, cx| {
                crate::getting_started::open_dialog(window, cx);
            }))
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

/// EXP-1294 — a Running row's mark state off its tree's facts: needs you
/// first, then working, else the quiet waiting mark (EXP-1312: the ×4
/// `chat_row_mark_state`). Pure.
pub(crate) fn running_row_state(busy: bool, needs_you: bool) -> domain::session_tree::SessionState {
    domain::session_tree::chat_row_mark_state(needs_you, busy)
}

/// #LOGS-11: the rail's renders, for the bench.
#[cfg(test)]
thread_local! {
    pub(crate) static RAIL_RENDERS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

impl Render for RailView {
    fn render(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let _span = tracing::trace_span!("render", view = "RailView").entered();
        #[cfg(test)]
        RAIL_RENDERS.with(|n| n.set(n.get() + 1));
        // Keep the git lifecycle live regardless of which tool window is
        // open: auto-clone on board open + the attention badge both ride the
        // GitBar's load gate.
        let git_bar = self.shared.read(cx).git_bar.clone();
        git_bar.update(cx, |bar, cx| bar.ensure_loaded(window, cx));

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

        // Trunk anomalies (paused conflict / local commits / dirty tree —
        // each parks the autopull, EXP-346) paint the amber badge, EXCEPT
        // while a live Action run is legitimately dirtying this clone.
        let mut sc_attention = git_bar.read(cx).attention();
        if sc_attention.is_some() && git_bar.read(cx).repo_tasks_alive(window, cx) {
            sc_attention = None;
        }
        let team_yolo = yolo_mode(&self.nav, cx);
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

        // EXP-1105: yolo mode hides Files / Source Control unless a git
        // failure needs a person (see `yolo_rail`).
        let rail_gate = yolo_rail(team_yolo.unwrap_or(false), sc_failing);
        let computer = ComputerState {
            files: rail_gate.files,
            source_control: rail_gate.source_control,
            tooltip: sc_tooltip,
            badge: sc_badge,
            active: matches!(resolved_screen(&self.nav, cx), Some(Screen::Files) | Some(Screen::SourceControl)),
        };
        let nav = self.nav.clone();
        let inputs = RailInputs::read(&nav, cx);
        let stale = self.derived.as_ref() != Some(&inputs);
        self.region.update(cx, |region, cx| {
            region.set_computer(computer);
            if stale {
                region.refresh(&nav, cx);
            } else {
                region.repaint_computer(cx);
            }
        });
        if stale {
            self.derived = Some(inputs);
        }
        // Getting-started entry (EXP-470/548): pinned to the rail's bottom
        // (below), rendered until every checklist entry is done.
        let getting_started_icon = crate::getting_started::getting_started_visible(&self.nav, cx)
            .then(|| self.rail_getting_started_entry(cx));

        // #NEW-97: no tray host = the voice button beside the Computer one.
        let voice_entry = self.rail_voice_entry(cx);

        // EXP-863: the rail starts UNDER the left column's fixed header —
        // the titlebar strip, the team switcher row and the rule beneath
        // them are the `Shell`'s (`render_left_column`), shared with the
        // settings nav, so the rail neither pads its top
        // nor renders a header of its own.
        // EXP-1156: the `main` width, read exactly as the Shell's column
        // reads it — a FIXED width rather than `w_full`, so while the column
        // animates into Settings the rows are clipped instead of reflowing
        // frame by frame.
        v_flex()
            .w(px(crate::shell::left_column_width_for(
                crate::shell::LeftOccupant::Rail,
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
            // #EXP-1725: the rows are the shared sidebar document; right-click
            // on the row under the pointer = THE agent or chat menu.
            .child(
                div()
                    .id("rail-region")
                    .w_full()
                    .flex_1()
                    .min_h_0()
                    .child(self.region.clone())
                    .pointer_context_menu({
                        let region = self.region.clone();
                        move |menu, _, cx| {
                            let region = region.read(cx);
                            let Some(id) = region.hovered() else { return menu };
                            if region.is_agent(&id) {
                                crate::chat_actions::agent_menu_items(menu.min_w(px(220.)), &id, cx)
                            } else if region.is_active(&id) {
                                crate::chat_actions::chat_menu_items(menu, &id, false, cx)
                            } else {
                                menu
                            }
                        }
                    }),
            )
            // #EXP-1723: the start and chat buttons are the document's corner;
            // what stays under them is what still needs a person: the
            // Getting-started entry, the sync spinner, the voice button.
            .map(|column| {
                column.child(
                    v_flex()
                        .w_full()
                        .px_2()
                        .gap_1()
                        .children(getting_started_icon)
                        .children(self.render_sync_indicator(cx))
                        .children(voice_entry),
                )
            })
            .into_any_element()
    }
}

// ---------------------------------------------------------------------------
// ListPanel — the board list, or a second sidebar inside the content card
// ---------------------------------------------------------------------------

/// EXP-851: the full-width board list.
pub struct ListPanel {
    nav: Entity<Navigation>,
    /// The Board Issues tool window — the full board (the All issues · My
    /// issues pills + the grouped virtualized list with inline status/priority
    /// menus), scoped to the active board. Lives in [`RailShared`] (EXP-48 —
    /// the detail switcher reads it too).
    board_active: Entity<BoardView>,
    _subscriptions: Vec<Subscription>,
}

impl ListPanel {
    pub fn new(window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        let nav = nav_for_window(window, cx);
        let shared = rail_shared_for_window(window, cx);
        let git_bar = shared.read(cx).git_bar.clone();
        let board_active = shared.read(cx).board_active.clone();
        let collections = Store::global(cx).collections().clone();
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
            // Sync state rides the shared trunk-sync engine.
            cx.observe(&git_bar, |_, _, cx| cx.notify()),
            // Active-row highlight follows navigation.
            cx.observe(&nav, |_, _, cx| cx.notify()),
        ];

        Self { nav, board_active, _subscriptions: subscriptions }
    }

    /// The full-width board list: the board view — the pills + the grouped
    /// virtualized list with inline status/priority menus.
    fn render_board_issues_tool(
        &mut self,
        board_id: Option<String>,
        cx: &mut gpui::Context<Self>,
    ) -> gpui::AnyElement {
        self.board_active.update(cx, |board, cx| board.set_board(board_id, cx));
        div()
            .flex_1()
            .min_h_0()
            .min_w_0()
            .child(self.board_active.clone())
            .into_any_element()
    }
}

impl Render for ListPanel {
    fn render(&mut self, _window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let _span = tracing::trace_span!("render", view = "ListPanel").entered();
        let body = match resolved_screen(&self.nav, cx) {
            Some(Screen::BoardIssues { board_id }) => {
                let board_id = (!board_id.is_empty())
                    .then_some(board_id)
                    .or_else(|| active_board_id(&self.nav, cx));
                self.render_board_issues_tool(board_id, cx)
            }
            // The panel is only mounted for the board list.
            _ => div().into_any_element(),
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
    use super::{running_row_state, yolo_rail, ToolWindow, YoloRail};
    use crate::navigation::Screen;

    /// EXP-1105: yolo mode hides Files / Source Control, but a git failure
    /// (trunk attention or a sync error) brings Source Control back. Off =
    /// today's rail.
    #[test]
    fn yolo_rail_hides_entries_until_a_failure_surfaces() {
        for sc in [false, true] {
            assert_eq!(
                yolo_rail(false, sc),
                YoloRail {
                    files: true,
                    source_control: true,
                }
            );
        }
        assert_eq!(
            yolo_rail(true, false),
            YoloRail {
                files: false,
                source_control: false,
            }
        );
        assert_eq!(
            yolo_rail(true, true),
            YoloRail {
                files: false,
                source_control: true,
            }
        );
    }

    /// EXP-1294: a Running row's mark — needs you first, then working, else
    /// the quiet mark.
    #[test]
    fn a_running_row_mark_follows_its_tree() {
        use domain::session_tree::SessionState;
        assert_eq!(running_row_state(true, true), SessionState::NeedsYou);
        assert_eq!(running_row_state(false, true), SessionState::NeedsYou);
        assert_eq!(running_row_state(true, false), SessionState::Working);
        assert_eq!(running_row_state(false, false), SessionState::Idle);
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
        assert_eq!(ToolWindow::Files.origin_screen(None), Screen::Files);
        assert_eq!(
            ToolWindow::SourceControl.origin_screen(None),
            Screen::SourceControl
        );
    }

}
