//! VAPP-183: an app's MCP side on its settings page (the web's
//! `app-mcp-sections.tsx`, the draft `web-editor-settings`), under the
//! VAPP-120 Tools group: ACCOUNTS = the upstreams the app's tools call, each
//! with the viewer's own connection and Connect; MCP TOOLS = the exposed
//! tools with their Read/Write level and an owner's switch (a tool switched
//! off is never listed on /api/mcp/apps). Both hidden when empty. The words
//! mirror `apps/web/src/lib/emcp/copy.ts` `APP_MCP_COPY`.

use gpui::{div, FontWeight, IntoElement, ParentElement, SharedString, Styled};
use gpui_component::{
    button::Button,
    v_flex, ActiveTheme as _, Disableable as _,
};

use api::app_access::{AppToolsOut, AppUpstream};

use super::app_detail::AppDetailPane;
use crate::controls::WebControl as _;

pub(crate) struct AppMcpCopy {
    pub accounts: &'static str,
    pub tools: &'static str,
    pub read: &'static str,
    pub write: &'static str,
    pub connect: &'static str,
    pub reconnect: &'static str,
    pub no_server: &'static str,
    /// `UPSTREAM_LABELS.connected` (packages/emcp/fixtures/upstream.json).
    pub connected: &'static str,
}

pub(crate) const APP_MCP_COPY: AppMcpCopy = AppMcpCopy {
    accounts: "Accounts",
    tools: "MCP tools",
    read: "Read",
    write: "Write",
    connect: "Connect",
    reconnect: "Reconnect",
    no_server: "Not set up",
    connected: "Connected",
};

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
    cx: &mut gpui::Context<AppDetailPane>,
) -> Option<gpui::Stateful<gpui::Div>> {
    if data.upstreams.is_empty() {
        return None;
    }
    let muted = cx.theme().muted_foreground;
    let rows = data
        .upstreams
        .iter()
        .map(|upstream| {
            let trailing = match account_trailing(upstream) {
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
            crate::surface::glass_row_shell()
                .justify_between()
                .gap_3()
                .child(div().min_w_0().text_sm().truncate().child(upstream.name.clone()))
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

#[cfg(test)]
mod tests {
    use super::*;
    use api::app_access::AppTool;

    fn upstream(server_id: Option<&str>, state: Option<&str>) -> AppUpstream {
        AppUpstream {
            name: "github".into(),
            server_id: server_id.map(str::to_string),
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
    fn a_switch_flips_one_tool() {
        let tool = |name: &str| AppTool { name: name.into(), title: name.into(), access: "read".into(), via: None, enabled: true };
        let data = AppToolsOut { can_toggle: true, tools: vec![tool("a"), tool("b")], upstreams: vec![] };
        let next = with_tool_enabled(&data, "b", false);
        assert!(next.tools[0].enabled);
        assert!(!next.tools[1].enabled);
    }
}
