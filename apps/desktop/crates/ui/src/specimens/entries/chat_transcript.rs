//! Session 58cb32df (#EXP-1373): `chat-transcript` (Special components): the
//! chat's transcript on the IDE (`fixtures/chats.json` TRANSCRIPT + SURFACE
//! CARD), the twin of web `chat-transcript.tsx` over the same rows: the IDE
//! keeps its transcript (the person's bubble right, the agent's prose), a
//! person's `/compact` / `/new` = THE divider pill
//! ([`crate::surface_card::divider_row`]), an agent's preview = THE surface
//! card ([`crate::surface_card::surface_row`], the plan card's painter in its
//! own glass frame; its presses `reply`), a plan the agent replaced paints
//! collapsed to `copy.superseded`, and a WAITING plan stays inline with its
//! decision (FEED-140). Every row goes through the chat screen's own
//! painters ([`crate::session_chat::card_row`]). Last, the surface card's
//! ENTER frame a third of the way in (fade + rise + the primary hairline).

use std::collections::BTreeMap;

use gpui::{div, px, AnyElement, App, Div, Entity, IntoElement as _, ParentElement as _, SharedString, Styled as _, Window};
use gpui_component::v_flex;
use serde_json::json;

use domain::chats::{DividerKind, CHATS};
use domain::session_tree::{CardSurface, SessionChatRow, SessionXref};

use crate::markdown::ImageCache;
use domain::message_delivery::{bubble_actions, DeliveryState};

use crate::session_chat::{card_row, message_bubble, message_markdown, BubbleMenu, ChatText, MessageBubble};

pub(crate) const ID: &str = "chat-transcript";
pub(crate) const OWNER: &str = "EXP-1373";

/// The agent's inline preview (the core catalog), its two presses `reply`
/// (the web entry's `LAUNCH_SURFACE`).
pub(crate) fn launch_surface() -> CardSurface {
    let reply = |text: &str| json!({"press": {"event": {"name": CHATS.surface_card.reply_event, "context": {"text": text}}}});
    CardSurface {
        version: None,
        root: json!({
            "id": "root", "component": "Stack", "props": {"gap": "md"},
            "children": [
                {"id": "stat", "component": "StatTile", "children": [], "props": {
                    "label": "Cold start, Pixel 6a", "value": "740 ms", "delta": "−47 %", "tone": "success",
                    "trend": [1400, 1310, 1180, 980, 820, 740]}},
                {"id": "actions", "component": "Stack", "props": {"direction": "horizontal", "gap": "sm"}, "children": [
                    {"id": "ship", "component": "Button", "props": {"label": "Ship snapshot"}, "children": [], "on": reply("Ship the snapshot")},
                    {"id": "keep", "component": "Button", "props": {"label": "Keep profiling", "variant": "secondary"}, "children": [], "on": reply("Keep profiling")}
                ]}
            ]
        }),
        data: Some(json!({})),
    }
}

pub(crate) fn message(id: &str, author: &str, body: &str) -> SessionChatRow {
    SessionChatRow::Message {
        id: id.into(),
        author: author.into(),
        author_id: None,
        body: body.into(),
        queued: false,
        author_session_id: None,
        xref: None,
    }
}

fn event(id: &str, kind: &str, text: &str) -> SessionChatRow {
    SessionChatRow::Event { id: id.into(), event: kind.into(), text: text.into(), session_id: None }
}

fn plan(id: &str, text: &str, decision: Option<&str>) -> SessionChatRow {
    SessionChatRow::Plan {
        id: id.into(),
        session_id: "sg-chat".into(),
        plan: text.into(),
        questions: Vec::new(),
        surface: None,
        decision: decision.map(str::to_string),
        fresh: false,
        answers: None::<BTreeMap<String, Vec<String>>>,
        feedback: None,
        surface_data: None,
    }
}

/// The web entry's rows (#EXP-1553: exactly web's `TRANSCRIPT_ROWS`).
pub(crate) fn rows() -> Vec<(SessionChatRow, &'static str)> {
    vec![
        (message("m1", "user", "Cold start is at 1.4 s on the Pixel 6a. Profile the launch path and cut the sync bootstrap out of it."), "21:02"),
        (plan("p-old", "# Plan\nProfile first", Some("superseded")), ""),
        (message("m2", "agent", "Deferred the shape subscribe until after the first frame and added a board snapshot cache. Here's where the time went:"), ""),
        (
            SessionChatRow::Surface {
                id: "s1".into(),
                session_id: "sg-chat".into(),
                title: Some("Launch path".into()),
                surface: launch_surface(),
            },
            "",
        ),
        (SessionChatRow::Divider { id: "d1".into(), kind: DividerKind::Compact, at: String::new() }, "18:20"),
        (message("m3", "user", "Anything new overnight?"), "09:02"),
        (message("m4", "agent", "Three reports, all about the iPad sign-in. I linked them to #APP-19."), ""),
        // #EXP-1772: a busy agent chat: a schedule firing the agent answered
        // (THE marker above the reply), a merge line, a relay (its name above).
        (event("e1", "trigger_fired", "Schedule"), ""),
        (message("m6", "agent", "PR #412 merged overnight; the iPad sign-in reports stopped."), "07:00"),
        (event("e2", "pr_merged", "Merged PR #412"), ""),
        (
            SessionChatRow::Message {
                id: "x2".into(),
                author: "agent".into(),
                author_id: None,
                body: "Verified on the iPad: sign-in keeps the return URL, and the redirect loop is gone on Safari too.".into(),
                queued: false,
                author_session_id: Some("c-fix".into()),
                xref: Some(SessionXref {
                    kind: "message".into(),
                    session_id: "c-fix".into(),
                    name: "Fix the sign-in redirect".into(),
                    agent_id: None,
                }),
            },
            "08:41",
        ),
        (SessionChatRow::Divider { id: "d2".into(), kind: DividerKind::New, at: String::new() }, "09:30"),
    ]
}

/// #FEED-237 (message-delivery.json): the viewer's own bubbles at the
/// bottom in all four states (the clock, one check, two checks, the red
/// mark), each with its menu where `bubbleActions` offers one: directly
/// under the FIRST own message (m1, `read`), the same three as iOS, web
/// and Android.
pub(crate) const OWN_AFTER: &str = "m1";
pub(crate) fn own_rows() -> Vec<(&'static str, &'static str, &'static str, DeliveryState, bool)> {
    vec![
        ("o1", "Use the Pixel 6a numbers as the baseline.", "21:02", DeliveryState::Sent, true),
        ("o2", "And keep the cache behind a flag.", "21:03", DeliveryState::Sending, false),
        ("o3", "Ping me when it's under a second.", "21:03", DeliveryState::Failed, false),
    ]
}

fn paint_own(id: &str, body: &str, time: &str, state: DeliveryState, stored: bool, images: &Entity<ImageCache>, cx: &mut App) -> AnyElement {
    let text = ChatText::default();
    let markdown = message_markdown(SharedString::from(format!("sg-chat-own-{id}")), body, &text, images);
    let actions = bubble_actions(state, stored);
    message_bubble(
        MessageBubble {
            id: SharedString::from(format!("sg-chat-own-bubble-{id}")),
            body: markdown.into_any_element(),
            time: time.to_string(),
            own: true,
            name: None,
            link: None,
            status: Some(state),
            menu: (!actions.is_empty()).then(|| BubbleMenu { actions, on_pick: std::rc::Rc::new(|_, _, _| {}) }),
            width: None,
        },
        cx,
    )
}

pub(crate) fn paint(row: &SessionChatRow, time: &str, images: &Entity<ImageCache>, window: &Window, cx: &mut App) -> AnyElement {
    let text = ChatText::default().answering(true);
    match row {
        // #EXP-1736: THE bubble: the viewer's own right, the agent's left.
        SessionChatRow::Message { id, author, body, queued, xref, .. } => {
            // A relay (another session's message) names its session, `name ›`.
            let relay = xref.as_ref().filter(|xref| xref.kind == "message");
            let markdown = message_markdown(SharedString::from(format!("sg-chat-message-{id}")), body, &text, images);
            message_bubble(
                MessageBubble {
                    id: SharedString::from(format!("sg-chat-bubble-{id}")),
                    body: markdown.into_any_element(),
                    time: time.to_string(),
                    own: author == domain::contract::MESSAGE_AUTHOR_KIND_USER,
                    name: relay.map(|xref| format!("{} {}", xref.name, domain::transcript_grouping::copy().link_chevron)),
                    link: relay.map(|xref| xref.session_id.clone()),
                    // #FEED-237: a consumed own row reads two checks.
                    status: (author == domain::contract::MESSAGE_AUTHOR_KIND_USER).then(|| {
                        if *queued {
                            DeliveryState::Sending
                        } else {
                            DeliveryState::Read
                        }
                    }),
                    menu: None,
                    width: None,
                },
                cx,
            )
        }
        SessionChatRow::Divider { id, kind, .. } => crate::surface_card::divider_pill(id, *kind, time, cx),
        _ => card_row(row, "sg-chat", &[], &text, images, window, cx).unwrap_or_else(|| div().into_any_element()),
    }
}

fn rows_list() -> Vec<(SessionChatRow, &'static str)> {
    rows()
}

pub(crate) fn render(window: &mut Window, cx: &mut App) -> Div {
    let images = window.use_keyed_state("sg-chat-transcript-images", cx, |_, _| ImageCache::new(None)).clone();
    let mut rows: Vec<AnyElement> = Vec::new();
    for (row, time) in rows_list() {
        rows.push(paint(&row, time, &images, window, cx));
        if matches!(&row, SessionChatRow::Message { id, .. } if id == OWN_AFTER) {
            rows.extend(own_rows().into_iter().map(|(id, body, time, state, stored)| paint_own(id, body, time, state, stored, &images, cx)));
        }
    }
    v_flex()
        // #EXP-1547: THE chat column's content box (the token inside the gutter).
        // Bounded, not fixed: the 1600 token is wider than the capture window.
        .w_full()
        .max_w(px(crate::work_header::CHAT_COLUMN_W - 2. * crate::work_header::WORK_GUTTER))
        .gap_5()
        .children(rows)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The specimen covers every new row kind and the superseded plan.
    #[test]
    fn the_rows_cover_the_new_kinds() {
        let rows = rows();
        assert!(rows.iter().any(|(row, _)| matches!(row, SessionChatRow::Surface { .. })));
        assert!(rows.iter().any(|(row, _)| matches!(row, SessionChatRow::Divider { kind: DividerKind::New, .. })));
        assert!(rows.iter().any(|(row, _)| matches!(row, SessionChatRow::Divider { kind: DividerKind::Compact, .. })));
        assert!(rows.iter().any(|(row, _)| matches!(row, SessionChatRow::Plan { decision: Some(d), .. } if d == "superseded")));
    }

    /// #FEED-237: m1 (an own, consumed row = `read`) is followed by one
    /// own bubble per other delivery state.
    #[test]
    fn the_own_bubbles_cover_every_delivery_state() {
        let states: Vec<DeliveryState> = own_rows().into_iter().map(|(_, _, _, state, _)| state).collect();
        assert_eq!(states, vec![DeliveryState::Sent, DeliveryState::Sending, DeliveryState::Failed]);
        assert!(rows().iter().any(|(row, _)| matches!(row, SessionChatRow::Message { id, author, queued: false, .. } if id == OWN_AFTER && author == "user")));
    }
}
