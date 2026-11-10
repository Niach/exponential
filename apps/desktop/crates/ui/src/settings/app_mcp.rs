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

use gpui::{div, px, App, FontWeight, IntoElement, ParentElement, SharedString, Styled};
use gpui_component::{
    button::Button,
    v_flex, ActiveTheme as _, Disableable as _, Icon,
};

use api::app_access::{AppServer, AppToolsOut, AppUpstream};

use super::app_detail::AppDetailPane;
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
    let on_pick: std::rc::Rc<dyn Fn(String, &mut App)> = std::rc::Rc::new(on_pick);
    upstream_trigger(id, name, &picker, disabled, cx).pointer_menu(move |menu, _window, _cx| upstream_menu(menu, &picker, on_pick.clone()))
}

/// [`upstream_select`]'s trigger: the outline sm Button holding the shown
/// option's label and the chevron (the bench draws it open, #EXP-1778).
pub(crate) fn upstream_trigger(id: SharedString, name: SharedString, picker: &domain::apps::UpstreamPicker, disabled: bool, cx: &App) -> Button {
    let label = picker.options.iter().find(|(value, _)| *value == picker.value).map(|(_, label)| label.clone()).unwrap_or_default();
    Button::new(id)
        .accessibility_label(name)
        .outline()
        .web_sm()
        .cursor_pointer()
        .w(px(UPSTREAM_SELECT_WIDTH))
        .disabled(disabled)
        .child(div().flex_1().min_w_0().truncate().child(label))
        .child(Icon::new(registry::UI_CHEVRON_DOWN).size(px(14.)).text_color(cx.theme().muted_foreground))
}

/// [`upstream_select`]'s menu: the options, the shown one checked (the
/// bench draws it in place, #EXP-1778).
pub(crate) fn upstream_menu(
    mut menu: gpui_component::menu::PopupMenu,
    picker: &domain::apps::UpstreamPicker,
    on_pick: std::rc::Rc<dyn Fn(String, &mut App)>,
) -> gpui_component::menu::PopupMenu {
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

/// A group's heading over its rows (the Tools group's look).
fn titled(id: &'static str, title: &'static str, rows: Vec<gpui::Div>) -> gpui::Stateful<gpui::Div> {
    use gpui::InteractiveElement as _;
    v_flex()
        .id(id)
        .gap_1()
        .child(div().px_1().text_sm().font_weight(FontWeight::MEDIUM).child(title))
        .child(crate::surface::glass_group_rows(rows))
}

/// The Accounts group (`app-settings-accounts`), `None` when the app declares
/// no upstreams.
pub(crate) fn accounts_group(
    data: &AppToolsOut,
    busy: Option<&str>,
    picking: Option<&str>,
    cx: &mut gpui::Context<AppDetailPane>,
) -> Option<gpui::Stateful<gpui::Div>> {
    use gpui::prelude::FluentBuilder as _;
    use gpui::InteractiveElement as _;
    if data.upstreams.is_empty() {
        return None;
    }
    let muted = cx.theme().muted_foreground;
    let account_row = &domain::apps::APPS.test_ids.account_row;
    let servers = server_options(&data.servers);
    let view = cx.entity().downgrade();
    let rows = data
        .upstreams
        .iter()
        .map(|upstream| {
            let trailing = match account_trailing(upstream) {
                // #VAPP-243: an owner adds the team server of that exact name.
                AccountTrailing::NotSetUp if data.can_toggle => {
                    let name = upstream.name.clone();
                    Button::new(SharedString::from(format!("app-account-{}-add", upstream.name)))
                        .outline()
                        .cursor_pointer()
                        .web_sm()
                        .label(APP_MCP_COPY.add_server)
                        .on_click(cx.listener(move |this, _, window, cx| this.add_upstream_server(name.clone(), window, cx)))
                        .into_any_element()
                }
                AccountTrailing::NotSetUp | AccountTrailing::Connected => div()
                    .flex_shrink_0()
                    .text_sm()
                    .text_color(muted)
                    .child(if account_trailing(upstream) == AccountTrailing::NotSetUp {
                        APP_MCP_COPY.no_server
                    } else {
                        APP_MCP_COPY.connected
                    })
                    .into_any_element(),
                kind => {
                    let target = upstream.clone();
                    Button::new(SharedString::from(format!("app-account-connect-{}", upstream.name)))
                        .outline()
                        .cursor_pointer()
                        .web_sm()
                        .label(if kind == AccountTrailing::Reconnect { APP_MCP_COPY.reconnect } else { APP_MCP_COPY.connect })
                        .loading(busy.is_some() && busy == upstream.server_id.as_deref())
                        .disabled(busy.is_some())
                        .on_click(cx.listener(move |this, _, window, cx| this.connect_upstream(&target, window, cx)))
                        .into_any_element()
                }
            };
            let caption = domain::apps::account_caption(&upstream.name, upstream.server_name.as_deref(), APP_MCP_COPY.uses_server);
            // #VAPP-254: an owner's Select (no servers = none, the row as before).
            let select = domain::apps::upstream_picker(&upstream.name, upstream.server_id.as_deref(), &servers, APP_MCP_COPY.no_server).map(|picker| {
                let (view, name) = (view.clone(), upstream.name.clone());
                upstream_select(
                    SharedString::from(format!("{account_row}-{}-picker", upstream.name)),
                    SharedString::from(upstream.name.clone()),
                    picker,
                    picking.is_some(),
                    move |value, cx| {
                        let name = name.clone();
                        let _ = view.update(cx, |this, cx| this.pick_upstream_server(name, value, cx));
                    },
                    cx,
                )
            });
            crate::surface::glass_row_shell()
                .justify_between()
                .gap_3()
                .child(
                    v_flex()
                        .id(SharedString::from(format!("{account_row}-{}", upstream.name)))
                        .flex_1()
                        .min_w_0()
                        .child(div().text_sm().truncate().child(upstream.name.clone()))
                        .when_some(caption, |col, caption| col.child(div().text_xs().text_color(muted).truncate().child(caption))),
                )
                .children(select)
                .child(trailing)
        })
        .collect();
    Some(titled("app-settings-accounts", APP_MCP_COPY.accounts, rows))
}

/// The MCP tools group (`app-settings-mcp-tools`), `None` when the app
/// exposes no tools.
pub(crate) fn tools_group(
    data: &AppToolsOut,
    busy: Option<&str>,
    cx: &mut gpui::Context<AppDetailPane>,
) -> Option<gpui::Stateful<gpui::Div>> {
    if data.tools.is_empty() {
        return None;
    }
    let muted = cx.theme().muted_foreground;
    let mono = cx.theme().mono_font_family.clone();
    let rows = data
        .tools
        .iter()
        .map(|tool| {
            let name = tool.name.clone();
            crate::surface::glass_row_shell()
                .justify_between()
                .gap_3()
                .child(
                    v_flex()
                        .flex_1()
                        .min_w_0()
                        .child(div().text_sm().truncate().child(tool.title.clone()))
                        .child(div().text_xs().font_family(mono.clone()).text_color(muted).truncate().child(tool.name.clone())),
                )
                .child(
                    crate::surface::glass_pill(
                        SharedString::from(format!("app-mcp-tool-access-{}", tool.name)),
                        crate::surface::PillSize::Sm,
                        crate::surface::PillMode::Readonly,
                        cx,
                    )
                    .flex_shrink_0()
                    .child(access_label(&tool.access)),
                )
                .child(
                    crate::controls::web_switch(SharedString::from(format!("app-mcp-tool-switch-{}", tool.name)))
                        .checked(tool.enabled)
                        .disabled(!data.can_toggle || busy == Some(tool.name.as_str()))
                        .on_click(cx.listener(move |this, checked: &bool, _, cx| {
                            this.toggle_mcp_tool(name.clone(), *checked, cx)
                        })),
                )
        })
        .collect();
    Some(titled("app-settings-mcp-tools", APP_MCP_COPY.tools, rows))
}

/// #VAPP-220: the Automations group (`app-settings-automations`), `None`
/// when the published package declares none: one row per automation = its
/// name over the trigger (muted), the status as a read-only Pill (the MCP
/// tools' Read/Write look). Nothing to switch.
pub(crate) fn automations_group(data: &AppToolsOut, cx: &mut gpui::Context<AppDetailPane>) -> Option<gpui::Stateful<gpui::Div>> {
    use gpui::prelude::FluentBuilder as _;
    use gpui::InteractiveElement as _;
    if data.automations.is_empty() {
        return None;
    }
    let apps = &*domain::apps::APPS;
    let muted = cx.theme().muted_foreground;
    let rows = data
        .automations
        .iter()
        .map(|automation| {
            // A trigger kind this build does not know: no trigger line, the
            // status from the paused/last-run fields alone.
            let on = automation.trigger();
            let unknown = domain::apps::AutomationOn::Event(String::new());
            let view =
                domain::apps::automation_row(on.as_ref().unwrap_or(&unknown), automation.paused.as_deref(), automation.last_status.as_deref());
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
            crate::surface::glass_row_shell()
                .justify_between()
                .gap_3()
                .child(
                    v_flex()
                        .id(SharedString::from(format!("{}-{}", apps.test_ids.automation_row, automation.name)))
                        .flex_1()
                        .min_w_0()
                        .child(div().text_sm().truncate().child(automation.name.clone()))
                        .when(!trigger.is_empty(), |col| col.child(div().text_xs().text_color(muted).line_clamp(2).child(trigger))),
                )
                .child(
                    crate::surface::glass_pill(
                        SharedString::from(format!("app-automation-status-{}", automation.name)),
                        crate::surface::PillSize::Sm,
                        crate::surface::PillMode::Readonly,
                        cx,
                    )
                    .flex_shrink_0()
                    .child(label),
                )
        })
        .collect();
    Some(
        v_flex()
            .id(SharedString::from(apps.test_ids.automations.clone()))
            .gap_1()
            .child(div().px_1().text_sm().font_weight(FontWeight::MEDIUM).child(apps.copy.automations.title.clone()))
            .child(crate::surface::glass_group_rows(rows)),
    )
}

/// VAPP-241 (app-share.json SETTINGS, `rowCases`): the Public links group
/// (`app-settings-share-links`), `None` when the published package declares
/// no shares. Everyone reads it: one row per share = the name over its
/// subtitle; an owner's Switch (on = enable, off = revoke) and, while on, the
/// `…` menu Copy link / New link, then the expiry items (VAPP-247).
pub(crate) fn shares_group(
    data: &AppToolsOut,
    busy: Option<&str>,
    cx: &mut gpui::Context<AppDetailPane>,
) -> Option<gpui::Stateful<gpui::Div>> {
    use domain::app_share::{share_row_subtitle, share_switch_on, ShareLinkState, ShareMode, APP_SHARE};
    use gpui::InteractiveElement as _;
    if data.shares.is_empty() {
        return None;
    }
    let muted = cx.theme().muted_foreground;
    let view = cx.entity().downgrade();
    let mut rows = Vec::new();
    for share in &data.shares {
        // A newer server's value reads as a link that is not on.
        let state = ShareLinkState::parse(&share.state).unwrap_or(ShareLinkState::Off);
        let mode = ShareMode::parse(&share.mode).unwrap_or(ShareMode::Submit);
        let on = state == ShareLinkState::On;
        let mut row = crate::surface::glass_row_shell().justify_between().gap_3().child(
            v_flex()
                .id(SharedString::from(format!("{}-{}", APP_SHARE.test_ids.share_row, share.name)))
                .flex_1()
                .min_w_0()
                .child(div().text_sm().truncate().child(share.name.clone()))
                .child(div().text_xs().text_color(muted).truncate().child(share_row_subtitle(mode, state, share.expires_at.as_deref()))),
        );
        if let Some(url) = share.url.clone().filter(|_| data.can_toggle && on) {
            let (name, view) = (share.name.clone(), view.clone());
            row = row.child(
                crate::controls::ghost_icon_button(SharedString::from(format!("app-share-menu-{}", share.name)), Icon::new(registry::UI_MORE), cx)
                    .accessibility_label(share.name.clone())
                    .disabled(busy.is_some())
                    .pointer_menu(move |menu, _, _| {
                        let (url, name, view) = (url.clone(), name.clone(), view.clone());
                        let (rotate_name, rotate_view) = (name.clone(), view.clone());
                        let mut menu = menu
                            .item(crate::controls::pointer_label_item(APP_SHARE.copy.copy_link.clone(), false).on_click(move |_, window, cx| {
                                cx.write_to_clipboard(gpui::ClipboardItem::new_string(url.clone()));
                                crate::toast::show(crate::toast::Toast::success(domain::apps::APPS.copy.webhooks.copied.clone()), window, cx);
                            }))
                            .item(crate::controls::pointer_label_item(APP_SHARE.copy.new_link.clone(), false).on_click(move |_, _, cx| {
                                let name = rotate_name.clone();
                                let _ = rotate_view.update(cx, |this, cx| this.set_share(name, "rotate", cx));
                            }));
                        for choice in &APP_SHARE.expiry_choices {
                            let (name, view, days) = (name.clone(), view.clone(), choice.days);
                            menu = menu.item(crate::controls::pointer_label_item(choice.label.clone(), false).on_click(move |_, _, cx| {
                                let name = name.clone();
                                let _ = view.update(cx, |this, cx| this.set_share_expiry(name, days, cx));
                            }));
                        }
                        menu
                    }),
            );
        }
        if data.can_toggle {
            let name = share.name.clone();
            row = row.child(
                crate::controls::web_switch(SharedString::from(format!("app-share-switch-{}", share.name)))
                    .checked(share_switch_on(state))
                    .disabled(busy.is_some())
                    .on_click(cx.listener(move |this, checked: &bool, _, cx| {
                        this.set_share(name.clone(), if *checked { "enable" } else { "revoke" }, cx)
                    })),
            );
        }
        rows.push(row);
    }
    Some(
        v_flex()
            .id(SharedString::from(APP_SHARE.test_ids.share_links.clone()))
            .gap_1()
            .child(div().px_1().text_sm().font_weight(FontWeight::MEDIUM).child(APP_SHARE.copy.title.clone()))
            .child(crate::surface::glass_group_rows(rows)),
    )
}

/// The copy pill of a URL or secret (Settings › MCP's endpoint row: the
/// clipboard, then the success toast).
fn copy_pill(id: SharedString, label: String, text: String, copied: String) -> Button {
    Button::new(id)
        .outline()
        .cursor_pointer()
        .web_sm()
        .label(label)
        .on_click(move |_, window, cx| {
            cx.write_to_clipboard(gpui::ClipboardItem::new_string(text.clone()));
            crate::toast::show(crate::toast::Toast::success(copied.clone()), window, cx);
        })
}

/// One mono line, middle-truncated (start…end): a hook's URL and the
/// shown-once secret (#EXP-1778: genUI Text `truncate: "middle"`).
pub(crate) fn mid_text(text: impl Into<SharedString>, mono: SharedString) -> gpui::Div {
    div().font_family(mono).overflow_hidden().whitespace_nowrap().text_ellipsis_middle().child(text.into())
}

/// #VAPP-238: the Webhooks group (`app-settings-webhooks`), under
/// Automations, `None` when the published package listens on none: one row
/// per hook = the name (mono) over its URL (mono, muted, middle-truncated)
/// and the status Pill; under them Copy URL and an owner's Create secret /
/// Rotate secret. A secret this pane minted shows
/// ONCE in a row right under its hook (`app-webhook-secret`).
pub(crate) fn webhooks_group(
    data: &AppToolsOut,
    secrets: &std::collections::HashMap<String, String>,
    busy: Option<&str>,
    cx: &mut gpui::Context<AppDetailPane>,
) -> Option<gpui::Stateful<gpui::Div>> {
    use gpui::InteractiveElement as _;
    if data.hooks.is_empty() {
        return None;
    }
    let apps = &*domain::apps::APPS;
    let copy = &apps.copy.webhooks;
    let muted = cx.theme().muted_foreground;
    let mono = cx.theme().mono_font_family.clone();
    let mut rows = Vec::new();
    for hook in &data.hooks {
        let view = domain::apps::webhook_row(hook.has_secret, data.can_toggle);
        // The top line = the name/URL column + the status Pill; under them,
        // leading-aligned, Copy URL then (an owner) the action pill.
        let top = gpui_component::h_flex()
            .w_full()
            .justify_between()
            .gap_3()
            .child(
                v_flex()
                    .id(SharedString::from(format!("{}-{}", apps.test_ids.webhook_row, hook.name)))
                    .flex_1()
                    .min_w_0()
                    .child(div().text_sm().font_family(mono.clone()).truncate().child(hook.name.clone()))
                    .child(mid_text(hook.url.clone(), mono.clone()).text_xs().text_color(muted)),
            )
            .child(
                crate::surface::glass_pill(
                    SharedString::from(format!("app-webhook-status-{}", hook.name)),
                    crate::surface::PillSize::Sm,
                    crate::surface::PillMode::Readonly,
                    cx,
                )
                .flex_shrink_0()
                .child(view.label),
            );
        let mut pills = gpui_component::h_flex().flex_wrap().items_center().gap_2().child(copy_pill(
            SharedString::from(format!("app-webhook-copy-url-{}", hook.name)),
            copy.copy_url.clone(),
            hook.url.clone(),
            copy.copied.clone(),
        ));
        if let (Some(action), Some(label)) = (view.action, view.action_label) {
            let name = hook.name.clone();
            pills = pills.child(
                Button::new(SharedString::from(format!("app-webhook-{}-{}", action.as_str(), hook.name)))
                    .outline()
                    .cursor_pointer()
                    .web_sm()
                    .label(label)
                    .loading(busy == Some(hook.name.as_str()))
                    .disabled(busy.is_some())
                    .on_click(cx.listener(move |this, _, window, cx| this.hook_action(name.clone(), action, window, cx))),
            );
        }
        rows.push(crate::surface::glass_row_shell().child(v_flex().w_full().min_w_0().gap_2().child(top).child(pills)));
        if let Some(secret) = secrets.get(&hook.name) {
            rows.push(
                crate::surface::glass_row_shell()
                    .justify_between()
                    .gap_3()
                    .child(
                        v_flex()
                            .id(SharedString::from(format!("{}-{}", apps.test_ids.webhook_secret, hook.name)))
                            .flex_1()
                            .min_w_0()
                            .child(div().text_xs().text_color(muted).truncate().child(copy.secret.clone()))
                            .child(mid_text(secret.clone(), mono.clone()).text_sm()),
                    )
                    .child(copy_pill(
                        SharedString::from(format!("app-webhook-copy-secret-{}", hook.name)),
                        copy.copy_secret.clone(),
                        secret.clone(),
                        copy.copied.clone(),
                    )),
            );
        }
    }
    Some(
        v_flex()
            .id(SharedString::from(apps.test_ids.webhooks.clone()))
            .gap_1()
            .child(div().px_1().text_sm().font_weight(FontWeight::MEDIUM).child(copy.title.clone()))
            .child(crate::surface::glass_group_rows(rows)),
    )
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
