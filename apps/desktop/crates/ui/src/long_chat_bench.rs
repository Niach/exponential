//! #LOGS-7: a team agent's very long chat (5,000 synced messages, long
//! Markdown, surface cards, tool rows), opened, scrolled, typed into, a
//! message arriving, the team's other sessions syncing. Release timings:
//!
//! ```sh
//! cargo test -p ui --release --lib long_chat_bench -- --ignored --nocapture
//! ```

use std::time::{Duration, Instant};

use gpui::{px, AppContext as _, Entity, Focusable as _};
use sync::{ShapeRow as _, Store};

use domain::rows::{MessageRow, SessionRow};

use super::SessionChatScreenView;

const MESSAGES: usize = 5_000;

fn body(n: usize) -> String {
    match n % 4 {
        0 => format!("Please look at **#EXP-{n}** again: the list under `crates/ui` still jumps when a row arrives. Message {n}."),
        1 => format!(
            "## Round {n}\n\nI checked the transcript and found three things:\n\n1. The rows re-measure on every frame.\n2. The composer repaints the list.\n3. The cards rebuild.\n\n```rust\nfn main() {{\n    println!(\"{n}\");\n}}\n```\n\n| Step | Time |\n| --- | --- |\n| open | 120 ms |\n| scroll | 30 ms |\n\nNext I'll fix the measure cache and push a PR. {}",
            "This paragraph is long enough to wrap a few times in the reading column so the rows differ in height. ".repeat(1 + n % 3)
        ),
        2 => format!("Done with step {n}. All green."),
        _ => format!("> quoted context {n}\n\n- a bullet\n- another bullet with `code`\n\nand a [link](https://exponential.at/{n})."),
    }
}

fn card() -> serde_json::Value {
    let tile = |id: &str, label: &str| serde_json::json!({"id": id, "component": "StatTile", "props": {"label": label, "value": {"@path": format!("/{id}")}}});
    serde_json::json!({
        "kind": "surface",
        "title": "Report",
        "surface": {
            "version": "1.0",
            "root": {"id": "root", "component": "Stack", "props": {"gap": "md"}, "children": [
                {"id": "tiles", "component": "Grid", "props": {"columns": 3, "gap": "sm"}, "children": [tile("new", "New"), tile("merged", "Merged"), tile("coding", "Coding")]}
            ]},
            "data": {"new": "2", "merged": "1", "coding": "1"}
        }
    })
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
                    "id": format!("m{n:05}"), "session_id": "r1",
                    "author_kind": if n % 2 == 0 { "user" } else { "agent" },
                    "body": body(n), "created_at": at(n),
                });
                if n % 25 == 24 {
                    row["card"] = card();
                    row["body"] = serde_json::json!("");
                }
                let message: MessageRow = serde_json::from_value(row).unwrap();
                collection.seed(message.key(), message);
            }
            cx.notify();
        });
    });
}

/// The feed the relay replays for the same chat: prompt, tool rows, reply.
fn feed_events() -> Vec<steer::ViewerEvent> {
    feed_events_of(MESSAGES)
}

fn feed_events_of(messages: usize) -> Vec<steer::ViewerEvent> {
    let mut events = vec![steer::ViewerEvent::Phase(steer::ViewerPhase::Live)];
    let mut seq = 0u64;
    let mut next = |event| {
        seq += 1;
        steer::ViewerEvent::Activity(seq, event)
    };
    for n in 0..messages {
        if n % 2 == 0 {
            events.push(next(steer::ActivityEvent::user_message(body(n))));
            events.push(next(steer::ActivityEvent::tool("Bash", Some(format!("cargo test -p crate{n}")))));
            events.push(next(steer::ActivityEvent::tool("Read", Some(format!("src/file{n}.rs")))));
        } else {
            events.push(next(steer::ActivityEvent::narration(body(n))));
        }
    }
    events
}

fn inner(view: &Entity<SessionChatScreenView>, cx: &mut gpui::VisualTestContext) -> Entity<crate::steer_viewer::SteerSessionView> {
    cx.update(|_, cx| view.read(cx).root_view.read(cx).inner().clone())
}

fn frame(cx: &mut gpui::VisualTestContext) -> Duration {
    let started = Instant::now();
    cx.update(|window, cx| window.draw(cx).clear(cx));
    cx.run_until_parked();
    started.elapsed()
}

struct Stats(Vec<Duration>);

impl std::fmt::Display for Stats {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut ms: Vec<f64> = self.0.iter().map(|d| d.as_secs_f64() * 1e3).collect();
        ms.sort_by(f64::total_cmp);
        let pick = |q: f64| ms[((ms.len() - 1) as f64 * q).round() as usize];
        write!(f, "n {:3} · p50 {:7.2} ms · p95 {:7.2} ms · max {:7.2} ms", ms.len(), pick(0.5), pick(0.95), pick(1.0))
    }
}

#[allow(deprecated)]
#[gpui::test]
#[ignore = "a release bench: run with --ignored --nocapture"]
fn long_chat_bench(cx: &mut gpui::TestAppContext) {
    seed(cx);
    // Warm: the app has drawn a chat before (fonts loaded, shaping caches).
    {
        let (warm, wcx) = cx.add_window_view(|window, cx| SessionChatScreenView::new("r1".to_string(), window, cx));
        wcx.simulate_resize(gpui::size(px(1440.), px(900.)));
        let steer = inner(&warm, wcx);
        steer.update(wcx, |inner, cx| inner.apply_events_for_test(feed_events_of(200), cx));
        for _ in 0..3 {
            frame(wcx);
        }
        wcx.update(|window, _| window.remove_window());
        wcx.run_until_parked();
    }
    let opened = Instant::now();
    let (view, cx) = cx.add_window_view(|window, cx| SessionChatScreenView::new("r1".to_string(), window, cx));
    cx.simulate_resize(gpui::size(px(1440.), px(900.)));
    let steer = inner(&view, cx);
    let created = opened.elapsed();
    // `LOGS7_PROFILE=replay`: a sampler attaches, then a long replay.
    let events = if std::env::var("LOGS7_PROFILE").as_deref() == Ok("replay") {
        eprintln!("[LOGS-7] profiling replay in pid {}", std::process::id());
        std::thread::sleep(Duration::from_millis(1500));
        feed_events_of(MESSAGES * std::env::var("LOGS7_REPLAY_X").ok().and_then(|x| x.parse().ok()).unwrap_or(8))
    } else {
        feed_events()
    };
    let count = events.len();
    let replay = Instant::now();
    steer.update(cx, |inner, cx| inner.apply_events_for_test(events, cx));
    let replay = replay.elapsed();
    println!("[LOGS-7] replay + its first frame: {count} events in {:.1} ms", replay.as_secs_f64() * 1e3);
    let first = frame(cx);
    let mut settle = Vec::new();
    for _ in 0..4 {
        settle.push(frame(cx));
    }
    let open = opened.elapsed();
    println!(
        "[LOGS-7] open: view {:.1} ms · 5,000-message replay {:.1} ms · first frame {:.1} ms · settled {:.1} ms",
        created.as_secs_f64() * 1e3,
        replay.as_secs_f64() * 1e3,
        first.as_secs_f64() * 1e3,
        open.as_secs_f64() * 1e3
    );
    println!("[LOGS-7] settle frames: {}", Stats(settle));

    // `LOGS7_PROFILE=<phase>`: loop that phase for a sampler (`sample <pid>`).
    if let Ok(phase) = std::env::var("LOGS7_PROFILE") {
        let until = Instant::now() + Duration::from_secs(8);
        eprintln!("[LOGS-7] profiling {phase} in pid {}", std::process::id());
        let mut k = 100usize;
        while Instant::now() < until {
            k += 1;
            match phase.as_str() {
                "type" => {
                    let input = steer.read_with(cx, |inner, _| inner.composer_for_test());
                    cx.update(|window, cx| {
                        let handle = input.read(cx).focus_handle(cx);
                        window.focus(&handle, cx);
                    });
                    cx.simulate_input("a");
                    frame(cx);
                }
                "scroll" => {
                    let document = steer.read_with(cx, |inner, _| inner.chat_document().cloned()).expect("the document");
                    let list = document.read_with(cx, |document, _| document.painted_bounds(crate::chat_tab_transcript::SCROLL)).expect("the list painted");
                    let dy = if (k / 40) % 2 == 0 { 120. } else { -120. };
                    let event = gpui::ScrollWheelEvent { position: list.center(), delta: gpui::ScrollDelta::Pixels(gpui::point(px(0.), px(dy))), ..Default::default() };
                    cx.update(|window, cx| {
                        window.dispatch_event(gpui::InputEvent::to_platform_input(event), cx);
                    });
                    frame(cx);
                }
                "open" => {
                    let (again, ocx) = cx.add_window_view(|window, cx| SessionChatScreenView::new("r1".to_string(), window, cx));
                    ocx.simulate_resize(gpui::size(px(1440.), px(900.)));
                    let fresh = inner(&again, ocx);
                    fresh.update(ocx, |inner, cx| inner.apply_events_for_test(feed_events(), cx));
                    frame(ocx);
                    ocx.update(|window, _| window.remove_window());
                    ocx.run_until_parked();
                }
                "other" => {
                    other_session_syncs(k, cx);
                    frame(cx);
                }
                "append" => {
                    message_arrives(&steer, k, cx);
                    frame(cx);
                }
                _ => {
                    frame(cx);
                }
            }
        }
    }

    // Idle: what asks the transcript to paint again while nothing happens.
    let notified = std::rc::Rc::new(std::cell::Cell::new(0u32));
    let count = notified.clone();
    let _watch = cx.update(|_, cx| cx.observe(&steer, move |_, _| count.set(count.get() + 1)));
    for _ in 0..50 {
        cx.executor().advance_clock(Duration::from_millis(100));
        cx.run_until_parked();
    }
    println!("[LOGS-7] idle: {} repaint requests in 5 s", notified.get());
    drop(_watch);

    // Idle: a frame with nothing changed.
    let idle: Vec<Duration> = (0..30).map(|_| frame(cx)).collect();
    println!("[LOGS-7] idle redraw: {}", Stats(idle));

    // Scroll: a wheel up, then its frame.
    let document = steer.read_with(cx, |inner, _| inner.chat_document().cloned()).expect("the document");
    let list = document.read_with(cx, |document, _| document.painted_bounds(crate::chat_tab_transcript::SCROLL)).expect("the list painted");
    let mut scroll = Vec::new();
    for n in 0..60 {
        let dy = if n < 40 { 120. } else { -120. };
        let event = gpui::ScrollWheelEvent { position: list.center(), delta: gpui::ScrollDelta::Pixels(gpui::point(px(0.), px(dy))), ..Default::default() };
        let started = Instant::now();
        cx.update(|window, cx| {
            window.dispatch_event(gpui::InputEvent::to_platform_input(event), cx);
        });
        cx.update(|window, cx| window.draw(cx).clear(cx));
        cx.run_until_parked();
        scroll.push(started.elapsed());
    }
    println!("[LOGS-7] scroll: {}", Stats(scroll));

    // Typing in the composer: one key, its frame.
    let input = steer.read_with(cx, |inner, _| inner.composer_for_test());
    cx.update(|window, cx| {
        let handle = input.read(cx).focus_handle(cx);
        window.focus(&handle, cx);
    });
    frame(cx);
    let mut typing = Vec::new();
    for _ in 0..40 {
        let started = Instant::now();
        cx.simulate_input("a");
        cx.update(|window, cx| window.draw(cx).clear(cx));
        cx.run_until_parked();
        typing.push(started.elapsed());
    }
    println!("[LOGS-7] typing: {}", Stats(typing));

    // A new message arriving: the synced row and the relay's reply.
    let mut append = Vec::new();
    for n in 0..20 {
        let started = Instant::now();
        message_arrives(&steer, n, cx);
        cx.update(|window, cx| window.draw(cx).clear(cx));
        cx.run_until_parked();
        append.push(started.elapsed());
    }
    println!("[LOGS-7] new message: {}", Stats(append));

    // The team's other sessions syncing (a busy team): an unrelated row moves.
    let mut other = Vec::new();
    for n in 0..20 {
        let started = Instant::now();
        other_session_syncs(n, cx);
        cx.update(|window, cx| window.draw(cx).clear(cx));
        cx.run_until_parked();
        other.push(started.elapsed());
    }
    println!("[LOGS-7] another session syncs: {}", Stats(other));
}

/// A new message arriving: the synced row and the relay's reply.
fn message_arrives(steer: &Entity<crate::steer_viewer::SteerSessionView>, n: usize, cx: &mut gpui::VisualTestContext) {
    cx.update(|_, cx| {
        let collections = Store::global(cx).collections().clone();
        let message: MessageRow = serde_json::from_value(serde_json::json!({
            "id": format!("n{n:08}"), "session_id": "r1", "author_kind": "agent", "body": body(MESSAGES + n), "created_at": "2026-11-05T09:00:00.000Z",
        }))
        .unwrap();
        collections.messages.update(cx, |collection, cx| {
            collection.seed(message.key(), message);
            cx.notify();
        });
    });
    steer.update(cx, |inner, cx| {
        inner.apply_events_for_test(vec![steer::ViewerEvent::Activity(1_000_000 + n as u64, steer::ActivityEvent::narration(body(MESSAGES + n)))], cx)
    });
}

/// The team's other sessions syncing (a busy team): an unrelated row moves.
fn other_session_syncs(n: usize, cx: &mut gpui::VisualTestContext) {
    cx.update(|_, cx| {
        let collections = Store::global(cx).collections().clone();
        let row: SessionRow = serde_json::from_value(serde_json::json!({
            "id": "s-other", "team_id": "t1", "root_id": "s-other", "status": "running", "shared": true,
            "created_at": "2026-11-04T09:00:00.000Z", "updated_at": format!("2026-11-04T09:00:{:02}.000Z", n % 60),
        }))
        .unwrap();
        collections.sessions.update(cx, |collection, cx| {
            collection.seed(row.key(), row);
            cx.notify();
        });
    });
}
