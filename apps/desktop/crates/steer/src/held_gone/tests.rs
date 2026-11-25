use super::*;
use chat_store::{StoredEvent, StoredMessage};
use serde_json::json;
use std::sync::atomic::AtomicUsize;

const H: i64 = 3_600_000;
const T0: i64 = 1_800_000_000_000;
const ME: &str = "app.test--me";
const YOU: &str = "app.test--you";

fn uid(n: usize) -> String {
    format!("00000000-0000-4000-8000-{n:012}")
}

/// The server: what it calls gone; `fail_after` = that many calls answer.
#[derive(Default)]
struct Fake {
    gone: Mutex<BTreeSet<String>>,
    asked: Mutex<Vec<Vec<String>>>,
    fail_after: Mutex<Option<usize>>,
}

impl HeldGoneApi for Arc<Fake> {
    fn gone(&self, ids: &[String]) -> Result<Vec<String>, ApiError> {
        let mut asked = lock(&self.asked);
        if lock(&self.fail_after).is_some_and(|calls| asked.len() >= calls) {
            return Err(ApiError::Http { status: 404, message: "No procedure found on path".into() });
        }
        asked.push(ids.to_vec());
        // Also names an id nobody asked for.
        let mut answer: Vec<String> = ids.iter().filter(|id| lock(&self.gone).contains(*id)).cloned().collect();
        answer.push(uid(999_999));
        Ok(answer)
    }
}

struct World {
    dir: PathBuf,
    fake: Arc<Fake>,
    deps: HeldGoneDeps,
    stopped: AtomicBool,
}

impl Drop for World {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

impl World {
    fn new(account: &str) -> World {
        static N: AtomicUsize = AtomicUsize::new(0);
        let dir = std::env::temp_dir().join(format!("steer-held-gone-{}-{}", std::process::id(), N.fetch_add(1, Ordering::SeqCst)));
        let _ = std::fs::remove_dir_all(&dir);
        let fake = Arc::new(Fake::default());
        let deps = HeldGoneDeps {
            data_dir: dir.clone(),
            account_id: account.to_string(),
            api: Arc::new(Arc::clone(&fake)),
            ready: Arc::new(|| true),
            store: Arc::new(|| None),
        };
        World { dir, fake, deps, stopped: AtomicBool::new(false) }
    }

    /// A run record, its journal with the diff sidecar and (repo-less) its
    /// scratch dir; returns the cwd.
    fn run(&self, id: &str, account: &str, repo: bool) -> PathBuf {
        let cwd = if repo { self.dir.join("worktrees").join(id) } else { coding::scratch::scratch_root(&self.dir).join(id).join(id) };
        std::fs::create_dir_all(&cwd).unwrap();
        std::fs::write(cwd.join("report.md"), "x").unwrap();
        let mut record: coding::run_registry::RunRecord = serde_json::from_value(json!({
            "sessionId": id, "accountId": account, "agent": "claude", "kind": "thread", "cwd": cwd, "recordedAt": 1
        }))
        .unwrap();
        record.clone = repo.then(|| self.dir.join("clone"));
        coding::run_registry::record(&self.dir, record);
        self.journal(id);
        cwd
    }

    fn journal(&self, id: &str) {
        std::fs::create_dir_all(crate::history::journal_dir(&self.dir)).unwrap();
        std::fs::write(crate::history::journal_path(&self.dir, id).unwrap(), "{}\n").unwrap();
        std::fs::write(crate::history::diff_sidecar_path(&self.dir, id).unwrap(), "{}").unwrap();
    }

    fn cache(&self, account: &str, id: &str) {
        let store = SqliteStore::open(&self.dir.join("accounts").join(account).join("chat-store.sqlite")).unwrap();
        store
            .put_events(&[StoredEvent {
                root_id: id.into(),
                session_id: id.into(),
                device_id: "d".into(),
                seq: 1,
                stored_at: 1,
                event: json!({"kind": "narration", "text": "wombat"}),
            }])
            .unwrap();
        store
            .put_messages(&[StoredMessage {
                id: format!("m-{id}"),
                root_id: id.into(),
                session_id: id.into(),
                author_kind: "user".into(),
                body: Some("wombat".into()),
                created_at: 1,
            }])
            .unwrap();
    }

    fn cached(&self, account: &str, id: &str) -> bool {
        let path = self.dir.join("accounts").join(account).join("chat-store.sqlite");
        path.is_file() && SqliteStore::open(&path).unwrap().names(id).unwrap()
    }

    /// (record, journal, sidecar) of `id`.
    fn left(&self, id: &str) -> (bool, bool, bool) {
        (
            coding::run_registry::get(&self.dir, id).is_some(),
            crate::history::journal_path(&self.dir, id).unwrap().exists(),
            crate::history::diff_sidecar_path(&self.dir, id).unwrap().exists(),
        )
    }

    fn gone(&self, ids: &[&str]) {
        *lock(&self.fake.gone) = ids.iter().map(|id| id.to_string()).collect();
    }

    fn all(&self, now: i64) -> Done {
        pass(&self.deps, Ask::Everything, now, Duration::ZERO, &self.stopped)
    }

    fn notices(&self, now: i64, ids: &[&str]) -> Done {
        self.deps.update(|held| held.notices.extend(ids.iter().map(|id| id.to_string())));
        pass(&self.deps, Ask::Notices, now, Duration::ZERO, &self.stopped)
    }
}

const ALL: (bool, bool, bool) = (true, true, true);
const NONE: (bool, bool, bool) = (false, false, false);

#[test]
fn two_gone_answers_a_day_apart_remove_everything_but_the_worktree() {
    let w = World::new(ME);
    let (chat, repo, kept) = (uid(1), uid(2), uid(3));
    let scratch = w.run(&chat, ME, false);
    let worktree = w.run(&repo, ME, true);
    w.run(&kept, ME, false);
    w.cache(ME, &chat);
    w.cache(ME, &kept);
    w.gone(&[&chat, &repo]);
    let first = w.all(T0);
    assert!(first.answered && first.removed.is_empty());
    // The first answer is durable.
    assert_eq!(w.deps.load().state[&chat], GoneAnswers { first: T0, last: T0 });
    assert!(w.all(T0 + 23 * H).removed.is_empty());
    assert_eq!((w.left(&chat), w.left(&repo)), (ALL, ALL));
    assert_eq!(w.all(T0 + 24 * H).removed, vec![chat.clone(), repo.clone()]);
    assert_eq!((w.left(&chat), w.left(&repo), w.left(&kept)), (NONE, NONE, ALL));
    assert!(!scratch.exists() && worktree.join("report.md").exists());
    assert!(!w.cached(ME, &chat) && w.cached(ME, &kept));
    assert_eq!(w.deps.load(), Held::default());
}

#[test]
fn a_kept_answer_or_a_clock_that_went_backwards_starts_over() {
    let w = World::new(ME);
    let id = uid(1);
    w.run(&id, ME, false);
    w.gone(&[&id]);
    w.all(T0);
    w.gone(&[]);
    w.all(T0 + 6 * H);
    assert_eq!(w.deps.load().state.len(), 0);
    w.gone(&[&id]);
    assert!(w.all(T0 + 30 * H).removed.is_empty());
    assert!(w.all(T0 + 20 * H).removed.is_empty(), "the clock went backwards");
    assert!(w.all(T0 + 43 * H).removed.is_empty(), "23 h after the restart");
    assert_eq!(w.all(T0 + 44 * H).removed, vec![id.clone()]);
}

#[test]
fn a_failed_ask_changes_nothing_not_even_the_batches_that_answered() {
    let w = World::new(ME);
    // Two batches, all from the chat store; the second fails.
    let store = SqliteStore::open(&w.dir.join("accounts").join(ME).join("chat-store.sqlite")).unwrap();
    let ids: Vec<String> = (0..held_gone::BATCH + 1).map(uid).collect();
    store
        .put_messages(
            &ids.iter()
                .map(|id| StoredMessage {
                    id: format!("m-{id}"),
                    root_id: id.clone(),
                    session_id: id.clone(),
                    author_kind: "user".into(),
                    body: None,
                    created_at: 1,
                })
                .collect::<Vec<_>>(),
        )
        .unwrap();
    w.gone(&[&ids[0]]);
    *lock(&w.fake.fail_after) = Some(1);
    assert!(!w.all(T0).answered);
    assert_eq!(w.deps.load(), Held::default());
    assert_eq!(lock(&w.fake.asked).iter().map(Vec::len).collect::<Vec<_>>(), vec![held_gone::BATCH]);
    // An older server (404 from the start): nothing either.
    *lock(&w.fake.fail_after) = Some(0);
    assert!(!w.all(T0 + 25 * H).answered);
    *lock(&w.fake.fail_after) = None;
    assert!(w.all(T0 + 26 * H).answered);
    assert_eq!(lock(&w.fake.asked).last().unwrap().len(), 1, "the second batch");
    assert_eq!(w.deps.load().state.keys().collect::<Vec<_>>(), vec![&ids[0]]);
}

#[test]
fn a_notice_and_one_gone_answer_remove_at_once_a_notice_alone_never() {
    let w = World::new(ME);
    let (chat, other) = (uid(1), uid(2));
    let scratch = w.run(&chat, ME, false);
    w.run(&other, ME, false);
    w.cache(ME, &chat);
    // Offline: the notice waits.
    *lock(&w.fake.fail_after) = Some(0);
    let done = w.notices(T0, &[&chat]);
    assert!(done.notices_left && done.removed.is_empty());
    assert_eq!(w.left(&chat), ALL);
    // The server still has it: the notice is dropped.
    *lock(&w.fake.fail_after) = None;
    assert!(!w.notices(T0, &[]).notices_left);
    assert_eq!(w.left(&chat), ALL);
    // Announced and confirmed: gone, and only that id was asked about.
    w.gone(&[&chat, &other]);
    assert_eq!(w.notices(T0, &[&chat, &uid(77)]).removed, vec![chat.clone()]);
    assert_eq!(lock(&w.fake.asked).last().unwrap(), &vec![chat.clone()]);
    assert_eq!((w.left(&chat), w.left(&other)), (NONE, ALL));
    assert!(!scratch.exists() && !w.cached(ME, &chat));
    assert_eq!(w.deps.load(), Held::default());
}

#[test]
fn a_pass_that_would_take_most_of_it_takes_nothing_but_announced_deletes() {
    let w = World::new(ME);
    let ids: Vec<String> = (0..30).map(uid).collect();
    for id in &ids {
        w.run(id, ME, true);
    }
    w.gone(&ids.iter().map(String::as_str).collect::<Vec<_>>());
    w.all(T0);
    assert!(w.all(T0 + 25 * H).removed.is_empty());
    assert_eq!(w.deps.load().state.len(), 30);
    assert_eq!(w.deps.load().state[&ids[0]].first, T0);
    assert_eq!(w.notices(T0 + 25 * H, &[&ids[3]]).removed, vec![ids[3].clone()]);
    // Fewer of them gone: the first answers still count.
    w.gone(&ids[..12].iter().map(String::as_str).collect::<Vec<_>>());
    assert_eq!(w.all(T0 + 26 * H).removed.len(), 11);
}

#[test]
fn another_account_keeps_its_own_and_its_answers() {
    let w = World::new(ME);
    let (mine, yours) = (uid(1), uid(2));
    w.run(&mine, ME, false);
    w.run(&yours, YOU, false);
    // This account read the other's shared chat: only its own rows go.
    w.cache(ME, &yours);
    w.cache(YOU, &yours);
    w.gone(&[&yours]);
    w.all(T0);
    assert_eq!(w.all(T0 + 24 * H).removed, vec![yours.clone()]);
    assert_eq!(w.left(&yours), ALL);
    assert!(!w.cached(ME, &yours) && w.cached(YOU, &yours));
    // The other account starts its own two answers.
    let theirs = HeldGoneDeps { account_id: YOU.into(), ..w.deps.clone() };
    assert!(pass(&theirs, Ask::Everything, T0 + 25 * H, Duration::ZERO, &w.stopped).removed.is_empty());
    assert_eq!(lock(&w.fake.asked).last().unwrap(), &vec![yours.clone()]);
    assert_eq!(w.left(&yours), ALL);
}

#[test]
fn a_journal_nobody_is_tied_to_stays() {
    let w = World::new(ME);
    let (loose, cached, both) = (uid(1), uid(2), uid(3));
    for id in [&loose, &cached, &both] {
        w.journal(id);
    }
    w.cache(ME, &cached);
    w.cache(ME, &both);
    w.cache(YOU, &both);
    w.gone(&[&loose, &cached, &both]);
    w.notices(T0, &[&loose, &cached, &both]);
    assert_eq!((w.left(&loose).1, w.left(&cached).1, w.left(&both).1), (true, false, true));
    assert!(!w.cached(ME, &cached) && !w.cached(ME, &both) && w.cached(YOU, &both));
}

#[test]
fn a_live_run_is_ended_before_anything_of_it_goes() {
    let w = World::new(ME);
    let id = uid(1);
    let cwd = w.run(&id, ME, false);
    coding::run_registry::update(&w.dir, &id, |record| {
        record.host_pid = Some(std::process::id());
        true
    });
    w.gone(&[&id]);
    let done = w.notices(T0, &[&id]);
    assert!(done.removed.is_empty() && done.notices_left);
    assert!(w.left(&id) == ALL && cwd.exists());
    coding::run_registry::update(&w.dir, &id, |record| {
        record.host_pid = None;
        true
    });
    assert_eq!(w.notices(T0, &[]).removed, vec![id.clone()]);
    assert_eq!(w.left(&id), NONE);
}

#[test]
fn a_stopped_host_removes_nothing() {
    let w = World::new(ME);
    let id = uid(1);
    w.run(&id, ME, false);
    w.gone(&[&id]);
    w.stopped.store(true, Ordering::SeqCst);
    assert_eq!(w.notices(T0, &[&id]), Done::default());
    assert_eq!(w.left(&id), ALL);
}

/// The thread asks once the account is synced, and a kill's notice at once.
#[test]
fn the_thread_waits_for_the_sync_and_hears_notices() {
    let w = World::new("app.test--thread");
    let (first, second) = (uid(1), uid(2));
    for id in [&first, &second] {
        w.run(id, "app.test--thread", false);
    }
    let ready = Arc::new(AtomicBool::new(false));
    let deps = HeldGoneDeps { ready: { let ready = Arc::clone(&ready); Arc::new(move || ready.load(Ordering::SeqCst)) }, ..w.deps.clone() };
    let handle = spawn(deps);
    std::thread::sleep(Duration::from_millis(300));
    assert!(lock(&w.fake.asked).is_empty());
    ready.store(true, Ordering::SeqCst);
    let wait = |what: &str, done: &dyn Fn() -> bool| {
        let until = Instant::now() + Duration::from_secs(15);
        while !done() {
            assert!(Instant::now() < until, "{what}");
            std::thread::sleep(Duration::from_millis(50));
        }
    };
    wait("the start ask", &|| lock(&w.fake.asked).len() == 1);
    w.gone(&[&first]);
    notice("app.test--nobody", first.clone());
    notice("app.test--thread", first.clone());
    wait("the notice", &|| w.left(&first) == NONE);
    assert_eq!(w.left(&second), ALL);
    handle.stop();
    notice("app.test--thread", second.clone());
}
