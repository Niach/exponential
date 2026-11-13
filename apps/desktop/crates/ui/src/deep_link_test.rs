//! waiting-cards.json DEEP LINKS on the desktop: a jump into a card the
//! chat has not loaded (older than its newest 200 messages, #2689/#2707)
//! pages the chat in, waits for the layout, centres the card and rings it;
//! the list never runs back to its newest row meanwhile.

use std::time::Duration;

use gpui::{px, Entity};
use sync::{ShapeRow as _, Store};

use domain::rows::{MessageRow, SessionRow};

use super::SessionChatScreenView;

const MESSAGES: usize = 600;
/// The card: far older than the first page.
const TARGET: usize = 10;

fn id(n: usize) -> String {
    format!("m{n:05}")
}

fn at(n: usize) -> String {
    let secs = n as i64 * 30;
    format!("2026-11-04T{:02}:{:02}:{:02}.000Z", (secs / 3600) % 24, (secs / 60) % 60, secs % 60)
}

fn seed(cx: &mut gpui::TestAppContext) {
    cx.update(|cx| {
        gpui_component::init(cx);
        theme::init(cx);
        let store = Store::open(cx, None, None);
        cx.set_global(store);
        crate::queries::set_own_device_id_for_test("d1", cx);
        let collections = Store::global(cx).collections().clone();
        let root: SessionRow = serde_json::from_value(serde_json::json!({
            "id": "r1", "team_id": "t1", "root_id": "r1", "agent_id": "ag1", "status": "idle", "shared": true,
            "created_at": "2026-11-04T09:00:00.000Z", "updated_at": "2026-11-04T09:00:00.000Z",
        }))
        .unwrap();
        collections.sessions.update(cx, |collection, cx| {
            collection.seed(root.key(), root);
            cx.notify();
        });
        collections.messages.update(cx, |collection, cx| {
            for n in 0..MESSAGES {
                let mut row = serde_json::json!({
                    "id": id(n), "session_id": "r1", "author_session_id": "r1",
                    "author_kind": if n % 2 == 0 { "user" } else { "agent" },
                    "body": format!("Message {n}: a line long enough to take a row of its own in the reading column."),
                    "created_at": at(n),
                });
                if n == TARGET {
                    // An answered question: not a waiting card, so the
                    // first page does not carry it.
                    row["body"] = serde_json::json!("");
                    row["card"] = serde_json::json!({"kind": "question", "question": "Which palette?", "answer": "Zinc"});
                    row["answered_at"] = serde_json::json!(at(n + 1));
                }
                let message: MessageRow = serde_json::from_value(row).unwrap();
                collection.seed(message.key(), message);
            }
            cx.notify();
        });
    });
}

/// The relay's replay of the same chat: a prompt and a reply per pair.
fn feed() -> Vec<steer::ViewerEvent> {
    let mut events = vec![steer::ViewerEvent::Phase(steer::ViewerPhase::Live)];
    for n in 0..MESSAGES {
        let body = format!("Message {n}: a line long enough to take a row of its own in the reading column.");
        let event = if n % 2 == 0 { steer::ActivityEvent::user_message(body) } else { steer::ActivityEvent::narration(body) };
        events.push(steer::ViewerEvent::Activity(n as u64 + 1, event));
    }
    events
}

fn inner(view: &Entity<SessionChatScreenView>, cx: &mut gpui::VisualTestContext) -> Entity<crate::steer_viewer::SteerSessionView> {
    cx.update(|_, cx| view.read(cx).root_view.read(cx).inner().clone())
}

fn frame(cx: &mut gpui::VisualTestContext) {
    cx.update(|window, cx| window.draw(cx).clear(cx));
    cx.run_until_parked();
    cx.executor().advance_clock(Duration::from_millis(300));
    cx.run_until_parked();
}

fn loaded(cx: &mut gpui::VisualTestContext) -> bool {
    cx.update(|_, cx| crate::chat_window::chat_messages("r1", cx).rows.iter().any(|row| row.id == id(TARGET)))
}

#[gpui::test]
fn a_card_older_than_the_first_page_pages_in_centres_and_rings(cx: &mut gpui::TestAppContext) {
    seed(cx);
    let (view, cx) = cx.add_window_view(|window, cx| SessionChatScreenView::new("r1".to_string(), window, cx));
    cx.simulate_resize(gpui::size(px(1440.), px(900.)));
    let steer = inner(&view, cx);
    steer.update(cx, |inner, cx| inner.apply_events_for_test(feed(), cx));
    for _ in 0..3 {
        frame(cx);
    }
    assert!(!loaded(cx), "the card sits outside the newest page");
    // A Needs you row / a `?card=` link.
    cx.update(|_, cx| crate::session_chat::request_chat_card("r1", &id(TARGET), cx));
    let mut frames = 0;
    while steer.read_with(cx, |steer, _| steer.deep_link_target().is_some()) {
        frames += 1;
        assert!(frames < 200, "the deep link settles");
        frame(cx);
        assert!(!steer.read_with(cx, |steer, _| steer.chat_for_test().following()), "a pending link never follows the newest row");
    }
    assert!(loaded(cx), "the chat paged the card in");
    let target = id(TARGET);
    assert_eq!(cx.update(|_, cx| crate::waiting_cards::highlighted_id(cx).map(str::to_string)), Some(target), "the card rings");
    let away = steer.read_with(cx, |steer, cx| steer.chat_for_test().distance_from_bottom(cx)).unwrap_or(0.);
    assert!(away > 900., "the list stands at the old card, far above the newest rows ({away} px)");
}

#[gpui::test]
fn a_card_that_is_nowhere_settles_at_the_newest_rows(cx: &mut gpui::TestAppContext) {
    seed(cx);
    let (view, cx) = cx.add_window_view(|window, cx| SessionChatScreenView::new("r1".to_string(), window, cx));
    cx.simulate_resize(gpui::size(px(1440.), px(900.)));
    let steer = inner(&view, cx);
    steer.update(cx, |inner, cx| inner.apply_events_for_test(feed(), cx));
    frame(cx);
    cx.update(|_, cx| crate::session_chat::request_chat_card("r1", "gone", cx));
    let mut frames = 0;
    while steer.read_with(cx, |steer, _| steer.deep_link_target().is_some()) {
        frames += 1;
        assert!(frames < 200, "the deep link settles");
        frame(cx);
    }
}
