//! EXP-638: real OS notifications off the synced `notifications` shape.
//!
//! The desktop has no push channel and needs none: the server already writes
//! every inbox row (`lib/integrations/notifications.ts`, all 8 types, with a
//! full human `title` + `body`) and Electric streams it into the
//! `notifications` [`Collection`]. This module observes that collection and
//! raises a system notification through gpui's cross-platform
//! [`App::show_system_notification`] — `UNUserNotificationCenter` on macOS,
//! WinRT toasts on Windows, XDG D-Bus on Linux. No transport, no new crate,
//! no server change.
//!
//! The rules that make it not annoying:
//!
//! - **No backfill toasts.** A cold start streams the whole unread history
//!   into the collection at once. The [`Gate`] arms on the first observation
//!   where the shape `is_ready` (a cold start's first `up-to-date`; on a warm
//!   relaunch the persisted `is_live` makes that the sqlite hydrate itself),
//!   seeds a seen-id set from everything present and takes the newest
//!   `created_at` as a SERVER-clock watermark; only unseen rows newer than
//!   that can toast. So a warm relaunch stays silent for what was cached and
//!   surfaces what arrived while the app was closed (still unread) as one
//!   coalesced bundle. Sign-out / a 409 resync drops the shape out of
//!   `is_ready` and re-arms the gate, so the next hydrate is a backfill again.
//! - **Coalesced.** A batch PR merge or a busy issue lands several rows
//!   within seconds; rows arriving inside [`COALESCE_WINDOW`] collapse into
//!   ONE toast ("3 new notifications" + the first titles) routed to the Inbox.
//! - **Redundancy-suppressed.** With the app focused and the row's issue open
//!   in the active window, or the Inbox tab up, or the ticket team's Support
//!   tool up, nothing is raised — the user is already looking at it.
//! - **Honours the per-type prefs.** The server writes the inbox row for
//!   EVERY type and applies `user_notification_prefs.type_prefs` only at
//!   push/email delivery (`notificationTypeAllowed`), so a muted type still
//!   syncs a row. Toasts apply the same channel-agnostic gate: prefs are
//!   fetched once per account (`notifications.emailPrefs`, refreshed after
//!   the pane toggles a type) and a fetch failure fails OPEN, like the server.
//! - **Per-machine switch.** `Settings::os_notifications` in the local
//!   settings.json (default ON) — whether THIS machine pops toasts is a
//!   per-device intent; the Notifications settings pane owns the toggle.
//!
//! Click routing reuses the rail Inbox's own paths: an issue-anchored row
//! opens the issue detail fully scoped on its board (switching the window's
//! team when needed), an issue-less `support_reply` row opens that team's
//! Support tool, a bundle opens the Inbox tab. With every shell window closed
//! (macOS dock-resident app) the host's open-window hook is asked for one
//! first. Mark-read stays what it is — the rail rows mark on click; a toast
//! click lands on the surface that does.
//!
//! Dev builds: gpui's macOS backend disables itself when the binary is not
//! inside an app bundle (`UNUserNotificationCenter` aborts the process
//! otherwise), so `cargo run` / `bun run dev:desktop` never toast on macOS —
//! the `.app` from `macapp:desktop` does. Windows needs the AUMID the binary
//! sets via `App::set_app_identity` at startup (gpui registers it for the
//! unpackaged exe, no Start-menu shortcut required); Linux works as is.

use std::collections::{HashMap, HashSet, VecDeque};
use std::time::{Duration, Instant};

use chrono::{DateTime, FixedOffset};
use domain::contract::NOTIFICATION_TYPE_SUPPORT_REPLY;
use domain::rows::Notification;
use gpui::{
    AnyWindowHandle, App, AppContext as _, Context, Entity, Global, SharedString,
    SystemNotification, WeakEntity, Window,
};
use sync::{Collection, Store};

use crate::coding_flow::CodingHub;
use crate::navigation::{self, Screen};
use crate::sidebar::{self, InboxTab, ToolWindow};
use crate::{queries, window_hooks};

/// How long a burst is allowed to accumulate before ONE toast goes out. The
/// server's own dedupe window is 30s; a merge fan-out lands its rows within
/// a second or two, so a short window catches the burst without making a
/// lone comment feel laggy.
const COALESCE_WINDOW: Duration = Duration::from_millis(1500);

/// How long a fetched per-type prefs snapshot is trusted before the next
/// flush refetches it (a change made on the web or another device).
const PREFS_TTL: Duration = Duration::from_secs(5 * 60);

/// How many toast → route mappings are remembered for click routing. A
/// banner older than that is long gone from every notification center.
const ROUTE_MEMORY: usize = 64;

/// Titles listed in a bundle toast's body before "+N more".
const BUNDLE_TITLES: usize = 3;

/// Polling budget for a shell window the host was just asked to open.
const WINDOW_WAIT_TICK: Duration = Duration::from_millis(100);
const WINDOW_WAIT_TICKS: usize = 30;

/// The fallback headline for a row without a title (never happens with the
/// server's composers; defensive).
const UNTITLED: &str = "New notification";

// ---------------------------------------------------------------------------
// Pure core — testable without gpui
// ---------------------------------------------------------------------------

/// The backfill gate: arms on the first ready observation, remembers every
/// id it has seen and the newest `created_at` present at arm time.
struct Gate {
    armed: bool,
    seen: HashSet<String>,
    watermark: Option<DateTime<FixedOffset>>,
}

impl Gate {
    fn new() -> Self {
        Self {
            armed: false,
            seen: HashSet::new(),
            watermark: None,
        }
    }

    /// Feed the collection's current state. Returns the rows that are new
    /// since the previous observation AND worth toasting (unread, not older
    /// than the arm-time watermark). A not-ready collection re-arms.
    fn observe<'a>(
        &mut self,
        ready: bool,
        rows: impl Iterator<Item = &'a Notification>,
    ) -> Vec<Notification> {
        if !ready {
            self.armed = false;
            self.seen.clear();
            self.watermark = None;
            return Vec::new();
        }
        if !self.armed {
            self.armed = true;
            for row in rows {
                self.seen.insert(row.id.clone());
                if let Some(created) = parse_created_at(row) {
                    if self.watermark.is_none_or(|mark| created > mark) {
                        self.watermark = Some(created);
                    }
                }
            }
            return Vec::new();
        }
        let mut fresh = Vec::new();
        for row in rows {
            if !self.seen.insert(row.id.clone()) {
                continue;
            }
            if row.read_at.is_some() || self.behind_watermark(row) {
                continue;
            }
            fresh.push(row.clone());
        }
        fresh
    }

    /// Older than the newest row present at arm time. Unparsable stamps
    /// fail OPEN (a late row is better than a swallowed one); an equal
    /// stamp counts as new (rows of one server transaction share `now()`).
    fn behind_watermark(&self, row: &Notification) -> bool {
        match (self.watermark, parse_created_at(row)) {
            (Some(mark), Some(created)) => created < mark,
            _ => false,
        }
    }
}

fn parse_created_at(row: &Notification) -> Option<DateTime<FixedOffset>> {
    crate::inbox::parse_timestamp(row.created_at.as_deref()?)
}

/// Where a toast click lands.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Route {
    /// Issue detail, scoped on the issue's board.
    Issue { issue_id: String },
    /// The ticket team's Support tool (`None` = the legacy team-less row —
    /// the current team's Support, like the generic rail group).
    Support { team_id: Option<String> },
    /// The Inbox tab (bundles, and rows whose issue is not synced).
    Inbox,
}

fn route_for(row: &Notification) -> Route {
    if let Some(issue_id) = &row.issue_id {
        return Route::Issue {
            issue_id: issue_id.clone(),
        };
    }
    if row.kind.as_deref() == Some(NOTIFICATION_TYPE_SUPPORT_REPLY) {
        return Route::Support {
            team_id: row.team_id.clone(),
        };
    }
    Route::Inbox
}

/// One composed system notification.
#[derive(Debug, PartialEq, Eq)]
struct Toast {
    title: String,
    body: String,
    route: Route,
}

/// One row → its own title/body; a burst → "N new notifications" listing
/// the first titles, routed to the one route they share or else the Inbox.
fn compose(rows: &[Notification]) -> Option<Toast> {
    match rows {
        [] => None,
        [row] => Some(Toast {
            title: row
                .title
                .clone()
                .filter(|title| !title.trim().is_empty())
                .unwrap_or_else(|| UNTITLED.to_string()),
            body: row.body.clone().unwrap_or_default(),
            route: route_for(row),
        }),
        many => {
            let titles: Vec<&str> = many
                .iter()
                .filter_map(|row| row.title.as_deref())
                .filter(|title| !title.trim().is_empty())
                .collect();
            let mut body = titles
                .iter()
                .take(BUNDLE_TITLES)
                .copied()
                .collect::<Vec<_>>()
                .join("\n");
            let hidden = many.len().saturating_sub(titles.len().min(BUNDLE_TITLES));
            if hidden > 0 {
                if !body.is_empty() {
                    body.push('\n');
                }
                body.push_str(&format!("+{hidden} more"));
            }
            let first = route_for(&many[0]);
            let route = if many.iter().all(|row| route_for(row) == first) {
                first
            } else {
                Route::Inbox
            };
            Some(Toast {
                title: format!("{} new notifications", many.len()),
                body,
                route,
            })
        }
    }
}

/// What the FOCUSED shell window shows — the redundancy check's input.
#[derive(Debug)]
struct FocusedSurface {
    screen: Option<Screen>,
    tool: ToolWindow,
    inbox_tab: InboxTab,
    team_id: Option<String>,
}

/// Whether raising `row` would only repeat what the user is looking at.
fn is_redundant(row: &Notification, focus: &FocusedSurface) -> bool {
    if let Some(issue_id) = &row.issue_id {
        if matches!(&focus.screen, Some(Screen::IssueDetail { issue_id: shown }) if shown == issue_id)
        {
            return true;
        }
    }
    if focus.tool == ToolWindow::Inbox && focus.inbox_tab == InboxTab::Inbox {
        return true;
    }
    if row.issue_id.is_none()
        && row.kind.as_deref() == Some(NOTIFICATION_TYPE_SUPPORT_REPLY)
        && focus.tool == ToolWindow::Support
    {
        return row.team_id.is_none() || row.team_id == focus.team_id;
    }
    false
}

/// The server's `notificationTypeAllowed`: missing = allowed; only an
/// explicit `false` mutes. `None` prefs (unfetched / failed) allow all.
fn type_allowed(prefs: Option<&HashMap<String, bool>>, kind: Option<&str>) -> bool {
    match (prefs, kind) {
        (Some(prefs), Some(kind)) => prefs.get(kind).copied() != Some(false),
        _ => true,
    }
}

// ---------------------------------------------------------------------------
// The App-level notifier entity
// ---------------------------------------------------------------------------

struct TypePrefsCache {
    account_id: String,
    type_prefs: HashMap<String, bool>,
    fetched_at: Instant,
}

/// The one process-wide notifier. Observes the shared `notifications`
/// collection (one observer for N windows — toasts are per machine, not per
/// window) and remembers which toast routes where.
pub struct OsNotifier {
    gate: Gate,
    pending: Vec<Notification>,
    flush_scheduled: bool,
    routes: HashMap<SharedString, Route>,
    route_order: VecDeque<SharedString>,
    seq: u64,
    prefs: Option<TypePrefsCache>,
}

struct OsNotifierGlobal(Entity<OsNotifier>);

impl Global for OsNotifierGlobal {}

/// Install the notifier: ONCE per app, after `Store::open` (it observes the
/// store's collections) and after the `AuthContext` global exists (the
/// per-machine switch is read through the coding hub's settings).
pub fn install(cx: &mut App) {
    let notifications = Store::global(cx).collections().notifications.clone();
    let notifier = cx.new(|cx: &mut Context<OsNotifier>| {
        cx.observe(&notifications, |this: &mut OsNotifier, collection, cx| {
            this.on_collection_changed(&collection, cx)
        })
        .detach();
        OsNotifier {
            gate: Gate::new(),
            pending: Vec::new(),
            flush_scheduled: false,
            routes: HashMap::new(),
            route_order: VecDeque::new(),
            seq: 0,
            prefs: None,
        }
    });
    cx.set_global(OsNotifierGlobal(notifier.clone()));

    // Click → route. gpui hands responses to this callback on the
    // foreground, outside any window update, so `window.update` below is
    // safe to call directly.
    cx.on_system_notification_response(move |response, cx| {
        let route = notifier.read(cx).routes.get(&response.tag).cloned();
        match route {
            Some(route) => route_to(route, cx),
            None => log::info!(
                "[ui] os-notifications: response for unknown tag {} ignored",
                response.tag
            ),
        }
    });
}

/// EXP-638: the Notifications pane changed a per-type switch — forget the
/// cached snapshot so the next flush refetches (the mutation is in flight;
/// the TTL alone would keep muting/unmuting stale for minutes).
pub(crate) fn invalidate_type_prefs(cx: &mut App) {
    if let Some(notifier) = cx.try_global::<OsNotifierGlobal>().map(|g| g.0.clone()) {
        notifier.update(cx, |this, _| this.prefs = None);
    }
}

impl OsNotifier {
    fn on_collection_changed(
        &mut self,
        collection: &Entity<Collection<Notification>>,
        cx: &mut Context<Self>,
    ) {
        let fresh = {
            let collection = collection.read(cx);
            self.gate.observe(collection.is_ready(), collection.iter())
        };
        if fresh.is_empty() {
            return;
        }
        self.pending.extend(fresh);
        self.schedule_flush(cx);
    }

    /// One timer per burst: the first fresh row starts it, everything that
    /// lands before it fires rides the same toast.
    fn schedule_flush(&mut self, cx: &mut Context<Self>) {
        if self.flush_scheduled {
            return;
        }
        self.flush_scheduled = true;
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(COALESCE_WINDOW).await;
            cx.update(|cx| Self::flush(this, cx));
        })
        .detach();
    }

    fn flush(this: WeakEntity<Self>, cx: &mut App) {
        let Some(entity) = this.upgrade() else {
            return;
        };
        let pending = entity.update(cx, |this, _| {
            this.flush_scheduled = false;
            std::mem::take(&mut this.pending)
        });
        if pending.is_empty() {
            return;
        }
        // The per-machine switch.
        if !CodingHub::global(cx).read(cx).settings.os_notifications {
            return;
        }
        // Redundancy: drop what the focused window already shows.
        let focus = focused_surface(cx);
        let rows: Vec<Notification> = pending
            .into_iter()
            .filter(|row| !focus.as_ref().is_some_and(|focus| is_redundant(row, focus)))
            .collect();
        if rows.is_empty() {
            return;
        }
        // Per-type prefs: the cached snapshot, or a fetch first.
        let account_id = Store::global(cx)
            .session(cx)
            .account_id()
            .map(str::to_string);
        let cached = entity
            .read(cx)
            .prefs
            .as_ref()
            .filter(|cache| {
                Some(&cache.account_id) == account_id.as_ref()
                    && cache.fetched_at.elapsed() < PREFS_TTL
            })
            .map(|cache| cache.type_prefs.clone());
        if let Some(prefs) = cached {
            Self::show(&entity, rows, Some(&prefs), cx);
            return;
        }
        let Some(trpc) = queries::trpc_client(cx) else {
            Self::show(&entity, rows, None, cx);
            return;
        };
        let weak = entity.downgrade();
        cx.spawn(async move |cx| {
            let result = cx
                .background_executor()
                .spawn(async move { api::notifications::notifications_email_prefs(&trpc) })
                .await;
            cx.update(|cx| {
                let Some(entity) = weak.upgrade() else {
                    return;
                };
                let prefs = match result {
                    Ok(prefs) => {
                        if let Some(account_id) = account_id {
                            let type_prefs = prefs.type_prefs.clone();
                            entity.update(cx, |this, _| {
                                this.prefs = Some(TypePrefsCache {
                                    account_id,
                                    type_prefs,
                                    fetched_at: Instant::now(),
                                });
                            });
                        }
                        Some(prefs.type_prefs)
                    }
                    Err(err) => {
                        // Fail open, like the server's push gate.
                        log::warn!("[ui] os-notifications: emailPrefs fetch failed: {err}");
                        None
                    }
                };
                Self::show(&entity, rows, prefs.as_ref(), cx);
            });
        })
        .detach();
    }

    fn show(
        entity: &Entity<Self>,
        rows: Vec<Notification>,
        prefs: Option<&HashMap<String, bool>>,
        cx: &mut App,
    ) {
        let rows: Vec<Notification> = rows
            .into_iter()
            .filter(|row| type_allowed(prefs, row.kind.as_deref()))
            .collect();
        let Some(toast) = compose(&rows) else {
            return;
        };
        let tag = entity.update(cx, |this, _| this.register_route(toast.route));
        log::info!(
            "[ui] os-notifications: raising {tag} ({} row(s))",
            rows.len()
        );
        cx.show_system_notification(SystemNotification {
            tag,
            title: toast.title.into(),
            body: toast.body.into(),
            actions: Vec::new(),
        });
    }

    fn register_route(&mut self, route: Route) -> SharedString {
        self.seq += 1;
        let tag: SharedString = format!("exp-notification-{}", self.seq).into();
        self.routes.insert(tag.clone(), route);
        self.route_order.push_back(tag.clone());
        while self.route_order.len() > ROUTE_MEMORY {
            if let Some(old) = self.route_order.pop_front() {
                self.routes.remove(&old);
            }
        }
        tag
    }
}

// ---------------------------------------------------------------------------
// Window plumbing
// ---------------------------------------------------------------------------

/// The active window's surface — `None` unless it is a focused shell window
/// (dialogs, undocked terminals and an unfocused app suppress nothing).
fn focused_surface(cx: &mut App) -> Option<FocusedSurface> {
    let window = cx.active_window()?;
    let window_id = window.window_id();
    let nav = navigation::nav_for_window_id(window_id, cx)?;
    let (tool, inbox_tab) = sidebar::rail_tool_for_window_id(window_id, cx)?;
    let active = window
        .update(cx, |_, window, _| window.is_window_active())
        .unwrap_or(false);
    if !active {
        return None;
    }
    Some(FocusedSurface {
        screen: navigation::resolved_screen(&nav, cx),
        tool,
        inbox_tab,
        team_id: navigation::active_team_id(&nav, cx),
    })
}

/// A shell window to land a click in: the active one when it is a shell,
/// else any window with a navigation entry.
fn shell_window(cx: &App) -> Option<AnyWindowHandle> {
    let is_shell =
        |window: &AnyWindowHandle| navigation::nav_for_window_id(window.window_id(), cx).is_some();
    if let Some(window) = cx.active_window().filter(is_shell) {
        return Some(window);
    }
    cx.windows().into_iter().find(is_shell)
}

fn route_to(route: Route, cx: &mut App) {
    cx.activate(true);
    if let Some(window) = shell_window(cx) {
        let _ = window.update(cx, |_, window, cx| {
            window.activate_window();
            land(route, window, cx);
        });
        return;
    }
    // Every shell window is closed: ask the host for one, then wait for its
    // navigation entry (the window opens inside a foreground spawn).
    if !window_hooks::request_shell_window(cx) {
        log::info!("[ui] os-notifications: no shell window to route {route:?} into");
        return;
    }
    cx.spawn(async move |cx| {
        for _ in 0..WINDOW_WAIT_TICKS {
            cx.background_executor().timer(WINDOW_WAIT_TICK).await;
            let route = route.clone();
            let landed = cx.update(|cx| match shell_window(cx) {
                Some(window) => window
                    .update(cx, |_, window, cx| {
                        window.activate_window();
                        land(route, window, cx);
                    })
                    .is_ok(),
                None => false,
            });
            if landed {
                return;
            }
        }
        log::warn!("[ui] os-notifications: shell window never came up; click dropped");
    })
    .detach();
}

/// Land a route in `window` — the rail Inbox rows' own paths.
fn land(route: Route, window: &mut Window, cx: &mut App) {
    match route {
        Route::Issue { issue_id } => {
            let target = {
                let collections = Store::global(cx).collections();
                let issues = collections.issues.read(cx);
                let boards = collections.boards.read(cx);
                issues.get(&issue_id).map(|issue| {
                    let team_id = boards
                        .get(&issue.board_id)
                        .map(|board| board.team_id.clone());
                    (issue.board_id.clone(), team_id)
                })
            };
            match target {
                Some((board_id, team_id)) => {
                    switch_team_if_needed(team_id, window, cx);
                    navigation::open_issue_scoped(window, cx, issue_id, board_id);
                }
                // Not synced (yet) — the Inbox row will resolve it later.
                None => sidebar::open_inbox_tab(window, cx, InboxTab::Inbox),
            }
        }
        Route::Support { team_id } => {
            switch_team_if_needed(team_id, window, cx);
            sidebar::activate_tool(window, cx, ToolWindow::Support);
        }
        Route::Inbox => sidebar::open_inbox_tab(window, cx, InboxTab::Inbox),
    }
}

/// The `OpenBoard` / Support-row cross-team rule: a target in another team
/// switches the window's team first (screen + back stack reset).
fn switch_team_if_needed(team_id: Option<String>, window: &mut Window, cx: &mut App) {
    let Some(team_id) = team_id else {
        return;
    };
    let nav = navigation::nav_for_window(window, cx);
    if navigation::active_team_id(&nav, cx).as_deref() != Some(team_id.as_str()) {
        navigation::switch_team(window, cx, team_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(id: &str, created_at: &str) -> Notification {
        Notification {
            id: id.to_string(),
            user_id: "u1".to_string(),
            issue_id: Some(format!("issue-{id}")),
            team_id: None,
            kind: Some("issue_comment".to_string()),
            title: Some(format!("Title {id}")),
            body: Some(format!("Body {id}")),
            read_at: None,
            pushed_at: None,
            created_at: Some(created_at.to_string()),
            updated_at: None,
        }
    }

    fn support_row(id: &str, team_id: Option<&str>) -> Notification {
        Notification {
            issue_id: None,
            team_id: team_id.map(str::to_string),
            kind: Some(NOTIFICATION_TYPE_SUPPORT_REPLY.to_string()),
            ..row(id, "2026-08-27T10:00:00+00:00")
        }
    }

    fn focus(tool: ToolWindow, inbox_tab: InboxTab, screen: Option<Screen>) -> FocusedSurface {
        FocusedSurface {
            screen,
            tool,
            inbox_tab,
            team_id: Some("team-a".to_string()),
        }
    }

    #[test]
    fn backfill_never_toasts_and_only_later_rows_do() {
        let mut gate = Gate::new();
        let history = [
            row("a", "2026-08-27T09:00:00+00:00"),
            row("b", "2026-08-27T09:30:00+00:00"),
        ];
        // Still syncing: nothing, and nothing is remembered.
        assert!(gate.observe(false, history.iter()).is_empty());
        // First ready observation = the backfill: swallowed, arms the gate.
        assert!(gate.observe(true, history.iter()).is_empty());
        // The same rows again: seen.
        assert!(gate.observe(true, history.iter()).is_empty());
        // A genuinely new row after the watermark toasts, once.
        let mut next = history.to_vec();
        next.push(row("c", "2026-08-27T09:45:00+00:00"));
        let fresh = gate.observe(true, next.iter());
        assert_eq!(
            fresh.iter().map(|n| n.id.as_str()).collect::<Vec<_>>(),
            ["c"]
        );
        assert!(gate.observe(true, next.iter()).is_empty());
    }

    #[test]
    fn rows_older_than_the_watermark_or_already_read_stay_silent() {
        let mut gate = Gate::new();
        let history = [row("a", "2026-08-27T09:30:00+00:00")];
        gate.observe(true, history.iter());
        let mut next = history.to_vec();
        // Older than anything present at arm time (a late shape catch-up).
        next.push(row("old", "2026-08-27T08:00:00+00:00"));
        // Newer but already read elsewhere.
        let mut read = row("read", "2026-08-27T09:50:00+00:00");
        read.read_at = Some("2026-08-27T09:51:00+00:00".to_string());
        next.push(read);
        // Postgres-form stamp newer than the RFC-form watermark: the parser,
        // not the string order, decides.
        next.push(row("pg", "2026-08-27 09:40:00.123456+00"));
        let fresh = gate.observe(true, next.iter());
        assert_eq!(
            fresh.iter().map(|n| n.id.as_str()).collect::<Vec<_>>(),
            ["pg"]
        );
    }

    #[test]
    fn losing_readiness_rearms_so_the_next_hydrate_is_a_backfill() {
        let mut gate = Gate::new();
        gate.observe(true, [row("a", "2026-08-27T09:00:00+00:00")].iter());
        // Sign-out clears the shape; the re-login hydrate must not toast.
        assert!(gate.observe(false, std::iter::empty()).is_empty());
        let again = [
            row("a", "2026-08-27T09:00:00+00:00"),
            row("b", "2026-08-27T09:30:00+00:00"),
        ];
        assert!(gate.observe(true, again.iter()).is_empty());
    }

    #[test]
    fn single_row_keeps_its_own_title_body_and_route() {
        let toast = compose(&[row("a", "2026-08-27T09:00:00+00:00")]).unwrap();
        assert_eq!(
            toast,
            Toast {
                title: "Title a".to_string(),
                body: "Body a".to_string(),
                route: Route::Issue {
                    issue_id: "issue-a".to_string()
                },
            }
        );
        let support = compose(&[support_row("s", Some("team-b"))]).unwrap();
        assert_eq!(
            support.route,
            Route::Support {
                team_id: Some("team-b".to_string())
            }
        );
    }

    #[test]
    fn bursts_collapse_into_one_toast() {
        let rows: Vec<Notification> = (1..=5)
            .map(|i| row(&i.to_string(), "2026-08-27T09:00:00+00:00"))
            .collect();
        let toast = compose(&rows).unwrap();
        assert_eq!(toast.title, "5 new notifications");
        assert_eq!(toast.body, "Title 1\nTitle 2\nTitle 3\n+2 more");
        // Different issues → the Inbox.
        assert_eq!(toast.route, Route::Inbox);

        // The same issue throughout → straight to it.
        let mut same: Vec<Notification> = rows[..2].to_vec();
        same[1].issue_id = same[0].issue_id.clone();
        let toast = compose(&same).unwrap();
        assert_eq!(toast.body, "Title 1\nTitle 2");
        assert_eq!(
            toast.route,
            Route::Issue {
                issue_id: "issue-1".to_string()
            }
        );
        assert!(compose(&[]).is_none());
    }

    #[test]
    fn redundancy_rules_match_what_the_focused_window_shows() {
        let comment = row("a", "2026-08-27T09:00:00+00:00");
        let showing_it = focus(
            ToolWindow::BoardIssues,
            InboxTab::MyIssues,
            Some(Screen::IssueDetail {
                issue_id: "issue-a".to_string(),
            }),
        );
        assert!(is_redundant(&comment, &showing_it));
        let showing_other = focus(
            ToolWindow::BoardIssues,
            InboxTab::MyIssues,
            Some(Screen::IssueDetail {
                issue_id: "issue-z".to_string(),
            }),
        );
        assert!(!is_redundant(&comment, &showing_other));
        // The Inbox tab up = everything is redundant; its My Issues tab is not.
        assert!(is_redundant(
            &comment,
            &focus(ToolWindow::Inbox, InboxTab::Inbox, None)
        ));
        assert!(!is_redundant(
            &comment,
            &focus(ToolWindow::Inbox, InboxTab::MyIssues, None)
        ));
        // Support rows: the ticket team's Support tool, or the legacy team-less row.
        let support = focus(ToolWindow::Support, InboxTab::MyIssues, None);
        assert!(is_redundant(&support_row("s", Some("team-a")), &support));
        assert!(!is_redundant(&support_row("s", Some("team-b")), &support));
        assert!(is_redundant(&support_row("s", None), &support));
        assert!(!is_redundant(
            &support_row("s", Some("team-a")),
            &showing_other
        ));
    }

    #[test]
    fn type_prefs_mute_only_on_explicit_false_and_fail_open() {
        let prefs: HashMap<String, bool> = [
            ("issue_comment".to_string(), false),
            ("pr_merged".to_string(), true),
        ]
        .into_iter()
        .collect();
        assert!(!type_allowed(Some(&prefs), Some("issue_comment")));
        assert!(type_allowed(Some(&prefs), Some("pr_merged")));
        assert!(type_allowed(Some(&prefs), Some("issue_assigned")));
        assert!(type_allowed(Some(&prefs), None));
        assert!(type_allowed(None, Some("issue_comment")));
    }

    #[test]
    fn route_memory_is_bounded() {
        let mut notifier = OsNotifier {
            gate: Gate::new(),
            pending: Vec::new(),
            flush_scheduled: false,
            routes: HashMap::new(),
            route_order: VecDeque::new(),
            seq: 0,
            prefs: None,
        };
        let first = notifier.register_route(Route::Inbox);
        for _ in 0..ROUTE_MEMORY {
            notifier.register_route(Route::Inbox);
        }
        assert_eq!(notifier.routes.len(), ROUTE_MEMORY);
        assert!(!notifier.routes.contains_key(&first));
    }
}
