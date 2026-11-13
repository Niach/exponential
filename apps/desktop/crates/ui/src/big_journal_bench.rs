//! A very large chat opened off this device's activity JOURNAL (Danny's:
//! 17,562 events / 16.7 MB, 14 MB of it `tool_update` output): what a
//! chat's open costs and what each scroll frame does. A synthetic journal in
//! the same shape (prompts, narration, tool calls with long Markdown output,
//! turn and usage slots) is written to a temp data dir, then the chat opens
//! the two ways the desktop reads a journal: an ENDED row (the `Journal`
//! source) and an idle row this machine ran (the relay view seeded from the
//! journal, #EXP-1389). Release timings:
//!
//! ```sh
//! cargo test -p ui --release --lib big_journal_bench -- --ignored --nocapture
//! # a real journal (a COPY), instead of the synthetic one:
//! BIG_JOURNAL=/tmp/copy.jsonl cargo test -p ui --release --lib big_journal_bench -- --ignored --nocapture
//! ```
//!
//! The guard (`a_large_journal_opens_on_a_window`) runs in the plain suite.

use std::time::{Duration, Instant};

use gpui::{px, Entity};
use sync::{ShapeRow as _, Store};

use domain::rows::SessionRow;

use super::SessionChatScreenView;
use crate::chat_tab_transcript::counters;
use crate::steer_viewer::SteerSessionView;

const SESSION: &str = "r1";

/// A tool's output: Markdown-ish, `kb` KB (the journal's p50 is ~1 KB,
/// its p90 ~7 KB).
fn output(n: usize, kb: usize) -> String {
    let mut out = format!("## Result {n}\n\n| col | value |\n| --- | --- |\n| a | {n} |\n\n```rust\n");
    while out.len() < kb * 1024 {
        out.push_str(&format!("fn line_{n}_{}() -> usize {{ {n} }} // a long enough line of tool output\n", out.len()));
    }
    out.push_str("```\n");
    out
}

/// A synthetic journal of `events` lines, in the real one's proportions.
pub(crate) fn synthetic_journal(events: usize) -> String {
    let mut lines: Vec<String> = Vec::with_capacity(events);
    let mut at = 1_794_028_546_009i64;
    let mut n = 0usize;
    let push = |lines: &mut Vec<String>, value: serde_json::Value| lines.push(value.to_string());
    push(&mut lines, serde_json::json!({"kind": "config_state", "options": [{"id": "model", "label": "Model", "category": "model", "value": "opus"}], "currentMode": "bypassPermissions", "modes": [], "at": at}));
    while lines.len() < events {
        n += 1;
        at += 1000;
        push(&mut lines, serde_json::json!({"kind": "turn", "state": "started", "at": at}));
        push(&mut lines, serde_json::json!({"kind": "user_message", "text": format!("## Request {n}\n\nPlease look at **#EXP-{n}** again: the list under `crates/ui` still jumps when a row arrives.\n\n- step one\n- step two"), "at": at}));
        for t in 0..6 {
            let id = format!("toolu_{n:06}_{t}");
            let (name, detail) = match t % 3 {
                0 => ("Bash", format!("cargo test -p crate{n}")),
                1 => ("Read", format!("src/file{n}_{t}.rs")),
                _ => ("mcp__exponential__exponential_issues_get", format!("EXP-{n}")),
            };
            push(&mut lines, serde_json::json!({"kind": "tool", "name": name, "detail": detail, "id": id, "toolKind": "other", "at": at}));
            push(&mut lines, serde_json::json!({"kind": "tool_update", "id": id, "status": "completed", "output": output(n, 1 + (n + t) % 8), "at": at + 10}));
        }
        push(&mut lines, serde_json::json!({"kind": "narration", "text": format!("Done with step {n}: the rows **measure once** now, and `cargo test` is green. {}", "This paragraph wraps a few lines in the reading column. ".repeat(1 + n % 3)), "messageId": format!("msg_{n}"), "at": at + 20}));
        push(&mut lines, serde_json::json!({"kind": "turn", "state": "ended", "at": at + 30}));
        push(&mut lines, serde_json::json!({"kind": "usage", "contextUsed": 40_000 + n, "contextSize": 200_000, "at": at + 30}));
    }
    lines.truncate(events);
    lines.join("\n") + "\n"
}

/// The store, a signed-in data dir holding `journal` for [`SESSION`], and
/// the session row (`status`, hosted by this device).
fn seed(cx: &mut gpui::TestAppContext, data: &std::path::Path, journal: &str, status: &str) {
    let path = steer::journal_path(data, SESSION).expect("a journal path");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, journal).unwrap();
    let data = data.to_path_buf();
    let status = status.to_string();
    cx.update(|cx| {
        gpui_component::init(cx);
        theme::init(cx);
        let store = Store::open(cx, None, None);
        cx.set_global(store);
        cx.set_global(crate::AuthContext { auth: api::AuthStore::load(data.clone()), client: std::sync::Arc::new(api::AuthClient::new()), data_dir: data });
        crate::queries::set_own_device_id_for_test("d1", cx);
        let collections = Store::global(cx).collections().clone();
        let root: SessionRow = serde_json::from_value(serde_json::json!({
            "id": SESSION, "team_id": "t1", "root_id": SESSION, "status": status, "shared": true, "device_id": "d1",
            "created_at": "2026-11-04T09:00:00.000Z", "updated_at": "2026-11-04T09:00:00.000Z",
        }))
        .unwrap();
        collections.sessions.update(cx, |collection, cx| {
            collection.seed(root.key(), root);
            cx.notify();
        });
    });
}

fn inner(view: &Entity<SessionChatScreenView>, cx: &mut gpui::VisualTestContext) -> Entity<SteerSessionView> {
    cx.update(|_, cx| view.read(cx).root_view.read(cx).inner().clone())
}

fn draw(cx: &mut gpui::VisualTestContext) -> Duration {
    let started = Instant::now();
    cx.update(|window, cx| window.draw(cx).clear(cx));
    started.elapsed()
}

/// One open: what it read, held, built and painted, and what it cost.
#[derive(Debug, Default)]
pub(crate) struct Open {
    pub journal_lines: usize,
    pub construct_ms: f64,
    pub open_ms: f64,
    pub first_frame_ms: f64,
    pub feed_items: usize,
    pub rows: usize,
    pub first_paints: usize,
    pub first_measures: usize,
    pub first_bodies: usize,
    pub scroll_p50_ms: f64,
    pub scroll_p95_ms: f64,
    pub scroll_max_ms: f64,
    pub paints_per_frame: f64,
    pub measures_per_frame: f64,
    pub bodies_per_frame: f64,
    pub max_paints_in_a_frame: usize,
    pub arrival_p50_ms: f64,
    pub arrival_p95_ms: f64,
    pub bodies_per_arrival: f64,
    pub items_after_top: usize,
    pub rows_after_top: usize,
    pub top_scroll_p50_ms: f64,
    pub top_scroll_p95_ms: f64,
}

fn quantiles(mut frames: Vec<f64>) -> (f64, f64, f64) {
    frames.sort_by(f64::total_cmp);
    let pick = |q: f64| frames.get(((frames.len().max(1) - 1) as f64 * q).round() as usize).copied().unwrap_or_default();
    (pick(0.5), pick(0.95), pick(1.0))
}

fn ms(d: Duration) -> f64 {
    d.as_secs_f64() * 1e3
}

/// Opens [`SESSION`]'s chat (already seeded) and measures the open and a
/// scroll of `steps` wheel ticks up, then a climb to the very top.
/// `climb`: wheel ticks toward the top; a guard stops at the first page.
pub(crate) fn open_and_scroll(cx: &mut gpui::TestAppContext, journal_lines: usize, steps: usize, climb: usize, first_page_only: bool) -> Open {
    // Warm: the app has drawn this chat before (fonts, shaping, the chat
    // document's factory); the open measured is the next one.
    {
        let (_warm, wcx) = cx.add_window_view(|window, cx| SessionChatScreenView::new(SESSION.to_string(), window, cx));
        wcx.simulate_resize(gpui::size(px(1440.), px(900.)));
        wcx.run_until_parked();
        for _ in 0..3 {
            draw(wcx);
            wcx.run_until_parked();
        }
        wcx.update(|window, _| window.remove_window());
        wcx.run_until_parked();
    }
    counters::reset();
    let opened = Instant::now();
    let (view, cx) = cx.add_window_view(|window, cx| SessionChatScreenView::new(SESSION.to_string(), window, cx));
    let construct = opened.elapsed();
    cx.simulate_resize(gpui::size(px(1440.), px(900.)));
    let steer = inner(&view, cx);
    // The journal read and its fold land (off the main thread where they
    // run there), then the frame that paints the newest rows.
    cx.run_until_parked();
    let first = draw(cx);
    cx.run_until_parked();
    let open_ms = ms(opened.elapsed());
    let (first_paints, first_measures, first_bodies) = counters::take();
    for _ in 0..4 {
        draw(cx);
        cx.run_until_parked();
    }
    let (feed_items, rows) = steer.read_with(cx, |steer, _| steer.bench_counts());
    counters::reset();
    let document = steer.read_with(cx, |steer, _| steer.chat_document().cloned()).expect("the document");
    let list = document.read_with(cx, |document, _| document.painted_bounds(crate::chat_tab_transcript::SCROLL)).expect("the list painted");
    let wheel = |dy: f32, cx: &mut gpui::VisualTestContext| {
        let event = gpui::ScrollWheelEvent { position: list.center(), delta: gpui::ScrollDelta::Pixels(gpui::point(px(0.), px(dy))), ..Default::default() };
        cx.update(|window, cx| {
            window.dispatch_event(gpui::InputEvent::to_platform_input(event), cx);
        });
    };
    let mut frames = Vec::new();
    let mut max_paints = 0;
    let mut sums = (0usize, 0usize, 0usize);
    for _ in 0..steps {
        let started = Instant::now();
        wheel(120., cx);
        cx.update(|window, cx| window.draw(cx).clear(cx));
        cx.run_until_parked();
        frames.push(ms(started.elapsed()));
        let (paints, measures, bodies) = counters::take();
        max_paints = max_paints.max(paints);
        sums = (sums.0 + paints, sums.1 + measures, sums.2 + bodies);
    }
    let (scroll_p50_ms, scroll_p95_ms, scroll_max_ms) = quantiles(frames);
    // A reply streaming in at the tail: one chunk, its frame.
    let mut arrivals = Vec::new();
    counters::reset();
    let mut reply = String::from("Streaming the reply:");
    for n in 0..20 {
        reply.push_str(&format!(" chunk {n} of the reply with **marks**"));
        let event = steer::ViewerEvent::Activity(10_000_000, steer::ActivityEvent::narration(reply.clone()));
        let started = Instant::now();
        steer.update(cx, |steer, cx| steer.apply_events_for_test(vec![event], cx));
        cx.update(|window, cx| window.draw(cx).clear(cx));
        cx.run_until_parked();
        arrivals.push(ms(started.elapsed()));
    }
    let (_, _, arrival_bodies) = counters::take();
    let (arrival_p50_ms, arrival_p95_ms, _) = quantiles(arrivals);
    // The climb: big wheel ticks until the top stops moving (every page in).
    for _ in 0..climb {
        wheel(4000., cx);
        cx.update(|window, cx| window.draw(cx).clear(cx));
        cx.run_until_parked();
        if first_page_only && steer.read_with(cx, |steer, _| steer.bench_counts()).0 > feed_items {
            break;
        }
    }
    let (items_after_top, rows_after_top) = steer.read_with(cx, |steer, _| steer.bench_counts());
    // Reading at the top: wheel ticks down through what the climb loaded.
    let mut top = Vec::new();
    for _ in 0..30 {
        let started = Instant::now();
        wheel(-120., cx);
        cx.update(|window, cx| window.draw(cx).clear(cx));
        cx.run_until_parked();
        top.push(ms(started.elapsed()));
    }
    let (top_scroll_p50_ms, top_scroll_p95_ms, _) = quantiles(top);
    // `BIG_JOURNAL_PROFILE=1`: keep scrolling at the top for a sampler.
    if std::env::var_os("BIG_JOURNAL_PROFILE").is_some() {
        eprintln!("[big-journal] profiling the top scroll in pid {}", std::process::id());
        let until = Instant::now() + Duration::from_secs(10);
        let mut k = 0usize;
        while Instant::now() < until {
            k += 1;
            wheel(if (k / 20) % 2 == 0 { -120. } else { 120. }, cx);
            cx.update(|window, cx| window.draw(cx).clear(cx));
            cx.run_until_parked();
        }
    }
    let steps = steps.max(1) as f64;
    Open {
        journal_lines,
        construct_ms: ms(construct),
        open_ms,
        first_frame_ms: ms(first),
        feed_items,
        rows,
        first_paints,
        first_measures,
        first_bodies,
        scroll_p50_ms,
        scroll_p95_ms,
        scroll_max_ms,
        paints_per_frame: sums.0 as f64 / steps,
        measures_per_frame: sums.1 as f64 / steps,
        bodies_per_frame: sums.2 as f64 / steps,
        max_paints_in_a_frame: max_paints,
        arrival_p50_ms,
        arrival_p95_ms,
        bodies_per_arrival: arrival_bodies as f64 / 20.,
        items_after_top,
        rows_after_top,
        top_scroll_p50_ms,
        top_scroll_p95_ms,
    }
}

fn report(label: &str, open: &Open) {
    println!("[big-journal] {label}: {open:#?}");
}

fn journal() -> (String, usize) {
    let text = match std::env::var_os("BIG_JOURNAL") {
        Some(path) => std::fs::read_to_string(path).expect("the journal copy"),
        None => synthetic_journal(20_000),
    };
    let lines = text.lines().filter(|line| !line.trim().is_empty()).count();
    (text, lines)
}

#[gpui::test]
#[ignore = "a release bench: run with --ignored --nocapture"]
fn big_journal_bench_ended(cx: &mut gpui::TestAppContext) {
    let (text, lines) = journal();
    let dir = tempfile::tempdir().unwrap();
    seed(cx, dir.path(), &text, "ended");
    // The read alone (what the open did ON the main thread before; off it
    // now), and the read + REPLAY TAIL cut + fold the view runs off it.
    let started = Instant::now();
    let events = steer::read_journal_seq(dir.path(), SESSION).map_or(0, |events| events.len());
    let read = ms(started.elapsed());
    let started = Instant::now();
    let tail = crate::steer_viewer::read_journal_tail(dir.path(), SESSION).map_or(0, |tail| tail.feed.items().len());
    println!("[big-journal] journal read: {events} events in {read:.1} ms · read + tail + fold off the main thread: {tail} items in {:.1} ms", ms(started.elapsed()));
    report("ended row (Journal source)", &open_and_scroll(cx, lines, 60, 400, false));
    std::mem::forget(dir);
}

#[gpui::test]
#[ignore = "a release bench: run with --ignored --nocapture"]
fn big_journal_bench_seeded(cx: &mut gpui::TestAppContext) {
    let (text, lines) = journal();
    let dir = tempfile::tempdir().unwrap();
    seed(cx, dir.path(), &text, "idle");
    report("idle row on this device (journal seed)", &open_and_scroll(cx, lines, 60, 400, false));
    std::mem::forget(dir);
}

/// The relay's replay as the viewer folds it: every event (before the
/// REPLAY TAIL) and the tail.
#[gpui::test]
#[ignore = "a release bench: run with --ignored --nocapture"]
fn big_journal_bench_relay(cx: &mut gpui::TestAppContext) {
    let (text, _) = journal();
    let dir = tempfile::tempdir().unwrap();
    let path = steer::journal_path(dir.path(), "x").unwrap();
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, &text).unwrap();
    let events = steer::read_journal_seq(dir.path(), "x").expect("the journal");
    seed(cx, dir.path(), "", "running");
    cx.update(|cx| crate::queries::set_own_device_id_for_test("d2", cx));
    for (label, events) in [("relay replay, whole journal", events.clone()), ("relay replay, REPLAY_TAIL", steer::replay_tail(events, steer::REPLAY_TAIL).0)] {
        let count = events.len();
        let (view, cx) = cx.add_window_view(|window, cx| SessionChatScreenView::new(SESSION.to_string(), window, cx));
        cx.simulate_resize(gpui::size(px(1440.), px(900.)));
        let steer = inner(&view, cx);
        let viewer: Vec<steer::ViewerEvent> = std::iter::once(steer::ViewerEvent::Phase(steer::ViewerPhase::Live))
            .chain(events.into_iter().map(|(seq, event)| steer::ViewerEvent::Activity(seq, event)))
            .collect();
        counters::reset();
        let started = Instant::now();
        steer.update(cx, |steer, cx| steer.apply_events_for_test(viewer, cx));
        let fold = started.elapsed();
        let first = draw(cx);
        cx.run_until_parked();
        let (paints, measures, bodies) = counters::take();
        let (items, rows) = steer.read_with(cx, |steer, _| steer.bench_counts());
        println!(
            "[big-journal] {label}: {count} events · fold {:.1} ms · first frame {:.1} ms · {items} items · {rows} rows · painted {paints} · measured {measures} · bodies prepared {bodies}",
            ms(fold),
            ms(first)
        );
        cx.update(|window, _| window.remove_window());
        cx.run_until_parked();
    }
    // `BIG_JOURNAL_PROFILE=1`: fold the tail into fresh views for a sampler.
    if std::env::var_os("BIG_JOURNAL_PROFILE").is_some() {
        let tail = steer::replay_tail(steer::read_journal_seq(dir.path(), "x").expect("the journal"), steer::REPLAY_TAIL).0;
        eprintln!("[big-journal] profiling the tail fold in pid {}", std::process::id());
        let until = Instant::now() + Duration::from_secs(10);
        while Instant::now() < until {
            let (view, cx) = cx.add_window_view(|window, cx| SessionChatScreenView::new(SESSION.to_string(), window, cx));
            let steer = inner(&view, cx);
            let viewer: Vec<steer::ViewerEvent> = tail.iter().cloned().map(|(seq, event)| steer::ViewerEvent::Activity(seq, event)).collect();
            steer.update(cx, |steer, cx| steer.apply_events_for_test(viewer, cx));
            cx.update(|window, _| window.remove_window());
            cx.run_until_parked();
        }
    }
    std::mem::forget(dir);
}

/// The guard: a 20,000-event journal opens on a WINDOW of rows (never the
/// run), a scroll frame paints a bounded handful, and the climb to the top
/// pages the older transcript in (the Load earlier path).
#[gpui::test]
fn a_large_journal_opens_on_a_window(cx: &mut gpui::TestAppContext) {
    let text = synthetic_journal(20_000);
    let dir = tempfile::tempdir().unwrap();
    seed(cx, dir.path(), &text, "ended");
    let open = open_and_scroll(cx, 20_000, 12, 200, true);
    // The newest REPLAY_TAIL log events at most, never the whole run.
    assert!(open.feed_items <= steer::REPLAY_TAIL, "{open:#?}");
    assert!(open.rows <= crate::steer_viewer::FEED_WINDOW_FOR_TEST, "{open:#?}");
    // The viewport's rows (and the list's overdraw), never the window's.
    assert!(open.first_paints < open.rows && open.max_paints_in_a_frame <= 60, "{open:#?}");
    // A debug build on a CI runner, generously (release: ~80 ms).
    assert!(open.open_ms < 5_000., "{open:#?}");
    // Every page of the run comes in on the way to the top.
    assert!(open.items_after_top > open.feed_items, "{open:#?}");
    drop(dir);
}

/// #PERF-4: a chat whose state did not move draws its frames (an animating
/// mark draws at display rate) without rebuilding the composer's document
/// (its "+" rows carry every open issue) or the transcript's.
#[gpui::test]
fn an_unchanged_chat_frame_rebuilds_no_document(cx: &mut gpui::TestAppContext) {
    use crate::steer_viewer::chat_tab_composer::COMPOSER_VIEWS;
    let dir = tempfile::tempdir().unwrap();
    seed(cx, dir.path(), &synthetic_journal(400), "idle");
    // Zeroed before the window: its first draw may already mount it all.
    COMPOSER_VIEWS.with(|n| n.set(0));
    let (view, cx) = cx.add_window_view(|window, cx| SessionChatScreenView::new(SESSION.to_string(), window, cx));
    cx.simulate_resize(gpui::size(px(1200.), px(800.)));
    // The chat settles (its journal read lands off the main thread, the
    // composer mounts) before the frames measured.
    for _ in 0..100 {
        cx.run_until_parked();
        draw(cx);
        if COMPOSER_VIEWS.with(|n| n.get()) > 0 {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    for _ in 0..4 {
        cx.run_until_parked();
        draw(cx);
    }
    let state = view.read_with(cx, |view, cx| {
        let steer = view.inner_for_bench(cx);
        let steer = steer.read(cx);
        format!("model {} · may_message {} · feed {:?} · painted {:?}", view.model.is_some(), steer.may_message(), steer.bench_counts(), counters::take())
    });
    assert!(COMPOSER_VIEWS.with(|n| n.get()) > 0, "the composer drew: {state}");
    COMPOSER_VIEWS.with(|n| n.set(0));
    counters::reset();
    for _ in 0..5 {
        draw(cx);
    }
    let (painted, _, bodies) = counters::take();
    assert!(painted > 0, "the frames painted the transcript");
    assert_eq!(COMPOSER_VIEWS.with(|n| n.get()), 0, "the composer's document rebuilt on an unchanged frame");
    assert_eq!(bodies, 0, "the transcript's document re-prepared on an unchanged frame");
    drop(dir);
}
