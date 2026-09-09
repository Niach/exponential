//! Settings → MCP servers (EXP-792, desktop half EXP-807).
//!
//! Web parity: `components/team/mcp-servers-section.tsx`. The team's MCP
//! server registry is NON-SECRET config only (name, transport, url or
//! command, the header/env NAMES a machine must supply, scopes, the auth
//! kind) plus a per-device readiness matrix; every CREDENTIAL lives in this
//! machine's 0600 secret store and never reaches the server, this pane, or a
//! log.
//!
//! What the desktop owes that the web cannot give: the machine is HERE. A
//! `secret` value and an OAuth sign-in are typed/completed ON the device, so
//! the web's own copy sends people to "Settings › MCP servers in the desktop
//! app" ([`secretSetupHint`](../../../../../web/src/lib/mcp-servers.ts)) —
//! this is that page. The device actions run locally
//! ([`coding::mcp_servers`]): a loopback OAuth sign-in, its paste fallback for
//! a provider that refuses a loopback redirect, the typed secret, and
//! "Forget on this machine". Each one reports readiness straight after
//! ([`coding::mcp_servers::report_now`]) so the web's matrix flips without
//! waiting for the next heartbeat.
//!
//! AUTHORING stays on the web, exactly like [`super::widget`]: adding a
//! server means a transport, a URL or a command line, declared header/env
//! names and OAuth scopes — a form, not an IDE surface. Owners get the
//! destructive half here (a server they remove is one a machine keeps
//! offering otherwise) and "Manage on the web" for the rest.
//!
//! `mcp_servers` is server-only (never an Electric shape), so this is a
//! fetch-on-open tRPC read like the widget pane's — with THIS machine's
//! readiness computed alongside it from the local store rather than read back
//! off the server, which would only ever be a heartbeat stale.

use std::path::PathBuf;

use gpui::{
    div, prelude::FluentBuilder as _, App, AppContext as _, Entity, FontWeight, IntoElement,
    ParentElement, Render, SharedString, Styled, Subscription, Window,
};
use gpui_component::{
    button::{Button, ButtonVariant},
    h_flex,
    input::InputState,
    notification::Notification,
    skeleton::Skeleton,
    v_flex, ActiveTheme as _, Disableable as _, WindowExt as _,
};

use api::mcp_servers::{McpReadinessReport, McpServerConfig, McpServerListEntry};
use coding::mcp_servers::LocalLogin;

use crate::controls::{glass_input, WebControl as _};
use crate::icons::registry;
use crate::native_dialog::{open_alert, AlertSpec};
use crate::navigation::{active_team_id, Navigation};
use crate::queries;
use crate::session::AuthContext;
use crate::surface::{glass_pill, glass_pill_button, PillMode, PillSize};

use super::{error_notice, open_url, section, section_description};

/// Web copy, verbatim (the section's own description).
const MCP_DESCRIPTION: &str =
    "MCP servers your agents can connect to beside Exponential's own. The team \
     registry holds configuration only \u{2014} every credential stays on the \
     machine that runs the agent, and never reaches this server.";

/// The transport chip (web `MCP_TRANSPORT_LABELS`).
fn transport_label(transport: &str) -> &'static str {
    match transport {
        "stdio" => "Command",
        _ => "HTTP",
    }
}

/// The auth chip (web `MCP_AUTH_LABELS`).
fn auth_label(auth: &str) -> &'static str {
    match auth {
        "oauth" => "OAuth",
        "secret" => "Secret",
        _ => "No auth",
    }
}

/// One machine's answer for one server, the shape both sources share: the
/// LOCAL read ([`McpReadinessReport`]) and a synced row of the server's
/// matrix ([`api::mcp_servers::McpReadinessRow`]).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Readiness<'a> {
    pub(crate) ready: bool,
    pub(crate) expires_at: Option<&'a str>,
    pub(crate) error: Option<&'a str>,
}

impl<'a> From<&'a McpReadinessReport> for Readiness<'a> {
    fn from(entry: &'a McpReadinessReport) -> Self {
        Self {
            ready: entry.ready,
            expires_at: entry.expires_at.as_deref(),
            error: entry.error.as_deref(),
        }
    }
}

impl<'a> From<&'a api::mcp_servers::McpReadinessRow> for Readiness<'a> {
    fn from(entry: &'a api::mcp_servers::McpReadinessRow) -> Self {
        Self {
            ready: entry.ready,
            expires_at: entry.expires_at.as_deref(),
            error: entry.error.as_deref(),
        }
    }
}

/// An OAuth token already past its expiry — the device refreshes on its
/// heartbeat, so this is only ever a beat late (web `readinessExpired`).
pub(crate) fn readiness_expired(entry: Readiness<'_>, now: chrono::DateTime<chrono::Utc>) -> bool {
    let Some(raw) = entry.expires_at else {
        return false;
    };
    match chrono::DateTime::parse_from_rfc3339(raw) {
        Ok(at) => at.with_timezone(&chrono::Utc) <= now,
        Err(_) => false,
    }
}

/// Is `auth` satisfied on the machine `entry` came from? (web `serverReadyOn`
/// — a `none` server needs nothing, so every machine is ready for it.)
pub(crate) fn ready_on(
    auth: &str,
    entry: Option<Readiness<'_>>,
    now: chrono::DateTime<chrono::Utc>,
) -> bool {
    if auth == "none" {
        return true;
    }
    match entry {
        Some(entry) => entry.ready && !readiness_expired(entry, now),
        None => false,
    }
}

/// `Ready`, `Signed in until 14:05`, or the error the machine reported. A
/// missing report reads as the per-auth "nothing on this machine yet" line.
/// Web `readinessLabel`, string for string.
pub(crate) fn readiness_label(
    auth: &str,
    entry: Option<Readiness<'_>>,
    now: chrono::DateTime<chrono::Utc>,
) -> String {
    let Some(entry) = entry else {
        return match auth {
            "oauth" => "Not signed in".to_string(),
            "secret" => "Not set".to_string(),
            _ => "Not checked yet".to_string(),
        };
    };
    if entry.ready {
        if let Some(raw) = entry.expires_at {
            if readiness_expired(entry, now) {
                return "Expired".to_string();
            }
            return format!("Signed in until {}", format_until(raw, now));
        }
        return "Ready".to_string();
    }
    match entry.error.map(str::trim).filter(|error| !error.is_empty()) {
        Some(error) => error.to_string(),
        None if auth == "oauth" => "Not signed in".to_string(),
        None => "Not set".to_string(),
    }
}

/// `14:05` today, `Tue 14:05` within the week, else `12 Sep` — the web
/// `formatUntil`, rendered in the machine's LOCAL clock (the person reading
/// it is sitting at that machine).
fn format_until(iso: &str, now: chrono::DateTime<chrono::Utc>) -> String {
    let Ok(parsed) = chrono::DateTime::parse_from_rfc3339(iso) else {
        return iso.to_string();
    };
    use chrono::Local;
    let at = parsed.with_timezone(&Local);
    let local_now = now.with_timezone(&Local);
    let time = at.format("%H:%M").to_string();
    if at.date_naive() == local_now.date_naive() {
        return time;
    }
    if (at - local_now) < chrono::Duration::days(7) {
        return format!("{} {time}", at.format("%a"));
    }
    at.format("%-d %b").to_string()
}

/// The ONE declared secret position of an `auth: secret` server — the header
/// name (http) or env name (stdio) whose value this machine types. `None`
/// for a server that declares none (the server refuses to store that
/// combination, but an older row could still carry it).
fn secret_name(config: &McpServerConfig) -> Option<&str> {
    config.secret_names().first().map(String::as_str)
}

/// Unix seconds, as the readiness reader takes them.
pub(crate) fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs())
        .unwrap_or(0)
}

/// This machine's secret-store coordinates. `None` when signed out (the pane
/// is unreachable then anyway) — the fetch turns that into a notice rather
/// than an empty list, so nothing spins forever.
#[derive(Clone)]
struct DeviceCtx {
    data_dir: PathBuf,
    account_id: String,
    /// The app base the OAuth client-id document (CIMD) is served from.
    instance_url: String,
    /// The steer device id readiness is reported under — the `devices`
    /// `device_id` column, NOT the row uuid.
    device_id: String,
}

fn device_ctx(cx: &App) -> Option<DeviceCtx> {
    let data_dir = cx.try_global::<AuthContext>()?.data_dir.clone();
    let account = queries::active_account(cx)?;
    let device_id = steer::persistent_device_id(&data_dir);
    Some(DeviceCtx {
        data_dir,
        account_id: account.id,
        instance_url: account.instance_url,
        device_id,
    })
}

struct Loaded {
    servers: Vec<McpServerListEntry>,
    /// THIS machine's readiness, in `servers` order — a pure local read of
    /// the 0600 store ([`coding::mcp_servers::readiness`]), never the
    /// server's (possibly a heartbeat stale) copy of it.
    local: Vec<McpReadinessReport>,
}

impl Loaded {
    fn local_for(&self, server_id: &str) -> Option<&McpReadinessReport> {
        self.local
            .iter()
            .find(|entry| entry.server_id == server_id)
    }
}

enum Load {
    Idle,
    Loading,
    Ready(Result<Loaded, String>),
}

/// A paste-fallback sign-in waiting for the person to bring the redirect URL
/// back. Held whole (the PKCE verifier and the state live in it) — a second
/// sign-in on the same row replaces it, and finishing takes it.
struct PendingPaste {
    server_id: String,
    server_name: String,
    login: LocalLogin,
}

pub struct McpServersPane {
    nav: Entity<Navigation>,
    load: Load,
    /// The team the loaded list belongs to — a team switch must refetch.
    team_id: Option<String>,
    /// Monotonic guard: a stale in-flight fetch must not clobber a newer one.
    generation: u64,
    /// A device action or an owner write in flight — disables the row's
    /// affordances so a double click cannot start two sign-ins.
    busy: bool,
    pending_paste: Option<PendingPaste>,
    /// The "Set value" dialog's field. One input reused by every row: only
    /// one dialog is ever open.
    value_input: Entity<InputState>,
    /// The "Paste redirect URL" dialog's field, same deal.
    paste_input: Entity<InputState>,
    _subscriptions: Vec<Subscription>,
}

impl McpServersPane {
    pub fn new(
        nav: Entity<Navigation>,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> Self {
        let subscriptions = vec![cx.observe(&nav, |_, _, cx| cx.notify())];
        let value_input =
            cx.new(|cx| InputState::new(window, cx).placeholder("Paste the value"));
        let paste_input = cx.new(|cx| {
            InputState::new(window, cx).placeholder("http://127.0.0.1:1/callback?code=…")
        });
        Self {
            nav,
            load: Load::Idle,
            team_id: None,
            generation: 0,
            busy: false,
            pending_paste: None,
            value_input,
            paste_input,
            _subscriptions: subscriptions,
        }
    }

    /// Server read (`mcpServers.list` is tRPC — `mcp_servers` never syncs),
    /// so every entry into the section drops the cache: a server added on the
    /// web through "Manage on the web" has to be here when you come back.
    pub fn mark_stale(&mut self, cx: &mut gpui::Context<Self>) {
        if matches!(self.load, Load::Ready(_)) {
            self.load = Load::Idle;
        }
        cx.notify();
    }

    fn refetch(&mut self, cx: &mut gpui::Context<Self>) {
        self.load = Load::Idle;
        cx.notify();
    }

    fn ensure_loaded(&mut self, team_id: &str, cx: &mut gpui::Context<Self>) {
        if self.team_id.as_deref() != Some(team_id) {
            self.team_id = Some(team_id.to_string());
            self.load = Load::Idle;
            self.pending_paste = None;
        }
        if !matches!(self.load, Load::Idle) {
            return;
        }
        let (Some(trpc), Some(device)) = (queries::trpc_client(cx), device_ctx(cx)) else {
            // Nothing to fetch and nothing to wait for — Idle renders the
            // loading line, so leaving it there would spin forever.
            self.load = Load::Ready(Err(
                "Sign in to load this team's MCP servers.".to_string()
            ));
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
                    let servers = api::mcp_servers::list(&trpc, &team)
                        .map_err(|err| err.user_message())?;
                    let configs: Vec<McpServerConfig> =
                        servers.iter().map(|entry| entry.config.clone()).collect();
                    // Pure local reads of the secret store — this is what
                    // makes the pane's own machine authoritative about
                    // itself rather than a heartbeat behind.
                    let local = coding::mcp_servers::readiness(
                        &device.data_dir,
                        &device.account_id,
                        &configs,
                        now_secs(),
                    );
                    Ok(Loaded { servers, local })
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

    // -- device actions -------------------------------------------------------

    /// Push this machine's readiness for every server NOW, so the web's
    /// matrix (and every launch picker keyed on it) flips on the sign-in
    /// rather than on the next heartbeat. Best-effort: a failure here only
    /// costs freshness, and the local store already holds the credential.
    fn report_now(&self, cx: &mut gpui::Context<Self>) {
        let (Some(trpc), Some(device)) = (queries::trpc_client(cx), device_ctx(cx)) else {
            return;
        };
        cx.background_executor()
            .spawn(async move {
                if let Err(err) = coding::mcp_servers::report_now(
                    &device.data_dir,
                    &device.account_id,
                    &trpc,
                    &device.device_id,
                ) {
                    log::debug!("[ui] mcpServers.reportReadiness after a local change: {err}");
                }
            })
            .detach();
    }

    fn fail(&self, message: String, window: &mut Window, cx: &mut App) {
        log::warn!("[ui] mcp servers: {message}");
        window.push_notification(Notification::error(SharedString::from(message)), cx);
    }

    /// LOOPBACK sign-in: bind a listener, send the browser to the authorize
    /// URL, then wait for the code.
    ///
    /// [`coding::mcp_servers::finish_local_login`] BLOCKS on that listener
    /// for minutes — it runs on `background_executor` for exactly that
    /// reason, and running it on the foreground would freeze the window
    /// while the person is still consenting in their browser.
    fn sign_in(
        &mut self,
        config: &McpServerConfig,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(device) = device_ctx(cx) else {
            return;
        };
        let config = config.clone();
        let name = config.name.clone();
        let login = match coding::mcp_servers::begin_local_login(
            &device.data_dir,
            &device.account_id,
            &device.instance_url,
            &config,
            false,
        ) {
            Ok(login) => login,
            Err(err) => {
                self.fail(format!("Could not start the sign-in to {name}: {err}"), window, cx);
                return;
            }
        };
        open_url(cx, login.authorize_url.clone());
        self.busy = true;
        cx.notify();
        let handle = window.window_handle();
        let dir = device.data_dir.clone();
        let account_id = device.account_id.clone();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    coding::mcp_servers::finish_local_login(&dir, &account_id, login)
                        .map(|_| ())
                        .map_err(|err| err.to_string())
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.busy = false;
                match result {
                    Ok(()) => {
                        this.report_now(cx);
                        this.refetch(cx);
                    }
                    Err(err) => {
                        let note = Notification::error(SharedString::from(format!(
                            "The sign-in to {name} did not complete: {err}"
                        )));
                        let _ = handle.update(cx, |_, window, cx| {
                            window.push_notification(note, cx);
                        });
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// The paste fallback (a provider that refuses a loopback redirect): the
    /// browser lands on an unroutable `127.0.0.1:1` URL and SHOWS it, so the
    /// person copies that address back here.
    fn begin_paste_login(
        &mut self,
        config: &McpServerConfig,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        // A sign-in already waiting for ITS redirect URL is re-opened, not
        // restarted: the browser is already on the consent page, and a
        // second `begin` would mint a new state the pasted URL cannot match
        // ("that URL belongs to a different sign-in").
        if let Some(pending) = &self.pending_paste {
            if pending.server_id == config.id {
                let name = pending.server_name.clone();
                self.open_paste_dialog(name, window, cx);
                return;
            }
        }
        let Some(device) = device_ctx(cx) else {
            return;
        };
        let name = config.name.clone();
        let login = match coding::mcp_servers::begin_local_login(
            &device.data_dir,
            &device.account_id,
            &device.instance_url,
            config,
            true,
        ) {
            Ok(login) => login,
            Err(err) => {
                self.fail(format!("Could not start the sign-in to {name}: {err}"), window, cx);
                return;
            }
        };
        open_url(cx, login.authorize_url.clone());
        self.pending_paste = Some(PendingPaste {
            server_id: config.id.clone(),
            server_name: name.clone(),
            login,
        });
        self.open_paste_dialog(name, window, cx);
    }

    fn open_paste_dialog(
        &mut self,
        name: String,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        self.paste_input.update(cx, |state, cx| {
            state.set_value("", window, cx);
        });
        let pane = cx.entity().downgrade();
        let handle = window.window_handle();
        let content_input = self.paste_input.clone();
        let ok_input = self.paste_input.clone();
        let spec = AlertSpec::new(
            "Paste redirect URL",
            format!(
                "Your browser was sent to {name}. After you approve it the browser \
                 lands on an address it cannot open \u{2014} copy that whole address \
                 from the address bar and paste it here."
            ),
            "Finish sign-in",
        )
        .height(gpui::px(320.))
        .content(move |window, cx| {
            v_flex()
                .gap_1()
                .mt_2()
                .child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child("Redirect URL"),
                )
                .child(glass_input(&content_input, window, cx).web_input_sm())
                .into_any_element()
        })
        .on_ok(move |_, cx| {
            let pasted = ok_input.read(cx).value().trim().to_string();
            if pasted.is_empty() {
                // Keep the dialog open rather than silently discarding the
                // pending sign-in (the typed-confirm idiom).
                return false;
            }
            let Some(pane) = pane.upgrade() else {
                return true;
            };
            let Some(device) = device_ctx(cx) else {
                return true;
            };
            let Some(pending) = pane.update(cx, |this, cx| {
                this.busy = true;
                cx.notify();
                this.pending_paste.take()
            }) else {
                return true;
            };
            cx.spawn(async move |cx| {
                let dir = device.data_dir.clone();
                let account_id = device.account_id.clone();
                let (pending, result) = cx
                    .background_executor()
                    .spawn(async move {
                        let result = coding::mcp_servers::finish_pasted_login(
                            &dir,
                            &account_id,
                            &pending.login,
                            &pasted,
                        )
                        .map(|_| ())
                        .map_err(|err| err.to_string());
                        (pending, result)
                    })
                    .await;
                let name = pending.server_name.clone();
                let _ = pane.update(cx, |this, cx| {
                    this.busy = false;
                    match result {
                        Ok(()) => {
                            this.report_now(cx);
                            this.refetch(cx);
                        }
                        Err(err) => {
                            // The verifier is still good for another paste
                            // (a mistyped URL is the common case), so the
                            // sign-in stays pending rather than restarting.
                            this.pending_paste = Some(pending);
                            let note = Notification::error(SharedString::from(format!(
                                "That URL did not finish the sign-in to {name}: {err}"
                            )));
                            let _ = handle.update(cx, |_, window, cx| {
                                window.push_notification(note, cx);
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

    /// The typed value of an `auth: secret` server's ONE declared header/env
    /// name. It goes straight into the 0600 store — nothing echoes it, and
    /// it never reaches the server.
    fn open_value_dialog(
        &mut self,
        config: &McpServerConfig,
        name: &str,
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
        let config = config.clone();
        let field: SharedString = name.to_string().into();
        let dialog_field = field.clone();
        let server = config.name.clone();
        let spec = AlertSpec::new(
            "Set value",
            format!(
                "The value for {server}'s {name}. It is written to this machine's \
                 credential store and never sent to Exponential."
            ),
            "Save value",
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
                        .child(dialog_field.clone()),
                )
                .child(glass_input(&content_input, window, cx).web_input_sm())
                .into_any_element()
        })
        .on_ok(move |_, cx| {
            let value = ok_input.read(cx).value().trim().to_string();
            if value.is_empty() {
                return false;
            }
            let Some(pane) = pane.upgrade() else {
                return true;
            };
            let Some(device) = device_ctx(cx) else {
                return true;
            };
            let result = coding::mcp_servers::set_secret(
                &device.data_dir,
                &device.account_id,
                &config,
                field.as_ref(),
                &value,
            );
            let server = server.clone();
            match result {
                Ok(()) => {
                    pane.update(cx, |this, cx| {
                        this.report_now(cx);
                        this.refetch(cx);
                    });
                }
                Err(err) => {
                    let note = Notification::error(SharedString::from(format!(
                        "Could not store the value for {server}: {err}"
                    )));
                    let _ = handle.update(cx, |_, window, cx| {
                        window.push_notification(note, cx);
                    });
                }
            }
            true
        });
        open_alert(window, cx, spec);
    }

    /// Drop every credential this machine holds for the server (the OAuth
    /// token set or the typed value). The server row is untouched: this is
    /// "sign out here", not "remove from the team".
    fn confirm_forget(
        &mut self,
        config: &McpServerConfig,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let pane = cx.entity().downgrade();
        let server_id = config.id.clone();
        let name = config.name.clone();
        let description = match config.auth.as_str() {
            "oauth" => "This machine forgets its sign-in. Runs that pick this server \
                        here will be refused until you sign in again; other machines \
                        keep theirs.",
            _ => "This machine forgets the value you typed. Runs that pick this \
                  server here will be refused until you set it again; other machines \
                  keep theirs.",
        };
        let identity: SharedString = name.into();
        let spec = AlertSpec::new("Forget on this machine", description, "Forget")
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
                let Some(pane) = pane.upgrade() else {
                    return true;
                };
                let Some(device) = device_ctx(cx) else {
                    return true;
                };
                coding::mcp_servers::forget_server(
                    &device.data_dir,
                    &device.account_id,
                    &server_id,
                );
                pane.update(cx, |this, cx| {
                    this.report_now(cx);
                    this.refetch(cx);
                });
                true
            });
        open_alert(window, cx, spec);
    }

    /// Owner-only `mcpServers.remove`. The row goes for the whole TEAM, and
    /// with it every machine's readiness — so the confirm says so and the
    /// local credential is dropped in the same breath (nothing else would
    /// ever collect it: the store is keyed by server id).
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
            "Remove MCP server",
            "Every member loses this server, and any run that still names it \
             fails to start. Credentials on each machine are forgotten. This \
             cannot be undone.",
            "Remove server",
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
            let device = device_ctx(cx);
            pane.update(cx, |this, cx| {
                this.busy = true;
                cx.notify();
            });
            let server_id = server_id.clone();
            let name = name.clone();
            cx.spawn(async move |cx| {
                let removed = server_id.clone();
                let result = cx
                    .background_executor()
                    .spawn(async move {
                        let result = api::mcp_servers::remove(&trpc, &removed);
                        if result.is_ok() {
                            if let Some(device) = device {
                                coding::mcp_servers::forget_server(
                                    &device.data_dir,
                                    &device.account_id,
                                    &removed,
                                );
                            }
                        }
                        result
                    })
                    .await;
                let _ = pane.update(cx, |this, cx| {
                    this.busy = false;
                    match result {
                        Ok(()) => this.refetch(cx),
                        Err(err) => {
                            let note = Notification::error(SharedString::from(format!(
                                "Could not remove {name}: {}",
                                err.user_message()
                            )));
                            let _ = handle.update(cx, |_, window, cx| {
                                window.push_notification(note, cx);
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

    // -- render pieces --------------------------------------------------------

    fn chip(&self, id: String, text: String, cx: &App) -> impl IntoElement {
        glass_pill(SharedString::from(id), PillSize::Sm, PillMode::Readonly, cx)
            .child(div().text_xs().child(SharedString::from(text)))
    }

    /// One server row: identity + config chips, THIS machine's readiness
    /// line with its device actions, then one muted line per OTHER machine
    /// that reported.
    fn render_row(
        &self,
        entry: &McpServerListEntry,
        local: Option<&McpReadinessReport>,
        this_device_id: Option<&str>,
        owner: bool,
        now: chrono::DateTime<chrono::Utc>,
        cx: &mut gpui::Context<Self>,
    ) -> gpui::Div {
        let config = &entry.config;
        let muted = cx.theme().muted_foreground;
        let auth = config.auth.clone();
        let local_readiness = local.map(Readiness::from);
        let here = ready_on(&auth, local_readiness, now);
        let status = readiness_label(&auth, local_readiness, now);

        // The endpoint line: the URL for http, the command line for stdio.
        let endpoint = if config.is_http() {
            config.url.clone().unwrap_or_default()
        } else {
            let mut parts = vec![config.command.clone().unwrap_or_default()];
            parts.extend(config.args.iter().cloned());
            parts.join(" ")
        };

        let mut chips = h_flex().gap_1().items_center().flex_wrap();
        chips = chips.child(self.chip(
            format!("mcp-transport-{}", config.id),
            transport_label(&config.transport).to_string(),
            cx,
        ));
        chips = chips.child(self.chip(
            format!("mcp-auth-{}", config.id),
            auth_label(&auth).to_string(),
            cx,
        ));
        if config.enabled_by_default {
            chips = chips.child(self.chip(
                format!("mcp-default-{}", config.id),
                "On by default".to_string(),
                cx,
            ));
        }

        // The device affordances. A `none` server needs nothing here at all —
        // there is no credential to hold, so every machine is ready for it.
        let mut actions = h_flex().gap_2().items_center().flex_wrap();
        let has_actions = auth != "none";
        if auth == "oauth" {
            let sign_in = config.clone();
            actions = actions.child(
                glass_pill_button(
                    SharedString::from(format!("mcp-signin-{}", config.id)),
                    PillSize::Sm,
                    cx,
                )
                .label(if here { "Sign in again" } else { "Sign in" })
                .disabled(self.busy)
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.sign_in(&sign_in, window, cx);
                })),
            );
            let paste = config.clone();
            // A sign-in whose dialog was dismissed is still live — say so,
            // and clicking picks it back up instead of starting over.
            let pending_here = self
                .pending_paste
                .as_ref()
                .is_some_and(|pending| pending.server_id == config.id);
            actions = actions.child(
                glass_pill_button(
                    SharedString::from(format!("mcp-paste-{}", config.id)),
                    PillSize::Sm,
                    cx,
                )
                .label(if pending_here {
                    "Finish pasted sign-in"
                } else {
                    "Paste redirect URL"
                })
                .disabled(self.busy)
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.begin_paste_login(&paste, window, cx);
                })),
            );
        }
        if auth == "secret" {
            if let Some(name) = secret_name(config) {
                let target = config.clone();
                let field = name.to_string();
                actions = actions.child(
                    glass_pill_button(
                        SharedString::from(format!("mcp-value-{}", config.id)),
                        PillSize::Sm,
                        cx,
                    )
                    .label(if here { "Replace value" } else { "Set value" })
                    .disabled(self.busy)
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.open_value_dialog(&target, &field, window, cx);
                    })),
                );
            }
        }
        if has_actions && here {
            let forget = config.clone();
            actions = actions.child(
                glass_pill_button(
                    SharedString::from(format!("mcp-forget-{}", config.id)),
                    PillSize::Sm,
                    cx,
                )
                .label("Forget on this machine")
                .disabled(self.busy)
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.confirm_forget(&forget, window, cx);
                })),
            );
        }
        if owner {
            let remove = config.clone();
            actions = actions.child(
                glass_pill_button(
                    SharedString::from(format!("mcp-remove-{}", config.id)),
                    PillSize::Sm,
                    cx,
                )
                .label("Remove")
                .disabled(self.busy)
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.confirm_remove(&remove, window, cx);
                })),
            );
        }

        // Every OTHER machine that reported — the web's readiness strip. The
        // pane's own device is the line above, so it never doubles up.
        let others: Vec<SharedString> = entry
            .readiness
            .iter()
            .filter(|row| row.device_id.as_deref() != this_device_id)
            .map(|row| {
                let label = row
                    .device_label
                    .clone()
                    .filter(|label| !label.trim().is_empty())
                    .or_else(|| row.device_id.clone())
                    .unwrap_or_else(|| "Another machine".to_string());
                SharedString::from(format!(
                    "{label}: {}",
                    readiness_label(&auth, Some(Readiness::from(row)), now)
                ))
            })
            .collect();

        crate::surface::glass_row_card()
            .flex()
            .flex_col()
            .w_full()
            .min_w_0()
            .gap_1p5()
            .px_3()
            .py_2()
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
            .when(!endpoint.trim().is_empty(), |this| {
                this.child(
                    div()
                        .min_w_0()
                        .text_xs()
                        .text_color(muted)
                        .font_family(theme::terminal::FONT_FAMILY)
                        .truncate()
                        .child(SharedString::from(endpoint.clone())),
                )
            })
            .child(
                div()
                    .text_xs()
                    .text_color(if here { muted } else { cx.theme().danger })
                    .child(SharedString::from(format!("This machine: {status}"))),
            )
            .children(others.into_iter().map(|line| {
                div().text_xs().text_color(muted).child(line)
            }))
            .when(has_actions || owner, |this| this.child(actions))
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
        let this_device_id = device_ctx(cx).map(|device| device.device_id);
        let now = chrono::Utc::now();

        let refresh = glass_pill_button("mcp-servers-refresh", PillSize::Sm, cx)
            .label("Refresh")
            .loading(matches!(self.load, Load::Loading))
            .on_click(cx.listener(|this, _, _, cx| this.refetch(cx)))
            .into_any_element();

        let mut body = section(cx).child(
            v_flex()
                .child(crate::surface::glass_section_header(
                    "MCP servers",
                    Some(refresh),
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
                        .child(Skeleton::new().h_4().w_full())
                        .child(Skeleton::new().h_4().w_full())
                        .child(Skeleton::new().h_4().w_64()),
                );
            }
            Load::Ready(Err(message)) => {
                body = body.child(error_notice(SharedString::from(message.clone()), cx));
            }
            Load::Ready(Ok(loaded)) if loaded.servers.is_empty() => {
                body = body.child(div().text_sm().text_color(muted).child(
                    "No MCP servers yet. Add one on the web and it shows up on \
                     every machine in the team.",
                ));
            }
            Load::Ready(Ok(loaded)) => {
                // Cloned so the row builder can take `&mut Context` (the
                // listeners it hangs need it) without holding `self.load`.
                let servers = loaded.servers.clone();
                let local: Vec<Option<McpReadinessReport>> = servers
                    .iter()
                    .map(|entry| loaded.local_for(&entry.config.id).cloned())
                    .collect();
                let mut list = v_flex().gap_2();
                for (entry, local) in servers.iter().zip(local.iter()) {
                    list = list.child(self.render_row(
                        entry,
                        local.as_ref(),
                        this_device_id.as_deref(),
                        owner,
                        now,
                        cx,
                    ));
                }
                body = body.child(list);
            }
        }

        // The web hand-off — authoring a server is a form (transport, URL or
        // command, declared names, scopes), which is the web's job.
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

    fn at(iso: &str) -> chrono::DateTime<chrono::Utc> {
        chrono::DateTime::parse_from_rfc3339(iso)
            .unwrap()
            .with_timezone(&chrono::Utc)
    }

    fn report(ready: bool, expires_at: Option<&str>, error: Option<&str>) -> McpReadinessReport {
        McpReadinessReport {
            server_id: "s1".into(),
            ready,
            expires_at: expires_at.map(str::to_string),
            error: error.map(str::to_string),
        }
    }

    /// The web `MCP_TRANSPORT_LABELS` / `MCP_AUTH_LABELS` vocabularies, and
    /// the tolerant fallbacks a newer contract value lands on.
    #[test]
    fn chip_labels_match_the_web_vocabulary() {
        assert_eq!(transport_label("http"), "HTTP");
        assert_eq!(transport_label("stdio"), "Command");
        assert_eq!(transport_label("future"), "HTTP");
        assert_eq!(auth_label("none"), "No auth");
        assert_eq!(auth_label("oauth"), "OAuth");
        assert_eq!(auth_label("secret"), "Secret");
        assert_eq!(auth_label("future"), "No auth");
    }

    /// Web `readinessLabel`: a missing report reads per-auth, a ready one
    /// with an expiry says until when, and a refusal shows the machine's own
    /// sentence (which is where `coding::mcp_servers::sign_in_expired`'s
    /// "sign in again, then resume the run" surfaces).
    #[test]
    fn readiness_label_mirrors_the_web() {
        let now = at("2026-09-09T10:00:00Z");
        assert_eq!(readiness_label("oauth", None, now), "Not signed in");
        assert_eq!(readiness_label("secret", None, now), "Not set");
        assert_eq!(readiness_label("none", None, now), "Not checked yet");

        let ready = report(true, None, None);
        assert_eq!(readiness_label("secret", Some((&ready).into()), now), "Ready");

        let expired = report(true, Some("2026-09-09T09:00:00Z"), None);
        assert_eq!(readiness_label("oauth", Some((&expired).into()), now), "Expired");

        let refused = report(false, None, Some("not signed in on this machine"));
        assert_eq!(
            readiness_label("oauth", Some((&refused).into()), now),
            "not signed in on this machine"
        );
        // A refusal with no sentence still says something useful.
        let blank = report(false, None, Some("   "));
        assert_eq!(readiness_label("oauth", Some((&blank).into()), now), "Not signed in");
        assert_eq!(readiness_label("secret", Some((&blank).into()), now), "Not set");
    }

    /// Web `serverReadyOn`: `none` needs nothing, an expired token is not
    /// ready however the row is flagged, and no report at all is not ready.
    #[test]
    fn ready_on_follows_the_web_rule() {
        let now = at("2026-09-09T10:00:00Z");
        assert!(ready_on("none", None, now), "a no-auth server needs nothing");
        assert!(!ready_on("oauth", None, now));
        let live = report(true, Some("2026-09-09T18:00:00Z"), None);
        assert!(ready_on("oauth", Some((&live).into()), now));
        let expired = report(true, Some("2026-09-09T09:59:59Z"), None);
        assert!(!ready_on("oauth", Some((&expired).into()), now));
        // An unparseable expiry never kills a ready row.
        let odd = report(true, Some("soon"), None);
        assert!(ready_on("oauth", Some((&odd).into()), now));
    }

    /// The ONE declared position an `auth: secret` server's value is typed
    /// under follows the transport (`McpServerConfig::secret_names`).
    #[test]
    fn secret_name_follows_the_transport() {
        let http = McpServerConfig {
            header_names: vec!["X-Api-Key".into()],
            env_names: vec!["IGNORED".into()],
            ..Default::default()
        };
        assert_eq!(secret_name(&http), Some("X-Api-Key"));
        let stdio = McpServerConfig {
            transport: "stdio".into(),
            env_names: vec!["GITHUB_TOKEN".into()],
            ..Default::default()
        };
        assert_eq!(secret_name(&stdio), Some("GITHUB_TOKEN"));
        assert_eq!(secret_name(&McpServerConfig::default()), None);
    }
}
