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
//!   (EXP-1192: Agent first, the landing page; EXP-1294: Threads replaced
//!   Reviews), Pinned, the team's **Agents** (EXP-1294), its boards, the
//!   **Running** section (my live threads, hidden while empty), then
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
use std::collections::{BTreeMap, HashMap};

use gpui::{
    div, prelude::FluentBuilder as _, px, App, AppContext as _, ClickEvent, Entity,
    FontWeight, Hsla, InteractiveElement as _, IntoElement, ParentElement, Render,
    ScrollHandle, SharedString, StatefulInteractiveElement as _, Styled,
    Subscription, Window, WindowId,
};
use gpui_component::{
    button::{Button, ButtonVariants as _},
    h_flex,
    menu::PopupMenuItem,
    spinner::Spinner,
    v_flex, ActiveTheme as _, Icon, Selectable as _, Sizable as _,
};
use sync::Store;



// EXP-282: `OpenSettings` is gone from this file — the rail gear navigates
// directly (EXP-17) and the account dropdown no longer duplicates it.
use crate::actions::{CreateTeam, JoinTeam, OpenAbout, OpenWhatsNew, SignOut, SwitchTeam};
use crate::board::BoardView;
use crate::coding_flow;
use crate::controls::WebControl as _;
use crate::trunk_sync::TrunkSync;
use crate::icons::{self, registry, ExpIcon};
use crate::navigation::{
    active_board_id, active_team_id, nav_for_window, navigate, resolved_screen,
    Navigation, Screen,
};
use crate::issue_header::parse_hex_color;

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
        "archived-boards" => Some(S::ArchivedBoards),
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

/// EXP-1105: whether the ACTIVE team runs in yolo mode — Files and Source
/// Control leave the rail for every member. `None` = no active team;
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

/// FEED-3: hover-reveal group name for the rail's board rows — the
/// gear that jumps to the board's settings page shows only under the cursor.
/// EXP-1557: the rail's bottom fade (the search list's `FADE_HEIGHT`).
const RAIL_FADE_H: f32 = 24.;

const BOARD_ROW_GROUP: &str = "rail-board-row";
/// EXP-1294: the agent rows' twin of [`BOARD_ROW_GROUP`].
const AGENT_ROW_GROUP: &str = "rail-agent-row";
/// VAPP-110: the app rows' twin of [`BOARD_ROW_GROUP`].
const APP_ROW_GROUP: &str = "rail-app-row";
/// EXP-863: the Pinned rows' hover group — reveals the trailing Unpin button
/// (the board rows' gear recipe).
const PIN_ROW_GROUP: &str = "rail-pin-row";
/// EXP-923: the space between the Running section's rule, label and rows —
/// the rail's own `gap_1`. EXP-1248: the rows themselves stack flush.
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

/// EXP-282: one row of the rail — icon + label, left-aligned, glass
/// row fills. Hand-rolled on purpose: gpui-component's `Button` centers its
/// inner layout with no `Styled` reach into it (the settings-nav rows set the
/// same precedent), and a centered label would defeat the whole point of the
/// labelled rail. Handlers are the caller's job.
/// EXP-533: the rail spinner's label and tooltip.
const SYNCING_LABEL: &str = "Syncing\u{2026}";

/// A rail entry's status badge (EXP-509 — the Source Control entry outgrew
/// the plain dot): the classic colored dot (EXP-699: Inbox primary;
/// EXP-1294: Threads amber/green — the mobile tab-bar palette), a
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

/// #EXP-1480 — THE rail status dot (a nav row's, an agent's needs-you, a
/// team's live dot): the `sidebar.trailingDot` disc, web `SidebarMenuDot`.
pub(crate) fn rail_dot(color: Hsla) -> gpui::Div {
    div()
        .size(px(theme::tokens::sidebar::TRAILING_DOT))
        .flex_shrink_0()
        .rounded_full()
        .bg(color)
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
    /// Scroll position of the rail's middle zone (tools + board icons) —
    /// small windows with many boards must not push Settings/Account off.
    rail_scroll: ScrollHandle,
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
        // EXP-1294: the Threads dot, the Agents rows and the Running section
        // are live reads over the session tree (+ the issues that title it).
        subscriptions.extend(crate::session_data::observe_session_collections(cx));
        // #NEW-97: the footer's voice button follows the tray (its host, the
        // wake word's microphone, Mute and the switches).
        if let Some(tray) = etalk_gpui::voice_tray::entity(cx) {
            subscriptions.push(cx.observe(&tray, |_, _, cx| cx.notify()));
        }
        Self {
            nav,
            shared,
            last_branch: None,
            rail_scroll: ScrollHandle::new(),
            _subscriptions: subscriptions,
        }
    }

    /// EXP-791: a muted section label ("Agents", "Boards", "Pinned",
    /// "Running").
    fn section_label(&self, text: &'static str, cx: &App) -> gpui::Div {
        rail_section_label(text, cx)
    }

    /// EXP-778: the Pinned section's rows — the caller's pins in the ACTIVE
    /// team, in `sort_order`, each resolved against its sibling collection
    /// and HIDDEN when the target is gone (a trashed board's issue, a deleted
    /// action). An issue row leads with its mono identifier and opens the
    /// detail scoped on its board (the Issues list's path); an action row
    /// leads with the action's glyph and opens the composer with that action
    /// picked (the Actions page's ▶ Run seam — EXP-1294: actions survive as
    /// saved prompts).
    ///
    /// Empty when nothing is pinned — the caller hides the section.
    fn render_pinned_rows(&mut self, cx: &mut gpui::Context<Self>) -> Vec<gpui::AnyElement> {
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
                _ => continue,
            };
            // EXP-863: hover reveals Unpin on the row's right — the board
            // rows' gear recipe, in the trailing column (EXP-1477). The
            // click stops at the button, so the row underneath never
            // navigates.
            let unpin = {
                let team_id = team_id.clone();
                let target_id = target_id.to_string();
                rail_trailing_hover_action(
                    PIN_ROW_GROUP,
                    None,
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

    /// EXP-923 — the rail's **Active** band (EXP-1318, `threads-list.json`
    /// `copy.rail`): the ACTIVE team's threads that need you, run, or still
    /// ask for a merge (`railThreads`), every thread the shapes make visible,
    /// under the boards (EXP-1075: the other teams stay the switcher's dot).
    ///
    /// A live run is a place, not a document: it never owns a tab, and the
    /// section HIDES entirely while nothing runs. EXP-1294: one plain row per
    /// thread ([`crate::session_data::live_roots_for_rail`]) — the agent's
    /// mark (animating while it works, the amber badge while it needs you)
    /// and the title (it names the issue already), NO caption. A row
    /// opens the thread with no tab of its own (`navigate_from_live_rail`)
    /// and stays lit on the thread AND on any of its sessions.
    ///
    /// Returns `None` with nothing running (the caller renders neither the
    /// rule nor the label).
    fn render_running_section(&mut self, cx: &mut gpui::Context<Self>) -> Option<gpui::AnyElement> {
        let team_id = active_team_id(&self.nav, cx)?;
        let running = crate::session_data::live_roots_for_rail(&team_id, cx);
        if running.is_empty() {
            return None;
        }
        let screen = resolved_screen(&self.nav, cx);
        // #EXP-1461: ROOT rows only (the tree's state rolled up into it); a
        // row stays lit while any session of its tree is on screen.
        let elements: Vec<gpui::AnyElement> = running
            .into_iter()
            .enumerate()
            .map(|(index, row)| {
                let active = match &screen {
                    Some(Screen::Thread { session_id }) => *session_id == row.root_id,
                    Some(Screen::Session { session_id }) => row.session_ids.contains(session_id),
                    _ => false,
                };
                let state = running_row_state(row.busy, row.needs_you);
                // The mark is the agent of the newest session working in the
                // tree, else the root's.
                let agent = row
                    .session_ids
                    .iter()
                    .rev()
                    .filter_map(|id| crate::session_data::session(id, cx))
                    .find(crate::session_data::session_is_working)
                    .or_else(|| crate::session_data::session(&row.root_id, cx))
                    .and_then(|session| crate::run_rows::run_agent(session.cli.as_deref()));
                let target = Screen::Thread {
                    session_id: row.root_id.clone(),
                };
                let menu_root = row.root_id.clone();
                let element = crate::run_rows::run_row(
                    crate::run_rows::RunRowSpec {
                        id_prefix: "rail-running",
                        index,
                        agent,
                        state,
                        identifier: None,
                        // EXP-1318: the root session's device icon trails.
                        trailing: crate::threads_view::thread_device_icon(
                            row.device_id.as_deref(),
                            &format!("rail-{}", row.root_id),
                            cx,
                        )
                        .map(|icon| rail_trailing_slot().child(icon).into_any_element()),
                        title: SharedString::from(row.title),
                        active,
                        on_open: Box::new(move |_, window, cx| {
                            crate::navigation::navigate_from_live_rail(window, cx, target.clone());
                        }),
                        guides: Default::default(),
                        unread: row.unread,
                    },
                    cx,
                );
                // Thread 58cb32df: a chat row's right-click = THE chat menu.
                crate::threads_view::chat_row_menu(&menu_root, element.into_any_element(), cx)
            })
            .collect();
        let rule = self.section_rule(cx);
        Some(
            v_flex()
                .w_full()
                .gap(px(RUNNING_ROW_GAP))
                .child(rule)
                .child(self.section_label(domain::session_tree::THREADS_LIST.copy.rail.as_str(), cx))
                .child(v_flex().w_full().children(elements))
                .into_any_element(),
        )
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
        rail_row_lead(id, lead, label, active, badge, cx)
    }

    /// A rail entry that navigates STRAIGHT to a tab-less full-page screen
    /// instead of activating a tool window (EXP-467's Actions entry,
    /// generalized by EXP-686 for Devices / Actions, and by
    /// EXP-1294 for Home / Threads): the settings gear's direct-navigation shape in
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
        let button = crate::controls::PointerMenu::for_button(Button::new("rail-computer")
            .ghost()
            .cursor_pointer()
            .small()
            .icon(registry::NAV_COMPUTER)
            .selected(active)
            .tooltip(tooltip),
            menu,
        )
        .anchor(gpui::Anchor::BottomLeft);
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

    /// [`Self::rail_screen_entry`] with the highlight decided by the CALLER.
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
                // EXP-851: the rail is not a list — an entry never lends one.
                crate::navigation::navigate_from_rail(window, cx, screen.clone());
            }))
            .into_any_element()
    }

    /// EXP-818/EXP-1294: the Home entry — the composer page, worded "Agent"
    /// (the web sidebar's word, EXP-772) with the chat glyph. EXP-1001: no
    /// badge — the Threads dot and the Running section are the live signal.
    fn rail_home_entry(&self, cx: &mut gpui::Context<Self>) -> gpui::AnyElement {
        self.rail_screen_entry(
            "rail-agent",
            Icon::from(registry::ACTION_CHAT),
            "Agent",
            Screen::Home,
            None,
            cx,
        )
    }

    /// EXP-1294: the Threads entry — the team's sessions list page, a dot
    /// while a thread needs you (amber) or one is running (green)
    /// ([`threads_badge`]); EXP-1323: else the primary unread dot while a
    /// thread has an agent reply the viewer has not read (`threads_unread`,
    /// the Inbox's dot).
    fn rail_threads_entry(&self, cx: &mut gpui::Context<Self>) -> gpui::AnyElement {
        let badge = active_team_id(&self.nav, cx).and_then(|team_id| {
            let (needs_you, running) = crate::session_data::rail_threads_badge(&team_id, cx);
            threads_badge(needs_you, running).map(RailBadge::Dot).or_else(|| {
                crate::session_data::rail_threads_unread(&team_id, cx)
                    .then(|| RailBadge::Dot(theme::tokens::PRIMARY.to_hsla()))
            })
        });
        self.rail_screen_entry(
            "rail-threads",
            Icon::from(registry::NAV_THREADS),
            domain::session_tree::THREADS_LIST.copy.title.as_str(),
            Screen::Threads,
            badge,
            cx,
        )
    }

    /// VAPP-110: one Apps row (the Agents row's twin): the app's glyph and
    /// name. VAPP-242: click = THE RUN VIEW, lit while it runs and in its
    /// editor; the hover gear (the creator's and the owners') opens THE EDITOR (its default tab).
    fn rail_app_row(
        &self,
        index: usize,
        entry: &crate::apps_view::AppEntry,
        manage: bool,
        cx: &mut gpui::Context<Self>,
    ) -> gpui::AnyElement {
        let active = matches!(
            resolved_screen(&self.nav, cx),
            Some(Screen::AppRun { app } | Screen::App { app }) if app == entry.id || app == entry.slug
        );
        let open = entry.id.clone();
        let settings_id = entry.id.clone();
        self.entry(
            ("rail-app-row", index),
            entry.rail_icon().with_size(gpui_component::Size::Medium).flex_shrink_0().into_any_element(),
            SharedString::from(entry.name.clone()),
            active,
            None,
            cx,
        )
        .group(APP_ROW_GROUP)
        .on_click(cx.listener(move |_, _: &ClickEvent, window, cx| {
            navigate(window, cx, Screen::AppRun { app: open.clone() })
        }))
        .when(manage, |row| {
            row.child(rail_trailing_hover_action(
                APP_ROW_GROUP,
                None,
                Button::new(("rail-app-settings", index))
                    .ghost()
                    .cursor_pointer()
                    .xsmall()
                    .icon(Icon::from(registry::NAV_SETTINGS))
                    .tooltip("App settings")
                    .on_click(cx.listener(move |_, _: &ClickEvent, window, cx| {
                        cx.stop_propagation();
                        // VAPP-242: the app's editor on its default tab.
                        navigate(window, cx, Screen::App { app: settings_id.clone() });
                    })),
            ))
        })
        .into_any_element()
    }

    /// EXP-1294: one Agents row — the agent root's glyph tinted with its
    /// color (the board rows' recipe), lit while its channel is up, the
    /// Threads entry's dot while it needs you (amber) or works (green).
    /// Click = the agent's channel; a hover gear (every member: the agent
    /// page is theirs to edit) opens the agent's settings page.
    fn rail_agent_row(
        &self,
        index: usize,
        agent: &domain::session_tree::AgentRow,
        cx: &mut gpui::Context<Self>,
    ) -> gpui::AnyElement {
        // NEW-6: the row is the agent DEFINITION; a click opens its page
        // (the viewer's own conversation with it, empty before the first
        // message, whose composer then opens it).
        let active = matches!(
            resolved_screen(&self.nav, cx),
            Some(Screen::Agent { agent_id })
                if crate::agent_screen::resolve_agent_id(&agent_id, cx) == agent.agent_id
        );
        let icon = crate::icons::agent_row_icon(agent);
        let settings_id = agent.agent_id.clone();
        let open_id = agent.agent_id.clone();
        self.entry(
            ("rail-agent-row", index),
            icon.with_size(gpui_component::Size::Medium).flex_shrink_0().into_any_element(),
            SharedString::from(agent.name.clone()),
            active,
            None,
            cx,
        )
        .group(AGENT_ROW_GROUP)
        .on_click(cx.listener(move |_, _: &ClickEvent, window, cx| {
            navigate(
                window,
                cx,
                Screen::Agent {
                    agent_id: open_id.clone(),
                },
            )
        }))
        // Thread 58cb32df (chats.json `agentMenuCases`): the right-click =
        // THE agent menu.
        .pointer_context_menu({
            let agent_id = agent.agent_id.clone();
            move |menu, _, cx| crate::chat_actions::agent_menu_items(menu.min_w(px(220.)), &agent_id, cx)
        })
        // EXP-1477: the dot and the hover gear share the trailing column
        // (the gear used to sit beside the dot, pushing it inwards).
        .child(rail_trailing_hover_action(
            AGENT_ROW_GROUP,
            threads_badge(agent.needs_you, agent.busy)
                .map(|tone| rail_badge_element(RailBadge::Dot(tone), 12., cx)),
            Button::new(("rail-agent-settings", index))
                .ghost()
                .cursor_pointer()
                .xsmall()
                .icon(Icon::from(registry::NAV_SETTINGS))
                .tooltip("Agent settings")
                .on_click(cx.listener(move |_, _: &ClickEvent, window, cx| {
                    cx.stop_propagation();
                    select_settings_section(
                        window,
                        cx,
                        crate::settings::SettingsSection::Agent(settings_id.clone()),
                    );
                    navigate(window, cx, Screen::Settings);
                })),
        ))
        .into_any_element()
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

        crate::controls::PointerMenu::for_button(Button::new("rail-account")
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
            .tooltip(full_name.clone()),
            move |menu, _window, _cx| {
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
            }).anchor(gpui::Anchor::BottomLeft)
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
                row.child(rail_trailing_hover_action(
                    BOARD_ROW_GROUP,
                    None,
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
                ))
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
/// plain live green (the Threads dot's tokens — one palette, one meaning). A DOT, never a count: the switcher answers "is something waiting
/// for me elsewhere", and the number is the rail's job once you are there.
fn team_run_dot(live: Option<&crate::session_data::TeamLiveRuns>) -> Option<Hsla> {
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

/// EXP-1294 — the rail Threads entry's dot: amber while a thread needs you
/// (it wins — a session asking a question is the thing you must be told),
/// green while one is queued or running, none otherwise. Pure.
fn threads_badge(needs_you: bool, running: bool) -> Option<Hsla> {
    if needs_you {
        Some(theme::tokens::YELLOW.to_hsla())
    } else if running {
        Some(theme::tokens::GREEN.to_hsla())
    } else {
        None
    }
}

/// EXP-1294 — a Running row's mark state off its tree's facts: needs you
/// first, then working, else the quiet waiting mark (EXP-1312: the ×4
/// `thread_row_mark_state`). Pure.
fn running_row_state(busy: bool, needs_you: bool) -> domain::session_tree::SessionState {
    domain::session_tree::thread_row_mark_state(needs_you, busy)
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
    live: &BTreeMap<String, crate::session_data::TeamLiveRuns>,
    cx: &mut App,
) -> impl IntoElement {
    let active_team = active_team_id(nav, cx);
    // EXP-1075: the rail's Running section is the ACTIVE team's runs now, so
    // the switcher carries the others — a dot per team, never a count.
    let teams: Vec<(String, String, bool, Option<Hsla>, Option<String>)> = Store::global(cx)
        .collections()
        .teams_sorted(cx)
        .into_iter()
        .map(|team| {
            let active = Some(team.id.as_str()) == active_team.as_deref();
            let dot = team_run_dot(live.get(&team.id));
            (team.id, team.name, active, dot, team.icon_url)
        })
        .collect();
    // The trigger's own dot folds every OTHER team into one: amber wins over
    // green, because a run asking a question is the thing you must be told.
    let elsewhere = teams
        .iter()
        .filter(|(_, _, active, _, _)| !*active)
        .filter_map(|(id, _, _, _, _)| live.get(id))
        .fold(crate::session_data::TeamLiveRuns::default(), |mut acc, runs| {
            acc.count += runs.count;
            acc.needs_input |= runs.needs_input && runs.count > 0;
            acc
        });
    let elsewhere_tone = team_run_dot(Some(&elsewhere));
    // The trigger names the ACTIVE team; nothing synced yet degrades to
    // the glyph (`team_avatar`'s own fallback) and an empty label.
    let active_row = teams.iter().find(|(_, _, active, _, _)| *active);
    let team_name: SharedString = active_row
        .map(|(_, name, _, _, _)| SharedString::from(name.clone()))
        .unwrap_or_default();
    let team_icon = active_row.and_then(|(_, _, _, _, icon)| icon.clone());
    // EXP-449: `active_board_id` falls back to the team's first board, so
    // this is `None` only when nothing is in scope at all.
    let board_id = active_board_id(nav, cx);

    let switcher = crate::controls::PointerMenu::for_button(Button::new("rail-team-switcher")
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
                .child(crate::user_avatar::team_avatar(team_icon.as_deref(), 28., cx))
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
                                rail_dot(tone)
                                    .absolute()
                                    .top(px(-2.))
                                    .right(px(-2.))
                                    // The hairline keeps the dot readable
                                    // over the glyph it overlaps.
                                    .border_1()
                                    .border_color(theme::tokens::BACKGROUND.to_hsla()),
                            )
                        }),
                ),
        ),
        move |menu, _window, _cx| {
            // Flat checked rows (the menu builder has no submenus); always
            // shown, even with a single team (EXP-434: no teams=1 special
            // case anywhere).
            let mut menu = menu;
            for (id, name, active, dot, icon) in &teams {
                let label = SharedString::from(name.clone());
                let icon = icon.clone();
                let switch = Box::new(SwitchTeam {
                    team_id: id.clone(),
                });
                // EXP-1075: the ACTIVE team never wears a dot — its runs are
                // the rail's Running section, right there. Every other team
                // with live runs of mine gets the trailing [`rail_dot`].
                let dot = if *active { None } else { *dot };
                // #EXP-1645: each row leads with the team's mark (web parity).
                menu = menu.menu_element_with_check(*active, switch, move |_window, cx| {
                    h_flex()
                        .flex_1()
                        .min_w_0()
                        .items_center()
                        .gap_2()
                        .child(crate::user_avatar::team_avatar(icon.as_deref(), 20., cx))
                        .child(div().flex_1().min_w_0().truncate().child(label.clone()))
                        .children(dot.map(rail_dot))
                });
            }
            menu.separator()
                .menu_with_icon("New team", registry::UI_ADD, Box::new(CreateTeam))
                .menu_with_icon("Join team", registry::UI_INVITE, Box::new(JoinTeam))
        }).anchor(gpui::Anchor::TopLeft);

    h_flex()
        .w_full()
        .gap_1()
        .items_center()
        .child(div().flex_1().min_w_0().child(switcher))
        // #FEED-191: the Inbox, a popover anchored under its icon.
        .child(crate::inbox_popover::InboxButton::new(nav.clone()))
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

        let team_yolo = yolo_mode(&self.nav, cx);
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
        // #FEED-191 (needs-you.json): Needs you lists only while a row exists
        // (or its page is up: it stays reachable after the last answer).
        let needs_you_count = active_team_id(&self.nav, cx)
            .map(|id| crate::needs_you::rows_in_team(&id, cx).rows.len())
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
        let mut board_icons: Vec<gpui::AnyElement> = boards
            .iter()
            .enumerate()
            .map(|(index, board)| self.rail_board_icon(index, board, cx))
            .collect();
        // EXP-1274: a team with no boards says so under the band, the shared
        // list empty state (web sidebar `board-switcher` parity).
        if board_icons.is_empty() && active_team.is_some() {
            board_icons.push(
                crate::controls::empty_state_list(
                    Icon::new(registry::NAV_BOARDS),
                    crate::empty_state_copy::BOARD_SWITCHER,
                    cx,
                )
                .into_any_element(),
            );
        }
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
        // EXP-1294: the Agents section — the team's agent roots as tinted
        // rows under an "Agents" label with a trailing `+` (the Boards
        // section's twin); no rows, no empty copy — just the header.
        let agents = active_team
            .as_deref()
            .map(|team_id| crate::session_data::agents_in_team(team_id, cx))
            .unwrap_or_default();
        let agent_rows: Vec<gpui::AnyElement> = agents
            .iter()
            .enumerate()
            .map(|(index, agent)| self.rail_agent_row(index, agent, cx))
            .collect();
        // EXP-1319: the `+` opens the New agent PAGE in Settings (web parity),
        // owner-only like the page.
        let agents_header: Option<gpui::AnyElement> =
            active_team.clone().map(|team_id| {
                let owner = crate::settings::is_owner(cx, &team_id);
                self.section_label(domain::session_tree::THREADS_LIST.copy.agents.as_str(), cx)
                    .when(owner, |label| {
                        label.child(
                            Button::new("rail-new-agent")
                                .ghost().cursor_pointer()
                                .xsmall()
                                .icon(registry::UI_ADD)
                                .tooltip(domain::agent_settings::AGENT_SETTINGS.copy.new_agent.clone())
                                .on_click(cx.listener(move |_, _: &ClickEvent, window, cx| {
                                    select_settings_section(
                                        window,
                                        cx,
                                        crate::settings::SettingsSection::NewAgent,
                                    );
                                    navigate(window, cx, Screen::Settings);
                                })),
                        )
                    })
                    .into_any_element()
            });
        // VAPP-110: the Apps section — the team's apps (by name) under an
        // "Apps" label whose `+` opens the New app form
        // (every member creates apps).
        let me = crate::queries::active_account(cx).map(|account| account.user_id);
        let app_owner = active_team.as_deref().is_some_and(|team_id| crate::settings::is_owner(cx, team_id));
        let app_rows: Vec<gpui::AnyElement> = active_team
            .as_deref()
            .map(|team_id| crate::apps_view::app_entries(team_id, cx))
            .unwrap_or_default()
            .iter()
            .enumerate()
            .map(|(index, entry)| {
                let manage = crate::apps_view::can_manage(entry, me.as_deref(), app_owner);
                self.rail_app_row(index, entry, manage, cx)
            })
            .collect();
        // VAPP-242: the label is NOT clickable (there is no Apps page); the
        // `+` opens THE NEW MENU (New app / Import…).
        let apps_header: Option<gpui::AnyElement> = active_team.clone().map(|team_id| {
            self.section_label(domain::apps::APPS.copy.title.as_str(), cx)
                .child(crate::apps_view::new_menu_button(team_id))
                .into_any_element()
        });
        // EXP-923: MY live threads — under the boards, hidden while nothing
        // is running.
        let running_section = self.render_running_section(cx);
        // EXP-778: the Pinned section — hidden while nothing is pinned.
        let pinned_rows = self.render_pinned_rows(cx);
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
        // EXP-1105: yolo mode hides Files / Source Control unless a git
        // failure needs a person (see `yolo_rail`).
        let rail_gate = yolo_rail(team_yolo.unwrap_or(false), sc_failing);
        // NEW-6: no Actions entry (prompts live in the composer's `/` menu
        // and Settings → Prompts); Drafts follows while any draft is parked.
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
        let on_needs_you = matches!(resolved_screen(&self.nav, cx), Some(Screen::NeedsYou));
        let needs_you_entry = (needs_you_count > 0 || on_needs_you).then(|| {
            self.rail_screen_entry(
                "rail-needs-you",
                Icon::from(registry::NAV_NEEDS_YOU),
                domain::needs_you::NEEDS_YOU.copy.entry.as_str(),
                Screen::NeedsYou,
                Some(RailBadge::Count(
                    needs_you_count,
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

        // #NEW-97: no tray host = the voice button beside the Computer one.
        let voice_entry = self.rail_voice_entry(cx);

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
        // EXP-1557: rows running on under the footer fade out instead of
        // being cut mid-row (the search list's bottom-fade idiom), into the
        // ground the Shell paints at the pane's bottom edge.
        let rail_fade = (!whats_new_clearance)
            .then(|| {
                let pane = self.rail_scroll.bounds();
                let max = f32::from(self.rail_scroll.max_offset().y);
                let offset = f32::from(self.rail_scroll.offset().y);
                let more = max > 0. && -offset < max - 1.;
                let height = f32::from(window.viewport_size().height);
                (more && height > 0.).then(|| {
                    theme::background_gradient_color_at(f32::from(pane.bottom()) / height)
                })
            })
            .flatten();
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
            // Settings/Account off small windows. Rail order (EXP-1294):
            // [Agent, Needs you?, Chats, Devices, Drafts?] / Pinned
            // (EXP-778) / Agents + "+" / Boards + "+" / Running (EXP-923).
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
                    // EXP-791: "Agent", the web sidebar's word for the Home
                    // page (EXP-772). EXP-1192: the first entry — the page
                    // the window lands on.
                    .child(self.rail_home_entry(cx))
                    // #FEED-191: Needs you (while any), right after Agent.
                    .children(needs_you_entry)
                    // EXP-1294: Threads, the team's sessions list.
                    .child(self.rail_threads_entry(cx))
                    // EXP-686: Devices, the machine list.
                    .child(self.rail_screen_entry(
                        "rail-devices",
                        Icon::from(icons::registry::NAV_DEVICES),
                        "Devices",
                        Screen::Devices,
                        None,
                        cx,
                    ))
                    // Drafts (while any) closes the nav entries.
                    .children(drafts_entry)
                    // EXP-778: Pinned sits between the nav entries and the
                    // Agents (rail order: entries / Pinned / Agents / boards /
                    // Running).
                    .children(pinned_section)
                    .child(self.section_rule(cx))
                    .children(agents_header)
                    .children(agent_rows)
                    // VAPP-110: the Apps group, the Agents' twin.
                    .child(self.section_rule(cx))
                    .children(apps_header)
                    .children(app_rows)
                    .child(self.section_rule(cx))
                    .children(boards_header)
                    .children(board_icons)
                    // EXP-923: Running — MY live threads.
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
            }))
            .children(rail_fade.map(|ground| {
                div()
                    .absolute()
                    .bottom_0()
                    .left_0()
                    .right_0()
                    .h(px(RAIL_FADE_H))
                    .bg(gpui::linear_gradient(
                        180.,
                        gpui::linear_color_stop(ground.opacity(0.), 0.),
                        gpui::linear_color_stop(ground, 1.),
                    ))
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
                            .children(voice_entry)
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
    use super::{running_row_state, threads_badge, yolo_rail, ToolWindow, YoloRail};
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

    /// EXP-1294: the Threads entry's dot — amber wins over green, nothing
    /// when no thread needs you or runs.
    #[test]
    fn the_threads_dot_says_needs_you_before_running() {
        let amber = theme::tokens::YELLOW.to_hsla();
        let green = theme::tokens::GREEN.to_hsla();
        assert_eq!(threads_badge(true, true), Some(amber));
        assert_eq!(threads_badge(true, false), Some(amber));
        assert_eq!(threads_badge(false, true), Some(green));
        assert_eq!(threads_badge(false, false), None);
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
