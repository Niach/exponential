//! VAPP-183: an app's MCP side on its settings page (the web's
//! `app-mcp-sections.tsx`, the draft `web-editor-settings`), under the
//! VAPP-120 Tools group: ACCOUNTS = the upstreams the app's tools call, each
//! with the viewer's own connection and Connect; MCP TOOLS = the exposed
//! tools with their Read/Write level and an owner's switch (a tool switched
//! off is never listed on /api/mcp/apps). Both hidden when empty. The words
//! mirror apps.json `copy.accounts` / `copy.mcpTools` (#VAPP-243). #VAPP-254
//! (`upstreamPickerCases`): an Accounts row's title is the DECLARED name, a
//! muted second line names the real server when it differs, and an owner picks
//! the server with [`upstream_select`] (the import step's Select too).
//!
//! #EXP-1629: the groups are parts of the Settings tab's document
//! (`app_detail`): each a builder of its rows' template and a projection of
//! its rows. The share rows' ⋯ are genUI Menus ([`share_menu_items`]);
//! the Accounts rows' Selects and the webhook rows' middle-truncated runs
//! stay the IDE's own (`host_leaf` Embeds).

use std::collections::HashMap;

use eui_gpui::controls::CTL_MD_H;
use gpui::{div, px, App, IntoElement, ParentElement, SharedString, Styled};
use gpui_component::{button::Button, ActiveTheme as _, Disableable as _, Icon};
use serde_json::{json, Value};

use api::app_access::{AppServer, AppToolsOut, AppUpstream};

use super::app_detail::{button, row_shell, switch, text_line, titled, titled_item, two_lines, visible, AppDetailPane, Paint, Tints};
use super::doc::{path, Ink, LINE, SM, XS};
use crate::controls::{PointerMenuButtonExt as _, WebControl as _};
use crate::icons::registry;

pub(crate) struct AppMcpCopy {
    pub accounts: &'static str,
    pub tools: &'static str,
    pub read: &'static str,
    pub write: &'static str,
    pub connect: &'static str,
    pub reconnect: &'static str,
    pub no_server: &'static str,
    /// #VAPP-243: an owner's action on a `Not set up` row.
    pub add_server: &'static str,
    /// `UPSTREAM_LABELS.connected` (packages/emcp/fixtures/upstream.json).
    pub connected: &'static str,
    /// #VAPP-254: a row's second line when another server fills the upstream.
    pub uses_server: &'static str,
}

pub(crate) const APP_MCP_COPY: AppMcpCopy = AppMcpCopy {
    accounts: "Accounts",
    tools: "MCP tools",
    read: "Read",
    write: "Write",
    connect: "Connect",
    reconnect: "Reconnect",
    no_server: "Not set up",
    add_server: "Add server",
    connected: "Connected",
    uses_server: "Uses {server}",
};

/// #VAPP-254: the width of an Accounts or import row's Select (the web's).
const UPSTREAM_SELECT_WIDTH: f32 = 144.;

/// #VAPP-254: `servers` as `domain::apps::upstream_picker` takes them.
pub(crate) fn server_options(servers: &[AppServer]) -> Vec<(String, String)> {
    servers.iter().map(|server| (server.id.clone(), server.name.clone())).collect()
}

/// #VAPP-254: an upstream's Select on an Accounts or import row: the app
/// screens' Select (the Data tab's kind, the preview's Run as) at the web's
/// width, named by the upstream (`name`, as on web); `on_pick` gets the
/// picked option's value.
pub(crate) fn upstream_select(
    id: SharedString,
    name: SharedString,
    picker: domain::apps::UpstreamPicker,
    disabled: bool,
    on_pick: impl Fn(String, &mut App) + 'static,
    cx: &App,
) -> impl IntoElement {
    let label = picker.options.iter().find(|(value, _)| *value == picker.value).map(|(_, label)| label.clone()).unwrap_or_default();
    let on_pick = std::rc::Rc::new(on_pick);
    Button::new(id)
        .accessibility_label(name)
        .outline()
        .web_sm()
        .cursor_pointer()
        .w(px(UPSTREAM_SELECT_WIDTH))
        .disabled(disabled)
        .child(div().flex_1().min_w_0().truncate().child(label))
        .child(Icon::new(registry::UI_CHEVRON_DOWN).size(px(14.)).text_color(cx.theme().muted_foreground))
        .pointer_menu(move |mut menu, _window, _cx| {
            menu = crate::pickers::single_select_menu(menu.scrollable(true).max_h(px(320.)));
            for (value, label) in &picker.options {
                let (on_pick, picked) = (on_pick.clone(), value.clone());
                menu = menu.item(
                    crate::controls::pointer_label_item(label.clone(), false)
                        .checked(*value == picker.value)
                        .on_click(move |_, _, cx| on_pick(picked.clone(), cx)),
                );
            }
            menu
        })
}

/// What an Accounts row shows on its right.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AccountTrailing {
    /// No team server of that name.
    NotSetUp,
    Connected,
    Connect,
    Reconnect,
}

pub(crate) fn account_trailing(upstream: &AppUpstream) -> AccountTrailing {
    match (upstream.server_id.as_deref(), upstream.state.as_deref()) {
        (None, _) => AccountTrailing::NotSetUp,
        (Some(_), Some("connected")) => AccountTrailing::Connected,
        (Some(_), Some("reconnect")) => AccountTrailing::Reconnect,
        _ => AccountTrailing::Connect,
    }
}

pub(crate) fn access_label(access: &str) -> &'static str {
    if access == "read" {
        APP_MCP_COPY.read
    } else {
        APP_MCP_COPY.write
    }
}

/// The data with one tool's switch flipped (the optimistic write).
pub(crate) fn with_tool_enabled(data: &AppToolsOut, tool: &str, enabled: bool) -> AppToolsOut {
    let mut next = data.clone();
    for t in next.tools.iter_mut().filter(|t| t.name == tool) {
        t.enabled = enabled;
    }
    next
}

/// VAPP-241 / VAPP-247: a live share row's `…` (an owner's): Copy link, New
/// link, then the expiry items (`expiry:<days>`, `expiry:never`).
pub(super) fn share_menu_items() -> Vec<Value> {
    use domain::app_share::APP_SHARE;
    let mut items = vec![
        json!({"label": APP_SHARE.copy.copy_link, "value": "copy"}),
        json!({"label": APP_SHARE.copy.new_link, "value": "rotate"}),
    ];
    for choice in &APP_SHARE.expiry_choices {
        let value = choice.days.map_or_else(|| "expiry:never".to_string(), |days| format!("expiry:{days}"));
        items.push(json!({"label": choice.label, "value": value}));
    }
    items
}

/// What an Accounts row's server Select needs ([`upstream_select`]).
pub(super) struct AccountSelect {
    pub name: String,
    pub picker: domain::apps::UpstreamPicker,
    pub disabled: bool,
}

/// #VAPP-254: an owner's server Select on an Accounts row.
// drop with EXP-1778: the outline trigger and its scrollable
// single-select menu stay the IDE's own (a `host_leaf` Embed): no genUI
// Select trigger draws the 32 px outline Button.
pub(super) fn account_select(select: &AccountSelect, view: gpui::WeakEntity<AppDetailPane>, cx: &App) -> gpui::AnyElement {
    let account_row = &domain::apps::APPS.test_ids.account_row;
    let name = select.name.clone();
    upstream_select(
        SharedString::from(format!("{account_row}-{}-picker", select.name)),
        SharedString::from(select.name.clone()),
        select.picker.clone(),
        select.disabled,
        move |value, cx| {
            let name = name.clone();
            let _ = view.update(cx, |this, cx| this.pick_upstream_server(name, value, cx));
        },
        cx,
    )
    .into_any_element()
}

/// The `Embed` surface of a host-drawn leaf: its `data` changes whenever
/// the leaf must draw again.
fn leaf_surface(data: Value) -> Value {
    json!({"root": {"id": "leaf", "component": "Box"}, "data": data})
}

/// The Accounts rows (`app-settings-accounts`) as the document binds them;
/// `busy` = the server whose Connect waits, `picking` = the upstream whose
/// Select writes. Fills `selects` with the rows' Selects.
pub(super) fn accounts_rows(data: &AppToolsOut, busy: Option<&str>, picking: Option<&str>, selects: &mut HashMap<String, AccountSelect>) -> Value {
    let servers = server_options(&data.servers);
    let rows: Vec<Value> = data
        .upstreams
        .iter()
        .enumerate()
        .map(|(ix, upstream)| {
            let trailing = account_trailing(upstream);
            let caption = domain::apps::account_caption(&upstream.name, upstream.server_name.as_deref(), APP_MCP_COPY.uses_server);
            let picker = domain::apps::upstream_picker(&upstream.name, upstream.server_id.as_deref(), &servers, APP_MCP_COPY.no_server);
            let select = picker.is_some();
            if let Some(picker) = picker {
                selects.insert(upstream.name.clone(), AccountSelect { name: upstream.name.clone(), picker, disabled: picking.is_some() });
            }
            let add = trailing == AccountTrailing::NotSetUp && data.can_toggle;
            let text = match trailing {
                AccountTrailing::NotSetUp if !data.can_toggle => Some(APP_MCP_COPY.no_server),
                AccountTrailing::Connected => Some(APP_MCP_COPY.connected),
                _ => None,
            };
            let connect = matches!(trailing, AccountTrailing::Connect | AccountTrailing::Reconnect);
            json!({
                "key": upstream.name,
                "divided": ix > 0,
                "name": upstream.name,
                "hasCaption": caption.is_some(),
                "caption": caption.unwrap_or_default(),
                "select": select,
                "selectSurface": leaf_surface(json!({"disabled": picking.is_some(), "value": upstream.server_id, "servers": servers.len()})),
                "add": add,
                "hasText": text.is_some(),
                "text": text.unwrap_or_default(),
                "connect": connect,
                "connectLabel": if trailing == AccountTrailing::Reconnect { APP_MCP_COPY.reconnect } else { APP_MCP_COPY.connect },
                "connectDisabled": busy.is_some(),
            })
        })
        .collect();
    json!({"show": !rows.is_empty(), "rows": rows})
}

/// The MCP tools rows (`app-settings-mcp-tools`); `busy` = the tool whose
/// switch writes.
pub(super) fn tools_rows(data: &AppToolsOut, busy: Option<&str>) -> Value {
    let rows: Vec<Value> = data
        .tools
        .iter()
        .enumerate()
        .map(|(ix, tool)| {
            json!({
                "key": tool.name,
                "divided": ix > 0,
                "title": tool.title,
                "name": tool.name,
                "access": access_label(&tool.access),
                "checked": tool.enabled,
                "disabled": !data.can_toggle || busy == Some(tool.name.as_str()),
            })
        })
        .collect();
    json!({"show": !rows.is_empty(), "rows": rows})
}

/// #VAPP-220: the Automations rows (`app-settings-automations`): the name
/// over the trigger (who it runs as or why it paused, the dropped runs),
/// the status Pill.
pub(super) fn automations_rows(data: &AppToolsOut) -> Value {
    let rows: Vec<Value> = data
        .automations
        .iter()
        .enumerate()
        .map(|(ix, automation)| {
            // A trigger kind this build does not know: no trigger line, the
            // status from the paused/last-run fields alone.
            let on = automation.trigger();
            let unknown = domain::apps::AutomationOn::Event(String::new());
            let view = domain::apps::automation_row(on.as_ref().unwrap_or(&unknown), automation.paused.as_deref(), automation.last_status.as_deref());
            let (trigger, label) = (if on.is_some() { view.trigger } else { String::new() }, view.label);
            // #VAPP-243: who it runs as, or why it is paused.
            let trigger = match domain::apps::automation_sponsor(automation.paused.as_deref(), automation.sponsor_name.as_deref()) {
                Some(sponsor) if trigger.is_empty() => sponsor,
                Some(sponsor) => format!("{trigger} · {sponsor}"),
                None => trigger,
            };
            // #VAPP-255: the runs the rate cap dropped.
            let trigger = match domain::apps::automation_dropped(automation.dropped_runs) {
                Some(dropped) if trigger.is_empty() => dropped,
                Some(dropped) => format!("{trigger} · {dropped}"),
                None => trigger,
            };
            json!({"key": automation.name, "divided": ix > 0, "name": automation.name, "hasTrigger": !trigger.is_empty(), "trigger": trigger, "label": label})
        })
        .collect();
    json!({"show": !rows.is_empty(), "rows": rows})
}

/// What a middle-truncated run needs (the IDE's own text, `mid_text`).
pub(super) struct MidText {
    pub text: String,
    /// `text_sm` (else `text_xs`).
    pub sm: bool,
    pub muted: bool,
}

/// A run of mono text cut in its MIDDLE (`text_ellipsis_middle`: a URL or
/// secret keeps its both ends).
// drop with EXP-1778: the text stays the IDE's own (a `host_leaf`
// Embed) until genUI's Text truncates in the middle.
pub(super) fn mid_text(text: &MidText, cx: &App) -> gpui::AnyElement {
    let theme = cx.theme();
    let run = div()
        .font_family(theme.mono_font_family.clone())
        .text_color(if text.muted { theme.muted_foreground } else { theme.foreground })
        .overflow_hidden()
        .whitespace_nowrap()
        .text_ellipsis_middle()
        .child(text.text.clone());
    let run = if text.sm { run.text_sm() } else { run.text_xs() };
    div().w_full().line_height(gpui::phi()).child(run).into_any_element()
}

/// #VAPP-238: the Webhooks rows (`app-settings-webhooks`): the name over
/// its URL and the status Pill, under them Copy URL and an owner's Create /
/// Rotate secret; a secret this pane minted shows ONCE in a row right under
/// its hook. Fills `texts` with the middle-truncated runs.
pub(super) fn hooks_rows(data: &AppToolsOut, secrets: &HashMap<String, String>, busy: Option<&str>, texts: &mut HashMap<String, MidText>) -> Value {
    let rows: Vec<Value> = data
        .hooks
        .iter()
        .enumerate()
        .map(|(ix, hook)| {
            let view = domain::apps::webhook_row(hook.has_secret, data.can_toggle);
            let secret = secrets.get(&hook.name);
            texts.insert(format!("{HOOK_URL}.{}", hook.name), MidText { text: hook.url.clone(), sm: false, muted: true });
            if let Some(secret) = secret {
                texts.insert(format!("{HOOK_SECRET}.{}", hook.name), MidText { text: secret.clone(), sm: true, muted: false });
            }
            json!({
                "key": hook.name,
                "divided": ix > 0,
                "name": hook.name,
                "url": hook.url,
                "urlSurface": leaf_surface(json!({"text": hook.url})),
                "status": view.label,
                "hasAction": view.action.is_some(),
                "action": view.action.map(|a| a.as_str()).unwrap_or_default(),
                "actionLabel": view.action_label.unwrap_or_default(),
                "actionDisabled": busy.is_some(),
                "hasSecret": secret.is_some(),
                "secret": secret.cloned().unwrap_or_default(),
                "secretSurface": leaf_surface(json!({"text": secret})),
            })
        })
        .collect();
    json!({"show": !rows.is_empty(), "rows": rows})
}

/// VAPP-241 (app-share.json SETTINGS, `rowCases`): the Public links rows
/// (`app-settings-share-links`): the name over its subtitle; an owner's
/// Switch (on = enable, off = revoke) and, while on, the `…` menu. Fills
/// `menuItems` = what its ⋯ opens.
pub(super) fn shares_rows(data: &AppToolsOut, busy: Option<&str>) -> Value {
    use domain::app_share::{share_row_subtitle, share_switch_on, ShareLinkState, ShareMode};
    let rows: Vec<Value> = data
        .shares
        .iter()
        .enumerate()
        .map(|(ix, share)| {
            // A newer server's value reads as a link that is not on.
            let state = ShareLinkState::parse(&share.state).unwrap_or(ShareLinkState::Off);
            let mode = ShareMode::parse(&share.mode).unwrap_or(ShareMode::Submit);
            let on = state == ShareLinkState::On;
            let menu = share.url.is_some() && data.can_toggle && on;
            json!({
                "key": share.name,
                "divided": ix > 0,
                "name": share.name,
                "subtitle": share_row_subtitle(mode, state, share.expires_at.as_deref()),
                "menu": menu,
                "items": if menu { share_menu_items() } else { Vec::new() },
                "switch": data.can_toggle,
                "checked": share_switch_on(state),
                "disabled": busy.is_some(),
            })
        })
        .collect();
    json!({"show": !rows.is_empty(), "rows": rows})
}

/// The Accounts row's Select and the webhook rows' middle-truncated runs
/// (`<node>.<row key>` once painted).
pub(super) const ACCOUNT_SELECT: &str = "account_select";
pub(super) const HOOK_URL: &str = "hook_url";
pub(super) const HOOK_SECRET: &str = "hook_secret";

/// The width of an Accounts row's Select leaf.
pub(super) const ACCOUNT_SELECT_W: f32 = UPSTREAM_SELECT_WIDTH;

/// The Public links group.
pub(super) fn shares_group(ink: &Ink, tints: &Tints) -> Value {
    use domain::app_share::APP_SHARE;
    // The row's ⋯ is inert (gpui-component's disabled ghost) while a share
    // change is in flight.
    let menu = ink.row_menu("share_menu", path("name"), path("menu"), &tints.list_hover, Some(path("disabled")), None);
    let switch = visible(switch("share_switch", "share", "share"), path("switch"));
    let row = row_shell(ink, "share", vec![two_lines(ink, "share", text_line(ink, "share_name", path("name"), SM, &ink.foreground, false), text_line(ink, "share_subtitle", path("subtitle"), XS, &ink.muted, false), None), menu, switch]);
    titled(ink, &APP_SHARE.test_ids.share_links, &APP_SHARE.copy.title, "/pane/shares", row, tints)
}

/// The Accounts group.
pub(super) fn accounts_group(ink: &Ink, tints: &Tints) -> Value {
    let mut caption = text_line(ink, "account_caption", path("caption"), XS, &ink.muted, false);
    caption["visible"] = path("hasCaption");
    let select = json!({"id": ACCOUNT_SELECT, "component": "Embed", "visible": path("select"), "props": {"surface": path("selectSurface")}, "style": {"width": ACCOUNT_SELECT_W, "height": CTL_MD_H, "flexShrink": 0}});
    let add = button(ink, tints, "account_add", Paint::Outline, None, json!(APP_MCP_COPY.add_server), json!(false), json!({"name": "addServer", "context": {"key": path("key")}}));
    let add = visible(add, path("add"));
    let mut text = text_line(ink, "account_text", path("text"), SM, &ink.muted, false);
    text["visible"] = path("hasText");
    text["style"]["flexShrink"] = json!(0);
    let connect = button(ink, tints, "account_connect", Paint::Outline, None, path("connectLabel"), path("connectDisabled"), json!({"name": "connect", "context": {"key": path("key")}}));
    let connect = visible(connect, path("connect"));
    let row = row_shell(ink, "account", vec![two_lines(ink, "account", text_line(ink, "account_name", path("name"), SM, &ink.foreground, false), caption, None), select, add, text, connect]);
    titled(ink, "app-settings-accounts", APP_MCP_COPY.accounts, "/pane/accounts", row, tints)
}

/// The MCP tools group.
pub(super) fn tools_group(ink: &Ink, tints: &Tints) -> Value {
    let switch = switch("mcp_tool_switch", "mcpTool", "mcpTool");
    let row = row_shell(ink, "mcp_tool", vec![
        two_lines(ink, "mcp_tool", text_line(ink, "mcp_tool_title", path("title"), SM, &ink.foreground, false), text_line(ink, "mcp_tool_name", path("name"), XS, &ink.muted, true), None),
        ink.pill("mcp_tool_access", path("access"), false, None),
        switch,
    ]);
    titled(ink, "app-settings-mcp-tools", APP_MCP_COPY.tools, "/pane/mcpTools", row, tints)
}

/// #VAPP-220: the Automations group: nothing to switch.
pub(super) fn automations_group(ink: &Ink, tints: &Tints) -> Value {
    let apps = &*domain::apps::APPS;
    let mut trigger = ink.text("automation_trigger", path("trigger"), XS, &ink.muted);
    trigger["props"]["lines"] = json!(2);
    trigger["visible"] = path("hasTrigger");
    let row = row_shell(ink, "automation", vec![
        two_lines(ink, "automation", text_line(ink, "automation_name", path("name"), SM, &ink.foreground, false), trigger, None),
        ink.pill("automation_status", path("label"), false, None),
    ]);
    titled(ink, &apps.test_ids.automations, &apps.copy.automations.title, "/pane/automations", row, tints)
}

/// #VAPP-238: the Webhooks group.
pub(super) fn webhooks_group(ink: &Ink, tints: &Tints) -> Value {
    let apps = &*domain::apps::APPS;
    let copy = &apps.copy.webhooks;
    let xs_line = ink.r(XS) * LINE;
    let sm_line = ink.r(SM) * LINE;
    let url = json!({"id": HOOK_URL, "component": "Embed", "props": {"surface": path("urlSurface")}, "style": {"width": "100%", "height": xs_line}});
    let top = json!({"id": "hook_top", "component": "Box", "style": {"display": "flex", "flexDirection": "row", "alignItems": "center", "width": "100%", "gap": ink.r(0.75)},
        "children": [
            two_lines(ink, "hook", text_line(ink, "hook_name", path("name"), SM, &ink.foreground, true), url, None),
            ink.pill("hook_status", path("status"), false, None),
        ]});
    let copy_url = button(ink, tints, "hook_copy_url", Paint::Outline, None, json!(copy.copy_url), json!(false), json!({"name": "copy", "context": {"text": path("url")}}));
    let action = button(ink, tints, "hook_action", Paint::Outline, None, path("actionLabel"), path("actionDisabled"), json!({"name": "hook", "context": {"key": path("key"), "action": path("action")}}));
    let pills = json!({"id": "hook_pills", "component": "Box", "style": {"display": "flex", "flexDirection": "row", "flexWrap": "wrap", "alignItems": "center", "gap": ink.r(0.5)},
        "children": [copy_url, visible(action, path("hasAction"))]});
    let column = json!({"id": "hook_column", "component": "Box", "style": {"display": "flex", "flexDirection": "column", "width": "100%", "minWidth": 0, "gap": ink.r(0.5)}, "children": [top, pills]});
    let hook = ink.shell("hook_row", false, json!(true), vec![column]);
    let mut label = text_line(ink, "secret_label", json!(copy.secret), XS, &ink.muted, false);
    label["style"]["flexShrink"] = json!(0);
    let value = json!({"id": HOOK_SECRET, "component": "Embed", "props": {"surface": path("secretSurface")}, "style": {"width": "100%", "height": sm_line}});
    let copy_secret = button(ink, tints, "hook_copy_secret", Paint::Outline, None, json!(copy.copy_secret), json!(false), json!({"name": "copy", "context": {"text": path("secret")}}));
    let secret = ink.shell("secret_row", false, path("hasSecret"), vec![two_lines(ink, "secret", label, value, None), copy_secret]);
    let item = json!({"id": "hook_item", "component": "Box", "style": {"display": "flex", "flexDirection": "column", "width": "100%", "minWidth": 0},
        "children": [ink.divider("hook_divider", path("divided")), hook, ink.divider("secret_divider", path("hasSecret")), secret]});
    titled_item(ink, &apps.test_ids.webhooks, &copy.title, "/pane/hooks", item, tints)
}

#[cfg(test)]
mod tests {
    use super::*;
    use api::app_access::AppTool;

    fn upstream(server_id: Option<&str>, state: Option<&str>) -> AppUpstream {
        AppUpstream {
            name: "github".into(),
            server_id: server_id.map(str::to_string),
            server_name: None,
            auth: Some("oauth".into()),
            state: state.map(str::to_string),
        }
    }

    #[test]
    fn an_account_row_reads_its_server_and_state() {
        assert_eq!(account_trailing(&upstream(None, None)), AccountTrailing::NotSetUp);
        assert_eq!(account_trailing(&upstream(Some("s"), Some("connected"))), AccountTrailing::Connected);
        assert_eq!(account_trailing(&upstream(Some("s"), Some("reconnect"))), AccountTrailing::Reconnect);
        for state in [None, Some("disconnected"), Some("failed"), Some("expired")] {
            assert_eq!(account_trailing(&upstream(Some("s"), state)), AccountTrailing::Connect);
        }
    }

    #[test]
    fn the_words_match_the_web() {
        assert_eq!(access_label("read"), "Read");
        assert_eq!(access_label("write"), "Write");
        assert_eq!(APP_MCP_COPY.no_server, "Not set up");
        assert_eq!(APP_MCP_COPY.tools, "MCP tools");
    }

    #[test]
    fn the_account_words_match_the_fixture() {
        let fixture: serde_json::Value =
            serde_json::from_str(include_str!("../../../../../../packages/domain-contract/fixtures/apps.json")).unwrap();
        let accounts = &fixture["copy"]["accounts"];
        for (key, word) in [
            ("title", APP_MCP_COPY.accounts),
            ("connect", APP_MCP_COPY.connect),
            ("reconnect", APP_MCP_COPY.reconnect),
            ("connected", APP_MCP_COPY.connected),
            ("notSetUp", APP_MCP_COPY.no_server),
            ("addServer", APP_MCP_COPY.add_server),
            ("usesServer", APP_MCP_COPY.uses_server),
        ] {
            assert_eq!(accounts[key].as_str(), Some(word), "{key}");
        }
    }

    #[test]
    fn the_picker_takes_the_servers_in_order() {
        let servers = [AppServer { id: "s1".into(), name: "crm".into() }, AppServer { id: "s2".into(), name: "hubspot".into() }];
        assert_eq!(server_options(&servers), vec![("s1".to_string(), "crm".to_string()), ("s2".to_string(), "hubspot".to_string())]);
    }

    #[test]
    fn a_switch_flips_one_tool() {
        let tool = |name: &str| AppTool { name: name.into(), title: name.into(), access: "read".into(), via: None, enabled: true };
        let data = AppToolsOut { can_toggle: true, tools: vec![tool("a"), tool("b")], upstreams: vec![], servers: vec![], automations: vec![], hooks: vec![], shares: vec![] };
        let next = with_tool_enabled(&data, "b", false);
        assert!(next.tools[0].enabled);
        assert!(!next.tools[1].enabled);
    }
}
