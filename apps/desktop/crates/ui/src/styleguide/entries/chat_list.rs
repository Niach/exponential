//! Session 58cb32df (#EXP-1373): `chat-list` (Special components): THE
//! Chats list's pieces (`fixtures/chats.json` LIST, #FEED-191). #EXP-1380: the
//! IDE's Chats page paints the SAME list ([`crate::chat_list`], shared with
//! this entry), so this twin draws the web entry's phone specimen with those
//! pieces, fed the fixture's own expected model (`listCases[0]`, S-3
//! `agentRowCases[0]`): the large title with the search and inbox circles (the
//! sky dot), no rings, no chips: the pinned agent rows under the `Agents`
//! header (the avatar disc: the brand mark for the Assistant), #EXP-1733 the
//! app rows under the `Apps` header (`appRowCases[0]`, the app's glyph in
//! the disc), then `chat_sections` under their headers: the 52px lead disc, the title + time,
//! the two-line muted preview, the row word and the unread badge, a hairline
//! under the text column only.

use gpui::{div, px, AnyElement, App, Div, FontWeight, IntoElement as _, ParentElement as _, SharedString, Styled as _, Window};
use gpui::prelude::FluentBuilder as _;
use gpui_component::{h_flex, v_flex, ActiveTheme as _, Icon, Sizable as _};

use domain::chats::{chat_sections, AgentChatRow, AppChatRow, ChatList, CHATS};

use crate::chat_list::{agent_chat_row, app_chat_row, chat_lead_disc, chat_row, chat_section_header};
use crate::icons::registry;

pub(crate) const ID: &str = "chat-list";
pub(crate) const OWNER: &str = "EXP-1373";

/// The phone column (the web island's 390px).
const PHONE_W: f32 = 390.;

const CHATS_JSON: &str = include_str!("../../../../../../../packages/domain-contract/fixtures/chats.json");

/// The fixture's own expected model: `listCases[0]` + S-3 `agentRowCases[0]`
/// (the pinned agent rows, the Assistant first).
pub(crate) fn model() -> (ChatList, Vec<AgentChatRow>) {
    let fixture: serde_json::Value = serde_json::from_str(CHATS_JSON).expect("chats.json parses");
    let list = serde_json::from_value(fixture["listCases"][0]["expected"].clone()).expect("list");
    let agent_rows = serde_json::from_value(fixture["agentRowCases"][0]["expected"].clone()).expect("agent rows");
    (list, agent_rows)
}

/// #EXP-1733: the fixture's app rows (`appRowCases[0]`).
pub(crate) fn app_rows() -> Vec<AppChatRow> {
    let fixture: serde_json::Value = serde_json::from_str(CHATS_JSON).expect("chats.json parses");
    serde_json::from_value(fixture["appRowCases"][0]["expected"].clone()).expect("app rows")
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
    let (list, agent_rows) = model();
    let app_rows = app_rows();
    let sections = chat_sections(&list.rows);
    v_flex()
        .w(px(PHONE_W))
        .p_4()
        .gap_4()
        .rounded_xl()
        .bg(cx.theme().background)
        .child(header(cx))
        .child(
            v_flex()
                .w_full()
                .when(!agent_rows.is_empty(), |list| {
                    list.child(chat_section_header(&CHATS.copy.sections.agents, cx))
                        .children(agent_rows.iter().map(|row| agent_chat_row(row, cx).into_any_element()))
                })
                .when(!app_rows.is_empty(), |list| {
                    list.child(chat_section_header(&CHATS.copy.sections.apps, cx))
                        .children(app_rows.iter().map(|row| app_chat_row(row, cx).into_any_element()))
                })
                .children(sections.iter().map(|section| {
                    v_flex()
                        .w_full()
                        .child(chat_section_header(section.key.title(), cx))
                        .children(section.rows.iter().map(|row| {
                            chat_row(row, chat_lead_disc(row.lead, None, false, cx), cx).into_any_element()
                        }))
                })),
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The specimen reads the fixture's expected model.
    #[test]
    fn the_specimen_is_the_fixture_model() {
        let (list, agent_rows) = model();
        assert_eq!(list.rows.len(), 4);
        assert!(!agent_rows.is_empty());
        assert!(agent_rows[0].assistant, "the Assistant leads the pinned agent rows");
        let apps = app_rows();
        assert_eq!(apps.iter().map(|row| row.slug.as_str()).collect::<Vec<_>>(), ["budget", "triage-board", "working-hours"]);
    }
}
