//! Thread 58cb32df (#EXP-1373): `chat-list` (Special components): THE
//! Chats list's pieces (`fixtures/chats.json` LIST + BUBBLES). #EXP-1380: the
//! IDE's Chats page paints the SAME list ([`crate::chat_list`], shared with
//! this entry), so this twin draws the web entry's phone specimen with those
//! pieces, fed the fixture's own expected model (`listCases[0]`,
//! `bubbleCases[0]` with the viewer's Assistant first, S-3
//! `agentRowCases[0]`): the
//! large title with the search and inbox circles (the sky dot), the agent
//! bubbles (66px, a 3px ring around a background gap: amber = needs you,
//! white = unread, a hairline = read; S-3: Triage + Designer wear the
//! `story-ring` specimen's rings, its two chat rings follow), the filter chips (select Pills, a
//! count after a non-zero one), then the pinned agent rows under the
//! `Agents` header (the avatar disc: the brand mark for the Assistant) and
//! the `Chats` header over the flat rows: the 52px lead disc, the
//! title + time, the two-line muted preview, the row word and the unread
//! badge, a hairline under the text column only.

use gpui::{div, px, AnyElement, App, Div, FontWeight, IntoElement as _, ParentElement as _, SharedString, Styled as _, Window};
use gpui::prelude::FluentBuilder as _;
use gpui_component::{h_flex, v_flex, ActiveTheme as _, Icon, Sizable as _};

use domain::chats::{AgentBubble, AgentChatRow, ChatFilter, ChatList, CHATS};

use crate::chat_list::{agent_bubbles_row, agent_chat_row, chat_filter_chips, chat_lead_disc, chat_row, chat_section_header};
use crate::icons::registry;
use crate::story_ring::RING_GLYPH_PX;

use super::story_ring::{specimen_avatar, specimen_rings};

pub(crate) const ID: &str = "chat-list";
pub(crate) const OWNER: &str = "EXP-1373";

/// The phone column (the web island's 390px).
const PHONE_W: f32 = 390.;

const CHATS_JSON: &str = include_str!("../../../../../../../packages/domain-contract/fixtures/chats.json");

/// The fixture's own expected model: `listCases[0]` + `bubbleCases[0]` +
/// S-3 `agentRowCases[0]` (the pinned agent rows, the Assistant first).
fn model() -> (ChatList, Vec<AgentBubble>, Vec<AgentChatRow>) {
    let fixture: serde_json::Value = serde_json::from_str(CHATS_JSON).expect("chats.json parses");
    let list = serde_json::from_value(fixture["listCases"][0]["expected"].clone()).expect("list");
    let bubbles = serde_json::from_value(fixture["bubbleCases"][0]["expected"].clone()).expect("bubbles");
    let agent_rows = serde_json::from_value(fixture["agentRowCases"][0]["expected"].clone()).expect("agent rows");
    (list, bubbles, agent_rows)
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
    let (list, bubbles, agent_rows) = model();
    v_flex()
        .w(px(PHONE_W))
        .p_4()
        .gap_4()
        .rounded_xl()
        .bg(cx.theme().background)
        .child(header(cx))
        // S-3 (stories.json specimen): Triage + Designer wear their rings,
        // the two chat rings follow.
        .children(agent_bubbles_row(
            &bubbles,
            &specimen_rings(),
            |ring| specimen_avatar(ring, RING_GLYPH_PX, cx),
            |_, _, _, bubble| bubble.into_any_element(),
            |_, _, ring| ring.into_any_element(),
            cx,
        ))
        .child(chat_filter_chips(&list.counts, ChatFilter::All, None, cx))
        .child(
            v_flex()
                .w_full()
                .when(!agent_rows.is_empty(), |list| {
                    list.child(chat_section_header(&CHATS.copy.sections.agents, cx))
                        .children(agent_rows.iter().map(|row| agent_chat_row(row, cx).into_any_element()))
                        .child(chat_section_header(&CHATS.copy.sections.chats, cx))
                })
                .children(list.rows.iter().map(|row| {
                    chat_row(row, chat_lead_disc(row.lead, None, false, cx), cx).into_any_element()
                })),
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The specimen reads the fixture's expected model.
    #[test]
    fn the_specimen_is_the_fixture_model() {
        let (list, bubbles, agent_rows) = model();
        assert_eq!(list.rows.len(), 4);
        assert_eq!(list.counts.needs_you, 1);
        assert_eq!(bubbles.len(), 4);
        assert!(bubbles[0].agent.assistant, "the viewer's Assistant leads the bubbles");
        assert!(bubbles[1..].iter().all(|bubble| !bubble.agent.assistant));
        assert!(!agent_rows.is_empty());
        assert!(agent_rows[0].assistant, "the Assistant leads the pinned agent rows");
    }

    /// S-3: the story-ring specimen's Triage + Designer rings ride their
    /// bubbles, the two chat rings follow.
    #[test]
    fn the_bubbles_wear_the_specimen_rings() {
        let (_, bubbles, _) = model();
        let ids: Vec<&str> = bubbles.iter().map(|bubble| bubble.agent.agent_id.as_str()).collect();
        let split = domain::stories::bubble_rings(&ids, &specimen_rings());
        let mut worn: Vec<&str> = split.by_agent.keys().map(String::as_str).collect();
        worn.sort();
        assert_eq!(worn, ["ag1", "ag2"]);
        let rest: Vec<&str> = split.rest.iter().map(|ring| ring.title.as_str()).collect();
        assert_eq!(rest, ["Board header polish", "Ship the launch checklist"]);
    }
}
