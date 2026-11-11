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
    let sessions = cx.update(|cx| sync::Store::global(cx).collections().sessions.read(cx).len());
    let messages = cx.update(|cx| sync::Store::global(cx).collections().messages.read(cx).len());
    println!("[real] hydrated {sessions} sessions, {messages} messages in {:.0} ms", started.elapsed().as_secs_f64() * 1e3);

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
    cx.run_until_parked();
    let first = frame(window, &mut cx);
    cx.run_until_parked();
    println!("[real] first frame {first:.1} ms");
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
