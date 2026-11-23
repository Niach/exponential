//! #EXP-1943 (session-card.json `call`, `callView`) THE call view ×4: a call
//! session's own screen — the heading (title over the muted subtitle), the
//! summary under the section band with a hairline under it, then the spoken
//! turns as the chat's bubbles (the agent's under its avatar). Existing parts
//! only: the glass section band, `message_bubble`, the locked pill.
//! #EXP-2076: an agent turn with Express blocks draws them in place like the
//! chat's synced agent rows ([`crate::express_view`]); their actions post into
//! the CALL session (`actions`; an ended call's are inert, as on web). The
//! summary band only reads its text: the pure prose, no band when empty.

use domain::session_tree::{CallView, SessionChatRow};
use gpui::{div, prelude::FluentBuilder as _, px, AnyElement, App, Entity, InteractiveElement as _, IntoElement, ParentElement as _, SharedString, Styled as _, Window};
use gpui_component::{h_flex, v_flex, ActiveTheme as _};

use crate::markdown::ImageCache;
use crate::session_chat::{message_bubble, message_markdown, ChatText, MessageBubble};

/// The chat's agent the call's bubbles come from (its avatar).
pub(crate) struct CallAgent {
    pub icon: Option<String>,
    pub color: Option<String>,
}

/// The call view's body. `heading` = the title + subtitle on top (the
/// screen's header does not carry them); `time` = a row's `HH:mm`.
pub(crate) fn call_view_body(
    view: &CallView,
    heading: bool,
    agent: &CallAgent,
    time: &dyn Fn(&str) -> String,
    images: &Entity<ImageCache>,
    actions: bool,
    window: &Window,
    cx: &mut App,
) -> AnyElement {
    let muted = cx.theme().muted_foreground;
    let text = ChatText::default();
    let sizes = crate::session_chat::transcript_sizes();
    let mut rows: Vec<AnyElement> = Vec::with_capacity(view.rows.len());
    let mut prev_side: Option<bool> = None;
    for row in &view.rows {
        match row {
            SessionChatRow::Message { id, author, body, .. } => {
                let own = author == domain::contract::MESSAGE_AUTHOR_KIND_USER;
                // #EXP-1876: a side change starts a group: its first bubble
                // wears THE tail (and the agent's its avatar).
                let first = prev_side != Some(own);
                prev_side = Some(own);
                let bubble = |key: &str, body: &str, tail: bool, time: String, cx: &mut App| {
                    let markdown = message_markdown(SharedString::from(format!("call-view-{key}")), body, &text, images);
                    message_bubble(
                        MessageBubble {
                            id: SharedString::from(format!("call-view-bubble-{key}")),
                            body: markdown.into_any_element(),
                            time,
                            own,
                            name: None,
                            link: None,
                            status: own.then_some(domain::message_delivery::DeliveryState::Read),
                            menu: None,
                            width: None,
                            tail: tail.into(),
                        },
                        cx,
                    )
                };
                // #EXP-2076 (chat-express.json DRAW, web: the call view IS
                // ChatConversation): an agent's spoken turn is a synced agent
                // `message` row: its Express blocks drawn in place (row key
                // `m:<id>`, final), its prose THE bubbles, ONE sender group
                // (the tail on the first segment, the `HH:mm` on the last
                // prose). Its actions post into the call session (web
                // `CallSessionBody`), inert once it ended.
                let segments = (!own)
                    .then(|| crate::express_view::split(&domain::chat_express::synced_row_key(id), body, true, cx))
                    .flatten();
                let bubble = match segments {
                    None => bubble(id, body, first, time(id), cx),
                    Some(segments) => {
                        let target = crate::express_view::ExpressTarget {
                            session_id: view.session_id.clone(),
                            message_id: Some(id.clone()),
                            row_key: domain::chat_express::synced_row_key(id),
                            actions,
                        };
                        let at = time(id);
                        let mut prose = |index: usize, text: &str, head: bool, last_prose: bool, cx: &mut App| {
                            bubble(&format!("{id}-{index}"), text, first && head, if last_prose { at.clone() } else { String::new() }, cx)
                        };
                        crate::express_view::row_column(&format!("call-view-{id}"), &segments, &target, &mut prose, window, cx)
                    }
                };
                rows.push(if own {
                    bubble
                } else {
                    let avatar = first.then(|| {
                        crate::user_avatar::agent_avatar(
                            agent.icon.as_deref(),
                            agent.color.as_deref(),
                            gpui_component::Size::Size(px(sizes.avatar)),
                            cx,
                        )
                    });
                    h_flex()
                        .w_full()
                        .min_w_0()
                        .items_start()
                        .gap(px(sizes.avatar_gap))
                        .child(div().flex_shrink_0().w(px(sizes.avatar)).children(avatar))
                        .child(v_flex().flex_1().min_w_0().child(bubble))
                        .into_any_element()
                });
            }
            SessionChatRow::Locked { id, reason, .. } => {
                prev_side = None;
                rows.push(crate::surface_card::locked_pill(id, *reason, cx));
            }
            _ => {}
        }
    }
    v_flex()
        .id(SharedString::from(format!("call-view-{}", view.session_id)))
        .w_full()
        .min_w_0()
        .gap_4()
        .when(heading, |this| {
            this.child(
                v_flex()
                    .min_w_0()
                    .child(
                        div()
                            .truncate()
                            .text_sm()
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(cx.theme().foreground)
                            .child(SharedString::from(view.title.clone())),
                    )
                    .child(div().truncate().text_xs().text_color(muted).child(SharedString::from(view.subtitle.clone()))),
            )
        })
        // #EXP-2076 (chat-express.json PROSE): the band only READS the
        // summary: its pure prose, no band when that is empty.
        .when_some(view.summary.as_deref().map(domain::chat_express::plain_prose).filter(|summary| !summary.trim().is_empty()), |this, summary| {
            let markdown = message_markdown(SharedString::from(format!("call-view-summary-{}", view.session_id)), &summary, &text, images);
            this.child(
                v_flex()
                    .w_full()
                    .min_w_0()
                    .pb_4()
                    .border_b_1()
                    .border_color(cx.theme().border)
                    .child(crate::surface::glass_section_header(view.summary_label.clone(), None, cx))
                    .child(div().pt_2().child(markdown)),
            )
        })
        .child(v_flex().w_full().min_w_0().gap_2().children(rows))
        .into_any_element()
}
