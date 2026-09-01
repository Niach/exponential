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
    GoBack, OpenInbox, OpenIssue, OpenMyIssues, OpenBoard, OpenSettings,
    OpenSourceControl, SwitchTeam, SyncNow,
};

/// One center TAB (§4.2, reworked): the center pane is tab-based — every
/// `Screen` value identifies one openable tab (issues and files can be open
/// several at a time; Source Control / Settings are singletons).
/// `None` on [`Navigation::screen`] means "no tab active" — the center shows
/// its empty state. Issue LISTS are not screens: they live in the sidebar
/// tool windows (the rail's Inbox / My Issues / All Issues).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Screen {
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
    /// The Reviews page (EXP-706 — the web `t/$teamSlug/reviews` page: every
    /// open PR across the team, issue-linked ones grouped by board plus the
    /// unlinked ones grouped by repo). It used to be a sidebar TOOL WINDOW
    /// (a split beside the diff); it is now a tab-less full-page screen like
    /// Devices / Actions, and the PR diff is the center view its rows open.
    Reviews,
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
    pub(crate) fn undockable(&self) -> bool {
        matches!(self, Screen::IssueDetail { .. } | Screen::PrDiff { .. })
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
    /// `ScreensPanel::dismiss_stale_pr_diff` retires them.
    pub(crate) fn is_detail(&self) -> bool {
        matches!(
            self,
            Screen::IssueDetail { .. } | Screen::SupportThread { .. }
        )
    }

    /// EXP-480/EXP-686/EXP-706: the tab-less FULL-PAGE screens the rail
    /// navigates to directly. While one is up the tool column unmounts and exactly one
    /// rail entry may read as selected (Settings is full-page too, but it
    /// replaces the rail outright — the rail highlight rules don't apply).
    pub(crate) fn is_rail_full_page(&self) -> bool {
        matches!(
            self,
            Screen::Devices
                | Screen::Actions
                | Screen::Automations
                | Screen::Reviews
                | Screen::GettingStarted { .. }
        )
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

/// The pending origin marker a navigation leaves for the screens panel
/// (consumed like [`Navigation::replaced_screen`]): `Capture` = read the
/// CURRENT rail tool + active board at consume time (right for every
/// sidebar-row click path); `Explicit` = the caller knows better (create
/// dialog, deep links — the rail may point anywhere).
pub(crate) enum PendingOrigin {
    Capture,
    Explicit(TabOrigin),
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
        Screen::Devices => "Devices".into(),
        Screen::Actions => "Actions".into(),
        Screen::Automations => "Automations".into(),
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

/// Per-window navigation state. Mutate through [`navigate`] /
/// [`switch_team`] / [`go_back`] so observers fire consistently.
pub struct Navigation {
    /// Selected team; `None` = "first synced team" (resolved at
    /// query time — a fresh install has no selection until sync lands).
    pub team_id: Option<String>,
    screen: Option<Screen>,
    back_stack: Vec<Screen>,
    /// The explicitly selected board (the top-bar picker) — the primary
    /// scope for [`active_board_id`] so the picker / files / git / run
    /// surfaces stay populated on every screen.
    last_board_id: Option<String>,
    /// The screen the last [`replace_screen`] displaced (EXP-48 prev/next):
    /// a pending marker the screens panel consumes to swap that tab's
    /// identity in place instead of opening a new tab. Cleared by every
    /// ordinary navigation.
    replaced_screen: Option<Screen>,
    /// EXP-288: the pending tab-origin marker the screens panel consumes
    /// when it opens/updates a tab for the navigated screen. Set by
    /// [`navigate`]/[`navigate_from`]/[`replace_screen`]; cleared by
    /// [`set_screen`]/[`go_back`]/[`switch_team`] so tab activation never
    /// rewrites a tab's remembered origin.
    pending_origin: Option<PendingOrigin>,
}

impl Navigation {
    fn new() -> Self {
        Self {
            // DEV-ONLY (§11.4 headless verification, same family as
            // EXP_DEV_SERVER/EXP_DEV_BOARD): pre-select a team and/or
            // pre-route the first screen so gate screenshots can reach
            // surfaces without synthetic input. Unset in normal runs.
            team_id: std::env::var("EXP_DEV_TEAM").ok(),
            screen: std::env::var("EXP_DEV_SCREEN")
                .ok()
                .as_deref()
                .and_then(parse_dev_screen)
                .or_else(legacy_reviews_tool_screen),
            back_stack: Vec::new(),
            // DEV-ONLY `EXP_DEV_BOARD_ID=<board uuid>` (EXP-642): pre-select
            // the board the first render opens — `EXP_DEV_BOARD=1` is the
            // (unrelated) debug-board switch, hence the `_ID` suffix. A blank
            // value is ignored. Never document for users.
            last_board_id: std::env::var("EXP_DEV_BOARD_ID")
                .ok()
                .map(|id| id.trim().to_string())
                .filter(|id| !id.is_empty()),
            replaced_screen: None,
            pending_origin: None,
        }
    }

    /// The current screen, `None` until first navigation (default applies).
    #[allow(dead_code)] // views read via `resolved_screen`; raw access for later steps
    pub fn screen(&self) -> Option<&Screen> {
        self.screen.as_ref()
    }

    /// Whether [`go_back`] has anywhere to go.
    #[allow(dead_code)]
    pub fn can_go_back(&self) -> bool {
        !self.back_stack.is_empty()
    }
}

/// DEV-ONLY `EXP_DEV_SCREEN` values: `settings` | `account` | `devices` |
/// `actions` | `automations` | `reviews` | `getting-started` | `issue:<uuid>` |
/// `pr:<issue-uuid>` (the PR-diff screen, keyed by the ISSUE whose linked PR
/// it shows) | `support:<uuid>` (anything else = no pre-route).
/// `getting-started` additionally reads `EXP_DEV_GETTING_STARTED_TAB`
/// ([`parse_getting_started_tab`]) so a capture run can land on the
/// suggestions tab without synthetic input.
fn parse_dev_screen(spec: &str) -> Option<Screen> {
    match spec {
        "settings" => Some(Screen::Settings),
        // EXP-238: Account merged into Settings — the dev value keeps working.
        "account" => Some(Screen::Settings),
        "devices" => Some(Screen::Devices),
        "actions" => Some(Screen::Actions),
        "automations" => Some(Screen::Automations),
        // EXP-706: Reviews left the rail's tool windows for its own page.
        "reviews" => Some(Screen::Reviews),
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
            spec.strip_prefix("support:")
                .map(|id| Screen::SupportThread {
                    thread_id: id.to_string(),
                })
        }
    }
}

/// DEV-ONLY back-compat for capture runs (EXP-706): Reviews used to be a rail
/// TOOL window, so the shots catalog drives it with `EXP_DEV_TOOL=reviews`.
/// It is a full-page SCREEN now — keep the old spelling landing on it (only
/// when `EXP_DEV_SCREEN` picked nothing, which stays authoritative).
fn legacy_reviews_tool_screen() -> Option<Screen> {
    (std::env::var("EXP_DEV_TOOL").ok().as_deref() == Some("reviews")).then_some(Screen::Reviews)
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
    navigate_inner(window, cx, screen, PendingOrigin::Capture);
}

/// [`navigate`] with an EXPLICIT tab origin (EXP-288).
pub(crate) fn navigate_from(window: &Window, cx: &mut App, screen: Screen, origin: TabOrigin) {
    navigate_inner(window, cx, screen, PendingOrigin::Explicit(origin));
}

/// Open an issue's detail LANDING FULLY SCOPED on its board (EXP-510): the
/// live rail switches to the board list (`select_tool_for_tab` — never
/// `activate_tool`, whose `set_screen(None)` would close the tab being
/// opened), the board becomes active, and the tab's origin is the board
/// explicitly. For paths where the rail may point anywhere while the
/// navigation fires (create dialog, deep links) — sidebar-row clicks keep
/// plain [`navigate`], their tool is already active.
pub(crate) fn open_issue_scoped(
    window: &mut Window,
    cx: &mut App,
    issue_id: String,
    board_id: String,
) {
    crate::sidebar::select_tool_for_tab(window, cx, crate::sidebar::ToolWindow::BoardIssues);
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

fn navigate_inner(window: &Window, cx: &mut App, screen: Screen, origin: PendingOrigin) {
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
        nav.replaced_screen = None;
        nav.pending_origin = Some(origin);
        cx.notify();
    });
}

/// Swap the current screen IN PLACE (EXP-48 prev/next issue switcher): no
/// back-stack push, and the screens panel replaces the active tab's identity
/// instead of opening a new tab (via the consumed [`take_replaced_screen`]
/// marker). No-op when already on `screen`.
pub fn replace_screen(window: &Window, cx: &mut App, screen: Screen) {
    let Some(nav) = nav_for_window_readonly(window, cx) else {
        return;
    };
    nav.update(cx, |nav, cx| {
        if nav.screen.as_ref() == Some(&screen) {
            return;
        }
        nav.replaced_screen = nav.screen.replace(screen);
        // The swapped tab KEEPS its origin (the screens panel only reads
        // this marker when it pushes a brand-new tab, i.e. the replaced
        // screen's tab was already closed).
        nav.pending_origin = Some(PendingOrigin::Capture);
        cx.notify();
    });
}

/// Consume the pending in-place-replacement marker (the screen the last
/// [`replace_screen`] displaced). The screens panel calls this from its nav
/// observer to swap that tab's identity instead of pushing a new tab.
pub fn take_replaced_screen(nav: &Entity<Navigation>, cx: &mut App) -> Option<Screen> {
    nav.update(cx, |nav, _| nav.replaced_screen.take())
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
            nav.replaced_screen = None;
            nav.pending_origin = None;
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

/// Drop every back-stack entry matching `pred` (EXP-525: a dismissed stale
/// PR diff must not be resurrectable via go-back).
pub(crate) fn purge_from_back_stack(
    window: &Window,
    cx: &mut App,
    pred: impl Fn(&Screen) -> bool,
) {
    let Some(nav) = nav_for_window_readonly(window, cx) else {
        return;
    };
    nav.update(cx, |nav, cx| {
        let before = nav.back_stack.len();
        nav.back_stack.retain(|screen| !pred(screen));
        if nav.back_stack.len() != before {
            cx.notify();
        }
    });
}

/// Pop the back stack (issue detail → board, …).
pub fn go_back(window: &Window, cx: &mut App) {
    let Some(nav) = nav_for_window_readonly(window, cx) else {
        return;
    };
    nav.update(cx, |nav, cx| {
        if let Some(previous) = nav.back_stack.pop() {
            nav.screen = Some(previous);
            nav.replaced_screen = None;
            nav.pending_origin = None;
            cx.notify();
        }
    });
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
        nav.last_board_id = None;
        nav.replaced_screen = None;
        nav.pending_origin = None;
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
    // My Issues / Inbox are tabs of the ONE Inbox tool window (EXP-186), not
    // screens — the actions select the rail tool + tab (the sidebar swaps to
    // the mini list; the center stays).
    cx.on_action(|_: &OpenMyIssues, cx| {
        on_active_window(cx, |window, cx| {
            crate::sidebar::open_inbox_tab(window, cx, crate::sidebar::InboxTab::MyIssues);
        });
    });
    cx.on_action(|_: &OpenInbox, cx| {
        on_active_window(cx, |window, cx| {
            crate::sidebar::open_inbox_tab(window, cx, crate::sidebar::InboxTab::Inbox);
        });
    });
    cx.on_action(|_: &OpenSettings, cx| navigate_active(cx, Screen::Settings));
    cx.on_action(|_: &OpenSourceControl, cx| {
        on_active_window(cx, |window, cx| {
            crate::sidebar::activate_tool(window, cx, crate::sidebar::ToolWindow::SourceControl);
        });
    });
    // Manual freshness sync (fetch + ff-only catch-up) on the trunk engine.
    cx.on_action(|_: &SyncNow, cx| {
        on_active_window(cx, |window, cx| {
            let shared = crate::sidebar::rail_shared_for_window(window, cx);
            let trunk_sync = shared.read(cx).trunk_sync().clone();
            trunk_sync.update(cx, |engine, cx| engine.refresh(window, cx));
        });
    });
    // The picker selects a board (scope) and brings up its issue list —
    // there is no board screen; the Board Issues tool window IS the board.
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
            set_active_board(window, cx, board_id);
            crate::sidebar::activate_tool(window, cx, crate::sidebar::ToolWindow::BoardIssues);
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
    cx.on_action(|_: &GoBack, cx| {
        on_active_window(cx, |window, cx| go_back(window, cx));
    });
    // App-global back binding (§8.11): `cmd-[` on macOS, `Alt+Left` everywhere
    // (the browser-style back chord). `None` context = fires regardless of
    // focus, matching the ⌘K search binding.
    #[cfg(target_os = "macos")]
    cx.bind_keys([
        KeyBinding::new("cmd-[", GoBack, None),
        KeyBinding::new("alt-left", GoBack, None),
    ]);
    #[cfg(not(target_os = "macos"))]
    cx.bind_keys([KeyBinding::new("alt-left", GoBack, None)]);
}

fn navigate_active(cx: &mut App, screen: Screen) {
    on_active_window(cx, move |window, cx| {
        navigate(window, cx, screen);
    });
}

// -----------------------------------------------------------------------
// Default-screen resolution (shared by screens panel + sidebar highlight)
// -----------------------------------------------------------------------

/// The active center TAB — `None` = nothing open (the center renders its
/// empty state once [`shapes_ready`]; a skeleton before). There is no default
/// screen anymore: issue lists live in the sidebar, the center only shows
/// what was explicitly opened.
pub fn resolved_screen(nav: &Entity<Navigation>, cx: &App) -> Option<Screen> {
    nav.read(cx).screen.clone()
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

    /// EXP-706: the full-page screens the rail navigates to directly — the
    /// tool column unmounts while one is up, and Reviews now belongs to them.
    /// Neither a tab nor undockable (the DIFF its rows open is both).
    #[test]
    fn reviews_is_a_rail_full_page_screen() {
        assert!(Screen::Reviews.is_rail_full_page());
        assert!(!Screen::Reviews.is_detail());
        assert!(!Screen::Reviews.undockable());
        assert!(Screen::PrDiff {
            issue_id: "i1".into()
        }
        .undockable());
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
}
