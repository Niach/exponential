//! VAPP-185: Settings › MCP, ONE page ×4 (`packages/domain-contract/
//! fixtures/settings-mcp.json` is the spec; web `components/mcp/
//! mcp-settings-page.tsx` + `routes/t/$teamSlug/settings/mcp_.activity.tsx`).
//! Endpoint · Servers ([`super::mcp_servers`], unchanged rows + "Used by") ·
//! Clients (Mine | Everyone for owners, Revoke) · Access (one group per app
//! with the members' levels, + the Activity row) · Built-in Exponential
//! tools, last. The Activity page ([`McpActivityPane`]) is its own section.
//!
//! Everything here is a server read (tRPC `mcpClients`, `appAccess`,
//! `mcpActivity`), fetched on entry like the servers list.

use std::rc::Rc;

use eui_gpui::{binding::Binding, host::HostPlugin, view::SurfaceView};
use gpui::{div, App, AppContext as _, Entity, InteractiveElement as _, IntoElement, ParentElement, Render, SharedString, Styled, Subscription, Window};
use gpui_component::{v_flex, ActiveTheme as _, Icon};
use serde_json::{json, Value};

use api::app_access::AccessOut;
use api::mcp_clients::{McpActivityOut, McpClient, McpClientsOut};

use crate::icons::registry;
use crate::native_dialog::{open_alert, AlertSpec};
use crate::navigation::{active_team_id, Navigation};
use crate::queries;
use crate::toast::Toast;

use super::doc::{empty_state, hex, path, IdeToasts, Ink, SM, XS};
use super::mcp_servers::McpServersPane;
use super::SettingsSection;

// ---------------------------------------------------------------------------
// The fixture's copy + the pure captions (replayed by the tests below)
// ---------------------------------------------------------------------------

pub(crate) struct McpLabels {
    pub endpoint: &'static str,
    pub servers: &'static str,
    pub clients: &'static str,
    pub access: &'static str,
    pub builtin: &'static str,
}

pub(crate) struct McpCopy {
    pub title: &'static str,
    pub labels: McpLabels,
    pub copy: &'static str,
    pub copied: &'static str,
    pub endpoint_path: &'static str,
    pub revoke: &'static str,
    pub activity: &'static str,
    pub never: &'static str,
    pub builtin_app: &'static str,
    pub first_party: &'static str,
    pub used_by: &'static str,
    /// VAPP-239: a device-routed server's caption part, the dialog row's
    /// label and its no-device item.
    pub runs_on: &'static str,
    pub runs_on_label: &'static str,
    pub runs_on_server: &'static str,
    pub tool_one: &'static str,
    pub tool_other: &'static str,
}

pub(crate) const SETTINGS_MCP: McpCopy = McpCopy {
    title: "MCP",
    labels: McpLabels {
        endpoint: "Endpoint",
        servers: "Servers",
        clients: "Clients",
        access: "Access",
        builtin: "Built-in Exponential tools",
    },
    copy: "Copy",
    copied: "Copied",
    endpoint_path: "/api/mcp/apps",
    revoke: "Revoke",
    activity: "Activity",
    never: "never used",
    builtin_app: "Exponential",
    first_party: "API key",
    used_by: "Used by {apps}",
    runs_on: "Runs on {device}",
    runs_on_label: "Runs on",
    runs_on_server: "Exponential",
    tool_one: "{n} tool",
    tool_other: "{n} tools",
};

/// `sections`, in order (the page renders them in this order).
#[cfg(test)]
const SECTIONS: [&str; 5] = ["endpoint", "servers", "clients", "access", "builtin"];
/// `scopes` (value, label).
const SCOPES: [(&str, &str); 2] = [("mine", "Mine"), ("everyone", "Everyone")];
/// `levels` (value, label), the Picker's rows in order.
const LEVELS: [(&str, &str); 3] = [("none", "None"), ("read", "Read only"), ("write", "Full")];
/// `kindLabels`, `builtin` first.
const KIND_LABELS: [(&str, &str); 2] = [("builtin", "Exponential tools"), ("apps", "Apps")];
/// `accessLabels`.
const ACCESS_LABELS: [(&str, &str); 2] = [("read", "Read only"), ("full", "Full")];
/// `statusLabels`.
const STATUS_LABELS: [(&str, &str); 5] = [
    ("ok", "OK"),
    ("denied", "Denied"),
    ("invalid", "Invalid"),
    ("failed", "Failed"),
    ("upstream", "Upstream"),
];
/// `testIds`.
const TEST_IDS: [(&str, &str); 7] = [
    ("page", "settings-mcp-page"),
    ("endpoint", "settings-mcp-endpoint"),
    ("servers", "settings-mcp-servers"),
    ("clients", "settings-mcp-clients"),
    ("access", "settings-mcp-access"),
    ("builtin", "settings-mcp-builtin"),
    ("activity", "settings-mcp-activity"),
];

/// EXP-1657 — `builtinFilter` (the filter field's placeholder) and its test
/// id, the anchor every client's capture and test use.
const BUILTIN_FILTER: &str = "Filter tools";
const BUILTIN_FILTER_TEST_ID: &str = "mcp-builtin-tools-filter";

fn lookup(table: &[(&'static str, &'static str)], key: &str) -> Option<&'static str> {
    table.iter().find(|(k, _)| *k == key).map(|(_, v)| *v)
}

fn test_id(key: &str) -> &'static str {
    lookup(&TEST_IDS, key).unwrap_or("settings-mcp")
}

/// A server row's "Used by …" caption (`usedByCases`), `None` = no app
/// uses it. Sorted by name, case-insensitively (web `localeCompare` base).
pub(crate) fn used_by_caption(apps: &[String]) -> Option<String> {
    if apps.is_empty() {
        return None;
    }
    let mut sorted = apps.to_vec();
    sorted.sort_by_key(|name| name.to_lowercase());
    Some(SETTINGS_MCP.used_by.replace("{apps}", &sorted.join(", ")))
}

/// VAPP-239: a device-routed server row's "Runs on …" part
/// (`runsOnCases`), `None` = the server broker calls it.
pub(crate) fn runs_on_caption(device: Option<&str>) -> Option<String> {
    device.filter(|d| !d.is_empty()).map(|d| SETTINGS_MCP.runs_on.replace("{device}", d))
}

/// A client row's caption (`clientCaptionCases`): [member ·] kinds (apps
/// access) · last used (`None` = never).
fn client_caption(
    scope: &str,
    user_name: &str,
    kinds: &[String],
    apps_access: Option<&str>,
    last_used: Option<&str>,
) -> String {
    let kinds = KIND_LABELS
        .iter()
        .filter(|(kind, _)| kinds.iter().any(|k| k == kind))
        .map(|(kind, label)| match (*kind, apps_access.and_then(|a| lookup(&ACCESS_LABELS, a))) {
            ("apps", Some(access)) => format!("{label} ({access})"),
            _ => label.to_string(),
        })
        .collect::<Vec<_>>()
        .join(", ");
    [
        (scope == "everyone").then(|| user_name.to_string()),
        Some(kinds),
        Some(last_used.unwrap_or(SETTINGS_MCP.never).to_string()),
    ]
    .into_iter()
    .flatten()
    .filter(|part| !part.is_empty())
    .collect::<Vec<_>>()
    .join(" · ")
}

/// An activity row's caption (`activityCaptionCases`): app · [member ·]
/// client · when.
fn activity_caption(
    owner: bool,
    app_name: Option<&str>,
    user_name: &str,
    client_name: Option<&str>,
    when: &str,
) -> String {
    [
        Some(app_name.unwrap_or(SETTINGS_MCP.builtin_app)),
        owner.then_some(user_name),
        Some(client_name.unwrap_or(SETTINGS_MCP.first_party)),
        Some(when),
    ]
    .into_iter()
    .flatten()
    .filter(|part| !part.is_empty())
    .collect::<Vec<_>>()
    .join(" · ")
}

/// "1 tool" | "3 tools".
fn tool_count_label(n: u32) -> String {
    let template = if n == 1 { SETTINGS_MCP.tool_one } else { SETTINGS_MCP.tool_other };
    template.replace("{n}", &n.to_string())
}

/// The label of a policy level.
fn level_label(level: &str) -> &str {
    lookup(&LEVELS, level).unwrap_or(level)
}

/// An activity row's Pill word.
fn status_label(status: &str) -> &str {
    lookup(&STATUS_LABELS, status).unwrap_or(status)
}

/// The global apps endpoint: `<instance>/api/mcp/apps`.
fn endpoint_url(instance_url: &str) -> String {
    format!("{}{}", instance_url.trim_end_matches('/'), SETTINGS_MCP.endpoint_path)
}

/// The one level a member row shows (web: an unset app reads `write`).
fn member_level<'a>(levels: &'a std::collections::HashMap<String, String>, app_id: &str) -> &'a str {
    levels.get(app_id).map(String::as_str).unwrap_or("write")
}

/// Open the Activity page (the Access group's last row).
fn open_activity(window: &mut Window, cx: &mut App) {
    crate::sidebar::select_settings_section(window, cx, SettingsSection::McpActivity);
}

/// A row's compact relative time (`just now · 5m · 2h · 3d`).
fn when(at: &str) -> String {
    crate::inbox::relative_time(at)
}

// ---------------------------------------------------------------------------
// The page
// ---------------------------------------------------------------------------

/// A state the pictures show (`settings_bench`; headless has no team,
/// account or server).
#[cfg(test)]
pub(crate) struct McpShot {
    /// `None` = no team selected.
    pub team: Option<String>,
    pub owner: bool,
    pub endpoint: String,
    pub scope: &'static str,
    /// `None` = loading.
    pub clients: Option<McpClientsOut>,
    /// `None` = loading or failed (no section).
    pub access: Option<AccessOut>,
    /// Typed into the built-in tools filter.
    pub filter: &'static str,
    pub revoking: bool,
    /// The servers list: `None` loading, `Some(Err)` failed.
    pub servers: Option<Result<Vec<api::mcp_servers::McpServerListEntry>, String>>,
}

/// The data-model path the documents' projections are written at.
const PATH: &str = "/pane";

/// The built-in tools filter field's width (today's 192 px box).
const FILTER_W: f32 = 192.;

/// The filter Input's text field (its focus decides the box's stroke).
const FILTER_FIELD: &str = "filter.field";

/// #EXP-1629: the page is two documents on eUI's native host binding
/// around the servers pane (itself a document): the Endpoint, then the
/// Clients, Access and Built-in sections. Every read and write is as before.
pub struct McpPane {
    nav: Entity<Navigation>,
    servers: Entity<McpServersPane>,
    /// EXP-1274: the built-in tools band's inline filter, as typed, and as
    /// the field was last given it (its bound value).
    filter: String,
    filter_focused: bool,
    team_id: Option<String>,
    /// `mine` | `everyone` (owners only).
    scope: &'static str,
    /// `None` = loading (the skeleton), a failed read = no clients (web).
    clients: Option<McpClientsOut>,
    clients_generation: u64,
    /// `None` = loading or failed (the section hides, web parity).
    access: Option<AccessOut>,
    access_generation: u64,
    /// A Revoke in flight.
    revoking: bool,
    stale: bool,
    top: Entity<Binding<Value>>,
    top_view: Entity<SurfaceView>,
    body: Entity<Binding<Value>>,
    body_view: Entity<SurfaceView>,
    /// The bench's state.
    #[cfg(test)]
    shot: Option<McpShot>,
    _subscriptions: Vec<Subscription>,
}

impl McpPane {
    pub fn new(nav: Entity<Navigation>, window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        let servers = cx.new(|cx| McpServersPane::new(nav.clone(), window, cx));
        let pane = cx.weak_entity();
        let handle = window.window_handle();
        let route = move |names: &'static [&'static str]| {
            let pane = pane.clone();
            move |binding: &mut Binding<Value>| {
                for &name in names {
                    let pane = pane.clone();
                    binding.on_action(name, move |_, event, cx| {
                        let context = event.context.clone();
                        let payload = event.payload.clone().unwrap_or(Value::Null);
                        let pane = pane.clone();
                        // Deferred: the binding is mid-update.
                        cx.defer(move |cx| {
                            let _ = handle.update(cx, |_, window, cx| {
                                let _ = pane.update(cx, |pane, cx| pane.action(name, &context, &payload, window, cx));
                            });
                        });
                        None
                    });
                }
            }
        };
        let new_binding = |names: &'static [&'static str], cx: &mut gpui::Context<Self>| {
            let wire = route(names);
            cx.new(|_| {
                let mut binding = Binding::new(PATH, Value::Null, |state: &Value, _| state.clone());
                binding.set_toasts(Rc::new(IdeToasts));
                wire(&mut binding);
                binding
            })
        };
        let top = new_binding(&["copy"], cx);
        let body = new_binding(&["scope", "revoke", "level", "activity", "filter", "filterFocus", "filterBlur", "clear", "focusFilter"], cx);
        let ink = Ink::of(cx);
        let chrome = Chrome::of(cx);
        let plugin = || -> Rc<dyn HostPlugin> { Rc::new(PanePlugin { inner: super::doc::ide_plugin() }) };
        let top_view = super::doc::mount_with("settings-mcp", top_document(&ink, &chrome), &top, plugin(), true, window, cx);
        let body_view = super::doc::mount_with("settings-mcp-body", body_document(&ink, &chrome), &body, plugin(), true, window, cx);
        let subscriptions = vec![
            cx.observe(&nav, |_, _, cx| cx.notify()),
            cx.observe(&servers, |_, _, cx| cx.notify()),
        ];
        Self {
            nav,
            servers,
            filter: String::new(),
            filter_focused: false,
            team_id: None,
            scope: "mine",
            clients: None,
            clients_generation: 0,
            access: None,
            access_generation: 0,
            revoking: false,
            stale: false,
            top,
            top_view,
            body,
            body_view,
            #[cfg(test)]
            shot: None,
            _subscriptions: subscriptions,
        }
    }

    #[cfg(test)]
    pub(crate) fn for_shots(window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        Self::new(crate::navigation::nav_for_window(window, cx), window, cx)
    }

    /// The page in a pinned state.
    #[cfg(test)]
    pub(crate) fn show(&mut self, shot: McpShot, cx: &mut gpui::Context<Self>) {
        self.team_id = shot.team.clone();
        self.scope = shot.scope;
        self.clients = shot.clients.clone();
        self.access = shot.access.clone();
        self.revoking = shot.revoking;
        self.filter = shot.filter.to_string();
        let servers = super::mcp_servers::McpShot { team: shot.team.clone(), owner: shot.owner };
        let load = shot.servers.clone();
        self.servers.update(cx, |pane, cx| pane.show(servers, load, false, None, None, cx));
        self.shot = Some(shot);
        cx.notify();
    }

    /// The bench's press on the first of `ids` (`|`-separated) the body
    /// has, then the popup's enter fade run out (the headless clock stands
    /// still).
    #[cfg(test)]
    pub(crate) fn press_for_shots(&mut self, ids: &str, window: &mut Window, cx: &mut gpui::Context<Self>) {
        self.body_view.update(cx, |view, cx| {
            let index = ids.split('|').find_map(|id| view.index_of(id)).unwrap_or_else(|| panic!("none of {ids}"));
            view.press(index, window, cx);
            view.advance_clock(std::time::Duration::from_millis(500), cx);
        });
        cx.notify();
    }

    /// The pane's team (the bench's, under test).
    fn active_team(&self, cx: &App) -> Option<String> {
        #[cfg(test)]
        if let Some(shot) = &self.shot {
            return shot.team.clone();
        }
        active_team_id(&self.nav, cx)
    }

    fn owner(&self, team_id: &str, cx: &App) -> bool {
        #[cfg(test)]
        if let Some(shot) = &self.shot {
            return shot.owner;
        }
        super::is_owner(cx, team_id)
    }

    /// The global apps endpoint of the active account.
    fn endpoint(&self, cx: &App) -> String {
        #[cfg(test)]
        if let Some(shot) = &self.shot {
            return shot.endpoint.clone();
        }
        queries::active_account(cx)
            .map(|account| endpoint_url(&queries::display_instance_url(&account.instance_url)))
            .unwrap_or_else(|| SETTINGS_MCP.endpoint_path.to_string())
    }

    /// Every entry refetches (server reads: a client authorized or a level
    /// changed on the web has to show on re-entry).
    pub fn mark_stale(&mut self, cx: &mut gpui::Context<Self>) {
        self.servers.update(cx, |pane, cx| pane.mark_stale(cx));
        self.stale = true;
        cx.notify();
    }

    fn ensure_loaded(&mut self, team_id: &str, cx: &mut gpui::Context<Self>) {
        #[cfg(test)]
        if self.shot.is_some() {
            return;
        }
        if self.team_id.as_deref() != Some(team_id) {
            self.team_id = Some(team_id.to_string());
            self.scope = "mine";
            self.clients = None;
            self.access = None;
            self.stale = true;
        }
        if !std::mem::take(&mut self.stale) {
            return;
        }
        self.load_clients(cx);
        self.load_access(cx);
    }

    fn load_clients(&mut self, cx: &mut gpui::Context<Self>) {
        let (Some(team), Some(trpc)) = (self.team_id.clone(), queries::trpc_client(cx)) else {
            self.clients = Some(McpClientsOut::default());
            return;
        };
        self.clients_generation += 1;
        let generation = self.clients_generation;
        let scope = self.scope;
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { api::mcp_clients::list(&trpc, &team, scope) })
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.clients_generation != generation {
                    return;
                }
                this.clients = Some(result.unwrap_or_default());
                cx.notify();
            });
        })
        .detach();
    }

    fn load_access(&mut self, cx: &mut gpui::Context<Self>) {
        let (Some(team), Some(trpc)) = (self.team_id.clone(), queries::trpc_client(cx)) else {
            return;
        };
        self.access_generation += 1;
        let generation = self.access_generation;
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { api::app_access::list(&trpc, &team) })
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.access_generation != generation {
                    return;
                }
                this.access = result.ok();
                cx.notify();
            });
        })
        .detach();
    }

    fn set_scope(&mut self, scope: &'static str, cx: &mut gpui::Context<Self>) {
        if self.scope == scope {
            return;
        }
        self.scope = scope;
        self.clients = None;
        self.load_clients(cx);
        cx.notify();
    }

    /// A document action.
    fn action(&mut self, name: &str, context: &Value, payload: &Value, window: &mut Window, cx: &mut gpui::Context<Self>) {
        let value = payload.get("value").and_then(Value::as_str).unwrap_or_default().to_string();
        let key = |field: &str| context.get(field).and_then(Value::as_str).unwrap_or_default().to_string();
        match name {
            "copy" => {
                cx.write_to_clipboard(gpui::ClipboardItem::new_string(self.endpoint(cx)));
                crate::toast::show(Toast::success(SETTINGS_MCP.copied), window, cx);
            }
            "scope" => {
                let value = key("value");
                if let Some((scope, _)) = SCOPES.iter().find(|(scope, _)| *scope == value) {
                    self.set_scope(scope, cx);
                }
            }
            "revoke" => {
                if self.revoking {
                    return;
                }
                let index: usize = key("key").parse().unwrap_or(usize::MAX);
                if let Some(client) = self.clients.as_ref().and_then(|data| data.clients.get(index)).cloned() {
                    self.confirm_revoke(&client, window, cx);
                }
            }
            "level" => {
                if !value.is_empty() {
                    self.set_level(key("app"), key("user"), value, cx);
                }
            }
            "activity" => open_activity(window, cx),
            "filter" => {
                self.filter = value;
                cx.notify();
            }
            "filterFocus" | "filterBlur" => {
                self.filter_focused = name == "filterFocus";
                cx.notify();
            }
            "clear" => {
                // A focused field keeps its own text over a later write.
                self.body_view.update(cx, |view, cx| view.blur_field(FILTER_FIELD, window, cx));
                self.filter.clear();
                self.push(cx);
                cx.notify();
                super::doc::focus_input(self.body_view.clone(), "filter", window, cx);
            }
            "focusFilter" => super::doc::focus_input(self.body_view.clone(), "filter", window, cx),
            _ => {}
        }
    }

    /// Revoke: the prompts.json `revoke-mcp-client` prompt, then
    /// `mcpClients.revoke` (`userId` only in Everyone) and a refetch.
    fn confirm_revoke(&mut self, client: &McpClient, window: &mut Window, cx: &mut gpui::Context<Self>) {
        let Some(team_id) = self.team_id.clone() else {
            return;
        };
        let view = cx.entity().downgrade();
        let handle = window.window_handle();
        let client_id = client.client_id.clone();
        let user_id = (self.scope == "everyone").then(|| client.user_id.clone());
        let spec = AlertSpec::from_prompt(
            "Revoke client",
            &domain::prompts::revoke_mcp_client(&client.name),
        )
        .on_ok(move |_, cx| {
            let Some(trpc) = queries::trpc_client(cx) else {
                return true;
            };
            let _ = view.update(cx, |this, cx| {
                this.revoking = true;
                cx.notify();
            });
            let (view, client_id, user_id, team_id) =
                (view.clone(), client_id.clone(), user_id.clone(), team_id.clone());
            cx.spawn(async move |cx| {
                let result = cx
                    .background_executor()
                    .spawn(async move {
                        api::mcp_clients::revoke(&trpc, &client_id, &team_id, user_id.as_deref())
                    })
                    .await;
                let _ = view.update(cx, |this, cx| {
                    this.revoking = false;
                    if result.is_ok() {
                        this.load_clients(cx);
                    }
                    cx.notify();
                });
                if let Err(error) = result {
                    let note = crate::action_error::failed_toast(
                        "revoke",
                        "the client",
                        crate::action_error::action_error_description(&error),
                    );
                    let _ = handle.update(cx, |_, window, cx| crate::toast::show(note, window, cx));
                }
            })
            .detach();
            true
        });
        open_alert(window, cx, spec);
    }

    /// An owner's level pick: optimistic, rolled back on failure (web).
    fn set_level(&mut self, app_id: String, user_id: String, level: String, cx: &mut gpui::Context<Self>) {
        let Some(data) = self.access.clone() else {
            return;
        };
        if !data.can_edit {
            return;
        }
        let Some(trpc) = queries::trpc_client(cx) else {
            return;
        };
        let mut next = data.clone();
        for row in next.rows.iter_mut().filter(|row| row.user_id == user_id) {
            row.levels.insert(app_id.clone(), level.clone());
        }
        self.access = Some(next);
        cx.notify();
        let generation = self.access_generation;
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { api::app_access::set(&trpc, &app_id, &user_id, &level) })
                .await;
            if let Err(error) = result {
                let _ = this.update(cx, |this, cx| {
                    if this.access_generation == generation {
                        this.access = Some(data);
                    }
                    crate::action_error::failed_action_err("save", "the access", &error, cx);
                    cx.notify();
                });
            }
        })
        .detach();
    }

    // -- projections ----------------------------------------------------------

    /// The Clients section's state.
    fn project_clients(&self, owner: bool, cx: &mut App) -> Value {
        let rows: Vec<Value> = self
            .clients
            .iter()
            .flat_map(|data| data.clients.iter().enumerate())
            .map(|(index, client)| {
                let last_used = client.last_used_at.as_deref().map(when);
                let caption = client_caption(self.scope, &client.user_name, &client.kinds, client.apps_access.as_deref(), last_used.as_deref());
                // The client's picture once the avatar cache has it (today's
                // `cached_avatar_image`), else the MCP concept.
                let picture = crate::user_avatar::cached_avatar_image(cx, client.icon.as_deref()).and(client.icon.clone());
                json!({"key": index.to_string(), "name": client.name, "caption": caption, "hasPicture": picture.is_some(), "noPicture": picture.is_none(), "picture": picture.unwrap_or_default()})
            })
            .collect();
        let loading = self.clients.is_none();
        let empty = !loading && rows.is_empty();
        json!({"scopes": owner, "scope": self.scope, "loading": loading, "empty": empty, "hasRows": !rows.is_empty(), "rows": rows, "revoking": self.revoking, "idle": !self.revoking})
    }

    /// The Access section's state: one flat list of app headers and member
    /// rows (each app's group, `gap_3` between groups), then the Activity row.
    fn project_access(&self, cx: &mut App) -> Value {
        let Some(data) = self.access.as_ref().filter(|data| !data.apps.is_empty()) else {
            return json!({"shown": false, "rows": []});
        };
        let foreground = hex(cx.theme().foreground);
        let mut rows = Vec::new();
        for (app_index, app) in data.apps.iter().enumerate() {
            let tint = app.color.as_deref().and_then(super::parse_hex_color).map(hex).unwrap_or_else(|| foreground.clone());
            rows.push(json!({"key": format!("app:{}", app.id), "header": true, "member": false, "gap": app_index > 0,
                "icon": app_icon_name(app.icon.as_deref()), "tint": tint, "name": app.name, "count": tool_count_label(app.tool_count)}));
            for row in &data.rows {
                let name = if row.name.trim().is_empty() { row.email.clone() } else { row.name.clone() };
                let level = member_level(&row.levels, &app.id).to_string();
                let (fill, ink) = crate::user_avatar::fallback_colors(&row.user_id);
                let picture = crate::user_avatar::cached_avatar_image(cx, row.image.as_deref()).and(row.image.clone());
                rows.push(json!({"key": format!("{}:{}", app.id, row.user_id), "header": false, "member": true, "gap": false,
                    "app": app.id, "user": row.user_id, "name": name, "initials": crate::user_avatar::initials(&name),
                    "avatarFill": hex(fill), "avatarInk": hex(ink), "hasPicture": picture.is_some(), "noPicture": picture.is_none(), "picture": picture.unwrap_or_default(),
                    "level": level, "levelLabel": level_label(&level), "editable": data.can_edit, "reader": !data.can_edit,
                    // The single-select Picker's check on the current level.
                    "levels": LEVELS.iter().map(|(value, label)| json!({"label": label, "value": value, "kind": "checkbox", "checked": *value == level})).collect::<Vec<_>>()}));
            }
        }
        json!({"shown": true, "rows": rows})
    }

    /// The built-in tools: per contract band, shown + its rows, and which
    /// band carries the filter (the first shown one).
    fn project_builtin(&self) -> Value {
        let bands = steer::exp_tool::builtin_tool_bands(&self.filter);
        let first = bands.first().map(|band| band.id);
        let mut out = serde_json::Map::new();
        for band in steer::exp_tool::builtin_tool_bands("") {
            let hit = bands.iter().find(|shown| shown.id == band.id);
            let rows: Vec<Value> = hit
                .map(|band| band.tools.iter().map(|tool| json!({"key": tool.name, "name": tool.name, "title": tool.title, "blurb": tool.blurb})).collect())
                .unwrap_or_default();
            out.insert(band.id.to_string(), json!({"shown": hit.is_some(), "first": first == Some(band.id), "rows": rows}));
        }
        json!({"bands": out, "none": bands.is_empty()})
    }

    /// Write both documents' projections now.
    fn push(&mut self, cx: &mut gpui::Context<Self>) {
        let team = self.active_team(cx);
        let owner = team.as_deref().is_some_and(|team| self.owner(team, cx));
        let top = json!({"noTeam": team.is_none(), "hasTeam": team.is_some(), "url": self.endpoint(cx)});
        let clients = self.project_clients(owner, cx);
        let body = json!({
            "hasTeam": team.is_some(),
            "clients": clients,
            "access": self.project_access(cx),
            "builtin": self.project_builtin(),
            "filter": self.filter,
            "hasFilter": !self.filter.is_empty(),
            "filterFocused": self.filter_focused,
            "filterIdle": !self.filter_focused,
        });
        self.top.update(cx, |binding, cx| binding.update_state(|state| *state = top, cx));
        self.body.update(cx, |binding, cx| binding.update_state(|state| *state = body, cx));
    }
}

impl Render for McpPane {
    fn render(&mut self, _window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let _span = tracing::trace_span!("render", view = "McpPane").entered();
        let team = self.active_team(cx);
        if let Some(team_id) = &team {
            self.ensure_loaded(team_id, cx);
        }
        self.push(cx);
        // The bench's popup: the headless clock stands still, so its enter
        // fade is run out by hand.
        #[cfg(test)]
        if self.shot.is_some() && !self.body_view.read(cx).layers().is_empty() {
            self.body_view.update(cx, |view, cx| view.advance_clock(std::time::Duration::from_millis(500), cx));
        }
        let page = v_flex().id(test_id("page")).w_full().min_w_0().gap_6().child(self.top_view.clone());
        if team.is_none() {
            return page;
        }
        page.child(div().id(test_id("servers")).child(self.servers.clone())).child(self.body_view.clone())
    }
}

/// `icons::app_icon`'s name: a curated board icon, else the Apps glyph.
fn app_icon_name(icon: Option<&str>) -> String {
    icon.filter(|name| registry::PICKABLE_ICONS.contains(name) && registry::icon_by_name(name).is_some())
        .map_or_else(|| "nav-apps".to_string(), str::to_string)
}

/// The built-in rows' mark and the Assistant's: the IDE's own logo.
const LOGO: &str = "brand-exponential";

/// The pane's plugin: the IDE's, and the Exponential mark of the built-in
/// rows (`ExpIcon::Logo`, as today).
struct PanePlugin {
    inner: crate::exponential_ui_host::IdePlugin,
}

impl HostPlugin for PanePlugin {
    fn icon(&self, name: &str) -> Option<SharedString> {
        self.inner.icon(name)
    }

    fn icon_element(&self, name: &str, size: f32, color: gpui::Hsla, cx: &App) -> Option<gpui::AnyElement> {
        if name == LOGO {
            return Some(Icon::from(crate::icons::ExpIcon::Logo).size(gpui::px(size)).text_color(color).into_any_element());
        }
        self.inner.icon_element(name, size, color, cx)
    }

    fn on_unknown(&self, node: &eui_core::surface::PlacedNode) {
        self.inner.on_unknown(node)
    }
}

/// The colours the documents need beyond [`Ink`].
struct Chrome {
    list_hover: String,
    skeleton: String,
    /// The segmented capsule (`controls::segmented`).
    stroke_section: String,
    stroke_active: String,
    /// `glass_input`'s fill and stroke, and its focus stroke.
    field_fill: String,
    field_stroke: String,
    field_active: String,
    /// A ghost `Button`'s hover wash (the search's clear).
    ghost_hover: String,
    radius: f32,
    /// The segmented capsule's fill (`controls::segmented`).
    fill_section: String,
    /// An outline `Button` (gpui-component's default variant, outlined):
    /// its fill, stroke, hover fill and ink, and the disabled face.
    outline_fill: String,
    outline_stroke: String,
    outline_hover: String,
    outline_fg: String,
    outline_fg_off: String,
}

impl Chrome {
    fn of(cx: &App) -> Self {
        let theme = cx.theme();
        let chrome = crate::controls::ide_chrome();
        Self {
            list_hover: hex(theme.list_hover),
            skeleton: hex(theme.muted_foreground.opacity(0.18)),
            stroke_section: hex(chrome.stroke_section),
            stroke_active: hex(chrome.stroke_active),
            field_fill: hex(theme.input_background()),
            field_stroke: hex(theme.input),
            field_active: hex(chrome.stroke_active),
            ghost_hover: premultiplied(theme.accent.opacity(0.5)),
            radius: f32::from(theme.radius),
            fill_section: hex(chrome.fill_section),
            outline_fill: hex(theme.input_background()),
            outline_stroke: hex(theme.input),
            outline_hover: hex(gpui_component::Colorize::mix_oklab(&theme.input, theme.transparent, 0.5)),
            outline_fg: hex(theme.button_foreground),
            outline_fg_off: hex(theme.muted_foreground.opacity(0.5)),
        }
    }
}

/// An outline `web_sm` `Button` (32 px, `px 12`, a 1 px stroke, the theme
/// radius, `text_sm` on a line of 1); `off` = its disabled face.
fn outline_button(ink: &Ink, chrome: &Chrome, id: &str, label: &str, off: bool, event: Value) -> Value {
    let size = ink.r(SM);
    // Disabled: the face at half (one layer at half opacity: a `#rrggbbaa`
    // alpha byte can't say the half of a faint fill).
    let (fill, stroke, fg) = if off { ("#00000000", "#00000000", &chrome.outline_fg_off) } else { (chrome.outline_fill.as_str(), chrome.outline_stroke.as_str(), &chrome.outline_fg) };
    let mut button = json!({"id": id, "component": "Box", "accessibility": {"label": label},
        "style": {"display": "flex", "flexDirection": "row", "alignItems": "center", "justifyContent": "center", "flexShrink": 0, "height": 32, "paddingHorizontal": 12, "borderRadius": chrome.radius, "borderWidth": 1, "borderColor": stroke, "backgroundColor": fill},
        "children": [{"id": format!("{id}_text"), "component": "Text", "props": {"text": label}, "style": {"fontSize": size, "lineHeight": size, "color": fg, "flexShrink": 0}}]});
    if off {
        button["style"]["position"] = json!("relative");
        button["children"].as_array_mut().expect("the button's children").insert(0, json!({"id": format!("{id}_face"), "component": "Box",
            "style": {"position": "absolute", "top": -1, "left": -1, "right": -1, "bottom": -1, "borderRadius": chrome.radius, "borderWidth": 1, "borderColor": chrome.outline_stroke, "backgroundColor": chrome.outline_fill, "opacity": 0.5}}));
    } else {
        button["props"] = json!({"pressable": true});
        button["on"] = json!({"press": {"event": event}});
        button["style"]["cursor"] = json!("pointer");
        button["style"][":hover"] = json!({"backgroundColor": chrome.outline_hover});
    }
    button
}

/// One `controls::segmented_item` (`flex_none`, `px_3`, `text_sm`, a 1 px
/// capsule stroke): the active one in the glass active fill and stroke, the
/// others muted. Drawn twice, `active` picks.
fn segment(ink: &Ink, chrome: &Chrome, value: &str, label: &str, active: Value) -> [Value; 2] {
    let draw = |on: bool| {
        let id = format!("scope_{value}{}", if on { "_on" } else { "" });
        let mut node = json!({"id": id, "component": "Box", "visible": if on { active.clone() } else { super::doc::not(active.clone()) },
            "style": {"display": "flex", "flexDirection": "row", "alignItems": "center", "justifyContent": "center", "flexShrink": 0, "height": "100%", "gap": ink.r(0.25), "paddingHorizontal": ink.r(0.75), "borderRadius": 9999, "borderWidth": 1, "borderColor": if on { chrome.stroke_active.as_str() } else { "#00000000" }, "cursor": "pointer"},
            "children": [ink.text(&format!("{id}_label"), json!(label), SM, if on { &ink.foreground } else { &ink.muted })]});
        if on {
            node["style"]["backgroundColor"] = json!(ink.fill_active);
        } else {
            node["props"] = json!({"pressable": true});
            node["on"] = json!({"press": {"event": {"name": "scope", "context": {"value": value}}}});
            node["style"][":hover"] = json!({"color": ink.foreground});
        }
        node
    };
    [draw(false), draw(true)]
}

/// `#rrggbbaa` of a colour gpui paints over the pane premultiplied.
fn premultiplied(color: gpui::Hsla) -> String {
    let c = color.to_rgb();
    let byte = |v: f32| (v.clamp(0., 1.) * 255.).round() as u8;
    format!("#{:02x}{:02x}{:02x}{:02x}", byte(c.r * c.a), byte(c.g * c.a), byte(c.b * c.a), byte(c.a))
}

/// The real height of a client or call row (#EXP-1788: the skeleton is the
/// row's own height): the `text_sm` name over the `text_xs` caption in
/// gpui's line height, `py_2` (web's 52 at its 16 px rem).
fn two_line_row_h(ink: &Ink) -> f32 {
    ink.r(SM) * super::doc::LINE + ink.r(XS) * super::doc::LINE + 2. * ink.r(0.5)
}

/// A flat row's frame (`surface::flat_row`, `px_3` / `py_2`, `gap_3`).
fn row_style(ink: &Ink) -> Value {
    json!({"display": "flex", "flexDirection": "row", "alignItems": "center", "width": "100%", "minWidth": 0, "gap": ink.r(0.75), "paddingHorizontal": ink.r(0.75), "paddingVertical": ink.r(0.5), "borderRadius": ink.radius_md})
}

/// One run on one line, truncating.
fn line(ink: &Ink, id: &str, text: Value, rems: f32, color: &str) -> Value {
    let mut node = ink.text(id, text, rems, color);
    node["props"]["lines"] = json!(1);
    node["style"]["minWidth"] = json!(0);
    node["style"]["flexShrink"] = json!(1);
    node
}

/// An `Icon` at `size`.
fn glyph(id: &str, name: Value, size: f32, color: &str) -> Value {
    json!({"id": id, "component": "Icon", "props": {"name": name}, "style": {"width": size, "height": size, "color": color, "flexShrink": 0}})
}

/// The name over its muted caption (`flex_1`).
fn name_caption(ink: &Ink, id: &str, name: Value, caption: Value, mono: bool) -> Value {
    let mut name = line(ink, &format!("{id}_name"), name, SM, &ink.foreground);
    if mono {
        name["style"]["fontFamily"] = json!(ink.mono);
    } else {
        name["style"]["fontWeight"] = json!(500);
    }
    let caption = line(ink, &format!("{id}_caption"), caption, XS, &ink.muted);
    json!({"id": id, "component": "Box", "style": {"display": "flex", "flexDirection": "column", "flexGrow": 1, "flexShrink": 1, "flexBasis": 0, "minWidth": 0}, "children": [name, caption]})
}

/// A band over its content (`section`: `gap_2`).
fn section_box(ink: &Ink, id: &str, visible: Value, children: Vec<Value>) -> Value {
    json!({"id": id, "component": "Box", "visible": visible, "style": {"display": "flex", "flexDirection": "column", "gap": ink.r(0.5), "width": "100%", "minWidth": 0}, "children": children})
}

/// A list of `row` over `data` (its template's own wrapper).
fn list(id: &str, visible: Value, data: &str, row: Value) -> Value {
    let wrap = format!("{id}_wrap");
    json!({"id": id, "component": "List", "visible": visible, "props": {"gap": "none"},
        "template": {"component": wrap, "path": data, "key": "key"},
        "children": [{"id": wrap, "component": "Box", "style": {"display": "flex", "flexDirection": "column", "width": "100%", "minWidth": 0}, "children": [row]}]})
}

/// The Endpoint (and the no-team state): the url in mono, the outline Copy.
fn top_document(ink: &Ink, _chrome: &Chrome) -> Value {
    let mut url = line(ink, "url", path("/pane/url"), SM, &ink.foreground);
    url["style"]["fontFamily"] = json!(ink.mono);
    url["style"]["flexGrow"] = json!(1);
    url["style"]["flexBasis"] = json!(0);
    let copy = outline_button(ink, _chrome, "copy", SETTINGS_MCP.copy, false, json!({"name": "copy"}));
    let row = json!({"id": "endpoint_row", "component": "Box", "style": row_style(ink), "children": [url, copy]});
    let endpoint = section_box(ink, "endpoint", path("/pane/hasTeam"), vec![ink.band("endpoint_band", SETTINGS_MCP.labels.endpoint, None), row]);
    json!({"id": "root", "component": "Box", "style": {"display": "flex", "flexDirection": "column", "width": "100%", "minWidth": 0},
        "children": [endpoint, empty_state(ink, "no_team", "ui-team", crate::empty_state_copy::NO_TEAM_SELECTED, path("/pane/noTeam"))]})
}

/// The search field (`controls::search_field(.., Sm)` in a 192 px box): the
/// 28 px `glass_input` frame with the glyph inside and the clear once there
/// is text; Box, Icon and a `bare` Input.
fn filter_field(ink: &Ink, chrome: &Chrome) -> Value {
    let clear_box = json!({"id": "clear", "component": "Box", "props": {"pressable": true}, "accessibility": {"label": "Clear search"}, "on": {"press": {"event": {"name": "clear"}}},
        "style": {"display": "flex", "alignItems": "center", "justifyContent": "center", "flexShrink": 0, "width": ink.r(1.25), "height": ink.r(1.25), "borderRadius": chrome.radius, "cursor": "pointer", ":hover": {"backgroundColor": chrome.ghost_hover}},
        "children": [glyph("clear_icon", json!("ui-clear"), ink.r(0.75), &ink.muted)]});
    let clear = json!({"id": "clear_tip", "component": "Tooltip", "visible": path("/pane/hasFilter"), "props": {"content": "Clear search"}, "children": [clear_box]});
    let field = json!({"id": "filter", "component": "Input", "props": {"label": BUILTIN_FILTER, "name": "filter", "variant": "bare", "placeholder": BUILTIN_FILTER, "value": path("/pane/filter")},
        "style": {"flexGrow": 1, "flexShrink": 1, "flexBasis": 0, "minWidth": 0},
        "on": {
            "change": {"event": {"name": "filter"}},
            "focus": {"event": {"name": "filterFocus"}},
            "blur": {"event": {"name": "filterBlur"}},
        }});
    let frame = |id: &str, stroke: &str, visible: Value| json!({"id": id, "component": "Box", "visible": visible,
        "style": {"position": "absolute", "top": 0, "left": 0, "right": 0, "bottom": 0, "borderRadius": ink.radius_lg, "borderWidth": 1, "borderColor": stroke, "backgroundColor": chrome.field_fill}});
    // The box sits over the first shown band's trailing slot (the header
    // keeps a slot its size), so the ONE field keeps its focus whichever
    // band carries it.
    json!({"id": "filter_box", "component": "Box", "props": {"pressable": true}, "accessibility": {"label": BUILTIN_FILTER}, "on": {"press": {"event": {"name": "focusFilter"}}},
        "style": {"position": "absolute", "top": ink.r(0.375), "right": ink.r(0.75), "display": "flex", "flexDirection": "row", "alignItems": "center", "width": FILTER_W, "height": eui_gpui::controls::SEARCH_FIELD_SM_H, "paddingHorizontal": 9, "gap": 4},
        "children": [
            frame("filter_frame", &chrome.field_stroke, path("/pane/filterIdle")),
            frame("filter_frame_on", &chrome.field_active, path("/pane/filterFocused")),
            glyph("filter_icon", json!("nav-search"), 14., &ink.muted), field, clear,
        ]})
}

/// The Clients, Access and Built-in sections (`gap_6`).
fn body_document(ink: &Ink, chrome: &Chrome) -> Value {
    // Clients: the owners' Mine | Everyone over the rows.
    // Web `SegmentedControl` (not embedded): a 32 px capsule that hugs its
    // segments (`controls::segmented`, `p 3`), at the group's leading edge.
    let segments: Vec<Value> = SCOPES.iter().flat_map(|(value, label)| segment(ink, chrome, value, label, super::doc::eq(path("/pane/clients/scope"), json!(value)))).collect();
    let scopes = json!({"id": "scopes", "component": "Box",
        "style": {"display": "flex", "flexDirection": "row", "alignItems": "center", "height": 32, "padding": 3, "borderRadius": 9999, "borderWidth": 1, "borderColor": chrome.stroke_section, "backgroundColor": chrome.fill_section},
        "children": segments});
    let scopes = json!({"id": "scopes_row", "component": "Box", "visible": path("/pane/clients/scopes"), "style": {"display": "flex", "flexDirection": "row"}, "children": [scopes]});
    let glyph16 = ink.r(1.);
    let mut client_icon = glyph("client_icon", json!("settings-mcp"), glyph16, &ink.foreground);
    client_icon["visible"] = path("noPicture");
    client_icon["style"]["opacity"] = json!(super::doc::VALUE_ALPHA);
    let client_picture = json!({"id": "client_picture", "component": "Avatar", "visible": path("hasPicture"), "props": {"name": path("name"), "src": path("picture"), "size": "sm"}, "style": {"width": glyph16, "height": glyph16, "flexShrink": 0, "borderRadius": 2}});
    let revoke = |id: &str, off: bool, visible: &str| {
        let mut button = outline_button(ink, chrome, id, SETTINGS_MCP.revoke, off, json!({"name": "revoke", "context": {"key": path("key")}}));
        button["visible"] = path(visible);
        button
    };
    let client_row = json!({"id": "client_row", "component": "Box", "style": row_style(ink),
        "children": [client_icon, client_picture, name_caption(ink, "client_text", path("name"), path("caption"), false), revoke("revoke", false, "/pane/clients/idle"), revoke("revoke_off", true, "/pane/clients/revoking")]});
    let clients = section_box(ink, "clients", json!(true), vec![
        ink.band("clients_band", SETTINGS_MCP.labels.clients, None),
        scopes,
        ink.skeleton_rows_at("clients_skeleton", &chrome.skeleton, two_line_row_h(ink), path("/pane/clients/loading")),
        empty_state(ink, "clients_empty", "settings-mcp", crate::empty_state_copy::MCP_CLIENTS, path("/pane/clients/empty")),
        list("clients_rows", path("/pane/clients/hasRows"), "/pane/clients/rows", client_row),
    ]);

    // Access: per app its header row over the members' rows, then Activity.
    let mut app_name = line(ink, "app_name", path("name"), SM, &ink.foreground);
    app_name["style"]["fontWeight"] = json!(500);
    app_name["style"]["flexGrow"] = json!(1);
    let mut count = ink.text("app_count", path("count"), XS, &ink.muted);
    count["style"]["flexShrink"] = json!(0);
    let header = json!({"id": "app_row", "component": "Box", "visible": path("header"), "style": row_style(ink),
        "children": [glyph("app_icon", path("icon"), 16., "#00000000"), app_name, count]});
    let mut header = header;
    header["children"][0]["style"]["color"] = path("tint");
    let gap = json!({"id": "app_gap", "component": "Box", "visible": path("gap"), "style": {"height": ink.r(0.75), "flexShrink": 0}});
    // `user_avatar` at `Size::XSmall`: the picture, else the initials on the
    // member's hue at 20%.
    let avatar_size = 16.;
    let mut initials = ink.text("member_initials", path("initials"), 0.65, "#00000000");
    initials["style"]["color"] = path("avatarInk");
    initials["style"]["flexShrink"] = json!(0);
    let disc = json!({"id": "member_disc", "component": "Box", "visible": path("noPicture"),
        "style": {"display": "flex", "alignItems": "center", "justifyContent": "center", "width": avatar_size, "height": avatar_size, "flexShrink": 0, "borderRadius": 9999, "overflow": "hidden", "backgroundColor": path("avatarFill")},
        "children": [initials]});
    let picture = json!({"id": "member_picture", "component": "Avatar", "visible": path("hasPicture"), "props": {"name": path("name"), "src": path("picture"), "size": "sm"}, "style": {"width": avatar_size, "height": avatar_size, "flexShrink": 0}});
    let mut member_name = line(ink, "member_name", path("name"), SM, &ink.foreground);
    member_name["style"]["flexGrow"] = json!(1);
    // An owner's level: the inline picker trigger (`text_sm` value at 70%,
    // `gap_1`, the 12 px muted chevron-down) opening the levels.
    // gpui's ghost Button sets its label half a point higher than the flex
    // centre.
    let mut level_value = super::doc::faded(ink.text("level_value", path("levelLabel"), SM, &ink.value), super::doc::VALUE_ALPHA);
    level_value["style"]["marginTop"] = json!(-1);
    let trigger = json!({"id": "level_trigger", "component": "Box", "props": {"pressable": true}, "accessibility": {"label": path("levelLabel")},
        "style": {"display": "flex", "flexDirection": "row", "alignItems": "center", "gap": ink.r(0.5), "cursor": "pointer"},
        "children": [
            level_value,
            glyph("level_chevron", json!("ui-chevron-down"), ink.r(0.75), &ink.muted),
        ]});
    let menu = json!({"id": "level_menu", "component": "Menu", "visible": path("editable"), "props": {"items": path("levels"), "align": "end"},
        "on": {"select": {"event": {"name": "level", "context": {"app": path("app"), "user": path("user")}}}}, "children": [trigger]});
    let mut reader = ink.text("level_text", path("levelLabel"), SM, &ink.muted);
    reader["visible"] = path("reader");
    reader["style"]["flexShrink"] = json!(0);
    let member = json!({"id": "member_row", "component": "Box", "visible": path("member"), "style": row_style(ink), "children": [disc, picture, member_name, menu, reader]});
    let access_row = json!({"id": "access_item", "component": "Box", "style": {"display": "flex", "flexDirection": "column", "width": "100%", "minWidth": 0}, "children": [gap, header, member]});
    let mut activity_label = ink.text("activity_label", json!(SETTINGS_MCP.activity), SM, &ink.foreground);
    activity_label["style"]["flexGrow"] = json!(1);
    activity_label["style"]["minWidth"] = json!(0);
    let mut activity_style = row_style(ink);
    activity_style["cursor"] = json!("pointer");
    let activity = json!({"id": "activity", "component": "Box", "props": {"pressable": true}, "accessibility": {"label": SETTINGS_MCP.activity}, "on": {"press": {"event": {"name": "activity"}}},
        "style": activity_style, "children": [activity_label, glyph("activity_chevron", json!("ui-chevron-right"), glyph16, &ink.muted)]});
    let groups = json!({"id": "access_groups", "component": "Box", "style": {"display": "flex", "flexDirection": "column", "gap": ink.r(0.75), "width": "100%", "minWidth": 0},
        "children": [list("access_rows", json!(true), "/pane/access/rows", access_row), activity]});
    let access = section_box(ink, "access", path("/pane/access/shown"), vec![ink.band("access_band", SETTINGS_MCP.labels.access, None), groups]);

    // Built-in: one band per contract group, the filter over the first.
    let slot = |id: String, visible: Value| json!({"id": id, "component": "Box", "visible": visible, "style": {"width": FILTER_W, "height": eui_gpui::controls::SEARCH_FIELD_SM_H, "flexShrink": 0}});
    let mut bands = Vec::new();
    for band in steer::exp_tool::builtin_tool_bands("") {
        let id = band.id;
        let data = format!("/pane/builtin/bands/{id}");
        let mut title = ink.text(&format!("tool_{id}_title"), path("title"), SM, &ink.foreground);
        title["style"]["flexShrink"] = json!(0);
        let blurb = line(ink, &format!("tool_{id}_blurb"), path("blurb"), XS, &ink.muted);
        let row = json!({"id": format!("tool_{id}"), "component": "Box",
            "style": {"display": "flex", "flexDirection": "row", "alignItems": "center", "width": "100%", "minWidth": 0, "gap": ink.r(0.5), "paddingHorizontal": ink.r(0.75), "paddingVertical": ink.r(0.375), "borderRadius": ink.radius_md},
            "children": [glyph(&format!("tool_{id}_mark"), json!(LOGO), ink.r(0.75), &ink.muted), title, blurb]});
        let row = json!({"id": format!("tool_{id}_tip"), "component": "Tooltip", "props": {"content": path("name")}, "children": [row]});
        bands.push(json!({"id": format!("band_{id}"), "component": "Box", "visible": path(&format!("{data}/shown")), "style": {"display": "flex", "flexDirection": "column", "width": "100%", "minWidth": 0},
            "children": [
                ink.band(&format!("band_{id}_header"), band.label, Some(slot(format!("band_{id}_slot"), path(&format!("{data}/first"))))),
                list(&format!("band_{id}_rows"), json!(true), &format!("{data}/rows"), row),
            ]}));
    }
    bands.push(json!({"id": "band_none", "component": "Box", "visible": path("/pane/builtin/none"), "style": {"display": "flex", "flexDirection": "column", "width": "100%", "minWidth": 0},
        "children": [
            ink.band("band_none_header", SETTINGS_MCP.labels.builtin, Some(slot("band_none_slot".into(), json!(true)))),
            empty_state(ink, "band_none_empty", "ui-empty-search", crate::empty_state_copy::BUILTIN_TOOLS_NO_MATCH, json!(true)),
        ]}));
    bands.push(filter_field(ink, chrome));
    let builtin = json!({"id": "builtin", "component": "Box", "style": {"position": "relative", "display": "flex", "flexDirection": "column", "gap": ink.r(0.75), "width": "100%", "minWidth": 0}, "children": bands});

    json!({"id": "root", "component": "Box", "visible": path("/pane/hasTeam"), "style": {"display": "flex", "flexDirection": "column", "gap": ink.r(1.5), "width": "100%", "minWidth": 0}, "children": [clients, access, builtin]})
}

// ---------------------------------------------------------------------------
// The Activity page
// ---------------------------------------------------------------------------

/// `activityPage`: the `mcp_calls` rows newest first (owners: the team's, a
/// member: their own). #EXP-1629: one document on the native binding.
pub struct McpActivityPane {
    nav: Entity<Navigation>,
    team_id: Option<String>,
    data: Option<McpActivityOut>,
    generation: u64,
    stale: bool,
    binding: Entity<Binding<Value>>,
    view: Entity<SurfaceView>,
    /// The bench's state: `Some(team)` pins it (`None` inside = no team).
    #[cfg(test)]
    shot: Option<Option<String>>,
    _subscriptions: Vec<Subscription>,
}

impl McpActivityPane {
    pub fn new(nav: Entity<Navigation>, window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        let subscriptions = vec![cx.observe(&nav, |_, _, cx| cx.notify())];
        let binding = cx.new(|_| {
            let mut binding = Binding::new(PATH, Value::Null, |state: &Value, _| state.clone());
            binding.set_toasts(Rc::new(IdeToasts));
            binding
        });
        let view = super::doc::mount("settings-mcp-activity", activity_document(&Ink::of(cx), &Chrome::of(cx)), &binding, window, cx);
        Self {
            nav,
            team_id: None,
            data: None,
            generation: 0,
            stale: false,
            binding,
            view,
            #[cfg(test)]
            shot: None,
            _subscriptions: subscriptions,
        }
    }

    #[cfg(test)]
    pub(crate) fn for_shots(window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        Self::new(crate::navigation::nav_for_window(window, cx), window, cx)
    }

    /// The page in a pinned state: `team` (`None` = no team), `data`
    /// (`None` = loading).
    #[cfg(test)]
    pub(crate) fn show(&mut self, team: Option<String>, data: Option<McpActivityOut>, cx: &mut gpui::Context<Self>) {
        self.team_id = team.clone();
        self.data = data;
        self.shot = Some(team);
        cx.notify();
    }

    /// The pane's team (the bench's, under test).
    fn active_team(&self, cx: &App) -> Option<String> {
        #[cfg(test)]
        if let Some(team) = &self.shot {
            return team.clone();
        }
        active_team_id(&self.nav, cx)
    }

    pub fn mark_stale(&mut self, cx: &mut gpui::Context<Self>) {
        self.stale = true;
        cx.notify();
    }

    fn ensure_loaded(&mut self, team_id: &str, cx: &mut gpui::Context<Self>) {
        #[cfg(test)]
        if self.shot.is_some() {
            return;
        }
        if self.team_id.as_deref() != Some(team_id) {
            self.team_id = Some(team_id.to_string());
            self.data = None;
            self.stale = true;
        }
        if !std::mem::take(&mut self.stale) {
            return;
        }
        let Some(trpc) = queries::trpc_client(cx) else {
            self.data = Some(McpActivityOut::default());
            return;
        };
        let team = team_id.to_string();
        self.generation += 1;
        let generation = self.generation;
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { api::mcp_clients::activity(&trpc, &team) })
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.generation != generation {
                    return;
                }
                this.data = Some(result.unwrap_or_default());
                cx.notify();
            });
        })
        .detach();
    }

    /// The state as the document binds it.
    fn project(&self, team: bool) -> Value {
        let rows: Vec<Value> = self
            .data
            .iter()
            .flat_map(|data| data.calls.iter().map(move |call| (data.owner, call)))
            .map(|(owner, call)| {
                let caption = activity_caption(owner, call.app_name.as_deref(), &call.user_name, call.client_name.as_deref(), &when(&call.created_at));
                json!({"key": call.id, "tool": call.tool, "caption": caption, "status": status_label(&call.status)})
            })
            .collect();
        let loading = team && self.data.is_none();
        json!({"noTeam": !team, "loading": loading, "empty": team && !loading && rows.is_empty(), "hasRows": team && !rows.is_empty(), "rows": rows})
    }
}

impl Render for McpActivityPane {
    fn render(&mut self, _window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let _span = tracing::trace_span!("render", view = "McpActivityPane").entered();
        let team = self.active_team(cx);
        if let Some(team_id) = &team {
            self.ensure_loaded(team_id, cx);
        }
        let projection = self.project(team.is_some());
        self.binding.update(cx, |binding, cx| binding.update_state(|state| *state = projection, cx));
        div().id(test_id("activity")).w_full().min_w_0().child(self.view.clone())
    }
}

/// The Activity page's tree: its band over the skeleton, the empty state,
/// the no-team state or the rows (the tool in mono over its caption, the
/// status Pill).
fn activity_document(ink: &Ink, chrome: &Chrome) -> Value {
    let pill = ink.pill("call_status", path("status"), false, None);
    let row = json!({"id": "call_row", "component": "Box", "style": row_style(ink), "children": [name_caption(ink, "call_text", path("tool"), path("caption"), true), pill]});
    section_box(ink, "root", json!(true), vec![
        ink.band("band", SETTINGS_MCP.activity, None),
        empty_state(ink, "no_team", "ui-team", crate::empty_state_copy::NO_TEAM_SELECTED, path("/pane/noTeam")),
        ink.skeleton_rows_at("skeleton", &chrome.skeleton, two_line_row_h(ink), path("/pane/loading")),
        empty_state(ink, "empty", "ui-history", crate::empty_state_copy::MCP_ACTIVITY, path("/pane/empty")),
        list("rows", path("/pane/hasRows"), "/pane/rows", row),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    fn ink() -> Ink {
        Ink {
            rem: 14.,
            primary: "#fafafaff".into(),
            foreground: "#ffffffff".into(),
            hint: "#ffffffff".into(),
            value: "#ffffffff".into(),
            muted: "#a1a1aaff".into(),
            muted_glyph: "#a1a1aaff".into(),
            fill_row: "#ffffff0a".into(),
            stroke_row: "#ffffff14".into(),
            fill_card: "#ffffff0f".into(),
            stroke_card: "#ffffff1a".into(),
            code: "#27272aff".into(),
            radius_lg: 12.,
            radius_md: 8.,
            fill_section: "#ffffff0a".into(),
            fill_active: "#ffffff1a".into(),
            mono: "$type.family.mono".into(),
            pill_button_fill: "#ffffff0f".into(),
            pill_button_fill_disabled: "#ffffff0b".into(),
            primary_button_fill: "#f4f4f5ff".into(),
            primary_button_hover: "#e4e4e7ff".into(),
            primary_button_fill_disabled: "#f4f4f580".into(),
            primary_button_fg: "#18181bff".into(),
        }
    }

    /// #EXP-1629: the documents raise no issue a debug build's mount check
    /// would refuse (only the IDE's literal chrome and the bound values).
    #[test]
    fn the_document_raises_no_unexpected_issue() {
        let chrome = Chrome {
            list_hover: "#ffffff0a".into(),
            skeleton: "#a1a1aa2e".into(),
            stroke_section: "#ffffff14".into(),
            stroke_active: "#ffffff29".into(),
            field_fill: "#ffffff0a".into(),
            field_stroke: "#ffffff26".into(),
            field_active: "#ffffff29".into(),
            ghost_hover: "#27272a80".into(),
            radius: 6.,
            fill_section: "#ffffff0a".into(),
            outline_fill: "#ffffff0d".into(),
            outline_stroke: "#ffffff26".into(),
            outline_hover: "#ffffff13".into(),
            outline_fg: "#fafafaff".into(),
            outline_fg_off: "#a1a1aa80".into(),
        };
        for tree in [top_document(&ink(), &chrome), body_document(&ink(), &chrome), activity_document(&ink(), &chrome)] {
            let bound = super::super::doc::bound_values(&tree);
            let mut surface = eui_core::surface::Surface::new("settings-mcp", Default::default());
            let outcome = surface.set_nested(serde_json::from_value(tree).expect("parses"));
            let issues = super::super::doc::unexpected_issues(&bound, &outcome.issues);
            assert!(issues.is_empty(), "{issues:?}");
        }
    }

    const FIXTURE: &str =
        include_str!("../../../../../../packages/domain-contract/fixtures/settings-mcp.json");

    fn fixture() -> Value {
        serde_json::from_str(FIXTURE).unwrap()
    }

    fn strings(value: &Value) -> Vec<String> {
        value.as_array().unwrap().iter().map(|v| v.as_str().unwrap().to_string()).collect()
    }

    fn pairs(value: &Value) -> Vec<(String, String)> {
        value
            .as_array()
            .unwrap()
            .iter()
            .map(|v| (v["value"].as_str().unwrap().to_string(), v["label"].as_str().unwrap().to_string()))
            .collect()
    }

    fn map(value: &Value) -> Vec<(String, String)> {
        let mut out: Vec<_> = value
            .as_object()
            .unwrap()
            .iter()
            .map(|(k, v)| (k.clone(), v.as_str().unwrap().to_string()))
            .collect();
        out.sort();
        out
    }

    fn owned(table: &[(&str, &str)]) -> Vec<(String, String)> {
        let mut out: Vec<_> = table.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();
        out.sort();
        out
    }

    #[test]
    fn the_copy_is_the_fixtures() {
        let f = fixture();
        let s = &SETTINGS_MCP;
        assert_eq!(f["title"], s.title);
        assert_eq!(strings(&f["sections"]), SECTIONS.map(String::from).to_vec());
        assert_eq!(f["labels"]["endpoint"], s.labels.endpoint);
        assert_eq!(f["labels"]["servers"], s.labels.servers);
        assert_eq!(f["labels"]["clients"], s.labels.clients);
        assert_eq!(f["labels"]["access"], s.labels.access);
        assert_eq!(f["labels"]["builtin"], s.labels.builtin);
        assert_eq!(f["builtinFilter"], BUILTIN_FILTER);
        assert_eq!(f["copy"], s.copy);
        assert_eq!(f["copied"], s.copied);
        assert_eq!(f["endpointPath"], s.endpoint_path);
        assert_eq!(f["revoke"], s.revoke);
        assert_eq!(f["activity"], s.activity);
        assert_eq!(f["never"], s.never);
        assert_eq!(f["builtinApp"], s.builtin_app);
        assert_eq!(f["firstParty"], s.first_party);
        assert_eq!(f["usedBy"], s.used_by);
        assert_eq!(f["toolCount"]["one"], s.tool_one);
        assert_eq!(f["toolCount"]["other"], s.tool_other);
        assert_eq!(pairs(&f["scopes"]), SCOPES.iter().map(|(a, b)| (a.to_string(), b.to_string())).collect::<Vec<_>>());
        assert_eq!(pairs(&f["levels"]), LEVELS.iter().map(|(a, b)| (a.to_string(), b.to_string())).collect::<Vec<_>>());
        assert_eq!(map(&f["kindLabels"]), owned(&KIND_LABELS));
        assert_eq!(map(&f["accessLabels"]), owned(&ACCESS_LABELS));
        assert_eq!(map(&f["statusLabels"]), owned(&STATUS_LABELS));
        assert_eq!(map(&f["testIds"]), owned(&TEST_IDS));
    }

    #[test]
    fn runs_on_cases_replay() {
        let f = fixture();
        assert_eq!(f["runsOn"], SETTINGS_MCP.runs_on);
        assert_eq!(f["runsOnLabel"], SETTINGS_MCP.runs_on_label);
        assert_eq!(f["runsOnServer"], SETTINGS_MCP.runs_on_server);
        for case in f["runsOnCases"].as_array().unwrap() {
            assert_eq!(runs_on_caption(case["device"].as_str()).as_deref(), case["expected"].as_str(), "{}", case["name"]);
        }
    }

    #[test]
    fn used_by_cases_replay() {
        for case in fixture()["usedByCases"].as_array().unwrap() {
            let apps = strings(&case["apps"]);
            assert_eq!(used_by_caption(&apps).as_deref(), case["expected"].as_str(), "{}", case["name"]);
        }
    }

    #[test]
    fn client_caption_cases_replay() {
        for case in fixture()["clientCaptionCases"].as_array().unwrap() {
            let client = &case["client"];
            let caption = client_caption(
                case["scope"].as_str().unwrap(),
                client["userName"].as_str().unwrap(),
                &strings(&client["kinds"]),
                client["appsAccess"].as_str(),
                client["lastUsed"].as_str(),
            );
            assert_eq!(caption, case["expected"].as_str().unwrap(), "{}", case["name"]);
        }
    }

    #[test]
    fn activity_caption_cases_replay() {
        for case in fixture()["activityCaptionCases"].as_array().unwrap() {
            let call = &case["call"];
            let caption = activity_caption(
                case["owner"].as_bool().unwrap(),
                call["appName"].as_str(),
                call["userName"].as_str().unwrap(),
                call["clientName"].as_str(),
                call["when"].as_str().unwrap(),
            );
            assert_eq!(caption, case["expected"].as_str().unwrap(), "{}", case["name"]);
        }
    }

    #[test]
    fn counts_levels_and_the_endpoint() {
        assert_eq!(tool_count_label(1), "1 tool");
        assert_eq!(tool_count_label(3), "3 tools");
        assert_eq!(level_label("read"), "Read only");
        assert_eq!(status_label("upstream"), "Upstream");
        assert_eq!(endpoint_url("https://app.exponential.at/"), "https://app.exponential.at/api/mcp/apps");
        assert_eq!(member_level(&Default::default(), "a-1"), "write");
    }
}
