//! #LOGS-3/4/5: the IDE on a REAL account's store, headless. A copy of an
//! install's `sync-v2.sqlite` hydrates the collections (no network: the
//! Local-desktop path), the whole `Shell` opens on a screen, then each sync
//! delta kind replays rows that already exist (the foreground half of a
//! shape batch: point reads, observers) and draws the frame it asks for.
//!
//! ```sh
//! LOGS_REAL_DB=/tmp/copy/sync-v2.sqlite LOGS_REAL_DATA=/tmp/copy/data \
//! LOGS_REAL_ACCOUNT=<accounts.json id> LOGS_REAL_TEAM=<team id> LOGS_REAL_SCREEN=session:<id> \
//! EXP_SKIP_ONBOARDING=1 \
//! cargo test -p ui --release --lib real_data_bench -- --ignored --nocapture
//! ```
//!
//! `LOGS_REAL_DATA` holds an `accounts.json` with the account (no token: the
//! bench never reaches a server). `LOGS_REAL_PROFILE=<shape>` loops that
//! shape's delta for 8 s, for `sample <pid>`.

use std::time::{Duration, Instant};

use gpui::{px, size, AnyWindowHandle, AppContext as _, HeadlessAppContext, Window};

use crate::shell_bench::headless;
use crate::transcript_bench::percentile;

fn frame(window: AnyWindowHandle, cx: &mut HeadlessAppContext) -> f64 {
    let started = Instant::now();
    cx.update_window(window, |_, window, cx| window.draw(cx).clear(cx)).expect("a frame draws");
    started.elapsed().as_secs_f64() * 1000.
}

/// #PERF-9: until every heavy shape's off-main hydrate is installed.
pub(crate) fn wait_hydrated(cx: &mut HeadlessAppContext) {
    let deadline = Instant::now() + Duration::from_secs(60);
    while !cx.update(|cx| ["messages", "conversations", "sessions", "issues", "issue_events", "pull_request_checks", "events"].iter().all(|shape| sync::Store::global(cx).collections().is_hydrated(shape))) {
        assert!(Instant::now() < deadline, "the hydrate never landed");
        cx.run_until_parked();
        std::thread::sleep(Duration::from_millis(2));
    }
    cx.run_until_parked();
}

struct Stats(Vec<f64>);

impl std::fmt::Display for Stats {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut ms = self.0.clone();
        ms.sort_by(f64::total_cmp);
        write!(f, "n {:3} · p50 {:8.2} ms · p95 {:8.2} ms · max {:8.2} ms", ms.len(), percentile(&ms, 0.5), percentile(&ms, 0.95), percentile(&ms, 1.0))
    }
}

/// The newest `n` ids of `table` (by `order`), as a delta would carry them.
fn recent_ids(db: &std::path::Path, table: &str, order: &str, n: usize) -> Vec<String> {
    let store = sync::store::ShapeStore::open(db).expect("the copy");
    let spec = sync::shapes::shape_by_name(table).expect("a shape");
    let mut rows = store.read_all(spec).expect("rows");
    let text = |row: &serde_json::Map<String, serde_json::Value>, key: &str| row.get(key).and_then(|v| v.as_str()).unwrap_or_default().to_string();
    rows.sort_by_key(|row| std::cmp::Reverse(text(row, order)));
    rows.iter().take(n).map(|row| text(row, "id")).collect()
}

#[test]
#[ignore = "a release bench on a real store: run with --ignored --nocapture"]
fn real_data_bench() {
    let (Some(db), Some(data), Some(account)) = (std::env::var_os("LOGS_REAL_DB"), std::env::var_os("LOGS_REAL_DATA"), std::env::var("LOGS_REAL_ACCOUNT").ok()) else {
        eprintln!("[real] LOGS_REAL_DB / LOGS_REAL_DATA / LOGS_REAL_ACCOUNT unset: skipped");
        return;
    };
    let db = std::path::PathBuf::from(db);
    let data = std::path::PathBuf::from(data);
    // #PERF-2/3/4: every main-thread span's self time, per phase below.
    let (spans, _spans) = crate::span_clock::SpanClock::install();
    let mut cx = headless();
    let (changes, rx) = flume::unbounded::<(String, Vec<String>)>();
    let started = Instant::now();
    cx.update(|cx| {
        crate::init(cx);
        cx.set_global(crate::AuthContext { auth: api::AuthStore::load(data.clone()), client: std::sync::Arc::new(api::AuthClient::new()), data_dir: data.clone() });
        let store = sync::Store::open(cx, None, None);
        cx.set_global(store);
        sync::Store::global(cx).clone().connect_local(&account, &db, rx, cx).expect("the store hydrates");
    });
    // #PERF-9: the connect itself is the main thread's share; the heavy
    // shapes land from the background executor after it.
    println!("[real] connect held the main thread {:.0} ms", started.elapsed().as_secs_f64() * 1e3);

    let screen = std::env::var("LOGS_REAL_SCREEN").ok();
    let handle = cx
        .open_window(size(px(1600.), px(1000.)), |window: &mut Window, cx| {
            if let Ok(team) = std::env::var("LOGS_REAL_TEAM") {
                let nav = crate::navigation::nav_for_window(window, cx);
                nav.update(cx, |nav, _| nav.team_id = Some(team));
            }
            let shell = cx.new(|cx| crate::Shell::new(0, window, cx));
            cx.new(|cx| gpui_component::Root::new(shell, window, cx))
        })
        .expect("a headless window");
    let window: AnyWindowHandle = handle.into();
    cx.update_window(window, |_, window, _| window.activate_window()).unwrap();
    let first = frame(window, &mut cx);
    println!("[real] first frame {first:.1} ms ({:.0} ms after start)", started.elapsed().as_secs_f64() * 1e3);
    // #PERF-9: the boot as the app runs it: the window draws while the heavy
    // shapes land; every foreground slice (the installs and what they wake)
    // and every frame is timed until the store is whole.
    let mut slices = Vec::new();
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        let hydrated = cx.update(|cx| ["messages", "conversations", "sessions", "issues", "issue_events", "pull_request_checks", "events"].iter().all(|shape| sync::Store::global(cx).collections().is_hydrated(shape)));
        let slice = Instant::now();
        cx.run_until_parked();
        slices.push(slice.elapsed().as_secs_f64() * 1e3);
        slices.push(frame(window, &mut cx));
        if hydrated {
            break;
        }
        assert!(Instant::now() < deadline, "the hydrate never landed");
        std::thread::sleep(Duration::from_millis(2));
    }
    let sessions = cx.update(|cx| sync::Store::global(cx).collections().sessions.read(cx).len());
    let messages = cx.update(|cx| sync::Store::global(cx).collections().messages.read(cx).len());
    println!("[real] hydrated {sessions} sessions, {messages} messages in {:.0} ms; boot slices: {}", started.elapsed().as_secs_f64() * 1e3, Stats(slices));
    // #PERF-9: what each heavy shape's install wakes on the main thread
    // (observers, derived views) and the frame after it; the read itself is
    // the background executor's in the app.
    {
        let sqlite = sync::store::ShapeStore::open(&db).expect("the copy");
        const HEAVY: [&str; 7] = ["issues", "sessions", "conversations", "issue_events", "pull_request_checks", "events", "messages"];
        for shape in HEAVY {
            let started = Instant::now();
            let read = cx.update(|cx| sync::Store::global(cx).collections().clone().reinstall_for_bench(shape, &sqlite, cx));
            let woke = started.elapsed().as_secs_f64() * 1e3 - read;
            let drawn = frame(window, &mut cx);
            println!("[real] install {shape:<20} main thread {woke:6.1} ms + frame {drawn:5.1} ms (read {read:.0} ms off main)");
        }
        // Against: one install of them all (every derived view at once).
        for _ in 0..3 {
            let started = Instant::now();
            let read: f64 = cx.update(|cx| HEAVY.iter().map(|shape| sync::Store::global(cx).collections().clone().reinstall_for_bench(shape, &sqlite, cx)).sum());
            let woke = started.elapsed().as_secs_f64() * 1e3 - read;
            let drawn = frame(window, &mut cx);
            println!("[real] install all heavy shapes at once: main thread {woke:6.1} ms + frame {drawn:5.1} ms (reads {read:.0} ms off main)");
        }
        cx.run_until_parked();
    }
    if let Some(spec) = screen.as_deref() {
        let screen = crate::navigation::parse_dev_screen(spec).expect("a dev screen");
        let open = Instant::now();
        cx.update_window(window, |_, window, cx| crate::navigation::navigate_from_rail(window, cx, screen)).unwrap();
        let drawn = frame(window, &mut cx);
        cx.run_until_parked();
        println!("[real] open {spec}: {:.1} ms (frame {drawn:.1} ms)", open.elapsed().as_secs_f64() * 1e3);
    }
    for _ in 0..4 {
        frame(window, &mut cx);
        cx.run_until_parked();
    }
    // A journal-seeded chat folds off the main thread: let it land.
    for _ in 0..20 {
        std::thread::sleep(Duration::from_millis(100));
        cx.run_until_parked();
        frame(window, &mut cx);
    }
    println!("[real] spans to here: {}", spans.top(14));
    spans.reset();

    if let Some(shot) = std::env::var_os("LOGS_REAL_SHOT") {
        cx.capture_screenshot(window).expect("the frame").save(std::path::Path::new(&shot)).expect("the PNG");
    }
    // Idle, focused: one simulated second in 10 ms display-link ticks.
    let mut idle = Vec::new();
    for _ in 0..100 {
        cx.advance_clock(Duration::from_millis(10));
        cx.run_until_parked();
        idle.push(frame(window, &mut cx));
    }
    println!("[real] idle frames: {}", Stats(idle));
    println!("[real] idle spans: {}", spans.top(12));
    spans.reset();

    let shapes: &[(&str, &str, usize)] = &[
        ("pull_requests", "updated_at", 3),
        ("sessions", "updated_at", 3),
        ("messages", "created_at", 3),
        ("issues", "updated_at", 3),
        ("session_issues", "created_at", 3),
        ("pull_request_checks", "updated_at", 3),
        ("events", "created_at", 3),
    ];
    let replay = |shape: &str, ids: &[String], cx: &mut HeadlessAppContext| -> (f64, f64) {
        let started = Instant::now();
        changes.send((shape.to_string(), ids.to_vec())).unwrap();
        cx.run_until_parked();
        let apply = started.elapsed().as_secs_f64() * 1000.;
        let drawn = frame(window, cx);
        cx.run_until_parked();
        (apply, drawn)
    };
    if std::env::var("LOGS_REAL_CYCLE").as_deref() != Ok("0") {
        crate::duty_cycle::enable_for_test(true);
    }
    if let Ok(profile) = std::env::var("LOGS_REAL_PROFILE") {
        let (_, order, n) = shapes.iter().find(|(shape, _, _)| *shape == profile).copied().expect("a known shape");
        let ids = recent_ids(&db, &profile, order, n);
        eprintln!("[real] profiling {profile} in pid {}", std::process::id());
        std::thread::sleep(Duration::from_millis(1500));
        let until = Instant::now() + Duration::from_secs(8);
        while Instant::now() < until {
            replay(&profile, &ids, &mut cx);
        }
    }
    // As production runs it (#LOGS-3's duty cycle on; `LOGS_REAL_CYCLE=0`
    // = off): one delta at a time, then a burst of ten back to back (a busy
    // team), timing the main thread's whole share of each.
    for (shape, order, n) in shapes {
        let ids = recent_ids(&db, shape, order, *n);
        let (mut apply, mut drawn) = (Vec::new(), Vec::new());
        for _ in 0..10 {
            let (a, d) = replay(shape, &ids, &mut cx);
            apply.push(a);
            drawn.push(d);
        }
        println!("[real] {shape:20} delta apply: {}", Stats(apply));
        println!("[real] {shape:20} delta frame: {}", Stats(drawn));
        println!("[real] {shape:20} spans: {}", spans.top(12));
        spans.reset();
        cx.advance_clock(Duration::from_secs(3));
        cx.run_until_parked();
        let started = Instant::now();
        for _ in 0..10 {
            changes.send((shape.to_string(), ids.clone())).unwrap();
            cx.run_until_parked();
            frame(window, &mut cx);
        }
        cx.run_until_parked();
        println!("[real] {shape:20} burst of 10: {:.1} ms", started.elapsed().as_secs_f64() * 1e3);
        cx.advance_clock(Duration::from_secs(3));
        cx.run_until_parked();
    }
    std::mem::forget(cx);
}

/// #LOGS-3: a delta's rows are read and decoded off the main thread, then
/// committed; batches still land in the order they came (an upsert, then a
/// delete of the same row, sent back to back).
#[gpui::test]
fn deltas_read_off_the_main_thread_land_in_order(cx: &mut gpui::TestAppContext) {
    use sync::protocol::{RowKey, ShapeMessage};
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("local.sqlite");
    let store = sync::store::ShapeStore::open(&db).unwrap();
    let spec = sync::shapes::shape_by_name("pull_requests").unwrap();
    let key = RowKey::Single("pr1".into());
    let row = serde_json::json!({"id": "pr1", "team_id": "t1", "number": "7", "state": "open", "title": "Fix it", "created_at": "2026-11-11T09:00:00.000Z"});
    let (changes, rx) = flume::unbounded::<(String, Vec<String>)>();
    cx.update(|cx| {
        let store = sync::Store::open(cx, None, None);
        cx.set_global(store);
        sync::Store::global(cx).clone().connect_local("local-test", &db, rx, cx).unwrap();
    });
    let pulls = cx.update(|cx| sync::Store::global(cx).collections().pull_requests.clone());

    store.apply_batch(spec, &[ShapeMessage::Insert { key: key.clone(), value: row.as_object().unwrap().clone() }], None).unwrap();
    changes.send(("pull_requests".into(), vec!["pr1".into()])).unwrap();
    cx.run_until_parked();
    assert_eq!(cx.read(|cx| pulls.read(cx).get("pr1").map(|pull| pull.title.clone())), Some(Some("Fix it".into())));

    // Updated, then deleted, before the drain runs: the last write wins.
    let mut edited = row.as_object().unwrap().clone();
    edited.insert("title".into(), "Fixed".into());
    store.apply_batch(spec, &[ShapeMessage::Update { key: key.clone(), value: edited }], None).unwrap();
    changes.send(("pull_requests".into(), vec!["pr1".into()])).unwrap();
    store.apply_batch(spec, &[ShapeMessage::Delete { key }], None).unwrap();
    changes.send(("pull_requests".into(), vec!["pr1".into()])).unwrap();
    cx.run_until_parked();
    assert!(cx.read(|cx| pulls.read(cx).get("pr1").is_none()));
    drop(dir);
}

/// #PERF-9: a heavy shape hydrates off the main thread; a delta of it that
/// arrives meanwhile waits for the install and lands on top (never under
/// the snapshot), and the shape reads not-ready until then.
#[gpui::test]
fn a_heavy_shapes_delta_lands_after_its_off_main_hydrate(cx: &mut gpui::TestAppContext) {
    use sync::protocol::{RowKey, ShapeMessage};
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("local.sqlite");
    let store = sync::store::ShapeStore::open(&db).unwrap();
    let spec = sync::shapes::shape_by_name("messages").unwrap();
    let row = |id: &str, body: &str| serde_json::json!({"id": id, "session_id": "s1", "author_kind": "agent", "body": body, "created_at": "2026-11-12T09:00:00.000Z"}).as_object().unwrap().clone();
    store.apply_batch(spec, &[ShapeMessage::Insert { key: RowKey::Single("m1".into()), value: row("m1", "old") }], None).unwrap();
    let (changes, rx) = flume::unbounded::<(String, Vec<String>)>();
    cx.update(|cx| {
        let store = sync::Store::open(cx, None, None);
        cx.set_global(store);
        sync::Store::global(cx).clone().connect_local("local-test", &db, rx, cx).unwrap();
    });
    let messages = cx.update(|cx| sync::Store::global(cx).collections().messages.clone());
    // Off the main thread: nothing installed yet, and not ready.
    assert!(cx.read(|cx| !sync::Store::global(cx).collections().is_hydrated("messages")));
    assert!(cx.read(|cx| messages.read(cx).is_empty() && !messages.read(cx).is_ready()));
    // A write and its delta queue up before the hydrate lands.
    store.apply_batch(spec, &[ShapeMessage::Update { key: RowKey::Single("m1".into()), value: row("m1", "new") }, ShapeMessage::Insert { key: RowKey::Single("m2".into()), value: row("m2", "two") }], None).unwrap();
    changes.send(("messages".into(), vec!["m1".into(), "m2".into()])).unwrap();
    cx.run_until_parked();
    assert!(cx.read(|cx| sync::Store::global(cx).collections().is_hydrated("messages")));
    let body = |id: &str, cx: &mut gpui::TestAppContext| cx.read(|cx| messages.read(cx).get(id).and_then(|row| row.body.clone()));
    assert_eq!(body("m1", cx).as_deref(), Some("new"));
    assert_eq!(body("m2", cx).as_deref(), Some("two"));
    assert!(cx.read(|cx| messages.read(cx).is_ready()));
    assert_eq!(cx.read(|cx| messages.read(cx).in_group("s1").count()), 2);
    drop(dir);
}

/// #EXP-1380 (chats.json WINDOW): open the biggest chat of a REAL store,
/// data only: the main thread's share of the first chat model (the root
/// view, the conversation cards, the root's messages) and of the Results
/// tab, the rows each derived over, before and after the page lands.
///
/// ```sh
/// cp /tmp/a4af/sync-v2.sqlite /tmp/a4af/bench/ && \
/// LOGS_REAL_DB=/tmp/a4af/bench/sync-v2.sqlite LOGS_REAL_ROOT=<root id> \
/// cargo test -p ui --release --lib open_biggest_chat_bench -- --ignored --nocapture
/// ```
#[test]
#[ignore = "a release bench on a real store: run with --ignored --nocapture"]
fn open_biggest_chat_bench() {
    let (Some(db), Ok(root)) = (std::env::var_os("LOGS_REAL_DB"), std::env::var("LOGS_REAL_ROOT")) else {
        eprintln!("[open] LOGS_REAL_DB / LOGS_REAL_ROOT unset: skipped");
        return;
    };
    let db = std::path::PathBuf::from(db);
    let mut cx = headless();
    let (_changes, rx) = flume::unbounded::<(String, Vec<String>)>();
    let started = Instant::now();
    cx.update(|cx| {
        let store = sync::Store::open(cx, None, None);
        cx.set_global(store);
        sync::Store::global(cx).clone().connect_local("bench", &db, rx, cx).expect("the store hydrates");
    });
    println!("[open] connect held the main thread {:.0} ms", started.elapsed().as_secs_f64() * 1e3);
    wait_hydrated(&mut cx);
    let messages = cx.update(|cx| sync::Store::global(cx).collections().messages.read(cx).len());
    println!("[open] hydrated {messages} messages in {:.0} ms", started.elapsed().as_secs_f64() * 1e3);
    cx.run_until_parked();
    let ms = |started: Instant| started.elapsed().as_secs_f64() * 1e3;
    // The chat model: what the session screen and the transcript read.
    let model = |cx: &mut HeadlessAppContext| {
        cx.update(|cx| {
            let started = Instant::now();
            let now = crate::session_data::now_ms();
            let view = crate::session_data::root_chat_view(&root, now, cx);
            let own = crate::session_data::messages_of_session(&root, cx);
            let cards = crate::session_data::conversation_cards(&root, cx);
            (ms(started), view.map_or(0, |view| view.rows.len()), own.len(), cards.len())
        })
    };
    let mut main = 0.;
    let (first, rows, own, cards) = model(&mut cx);
    main += first;
    println!("[open] first model: {first:.1} ms main thread · {rows} view rows · {own} root messages · {cards} cards");
    let waited = Instant::now();
    cx.run_until_parked();
    let parked = ms(waited);
    let (second, rows, own, cards) = model(&mut cx);
    main += second;
    println!("[open] after the page ({parked:.1} ms parked): {second:.1} ms main thread · {rows} view rows · {own} root messages · {cards} cards");
    println!("[open] main thread to the paged model: {main:.1} ms");
    let (warm, ..) = model(&mut cx);
    println!("[open] warm re-read: {warm:.2} ms");
    let results = |cx: &mut HeadlessAppContext| {
        cx.update(|cx| {
            let started = Instant::now();
            let n = crate::results_tab::results_of_root(&root, cx).len();
            (ms(started), n)
        })
    };
    let (first, n) = results(&mut cx);
    cx.run_until_parked();
    let (second, m) = results(&mut cx);
    println!("[open] results: first {first:.1} ms ({n}), after the read {second:.1} ms ({m})");
    std::mem::forget(cx);
}

/// #PERF-4: ONE chat on a REAL store with a REAL journal's feed (the
/// replay tail folded into its root view, as the relay replays it): the
/// span self time of its idle frames, of the frames after each store delta
/// kind, and of a streaming reply's frames.
///
/// ```sh
/// LOGS_REAL_DB=… LOGS_REAL_DATA=… LOGS_REAL_ACCOUNT=… LOGS_REAL_ROOT=<root id> \
/// LOGS_REAL_JOURNAL=/tmp/copy/<root id>.jsonl \
/// cargo test -p ui --release --lib real_chat_bench -- --ignored --nocapture
/// ```
#[test]
#[ignore = "a release bench on a real store: run with --ignored --nocapture"]
fn real_chat_bench() {
    let (Some(db), Some(data), Ok(account), Ok(root), Some(journal)) = (
        std::env::var_os("LOGS_REAL_DB"),
        std::env::var_os("LOGS_REAL_DATA"),
        std::env::var("LOGS_REAL_ACCOUNT"),
        std::env::var("LOGS_REAL_ROOT"),
        std::env::var_os("LOGS_REAL_JOURNAL"),
    ) else {
        eprintln!("[chat] LOGS_REAL_DB / _DATA / _ACCOUNT / _ROOT / _JOURNAL unset: skipped");
        return;
    };
    let db = std::path::PathBuf::from(db);
    let data = std::path::PathBuf::from(data);
    let (spans, _spans) = crate::span_clock::SpanClock::install();
    let mut cx = headless();
    let (changes, rx) = flume::unbounded::<(String, Vec<String>)>();
    cx.update(|cx| {
        crate::init(cx);
        cx.set_global(crate::AuthContext { auth: api::AuthStore::load(data.clone()), client: std::sync::Arc::new(api::AuthClient::new()), data_dir: data.clone() });
        let store = sync::Store::open(cx, None, None);
        cx.set_global(store);
        sync::Store::global(cx).clone().connect_local(&account, &db, rx, cx).expect("the store hydrates");
    });
    wait_hydrated(&mut cx);
    let mut chat = None;
    let handle = cx
        .open_window(size(px(1440.), px(900.)), |window: &mut Window, cx| {
            let view = cx.new(|cx| crate::session_chat::SessionChatScreenView::new(root.clone(), window, cx));
            chat = Some(view.clone());
            cx.new(|cx| gpui_component::Root::new(view, window, cx))
        })
        .expect("a headless window");
    let window: AnyWindowHandle = handle.into();
    let chat = chat.expect("the chat view");
    cx.update_window(window, |_, window, _| window.activate_window()).unwrap();
    for _ in 0..3 {
        frame(window, &mut cx);
        cx.run_until_parked();
    }
    let steer = cx.update(|cx| chat.read(cx).inner_for_bench(cx));
    let path = std::path::PathBuf::from(journal);
    let dir = tempfile::tempdir().unwrap();
    let copy = steer::journal_path(dir.path(), "bench").expect("a path");
    std::fs::create_dir_all(copy.parent().unwrap()).unwrap();
    std::fs::copy(&path, &copy).unwrap();
    let events = steer::read_journal_seq(dir.path(), "bench").expect("the journal");
    let tail = steer::replay_tail(events, steer::REPLAY_TAIL).0;
    let viewer: Vec<steer::ViewerEvent> = std::iter::once(steer::ViewerEvent::Phase(steer::ViewerPhase::Live))
        .chain(tail.into_iter().map(|(seq, event)| steer::ViewerEvent::Activity(seq, event)))
        .collect();
    let started = Instant::now();
    cx.update(|cx| steer.update(cx, |steer, cx| steer.apply_events_for_test(viewer, cx)));
    let fold = started.elapsed().as_secs_f64() * 1e3;
    let first = frame(window, &mut cx);
    cx.run_until_parked();
    let (items, rows) = cx.update(|cx| steer.read(cx).bench_counts());
    println!("[chat] fold {fold:.0} ms · first frame {first:.1} ms · {items} items · {rows} rows · spans {}", spans.top(12));
    spans.reset();
    let mut idle = Vec::new();
    for _ in 0..60 {
        cx.advance_clock(Duration::from_millis(16));
        cx.run_until_parked();
        idle.push(frame(window, &mut cx));
    }
    println!("[chat] idle frames: {} · spans {}", Stats(idle), spans.top(14));
    spans.reset();
    crate::duty_cycle::enable_for_test(true);
    for (shape, order) in [("sessions", "updated_at"), ("messages", "created_at"), ("issues", "updated_at")] {
        let ids = recent_ids(&db, shape, order, 3);
        let mut frames = Vec::new();
        for _ in 0..10 {
            let started = Instant::now();
            changes.send((shape.to_string(), ids.clone())).unwrap();
            cx.run_until_parked();
            frame(window, &mut cx);
            frames.push(started.elapsed().as_secs_f64() * 1e3);
            cx.advance_clock(Duration::from_millis(400));
            cx.run_until_parked();
        }
        println!("[chat] {shape:10} delta + frame: {} · spans {}", Stats(frames), spans.top(14));
        spans.reset();
    }
    let mut arrivals = Vec::new();
    let mut reply = String::from("Streaming the reply:");
    for n in 0..30 {
        reply.push_str(&format!(" chunk {n} of the reply with **marks** and #EXP-{n}"));
        let event = steer::ViewerEvent::Activity(10_000_000 + n, steer::ActivityEvent::narration(reply.clone()));
        let started = Instant::now();
        cx.update(|cx| steer.update(cx, |steer, cx| steer.apply_events_for_test(vec![event], cx)));
        frame(window, &mut cx);
        cx.run_until_parked();
        arrivals.push(started.elapsed().as_secs_f64() * 1e3);
    }
    println!("[chat] streaming chunk + frame: {} · spans {}", Stats(arrivals), spans.top(14));
    std::mem::forget(cx);
    std::mem::forget(dir);
}
