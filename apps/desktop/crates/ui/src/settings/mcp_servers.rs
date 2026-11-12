//! Settings → MCP › Servers (EXP-792, desktop half EXP-807; VAPP-185 folded
//! the page into [`super::mcp`], the merged MCP page).
//!
//! Web parity: `components/team/mcp-servers-section.tsx`. The team's MCP
//! server registry lives in Exponential, and so do the credentials: each
//! member connects ONCE (an OAuth sign-in the instance runs, or a typed API
//! key it stores encrypted) and every device, remote start and trigger
//! uses it. This machine holds none and runs no OAuth; the launcher asks the
//! server for a run's values at spawn (`coding::mcp_servers`).
//!
//! One list: per server the person's own connection and ONE action —
//! Connect (the web settings page's connect in the signed-in system browser,
//! then a 2 s poll of `mcpServers.list` until it reads connected), Set key, or a
//! read-only "Connected" caption. EXP-1285: a row has ONE "…" menu — the
//! person's connection items (Test / Replace key / Share with team (FEED-73) /
//! Disconnect) and, for an owner, Edit / Remove. Owners add, edit and remove
//! servers through [`super::mcp_server_dialog`] (EXP-810).
//!
//! `mcp_servers` is server-only (never an Electric shape), so this is a
//! fetch-on-open tRPC read like the widget pane's.
//!
//! Owner 2026-11-12 (fixtures/settings-mcp.json `serverOrder`): the pane
//! is THE Servers group of Settings › MCP: the built-ins first (Exponential,
//! then Team apps when an app exposes tools; a Copy each, never a ⋯), then
//! the team's servers. Every row is a disclosure (chevron leading, closed by
//! default, several may be open) opening its tools under it: Exponential =
//! the filter + one sub-label per contract tool group, Team apps = one
//! sub-label per app, a team server = `mcpServers.tools` on first open
//! (`toolsStateCases`).
//!
//! #EXP-1629: the pane is a document on the native binding (projection at
//! `/pane`): the band with the owner's Add server pill and the reload
//! spinner, the skeleton, the failed and empty states, and the rows (glyph,
//! name + Default pill over the caption, the one connection control) and each
//! row's ⋯ (a genUI `Menu`, [`menu_items`]); the dialogs stay native, the
//! Set key / Disconnect / Remove alerts' bodies documents ([`KeyContent`],
//! [`ServerName`]).

use crate::toast::Toast;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::time::{Duration, Instant};

use gpui::{div, App, AppContext as _, Entity, IntoElement, ParentElement, Render, SharedString, Styled, Subscription, Window};
use gpui_component::{button::ButtonVariant, ActiveTheme as _};
use serde_json::{json, Value};

use api::app_access::AccessApp;
use api::mcp_servers::{McpServerConfig, McpServerListEntry, McpToolRow};
use eui_gpui::binding::Binding;
use eui_gpui::host::HostPlugin;
use eui_gpui::view::SurfaceView;
use sync::Store;

use crate::native_dialog::{open_alert, AlertSpec};
use crate::navigation::{active_team_id, Navigation};
use crate::queries;

use super::doc::{empty_state, eq, failed_state, hex, mount_with, not, path, IdeToasts, Ink, PillPaint, SM, XS};
use super::mcp::{builtin_endpoint, mcp_server_order, mcp_tools_state, tools_failed_line, Chrome, McpToolsState, BUILTIN_APPS, BUILTIN_EXPONENTIAL, SETTINGS_MCP};
use super::open_url;

/// How often a Connect re-reads the list while the browser is out, and for
/// how long before it gives up.
const CONNECT_POLL: Duration = Duration::from_secs(2);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5 * 60);

/// The row's second line — web `mcpServerTarget`: an HTTP server's HOST,
/// a command server's command line. Nothing else (no member counts, no
/// transport/auth qualifiers): the row reads name · Default · host.
fn row_detail(entry: &McpServerListEntry) -> String {
    let config = &entry.config;
    if config.is_http() {
        url_host(config.url.as_deref().unwrap_or_default())
    } else {
        let mut parts = vec![config.command.clone().unwrap_or_default()];
        parts.extend(config.args.iter().cloned());
        parts
            .into_iter()
            .filter(|part| !part.is_empty())
            .collect::<Vec<_>>()
            .join(" ")
    }
}

/// VAPP-185 (web `ServerRow`): [`row_detail`] then ` · Used by <apps>`
/// when an app's package declares the server in `upstreams`.
fn row_caption(entry: &McpServerListEntry) -> String {
    let mut detail = row_detail(entry);
    // VAPP-239: host · Runs on <device> · Used by …
    if let Some(runs_on) = super::mcp::runs_on_caption(entry.device.as_ref().map(|d| d.label.as_str())) {
        detail = if detail.is_empty() { runs_on } else { format!("{detail} · {runs_on}") };
    }
    match super::mcp::used_by_caption(&entry.used_by) {
        Some(used_by) if detail.is_empty() => used_by,
        Some(used_by) => format!("{detail} · {used_by}"),
        None => detail,
    }
}

/// Web `new URL(url).host` (port included, userinfo dropped); a string with
/// no scheme echoes back verbatim, like the web's catch.
fn url_host(url: &str) -> String {
    let url = url.trim();
    let Some((_, rest)) = url.split_once("://") else {
        return url.to_string();
    };
    let authority = rest.split(['/', '?', '#']).next().unwrap_or_default();
    let host = authority.rsplit_once('@').map_or(authority, |(_, host)| host);
    if host.is_empty() {
        url.to_string()
    } else {
        host.to_ascii_lowercase()
    }
}

/// EXP-1285 — whether a row carries the "…" menu at all (web `ServerRow`):
/// an owner always (Edit / Remove), a member only on a connected row (its
/// connection items).
fn row_menu_offered(owner: bool, connected: bool) -> bool {
    owner || connected
}

/// FEED-73 — the row menu's share item (web `ServerRow`), by whether the
/// viewer's OWN connection is shared.
fn share_menu_label(shared: bool) -> &'static str {
    if shared {
        "Stop sharing"
    } else {
        "Share with team"
    }
}

/// FEED-73: whether the row menu carries a share item at all (web
/// `ServerRow`: `http || shared`). A stdio server's secret stays on
/// this machine and the server refuses the share, so the item only shows
/// on such a row to undo a share made before that refusal existed.
fn share_menu_offered(http: bool, shared: bool) -> bool {
    http || shared
}

/// FEED-73 — the share toggle's toasts (web `share` + `useMcpServers.
/// setShared`): `Ok` = the success line (it says what sharing means),
/// `Err` = the failure title.
fn share_toast(name: &str, shared: bool) -> (String, &'static str) {
    if shared {
        (
            format!("Shared {name} with the team: action sessions on teammates' machines use your connection"),
            "Could not share",
        )
    } else {
        (format!("Stopped sharing {name}"), "Could not stop sharing")
    }
}

/// What the row's one action is, from the person's own connection (web
/// `ConnectionAction`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RowAction {
    /// `none` auth: nothing to do.
    NotNeeded,
    /// Connected: a read-only caption (the actions live in the row menu).
    Connected,
    /// Expired or a failed refresh: Reconnect (the reason in the tooltip).
    Reconnect,
    /// Not connected: Connect (OAuth) or Set key (secret).
    Connect,
}

fn row_action(entry: &McpServerListEntry) -> RowAction {
    match entry.connection.status.as_str() {
        "not_needed" => RowAction::NotNeeded,
        "connected" => RowAction::Connected,
        "expired" | "error" => RowAction::Reconnect,
        _ => RowAction::Connect,
    }
}

/// The Reconnect tooltip: the server's reason, else the web's fallback.
fn reconnect_reason(entry: &McpServerListEntry) -> String {
    entry
        .connection
        .error
        .clone()
        .filter(|error| !error.trim().is_empty())
        .unwrap_or_else(|| {
            if entry.connection.status == "expired" {
                "Your sign-in expired.".to_string()
            } else {
                "The last refresh failed.".to_string()
            }
        })
}

/// The `test` result as a notification line (web `test` toasts).
fn test_message(name: &str, result: &api::mcp_servers::McpTestResult) -> String {
    match (result.ok, result.tools) {
        (true, None) => format!("{name} answered"),
        (true, Some(1)) => format!("{name} answered with 1 tool"),
        (true, Some(tools)) => format!("{name} answered with {tools} tools"),
        (false, _) => match result.error.as_deref().map(str::trim).filter(|e| !e.is_empty()) {
            Some(error) => format!("{name} did not answer: {error}"),
            None => format!("{name} did not answer"),
        },
    }
}

/// An open row's tools state (`toolsStateCases`; a row with no transport
/// reads http, the API's default).
fn tools_state(entry: &McpServerListEntry) -> McpToolsState {
    mcp_tools_state(if entry.config.is_http() { "http" } else { "stdio" }, &entry.connection.status)
}

/// The data-model path the pane's projection is written at.
const PATH: &str = "/pane";

enum Load {
    Idle,
    Loading,
    Ready(Result<Vec<McpServerListEntry>, String>),
}

/// An open team server's tools: `None` = loading (skeleton rows),
/// `Some(Err)` = the `toolsFailed` line with Retry.
type ToolsLoad = Option<Result<Vec<McpToolRow>, String>>;

/// A built-in row's open key (`serverOrder`'s `builtin:<id>`).
fn builtin_key(id: &str) -> String {
    format!("builtin:{id}")
}

/// The filter Input's text field (its focus decides the box's stroke).
const FILTER_FIELD: &str = "filter.field";

pub struct McpServersPane {
    nav: Entity<Navigation>,
    load: Load,
    /// The team the loaded list belongs to — a team switch must refetch.
    team_id: Option<String>,
    /// Monotonic guard: a stale in-flight fetch must not clobber a newer one.
    generation: u64,
    /// An owner write (remove) in flight.
    busy: bool,
    /// The row whose connection action is in flight (test / disconnect /
    /// set key) — its controls disable, the others stay live.
    pending: Option<String>,
    /// The row a Connect is waiting on (the browser is out; the list is
    /// polled until it reads connected), and that wait's own generation so a
    /// Cancel or a second Connect retires the old poll.
    connecting: Option<String>,
    connect_generation: u64,
    /// EXP-1274: a refetch of a list that is already showing. The rows stay
    /// and the band spins ([`crate::surface::glass_section_header_loading`]);
    /// only the FIRST load draws the skeleton. `reload_requested` asks the
    /// next render to start it, `reloading` is the fetch in flight.
    reload_requested: bool,
    reloading: bool,
    /// FEED-79: the load in flight is the person's Retry press (its failure
    /// toasts); a load on entry or after a write fails quietly.
    retry_pressed: bool,
    #[cfg(test)]
    shot: Option<McpShot>,
    /// What each row's ⋯ acts on, by server id (refilled by every
    /// projection).
    menus: RefCell<HashMap<String, MenuRow>>,
    /// The team's apps exposing tools (`appAccess.list`, set by the MCP
    /// page): the Team apps row shows only when there is one.
    apps: Vec<AccessApp>,
    /// The open rows: `builtin:<id>` or a server id.
    open: HashSet<String>,
    /// EXP-1274: the built-in tools' inline filter, as typed, and whether
    /// its field has focus.
    filter: String,
    filter_focused: bool,
    /// The team servers' listed tools, by server id, with the fetch's
    /// generation (a Retry retires an older answer).
    tools: HashMap<String, (u64, ToolsLoad)>,
    tools_generation: u64,
    binding: Entity<Binding<Value>>,
    view: Entity<SurfaceView>,
    _subscriptions: Vec<Subscription>,
}

/// A pinned state for the headless bench (the headless store has no team).
#[cfg(test)]
#[derive(Default)]
pub(crate) struct McpShot {
    pub team: Option<String>,
    pub owner: bool,
    /// The instance the built-in rows' URLs start with.
    pub instance: String,
    pub filter: String,
    /// The open rows (`builtin:<id>` or a server id).
    pub open: Vec<String>,
    /// Team servers' tools by id.
    pub tools: Vec<(String, ToolsLoad)>,
}

impl McpServersPane {
    pub fn new(
        nav: Entity<Navigation>,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> Self {
        let subscriptions = vec![cx.observe(&nav, |_, _, cx| cx.notify())];
        // The actions leave the binding for the pane: Add server and Connect
        // open dialogs or the browser, Retry and Cancel restart the reads.
        let pane = cx.weak_entity();
        let handle = window.window_handle();
        let binding = cx.new(|_| {
            let mut binding = Binding::new(PATH, Value::Null, |state: &Value, _| state.clone());
            binding.set_toasts(Rc::new(IdeToasts));
            for name in ["add", "retry", "connect", "cancel", "menu", "toggle", "copy", "toolsRetry", "filter", "filterFocus", "filterBlur", "clear", "focusFilter"] {
                let pane = pane.clone();
                binding.on_action(name, move |_, event, cx| {
                    let key = event.context.get("key").and_then(Value::as_str).unwrap_or_default().to_string();
                    let value = event.payload.as_ref().and_then(|payload| payload.get("value")).and_then(Value::as_str).unwrap_or_default().to_string();
                    let pane = pane.clone();
                    // Deferred: the binding is mid-update and these write it.
                    cx.defer(move |cx| {
                        let _ = handle.update(cx, |_, window, cx| {
                            let _ = pane.update(cx, |pane, cx| pane.action(name, &key, &value, window, cx));
                        });
                    });
                    None
                });
            }
            binding
        });
        let tints = Tints { success: hex(cx.theme().success), warning: hex(cx.theme().warning), skeleton: hex(cx.theme().muted_foreground.opacity(0.18)), list_hover: hex(cx.theme().list_hover) };
        let plugin = Rc::new(McpHost { ide: super::doc::ide_plugin() });
        let view = mount_with("settings-mcp-servers", document(&Ink::of(cx), &tints, &Chrome::of(cx)), &binding, plugin, false, window, cx);
        Self {
            nav,
            load: Load::Idle,
            team_id: None,
            generation: 0,
            busy: false,
            pending: None,
            connecting: None,
            connect_generation: 0,
            reload_requested: false,
            reloading: false,
            retry_pressed: false,
            #[cfg(test)]
            shot: None,
            menus: RefCell::default(),
            apps: Vec::new(),
            open: HashSet::new(),
            filter: String::new(),
            filter_focused: false,
            tools: HashMap::new(),
            tools_generation: 0,
            binding,
            view,
            _subscriptions: subscriptions,
        }
    }

    #[cfg(test)]
    pub(crate) fn for_shots(window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        Self::new(crate::navigation::nav_for_window(window, cx), window, cx)
    }

    /// The pane in a pinned state: `load` = `None` loading, `Some(Err)`
    /// failed, `Some(Ok)` the rows; `pending` / `connecting` a row's id.
    #[cfg(test)]
    pub(crate) fn show(&mut self, shot: McpShot, load: Option<Result<Vec<McpServerListEntry>, String>>, reloading: bool, pending: Option<&str>, connecting: Option<&str>, cx: &mut gpui::Context<Self>) {
        self.team_id = shot.team.clone();
        self.filter = shot.filter.clone();
        self.open = shot.open.iter().cloned().collect();
        self.tools = shot.tools.iter().map(|(id, load)| (id.clone(), (0, load.clone()))).collect();
        self.shot = Some(shot);
        self.load = match load {
            None => Load::Loading,
            Some(result) => Load::Ready(result),
        };
        self.reloading = reloading;
        self.pending = pending.map(str::to_string);
        self.connecting = connecting.map(str::to_string);
        cx.notify();
    }

    fn pane_team(&self, cx: &App) -> Option<String> {
        #[cfg(test)]
        if let Some(shot) = &self.shot {
            return shot.team.clone();
        }
        active_team_id(&self.nav, cx)
    }

    /// The instance the built-in rows' URLs start with (the active
    /// account's, as the person reads it).
    fn instance(&self, cx: &App) -> String {
        #[cfg(test)]
        if let Some(shot) = &self.shot {
            return shot.instance.clone();
        }
        queries::active_account(cx).map(|account| queries::display_instance_url(&account.instance_url)).unwrap_or_default()
    }

    /// The MCP page's `appAccess.list` apps (the Team apps row and its
    /// tools); an empty list hides the row.
    pub(crate) fn set_apps(&mut self, apps: Vec<AccessApp>, cx: &mut gpui::Context<Self>) {
        if self.apps != apps {
            self.apps = apps;
            cx.notify();
        }
    }

    /// Open or close a row; a team server opening on `list` fetches its
    /// tools the first time.
    fn toggle(&mut self, key: &str, cx: &mut gpui::Context<Self>) {
        if !self.open.remove(key) {
            self.open.insert(key.to_string());
            let listed = match &self.load {
                Load::Ready(Ok(rows)) => rows.iter().find(|entry| entry.config.id == key).map(|entry| tools_state(entry) == McpToolsState::List),
                _ => None,
            };
            if listed == Some(true) && !self.tools.contains_key(key) {
                self.load_tools(key, cx);
            }
        }
        cx.notify();
    }

    /// `mcpServers.tools {serverId}` for an open row (skeleton meanwhile).
    fn load_tools(&mut self, server_id: &str, cx: &mut gpui::Context<Self>) {
        #[cfg(test)]
        if self.shot.is_some() {
            return;
        }
        let Some(trpc) = queries::trpc_client(cx) else {
            return;
        };
        self.tools_generation += 1;
        let generation = self.tools_generation;
        self.tools.insert(server_id.to_string(), (generation, None));
        let id = server_id.to_string();
        cx.spawn(async move |this, cx| {
            let fetched = {
                let id = id.clone();
                cx.background_executor().spawn(async move { api::mcp_servers::tools(&trpc, &id) }).await
            };
            let result = match fetched {
                Ok(out) if out.ok => Ok(out.tools.unwrap_or_default()),
                Ok(out) => Err(out.error.filter(|error| !error.trim().is_empty()).unwrap_or_else(|| "no answer".to_string())),
                Err(err) => Err(err.user_message()),
            };
            let _ = this.update(cx, |this, cx| {
                if this.tools.get(&id).is_some_and(|(at, _)| *at == generation) {
                    this.tools.insert(id, (generation, Some(result)));
                    cx.notify();
                }
            });
        })
        .detach();
    }

    /// Copy a built-in's URL (the way clients like Claude connect).
    fn copy(&mut self, id: &str, window: &mut Window, cx: &mut gpui::Context<Self>) {
        let url = builtin_endpoint(&self.instance(cx), id);
        cx.write_to_clipboard(gpui::ClipboardItem::new_string(url));
        crate::toast::show(Toast::success(SETTINGS_MCP.copied), window, cx);
    }

    fn pane_owner(&self, team_id: &str, cx: &App) -> bool {
        #[cfg(test)]
        if let Some(shot) = &self.shot {
            return shot.owner;
        }
        super::is_owner(cx, team_id)
    }

    /// Server read (`mcpServers.list` is tRPC — `mcp_servers` never syncs),
    /// so every entry into the section drops the cache: a connection made on
    /// the web or from the CLI has to be here when you come back.
    pub fn mark_stale(&mut self, cx: &mut gpui::Context<Self>) {
        self.refetch(cx);
    }

    /// Refetch on the next render — the dialog calls this after a
    /// create/edit lands (`mcpServers.list` is a server read with no
    /// Electric echo). EXP-1274: a list already on screen KEEPS its rows and
    /// the band spins; anything else (an error, nothing yet) starts over at
    /// the skeleton.
    pub(super) fn refetch(&mut self, cx: &mut gpui::Context<Self>) {
        // A connection may have changed: a failed tools list asks again on
        // its next open.
        self.tools.retain(|_, (_, load)| !matches!(load, Some(Err(_))));
        match self.load {
            Load::Ready(Ok(_)) => self.reload_requested = true,
            Load::Ready(Err(_)) => self.load = Load::Idle,
            Load::Idle | Load::Loading => {}
        }
        cx.notify();
    }

    /// The failed list's Retry: the same load, attended.
    fn retry_load(&mut self, cx: &mut gpui::Context<Self>) {
        self.retry_pressed = true;
        self.refetch(cx);
    }

    fn ensure_loaded(&mut self, team_id: &str, cx: &mut gpui::Context<Self>) {
        if self.team_id.as_deref() != Some(team_id) {
            self.team_id = Some(team_id.to_string());
            self.load = Load::Idle;
            self.connecting = None;
            self.pending = None;
            self.reload_requested = false;
            self.reloading = false;
            self.open.clear();
            self.tools.clear();
            self.filter.clear();
        }
        let reload = self.reload_requested && matches!(self.load, Load::Ready(Ok(_)));
        if !matches!(self.load, Load::Idle) && !reload {
            return;
        }
        self.reload_requested = false;
        let Some(trpc) = queries::trpc_client(cx) else {
            // Nothing to fetch and nothing to wait for — Idle renders the
            // loading line, so leaving it there would spin forever.
            self.load = Load::Ready(Err("Sign in to load this team's MCP servers.".to_string()));
            return;
        };
        let team = team_id.to_string();

        if reload {
            self.reloading = true;
        } else {
            self.load = Load::Loading;
        }
        self.generation += 1;
        let generation = self.generation;

        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    api::mcp_servers::list(&trpc, &team).map_err(|err| err.user_message())
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.generation != generation {
                    return;
                }
                // FEED-79: a failed load = the empty state + Retry; only a
                // Retry press that fails again toasts.
                let attended = std::mem::take(&mut this.retry_pressed);
                if let Err(message) = &result {
                    crate::action_error::failed_load(
                        "the MCP servers",
                        attended,
                        Some(message.clone()),
                        cx,
                    );
                }
                this.load = Load::Ready(result);
                this.reloading = false;
                cx.notify();
            });
        })
        .detach();
    }

    // -- the person's own connection -------------------------------------------

    /// Connect (OAuth): the system browser opens the web settings deep link
    /// (`api::mcp_servers::connect_page_url`), whose page starts the connect
    /// in the SIGNED-IN browser — the instance's callback only accepts a code
    /// when the browser's Exponential session is the member who started the
    /// flow, so this side never calls `mcpServers.connect` itself. It only
    /// POLLS the list ([`CONNECT_POLL`], up to [`CONNECT_TIMEOUT`]) until the
    /// row reads connected. A secret server's "connect" is the key dialog
    /// instead.
    fn connect(
        &mut self,
        config: &McpServerConfig,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        if config.auth == "secret" {
            self.open_key_dialog(config, window, cx);
            return;
        }
        if config.auth != "oauth" {
            return;
        }
        let (Some(trpc), Some(team_id)) = (queries::trpc_client(cx), self.team_id.clone()) else {
            return;
        };
        let handle = window.window_handle();
        let name = config.name.clone();
        let row_team = if config.team_id.is_empty() {
            team_id.clone()
        } else {
            config.team_id.clone()
        };
        let slug = Store::global(cx)
            .collections()
            .teams
            .read(cx)
            .get(&row_team)
            .and_then(|team| team.slug.clone());
        let page = match (slug, queries::active_account(cx)) {
            (Some(slug), Some(account)) => {
                api::mcp_servers::connect_page_url(&account.instance_url, &slug, &config.id)
            }
            _ => None,
        };
        let Some(page) = page else {
            let note = crate::action_error::failed_toast(
                "connect to",
                &name,
                Some("Open Settings → MCP servers on the web and connect it there.".into()),
            );
            crate::toast::show(note, window, cx);
            return;
        };
        let trpc = std::sync::Arc::new(trpc);
        self.connect_generation += 1;
        let generation = self.connect_generation;
        self.connecting = Some(config.id.clone());
        cx.notify();
        let server_id = config.id.clone();
        open_url(cx, page);
        cx.spawn(async move |this, cx| {
            let deadline = Instant::now() + CONNECT_TIMEOUT;
            loop {
                cx.background_executor().timer(CONNECT_POLL).await;
                // A Cancel, a second Connect or a team switch retires this
                // wait (as does the pane going away).
                let live = this
                    .update(cx, |this, _| {
                        this.connect_generation == generation
                            && this.connecting.as_deref() == Some(server_id.as_str())
                    })
                    .unwrap_or(false);
                if !live {
                    return;
                }
                let listed = cx
                    .background_executor()
                    .spawn({
                        let (trpc, team_id) = (trpc.clone(), team_id.clone());
                        async move { api::mcp_servers::list(&trpc, &team_id) }
                    })
                    .await;
                let connected = match listed {
                    Ok(servers) => {
                        let connected = servers.iter().any(|entry| {
                            entry.config.id == server_id && entry.connection.status == "connected"
                        });
                        let _ = this.update(cx, |this, cx| {
                            if this.team_id.as_deref() == Some(team_id.as_str()) {
                                this.load = Load::Ready(Ok(servers));
                            }
                            if connected {
                                this.connecting = None;
                                let note = Toast::success(format!(
                                    "Connected to {name}"
                                ));
                                let _ = handle
                                    .update(cx, |_, window, cx| crate::toast::show(note, window, cx));
                            }
                            cx.notify();
                        });
                        connected
                    }
                    Err(err) => {
                        log::debug!("[ui] mcpServers.list while connecting {name}: {err}");
                        false
                    }
                };
                if connected {
                    return;
                }
                if Instant::now() >= deadline {
                    let _ = this.update(cx, |this, cx| {
                        if this.connect_generation == generation {
                            this.connecting = None;
                        }
                        let note = crate::action_error::failed_toast(
                            "connect to",
                            &name,
                            Some("Finish the sign-in in your browser, or Connect again.".into()),
                        );
                        let _ = handle.update(cx, |_, window, cx| crate::toast::show(note, window, cx));
                        cx.notify();
                    });
                    return;
                }
            }
        })
        .detach();
    }

    /// Stop waiting on a Connect (the browser tab may still finish it; the
    /// next refresh would show that).
    fn cancel_connect(&mut self, cx: &mut gpui::Context<Self>) {
        self.connecting = None;
        cx.notify();
    }

    /// Set key: the person's OWN API key for an `auth: secret` server, sent
    /// once to `mcpServers.setSecret` (stored encrypted, never shown again —
    /// web `SecretDialog`).
    // dialog stays native: the native alert; its body is a document ([`KeyContent`]).
    fn open_key_dialog(
        &mut self,
        config: &McpServerConfig,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let pane = cx.entity().downgrade();
        let handle = window.window_handle();
        // #EXP-1793: one masked field per declared header/env name.
        let names: Vec<String> = config.secret_names().to_vec();
        let fields = if names.is_empty() { vec!["Key".to_string()] } else { names.clone() };
        // The alert is a layer in this window: the body mounts on it.
        let content = cx.new(|cx| KeyContent::new(&fields, &[], window, cx));
        let ok_content = content.clone();
        let focus_view = content.clone();
        let server_id = config.id.clone();
        let server = config.name.clone();
        let spec = AlertSpec::new(
            format!("API key for {server}"),
            "Only your sessions use it. Stored encrypted; nobody can read it back.",
            "Save key",
        )
        .content(move |_, _| content.clone().into_any_element())
        .on_ok(move |_, cx| {
            let typed: Vec<String> = ok_content.read(cx).typed(cx).into_iter().map(|value| value.trim().to_string()).collect();
            if typed.iter().any(String::is_empty) {
                return false;
            }
            // `{values}` keyed by name (the server keeps them so); a server
            // that declares none takes the lone `{value}`.
            let values: Option<std::collections::BTreeMap<String, String>> =
                (!names.is_empty()).then(|| names.iter().cloned().zip(typed.iter().cloned()).collect());
            let value = typed.first().cloned().unwrap_or_default();
            let (Some(trpc), Some(pane)) = (queries::trpc_client(cx), pane.upgrade()) else {
                return true;
            };
            pane.update(cx, |this, cx| {
                this.pending = Some(server_id.clone());
                cx.notify();
            });
            let server_id = server_id.clone();
            let server = server.clone();
            cx.spawn(async move |cx| {
                let result = cx
                    .background_executor()
                    .spawn(async move {
                        match values {
                            Some(values) => api::mcp_servers::set_secret_values(&trpc, &server_id, &values),
                            None => api::mcp_servers::set_secret(&trpc, &server_id, &value),
                        }
                    })
                    .await;
                let _ = pane.update(cx, |this, cx| {
                    this.pending = None;
                    let note = match result {
                        Ok(()) => {
                            this.refetch(cx);
                            Toast::success(format!(
                                "Key saved for {server}"
                            ))
                        }
                        Err(err) => crate::action_error::failed_toast(
                            "save",
                            "the key",
                            crate::action_error::action_error_description(&err),
                        ),
                    };
                    let _ = handle.update(cx, |_, window, cx| crate::toast::show(note, window, cx));
                    cx.notify();
                });
            })
            .detach();
            true
        });
        open_alert(window, cx, spec);
        // Web dialogs focus their first field; Enter in it confirms.
        let view = focus_view.read(cx).view.clone();
        super::doc::focus_input(view, "value", window, cx);
    }

    /// Test connection (http only): an MCP `initialize` + `tools/list` the
    /// SERVER runs with the person's credential.
    fn test(&mut self, config: &McpServerConfig, window: &mut Window, cx: &mut gpui::Context<Self>) {
        let Some(trpc) = queries::trpc_client(cx) else {
            return;
        };
        self.pending = Some(config.id.clone());
        cx.notify();
        let handle = window.window_handle();
        let server_id = config.id.clone();
        let name = config.name.clone();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { api::mcp_servers::test(&trpc, &server_id) })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.pending = None;
                let note = match result {
                    Ok(result) if result.ok => {
                        Toast::success(test_message(&name, &result))
                    }
                    Ok(result) => Toast::error(test_message(&name, &result)),
                    Err(err) => Toast::error(format!(
                        "{name} did not answer: {}",
                        err.user_message()
                    )),
                };
                let _ = handle.update(cx, |_, window, cx| crate::toast::show(note, window, cx));
                cx.notify();
            });
        })
        .detach();
    }

    /// Disconnect: delete the person's credential for the server (the row
    /// stays on the team; runs just stop getting its tools).
    // dialog stays native: the shared native confirm alert; its body is a document ([`ServerName`]).
    fn confirm_disconnect(
        &mut self,
        config: &McpServerConfig,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let pane = cx.entity().downgrade();
        let handle = window.window_handle();
        let server_id = config.id.clone();
        let name = config.name.clone();
        let identity = cx.new(|cx| ServerName::new(&name, window, cx));
        let spec = AlertSpec::new(
            "Disconnect",
            "Your credential for this server is deleted. Your sessions stop getting its \
             tools until you connect again; other members keep theirs.",
            "Disconnect",
        )
        .content(move |_, _| identity.clone().into_any_element())
        .ok_variant(ButtonVariant::Danger)
        .on_ok(move |_, cx| {
            let (Some(trpc), Some(pane)) = (queries::trpc_client(cx), pane.upgrade()) else {
                return true;
            };
            pane.update(cx, |this, cx| {
                this.pending = Some(server_id.clone());
                cx.notify();
            });
            let server_id = server_id.clone();
            let name = name.clone();
            cx.spawn(async move |cx| {
                let result = cx
                    .background_executor()
                    .spawn(async move { api::mcp_servers::disconnect(&trpc, &server_id) })
                    .await;
                let _ = pane.update(cx, |this, cx| {
                    this.pending = None;
                    let note = match result {
                        Ok(()) => {
                            this.refetch(cx);
                            Toast::success(format!(
                                "Disconnected from {name}"
                            ))
                        }
                        Err(err) => crate::action_error::failed_toast(
                            "disconnect from",
                            &name,
                            crate::action_error::action_error_description(&err),
                        ),
                    };
                    let _ = handle.update(cx, |_, window, cx| crate::toast::show(note, window, cx));
                    cx.notify();
                });
            })
            .detach();
            true
        });
        open_alert(window, cx, spec);
    }

    /// FEED-73: Share with team / Stop sharing — the person's OWN connection
    /// offered to (or withdrawn from) the team's action runs, like a shared
    /// device. The list refetches either way (web `useMcpServers.setShared`).
    fn toggle_shared(
        &mut self,
        config: &McpServerConfig,
        shared: bool,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(trpc) = queries::trpc_client(cx) else {
            return;
        };
        self.pending = Some(config.id.clone());
        cx.notify();
        let handle = window.window_handle();
        let server_id = config.id.clone();
        let (success, failure) = share_toast(&config.name, shared);
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { api::mcp_servers::set_shared(&trpc, &server_id, shared) })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.pending = None;
                let note = match result {
                    Ok(_) => Toast::success(success),
                    Err(err) => Toast::error(failure).description(err.user_message()),
                };
                this.refetch(cx);
                let _ = handle.update(cx, |_, window, cx| crate::toast::show(note, window, cx));
            });
        })
        .detach();
    }

    // -- owner writes -----------------------------------------------------------

    /// Owner-only `mcpServers.remove`. The row goes for the whole TEAM, and
    /// with it every member's connection — so the confirm says so.
    // dialog stays native: the shared native confirm alert; its body is a document ([`ServerName`]).
    fn confirm_remove(
        &mut self,
        config: &McpServerConfig,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let pane = cx.entity().downgrade();
        let handle = window.window_handle();
        let server_id = config.id.clone();
        let name = config.name.clone();
        let identity = cx.new(|cx| ServerName::new(&name, window, cx));
        let spec = AlertSpec::new(
            "Remove server",
            "Sessions stop offering it and every member's connection to it is deleted. \
             This cannot be undone.",
            "Remove",
        )
        .content(move |_, _| identity.clone().into_any_element())
        .ok_variant(ButtonVariant::Danger)
        .on_ok(move |_, cx| {
            let (Some(trpc), Some(pane)) = (queries::trpc_client(cx), pane.upgrade()) else {
                return true;
            };
            pane.update(cx, |this, cx| {
                this.busy = true;
                cx.notify();
            });
            let server_id = server_id.clone();
            cx.spawn(async move |cx| {
                let result = cx
                    .background_executor()
                    .spawn(async move { api::mcp_servers::remove(&trpc, &server_id) })
                    .await;
                let _ = pane.update(cx, |this, cx| {
                    this.busy = false;
                    match result {
                        Ok(()) => this.refetch(cx),
                        Err(err) => {
                            let note = crate::action_error::failed_toast(
                                "remove",
                                "the server",
                                crate::action_error::action_error_description(&err),
                            );
                            let _ = handle.update(cx, |_, window, cx| {
                                crate::toast::show(note, window, cx);
                            });
                        }
                    }
                    cx.notify();
                });
            })
            .detach();
            true
        });
        open_alert(window, cx, spec);
    }

    /// Open the add/edit form (EXP-810). `config` = the row being edited;
    /// `None` adds one. `team_id` comes from the caller so an edit and an add
    /// name the same team even mid team switch.
    // dialog: `mcp_server_dialog` is its own native window, its content a document.
    fn open_editor(
        &mut self,
        config: Option<McpServerConfig>,
        device: Option<api::mcp_servers::McpServerDevice>,
        team_id: &str,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let team_id = match config.as_ref() {
            // A row knows its own team; the header button has only the pane's.
            Some(row) if !row.team_id.is_empty() => row.team_id.clone(),
            _ => team_id.to_string(),
        };
        let pane = cx.entity().downgrade();
        super::mcp_server_dialog::open(window, cx, team_id, config, device, pane, None);
    }

    /// #VAPP-243: the add form pre-filled with an app's upstream name (an
    /// owner's Add server on the app's Accounts row).
    fn open_add_named(&mut self, name: String, team_id: &str, window: &mut Window, cx: &mut gpui::Context<Self>) {
        let pane = cx.entity().downgrade();
        super::mcp_server_dialog::open(window, cx, team_id.to_string(), None, None, pane, Some(name));
    }

    // -- the document ---------------------------------------------------------

    /// Write the projection now (the filter's Clear, before the field takes
    /// focus again).
    fn push(&mut self, cx: &mut gpui::Context<Self>) {
        let team = self.pane_team(cx);
        let owner = team.as_deref().is_some_and(|team| self.pane_owner(team, cx));
        let projection = self.project(team.as_deref(), owner, &self.instance(cx));
        self.binding.update(cx, |binding, cx| binding.update_state(|state| *state = projection, cx));
    }

    /// A document action (`add`, `retry`, `connect` / `cancel` on the row
    /// `key`, `menu` = the row's ⋯ pick `value`).
    fn action(&mut self, name: &str, key: &str, value: &str, window: &mut Window, cx: &mut gpui::Context<Self>) {
        match name {
            "add" if !self.busy => {
                if let Some(team) = self.pane_team(cx) {
                    self.open_editor(None, None, &team, window, cx);
                }
            }
            "retry" => self.retry_load(cx),
            "cancel" => self.cancel_connect(cx),
            "toggle" => self.toggle(key, cx),
            "copy" => self.copy(key, window, cx),
            "toolsRetry" => {
                self.load_tools(key, cx);
                cx.notify();
            }
            "filter" => {
                self.filter = value.to_string();
                cx.notify();
            }
            "filterFocus" | "filterBlur" => {
                self.filter_focused = name == "filterFocus";
                cx.notify();
            }
            "clear" => {
                // A focused field keeps its own text over a later write.
                self.view.update(cx, |view, cx| view.blur_field(FILTER_FIELD, window, cx));
                self.filter.clear();
                self.push(cx);
                cx.notify();
                super::doc::focus_input(self.view.clone(), "filter", window, cx);
            }
            "focusFilter" => super::doc::focus_input(self.view.clone(), "filter", window, cx),
            "connect" => {
                let Load::Ready(Ok(rows)) = &self.load else { return };
                let Some(config) = rows.iter().find(|entry| entry.config.id == key).map(|entry| entry.config.clone()) else { return };
                if self.pending.as_deref() != Some(key) {
                    self.connect(&config, window, cx);
                }
            }
            "menu" => {
                let Some(row) = self.menus.borrow().get(key).cloned() else { return };
                // The trigger is inert while busy or pending.
                if row.busy || row.pending {
                    return;
                }
                let target = row.entry.config.clone();
                let (connected, owner) = (row.connected, row.owner);
                match value {
                    "test" if connected && target.is_http() => self.test(&target, window, cx),
                    "key" if connected && target.auth == "secret" => self.open_key_dialog(&target, window, cx),
                    // FEED-73: share the person's own connection with the team.
                    "share" if connected && share_menu_offered(target.is_http(), row.entry.shared) => {
                        self.toggle_shared(&target, !row.entry.shared, window, cx)
                    }
                    // VAPP-239: the device it runs on, for the dialog's Runs on row.
                    "edit" if owner => {
                        let team = target.team_id.clone();
                        self.open_editor(Some(target), row.entry.device.clone(), &team, window, cx)
                    }
                    "disconnect" if connected => self.confirm_disconnect(&target, window, cx),
                    "remove" if owner => self.confirm_remove(&target, window, cx),
                    _ => {}
                }
            }
            _ => {}
        }
    }

    /// The pane's state as the document reads it; what the rows' ⋯ menus
    /// act on is kept for their picks at the same time.
    fn project(&self, team: Option<&str>, owner: bool, instance: &str) -> Value {
        let mut menus = HashMap::new();
        let Some(_) = team else {
            *self.menus.borrow_mut() = menus;
            return json!({"noTeam": true, "rows": [], "apps": {"rows": []}});
        };
        let is_open = |key: &str| self.open.contains(key);
        let team_ids: Vec<String> = match &self.load {
            Load::Ready(Ok(rows)) => rows.iter().map(|entry| entry.config.id.clone()).collect(),
            _ => Vec::new(),
        };
        let order = mcp_server_order(!self.apps.is_empty(), &team_ids);
        let builtin = |id: &str| {
            let key = builtin_key(id);
            json!({"shown": order.contains(&key), "url": builtin_endpoint(instance, id), "open": is_open(&key), "closed": !is_open(&key)})
        };
        // Team apps: one sub-label per app (alphabetical), its tools.
        let mut apps = self.apps.clone();
        apps.sort_by_key(|app| app.name.to_lowercase());
        let app_rows: Vec<Value> = apps
            .iter()
            .flat_map(|app| {
                std::iter::once(json!({"key": format!("app:{}", app.id), "label": true, "tool": false, "text": app.name, "title": "", "blurb": "", "name": ""}))
                    .chain(app.tools.iter().map(move |tool| {
                        let title = if tool.title.is_empty() { tool.name.clone() } else { tool.title.clone() };
                        json!({"key": format!("{}:{}", app.id, tool.name), "label": false, "tool": true, "text": "", "title": title, "blurb": tool.blurb, "name": tool.name})
                    }))
            })
            .collect();
        let exponential_open = is_open(&builtin_key(BUILTIN_EXPONENTIAL));
        let mut out = json!({"noTeam": false, "owner": owner, "busy": self.busy, "reloading": self.reloading,
            "loading": false, "failed": false, "empty": false, "hasRows": false, "rows": [],
            "builtins": {BUILTIN_EXPONENTIAL: builtin(BUILTIN_EXPONENTIAL), BUILTIN_APPS: builtin(BUILTIN_APPS)},
            "builtin": super::mcp::project_builtin(&self.filter, self.filter_focused, exponential_open),
            "apps": {"rows": app_rows}});
        match &self.load {
            Load::Idle | Load::Loading => out["loading"] = json!(true),
            Load::Ready(Err(_)) => out["failed"] = json!(true),
            // No team server: the built-ins alone.
            Load::Ready(Ok(rows)) if rows.is_empty() => out["empty"] = json!(true),
            Load::Ready(Ok(rows)) => {
                out["hasRows"] = json!(true);
                let rows: Vec<Value> = rows
                    .iter()
                    .map(|entry| {
                        let config = &entry.config;
                        let pending = self.pending.as_deref() == Some(config.id.as_str());
                        let waiting = self.connecting.as_deref() == Some(config.id.as_str());
                        let action = row_action(entry);
                        let connected = action == RowAction::Connected;
                        let menu = row_menu_offered(owner, connected);
                        let items = if menu { menu_items(entry, owner, connected) } else { Vec::new() };
                        if menu {
                            menus.insert(config.id.clone(), MenuRow { entry: entry.clone(), owner, connected, pending, busy: self.busy });
                        }
                        let secret = config.auth == "secret";
                        let detail = row_caption(entry);
                        let icon = crate::picker::mcp_server_picker::mcp_server_icon(config.url.as_deref(), config.command.as_deref());
                        let open = is_open(&config.id);
                        let state = tools_state(entry);
                        let load = self.tools.get(&config.id).map(|(_, load)| load);
                        let tools: Vec<Value> = match (state, load) {
                            (McpToolsState::List, Some(Some(Ok(tools)))) => tools
                                .iter()
                                .map(|tool| {
                                    let title = if tool.title.is_empty() { tool.name.clone() } else { tool.title.clone() };
                                    json!({"key": tool.name, "name": tool.name, "title": title, "blurb": tool.blurb})
                                })
                                .collect(),
                            _ => Vec::new(),
                        };
                        let (tools_loading, failed_line) = match (state, load) {
                            (McpToolsState::List, None | Some(None)) => (true, None),
                            (McpToolsState::List, Some(Some(Err(reason)))) => (false, Some(tools_failed_line(reason))),
                            _ => (false, None),
                        };
                        let tools_line = match (state, load) {
                            (McpToolsState::Connect, _) => Some(SETTINGS_MCP.tools_connect),
                            (McpToolsState::Stdio, _) => Some(SETTINGS_MCP.tools_stdio),
                            (McpToolsState::List, Some(Some(Ok(listed)))) if listed.is_empty() => Some(SETTINGS_MCP.tools_none),
                            _ => None,
                        };
                        json!({
                            "open": open,
                            "closed": !open,
                            "toolsLoading": tools_loading,
                            "toolsFailed": failed_line.is_some(),
                            "failedLine": failed_line.unwrap_or_default(),
                            "hasToolsLine": tools_line.is_some(),
                            "toolsLine": tools_line.unwrap_or_default(),
                            "hasTools": !tools.is_empty(),
                            "tools": tools,
                            "key": config.id,
                            "icon": format!("{ASSET}{}", gpui_component::IconNamed::path(icon)),
                            "name": config.name,
                            "isDefault": config.enabled_by_default,
                            "detail": detail,
                            "hasDetail": !detail.is_empty(),
                            "action": if waiting { "waiting" } else { match action {
                                RowAction::NotNeeded => "none",
                                RowAction::Connected => "connected",
                                RowAction::Reconnect => "reconnect",
                                RowAction::Connect => "connect",
                            } },
                            "pending": pending,
                            "reason": reconnect_reason(entry),
                            "connectLabel": if secret { "Set key" } else { "Connect" },
                            "connectIcon": if secret { "ui-permission" } else { "ui-sign-in" },
                            "menu": menu,
                            "menuLabel": format!("Server menu for {}", config.name),
                            "items": items,
                            // gpui-component's disabled / loading ghost ⋯.
                            "menuLocked": self.busy || pending,
                            "menuLoading": connected && pending,
                        })
                    })
                    .collect();
                out["rows"] = Value::Array(rows);
            }
        }
        *self.menus.borrow_mut() = menus;
        out
    }
}

/// An icon the projection names by its asset path (the brand marks have no
/// concept name).
const ASSET: &str = "asset:";

/// What a row's ⋯ pick acts on.
#[derive(Clone)]
struct MenuRow {
    entry: McpServerListEntry,
    owner: bool,
    connected: bool,
    pending: bool,
    busy: bool,
}

/// The pane's host: the IDE's plugin and the brand marks by asset path.
struct McpHost {
    ide: crate::exponential_ui_host::IdePlugin,
}

impl HostPlugin for McpHost {
    fn icon(&self, name: &str) -> Option<SharedString> {
        match name.strip_prefix(ASSET) {
            Some(path) => Some(SharedString::from(path.to_string())),
            None => self.ide.icon(name),
        }
    }

    fn icon_element(&self, name: &str, size: f32, color: gpui::Hsla, cx: &App) -> Option<gpui::AnyElement> {
        // The Exponential built-in's mark (`ExpIcon::Logo`, as the tools').
        if name == super::mcp::LOGO {
            return Some(gpui_component::Icon::from(crate::icons::ExpIcon::Logo).size(gpui::px(size)).text_color(color).into_any_element());
        }
        self.ide.icon_element(name, size, color, cx)
    }

    fn on_unknown(&self, node: &eui_core::surface::PlacedNode) {
        self.ide.on_unknown(node)
    }
}

/// EXP-862 / EXP-1285 (web parity): everything a person can DO to a row
/// sits behind ONE ghost "..." menu — the connection items (Test, Replace
/// key, Share) with the owner's Edit, then the destructive Disconnect /
/// Remove below the separator (web `DropdownMenuItem
/// variant="destructive"`).
fn menu_items(entry: &McpServerListEntry, owner: bool, connected: bool) -> Vec<Value> {
    let config = &entry.config;
    let http = config.is_http();
    let mut items = Vec::new();
    if connected && http {
        items.push(json!({"label": "Test connection", "value": "test", "icon": "ui-refresh"}));
    }
    if connected && config.auth == "secret" {
        items.push(json!({"label": "Replace key", "value": "key", "icon": "ui-private"}));
    }
    // FEED-73: a stdio row only offers the undo of an existing share.
    if connected && share_menu_offered(http, entry.shared) {
        items.push(json!({"label": share_menu_label(entry.shared), "value": "share", "icon": "ui-share"}));
    }
    if owner {
        items.push(json!({"label": "Edit", "value": "edit", "icon": "ui-edit"}));
    }
    let mut danger = Vec::new();
    if connected {
        danger.push(json!({"label": "Disconnect", "value": "disconnect", "icon": "ui-clear", "destructive": true}));
    }
    if owner {
        danger.push(json!({"label": "Remove", "value": "remove", "icon": "ui-delete", "destructive": true}));
    }
    if !items.is_empty() && !danger.is_empty() {
        items.push(json!({"kind": "separator"}));
    }
    items.extend(danger);
    items
}

/// The colours the document needs beyond [`Ink`].
struct Tints {
    success: String,
    warning: String,
    skeleton: String,
    /// The ghost ⋯'s hover wash.
    list_hover: String,
}

/// The row's ONE connection control (web `ConnectionAction`): the waiting
/// pair, the "No sign-in needed" caption, the Connected caption, Reconnect
/// (the reason in its tooltip) or the primary Connect / Set key.
fn action_slot(ink: &Ink, tints: &Tints) -> Value {
    let is = |action: &str| eq(path("action"), json!(action));
    let connect_event = json!({"name": "connect", "context": {"key": path("key")}});
    let [_, waiting] = ink.pill_button_with("waiting", PillPaint::Glass, None, None, json!("Waiting for sign-in\u{2026}"), json!(true), None, json!({}));
    let [cancel, _] = ink.pill_button("cancel", None, json!("Cancel"), json!(false), json!({"name": "cancel"}));
    let row = |id: &str, visible: Value, children: Vec<Value>| json!({"id": id, "component": "Box", "visible": visible,
        "style": {"display": "flex", "flexDirection": "row", "alignItems": "center", "gap": ink.r(0.25), "flexShrink": 0}, "children": children});
    let mut none = ink.text("none", json!("No sign-in needed"), XS, &ink.muted);
    none["visible"] = is("none");
    none["style"]["flexShrink"] = json!(0);
    let connected = row("connected", is("connected"), vec![
        json!({"id": "connected_icon", "component": "Icon", "props": {"name": "ui-success"}, "style": {"width": 14, "height": 14, "flexShrink": 0, "color": tints.success}}),
        ink.text("connected_text", json!("Connected"), XS, &tints.success),
    ]);
    let reconnect = ink.pill_button_with("reconnect", PillPaint::Glass, Some(json!("ui-warning")), Some(&tints.warning), json!("Reconnect"), path("pending"), Some(path("pending")), connect_event.clone());
    let reconnect = json!({"id": "reconnect_tip", "component": "Tooltip", "visible": is("reconnect"), "props": {"content": path("reason")}, "children": reconnect});
    let connect = ink.pill_button_with("connect", PillPaint::Primary, Some(path("connectIcon")), None, path("connectLabel"), path("pending"), Some(path("pending")), connect_event);
    let connect = row("connect", is("connect"), connect.to_vec());
    json!({"id": "action", "component": "Box", "style": {"display": "flex", "flexDirection": "row", "alignItems": "center", "flexShrink": 0},
        "children": [row("waiting_pair", is("waiting"), vec![waiting, cancel]), none, connected, reconnect, connect]})
}

/// The chevron's size (`disclosure_header`'s `xsmall` glyph).
const CHEVRON: f32 = 12.;

/// The tools' indent under an open row: the row's `px_3`, the chevron and
/// its `gap_2`, so a tool lines up with the row's glyph.
fn tools_indent(ink: &Ink) -> f32 {
    ink.r(0.75) + CHEVRON + ink.r(0.5)
}

/// THE disclosure header (`controls::disclosure_header`, chevron leading,
/// the whole leading part the target): the chevron (right closed, down
/// open) before `content` (the row's glyph, name and caption). A row's
/// trailing actions stay outside it.
fn disclosure(ink: &Ink, id: &str, label: Value, open: Value, closed: Value, event: Value, content: Vec<Value>) -> Value {
    let chevron = |suffix: &str, icon: &str, visible: Value| json!({"id": format!("{id}_{suffix}"), "component": "Icon", "visible": visible, "props": {"name": icon}, "style": {"width": CHEVRON, "height": CHEVRON, "color": ink.muted, "flexShrink": 0}});
    let mut children = vec![chevron("closed", "ui-chevron-right", closed), chevron("open", "ui-chevron-down", open)];
    children.extend(content);
    json!({"id": id, "component": "Box", "props": {"pressable": true}, "accessibility": {"label": label}, "on": {"press": {"event": event}},
        "style": {"display": "flex", "flexDirection": "row", "alignItems": "center", "gap": ink.r(0.5), "flexGrow": 1, "flexShrink": 1, "flexBasis": 0, "minWidth": 0, "cursor": "pointer"},
        "children": children})
}

/// A row's frame (`flat_row`, `px_3` / `py_2`, `gap_3`).
fn row_frame(ink: &Ink, id: &str, children: Vec<Value>) -> Value {
    json!({"id": id, "component": "Box",
        "style": {"display": "flex", "flexDirection": "row", "alignItems": "center", "width": "100%", "minWidth": 0, "gap": ink.r(0.75), "paddingHorizontal": ink.r(0.75), "paddingVertical": ink.r(0.5)},
        "children": children})
}

/// The name (medium, with its pill) over the muted caption (`flex_1`);
/// `mono` sets the caption in the mono family (a built-in's URL).
fn name_column(ink: &Ink, id: &str, name: Value, pill: Value, caption: Value, has_caption: Value, mono: bool) -> Value {
    let mut name = ink.text(&format!("{id}_name"), name, SM, &ink.foreground);
    name["style"]["fontWeight"] = json!(500);
    name["style"]["minWidth"] = json!(0);
    let mut detail = ink.text(&format!("{id}_detail"), caption, XS, &ink.muted);
    detail["visible"] = has_caption;
    detail["props"]["lines"] = json!(1);
    detail["style"]["minWidth"] = json!(0);
    if mono {
        detail["style"]["fontFamily"] = json!(ink.mono);
    }
    json!({"id": id, "component": "Box", "style": {"display": "flex", "flexDirection": "column", "gap": ink.r(0.25), "flexGrow": 1, "flexShrink": 1, "flexBasis": 0, "minWidth": 0},
        "children": [
            {"id": format!("{id}_head"), "component": "Box", "style": {"display": "flex", "flexDirection": "row", "alignItems": "center", "gap": ink.r(0.5), "width": "100%", "minWidth": 0}, "children": [name, pill]},
            detail,
        ]})
}

/// One team server row: the disclosure (glyph, name with the Default pill
/// over the caption), the action, the ⋯; open = its tools under it.
fn server_row(ink: &Ink, tints: &Tints) -> Value {
    // The glyph is a fixed 14px (`Icon::size(px(14.))`), not a rem.
    let glyph = 14.;
    let mut chip = ink.pill("row_default", json!("Default"), false, None);
    chip["visible"] = path("isDefault");
    let icon = json!({"id": "row_icon", "component": "Icon", "props": {"name": path("icon")}, "style": {"width": glyph, "height": glyph, "color": ink.muted, "flexShrink": 0}});
    let head = disclosure(ink, "row_toggle", path("name"), path("open"), path("closed"), json!({"name": "toggle", "context": {"key": path("key")}}),
        vec![icon, name_column(ink, "row_text", path("name"), chip, path("detail"), path("hasDetail"), false)]);
    let menu = ink.row_menu("menu", path("menuLabel"), path("menu"), &tints.list_hover, Some(path("menuLocked")), Some(path("menuLoading")));
    row_frame(ink, "row", vec![head, action_slot(ink, tints), menu])
}

/// An open team server's tools (`toolsStateCases`): the skeleton while
/// `mcpServers.tools` loads, the failed line with its Retry, the muted
/// none / connect / stdio line, or the flat list.
fn server_tools(ink: &Ink, tints: &Tints) -> Value {
    let indent = tools_indent(ink);
    let muted_line = |id: &str, text: Value| {
        let mut node = ink.text(id, text, XS, &ink.muted);
        node["style"]["minWidth"] = json!(0);
        node["style"]["flexShrink"] = json!(1);
        node
    };
    let mut retry = ink.text("tools_retry_text", json!(SETTINGS_MCP.retry), XS, &ink.primary);
    retry["style"]["cursor"] = json!("pointer");
    let retry = json!({"id": "tools_retry", "component": "Box", "props": {"pressable": true}, "accessibility": {"label": SETTINGS_MCP.retry},
        "on": {"press": {"event": {"name": "toolsRetry", "context": {"key": path("key")}}}}, "style": {"flexShrink": 0}, "children": [retry]});
    let line_box = |id: &str, visible: Value, children: Vec<Value>| json!({"id": id, "component": "Box", "visible": visible,
        "style": {"display": "flex", "flexDirection": "row", "alignItems": "center", "gap": ink.r(0.5), "width": "100%", "minWidth": 0, "paddingLeft": indent, "paddingRight": ink.r(0.75), "paddingVertical": ink.r(0.375)},
        "children": children});
    let mut skeleton = ink.skeleton_rows_at("tools_skeleton", &tints.skeleton, ink.r(SM) * super::doc::LINE + 2. * ink.r(0.375), path("toolsLoading"));
    skeleton["style"]["paddingLeft"] = json!(indent - ink.r(0.75));
    let tools = json!({"id": "tools", "component": "List", "visible": path("hasTools"), "props": {"gap": "none"},
        "template": {"component": "tools_wrap", "path": "tools", "key": "key"},
        "children": [{"id": "tools_wrap", "component": "Box", "style": {"display": "flex", "flexDirection": "column", "width": "100%", "minWidth": 0}, "children": [super::mcp::tool_row(ink, "srv_tool", indent, false)]}]});
    json!({"id": super::mcp::test_id("serverTools"), "component": "Box", "visible": path("open"), "style": {"display": "flex", "flexDirection": "column", "width": "100%", "minWidth": 0, "paddingBottom": ink.r(0.5)},
        "children": [
            skeleton,
            line_box("tools_failed", path("toolsFailed"), vec![muted_line("tools_failed_text", path("failedLine")), retry]),
            line_box("tools_line", path("hasToolsLine"), vec![muted_line("tools_line_text", path("toolsLine"))]),
            tools,
        ]})
}

/// A built-in row (`builtins.<id>`): the disclosure (the Exponential mark,
/// the name with the `builtinPill`, its URL in mono) and ONE action, Copy.
fn builtin_row(ink: &Ink, chrome: &Chrome, id: &str, name: &str, icon: Value, tint: Value) -> Value {
    let data = format!("/pane/builtins/{id}");
    let pill = ink.pill(&format!("{id}_pill"), json!(SETTINGS_MCP.builtin_pill), false, None);
    let mut glyph = super::mcp::glyph(&format!("{id}_icon"), icon, 14., &ink.foreground);
    glyph["style"]["color"] = tint;
    let head = disclosure(ink, &format!("{id}_toggle"), json!(name), path(&format!("{data}/open")), path(&format!("{data}/closed")),
        json!({"name": "toggle", "context": {"key": builtin_key(id)}}),
        vec![glyph, name_column(ink, &format!("{id}_text"), json!(name), pill, path(&format!("{data}/url")), json!(true), true)]);
    let copy = super::mcp::outline_button(ink, chrome, &format!("{id}_copy"), SETTINGS_MCP.copy, false, json!({"name": "copy", "context": {"key": id}}));
    row_frame(ink, &format!("{id}_row"), vec![head, copy])
}

/// The pane's tree: the band (the owner's Add server pill, the reload
/// spinner) over THE one group: the built-ins (each with its open tools),
/// then the team's rows (or the skeleton / failed state); no team = its
/// empty state alone.
fn document(ink: &Ink, tints: &Tints, chrome: &Chrome) -> Value {
    let mut add = ink.pill_button("add", Some("ui-add"), json!("Add server"), path("/pane/busy"), json!({"name": "add"}));
    // gpui centres the label's line half a point lower than the flex centre.
    for face in &mut add {
        face["children"][1]["style"]["paddingTop"] = json!(1);
    }
    let add = json!({"id": "add_slot", "component": "Box", "visible": path("/pane/owner"), "style": {"display": "flex", "flexDirection": "row", "alignItems": "center", "flexShrink": 0}, "children": add});
    let indent = tools_indent(ink);

    // Exponential: always; open = the filter + the contract tool groups.
    let exponential = json!({"id": super::mcp::test_id("builtin"), "component": "Box", "style": {"display": "flex", "flexDirection": "column", "width": "100%", "minWidth": 0},
        "children": [
            builtin_row(ink, chrome, BUILTIN_EXPONENTIAL, SETTINGS_MCP.builtin_exponential.name, json!(super::mcp::LOGO), json!(ink.foreground)),
            super::mcp::builtin_tools(ink, chrome, indent),
        ]});
    // Team apps: when an app exposes tools; open = one sub-label per app.
    let mut app_label = super::mcp::sub_label(ink, "apps_label", path("text"), indent);
    app_label["visible"] = path("label");
    let mut app_tool = super::mcp::tool_row(ink, "apps_tool", indent, false);
    app_tool["visible"] = path("tool");
    let app_tools = json!({"id": "apps_tools", "component": "Box", "visible": path("/pane/builtins/apps/open"), "style": {"display": "flex", "flexDirection": "column", "width": "100%", "minWidth": 0, "paddingBottom": ink.r(0.5)},
        "children": [super::mcp::list("apps_rows", json!(true), "/pane/apps/rows", json!({"id": "apps_item", "component": "Box", "style": {"display": "flex", "flexDirection": "column", "width": "100%", "minWidth": 0}, "children": [app_label, app_tool]}))]});
    let apps = json!({"id": super::mcp::test_id("appsServer"), "component": "Box", "visible": path("/pane/builtins/apps/shown"), "style": {"display": "flex", "flexDirection": "column", "width": "100%", "minWidth": 0},
        "children": [
            ink.divider("apps_rule", json!(true)),
            builtin_row(ink, chrome, BUILTIN_APPS, SETTINGS_MCP.builtin_apps.name, json!("nav-apps"), json!(ink.foreground)),
            app_tools,
        ]});

    // The team's servers, each under a hairline.
    let rows = json!({"id": "rows", "component": "List", "visible": path("/pane/hasRows"), "props": {"gap": "none"},
        "template": {"component": "row_wrap", "path": "/pane/rows", "key": "key"},
        "children": [{"id": "row_wrap", "component": "Box", "style": {"display": "flex", "flexDirection": "column", "width": "100%", "minWidth": 0},
            "children": [ink.divider("row_rule", json!(true)), server_row(ink, tints), server_tools(ink, tints)]}]});
    let group = ink.group("group", vec![
        exponential,
        apps,
        ink.divider("loading_rule", path("/pane/loading")),
        ink.skeleton_rows("skeleton", &tints.skeleton, path("/pane/loading")),
        ink.divider("failed_rule", path("/pane/failed")),
        failed_state(ink, "failed", "settings-mcp", crate::empty_state_copy::MCP_SERVERS, path("/pane/failed")),
        rows,
    ]);
    let list = json!({"id": "list", "component": "Box", "visible": not(path("/pane/noTeam")), "style": {"display": "flex", "flexDirection": "column", "gap": ink.r(0.5), "width": "100%", "minWidth": 0},
        "children": [
            ink.band_with("band", None, SETTINGS_MCP.labels.servers, Some(path("/pane/reloading")), Some(add)),
            group,
        ]});
    json!({"id": "root", "component": "Box", "style": {"display": "flex", "flexDirection": "column", "width": "100%"},
        "children": [list, empty_state(ink, "no_team", "ui-team", crate::empty_state_copy::NO_TEAM_SELECTED, path("/pane/noTeam"))]})
}

/// The servers document, for the MCP page's mount check.
#[cfg(test)]
pub(super) fn test_document(ink: &Ink, chrome: &Chrome) -> Value {
    let tints = Tints { success: "#22c55eff".into(), warning: "#f59e0bff".into(), skeleton: "#a1a1aa2e".into(), list_hover: "#ffffff0a".into() };
    document(ink, &tints, chrome)
}

/// #VAPP-243: an app's Add server waiting for this pane (set, then Settings
/// › MCP opens; the pane's next render takes it and opens the add form).
#[derive(Default)]
pub(crate) struct PendingMcpAdd(pub Option<String>);

impl gpui::Global for PendingMcpAdd {}

/// Ask the MCP servers pane to open its add form with `name`.
pub(crate) fn request_add(name: String, cx: &mut App) {
    cx.set_global(PendingMcpAdd(Some(name)));
}

impl Render for McpServersPane {
    fn render(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let _span = tracing::trace_span!("render", view = "McpServersPane").entered();
        // The bench: an open popup's enter fade run out by hand (the
        // headless clock stands still).
        #[cfg(test)]
        if self.shot.is_some() && !self.view.read(cx).layers().is_empty() {
            self.view.update(cx, |view, cx| view.advance_clock(std::time::Duration::from_millis(500), cx));
        }
        let team = self.pane_team(cx);
        let mut owner = false;
        if let Some(team_id) = team.as_deref() {
            #[cfg(test)]
            let pinned = self.shot.is_some();
            #[cfg(not(test))]
            let pinned = false;
            if !pinned {
                self.ensure_loaded(team_id, cx);
            }
            owner = self.pane_owner(team_id, cx);
            if let Some(name) = cx.try_global::<PendingMcpAdd>().and_then(|p| p.0.clone()) {
                cx.set_global(PendingMcpAdd(None));
                if owner {
                    let team = team_id.to_string();
                    cx.defer_in(window, move |this, window, cx| this.open_add_named(name, &team, window, cx));
                }
            }
        }
        let projection = self.project(team.as_deref(), owner, &self.instance(cx));
        self.binding.update(cx, |binding, cx| binding.update_state(|state| *state = projection, cx));
        div().w_full().min_w_0().child(self.view.clone())
    }
}

/// The Set key alert's body (#EXP-1629: an eUI document, web
/// `SecretDialog`): the secret's name (`text_xs` muted) over the masked
/// boxed field (`web_input_sm`), `gap_1` under the description's `mt_2`;
/// #EXP-1793: one such pair per declared name. Its own entity so the alert
/// reads the fields' live text on Save.
pub(crate) struct KeyContent {
    view: Entity<SurfaceView>,
    /// How many fields the body holds (one per declared name).
    count: usize,
}

/// The field id (and bound key) of the `index`th name: the first keeps
/// the single-name body's `value`.
fn key_field_id(index: usize) -> String {
    if index == 0 {
        "value".to_string()
    } else {
        format!("value_{index}")
    }
}

impl KeyContent {
    /// `typed`: the fields' bound text (only the bench writes any).
    fn new(fields: &[String], typed: &[&str], window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        let mut state = serde_json::Map::new();
        for index in 0..fields.len().max(1) {
            state.insert(key_field_id(index), json!(typed.get(index).copied().unwrap_or_default()));
        }
        let binding = cx.new(|_| {
            let mut binding = Binding::new(PATH, Value::Object(state), |state: &Value, _| state.clone());
            binding.set_toasts(Rc::new(IdeToasts));
            binding
        });
        let view = super::doc::mount("mcp-server-key", key_document(&Ink::of(cx), fields), &binding, window, cx);
        Self { view, count: fields.len().max(1) }
    }

    #[cfg(test)]
    pub(crate) fn for_shots(field: &str, typed: Option<&str>, window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        Self::new(&[field.to_string()], &typed.into_iter().collect::<Vec<_>>(), window, cx)
    }

    /// #EXP-1793: a server that declares several names.
    #[cfg(test)]
    pub(crate) fn for_shots_many(fields: &[&str], typed: &[&str], window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        Self::new(&fields.iter().map(|field| field.to_string()).collect::<Vec<_>>(), typed, window, cx)
    }

    /// The keys as typed, one per field (the fields' live text, never a
    /// debounced echo).
    fn typed(&self, cx: &App) -> Vec<String> {
        (0..self.count)
            .map(|index| self.view.read(cx).field_text(&format!("{}.field", key_field_id(index)), cx).unwrap_or_default())
            .collect()
    }
}

impl Render for KeyContent {
    fn render(&mut self, _window: &mut Window, _cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let _span = tracing::trace_span!("render", view = "KeyContent").entered();
        div().w_full().min_w_0().child(self.view.clone())
    }
}

/// The Set key body's tree (`gap_1`, `mt_2` as the root's top padding):
/// the secret's name, the masked field. #EXP-1793: several names = one
/// such pair per name (`gap_3` between them).
fn key_document(ink: &Ink, fields: &[String]) -> Value {
    let pair = |index: usize, field: &str| -> [Value; 2] {
        let id = key_field_id(index);
        let label = if index == 0 { "label".to_string() } else { format!("label_{index}") };
        let input = json!({"id": id, "component": "Input", "props": {"type": "password", "variant": "box", "size": "sm", "label": field, "name": id, "placeholder": "Paste the key", "value": path(&format!("/pane/{id}"))},
            "style": {"width": "100%"}});
        [ink.text(&label, json!(field), XS, &ink.muted), input]
    };
    if fields.len() <= 1 {
        let field = fields.first().map(String::as_str).unwrap_or("Key");
        return json!({"id": "root", "component": "Box", "style": {"display": "flex", "flexDirection": "column", "width": "100%", "minWidth": 0, "gap": ink.r(0.25), "paddingTop": ink.r(0.5)},
            "children": pair(0, field)});
    }
    let groups: Vec<Value> = fields
        .iter()
        .enumerate()
        .map(|(index, field)| {
            json!({"id": format!("field_{index}"), "component": "Box", "style": {"display": "flex", "flexDirection": "column", "width": "100%", "minWidth": 0, "gap": ink.r(0.25)},
                "children": pair(index, field)})
        })
        .collect();
    json!({"id": "root", "component": "Box", "style": {"display": "flex", "flexDirection": "column", "width": "100%", "minWidth": 0, "gap": ink.r(0.75), "paddingTop": ink.r(0.5)},
        "children": groups})
}

/// The Disconnect / Remove alerts' body (#EXP-1629: an eUI document): the
/// server the action hits, `text_sm` medium.
pub(crate) struct ServerName {
    view: Entity<SurfaceView>,
}

impl ServerName {
    fn new(name: &str, window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        let binding = cx.new(|_| {
            let mut binding = Binding::new(PATH, json!({"name": name}), |state: &Value, _| state.clone());
            binding.set_toasts(Rc::new(IdeToasts));
            binding
        });
        let view = super::doc::mount("mcp-server-name", name_document(&Ink::of(cx)), &binding, window, cx);
        Self { view }
    }

    #[cfg(test)]
    pub(crate) fn for_shots(name: &str, window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        Self::new(name, window, cx)
    }
}

impl Render for ServerName {
    fn render(&mut self, _window: &mut Window, _cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let _span = tracing::trace_span!("render", view = "ServerName").entered();
        div().w_full().min_w_0().child(self.view.clone())
    }
}

/// The Disconnect / Remove body's tree: the server's name.
fn name_document(ink: &Ink) -> Value {
    let mut name = ink.text("name", path("/pane/name"), SM, &ink.foreground);
    name["style"]["fontWeight"] = json!(500);
    name
}

#[cfg(test)]
mod tests {
    use super::*;
    use api::mcp_servers::{McpConnection, McpTestResult};

    /// #EXP-1629: the alert bodies raise no issue a debug build's mount
    /// check refuses (#EXP-1767's class).
    #[test]
    fn the_alert_documents_raise_no_unexpected_issue() {
        let ink = super::super::readiness::tests::test_ink();
        for (id, tree) in [("mcp-server-key", key_document(&ink, &["STRIPE_API_KEY".to_string()])), ("mcp-server-key-many", key_document(&ink, &["X-Api-Key".to_string(), "X-Org".to_string()])), ("mcp-server-name", name_document(&ink))] {
            let bound = super::super::doc::bound_values(&tree);
            let mut surface = eui_core::surface::Surface::new(id, Default::default());
            let outcome = surface.set_nested(serde_json::from_value(tree).expect("parses"));
            let issues = super::super::doc::unexpected_issues(&bound, &outcome.issues);
            assert!(issues.is_empty(), "{id}: {issues:?}");
        }
    }

    fn entry(auth: &str, status: &str) -> McpServerListEntry {
        McpServerListEntry {
            config: McpServerConfig {
                id: "s1".into(),
                name: "Linear".into(),
                url: Some("https://mcp.linear.app/mcp".into()),
                auth: auth.into(),
                ..Default::default()
            },
            connection: McpConnection {
                status: status.into(),
                expires_at: None,
                error: None,
            },
            connected_count: 2,
            member_count: 5,
            ..Default::default()
        }
    }

    /// FEED-73 — the row menu's share item and its toasts, web copy verbatim.
    #[test]
    fn share_copy_matches_the_web() {
        assert_eq!(share_menu_label(false), "Share with team");
        assert_eq!(share_menu_label(true), "Stop sharing");
        assert_eq!(
            share_toast("Linear", true),
            (
                "Shared Linear with the team: action sessions on teammates' machines use your connection"
                    .to_string(),
                "Could not share"
            )
        );
        assert_eq!(
            share_toast("Linear", false),
            ("Stopped sharing Linear".to_string(), "Could not stop sharing")
        );
    }

    /// EXP-1285 — ONE "…" menu per row: an owner always has it (Edit /
    /// Remove), a member only on a connected row (web `ServerRow`).
    #[test]
    fn row_menu_for_owners_and_connected_rows_only() {
        assert!(row_menu_offered(true, false));
        assert!(row_menu_offered(true, true));
        assert!(row_menu_offered(false, true));
        assert!(!row_menu_offered(false, false));
    }

    /// A stdio server's secret stays on this machine: no share item, except
    /// to undo a share made before the server refused them (web
    /// `ServerRow`: `http || shared`).
    #[test]
    fn share_item_hidden_on_an_unshared_stdio_row() {
        assert!(share_menu_offered(true, false));
        assert!(share_menu_offered(true, true));
        assert!(!share_menu_offered(false, false));
        assert!(share_menu_offered(false, true));
    }

    /// Web `ConnectionAction`: one action per row, from the person's own
    /// connection.
    #[test]
    fn row_action_follows_the_connection() {
        assert_eq!(row_action(&entry("none", "not_needed")), RowAction::NotNeeded);
        assert_eq!(row_action(&entry("oauth", "connected")), RowAction::Connected);
        assert_eq!(row_action(&entry("oauth", "expired")), RowAction::Reconnect);
        assert_eq!(row_action(&entry("oauth", "error")), RowAction::Reconnect);
        assert_eq!(row_action(&entry("oauth", "not_connected")), RowAction::Connect);
        assert_eq!(row_action(&entry("secret", "not_connected")), RowAction::Connect);
    }

    #[test]
    fn row_detail_is_the_host_or_the_command_only() {
        assert_eq!(row_detail(&entry("oauth", "connected")), "mcp.linear.app");
        assert_eq!(row_detail(&entry("none", "not_needed")), "mcp.linear.app");
        let mut stdio = entry("secret", "connected");
        stdio.config.transport = "stdio".into();
        stdio.config.command = Some("npx".into());
        stdio.config.args = vec!["-y".into(), "@acme/mcp".into()];
        assert_eq!(row_detail(&stdio), "npx -y @acme/mcp");
        assert_eq!(url_host("https://u:p@Example.com:8443/mcp?x=1"), "example.com:8443");
        assert_eq!(url_host("not a url"), "not a url");
    }

    #[test]
    fn reconnect_reason_prefers_the_servers_sentence() {
        let mut failed = entry("oauth", "error");
        assert_eq!(reconnect_reason(&failed), "The last refresh failed.");
        failed.connection.error = Some("invalid_grant".into());
        assert_eq!(reconnect_reason(&failed), "invalid_grant");
        assert_eq!(reconnect_reason(&entry("oauth", "expired")), "Your sign-in expired.");
    }

    /// Web `test` toasts, string for string.
    #[test]
    fn test_messages_match_the_web() {
        let result = |ok: bool, tools: Option<u32>, error: Option<&str>| McpTestResult {
            ok,
            tools,
            error: error.map(str::to_string),
        };
        assert_eq!(test_message("Linear", &result(true, None, None)), "Linear answered");
        assert_eq!(
            test_message("Linear", &result(true, Some(1), None)),
            "Linear answered with 1 tool"
        );
        assert_eq!(
            test_message("Linear", &result(true, Some(12), None)),
            "Linear answered with 12 tools"
        );
        assert_eq!(
            test_message("Linear", &result(false, None, Some("401 Unauthorized"))),
            "Linear did not answer: 401 Unauthorized"
        );
        assert_eq!(test_message("Linear", &result(false, None, None)), "Linear did not answer");
    }

    /// EXP-1274/1657: the built-in tools filter (now
    /// `steer::exp_tool::builtin_tool_matches`, the bands' rule) matches
    /// title, blurb or wire name, case-insensitively; blank keeps all,
    /// nonsense keeps none.
    #[test]
    fn builtin_tools_filter_matches_title_blurb_or_name() {
        use steer::exp_tool::{builtin_tool_bands, builtin_tool_matches, builtin_tools};
        let all = builtin_tools();
        assert!(!all.is_empty());
        let first = all[0].clone();
        assert!(builtin_tool_matches(&first, "  "));
        assert!(builtin_tool_matches(&first, &first.title.to_uppercase()));
        assert!(builtin_tool_matches(&first, &first.name));
        assert!(builtin_tool_matches(&first, first.blurb));
        assert!(!builtin_tool_matches(&first, "zzqx-no-such-tool"));
        assert!(builtin_tool_bands("zzqx-no-such-tool").is_empty());
    }
}
