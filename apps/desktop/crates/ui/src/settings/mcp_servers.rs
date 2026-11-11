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
//! #EXP-1629: the pane is a document on the native binding (projection at
//! `/pane`): the band with the owner's Add server pill and the reload
//! spinner, the skeleton, the failed and empty states, and the rows (glyph,
//! name + Default pill over the caption, the one connection control). Each
//! row's ⋯ stays the IDE's own menu, drawn into the document as a
//! `host_leaf` Embed (`row_menu`); the dialogs stay native.

use crate::controls::PointerMenuButtonExt as _;
use crate::toast::Toast;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::time::{Duration, Instant};

use gpui::{div, AnyElement, App, AppContext as _, Entity, IntoElement, ParentElement, Render, SharedString, Styled, Subscription, WeakEntity, Window};
use gpui::FontWeight;
use gpui_component::{button::ButtonVariant, input::InputState, v_flex, ActiveTheme as _, Disableable as _, Icon};
use serde_json::{json, Value};

use api::mcp_servers::{McpServerConfig, McpServerListEntry};
use eui_gpui::binding::Binding;
use eui_gpui::controls::CTL_MD_H;
use eui_gpui::host::HostPlugin;
use eui_gpui::view::SurfaceView;
use sync::Store;

use crate::controls::{glass_input, WebControl as _};
use crate::icons::registry;
use crate::native_dialog::{open_alert, AlertSpec};
use crate::navigation::{active_team_id, Navigation};
use crate::queries;

use super::doc::{empty_state, eq, failed_state, hex, mount_with, not, path, IdeToasts, Ink, PillPaint, SM, XS};
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

/// The data-model path the pane's projection is written at.
const PATH: &str = "/pane";

enum Load {
    Idle,
    Loading,
    Ready(Result<Vec<McpServerListEntry>, String>),
}

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
    /// The "Set key" dialog's field. One input reused by every row: only
    /// one dialog is ever open.
    value_input: Entity<InputState>,
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
    /// The rows' ⋯ menus the document's host draws (`row_menu`).
    host: Rc<RefCell<HostState>>,
    binding: Entity<Binding<Value>>,
    view: Entity<SurfaceView>,
    _subscriptions: Vec<Subscription>,
}

/// A pinned state for the headless bench (the headless store has no team).
#[cfg(test)]
pub(crate) struct McpShot {
    pub team: Option<String>,
    pub owner: bool,
}

impl McpServersPane {
    pub fn new(
        nav: Entity<Navigation>,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> Self {
        let value_input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("Paste the key")
                .masked(true)
        });
        let subscriptions = vec![cx.observe(&nav, |_, _, cx| cx.notify())];
        // The actions leave the binding for the pane: Add server and Connect
        // open dialogs or the browser, Retry and Cancel restart the reads.
        let pane = cx.weak_entity();
        let handle = window.window_handle();
        let binding = cx.new(|_| {
            let mut binding = Binding::new(PATH, Value::Null, |state: &Value, _| state.clone());
            binding.set_toasts(Rc::new(IdeToasts));
            for name in ["add", "retry", "connect", "cancel"] {
                let pane = pane.clone();
                binding.on_action(name, move |_, event, cx| {
                    let key = event.context.get("key").and_then(Value::as_str).unwrap_or_default().to_string();
                    let pane = pane.clone();
                    // Deferred: the binding is mid-update and these write it.
                    cx.defer(move |cx| {
                        let _ = handle.update(cx, |_, window, cx| {
                            let _ = pane.update(cx, |pane, cx| pane.action(name, &key, window, cx));
                        });
                    });
                    None
                });
            }
            binding
        });
        let host = Rc::new(RefCell::new(HostState { menus: HashMap::new(), pane: Some(pane) }));
        let tints = Tints { success: hex(cx.theme().success), warning: hex(cx.theme().warning), skeleton: hex(cx.theme().muted_foreground.opacity(0.18)) };
        let plugin = Rc::new(McpHost { ide: super::doc::ide_plugin(), state: host.clone() });
        let view = mount_with("settings-mcp-servers", document(&Ink::of(cx), &tints), &binding, plugin, false, window, cx);
        Self {
            nav,
            load: Load::Idle,
            team_id: None,
            generation: 0,
            busy: false,
            pending: None,
            connecting: None,
            connect_generation: 0,
            value_input,
            reload_requested: false,
            reloading: false,
            retry_pressed: false,
            #[cfg(test)]
            shot: None,
            host,
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
    // dialog stays native: the native alert's content is a boxed `glass_input` (genUI Input `box` lands first, EXP-1629).
    fn open_key_dialog(
        &mut self,
        config: &McpServerConfig,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        self.value_input.update(cx, |state, cx| {
            state.set_value("", window, cx);
        });
        let pane = cx.entity().downgrade();
        let handle = window.window_handle();
        let content_input = self.value_input.clone();
        let ok_input = self.value_input.clone();
        let field: SharedString = config
            .secret_names()
            .first()
            .cloned()
            .unwrap_or_else(|| "Key".to_string())
            .into();
        let server_id = config.id.clone();
        let server = config.name.clone();
        let spec = AlertSpec::new(
            format!("API key for {server}"),
            "Only your sessions use it. Stored encrypted; nobody can read it back.",
            "Save key",
        )
        .content(move |window, cx| {
            v_flex()
                .gap_1()
                .mt_2()
                .child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child(field.clone()),
                )
                .child(glass_input(&content_input, window, cx).web_input_sm())
                .into_any_element()
        })
        .on_ok(move |_, cx| {
            let value = ok_input.read(cx).value().trim().to_string();
            if value.is_empty() {
                return false;
            }
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
                    .spawn(async move { api::mcp_servers::set_secret(&trpc, &server_id, &value) })
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
    // dialog stays native: the shared native confirm alert.
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
        let identity: SharedString = name.clone().into();
        let spec = AlertSpec::new(
            "Disconnect",
            "Your credential for this server is deleted. Your sessions stop getting its \
             tools until you connect again; other members keep theirs.",
            "Disconnect",
        )
        .content(move |_, _| {
            div()
                .text_sm()
                .font_weight(FontWeight::MEDIUM)
                .child(identity.clone())
                .into_any_element()
        })
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
    // dialog stays native: the shared native confirm alert.
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
        let identity: SharedString = name.clone().into();
        let spec = AlertSpec::new(
            "Remove server",
            "Sessions stop offering it and every member's connection to it is deleted. \
             This cannot be undone.",
            "Remove",
        )
        .content(move |_, _| {
            div()
                .text_sm()
                .font_weight(FontWeight::MEDIUM)
                .child(identity.clone())
                .into_any_element()
        })
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
    // dialog: `mcp_server_dialog` is its own native window, its content a document (the pickers its host's Embeds).
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

    /// A document action (`add`, `retry`, `connect` / `cancel` on the row `key`).
    fn action(&mut self, name: &str, key: &str, window: &mut Window, cx: &mut gpui::Context<Self>) {
        match name {
            "add" if !self.busy => {
                if let Some(team) = self.pane_team(cx) {
                    self.open_editor(None, None, &team, window, cx);
                }
            }
            "retry" => self.retry_load(cx),
            "cancel" => self.cancel_connect(cx),
            "connect" => {
                let Load::Ready(Ok(rows)) = &self.load else { return };
                let Some(config) = rows.iter().find(|entry| entry.config.id == key).map(|entry| entry.config.clone()) else { return };
                if self.pending.as_deref() != Some(key) {
                    self.connect(&config, window, cx);
                }
            }
            _ => {}
        }
    }

    /// The pane's state as the document reads it; the rows' ⋯ menus are
    /// written into the host's table at the same time.
    fn project(&self, team: Option<&str>, owner: bool) -> Value {
        let mut menus = HashMap::new();
        let Some(_) = team else {
            self.host.borrow_mut().menus = menus;
            return json!({"noTeam": true, "rows": []});
        };
        let mut out = json!({"noTeam": false, "owner": owner, "busy": self.busy, "reloading": self.reloading,
            "loading": false, "failed": false, "empty": false, "hasRows": false, "rows": []});
        match &self.load {
            Load::Idle | Load::Loading => out["loading"] = json!(true),
            Load::Ready(Err(_)) => out["failed"] = json!(true),
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
                        if menu {
                            menus.insert(config.id.clone(), MenuRow { entry: entry.clone(), owner, connected, pending, busy: self.busy });
                        }
                        let secret = config.auth == "secret";
                        let detail = row_caption(entry);
                        let icon = crate::picker::mcp_server_picker::mcp_server_icon(config.url.as_deref(), config.command.as_deref());
                        json!({
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
                            // The ⋯ leaf repaints when what it draws moves.
                            "menuSurface": {"root": {"id": "menu", "component": "Box"}, "data": {"pending": pending, "busy": self.busy, "connected": connected, "shared": entry.shared}},
                        })
                    })
                    .collect();
                out["rows"] = Value::Array(rows);
            }
        }
        self.host.borrow_mut().menus = menus;
        out
    }
}

/// An icon the projection names by its asset path (the brand marks have no
/// concept name).
const ASSET: &str = "asset:";

/// The row's ⋯ (`menu.<key>` once painted).
const MENU_NODE: &str = "menu";

/// What a row's ⋯ needs to draw and open its menu.
struct MenuRow {
    entry: McpServerListEntry,
    owner: bool,
    connected: bool,
    pending: bool,
    busy: bool,
}

#[derive(Default)]
struct HostState {
    menus: HashMap<String, MenuRow>,
    pane: Option<WeakEntity<McpServersPane>>,
}

/// The pane's host: the IDE's plugin, the brand marks by asset path, and
/// the rows' ⋯ menus drawn by the IDE itself.
struct McpHost {
    ide: crate::exponential_ui_host::IdePlugin,
    state: Rc<RefCell<HostState>>,
}

impl HostPlugin for McpHost {
    fn icon(&self, name: &str) -> Option<SharedString> {
        match name.strip_prefix(ASSET) {
            Some(path) => Some(SharedString::from(path.to_string())),
            None => self.ide.icon(name),
        }
    }

    fn on_unknown(&self, node: &eui_core::surface::PlacedNode) {
        self.ide.on_unknown(node)
    }

    fn host_leaf_height(&self, component_id: &str, _width: f32, _window: &mut Window, _cx: &mut App) -> Option<f32> {
        let key = component_id.strip_prefix(MENU_NODE)?.strip_prefix('.')?;
        self.state.borrow().menus.contains_key(key).then_some(CTL_MD_H)
    }

    fn host_leaf(&self, component_id: &str, _window: &mut Window, cx: &mut App) -> Option<AnyElement> {
        let key = component_id.strip_prefix(MENU_NODE)?.strip_prefix('.')?;
        let state = self.state.borrow();
        let row = state.menus.get(key)?;
        let pane = state.pane.as_ref()?.upgrade()?;
        Some(row_menu(row, pane, cx).into_any_element())
    }
}

/// EXP-862 / EXP-1285 (web parity): everything a person can DO to a row
/// sits behind ONE ghost "..." menu — the connection items (Test, Replace
/// key, Share) with the owner's Edit, then the destructive Disconnect /
/// Remove below the separator.
// drop with EXP-1629-group4: the per-row menu stays the IDE's own (a
// `host_leaf` Embed) until the catalog Menu carries icons, a separator and
// danger items.
fn row_menu(row: &MenuRow, pane: Entity<McpServersPane>, cx: &App) -> impl IntoElement {
    let config = &row.entry.config;
    let (connected, owner, pending) = (row.connected, row.owner, row.pending);
    let http = config.is_http();
    let secret = config.auth == "secret";
    let shared = row.entry.shared;
    let target = config.clone();
    // VAPP-239: the device it runs on, for the dialog's Runs on row.
    let device = row.entry.device.clone();
    let team = config.team_id.clone();
    crate::controls::ghost_icon_button(
        SharedString::from(format!("mcp-menu-{}", config.id)),
        Icon::new(registry::UI_MORE),
        cx,
    )
    .tooltip(SharedString::from(format!("Server menu for {}", config.name)))
    .loading(connected && pending)
    .disabled(row.busy || pending)
    .pointer_menu(move |menu, _window, cx| {
        let mut menu = menu;
        if connected && http {
            let (target, pane) = (target.clone(), pane.clone());
            menu = menu.item(
                crate::controls::pointer_label_item("Test connection", false)
                    .icon(Icon::new(registry::UI_REFRESH))
                    .on_click(move |_, window, cx| {
                        let target = target.clone();
                        pane.update(cx, |this, cx| this.test(&target, window, cx));
                    }),
            );
        }
        if connected && secret {
            let (target, pane) = (target.clone(), pane.clone());
            menu = menu.item(
                crate::controls::pointer_label_item("Replace key", false)
                    .icon(Icon::new(registry::UI_PRIVATE))
                    .on_click(move |_, window, cx| {
                        let target = target.clone();
                        pane.update(cx, |this, cx| this.open_key_dialog(&target, window, cx));
                    }),
            );
        }
        // FEED-73: share the person's own connection with the team. A
        // stdio row only offers the undo of an existing share.
        if connected && share_menu_offered(http, shared) {
            let (target, pane) = (target.clone(), pane.clone());
            menu = menu.item(
                crate::controls::pointer_label_item(share_menu_label(shared), false)
                    .icon(Icon::new(registry::UI_SHARE))
                    .on_click(move |_, window, cx| {
                        let target = target.clone();
                        pane.update(cx, |this, cx| this.toggle_shared(&target, !shared, window, cx));
                    }),
            );
        }
        if owner {
            let (edit, team, pane, device) = (target.clone(), team.clone(), pane.clone(), device.clone());
            menu = menu.item(
                crate::controls::pointer_label_item("Edit", false)
                    .icon(Icon::new(registry::UI_EDIT))
                    .on_click(move |_, window, cx| {
                        let (edit, team, device) = (edit.clone(), team.clone(), device.clone());
                        pane.update(cx, |this, cx| this.open_editor(Some(edit), device, &team, window, cx));
                    }),
            );
        }
        menu = menu.separator();
        // Destructive rows wear the danger tint (web
        // `DropdownMenuItem variant="destructive"`).
        if connected {
            let (target, pane) = (target.clone(), pane.clone());
            menu = menu.item(
                crate::controls::danger_menu_item("Disconnect", Icon::new(registry::UI_CLEAR), cx)
                    .on_click(move |_, window, cx| {
                        let target = target.clone();
                        pane.update(cx, |this, cx| this.confirm_disconnect(&target, window, cx));
                    }),
            );
        }
        if owner {
            let (remove, pane) = (target.clone(), pane.clone());
            menu = menu.item(
                crate::controls::danger_menu_item("Remove", Icon::new(registry::UI_DELETE), cx)
                    .on_click(move |_, window, cx| {
                        let remove = remove.clone();
                        pane.update(cx, |this, cx| this.confirm_remove(&remove, window, cx));
                    }),
            );
        }
        menu
    })
}

/// The colours the document needs beyond [`Ink`].
struct Tints {
    success: String,
    warning: String,
    skeleton: String,
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

/// One server row (`flat_row`, `px_3` / `py_2`, `gap_3`): the server's glyph,
/// its name with the Default pill over the caption, the action, the ⋯.
fn server_row(ink: &Ink, tints: &Tints) -> Value {
    // The glyph is a fixed 14px (`Icon::size(px(14.))`), not a rem.
    let glyph = 14.;
    let mut name = ink.text("row_name", path("name"), SM, &ink.foreground);
    name["style"]["fontWeight"] = json!(500);
    name["style"]["minWidth"] = json!(0);
    let mut chip = ink.pill("row_default", json!("Default"), false, None);
    chip["visible"] = path("isDefault");
    let mut detail = ink.text("row_detail", path("detail"), XS, &ink.muted);
    detail["visible"] = path("hasDetail");
    detail["props"]["lines"] = json!(1);
    detail["style"]["minWidth"] = json!(0);
    let menu = json!({"id": MENU_NODE, "component": "Embed", "visible": path("menu"), "props": {"surface": path("menuSurface")}, "style": {"width": CTL_MD_H, "height": CTL_MD_H, "flexShrink": 0}});
    json!({"id": "row", "component": "Box",
        "style": {"display": "flex", "flexDirection": "row", "alignItems": "center", "width": "100%", "minWidth": 0, "gap": ink.r(0.75), "paddingHorizontal": ink.r(0.75), "paddingVertical": ink.r(0.5), "borderRadius": ink.radius_md},
        "children": [
            {"id": "row_icon", "component": "Icon", "props": {"name": path("icon")}, "style": {"width": glyph, "height": glyph, "color": ink.muted, "flexShrink": 0}},
            {"id": "row_text", "component": "Box", "style": {"display": "flex", "flexDirection": "column", "gap": ink.r(0.25), "flexGrow": 1, "flexShrink": 1, "flexBasis": 0, "minWidth": 0},
                "children": [
                    {"id": "row_head", "component": "Box", "style": {"display": "flex", "flexDirection": "row", "alignItems": "center", "gap": ink.r(0.5), "width": "100%", "minWidth": 0}, "children": [name, chip]},
                    detail,
                ]},
            action_slot(ink, tints),
            menu,
        ]})
}

/// The pane's tree: the band (the owner's Add server pill, the reload
/// spinner) over the skeleton, the failed and empty states or the rows;
/// no team = its empty state alone.
fn document(ink: &Ink, tints: &Tints) -> Value {
    let mut add = ink.pill_button("add", Some("ui-add"), json!("Add server"), path("/pane/busy"), json!({"name": "add"}));
    // gpui centres the label's line half a point lower than the flex centre.
    for face in &mut add {
        face["children"][1]["style"]["paddingTop"] = json!(1);
    }
    let add = json!({"id": "add_slot", "component": "Box", "visible": path("/pane/owner"), "style": {"display": "flex", "flexDirection": "row", "alignItems": "center", "flexShrink": 0}, "children": add});
    let rows = json!({"id": "rows", "component": "List", "visible": path("/pane/hasRows"), "props": {"gap": "none"},
        "template": {"component": "row_wrap", "path": "/pane/rows", "key": "key"},
        "children": [{"id": "row_wrap", "component": "Box", "style": {"display": "flex", "flexDirection": "column", "width": "100%", "minWidth": 0}, "children": [server_row(ink, tints)]}]});
    let list = json!({"id": "list", "component": "Box", "visible": not(path("/pane/noTeam")), "style": {"display": "flex", "flexDirection": "column", "gap": ink.r(0.5), "width": "100%", "minWidth": 0},
        "children": [
            ink.band_with("band", None, super::mcp::SETTINGS_MCP.labels.servers, Some(path("/pane/reloading")), Some(add)),
            ink.skeleton_rows("skeleton", &tints.skeleton, path("/pane/loading")),
            failed_state(ink, "failed", "settings-mcp", crate::empty_state_copy::MCP_SERVERS, path("/pane/failed")),
            empty_state(ink, "empty", "settings-mcp", crate::empty_state_copy::MCP_SERVERS, path("/pane/empty")),
            rows,
        ]});
    json!({"id": "root", "component": "Box", "style": {"display": "flex", "flexDirection": "column", "width": "100%"},
        "children": [list, empty_state(ink, "no_team", "ui-team", crate::empty_state_copy::NO_TEAM_SELECTED, path("/pane/noTeam"))]})
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
        let projection = self.project(team.as_deref(), owner);
        self.binding.update(cx, |binding, cx| binding.update_state(|state| *state = projection, cx));
        div().w_full().min_w_0().child(self.view.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use api::mcp_servers::{McpConnection, McpTestResult};

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
