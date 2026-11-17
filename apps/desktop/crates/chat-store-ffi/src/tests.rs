use super::*;

const FIXTURE: &str = include_str!("../../../../../packages/domain-contract/fixtures/chat-store.json");

fn spec() -> Value {
    serde_json::from_str(FIXTURE).unwrap()
}

fn from<T: serde::de::DeserializeOwned>(value: &Value) -> T {
    serde_json::from_value(value.clone()).unwrap()
}

fn feed(seqs: &[u64], at: i64) -> String {
    let rows: Vec<Value> = seqs.iter().map(|seq| json!({ "seq": seq, "event": { "kind": "narration", "text": format!("n{seq}"), "at": at } })).collect();
    Value::Array(rows).to_string()
}

fn session(id: &str, device: &str) -> CatchUpSession {
    CatchUpSession { session_id: id.into(), root_id: "r".into(), device_id: device.into(), updated_at: Some("2026-11-17T10:00:00Z".into()), busy: false }
}

fn install(endpoint: &str, user: &str, device: &str) -> CatchUpInstall {
    CatchUpInstall { endpoint_id: endpoint.into(), user_id: user.into(), device_id: Some(device.into()), last_seen_at: Some("2026-11-17T10:00:00Z".into()) }
}

/// Runs one walk: `holders` answer off their own stores (an endpoint not in
/// it does not answer); returns the asks each answered.
fn walk(catch_up: &CatchUp, holders: &HashMap<String, (Arc<ChatStore>, Option<u64>)>, sid: &str, now: i64, order: &[String]) -> HashMap<String, u64> {
    let mut asks: HashMap<String, u64> = HashMap::new();
    let walked = catch_up.next_walk(now).expect("a walk");
    assert_eq!(walked, sid);
    if !order.is_empty() {
        // The fixture walks its holders in the order it lists them.
        let mut state = lock(&catch_up.state);
        let walk = state.walks.get_mut(sid).unwrap();
        walk.holders.sort_by_key(|holder| order.iter().position(|name| *name == holder.endpoint_id));
    }
    while let Some(ask) = catch_up.next_ask(walked.clone(), now) {
        let chunk = holders.get(&ask.peer).map(|(store, full_to)| {
            if let Some(full) = full_to {
                store.set_meta(format!("catchup.complete.{sid}.dev-run"), full.to_string()).unwrap();
            }
            *asks.entry(ask.peer.clone()).or_default() += 1;
            store.answer_page(sid.into(), ask.frame.clone(), PageAccess::Allowed).unwrap().unwrap()
        });
        catch_up.take_answer(ask.request_id, chunk, now).unwrap();
    }
    asks
}

#[test]
fn walk_cases_replay_through_two_stores() {
    for case in spec()["catchUp"]["walkCases"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let asker = ChatStore::in_memory().unwrap();
        let seqs: Vec<u64> = from(&case["asker"]["seqs"]);
        asker.put_feed("r".into(), "s".into(), "dev-run".into(), feed(&seqs, 0), 0).unwrap();
        if let Some(complete) = from::<Option<u64>>(&case["asker"]["completeTo"]) {
            asker.set_meta("catchup.complete.s.dev-run".into(), complete.to_string()).unwrap();
        }
        let mut installs = Vec::new();
        let mut holders = HashMap::new();
        for (ix, holder) in case["holders"].as_array().unwrap().iter().enumerate() {
            let endpoint = holder["name"].as_str().unwrap().to_string();
            let runner = holder.get("runner") == Some(&json!(true));
            let mut entry = install(&endpoint, "me", if runner { "dev-run" } else { "dev-other" });
            // The fixture's holder order = newest seen first.
            entry.last_seen_at = Some(format!("2026-11-17T10:00:{:02}Z", 59 - ix));
            installs.push(entry);
            if holder.get("reachable") == Some(&json!(false)) {
                continue;
            }
            let store = ChatStore::in_memory().unwrap();
            store.put_feed("r".into(), "s".into(), "dev-run".into(), feed(&from::<Vec<u64>>(&holder["seqs"]), 0), 0).unwrap();
            holders.insert(endpoint, (store, from::<Option<u64>>(&holder["fullTo"])));
        }
        let catch_up = CatchUp::new(Arc::clone(&asker), "dev-me".into(), "me".into(), None);
        lock(&catch_up.state).page_limit = from(&case["limit"]);
        catch_up.set_installs("self".into(), installs);
        assert_eq!(catch_up.begin_pass(vec![session("s", "dev-run")], 1_000), 1, "{name}");
        let order: Vec<String> = case["holders"].as_array().unwrap().iter().map(|holder| holder["name"].as_str().unwrap().to_string()).collect();
        let asks = walk(&catch_up, &holders, "s", 1_000, &order);
        let held: Vec<u64> = from(&serde_json::from_str::<Value>(&asker.page("s".into(), "dev-run".into(), i64::MAX as u64, 10_000).unwrap()).unwrap()["seqs"]);
        let caught = catch_up.stats().caught_up == 1;
        let got = json!({
            "seqs": held,
            "completeTo": asker.store.complete_to("s", "dev-run").unwrap(),
            "asks": asks,
            "caughtUp": caught,
        });
        assert_eq!(got, case["expect"], "{name}");
        // A caught-up session waits for its row to change; a partial one for its retry.
        assert_eq!(catch_up.begin_pass(vec![session("s", "dev-run")], 2_000), 0, "{name}");
    }
}

#[test]
fn a_phone_asks_with_its_retention_cutoff_and_keeps_prose() {
    let day = DAY_MS;
    let now = 100 * day;
    let holder = ChatStore::in_memory().unwrap();
    let old = now - 40 * day;
    let rows = json!([
        { "seq": 0, "event": { "kind": "user_message", "text": "old ask", "at": old } },
        { "seq": 1, "event": { "kind": "tool", "id": "t", "name": "Bash", "at": old } },
        { "seq": 2, "event": { "kind": "narration", "text": "old answer", "at": old } },
        { "seq": 3, "event": { "kind": "tool", "id": "u", "name": "Read", "at": now - day } },
    ]);
    holder.put_feed("r".into(), "s".into(), "dev-run".into(), rows.to_string(), now).unwrap();
    holder.set_meta("catchup.complete.s.dev-run".into(), "3".into()).unwrap();
    let asker = ChatStore::in_memory().unwrap();
    let catch_up = CatchUp::new(Arc::clone(&asker), "phone".into(), "me".into(), Some(phone_retention_days()));
    catch_up.set_installs("self".into(), vec![install("run", "me", "dev-run")]);
    catch_up.begin_pass(vec![session("s", "dev-run")], now);
    let sid = catch_up.next_walk(now).unwrap();
    let ask = catch_up.next_ask(sid.clone(), now).unwrap();
    let frame: Value = serde_json::from_str(&ask.frame).unwrap();
    assert_eq!(frame["minAt"], json!(now - 30 * day));
    assert_eq!(frame["deviceId"], json!("dev-run"));
    let chunk = holder.answer_page("s".into(), ask.frame, PageAccess::Allowed).unwrap();
    assert_eq!(catch_up.take_answer(ask.request_id, chunk, now).unwrap().added, 3);
    assert!(catch_up.next_ask(sid, now).is_none());
    let held: Value = serde_json::from_str(&asker.page("s".into(), "dev-run".into(), i64::MAX as u64, 100).unwrap()).unwrap();
    assert_eq!(held["seqs"], json!([0, 2, 3]));
    // RETENTION on the phone's own store: prose stays, old tool steps go.
    asker.put_feed("r".into(), "s".into(), "dev-run".into(), json!([{ "seq": 9, "event": { "kind": "tool", "id": "v", "name": "Grep", "at": old } }]).to_string(), now).unwrap();
    assert_eq!(asker.apply_retention(now, Some(30)).unwrap(), 1);
    assert_eq!(asker.count("s".into(), "dev-run".into()).unwrap(), 3);
}

#[test]
fn a_refused_or_silent_holder_is_skipped_and_backed_off() {
    let asker = ChatStore::in_memory().unwrap();
    let catch_up = CatchUp::new(Arc::clone(&asker), "phone".into(), "me".into(), None);
    catch_up.set_installs("self".into(), vec![install("run", "me", "dev-run"), install("laptop", "me", "dev-b"), install("teammate", "them", "dev-c")]);
    catch_up.begin_pass(vec![session("s", "dev-run")], 0);
    let sid = catch_up.next_walk(0).unwrap();
    let first = catch_up.next_ask(sid.clone(), 0).unwrap();
    assert_eq!(first.peer, "run");
    catch_up.take_answer(first.request_id, None, 0).unwrap();
    let second = catch_up.next_ask(sid.clone(), 0).unwrap();
    // Never a teammate's device; the asker's own other install next.
    assert_eq!(second.peer, "laptop");
    let refusal = catchup::refusal_frame(&serde_json::from_str(&second.frame).unwrap(), "unauthorized");
    catch_up.take_answer(second.request_id.clone(), refusal, 0).unwrap();
    // A late or repeated answer is not taken twice.
    assert!(catch_up.take_answer(second.request_id, Some("{}".into()), 0).is_none());
    assert!(catch_up.next_ask(sid, 0).is_none());
    // Readers only (none here): retried after `passIntervalMs`, the
    // endpoints backed off for `endpointBackoffMs`.
    assert_eq!(catch_up.begin_pass(vec![session("s", "dev-run")], 1), 0);
    let later = catchup::PASS_INTERVAL_MS as i64;
    assert_eq!(catch_up.begin_pass(vec![session("s", "dev-run")], later), 1);
    let sid = catch_up.next_walk(later).unwrap();
    // Their backoff is over by then: the runner first again.
    assert_eq!(catch_up.next_ask(sid, later).unwrap().peer, "run");
    // This device's own runs are its journals', never walked.
    assert_eq!(catch_up.begin_pass(vec![session("mine", "phone")], later * 10), 0);
}

#[test]
fn the_holder_answers_under_the_access_rule() {
    let store = ChatStore::in_memory().unwrap();
    store.put_feed("r".into(), "s".into(), "d".into(), feed(&[0, 1, 2], 5), 5).unwrap();
    let page = json!({ "t": "history_page", "requestId": "q", "deviceId": "d", "limit": 2, "have": [] }).to_string();
    assert!(is_catch_up_page(page.clone()));
    let chunk: Value = serde_json::from_str(&store.answer_page("s".into(), page.clone(), PageAccess::Allowed).unwrap().unwrap()).unwrap();
    assert_eq!((chunk["seqs"].clone(), chunk["done"].clone(), chunk["fullTo"].clone()), (json!([0, 1]), json!(false), Value::Null));
    let refused: Value = serde_json::from_str(&store.answer_page("s".into(), page, PageAccess::NoSuchSession).unwrap().unwrap()).unwrap();
    assert_eq!((refused["error"].clone(), refused["seqs"].clone()), (json!("no_such_session"), json!([])));
    // A viewer's page (no device) is not one.
    let viewer = json!({ "t": "history_page", "requestId": "v", "beforeSeq": 3, "limit": 5 }).to_string();
    assert!(!is_catch_up_page(viewer.clone()));
    assert!(store.answer_page("s".into(), viewer, PageAccess::Allowed).unwrap().is_none());
}

#[test]
fn a_chat_opens_from_the_store_and_sign_out_deletes_it() {
    let dir = std::env::temp_dir().join(format!("chat-store-ffi-{}-{}", std::process::id(), line!()));
    let _ = std::fs::remove_dir_all(&dir);
    let data_dir = dir.to_string_lossy().to_string();
    let store = ChatStore::open(data_dir.clone(), "acct".into()).unwrap();
    store.put_feed("r".into(), "s".into(), "d".into(), feed(&(0..800).collect::<Vec<_>>(), 5), 5).unwrap();
    store
        .put_messages(json!([{ "id": "m1", "rootId": "r", "sessionId": "s", "authorKind": "agent", "body": "Shipped the café fix", "createdAt": 9 }]).to_string())
        .unwrap();
    let opened: Value = serde_json::from_str(&store.open_tail("s".into(), "d".into(), 500).unwrap()).unwrap();
    assert_eq!(opened["events"].as_array().unwrap().len(), 500);
    assert_eq!((opened["events"][0]["seq"].clone(), opened["truncated"].clone()), (json!(300), json!(true)));
    let older: Value = serde_json::from_str(&store.page("s".into(), "d".into(), 300, 100).unwrap()).unwrap();
    assert_eq!((older["seqs"][0].clone(), older["seqs"][99].clone()), (json!(200), json!(299)));
    let found: Value = serde_json::from_str(&store.search_chat("r".into(), "CAFE".into(), 10).unwrap()).unwrap();
    assert_eq!(found["total"], json!(1));
    assert!(ChatStore::open(data_dir.clone(), "../other".into()).is_err());
    drop(store);
    assert_eq!(delete_account_store(data_dir.clone(), "acct".into()).unwrap() >= 1, true);
    assert!(!dir.join("accounts/acct/chat-store.sqlite").exists());
    let _ = std::fs::remove_dir_all(&dir);
}

/// OPEN on a big chat (a 16 MiB journal is ~17k events): the time to the
/// first rows from the store. `cargo test -p chat-store-ffi --release
/// open_time -- --ignored --nocapture`.
#[test]
#[ignore]
fn open_time_on_a_big_chat() {
    let dir = std::env::temp_dir().join(format!("chat-store-ffi-bench-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let data_dir = dir.to_string_lossy().to_string();
    let store = ChatStore::open(data_dir.clone(), "acct".into()).unwrap();
    let body = "x".repeat(900);
    for chunk in (0..17_000u64).collect::<Vec<_>>().chunks(1_000) {
        let rows: Vec<Value> = chunk
            .iter()
            .map(|seq| match seq % 3 {
                0 => json!({ "seq": seq, "event": { "kind": "tool", "id": format!("t{seq}"), "name": "Bash", "detail": "cargo test", "at": seq } }),
                1 => json!({ "seq": seq, "event": { "kind": "tool_update", "id": format!("t{}", seq - 1), "output": body, "at": seq } }),
                _ => json!({ "seq": seq, "event": { "kind": "narration", "text": format!("step {seq} {body}"), "at": seq } }),
            })
            .collect();
        store.put_feed("r".into(), "s".into(), "d".into(), Value::Array(rows).to_string(), 0).unwrap();
    }
    drop(store);
    let started = std::time::Instant::now();
    let store = ChatStore::open(data_dir.clone(), "acct".into()).unwrap();
    let opened = store.open_tail("s".into(), "d".into(), 500).unwrap();
    let ms = started.elapsed().as_secs_f64() * 1000.0;
    let rows = serde_json::from_str::<Value>(&opened).unwrap()["events"].as_array().unwrap().len();
    println!("open_tail of 17000 events: {rows} rows in {ms:.1} ms ({} KiB of JSON)", opened.len() / 1024);
    drop(store);
    let _ = std::fs::remove_dir_all(&dir);
}
