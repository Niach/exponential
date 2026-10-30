//! EXP-1477 — `sidebar-rows` (General components): the rail's REAL rows
//! (`sidebar::rail_row_lead`, `rail_section_label`, the trailing-column
//! helpers): nav rows with a dot and a count, an "Agents" header with its
//! `+`, agent rows with a needs-you dot and the hover gear — every trailing
//! indicator in ONE 20px column 4px in from the row's right edge. #EXP-1480:
//! the dot is `rail_dot` (`sidebar.trailingDot`, web `SidebarMenuDot`), the
//! count the eUI `count_badge` (web `Badge`).

use gpui::{
    div, px, App, Div, InteractiveElement as _, IntoElement as _, ParentElement as _, Styled as _,
    Window,
};
use gpui_component::{button::Button, button::ButtonVariants as _, Icon, Sizable as _};

use crate::icons::registry;
use crate::sidebar::{
    rail_badge_element, rail_row_lead, rail_section_label, rail_trailing_hover_action,
    rail_trailing_slot, RailBadge,
};
use crate::surface::BadgeTone;

pub(crate) const ID: &str = "sidebar-rows";
pub(crate) const OWNER: &str = "EXP-1477";

const ROW_GROUP: &str = "sg-sidebar-row";

fn lead(icon: Icon) -> gpui::AnyElement {
    icon.with_size(gpui_component::Size::Medium).flex_shrink_0().into_any_element()
}

pub(crate) fn render(_window: &mut Window, cx: &mut App) -> Div {
    let yellow = theme::tokens::YELLOW.to_hsla();
    div()
        .w(px(256.))
        .p_2()
        .flex()
        .flex_col()
        .gap_1()
        .child(rail_row_lead(
            "sg-sidebar-inbox",
            lead(Icon::from(registry::NAV_INBOX)),
            "Inbox",
            false,
            Some(RailBadge::Dot(theme::tokens::PRIMARY.to_hsla())),
            cx,
        ))
        .child(rail_row_lead(
            "sg-sidebar-chats",
            lead(Icon::from(registry::NAV_THREADS)),
            "Chats",
            false,
            Some(RailBadge::Dot(yellow)),
            cx,
        ))
        .child(rail_row_lead(
            "sg-sidebar-drafts",
            lead(Icon::from(registry::NAV_DRAFTS)),
            "Drafts",
            false,
            Some(RailBadge::Count(2, BadgeTone::Muted)),
            cx,
        ))
        .child(div().h(px(12.)))
        .child(
            rail_section_label("Agents", cx).child(
                Button::new("sg-sidebar-new-agent").ghost().xsmall().icon(registry::UI_ADD),
            ),
        )
        .child(
            rail_row_lead(
                "sg-sidebar-agent",
                lead(Icon::from(registry::UI_WATCH).text_color(gpui::rgb(0xa855f7))),
                "Big Brother",
                false,
                None,
                cx,
            )
            .group(ROW_GROUP)
            .child(rail_trailing_hover_action(
                ROW_GROUP,
                Some(rail_badge_element(RailBadge::Dot(yellow), 12., cx)),
                Button::new("sg-sidebar-agent-settings")
                    .ghost()
                    .xsmall()
                    .icon(Icon::from(registry::NAV_SETTINGS)),
            )),
        )
        .child(
            rail_row_lead(
                "sg-sidebar-agent-2",
                lead(Icon::from(registry::NAV_REPORT_BUG).text_color(gpui::rgb(0x8b5cf6))),
                "Triage",
                false,
                None,
                cx,
            )
            .child(rail_trailing_slot()),
        )
}
