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
use crate::session_chat::{card_row, message_bubble, message_markdown, ChatText, MessageBubble};

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
                    queued: *queued,
                    unqueue: None,
                    width: None,
                },
                cx,
            )
        }
        SessionChatRow::Divider { id, kind, .. } => crate::surface_card::divider_pill(id, *kind, time, cx),
        _ => card_row(row, "sg-chat", &[], &text, images, window, cx).unwrap_or_else(|| div().into_any_element()),
    }
}

pub(crate) fn render(window: &mut Window, cx: &mut App) -> Div {
    let images = window.use_keyed_state("sg-chat-transcript-images", cx, |_, _| ImageCache::new(None)).clone();
    let rows: Vec<AnyElement> = rows().iter().map(|(row, time)| paint(row, time, &images, window, cx)).collect();
    v_flex()
        // #EXP-1547: THE chat column's content box (the token inside the gutter).
        .w(px(crate::work_header::CHAT_COLUMN_W - 2. * crate::work_header::WORK_GUTTER))
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
}
