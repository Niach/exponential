//! Thread 58cb32df (#EXP-1373): `chat-list` (Special components): THE
//! Chats list's pieces (`fixtures/chats.json` LIST + BUBBLES). #EXP-1380: the
//! IDE's Chats page paints the SAME list ([`crate::chat_list`], shared with
//! this entry), so this twin draws the web entry's phone specimen with those
//! pieces, fed the fixture's own expected model (`listCases[0]`,
//! `bubbleCases[0]`): the
//! large title with the search and inbox circles (the sky dot), the agent
//! bubbles (66px, a 3px ring around a background gap: amber = needs you,
//! white = unread, a hairline = read), the filter chips (select Pills, a
//! count after a non-zero one), then the flat rows: the 52px lead disc, the
//! title + time, the two-line muted preview, the row word and the unread
//! badge, a hairline under the text column only.

use gpui::{div, px, AnyElement, App, Div, FontWeight, IntoElement as _, ParentElement as _, SharedString, Styled as _, Window};
use gpui_component::{h_flex, v_flex, ActiveTheme as _, Icon, Sizable as _};

use domain::chats::{AgentBubble, ChatFilter, ChatList, CHATS};

use crate::chat_list::{agent_bubbles_row, chat_filter_chips, chat_lead_disc, chat_row};
use crate::icons::registry;

pub(crate) const ID: &str = "chat-list";
pub(crate) const OWNER: &str = "EXP-1373";

/// The phone column (the web island's 390px).
const PHONE_W: f32 = 390.;

const CHATS_JSON: &str = include_str!("../../../../../../../packages/domain-contract/fixtures/chats.json");

/// The fixture's own expected model: `listCases[0]` + `bubbleCases[0]`.
fn model() -> (ChatList, Vec<AgentBubble>) {
    let fixture: serde_json::Value = serde_json::from_str(CHATS_JSON).expect("chats.json parses");
    let list = serde_json::from_value(fixture["listCases"][0]["expected"].clone()).expect("list");
    let bubbles = serde_json::from_value(fixture["bubbleCases"][0]["expected"].clone()).expect("bubbles");
    (list, bubbles)
}

fn circle(icon: crate::icons::ExpIcon, dot: bool, cx: &App) -> AnyElement {
    div()
        .relative()
        .size(px(40.))
        .rounded_full()
        .flex()
        .items_center()
        .justify_center()
        .bg(theme::tokens::glass::FILL_CARD.to_hsla())
        .border_1()
        .border_color(theme::tokens::glass::STROKE_CARD.to_hsla())
        .child(Icon::new(icon).with_size(px(18.)).text_color(cx.theme().foreground))
        .when_dot(dot)
        .into_any_element()
}

trait DotExt {
    fn when_dot(self, dot: bool) -> Self;
}

impl DotExt for Div {
    /// The inbox circle's sky dot.
    fn when_dot(self, dot: bool) -> Self {
        if !dot {
            return self;
        }
        self.child(
            div()
                .absolute()
                .top(px(6.))
                .right(px(6.))
                .size(px(8.))
                .rounded_full()
                .bg(theme::tokens::BLUE.to_hsla()),
        )
    }
}

fn header(cx: &App) -> AnyElement {
    h_flex()
        .w_full()
        .items_center()
        .gap_2()
        .child(
            div()
                .flex_1()
                .text_3xl()
                .font_weight(FontWeight::BOLD)
                .text_color(cx.theme().foreground)
                .child(SharedString::from(CHATS.copy.title.clone())),
        )
        .child(circle(registry::NAV_SEARCH, false, cx))
        .child(circle(registry::NAV_INBOX, true, cx))
        .into_any_element()
}

pub(crate) fn render(_window: &mut Window, cx: &mut App) -> Div {
    let (list, bubbles) = model();
    v_flex()
        .w(px(PHONE_W))
        .p_4()
        .gap_4()
        .rounded_xl()
        .bg(cx.theme().background)
        .child(header(cx))
        .children(agent_bubbles_row(&bubbles, |_, _, bubble| bubble.into_any_element(), cx))
        .child(chat_filter_chips(&list.counts, ChatFilter::All, None, cx))
        .child(v_flex().w_full().children(list.rows.iter().map(|row| {
            chat_row(row, chat_lead_disc(row.lead, None, false, cx), cx).into_any_element()
        })))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The specimen reads the fixture's expected model.
    #[test]
    fn the_specimen_is_the_fixture_model() {
        let (list, bubbles) = model();
        assert_eq!(list.rows.len(), 4);
        assert_eq!(list.counts.needs_you, 1);
        assert_eq!(bubbles.len(), 3);
    }
}
