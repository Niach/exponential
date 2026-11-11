//! #EXP-1384: `phone-page-header` (Special components): THE phone page
//! header (`fixtures/chats.json` `shell.header`) on every top-level phone
//! page. The IDE has no phone shell, so this twin paints the web entry's
//! specimen statically: the large title on the left and the round trailing
//! circles (`header.buttons`: search, then the inbox wearing a sky dot while
//! a notification is unread), `header.circle` px FAB-chrome circles. Three
//! headers: Chats with the dot, the board's name without it, Devices (pushed
//! from the Team page) with the back control before the title.

use gpui::{div, px, AnyElement, App, Div, FontWeight, IntoElement as _, ParentElement as _, SharedString, Styled as _, Window};
use gpui_component::{h_flex, v_flex, ActiveTheme as _, Icon, Sizable as _};

use domain::chats::CHATS;

use super::phone_tab_bar::BOARDS;
use crate::icons::registry;

pub(crate) const ID: &str = "phone-page-header";
pub(crate) const OWNER: &str = "EXP-1384";

/// The phone column (the web island's 390px).
const PHONE_W: f32 = 390.;

fn circle(icon: crate::icons::ExpIcon, dot: bool, cx: &App) -> AnyElement {
    let size = CHATS.shell.header.circle;
    let mut circle = div()
        .relative()
        .size(px(size))
        .flex_shrink_0()
        .rounded_full()
        .flex()
        .items_center()
        .justify_center()
        .bg(theme::tokens::glass::FILL_CARD.to_hsla())
        .border_1()
        .border_color(theme::tokens::glass::STROKE_CARD.to_hsla())
        .child(Icon::new(icon).with_size(px(20.)).text_color(cx.theme().foreground));
    if dot {
        circle = circle.child(
            div()
                .absolute()
                .top(px(8.))
                .right(px(8.))
                .size(px(10.))
                .rounded_full()
                .border_2()
                .border_color(cx.theme().background)
                .bg(theme::tokens::BLUE.to_hsla()),
        );
    }
    circle.into_any_element()
}

fn header(title: &str, back: bool, unread: bool, cx: &App) -> AnyElement {
    let mut row = h_flex().w_full().items_center().gap_3();
    if back {
        row = row.child(
            div()
                .size(px(36.))
                .ml(px(-8.))
                .flex()
                .items_center()
                .justify_center()
                .child(Icon::new(registry::UI_BACK).with_size(px(20.)).text_color(cx.theme().muted_foreground)),
        );
    }
    row.child(
        div()
            .flex_1()
            .min_w_0()
            .text_size(px(34.))
            .font_weight(FontWeight::BOLD)
            .text_color(cx.theme().foreground)
            .child(SharedString::from(title.to_string())),
    )
    .child(circle(registry::NAV_SEARCH, false, cx))
    .child(circle(registry::NAV_INBOX, unread, cx))
    .into_any_element()
}

pub(crate) fn render(_window: &mut Window, cx: &mut App) -> Div {
    let muted = cx.theme().muted_foreground;
    let caption = |text: &'static str| div().text_xs().text_color(muted).child(text);
    let headers = &CHATS.copy.headers;
    v_flex()
        .w(px(PHONE_W))
        .p_4()
        .gap_4()
        .rounded_xl()
        .bg(cx.theme().background)
        .child(caption("Unread notification (the inbox dot)"))
        .child(header(&headers.chats, false, true, cx))
        .child(caption("All read"))
        .child(header(BOARDS[0].0, false, false, cx))
        .child(caption("Pushed (Devices, the back control)"))
        .child(header(&headers.devices, true, false, cx))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The circles are the fixture's, in order.
    #[test]
    fn the_header_follows_the_shell() {
        assert_eq!(CHATS.shell.header.buttons, ["nav-search", "nav-inbox"]);
        assert_eq!(CHATS.shell.header.circle, 44.);
    }
}
