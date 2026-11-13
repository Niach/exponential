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
//! - **Presented per #EXP-1815** (`notification-delivery.json`,
//!   [`domain::notification_delivery::notification_presentation`], platform
//!   desktop): every row has a THREAD (its issue, its session's chat = the
//!   synced root, its device, else the Inbox). A focused window showing that
//!   thread (or the Inbox) raises nothing; a focused window elsewhere shows
//!   THE in-app toast ([`crate::toast`], kind info, the row's title + body,
//!   `Open` routing like an OS click); an unfocused app raises the OS
//!   notification. A row whose `pushed_at` is NULL was muted by the member's
//!   source switches (the server stamps every other row) and raises nothing.
//! - **Cleared when read.** Each raised row remembers its OS tag / in-app
//!   toast; when the synced row's `read_at` lands (read on ANY device: the
//!   server marks rows read on every read write, `sessions.markRead`
//!   included) both are taken back (`App::dismiss_system_notification`).
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
//! team when needed; SLOP-4: a `reporter_reply` row is one of those), a
//! issue-less `session_blocked` / `agent_message` row naming its session
//! opens that session's screen (#EXP-1456 inbox-route.json: the viewer's own
//! agent conversation = the agent's page, a chat, or the session), a
//! bundle opens the Inbox tab. With every shell window closed
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
use domain::notification_delivery::{
    self as delivery, DeliveryPlatform, Presentation,
};
use domain::rows::Notification;
use gpui::{
    AnyWindowHandle, App, AppContext as _, Context, Entity, Global, SharedString,
    SystemNotification, WeakEntity, Window,
};
use sync::{Collection, Store};

use crate::coding_flow::CodingHub;
use crate::navigation::{self, Screen};
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
    /// The Inbox tab (bundles, and rows whose issue is not synced).
    Inbox,
    /// EXP-856/EXP-1294: a session-level row (a blocked session, an agent's
    /// message, the duplicate-agent warning) — opens the screen that session
    /// is ([`navigation::session_screen_for`]).
    Session { session_id: String },
    /// #NEW-68: a Settings section (the voice tray's Voice settings…).
    Settings { section: crate::settings::SettingsSection },
    /// #EXP-1534: an `agent_signed_out` row — the Devices page with this
    /// devices row's settings open.
    Device { device_id: String },
}

fn route_for(row: &Notification) -> Route {
    // #EXP-1456 (inbox-route.json), the inbox row's own rule: an issue row
    // opens its issue; an issue-less `agent_message` / `session_blocked`
    // naming a session (an Assistant's decision, a mention, a wall) opens
    // that session; #EXP-1534: an `agent_signed_out` row its device's
    // settings; anything else (a pruned session, a broadcast, a removed
    // device) the Inbox.
    use domain::inbox_route::InboxRowTarget;
    match queries::row_target(row) {
        InboxRowTarget::Issue { issue_id, .. } => Route::Issue { issue_id },
        InboxRowTarget::Session { session_id } => Route::Session { session_id },
        InboxRowTarget::Device { device_id } => Route::Device { device_id },
        InboxRowTarget::None => Route::Inbox,
    }
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

/// What the app shows right now — the presentation's input.
#[derive(Debug, Default)]
struct FocusedSurface {
    /// One of the app's windows is focused.
    active: bool,
    /// The focused shell window's thread (#EXP-1815 `viewing`): its issue,
    /// its chat, or `inbox` while the Inbox is up; None = a screen without
    /// one (or no shell window focused).
    viewing: Option<String>,
}

/// A row's thread: its session's chat is the synced root (`root_of`), else
/// the session id itself (notification-delivery.json THREAD).
fn row_thread(row: &Notification, root_of: &dyn Fn(&str) -> Option<String>) -> String {
    let root = row
        .session_id
        .as_deref()
        .filter(|id| !id.is_empty())
        .map(|id| root_of(id).unwrap_or_else(|| id.to_string()));
    delivery::notification_thread(
        row.kind.as_deref(),
        row.issue_id.as_deref(),
        root.as_deref(),
        row.device_id.as_deref(),
    )
}

/// Where each fresh row goes: `(toast, system)`; the rest raise nothing.
fn present(
    rows: Vec<Notification>,
    focus: &FocusedSurface,
    root_of: &dyn Fn(&str) -> Option<String>,
) -> (Vec<Notification>, Vec<Notification>) {
    let mut toast = Vec::new();
    let mut system = Vec::new();
    for row in rows {
        let thread = row_thread(&row, root_of);
        match delivery::notification_presentation(
            DeliveryPlatform::Desktop,
            focus.active,
            focus.viewing.as_deref(),
            &thread,
            row.pushed_at.is_none(),
        ) {
            Presentation::None => {}
            Presentation::Toast => toast.push(row),
            Presentation::System => system.push(row),
        }
    }
    (toast, system)
}

/// The thread a shell screen shows (#EXP-1815 `viewing`).
fn screen_thread(
    screen: &Screen,
    root_of: &dyn Fn(&str) -> Option<String>,
    agent_root: &dyn Fn(&str) -> Option<String>,
) -> Option<String> {
    match screen {
        Screen::IssueDetail { issue_id } => Some(format!("issue:{issue_id}")),
        Screen::Chat { session_id } | Screen::Session { session_id } => Some(format!(
            "session:{}",
            root_of(session_id).unwrap_or_else(|| session_id.clone())
        )),
        Screen::Agent { agent_id } => agent_root(agent_id).map(|root| format!("session:{root}")),
        _ => None,
    }
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
    /// #EXP-1815: what each raised row put up (notification id → the OS
    /// tag / the in-app toast), taken back when the row turns read.
    raised: HashMap<String, Raised>,
    raised_order: VecDeque<String>,
}

/// What one raised row put up.
#[derive(Clone)]
enum Raised {
    /// The OS notification's tag (a bundle shares one tag across its rows).
    Os(SharedString),
    /// The in-app toast.
    Toast(crate::toast::ShownToast),
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
            raised: HashMap::new(),
            raised_order: VecDeque::new(),
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
        let (fresh, read) = {
            let collection = collection.read(cx);
            let fresh = self.gate.observe(collection.is_ready(), collection.iter());
            (fresh, self.turned_read(|id| collection.get(id).map(|row| row.read_at.is_some())))
        };
        for raised in read {
            raised.take_back(cx);
        }
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
        // Per-type prefs: the cached snapshot, or a fetch first.
        let rows = pending;
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

    /// #EXP-1815: split the burst by presentation (read at raise time, so a
    /// focus change during the prefs fetch counts) and raise each part: the
    /// in-app toast in the focused window, the OS notification when the
    /// per-machine switch is on.
    fn show(
        entity: &Entity<Self>,
        rows: Vec<Notification>,
        prefs: Option<&HashMap<String, bool>>,
        cx: &mut App,
    ) {
        // A row read while the burst waited raises nothing.
        let rows: Vec<Notification> = {
            let notifications = Store::global(cx).collections().notifications.read(cx);
            rows.into_iter()
                .filter(|row| type_allowed(prefs, row.kind.as_deref()))
                .filter(|row| {
                    notifications
                        .get(&row.id)
                        .is_none_or(|synced| synced.read_at.is_none())
                })
                .collect()
        };
        if rows.is_empty() {
            return;
        }
        let root_of = session_roots(cx);
        let focus = focused_surface(cx);
        let (toast_rows, system_rows) = present(rows, &focus, &root_of);
        if let Some(toast) = compose(&toast_rows) {
            Self::raise_toast(entity, toast, &toast_rows, cx);
        }
        // The per-machine switch gates the OS channel.
        if !CodingHub::global(cx).read(cx).settings.os_notifications {
            return;
        }
        let Some(toast) = compose(&system_rows) else {
            return;
        };
        let tag = entity.update(cx, |this, _| {
            let tag = this.register_route(toast.route);
            this.remember(&system_rows, Raised::Os(tag.clone()));
            tag
        });
        log::info!(
            "[ui] os-notifications: raising {tag} ({} row(s))",
            system_rows.len()
        );
        cx.show_system_notification(SystemNotification {
            tag,
            title: toast.title.into(),
            body: toast.body.into(),
            actions: Vec::new(),
        });
    }

    /// THE toast (toast-stack.json), kind info, in the focused window: the
    /// row's title + body, `Open` = the OS click's route.
    fn raise_toast(entity: &Entity<Self>, toast: Toast, rows: &[Notification], cx: &mut App) {
        let Some(window) = cx.active_window() else {
            return;
        };
        let route = toast.route;
        let mut card = crate::toast::Toast::info(toast.title).action(
            delivery::copy::TOAST_ACTION,
            move |_, cx| {
                let route = route.clone();
                // Outside the toast's own window update: the route may land
                // in this very window.
                cx.defer(move |cx| route_to(route, cx));
            },
        );
        if !toast.body.trim().is_empty() {
            card = card.description(toast.body);
        }
        let shown = window.update(cx, |_, window, cx| crate::toast::show_dismissible(card, window, cx));
        if let Ok(shown) = shown {
            log::info!("[ui] os-notifications: in-app toast ({} row(s))", rows.len());
            entity.update(cx, |this, _| this.remember(rows, Raised::Toast(shown)));
        }
    }

    /// Remember what `rows` put up, bounded like the routes.
    fn remember(&mut self, rows: &[Notification], raised: Raised) {
        for row in rows {
            if self.raised.insert(row.id.clone(), raised.clone()).is_none() {
                self.raised_order.push_back(row.id.clone());
            }
        }
        while self.raised_order.len() > ROUTE_MEMORY {
            if let Some(old) = self.raised_order.pop_front() {
                self.raised.remove(&old);
            }
        }
    }

    /// The raised rows that turned read (`is_read`: None = the row left the
    /// shape, which forgets it quietly). A bundle goes once its first row
    /// is read (the dismiss rule matches by id).
    fn turned_read(&mut self, is_read: impl Fn(&str) -> Option<bool>) -> Vec<Raised> {
        let mut out: Vec<Raised> = Vec::new();
        let ids: Vec<String> = self.raised_order.iter().cloned().collect();
        for id in ids {
            match is_read(&id) {
                Some(false) => continue,
                gone_or_read => {
                    self.raised_order.retain(|other| other != &id);
                    if let Some(raised) = self.raised.remove(&id) {
                        if gone_or_read == Some(true) && !out.iter().any(|seen| seen.same(&raised)) {
                            out.push(raised);
                        }
                    }
                }
            }
        }
        out
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

impl Raised {
    fn same(&self, other: &Raised) -> bool {
        match (self, other) {
            (Raised::Os(a), Raised::Os(b)) => a == b,
            (Raised::Toast(a), Raised::Toast(b)) => a.same(b),
            _ => false,
        }
    }

    /// Take it back: the OS notification out of the notification center
    /// (gpui: XDG `CloseNotification` on Linux, the WinRT history on
    /// Windows, `removeDeliveredNotifications` on macOS), the toast off its
    /// stack.
    fn take_back(self, cx: &mut App) {
        match self {
            Raised::Os(tag) => {
                log::info!("[ui] os-notifications: {tag} read, dismissing");
                cx.dismiss_system_notification(&tag);
            }
            Raised::Toast(shown) => shown.dismiss(cx),
        }
    }
}

// ---------------------------------------------------------------------------
// Window plumbing
// ---------------------------------------------------------------------------

/// The synced sessions' roots (`id` → its chat): a root maps to itself, a
/// session not synced to None.
fn session_roots(cx: &App) -> impl Fn(&str) -> Option<String> + 'static {
    let sessions = Store::global(cx).collections().sessions.read(cx);
    let roots: HashMap<String, String> = sessions
        .iter()
        .map(|session| {
            let root = session
                .root_id
                .clone()
                .filter(|root| !root.is_empty())
                .unwrap_or_else(|| session.id.clone());
            (session.id.clone(), root)
        })
        .collect();
    move |id: &str| roots.get(id).cloned()
}

/// The viewer's own conversation with an agent (the Agent page's chat): the
/// root session carrying that agent and the viewer's user id.
fn agent_conversation(agent_id: &str, cx: &App) -> Option<String> {
    let user_id = queries::active_account(cx)?.user_id;
    let sessions = Store::global(cx).collections().sessions.read(cx);
    sessions
        .iter()
        .find(|session| {
            session.agent_id.as_deref() == Some(agent_id)
                && session.user_id.as_deref() == Some(user_id.as_str())
                && session.parent_id.is_none()
        })
        .map(|session| session.id.clone())
}

/// What the app shows: focused or not, and the focused shell window's
/// thread (dialogs and undocked terminals show none).
fn focused_surface(cx: &mut App) -> FocusedSurface {
    let Some(window) = cx.active_window() else {
        return FocusedSurface::default();
    };
    let active = window
        .update(cx, |_, window, _| window.is_window_active())
        .unwrap_or(false);
    if !active {
        return FocusedSurface::default();
    }
    let Some(nav) = navigation::nav_for_window_id(window.window_id(), cx) else {
        return FocusedSurface { active, viewing: None };
    };
    if nav.read(cx).inbox_open {
        return FocusedSurface { active, viewing: Some(delivery::INBOX_THREAD.to_string()) };
    }
    let root_of = session_roots(cx);
    let viewing = navigation::resolved_screen(&nav, cx).and_then(|screen| {
        screen_thread(&screen, &root_of, &|agent_id| agent_conversation(agent_id, cx))
    });
    FocusedSurface { active, viewing }
}

/// A shell window to land a click in: the active one when it is a shell,
/// else any window with a navigation entry (never a dialog or an undocked
/// terminal).
pub(crate) fn shell_window(cx: &App) -> Option<AnyWindowHandle> {
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

/// Land a route in `window` — the rail Inbox rows' own paths: a target in
/// another team opens WITHOUT switching the window's team (the Inbox rule).
/// EXP-1212: on a New issue page the landing is ONE held move
/// (`navigation::leave_held`).
/// #NEW-68: open a Settings section in a shell window (a new one when every
/// shell window is closed), like a toast click.
pub(crate) fn open_settings(section: crate::settings::SettingsSection, cx: &mut App) {
    route_to(Route::Settings { section }, cx);
}

fn land(route: Route, window: &mut Window, cx: &mut App) {
    navigation::leave_held(window, cx, None, move |window, cx| land_now(route, window, cx));
}

fn land_now(route: Route, window: &mut Window, cx: &mut App) {
    match route {
        Route::Issue { issue_id } => land_issue(issue_id, window, cx),
        Route::Inbox => crate::inbox_popover::open(window, cx),
        Route::Settings { section } => {
            navigation::navigate(window, cx, Screen::Settings);
            crate::sidebar::select_settings_section(window, cx, section);
        }
        // #EXP-1534: the inbox row's own landing — the Devices page, the
        // device's settings dialog on top.
        Route::Device { device_id } => {
            navigation::navigate(window, cx, Screen::Devices);
            crate::device_settings::open(window, cx, device_id);
        }
        // #EXP-1662: a session waiting on its own plan = its chat's plan tab.
        Route::Session { session_id } => {
            let screen = crate::plan_page::screen_for_session(&session_id, cx);
            crate::plan_page::prime_session(&session_id, cx);
            navigation::navigate(window, cx, screen);
        }
    }
}

/// An issue route, scoped on the issue's board when that board is in the
/// window's team (another team's issue opens as an Inbox row does, with the
/// window's team and board untouched). An issue that has not synced yet
/// opens the Inbox.
fn land_issue(issue_id: String, window: &mut Window, cx: &mut App) {
    let target = {
        let collections = Store::global(cx).collections();
        let issues = collections.issues.read(cx);
        let boards = collections.boards.read(cx);
        issues.get(&issue_id).map(|issue| {
            let team_id = boards.get(&issue.board_id).map(|board| board.team_id.clone());
            (issue.board_id.clone(), team_id)
        })
    };
    let Some((board_id, team_id)) = target else {
        // Not synced (yet) — the Inbox row will resolve it later.
        crate::inbox_popover::open(window, cx);
        return;
    };
    let nav = navigation::nav_for_window(window, cx);
    let same_team = team_id.is_some() && navigation::active_team_id(&nav, cx) == team_id;
    if same_team {
        navigation::open_issue_scoped(window, cx, issue_id, board_id, None);
    } else {
        crate::issue_list::open_issue_from_list(window, cx, issue_id, None);
    }
}

/// EXP-856 §4 — the duplicate-agent toast: a session-level alert that does
/// NOT come off the `notifications` shape (nothing server-side knows a second
/// copy of an agent started), so it bypasses the gate, the coalescer and the
/// per-type prefs and raises directly.
///
/// It still honours the per-machine switch, and the caller raises it ONCE per
/// duplicate id ([`crate::steer_viewer`] holds that set) — a repeated
/// `workflow` republish must not re-toast.
pub(crate) fn raise_duplicate_agent(session_id: &str, detail: &str, cx: &mut App) {
    if !CodingHub::global(cx).read(cx).settings.os_notifications {
        return;
    }
    let Some(notifier) = cx.try_global::<OsNotifierGlobal>().map(|global| global.0.clone()) else {
        return;
    };
    let route = Route::Session {
        session_id: session_id.to_string(),
    };
    let tag = notifier.update(cx, |this, _| this.register_route(route));
    cx.show_system_notification(SystemNotification {
        tag,
        title: DUPLICATE_AGENT_TITLE.into(),
        body: detail.to_string().into(),
        actions: Vec::new(),
    });
}

/// #NEW-57: a device-local alert that opens a Settings section (the meeting
/// auto-stop): no inbox row behind it, so like the duplicate-agent toast it
/// bypasses the gate and the per-type prefs but honours the per-machine
/// switch.
pub(crate) fn raise_settings(section: crate::settings::SettingsSection, title: &str, body: &str, cx: &mut App) {
    if !CodingHub::global(cx).read(cx).settings.os_notifications {
        return;
    }
    let Some(notifier) = cx.try_global::<OsNotifierGlobal>().map(|global| global.0.clone()) else {
        return;
    };
    let tag = notifier.update(cx, |this, _| this.register_route(Route::Settings { section }));
    cx.show_system_notification(SystemNotification {
        tag,
        title: title.to_string().into(),
        body: body.to_string().into(),
        actions: Vec::new(),
    });
}

/// The duplicate toast's title (the body is the wire's own sentence).
pub(crate) const DUPLICATE_AGENT_TITLE: &str = "Duplicate agent";

#[cfg(test)]
mod tests {
    use super::*;
    use domain::contract::{NOTIFICATION_TYPE_AGENT_MESSAGE, NOTIFICATION_TYPE_SESSION_BLOCKED};

    fn row(id: &str, created_at: &str) -> Notification {
        Notification {
            id: id.to_string(),
            user_id: "u1".to_string(),
            issue_id: Some(format!("issue-{id}")),
            team_id: None,
            session_id: None,
            device_id: None,
            kind: Some("issue_comment".to_string()),
            title: Some(format!("Title {id}")),
            body: Some(format!("Body {id}")),
            read_at: None,
            pushed_at: None,
            created_at: Some(created_at.to_string()),
            updated_at: None,
        }
    }

    /// SLOP-4: a widget reporter's reply — issue-scoped like a comment.
    fn reporter_row(id: &str) -> Notification {
        Notification {
            kind: Some(domain::contract::NOTIFICATION_TYPE_REPORTER_REPLY.to_string()),
            ..row(id, "2026-08-27T10:00:00+00:00")
        }
    }

    /// EXP-980: an issue-less blocked-run row naming its run.
    fn blocked_row(id: &str, session_id: Option<&str>) -> Notification {
        Notification {
            issue_id: None,
            team_id: Some("team-a".to_string()),
            session_id: session_id.map(str::to_string),
            kind: Some(NOTIFICATION_TYPE_SESSION_BLOCKED.to_string()),
            ..row(id, "2026-08-27T10:00:00+00:00")
        }
    }

    /// #EXP-1534: a team-less signed-out-agent row naming its device.
    fn signed_out_row(id: &str, device_id: Option<&str>) -> Notification {
        Notification {
            issue_id: None,
            device_id: device_id.map(str::to_string),
            kind: Some(domain::contract::NOTIFICATION_TYPE_AGENT_SIGNED_OUT.to_string()),
            title: Some("Claude Code account a@b.dev is signed out on mint".to_string()),
            body: None,
            ..row(id, "2026-11-01T10:00:00+00:00")
        }
    }

    /// #EXP-1534 (inbox-route.json): a signed-out agent login toasts its
    /// own sentence and opens its device's settings; a removed device the
    /// Inbox.
    #[test]
    fn a_signed_out_agent_routes_to_its_device() {
        let toast = compose(&[signed_out_row("a", Some("d-1"))]).unwrap();
        assert_eq!(toast.title, "Claude Code account a@b.dev is signed out on mint");
        assert_eq!(toast.body, "");
        assert_eq!(
            toast.route,
            Route::Device {
                device_id: "d-1".to_string()
            }
        );
        assert_eq!(route_for(&signed_out_row("b", None)), Route::Inbox);
    }

    /// #EXP-1456 (inbox-route.json): an issue-less agent message (an
    /// Assistant's decision, a mention) opens the session it names; one
    /// about an issue opens the issue, and an issue-less, session-less one
    /// (EXP-801) still lands in the Inbox.
    #[test]
    fn an_agent_message_routes_to_its_session() {
        let message = Notification {
            kind: Some(NOTIFICATION_TYPE_AGENT_MESSAGE.to_string()),
            issue_id: None,
            team_id: Some("team-a".to_string()),
            session_id: Some("s-m".to_string()),
            ..row("m", "2026-08-27T10:00:00+00:00")
        };
        assert_eq!(
            route_for(&message),
            Route::Session {
                session_id: "s-m".to_string()
            }
        );
        let about_issue = Notification {
            issue_id: Some("issue-m".to_string()),
            ..message.clone()
        };
        assert_eq!(
            route_for(&about_issue),
            Route::Issue {
                issue_id: "issue-m".to_string()
            }
        );
        let session_less = Notification {
            session_id: None,
            ..about_issue
        };
        assert_eq!(
            route_for(&session_less),
            Route::Issue {
                issue_id: "issue-m".to_string()
            }
        );
        let bare = Notification {
            issue_id: None,
            ..session_less
        };
        assert_eq!(route_for(&bare), Route::Inbox);
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
        // SLOP-4: a reporter's reply opens its issue, like a comment.
        let reporter = compose(&[reporter_row("r")]).unwrap();
        assert_eq!(
            reporter.route,
            Route::Issue {
                issue_id: "issue-r".to_string()
            }
        );
        // EXP-980: a blocked run opens the run; a pruned one falls back to
        // the inbox rather than routing nowhere.
        let blocked = compose(&[blocked_row("b", Some("s-1"))]).unwrap();
        assert_eq!(
            blocked.route,
            Route::Session {
                session_id: "s-1".to_string()
            }
        );
        let pruned = compose(&[blocked_row("b", None)]).unwrap();
        assert_eq!(pruned.route, Route::Inbox);
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

    fn pushed(mut row: Notification) -> Notification {
        row.pushed_at = Some("2026-11-13T10:00:00+00:00".to_string());
        row
    }

    fn ids(rows: &[Notification]) -> Vec<&str> {
        rows.iter().map(|row| row.id.as_str()).collect()
    }

    /// #EXP-1815 (notification-delivery.json, platform desktop): focused on
    /// the row's thread or the Inbox = nothing, focused elsewhere = THE
    /// toast, unfocused = the OS notification, muted (`pushed_at` NULL) =
    /// nothing anywhere.
    #[test]
    fn presentation_splits_toast_system_and_nothing() {
        let roots = |id: &str| (id == "child").then(|| "root".to_string());
        let comment = pushed(row("a", "2026-11-13T10:00:00+00:00"));
        let chat = pushed(Notification {
            issue_id: None,
            session_id: Some("child".to_string()),
            kind: Some(NOTIFICATION_TYPE_AGENT_MESSAGE.to_string()),
            ..row("c", "2026-11-13T10:00:00+00:00")
        });
        let muted = row("m", "2026-11-13T10:00:00+00:00");
        let all = || vec![comment.clone(), chat.clone(), muted.clone()];

        let away = FocusedSurface { active: false, viewing: Some("issue:issue-a".to_string()) };
        let (toast, system) = present(all(), &away, &roots);
        assert!(toast.is_empty());
        assert_eq!(ids(&system), ["a", "c"]);

        // On the chat's ROOT: the child's row is that thread.
        let on_chat = FocusedSurface { active: true, viewing: Some("session:root".to_string()) };
        let (toast, system) = present(all(), &on_chat, &roots);
        assert_eq!(ids(&toast), ["a"]);
        assert!(system.is_empty());

        let on_issue = FocusedSurface { active: true, viewing: Some("issue:issue-a".to_string()) };
        assert_eq!(ids(&present(all(), &on_issue, &roots).0), ["c"]);

        let inbox = FocusedSurface { active: true, viewing: Some("inbox".to_string()) };
        let (toast, system) = present(all(), &inbox, &roots);
        assert!(toast.is_empty() && system.is_empty());

        let elsewhere = FocusedSurface { active: true, viewing: None };
        assert_eq!(ids(&present(all(), &elsewhere, &roots).0), ["a", "c"]);
    }

    #[test]
    fn a_rows_thread_is_its_issue_chat_or_device() {
        let roots = |id: &str| (id == "child").then(|| "root".to_string());
        assert_eq!(row_thread(&row("a", "2026-11-13T10:00:00+00:00"), &roots), "issue:issue-a");
        assert_eq!(row_thread(&blocked_row("b", Some("child")), &roots), "session:root");
        // Not synced: the session id itself.
        assert_eq!(row_thread(&blocked_row("b", Some("s-9")), &roots), "session:s-9");
        assert_eq!(row_thread(&signed_out_row("d", Some("d-1")), &roots), "device:d-1");
        assert_eq!(row_thread(&blocked_row("b", None), &roots), "inbox");
    }

    #[test]
    fn a_screens_thread() {
        let roots = |id: &str| (id == "child").then(|| "root".to_string());
        let agent = |id: &str| (id == "ag").then(|| "conv".to_string());
        let thread = |screen: Screen| screen_thread(&screen, &roots, &agent);
        assert_eq!(thread(Screen::IssueDetail { issue_id: "i".into() }).as_deref(), Some("issue:i"));
        assert_eq!(thread(Screen::Chat { session_id: "root".into() }).as_deref(), Some("session:root"));
        assert_eq!(thread(Screen::Session { session_id: "child".into() }).as_deref(), Some("session:root"));
        assert_eq!(thread(Screen::Agent { agent_id: "ag".into() }).as_deref(), Some("session:conv"));
        assert_eq!(thread(Screen::Agent { agent_id: "other".into() }), None);
        assert_eq!(thread(Screen::Settings), None);
    }

    fn notifier() -> OsNotifier {
        OsNotifier {
            gate: Gate::new(),
            pending: Vec::new(),
            flush_scheduled: false,
            routes: HashMap::new(),
            route_order: VecDeque::new(),
            seq: 0,
            prefs: None,
            raised: HashMap::new(),
            raised_order: VecDeque::new(),
        }
    }

    /// #EXP-1815 CLEARING: a raised row's notification goes when its
    /// `read_at` lands; a bundle once (its first read row); a row that left
    /// the shape is forgotten without a dismiss.
    #[test]
    fn read_rows_take_back_what_they_raised() {
        let mut notifier = notifier();
        let rows = |names: &[&str]| -> Vec<Notification> {
            names.iter().map(|id| row(id, "2026-11-13T10:00:00+00:00")).collect()
        };
        notifier.remember(&rows(&["a", "b"]), Raised::Os("bundle".into()));
        notifier.remember(&rows(&["c"]), Raised::Os("single".into()));
        notifier.remember(&rows(&["gone"]), Raised::Os("gone".into()));
        let read: HashSet<&str> = ["a", "b"].into_iter().collect();
        let taken = notifier.turned_read(|id| match id {
            "gone" => None,
            id => Some(read.contains(id)),
        });
        let tags: Vec<String> = taken
            .iter()
            .map(|raised| match raised {
                Raised::Os(tag) => tag.to_string(),
                Raised::Toast(_) => unreachable!(),
            })
            .collect();
        assert_eq!(tags, ["bundle"]);
        assert_eq!(notifier.raised_order.iter().map(String::as_str).collect::<Vec<_>>(), ["c"]);
        // Nothing turned read since: nothing more.
        assert!(notifier.turned_read(|_| Some(false)).is_empty());
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
        let mut notifier = notifier();
        let first = notifier.register_route(Route::Inbox);
        for _ in 0..ROUTE_MEMORY {
            notifier.register_route(Route::Inbox);
        }
        assert_eq!(notifier.routes.len(), ROUTE_MEMORY);
        assert!(!notifier.routes.contains_key(&first));
        for ix in 0..=ROUTE_MEMORY {
            notifier.remember(&[row(&ix.to_string(), "2026-11-13T10:00:00+00:00")], Raised::Os("t".into()));
        }
        assert_eq!(notifier.raised.len(), ROUTE_MEMORY);
        assert!(!notifier.raised.contains_key("0"));
    }
}
