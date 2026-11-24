//! Session 58cb32df (#EXP-1373): `chat-transcript` (Special components): the
//! chat's transcript on the IDE (`fixtures/chats.json` TRANSCRIPT + SURFACE
//! CARD), the twin of web `chat-transcript.tsx` over the same rows: the IDE
//! keeps its transcript (the person's bubble right, the agent's prose), a
//! person's `/compact` / `/new` = THE divider pill
//! ([`crate::surface_card::divider_row`]), an agent's preview = THE surface
//! card ([`crate::surface_card::surface_row`], the plan card's painter in its
//! own glass frame; its presses `reply`), a plan the agent replaced paints
//! collapsed to `copy.superseded`, and a WAITING plan stays inline with its
//! decision (FEED-140). #EXP-1864: the agent's bubbles wear their `HH:mm`,
//! and the agent's quiet turn is THE inline marker on its own line. #EXP-1873:
//! on top `checkInSpecimen` through `feed_rows` (quiet check-ins = ONE line
//! per run, the one that answered keeps its marker). Every row goes through the chat screen's own
//! painters ([`crate::session_chat::card_row`]). Last, the surface card's
//! ENTER frame a third of the way in (fade + rise + the primary hairline).

use std::collections::BTreeMap;

use gpui::{div, px, AnyElement, App, Div, Entity, IntoElement as _, ParentElement as _, SharedString, Styled as _, Window};
use gpui_component::{h_flex, v_flex};
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
        action: None,
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
        (message("m2", "agent", "Deferred the shape subscribe until after the first frame and added a board snapshot cache. Here's where the time went:"), "21:14"),
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
        (message("m4", "agent", "Three reports, all about the iPad sign-in. I linked them to #APP-19."), "09:03"),
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
                action: None,
            },
            "08:41",
        ),
        (SessionChatRow::Divider { id: "d2".into(), kind: DividerKind::New, at: String::new() }, "09:30"),
    ]
}

/// #EXP-1864 NOTICES: right under the agent's genUI card its quiet turn
/// (`exponential_session_self_quiet`) = THE inline marker on its own line.
pub(crate) const NOTICE_AFTER: &str = "s1";
pub(crate) const NOTICE_TOOL: &str = "mcp__exponential__exponential_session_self_quiet";

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
            // #EXP-1876: they follow m1 in its own group.
            tail: false.into(),
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
                    // #EXP-1876: every message here starts its group (a
                    // side change or a card/marker between them).
                    tail: true.into(),
                },
                cx,
            )
        }
        SessionChatRow::Divider { id, kind, .. } => crate::surface_card::divider_pill(id, *kind, time, cx),
        _ => card_row(row, "sg-chat", &[], &text, images, window, cx).unwrap_or_else(|| div().into_any_element()),
    }
}

/// #EXP-1873 (transcript-grouping.json `checkInSpecimen`): an agent chat's
/// check-ins through `feed_rows`: quiet runs = ONE line each, the one that
/// answered keeps its marker above the agent's bubble.
fn check_in_rows(images: &Entity<ImageCache>, cx: &mut App) -> Vec<AnyElement> {
    use domain::transcript_grouping::{check_in_label, check_in_specimen, copy, feed_rows, trace_notice, FeedRowRole, FeedRowShown};
    let rows = check_in_specimen();
    let agent = domain::transcript_grouping::check_in_specimen_agent();
    let kinds: Vec<FeedRowRole> = rows.iter().map(|row| FeedRowRole::parse(&row.kind).unwrap_or(FeedRowRole::Other)).collect();
    let text = ChatText::default();
    let mut out = Vec::new();
    for (index, (row, drawn)) in rows.iter().zip(feed_rows(&kinds)).enumerate() {
        let id = SharedString::from(format!("sg-chat-check-in-{index}"));
        let marker = |label: &str, cx: &mut App| crate::surface_card::transcript_marker(id.clone(), label, None, cx);
        out.push(match (drawn, kinds[index]) {
            (FeedRowShown::Hidden, _) => continue,
            (FeedRowShown::CheckIn(count), _) => marker(&check_in_label(count, row.time.as_deref()), cx),
            (_, FeedRowRole::Schedule) => marker(copy().markers.get("schedule").map_or("", String::as_str), cx),
            (_, FeedRowRole::Quiet) => marker(trace_notice(NOTICE_TOOL).unwrap_or_default(), cx),
            _ => {
                let body = row.text.clone().unwrap_or_default();
                let markdown = message_markdown(SharedString::from(format!("sg-chat-check-in-text-{index}")), &body, &text, images);
                let bubble = message_bubble(
                    MessageBubble {
                        id: SharedString::from(format!("sg-chat-check-in-bubble-{index}")),
                        body: markdown.into_any_element(),
                        time: row.time.clone().unwrap_or_default(),
                        own: false,
                        name: Some(agent.name.clone()),
                        link: None,
                        status: None,
                        menu: None,
                        width: None,
                        // #EXP-1873: a real agent bubble: its group's first, the tail.
                        tail: true.into(),
                    },
                    cx,
                );
                // ... under the agent's own avatar (`sender_row`'s column).
                let sizes = crate::session_chat::transcript_sizes();
                let avatar = crate::user_avatar::agent_avatar(
                    agent.icon.as_deref(),
                    agent.color.as_deref(),
                    gpui_component::Size::Size(px(sizes.avatar)),
                    cx,
                );
                h_flex()
                    .w_full()
                    .min_w_0()
                    .items_start()
                    .gap(px(sizes.avatar_gap))
                    .child(div().flex_shrink_0().w(px(sizes.avatar)).child(avatar))
                    .child(v_flex().flex_1().min_w_0().child(bubble))
                    .into_any_element()
            }
        });
    }
    out
}

// ---------------------------------------------------------------------------
// #EXP-1943 (session-card.json `call`): a call in the agent chat = ONE
// session card (an ended call with its summary, a running one), from the
// session-chat.json case's input through `build_session_chat`, and THE call
// view (`callViewCases[0]` through `call_view`), on top of the specimen.
// ---------------------------------------------------------------------------

const SESSION_CHAT_JSON: &str = include_str!("../../../../../../../packages/domain-contract/fixtures/session-chat.json");
const SESSION_CARD_JSON: &str = include_str!("../../../../../../../packages/domain-contract/fixtures/session-card.json");

/// A fixture row (camelCase keys) as the synced row (snake_case).
fn fixture_row<T: serde::de::DeserializeOwned>(value: &serde_json::Value) -> Option<T> {
    let mut out = serde_json::Map::new();
    for (key, cell) in value.as_object()? {
        let mut snake = String::new();
        for ch in key.chars() {
            if ch.is_ascii_uppercase() {
                snake.push('_');
                snake.push(ch.to_ascii_lowercase());
            } else {
                snake.push(ch);
            }
        }
        out.insert(snake, cell.clone());
    }
    serde_json::from_value(serde_json::Value::Object(out)).ok()
}

fn fixture_messages(value: &serde_json::Value) -> Vec<domain::rows::MessageRow> {
    value
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|item| {
            let session_id = item["sessionId"].as_str().unwrap_or_default().to_string();
            let mut item = item.clone();
            let object = item.as_object_mut()?;
            object.entry("conversationId").or_insert_with(|| json!(format!("c-{session_id}")));
            let locked = object.remove("locked").is_some();
            let mut message: domain::rows::MessageRow = fixture_row(&item)?;
            message.session_id = session_id;
            if locked {
                message.seal = Some(domain::sealed::SealState::Locked);
            }
            Some(message)
        })
        .collect()
}

fn instant(value: &serde_json::Value) -> i64 {
    value.as_str().and_then(domain::instant::parse_instant).unwrap_or_default()
}

/// The call specimen's rows: the chat (its call cards) then the call view.
fn call_rows(images: &Entity<ImageCache>, window: &Window, cx: &mut App) -> Vec<AnyElement> {
    let mut out = Vec::new();
    let chat: serde_json::Value = serde_json::from_str(SESSION_CHAT_JSON).unwrap_or_default();
    let case = chat["cases"].as_array().into_iter().flatten().find(|case| case["name"].as_str().is_some_and(|name| name.starts_with("#EXP-1943")));
    if let Some(input) = case.map(|case| &case["input"]) {
        let sessions: Vec<domain::rows::SessionRow> = input["sessions"].as_array().into_iter().flatten().filter_map(fixture_row).collect();
        let messages = fixture_messages(&input["messages"]);
        if let Some(root) = fixture_row::<domain::rows::SessionRow>(&input["root"]) {
            let session_refs: Vec<&domain::rows::SessionRow> = sessions.iter().collect();
            let message_refs: Vec<&domain::rows::MessageRow> = messages.iter().collect();
            let view = domain::session_tree::build_session_chat(
                domain::session_tree::SessionChatInput {
                    root: &root,
                    sessions: &session_refs,
                    session_issues: &[],
                    issues: &[],
                    messages: &message_refs,
                    pull_requests: &[],
                    now_ms: instant(&input["now"]),
                },
                None,
            );
            for row in &view.rows {
                let time = messages
                    .iter()
                    .find(|message| message.id == row.id())
                    .map(|message| crate::session_chat::clock(message.created_at.as_deref()))
                    .unwrap_or_default();
                out.push(paint(row, &time, images, window, cx));
            }
        }
    }
    let card: serde_json::Value = serde_json::from_str(SESSION_CARD_JSON).unwrap_or_default();
    let input = &card["callViewCases"][0]["input"];
    if let Some(session) = fixture_row::<domain::rows::SessionRow>(&input["session"]) {
        let messages = fixture_messages(&input["messages"]);
        let refs: Vec<&domain::rows::MessageRow> = messages.iter().collect();
        let view = domain::session_tree::call_view(
            &session,
            &refs,
            input["agentName"].as_str().unwrap_or_default(),
            instant(&input["now"]),
            input["utcOffsetMinutes"].as_i64().unwrap_or_default() as i32,
        );
        let times: std::collections::HashMap<String, String> = messages
            .iter()
            .map(|message| (message.id.clone(), crate::session_chat::clock(message.created_at.as_deref())))
            .collect();
        let time = move |id: &str| times.get(id).cloned().unwrap_or_default();
        let agent = crate::call_view::CallAgent { icon: None, color: None };
        out.push(div().pt_4().child(crate::call_view::call_view_body(&view, true, &agent, &time, images, cx)).into_any_element());
    }
    out
}

fn rows_list() -> Vec<(SessionChatRow, &'static str)> {
    rows()
}

pub(crate) fn render(window: &mut Window, cx: &mut App) -> Div {
    let images = window.use_keyed_state("sg-chat-transcript-images", cx, |_, _| ImageCache::new(None)).clone();
    // #EXP-1873: the check-ins lead the specimen (web's order), in frame.
    // #EXP-1943: the call cards and the call view lead it.
    let mut rows: Vec<AnyElement> = call_rows(&images, window, cx);
    rows.extend(check_in_rows(&images, cx));
    for (row, time) in rows_list() {
        rows.push(paint(&row, time, &images, window, cx));
        if matches!(&row, SessionChatRow::Message { id, .. } if id == OWN_AFTER) {
            rows.extend(own_rows().into_iter().map(|(id, body, time, state, stored)| paint_own(id, body, time, state, stored, &images, cx)));
        }
        if matches!(&row, SessionChatRow::Surface { id, .. } if id == NOTICE_AFTER) {
            let label = domain::transcript_grouping::trace_notice(NOTICE_TOOL).unwrap_or_default();
            rows.push(crate::surface_card::transcript_marker(SharedString::from("sg-chat-notice"), label, None, cx));
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

    /// #EXP-1864: the agent's bubbles wear their time; the notice resolves.
    #[test]
    fn the_agent_bubbles_carry_a_time_and_the_notice_resolves() {
        assert!(rows().iter().any(|(row, time)| matches!(row, SessionChatRow::Message { author, .. } if author == "agent") && !time.is_empty()));
        assert_eq!(domain::transcript_grouping::trace_notice(NOTICE_TOOL), Some("Ended turn quietly"));
        assert!(rows().iter().any(|(row, _)| matches!(row, SessionChatRow::Surface { id, .. } if id == NOTICE_AFTER)));
    }

    /// #EXP-1873: the specimen draws two check-in runs and the answered one.
    #[test]
    fn the_check_in_specimen_draws_two_lines_and_the_answer() {
        use domain::transcript_grouping::{check_in_specimen, feed_rows, FeedRowRole, FeedRowShown};
        let kinds: Vec<FeedRowRole> = check_in_specimen().iter().map(|row| FeedRowRole::parse(&row.kind).unwrap()).collect();
        let drawn: Vec<FeedRowShown> = feed_rows(&kinds).into_iter().filter(|shown| *shown != FeedRowShown::Hidden).collect();
        assert_eq!(
            drawn,
            vec![FeedRowShown::CheckIn(4), FeedRowShown::Row, FeedRowShown::Row, FeedRowShown::Row, FeedRowShown::CheckIn(2)]
        );
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
