//! VAPP-91: the IDE as an Exponential UI HOST, the desktop twin of the web
//! app's `lib/exponential-ui-host.tsx`. Everything goes through the SDK's
//! PUBLIC gpui host API (`exponential_ui_gpui::runtime::ExponentialHost`),
//! like a third party's host would; no private hook:
//!
//! - functions: `harness.toast` (the IDE toast stack), `harness.openIssue`
//!   (the synced issue's detail, else an error toast), `harness.openDevice`
//!   (the device settings dialog for an OWN row, else a toast with its
//!   name), `harness.mcp` (a `tools/call` on the instance's own `/api/mcp`
//!   with the signed-in account's credentials, `api::TrpcClient::post_json`);
//! - bindings: the `exp:` scheme over the synced store (`exp:devices` →
//!   `{rows, count, online}`, derived exactly as the web derives it;
//!   VAPP-119 `exp:issues` / `exp:boards` / `exp:members` / `exp:sessions`
//!   / `exp:comments` / `exp:attachments` →
//!   `domain::app_sources::source_value` over the scope team's synced rows;
//!   VAPP-122 `exp:records?kind=…` → `domain::app_records::records_value`
//!   over the hosting app's synced `app_records` rows, [`HostScope`]);
//! - VAPP-122 `harness.records.upsert {kind, data, id?}` and
//!   `harness.records.delete {id}` (tRPC `apps.records.*` as the viewer, no
//!   consent card: the app's own rows; a refusal = a failed-action toast);
//! - VAPP-120 typed host actions: every `exponential.<noun>.<op>` of
//!   `domain::app_actions::APP_ACTIONS`, the same `tools/call` as
//!   `harness.mcp` with `{op, …args}` (no `op` for `exponential.sessions.send`);
//! - extensions: the app extension (`packages/ui/exponential-ui/extension.json`)
//!   with gpui painters for its `IconDisc` and `LiveDot` natives and
//!   (VAPP-110) a team app's own package ([`IdeHost::with_packages`], the
//!   editor's preview); no package is preinstalled (Exponential ships no
//!   built-in apps, a team app may still bind `exp:devices`);
//! - policy: `harness.mcp` and `exponential.*` ASK first, through a native
//!   confirm window (`crate::native_dialog`), Deny on Return; a typed action
//!   the hosting app's `granted_tools` names runs without the window.
//!
//! The SDK's `SourceResolver` gets no `App`, so the store is observed HERE
//! (one observer per collection + a liveness tick) into a snapshot cache the
//! resolver reads synchronously; every subscription is re-emitted when its
//! value moves.

use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::rc::{Rc, Weak};
use std::sync::Arc;
use std::time::Duration;

use exponential_ui::extension::parse_extension;
use exponential_ui::host::{FunctionPolicy, MediaOptions, ParsedSource, UrlPolicy};
use exponential_ui::measure::LeafRequest;
use exponential_ui::surface::PlacedNode;
use exponential_ui::theme::{Mode, ResolvedTheme};
use exponential_ui_gpui::extension::{ExtensionPainter, PaintContext};
use exponential_ui_gpui::host::HostPlugin;
use exponential_ui_gpui::runtime::{
    ConsentHook, Emit, ExponentialHost, FunctionCallInfo, HostFunction, HostOptions, HostPolicy,
    SourceResolver,
};
use gpui::{
    div, px, svg, AnyElement, App, AppContext as _, Entity, Hsla, IntoElement, ParentElement as _,
    SharedString, Styled as _, Subscription, Task, Window,
};
use gpui_component::IconNamed as _;
use serde_json::{json, Value};

use crate::toast::Toast;

/// The SDK's declarative Devices template (VAPP-83), the tests' sample
/// package; no host installs it on its own.
#[cfg(test)]
pub(crate) const DEVICES_PACKAGE_JSON: &str =
    include_str!("../../../../../packages/ui/exponential-ui/templates/devices.json");

/// The app extension catalog (`IconDisc`, `LiveDot`, …).
pub(crate) const APP_EXTENSION_JSON: &str =
    include_str!("../../../../../packages/ui/exponential-ui/extension.json");

/// The theme every surface of this host paints with (the web's
/// `DEFAULT_THEME_ID`).
pub(crate) const THEME_ID: &str = "exponential";

/// Online-ness ages out on a clock, not on a delta (the Devices page's tick).
const LIVENESS_TICK: Duration = Duration::from_secs(30);

/// What the MCP transport requires (406 without both).
const MCP_ACCEPT: &str = "application/json, text/event-stream";
const MCP_TIMEOUT: Duration = Duration::from_secs(60);

// ---------------------------------------------------------------------------
// The `exp:devices` derivation (pure, tested)
// ---------------------------------------------------------------------------

/// The web's `readinessAgo` (Last seen): floored, `just now` under a minute,
/// then `N min ago`, `N h ago`, `N d ago`.
pub(crate) fn format_ago(seen_ms: i64, now_ms: i64) -> String {
    domain::coding_readiness::ago(now_ms, seen_ms)
}

/// One `exp:devices` row, exactly as the web host derives it:
/// `{id, label, detail, icon, discTone, tone, status}`.
pub(crate) fn device_row(row: &domain::rows::DeviceRow, now_ms: i64) -> Value {
    let online = crate::device_settings::row_is_online(row.last_seen_at.as_deref(), now_ms);
    let seen_ms = row
        .last_seen_at
        .as_deref()
        .and_then(crate::comments::parse_epoch)
        .map(|s| s * 1_000);
    let kind = if row.is_server() { "Server" } else { "Desktop" };
    let detail = [
        Some(kind.to_string()),
        row.platform.clone(),
        row.version.as_ref().map(|v| format!("v{v}")),
    ]
    .into_iter()
    .flatten()
    .filter(|part| !part.is_empty())
    .collect::<Vec<_>>()
    .join(" · ");
    let status = if online {
        "Online".to_string()
    } else {
        match seen_ms {
            Some(seen) => format!("Last seen {}", format_ago(seen, now_ms)),
            None => "Offline".to_string(),
        }
    };
    json!({
        "id": row.id,
        "label": row.label.clone().unwrap_or_default(),
        "detail": detail,
        "icon": crate::icons::device_icon_name(row.icon.as_deref(), row.is_server()),
        "discTone": if online { "success" } else { "muted" },
        "tone": if online { "live" } else { "idle" },
        "status": status,
    })
}

/// `exp:devices` → `{rows, count, online}`, rows by label.
pub(crate) fn devices_value<'a>(
    rows: impl IntoIterator<Item = &'a domain::rows::DeviceRow>,
    now_ms: i64,
) -> Value {
    let mut sorted: Vec<&domain::rows::DeviceRow> = rows.into_iter().collect();
    // `localeCompare` ≈ case-insensitive first, then the raw label.
    sorted.sort_by(|a, b| {
        let (a, b) = (a.label.as_deref().unwrap_or(""), b.label.as_deref().unwrap_or(""));
        a.to_lowercase().cmp(&b.to_lowercase()).then_with(|| a.cmp(b))
    });
    let rows: Vec<Value> = sorted.into_iter().map(|row| device_row(row, now_ms)).collect();
    let online = rows.iter().filter(|r| r["tone"] == "live").count();
    json!({ "count": rows.len(), "online": online, "rows": rows })
}

// ---------------------------------------------------------------------------
// The `exp:` source over the store
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq)]
enum Query {
    Devices,
    /// VAPP-119 `exp:<name>?…` for a `domain::app_sources` name.
    Source { name: String, params: BTreeMap<String, String> },
    /// VAPP-122 `exp:records?kind=…`.
    Records(domain::app_records::RecordsQuery),
}

/// VAPP-122: the app a host keeps records for (`exp:records`, the
/// `harness.records.*` writes): the hosting app and its team (an `app=`
/// slug reads another app of the SAME team).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct HostScope {
    pub(crate) app_id: String,
    pub(crate) team_id: Option<String>,
}

/// One synced `app_records` row as the cache holds it (timestamps as the
/// web's `toISOString`).
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct RecordLite {
    pub(crate) id: String,
    pub(crate) app_id: String,
    pub(crate) kind: String,
    pub(crate) data: Value,
    pub(crate) created_by: Option<String>,
    pub(crate) created_at: String,
    pub(crate) updated_at: String,
}

/// One live app (`id`, `slug`, `team_id`): an `app=<slug>` lookup.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct AppLite {
    pub(crate) id: String,
    pub(crate) slug: String,
    pub(crate) team_id: Option<String>,
}

/// VAPP-119: the sources side of the snapshot cache: the scope team's rows
/// (none without a team) and the viewer.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct SourcesFeed {
    pub(crate) store: domain::app_sources::SourceStore,
    pub(crate) me: Option<String>,
}

/// The records side of the snapshot cache.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct RecordsFeed {
    pub(crate) scope: Option<HostScope>,
    pub(crate) me: Option<String>,
    pub(crate) records: Vec<RecordLite>,
    pub(crate) apps: Vec<AppLite>,
    /// user id → display name.
    pub(crate) names: HashMap<String, String>,
}

/// A store timestamp (RFC 3339 or the Postgres text form) → the web's
/// `toISOString` (`2026-10-05T18:00:00.000Z`); unparsable = as is.
pub(crate) fn iso_millis(timestamp: &str) -> String {
    let trimmed = timestamp.trim();
    let t_form = trimmed.replacen(' ', "T", 1);
    [trimmed.to_string(), t_form.clone(), format!("{t_form}:00"), format!("{t_form}Z")]
        .iter()
        .find_map(|c| chrono::DateTime::parse_from_rfc3339(c).ok())
        .map(|t| t.with_timezone(&chrono::Utc).to_rfc3339_opts(chrono::SecondsFormat::Millis, true))
        .unwrap_or_else(|| trimmed.to_string())
}

/// `exp:records` over the cache: the scope's app (or the `app=` slug of the
/// same team), names of the rows' authors, the clock = `now`.
pub(crate) fn records_feed_value(feed: &RecordsFeed, query: &domain::app_records::RecordsQuery, now: &str) -> Value {
    let scope = feed.scope.as_ref();
    let app_id: Option<&str> = match &query.app {
        Some(slug) => feed
            .apps
            .iter()
            .find(|a| &a.slug == slug && scope.is_some_and(|s| a.team_id == s.team_id))
            .map(|a| a.id.as_str()),
        None => scope.map(|s| s.app_id.as_str()),
    };
    let rows: Vec<domain::app_records::RecordRowInput> = feed
        .records
        .iter()
        .filter(|r| app_id.is_some_and(|id| r.app_id == id))
        .map(|r| domain::app_records::RecordRowInput {
            id: &r.id,
            kind: &r.kind,
            data: &r.data,
            created_by: r.created_by.as_deref(),
            created_at: &r.created_at,
            updated_at: &r.updated_at,
        })
        .collect();
    let ctx = domain::app_records::RecordsContext { me: feed.me.as_deref(), now, names: &feed.names };
    domain::app_records::records_value(&rows, query, ctx).unwrap_or(Value::Null)
}

struct Sub {
    id: u64,
    query: Query,
    emit: Emit,
    last: Option<Value>,
}

/// The snapshot cache the resolver reads (it gets no `App`).
struct Feeds {
    devices: Value,
    sources: SourcesFeed,
    records: RecordsFeed,
    subs: Vec<Sub>,
    next: u64,
}

impl Default for Feeds {
    /// The template's own empty value until the store says otherwise.
    fn default() -> Self {
        Feeds {
            devices: json!({"rows": [], "count": 0, "online": 0}),
            sources: SourcesFeed::default(),
            records: RecordsFeed::default(),
            subs: Vec::new(),
            next: 0,
        }
    }
}

impl Feeds {
    fn value_for(&self, query: &Query) -> Value {
        match query {
            Query::Devices => self.devices.clone(),
            Query::Source { name, params } => {
                domain::app_sources::source_value(name, params, &self.sources.store, self.sources.me.as_deref())
            }
            Query::Records(query) => records_feed_value(&self.records, query, &now_iso()),
        }
    }

    /// Re-emit every subscription whose value moved.
    fn publish(&mut self) {
        let values: Vec<Value> = self.subs.iter().map(|s| self.value_for(&s.query)).collect();
        for (sub, value) in self.subs.iter_mut().zip(values) {
            if sub.last.as_ref() != Some(&value) {
                (sub.emit)(value.clone());
                sub.last = Some(value);
            }
        }
    }
}

/// The clock `exp:records` reads (the month view's default month, `today`).
fn now_iso() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

fn query_of(source: &ParsedSource) -> Option<Query> {
    match source.name.as_str() {
        "records" => Some(Query::Records(domain::app_records::parse_records_query(&source.params))),
        "devices" => Some(Query::Devices),
        name if domain::app_sources::is_source(name) => {
            Some(Query::Source { name: name.to_string(), params: source.params.clone() })
        }
        _ => None,
    }
}

fn exp_resolver(feeds: Weak<RefCell<Feeds>>) -> SourceResolver {
    Box::new(move |source: ParsedSource, emit: Emit| {
        let (Some(query), Some(shared)) = (query_of(&source), feeds.upgrade()) else {
            emit(Value::Null);
            return Box::new(|| {});
        };
        let id = {
            let mut f = shared.borrow_mut();
            f.next += 1;
            let id = f.next;
            let value = f.value_for(&query);
            emit(value.clone());
            f.subs.push(Sub { id, query, emit, last: Some(value) });
            id
        };
        let feeds = feeds.clone();
        Box::new(move || {
            if let Some(shared) = feeds.upgrade() {
                shared.borrow_mut().subs.retain(|s| s.id != id);
            }
        })
    })
}

fn refresh_devices(feeds: &Rc<RefCell<Feeds>>, cx: &App) {
    let Some(store) = sync::Store::try_global(cx) else { return };
    let devices = store.collections().devices.read(cx);
    let value = devices_value(devices.iter(), chrono::Utc::now().timestamp_millis());
    let mut f = feeds.borrow_mut();
    f.devices = value;
    f.publish();
}

/// VAPP-119: which part of the sources store a collection move rebuilds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SourcePart {
    /// Boards and issues (and the comments / attachments of those issues).
    Issues,
    Members,
    Users,
    Sessions,
    Comments,
    Attachments,
}

/// A store row's timestamp as the web's `toISOString` (missing = empty).
fn iso_of(timestamp: Option<&str>) -> String {
    timestamp.map(iso_millis).unwrap_or_default()
}

fn wire<T: serde::Serialize>(value: &T) -> Option<String> {
    serde_json::to_value(value).ok().and_then(|v| v.as_str().map(str::to_string))
}

fn source_comments(store: &sync::Store, issues: &HashSet<&str>, cx: &App) -> Vec<domain::app_sources::SourceComment> {
    store
        .collections()
        .comments
        .read(cx)
        .iter()
        .filter(|c| issues.contains(c.issue_id.as_str()))
        .map(|c| domain::app_sources::SourceComment {
            id: c.id.clone(),
            issue_id: c.issue_id.clone(),
            author_id: c.author_id.clone(),
            parent_id: c.parent_id.clone(),
            source: c.source.clone().unwrap_or_else(|| "user".into()),
            body: c.body.clone().unwrap_or_default(),
            created_at: iso_of(c.created_at.as_deref()),
        })
        .collect()
}

fn source_attachments(
    store: &sync::Store,
    issues: &HashSet<&str>,
    cx: &App,
) -> Vec<domain::app_sources::SourceAttachment> {
    store
        .collections()
        .attachments
        .read(cx)
        .iter()
        .filter(|a| a.issue_id.as_deref().is_some_and(|i| issues.contains(i)))
        .map(|a| domain::app_sources::SourceAttachment {
            id: a.id.clone(),
            issue_id: a.issue_id.clone(),
            comment_id: a.comment_id.clone(),
            uploader_id: a.uploader_id.clone(),
            filename: a.filename.clone().unwrap_or_default(),
            content_type: a.content_type.clone().unwrap_or_default(),
            size_bytes: a.size_bytes.unwrap_or(0),
            width: a.width,
            height: a.height,
            created_at: iso_of(a.created_at.as_deref()),
        })
        .collect()
}

/// VAPP-119: rebuild `part` of the sources store off the IDE store for the
/// scope's team (no team = no rows), re-published only on a move.
fn refresh_sources(feeds: &Rc<RefCell<Feeds>>, part: SourcePart, cx: &App) {
    use domain::app_sources as src;
    let Some(store) = sync::Store::try_global(cx) else { return };
    let collections = store.collections();
    let team = feeds.borrow().records.scope.as_ref().and_then(|s| s.team_id.clone());
    let mut next = feeds.borrow().sources.clone();
    next.me = crate::queries::active_account(cx).map(|a| a.user_id);
    match part {
        SourcePart::Issues => {
            next.store.boards = collections
                .boards
                .read(cx)
                .iter()
                .filter(|b| team.as_deref() == Some(b.team_id.as_str()))
                .map(|b| src::SourceBoard {
                    id: b.id.clone(),
                    slug: b.slug.clone().unwrap_or_default(),
                    name: b.name.clone(),
                    prefix: b.prefix.clone().unwrap_or_default(),
                    icon: b.icon.clone(),
                    color: b.color.clone(),
                    sort_order: b.sort_order.unwrap_or(0.),
                    // The synced shape leaves the trash behind.
                    deleted_at: None,
                })
                .collect();
            let boards: HashSet<&str> = next.store.boards.iter().map(|b| b.id.as_str()).collect();
            next.store.issues = collections
                .issues
                .read(cx)
                .iter()
                .filter(|i| boards.contains(i.board_id.as_str()))
                .map(|i| src::SourceIssue {
                    id: i.id.clone(),
                    board_id: i.board_id.clone(),
                    identifier: i.identifier.clone(),
                    title: i.title.clone(),
                    status: wire(&i.status).unwrap_or_default(),
                    priority: wire(&i.priority),
                    assignee_id: i.assignee_id.clone(),
                    creator_id: i.creator_id.clone(),
                    due_date: i.due_date.clone(),
                    sort_order: i.sort_order.unwrap_or(0.),
                    created_at: iso_of(i.created_at.as_deref()),
                    updated_at: iso_of(i.updated_at.as_deref()),
                })
                .collect();
            let issues: HashSet<&str> = next.store.issues.iter().map(|i| i.id.as_str()).collect();
            let comments = source_comments(store, &issues, cx);
            let attachments = source_attachments(store, &issues, cx);
            next.store.comments = comments;
            next.store.attachments = attachments;
        }
        SourcePart::Comments => {
            let issues: HashSet<&str> = next.store.issues.iter().map(|i| i.id.as_str()).collect();
            let comments = source_comments(store, &issues, cx);
            next.store.comments = comments;
        }
        SourcePart::Attachments => {
            let issues: HashSet<&str> = next.store.issues.iter().map(|i| i.id.as_str()).collect();
            let attachments = source_attachments(store, &issues, cx);
            next.store.attachments = attachments;
        }
        SourcePart::Members => {
            next.store.members = collections
                .team_members
                .read(cx)
                .iter()
                .filter(|m| team.as_deref() == Some(m.team_id.as_str()))
                .map(|m| src::SourceMember { user_id: m.user_id.clone(), role: m.role.clone().unwrap_or_default() })
                .collect();
        }
        SourcePart::Users => {
            // Every synced user: a former member still names their rows.
            next.store.users = collections
                .users
                .read(cx)
                .iter()
                .map(|u| src::SourceUser {
                    id: u.id.clone(),
                    name: u.name.clone().unwrap_or_default(),
                    email: u.email.clone(),
                    image: u.image.clone(),
                })
                .collect();
        }
        SourcePart::Sessions => {
            next.store.sessions = collections
                .sessions
                .read(cx)
                .iter()
                .filter(|s| team.is_some() && s.team_id == team)
                .map(|s| src::SourceSession {
                    id: s.id.clone(),
                    parent_id: s.parent_id.clone(),
                    agent_id: s.agent_id.clone(),
                    app_id: s.app_id.clone(),
                    user_id: s.user_id.clone(),
                    status: s.status.clone().unwrap_or_default(),
                    busy: s.busy,
                    needs_input: s.needs_input,
                    title: s.title.clone(),
                    created_at: iso_of(s.created_at.as_deref()),
                    updated_at: iso_of(s.updated_at.as_deref()),
                })
                .collect();
        }
    }
    let mut f = feeds.borrow_mut();
    if f.sources != next {
        f.sources = next;
        f.publish();
    }
}

/// VAPP-122: the records, the live apps and the members' names off the
/// store (one refresh for the three collections), re-published on a move.
fn refresh_records(feeds: &Rc<RefCell<Feeds>>, cx: &App) {
    let Some(store) = sync::Store::try_global(cx) else { return };
    let collections = store.collections();
    let records: Vec<RecordLite> = collections
        .app_records
        .read(cx)
        .iter()
        .map(|r| RecordLite {
            id: r.id.clone(),
            app_id: r.app_id.clone(),
            kind: r.kind.clone(),
            data: r.data.clone(),
            created_by: r.created_by.clone(),
            created_at: iso_millis(r.created_at.as_deref().unwrap_or("")),
            updated_at: iso_millis(r.updated_at.as_deref().unwrap_or("")),
        })
        .collect();
    let apps: Vec<AppLite> = collections
        .apps
        .read(cx)
        .iter()
        .filter(|a| a.deleted_at.is_none())
        .map(|a| AppLite { id: a.id.clone(), slug: a.slug.clone(), team_id: a.team_id.clone() })
        .collect();
    let names: HashMap<String, String> = collections
        .users
        .read(cx)
        .iter()
        .map(|u| (u.id.clone(), u.name.clone().unwrap_or_default()))
        .collect();
    let me = crate::queries::active_account(cx).map(|a| a.user_id);
    let mut f = feeds.borrow_mut();
    let next = RecordsFeed { scope: f.records.scope.clone(), me, records, apps, names };
    if f.records != next {
        f.records = next;
        f.publish();
    }
}

// ---------------------------------------------------------------------------
// Functions
// ---------------------------------------------------------------------------

fn arg_str(args: &Value, key: &str) -> String {
    match args.get(key) {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Null) | None => String::new(),
        Some(other) => other.to_string(),
    }
}

fn toast_fn() -> HostFunction {
    exponential_ui_gpui::runtime::sync_function(|args, _, cx| {
        let text = arg_str(&args, "message");
        let toast = if args.get("tone").and_then(Value::as_str) == Some("error") {
            Toast::error(text)
        } else {
            Toast::info(text)
        };
        crate::toast::show_in_active_window(toast, cx);
        Ok(Value::Null)
    })
}

fn open_issue_fn() -> HostFunction {
    exponential_ui_gpui::runtime::sync_function(|args, _, cx| {
        let identifier = arg_str(&args, "identifier");
        let hit = sync::Store::try_global(cx).and_then(|store| {
            store
                .collections()
                .issues
                .read(cx)
                .iter()
                .find(|i| i.identifier == identifier)
                .map(|i| (i.id.clone(), i.board_id.clone()))
        });
        match hit {
            Some((issue_id, board_id)) => crate::navigation::on_active_window(cx, move |window, cx| {
                crate::navigation::open_issue_scoped(window, cx, issue_id, board_id, None)
            }),
            None => crate::action_error::failed_action(
                "open",
                &identifier,
                Some("It is not synced here.".into()),
                cx,
            ),
        }
        Ok(Value::Null)
    })
}

fn open_device_fn() -> HostFunction {
    exponential_ui_gpui::runtime::sync_function(|args, _, cx| {
        let id = arg_str(&args, "id");
        let me = crate::queries::active_account(cx).map(|a| a.user_id);
        // The settings dialog edits OWN rows only (the machines menu never
        // offers it on a teammate's); anything else gets the web's toast.
        let own = sync::Store::try_global(cx).is_some_and(|store| {
            store
                .collections()
                .devices
                .read(cx)
                .get(&id)
                .is_some_and(|row| row.user_id.is_some() && row.user_id == me)
        });
        if own {
            crate::navigation::on_active_window(cx, move |window, cx| {
                crate::device_settings::open(window, cx, id)
            });
        } else {
            let label = arg_str(&args, "label");
            crate::toast::show_in_active_window(Toast::info(if label.is_empty() { id } else { label }), cx);
        }
        Ok(Value::Null)
    })
}

/// VAPP-122: a `harness.records.*` write on the background executor; a
/// refusal (the kind's schema, the caps, the plan quota) toasts the web's
/// failed action (`Couldn't <verb> the record`) and rejects the call. The
/// synced shape repaints the binding.
fn records_write<T: Send + 'static>(
    verb: &'static str,
    scope: Option<HostScope>,
    cx: &mut App,
    call: impl FnOnce(&api::TrpcClient, &str) -> Result<T, api::ApiError> + Send + 'static,
    answer: impl FnOnce(T) -> Value + 'static,
) -> Task<Result<Value, String>> {
    let (Some(scope), Some(trpc)) = (scope, crate::queries::trpc_client(cx)) else {
        crate::action_error::failed_action(verb, "the record", None, cx);
        return Task::ready(Err("No app to keep records for".to_string()));
    };
    cx.spawn(async move |cx| {
        let result = cx
            .background_executor()
            .spawn(async move { call(&trpc, &scope.app_id) })
            .await;
        match result {
            Ok(out) => Ok(answer(out)),
            Err(err) => {
                let message = err.to_string();
                let _ = cx.update(|cx| crate::action_error::failed_action_err(verb, "the record", &err, cx));
                Err(message)
            }
        }
    })
}

fn records_upsert_fn(scope: Option<HostScope>) -> HostFunction {
    Rc::new(move |args: Value, _: &FunctionCallInfo, cx: &mut App| -> Task<Result<Value, String>> {
        let kind = arg_str(&args, "kind");
        let data = args.get("data").cloned().filter(Value::is_object).unwrap_or_else(|| json!({}));
        let record_id = args.get("id").and_then(Value::as_str).map(str::to_string);
        records_write(
            "save",
            scope.clone(),
            cx,
            move |trpc, app_id| {
                api::apps::records_upsert(
                    trpc,
                    &api::apps::RecordUpsertInput { id: app_id.to_string(), kind, data, record_id },
                )
            },
            |out| json!({ "id": out.id }),
        )
    })
}

fn records_delete_fn(scope: Option<HostScope>) -> HostFunction {
    Rc::new(move |args: Value, _: &FunctionCallInfo, cx: &mut App| -> Task<Result<Value, String>> {
        let record_id = arg_str(&args, "id");
        records_write(
            "delete",
            scope.clone(),
            cx,
            move |trpc, app_id| api::apps::records_delete(trpc, app_id, &record_id),
            |_| Value::Null,
        )
    })
}

/// The id `api::mcp_tools::tool_call_body` stamps on every request.
const MCP_REQUEST_ID: i64 = 1;

/// The message with `id` in one JSON text (a message or a batch), like the
/// web's `parseJsonRpcMessage`; not JSON / no match = `None`.
fn json_rpc_message(text: &str, id: i64) -> Option<Value> {
    let parsed: Value = serde_json::from_str(text).ok()?;
    let messages = match parsed {
        Value::Array(items) => items,
        one => vec![one],
    };
    messages.into_iter().find(|m| m.get("id").and_then(Value::as_i64) == Some(id))
}

/// The answer to request `id` in a JSON or SSE body (the web's
/// `readJsonRpcAnswer`): SSE events are read in order and notifications or
/// foreign ids skipped.
fn json_rpc_answer(body: &str, id: i64) -> Option<Value> {
    if body.trim_start().starts_with(['{', '[']) {
        return json_rpc_message(body, id);
    }
    let mut data: Vec<&str> = Vec::new();
    for line in body.lines().map(|l| l.strip_suffix('\r').unwrap_or(l)) {
        if let Some(rest) = line.strip_prefix("data:") {
            data.push(rest.strip_prefix(' ').unwrap_or(rest));
        } else if line.is_empty() && !data.is_empty() {
            if let Some(message) = json_rpc_message(&data.join("\n"), id) {
                return Some(message);
            }
            data.clear();
        }
    }
    (!data.is_empty()).then(|| json_rpc_message(&data.join("\n"), id)).flatten()
}

/// A JSON-RPC `tools/call` response (plain JSON or SSE) → the `result` of
/// the message answering request `id`, like the web host's `callMcp`. A
/// JSON-RPC `error` rejects with its message and code (the web's
/// `McpCallError`); no answer with `id` rejects too.
pub(crate) fn mcp_result(body: &str, id: i64) -> Result<Value, String> {
    let message = json_rpc_answer(body, id).ok_or_else(|| "MCP: no answer".to_string())?;
    if let Some(error) = message.get("error") {
        let text = error["message"].as_str().filter(|m| !m.is_empty()).unwrap_or("MCP error");
        return Err(match error.get("code").and_then(Value::as_i64) {
            Some(code) => format!("{text} (code {code})"),
            None => text.to_string(),
        });
    }
    Ok(message.get("result").cloned().unwrap_or(Value::Null))
}

/// One `tools/call` on the instance's own `/api/mcp` as the signed-in
/// account (the web's `callMcp`), on the background executor.
fn mcp_call(tool: String, arguments: Value, cx: &mut App) -> Task<Result<Value, String>> {
    let Some(trpc) = crate::queries::trpc_client(cx) else {
        return Task::ready(Err("not signed in".to_string()));
    };
    cx.background_spawn(async move {
        let body = api::mcp_tools::tool_call_body(&tool, &arguments);
        let response = trpc
            .post_json("/api/mcp", &body, MCP_ACCEPT, MCP_TIMEOUT)
            .map_err(|e| format!("MCP {e}"))?;
        mcp_result(&response, MCP_REQUEST_ID)
    })
}

fn mcp_fn() -> HostFunction {
    Rc::new(|args: Value, _: &FunctionCallInfo, cx: &mut App| -> Task<Result<Value, String>> {
        let tool = arg_str(&args, "tool");
        let arguments = args.get("arguments").cloned().filter(Value::is_object).unwrap_or_else(|| json!({}));
        mcp_call(tool, arguments, cx)
    })
}

/// VAPP-120: a typed action's MCP `arguments`: `{op, …args}` (the web's
/// `{...args, op}`: the NAME's op wins, an app-supplied `op` key is dropped,
/// so the card and the grant always name the operation that runs), the args
/// alone for a verb tool (`op` = `None`); a non-object `args` reads as `{}`.
pub(crate) fn action_arguments(op: Option<&str>, args: &Value) -> Value {
    let mut map = serde_json::Map::new();
    if let Some(op) = op {
        map.insert("op".into(), Value::String(op.to_string()));
    }
    if let Some(given) = args.as_object() {
        for (key, value) in given {
            if op.is_some() && key == "op" {
                continue;
            }
            map.insert(key.clone(), value.clone());
        }
    }
    Value::Object(map)
}

/// VAPP-120: `exponential.<noun>.<op>` → a `tools/call` of its tool.
fn action_fn(target: domain::app_actions::ActionTarget) -> HostFunction {
    Rc::new(move |args: Value, _: &FunctionCallInfo, cx: &mut App| -> Task<Result<Value, String>> {
        mcp_call(target.tool.clone(), action_arguments(target.op.as_deref(), &args), cx)
    })
}

/// VAPP-120: a typed action the app's grants name runs without asking
/// (the web's `onFunctionCall`); `harness.mcp` and anything untyped always
/// ask.
pub(crate) fn call_granted(name: &str, args: &Value, granted: &[String]) -> bool {
    let empty = serde_json::Map::new();
    let args = args.as_object().unwrap_or(&empty);
    domain::app_actions::parse_action_name(name).is_some() && domain::app_actions::grant_allows(granted, name, args)
}

/// The hosting app's live `granted_tools` (no scope / no row = none).
fn scope_grants(scope: Option<&HostScope>, cx: &App) -> Vec<String> {
    let (Some(scope), Some(store)) = (scope, sync::Store::try_global(cx)) else {
        return Vec::new();
    };
    store
        .collections()
        .apps
        .read(cx)
        .get(&scope.app_id)
        .map(|row| row.granted_tools.clone())
        .unwrap_or_default()
}

/// VAPP-107: the private hosts the IDE's policy opts back in (the web
/// host's `ownPrivateHosts`): the instance's OWN host when it is private
/// (a localhost dev instance), else none; the LAN stays denied either way.
pub(crate) fn own_private_hosts(instance_url: &str) -> Vec<String> {
    url::Url::parse(instance_url)
        .ok()
        .and_then(|u| u.host_str().map(str::to_lowercase))
        .filter(|h| exponential_ui::host::is_private_host(h))
        .into_iter()
        .collect()
}

/// VAPP-107 (the consent prompt's `slot`): what the window shows of a call,
/// derived exactly as the web's `consentWords`: the tool it names (a
/// `harness.mcp` call's MCP tool, else the function's own name), its
/// arguments as compact JSON (the MCP `arguments`, else the function's
/// args; an empty object = none) and whether it sounds destructive.
pub(crate) struct ConsentWords {
    pub tool: String,
    pub args: Option<String>,
    pub destructive: bool,
    /// VAPP-120: a typed action's own title (`actionConsent`), replacing the
    /// generic prompt's.
    pub title: Option<String>,
}

/// The desktop shows this many characters of the arguments (the web's
/// preview length; it has no disclosure in a native alert).
pub(crate) const CONSENT_ARGS_PREVIEW: usize = 160;

pub(crate) fn consent_words(call: &FunctionCallInfo) -> ConsentWords {
    // VAPP-120: a typed action is worded for its operation (the contract's rows).
    let empty = serde_json::Map::new();
    let given = call.args.as_object().unwrap_or(&empty);
    if let Some(typed) = domain::app_actions::action_consent(&call.name, given) {
        return ConsentWords {
            tool: typed.tool,
            args: typed.args,
            destructive: typed.destructive,
            title: Some(typed.title),
        };
    }
    let mcp = call.name == "harness.mcp";
    let tool = if mcp { arg_str(&call.args, "tool") } else { call.name.clone() };
    let raw = if mcp { call.args.get("arguments") } else { Some(&call.args) };
    let raw = raw.filter(|v| v.is_object());
    let args = raw.map(|v| v.to_string()).filter(|s| s != "{}");
    let op = raw.and_then(|v| v.get("op")).and_then(Value::as_str);
    ConsentWords { tool: tool.clone(), args, destructive: domain::prompts::destructive_tool(&tool, op), title: None }
}

/// What the consent window asks (`prompts.json` `exponential-ui-consent`,
/// Deny focused so Return denies): the tool a `harness.mcp` call runs, else
/// the function's own name (the web's `ConsentCard`); a destructive-sounding
/// call takes the `exponential-ui-consent-destructive` entry. VAPP-120: a
/// typed action's title (`action_consent`) replaces the entry's.
pub(crate) fn consent_prompt(call: &FunctionCallInfo) -> domain::prompts::Prompt {
    let words = consent_words(call);
    let mut prompt = if words.destructive {
        domain::prompts::exponential_ui_consent_destructive(&words.tool)
    } else {
        domain::prompts::exponential_ui_consent(&words.tool)
    };
    if let Some(title) = words.title {
        prompt.title = title;
    }
    prompt
}

/// The arguments line the alert's code block shows: the compact JSON, cut
/// to [`CONSENT_ARGS_PREVIEW`] characters with an ellipsis past it.
pub(crate) fn consent_args_preview(args: &str) -> String {
    match args.char_indices().nth(CONSENT_ARGS_PREVIEW) {
        Some((at, _)) => format!("{}…", &args[..at]),
        None => args.to_string(),
    }
}

/// `ask` decisions: a native confirm over the active window. No window, a
/// dismissal or Deny = `false`. The arguments (when any) sit under the body
/// in the CodeBlock (sm, json, no header), like the web card's. VAPP-120: a
/// typed action the hosting app's live row grants runs without the window.
fn consent_hook(scope: Option<HostScope>) -> ConsentHook {
    Rc::new(move |call: &FunctionCallInfo, cx: &mut App| -> Task<bool> {
        if call_granted(&call.name, &call.args, &scope_grants(scope.as_ref(), cx)) {
            return Task::ready(true);
        }
        let (tx, rx) = flume::bounded::<bool>(1);
        let prompt = consent_prompt(call);
        let args = consent_words(call).args.map(|a| consent_args_preview(&a));
        crate::navigation::on_active_window(cx, move |window, cx| {
            let (ok, closed) = (tx.clone(), tx);
            let mut spec = crate::native_dialog::AlertSpec::from_prompt("Exponential UI", &prompt)
                .on_ok(move |_, _| {
                    let _ = ok.try_send(true);
                    true
                })
                .on_closed(move |_| {
                    let _ = closed.try_send(false);
                });
            if let Some(args) = args {
                spec = spec.height(gpui::px(300.)).content(move |_, cx| {
                    crate::code_block::code_block(
                        crate::code_block::CodeBlockProps {
                            id: "exponential-ui-consent-arguments".into(),
                            title: None,
                            code: args.clone(),
                            language: "json",
                            destructive: false,
                            copyable: false,
                        },
                        cx,
                    )
                });
            }
            crate::native_dialog::open_alert(window, cx, spec);
        });
        cx.background_spawn(async move { rx.recv_async().await.unwrap_or(false) })
    })
}

// ---------------------------------------------------------------------------
// The painter plugin + the app-extension painters
// ---------------------------------------------------------------------------

/// The rest of the painter callbacks: icons by CONCEPT from the generated
/// registry (then raw Lucide names, the web host's map), fonts as the
/// kitchen sink (the painter default).
pub(crate) struct IdePlugin;

pub(crate) fn icon_path(name: &str) -> Option<SharedString> {
    crate::icons::registry::concept_by_name(name)
        .or_else(|| crate::icons::registry::icon_by_name(name))
        .map(|icon| icon.path())
}

impl HostPlugin for IdePlugin {
    fn icon(&self, name: &str) -> Option<SharedString> {
        icon_path(name)
    }

    fn on_unknown(&self, node: &PlacedNode) {
        log::warn!("[exponential-ui host] unknown component {} ({})", node.component, node.id);
    }
}

fn theme_color(theme: Option<&Arc<ResolvedTheme>>, mode: Mode, key: &str) -> Option<Hsla> {
    theme?
        .modes
        .get(mode)
        .color
        .get(key)
        .and_then(|hex| exponential_ui_gpui::paint::color::parse_hex(hex))
}

fn prop_str<'a>(node: &'a PlacedNode, key: &str) -> Option<&'a str> {
    node.props.get(key).and_then(Value::as_str)
}

/// `IconDisc`: a 32 px circle in the tone colour at 15 % with the icon
/// concept centred in the tone colour (the web's `IconDiscPainter`).
pub(crate) struct IconDiscPainter;

/// The disc tone → the theme colour key.
pub(crate) fn disc_color_key(tone: Option<&str>) -> &'static str {
    match tone {
        Some("success") => "success",
        Some("danger") => "destructive",
        Some("muted") => "mutedForeground",
        _ => "primary",
    }
}

impl ExtensionPainter for IconDiscPainter {
    fn measure(&self, _: &LeafRequest, _: Option<f32>, _: &mut Window, _: &mut App) -> Option<(f32, f32)> {
        Some((32., 32.))
    }

    fn paint(&self, ctx: PaintContext, _: &mut Window, _: &mut App) -> AnyElement {
        let color = theme_color(ctx.theme, ctx.mode, disc_color_key(prop_str(ctx.node, "tone")))
            .or_else(|| theme_color(ctx.theme, ctx.mode, "primary"))
            .unwrap_or_else(|| theme::tokens::BLUE.to_hsla());
        let disc = div()
            .size(px(32.))
            .flex_shrink_0()
            .flex()
            .items_center()
            .justify_center()
            .rounded_full()
            .bg(color.opacity(0.15));
        match prop_str(ctx.node, "icon").and_then(icon_path) {
            Some(path) => disc.child(svg().path(path).size(px(16.)).text_color(color)),
            None => disc,
        }
        .into_any_element()
    }
}

/// `LiveDot`: the 8 px status dot (the SDK's `controls::live_dot`), in the
/// colours the IDE paints its device and session dots with.
pub(crate) struct LiveDotPainter;

impl ExtensionPainter for LiveDotPainter {
    fn measure(&self, _: &LeafRequest, _: Option<f32>, _: &mut Window, _: &mut App) -> Option<(f32, f32)> {
        let size = exponential_ui_gpui::controls::LIVE_DOT_PX;
        Some((size, size))
    }

    fn paint(&self, ctx: PaintContext, _: &mut Window, _: &mut App) -> AnyElement {
        let muted = theme_color(ctx.theme, ctx.mode, "mutedForeground")
            .unwrap_or_else(|| theme::tokens::NEUTRAL.to_hsla());
        let tone = match prop_str(ctx.node, "tone") {
            Some("live") => theme::tokens::GREEN.to_hsla(),
            Some("attention") => theme::tokens::YELLOW.to_hsla(),
            Some("done") => theme::tokens::BLUE.to_hsla(),
            Some("unread") => theme_color(ctx.theme, ctx.mode, "primary").unwrap_or(muted),
            Some("idle") => muted.opacity(0.4),
            _ => muted,
        };
        let ping = ctx.node.props.get("ping").and_then(Value::as_bool).unwrap_or(false);
        div()
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .child(exponential_ui_gpui::controls::live_dot(tone, ping))
            .into_any_element()
    }
}

// ---------------------------------------------------------------------------
// The host
// ---------------------------------------------------------------------------

/// The IDE's host: the SDK's [`ExponentialHost`] entity plus the store
/// observers feeding its `exp:` source. Keep it alive as long as its
/// surfaces are on screen.
pub(crate) struct IdeHost {
    pub(crate) host: Entity<ExponentialHost>,
    _feeds: Rc<RefCell<Feeds>>,
    _subscriptions: Vec<Subscription>,
    _tick: Task<()>,
}

impl IdeHost {
    /// VAPP-110: the host with `packages` installed (a team app's package,
    /// the editor's preview) and the package's
    /// `theme` (the default theme when `None` or unknown). VAPP-122:
    /// `scope` = the app whose records `exp:records` reads and
    /// `harness.records.*` write (`None` = no app: the binding reads no
    /// rows, a write fails).
    pub(crate) fn with_packages(
        mode: Mode,
        packages: &[Value],
        theme_id: Option<&str>,
        scope: Option<HostScope>,
        cx: &mut App,
    ) -> Self {
        let feeds = Rc::new(RefCell::new(Feeds::default()));
        feeds.borrow_mut().records.scope = scope.clone();
        refresh_devices(&feeds, cx);
        for part in [
            SourcePart::Issues,
            SourcePart::Members,
            SourcePart::Users,
            SourcePart::Sessions,
        ] {
            refresh_sources(&feeds, part, cx);
        }
        refresh_records(&feeds, cx);
        let mut subscriptions = Vec::new();
        if let Some(store) = sync::Store::try_global(cx) {
            let collections = store.collections().clone();
            let devices = collections.devices.clone();
            let (records, apps, users) =
                (collections.app_records.clone(), collections.apps.clone(), collections.users.clone());
            let f = feeds.clone();
            subscriptions.push(cx.observe(&devices, move |_, cx| refresh_devices(&f, cx)));
            // VAPP-119: each collection rebuilds the sources part it feeds.
            macro_rules! observe_part {
                ($entity:expr, $part:expr) => {{
                    let f = feeds.clone();
                    subscriptions.push(cx.observe(&$entity, move |_, cx| refresh_sources(&f, $part, cx)));
                }};
            }
            observe_part!(collections.boards, SourcePart::Issues);
            observe_part!(collections.issues, SourcePart::Issues);
            observe_part!(collections.team_members, SourcePart::Members);
            observe_part!(collections.users, SourcePart::Users);
            observe_part!(collections.sessions, SourcePart::Sessions);
            observe_part!(collections.comments, SourcePart::Comments);
            observe_part!(collections.attachments, SourcePart::Attachments);
            let f = feeds.clone();
            subscriptions.push(cx.observe(&records, move |_, cx| refresh_records(&f, cx)));
            let f = feeds.clone();
            subscriptions.push(cx.observe(&apps, move |_, cx| refresh_records(&f, cx)));
            let f = feeds.clone();
            subscriptions.push(cx.observe(&users, move |_, cx| refresh_records(&f, cx)));
        }
        let weak = Rc::downgrade(&feeds);
        let tick = cx.spawn(async move |cx| loop {
            cx.background_executor().timer(LIVENESS_TICK).await;
            let Some(feeds) = weak.upgrade() else { return };
            cx.update(|cx| refresh_devices(&feeds, cx));
        });

        let instance = sync::Store::try_global(cx)
            .and_then(|_| crate::queries::active_account(cx))
            .map(|a| a.instance_url);
        let allow_private = instance.as_deref().map(own_private_hosts);
        let mut functions: HashMap<String, HostFunction> = HashMap::new();
        functions.insert("harness.toast".into(), toast_fn());
        functions.insert("harness.openIssue".into(), open_issue_fn());
        functions.insert("harness.openDevice".into(), open_device_fn());
        functions.insert("harness.mcp".into(), mcp_fn());
        functions.insert("harness.records.upsert".into(), records_upsert_fn(scope.clone()));
        functions.insert("harness.records.delete".into(), records_delete_fn(scope.clone()));
        // VAPP-120: every MCP operation as a typed function.
        for name in domain::app_actions::APP_ACTIONS.iter() {
            if let Some(target) = domain::app_actions::parse_action_name(name) {
                functions.insert(name.clone(), action_fn(target));
            }
        }
        let mut sources: HashMap<String, SourceResolver> = HashMap::new();
        sources.insert("exp".into(), exp_resolver(Rc::downgrade(&feeds)));
        let extensions = match parse_extension(APP_EXTENSION_JSON) {
            Ok(ext) => vec![ext],
            Err(e) => {
                log::error!("[exponential-ui host] the app extension does not parse: {e}");
                Vec::new()
            }
        };
        let options = HostOptions {
            functions,
            sources,
            extensions,
            painters: vec![
                ("IconDisc".into(), Rc::new(IconDiscPainter) as Rc<dyn ExtensionPainter>),
                ("LiveDot".into(), Rc::new(LiveDotPainter) as Rc<dyn ExtensionPainter>),
            ],
            policy: HostPolicy {
                functions: Some(FunctionPolicy {
                    ask: Some(vec!["harness.mcp".into(), "exponential.*".into()]),
                    ..Default::default()
                }),
                on_function_call: Some(consent_hook(scope)),
                urls: Some(UrlPolicy {
                    base_url: instance.clone(),
                    allow_private: allow_private.clone(),
                    ..Default::default()
                }),
                open_url: None,
                media: Some(MediaOptions {
                    base_url: instance,
                    allow_private,
                    ..Default::default()
                }),
            },
            theme: theme_id
                .and_then(exponential_ui::themes::builtin_theme)
                .or_else(|| exponential_ui::themes::builtin_theme(THEME_ID)),
            mode,
            plugin: Rc::new(IdePlugin),
            ..HostOptions::default()
        };
        let host = cx.new(|cx| {
            let mut host = ExponentialHost::new(options, cx);
            for pkg in packages {
                for issue in host.install_package(pkg) {
                    log::warn!("[exponential-ui host] app package: {issue:?}");
                }
            }
            host
        });
        Self { host, _feeds: feeds, _subscriptions: subscriptions, _tick: tick }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn device(value: Value) -> domain::rows::DeviceRow {
        serde_json::from_value(value).expect("a devices row")
    }

    const NOW: i64 = 1_791_460_800_000; // 2026-10-08T12:00:00Z

    #[test]
    fn format_ago_matches_the_web() {
        // The web's `readinessAgo`: floored, never rounded up.
        assert_eq!(format_ago(NOW, NOW), "just now");
        assert_eq!(format_ago(NOW - 59_000, NOW), "just now");
        assert_eq!(format_ago(NOW - 60_000, NOW), "1 min ago");
        assert_eq!(format_ago(NOW - 119_000, NOW), "1 min ago");
        assert_eq!(format_ago(NOW - 59 * 60_000, NOW), "59 min ago");
        assert_eq!(format_ago(NOW - 119 * 60_000, NOW), "1 h ago");
        assert_eq!(format_ago(NOW - 3 * 3_600_000, NOW), "3 h ago");
        assert_eq!(format_ago(NOW - 47 * 3_600_000, NOW), "1 d ago");
        assert_eq!(format_ago(NOW - 72 * 3_600_000, NOW), "3 d ago");
    }

    #[test]
    fn device_rows_derive_like_the_web_host() {
        let online = device(json!({
            "id": "d1", "label": "MacBook Pro", "kind": "desktop", "platform": "macos",
            "version": "0.18.80", "last_seen_at": "2026-10-08T11:59:50Z"
        }));
        let server = device(json!({
            "id": "d2", "label": "homeserver", "kind": "server", "platform": "linux",
            "icon": "server", "last_seen_at": "2026-10-08 09:00:00+00"
        }));
        let never = device(json!({ "id": "d3", "label": "Old box", "kind": "desktop" }));
        let value = devices_value([&server, &never, &online], NOW);
        assert_eq!(value["count"], 3);
        assert_eq!(value["online"], 1);
        let rows = value["rows"].as_array().unwrap();
        let labels: Vec<&str> = rows.iter().map(|r| r["label"].as_str().unwrap()).collect();
        assert_eq!(labels, ["homeserver", "MacBook Pro", "Old box"]);
        assert_eq!(
            rows[1],
            json!({
                "id": "d1", "label": "MacBook Pro", "detail": "Desktop · macos · v0.18.80",
                "icon": "monitor", "discTone": "success", "tone": "live", "status": "Online"
            })
        );
        assert_eq!(rows[0]["detail"], "Server · linux");
        assert_eq!(rows[0]["icon"], "server");
        assert_eq!(rows[0]["tone"], "idle");
        assert_eq!(rows[0]["discTone"], "muted");
        assert_eq!(rows[0]["status"], "Last seen 3 h ago");
        assert_eq!(rows[2]["detail"], "Desktop");
        assert_eq!(rows[2]["status"], "Offline");
    }

    #[test]
    fn the_exp_source_emits_its_snapshot_and_cancels() {
        let feeds = Rc::new(RefCell::new(Feeds::default()));
        feeds.borrow_mut().devices = json!({"rows": [], "count": 0, "online": 0});
        let resolver = exp_resolver(Rc::downgrade(&feeds));
        let got = Arc::new(std::sync::Mutex::new(Vec::<Value>::new()));
        let sink = got.clone();
        let emit: Emit = Arc::new(move |v| sink.lock().unwrap().push(v));
        let parse = |s: &str| exponential_ui::host::parse_source(s).unwrap();
        let cancel = resolver(parse("exp:devices"), emit.clone());
        assert_eq!(got.lock().unwrap().len(), 1);
        // A move re-emits; an unchanged publish does not.
        feeds.borrow_mut().devices = json!({"rows": [], "count": 1, "online": 0});
        feeds.borrow_mut().publish();
        feeds.borrow_mut().publish();
        assert_eq!(got.lock().unwrap().len(), 2);
        cancel();
        assert!(feeds.borrow().subs.is_empty());
        // An unknown name emits null at once.
        let _ = resolver(parse("exp:nope"), emit);
        assert_eq!(got.lock().unwrap().last(), Some(&Value::Null));
    }

    fn record(id: &str, app: &str, hours: i64, by: &str) -> RecordLite {
        RecordLite {
            id: id.into(),
            app_id: app.into(),
            kind: "entry".into(),
            data: json!({"date": "2026-10-05", "hours": hours}),
            created_by: Some(by.into()),
            created_at: iso_millis("2026-10-05 18:00:00+00"),
            updated_at: iso_millis("2026-10-05T18:00:00Z"),
        }
    }

    fn records_feed() -> RecordsFeed {
        RecordsFeed {
            scope: Some(HostScope { app_id: "a1".into(), team_id: Some("t1".into()) }),
            me: Some("u1".into()),
            records: vec![record("r1", "a1", 3, "u1"), record("r2", "a2", 4, "u2"), record("r3", "a3", 5, "u2")],
            apps: vec![
                AppLite { id: "a1".into(), slug: "hours".into(), team_id: Some("t1".into()) },
                AppLite { id: "a2".into(), slug: "other".into(), team_id: Some("t1".into()) },
                AppLite { id: "a3".into(), slug: "foreign".into(), team_id: Some("t2".into()) },
            ],
            names: HashMap::from([("u1".to_string(), "Danny".to_string())]),
        }
    }

    /// VAPP-122: the hosting app's rows, `app=` = another app of the SAME
    /// team, the web's ISO timestamps and author names.
    #[test]
    fn records_read_the_scoped_app() {
        let feed = records_feed();
        let query = |s: &str| match query_of(&exponential_ui::host::parse_source(s).unwrap()) {
            Some(Query::Records(q)) => q,
            other => panic!("{other:?}"),
        };
        let now = "2026-10-24T09:00:00.000Z";
        let own = records_feed_value(&feed, &query("exp:records?kind=entry&sum=hours"), now);
        assert_eq!(own["count"], 1);
        assert_eq!(own["sum"], 3);
        assert_eq!(own["rows"][0]["author"], "Danny");
        assert_eq!(own["rows"][0]["createdAt"], "2026-10-05T18:00:00.000Z");
        let other = records_feed_value(&feed, &query("exp:records?kind=entry&app=other"), now);
        assert_eq!(other["rows"][0]["id"], "r2");
        assert_eq!(other["rows"][0]["author"], "");
        let foreign = records_feed_value(&feed, &query("exp:records?kind=entry&app=foreign"), now);
        assert_eq!(foreign["count"], 0, "another team's app reads nothing");
        assert_eq!(records_feed_value(&feed, &query("exp:records"), now), Value::Null);
        let unscoped = RecordsFeed { scope: None, ..records_feed() };
        assert_eq!(records_feed_value(&unscoped, &query("exp:records?kind=entry"), now)["count"], 0);
    }

    #[test]
    fn records_re_emit_only_on_a_move() {
        let feeds = Rc::new(RefCell::new(Feeds::default()));
        feeds.borrow_mut().records = records_feed();
        let resolver = exp_resolver(Rc::downgrade(&feeds));
        let got = Arc::new(std::sync::Mutex::new(Vec::<Value>::new()));
        let sink = got.clone();
        let emit: Emit = Arc::new(move |v| sink.lock().unwrap().push(v));
        let _cancel = resolver(exponential_ui::host::parse_source("exp:records?kind=entry").unwrap(), emit);
        assert_eq!(got.lock().unwrap()[0]["count"], 1);
        feeds.borrow_mut().publish();
        assert_eq!(got.lock().unwrap().len(), 1);
        feeds.borrow_mut().records.records.push(record("r4", "a1", 1, "u1"));
        feeds.borrow_mut().publish();
        assert_eq!(got.lock().unwrap().len(), 2);
        assert_eq!(got.lock().unwrap()[1]["count"], 2);
    }

    struct Blank;
    impl gpui::Render for Blank {
        fn render(&mut self, _: &mut Window, _: &mut gpui::Context<Self>) -> impl IntoElement {
            div()
        }
    }

    fn seed<T: sync::ShapeRow + serde::de::DeserializeOwned>(
        collection: &Entity<sync::Collection<T>>,
        rows: Value,
        cx: &mut App,
    ) {
        collection.update(cx, |c, cx| {
            for row in rows.as_array().unwrap() {
                let row: T = serde_json::from_value(row.clone()).expect("a store row");
                c.seed(row.key(), row);
            }
            cx.notify();
        });
    }

    /// VAPP-119: `exp:members` and `exp:comments?issue=…` bound through the
    /// host over a seeded store emit exactly `domain::app_sources` over the
    /// same rows (the scope's team only, the web's ISO timestamps), and a
    /// new comment re-emits.
    #[gpui::test]
    fn sources_bind_over_the_seeded_store(cx: &mut gpui::TestAppContext) {
        use domain::app_sources as src;
        cx.update(|cx| {
            gpui_component::init(cx);
            let store = sync::Store::open(cx, None, None);
            cx.set_global(store);
            let c = sync::Store::global(cx).collections().clone();
            seed(&c.boards, json!([
                {"id": "b1", "team_id": "t1", "name": "Mobile App", "slug": "mobile-app", "prefix": "APP", "sort_order": 0},
                {"id": "b9", "team_id": "t2", "name": "Elsewhere", "slug": "else", "prefix": "ELS"}
            ]), cx);
            seed(&c.issues, json!([
                {"id": "i1", "board_id": "b1", "number": 1, "identifier": "APP-1", "title": "Crash on launch", "status": "in_progress", "priority": "high", "created_at": "2026-10-01 09:00:00+00", "updated_at": "2026-10-20T09:00:00Z"},
                {"id": "i9", "board_id": "b9", "number": 1, "identifier": "ELS-1", "title": "Foreign", "status": "backlog"}
            ]), cx);
            seed(&c.team_members, json!([
                {"id": "m1", "team_id": "t1", "user_id": "u2", "role": "member"},
                {"id": "m2", "team_id": "t1", "user_id": "u1", "role": "owner"},
                {"id": "m3", "team_id": "t2", "user_id": "u3", "role": "owner"}
            ]), cx);
            seed(&c.users, json!([
                {"id": "u1", "name": "Alex Carter", "email": "alex@acme.dev", "image": "https://cdn.example/alex.png"},
                {"id": "u2", "name": "Jonas Weber", "email": "jonas@acme.dev"},
                {"id": "u3", "name": "Mira Sato"}
            ]), cx);
            seed(&c.comments, json!([
                {"id": "c2", "issue_id": "i1", "author_id": "u1", "parent_id": "c1", "source": "user", "body": "Fix is in #1210.", "created_at": "2026-10-21 10:00:00+00"},
                {"id": "c1", "issue_id": "i1", "author_id": "u2", "body": "Repro on the Pixel 8.", "created_at": "2026-10-20T10:00:00Z"},
                {"id": "c9", "issue_id": "i9", "author_id": "u3", "body": "Elsewhere", "created_at": "2026-10-20T10:00:00Z"}
            ]), cx);
        });
        let ide = cx.update(|cx| {
            let scope = HostScope { app_id: "a1".into(), team_id: Some("t1".into()) };
            IdeHost::with_packages(Mode::Dark, &[], None, Some(scope), cx)
        });
        let feeds = ide._feeds.clone();
        let resolver = exp_resolver(Rc::downgrade(&feeds));
        let bind = |uri: &str| {
            let got = Arc::new(std::sync::Mutex::new(Vec::<Value>::new()));
            let sink = got.clone();
            let emit: Emit = Arc::new(move |v: Value| sink.lock().unwrap().push(v));
            let cancel = resolver(exponential_ui::host::parse_source(uri).unwrap(), emit);
            (got, cancel)
        };
        let (members, _m) = bind("exp:members");
        let (comments, _c) = bind("exp:comments?issue=app-1");
        let user = |id: &str, name: &str, email: Option<&str>, image: Option<&str>| src::SourceUser {
            id: id.into(),
            name: name.into(),
            email: email.map(Into::into),
            image: image.map(Into::into),
        };
        let comment = |id: &str, author: &str, parent: Option<&str>, body: &str, at: &str| src::SourceComment {
            id: id.into(),
            issue_id: "i1".into(),
            author_id: Some(author.into()),
            parent_id: parent.map(Into::into),
            source: "user".into(),
            body: body.into(),
            created_at: at.into(),
        };
        let mut expected = src::SourceStore {
            members: vec![
                src::SourceMember { user_id: "u2".into(), role: "member".into() },
                src::SourceMember { user_id: "u1".into(), role: "owner".into() },
            ],
            users: vec![
                user("u1", "Alex Carter", Some("alex@acme.dev"), Some("https://cdn.example/alex.png")),
                user("u2", "Jonas Weber", Some("jonas@acme.dev"), None),
                user("u3", "Mira Sato", None, None),
            ],
            issues: vec![src::SourceIssue {
                id: "i1".into(),
                board_id: "b1".into(),
                identifier: "APP-1".into(),
                title: "Crash on launch".into(),
                status: "in_progress".into(),
                ..Default::default()
            }],
            comments: vec![
                comment("c1", "u2", None, "Repro on the Pixel 8.", "2026-10-20T10:00:00.000Z"),
                comment("c2", "u1", Some("c1"), "Fix is in #1210.", "2026-10-21T10:00:00.000Z"),
            ],
            ..Default::default()
        };
        let params = |uri: &str| exponential_ui::host::parse_source(uri).unwrap().params;
        let members_now = members.lock().unwrap().last().cloned().unwrap();
        assert_eq!(members_now, src::source_value("members", &params("exp:members"), &expected, None));
        assert_eq!(members_now["count"], 2, "the scope team's members only");
        assert_eq!(members_now["rows"][0]["label"], "Alex Carter");
        let comments_now = comments.lock().unwrap().last().cloned().unwrap();
        assert_eq!(
            comments_now,
            src::source_value("comments", &params("exp:comments?issue=app-1"), &expected, None)
        );
        assert_eq!(comments_now["rows"][1]["createdAt"], "2026-10-21T10:00:00.000Z");
        assert_eq!(comments_now["rows"][0]["authorName"], "Jonas Weber");
        // The richer `exp:issues` row keeps the old four fields (team-scoped).
        let (issues, _i) = bind("exp:issues");
        let issues_now = issues.lock().unwrap().last().cloned().unwrap();
        assert_eq!(issues_now["count"], 1);
        assert_eq!(issues_now["rows"][0]["status"], "in_progress");
        assert_eq!(issues_now["rows"][0]["boardSlug"], "mobile-app");
        assert_eq!(issues_now["rows"][0]["updatedAt"], "2026-10-20T09:00:00.000Z");

        // A new comment re-emits the comments binding, not the members one.
        let (m_before, c_before) = (members.lock().unwrap().len(), comments.lock().unwrap().len());
        cx.update(|cx| {
            let c = sync::Store::global(cx).collections().clone();
            seed(&c.comments, json!([
                {"id": "c3", "issue_id": "i1", "author_id": "u1", "body": "Shipped.", "created_at": "2026-10-22T10:00:00Z"}
            ]), cx);
        });
        cx.run_until_parked();
        expected.comments.push(comment("c3", "u1", None, "Shipped.", "2026-10-22T10:00:00.000Z"));
        assert_eq!(comments.lock().unwrap().len(), c_before + 1);
        assert_eq!(members.lock().unwrap().len(), m_before, "an unchanged value is not re-emitted");
        assert_eq!(
            comments.lock().unwrap().last().cloned().unwrap(),
            src::source_value("comments", &params("exp:comments?issue=APP-1"), &expected, None)
        );
        // `add_window_view`'s leaked context keeps the app alive past gpui's
        // exit-time leak check (`Store::open` holds its handles).
        let _ = cx.add_window_view(|_, _| Blank);
        std::mem::forget(ide);
    }

    struct Holder(Entity<exponential_ui_gpui::view::SurfaceView>);
    impl gpui::Render for Holder {
        fn render(&mut self, _: &mut Window, _: &mut gpui::Context<Self>) -> impl IntoElement {
            div().w(px(600.)).child(self.0.clone())
        }
    }

    /// A row press (a list TEMPLATE instance, `row.0`) reaches
    /// `harness.openDevice` through the host.
    #[gpui::test]
    fn a_row_press_reaches_the_host_function(cx: &mut gpui::TestAppContext) {
        cx.update(gpui_component::init);
        let calls = Rc::new(RefCell::new(Vec::<Value>::new()));
        let c = calls.clone();
        let host = cx.update(|cx| {
            let pkg: Value = serde_json::from_str(DEVICES_PACKAGE_JSON).unwrap();
            let ide = IdeHost::with_packages(Mode::Dark, &[pkg], None, None, cx);
            ide.host.update(cx, |h, _| {
                h.register_function(
                    "harness.openDevice",
                    exponential_ui_gpui::runtime::sync_function(move |args, _, _| {
                        c.borrow_mut().push(args);
                        Ok(Value::Null)
                    }),
                )
            });
            let host = ide.host.clone();
            std::mem::forget(ide);
            host
        });
        host.update(cx, |h, cx| {
            h.receive(&json!({"version": "v0.9", "applyTemplate": {"surfaceId": "devices", "templateId": "devices"}}), cx);
        });
        cx.run_until_parked();
        let view = host.read_with(cx, |h, _| h.surface("devices")).expect("the surface");
        let row = device(json!({"id": "d1", "label": "Mac", "kind": "desktop"}));
        view.update(cx, |v, cx| v.set_data("/devices", Some(devices_value([&row], NOW)), cx)).unwrap();
        let v2 = view.clone();
        let (_, vcx) = cx.add_window_view(move |_, _| Holder(v2));
        vcx.update(|window, cx| window.draw(cx).clear(cx));
        vcx.run_until_parked();
        vcx.update(|window, cx| window.draw(cx).clear(cx));
        vcx.run_until_parked();
        let pressable: Vec<(u32, String)> = view.read_with(vcx, |v, _| {
            v.placed_nodes().iter().filter(|n| n.pressable).map(|n| (n.index, n.id.clone())).collect()
        });
        assert!(!pressable.is_empty(), "a pressable row");
        let index = pressable[0].0;
        vcx.update(|window, cx| view.update(cx, |v, cx| v.press(index, window, cx)));
        vcx.run_until_parked();
        assert_eq!(*calls.borrow(), vec![json!({"id": "d1", "label": "Mac"})], "{pressable:?}");
    }

    #[test]
    fn mcp_results_decode_json_and_sse() {
        let json_body = r#"{"jsonrpc":"2.0","id":1,"result":{"content":[{"type":"text","text":"ok"}]}}"#;
        assert_eq!(mcp_result(json_body, 1).unwrap()["content"][0]["text"], "ok");
        let sse = "event: message\ndata: {\"jsonrpc\":\"2.0\",\"id\":1,\"result\":{\"x\":1}}\n\n";
        assert_eq!(mcp_result(sse, 1).unwrap(), json!({"x": 1}));
        assert_eq!(mcp_result(json_body, 2).unwrap_err(), "MCP: no answer");
    }

    #[test]
    fn mcp_results_read_only_the_answer_with_our_id() {
        // A notification, a foreign id, then ours (multi-line data, CRLF).
        let sse = concat!(
            ": keep-alive\n\n",
            "event: message\ndata: {\"jsonrpc\":\"2.0\",\"method\":\"notifications/progress\",\"params\":{}}\n\n",
            "event: message\ndata: {\"jsonrpc\":\"2.0\",\"id\":9,\"result\":{\"x\":\"foreign\"}}\n\n",
            "event: message\r\ndata: {\"jsonrpc\":\"2.0\",\"id\":7,\r\ndata: \"result\":{\"x\":\"ours\"}}\r\n\r\n",
        );
        assert_eq!(mcp_result(sse, 7).unwrap(), json!({"x": "ours"}));
        assert_eq!(mcp_result(sse, 3).unwrap_err(), "MCP: no answer");
        // A batch body answers by id too.
        let batch = r#"[{"jsonrpc":"2.0","id":2,"result":{}},{"jsonrpc":"2.0","id":1,"result":{"y":1}}]"#;
        assert_eq!(mcp_result(batch, 1).unwrap(), json!({"y": 1}));
        // An error reply rejects with its message and code.
        let error = "data: {\"jsonrpc\":\"2.0\",\"id\":1,\"error\":{\"code\":-32602,\"message\":\"Unknown tool\"}}\n\n";
        assert_eq!(mcp_result(error, 1).unwrap_err(), "Unknown tool (code -32602)");
        assert_eq!(
            mcp_result(r#"{"jsonrpc":"2.0","id":1,"error":{"message":""}}"#, 1).unwrap_err(),
            "MCP error"
        );
    }

    #[test]
    fn only_a_private_instance_origin_is_opted_in() {
        assert_eq!(own_private_hosts("http://localhost:3000"), vec!["localhost".to_string()]);
        assert_eq!(own_private_hosts("http://127.0.0.1:3000/"), vec!["127.0.0.1".to_string()]);
        assert!(own_private_hosts("https://app.exponential.at").is_empty());
        assert!(own_private_hosts("not a url").is_empty());
    }

    #[test]
    fn consent_names_the_tool() {
        let call = FunctionCallInfo {
            surface_id: "s".into(),
            component_id: "c".into(),
            name: "harness.mcp".into(),
            args: json!({"tool": "exponential_issues_list"}),
        };
        let prompt = consent_prompt(&call);
        assert_eq!(prompt, domain::prompts::exponential_ui_consent("exponential_issues_list"));
        // VAPP-107: the arguments as compact JSON, a destructive op picks the destructive entry.
        let get = FunctionCallInfo { surface_id: "s".into(), component_id: "b".into(), name: "harness.mcp".into(), args: json!({"tool": "exponential_issues", "arguments": {"op": "get", "id": "EXP-12"}}) };
        let words = consent_words(&get);
        assert_eq!((words.tool.as_str(), words.args.as_deref(), words.destructive), ("exponential_issues", Some(r#"{"op":"get","id":"EXP-12"}"#), false));
        let delete = FunctionCallInfo { args: json!({"tool": "exponential_issues", "arguments": {"op": "delete", "id": "EXP-12"}}), ..get.clone() };
        assert!(consent_words(&delete).destructive);
        assert_eq!(consent_prompt(&delete), domain::prompts::exponential_ui_consent_destructive("exponential_issues"));
        let none = FunctionCallInfo { args: json!({"tool": "exponential_devices"}), ..get.clone() };
        assert_eq!(consent_words(&none).args, None);
        let own = FunctionCallInfo { name: "harness.openIssue".into(), args: json!({"identifier": "EXP-1"}), ..get.clone() };
        assert_eq!(consent_words(&own).args.as_deref(), Some(r#"{"identifier":"EXP-1"}"#));
        let long = "x".repeat(400);
        assert_eq!(consent_args_preview(&long), format!("{}…", "x".repeat(160)));
        assert_eq!(consent_args_preview("short"), "short");
        assert_eq!(prompt.title, "Allow this surface to run exponential_issues_list?");
        assert_eq!(prompt.body.as_deref(), Some("It acts as you, with your access to this team."));
        // Return denies: Deny = cancel + focused, Allow = a plain default.
        assert_eq!(prompt.focused().label, "Deny");
        assert_eq!(prompt.focused().role, domain::prompts::Role::Cancel);
        assert_eq!(prompt.action("allow").role, domain::prompts::Role::Default);
        let named = FunctionCallInfo { name: "harness.openIssue".into(), args: json!({}), ..call };
        assert_eq!(consent_prompt(&named).title, "Allow this surface to run harness.openIssue?");
    }

    #[test]
    fn typed_actions_call_their_tool_and_ask_in_their_own_words() {
        // VAPP-120: `{op, …args}`, op first; the verb tool takes the args alone.
        assert_eq!(
            action_arguments(Some("update_status"), &json!({"id": "APP-5", "status": "done"})).to_string(),
            r#"{"op":"update_status","id":"APP-5","status":"done"}"#
        );
        // A forged `op` in the args never changes the operation (review of #1222).
        assert_eq!(
            action_arguments(Some("get"), &json!({"op": "delete", "id": "APP-5"})).to_string(),
            r#"{"op":"get","id":"APP-5"}"#
        );
        // The verb tool keeps the app's keys as given (no op to protect).
        assert_eq!(action_arguments(None, &json!({"op": "x", "text": "hi"})).to_string(), r#"{"op":"x","text":"hi"}"#);
        assert_eq!(action_arguments(None, &json!({"text": "hi"})).to_string(), r#"{"text":"hi"}"#);
        assert_eq!(action_arguments(Some("list"), &Value::Null).to_string(), r#"{"op":"list"}"#);
        // A grant lets a typed action run without asking; harness.mcp never.
        let granted = vec!["exponential.issues.update_status".to_string(), "harness.mcp".to_string()];
        let none = Value::Null;
        assert!(call_granted("exponential.issues.update_status", &none, &granted));
        assert!(!call_granted("exponential.issues.delete", &none, &granted));
        assert!(!call_granted("harness.mcp", &none, &granted));
        assert!(!call_granted("exponential.issues.update_status", &none, &[]));
        // A granted send never stops a session without the window (stop-run).
        let send = vec!["exponential.sessions.send".to_string()];
        assert!(call_granted("exponential.sessions.send", &json!({"text": "hi"}), &send));
        assert!(!call_granted("exponential.sessions.send", &json!({"sessionId": "s1", "stop": true}), &send));
        // The card's words: the action's title, its args, its destructive flag.
        let call = FunctionCallInfo {
            surface_id: "s".into(),
            component_id: "c".into(),
            name: "exponential.issues.update_status".into(),
            args: json!({"id": "APP-5", "status": "done"}),
        };
        let words = consent_words(&call);
        assert_eq!(words.tool, "exponential_issues");
        assert_eq!(words.args.as_deref(), Some(r#"{"id":"APP-5","status":"done"}"#));
        let prompt = consent_prompt(&call);
        assert_eq!(prompt.title, "Allow this surface to set issue status APP-5?");
        assert_eq!(prompt.action("allow").role, domain::prompts::Role::Default);
        let delete = FunctionCallInfo { name: "exponential.issues.delete".into(), args: json!({"id": "APP-5"}), ..call.clone() };
        let prompt = consent_prompt(&delete);
        assert_eq!(prompt.title, "Allow this surface to delete issue APP-5?");
        assert_eq!(prompt.action("allow").role, domain::prompts::Role::Destructive);
        assert_eq!(prompt.focused().label, "Deny");
        let list = FunctionCallInfo { name: "exponential.boards.list".into(), args: json!({}), ..call };
        assert_eq!(consent_words(&list).args, None);
        assert_eq!(consent_prompt(&list).title, "Allow this surface to list boards?");
    }

    #[test]
    fn the_package_and_extension_parse_and_name_the_painted_natives() {
        let pkg: Value = serde_json::from_str(DEVICES_PACKAGE_JSON).unwrap();
        assert_eq!(pkg["id"], "exponential.devices");
        let ext = parse_extension(APP_EXTENSION_JSON).expect("the app extension parses");
        assert_eq!(ext.id, pkg["catalogId"].as_str().unwrap());
        assert_eq!(disc_color_key(Some("danger")), "destructive");
        assert_eq!(disc_color_key(None), "primary");
        for name in ["nav-devices", "monitor", "server"] {
            assert!(icon_path(name).is_some(), "{name} resolves");
        }
    }
}
