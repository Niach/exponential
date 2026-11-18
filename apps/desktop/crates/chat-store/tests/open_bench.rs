//! #EDE-105: a big chat opened from its journal (what a desktop does for its
//! OWN session) vs from the chat store (what it now does for every other
//! one, which before had nothing on disk and waited for the eNet join), the
//! one-time cost of taking it in, and searches over it.
//!
//! ```sh
//! CHAT_STORE_BENCH_JOURNAL=~/…/journal/<id>.jsonl \
//! cargo test -p chat-store --release --test open_bench -- --ignored --nocapture
//! ```

use std::time::Instant;

use chat_store::sqlite::SqliteStore;
use chat_store::StoredEvent;

fn ms(started: Instant) -> f64 {
    started.elapsed().as_secs_f64() * 1e3
}

/// The view's fold: the REPLAY TAIL into a feed (`steer_viewer::fold_tail`).
fn fold(events: &[(u64, steer::frames::ActivityEvent)]) -> usize {
    let (kept, _) = steer::replay_tail(events.iter().map(|(seq, event)| (*seq, event)).collect(), steer::REPLAY_TAIL);
    let mut feed = steer::SteerFeed::new();
    for (seq, event) in kept {
        feed.apply_seq(Some(seq), event.clone());
    }
    feed.items().len()
}

#[test]
#[ignore = "a release bench on a real journal: run with --ignored --nocapture"]
fn open_bench() {
    let Some(journal) = std::env::var_os("CHAT_STORE_BENCH_JOURNAL") else {
        eprintln!("[open] CHAT_STORE_BENCH_JOURNAL unset: skipped");
        return;
    };
    let dir = std::env::temp_dir().join(format!("chat-store-bench-{}", std::process::id()));
    let copy = steer::journal_path(&dir, "bench").unwrap();
    std::fs::create_dir_all(copy.parent().unwrap()).unwrap();
    std::fs::copy(&journal, &copy).unwrap();
    let bytes = std::fs::metadata(&copy).unwrap().len();

    let mut journal_runs = Vec::new();
    let mut events = Vec::new();
    for _ in 0..5 {
        let started = Instant::now();
        events = steer::read_journal_seq(&dir, "bench").unwrap();
        fold(&events);
        journal_runs.push(ms(started));
    }

    let store = SqliteStore::open(&dir.join("chat-store.sqlite")).unwrap();
    let stored: Vec<StoredEvent> = events
        .iter()
        .map(|(seq, event)| StoredEvent {
            root_id: "root".into(),
            session_id: "bench".into(),
            device_id: "device".into(),
            seq: *seq,
            stored_at: 0,
            event: serde_json::to_value(event).unwrap(),
        })
        .collect();
    let started = Instant::now();
    store.put_events(&stored).unwrap();
    let ingest = ms(started);
    let store_bytes: u64 = std::fs::read_dir(&dir)
        .unwrap()
        .flatten()
        .filter(|entry| entry.file_name().to_string_lossy().starts_with("chat-store"))
        .map(|entry| entry.metadata().unwrap().len())
        .sum();

    let mut store_runs = Vec::new();
    let mut items = 0;
    for _ in 0..5 {
        let started = Instant::now();
        let read: Vec<(u64, steer::frames::ActivityEvent)> = store
            .read_events("bench", "device", None, None)
            .unwrap()
            .into_iter()
            .map(|stored| (stored.seq, serde_json::from_value(stored.event).unwrap()))
            .collect();
        items = fold(&read);
        store_runs.push(ms(started));
    }

    let mut searches = Vec::new();
    for query in ["error", "cargo test", "fn ", "zzzz-not-there", "é"] {
        let started = Instant::now();
        let chat = store.search_chat("root", query, 50).unwrap();
        let chat_ms = ms(started);
        let started = Instant::now();
        let all = store.search_all(query, 50).unwrap();
        searches.push(format!("{query:?}: {} hits in chat {chat_ms:.1} ms, global {:.1} ms ({} chats)", chat.total, ms(started), all.total));
    }

    let median = |mut runs: Vec<f64>| {
        runs.sort_by(f64::total_cmp);
        runs[runs.len() / 2]
    };
    println!("[open] journal {:.1} MiB · {} events · {items} feed items", bytes as f64 / 1048576., events.len());
    println!("[open] from the journal (own session): median {:.1} ms", median(journal_runs));
    println!("[open] from the chat store (any session): median {:.1} ms", median(store_runs));
    println!("[open] taking it in once: {ingest:.0} ms · store on disk {:.1} MiB", store_bytes as f64 / 1048576.);
    for line in searches {
        println!("[search] {line}");
    }
    drop(store);
    let _ = std::fs::remove_dir_all(&dir);
}
