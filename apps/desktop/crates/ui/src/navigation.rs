//! Per-window screen routing (masterplan-v3 §4.2 — "sidebar selection →
//! center content swap"; the desktop analog of the web's TanStack routes
//! under `routes/t/$teamSlug/`).
//!
//! Model: every shell window owns one [`Navigation`] entity — the active
//! team + the current [`Screen`] + a back stack. Views observe it
//! (`cx.observe`) and re-render on change; the `screens::ScreensPanel` in the
//! center dock swaps its content on the current screen.
//!
//! The entities live in a window-keyed registry global so BOTH construction
//! paths reach the same instance: the `Shell` shell creates it, and
//! panels rebuilt by the §3.3 `register_panel` cold-restore path (which only
//! get `(window, cx)`) look it up by `WindowId`. The `Shell` removes the
//! entry on window release.
//!
//! Chrome affordances dispatch typed actions (`crate::actions` — §3.6);
//! [`init`] registers **App-global** handlers that resolve the active window
//! and call [`navigate`]. Global handlers are load-bearing here: menu items
//! render in the `Root` overlay layer, so an element-tree `.on_action` on the
//! team div would never see actions dispatched from an open menu.
//! In-tree click handlers that already hold `(window, cx)` (issue rows,
//! sidebar items) may call [`navigate`] directly.

use std::collections::HashMap;

use gpui::{
    AnyWindowHandle, App, AppContext as _, Entity, Global, KeyBinding, Window, WindowId,
};
use sync::Store;

use crate::actions::{
    GoBack, GoForward, OpenAbout, OpenInbox, OpenIssue, OpenMyIssues, OpenBoard, OpenSettings,
    OpenSourceControl, OpenWhatsNew, SwitchTeam, SyncNow,
};

/// One center TAB (§4.2, reworked): the center pane is tab-based — every
/// `Screen` value identifies one openable tab (issues and files can be open
/// several at a time; Source Control / Settings are singletons).
/// `None` on [`Navigation::screen`] means "no tab active" — the center shows
/// its empty state. Issue LISTS are not screens: they live in the sidebar
/// tool windows (the rail's Inbox / My Issues / All Issues).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Screen {
    /// EXP-851: one BOARD's issue list, full width — the filter bar plus the
    /// grouped list that used to live in the rail's tool column. `board_id`
    /// is EMPTY for the dev route (`EXP_DEV_TOOL=board`), which means "the
    /// window's active board"; the render resolves it through
    /// [`board_for_screen`].
    BoardIssues { board_id: String },
    /// EXP-851: the personal list, full width — the Inbox notification stream
    /// and the My Issues board behind ONE segmented strip (EXP-186's two tabs,
    /// carried on the screen so a tab restore and go-back keep the tab).
    Inbox { tab: crate::sidebar::InboxTab },
    /// EXP-851: the Support ticket list, full width (EXP-180 — server-only
    /// tRPC rows, polled; its Open/Resolved strip lives inside the view).
    Support,
    /// EXP-851: the trunk file tree beside the read-only viewer — one screen
    /// now that the tool column is gone.
    Files,
    /// EXP-851: the trunk's commit history beside its diff — one screen for
    /// the same reason as [`Screen::Files`].
    SourceControl,
    /// Full-page issue detail (`routes/.../issues/$issueIdentifier`).
    IssueDetail { issue_id: String },
    /// `routes/t/$ws/settings/` — team, device AND personal sections
    /// (EXP-238 folded the old Account screen into the settings nav).
    Settings,
    /// One support ticket's conversation (EXP-180 — server-only tRPC data,
    /// opened from the Support tool window's thread list).
    SupportThread { thread_id: String },
    /// Read-only PR diff for an issue's linked PR (EXP-181 — the Reviews
    /// page's rows open this instead of the issue detail; data via
    /// `issues.prFiles`, rendered by the shared side-by-side `DiffView`).
    PrDiff { issue_id: String },
    /// The Devices page (EXP-686 — the web `t/$teamSlug/devices` page: the
    /// user's machines and nothing else). Tab-less full-page mode like
    /// Settings (no sidebar, no tab chip), opened from the rail.
    Devices,
    /// The Actions page (EXP-467 — the web `t/$teamSlug/actions` page 1:1:
    /// the team's action rows; editing lives in the edit dialog).
    /// EXP-480: a tab-less full-page mode like Settings (no sidebar, no tab
    /// chip), opened from the rail's Actions entry; the rail stays up.
    /// EXP-686 split machines and automations out into their own screens.
    Actions,
    /// The Automations page (EXP-686 — the web `t/$teamSlug/automations`
    /// page: the automation rows plus "Recent automated runs").
    Automations,
    /// The Chat page (EXP-772 — the web `t/$teamSlug/chat` page: one centred
    /// prompt box over a subtle row of launch pickers). Tab-less full-page
    /// mode like Devices; sending starts a chat run and navigates to its
    /// [`Screen::Session`].
    Chat,
    /// The Reviews page (EXP-706 — the web `t/$teamSlug/reviews` page: every
    /// open PR across the team, issue-linked ones grouped by board plus the
    /// unlinked ones grouped by repo). It used to be a sidebar TOOL WINDOW
    /// (a split beside the diff); it is now a tab-less full-page screen like
    /// Devices / Actions, and the PR diff is the center view its rows open.
    Reviews,
    /// One live/ended ACP or remote coding session (EXP-746), keyed by the
    /// `coding_sessions` ROW id — never the issue, the branch or the tab: a
    /// resume mints a NEW row, and the two runs are two screens (the resumed
    /// one takes the old one's tab slot, see `ScreensPanel::sync_session_tabs`).
    /// EXP-773: EVERY coding run is one of these — there is no terminal
    /// surface left for a run to live on — which is why every entry point
    /// funnels through [`crate::session_screen::open_session`] rather than
    /// navigating here directly. EXP-769: a session's tab lives in the
    /// BOTTOM session bar, not the top strip ([`Screen::is_dock_tab`]).
    Session { session_id: String },
    /// One PTY terminal of this window's `TerminalManager` (EXP-769): a plain
    /// shell or an agent login, never a coding run (EXP-773). The
    /// terminal used to live in a sliding bottom dock; it renders FULLSCREEN
    /// in the center now, like every other screen, and its tab sits in the
    /// bottom session bar beside the session tabs (web `AgentDock` parity).
    /// Keyed by the manager's stable [`terminal::TabId`] — never persisted
    /// (EXP-301: nothing terminal-side survives a relaunch).
    Terminal { tab: terminal::TabId },
    /// The Getting-started checklist (EXP-470 — the desktop mirror of the
    /// web checklist). Tab-less full-page mode exactly like Actions, opened
    /// from a conditional rail entry. EXP-686: the page carries the
    /// suggestion rows as a second tab, and the tab rides the SCREEN so the
    /// Actions/Automations lightbulb can navigate straight into it (and so
    /// go-back / tab restore keep the tab the user was on).
    GettingStarted { tab: GettingStartedTab },
}

/// Which tab of the Getting-started page is up (EXP-686): the checklist, or
/// the curated action suggestions that used to live on the Actions page.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum GettingStartedTab {
    #[default]
    FirstSteps,
    Suggestions,
}

impl Screen {
    /// Whether the screen can be undocked into its own native window
    /// (EXP-65). Content screens only — Settings is app-config
    /// singletons with near-zero value as standalone windows.
    /// EXP-746 keeps [`Screen::Session`] OUT: an undocked window builds a
    /// FRESH view (`screens::build_screen_content`), and a second view over a
    /// live local run would mean a second engine handle for one agent.
    /// [`Screen::Terminal`] is out too — a terminal pops out through its OWN
    /// path (`undock::open_undocked_terminal_tab`, the manager keeps the tab),
    /// offered from the session bar chip's context menu.
    pub(crate) fn undockable(&self) -> bool {
        matches!(self, Screen::IssueDetail { .. } | Screen::PrDiff { .. })
    }

    /// EXP-769: whether the screen's tab lives in the BOTTOM session bar
    /// (coding sessions and PTY terminals — web `AgentDock` parity) rather
    /// than the top strip (issue and support-thread tabs). Both kinds are
    /// [`Self::is_detail`] tabs of the one `ScreensPanel` list; only where the
    /// chip renders differs. The split is the whole point: issue tabs and
    /// coding tabs were hard to tell apart in one strip.
    pub(crate) fn is_dock_tab(&self) -> bool {
        matches!(self, Screen::Session { .. } | Screen::Terminal { .. })
    }

    /// EXP-288: whether the screen is a DETAIL view — the only kind that
    /// gets a tab chip. Settings is a tab-less full-screen mode
    /// (leave via the settings nav's back button, or by clicking any open
    /// tab — the rail is slid away while it is up, EXP-456); Source
    /// Control's diff and the file viewer are TOOL-DEFAULT center content
    /// driven by the sidebar selection, never tabs. EXP-480: Actions is a
    /// tab-less full-page mode too — the rail stays (its Actions entry
    /// highlights like a tool window's) but the tool column unmounts, and
    /// any rail-tool click or tab click leaves it. EXP-525: PrDiff stopped
    /// being a tab — review diffs are transient center views driven by the
    /// Reviews page (a merged PR used to leave a stale diff tab behind);
    /// `ScreensPanel::dismiss_stale_pr_diff` retires them. EXP-746: a coding
    /// session is a detail tab too — several run at once, and an ended one
    /// keeps its tab as a read-only transcript instead of closing. EXP-769: so
    /// is a PTY terminal (its chip sits in the bottom bar, [`Self::is_dock_tab`]).
    pub(crate) fn is_detail(&self) -> bool {
        matches!(
            self,
            Screen::IssueDetail { .. }
                | Screen::SupportThread { .. }
                | Screen::Session { .. }
                | Screen::Terminal { .. }
        )
    }

    /// EXP-851: whether this screen can sit BESIDE a list — every tab detail,
    /// plus two tab-less centre views:
    ///
    /// * the PR diff (EXP-525 took its tab away so a merged PR can't leave a
    ///   stale chip), which is opened FROM the Reviews queue and carries it;
    /// * the Agent page, which is BOTH a list of its own AND a step on the
    ///   way from an issue to its coding run — "start coding" on an issue
    ///   that sits beside its board must keep that board all the way into the
    ///   session, so Chat carries whatever led to it and falls back to its
    ///   own sessions list when nothing did.
    ///
    /// A screen that carries a list holds it in the screens panel's transient
    /// slot rather than a tab entry (`ScreensPanel::transient_origin`).
    pub(crate) fn carries_list(&self) -> bool {
        self.is_detail() || matches!(self, Screen::PrDiff { .. } | Screen::Chat)
    }

    /// EXP-851: which LIST this screen IS, expressed as the [`TabOrigin`] a
    /// detail opened from it inherits. The five list screens are the Agent
    /// page (its Running/Past rows), a board, the Inbox, Support and Reviews;
    /// everything else — Settings, Devices, Actions, Automations, Getting
    /// started, Files, Source Control, a terminal, any detail — is
    /// CONTEXT-FREE and leaves the rail up.
    pub(crate) fn list_origin(&self) -> Option<TabOrigin> {
        use crate::sidebar::ToolWindow;
        let tool = match self {
            Screen::BoardIssues { board_id } => {
                return Some(TabOrigin {
                    tool: ToolWindow::BoardIssues,
                    board_id: (!board_id.is_empty()).then(|| board_id.clone()),
                    inbox_tab: None,
                })
            }
            Screen::Inbox { tab } => {
                return Some(TabOrigin {
                    tool: ToolWindow::Inbox,
                    board_id: None,
                    inbox_tab: Some(*tab),
                })
            }
            Screen::Support => ToolWindow::Support,
            Screen::Chat => ToolWindow::Sessions,
            Screen::Reviews => ToolWindow::Reviews,
            // EXP-862: the Automations page's run log is a list like any
            // other — a run opened from it keeps it in the left column.
            Screen::Automations => ToolWindow::Automations,
            _ => return None,
        };
        Some(TabOrigin {
            tool,
            board_id: None,
            inbox_tab: None,
        })
    }

}

/// EXP-288: which sidebar entry a detail tab was opened from — clicking the
/// tab re-selects that entry (and its board, for the board-scoped list) so
/// the sidebar always shows the list the tab came from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TabOrigin {
    pub tool: crate::sidebar::ToolWindow,
    /// The origin board when `tool == BoardIssues` (boards share one tool
    /// window; the board id disambiguates which list to restore).
    pub board_id: Option<String>,
    /// The Inbox tool window's active tab when `tool == Inbox` (EXP-426 —
    /// distinguishes "came from My Issues" from "came from a notification";
    /// the detail's prev/next switcher follows this, and tab activation
    /// restores it).
    pub inbox_tab: Option<crate::sidebar::InboxTab>,
}

/// The pending origin marker a navigation leaves for the screens panel:
/// `Derive` = run the EXP-851 breadcrumb rule ([`derive_origin`]) at consume
/// time, which is right for every in-app click path; `Explicit` = the caller
/// knows the list itself (a deep link, an OS notification, the create
/// dialog — nothing on screen names it).
pub(crate) enum PendingOrigin {
    Derive,
    Explicit(TabOrigin),
    /// EXP-851: opened from the RAIL (a pinned row, a Sessions row). The rail
    /// is not a list, so the detail gets none — whatever screen happened to be
    /// up before must not lend it one.
    Rail,
}

/// EXP-851: the ONE rule for which LIST the left column shows beside a
/// freshly opened detail — the "breadcrumb" rule, one layer only:
///
/// * Opened from a LIST SCREEN (a board, the Inbox, Support, the Agent page,
///   Reviews): that list comes along, so the detail sits beside the rows it
///   was picked from.
/// * Opened from another screen that already CARRIES a list (an issue → its
///   coding session, an issue → the Agent page → the run it starts): the list
///   is inherited, one layer at a time.
/// * Opened from anywhere ELSE — the rail itself (a pinned row, a session
///   row, the Agent/Devices/Reviews entries), a context-free page, Settings,
///   a deep link at boot: NO list. The rail stays up, which is the EXP-851
///   change: a detail no longer DERIVES a list it was never opened from.
///
/// `previous` is the screen navigated away from, `previous_origin` the list
/// IT carried (only meaningful while `previous` is a detail). Pure, so every
/// combination is a unit test.
pub(crate) fn derive_origin(
    previous: Option<&Screen>,
    previous_origin: Option<TabOrigin>,
    target: &Screen,
) -> Option<TabOrigin> {
    // Only a detail (or the PR diff) gets a left-column list; a list screen
    // and every full page show the rail.
    if !target.carries_list() {
        return None;
    }
    let previous = previous?;
    // A screen that CARRIES a list hands that one on (an issue beside its
    // board → its coding run; the Agent page reached from that issue → the
    // same board). Only when it carries none does its OWN list apply — which
    // is how the Agent page reached from the rail still lends its sessions
    // list to the run a row starts.
    if previous.carries_list() {
        if let Some(origin) = previous_origin {
            return Some(origin);
        }
    }
    previous.list_origin()
}

/// EXP-827: where a SESSION screen's Back goes — the place the run was opened
/// from, not whatever happens to sit on the window's history.
///
/// Back used to be a plain [`go_back`], so a reader who walked from the session
/// to two other issues and back landed on an unrelated issue. A session knows
/// better: its tab carries the list it was opened beside ([`TabOrigin`], the
/// [`derive_origin`] breadcrumb), so
///
/// * opened from the Agent page / anything context-free (`ToolWindow::Sessions`)
///   → the Agent page, where its row is;
/// * opened from the Automations page's run log (`ToolWindow::Automations`,
///   EXP-862) → that page, where its row is — an unattended run has no row on
///   the Agent page;
/// * opened from a LIST beside an issue the run is bound to → that issue's
///   detail, which offers Watch again;
/// * anything else (a batch, an action, a chat run opened from a board) → the
///   Agent page.
///
/// Pure, so every combination is a unit test below.
pub(crate) fn session_back_target(origin: Option<&TabOrigin>, issue_id: Option<&str>) -> Screen {
    use crate::sidebar::ToolWindow;
    // The two RUN lists: a run opened from either goes back to the list, not
    // to the issue it happens to be bound to.
    let from_run_list = origin
        .map(|origin| matches!(origin.tool, ToolWindow::Sessions | ToolWindow::Automations))
        .unwrap_or(true);
    let automations = origin.is_some_and(|origin| origin.tool == ToolWindow::Automations);
    match issue_id {
        Some(issue_id) if !from_run_list => Screen::IssueDetail {
            issue_id: issue_id.to_string(),
        },
        _ if automations => Screen::Automations,
        _ => Screen::Chat,
    }
}

/// Go to `screen` the way BACK goes there: when it is already the top of the
/// back stack this is a plain [`go_back`] (the history pops, the forward stack
/// gets the screen we left), otherwise a normal navigation to it. Used by the
/// session screen's Back, which aims at an ORIGIN rather than at history.
pub(crate) fn go_back_to(window: &Window, cx: &mut App, screen: Screen) {
    let previous_matches = nav_for_window_readonly(window, cx)
        .map(|nav| nav.read(cx).previous_screen() == Some(&screen))
        .unwrap_or(false);
    if previous_matches {
        go_back(window, cx);
    } else {
        navigate(window, cx, screen);
    }
}

/// Human title for a screen — the center tab label, and the undocked
/// window's header/title (EXP-65). Issue tabs show the synced issue TITLE
/// (EXP-288 — "the tabs [carry] our issue names, not only the shortcode"),
/// degrading to the identifier while the title is blank and to a generic
/// label for unknown ids.
pub(crate) fn screen_title(screen: &Screen, cx: &App) -> gpui::SharedString {
    match screen {
        Screen::IssueDetail { issue_id } => Store::global(cx)
            .collections()
            .issues
            .read(cx)
            .get(issue_id)
            .map(issue_tab_title)
            .unwrap_or_else(|| "Issue".into()),
        Screen::Settings => "Settings".into(),
        // Thread titles are tRPC-only (never synced) — the support surfaces
        // remember them in a process global; unknown ids degrade generically.
        Screen::SupportThread { thread_id } => crate::support_thread::title_of(cx, thread_id)
            .map(gpui::SharedString::from)
            .unwrap_or_else(|| "Support ticket".into()),
        // "· Diff" keeps the tab distinguishable from the same issue's
        // detail tab.
        Screen::PrDiff { issue_id } => Store::global(cx)
            .collections()
            .issues
            .read(cx)
            .get(issue_id)
            .map(|issue| gpui::SharedString::from(format!("{} · Diff", issue_tab_title(issue))))
            .unwrap_or_else(|| "Diff".into()),
        Screen::Session { session_id } => session_tab_title(session_id, cx),
        // EXP-769: the manager's live tab title (OSC-updated), looked up
        // across this process's windows — a tab id is process-unique.
        Screen::Terminal { tab } => crate::session_bar::terminal_tab_title(*tab, cx)
            .unwrap_or_else(|| "Terminal".into()),
        // EXP-851: the list screens name themselves — a board by its synced
        // name (the rail entry's label), the rest by the rail entry's word.
        Screen::BoardIssues { board_id } => Store::global(cx)
            .collections()
            .boards
            .read(cx)
            .get(board_id)
            .map(|board| gpui::SharedString::from(board.name.clone()))
            .unwrap_or_else(|| "Board".into()),
        Screen::Inbox { tab } => match tab {
            crate::sidebar::InboxTab::Inbox => "Inbox".into(),
            crate::sidebar::InboxTab::MyIssues => "My Issues".into(),
        },
        Screen::Support => "Support".into(),
        Screen::Files => "Files".into(),
        Screen::SourceControl => "Source Control".into(),
        Screen::Devices => "Devices".into(),
        Screen::Actions => "Actions".into(),
        Screen::Automations => "Automations".into(),
        Screen::Chat => "Chat".into(),
        Screen::Reviews => "Reviews".into(),
        Screen::GettingStarted { .. } => "Getting started".into(),
    }
}

/// An issue's tab label: the title, or the identifier while the title is
/// blank (EXP-288). The chip's own `max_w` + truncation handles long titles.
fn issue_tab_title(issue: &domain::rows::Issue) -> gpui::SharedString {
    let title = issue.title.trim();
    if title.is_empty() {
        gpui::SharedString::from(issue.identifier.clone())
    } else {
        gpui::SharedString::from(title.to_string())
    }
}

/// A coding session's tab label (EXP-746) — the SAME identity the session
/// screen's header shows (`steer_viewer::SteerSessionView::identity`, itself
/// the web `sessionIdentity`): the linked issue's `EXP-42 · title`, else the
/// action name, else "Batch run" for a batch. Every degrade (no row yet, the
/// issue still syncing) lands on the generic label, like an issue tab's
/// "Issue" — a tab is chrome, so it never renders a transient status string.
fn session_tab_title(session_id: &str, cx: &App) -> gpui::SharedString {
    let Some(store) = Store::try_global(cx) else {
        return "Session".into();
    };
    let collections = store.collections();
    let Some(row) = collections.coding_sessions.read(cx).get(session_id).cloned() else {
        return "Session".into();
    };
    if let Some(issue_id) = row.issue_id.as_deref() {
        let Some(issue) = collections.issues.read(cx).get(issue_id) else {
            return "Session".into();
        };
        let title = issue.title.trim();
        if title.is_empty() {
            return gpui::SharedString::from(issue.identifier.clone());
        }
        return gpui::SharedString::from(format!("{} · {title}", issue.identifier));
    }
    row.action_name
        .as_deref()
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(|name| gpui::SharedString::from(name.to_string()))
        // An issue-less, action-less run is a batch (`exp/batch-<id8>`).
        .unwrap_or_else(|| "Batch run".into())
}

/// EXP-825: what a play button hands the Agent page's composer — the
/// desktop twin of the web `/t/$teamSlug/agent` search params (`issues`,
/// `action`, `device`, `pr`, `text`, `icon`) and the mobile `AgentComposerSeed`.
/// Consumed ONCE by `ChatScreenView` ([`take_pending_chat_seed`]); every
/// field is optional, an action wins over issues when both arrive.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct ChatSeed {
    /// Issues to pre-check (1 = Start coding, 2+ = a batch).
    pub(crate) issue_ids: Vec<String>,
    /// The action to pre-pick (a row id or a builtin literal).
    pub(crate) action_id: Option<String>,
    /// The machine to preselect in the Device pick (a machines-row ▶).
    pub(crate) device_id: Option<String>,
    /// The representative issue of an open PR — fills the fix-conflicts
    /// builtin's `pr` input.
    pub(crate) pr_issue_id: Option<String>,
    /// Text inserted into an EMPTY draft (a suggestion's brief).
    pub(crate) text: Option<String>,
    /// A curated icon name seeding the Create-action builtin's `icon` pick.
    pub(crate) icon: Option<String>,
}

impl ChatSeed {
    /// A seed naming only `issue_ids`.
    pub(crate) fn issues(issue_ids: Vec<String>) -> Self {
        Self {
            issue_ids,
            ..Default::default()
        }
    }

    /// A seed naming only an action.
    pub(crate) fn action(action_id: impl Into<String>) -> Self {
        Self {
            action_id: Some(action_id.into()),
            ..Default::default()
        }
    }

    /// The fix-conflicts builtin with its PR preselected (Reviews, the PR
    /// diff, the issue header).
    pub(crate) fn fix_conflicts(pr_issue_id: impl Into<String>) -> Self {
        Self {
            action_id: Some(api::actions::BUILTIN_FIX_CONFLICTS_ID.to_string()),
            pr_issue_id: Some(pr_issue_id.into()),
            ..Default::default()
        }
    }

    /// A seed naming only the machine (the machines list's ▶).
    pub(crate) fn device(device_id: impl Into<String>) -> Self {
        Self {
            device_id: Some(device_id.into()),
            ..Default::default()
        }
    }
}

/// Per-window navigation state. Mutate through [`navigate`] /
/// [`switch_team`] / [`go_back`] so observers fire consistently.
pub struct Navigation {
    /// Selected team; `None` = "first synced team" (resolved at
    /// query time — a fresh install has no selection until sync lands).
    pub team_id: Option<String>,
    screen: Option<Screen>,
    back_stack: Vec<Screen>,
    /// EXP-818: what [`go_back`] left, so [`go_forward`] (the mouse's
    /// forward button, `Alt+Right`) can re-enter it. Cleared by every REAL
    /// navigation — the browser rule.
    forward_stack: Vec<Screen>,
    /// The explicitly selected board (the top-bar picker) — the primary
    /// scope for [`active_board_id`] so the picker / files / git / run
    /// surfaces stay populated on every screen.
    last_board_id: Option<String>,
    /// EXP-288: the pending tab-origin marker the screens panel consumes
    /// when it opens/updates a tab for the navigated screen. Set by
    /// [`navigate`]/[`navigate_from`]; cleared by
    /// [`set_screen`]/[`go_back`]/[`switch_team`] so tab activation never
    /// rewrites a tab's remembered origin.
    pending_origin: Option<PendingOrigin>,
    /// EXP-825: the composer preselection the next `Screen::Chat` render
    /// consumes ([`take_pending_chat_seed`]). Set by [`navigate_to_chat`]
    /// (and the dev `chat?…` route); cleared wherever `pending_origin` is,
    /// so a tab click or go-back never replays a stale seed.
    pending_chat_seed: Option<ChatSeed>,
}

impl Navigation {
    fn new() -> Self {
        let screen = std::env::var("EXP_DEV_SCREEN")
            .ok()
            .as_deref()
            .and_then(parse_dev_screen)
            .or_else(legacy_tool_screen);
        // DEV-ONLY (EXP-851): `EXP_DEV_TOOL` beside a detail `EXP_DEV_SCREEN`
        // is the capture run asking for that list in the LEFT column.
        let pending_origin = dev_tab_origin(screen.as_ref()).map(PendingOrigin::Explicit);
        Self {
            // DEV-ONLY (§11.4 headless verification, same family as
            // EXP_DEV_SERVER/EXP_DEV_BOARD): pre-select a team and/or
            // pre-route the first screen so gate screenshots can reach
            // surfaces without synthetic input. Unset in normal runs.
            team_id: std::env::var("EXP_DEV_TEAM").ok(),
            screen,
            back_stack: Vec::new(),
            forward_stack: Vec::new(),
            // DEV-ONLY `EXP_DEV_BOARD_ID=<board uuid>` (EXP-642): pre-select
            // the board the first render opens — `EXP_DEV_BOARD=1` is the
            // (unrelated) debug-board switch, hence the `_ID` suffix. A blank
            // value is ignored. Never document for users.
            last_board_id: std::env::var("EXP_DEV_BOARD_ID")
                .ok()
                .map(|id| id.trim().to_string())
                .filter(|id| !id.is_empty()),
            pending_origin,
            // DEV-ONLY (EXP-825): `EXP_DEV_SCREEN='chat?issues=a,b&action=…'`
            // seeds the composer the way a play button would, so a capture
            // run photographs the chips without synthetic input.
            pending_chat_seed: std::env::var("EXP_DEV_SCREEN")
                .ok()
                .as_deref()
                .and_then(parse_dev_chat_seed),
        }
    }

    /// EXP-818: the screen the current one was navigated FROM — the back
    /// stack's top. `None` on the first navigation.
    pub(crate) fn previous_screen(&self) -> Option<&Screen> {
        self.back_stack.last()
    }

    /// The current screen, `None` until first navigation (default applies).
    #[allow(dead_code)] // views read via `resolved_screen`; raw access for later steps
    pub fn screen(&self) -> Option<&Screen> {
        self.screen.as_ref()
    }

    /// Whether [`go_back`] has anywhere to go.
    pub fn can_go_back(&self) -> bool {
        !self.back_stack.is_empty()
    }

    /// Whether [`go_forward`] has anywhere to go.
    #[allow(dead_code)] // the mouse/keyboard paths call `go_forward` blind
    pub fn can_go_forward(&self) -> bool {
        !self.forward_stack.is_empty()
    }

    /// The pure rule behind [`purge_from_history`]: drop every entry
    /// matching `pred` from BOTH stacks. A screen that must not be
    /// re-enterable (a dismissed stale PR diff, a closed terminal) is
    /// reachable through go-forward exactly as through go-back, so purging
    /// only the back stack left the forward button re-entering it.
    /// Returns whether anything was dropped.
    fn purge_history(&mut self, pred: impl Fn(&Screen) -> bool) -> bool {
        let before = self.back_stack.len() + self.forward_stack.len();
        self.back_stack.retain(|screen| !pred(screen));
        self.forward_stack.retain(|screen| !pred(screen));
        self.back_stack.len() + self.forward_stack.len() != before
    }

    /// The pure rule behind [`go_back`]: pop the back stack, park the
    /// current screen for [`go_forward`]. `None` with nothing to go back to.
    fn step_back(&mut self) -> Option<Screen> {
        let previous = self.back_stack.pop()?;
        if let Some(current) = self.screen.take() {
            self.forward_stack.push(current);
        }
        self.screen = Some(previous.clone());
        self.pending_origin = None;
        self.pending_chat_seed = None;
        Some(previous)
    }

    /// The pure rule behind [`go_forward`]: pop the forward stack, park the
    /// current screen for [`go_back`]. `None` with nothing forward.
    fn step_forward(&mut self) -> Option<Screen> {
        let next = self.forward_stack.pop()?;
        if let Some(current) = self.screen.take() {
            self.back_stack.push(current);
        }
        self.screen = Some(next.clone());
        self.pending_origin = None;
        self.pending_chat_seed = None;
        Some(next)
    }
}

/// DEV-ONLY `EXP_DEV_SCREEN` values: `settings` | `account` | `devices` |
/// `actions` | `automations` | `usage` | `board-issues` | `inbox` |
/// `inbox-my-issues` | `support` | `files` | `source-control` (EXP-851 — the
/// list screens the rail's tool windows became) | `chat` | `chat?<seed>` (EXP-825:
/// `issues=<a>,<b>&action=<id>&pr=<issue>&device=<id>&text=<url-encoded>`
/// &icon=<name>`, any subset — [`parse_dev_chat_seed`]) | `reviews` |
/// `getting-started` | `issue:<uuid>` |
/// `pr:<issue-uuid>` (the PR-diff screen, keyed by the ISSUE whose linked PR
/// it shows) | `support:<uuid>` | `session:<uuid>` (a coding session, keyed by
/// its `coding_sessions` ROW id — EXP-746) (anything else = no pre-route).
/// `getting-started` additionally reads `EXP_DEV_GETTING_STARTED_TAB`
/// ([`parse_getting_started_tab`]) so a capture run can land on the
/// suggestions tab without synthetic input.
fn parse_dev_screen(spec: &str) -> Option<Screen> {
    // EXP-825: the seeded composer route — the query is parsed separately
    // into the nav's `pending_chat_seed`.
    if spec.starts_with("chat?") {
        return Some(Screen::Chat);
    }
    match spec {
        "settings" => Some(Screen::Settings),
        // EXP-238: Account merged into Settings — the dev value keeps working.
        "account" => Some(Screen::Settings),
        "devices" => Some(Screen::Devices),
        "actions" => Some(Screen::Actions),
        "automations" => Some(Screen::Automations),
        // EXP-818: Usage folded into Devices (its Accounts section); the old
        // dev value lands there.
        "usage" => Some(Screen::Devices),
        "chat" => Some(Screen::Chat),
        // EXP-706: Reviews left the rail's tool windows for its own page.
        "reviews" => Some(Screen::Reviews),
        // EXP-851: the four list screens the rail's tool windows became. A
        // board takes the window's ACTIVE board (the empty `board_id`
        // sentinel, resolved at render time) so `EXP_DEV_BOARD_ID` still
        // picks which one.
        "board-issues" | "board" | "issues" => Some(Screen::BoardIssues {
            board_id: String::new(),
        }),
        "inbox" => Some(Screen::Inbox {
            tab: crate::sidebar::InboxTab::Inbox,
        }),
        "inbox-my-issues" | "my-issues" => Some(Screen::Inbox {
            tab: crate::sidebar::InboxTab::MyIssues,
        }),
        "support" => Some(Screen::Support),
        "files" => Some(Screen::Files),
        "source-control" => Some(Screen::SourceControl),
        "getting-started" => Some(Screen::GettingStarted {
            tab: std::env::var("EXP_DEV_GETTING_STARTED_TAB")
                .ok()
                .as_deref()
                .map(str::trim)
                .and_then(parse_getting_started_tab)
                .unwrap_or_default(),
        }),
        _ => {
            if let Some(id) = spec.strip_prefix("issue:") {
                return Some(Screen::IssueDetail {
                    issue_id: id.to_string(),
                });
            }
            if let Some(id) = spec.strip_prefix("pr:") {
                return Some(Screen::PrDiff {
                    issue_id: id.to_string(),
                });
            }
            if let Some(id) = spec.strip_prefix("session:") {
                return Some(Screen::Session {
                    session_id: id.to_string(),
                });
            }
            spec.strip_prefix("support:")
                .map(|id| Screen::SupportThread {
                    thread_id: id.to_string(),
                })
        }
    }
}

/// EXP-825 (DEV-ONLY): the composer seed of a `chat?k=v&…` spec — the web
/// route's search params, spelled the same (`issues` csv, `action`, `pr`,
/// `device`, `text`, `icon`), percent-decoded, `+` as a space (the web
/// encodes `+` itself, so this is safe). `None` for anything but a `chat?`
/// spec; a spec with no known key yields an EMPTY seed, which the composer
/// consumes as a no-op.
fn parse_dev_chat_seed(spec: &str) -> Option<ChatSeed> {
    let query = spec.strip_prefix("chat?")?;
    let mut seed = ChatSeed::default();
    for pair in query.split('&') {
        let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
        let value = percent_decode(value);
        let value = value.trim();
        if value.is_empty() {
            continue;
        }
        match key {
            "issues" => {
                seed.issue_ids = value
                    .split(',')
                    .map(str::trim)
                    .filter(|id| !id.is_empty())
                    .map(str::to_string)
                    .collect()
            }
            "action" => seed.action_id = Some(value.to_string()),
            "pr" => seed.pr_issue_id = Some(value.to_string()),
            "device" => seed.device_id = Some(value.to_string()),
            "text" => seed.text = Some(value.to_string()),
            "icon" => seed.icon = Some(value.to_string()),
            _ => {}
        }
    }
    Some(seed)
}

/// `%XX` → byte, `+` → space; malformed escapes are kept verbatim.
fn percent_decode(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut at = 0;
    while at < bytes.len() {
        match bytes[at] {
            b'+' => {
                out.push(b' ');
                at += 1;
            }
            b'%' if at + 2 < bytes.len() => {
                let hex = std::str::from_utf8(&bytes[at + 1..at + 3]).unwrap_or("");
                match u8::from_str_radix(hex, 16) {
                    Ok(byte) => {
                        out.push(byte);
                        at += 3;
                    }
                    Err(_) => {
                        out.push(b'%');
                        at += 1;
                    }
                }
            }
            byte => {
                out.push(byte);
                at += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// DEV-ONLY back-compat for capture runs (EXP-706/EXP-851): the rail's tool
/// windows ARE screens now, but the shots catalog drives them with
/// `EXP_DEV_TOOL`. Every legacy spelling maps onto its screen — only when
/// `EXP_DEV_SCREEN` picked nothing, which stays authoritative.
///
/// Pure over the spec so the whole table is a unit test.
pub(crate) fn dev_tool_screen(spec: &str) -> Option<Screen> {
    use crate::sidebar::InboxTab;
    match spec.trim() {
        "inbox" => Some(Screen::Inbox {
            tab: InboxTab::Inbox,
        }),
        "my-issues" => Some(Screen::Inbox {
            tab: InboxTab::MyIssues,
        }),
        "board" | "board-issues" | "issues" => Some(Screen::BoardIssues {
            board_id: String::new(),
        }),
        "support" => Some(Screen::Support),
        "files" => Some(Screen::Files),
        "source-control" => Some(Screen::SourceControl),
        // EXP-818: the Agent page (its sessions list is the page now).
        "sessions" | "agent" => Some(Screen::Chat),
        // EXP-706: Reviews was a tool window once.
        "reviews" => Some(Screen::Reviews),
        _ => None,
    }
}

/// The `EXP_DEV_TOOL` screen of the running process (see [`dev_tool_screen`]).
fn legacy_tool_screen() -> Option<Screen> {
    dev_tool_screen(std::env::var("EXP_DEV_TOOL").ok().as_deref()?)
}

/// DEV-ONLY (EXP-851): the left-column LIST a capture run wants beside a
/// detail screen. `EXP_DEV_TOOL` names a list and `EXP_DEV_SCREEN` a detail
/// (`issue:…`, `support:…`, `session:…`) — the pair used to mean "tool column
/// + centre tab" and now means "ListNav + main view", so it seeds the first
/// navigation's explicit origin. `None` whenever either half is missing or
/// the screen is not a detail (a list screen carries its own rail).
fn dev_tab_origin(screen: Option<&Screen>) -> Option<TabOrigin> {
    if !screen?.is_detail() {
        return None;
    }
    let mut origin = legacy_tool_screen()?.list_origin()?;
    // `EXP_DEV_INBOX_TAB` still wins over the `my-issues` spelling.
    if origin.tool == crate::sidebar::ToolWindow::Inbox {
        if let Some(tab) = std::env::var("EXP_DEV_INBOX_TAB")
            .ok()
            .as_deref()
            .map(str::trim)
            .and_then(parse_dev_inbox_tab)
        {
            origin.inbox_tab = Some(tab);
        }
    }
    Some(origin)
}

/// DEV-ONLY `EXP_DEV_INBOX_TAB` values: `inbox` | `my-issues`.
fn parse_dev_inbox_tab(spec: &str) -> Option<crate::sidebar::InboxTab> {
    match spec {
        "inbox" => Some(crate::sidebar::InboxTab::Inbox),
        "my-issues" => Some(crate::sidebar::InboxTab::MyIssues),
        _ => None,
    }
}

/// DEV-ONLY `EXP_DEV_GETTING_STARTED_TAB` values (EXP-686): `first-steps` |
/// `suggestions` (anything else = the ordinary default). Split out of
/// [`parse_dev_screen`] so it is testable without touching the process
/// environment. Never document for users.
fn parse_getting_started_tab(spec: &str) -> Option<GettingStartedTab> {
    match spec {
        "first-steps" => Some(GettingStartedTab::FirstSteps),
        "suggestions" => Some(GettingStartedTab::Suggestions),
        _ => None,
    }
}

/// Window-keyed registry of navigation entities.
#[derive(Default)]
struct NavRegistry {
    by_window: HashMap<WindowId, Entity<Navigation>>,
}

impl Global for NavRegistry {}

/// The window's navigation entity, created on first access. A fresh window
/// starts on the LAST team **and board** this install had active
/// (persisted in `settings.json`, EXP-116); ids that no longer sync fall back
/// via `active_team_id` / `active_board_id` at query time.
pub fn nav_for_window(window: &Window, cx: &mut App) -> Entity<Navigation> {
    let window_id = window.window_handle().window_id();
    if let Some(existing) = cx
        .try_global::<NavRegistry>()
        .and_then(|registry| registry.by_window.get(&window_id).cloned())
    {
        return existing;
    }
    let nav = cx.new(|_| Navigation::new());
    if nav.read(cx).team_id.is_none() {
        // The EXP_DEV_TEAM override (Navigation::new) wins over the
        // persisted pair — dev runs must land where they were pointed.
        let last_team = load_settings_string(cx, LAST_TEAM_KEY);
        let last_board = load_settings_string(cx, LAST_BOARD_KEY);
        if last_team.is_some() || last_board.is_some() {
            nav.update(cx, |nav, _| {
                nav.team_id = last_team;
                // Same rule for the board (EXP-642): an EXP_DEV_BOARD_ID
                // seed must survive the persisted value.
                if nav.last_board_id.is_none() {
                    nav.last_board_id = last_board;
                }
            });
        }
    }
    cx.default_global::<NavRegistry>()
        .by_window
        .insert(window_id, nav.clone());
    nav
}

/// Registry lookup by raw `WindowId` (EXP-65 undock: the undocked window
/// seeds its scope from the ORIGIN window's nav, which it only knows by id).
pub(crate) fn nav_for_window_id(window_id: WindowId, cx: &App) -> Option<Entity<Navigation>> {
    cx.try_global::<NavRegistry>()
        .and_then(|registry| registry.by_window.get(&window_id).cloned())
}

/// Seed a fresh window's navigation from a source window (EXP-65 undock):
/// copy the team/board scope and pin the given screen so every
/// scope-resolving surface (`active_board_id` → git bar, `+` shell cwd,
/// Source Control file scope) sees the same context the tab had when it was
/// undocked. No-op scope copy when the source nav is already gone.
pub(crate) fn seed_window_scope(
    window: &Window,
    cx: &mut App,
    source: WindowId,
    screen: Screen,
) {
    let scope = nav_for_window_id(source, cx).map(|nav| {
        let nav = nav.read(cx);
        (nav.team_id.clone(), nav.last_board_id.clone())
    });
    let nav = nav_for_window(window, cx);
    nav.update(cx, |nav, cx| {
        if let Some((team_id, last_board_id)) = scope {
            nav.team_id = team_id;
            nav.last_board_id = last_board_id;
        }
        nav.screen = Some(screen);
        cx.notify();
    });
}

/// Drop a closed window's entry (called from the `Shell` release hook —
/// entities die with the window; the registry must not leak handles).
pub fn remove_window(window_id: WindowId, cx: &mut App) {
    if let Some(registry) = cx.try_global::<NavRegistry>() {
        if registry.by_window.contains_key(&window_id) {
            cx.global_mut::<NavRegistry>().by_window.remove(&window_id);
        }
    }
}

/// Navigate the window to `screen`, pushing the previous screen onto the
/// back stack (no-op when already there). The screens panel captures the
/// tab's origin from the CURRENT rail tool + board (every sidebar-row click
/// path runs with its tool already active); use [`navigate_from`] where the
/// rail may point anywhere (create dialog, deep links).
pub fn navigate(window: &Window, cx: &mut App, screen: Screen) {
    navigate_inner(window, cx, screen, PendingOrigin::Derive);
}

/// [`navigate`] with an EXPLICIT tab origin (EXP-288).
pub(crate) fn navigate_from(window: &Window, cx: &mut App, screen: Screen, origin: TabOrigin) {
    navigate_inner(window, cx, screen, PendingOrigin::Explicit(origin));
}

/// EXP-851: [`navigate`] from the RAIL — the detail opens with NO list beside
/// it, so the rail stays up. Every rail row that opens a detail (a pinned
/// issue or session, a Sessions-section row) goes through here; without it a
/// click would inherit whatever list the main view happened to be showing.
pub(crate) fn navigate_from_rail(window: &Window, cx: &mut App, screen: Screen) {
    navigate_inner(window, cx, screen, PendingOrigin::Rail);
}

/// Open an issue's detail LANDING FULLY SCOPED on its board (EXP-510): the
/// board becomes the window's active one and the detail's left column is its
/// board list, EXPLICITLY — for the paths where nothing on screen names the
/// list (the create dialog, deep links, an OS notification). In-app row
/// clicks keep plain [`navigate`]: the EXP-851 breadcrumb reads the list they
/// came from.
pub(crate) fn open_issue_scoped(
    window: &mut Window,
    cx: &mut App,
    issue_id: String,
    board_id: String,
) {
    set_active_board(window, cx, board_id.clone());
    navigate_from(
        window,
        cx,
        Screen::IssueDetail { issue_id },
        TabOrigin {
            tool: crate::sidebar::ToolWindow::BoardIssues,
            board_id: Some(board_id),
            inbox_tab: None,
        },
    );
}

/// EXP-771: whether a navigation to `screen` must first look for an existing
/// UNDOCKED window (`undock::reveal_screen`) instead of opening a tab. Exactly
/// the [`Screen::undockable`] screens — no other kind can be living in one, so
/// asking the registry about them would always miss.
///
/// Split out as a pure predicate so the rule is testable without a window;
/// the reveal itself needs the app's window registry.
pub(crate) fn reveals_undocked_window(screen: &Screen) -> bool {
    screen.undockable()
}

/// EXP-781: a navigation raised inside an UNDOCKED window has nowhere to go.
///
/// That window mounts one fixed `AnyView` and no [`crate::screens::
/// ScreensPanel`], so writing the screen into its nav paints nothing — an
/// issue ref clicked in an undocked issue, or the prev/next switcher, just
/// looked dead. Forward to the shell the window was undocked from instead:
/// navigate THERE and raise it, the shape `undock::restore_tab_in_owner`
/// already uses.
///
/// `true` = handled, the caller must not touch this window's nav. A window
/// that HAS a panel takes the normal path, so this costs one registry lookup
/// per navigation in the common case.
fn forward_to_owner_shell(window: &Window, cx: &mut App, screen: &Screen) -> bool {
    if crate::screens::screens_for_window(window, cx).is_some() {
        return false;
    }
    let window_id = window.window_handle().window_id();
    let Some(owner) = crate::undock::owner_shell_for_window(window_id, cx) else {
        return false;
    };
    let screen = screen.clone();
    // Deferred: this runs from inside this window's update, and a
    // cross-window `update` from there silently no-ops.
    cx.defer(move |cx| {
        let _ = owner.update(cx, |_, window, cx| {
            navigate(window, cx, screen.clone());
            window.activate_window();
        });
    });
    true
}

fn navigate_inner(window: &Window, cx: &mut App, screen: Screen, origin: PendingOrigin) {
    // EXP-771: the screen may already have its own window (EXP-65 undock) —
    // then this is a REVEAL, not a navigation: bring that window forward and
    // leave this window's tab strip alone. Every tab-opening entry funnels
    // through here (issue list, search sheet, inbox, issue refs, deep links,
    // `screens::open_screen`), so the dedupe lives here rather than at each
    // call site.
    if reveals_undocked_window(&screen) && crate::undock::reveal_screen(&screen, cx) {
        return;
    }
    if forward_to_owner_shell(window, cx, &screen) {
        return;
    }
    let Some(nav) = nav_for_window_readonly(window, cx) else {
        return;
    };
    nav.update(cx, |nav, cx| {
        if nav.screen.as_ref() == Some(&screen) {
            // Re-navigating to the already-active screen still refreshes the
            // tab's origin (dedupe keeps ONE tab; the LATEST origin wins).
            nav.pending_origin = Some(origin);
            cx.notify();
            return;
        }
        if let Some(previous) = nav.screen.take() {
            nav.back_stack.push(previous);
        }
        nav.screen = Some(screen);
        // A real navigation forks history: the forward stack is gone.
        nav.forward_stack.clear();
        nav.pending_origin = Some(origin);
        cx.notify();
    });
}

/// EXP-825: open the Agent page's composer with `seed` preselected — what
/// every play button does now (issue detail, the bulk bar, an action's Run,
/// a machine's ▶, Fix conflicts). The seed lands on the nav the composer
/// reads and the navigation itself is the ordinary [`navigate`] — EXP-851:
/// it no longer forces a list column, so a run started from a board's issue
/// keeps that board beside it all the way into the session.
///
/// An UNDOCKED window (an issue in its own window) mounts no screens panel,
/// so the seed goes to the shell it was undocked from and that window is
/// raised — the `forward_to_owner_shell` shape, only with the seed carried
/// along (writing it onto the undocked window's nav would lose it).
pub(crate) fn navigate_to_chat(window: &mut Window, cx: &mut App, seed: ChatSeed) {
    if crate::screens::screens_for_window(window, cx).is_none() {
        let window_id = window.window_handle().window_id();
        if let Some(owner) = crate::undock::owner_shell_for_window(window_id, cx) {
            // Deferred: a cross-window `update` from inside this window's
            // update silently no-ops.
            cx.defer(move |cx| {
                let _ = owner.update(cx, |_, window, cx| {
                    navigate_to_chat(window, cx, seed.clone());
                    window.activate_window();
                });
            });
            return;
        }
    }
    // EXP-851: NO list forcing. The Agent page is a full-width screen; a
    // seed raised from a board's issue keeps that board in the left column,
    // and so does the session the composer starts.
    if let Some(nav) = nav_for_window_readonly(window, cx) {
        nav.update(cx, |nav, cx| {
            nav.pending_chat_seed = Some(seed);
            cx.notify();
        });
    }
    navigate(window, cx, Screen::Chat);
}

/// EXP-825: consume the composer seed (`None` = nothing pending). The chat
/// screen calls this at the top of every render, so a seed set while the
/// page is already up (Run clicked twice, a second issue's Start coding) is
/// applied on the notify it triggers.
pub(crate) fn take_pending_chat_seed(nav: &Entity<Navigation>, cx: &mut App) -> Option<ChatSeed> {
    if nav.read(cx).pending_chat_seed.is_none() {
        return None;
    }
    nav.update(cx, |nav, _| nav.pending_chat_seed.take())
}

/// EXP-862: PEEK at the pending seed's action — the rail's pinned action rows
/// light up on the CLICK, and the click writes the seed a beat before the
/// chat screen mounts and consumes it (`chat_screen::active_action_id` is the
/// answer from then on). Never consumes.
pub(crate) fn pending_chat_action_id(nav: &Entity<Navigation>, cx: &App) -> Option<String> {
    nav.read(cx)
        .pending_chat_seed
        .as_ref()
        .and_then(|seed| seed.action_id.clone())
}

/// Consume the pending tab-origin marker (EXP-288). `None` = the screen
/// change wasn't a real navigation (tab click / close-reactivation /
/// go-back) — the tab keeps whatever origin it has.
pub(crate) fn take_pending_origin(
    nav: &Entity<Navigation>,
    cx: &mut App,
) -> Option<PendingOrigin> {
    nav.update(cx, |nav, _| nav.pending_origin.take())
}

/// Set the active tab DIRECTLY — no back-stack push. Tab clicks and
/// tab-close reactivation use this (only real navigations stack); `None`
/// clears the center (last tab closed).
pub fn set_screen(window: &Window, cx: &mut App, screen: Option<Screen>) {
    let Some(nav) = nav_for_window_readonly(window, cx) else {
        return;
    };
    nav.update(cx, |nav, cx| {
        if nav.screen != screen {
            nav.screen = screen;
            nav.pending_origin = None;
            nav.pending_chat_seed = None;
            cx.notify();
        }
    });
}

/// Select the window's active board (the top-bar picker) — re-scopes the
/// Files / Source Control / run / shell surfaces. Persisted alongside the
/// team so the next launch reopens on the same board (EXP-116).
pub fn set_active_board(window: &Window, cx: &mut App, board_id: String) {
    let Some(nav) = nav_for_window_readonly(window, cx) else {
        return;
    };
    let changed = nav.update(cx, |nav, cx| {
        if nav.last_board_id.as_deref() == Some(board_id.as_str()) {
            return false;
        }
        nav.last_board_id = Some(board_id.clone());
        cx.notify();
        true
    });
    if changed {
        let team_id = nav.read(cx).team_id.clone();
        persist_nav_state(cx, team_id, Some(board_id));
    }
}

/// Drop every history entry matching `pred` from BOTH the back and the
/// forward stack (EXP-525: a dismissed stale PR diff must not be
/// resurrectable via go-back; EXP-818: nor via go-forward, which is the
/// same history read the other way).
pub(crate) fn purge_from_history(
    window: &Window,
    cx: &mut App,
    pred: impl Fn(&Screen) -> bool,
) {
    let Some(nav) = nav_for_window_readonly(window, cx) else {
        return;
    };
    nav.update(cx, |nav, cx| {
        if nav.purge_history(pred) {
            cx.notify();
        }
    });
}

/// Pop the back stack (issue detail → board, …). EXP-818: the screen left
/// goes onto the forward stack, and the screen re-entered gets its list
/// column back (`screens::restore_origin_for_screen`) — the rail follows the
/// breadcrumb, not the other way round.
pub fn go_back(window: &Window, cx: &mut App) {
    let Some(nav) = nav_for_window_readonly(window, cx) else {
        return;
    };
    let landed = nav.update(cx, |nav, cx| {
        let previous = nav.step_back()?;
        cx.notify();
        Some(previous)
    });
    if let Some(screen) = landed {
        crate::screens::restore_origin_for_screen(window, cx, &screen);
    }
}

/// EXP-818: re-enter what [`go_back`] left (`cmd-]` / `Alt+Right`, the mouse
/// forward button). No-op with nothing forward.
pub fn go_forward(window: &Window, cx: &mut App) {
    let Some(nav) = nav_for_window_readonly(window, cx) else {
        return;
    };
    let landed = nav.update(cx, |nav, cx| {
        let next = nav.step_forward()?;
        cx.notify();
        Some(next)
    });
    if let Some(screen) = landed {
        crate::screens::restore_origin_for_screen(window, cx, &screen);
    }
}

/// Switch the window's active team. Resets the screen + back stack —
/// screens are team-scoped (a board of team A is meaningless in B);
/// the default-screen resolution then picks the new team's first board.
pub fn switch_team(window: &Window, cx: &mut App, team_id: String) {
    let Some(nav) = nav_for_window_readonly(window, cx) else {
        return;
    };
    let changed = nav.update(cx, |nav, cx| {
        if nav.team_id.as_deref() == Some(team_id.as_str()) {
            return false;
        }
        nav.team_id = Some(team_id.clone());
        nav.screen = None;
        nav.back_stack.clear();
        nav.forward_stack.clear();
        nav.last_board_id = None;
        nav.pending_origin = None;
        nav.pending_chat_seed = None;
        cx.notify();
        true
    });
    if changed {
        // Clearing the board key keeps the file consistent with the
        // in-memory reset above — a restart must not resurrect a board
        // from the team this window just left.
        persist_nav_state(cx, Some(team_id), None);
    }
}

// -----------------------------------------------------------------------
// Last-team/-board persistence (`settings.json`, merge-preserving
// like the `deviceId` key — other subsystems' keys survive)
// -----------------------------------------------------------------------

const LAST_TEAM_KEY: &str = "lastTeamId";
const LAST_BOARD_KEY: &str = "lastBoardId";

fn settings_json_path(cx: &App) -> Option<std::path::PathBuf> {
    cx.try_global::<crate::session::AuthContext>()
        .map(|auth| auth.data_dir.join("settings.json"))
}

/// A persisted non-empty string value (a fresh window's starting point).
fn load_settings_string(cx: &App, key: &str) -> Option<String> {
    let raw = std::fs::read_to_string(settings_json_path(cx)?).ok()?;
    serde_json::from_str::<serde_json::Value>(&raw)
        .ok()?
        .get(key)?
        .as_str()
        .filter(|id| !id.trim().is_empty())
        .map(str::to_string)
}

/// Remember the window's team/board selection for the next launch
/// (best-effort, off-thread). ONE writer for BOTH keys: the `OpenBoard`
/// cross-team path mutates team then board back-to-back, and two
/// independent read-modify-write tasks against the shared `settings.json`
/// could land in either order — the sequence stamp (checked under the write
/// lock) lets a superseded snapshot skip instead of clobbering the newest.
/// `team_id: None` leaves the team key untouched;
/// `board_id: None` REMOVES the board key (team switch reset).
fn persist_nav_state(cx: &mut App, team_id: Option<String>, board_id: Option<String>) {
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::Mutex;
    static SEQ: AtomicU64 = AtomicU64::new(0);
    static WRITE_LOCK: Mutex<()> = Mutex::new(());

    let Some(path) = settings_json_path(cx) else {
        return;
    };
    let seq = SEQ.fetch_add(1, Ordering::SeqCst) + 1;
    cx.background_executor()
        .spawn(async move {
            let _guard = WRITE_LOCK.lock().unwrap_or_else(|poison| poison.into_inner());
            if SEQ.load(Ordering::SeqCst) != seq {
                return; // a newer snapshot is queued (or already written)
            }
            let mut root = std::fs::read_to_string(&path)
                .ok()
                .and_then(|raw| serde_json::from_str::<serde_json::Value>(&raw).ok())
                .filter(serde_json::Value::is_object)
                .unwrap_or_else(|| serde_json::Value::Object(Default::default()));
            if let Some(object) = root.as_object_mut() {
                if let Some(team_id) = team_id {
                    object.insert(
                        LAST_TEAM_KEY.to_string(),
                        serde_json::Value::String(team_id),
                    );
                }
                match board_id {
                    Some(board_id) => {
                        object.insert(
                            LAST_BOARD_KEY.to_string(),
                            serde_json::Value::String(board_id),
                        );
                    }
                    None => {
                        object.remove(LAST_BOARD_KEY);
                    }
                }
            }
            let write = || -> std::io::Result<()> {
                if let Some(parent) = path.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                let mut rendered =
                    serde_json::to_string_pretty(&root).unwrap_or_else(|_| "{}".to_string());
                rendered.push('\n');
                std::fs::write(&path, rendered)
            };
            if let Err(err) = write() {
                log::warn!("[ui] persisting nav state failed: {err}");
            }
        })
        .detach();
}

/// Read-only registry lookup (used by mutators so a dispatch on a window
/// whose team never initialized nav — e.g. the login surface — is a
/// clean no-op instead of creating orphan state).
fn nav_for_window_readonly(window: &Window, cx: &App) -> Option<Entity<Navigation>> {
    let window_id = window.window_handle().window_id();
    cx.try_global::<NavRegistry>()
        .and_then(|registry| registry.by_window.get(&window_id).cloned())
}

/// Resolve the window a global action should target.
///
/// [`App::active_window`] is the right source. The fallback to the sole window
/// only matters on the Linux backend, where `active_window` is derived from the
/// compositor's `keyboard_focused_window` and can momentarily be `None` (e.g.
/// under focus-follows-mouse before the first keyboard enter); with a single
/// window there is no ambiguity. With several windows and no active one we
/// can't tell them apart, so we return `None` (unchanged prior behavior).
pub fn active_or_primary_window(cx: &App) -> Option<AnyWindowHandle> {
    if let Some(window) = cx.active_window() {
        return Some(window);
    }
    let windows = cx.windows();
    if windows.len() == 1 {
        windows.into_iter().next()
    } else {
        None
    }
}

/// Run `f` with the target window's `&mut Window`, **deferred**.
///
/// Every App-global `cx.on_action` handler runs INSIDE gpui's `window.update`:
/// [`gpui::Window::dispatch_action`] defers its work into
/// `window.update(|_, window, cx| dispatch_action_on_node(..))`, and the global
/// action listeners fire from there. `update_window_id` `.take()`s the window
/// out of its slot for the duration of an update, so calling `window.update`
/// *again* on that same window — synchronously, from the handler — finds `None`
/// and returns `Err("window not found")` WITHOUT ever running the closure (the
/// usual `let _ =` then swallows the error). That silent re-entrancy failure is
/// why every action-dispatched nav/dialog appeared completely dead.
///
/// Deferring lets the outer dispatch update unwind first (window back in its
/// slot), so the re-entrant `window.update` succeeds and `f` actually runs.
pub fn on_active_window(cx: &mut App, f: impl FnOnce(&mut Window, &mut App) + 'static) {
    let Some(window) = active_or_primary_window(cx) else {
        return;
    };
    cx.defer(move |cx| {
        let _ = window.update(cx, move |_, window, cx| f(window, cx));
    });
}

/// Register the App-global action handlers (call once from `ui::init`).
/// Actions navigate the **active** window — nav actions only ever originate
/// from user interaction (sidebar, menus, future keymap), which happens in
/// the active window.
pub fn init(cx: &mut App) {
    // EXP-851: My Issues / Inbox are the two tabs of the ONE Inbox SCREEN
    // (EXP-186's tabs, carried on the screen since the tool column went).
    cx.on_action(|_: &OpenMyIssues, cx| {
        navigate_active(
            cx,
            Screen::Inbox {
                tab: crate::sidebar::InboxTab::MyIssues,
            },
        );
    });
    cx.on_action(|_: &OpenInbox, cx| {
        navigate_active(
            cx,
            Screen::Inbox {
                tab: crate::sidebar::InboxTab::Inbox,
            },
        );
    });
    cx.on_action(|_: &OpenSettings, cx| navigate_active(cx, Screen::Settings));
    cx.on_action(|_: &OpenSourceControl, cx| navigate_active(cx, Screen::SourceControl));
    // Manual freshness sync (fetch + ff-only catch-up) on the trunk engine.
    cx.on_action(|_: &SyncNow, cx| {
        on_active_window(cx, |window, cx| {
            let shared = crate::sidebar::rail_shared_for_window(window, cx);
            let trunk_sync = shared.read(cx).trunk_sync().clone();
            trunk_sync.update(cx, |engine, cx| engine.refresh(window, cx));
        });
    });
    // The picker selects a board (scope) and opens its issue list — EXP-851
    // made that a SCREEN (`Screen::BoardIssues`), not a tool window.
    cx.on_action(|action: &OpenBoard, cx| {
        let board_id = action.board_id.clone();
        on_active_window(cx, move |window, cx| {
            // EXP-69 merged picker: a board picked from ANOTHER team
            // switches the window's team first (same reset semantics as
            // the old footer switcher — screen + back stack cleared), then
            // scopes to the picked board. One action, one gesture.
            let nav = nav_for_window(window, cx);
            let board_team = Store::global(cx)
                .collections()
                .boards
                .read(cx)
                .get(&board_id)
                .map(|board| board.team_id.clone());
            if let Some(board_team) = board_team {
                if active_team_id(&nav, cx).as_deref()
                    != Some(board_team.as_str())
                {
                    switch_team(window, cx, board_team);
                }
            }
            set_active_board(window, cx, board_id.clone());
            navigate(window, cx, Screen::BoardIssues { board_id });
        });
    });
    cx.on_action(|action: &OpenIssue, cx| {
        let issue_id = action.issue_id.clone();
        navigate_active(cx, Screen::IssueDetail { issue_id });
    });
    cx.on_action(|action: &SwitchTeam, cx| {
        let team_id = action.team_id.clone();
        on_active_window(cx, move |window, cx| {
            switch_team(window, cx, team_id);
        });
    });
    // EXP-723: the rail's account menu is What's new / About / Sign out. Both
    // of the first two are App-global actions so the keymap can bind them;
    // the rail's own affordances still call the openers directly (EXP-17).
    cx.on_action(|_: &OpenWhatsNew, cx| {
        on_active_window(cx, |window, cx| {
            crate::changelog::open_whats_new(window, cx);
        });
    });
    cx.on_action(|_: &OpenAbout, cx| {
        on_active_window(cx, |window, cx| {
            crate::sidebar::select_settings_section(
                window,
                cx,
                crate::settings::SettingsSection::About,
            );
            navigate(window, cx, Screen::Settings);
        });
    });
    cx.on_action(|_: &GoBack, cx| {
        on_active_window(cx, |window, cx| go_back(window, cx));
    });
    cx.on_action(|_: &GoForward, cx| {
        on_active_window(cx, |window, cx| go_forward(window, cx));
    });
    // App-global back/forward bindings (§8.11): `cmd-[` / `cmd-]` on macOS,
    // `Alt+Left` / `Alt+Right` everywhere (the browser chords). `None`
    // context = fires regardless of focus, matching the ⌘K search binding.
    // EXP-818: the mouse's back/forward buttons dispatch the same two
    // actions from the shell root (`shell::Shell::render`).
    #[cfg(target_os = "macos")]
    cx.bind_keys([
        KeyBinding::new("cmd-[", GoBack, None),
        KeyBinding::new("alt-left", GoBack, None),
        KeyBinding::new("cmd-]", GoForward, None),
        KeyBinding::new("alt-right", GoForward, None),
    ]);
    #[cfg(not(target_os = "macos"))]
    cx.bind_keys([
        KeyBinding::new("alt-left", GoBack, None),
        KeyBinding::new("alt-right", GoForward, None),
    ]);
}

fn navigate_active(cx: &mut App, screen: Screen) {
    on_active_window(cx, move |window, cx| {
        navigate(window, cx, screen);
    });
}

// -----------------------------------------------------------------------
// Default-screen resolution (shared by screens panel + sidebar highlight)
// -----------------------------------------------------------------------

/// The window's active SCREEN. EXP-851 gave the default back: with no
/// explicit navigation yet (a fresh window, a team switch, the last tab
/// closed) a team that HAS boards opens on its board list — the issues-first
/// default the retired rail tool used to provide. `None` (the empty state)
/// means there is genuinely nothing to show: no team, or no board yet.
///
/// The default names no board (the empty sentinel) so it follows
/// `active_board_id` — which must therefore never recurse through here for
/// it, and doesn't ([`active_board_id`] skips the empty sentinel).
pub fn resolved_screen(nav: &Entity<Navigation>, cx: &App) -> Option<Screen> {
    if let Some(screen) = nav.read(cx).screen.clone() {
        return Some(screen);
    }
    // `try_global`: the panel-rehydrate tests build windows without a store.
    let collections = Store::try_global(cx)?.collections();
    let team_id = active_team_id(nav, cx)?;
    (!collections.boards_in_team(&team_id, cx).is_empty()).then(|| Screen::BoardIssues {
        board_id: String::new(),
    })
}

/// Whether the teams + boards shapes have seen their first
/// `up-to-date` — the gate between "skeleton" and real empty states.
pub fn shapes_ready(cx: &App) -> bool {
    let collections = Store::global(cx).collections();
    collections.boards.read(cx).is_ready() && collections.teams.read(cx).is_ready()
}

/// The window's active BOARD — the scope every repo-backed surface (files
/// tree, git chrome, run configs, source control, `+` shell cwd, board
/// picker, All Issues list) resolves through. Resolution order: the
/// explicitly picked board (while it still exists in the active
/// team), then the active issue tab's board (its synced row), then
/// the team's first board.
pub fn active_board_id(nav: &Entity<Navigation>, cx: &App) -> Option<String> {
    let collections = Store::global(cx).collections();
    let team_id = active_team_id(nav, cx)?;
    if let Some(picked) = nav.read(cx).last_board_id.clone() {
        let still_here = collections
            .boards
            .read(cx)
            .get(&picked)
            .is_some_and(|board| board.team_id == team_id);
        if still_here {
            return Some(picked);
        }
    }
    // EXP-851: the board LIST screen names its board outright.
    if let Some(Screen::BoardIssues { board_id }) = resolved_screen(nav, cx) {
        if !board_id.is_empty()
            && collections
                .boards
                .read(cx)
                .get(&board_id)
                .is_some_and(|board| board.team_id == team_id)
        {
            return Some(board_id);
        }
    }
    if let Some(Screen::IssueDetail { issue_id }) = resolved_screen(nav, cx) {
        if let Some(board_id) = collections
            .issues
            .read(cx)
            .get(&issue_id)
            .map(|issue| issue.board_id.clone())
        {
            return Some(board_id);
        }
    }
    collections
        .boards_in_team(&team_id, cx)
        .first()
        .map(|board| board.id.clone())
}

/// The active team id: the explicit selection when it still exists,
/// else the first synced team WITH boards (name-sorted, web picker
/// order), else the plain first team. The boards preference is EXP-181:
/// without a persisted selection the name-sort used to land startups on
/// an empty personal team ("Select board", blank lists) while another
/// team was fully usable. Before the boards shape's first sync every
/// team looks empty and the plain first team wins — the resolution is
/// query-time, so it self-corrects the moment boards land.
pub fn active_team_id(nav: &Entity<Navigation>, cx: &App) -> Option<String> {
    let collections = Store::global(cx).collections();
    let selected = nav.read(cx).team_id.clone();
    if let Some(id) = selected {
        if collections.teams.read(cx).get(&id).is_some() {
            return Some(id);
        }
    }
    let teams = collections.teams_sorted(cx);
    teams
        .iter()
        .find(|team| !collections.boards_in_team(&team.id, cx).is_empty())
        .or_else(|| teams.first())
        .map(|team| team.id.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// EXP-686: the three full-page rail screens each have their own
    /// `EXP_DEV_SCREEN` value — a capture run reaches Devices and Automations
    /// without synthetic input, and the pre-split `actions` value keeps
    /// meaning the Actions list.
    #[test]
    fn dev_screen_values_cover_the_full_page_screens() {
        assert_eq!(parse_dev_screen("devices"), Some(Screen::Devices));
        assert_eq!(parse_dev_screen("actions"), Some(Screen::Actions));
        assert_eq!(parse_dev_screen("automations"), Some(Screen::Automations));
        // EXP-706: Reviews joined them (it was a rail TOOL window before).
        assert_eq!(parse_dev_screen("reviews"), Some(Screen::Reviews));
        // EXP-818: Usage folded into the Devices page.
        assert_eq!(parse_dev_screen("usage"), Some(Screen::Devices));
        assert_eq!(parse_dev_screen("settings"), Some(Screen::Settings));
        // EXP-238: the legacy Account value still lands on Settings.
        assert_eq!(parse_dev_screen("account"), Some(Screen::Settings));
        assert_eq!(
            parse_dev_screen("issue:abc"),
            Some(Screen::IssueDetail { issue_id: "abc".into() })
        );
        assert_eq!(parse_dev_screen("nonsense"), None);
        // The old EXP-530 tab values were never screens.
        assert_eq!(parse_dev_screen("suggestions"), None);
    }

    /// EXP-851: the rail's tool windows are SCREENS — both spellings of every
    /// capture drive (`EXP_DEV_SCREEN` and the legacy `EXP_DEV_TOOL`) land on
    /// the same screen, so the committed view catalog keeps resolving.
    #[test]
    fn the_list_screens_have_dev_values_under_both_spellings() {
        use crate::sidebar::InboxTab;
        let board = Screen::BoardIssues {
            board_id: String::new(),
        };
        for spec in ["board-issues", "board", "issues"] {
            assert_eq!(parse_dev_screen(spec), Some(board.clone()), "{spec}");
            assert_eq!(dev_tool_screen(spec), Some(board.clone()), "{spec}");
        }
        assert_eq!(
            parse_dev_screen("inbox"),
            Some(Screen::Inbox {
                tab: InboxTab::Inbox
            })
        );
        assert_eq!(
            parse_dev_screen("inbox-my-issues"),
            Some(Screen::Inbox {
                tab: InboxTab::MyIssues
            })
        );
        assert_eq!(
            dev_tool_screen("my-issues"),
            Some(Screen::Inbox {
                tab: InboxTab::MyIssues
            })
        );
        assert_eq!(parse_dev_screen("support"), Some(Screen::Support));
        assert_eq!(dev_tool_screen("support"), Some(Screen::Support));
        assert_eq!(parse_dev_screen("files"), Some(Screen::Files));
        assert_eq!(dev_tool_screen("files"), Some(Screen::Files));
        assert_eq!(parse_dev_screen("source-control"), Some(Screen::SourceControl));
        assert_eq!(dev_tool_screen("source-control"), Some(Screen::SourceControl));
        // EXP-818/EXP-706 spellings the catalog still carries.
        assert_eq!(dev_tool_screen("agent"), Some(Screen::Chat));
        assert_eq!(dev_tool_screen("sessions"), Some(Screen::Chat));
        assert_eq!(dev_tool_screen("reviews"), Some(Screen::Reviews));
        assert_eq!(dev_tool_screen("nonsense"), None);
    }

    /// EXP-851: `EXP_DEV_TOOL` beside a DETAIL screen means "that list in the
    /// left column" — the pair a capture run uses for the ListNav views.
    #[test]
    fn dev_inbox_tab_parses_both_values() {
        use crate::sidebar::InboxTab;
        assert_eq!(parse_dev_inbox_tab("inbox"), Some(InboxTab::Inbox));
        assert_eq!(parse_dev_inbox_tab("my-issues"), Some(InboxTab::MyIssues));
        assert_eq!(parse_dev_inbox_tab("My-Issues"), None);
    }

    /// The Getting-started tab override parses exactly the two documented
    /// values; anything else falls back to the checklist.
    #[test]
    fn getting_started_tab_override_parses_both_tabs() {
        assert_eq!(
            parse_getting_started_tab("first-steps"),
            Some(GettingStartedTab::FirstSteps)
        );
        assert_eq!(
            parse_getting_started_tab("suggestions"),
            Some(GettingStartedTab::Suggestions)
        );
        assert_eq!(parse_getting_started_tab(""), None);
        assert_eq!(parse_getting_started_tab("Suggestions"), None);
        assert_eq!(GettingStartedTab::default(), GettingStartedTab::FirstSteps);
    }

    /// EXP-851: Reviews is a LIST screen — a detail opened from it inherits
    /// the Reviews rows in the left column. Neither a tab nor undockable
    /// (the DIFF its rows open is both).
    #[test]
    fn reviews_is_a_list_screen() {
        assert!(Screen::Reviews.list_origin().is_some());
        assert_eq!(
            Screen::Reviews.list_origin().map(|origin| origin.tool),
            Some(crate::sidebar::ToolWindow::Reviews)
        );
        assert!(!Screen::Reviews.is_detail());
        assert!(!Screen::Reviews.undockable());
        assert!(Screen::PrDiff {
            issue_id: "i1".into()
        }
        .undockable());
    }

    /// EXP-746: a capture run (and a debug session) reaches ONE coding
    /// session's screen by its row id, the same shape as `issue:` / `pr:`.
    #[test]
    fn dev_screen_parses_a_session_id() {
        assert_eq!(
            parse_dev_screen("session:5f2a"),
            Some(Screen::Session {
                session_id: "5f2a".into()
            })
        );
        // The prefix is the whole grammar: a bare word is not a session.
        assert_eq!(parse_dev_screen("session"), None);
        // An empty id parses (it simply matches no row) — the screen degrades
        // to its generic title rather than the app pre-routing nowhere.
        assert_eq!(
            parse_dev_screen("session:"),
            Some(Screen::Session {
                session_id: String::new()
            })
        );
    }

    /// EXP-746: a session gets a tab chip (several runs are open at once, and
    /// an ended one stays as a read-only transcript), but it is neither
    /// undockable — a fresh view in a second window would mean a second
    /// engine handle for one live agent — nor a LIST a detail could inherit.
    #[test]
    fn session_screens_are_detail_but_not_undockable_or_a_list() {
        let session = Screen::Session {
            session_id: "s1".into(),
        };
        assert!(session.is_detail());
        assert!(!session.undockable());
        assert!(session.list_origin().is_none());
    }

    /// EXP-851/EXP-862: exactly six screens are LISTS — a board, the Inbox,
    /// Support, the Agent page, Reviews and (EXP-862) the Automations page's
    /// run log. Every other screen (and every detail) is context-free: a
    /// detail opened from it keeps the rail up.
    #[test]
    fn the_list_screens_are_the_six_the_left_column_can_show() {
        use crate::sidebar::{InboxTab, ToolWindow};
        let board = Screen::BoardIssues {
            board_id: "b1".into(),
        };
        assert_eq!(
            board.list_origin(),
            Some(TabOrigin {
                tool: ToolWindow::BoardIssues,
                board_id: Some("b1".into()),
                inbox_tab: None,
            })
        );
        // The dev sentinel (`EXP_DEV_TOOL=board`) names no board — the render
        // resolves the window's active one.
        assert_eq!(
            Screen::BoardIssues {
                board_id: String::new()
            }
            .list_origin()
            .and_then(|origin| origin.board_id),
            None
        );
        assert_eq!(
            Screen::Inbox {
                tab: InboxTab::MyIssues
            }
            .list_origin(),
            Some(TabOrigin {
                tool: ToolWindow::Inbox,
                board_id: None,
                inbox_tab: Some(InboxTab::MyIssues),
            })
        );
        for (screen, tool) in [
            (Screen::Support, ToolWindow::Support),
            (Screen::Chat, ToolWindow::Sessions),
            (Screen::Reviews, ToolWindow::Reviews),
            (Screen::Automations, ToolWindow::Automations),
        ] {
            assert_eq!(screen.list_origin().map(|origin| origin.tool), Some(tool));
        }
        for screen in [
            Screen::Settings,
            Screen::Devices,
            Screen::Actions,
            Screen::Files,
            Screen::SourceControl,
            Screen::GettingStarted {
                tab: GettingStartedTab::FirstSteps,
            },
            Screen::IssueDetail {
                issue_id: "i1".into(),
            },
            Screen::PrDiff {
                issue_id: "i1".into(),
            },
            Screen::SupportThread {
                thread_id: "t1".into(),
            },
            Screen::Session {
                session_id: "s1".into(),
            },
        ] {
            assert!(screen.list_origin().is_none(), "{screen:?}");
        }
    }

    /// EXP-851: the breadcrumb rule, one row per navigation the product can
    /// make. A detail keeps the list it was opened FROM, inherits one from
    /// another detail, and gets NOTHING anywhere else — the rail stays.
    #[test]
    fn derive_origin_carries_a_list_one_layer_and_never_invents_one() {
        use crate::sidebar::{InboxTab, ToolWindow};
        let issue = Screen::IssueDetail {
            issue_id: "i1".into(),
        };
        let session = Screen::Session {
            session_id: "s1".into(),
        };
        let ticket = Screen::SupportThread {
            thread_id: "t1".into(),
        };
        let diff = Screen::PrDiff {
            issue_id: "i1".into(),
        };
        let board_screen = Screen::BoardIssues {
            board_id: "b1".into(),
        };
        let board = board_screen.list_origin().unwrap();
        let inbox_screen = Screen::Inbox {
            tab: InboxTab::Inbox,
        };
        let inbox = inbox_screen.list_origin().unwrap();

        // A board list → an issue: the board comes along.
        assert_eq!(
            derive_origin(Some(&board_screen), None, &issue),
            Some(board.clone())
        );
        // The Inbox → an issue: the Inbox, with the tab it was on.
        assert_eq!(
            derive_origin(Some(&inbox_screen), None, &issue),
            Some(inbox.clone())
        );
        // Support → a ticket.
        assert_eq!(
            derive_origin(Some(&Screen::Support), None, &ticket)
                .map(|origin| origin.tool),
            Some(ToolWindow::Support)
        );
        // The Agent page reached from the RAIL (carrying nothing) → a session:
        // its own sessions list comes along.
        assert_eq!(
            derive_origin(Some(&Screen::Chat), None, &session).map(|origin| origin.tool),
            Some(ToolWindow::Sessions)
        );
        // … but reached from an issue that sits beside its board, the Agent
        // page CARRIES that board, and so does the run it starts (EXP-851's
        // start-coding chain: board → issue → Chat → session).
        assert_eq!(
            derive_origin(Some(&issue), Some(board.clone()), &Screen::Chat),
            Some(board.clone())
        );
        assert_eq!(
            derive_origin(Some(&Screen::Chat), Some(board.clone()), &session),
            Some(board.clone())
        );
        // The rail's own Agent entry is a `navigate_from_rail`, so nothing is
        // derived for it at all — the marker wins (screens::resolve_tab_origin).
        // Reviews → the PR diff.
        assert_eq!(
            derive_origin(Some(&Screen::Reviews), None, &diff).map(|origin| origin.tool),
            Some(ToolWindow::Reviews)
        );
        // Detail → detail INHERITS: an issue (opened from a board) starting a
        // coding run keeps the board beside the session.
        assert_eq!(
            derive_origin(Some(&issue), Some(board.clone()), &session),
            Some(board.clone())
        );
        // … and a detail with no list of its own hands on nothing.
        assert_eq!(derive_origin(Some(&issue), None, &session), None);
        // EXP-851's change: a context-free screen (the rail's pages, Settings,
        // Files) derives NO list — the rail stays up.
        for previous in [
            Screen::Devices,
            Screen::Actions,
            Screen::Settings,
            Screen::Files,
            Screen::SourceControl,
            Screen::GettingStarted {
                tab: GettingStartedTab::FirstSteps,
            },
        ] {
            assert_eq!(derive_origin(Some(&previous), None, &issue), None, "{previous:?}");
            assert_eq!(derive_origin(Some(&previous), None, &session), None);
        }
        // EXP-862: the Automations page IS a list (its run log), so a run
        // opened from it comes along with it.
        assert_eq!(
            derive_origin(Some(&Screen::Automations), None, &session).map(|origin| origin.tool),
            Some(ToolWindow::Automations)
        );
        // A deep link at boot (nothing before) has no list either.
        assert_eq!(derive_origin(None, None, &issue), None);
        // A plain LIST screen never gets a left-column list of its own.
        assert_eq!(derive_origin(Some(&board_screen), None, &inbox_screen), None);
        assert_eq!(derive_origin(Some(&board_screen), None, &Screen::Reviews), None);
    }

    /// EXP-827: a session's Back follows the breadcrumb, not history — the
    /// issue it was opened beside, or the list its row is in.
    #[test]
    fn session_back_follows_the_tab_origin() {
        use crate::sidebar::{InboxTab, ToolWindow};
        let inbox = TabOrigin {
            tool: ToolWindow::Inbox,
            board_id: None,
            inbox_tab: Some(InboxTab::Inbox),
        };
        let board = TabOrigin {
            tool: ToolWindow::BoardIssues,
            board_id: Some("b1".into()),
            inbox_tab: None,
        };
        let sessions = TabOrigin {
            tool: ToolWindow::Sessions,
            board_id: None,
            inbox_tab: None,
        };
        let issue = Screen::IssueDetail {
            issue_id: "i1".into(),
        };
        // Opened from a board / the Inbox beside its issue: back to the issue
        // (where Watch again is).
        assert_eq!(session_back_target(Some(&board), Some("i1")), issue);
        assert_eq!(session_back_target(Some(&inbox), Some("i1")), issue);
        // Opened from the Agent page: back to the Agent page, even for an
        // issue-bound run — the list the row is in is the one to return to.
        assert_eq!(session_back_target(Some(&sessions), Some("i1")), Screen::Chat);
        // EXP-862: opened from the Automations page's run log — back THERE,
        // issue-bound or not. An unattended run has no row on the Agent page,
        // so sending Back to it would land on a list the run is not in.
        let automations = TabOrigin {
            tool: ToolWindow::Automations,
            board_id: None,
            inbox_tab: None,
        };
        assert_eq!(
            session_back_target(Some(&automations), Some("i1")),
            Screen::Automations
        );
        assert_eq!(
            session_back_target(Some(&automations), None),
            Screen::Automations
        );
        // A batch / action / chat run has no issue to return to.
        assert_eq!(session_back_target(Some(&board), None), Screen::Chat);
        // No tab origin at all (a closed tab, a fresh window): the Agent page.
        assert_eq!(session_back_target(None, Some("i1")), Screen::Chat);
    }

    /// EXP-818: go-back parks the screen it left for go-forward; a real
    /// navigation forks history and drops the forward stack.
    #[test]
    fn forward_stack_is_browser_shaped() {
        let mut nav = Navigation::new();
        nav.screen = Some(Screen::Devices);
        nav.back_stack.push(Screen::Reviews);
        assert!(nav.can_go_back());
        assert!(!nav.can_go_forward());
        // go_back's bookkeeping
        assert_eq!(nav.step_back(), Some(Screen::Reviews));
        assert_eq!(nav.screen, Some(Screen::Reviews));
        assert_eq!(nav.forward_stack, vec![Screen::Devices]);
        assert!(nav.can_go_forward());
        // go_forward's bookkeeping
        assert_eq!(nav.step_forward(), Some(Screen::Devices));
        assert_eq!(nav.back_stack, vec![Screen::Reviews]);
        assert!(!nav.can_go_forward());
        assert_eq!(nav.step_back(), Some(Screen::Reviews));
        // a real navigation (navigate_inner's bookkeeping)
        nav.back_stack.push(nav.screen.take().unwrap());
        nav.screen = Some(Screen::Actions);
        nav.forward_stack.clear();
        assert!(!nav.can_go_forward());
    }

    /// EXP-818: a purge drops matching entries from BOTH stacks — the
    /// forward stack is the same history read the other way, so purging
    /// the back stack alone left go-forward re-entering the screen.
    #[test]
    fn purge_history_covers_both_stacks() {
        let diff = Screen::PrDiff { issue_id: "i1".into() };
        let mut nav = Navigation::new();
        nav.screen = Some(Screen::Reviews);
        nav.back_stack = vec![Screen::Devices, diff.clone(), Screen::Actions];
        nav.forward_stack = vec![diff.clone(), Screen::Settings, diff.clone()];
        assert!(nav.purge_history(|screen| *screen == diff));
        assert_eq!(nav.back_stack, vec![Screen::Devices, Screen::Actions]);
        assert_eq!(nav.forward_stack, vec![Screen::Settings]);
        // the current screen is not history
        assert_eq!(nav.screen, Some(Screen::Reviews));
        // idempotent: a second purge changes nothing
        assert!(!nav.purge_history(|screen| *screen == diff));
    }

    /// EXP-525/EXP-818: `screens::dismiss_stale_pr_diff` purges, goes back,
    /// then purges again — the go-back parks the dismissed diff on the
    /// forward stack, and the second purge is what keeps Forward alive
    /// (before it, Forward re-entered the diff, the dismiss fired again and
    /// the diff landed forward again: a dead button for the window's life).
    #[test]
    fn dismissed_pr_diff_never_survives_on_the_forward_stack() {
        let diff = Screen::PrDiff { issue_id: "i1".into() };
        let mut nav = Navigation::new();
        // Reviews → diff → back to Reviews → forward into the diff again:
        // the diff sits on the current slot, nothing on the back stack but
        // Reviews, and an unrelated screen is parked forward.
        nav.screen = Some(diff.clone());
        nav.back_stack = vec![Screen::Devices, Screen::Reviews];
        nav.forward_stack = vec![Screen::Settings];
        // the dismiss sequence
        nav.purge_history(|screen| *screen == diff);
        assert_eq!(nav.step_back(), Some(Screen::Reviews));
        assert_eq!(nav.forward_stack, vec![Screen::Settings, diff.clone()]);
        nav.purge_history(|screen| *screen == diff);
        assert!(!nav.forward_stack.contains(&diff));
        assert!(!nav.back_stack.contains(&diff));
        // Forward still works and lands somewhere real
        assert!(nav.can_go_forward());
        assert_eq!(nav.step_forward(), Some(Screen::Settings));
        assert_eq!(nav.back_stack, vec![Screen::Devices, Screen::Reviews]);
    }

    /// EXP-769/EXP-818: a closed terminal is purged from history as a whole
    /// — went back from it, then closed it: go-forward must not re-enter a
    /// `Screen::Terminal` with no tab (the ghost-chip case). `close_tab`'s
    /// predicate is plain screen equality; a `terminal::TabId` cannot be
    /// minted outside its crate, so the other bottom-bar tab screen stands
    /// in — the rule is the same for both.
    #[test]
    fn purged_terminal_is_not_re_entered_by_go_forward() {
        let terminal = Screen::Session { session_id: "s1".into() };
        assert!(terminal.is_dock_tab());
        let mut nav = Navigation::new();
        nav.screen = Some(terminal.clone());
        nav.back_stack = vec![Screen::Reviews];
        // the user went back from the terminal …
        assert_eq!(nav.step_back(), Some(Screen::Reviews));
        assert_eq!(nav.forward_stack, vec![terminal.clone()]);
        // … then closed it (close_tab's purge)
        assert!(nav.purge_history(|screen| *screen == terminal));
        assert!(!nav.can_go_forward());
        assert_eq!(nav.step_forward(), None);
        assert_eq!(nav.screen, Some(Screen::Reviews));
    }

    /// EXP-771: which screens a navigation checks the undock registry for.
    /// Exactly the undockable ones: a tab-less rail page, a session and a
    /// terminal can never be in an undocked SCREEN window (a terminal has its
    /// own path), so they must not pay a registry lookup — and must never be
    /// swallowed by one either.
    #[test]
    fn only_undockable_screens_reveal_an_undocked_window() {
        assert!(reveals_undocked_window(&Screen::IssueDetail {
            issue_id: "i1".into()
        }));
        assert!(reveals_undocked_window(&Screen::PrDiff {
            issue_id: "i1".into()
        }));
        assert!(!reveals_undocked_window(&Screen::Session {
            session_id: "s1".into()
        }));
        assert!(!reveals_undocked_window(&Screen::SupportThread {
            thread_id: "t1".into()
        }));
        assert!(!reveals_undocked_window(&Screen::Reviews));
        assert!(!reveals_undocked_window(&Screen::Settings));
    }

    /// The rail entries and the tab-less page headers read these titles —
    /// EXP-686 split "Agents" into three, so each screen must name itself.
    #[gpui::test]
    async fn full_page_screens_carry_their_own_titles(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| {
            assert_eq!(screen_title(&Screen::Devices, cx), "Devices");
            assert_eq!(screen_title(&Screen::Actions, cx), "Actions");
            assert_eq!(screen_title(&Screen::Automations, cx), "Automations");
            assert_eq!(screen_title(&Screen::Reviews, cx), "Reviews");
            assert_eq!(
                screen_title(
                    &Screen::GettingStarted {
                        tab: GettingStartedTab::Suggestions
                    },
                    cx
                ),
                "Getting started",
                "the tab never changes the page's identity"
            );
        });
    }

    /// EXP-825: the dev `chat?…` spec parses the web route's search params
    /// — any subset, csv issues, percent-decoded text — and a plain `chat`
    /// (or any other spec) seeds nothing.
    #[test]
    fn dev_chat_seed_parses_the_web_search_params() {
        assert_eq!(parse_dev_chat_seed("chat"), None);
        assert_eq!(parse_dev_chat_seed("devices"), None);
        assert_eq!(parse_dev_chat_seed("chat?"), Some(ChatSeed::default()));
        assert_eq!(parse_dev_screen("chat?issues=a"), Some(Screen::Chat));
        let full = parse_dev_chat_seed(
            "chat?issues=a,b,%20&action=builtin:fix-conflicts&pr=i-1&device=dev-1\
&text=Review%20%23EXP-1+please&icon=bug&bogus=1",
        )
        .unwrap();
        assert_eq!(
            full,
            ChatSeed {
                issue_ids: vec!["a".into(), "b".into()],
                action_id: Some("builtin:fix-conflicts".into()),
                device_id: Some("dev-1".into()),
                pr_issue_id: Some("i-1".into()),
                text: Some("Review #EXP-1 please".into()),
                icon: Some("bug".into()),
            }
        );
        // A subset leaves the rest empty; blank values are absent.
        assert_eq!(
            parse_dev_chat_seed("chat?action=&text=hi"),
            Some(ChatSeed {
                text: Some("hi".into()),
                ..Default::default()
            })
        );
        assert_eq!(percent_decode("a%2Bb%zz"), "a+b%zz");
    }

    /// The seed constructors name exactly one thing each, and the
    /// fix-conflicts one pins the builtin beside its PR.
    #[test]
    fn chat_seed_constructors() {
        assert_eq!(ChatSeed::issues(vec!["i".into()]).issue_ids, vec!["i".to_string()]);
        assert_eq!(ChatSeed::action("act").action_id.as_deref(), Some("act"));
        assert_eq!(ChatSeed::device("dev").device_id.as_deref(), Some("dev"));
        let fix = ChatSeed::fix_conflicts("issue-9");
        assert_eq!(fix.action_id.as_deref(), Some("builtin:fix-conflicts"));
        assert_eq!(fix.pr_issue_id.as_deref(), Some("issue-9"));
        assert!(fix.issue_ids.is_empty());
    }
}
