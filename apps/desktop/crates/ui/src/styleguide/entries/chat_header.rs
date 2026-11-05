//! #EXP-1688: `chat-header` (Special components): THE chat header on a phone
//! (`fixtures/chats.json` `shell.chatHeader`). The IDE has no phone shell, so
//! this twin paints the web entry's specimen statically: the ghost back
//! button, the lead disc, the title over one subtitle line, then an agent
//! chat's telephone and the `…`.

use gpui::{div, px, AnyElement, App, Div, FontWeight, Hsla, IntoElement as _, ParentElement as _, SharedString, Styled as _, Window};
use gpui_component::{h_flex, v_flex, ActiveTheme as _, Icon, Sizable as _};

use domain::chats::{ChatLead, CHATS};

use crate::chat_list::chat_lead_disc;
use crate::icons::{registry, ExpIcon};

pub(crate) const ID: &str = "chat-header";
pub(crate) const OWNER: &str = "EXP-1688";

/// The phone column (the web island's 390px).
const PHONE_W: f32 = 390.;

fn button(icon: ExpIcon, cx: &App) -> AnyElement {
    let spec = &CHATS.shell.chat_header;
    div()
        .size(px(spec.button))
        .flex_shrink_0()
        .flex()
        .items_center()
        .justify_center()
        .child(Icon::new(icon).with_size(px(spec.glyph)).text_color(cx.theme().muted_foreground))
        .into_any_element()
}

fn header(lead: AnyElement, title: &str, subtitle: &str, tone: Hsla, call: bool, cx: &App) -> AnyElement {
    let spec = &CHATS.shell.chat_header;
    let mut row = h_flex()
        .w_full()
        .h(px(spec.row))
        .px(px(spec.padding_x))
        .gap(px(spec.gap))
        .items_center()
        .child(button(registry::UI_BACK, cx))
        .child(
            h_flex().flex_1().min_w_0().gap(px(spec.disc_gap)).items_center().child(lead).child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .child(
                        div()
                            .truncate()
                            .text_size(px(spec.title))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(cx.theme().foreground)
                            .child(SharedString::from(title.to_string())),
                    )
                    .child(div().truncate().text_size(px(spec.subtitle)).text_color(tone).child(SharedString::from(subtitle.to_string()))),
            ),
        );
    if call {
        row = row.child(button(registry::VOICE_CALL, cx));
    }
    row.child(button(registry::UI_MORE, cx)).into_any_element()
}

fn disc(lead: ChatLead, cx: &App) -> AnyElement {
    let size = CHATS.shell.chat_header.disc;
    div().size(px(size)).flex_shrink_0().child(chat_lead_disc(lead, Some(coding::CodingAgent::Claude), false, cx)).into_any_element()
}

pub(crate) fn render(_window: &mut Window, cx: &mut App) -> Div {
    let muted = cx.theme().muted_foreground;
    let caption = |text: &'static str| div().text_xs().text_color(muted).child(text);
    let foreground = cx.theme().foreground;
    v_flex()
        .w(px(PHONE_W))
        .p_4()
        .gap_3()
        .rounded_xl()
        .bg(cx.theme().background)
        .child(caption("A chat that needs you (48px, ghost buttons, always leading)"))
        .child(header(
            disc(ChatLead::NeedsYou, cx),
            "Reduce cold start",
            "needs input · Alex's MacBook",
            theme::tokens::YELLOW.to_hsla(),
            false,
            cx,
        ))
        .child(caption("A working chat"))
        .child(header(
            disc(ChatLead::Working, cx),
            "A title long enough that it truncates on one line",
            &CHATS.copy.preview.working,
            foreground,
            false,
            cx,
        ))
        .child(caption("An agent chat (the telephone)"))
        .child(header(disc(ChatLead::Waiting, cx), "Triage", "Agent", muted, true, cx))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The header's measures are the fixture's.
    #[test]
    fn the_header_follows_the_shell() {
        let spec = &CHATS.shell.chat_header;
        assert_eq!((spec.row, spec.button, spec.disc), (48., 36., 40.));
    }
}
