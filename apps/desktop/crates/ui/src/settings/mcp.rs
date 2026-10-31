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

use gpui::{
    div, img, App, AppContext as _, Entity, FontWeight,
    InteractiveElement as _, IntoElement, ParentElement, Render, SharedString,
    StatefulInteractiveElement as _, Styled, Subscription, Window,
};
use gpui_component::{
    button::{Button, ButtonVariants as _}, h_flex, input::InputState, v_flex, ActiveTheme as _, Disableable as _, Icon,
    Sizable as _,
};

use api::app_access::AccessOut;
use api::mcp_clients::{McpActivityOut, McpClient, McpClientsOut};

use crate::controls::WebControl as _;
use crate::icons::registry;
use crate::native_dialog::{open_alert, AlertSpec};
use crate::navigation::{active_team_id, Navigation};
use crate::queries;
use crate::surface::{glass_pill, PillMode, PillSize};
use crate::toast::Toast;

use super::mcp_servers::{filter_builtin_tools, McpServersPane};
use super::{section, SettingsSection};

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

pub struct McpPane {
    nav: Entity<Navigation>,
    servers: Entity<McpServersPane>,
    /// EXP-1274: the built-in tools band's inline filter.
    tool_filter: Entity<InputState>,
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
    _subscriptions: Vec<Subscription>,
}

impl McpPane {
    pub fn new(nav: Entity<Navigation>, window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        let servers = cx.new(|cx| McpServersPane::new(nav.clone(), window, cx));
        let tool_filter = cx.new(|cx| InputState::new(window, cx).placeholder("Filter tools"));
        let subscriptions = vec![
            cx.observe(&nav, |_, _, cx| cx.notify()),
            cx.observe(&servers, |_, _, cx| cx.notify()),
            // Typing in the built-in tools filter re-filters its rows.
            cx.observe(&tool_filter, |_, _, cx| cx.notify()),
        ];
        Self {
            nav,
            servers,
            tool_filter,
            team_id: None,
            scope: "mine",
            clients: None,
            clients_generation: 0,
            access: None,
            access_generation: 0,
            revoking: false,
            stale: false,
            _subscriptions: subscriptions,
        }
    }

    /// Every entry refetches (server reads: a client authorized or a level
    /// changed on the web has to show on re-entry).
    pub fn mark_stale(&mut self, cx: &mut gpui::Context<Self>) {
        self.servers.update(cx, |pane, cx| pane.mark_stale(cx));
        self.stale = true;
        cx.notify();
    }

    fn ensure_loaded(&mut self, team_id: &str, cx: &mut gpui::Context<Self>) {
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

    // -- sections -------------------------------------------------------------

    fn render_endpoint(&self, cx: &mut gpui::Context<Self>) -> gpui::Stateful<gpui::Div> {
        let url = queries::active_account(cx)
            .map(|account| endpoint_url(&account.instance_url))
            .unwrap_or_else(|| SETTINGS_MCP.endpoint_path.to_string());
        let mono = cx.theme().mono_font_family.clone();
        let copy_url = url.clone();
        section(cx)
            .id(test_id("endpoint"))
            .child(crate::surface::glass_section_header(SETTINGS_MCP.labels.endpoint, None, cx))
            .child(crate::surface::list_row(
                crate::surface::flat_row()
                    .flex()
                    .w_full()
                    .min_w_0()
                    .items_center()
                    .gap_3()
                    .px_3()
                    .py_2()
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .font_family(mono)
                            .text_sm()
                            .child(SharedString::from(url)),
                    )
                    .child(
                        Button::new("settings-mcp-endpoint-copy")
                            .outline()
                            .cursor_pointer()
                            .web_sm()
                            .label(SETTINGS_MCP.copy)
                            .on_click(move |_, window, cx| {
                                cx.write_to_clipboard(gpui::ClipboardItem::new_string(copy_url.clone()));
                                crate::toast::show(Toast::success(SETTINGS_MCP.copied), window, cx);
                            }),
                    ),
            ))
    }

    fn render_client_row(&self, index: usize, client: &McpClient, cx: &mut gpui::Context<Self>) -> gpui::Div {
        let muted = cx.theme().muted_foreground;
        let glyph = match crate::user_avatar::cached_avatar_image(cx, client.icon.as_deref()) {
            Some(image) => img(image).size_4().flex_shrink_0().rounded_sm().into_any_element(),
            None => Icon::new(registry::SETTINGS_MCP)
                .size_4()
                .flex_shrink_0()
                .text_color(cx.theme().foreground.opacity(0.7))
                .into_any_element(),
        };
        let last_used = client.last_used_at.as_deref().map(when);
        let caption = client_caption(
            self.scope,
            &client.user_name,
            &client.kinds,
            client.apps_access.as_deref(),
            last_used.as_deref(),
        );
        let target = client.clone();
        crate::surface::flat_row()
            .flex()
            .w_full()
            .min_w_0()
            .items_center()
            .gap_3()
            .px_3()
            .py_2()
            .child(glyph)
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .child(
                        div()
                            .truncate()
                            .text_sm()
                            .font_weight(FontWeight::MEDIUM)
                            .child(SharedString::from(client.name.clone())),
                    )
                    .child(div().truncate().text_xs().text_color(muted).child(SharedString::from(caption))),
            )
            .child(
                Button::new(("mcp-client-revoke", index))
                    .outline()
                    .cursor_pointer()
                    .web_sm()
                    .label(SETTINGS_MCP.revoke)
                    .disabled(self.revoking)
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.confirm_revoke(&target, window, cx)
                    })),
            )
    }

    fn render_clients(&self, owner: bool, cx: &mut gpui::Context<Self>) -> gpui::Stateful<gpui::Div> {
        let mut body = section(cx)
            .id(test_id("clients"))
            .child(crate::surface::glass_section_header(SETTINGS_MCP.labels.clients, None, cx));
        if owner {
            // Web `SegmentedControl` (not embedded): a capsule that hugs its
            // segments, at the group's leading edge.
            let mut control = crate::controls::segmented(cx).w_auto().h(gpui::px(32.));
            for (value, label) in SCOPES {
                control = control.child(
                    crate::controls::segmented_item(self.scope == value, cx)
                        .id(SharedString::from(format!("settings-mcp-scope-{value}")))
                        .flex_none()
                        .px_3()
                        .child(label)
                        .on_click(cx.listener(move |this, _, _, cx| this.set_scope(value, cx))),
                );
            }
            body = body.child(h_flex().child(control));
        }
        match &self.clients {
            None => body.child(super::settings_skeleton("settings-mcp-clients-skeleton", cx)),
            Some(data) if data.clients.is_empty() => body.child(crate::controls::empty_state_list(
                Icon::new(registry::SETTINGS_MCP),
                crate::empty_state_copy::MCP_CLIENTS,
                cx,
            )),
            Some(data) => {
                let clients = data.clients.clone();
                let mut list = v_flex().w_full().min_w_0();
                for (index, client) in clients.iter().enumerate() {
                    list = list.child(crate::surface::list_row(self.render_client_row(index, client, cx)));
                }
                body.child(list)
            }
        }
    }

    fn render_level(
        &self,
        app_id: &str,
        app_slug: &str,
        row: &api::app_access::AccessRow,
        can_edit: bool,
        cx: &mut gpui::Context<Self>,
    ) -> gpui::AnyElement {
        let level = member_level(&row.levels, app_id).to_string();
        let muted = cx.theme().muted_foreground;
        if !can_edit {
            return div()
                .flex_shrink_0()
                .text_sm()
                .text_color(muted)
                .child(SharedString::from(level_label(&level).to_string()))
                .into_any_element();
        }
        let id = SharedString::from(format!("mcp-access-{app_slug}-{}", row.user_id));
        let trigger = Button::new(SharedString::from(format!("{id}-trigger")))
            .ghost()
            .cursor_pointer()
            .h_auto()
            .px_0()
            .py_0()
            .text_color(cx.theme().foreground.opacity(0.7))
            .child(crate::surface::picker_value_label(level_label(&level).to_string()))
            .text_sm()
            .gap_1()
            // Web `Picker triggerVariant="inline"`: the label + a chevron-down.
            .child(
                Icon::new(registry::UI_CHEVRON_DOWN)
                    .size_3()
                    .flex_shrink_0()
                    .text_color(muted),
            );
        let items: Vec<crate::picker::PickerItem<String>> = LEVELS
            .iter()
            .map(|(value, label)| crate::picker::PickerItem::new(value.to_string(), *label))
            .collect();
        let pane = cx.entity().downgrade();
        let (app_id, user_id) = (app_id.to_string(), row.user_id.clone());
        crate::picker::deferred(move |window, cx| {
            let (app_id, user_id, pane) = (app_id.clone(), user_id.clone(), pane.clone());
            crate::picker::Picker::single(
                items.clone(),
                Some(level.clone()),
                trigger.into_any_element(),
                std::rc::Rc::new(move |next: Vec<String>, _window, cx: &mut App| {
                    let Some(level) = next.into_iter().next() else {
                        return;
                    };
                    let (app_id, user_id) = (app_id.clone(), user_id.clone());
                    let _ = pane.update(cx, |this, cx| this.set_level(app_id, user_id, level, cx));
                }),
            )
            .id(id.clone())
            .render(window, cx)
        })
        .into_any_element()
    }

    fn render_access(&self, cx: &mut gpui::Context<Self>) -> Option<gpui::Stateful<gpui::Div>> {
        let data = self.access.clone()?;
        if data.apps.is_empty() {
            return None;
        }
        let muted = cx.theme().muted_foreground;
        let mut groups = v_flex().w_full().min_w_0().gap_3();
        for app in &data.apps {
            let mut group = v_flex().w_full().min_w_0().child(crate::surface::list_row(
                crate::surface::flat_row()
                    .flex()
                    .w_full()
                    .min_w_0()
                    .items_center()
                    .gap_3()
                    .px_3()
                    .py_2()
                    .child(
                        crate::icons::app_icon(app.icon.as_deref(), app.color.as_deref())
                            .size(gpui::px(16.))
                            .flex_shrink_0(),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .text_sm()
                            .font_weight(FontWeight::MEDIUM)
                            .child(SharedString::from(app.name.clone())),
                    )
                    .child(
                        div()
                            .flex_shrink_0()
                            .text_xs()
                            .text_color(muted)
                            .child(SharedString::from(tool_count_label(app.tool_count))),
                    ),
            ));
            for row in &data.rows {
                let name = if row.name.trim().is_empty() { row.email.clone() } else { row.name.clone() };
                let level = self.render_level(&app.id, &app.slug, row, data.can_edit, cx);
                group = group.child(crate::surface::list_row(
                    crate::surface::flat_row()
                        .flex()
                        .w_full()
                        .min_w_0()
                        .items_center()
                        .gap_3()
                        .px_3()
                        .py_2()
                        .child(crate::user_avatar::user_avatar(
                            &row.user_id,
                            &name,
                            row.image.as_deref(),
                            gpui_component::Size::XSmall,
                            cx,
                        ))
                        .child(div().flex_1().min_w_0().truncate().text_sm().child(SharedString::from(name)))
                        .child(level),
                ));
            }
            groups = groups.child(group);
        }
        groups = groups.child(crate::surface::list_row(
            crate::surface::flat_row()
                .id(test_id("activity"))
                .flex()
                .w_full()
                .min_w_0()
                .items_center()
                .gap_3()
                .px_3()
                .py_2()
                .cursor_pointer()
                .child(div().flex_1().min_w_0().text_sm().child(SETTINGS_MCP.activity))
                .child(
                    Icon::new(registry::UI_CHEVRON_RIGHT)
                        .size_4()
                        .flex_shrink_0()
                        .text_color(muted),
                )
                .on_click(|_, window, cx| open_activity(window, cx)),
        ));
        Some(
            section(cx)
                .id(test_id("access"))
                .child(crate::surface::glass_section_header(SETTINGS_MCP.labels.access, None, cx))
                .child(groups),
        )
    }

    /// EXP-862/1274 — the "Built-in Exponential tools" group, unchanged
    /// (now the page's last section): a plain band with an inline filter
    /// ([`filter_builtin_tools`]: title, blurb or wire name); no match = the
    /// shared LIST empty state (`builtin-tools-no-match`). One row per
    /// contract tool like the session feed's Exponential tool row; the raw
    /// wire name is the row's TOOLTIP only.
    fn render_builtin_tools(&self, window: &Window, cx: &mut gpui::Context<Self>) -> gpui::Stateful<gpui::Div> {
        let query = self.tool_filter.read(cx).value().to_string();
        let tools = filter_builtin_tools(steer::exp_tool::builtin_tools(), &query);
        let muted = cx.theme().muted_foreground;
        let filter = crate::controls::search_field(
            &self.tool_filter,
            crate::controls::SearchFieldSize::Sm,
            window,
            cx,
        );
        let trailing = h_flex()
            .gap_2()
            .items_center()
            .child(div().w(gpui::px(192.)).child(filter))
            .into_any_element();
        let band = crate::surface::glass_section_header(SETTINGS_MCP.labels.builtin, Some(trailing), cx);
        let mut group = v_flex().id(test_id("builtin")).w_full().min_w_0().child(band);
        if tools.is_empty() {
            return group.child(crate::controls::empty_state_list(
                Icon::new(registry::UI_EMPTY_SEARCH),
                crate::empty_state_copy::BUILTIN_TOOLS_NO_MATCH,
                cx,
            ));
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
                        move |window, cx| gpui_component::tooltip::Tooltip::new(name.clone()).build(window, cx)
                    })
                    .child(
                        Icon::from(crate::icons::ExpIcon::Logo)
                            .xsmall()
                            .flex_shrink_0()
                            .text_color(muted),
                    )
                    .child(div().flex_shrink_0().text_sm().child(SharedString::from(tool.title)))
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
}

impl Render for McpPane {
    fn render(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let Some(team_id) = active_team_id(&self.nav, cx) else {
            return v_flex().id(test_id("page")).child(crate::controls::empty_state_list(
                Icon::new(registry::UI_TEAM),
                crate::empty_state_copy::NO_TEAM_SELECTED,
                cx,
            ));
        };
        self.ensure_loaded(&team_id, cx);
        let owner = super::is_owner(cx, &team_id);
        let endpoint = self.render_endpoint(cx);
        let clients = self.render_clients(owner, cx);
        let access = self.render_access(cx);
        let builtin = self.render_builtin_tools(window, cx);
        v_flex()
            .id(test_id("page"))
            .w_full()
            .min_w_0()
            .gap_6()
            .child(endpoint)
            .child(div().id(test_id("servers")).child(self.servers.clone()))
            .child(clients)
            .children(access)
            .child(builtin)
    }
}

// ---------------------------------------------------------------------------
// The Activity page
// ---------------------------------------------------------------------------

/// `activityPage`: the `mcp_calls` rows newest first (owners: the team's, a
/// member: their own).
pub struct McpActivityPane {
    nav: Entity<Navigation>,
    team_id: Option<String>,
    data: Option<McpActivityOut>,
    generation: u64,
    stale: bool,
    _subscriptions: Vec<Subscription>,
}

impl McpActivityPane {
    pub fn new(nav: Entity<Navigation>, cx: &mut gpui::Context<Self>) -> Self {
        let subscriptions = vec![cx.observe(&nav, |_, _, cx| cx.notify())];
        Self { nav, team_id: None, data: None, generation: 0, stale: false, _subscriptions: subscriptions }
    }

    pub fn mark_stale(&mut self, cx: &mut gpui::Context<Self>) {
        self.stale = true;
        cx.notify();
    }

    fn ensure_loaded(&mut self, team_id: &str, cx: &mut gpui::Context<Self>) {
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
}

impl Render for McpActivityPane {
    fn render(&mut self, _window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let body = section(cx)
            .id(test_id("activity"))
            .child(crate::surface::glass_section_header(SETTINGS_MCP.activity, None, cx));
        let Some(team_id) = active_team_id(&self.nav, cx) else {
            return body.child(crate::controls::empty_state_list(
                Icon::new(registry::UI_TEAM),
                crate::empty_state_copy::NO_TEAM_SELECTED,
                cx,
            ));
        };
        self.ensure_loaded(&team_id, cx);
        let muted = cx.theme().muted_foreground;
        let mono = cx.theme().mono_font_family.clone();
        match &self.data {
            None => body.child(super::settings_skeleton("settings-mcp-activity-skeleton", cx)),
            Some(data) if data.calls.is_empty() => body.child(crate::controls::empty_state_list(
                Icon::new(registry::UI_HISTORY),
                crate::empty_state_copy::MCP_ACTIVITY,
                cx,
            )),
            Some(data) => {
                let owner = data.owner;
                let mut list = v_flex().w_full().min_w_0();
                for call in data.calls.clone() {
                    let caption = activity_caption(
                        owner,
                        call.app_name.as_deref(),
                        &call.user_name,
                        call.client_name.as_deref(),
                        &when(&call.created_at),
                    );
                    list = list.child(crate::surface::list_row(
                        crate::surface::flat_row()
                            .flex()
                            .w_full()
                            .min_w_0()
                            .items_center()
                            .gap_3()
                            .px_3()
                            .py_2()
                            .child(
                                v_flex()
                                    .flex_1()
                                    .min_w_0()
                                    .child(
                                        div()
                                            .truncate()
                                            .font_family(mono.clone())
                                            .text_sm()
                                            .child(SharedString::from(call.tool.clone())),
                                    )
                                    .child(
                                        div().truncate().text_xs().text_color(muted).child(SharedString::from(caption)),
                                    ),
                            )
                            .child(
                                glass_pill(
                                    SharedString::from(format!("mcp-activity-status-{}", call.id)),
                                    PillSize::Sm,
                                    PillMode::Readonly,
                                    cx,
                                )
                                .flex_shrink_0()
                                .child(div().text_xs().child(SharedString::from(status_label(&call.status).to_string()))),
                            ),
                    ));
                }
                body.child(list)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

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
