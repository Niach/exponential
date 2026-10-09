//! Settings → MCP servers (EXP-792, desktop half EXP-807).
//!
//! Web parity: `components/team/mcp-servers-section.tsx`. The team's MCP
//! server registry lives in Exponential, and so do the credentials: each
//! member connects ONCE (an OAuth sign-in the instance runs, or a typed API
//! key it stores encrypted) and every device, remote start and automation
//! uses it. This machine holds none and runs no OAuth; the launcher asks the
//! server for a run's values at spawn (`coding::mcp_servers`).
//!
//! One list: per server the person's own connection and ONE action —
//! Connect (the web settings page's connect in the signed-in system browser,
//! then a 2 s poll of `mcpServers.list` until it reads connected), Set key, or a "Connected"
//! menu with Test / Replace key / Share with team (FEED-73) / Disconnect. Owners add, edit and remove
//! servers through [`super::mcp_server_dialog`] (EXP-810).
//!
//! `mcp_servers` is server-only (never an Electric shape), so this is a
//! fetch-on-open tRPC read like the widget pane's.

use crate::toast::Toast;
use std::time::{Duration, Instant};

use gpui::{
    div, prelude::FluentBuilder as _, App, AppContext as _, Entity, FontWeight,
    InteractiveElement as _, IntoElement, ParentElement, Render, SharedString,
    StatefulInteractiveElement as _, Styled, Subscription, Window,
};
use gpui_component::{
    button::{Button, ButtonVariant},
    h_flex,
    input::InputState,
    menu::DropdownMenu as _,
    v_flex, ActiveTheme as _, Disableable as _, Icon, Sizable as _,
};

use api::mcp_servers::{McpServerConfig, McpServerListEntry};
use sync::Store;

use crate::controls::{glass_input, WebControl as _};
use crate::icons::registry;
use crate::native_dialog::{open_alert, AlertSpec};
use crate::navigation::{active_team_id, Navigation};
use crate::queries;
use crate::surface::{glass_pill, glass_pill_button, glass_pill_button_primary, PillMode, PillSize};

use super::{error_notice, open_url, section, section_description};

/// Web copy, verbatim (the section's own description).
const MCP_DESCRIPTION: &str =
    "Tools your agents can use in runs. Each member connects their own account.";

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

/// FEED-73 — the Connected menu's share item (web `ConnectionAction`), by
/// whether the viewer's OWN connection is shared.
fn share_menu_label(shared: bool) -> &'static str {
    if shared {
        "Stop sharing"
    } else {
        "Share with team"
    }
}

/// FEED-73: whether the Connected menu carries a share item at all (web
/// `ConnectionAction`: `http || shared`). A stdio server's secret stays on
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
            format!("Shared {name} with the team: action runs on teammates' machines use your connection"),
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
    /// Connected: the menu (Test / Replace key / Disconnect).
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
        _ if entry.config.auth == "none" => RowAction::NotNeeded,
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
    /// EXP-862: the built-in Exponential tools group is COLLAPSED until it is
    /// asked for — 77 rows are a reference list, not the page.
    builtins_expanded: bool,
    _subscriptions: Vec<Subscription>,
}

impl McpServersPane {
    pub fn new(
        nav: Entity<Navigation>,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> Self {
        let subscriptions = vec![cx.observe(&nav, |_, _, cx| cx.notify())];
        let value_input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("Paste the key")
                .masked(true)
        });
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
            builtins_expanded: false,
            _subscriptions: subscriptions,
        }
    }

    /// Server read (`mcpServers.list` is tRPC — `mcp_servers` never syncs),
    /// so every entry into the section drops the cache: a connection made on
    /// the web or from the CLI has to be here when you come back.
    pub fn mark_stale(&mut self, cx: &mut gpui::Context<Self>) {
        if matches!(self.load, Load::Ready(_)) {
            self.load = Load::Idle;
        }
        cx.notify();
    }

    /// Drop the cache so the next render refetches — the dialog calls this
    /// after a create/edit lands (`mcpServers.list` is a server read with no
    /// Electric echo).
    pub(super) fn refetch(&mut self, cx: &mut gpui::Context<Self>) {
        self.load = Load::Idle;
        cx.notify();
    }

    fn ensure_loaded(&mut self, team_id: &str, cx: &mut gpui::Context<Self>) {
        if self.team_id.as_deref() != Some(team_id) {
            self.team_id = Some(team_id.to_string());
            self.load = Load::Idle;
            self.connecting = None;
            self.pending = None;
        }
        if !matches!(self.load, Load::Idle) {
            return;
        }
        let Some(trpc) = queries::trpc_client(cx) else {
            // Nothing to fetch and nothing to wait for — Idle renders the
            // loading line, so leaving it there would spin forever.
            self.load = Load::Ready(Err("Sign in to load this team's MCP servers.".to_string()));
            return;
        };
        let team = team_id.to_string();

        self.load = Load::Loading;
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
                this.load = Load::Ready(result);
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
            let note = Toast::error(format!(
                "Could not connect to {name}: open Settings → MCP servers on the web and connect it there."
            ));
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
                        let note = Toast::error(format!(
                            "{name} is still not connected. Finish the sign-in in your browser, or Connect again."
                        ));
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
            "Only your runs use it. Stored encrypted; nobody can read it back.",
            "Save key",
        )
        .height(gpui::px(300.))
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
                        Err(err) => Toast::error(format!(
                            "Could not save the key for {server}: {}",
                            err.user_message()
                        )),
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
            "Your credential for this server is deleted. Your runs stop getting its \
             tools until you connect again; other members keep theirs.",
            "Disconnect",
        )
        .height(gpui::px(250.))
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
                        Err(err) => Toast::error(format!(
                            "Could not disconnect: {}",
                            err.user_message()
                        )),
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
            "Runs stop offering it and every member's connection to it is deleted. \
             This cannot be undone.",
            "Remove",
        )
        .height(gpui::px(260.))
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
            let name = name.clone();
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
                            let note = Toast::error(format!(
                                "Could not remove {name}: {}",
                                err.user_message()
                            ));
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
    fn open_editor(
        &mut self,
        config: Option<McpServerConfig>,
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
        super::mcp_server_dialog::open(window, cx, team_id, config, pane);
    }

    // -- render pieces --------------------------------------------------------

    fn chip(&self, id: String, text: String, cx: &App) -> impl IntoElement {
        glass_pill(SharedString::from(id), PillSize::Sm, PillMode::Readonly, cx)
            .child(div().text_xs().child(SharedString::from(text)))
    }

    /// EXP-862 — the COLLAPSED "Built-in Exponential tools" group: what the
    /// agents can already do without a server in the registry above. One row
    /// per contract tool, styled like the session feed's Exponential tool row
    /// (`steer_viewer::exp_tool_call_row`): the app's own mark, the tool's
    /// title, the blurb muted beside it. The raw wire name
    /// (`exponential_issues_create`) is the row's TOOLTIP only — nobody reads
    /// a reference list in snake_case.
    fn render_builtin_tools(&self, cx: &mut gpui::Context<Self>) -> gpui::Div {
        let tools = steer::exp_tool::builtin_tools();
        let muted = cx.theme().muted_foreground;
        let collapsed = !self.builtins_expanded;
        // The foldable band WITHOUT a count (list bands carry none): the
        // leading chevron is the only fold affordance.
        let chevron = Icon::new(if collapsed {
            registry::UI_CHEVRON_RIGHT
        } else {
            registry::UI_CHEVRON_DOWN
        })
        .xsmall()
        .flex_shrink_0()
        .text_color(cx.theme().foreground.opacity(0.7))
        .into_any_element();
        let band = crate::surface::glass_section_band(
            Some(chevron),
            "Built-in Exponential tools",
            None,
            cx,
        )
        .id("mcp-builtin-tools")
        .cursor_pointer()
        .on_click(cx.listener(|this, _, _, cx| {
            this.builtins_expanded = !this.builtins_expanded;
            cx.notify();
        }));
        let mut group = v_flex().w_full().min_w_0().child(band);
        if collapsed {
            return group;
        }
        for (index, tool) in tools.into_iter().enumerate() {
            group = group.child(
                crate::surface::flat_row()
                    .id(("mcp-builtin-tool", index))
                    .flex()
                    .w_full()
                    .min_w_0()
                    .items_center()
                    .gap_2()
                    .px_3()
                    .py_1p5()
                    .tooltip({
                        let name = SharedString::from(tool.name.clone());
                        move |window, cx| {
                            gpui_component::tooltip::Tooltip::new(name.clone()).build(window, cx)
                        }
                    })
                    .child(
                        Icon::from(crate::icons::ExpIcon::Logo)
                            .xsmall()
                            .flex_shrink_0()
                            .text_color(muted),
                    )
                    .child(
                        div()
                            .flex_shrink_0()
                            .text_sm()
                            .child(SharedString::from(tool.title)),
                    )
                    .child(
                        div()
                            .min_w_0()
                            .truncate()
                            .text_xs()
                            .text_color(muted)
                            .child(SharedString::from(tool.blurb)),
                    ),
            );
        }
        group
    }

    /// The row's ONE connection control (web `ConnectionAction`).
    fn render_action(&self, entry: &McpServerListEntry, cx: &mut gpui::Context<Self>) -> gpui::AnyElement {
        let config = &entry.config;
        let muted = cx.theme().muted_foreground;
        let pending = self.pending.as_deref() == Some(config.id.as_str());
        let waiting = self.connecting.as_deref() == Some(config.id.as_str());
        if waiting {
            return h_flex()
                .gap_1()
                .items_center()
                .child(
                    glass_pill_button(
                        SharedString::from(format!("mcp-waiting-{}", config.id)),
                        PillSize::Sm,
                        cx,
                    )
                    .label("Waiting for sign-in…")
                    .loading(true)
                    .disabled(true),
                )
                .child(
                    glass_pill_button(
                        SharedString::from(format!("mcp-cancel-{}", config.id)),
                        PillSize::Sm,
                        cx,
                    )
                    .label("Cancel")
                    .on_click(cx.listener(|this, _, _, cx| this.cancel_connect(cx))),
                )
                .into_any_element();
        }
        match row_action(entry) {
            RowAction::NotNeeded => div()
                .flex_shrink_0()
                .text_xs()
                .text_color(muted)
                .child("No sign-in needed")
                .into_any_element(),
            RowAction::Connected => {
                let pane = cx.entity();
                let http = config.is_http();
                let secret = config.auth == "secret";
                let shared = entry.shared;
                let target = config.clone();
                glass_pill_button(
                    SharedString::from(format!("mcp-connected-{}", config.id)),
                    PillSize::Sm,
                    cx,
                )
                .icon(Icon::new(registry::UI_CHECK).text_color(cx.theme().success))
                .label("Connected")
                .loading(pending)
                .disabled(pending)
                .tooltip(SharedString::from(match &entry.connection.expires_at {
                    Some(expires_at) => format!("Connected to {} (token until {expires_at})", config.name),
                    None => format!("Connected to {}", config.name),
                }))
                .dropdown_menu(move |menu, _window, cx| {
                    let mut menu = menu;
                    if http {
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
                    if secret {
                        let (target, pane) = (target.clone(), pane.clone());
                        menu = menu.item(
                            crate::controls::pointer_label_item("Replace key", false)
                                .icon(Icon::new(registry::UI_EDIT))
                                .on_click(move |_, window, cx| {
                                    let target = target.clone();
                                    pane.update(cx, |this, cx| {
                                        this.open_key_dialog(&target, window, cx)
                                    });
                                }),
                        );
                    }
                    // FEED-73: share the person's own connection with the
                    // team (web: right before the separator). A stdio row
                    // only offers the undo of an existing share.
                    if share_menu_offered(http, shared) {
                        let (target, pane) = (target.clone(), pane.clone());
                        menu = menu.item(
                            crate::controls::pointer_label_item(share_menu_label(shared), false)
                                .icon(Icon::new(registry::UI_SHARE))
                                .on_click(move |_, window, cx| {
                                    let target = target.clone();
                                    pane.update(cx, |this, cx| {
                                        this.toggle_shared(&target, !shared, window, cx)
                                    });
                                }),
                        );
                    }
                    menu = menu.separator();
                    let (target, pane) = (target.clone(), pane.clone());
                    menu.item(
                        crate::controls::danger_menu_item("Disconnect", Icon::new(registry::UI_CLOSE), cx)
                            .on_click(move |_, window, cx| {
                                let target = target.clone();
                                pane.update(cx, |this, cx| {
                                    this.confirm_disconnect(&target, window, cx)
                                });
                            }),
                    )
                })
                .into_any_element()
            }
            RowAction::Reconnect => {
                let target = config.clone();
                glass_pill_button(
                    SharedString::from(format!("mcp-reconnect-{}", config.id)),
                    PillSize::Sm,
                    cx,
                )
                .icon(Icon::new(registry::UI_WARNING).text_color(cx.theme().warning))
                .label("Reconnect")
                .loading(pending)
                .disabled(pending)
                .tooltip(SharedString::from(reconnect_reason(entry)))
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.connect(&target, window, cx);
                }))
                .into_any_element()
            }
            RowAction::Connect => {
                let secret = config.auth == "secret";
                let target = config.clone();
                glass_pill_button_primary(
                    SharedString::from(format!("mcp-connect-{}", config.id)),
                    PillSize::Sm,
                )
                .icon(Icon::new(if secret { registry::UI_PERMISSION } else { registry::UI_SIGN_IN }))
                .label(if secret { "Set key" } else { "Connect" })
                .loading(pending)
                .disabled(pending)
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.connect(&target, window, cx);
                }))
                .into_any_element()
            }
        }
    }

    /// One server row: identity + chips, the endpoint and member count, the
    /// person's one connection action, and (owners) the Edit / Remove menu.
    fn render_row(
        &self,
        entry: &McpServerListEntry,
        owner: bool,
        cx: &mut gpui::Context<Self>,
    ) -> gpui::Div {
        let config = &entry.config;
        let muted = cx.theme().muted_foreground;
        let detail = row_detail(entry);

        // Web `ServerRow`: the Default pill is the row's ONLY chip.
        let mut chips = h_flex().gap_1().items_center().flex_wrap();
        if config.enabled_by_default {
            chips = chips.child(self.chip(
                format!("mcp-default-{}", config.id),
                "Default".to_string(),
                cx,
            ));
        }

        let action = self.render_action(entry, cx);

        // EXP-862 (web parity): the owner's writes sit behind ONE ghost "..."
        // menu, not two pills competing with the connection action.
        let owner_menu = owner.then(|| {
            let edit = config.clone();
            let remove = config.clone();
            let team = entry.config.team_id.clone();
            let pane = cx.entity();
            crate::controls::ghost_icon_button(
                SharedString::from(format!("mcp-menu-{}", config.id)),
                Icon::new(registry::UI_MORE),
                cx,
            )
            .tooltip(SharedString::from(format!("Server menu for {}", config.name)))
            .disabled(self.busy)
            .dropdown_menu(move |menu, _window, cx| {
                let (edit, team, pane_edit) = (edit.clone(), team.clone(), pane.clone());
                let (remove, pane_remove) = (remove.clone(), pane.clone());
                menu.item(
                    crate::controls::pointer_label_item("Edit", false)
                        .icon(Icon::new(registry::UI_EDIT))
                        .on_click(move |_, window, cx| {
                            let (edit, team) = (edit.clone(), team.clone());
                            pane_edit.update(cx, |this, cx| {
                                this.open_editor(Some(edit), &team, window, cx);
                            });
                        }),
                )
                .item(
                    // Destructive rows wear the danger tint (web
                    // `DropdownMenuItem variant="destructive"`).
                    crate::controls::danger_menu_item(
                        "Remove",
                        Icon::new(registry::UI_DELETE),
                        cx,
                    )
                        .on_click(move |_, window, cx| {
                            let remove = remove.clone();
                            pane_remove.update(cx, |this, cx| {
                                this.confirm_remove(&remove, window, cx);
                            });
                        }),
                )
            })
        });

        crate::surface::flat_row()
            .flex()
            .w_full()
            .min_w_0()
            .gap_3()
            .items_center()
            .px_3()
            .py_2()
            // EXP-792: the row wears the server's glyph (the picker's
            // `mcp_server_icon`: the brand mark of a catalog URL, a shell for
            // a command, else the plug), so the list and the composer's pick
            // read the same. A mark is a one-tint mask like any Lucide glyph.
            .child(
                Icon::from(crate::picker::mcp_server_picker::mcp_server_icon(
                    config.url.as_deref(),
                    config.command.as_deref(),
                ))
                .size(gpui::px(14.))
                .flex_shrink_0()
                .text_color(muted),
            )
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .gap_1()
                    .child(
                        h_flex()
                            .w_full()
                            .min_w_0()
                            .gap_2()
                            .items_center()
                            .child(
                                div()
                                    .min_w_0()
                                    .text_sm()
                                    .font_weight(FontWeight::MEDIUM)
                                    .child(SharedString::from(config.name.clone())),
                            )
                            .child(chips),
                    )
                    .when(!detail.is_empty(), |this| {
                        this.child(
                            div()
                                .min_w_0()
                                .text_xs()
                                .text_color(muted)
                                .truncate()
                                .child(SharedString::from(detail.clone())),
                        )
                    }),
            )
            .child(div().flex_shrink_0().child(action))
            .children(owner_menu)
    }
}

impl Render for McpServersPane {
    fn render(&mut self, _window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let Some(team_id) = active_team_id(&self.nav, cx) else {
            return v_flex().child(
                div()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child("No team selected."),
            );
        };
        self.ensure_loaded(&team_id, cx);
        let owner = super::is_owner(cx, &team_id);

        let refresh = glass_pill_button("mcp-servers-refresh", PillSize::Sm, cx)
            .label("Refresh")
            .loading(matches!(self.load, Load::Loading))
            .on_click(cx.listener(|this, _, _, cx| this.refetch(cx)));
        // EXP-810: authoring is owner-only, exactly like Remove.
        let add_team = team_id.clone();
        let trailing = h_flex()
            .gap_1()
            .items_center()
            .when(owner, |this| {
                this.child(
                    glass_pill_button("mcp-servers-add", PillSize::Sm, cx)
                        .label("Add server")
                        .disabled(self.busy)
                        .on_click(cx.listener(move |this, _, window, cx| {
                            let team = add_team.clone();
                            this.open_editor(None, &team, window, cx);
                        })),
                )
            })
            .child(refresh)
            .into_any_element();

        let mut body = section(cx).child(
            v_flex()
                .child(crate::surface::glass_section_header(
                    "MCP servers",
                    Some(trailing),
                    cx,
                ))
                .child(section_description(MCP_DESCRIPTION, cx)),
        );

        let muted = cx.theme().muted_foreground;
        match &self.load {
            Load::Idle | Load::Loading => {
                body = body.child(
                    v_flex()
                        .gap_2()
                        .child(crate::controls::skeleton().h_4().w_full())
                        .child(crate::controls::skeleton().h_4().w_full())
                        .child(crate::controls::skeleton().h_4().w_64()),
                );
            }
            Load::Ready(Err(message)) => {
                body = body.child(error_notice(SharedString::from(message.clone()), cx));
            }
            Load::Ready(Ok(servers)) if servers.is_empty() => {
                body = body.child(div().text_sm().text_color(muted).child(if owner {
                    "No MCP servers yet. Add one to offer it in new runs."
                } else {
                    "No MCP servers yet. A team owner can add them."
                }));
            }
            Load::Ready(Ok(servers)) => {
                // Cloned so the row builder can take `&mut Context` (the
                // listeners it hangs need it) without holding `self.load`.
                let servers = servers.clone();
                // EXP-1076: the team registry is an entity LIST — a gapless
                // hairline ladder (`list_row` + `flat_row`, the web
                // `SETTINGS_LIST_CLASS` twin), not gapped cards.
                let mut list = v_flex().w_full().min_w_0();
                for (index, entry) in servers.iter().enumerate() {
                    list = list.child(crate::surface::list_row(
                        self.render_row(entry, owner, cx),
                        index,
                    ));
                }
                body = body.child(list);
            }
        }

        body = body.child(self.render_builtin_tools(cx));

        // The web hand-off — the web page adds the catalog and the probe.
        let slug = super::active_team(cx, &self.nav).and_then(|team| team.slug);
        if let (Some(slug), Some(account)) = (slug, queries::active_account(cx)) {
            let url = format!(
                "{}/t/{slug}/settings/mcp-servers",
                account.instance_url.trim_end_matches('/')
            );
            body = body.child(
                h_flex().child(
                    Button::new("mcp-servers-manage")
                        .outline()
                        .web_sm()
                        .icon(registry::UI_EXTERNAL_LINK)
                        .label("Manage on the web")
                        .on_click(cx.listener(move |_, _, _, cx| {
                            open_url(cx, url.clone());
                        })),
                ),
            );
        }

        v_flex().child(body)
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

    /// FEED-73 — the Connected menu item and its toasts, web copy verbatim.
    #[test]
    fn share_copy_matches_the_web() {
        assert_eq!(share_menu_label(false), "Share with team");
        assert_eq!(share_menu_label(true), "Stop sharing");
        assert_eq!(
            share_toast("Linear", true),
            (
                "Shared Linear with the team: action runs on teammates' machines use your connection"
                    .to_string(),
                "Could not share"
            )
        );
        assert_eq!(
            share_toast("Linear", false),
            ("Stopped sharing Linear".to_string(), "Could not stop sharing")
        );
    }

    /// A stdio server's secret stays on this machine: no share item, except
    /// to undo a share made before the server refused them (web
    /// `ConnectionAction`: `http || shared`).
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
        // An older server that never sends `not_needed` for a no-auth row.
        assert_eq!(row_action(&entry("none", "not_connected")), RowAction::NotNeeded);
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
}
