//! EXP-1183: `mcp-app-views` (Special components): Exponential's MCP Apps
//! views (`packages/mcp-apps`) render inside a third-party MCP host —
//! OpenClaw's dashboard, Claude, ChatGPT — never in a client, so the IDE has
//! no form to draw. The entry says so; the web styleguide islands the views.

use gpui::{div, px, App, Div, ParentElement as _, Styled as _, Window};
use gpui_component::ActiveTheme as _;

pub(crate) const ID: &str = "mcp-app-views";
pub(crate) const OWNER: &str = "EXP-1183";

pub(crate) fn render(_window: &mut Window, cx: &mut App) -> Div {
    div()
        .max_w(px(420.))
        .text_sm()
        .text_color(cx.theme().muted_foreground)
        .child(
            "Web only: five views render inside the MCP host (OpenClaw, Claude, \
             ChatGPT) beside their call: the issue list (exponential_issues_show), \
             the run list (exponential_sessions_list), the run report \
             (exponential_sessions_get), the inbox (exponential_notifications_list) \
             and the devices page (exponential_devices_list). See the web styleguide.",
        )
}
