//! #EXP-1943 (session-card.json `call`, `callView`) THE call view ×4: a call
//! session's own screen — the heading (title over the muted subtitle), the
//! summary under the section band with a hairline under it, then the spoken
//! turns as the chat's bubbles (the agent's under its avatar). Existing parts
//! only: the glass section band, `message_bubble`, the locked pill.

use domain::session_tree::{CallView, SessionChatRow};
use gpui::{div, prelude::FluentBuilder as _, px, AnyElement, App, Entity, InteractiveElement as _, IntoElement, ParentElement as _, SharedString, Styled as _};
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
                let markdown = message_markdown(SharedString::from(format!("call-view-{id}")), body, &text, images);
                let bubble = message_bubble(
                    MessageBubble {
                        id: SharedString::from(format!("call-view-bubble-{id}")),
                        body: markdown.into_any_element(),
                        time: time(id),
                        own,
                        name: None,
                        link: None,
                        status: own.then_some(domain::message_delivery::DeliveryState::Read),
                        menu: None,
                        width: None,
                        tail: first.into(),
                    },
                    cx,
                );
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
        .when_some(view.summary.clone(), |this, summary| {
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
