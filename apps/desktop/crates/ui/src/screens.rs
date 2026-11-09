//! The ONE main view (masterplan-v3 §4.2, reworked — EXP-288/EXP-851): a
//! TAB-BASED area whose tabs are DETAIL VIEWS ONLY (issue detail, a chat
//! or a session of it, an agent's channel, terminal). Everything else is a
//! plain
//! full-width screen: the LIST screens (a board, the Inbox,
//! Files, Source Control), the rail's pages and Settings. Every detail tab
//! REMEMBERS the LIST it was opened from ([`TabEntry::origin`]) — EXP-1192:
//! the Inbox, drawn as the card's second sidebar beside it, so a
//! tab click, a go-back and a go-forward all restore the sidebar for free.
//!
//! One panel: a compact chip strip over content swapped on the per-window
//! [`Navigation`] state. The heavyweight views (issue detail, file viewer,
//! …) stay single instances re-pointed on tab switch; EXP-894: what a switch
//! would lose there (comment/reply drafts, scroll) is stashed per tab by the
//! view itself (`crate::tab_state` documents why not one view per tab), and
//! a chat / session / agent tab keeps its own view outright. Closing the
//! active tab activates its
//! neighbor; closing the last shows the empty state. A
//! team switch drops all tabs (they are team-scoped). Tabs that don't fit
//! the strip collapse into a "+N" overflow menu (EXP-288).

use std::collections::HashMap;

use gpui::{
    div, prelude::FluentBuilder as _, px, App, AppContext as _, ClickEvent, Entity, FocusHandle,
    Focusable, FontWeight, InteractiveElement as _, IntoElement, ParentElement,
    Render, StatefulInteractiveElement as _, Styled, Subscription, Window, WindowId,
};
use gpui_component::{
    button::{Button, ButtonVariants as _},
    dock::{Panel, PanelControl, PanelEvent},
    h_flex,
    v_flex, ActiveTheme as _, Icon, Sizable as _,
};
use sync::Store;


use crate::actions::{CreateTeam, JoinTeam, NewBoard};
use crate::controls::WebControl as _;
use crate::icons::{registry, ExpIcon};
use crate::issue_detail::IssueDetailView;
use crate::navigation::{
    active_team_id, nav_for_window, resolved_screen, screen_title, set_screen,
    shapes_ready, Navigation, PendingOrigin, Screen, TabOrigin,
};
use crate::sidebar::{rail_shared_for_window, ListPanel, RailShared};

/// Stable serialization name (§3.3: never change once shipped in a layout).
pub const PANEL_NAME: &str = "Screens";

/// Per-tab hover group (EXP-65): reveals the undock button. Reused per tab —
/// gpui resolves `group_hover` against the innermost enclosing group (the
/// same idiom as the issue list's `ROW_GROUP`).
pub(crate) const TAB_GROUP: &str = "center-tab";

/// EXP-277: per-window handle to the center [`ScreensPanel`] so the titlebar
/// ([`crate::app_title_bar::AppTitleBar`]) can host the tab strip. Mirrors
/// `sidebar::RailRegistry` / `navigation`'s per-window globals. Shell windows
/// only — undocked windows have no center panel and no strip.
#[derive(Default)]
struct ScreensRegistry {
    by_window: HashMap<WindowId, Entity<ScreensPanel>>,
}

impl gpui::Global for ScreensRegistry {}

/// This window's center screens panel, if one exists yet.
pub(crate) fn screens_for_window(window: &Window, cx: &App) -> Option<Entity<ScreensPanel>> {
    cx.try_global::<ScreensRegistry>()
        .and_then(|registry| registry.by_window.get(&window.window_handle().window_id()).cloned())
}

/// [`screens_for_window`] by id — for callers that hold a `WindowId` instead
/// of a live `&Window` (the session bar's repaint poke). `None` in windows
/// without a panel (undocked screens).
pub(crate) fn screens_for_window_id(
    window_id: WindowId,
    cx: &App,
) -> Option<Entity<ScreensPanel>> {
    cx.try_global::<ScreensRegistry>()
        .and_then(|registry| registry.by_window.get(&window_id).cloned())
}

/// EXP-746: every window's open screen for `session_id` (usually zero or
/// one). The engine's exit edge is app-global — it knows the row, not the
/// window that opened it — so it marks whatever is up.
pub(crate) fn session_views(
    session_id: &str,
    cx: &App,
) -> Vec<Entity<crate::session_screen::SessionScreenView>> {
    let Some(registry) = cx.try_global::<ScreensRegistry>() else {
        return Vec::new();
    };
    registry
        .by_window
        .values()
        .flat_map(|panel| {
            let panel = panel.read(cx);
            panel.session_view(session_id)
        })
        .collect()
}

/// EXP-851: keep the window's active BOARD in step with the screen go-back /
/// go-forward landed on — the second sidebar follows the tab's own origin
/// (EXP-1192), so nothing else needs restoring, but the
/// repo-backed surfaces (files, git, the `+` shell cwd) still resolve through
/// `active_board_id`.
pub(crate) fn restore_origin_for_screen(window: &Window, cx: &mut App, screen: &Screen) {
    if let Screen::BoardIssues { board_id } = screen {
        if !board_id.is_empty() {
            crate::navigation::set_active_board(window, cx, board_id.clone());
            return;
        }
    }
    let origin = screens_for_window(window, cx).and_then(|panel| panel.read(cx).origin_of(screen));
    if let Some(origin) = origin {
        crate::sidebar::apply_origin(window, cx, &origin);
    }
}

/// Drop a closed window's entry (called from the `Shell` release hook,
/// mirroring `sidebar::remove_window`).
pub(crate) fn remove_window(window_id: WindowId, cx: &mut App) {
    if let Some(registry) = cx.try_global::<ScreensRegistry>() {
        if registry.by_window.contains_key(&window_id) {
            cx.global_mut::<ScreensRegistry>().by_window.remove(&window_id);
        }
    }
}

/// EXP-1294 — the strip's "+": Home, the composer every new chat starts
/// from (the New session tab and its duplicate-tab hand-off are gone).
pub(crate) fn open_home(window: &mut Window, cx: &mut App) {
    crate::navigation::navigate_to_chat(window, cx, crate::navigation::ChatSeed::default());
}

/// EXP-1316 (1b): the CHAT root a session opens inside (its tree's root
/// when that root is a chat); `None` = its own session screen (an agent
/// channel's session, or a row not synced yet).
pub(crate) fn in_place_root(session_id: &str, cx: &App) -> Option<String> {
    crate::session_data::root_of(session_id, cx)
        .filter(|root| root.is_root() && root.is_chat())
        .map(|root| root.id)
}

/// Build a FRESH content view for `screen` (EXP-65 undocked windows). The
/// panel's own shared single-instance views (re-pointed on tab switch) must
/// never be moved to another window; a fresh construction also binds the
/// view to the new window's per-window registries (rail, nav, resolver).
pub(crate) fn build_screen_content(
    screen: &Screen,
    window: &mut Window,
    cx: &mut App,
) -> gpui::AnyView {
    match screen {
        Screen::IssueDetail { issue_id } => {
            let view = cx.new(|cx| IssueDetailView::new(window, cx));
            let issue_id = issue_id.clone();
            view.update(cx, |detail, cx| detail.set_issue(issue_id, window, cx));
            view.into()
        }
        // EXP-746: a fresh view is always the REMOTE one — a second view over
        // a live local run would take a second handle on one engine. Never
        // undockable today, so this only keeps the match total.
        Screen::Session { session_id } => cx
            .new(|cx| {
                crate::session_screen::SessionScreenView::remote(session_id.clone(), window, cx)
            })
            .into(),
        // EXP-1294: a chat and an agent's channel are never undocked
        // either; kept total with their real views.
        Screen::Chat { session_id } => cx
            .new(|cx| crate::session_chat::SessionChatScreenView::new(session_id.clone(), window, cx))
            .into(),
        Screen::Agent { agent_id } => cx
            .new(|cx| crate::agent_screen::AgentScreenView::new(agent_id.clone(), window, cx))
            .into(),
        // EXP-769: a terminal pops out through `undock::open_undocked_terminal_tab`
        // (the manager keeps the tab; the window renders the same view) —
        // never through here. Kept total for the compiler.
        Screen::Terminal { .. } => cx.new(|_| NeverUndocked).into(),
        // Never undockable — unreachable via the undock path, kept total for
        // the compiler.
        Screen::Devices => cx
            .new(|cx| crate::devices_view::DevicesView::new(window, cx))
            .into(),
        Screen::Drafts => cx
            .new(|cx| crate::drafts_view::DraftsView::new(window, cx))
            .into(),
        Screen::NeedsYou => cx
            .new(|cx| crate::needs_you::NeedsYouView::new(window, cx))
            .into(),
        // EXP-1170: the New issue page is never undocked (one shared view
        // owns the draft's autosave).
        Screen::IssueDraft { .. } => cx.new(|_| NeverUndocked).into(),
        Screen::Home => cx
            .new(|cx| crate::chat_screen::ChatScreenView::new(window, cx))
            .into(),
        Screen::Chats => cx
            .new(|cx| crate::chats_view::ChatsView::new(window, cx))
            .into(),
        // VAPP-110: an editor and (VAPP-242) a run view are never undocked
        // (the editor owns the member's live editor session).
        Screen::App { .. } | Screen::AppRun { .. } => cx.new(|_| NeverUndocked).into(),
        // VAPP-90: the dev-only kitchen sink is never undocked.
        Screen::ExponentialUiKitchenSink
        | Screen::ExponentialUiSpecimen { .. }
        | Screen::StyleguideEntry { .. } => {
            cx.new(|_| NeverUndocked).into()
        }
        Screen::Settings => cx.new(|cx| crate::settings::SettingsView::new(window, cx)).into(),
        // EXP-851: the list screens are never undockable (only a detail is),
        // so this arm exists to keep the match total.
        Screen::BoardIssues { .. }
        | Screen::Files
        | Screen::SourceControl => cx.new(|_| NeverUndocked).into(),
    }
}

/// The stand-in view for a screen kind that is never undocked
/// (`Screen::undockable` is false for it) — `build_screen_content` stays a
/// total match without inventing a real view for a path nothing takes.
struct NeverUndocked;

impl Render for NeverUndocked {
    fn render(&mut self, _window: &mut Window, _cx: &mut gpui::Context<Self>) -> impl IntoElement {
        div()
    }
}

/// One open tab: the detail screen it shows plus the LIST it was opened from
/// (EXP-288/EXP-851; EXP-1192: the second sidebar beside it). `None` =
/// opened from a board, the rail or a context-free page: nothing beside it.
///
/// EXP-1294: a chat and the sessions of its tree are ONE tab. `screen` is
/// what the tab shows right now (the chat, or a session opened from one of
/// its cards — Back returns to the chat), `root_id` the tree's root, so a
/// session never gets a tab of its own beside its chat. An agent's channel
/// is keyed the same way (its root is the agent).
#[derive(Clone)]
struct TabEntry {
    screen: Screen,
    origin: Option<TabOrigin>,
    root_id: Option<String>,
}

impl TabEntry {
    fn new(screen: Screen, origin: Option<TabOrigin>, root_id: Option<String>) -> Self {
        let root_id = root_id.or_else(|| match &screen {
            Screen::Chat { session_id } | Screen::Agent { agent_id: session_id } => {
                Some(session_id.clone())
            }
            _ => None,
        });
        Self {
            screen,
            origin,
            root_id,
        }
    }

    /// The issue an issue tab shows.
    fn issue_id(&self) -> Option<&str> {
        match &self.screen {
            Screen::IssueDetail { issue_id } => Some(issue_id),
            _ => None,
        }
    }

    /// EXP-923 — whether a BULK close (Close others / Close all) takes this
    /// tab. `keep` is the one tab Close others spares; `None` = Close all.
    /// The bottom bar's terminals are a different strip and never go (EXP-769).
    fn swept_by_bulk_close(&self, keep: Option<&Screen>) -> bool {
        !self.screen.is_dock_tab() && keep.is_none_or(|keep| &self.screen != keep)
    }

    /// Whether `screen` is this tab's — the screen on show, or (EXP-1294) the
    /// root it belongs to: its chat or its agent. A SESSION of the tree is
    /// matched by its root ([`Self::holds_root`]), which needs the synced row.
    fn holds(&self, screen: &Screen) -> bool {
        if &self.screen == screen {
            return true;
        }
        match screen {
            Screen::Chat { session_id } | Screen::Agent { agent_id: session_id } => {
                self.holds_root(session_id)
            }
            _ => false,
        }
    }

    /// EXP-1294: whether this tab belongs to the tree rooted at `root_id`.
    fn holds_root(&self, root_id: &str) -> bool {
        self.root_id.as_deref() == Some(root_id)
    }
}

/// EXP-1294: the tree root a chat / session / agent screen belongs to — the
/// screen's own id for a root, the row's `root_id` for a session (`None`
/// while that row has not synced, and for every other screen).
fn screen_root(screen: &Screen, cx: &App) -> Option<String> {
    match screen {
        Screen::Chat { session_id } => Some(session_id.clone()),
        // NEW-6: the caller's conversation with the agent (the agent id
        // until the first message creates it).
        Screen::Agent { agent_id } => Some(crate::agent_screen::agent_screen_root(agent_id, cx)),
        Screen::Session { session_id } => {
            crate::session_data::session(session_id, cx).map(|row| row.root_id_or_self().to_string())
        }
        _ => None,
    }
}

/// EXP-923 — the work the rail's Running section opened with NO tab: a
/// live chat (EXP-1294: the tree, so a session opened from its cards stays
/// tab-less too). It has to be remembered, not decided per navigation: Back
/// inside it carries no marker at all.
#[derive(Clone, Debug, PartialEq)]
struct TablessWork {
    root_id: String,
}

impl TablessWork {
    /// Whether a screen whose tree root is `root` is this work.
    fn holds(&self, root: Option<&str>) -> bool {
        root == Some(self.root_id.as_str())
    }
}

/// EXP-923 — whether a navigation opens NO tab. Two ways in: the rail's
/// Running section asked for it ([`PendingOrigin::LiveRail`]), or the window
/// is on that tab-less work and this is a step INSIDE its tree
/// (`step_inside`: a session card's plain navigation, or a Back with no
/// marker at all). Either way an EXISTING tab for the same work wins: it is
/// reused and goes active, so one piece of work is one chip and never two.
///
/// Pure, because the halves come from opposite ends of `sync_tabs` and
/// getting any of them wrong is silent: miss the marker and every rail click
/// grows the strip; miss `existing` and clicking a chat that is open
/// leaves the chip behind unhighlighted; miss `step_inside` and a session
/// card inside it grows the chip the rail click did not.
fn opens_no_tab(
    pending: Option<&PendingOrigin>,
    existing: Option<usize>,
    step_inside: bool,
) -> bool {
    existing.is_none()
        && match pending {
            Some(PendingOrigin::LiveRail) => true,
            // A step from inside the tree (a card's plain navigation) or a
            // Back / a tab activation (no marker).
            None | Some(PendingOrigin::Derive) => step_inside,
            // An explicit list, the rail or a Stack pick opens its tab.
            Some(_) => false,
        }
}

/// EXP-1294 — the strip's "+": a ghost icon button after the last chip (and
/// after "+N") that opens Home ([`open_home`]).
pub(crate) fn home_button(id: impl Into<gpui::ElementId>) -> Button {
    Button::new(id)
        .ghost()
        .cursor_pointer()
        .web_icon_xs()
        .icon(registry::UI_ADD)
        .tooltip("New session")
        .on_click(|_: &ClickEvent, window, cx| {
            cx.stop_propagation();
            open_home(window, cx);
        })
}

/// EXP-1255 — a chip's ×, shown on HOVER only, on every chip including the
/// active one (Cmd+W and the middle-click still close). `invisible` keeps
/// its slot, so the measured width never jumps. `close` is the bare button
/// with its click wired; the chip must be in the [`TAB_GROUP`] group.
pub(crate) fn hover_close(close: Button) -> gpui::Div {
    div()
        .invisible()
        .group_hover(TAB_GROUP, |style| style.visible())
        .child(close.ghost().web_icon_xs().icon(registry::UI_CLOSE))
}

/// EXP-1250 — swap the strip to another team's tabs, never dropping any: the
/// leaving team's non-terminal tabs are PARKED under its id, the arriving
/// team's parked tabs come back in their order, and terminals (EXP-769: a PTY
/// is not team-scoped) stay in the strip throughout. Pure.
/// `from_alive = false` = the leaving team's row is GONE (left or deleted,
/// [`crate::navigation::tabs_team_id`] fell back): its tabs are dropped, not
/// parked — nothing could ever switch back to it.
fn swap_team_tabs(
    tabs: Vec<TabEntry>,
    parked: &mut HashMap<String, Vec<TabEntry>>,
    from: Option<&str>,
    from_alive: bool,
    to: Option<&str>,
) -> Vec<TabEntry> {
    let (terminals, team_tabs): (Vec<TabEntry>, Vec<TabEntry>) = tabs
        .into_iter()
        .partition(|tab| matches!(tab.screen, Screen::Terminal { .. }));
    if let Some(from) = from {
        if team_tabs.is_empty() || !from_alive {
            parked.remove(from);
        } else {
            parked.insert(from.to_string(), team_tabs);
        }
    }
    let mut next = to.and_then(|to| parked.remove(to)).unwrap_or_default();
    next.extend(terminals);
    next
}

/// EXP-1250 — a navigation lands on a tab that ALREADY holds the work: a real
/// navigation refreshes the list (latest wins). Pure.
fn focus_existing_tab(tab: &mut TabEntry, pending: Option<&PendingOrigin>, derived: Option<TabOrigin>) {
    tab.origin = resolve_tab_origin(pending, tab.origin.as_ref(), derived);
}

/// EXP-851: which list a tab ends up carrying. `pending` is the marker the
/// navigation left (`None` = not a navigation at all: a tab click, a go-back,
/// a close-reactivation — the tab keeps what it has), `existing` the tab's
/// current list, `derived` the breadcrumb rule's answer. Pure, so the four
/// cases are a unit test.
///
/// EXP-862: a `Derive` that derives NOTHING keeps the origin the tab already
/// has. Deriving nothing means "this click named no list", not "this tab has
/// no list": a row clicked in a list while a rail-opened detail is up used to
/// blank the list mid-click. Only [`PendingOrigin::Rail`] clears an origin,
/// because the rail (and, EXP-1192, a board row) really is the answer there.
fn resolve_tab_origin(
    pending: Option<&PendingOrigin>,
    existing: Option<&TabOrigin>,
    derived: Option<TabOrigin>,
) -> Option<TabOrigin> {
    match pending {
        None => existing.cloned(),
        Some(PendingOrigin::Explicit(origin)) => Some(origin.clone()),
        Some(PendingOrigin::Derive) => {
            derived.or_else(|| existing.cloned())
        }
        // EXP-923: the rail's Running rows lend no list either.
        Some(PendingOrigin::Rail) | Some(PendingOrigin::LiveRail) => None,
    }
}

/// EXP-769: what a session-bar chip's × does (web `DockTab` semantics).
#[derive(Clone)]
enum DockClose {
    /// A terminal: close it (the child is killed) — the retired dock's cmd-w.
    CloseTerminal(terminal::TabId),
    /// Anything else: just close the tab.
    CloseTab(Screen),
}


// EXP-288: which tabs get a chip, in strip order — the rest collapse into
// the trailing "+N" dropdown (also the terminal dock, EXP-497, and the chat
// strip, EXP-1334); it lives in the domain crate now. `widths`, `gap` and
// `overflow_w` are measured against the window (EXP-326, see
// [`ScreensPanel::measure_chip_width`]), so "fits" means fits.

/// Chip content for one tab (EXP-310): issue-backed tabs lead with the
/// colored status icon and the identifier shortcode. A blank issue title
/// drops the title part — the shortcode already labels the chip, where
/// `screen_title`'s identifier fallback would render it twice. Non-issue tabs
/// (and issue rows not yet synced) keep the plain `screen_title`.
///
/// EXP-769: the bottom bar's terminal chips are built from the same struct —
/// a terminal chip adds its exit-code badge.
///
/// EXP-877: no ` · machine` caption and no paused dimming. A tab is chrome.
struct ChipContent {
    lead: ChipLead,
    identifier: Option<gpui::SharedString>,
    title: Option<gpui::SharedString>,
    /// EXP-769: a tinted exit-code badge — terminal chips whose child exited.
    badge: Option<(gpui::SharedString, gpui::Hsla)>,
}

impl ChipContent {
    fn plain(title: gpui::SharedString) -> Self {
        Self {
            lead: ChipLead::None,
            identifier: None,
            title: Some(title),
            badge: None,
        }
    }
}

/// #EXP-1726: one tab of the bottom tab bar.
pub(crate) struct TabItem {
    pub(crate) screen: Screen,
    pub(crate) title: String,
    pub(crate) status: crate::surface::RichTabStatus,
}

/// A chip's leading glyph (EXP-426). Cloneable — the overflow dropdown
/// collects entries up front and builds `Icon`s (not `Clone` at the pinned
/// gpui-component rev) only at menu-build time.
#[derive(Clone)]
enum ChipLead {
    None,
    /// EXP-314: the issue's RESOLVED status (custom rows included).
    /// Resolution is per-issue, so it stays correct on this cross-team strip
    /// — only GROUPING is team-scoped.
    Status(domain::statuses::ResolvedStatus),
    /// EXP-746: a steady tone dot — a session chip whose row has not synced.
    Dot(gpui::Hsla),
    /// EXP-1294: a tinted concept glyph — a chat / session chip's live
    /// state mark (`fixtures/session-card.json` icon + tone of the root's
    /// state) and an agent chip's own icon in its colour.
    Tinted(ExpIcon, gpui::Hsla),
    /// EXP-769: the `session-shell` glyph — a plain terminal chip (EXP-723: a
    /// chip carrying only a title read as a nameless tab next to the issue
    /// chips' status glyphs).
    Shell,
}

impl ChipLead {
    /// The GLYPH lead, when the chip has one. A [`ChipLead::Dot`] is painted
    /// by `surface::rich_tab` itself and has no icon.
    fn icon(&self, cx: &App) -> Option<gpui_component::Icon> {
        match self {
            ChipLead::None | ChipLead::Dot(_) => None,
            ChipLead::Status(status) => Some(crate::icons::resolved_status_icon(status, cx)),
            ChipLead::Tinted(icon, tint) => Some(Icon::new(icon.clone()).text_color(*tint)),
            ChipLead::Shell => Some(Icon::new(registry::SESSION_SHELL)),
        }
    }
}

fn rich_tab_status(lead: &ChipLead, cx: &App) -> crate::surface::RichTabStatus {
    match lead {
        ChipLead::Dot(tone) => crate::surface::RichTabStatus::Dot(*tone),
        lead => match lead.icon(cx) {
            Some(icon) => crate::surface::RichTabStatus::Glyph(icon),
            None => crate::surface::RichTabStatus::None,
        },
    }
}

/// D8: a chat / session / agent chip whose row has not synced yet.
fn loading_chip(cx: &App) -> ChipContent {
    ChipContent {
        lead: ChipLead::Dot(cx.theme().muted_foreground.opacity(0.5)),
        identifier: None,
        title: None,
        badge: None,
    }
}

/// EXP-1294: the live state mark of a tree — the fixture icon of its root's
/// state ([`domain::session_tree::session_state`]), tinted by its tone.
fn state_lead(root: &domain::rows::SessionRow, cx: &App) -> ChipLead {
    state_lead_of(domain::session_tree::session_state(root), cx)
}

/// A chat / session chip's lead for `state` (`session-card.json` icon +
/// tone).
fn state_lead_of(state: domain::session_tree::SessionState, cx: &App) -> ChipLead {
    ChipLead::Tinted(
        registry::concept_by_name(state.icon()).unwrap_or(registry::NAV_CHATS),
        crate::work_card::session_tone_color(state.tone(), cx),
    )
}

/// EXP-1294 — a chat tab's chip: the root's live state mark and the
/// chat's title (`domain::session_title`, EXP-1341).
fn chat_chip_content(root_id: &str, cx: &App) -> ChipContent {
    let Some(root) = crate::session_data::session(root_id, cx) else {
        return loading_chip(cx);
    };
    ChipContent {
        lead: state_lead(&root, cx),
        identifier: None,
        title: Some(screen_title(
            &Screen::Chat {
                session_id: root_id.to_string(),
            },
            cx,
        )),
        badge: None,
    }
}

/// EXP-1294 — a session chip (the tab of its chat, showing the session):
/// the root's state mark and the tab's title ([`screen_title`]: the
/// chat's, EXP-1676, like web's chat tab).
fn session_chip_content(session_id: &str, cx: &App) -> ChipContent {
    let Some(root) = crate::session_data::root_of(session_id, cx) else {
        return loading_chip(cx);
    };
    ChipContent {
        lead: state_lead(&root, cx),
        identifier: None,
        title: Some(screen_title(
            &Screen::Session {
                session_id: session_id.to_string(),
            },
            cx,
        )),
        badge: None,
    }
}

/// EXP-1294 — an agent page's chip: the `agents` row's icon in its colour
/// and its name.
fn agent_chip_content(agent_id: &str, cx: &App) -> ChipContent {
    let resolved = crate::agent_screen::resolve_agent_id(agent_id, cx);
    let Some(agent) = crate::session_data::agent_def(&resolved, cx) else {
        return loading_chip(cx);
    };
    ChipContent {
        lead: ChipLead::Tinted(
            crate::icons::agent_glyph(agent.icon.as_deref()),
            crate::icons::agent_tint(agent.color.as_deref()),
        ),
        identifier: None,
        title: Some(screen_title(
            &Screen::Agent {
                agent_id: agent_id.to_string(),
            },
            cx,
        )),
        badge: None,
    }
}

/// EXP-769: a terminal chip — the retired dock strip's local chip. EXP-773
/// left terminals with no coding runs in them, so this is always the plain
/// treatment: the terminal glyph and the tab's own title, with an exited
/// child's code as a badge.
fn terminal_chip_content(tab: terminal::TabId, cx: &App) -> ChipContent {
    let Some(manager) = crate::session_bar::manager_for_tab(tab, cx) else {
        return ChipContent {
            lead: ChipLead::Shell,
            ..ChipContent::plain("Terminal".into())
        };
    };
    let manager = manager.read(cx);
    let Some(entry) = manager.tab(tab) else {
        return ChipContent {
            lead: ChipLead::Shell,
            ..ChipContent::plain("Terminal".into())
        };
    };
    let badge = entry.exit_code().map(|code| {
        let color = if code == 0 {
            cx.theme().success
        } else {
            cx.theme().danger
        };
        (gpui::SharedString::from(code.to_string()), color)
    });
    // EXP-773: a terminal tab is a plain shell or an agent login — a coding
    // run has its own screen, so no issue chip can resolve from a tab.
    let mut content = ChipContent {
        lead: ChipLead::Shell,
        ..ChipContent::plain(entry.title().clone())
    };
    content.badge = badge;
    content
}

fn chip_content(screen: &Screen, cx: &App) -> ChipContent {
    match screen {
        Screen::Session { session_id } => return session_chip_content(session_id, cx),
        Screen::Chat { session_id } => return chat_chip_content(session_id, cx),
        Screen::Agent { agent_id } => return agent_chip_content(agent_id, cx),
        Screen::Terminal { tab } => return terminal_chip_content(*tab, cx),
        _ => {}
    }
    if let Screen::IssueDetail { issue_id } = screen {
        let store = Store::global(cx);
        let issues = store.collections().issues.read(cx);
        if let Some(issue) = issues.get(issue_id) {
            let title = issue.title.trim();
            let title = if title.is_empty() {
                None
            } else {
                Some(gpui::SharedString::from(title.to_string()))
            };
            let resolved = crate::queries::resolve_issue_status(cx, issue);
            return ChipContent {
                lead: ChipLead::Status(resolved),
                identifier: Some(gpui::SharedString::from(issue.identifier.clone())),
                title,
                badge: None,
            };
        }
    }
    ChipContent::plain(screen_title(screen, cx))
}

/// Which tab the center falls back to when the ACTIVE tab at `ix` closes.
///
/// `strip[i]` is tab `i`'s [`Screen::is_dock_tab`], and `dock` is the closing
/// tab's own — the search stays strictly inside that ONE strip: the next tab
/// to the right, else back to the left. Closing a bottom-bar session tab
/// lands on the next session, closing a top tab on the next top tab.
///
/// EXP-781: `None` when the strip is now empty, which [`set_screen`] reads as
/// "clear the center". This used to fall back to any remaining tab, so
/// closing the LAST session tab activated an unrelated issue.
///
/// `ix` is the index the closed tab occupied, i.e. `strip` is already the
/// post-removal list and `strip[ix]` is the tab that shifted into its place.
fn neighbor_in_strip(strip: &[bool], ix: usize, dock: bool) -> Option<usize> {
    (ix..strip.len())
        .chain((0..ix).rev())
        .find(|&candidate| strip[candidate] == dock)
}

pub struct ScreensPanel {
    focus_handle: FocusHandle,
    nav: Entity<Navigation>,
    issue_detail: Entity<IssueDetailView>,
    settings: Entity<crate::settings::SettingsView>,
    source_control: Entity<crate::source_control::SourceControlView>,
    file_viewer: Entity<crate::file_viewer::FileViewerView>,
    /// The Devices page (EXP-686 — the user's machines; the same tab-less
    /// full-page mode).
    devices: Entity<crate::devices_view::DevicesView>,
    /// The Drafts page (EXP-878 — the New issue pages left with content in
    /// them; the same tab-less full-page mode).
    drafts: Entity<crate::drafts_view::DraftsView>,
    needs_you: Entity<crate::needs_you::NeedsYouView>,
    /// EXP-1170: the New issue page — ONE shared view re-pointed per draft
    /// like the PR diff ([`Self::sync_issue_draft`]); a navigation off it
    /// pays the draft out.
    issue_draft: Entity<crate::issue_draft_screen::IssueDraftView>,
    /// HOME (EXP-772's Chat page — the centred prompt box; the same tab-less
    /// full-page mode).
    chat: Entity<crate::chat_screen::ChatScreenView>,
    /// EXP-1294: the Chats page (the same tab-less full-page mode).
    chats_page: Entity<crate::chats_view::ChatsView>,
    /// VAPP-90: the Exponential UI kitchen sink, built on first use — a
    /// dev-only screen pays its surface build only when opened.
    eui_core: Option<Entity<crate::exponential_ui_screen::ExponentialUiKitchenSink>>,
    /// VAPP-110: the open app editor, built on entry and DROPPED on every
    /// navigation off it (which ends the member's live editor session).
    app_editor: Option<Entity<crate::app_screen::AppScreenView>>,
    /// VAPP-93: the open specimen (rebuilt when another id opens).
    exponential_ui_specimen: Option<Entity<crate::exponential_ui_screen::ExponentialUiSpecimen>>,
    /// EXP-746: one session screen per session a tab SHOWS, keyed by the
    /// `sessions` row id. Not a shared single instance like the views above:
    /// each one owns a feed (a relay socket, or the local engine's drain), so
    /// it is created on first show and shut down when its tab moves off it
    /// (EXP-1294: Back to the chat) or closes.
    sessions: HashMap<String, Entity<crate::session_screen::SessionScreenView>>,
    /// EXP-1294: one chat screen per OPEN chat tab, keyed by the root
    /// session id — kept while the tab shows one of the tree's sessions, so
    /// Back lands on the same view (its reply draft, its scroll).
    chats: HashMap<String, Entity<crate::session_chat::SessionChatScreenView>>,
    /// EXP-1294: one agent channel per OPEN agent tab, keyed by the agent's
    /// root session id.
    agents: HashMap<String, Entity<crate::agent_screen::AgentScreenView>>,
    /// #EXP-1493: the plan page a tab SHOWS, keyed by `(session, message)`;
    /// dropped when its tab moves off it or closes (like a session view).
    /// EXP-851: the board list screen's view, full width.
    list: Entity<ListPanel>,
    /// EXP-851: the Source Control screen's commit history (it lived on the
    /// retired tool column). Its diff is [`Self::source_control`].
    history: Entity<crate::source_control::HistoryList>,
    /// The window's shared rail state (EXP-288): the file selection
    /// re-points the viewer, and the SC selection the diff.
    rail: Entity<RailShared>,
    /// Open tabs in strip order — detail screens only, deduped by `screen`
    /// and (EXP-1294) by tree root (re-opening focuses + refreshes the
    /// origin).
    tabs: Vec<TabEntry>,
    /// EXP-923: the live chat this window is showing WITHOUT a tab — opened
    /// from the rail's Running section ([`TablessWork`]).
    tabless: Option<TablessWork>,
    /// The team the strip's tabs belong to ([`crate::navigation::tabs_team_id`]).
    tabs_team: Option<String>,
    /// EXP-1250: the OTHER teams' tabs, parked on a team switch and restored
    /// on the way back ([`swap_team_tabs`]) — a switch never drops a tab.
    parked_tabs: HashMap<String, Vec<TabEntry>>,
    /// The screen shown at the last nav notify (EXP-369): the panes are
    /// long-lived, so a transition INTO one is the only "opened" signal a
    /// pane that fetches server-only data gets.
    active_screen: Option<Screen>,
    /// EXP-492/EXP-499: the panel's painted slot width, recorded each
    /// prepaint (the editor's EXP-421/436 recipe). The center content gets
    /// this as a DEFINITE pixel width, because between real layout frames
    /// gpui runs passes that resolve the panel subtree at fit-content —
    /// percent chains collapse (the Actions page's machines rows shrink-wrap
    /// and its card wrap-grid stops wrapping, running off the right edge).
    /// One frame stale during a live resize; 0.0 before the first paint
    /// falls back to stretch.
    slot_width: std::rc::Rc<std::cell::Cell<f32>>,
    /// EXP-1156: the live edge drag of the Files / Source Control list
    /// column or (EXP-1192) the second sidebar — lists INSIDE the card, so
    /// this panel hosts them, not the Shell ([`crate::resize_edge`]).
    resize_drag: Option<crate::resize_edge::ResizeDrag>,
    /// EXP-698 round 5: the "No boards yet" state scrolls — it carries the
    /// Getting-started cards under it.
    empty_scroll: gpui::ScrollHandle,
    _subscriptions: Vec<Subscription>,
}

/// EXP-1259 — `EXP_DEV_COMPOSER_MENU=implement-issue` opens Home's "+"
/// menu with its Implement issue submenu open, once
/// ([`crate::controls::PointerMenu::dev_open`]): true for the FIRST caller
/// only, so the menu still closes like any other after. N-6:
/// `dialog-implement-issue` is the same for the composer DIALOG's "+"
/// (`dialog` = the caller is the dialog presentation), whose menu opens in
/// a menu window over the dialog (`composer-dialog-menu`).
///
/// EXP-1551: `temporary` is Home's "+" at its ROOT, no row lit, with the
/// agent row on Temporary session ([`dev_composer_menu_temporary`]): the
/// state every client photographs for `composer-menu`.
///
/// #EXP-1749: the open also waits for the "+" (`trigger`, window bounds) to
/// hold still for [`DEV_MENU_STEADY`]: the page settles after its data is in
/// (rows landing above the composer move it), and #EXP-1739 closes a menu
/// whose anchor moved, so an early open photographed closed. `view` is
/// re-rendered once the wait may be over.
pub(crate) fn take_dev_composer_menu(
    dialog: bool,
    trigger: gpui::Bounds<gpui::Pixels>,
    view: gpui::EntityId,
    cx: &mut App,
) -> bool {
    use std::sync::atomic::{AtomicBool, Ordering};
    static TAKEN: AtomicBool = AtomicBool::new(false);
    let value = std::env::var("EXP_DEV_COMPOSER_MENU").ok();
    dev_composer_menu_wanted(value.as_deref(), dialog)
        && !TAKEN.load(Ordering::SeqCst)
        && dev_trigger_steady(trigger, view, cx)
        && !TAKEN.swap(true, Ordering::SeqCst)
}

/// #EXP-1749: how long a dev-opened menu's trigger holds still before it opens.
const DEV_MENU_STEADY: std::time::Duration = std::time::Duration::from_millis(1000);

/// #EXP-1749: whether `trigger` has sat where it is for [`DEV_MENU_STEADY`];
/// a move restarts the wait and re-renders `view` when it ends.
fn dev_trigger_steady(trigger: gpui::Bounds<gpui::Pixels>, view: gpui::EntityId, cx: &mut App) -> bool {
    thread_local! {
        static SINCE: std::cell::Cell<Option<(gpui::Bounds<gpui::Pixels>, std::time::Instant)>> =
            const { std::cell::Cell::new(None) };
    }
    match SINCE.get() {
        Some((at, since)) if at == trigger => since.elapsed() >= DEV_MENU_STEADY,
        _ => {
            SINCE.set(Some((trigger, std::time::Instant::now())));
            cx.spawn(async move |cx| {
                cx.background_executor().timer(DEV_MENU_STEADY).await;
                let _ = cx.update(|cx| cx.notify(view));
            })
            .detach();
            false
        }
    }
}

/// Whether `EXP_DEV_COMPOSER_MENU=value` opens the "+" of a page
/// (`dialog` false) or a dialog composer.
fn dev_composer_menu_wanted(value: Option<&str>, dialog: bool) -> bool {
    match value {
        Some("dialog-implement-issue") => dialog,
        Some("implement-issue" | "temporary" | "teams") => !dialog,
        _ => false,
    }
}

/// EXP-1551 — `EXP_DEV_COMPOSER_MENU=temporary`: the Home page starts its
/// agent row on Temporary session and its dev-opened "+" menu selects no
/// row. Read once.
pub(crate) fn dev_composer_menu_temporary() -> bool {
    static TEMPORARY: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *TEMPORARY.get_or_init(|| {
        matches!(std::env::var("EXP_DEV_COMPOSER_MENU").ok().as_deref(), Some("temporary" | "skills"))
    })
}

/// #EXP-1715 — `EXP_DEV_COMPOSER_MENU=teams`: Home's "+" opens once with
/// Teams › open (the agent row on the viewer's Assistant, the default), the
/// `composer-teams-menu` capture. Read once.
pub(crate) fn dev_composer_menu_teams() -> bool {
    static TEAMS: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *TEAMS.get_or_init(|| std::env::var("EXP_DEV_COMPOSER_MENU").ok().as_deref() == Some("teams"))
}

/// #EXP-1660 — `EXP_DEV_COMPOSER_MENU=skills`: Home starts on Temporary
/// session and opens "+" › Skills (THE skills picker, anchored to the +)
/// once, the `composer-skills-menu` capture. Taken once.
pub(crate) fn take_dev_skills_picker() -> bool {
    use std::sync::atomic::{AtomicBool, Ordering};
    static TAKEN: AtomicBool = AtomicBool::new(false);
    std::env::var("EXP_DEV_COMPOSER_MENU").ok().as_deref() == Some("skills") && !TAKEN.swap(true, Ordering::SeqCst)
}

/// EXP-1557 — `EXP_DEV_COMPOSER_TEXT=<text>` types `<text>` into Home's
/// composer once (the first page composer with a team and synced shapes):
/// `Explain #APP` opens the `#` search list at the caret, prefilled with
/// `APP` (web's `typeIssueRef`, the `search-list` capture).
pub(crate) fn take_dev_composer_text() -> Option<String> {
    use std::sync::atomic::{AtomicBool, Ordering};
    static TAKEN: AtomicBool = AtomicBool::new(false);
    let text = std::env::var("EXP_DEV_COMPOSER_TEXT").ok().filter(|text| !text.is_empty())?;
    (!TAKEN.swap(true, Ordering::SeqCst)).then_some(text)
}

/// EXP-1649 — `EXP_DEV_COMPOSER_ISSUES=<issue-uuid>[,…]` seeds Home's
/// composer INLINE with these issues once (the first page composer with a
/// team and synced shapes): web's `/agent?issues=` deep link, the
/// `chat-issues` capture (a ▶ seed opens the DIALOG instead, EXP-1037).
pub(crate) fn take_dev_composer_issues() -> Option<Vec<String>> {
    use std::sync::atomic::{AtomicBool, Ordering};
    static TAKEN: AtomicBool = AtomicBool::new(false);
    let ids: Vec<String> = std::env::var("EXP_DEV_COMPOSER_ISSUES")
        .ok()?
        .split(',')
        .map(str::trim)
        .filter(|id| !id.is_empty())
        .map(str::to_string)
        .collect();
    (!ids.is_empty() && !TAKEN.swap(true, Ordering::SeqCst)).then_some(ids)
}

/// EXP-1558 — `EXP_DEV_TYPEAHEAD_QUERY=<text>` types `<text>` into the
/// first `#` search list's own input once it opens: with
/// `EXP_DEV_COMPOSER_TEXT=Explain #` the composer keeps `Explain #` and the
/// list's field reads `APP`, web's `typeIssueRef` state.
pub(crate) fn take_dev_typeahead_query() -> Option<String> {
    use std::sync::atomic::{AtomicBool, Ordering};
    static TAKEN: AtomicBool = AtomicBool::new(false);
    let query = std::env::var("EXP_DEV_TYPEAHEAD_QUERY").ok().filter(|query| !query.is_empty())?;
    (!TAKEN.swap(true, Ordering::SeqCst)).then_some(query)
}

/// #EXP-1535 — `EXP_DEV_COMPOSER_MENU=session-account` opens a session
/// composer's "+" menu with its Account submenu open, once (the first
/// session view whose menu has the Account row).
pub(crate) fn take_dev_session_account_menu() -> bool {
    use std::sync::atomic::{AtomicBool, Ordering};
    static TAKEN: AtomicBool = AtomicBool::new(false);
    std::env::var("EXP_DEV_COMPOSER_MENU").ok().as_deref() == Some("session-account")
        && !TAKEN.swap(true, Ordering::SeqCst)
}

/// EXP-1259 — `EXP_DEV_SCROLL_TO=script-card` parks a session's transcript
/// on its first code-mode Script card instead of the tail, so the
/// `script-card` capture has it in frame; EXP-1324 `work-card` parks it on
/// the first workflow's Work card the same way; #EXP-1553 `comment-call-card`
/// parks it on the first settled `exponential_comments_create` card.
/// EXP-1558 `end` parks the issue page and the settings page at their end
/// ([`dev_park_at_end`]): `issue-comments` and `settings-assistant-meeting`.
/// Read once: the transcript asks every frame.
pub(crate) fn dev_scroll_to() -> Option<&'static str> {
    static TARGET: std::sync::OnceLock<Option<&'static str>> = std::sync::OnceLock::new();
    *TARGET.get_or_init(|| match std::env::var("EXP_DEV_SCROLL_TO").ok().as_deref() {
        Some("script-card") => Some("script-card"),
        Some("work-card") => Some("work-card"),
        Some("comment-call-card") => Some("comment-call-card"),
        Some("end") => Some("end"),
        _ => None,
    })
}

/// EXP-1558 (DEV-ONLY, `EXP_DEV_SCROLL_TO=end`): park `handle`'s scroller at
/// its end, re-applied while its content still grows (the activity feed,
/// a pane's rows landing after sync) or a host reset it to the top (a
/// settings section resolving to another pane), never otherwise, so a
/// wheel is not fought. `parked` is the caller's last applied max offset. Keeps
/// frames coming while it waits: a layout that grew after the last render
/// would otherwise never be seen.
pub(crate) fn dev_park_at_end(
    handle: &gpui::ScrollHandle,
    parked: &std::cell::Cell<gpui::Pixels>,
    window: &mut Window,
) {
    if dev_scroll_to() != Some("end") {
        return;
    }
    let max = handle.max_offset().y;
    let reset_to_top = max > gpui::px(0.) && handle.offset().y == gpui::px(0.);
    if max > parked.get() || reset_to_top {
        parked.set(max);
        handle.scroll_to_bottom();
    }
    window.request_animation_frame();
}

impl ScreensPanel {
    pub fn new(window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        // Full-page issue detail (§4.2): one instance, re-pointed on
        // navigation (its local edit state resets per issue, web parity).
        let issue_detail = cx.new(|cx| IssueDetailView::new(window, cx));
        let settings = cx.new(|cx| crate::settings::SettingsView::new(window, cx));
        let source_control = cx.new(|cx| crate::source_control::SourceControlView::new(window, cx));
        let file_viewer = cx.new(|cx| crate::file_viewer::FileViewerView::new(window, cx));
        let devices = cx.new(|cx| crate::devices_view::DevicesView::new(window, cx));
        let drafts = cx.new(|cx| crate::drafts_view::DraftsView::new(window, cx));
        let needs_you = cx.new(|cx| crate::needs_you::NeedsYouView::new(window, cx));
        let issue_draft =
            cx.new(|cx| crate::issue_draft_screen::IssueDraftView::new(window, cx));
        let chat = cx.new(|cx| crate::chat_screen::ChatScreenView::new(window, cx));
        let chats_page = cx.new(|cx| crate::chats_view::ChatsView::new(window, cx));
        let nav = nav_for_window(window, cx);
        let rail = rail_shared_for_window(window, cx);
        let list = cx.new(|cx| ListPanel::new(window, cx));
        let history = cx.new(|cx| crate::source_control::HistoryList::new(window, cx));

        let mut subscriptions = Vec::new();
        // Navigation changes open/focus tabs and retarget the shared views
        // (needs `window` for the detail's input resets, hence `observe_in`).
        subscriptions.push(cx.observe_in(&nav, window, |this, _, window, cx| {
            this.sync_tabs(window, cx);
            this.sync_active_screen(cx);
            // EXP-1170: point the New issue page at its draft, or pay the
            // draft out when the navigation left it.
            this.sync_issue_draft(window, cx);
            // VAPP-110: the app editor lives exactly as long as its screen.
            this.sync_app_editor(window, cx);
            cx.notify();
        }));
        // EXP-288: the rail drives the tab-less center — tool switches swap
        // the default content, and the Files selection re-points the viewer.
        // Loop-safe: this only updates the leaf viewer entity.
        subscriptions.push(cx.observe(&rail, |this, _, cx| {
            this.sync_file_viewer(cx);
            cx.notify();
        }));
        let collections = Store::global(cx).collections().clone();
        subscriptions.push(cx.observe_in(
            &collections.teams,
            window,
            |this, _, window, cx| {
                this.sync_tabs(window, cx);
                cx.notify();
            },
        ));
        // Tab titles join issue identifiers live; a deleted issue's tabs
        // close instead of lingering as "not found" (EXP-493).
        subscriptions.push(cx.observe_in(
            &collections.issues,
            window,
            |this, _, window, cx| {
                this.prune_missing_issue_tabs(window, cx);
                cx.notify();
            },
        ));
        subscriptions.push(cx.observe(&collections.boards, |_, _, cx| cx.notify()));
        // EXP-1294: the chat / session / agent chips read their state mark
        // and title off the session collections.
        subscriptions.extend(crate::session_data::observe_session_collections(cx));
        // EXP-698 round 5: the "No boards yet" center renders the
        // Getting-started cards — same reason the issue list observes it
        // (tRPC one-shot signals no collection echo covers).
        let gs_progress = crate::getting_started::GettingStartedProgress::global(cx);
        subscriptions.push(cx.observe(&gs_progress, |_, _, cx| cx.notify()));
        subscriptions.push(cx.observe_in(
            &Store::global(cx).state(),
            window,
            |this, _, window, cx| {
                this.sync_tabs(window, cx);
                // The session phase rides this entity: a screen pointed
                // during construction (before the phase reached Synced) never
                // got a tRPC client, so re-drive it here rather than waiting
                // for a navigation that may never come.
                this.redrive_active_screen(cx);
                cx.notify();
            },
        ));

        // EXP-277: publish this window's panel so the titlebar can render the
        // tab strip (insert overwrites — a rebuilt center wins).
        let panel_entity = cx.entity();
        cx.default_global::<ScreensRegistry>()
            .by_window
            .insert(window.window_handle().window_id(), panel_entity);

        let mut this = Self {
            focus_handle: cx.focus_handle(),
            empty_scroll: gpui::ScrollHandle::new(),
            nav,
            issue_detail,
            settings,
            source_control,
            file_viewer,
            devices,
            drafts,
            needs_you,
            issue_draft,
            chat,
            chats_page,
            eui_core: None,
            app_editor: None,
            exponential_ui_specimen: None,
            sessions: HashMap::new(),
            chats: HashMap::new(),
            agents: HashMap::new(),
            list,
            history,
            rail,
            tabs: Vec::new(),
            tabless: None,
            tabs_team: None,
            parked_tabs: HashMap::new(),
            active_screen: None,
            slot_width: std::rc::Rc::new(std::cell::Cell::new(0.0)),
            resize_drag: None,
            _subscriptions: subscriptions,
        };
        this.sync_tabs(window, cx);
        this.sync_active_screen(cx);
        this.sync_issue_draft(window, cx);
        this.sync_app_editor(window, cx);
        this.sync_file_viewer(cx);
        // EXP-1192: seed the slide from the CURRENT state, so a window that
        // opens straight onto the Inbox shows no entry animation (the shell
        // does the same for a window opening into Settings).
        this
    }

    /// EXP-1170: the New issue page follows the navigation — re-pointed at
    /// the draft a [`Screen::IssueDraft`] names, and LEFT (its draft paid
    /// out: saved, deleted when emptied, nothing when untouched) by every
    /// navigation off it, a team switch included (that clears the screen).
    fn sync_issue_draft(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        match resolved_screen(&self.nav, cx) {
            Some(Screen::IssueDraft {
                draft_id,
                board_id,
                status_id,
            }) => self.issue_draft.update(cx, |view, cx| {
                view.set_draft(draft_id, board_id, status_id, window, cx);
            }),
            _ => self.issue_draft.update(cx, |view, cx| view.leave(cx)),
        }
    }

    /// VAPP-110: the app editor follows the navigation — built when a
    /// [`Screen::App`] comes up (rebuilt for another app), and LEFT (the
    /// member's live editor session ended) and dropped by every navigation
    /// off it, a team switch included.
    fn sync_app_editor(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        match resolved_screen(&self.nav, cx) {
            // VAPP-242: a run view shares the slot (the same host pieces),
            // rebuilt when the mode flips.
            Some(screen @ (Screen::App { .. } | Screen::AppRun { .. })) => {
                let (app, run) = match screen {
                    Screen::AppRun { app } => (app, true),
                    Screen::App { app } => (app, false),
                    _ => unreachable!(),
                };
                let current = self
                    .app_editor
                    .as_ref()
                    .is_some_and(|view| view.read(cx).is_run() == run && view.read(cx).shows(&app));
                if !current {
                    if let Some(old) = self.app_editor.take() {
                        old.update(cx, |view, cx| view.leave(cx));
                    }
                    self.app_editor = Some(cx.new(|cx| {
                        if run {
                            crate::app_screen::AppScreenView::run(app, window, cx)
                        } else {
                            crate::app_screen::AppScreenView::new(app, window, cx)
                        }
                    }));
                }
            }
            _ => {
                if let Some(old) = self.app_editor.take() {
                    old.update(cx, |view, cx| view.leave(cx));
                }
            }
        }
    }

    /// EXP-369 (re-homed by EXP-238): the settings Personal panes hold
    /// server-only reads (email prefs, timezone, API keys) that would show
    /// the first visit's snapshot forever. Every transition INTO the
    /// settings screen marks them stale; each pane refetches on its next
    /// render.
    fn sync_active_screen(&mut self, cx: &mut gpui::Context<Self>) {
        self.sync_active_screen_inner(false, cx);
    }

    /// Re-drive the CURRENT screen even though it hasn't changed. This panel
    /// is built from the shell's constructor, which runs while the session is
    /// still validating — a tRPC-backed view pointed at that moment has no
    /// client, leaves itself unpointed, and would never be asked again
    /// (`sync_active_screen` early-returns on an unchanged screen). The
    /// session observer calls this on every phase flip so the pending screen
    /// finally loads once the phase reaches Synced.
    fn redrive_active_screen(&mut self, cx: &mut gpui::Context<Self>) {
        self.sync_active_screen_inner(true, cx);
    }

    fn sync_active_screen_inner(&mut self, force: bool, cx: &mut gpui::Context<Self>) {
        let screen = resolved_screen(&self.nav, cx);
        let changed = screen != self.active_screen;
        if !changed && !force {
            return;
        }
        if changed {
            let entered_settings = matches!(screen, Some(Screen::Settings));
            self.active_screen = screen;
            if entered_settings {
                self.settings
                    .update(cx, |settings, cx| settings.mark_personal_stale(cx));
            }
        }
    }

    /// Re-point the file viewer at the rail's file selection (EXP-288 —
    /// files are not tabs; the viewer is the Files tool's center content).
    fn sync_file_viewer(&mut self, cx: &mut gpui::Context<Self>) {
        let selected = self
            .rail
            .read(cx)
            .selected_file()
            .map(str::to_string);
        self.file_viewer.update(cx, |viewer, cx| match selected {
            Some(path) => viewer.set_path(path, cx),
            None => viewer.clear(cx),
        });
    }

    /// Reconcile tabs with the navigation state: swap the strip to the new
    /// team's tabs on a team switch (EXP-1250: parked, never dropped), open
    /// (or keep) a tab for the active DETAIL screen, and
    /// re-point the shared views at it. Runs in observers (never
    /// mid-render). MUST never call `activate_tool`/`select_*` — the rail
    /// observer + this nav observer would feed back.
    fn sync_tabs(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        // EXP-288: the origin marker rides every REAL navigation (tab clicks
        // never set it) — consumed HERE, unconditionally, so a marker left by
        // a navigation that never reached its tab can't survive to the next.
        let mut pending_origin = crate::navigation::take_pending_origin(&self.nav, cx);
        // EXP-1250: the window's CHOSEN team — `active_team_id`'s fallback
        // (its row still syncing) never counts as a switch.
        let team = crate::navigation::tabs_team_id(&self.nav, cx);
        if team != self.tabs_team {
            // EXP-1192: a window that opened on a detail BEFORE its team
            // synced (a dev-seeded launch) rebuilds that tab below — it keeps
            // the list it was seeded with, there was no other team to leave.
            if self.tabs_team.is_none() && pending_origin.is_none() {
                pending_origin = resolved_screen(&self.nav, cx)
                    .and_then(|screen| self.origin_of(&screen))
                    .map(PendingOrigin::Explicit);
            }
            // Parking the tabs tears the issue detail down without a blur —
            // flush a pending description edit first (EXP-68).
            self.issue_detail
                .update(cx, |detail, cx| detail.flush_description(cx));
            // EXP-1250: the leaving team's tabs are PARKED and the arriving
            // team's come back (EXP-769: terminals stay put throughout).
            let previous = self.tabs_team.take();
            let tabs = std::mem::take(&mut self.tabs);
            // The leaving team's row gone (left or deleted) = nothing to park
            // for; a row still syncing counts as alive.
            let previous_alive = previous.as_deref().is_some_and(|id| {
                let teams = Store::global(cx).collections().teams.read(cx);
                !teams.is_ready() || teams.get(id).is_some()
            });
            self.tabs = swap_team_tabs(
                tabs,
                &mut self.parked_tabs,
                previous.as_deref(),
                previous_alive,
                team.as_deref(),
            );
            self.tabs_team = team;
            self.tabless = None;
            // EXP-746: the per-tab views go (a parked tab must not keep a
            // relay socket or an engine drain alive); they rebuild lazily
            // when their tab is activated again.
            self.shutdown_all_tab_views(cx);
            // EXP-894/EXP-1250: the per-tab drafts stay stashed: their tabs
            // are parked, not closed, and come back with them.
            // The sidebar selections are team-scoped too (trunk-relative
            // paths / commit hashes of the OLD team's clone).
            self.rail.update(cx, |rail, cx| {
                rail.clear_selected_file(cx);
                rail.clear_sc_selection(cx);
            });
        }
        // #FEED-191 (plan-page.json `kept`): every chat the pane no longer
        // shows was LEFT: its decided plan leaves the strip on the next visit.
        let here = resolved_screen(&self.nav, cx).and_then(|screen| screen_root(&screen, cx));
        self.leave_other_chats(here.as_deref(), cx);
        let Some(screen) = resolved_screen(&self.nav, cx) else {
            return;
        };
        // EXP-851: only a detail gets a tab — a list screen and every full
        // page own none (EXP-1192: and carry no list either).
        if !screen.is_detail() {
            return;
        }
        // Only an explicit marker names a list (#FEED-191: no list lends one).
        let previous = self.nav.read(cx).previous_screen().cloned();
        let derived = None;
        // EXP-1294: a session lands on its ROOT's tab (one tab per tree), so
        // the lookup is by screen and then by tree root.
        let root = screen_root(&screen, cx);
        let existing = self.tabs.iter().position(|tab| tab.holds(&screen)).or_else(|| {
            root.as_deref()
                .and_then(|root| self.tabs.iter().position(|tab| tab.holds_root(root)))
        });
        // EXP-923: a live chat opened from the rail's Running section gets
        // NO tab of its own, and that decision outlives the click: a step
        // INSIDE the tree (a session card, Back to the chat) stays
        // tab-less. An EXISTING tab for the same work is still reused and
        // still goes active.
        let in_tabless = |root: Option<&str>| {
            self.tabless.as_ref().is_some_and(|work| work.holds(root))
        };
        let previous_root = previous.as_ref().and_then(|previous| screen_root(previous, cx));
        let step_inside = in_tabless(root.as_deref())
            && (pending_origin.is_none() || in_tabless(previous_root.as_deref()));
        if let Some(PendingOrigin::LiveRail) = pending_origin {
            self.tabless = existing
                .is_none()
                .then(|| root.clone().map(|root_id| TablessWork { root_id }))
                .flatten();
        } else if pending_origin.is_some() && !step_inside {
            // A real navigation somewhere else ends it.
            self.tabless = None;
        }
        let opens_no_tab = opens_no_tab(pending_origin.as_ref(), existing, step_inside);
        match existing {
            Some(ix) => {
                // Dedupe keeps ONE tab; a real re-navigation refreshes its
                // list (LATEST wins), a plain activation or a list step
                // keeps it (EXP-1250).
                focus_existing_tab(&mut self.tabs[ix], pending_origin.as_ref(), derived);
                if self.tabs[ix].root_id.is_none() {
                    self.tabs[ix].root_id = root.clone();
                }
                // EXP-1294: a session opened from its chat's card (or Back
                // to the chat) rebinds the SAME tab.
                self.show_in_tab(ix, screen.clone(), cx);
            }
            None if opens_no_tab => {}
            None => {
                self.tabs.push(TabEntry::new(
                    screen.clone(),
                    resolve_tab_origin(pending_origin.as_ref(), None, derived),
                    root.clone(),
                ));
            }
        }
        match screen {
            Screen::IssueDetail { issue_id } => {
                self.issue_detail.update(cx, |detail, cx| {
                    detail.set_issue(issue_id, window, cx);
                });
            }
            Screen::Session { session_id } => {
                // ENTRY-OR-INSERT, never a re-point: each session owns its
                // feed, so re-pointing one view at another row would hand the
                // new session the old one's socket.
                let view = self
                    .sessions
                    .entry(session_id.clone())
                    .or_insert_with(|| {
                        cx.new(|cx| {
                            crate::session_screen::SessionScreenView::new(session_id.clone(), window, cx)
                        })
                    })
                    .clone();
                // EXP-1316 (1b): a session of a CHAT opens in place — the
                // chat's view hosts its transcript under the chat's own
                // header + strip (an agent channel's keeps its own screen).
                if let Some(root_id) = in_place_root(&session_id, cx) {
                    let chat = self
                        .chats
                        .entry(root_id.clone())
                        .or_insert_with(|| {
                            cx.new(|cx| crate::session_chat::SessionChatScreenView::new(root_id, window, cx))
                        })
                        .clone();
                    chat.update(cx, |chat, cx| chat.set_focus(Some((session_id, view)), cx));
                }
            }
            Screen::Terminal { tab } => {
                // EXP-769: the manager's active tab follows the screen (cmd-w
                // closes the ACTIVE tab; the Latest-changes poll reads it),
                // and the grid takes the keyboard.
                if let Some(host) = crate::session_bar::host_for_window(window, cx) {
                    host.update(cx, |host, cx| host.activate_tab_by_id(tab, window, cx));
                }
            }
            // EXP-1294: entry-or-insert like a session, kept per tree root.
            Screen::Chat { session_id } => {
                let chat = self
                    .chats
                    .entry(session_id.clone())
                    .or_insert_with(|| {
                        cx.new(|cx| crate::session_chat::SessionChatScreenView::new(session_id, window, cx))
                    })
                    .clone();
                // EXP-1316 (1b): back on the chat itself.
                chat.update(cx, |chat, cx| chat.set_focus(None, cx));
            }
            Screen::Agent { agent_id } => {
                self.agents.entry(agent_id.clone()).or_insert_with(|| {
                    cx.new(|cx| crate::agent_screen::AgentScreenView::new(agent_id, window, cx))
                });
            }
            Screen::BoardIssues { .. }
                | Screen::Files
            | Screen::SourceControl
            | Screen::Devices
            | Screen::Drafts
            | Screen::NeedsYou
            | Screen::IssueDraft { .. }
            | Screen::Home
            | Screen::Chats
            | Screen::AppRun { .. }
            | Screen::App { .. }
            | Screen::ExponentialUiKitchenSink
            | Screen::ExponentialUiSpecimen { .. }
            | Screen::StyleguideEntry { .. }
            | Screen::Settings => {
                unreachable!("filtered by is_detail")
            }
        }
        // A back-navigation can re-open a tab for an issue deleted while its
        // screen sat on the stack — prune it right away (EXP-493).
        self.prune_missing_issue_tabs(window, cx);
    }

    /// EXP-1294: tab `ix` now shows `screen`. A session it showed before
    /// loses its view (its feed) — the chat / agent view of the tree stays
    /// for as long as the tab does, so Back lands on it unchanged.
    fn show_in_tab(&mut self, ix: usize, screen: Screen, cx: &mut gpui::Context<Self>) {
        let Some(tab) = self.tabs.get_mut(ix) else {
            return;
        };
        let previous = std::mem::replace(&mut tab.screen, screen);
        if previous != tab.screen {
            self.shutdown_session_view(&previous, cx);
        }
    }

    /// EXP-493: close the tabs of issues that no longer exist — the user
    /// deleting the open issue, a teammate deleting it mid-view, a board
    /// trashing, or a lost membership all remove the row from the synced
    /// collection, and the tab would otherwise linger as a generic "Issue"
    /// chip over "Issue not found in this team". Only a READY collection may
    /// close anything (§4.1 — absence in an unsynced snapshot is "still
    /// syncing", never "deleted"); this mirrors the detail view's own
    /// not-found condition.
    fn prune_missing_issue_tabs(
        &mut self,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let missing: Vec<usize> = {
            let issues = Store::global(cx).collections().issues.read(cx);
            if !issues.is_ready() {
                return;
            }
            self.tabs
                .iter()
                .enumerate()
                .filter_map(|(ix, tab)| {
                    let issue_id = tab.issue_id()?;
                    issues.get(issue_id).is_none().then_some(ix)
                })
                .collect()
        };
        // Highest index first — removal is by index, and closing an active
        // tab re-activates a neighbor safely mid-loop.
        for ix in missing.into_iter().rev() {
            self.remove_tab(ix, true, window, cx);
        }
    }

    /// EXP-818: the remembered origin of `screen`'s tab, if it has one.
    /// EXP-1192: tabs only — no tab-less screen carries a list any more.
    pub(crate) fn origin_of(&self, screen: &Screen) -> Option<TabOrigin> {
        let origin = self
            .tabs
            .iter()
            .find(|tab| tab.holds(screen))
            .and_then(|tab| tab.origin.clone());
        // EXP-890: a screen never sits beside ITSELF (whatever stamped it).
        origin.filter(|origin| !origin.is_list_of(screen))
    }

    /// This panel's screen for `session_id`, if a tab shows it.
    fn session_view(
        &self,
        session_id: &str,
    ) -> Option<Entity<crate::session_screen::SessionScreenView>> {
        self.sessions.get(session_id).cloned()
    }

    /// Activate the tab at `ix`: re-select its origin sidebar entry (and
    /// board), then show its screen (EXP-288). Order matters — tool, board,
    /// then screen — so observers reading tool/board during the nav notify
    /// see final state. Deliberately NOT `navigate`: activation must never
    /// rewrite the tab's remembered origin.
    ///
    /// EXP-769: a terminal tab just shows its screen — it has no meaningful
    /// origin at all. EXP-818: a chat tab restores its origin like an
    /// issue tab does (the list it was opened from sits beside it).
    fn activate_tab(&mut self, ix: usize, window: &mut Window, cx: &mut gpui::Context<Self>) {
        let Some(entry) = self.tabs.get(ix).cloned() else {
            return;
        };
        if matches!(entry.screen, Screen::Terminal { .. }) {
            set_screen(window, cx, Some(entry.screen));
            return;
        }
        // EXP-851: the tab CARRIES its list, so the card's second sidebar
        // follows the screen by itself (EXP-1192). Only the window's board scope (files,
        // git, the `+` shell cwd) has to be put back; it degrades safely if
        // the board has since been trashed (`active_board_id` existence-checks
        // at query time).
        // EXP-1212: the board scope + the screen = ONE held move on a New
        // issue page (a Stay leaves the scope untouched).
        let target = entry.screen.clone();
        crate::navigation::leave_held(window, cx, Some(&target), move |window, cx| {
            if let Some(origin) = &entry.origin {
                crate::sidebar::apply_origin(window, cx, origin);
            }
            set_screen(window, cx, Some(entry.screen));
        });
    }

    /// Close the tab at `ix`. Closing the active tab activates its right
    /// neighbor (else the new last); closing the last clears the center.
    /// Direct tab management never touches the back stack.
    ///
    /// EXP-769: a TERMINAL tab's close is the terminal's close — the child is
    /// killed and the manager drops the tab (the retired dock's cmd-w). No
    /// session row rides on it since EXP-773. The manager's
    /// `TabClosed` echo then finds the entry already gone.
    ///
    /// EXP-923: EVERY tab closes — a live chat is a row in the rail's
    /// Running section for as long as it runs, so closing the chip cannot
    /// lose it; closing a tab is never stopping a session.
    fn close_tab(&mut self, ix: usize, window: &mut Window, cx: &mut gpui::Context<Self>) {
        self.remove_tab(ix, true, window, cx);
    }

    /// [`Self::close_tab`]'s core. `kill_terminal` is false on the paths where
    /// the terminal itself must survive: the manager's own `TabClosed` echo
    /// (already gone) and an undock (the tab moves to its own window).
    fn remove_tab(
        &mut self,
        ix: usize,
        kill_terminal: bool,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        if ix >= self.tabs.len() {
            return;
        }
        // Closing (or undocking) the active issue tab unmounts the detail's
        // description editor without a blur — flush the pending edit so it
        // is written before teardown (EXP-68).
        if let Screen::IssueDetail { .. } = &self.tabs[ix].screen {
            self.issue_detail.update(cx, |detail, cx| {
                // EXP-781: the title saves on blur too, so closing straight
                // from a half-typed title dropped it.
                detail.flush_title(cx);
                detail.flush_description(cx);
            });
        }
        // EXP-923: the strip is tab order, so the chip's display position IS
        // its index.
        let display_pos = ix;
        let closed = self.tabs.remove(ix);
        self.forget_tab(&closed, cx);
        let active = resolved_screen(&self.nav, cx);
        if active.as_ref().is_some_and(|active| closed.holds(active)) {
            // The neighbor within the SAME strip: closing a bottom-bar tab
            // lands on the next bottom-bar tab (the web's dock never jumps to
            // an issue), closing a top tab on the next top tab.
            let strip: Vec<bool> = self
                .tabs
                .iter()
                .map(|tab| tab.screen.is_dock_tab())
                .collect();
            let next = neighbor_in_strip(&strip, display_pos, closed.screen.is_dock_tab())
                .map(|pos| self.tabs[pos].screen.clone());
            set_screen(window, cx, next);
        }
        if let (true, Screen::Terminal { tab }) = (kill_terminal, &closed.screen) {
            let tab = *tab;
            if let Some(host) = crate::session_bar::host_for_window(window, cx) {
                host.update(cx, |host, cx| host.close_terminal(tab, cx));
            }
        }
        // EXP-769: a terminal that left this strip — closed (the PTY is gone)
        // or undocked (it paints in its own window now) — must not come back
        // through go-back OR go-forward (EXP-818: went back from it, then
        // closed it): `sync_tabs` would see a screen with no tab and push a
        // ghost chip over "This terminal was closed". An issue tab is
        // different — its screen is still openable, so its history stays.
        if matches!(closed.screen, Screen::Terminal { .. }) {
            crate::navigation::purge_from_history(window, cx, |screen| *screen == closed.screen);
        }
        cx.notify();
    }

    /// EXP-769: drop `screen`'s tab if it is open — the session bar host's
    /// `TabClosed` echo (the terminal is already gone, so never kill). A
    /// no-op for a screen without a tab.
    pub(crate) fn close_screen_tab(
        &mut self,
        screen: &Screen,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        if let Some(ix) = self.tabs.iter().position(|tab| tab.holds(screen)) {
            self.remove_tab(ix, false, window, cx);
        }
    }

    /// Close every TOP-strip tab except `ix` (EXP-235 context menu). The kept
    /// tab becomes active — the active tab may be among the closed ones.
    /// EXP-769: the session bar's tabs are another strip and stay.
    fn close_other_tabs(&mut self, ix: usize, window: &mut Window, cx: &mut gpui::Context<Self>) {
        if ix >= self.tabs.len() || self.top_tab_count() <= 1 {
            return;
        }
        let keep = self.tabs[ix].screen.clone();
        let goes = |tab: &TabEntry| tab.swept_by_bulk_close(Some(&keep));
        // Same EXP-68 flush as `close_tab`: a closing issue tab may hold a
        // pending description edit.
        if self
            .tabs
            .iter()
            .any(|tab| goes(tab) && matches!(tab.screen, Screen::IssueDetail { .. }))
        {
            self.issue_detail
                .update(cx, |detail, cx| detail.flush_description(cx));
        }
        let dropped: Vec<TabEntry> = self.tabs.iter().filter(|tab| goes(tab)).cloned().collect();
        self.tabs.retain(|tab| !goes(tab));
        for tab in &dropped {
            self.forget_tab(tab, cx);
        }
        set_screen(window, cx, Some(keep));
        cx.notify();
    }

    /// Close every TOP-strip tab (EXP-235 context menu) and clear the center.
    /// EXP-769: the session bar's tabs stay (a terminal must never be killed
    /// by a context menu on the issue strip).
    fn close_all_tabs(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        if self.top_tab_count() == 0 {
            return;
        }
        let goes = |tab: &TabEntry| tab.swept_by_bulk_close(None);
        if self
            .tabs
            .iter()
            .any(|tab| goes(tab) && matches!(tab.screen, Screen::IssueDetail { .. }))
        {
            self.issue_detail
                .update(cx, |detail, cx| detail.flush_description(cx));
        }
        let dropped: Vec<TabEntry> = self.tabs.iter().filter(|tab| goes(tab)).cloned().collect();
        self.tabs.retain(|tab| !goes(tab));
        for tab in &dropped {
            self.forget_tab(tab, cx);
        }
        // The center clears only if a closed tab was showing; a session bar
        // tab that was up stays up.
        if resolved_screen(&self.nav, cx)
            .is_some_and(|screen| dropped.iter().any(|tab| tab.holds(&screen)))
        {
            set_screen(window, cx, None);
        }
        cx.notify();
    }

    /// #FEED-191: clears the per-visit plan (`kept`) of every chat view
    /// (an agent's included) that is not the chat at `here`.
    fn leave_other_chats(&mut self, here: Option<&str>, cx: &mut gpui::Context<Self>) {
        let mut left: Vec<Entity<crate::session_chat::SessionChatScreenView>> =
            self.chats.iter().filter(|(root, _)| Some(root.as_str()) != here).map(|(_, view)| view.clone()).collect();
        left.extend(self.agents.values().filter_map(|agent| {
            let (root, view) = agent.read(cx).chat_view()?;
            (Some(root.as_str()) != here).then(|| view.clone())
        }));
        for view in left {
            view.update(cx, |view, _| view.leave());
        }
    }

    /// EXP-769: how many tabs the TOP strip holds.
    fn top_tab_count(&self) -> usize {
        self.tabs
            .iter()
            .filter(|tab| !tab.screen.is_dock_tab())
            .count()
    }

    /// A tab leaving the strip — the views it owned go with it (EXP-1294:
    /// its tree's chat or agent view and the session it showed). The
    /// sessions themselves are never touched: closing a tab is not stopping.
    fn forget_tab(&mut self, tab: &TabEntry, cx: &mut gpui::Context<Self>) {
        if let Some(root_id) = &tab.root_id {
            // The chat view flushes its embedded issue's pending edits.
            if let Some(view) = self.chats.remove(root_id) {
                view.update(cx, |view, cx| view.shutdown(cx));
            }
            // NEW-6: agent views are keyed by the AGENT id; the tab's root
            // is the caller's conversation with it.
            self.agents.retain(|agent_id, _| {
                agent_id != root_id && crate::agent_screen::agent_screen_root(agent_id, cx) != *root_id
            });
        }
        if let Screen::Agent { agent_id } = &tab.screen {
            self.agents.remove(agent_id);
        }
        // EXP-894: a closed tab's stashed drafts (`crate::tab_state`) go too.
        if let Some(issue_id) = tab.issue_id() {
            self.issue_detail
                .update(cx, |detail, _| detail.forget_tab_state(issue_id));
        }
        self.shutdown_session_view(&tab.screen, cx);
    }

    /// EXP-746: drop the session view a tab owned. Never called for any other
    /// screen kind — the shared single-instance views outlive every tab.
    /// Shutting the view down drops its FEED, not the session.
    fn shutdown_session_view(&mut self, screen: &Screen, cx: &mut gpui::Context<Self>) {
        let Screen::Session { session_id } = screen else {
            return;
        };
        if let Some(view) = self.sessions.remove(session_id) {
            view.update(cx, |view, cx| view.shutdown(cx));
        }
    }

    /// Every per-tab view at once (the team switch, which parks the strip
    /// wholesale): the session feeds shut down, the chat and agent views
    /// drop and rebuild when their tab comes back.
    fn shutdown_all_tab_views(&mut self, cx: &mut gpui::Context<Self>) {
        let views: Vec<_> = self.sessions.drain().map(|(_, view)| view).collect();
        for view in views {
            view.update(cx, |view, cx| view.shutdown(cx));
        }
        let chats: Vec<_> = self.chats.drain().map(|(_, view)| view).collect();
        for view in chats {
            view.update(cx, |view, cx| view.shutdown(cx));
        }
        self.agents.clear();
    }

    /// Undock the tab at `ix` into its own native window (EXP-65): open (or
    /// focus) the undocked window, then close the tab here — the screen now
    /// lives in that window until reattached.
    fn undock_tab(&mut self, ix: usize, window: &mut Window, cx: &mut gpui::Context<Self>) {
        let Some(screen) = self.tabs.get(ix).map(|tab| tab.screen.clone()) else {
            return;
        };
        crate::undock::open_undocked_screen(screen, window.window_handle(), cx);
        self.close_tab(ix, window, cx);
    }

    /// EXP-769: pop a TERMINAL tab out into its own window (EXP-65's terminal
    /// path: the manager keeps the tab, the new window renders its view). The
    /// tab entry leaves this bar WITHOUT killing the terminal; reattaching
    /// (or closing that window) navigates back here and the entry returns.
    fn undock_terminal_tab(
        &mut self,
        ix: usize,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(Screen::Terminal { tab }) = self.tabs.get(ix).map(|tab| tab.screen.clone())
        else {
            return;
        };
        let Some(host) = crate::session_bar::host_for_window(window, cx) else {
            return;
        };
        self.remove_tab(ix, false, window, cx);
        host.update(cx, |host, cx| host.undock_tab(tab, window, cx));
    }

    /// #EXP-1726: every open tab in tab order for the bottom tab bar: its
    /// screen, its strip label and its lead.
    pub(crate) fn tab_items(&self, cx: &App) -> Vec<TabItem> {
        self.tabs
            .iter()
            .map(|tab| {
                let content = chip_content(&tab.screen, cx);
                let title = content.title.clone().unwrap_or_else(|| screen_title(&tab.screen, cx));
                let mut title = match &content.identifier {
                    Some(identifier) => format!("{identifier} {title}"),
                    None => title.to_string(),
                };
                if let Some((badge, _)) = &content.badge {
                    title = format!("{title} ({badge})");
                }
                TabItem { screen: tab.screen.clone(), title, status: rich_tab_status(&content.lead, cx) }
            })
            .collect()
    }

    /// #EXP-1726: the tab bar's × and middle-click: a terminal's kills it.
    pub(crate) fn close_screen(&mut self, screen: &Screen, window: &mut Window, cx: &mut gpui::Context<Self>) {
        let close = self.dock_close_action(screen, cx);
        self.run_dock_close(&close, window, cx);
    }

    pub(crate) fn close_others_of(&mut self, screen: &Screen, window: &mut Window, cx: &mut gpui::Context<Self>) {
        if let Some(ix) = self.tabs.iter().position(|tab| &tab.screen == screen) {
            self.close_other_tabs(ix, window, cx);
        }
    }

    pub(crate) fn close_every_tab(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        self.close_all_tabs(window, cx);
    }

    /// #EXP-1726: a tab into its own window (a terminal, an undockable tab).
    pub(crate) fn undock_screen(&mut self, screen: &Screen, window: &mut Window, cx: &mut gpui::Context<Self>) {
        let Some(ix) = self.tabs.iter().position(|tab| &tab.screen == screen) else { return };
        if matches!(screen, Screen::Terminal { .. }) {
            self.undock_terminal_tab(ix, window, cx);
        } else if screen.undockable() {
            self.undock_tab(ix, window, cx);
        }
    }

    /// #EXP-1726: the strip's drag: the tab lands at index `to`.
    pub(crate) fn move_tab(&mut self, screen: &Screen, to: usize, cx: &mut gpui::Context<Self>) {
        let Some(from) = self.tabs.iter().position(|tab| &tab.screen == screen) else { return };
        let tab = self.tabs.remove(from);
        self.tabs.insert(to.min(self.tabs.len()), tab);
        cx.notify();
    }

    /// EXP-1347: whether a top tab holds `screen`.
    pub(crate) fn has_tab(&self, screen: &Screen) -> bool {
        self.tabs.iter().any(|tab| tab.holds(screen))
    }

    /// EXP-1347: the open chat view of `root_id` in this window.
    pub(crate) fn chat_view(&self, root_id: &str) -> Option<Entity<crate::session_chat::SessionChatScreenView>> {
        self.chats.get(root_id).cloned()
    }

    /// Show `screen`: activate its tab when one is open (origin restore and
    /// all), else navigate to it.
    pub(crate) fn open_screen(&mut self, screen: Screen, window: &mut Window, cx: &mut gpui::Context<Self>) {
        match self.tabs.iter().position(|tab| tab.holds(&screen)) {
            Some(ix) => self.activate_tab(ix, window, cx),
            None => crate::navigation::navigate(window, cx, screen),
        }
    }

    /// EXP-769: what a session-bar chip's × does — resolved at render time
    /// off the synced row, so the tooltip can say it.
    fn dock_close_action(&self, screen: &Screen, _cx: &App) -> DockClose {
        match screen {
            Screen::Terminal { tab } => DockClose::CloseTerminal(*tab),
            other => DockClose::CloseTab(other.clone()),
        }
    }

    fn run_dock_close(&mut self, close: &DockClose, window: &mut Window, cx: &mut gpui::Context<Self>) {
        match close {
            DockClose::CloseTerminal(tab) => {
                let screen = Screen::Terminal { tab: *tab };
                self.close_screen_tab_killing(&screen, window, cx);
            }
            DockClose::CloseTab(screen) => self.close_screen_tab(screen, window, cx),
        }
    }

    /// Close `screen`'s tab AND, for a terminal, the terminal itself.
    fn close_screen_tab_killing(
        &mut self,
        screen: &Screen,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        match self.tabs.iter().position(|tab| &tab.screen == screen) {
            Some(ix) => self.close_tab(ix, window, cx),
            // No entry (undocked terminal?) — still honor the close.
            None => {
                if let Screen::Terminal { tab } = screen {
                    let tab = *tab;
                    if let Some(host) = crate::session_bar::host_for_window(window, cx) {
                        host.update(cx, |host, cx| host.close_terminal(tab, cx));
                    }
                }
            }
        }
    }

    /// §4.1: while the team/boards shapes have not caught up, render a
    /// skeleton — never a wrong empty state.
    fn render_syncing(&self, cx: &mut gpui::Context<Self>) -> gpui::AnyElement {
        v_flex()
            .size_full()
            .p_4()
            .child(crate::feedback::skeleton_rows(
                "screen-syncing-skeleton",
                crate::feedback::SKELETON_ROWS,
                crate::settings::SETTINGS_ROW_H,
                cx,
            ))
            .into_any_element()
    }

    /// EXP-1156: the clamp extent of a screen's own list column — the screen
    /// area itself (this panel's recorded painted width), so the list stays
    /// at most half of it ([`crate::resize_edge::clamp_width`]). Before the
    /// first paint there is no measure yet and only MIN/MAX apply.
    fn screen_list_extent(&self) -> f32 {
        let recorded = self.slot_width.get();
        if recorded > 1. {
            recorded
        } else {
            f32::INFINITY
        }
    }

    /// EXP-1156: a screen list's edge handle (Files, Source Control), over
    /// the list's right edge, full height. The list draws its own
    /// `border_r_1` in its last pixel, so the hover hairline lands ON that
    /// border instead of doubling it. EXP-1163: the neighbour here is the
    /// plain viewer inside the card, not a card, so the strip stays a
    /// [`crate::resize_edge::EdgeAnchor::Column`] one (the list's scrollbar
    /// keeps its hit strip), full height like the border it rides.
    fn screen_list_handle(
        &self,
        panel: crate::resize_edge::SidebarPanel,
        width: f32,
        extent: f32,
        cx: &gpui::Context<Self>,
    ) -> gpui::AnyElement {
        let active = self.resize_drag.is_some_and(|drag| drag.panel == panel);
        let anchor = crate::resize_edge::EdgeAnchor::Column;
        crate::resize_edge::handle(panel, anchor, extent, active, cx)
            .left(px(anchor.strip_left(width - 1.)))
            .top_0()
            .bottom_0()
            .into_any_element()
    }

    /// EXP-851: the Files SCREEN — the trunk tree beside the read-only
    /// viewer. It was a tool column plus a tool-default centre; one screen
    /// now, with the tree as its own list. EXP-1156: the `files` panel — its
    /// right edge drags.
    fn render_files_screen(&mut self, cx: &mut gpui::Context<Self>) -> gpui::AnyElement {
        let file_tree = self.rail.read(cx).file_tree();
        let refresh_tree = file_tree.clone();
        let panel = crate::resize_edge::SidebarPanel::Files;
        let extent = self.screen_list_extent();
        let list_w = crate::resize_edge::panel_width(panel, extent);
        h_flex()
            .size_full()
            .min_h_0()
            .relative()
            .child(
                v_flex()
                    .w(px(list_w))
                    .flex_shrink_0()
                    .h_full()
                    .min_h_0()
                    .border_r_1()
                    .border_color(theme::tokens::glass::STROKE_ROW.to_hsla())
                    .child(
                        self.screen_list_header(
                            Icon::new(registry::NAV_FILES),
                            "Files",
                            cx,
                        )
                        .child(
                            Button::new("files-refresh")
                                .ghost()
                                .cursor_pointer()
                                .xsmall()
                                .icon(Icon::from(ExpIcon::Repeat))
                                .tooltip("Refresh")
                                .on_click(move |_, _, cx| {
                                    refresh_tree.update(cx, |tree, cx| tree.refresh(cx));
                                }),
                        ),
                    )
                    .child(div().flex_1().min_h_0().child(file_tree)),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .child(self.file_viewer.clone()),
            )
            .child(self.screen_list_handle(panel, list_w, extent, cx))
            .children(crate::resize_edge::drag_capture(self.resize_drag, cx))
            .into_any_element()
    }

    /// EXP-851: the Source Control SCREEN — the trunk's commit history beside
    /// the diff of whatever the history list has selected (EXP-253/EXP-509).
    fn render_source_control_screen(
        &mut self,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> gpui::AnyElement {
        let trunk_sync = self.rail.read(cx).trunk_sync().clone();
        // EXP-1133: the shared repo picker (multi-repo teams only) — the
        // same scope row the Files tree carries above its worktree switcher.
        let repo_picker = crate::repo_scope::repo_picker("source-control-repo", window, cx)
            .map(|picker| {
                h_flex()
                    .flex_shrink_0()
                    .w_full()
                    .min_w_0()
                    .px_2()
                    .pb_1()
                    .child(picker)
            });
        // EXP-1156: the `sourceControl` panel — its right edge drags.
        let panel = crate::resize_edge::SidebarPanel::SourceControl;
        let extent = self.screen_list_extent();
        let list_w = crate::resize_edge::panel_width(panel, extent);
        h_flex()
            .size_full()
            .min_h_0()
            .relative()
            .child(
                v_flex()
                    .w(px(list_w))
                    .flex_shrink_0()
                    .h_full()
                    .min_h_0()
                    .border_r_1()
                    .border_color(theme::tokens::glass::STROKE_ROW.to_hsla())
                    .child(
                        self.screen_list_header(
                            Icon::from(ExpIcon::GitMerge),
                            "Source Control",
                            cx,
                        )
                        .child(
                            Button::new("history-refresh")
                                .ghost()
                                .cursor_pointer()
                                .xsmall()
                                .icon(Icon::from(ExpIcon::Repeat))
                                .tooltip("Check for updates")
                                .on_click(move |_, window, cx| {
                                    trunk_sync.update(cx, |engine, cx| engine.refresh(window, cx));
                                }),
                        ),
                    )
                    .children(repo_picker)
                    // The explicit sized wrapper is load-bearing for entity
                    // children (the dock wrapper's flex-child rule).
                    .child(
                        div()
                            .flex_1()
                            .min_h_0()
                            .w_full()
                            .flex()
                            .flex_col()
                            .child(self.history.clone()),
                    ),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .child(self.source_control.clone()),
            )
            .child(self.screen_list_handle(panel, list_w, extent, cx))
            .children(crate::resize_edge::drag_capture(self.resize_drag, cx))
            .into_any_element()
    }

    /// The header strip over the Files / Source Control screens' own list
    /// column (the retired tool header, unchanged).
    fn screen_list_header(
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

    /// Nothing open: point at the sidebar (or at board creation when the
    /// team has none, or team creation when the account has none).
    fn render_empty(&self, cx: &mut gpui::Context<Self>) -> gpui::AnyElement {
        // EXP-188 zero-team state (signup no longer auto-creates a personal
        // team, and the last team is deletable): offer create-or-join. Only
        // a READY-and-empty teams shape counts — empty-because-loading must
        // never show this (§4.1), though `shapes_ready` already gates us.
        {
            let teams = Store::global(cx).collections().teams.read(cx);
            if teams.is_ready() && teams.is_empty() {
                // EXP-1274: the PAGE empty state, fixture `no-team`.
                return v_flex()
                    .size_full()
                    .px_4()
                    .items_center()
                    .justify_center()
                    .child(
                        crate::controls::empty_state(
                            Icon::new(registry::UI_TEAM),
                            crate::empty_state_copy::NO_TEAM,
                            Some(crate::empty_state_copy::NO_TEAM_DESCRIPTION.into()),
                            cx,
                        )
                        .child(
                            crate::controls::empty_state_actions()
                                .child(
                                    Button::new("screens-create-team")
                                        .primary()
                                        .web_sm()
                                        .label("Create team…")
                                        .on_click(|_, window, cx| {
                                            window.dispatch_action(Box::new(CreateTeam), cx);
                                        }),
                                )
                                .child(
                                    Button::new("screens-join-team")
                                        .web_sm()
                                        .label("Join team…")
                                        .on_click(|_, window, cx| {
                                            window.dispatch_action(Box::new(JoinTeam), cx);
                                        }),
                                ),
                        ),
                    )
                    .into_any_element();
            }
        }
        let active_team = active_team_id(&self.nav, cx);
        // EXP-470: a just-created/joined team exists only as an optimistic
        // seed until the Electric echo confirms it — its real boards (join)
        // haven't synced yet, so "No boards yet" would be a wrong empty
        // state. Show a "setting up" surface instead.
        if let Some(team_id) = active_team.as_deref() {
            let teams = Store::global(cx).collections().teams.read(cx);
            if teams.is_seeded(team_id) {
                let name = teams
                    .get(team_id)
                    .map(|team| team.name.clone())
                    .unwrap_or_else(|| "your team".to_string());
                return v_flex()
                    .size_full()
                    .items_center()
                    .justify_center()
                    .gap_2()
                    .child(
                        Icon::new(registry::UI_TEAM)
                            .size_6()
                            .text_color(cx.theme().muted_foreground),
                    )
                    .child(
                        div()
                            .text_sm()
                            .font_weight(FontWeight::MEDIUM)
                            .child(gpui::SharedString::from(format!("Setting up {name}…"))),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child("Syncing the team's data."),
                    )
                    .into_any_element();
            }
        }
        // EXP-698 round 5: the shared empty-state shape (`controls::empty_state`)
        // with the Getting-started checklist under it — the same block the
        // empty board shows, and the web's `/t/$teamSlug` empty page.
        // The 60% band keeps the state optically centred when no cards
        // follow it (a complete checklist, or a team still answering).
        let mut column = v_flex()
            .w_full()
            .min_w_0()
            .min_h(gpui::relative(0.6))
            .items_center()
            .justify_center()
            .gap_3();
        // EXP-1274: the PAGE empty state, fixture `boards`, its button in
        // the actions row. No team resolves (e.g. mid team-switch churn):
        // the create action would silently no-op, so don't offer a dead
        // button. (The fully-teamless account is handled by the zero-team
        // branch above.)
        let mut state = crate::controls::empty_state(
            Icon::new(registry::NAV_BOARDS),
            crate::empty_state_copy::BOARDS,
            Some(crate::empty_state_copy::BOARDS_DESCRIPTION.into()),
            cx,
        );
        if active_team.is_some() {
            state = state.child(
                crate::controls::empty_state_actions().child(
                    Button::new("screens-new-board")
                        .primary()
                        .web_sm()
                        .icon(registry::NAV_BOARDS)
                        .label(crate::empty_state_copy::BOARDS_ACTION)
                        .on_click(|_, window, cx| {
                            window.dispatch_action(Box::new(NewBoard), cx);
                        }),
                ),
            );
        }
        column = column.child(state);
        let cards = active_team
            .as_deref()
            .and_then(|team_id| crate::getting_started::inline_list(team_id, cx));
        v_flex()
            .size_full()
            .min_h_0()
            .child(crate::scroll_pane::v_scroll_pane(
                "screens-empty-scroll",
                &self.empty_scroll,
                v_flex()
                    .w_full()
                    .min_w_0()
                    .px_4()
                    .pb_4()
                    .gap_4()
                    .child(column)
                    .children(cards),
            ))
            .into_any_element()
    }
}

impl Panel for ScreensPanel {
    fn panel_name(&self) -> &'static str {
        PANEL_NAME
    }

    fn title(&mut self, _window: &mut Window, _cx: &mut gpui::Context<Self>) -> impl IntoElement {
        "Team"
    }

    /// The screens ARE the center — closing them would leave an empty center
    /// baked into the persisted layout.
    fn closable(&self, _cx: &App) -> bool {
        false
    }

    fn zoomable(&self, _cx: &App) -> Option<PanelControl> {
        None
    }
}

impl gpui::EventEmitter<PanelEvent> for ScreensPanel {}

impl Focusable for ScreensPanel {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

// ---------------------------------------------------------------------------
// DEV-ONLY one-shot dialog hook (§11.4 headless verification)
// ---------------------------------------------------------------------------

/// DEV-ONLY `EXP_DEV_DIALOG` values: `search` | `create-board` |
/// `create-team` | `join-team[:<invite-token>]` (a token prefills the paste
/// field and previews the invite) | `add-server` | `device-settings:<uuid>` |
/// `duplicate-picker:<issue-uuid>` | `getting-started` (EXP-1259: the rail
/// entry's Getting started dialog on the active team) |
/// `run:<issue-uuid>[,…]` (the composer dialog on those issues) |
/// `app-export:<slug>` (VAPP-121: that app's Export dialog with Include
/// records switched on) | `app-import` (New app on its import step holding
/// the fixture's `capture.importText`, so the server's issue row shows) |
/// `app-consent` (EXP-1558: THE consent window for the natives'
/// `sg_app-consent` call, over whatever screen is open)
/// (anything else = no dialog, logged once).
/// Each opens EXACTLY ONCE, 1500ms after the state it needs has resolved, so
/// a capture run lands on the overlay without synthetic input. Never document
/// for users.
#[derive(Clone, Debug, PartialEq, Eq)]
enum DevDialog {
    Search,
    CreateBoard,
    CreateTeam,
    /// EXP-642: an optional invite token — `join-team:<token>` opens the
    /// dialog already previewing that invite.
    JoinTeam(Option<String>),
    AddServer,
    DeviceSettings(String),
    DuplicatePicker(String),
    /// EXP-1155: the composer dialog on these issues.
    Run(Vec<String>),
    /// EXP-1259: the Getting started dialog (the rail entry) on the active team.
    GettingStarted,
    /// VAPP-121: the Export dialog of the app with this slug, records on.
    AppExport(String),
    /// VAPP-121: New app on its import step with the capture text.
    AppImport,
    /// EXP-1558: the consent window for the `sg_app-consent` call.
    AppConsent,
}

/// The accepted [`DevDialog`] spellings, for the parse-failure log — a typo
/// in a capture recipe must name its alternatives, not fail silently.
const DEV_DIALOG_SPECS: &str = "search | create-board | create-team | join-team[:<invite-token>] | add-server | \
    device-settings:<uuid> | duplicate-picker:<issue-uuid> | \
    run:<issue-uuid>[,<issue-uuid>…] | getting-started | app-export:<slug> | app-import | app-consent";

fn parse_dev_dialog(spec: &str) -> Option<DevDialog> {
    match spec {
        "search" => Some(DevDialog::Search),
        "create-board" => Some(DevDialog::CreateBoard),
        "create-team" => Some(DevDialog::CreateTeam),
        "join-team" => Some(DevDialog::JoinTeam(None)),
        "add-server" => Some(DevDialog::AddServer),
        "getting-started" => Some(DevDialog::GettingStarted),
        "app-import" => Some(DevDialog::AppImport),
        "app-consent" => Some(DevDialog::AppConsent),
        _ => {
            if let Some(slug) = spec.strip_prefix("app-export:") {
                let slug = slug.trim();
                return (!slug.is_empty()).then(|| DevDialog::AppExport(slug.to_string()));
            }
            if let Some(ids) = spec.strip_prefix("run:") {
                let ids: Vec<String> = ids
                    .split(',')
                    .map(str::trim)
                    .filter(|id| !id.is_empty())
                    .map(str::to_string)
                    .collect();
                return (!ids.is_empty()).then_some(DevDialog::Run(ids));
            }
            if let Some(token) = spec.strip_prefix("join-team:") {
                let token = token.trim();
                return Some(DevDialog::JoinTeam(
                    (!token.is_empty()).then(|| token.to_string()),
                ));
            }
            if let Some(id) = spec.strip_prefix("device-settings:") {
                return Some(DevDialog::DeviceSettings(id.to_string()));
            }
            spec.strip_prefix("duplicate-picker:")
                .map(|id| DevDialog::DuplicatePicker(id.to_string()))
        }
    }
}

/// A [`DevDialog`] whose precondition has RESOLVED — the ids its opener takes
/// are snapshotted here so the delayed spawn never re-reads the store.
enum DevDialogTarget {
    Search,
    CreateBoard { team_id: String },
    CreateTeam,
    JoinTeam { token: Option<String> },
    AddServer,
    DeviceSettings { device_id: String },
    DuplicatePicker { issue_id: String },
    Composer { seed: crate::navigation::ChatSeed },
    GettingStarted,
    AppExport { app_id: String },
    AppImport { team_id: String },
    AppConsent,
}

fn open_dev_dialog(target: DevDialogTarget, window: &mut Window, cx: &mut App) {
    match target {
        DevDialogTarget::Search => crate::search_sheet::open_search(window, cx),
        DevDialogTarget::CreateBoard { team_id } => {
            crate::create_board_dialog::open(window, cx, team_id)
        }
        DevDialogTarget::CreateTeam => crate::create_team_dialog::open(window, cx),
        DevDialogTarget::JoinTeam { token } => crate::join_team::open(window, cx, token),
        DevDialogTarget::AddServer => crate::machines::open_add_server_dialog(window, cx),
        DevDialogTarget::DeviceSettings { device_id } => {
            crate::device_settings::open(window, cx, device_id)
        }
        DevDialogTarget::DuplicatePicker { issue_id } => {
            crate::issue_detail::open_duplicate_picker(issue_id, window, cx)
        }
        DevDialogTarget::Composer { seed } => crate::composer_dialog::open(window, cx, seed),
        DevDialogTarget::GettingStarted => crate::getting_started::open_dialog(window, cx),
        DevDialogTarget::AppExport { app_id } => crate::app_export_dialog::open(window, cx, app_id, true),
        DevDialogTarget::AppConsent => crate::exponential_ui_host::dev_open_consent(cx),
        DevDialogTarget::AppImport { team_id } => crate::app_import_dialog::open_import(
            window,
            cx,
            team_id,
            domain::app_export::APP_EXPORT_CAPTURE.import_text.clone(),
        ),
    }
}

/// EXP-633: set once the requested dev dialog is actually UP (or once the
/// spec turned out to be unrecognised, so a typo can never hang the probe).
static DIALOG_OPENED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// EXP-633: has the DEV-ONLY dialog hook finished its work? `true` when this
/// run asked for no dialog at all, otherwise `true` once the dialog opened.
/// The ready probe ([`crate::dev_ready`]) waits on this so a capture never
/// photographs the screen a beat before its overlay appears.
pub(crate) fn dev_dialog_settled() -> bool {
    dev_dialog_spec().is_none() || DIALOG_OPENED.load(std::sync::atomic::Ordering::SeqCst)
}

/// The DEV-ONLY spec this run asked for: `EXP_DEV_DIALOG`. (EXP-1170: the
/// create dialog and its `EXP_DEV_CREATE_DIALOG` alias are gone — the New
/// issue page is `EXP_DEV_SCREEN=draft`.)
fn dev_dialog_spec() -> Option<String> {
    let spec = std::env::var("EXP_DEV_DIALOG").ok()?;
    let spec = spec.trim();
    (!spec.is_empty()).then(|| spec.to_string())
}

impl ScreensPanel {
    /// Resolve `dialog`'s precondition against the CURRENT state. `None` =
    /// not ready yet (an unsynced row, no scope, still signing in) — the
    /// caller retries on the next render instead of opening the wrong thing.
    fn resolve_dev_dialog(&self, dialog: &DevDialog, cx: &App) -> Option<DevDialogTarget> {
        let store = Store::global(cx);
        let signed_in = matches!(store.session(cx), sync::SessionPhase::Synced { .. });
        Some(match dialog {
            DevDialog::Search => {
                // `open_search` self-guards on both — gate here too so the
                // one-shot latch is never spent on a silent no-op.
                if !signed_in {
                    return None;
                }
                active_team_id(&self.nav, cx)?;
                DevDialogTarget::Search
            }
            DevDialog::CreateBoard => DevDialogTarget::CreateBoard {
                team_id: active_team_id(&self.nav, cx)?,
            },
            DevDialog::CreateTeam => {
                if !signed_in {
                    return None;
                }
                DevDialogTarget::CreateTeam
            }
            DevDialog::JoinTeam(token) => {
                if !signed_in {
                    return None;
                }
                DevDialogTarget::JoinTeam {
                    token: token.clone(),
                }
            }
            DevDialog::AddServer => {
                // The snippet names the account's instance — without one it
                // would photograph the hardcoded cloud fallback.
                crate::queries::active_account(cx)?;
                DevDialogTarget::AddServer
            }
            DevDialog::DeviceSettings(device_id) => {
                store.collections().devices.read(cx).get(device_id)?;
                DevDialogTarget::DeviceSettings {
                    device_id: device_id.clone(),
                }
            }
            DevDialog::DuplicatePicker(issue_id) => {
                store.collections().issues.read(cx).get(issue_id)?;
                DevDialogTarget::DuplicatePicker {
                    issue_id: issue_id.clone(),
                }
            }
            DevDialog::GettingStarted => {
                // `open_dialog` self-guards on the active team: gate here so
                // the one-shot latch is never spent on a silent no-op.
                if !signed_in {
                    return None;
                }
                active_team_id(&self.nav, cx)?;
                DevDialogTarget::GettingStarted
            }
            DevDialog::AppExport(slug) => {
                // The switch row shows only with records: wait for both shapes.
                if !store.collections().app_records.read(cx).is_ready() {
                    return None;
                }
                let team_id = active_team_id(&self.nav, cx);
                let entry = crate::apps_view::find_app(slug, team_id.as_deref(), cx)?;
                DevDialogTarget::AppExport { app_id: entry.id }
            }
            DevDialog::AppConsent => {
                if !signed_in {
                    return None;
                }
                DevDialogTarget::AppConsent
            }
            DevDialog::AppImport => {
                if !signed_in {
                    return None;
                }
                DevDialogTarget::AppImport {
                    team_id: active_team_id(&self.nav, cx)?,
                }
            }
            DevDialog::Run(issue_ids) => {
                let issues = store.collections().issues.read(cx);
                if !issue_ids.iter().all(|id| issues.get(id).is_some()) {
                    return None;
                }
                DevDialogTarget::Composer {
                    seed: crate::navigation::ChatSeed::issues(issue_ids.clone()),
                }
            }
        })
    }

    /// DEV-ONLY (§11.4 headless verification, EXP_DEV_* family): open the
    /// [`DevDialog`] this run asked for, exactly once, from the render path.
    /// Unset in normal runs.
    fn fire_dev_dialog(&self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        use std::sync::atomic::{AtomicBool, Ordering};
        static FIRED: AtomicBool = AtomicBool::new(false);
        if FIRED.load(Ordering::SeqCst) {
            return;
        }
        let Some(spec) = dev_dialog_spec() else {
            return;
        };
        let Some(dialog) = parse_dev_dialog(&spec) else {
            // A typo would otherwise photograph the bare screen with no hint
            // that no dialog was ever going to open. Once per run — this is
            // the render path. (A spec that parses but whose precondition is
            // still resolving stays SILENT: that one is expected per frame.)
            static WARNED: AtomicBool = AtomicBool::new(false);
            if !WARNED.swap(true, Ordering::SeqCst) {
                eprintln!(
                    "[exp-desktop] dev: EXP_DEV_DIALOG={spec} unrecognized — \
                     expected {DEV_DIALOG_SPECS}"
                );
            }
            // Nothing will ever open — release the ready probe (EXP-633).
            DIALOG_OPENED.store(true, Ordering::SeqCst);
            return;
        };
        let Some(target) = self.resolve_dev_dialog(&dialog, cx) else {
            return;
        };
        if FIRED.swap(true, Ordering::SeqCst) {
            return;
        }
        cx.spawn_in(window, async move |_this, cx| {
            cx.background_executor()
                .timer(std::time::Duration::from_millis(1500))
                .await;
            let opened = cx.update(|window, cx| open_dev_dialog(target, window, cx));
            // EXP-633: the ready probe waits for the overlay, not the timer.
            DIALOG_OPENED.store(true, Ordering::SeqCst);
            eprintln!("[exp-desktop] dev: EXP_DEV_DIALOG={spec} fired ({opened:?})");
        })
        .detach();
    }
}

impl crate::resize_edge::ResizeHost for ScreensPanel {
    fn resize_drag(&mut self) -> &mut Option<crate::resize_edge::ResizeDrag> {
        &mut self.resize_drag
    }
}

impl ScreensPanel {
    /// EXP-1316 (1b): the chat view hosting `session_id` in place, its
    /// focus (re)pointed at this session. Activation (`sync_tabs`) sets the
    /// focus once, but a root row that syncs AFTER it never re-ran that —
    /// the panel then showed the chat's Conversation (or the bare session
    /// screen) instead of the opened session; render settles it.
    fn in_place_chat(
        &mut self,
        session_id: &str,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> Option<gpui::AnyElement> {
        let root_id = in_place_root(session_id, cx)?;
        let view = self.sessions.get(session_id).cloned()?;
        let chat = self
            .chats
            .entry(root_id.clone())
            .or_insert_with(|| cx.new(|cx| crate::session_chat::SessionChatScreenView::new(root_id, window, cx)))
            .clone();
        if chat.read(cx).focus_id() != Some(session_id) {
            let session_id = session_id.to_string();
            chat.update(cx, |chat, cx| chat.set_focus(Some((session_id, view)), cx));
        }
        Some(chat.into_any_element())
    }
}

impl Render for ScreensPanel {
    fn render(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let screen = resolved_screen(&self.nav, cx);

        self.fire_dev_dialog(window, cx);

        let content = match &screen {
            Some(Screen::IssueDetail { .. }) => self.issue_detail.clone().into_any_element(),
            Some(Screen::Settings) => self.settings.clone().into_any_element(),
            // EXP-746: built by `sync_tabs` on activation — the fallback only
            // shows for the frame between a navigation and that observer.
            // EXP-1316 (1b): a chat's session renders inside its chat.
            Some(Screen::Session { session_id }) => self
                .in_place_chat(session_id, window, cx)
                .or_else(|| {
                    self.sessions
                        .get(session_id)
                        .cloned()
                        .map(gpui::IntoElement::into_any_element)
                })
                .unwrap_or_else(|| self.render_syncing(cx)),
            // EXP-769: the terminal fills the center — its grid, exit strip
            // and Latest-changes bar are the session bar host's (the entity
            // that owns the manager and the dock-scoped key bindings).
            Some(Screen::Terminal { tab }) => {
                match crate::session_bar::host_for_window(window, cx) {
                    Some(host) => {
                        let tab = *tab;
                        host.update(cx, |host, cx| host.render_terminal_screen(tab, window, cx))
                    }
                    None => self.render_syncing(cx),
                }
            }
            // EXP-851: a board's list, full width.
            Some(Screen::BoardIssues { .. }) => self.list.clone().into_any_element(),
            Some(Screen::Files) => self.render_files_screen(cx),
            Some(Screen::SourceControl) => self.render_source_control_screen(window, cx),
            Some(Screen::Devices) => self.devices.clone().into_any_element(),
            Some(Screen::Drafts) => self.drafts.clone().into_any_element(),
            Some(Screen::NeedsYou) => self.needs_you.clone().into_any_element(),
            Some(Screen::IssueDraft { .. }) => self.issue_draft.clone().into_any_element(),
            Some(Screen::Home) => self.chat.clone().into_any_element(),
            Some(Screen::Chats) => self.chats_page.clone().into_any_element(),
            // VAPP-110 + VAPP-242: built by `sync_app_editor` on entry.
            Some(Screen::App { .. } | Screen::AppRun { .. }) => self
                .app_editor
                .clone()
                .map(gpui::IntoElement::into_any_element)
                .unwrap_or_else(|| self.render_syncing(cx)),
            // EXP-1294: built by `sync_tabs` on activation, like a session.
            Some(Screen::Chat { session_id }) => self
                .chats
                .get(session_id)
                .cloned()
                .map(gpui::IntoElement::into_any_element)
                .unwrap_or_else(|| self.render_syncing(cx)),
            Some(Screen::Agent { agent_id }) => self
                .agents
                .get(agent_id)
                .cloned()
                .map(gpui::IntoElement::into_any_element)
                .unwrap_or_else(|| self.render_syncing(cx)),
            // VAPP-90: the Exponential UI kitchen sink (dev-only).
            Some(Screen::ExponentialUiKitchenSink) => self
                .eui_core
                .get_or_insert_with(|| {
                    cx.new(|cx| {
                        crate::exponential_ui_screen::ExponentialUiKitchenSink::new(window, cx)
                    })
                })
                .clone()
                .into_any_element(),
            // VAPP-93: one specimen of the site (dev-only).
            Some(Screen::ExponentialUiSpecimen { id }) => {
                let stale = self
                    .exponential_ui_specimen
                    .as_ref()
                    .is_none_or(|view| view.read(cx).id() != id.as_str());
                if stale {
                    let id = id.clone();
                    self.exponential_ui_specimen = Some(cx.new(|cx| {
                        crate::exponential_ui_screen::ExponentialUiSpecimen::new(&id, window, cx)
                    }));
                }
                self.exponential_ui_specimen.clone().expect("built above").into_any_element()
            }
            // EXP-1259: one IDE styleguide entry, full-window (dev-only).
            Some(Screen::StyleguideEntry { id }) => match crate::styleguide::entries::find(id) {
                Some(entry) => div()
                    .id("styleguide-entry")
                    .size_full()
                    .overflow_y_scroll()
                    .p_6()
                    .child((entry.render)(window, cx))
                    .into_any_element(),
                None => self.render_empty(cx),
            },
            // EXP-851: nothing open at all (a fresh window, the last tab
            // closed, a team switch). There is no tool default left to fall
            // back on — the empty state points at the rail.
            None if !shapes_ready(cx) => self.render_syncing(cx),
            None => self.render_empty(cx),
        };
        pinned_panel_root(
            cx.theme().colors.list,
            None,
            self.slot_width.clone(),
            content,
        )
    }
}

/// EXP-492/EXP-499: the panel root — background, the optional fallback tab
/// strip, then the center content pinned to the panel's recorded painted
/// width. Between real layout frames, gpui resolves this subtree under
/// fit-content constraints (diagnosed in EXP-492 with a per-frame bounds
/// logger: whole-column collapses to ~173px); a percent-width (`w_full`)
/// chain below then latches its collapsed result — on the Actions page the
/// machines rows shrink-wrapped and the card wrap-grid resolved its
/// unwrapped max-content line, running off the right edge (EXP-499). A
/// DEFINITE pixel width on the content slot makes those passes resolve
/// every percent chain below correctly. One frame stale during a live
/// resize, like the editor's own EXP-421/436 slot recording; before the
/// first paint (0.0) the content stretches as before.
fn pinned_panel_root(
    background: gpui::Hsla,
    fallback_strip: Option<gpui::Div>,
    slot_width: std::rc::Rc<std::cell::Cell<f32>>,
    content: gpui::AnyElement,
) -> gpui::Div {
    let recorded = slot_width.get();
    div()
        .size_full()
        .bg(background)
        .on_children_prepainted(move |bounds, _, _| {
            if let Some(first) = bounds.first() {
                slot_width.set(f32::from(first.size.width));
            }
        })
        .child(
            v_flex()
                .size_full()
                .children(fallback_strip)
                .child(
                    div()
                        .flex_1()
                        .min_h_0()
                        .when(recorded > 1.0, |this| this.w(px(recorded)))
                        .child(content),
                ),
        )
}

#[cfg(test)]
mod tests {
    use super::{
        dev_composer_menu_wanted, neighbor_in_strip, parse_dev_dialog, resolve_tab_origin,
        DevDialog, DEV_DIALOG_SPECS,
    };

    /// EXP-1551: `temporary` and `implement-issue` open the PAGE's "+",
    /// `dialog-implement-issue` the dialog's; anything else opens none.
    #[test]
    fn dev_composer_menu_opens_the_named_composer() {
        assert!(dev_composer_menu_wanted(Some("temporary"), false));
        assert!(!dev_composer_menu_wanted(Some("temporary"), true));
        assert!(dev_composer_menu_wanted(Some("implement-issue"), false));
        assert!(!dev_composer_menu_wanted(Some("implement-issue"), true));
        assert!(dev_composer_menu_wanted(Some("dialog-implement-issue"), true));
        assert!(!dev_composer_menu_wanted(Some("dialog-implement-issue"), false));
        assert!(!dev_composer_menu_wanted(Some("session-account"), false));
        assert!(!dev_composer_menu_wanted(None, false));
    }

    /// EXP-1170: the create dialog is gone, and so is its dev spelling —
    /// the New issue page is a SCREEN (`EXP_DEV_SCREEN=draft`).
    #[test]
    fn the_create_issue_dev_dialog_is_gone() {
        assert_eq!(parse_dev_dialog("create-issue"), None);
        assert_eq!(parse_dev_dialog("search"), Some(DevDialog::Search));
        assert!(!DEV_DIALOG_SPECS.contains("create-issue"));
    }

    /// EXP-1259 (dev): `EXP_DEV_DIALOG=getting-started` = the rail entry's
    /// Getting started dialog (the view catalog's desktop drive).
    #[test]
    fn the_getting_started_dev_dialog_parses() {
        assert_eq!(parse_dev_dialog("getting-started"), Some(DevDialog::GettingStarted));
        assert!(DEV_DIALOG_SPECS.contains("getting-started"));
    }

    /// VAPP-121 (dev): the export + import captures.
    #[test]
    fn the_app_export_dev_dialogs_parse() {
        assert_eq!(parse_dev_dialog("app-export:working-hours"), Some(DevDialog::AppExport("working-hours".into())));
        assert_eq!(parse_dev_dialog("app-export:"), None);
        assert_eq!(parse_dev_dialog("app-import"), Some(DevDialog::AppImport));
        assert_eq!(parse_dev_dialog("app-consent"), Some(DevDialog::AppConsent));
        assert!(DEV_DIALOG_SPECS.contains("app-export:<slug>") && DEV_DIALOG_SPECS.contains("app-import"));
    }

    /// EXP-1294 (dev): the builtin and trigger dialogs are gone with their
    /// surfaces; EXP-1319: New agent is a settings page
    /// (`EXP_DEV_SETTINGS=new-agent`), no dialog.
    #[test]
    fn the_dev_dialogs_follow_the_session_model() {
        assert_eq!(parse_dev_dialog("create-agent"), None);
        assert_eq!(parse_dev_dialog("tidy-up"), None);
        assert_eq!(parse_dev_dialog("trigger-new:a1"), None);
        assert_eq!(parse_dev_dialog("trigger-edit:a1:t1"), None);
        assert_eq!(
            parse_dev_dialog("run:i1, i2"),
            Some(DevDialog::Run(vec!["i1".into(), "i2".into()]))
        );
        assert!(!DEV_DIALOG_SPECS.contains("create-agent"));
        assert!(!DEV_DIALOG_SPECS.contains("tidy-up"));
    }

    use super::{
        opens_no_tab, swap_team_tabs, TabEntry, TablessWork,
    };
    use std::collections::HashMap;
    use crate::navigation::{PendingOrigin, Screen, TabOrigin};
    use crate::sidebar::ToolWindow;

    fn origin(tool: ToolWindow) -> TabOrigin {
        TabOrigin {
            tool,
            board_id: None,
        }
    }

    fn issue_tab(id: &str, origin: Option<TabOrigin>) -> TabEntry {
        TabEntry::new(
            Screen::IssueDetail {
                issue_id: id.to_string(),
            },
            origin,
            None,
        )
    }

    fn tab_ids(tabs: &[TabEntry]) -> Vec<String> {
        tabs.iter()
            .map(|tab| tab.issue_id().unwrap_or_default().to_string())
            .collect()
    }

    /// EXP-1250: a team switch PARKS the strip and the way back RESTORES it,
    /// in order; the other team's tabs never leak across.
    #[test]
    fn a_team_switch_and_back_restores_the_tabs() {
        let mut parked = HashMap::new();
        let team_a = vec![issue_tab("a1", None), issue_tab("a2", Some(origin(ToolWindow::Files)))];
        let on_b = swap_team_tabs(team_a, &mut parked, Some("A"), true, Some("B"));
        assert!(on_b.is_empty(), "B has no tabs yet");
        let on_b = vec![issue_tab("b1", None)];
        let back_on_a = swap_team_tabs(on_b, &mut parked, Some("B"), true, Some("A"));
        assert_eq!(tab_ids(&back_on_a), ["a1", "a2"]);
        assert_eq!(
            back_on_a[1].origin.as_ref().map(|origin| origin.tool),
            Some(ToolWindow::Files),
            "a tab keeps its list across the round trip"
        );
        let again_on_b = swap_team_tabs(back_on_a, &mut parked, Some("A"), true, Some("B"));
        assert_eq!(tab_ids(&again_on_b), ["b1"]);
        // The first sync (no team before) parks nothing.
        let mut fresh = HashMap::new();
        let first = swap_team_tabs(vec![issue_tab("x", None)], &mut fresh, None, true, Some("A"));
        assert!(first.is_empty());
        assert!(fresh.is_empty());
    }

    /// Leaving or deleting the strip's team: its tabs are DROPPED (nothing
    /// can switch back to a team that no longer exists), an earlier parked
    /// entry of it goes too, and the arriving team's tabs come back as ever.
    #[test]
    fn a_dead_team_s_tabs_are_dropped_not_parked() {
        let mut parked = HashMap::new();
        parked.insert("A".to_string(), vec![issue_tab("stale", None)]);
        parked.insert("B".to_string(), vec![issue_tab("b1", None)]);
        let dead_a = vec![issue_tab("a1", None), issue_tab("a2", None)];
        let on_b = swap_team_tabs(dead_a, &mut parked, Some("A"), false, Some("B"));
        assert!(!parked.contains_key("A"), "the dead team keeps no parked tabs");
        assert_eq!(tab_ids(&on_b), ["b1"]);
        assert!(parked.is_empty());
    }

    /// EXP-851: which list a tab ends up with. A REAL navigation re-derives
    /// (latest wins), an explicit marker overrides, and a screen change that
    /// is not a navigation at all — a tab click, a go-back, the reactivation
    /// after a close — leaves the tab's list exactly as it was. That last
    /// case is what makes tab activation restore the second sidebar: the
    /// tab keeps its origin, and the card reads the sidebar off it.
    #[test]
    fn a_tab_keeps_its_list_unless_a_navigation_says_otherwise() {
        let reviews = origin(ToolWindow::BoardIssues);
        let inbox = origin(ToolWindow::Files);
        // Tab click / go-back: no marker, the tab keeps what it has.
        assert_eq!(
            resolve_tab_origin(None, Some(&reviews), Some(inbox.clone())),
            Some(reviews.clone())
        );
        // … including "no list at all" (a rail-opened detail stays rail-side).
        assert_eq!(resolve_tab_origin(None, None, Some(inbox.clone())), None);
        // A real navigation takes the breadcrumb's answer, replacing the old.
        assert_eq!(
            resolve_tab_origin(
                Some(&PendingOrigin::Derive),
                Some(&reviews),
                Some(inbox.clone())
            ),
            Some(inbox.clone())
        );
        // EXP-862: … and a breadcrumb with NO list keeps the tab's. Deriving
        // nothing means the click named no list, not that this tab has none:
        // a row clicked in a list beside a rail-opened detail used
        // to blank the column and throw the reader back to the rail.
        assert_eq!(
            resolve_tab_origin(Some(&PendingOrigin::Derive), Some(&reviews), None),
            Some(reviews.clone())
        );
        // With nothing on either side there is still nothing.
        assert_eq!(resolve_tab_origin(Some(&PendingOrigin::Derive), None, None), None);
        // A RAIL row opens with no list, whatever is on screen.
        assert_eq!(
            resolve_tab_origin(Some(&PendingOrigin::Rail), Some(&reviews), Some(inbox.clone())),
            None
        );
        // An explicit marker (deep link, OS notification) always wins.
        assert_eq!(
            resolve_tab_origin(
                Some(&PendingOrigin::Explicit(reviews.clone())),
                None,
                Some(inbox)
            ),
            Some(reviews)
        );
    }

    /// EXP-1294: a chat and the sessions of its tree share ONE tab — the
    /// tab holds its chat by root, a session by its resolved root, and
    /// nothing of another tree; an agent's channel is keyed the same way.
    #[test]
    fn a_session_shares_its_chat_s_tab() {
        let chat = Screen::Chat {
            session_id: "t1".into(),
        };
        let session = Screen::Session {
            session_id: "s2".into(),
        };
        let mut tab = TabEntry::new(chat.clone(), None, None);
        assert_eq!(tab.root_id.as_deref(), Some("t1"));
        assert!(tab.holds(&chat));
        assert!(tab.holds_root("t1"));
        // A session of the tree is found by its root, then rebinds the tab.
        assert!(!tab.holds(&session));
        tab.screen = session.clone();
        assert!(tab.holds(&session));
        assert!(tab.holds(&chat), "Back to the chat lands on the same tab");
        assert!(!tab.holds(&Screen::Chat {
            session_id: "t2".into()
        }));
        assert!(!tab.holds_root("t2"));
        // A session tab opened before its row synced carries no root yet.
        let early = TabEntry::new(session.clone(), None, None);
        assert_eq!(early.root_id, None);
        let agent = TabEntry::new(
            Screen::Agent {
                agent_id: "a1".into(),
            },
            None,
            None,
        );
        assert!(agent.holds_root("a1"));
        assert!(!agent.holds(&chat));
        // An issue tab owns no tree.
        let issue = issue_tab("i1", None);
        assert_eq!(issue.root_id, None);
        assert_eq!(issue.issue_id(), Some("i1"));
    }

    /// EXP-781: closing a tab activates its own strip's neighbour and NOTHING
    /// else. The last terminal tab closing leaves the center empty rather than
    /// pulling an issue tab into view (EXP-870: the bottom strip is terminals;
    /// `true` below marks a terminal).
    #[test]
    fn a_close_never_activates_the_other_strip() {
        // [issue, issue, terminal] — the terminal closes, none is left.
        assert_eq!(neighbor_in_strip(&[false, false], 2, true), None);
        // [issue] — the last issue closes with a session still open.
        assert_eq!(neighbor_in_strip(&[true], 0, false), None);

        // Within the strip: the tab that shifted into the closed slot wins.
        // [issue, session, session] after closing the middle session.
        assert_eq!(neighbor_in_strip(&[false, true, true], 1, true), Some(1));
        // Closing the LAST session falls back leftwards, skipping the issue.
        assert_eq!(neighbor_in_strip(&[false, true], 2, true), Some(1));
        // The same for the top strip, skipping the session between them.
        assert_eq!(neighbor_in_strip(&[false, true], 1, false), Some(0));
    }

    /// EXP-923: EVERY tab closes — a live chat is a row in the rail's
    /// Running section for as long as it runs, so closing its chip cannot
    /// lose it, and the bulk closes take the same tabs the × would.
    #[test]
    fn every_tab_is_closable() {
        let live = TabEntry::new(
            Screen::Chat {
                session_id: "t1".into(),
            },
            None,
            None,
        );
        let ended = TabEntry::new(
            Screen::Session {
                session_id: "s2".into(),
            },
            None,
            None,
        );
        let issue = TabEntry::new(
            Screen::IssueDetail {
                issue_id: "i1".into(),
            },
            None,
            None,
        );

        // Close all takes the top strip's tabs. (A
        // terminal is the bottom bar's strip and is spared by `is_dock_tab`;
        // `TabId` has no test constructor, and the strip split itself is
        // pinned by `a_close_never_activates_the_other_strip`.)
        assert!(live.swept_by_bulk_close(None));
        assert!(ended.swept_by_bulk_close(None));
        assert!(issue.swept_by_bulk_close(None));

        // Close others spares exactly the tab it was invoked on.
        let keep = issue.screen.clone();
        assert!(!issue.swept_by_bulk_close(Some(&keep)));
        assert!(ended.swept_by_bulk_close(Some(&keep)));
        assert!(live.swept_by_bulk_close(Some(&keep)));
        assert!(!live.swept_by_bulk_close(Some(&live.screen)));
    }

    /// EXP-923: a click in the rail's Running section opens NO tab — unless
    /// the chat already has one, which is reused (and goes active) rather
    /// than duplicated. Every OTHER navigation still opens its tab.
    #[test]
    fn a_rail_running_click_opens_no_tab_but_reuses_one() {
        assert!(opens_no_tab(Some(&PendingOrigin::LiveRail), None, false));
        assert!(
            !opens_no_tab(Some(&PendingOrigin::LiveRail), Some(2), false),
            "the chat's tab already holds this work: reuse it"
        );
        // Every other MARKER opens a tab as before.
        assert!(!opens_no_tab(Some(&PendingOrigin::Rail), None, false));
        assert!(!opens_no_tab(Some(&PendingOrigin::Derive), None, false));
        assert!(!opens_no_tab(
            Some(&PendingOrigin::Explicit(origin(ToolWindow::Files))),
            None,
            false
        ));
        // EXP-1294: a session card INSIDE the tab-less chat stays tab-less
        // (`sync_tabs` only calls it a step inside when the window was on
        // that tree), while a rail or list open of it gets its tab.
        assert!(opens_no_tab(Some(&PendingOrigin::Derive), None, true));
        assert!(!opens_no_tab(Some(&PendingOrigin::Rail), None, true));

        // A Back inside a tab-less chat carries no marker at all — the
        // remembered work is what keeps the chip away.
        assert!(opens_no_tab(None, None, true));
        assert!(
            !opens_no_tab(None, None, false),
            "an ordinary set_screen with nothing remembered still opens its tab"
        );
        assert!(!opens_no_tab(None, Some(0), true));
    }

    /// EXP-923/EXP-1294: the remembered tab-less work is a TREE — its chat
    /// and every session of it (each resolved to its root), nothing else.
    #[test]
    fn tabless_work_holds_its_tree() {
        let work = TablessWork {
            root_id: "t1".into(),
        };
        assert!(work.holds(Some("t1")));
        assert!(!work.holds(Some("t2")));
        // A screen with no tree (an issue, an unsynced session) is not it.
        assert!(!work.holds(None));
    }

    /// EXP-499/EXP-508 regression (the EXP-492 fit-content collapse, on the
    /// Actions page): an Actions-shaped center — full-width section band,
    /// machine rows, the action row list (EXP-618 replaced the old wrapping
    /// card grid), all with real text inside the page's capped `mx_auto`
    /// column and scroll pane — must resolve its column to exactly
    /// `min(1024, panel)` at EVERY panel width, the machines section and its
    /// band must span that column, and the list must never run past the
    /// window's right edge. Two gates: a width sweep under the production
    /// `h_resizable` split (settled frames stay healthy), then the stray
    /// fit-content pass modeled directly — the one place the un-pinned tree
    /// demonstrably breaks (clean `cx.draw` frames alone never reproduce the
    /// live app's between-frame passes).
    ///
    /// The sweep runs to 3000px because the EXP-508 failure only started at
    /// ~1940px (the EXP-499 sweep stopped at 1700 and missed it): a `w_full`
    /// PERCENT child of the centered column resolves against the UNCLAMPED
    /// ancestor available width, so content resolved its max-content line
    /// and the machines section shrink-wrapped — the EXP-436 block-hop leak.
    /// The probe mirrors the fixed page: the column's direct children carry
    /// NO `w_full` and are sized by flex-col stretch; percent widths below
    /// those stretch-sized parents (the band, the machine and action rows)
    /// resolve fine and stay covered.
    #[gpui::test]
    async fn actions_shaped_center_spans_the_panel_at_every_width(
        cx: &mut gpui::TestAppContext,
    ) {
        use gpui::{
            div, point, px, size, AppContext as _, InteractiveElement as _, IntoElement,
            ParentElement as _, Render, ScrollHandle, SharedString, Styled as _, Window,
        };
        use gpui_component::{h_flex, v_flex};

        use super::pinned_panel_root;

        struct PanelProbe {
            scroll: ScrollHandle,
            slot_width: std::rc::Rc<std::cell::Cell<f32>>,
        }
        impl Render for PanelProbe {
            fn render(
                &mut self,
                _window: &mut Window,
                _cx: &mut gpui::Context<Self>,
            ) -> impl IntoElement {
                // A settings list's real shapes (the retired Actions page's
                // rows, machines.rs), with production-like text. The action rows
                // mirror `render_action_row` (EXP-618): full-width, a
                // truncating middle column, a nowrap trailing control.
                let action_row = |name: &'static str, blurb: &'static str| {
                    h_flex()
                        .w_full()
                        .min_w_0()
                        .items_center()
                        .gap_3()
                        .px_3()
                        .py_2()
                        .child(
                            v_flex()
                                .flex_1()
                                .min_w_0()
                                .gap_0p5()
                                .child(
                                    div()
                                        .min_w_0()
                                        .text_sm()
                                        .truncate()
                                        .child(SharedString::from(name)),
                                )
                                .child(
                                    div()
                                        .w_full()
                                        .min_w_0()
                                        .text_xs()
                                        .line_clamp(2)
                                        .child(SharedString::from(blurb)),
                                ),
                        )
                        .child(
                            div()
                                .text_xs()
                                .whitespace_nowrap()
                                .child(SharedString::from("Run")),
                        )
                };
                let machine_row = |label: &'static str| {
                    h_flex()
                        .w_full()
                        .min_w_0()
                        .items_center()
                        .gap_2()
                        .px_3()
                        .py_2()
                        .child(
                            div()
                                .text_sm()
                                .whitespace_nowrap()
                                .child(SharedString::from(label)),
                        )
                        .child(h_flex().flex_1())
                        .child(div().text_xs().child(SharedString::from("Online")))
                };
                let band = h_flex()
                    .w_full()
                    .min_w_0()
                    .debug_selector(|| "probe-band".into())
                    .items_center()
                    .gap_1p5()
                    .px_3()
                    .py_1p5()
                    .child(div().text_sm().child(SharedString::from("My devices")))
                    .child(div().flex_1())
                    .child(div().text_xs().child(SharedString::from("Add server")));
                let list = v_flex()
                    .min_w_0()
                    .debug_selector(|| "probe-list".into())
                    .child(action_row(
                        "Fix merge conflicts",
                        "Pick a conflicted pull request and let your agent rebase, \
                         resolve, and merge it",
                    ))
                    .child(action_row(
                        "Triage code review findings",
                        "Re-check the latest code review board against today's code, \
                         cancel stale findings, and file one grouped issue",
                    ))
                    .child(action_row(
                        "Release staging",
                        "Copy the prod DB over staging, refresh staging web + steer \
                         relay, and push the staging iOS/Android build",
                    ))
                    .child(action_row(
                        "Release prod",
                        "Review the unreleased commit wave, then ship production — \
                         prod web, relays, marketing, desktop",
                    ))
                    .child(action_row(
                        "Release all",
                        "Run the full release train in one go: the staging refresh \
                         first, then the prod train — one combined wave, one report",
                    ))
                    .child(action_row(
                        "Extended code review",
                        "Run a deep multi-pass code review over the whole codebase \
                         and file every confirmed finding as an issue",
                    ));
                let column = v_flex()
                    .w_full()
                    .min_w_0()
                    .px_4()
                    .py_4()
                    .gap_4()
                    .debug_selector(|| "probe-column".into())
                    .child(
                        v_flex()
                            .min_w_0()
                            .debug_selector(|| "probe-machines".into())
                            .child(band)
                            .child(machine_row("mint · Danny Strähhuber  v0.14.8"))
                            .child(machine_row("macbook · Danny Strähhuber  v0.14.8")),
                    )
                    .child(v_flex().min_w_0().gap_2().child(list));
                let content = v_flex()
                    .size_full()
                    .min_h_0()
                    .min_w_0()
                    .child(crate::scroll_pane::v_scroll_pane(
                        "probe-scroll",
                        &self.scroll,
                        div()
                            .w_full()
                            .min_w_0()
                            .child(column.max_w(px(1024.)).mx_auto()),
                    ))
                    .into_any_element();
                div()
                    .size_full()
                    .debug_selector(|| "probe-slot".into())
                    .child(pinned_panel_root(
                        gpui::hsla(0., 0., 0., 1.),
                        None,
                        self.slot_width.clone(),
                        content,
                    ))
            }
        }

        // EXP-492/EXP-499's REGRESSION harness: the retired centre split
        // (EXP-851 removed it from the app) — an `h_resizable` whose right
        // panel holds the screens panel, the nesting that produced the
        // collapse this test guards `pinned_panel_root` against. The resizable
        // panels are flex items with `flex_basis` fed BACK from prepaint
        // bounds via `ResizableState` — at widths where the bases mismatch
        // the container, taffy's flex resolution measures the panel CONTENT
        // under fit-content constraints (the EXP-492 diagnosis).
        struct Split {
            probe: gpui::Entity<PanelProbe>,
            state: gpui::Entity<gpui_component::resizable::ResizableState>,
        }
        impl Render for Split {
            fn render(
                &mut self,
                _window: &mut Window,
                _cx: &mut gpui::Context<Self>,
            ) -> impl IntoElement {
                use gpui_component::resizable::{h_resizable, resizable_panel};
                div().size_full().child(
                    h_resizable("center-split")
                        .with_state(&self.state)
                        .child(
                            resizable_panel()
                                // The split's old defaults, inlined: the
                                // constants went with the split itself.
                                .size(px(520.))
                                .size_range(px(320.)..px(880.))
                                .child(div().size_full()),
                        )
                        .child(resizable_panel().child(self.probe.clone())),
                )
            }
        }

        let cx = cx.add_empty_window();
        // The resizable components read the gpui-component Theme global.
        cx.update(|_, cx| gpui_component::init(cx));
        let split = cx.update(|_, cx| {
            let probe = cx.new(|_| PanelProbe {
                scroll: ScrollHandle::new(),
                slot_width: std::rc::Rc::new(std::cell::Cell::new(0.0)),
            });
            let state = cx.new(|_| gpui_component::resizable::ResizableState::default());
            cx.new(|_| Split { probe, state })
        });
        let mut failures: Vec<(f32, f32, f32)> = Vec::new();
        // Every integer window width — the collapse bands are a few px wide,
        // so a coarse step would miss them. Each width draws TWICE: the
        // resizable state and the recorded slot width are both fed from
        // prepaint bounds into the NEXT frame, so the second frame is the
        // settled one users actually see.
        for width in (700..=3000).map(|w| w as f32) {
            for _ in 0..2 {
                cx.draw(point(px(0.), px(0.)), size(px(width), px(600.)), |_, _| {
                    div().size_full().child(split.clone())
                });
            }
            let panel = cx
                .debug_bounds("probe-slot")
                .unwrap_or_else(|| panic!("slot bounds missing at width {width}"));
            let panel_width = f32::from(panel.size.width);
            assert!(
                panel_width > 100.0,
                "panel itself collapsed at width {width}: {panel_width}"
            );
            let column = cx
                .debug_bounds("probe-column")
                .unwrap_or_else(|| panic!("column bounds missing at width {width}"));
            let expected = panel_width.min(1024.);
            let actual = f32::from(column.size.width);
            if (actual - expected).abs() > 1.5 {
                failures.push((width, actual, expected));
            }
            // The literal EXP-499 symptom: the actions content running off
            // the window's right edge.
            let list = cx
                .debug_bounds("probe-list")
                .unwrap_or_else(|| panic!("list bounds missing at width {width}"));
            let right = f32::from(list.origin.x) + f32::from(list.size.width);
            if right > width + 1.5 {
                failures.push((width, right, width));
            }
            // The screenshot's second symptom: the machines section (and its
            // band, a `w_full` percent child of the stretch-sized section)
            // shrink-wrapping instead of spanning the column.
            let content = expected - 32.;
            for selector in ["probe-machines", "probe-band"] {
                let bounds = cx
                    .debug_bounds(selector)
                    .unwrap_or_else(|| panic!("{selector} bounds missing at width {width}"));
                let actual = f32::from(bounds.size.width);
                if (actual - content).abs() > 1.5 {
                    failures.push((width, actual, content));
                }
            }
        }
        assert!(
            failures.is_empty(),
            "actions column broke at {} widths (width, actual, expected): {:?}",
            failures.len(),
            &failures[..failures.len().min(20)]
        );

        // The EXP-492 stray pass itself, modeled directly: between real
        // layout frames gpui can resolve the panel subtree under FIT-CONTENT
        // constraints (a flex parent that neither stretches nor sizes its
        // child), and the app can idle on that frame — the EXP-499
        // screenshot. Un-pinned, the percent chains collapse: the machines
        // band shrink-wraps and the list resolves its max-content line. The
        // recorded-width pin must make even this pass resolve the page like
        // the real panel slot.
        let recorded = f32::from(
            cx.debug_bounds("probe-slot")
                .expect("slot bounds after the sweep")
                .size
                .width,
        );
        let probe = split.read_with(cx, |split, _| split.probe.clone());
        cx.draw(point(px(0.), px(0.)), size(px(1700.), px(600.)), |_, _| {
            div()
                .size_full()
                .flex()
                .flex_row()
                .items_start()
                .child(probe.clone())
        });
        let column = cx
            .debug_bounds("probe-column")
            .expect("column bounds in the fit-content pass");
        let expected = recorded.min(1024.);
        let actual = f32::from(column.size.width);
        assert!(
            (actual - expected).abs() <= 1.5,
            "fit-content pass collapsed the column: {actual} != {expected} \
             (recorded panel {recorded})"
        );
        let list = cx
            .debug_bounds("probe-list")
            .expect("list bounds in the fit-content pass");
        let list_right = f32::from(list.origin.x) + f32::from(list.size.width);
        assert!(
            list_right <= recorded + 1.5,
            "fit-content pass ran the action list past the panel edge: \
             {list_right} > {recorded}"
        );
        for selector in ["probe-machines", "probe-band"] {
            let bounds = cx
                .debug_bounds(selector)
                .unwrap_or_else(|| panic!("{selector} bounds missing in the fit-content pass"));
            let actual = f32::from(bounds.size.width);
            let content = expected - 32.;
            assert!(
                (actual - content).abs() <= 1.5,
                "fit-content pass shrink-wrapped {selector}: {actual} != {content}"
            );
        }
    }
}
