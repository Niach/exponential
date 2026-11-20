//! #EDE-141: the open team chat's rows, held in memory and moved by the
//! sync's deltas. The chat used to read every row of twelve shapes (every
//! message of the account, each opened) on the gpui thread at each repaint,
//! up to four times a second while ANY session of the account streamed.
//! Now the first read takes the shapes the chat draws from (the open
//! tree's messages only; never the notifications or the boards), a batch
//! the sync applied is a point read of its keys, and a repaint is owed only
//! when what moved is the open chat's: its tree's sessions and messages, or
//! a shape every delta of which repainted before (`team_chat::SHAPES`).

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;
use std::time::SystemTime;

use domain::rows::{AgentDefRow, Issue, MessageRow, PromptRow, PullRequestRow, SessionIssueRow, SessionRow, Team, TeamMember, User};
use serde::de::DeserializeOwned;
use serde_json::Value;
use sync::sealed::Opener;
use sync::store::ShapeStore;

use crate::inbox::AccountRows;
use crate::team_chat::{TeamChat, TeamRows};

/// The shapes the chat reads (each keyed by `id`), in the order a refresh
/// takes them (the sessions before the messages: the opener learns each
/// session's root).
const READS: &[&str] = &["teams", "issues", "session_issues", "agents", "pull_requests", "users", "prompts", "team_members", "sessions", "messages"];

/// More keys than this waiting on one shape: the shape is read whole.
const MAX_KEYS: usize = 512;

/// What moved on one shape since the last refresh.
enum Pending {
    /// These rows (point reads), in batch order.
    Keys(Vec<String>, HashSet<String>),
    /// The whole shape (a refetch, too many keys).
    Whole,
}

pub struct ChatRows {
    account_id: String,
    opener: Arc<Opener>,
    /// The app's keyring file: a key written there later opens the rows
    /// that were sealed (they are read and opened again).
    keyring: PathBuf,
    keyring_stamp: Option<SystemTime>,
    rows: Rc<TeamRows>,
    /// The session whose tree `rows.messages` holds (`None` = the home).
    tree: Option<String>,
    first: bool,
    /// What moved is the open chat's and it has not been drawn since (a
    /// send or a press may refresh the rows before the next paint).
    owed: bool,
    pending: BTreeMap<&'static str, Pending>,
    /// Rows read off the store so far (#EDE-141's measure).
    pub rows_read: usize,
}

/// A row's primary key (every shape the chat reads is keyed by `id`).
trait Keyed {
    fn key(&self) -> &str;
}

macro_rules! keyed {
    ($($row:ty),*) => {$(
        impl Keyed for $row {
            fn key(&self) -> &str {
                &self.id
            }
        }
    )*};
}

keyed!(Team, Issue, SessionIssueRow, AgentDefRow, PullRequestRow, User, PromptRow, TeamMember, SessionRow, MessageRow);

/// `shape`'s rows `column` names with one of `values` (`None` = every
/// row), typed (rows that do not parse drop out); counted into `read`.
fn fetch<T: DeserializeOwned>(store: &ShapeStore, shape: &str, by: Option<(&str, &[String])>, read: &mut usize) -> Vec<T> {
    let Some(spec) = sync::shapes::shape_by_name(shape) else { return Vec::new() };
    let maps = match by {
        Some((column, values)) => store.read_where_in(spec, column, &values.iter().map(String::as_str).collect::<Vec<_>>()),
        None => store.read_all(spec),
    };
    let maps = maps.unwrap_or_default();
    *read += maps.len();
    maps.into_iter().filter_map(|row| serde_json::from_value(Value::Object(row)).ok()).collect()
}

/// `rows` of `shape` brought up to the store: all of them, or `what`'s keys.
fn bring<T: Keyed + DeserializeOwned>(rows: &mut Vec<T>, store: &ShapeStore, shape: &str, what: &Pending, read: &mut usize) {
    match what {
        Pending::Whole => *rows = fetch(store, shape, None, read),
        Pending::Keys(keys, _) => merge(rows, keys, fetch(store, shape, Some(("id", keys.as_slice())), read)),
    }
}

impl ChatRows {
    pub fn new(account_id: &str, user_id: &str, opener: Arc<Opener>, keyring: PathBuf) -> Self {
        let rows = TeamRows { account: AccountRows { account_id: account_id.to_string(), user_id: user_id.to_string(), ..Default::default() }, messages: Vec::new(), users: Vec::new(), prompts: Vec::new(), members: Vec::new() };
        ChatRows { account_id: account_id.to_string(), opener, keyring, keyring_stamp: None, rows: Rc::new(rows), tree: None, first: true, owed: false, pending: BTreeMap::new(), rows_read: 0 }
    }

    pub fn account_id(&self) -> &str {
        &self.account_id
    }

    /// The rows as of the last refresh.
    pub fn rows(&self) -> Rc<TeamRows> {
        self.rows.clone()
    }

    /// A batch the sync applied to `shape` (its keys; `whole` = the table
    /// was replaced). Nothing is read here.
    pub fn note(&mut self, shape: &str, keys: &[sync::protocol::RowKey], whole: bool) {
        let Some(shape) = READS.iter().copied().find(|s| *s == shape) else { return };
        let pending = self.pending.entry(shape).or_insert_with(|| Pending::Keys(Vec::new(), HashSet::new()));
        let mut whole = whole;
        if let Pending::Keys(list, seen) = pending {
            for key in keys {
                match key {
                    sync::protocol::RowKey::Single(id) => {
                        if seen.insert(id.clone()) {
                            list.push(id.clone());
                        }
                    }
                    // No shape the chat reads has a composite key.
                    sync::protocol::RowKey::Pair(..) => whole = true,
                }
            }
            whole |= list.len() > MAX_KEYS;
        }
        if whole {
            *pending = Pending::Whole;
        }
    }

    /// Brings the rows up to `chat` (the open session, what the sync
    /// noted, a key the app wrote since): `true` when what moved is the
    /// open chat's, so it owes a repaint ([`Self::take_owed`]).
    pub fn refresh(&mut self, store: &ShapeStore, chat: &TeamChat) -> bool {
        let moved = self.bring_up(store, chat);
        self.owed |= moved;
        moved
    }

    /// Whether a repaint is owed since the last one; the caller paints.
    pub fn take_owed(&mut self) -> bool {
        std::mem::take(&mut self.owed)
    }

    fn bring_up(&mut self, store: &ShapeStore, chat: &TeamChat) -> bool {
        let stamp = std::fs::metadata(&self.keyring).and_then(|m| m.modified()).ok();
        if std::mem::take(&mut self.first) {
            self.pending.clear();
            self.keyring_stamp = stamp;
            self.tree = chat.session_id.clone();
            for shape in READS {
                self.read_shape(store, shape, &Pending::Whole);
            }
            return true;
        }
        let mut moved = false;
        if stamp != self.keyring_stamp {
            self.keyring_stamp = stamp;
            self.pending.insert("sessions", Pending::Whole);
            self.pending.insert("messages", Pending::Whole);
            moved = true;
        }
        if chat.session_id != self.tree {
            self.tree = chat.session_id.clone();
            self.pending.insert("messages", Pending::Whole);
            moved = true;
        }
        let mut pending = std::mem::take(&mut self.pending);
        for shape in READS {
            let Some(what) = pending.remove(shape) else { continue };
            moved |= self.read_shape(store, shape, &what);
        }
        moved
    }

    /// `shape` brought up to the store (`what` moved): `true` when the
    /// open chat owes a repaint for it.
    fn read_shape(&mut self, store: &ShapeStore, shape: &str, what: &Pending) -> bool {
        match (shape, what) {
            ("sessions", Pending::Keys(keys, _)) => return self.sessions_moved(store, keys),
            ("messages", Pending::Keys(keys, _)) => return self.messages_moved(store, keys),
            _ => {}
        }
        let read = &mut self.rows_read;
        let rows = Rc::make_mut(&mut self.rows);
        match shape {
            "teams" => bring(&mut rows.account.teams, store, shape, what, read),
            "issues" => bring(&mut rows.account.issues, store, shape, what, read),
            "session_issues" => bring(&mut rows.account.session_issues, store, shape, what, read),
            "agents" => bring(&mut rows.account.agents, store, shape, what, read),
            "pull_requests" => bring(&mut rows.account.pull_requests, store, shape, what, read),
            "users" => bring(&mut rows.users, store, shape, what, read),
            "prompts" => bring(&mut rows.prompts, store, shape, what, read),
            "team_members" => bring(&mut rows.members, store, shape, what, read),
            "sessions" => {
                rows.account.sessions = fetch(store, shape, None, read);
                for row in &mut rows.account.sessions {
                    self.opener.open_session(row);
                }
            }
            // The open tree's messages (the home: none), off the `root_id`
            // index. The read before took a message without a `root_id` into
            // every chat; the shape has carried the column since before the
            // shell had a store, so no row of the shell's lacks it.
            "messages" => {
                rows.messages = match &self.tree {
                    Some(root) => fetch(store, shape, Some(("root_id", std::slice::from_ref(root))), read),
                    None => Vec::new(),
                };
                for message in &mut rows.messages {
                    self.opener.open_message(message);
                }
            }
            _ => {}
        }
        crate::team_chat::SHAPES.contains(&shape)
    }

    /// The open tree's root as the rows know it (the open id until its row syncs).
    fn open_root(&self) -> Option<String> {
        let open = self.tree.as_deref()?;
        Some(self.rows.account.sessions.iter().find(|s| s.id == open).map_or(open, |s| s.root_id_or_self()).to_string())
    }

    fn in_tree(&self, root: Option<&str>, session: &SessionRow) -> bool {
        self.tree.as_deref() == Some(session.id.as_str()) || root.is_some_and(|root| session.root_id_or_self() == root)
    }

    /// `keys` of `sessions` again: `true` when one of them is (or was) in the open tree.
    fn sessions_moved(&mut self, store: &ShapeStore, keys: &[String]) -> bool {
        let mut fresh: Vec<SessionRow> = fetch(store, "sessions", Some(("id", keys)), &mut self.rows_read);
        for row in &mut fresh {
            self.opener.open_session(row);
        }
        let before = self.open_root();
        let was = self.rows.account.sessions.iter().any(|s| keys.contains(&s.id) && self.in_tree(before.as_deref(), s));
        merge(&mut Rc::make_mut(&mut self.rows).account.sessions, keys, fresh.clone());
        let after = self.open_root();
        was || fresh.iter().any(|s| self.in_tree(after.as_deref(), s))
    }

    /// `keys` of `messages` again, the open tree's kept: `true` when one of them is (or was) the open tree's.
    fn messages_moved(&mut self, store: &ShapeStore, keys: &[String]) -> bool {
        let Some(root) = self.tree.clone() else { return false };
        let mut fresh: Vec<MessageRow> = fetch(store, "messages", Some(("id", keys)), &mut self.rows_read);
        fresh.retain(|m| m.root_id.as_deref() == Some(root.as_str()));
        for message in &mut fresh {
            self.opener.open_message(message);
        }
        let was = self.rows.messages.iter().any(|m| keys.contains(&m.id));
        let moved = was || !fresh.is_empty();
        if moved {
            merge(&mut Rc::make_mut(&mut self.rows).messages, keys, fresh);
        }
        moved
    }
}

/// `keys`' rows as `fresh` holds them now: a row in place keeps its place,
/// a new one goes last in batch order (a table's own order), one `fresh`
/// lacks is gone.
fn merge<T: Keyed>(rows: &mut Vec<T>, keys: &[String], fresh: Vec<T>) {
    let mut by_key: HashMap<String, T> = fresh.into_iter().map(|row| (row.key().to_string(), row)).collect();
    let touched: HashSet<&str> = keys.iter().map(String::as_str).collect();
    rows.retain_mut(|row| {
        if !touched.contains(row.key()) {
            return true;
        }
        match by_key.remove(row.key()) {
            Some(new) => {
                *row = new;
                true
            }
            None => false,
        }
    });
    for key in keys {
        if let Some(new) = by_key.remove(key) {
            rows.push(new);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use serde_json::json;
    use sync::protocol::{RowKey, ShapeMessage};

    use super::*;
    use crate::sealed::KeyringFile;

    const TEAM: &str = "t1";
    const NOW: i64 = 1_795_000_000_000;

    /// Every shape the chat ever read before #EDE-141 (main.rs `team_chat`).
    const TWELVE: &[&str] = &["teams", "notifications", "issues", "boards", "sessions", "session_issues", "agents", "pull_requests", "messages", "users", "prompts", "team_members"];

    struct Account {
        dir: tempfile::TempDir,
        store: ShapeStore,
    }

    impl Account {
        fn keyring(&self) -> PathBuf {
            self.dir.path().join("seal-keys")
        }

        fn opener(&self) -> Arc<Opener> {
            Arc::new(Opener::new(Arc::new(KeyringFile::new(self.keyring()))))
        }

        fn rows(&self) -> usize {
            TWELVE.iter().map(|s| self.store.count(sync::shapes::shape_by_name(s).unwrap()).unwrap() as usize).sum()
        }
    }

    fn put(store: &ShapeStore, shape: &str, rows: Vec<Value>) -> Vec<RowKey> {
        let spec = sync::shapes::shape_by_name(shape).unwrap();
        let keys: Vec<RowKey> = rows.iter().map(|r| RowKey::Single(r["id"].as_str().unwrap().to_string())).collect();
        let msgs: Vec<ShapeMessage> = rows.into_iter().zip(&keys).map(|(r, key)| ShapeMessage::Insert { key: key.clone(), value: r.as_object().unwrap().clone() }).collect();
        store.apply_batch(spec, &msgs, None).unwrap();
        keys
    }

    fn gone(store: &ShapeStore, shape: &str, id: &str) -> Vec<RowKey> {
        let key = RowKey::Single(id.to_string());
        store.apply_batch(sync::shapes::shape_by_name(shape).unwrap(), &[ShapeMessage::Delete { key: key.clone() }], None).unwrap();
        vec![key]
    }

    fn root(t: usize) -> String {
        format!("r{t:04}")
    }

    fn message(root: &str, n: usize) -> Value {
        let user = n % 2 == 0;
        json!({
            "id": format!("{root}-m{n:05}"), "session_id": root, "root_id": root, "team_id": TEAM,
            "author_kind": if user { "user" } else { "agent" }, "author_id": if user { json!("u1") } else { Value::Null },
            "body": format!("Message {n} of {root}"), "created_at": format!("2026-11-19T{:02}:{:02}:{:02}Z", 8 + n / 3600, (n / 60) % 60, n % 60),
        })
    }

    /// An account of `trees` conversations (a root and a child each) of
    /// `per_tree` messages, an issue, a link and a pull request per tree,
    /// and the notifications and boards the chat never draws.
    fn account(trees: usize, per_tree: usize) -> Account {
        let dir = tempfile::tempdir().unwrap();
        let store = ShapeStore::open(&dir.path().join("sync.sqlite")).unwrap();
        put(&store, "teams", vec![json!({"id": TEAM, "name": "Acme", "slug": "acme"})]);
        put(&store, "agents", vec![json!({"id": "a1", "team_id": TEAM, "name": "Assistant", "role": "assistant", "owner_id": "u1"}), json!({"id": "a2", "team_id": TEAM, "name": "Triage", "icon": "bug", "role": "agent", "sort_order": 1})]);
        put(&store, "users", (1..=5).map(|i| json!({"id": format!("u{i}"), "name": format!("Person {i}"), "email": format!("p{i}@example.com")})).collect());
        put(&store, "team_members", (1..=5).map(|i| json!({"id": format!("tm{i}"), "team_id": TEAM, "user_id": format!("u{i}")})).collect());
        put(&store, "prompts", (1..=3).map(|i| json!({"id": format!("p{i}"), "team_id": TEAM, "name": format!("Prompt {i}")})).collect());
        put(&store, "boards", vec![json!({"id": "b1", "team_id": TEAM, "name": "Desk", "slug": "desk"})]);
        put(
            &store,
            "sessions",
            (0..trees)
                .flat_map(|t| {
                    let r = root(t);
                    [
                        json!({"id": r, "team_id": TEAM, "user_id": "u1", "agent_id": "a2", "status": "idle", "title": format!("Chat {t}"), "created_at": "2026-11-19T08:00:00Z"}),
                        json!({"id": format!("{r}-child"), "root_id": r, "parent_id": r, "team_id": TEAM, "user_id": "u1", "status": "running", "title": format!("Child of {t}"), "created_at": "2026-11-19T08:01:00Z"}),
                    ]
                })
                .collect(),
        );
        put(&store, "issues", (0..trees).map(|t| json!({"id": format!("i{t}"), "team_id": TEAM, "board_id": "b1", "identifier": format!("ACME-{t}"), "title": format!("Issue {t}"), "status": "backlog", "number": t})).collect());
        put(&store, "session_issues", (0..trees).map(|t| json!({"id": format!("si{t}"), "session_id": root(t), "issue_id": format!("i{t}")})).collect());
        put(&store, "pull_requests", (0..trees).map(|t| json!({"id": format!("pr{t}"), "team_id": TEAM, "number": t, "title": format!("PR {t}"), "state": "open"})).collect());
        put(&store, "notifications", (0..trees).map(|t| json!({"id": format!("n{t}"), "user_id": "u1", "issue_id": format!("i{t}"), "type": "comment"})).collect());
        for t in 0..trees {
            put(&store, "messages", (0..per_tree).map(|n| message(&root(t), n)).collect());
        }
        Account { dir, store }
    }

    /// The chat's read before #EDE-141 (main.rs `team_chat`): every row of
    /// the twelve shapes, every one deserialised and opened.
    fn read_everything(a: &Account, opener: &Opener) -> TeamRows {
        let mut rows = TeamRows {
            account: AccountRows::read("acc", "u1", &a.store),
            messages: crate::inbox::read_rows(&a.store, "messages"),
            users: crate::inbox::read_rows(&a.store, "users"),
            prompts: crate::inbox::read_rows(&a.store, "prompts"),
            members: crate::inbox::read_rows(&a.store, "team_members"),
        };
        crate::sealed::open(opener, &mut rows.account.sessions, &mut rows.messages);
        rows
    }

    fn chat(session: Option<&str>) -> TeamChat {
        TeamChat { account_id: "acc".into(), team_id: TEAM.into(), session_id: session.map(str::to_string) }
    }

    /// What `chatting_changed` builds off the rows (the model, before the views).
    fn model(rows: &TeamRows, chat: &TeamChat) -> (uicore_model::chat::ChatInput, Value) {
        let input = crate::team_chat::input(rows, chat, NOW);
        let view = uicore_model::chat::chat_view(&input);
        (input, crate::chat::chatting(&view))
    }

    /// One more step of a reply: `rows` read and whether the chat repaints;
    /// the model is the one the whole read builds.
    fn step(a: &Account, cache: &mut ChatRows, chat: &TeamChat, shape: &str, keys: &[RowKey]) -> (usize, bool) {
        let before = cache.rows_read;
        cache.note(shape, keys, false);
        let moved = cache.refresh(&a.store, chat);
        assert_eq!(model(&cache.rows(), chat).0, model(&read_everything(a, &a.opener()), chat).0, "{shape}: the same chat as the whole read");
        (cache.rows_read - before, moved)
    }

    /// #EDE-141: the open chat reads its own tree and the rows that moved
    /// (never the other trees' messages, the notifications or the boards),
    /// repaints only for its own, and draws what the whole read draws.
    #[test]
    fn the_open_chat_reads_its_tree_and_what_moved_and_draws_the_same() {
        let a = account(12, 40);
        let open = chat(Some("r0003"));
        let mut cache = ChatRows::new("acc", "u1", a.opener(), a.keyring());
        assert!(cache.refresh(&a.store, &open), "the first read paints");
        let other_messages = 11 * 40;
        assert_eq!(cache.rows_read, a.rows() - other_messages - 12 - 1, "the first read: all but the other trees' messages, the notifications and the board");
        let (input, _) = model(&cache.rows(), &open);
        assert_eq!(input.session_id.as_deref(), Some("r0003"));
        assert_eq!(input.rows.iter().filter(|r| matches!(r, uicore_model::chat::ChatRowInput::Message { .. })).count(), 40, "the open tree's messages");
        assert_eq!(model(&cache.rows(), &open).0, model(&read_everything(&a, &a.opener()), &open).0);

        // A reply streams in the open chat: its message, then its session.
        // A read for a send before the next paint leaves the repaint owed.
        cache.take_owed();
        let keys = put(&a.store, "messages", vec![message("r0003", 40)]);
        assert_eq!(step(&a, &mut cache, &open, "messages", &keys), (1, true));
        assert!(!cache.refresh(&a.store, &open), "nothing new");
        assert!(cache.take_owed(), "the paint is still owed");
        assert!(!cache.take_owed());
        let mut edited = message("r0003", 40);
        edited["body"] = json!("Message 40, streamed further");
        let keys = put(&a.store, "messages", vec![edited]);
        assert_eq!(step(&a, &mut cache, &open, "messages", &keys), (1, true));
        let keys = put(&a.store, "sessions", vec![json!({"id": "r0003-child", "root_id": "r0003", "parent_id": "r0003", "team_id": TEAM, "user_id": "u1", "status": "idle", "title": "Child of 3", "created_at": "2026-11-19T08:01:00Z"})]);
        assert_eq!(step(&a, &mut cache, &open, "sessions", &keys), (1, true));
        // A new child joins the tree; a message goes.
        let keys = put(&a.store, "sessions", vec![json!({"id": "r0003-late", "root_id": "r0003", "parent_id": "r0003", "team_id": TEAM, "user_id": "u1", "status": "running", "title": "Late", "created_at": "2026-11-19T09:00:00Z"})]);
        assert_eq!(step(&a, &mut cache, &open, "sessions", &keys), (1, true));
        let keys = gone(&a.store, "messages", "r0003-m00002");
        assert_eq!(step(&a, &mut cache, &open, "messages", &keys), (0, true));

        // Another conversation of the account streams: nothing to repaint.
        let keys = put(&a.store, "messages", vec![message("r0007", 40)]);
        assert_eq!(step(&a, &mut cache, &open, "messages", &keys), (1, false));
        let keys = put(&a.store, "sessions", vec![json!({"id": "r0007", "team_id": TEAM, "user_id": "u1", "agent_id": "a2", "status": "running", "title": "Chat 7", "created_at": "2026-11-19T08:00:00Z"})]);
        assert_eq!(step(&a, &mut cache, &open, "sessions", &keys), (1, false));
        // An issue moved: its row, no repaint of its own (as before).
        let keys = put(&a.store, "issues", vec![json!({"id": "i3", "team_id": TEAM, "board_id": "b1", "identifier": "ACME-3", "title": "Issue 3, renamed", "status": "started", "number": 3})]);
        assert_eq!(step(&a, &mut cache, &open, "issues", &keys), (1, false));
        // An agent moved (its row; a new one after it): a shape every delta of which repaints.
        let keys = put(&a.store, "agents", vec![json!({"id": "a2", "team_id": TEAM, "name": "Triage bot", "icon": "bug", "role": "agent", "sort_order": 1})]);
        assert_eq!(step(&a, &mut cache, &open, "agents", &keys), (1, true));
        let keys = put(&a.store, "agents", vec![json!({"id": "a3", "team_id": TEAM, "name": "Reviewer", "role": "agent", "sort_order": 1})]);
        assert_eq!(step(&a, &mut cache, &open, "agents", &keys), (1, true));
        // A link and its pull request arrive; a prompt goes.
        let keys = put(&a.store, "session_issues", vec![json!({"id": "si-late", "session_id": "r0003-late", "issue_id": "i5"})]);
        assert_eq!(step(&a, &mut cache, &open, "session_issues", &keys), (1, true));
        let keys = put(&a.store, "pull_requests", vec![json!({"id": "pr3", "team_id": TEAM, "number": 3, "title": "PR 3", "state": "merged"})]);
        assert_eq!(step(&a, &mut cache, &open, "pull_requests", &keys), (1, true));
        let keys = gone(&a.store, "prompts", "p2");
        assert_eq!(step(&a, &mut cache, &open, "prompts", &keys), (0, true));
        assert_eq!(cache.rows().prompts.iter().map(|p| p.id.as_str()).collect::<Vec<_>>(), ["p1", "p3"]);

        // Another chat opens: its tree's messages; the home reads none.
        let other = chat(Some("r0005"));
        let before = cache.rows_read;
        assert!(cache.refresh(&a.store, &other));
        assert_eq!(cache.rows_read - before, 40);
        assert_eq!(model(&cache.rows(), &other).0, model(&read_everything(&a, &a.opener()), &other).0);
        let home = chat(None);
        assert!(cache.refresh(&a.store, &home));
        assert!(cache.rows().messages.is_empty());
        assert_eq!(model(&cache.rows(), &home).0, model(&read_everything(&a, &a.opener()), &home).0);
        assert_eq!(step(&a, &mut cache, &home, "messages", &put(&a.store, "messages", vec![message("r0005", 41)])), (0, false), "the home draws no messages");
    }

    /// A refetch (the whole table replaced) reads the shape again.
    #[test]
    fn a_refetch_reads_the_shape_again() {
        let a = account(3, 5);
        let open = chat(Some("r0001"));
        let mut cache = ChatRows::new("acc", "u1", a.opener(), a.keyring());
        cache.refresh(&a.store, &open);
        put(&a.store, "messages", vec![message("r0001", 5)]);
        let before = cache.rows_read;
        cache.note("messages", &[], true);
        assert!(cache.refresh(&a.store, &open));
        assert_eq!(cache.rows_read - before, 6);
        assert_eq!(cache.rows().messages.len(), 6);
    }

    fn timed<T>(f: impl FnOnce() -> T) -> (T, Duration) {
        let started = Instant::now();
        let out = f();
        (out, started.elapsed())
    }

    /// #EDE-141's measure (`cargo test -p ede-shell --bin exponential-shell
    /// chat_rows::tests::measure -- --ignored --nocapture`): a long-lived
    /// account (300 conversations, 30,000 messages); the chat on one of
    /// them while a reply streams there and while another conversation
    /// streams. Per repaint: the rows read off the store, and the gpui
    /// thread's time for the read and the chat's model.
    #[test]
    #[ignore]
    fn measure() {
        let a = account(300, 100);
        let open = chat(Some("r0150"));
        let rounds = 20u32;
        // Before: every delta of the account repainted, each repaint read everything.
        let mut before = Duration::ZERO;
        for _ in 0..rounds {
            let (_, took) = timed(|| {
                let rows = read_everything(&a, &a.opener());
                model(&rows, &open)
            });
            before += took;
        }
        println!("before: {} rows read per repaint, {:.1} ms per repaint (read + model), every delta of the account repaints", a.rows(), before.as_secs_f64() * 1000.0 / rounds as f64);

        let mut cache = ChatRows::new("acc", "u1", a.opener(), a.keyring());
        let (_, cold) = timed(|| {
            cache.refresh(&a.store, &open);
            model(&cache.rows(), &open)
        });
        println!("after, the chat opens: {} rows read, {:.1} ms", cache.rows_read, cold.as_secs_f64() * 1000.0);

        let (mut own, mut other) = ((0usize, Duration::ZERO, 0u32), (0usize, Duration::ZERO, 0u32));
        for n in 0..rounds as usize {
            for (tree, acc) in [("r0150", &mut own), ("r0151", &mut other)] {
                let keys = put(&a.store, "messages", vec![message(tree, 100 + n)]);
                let read = cache.rows_read;
                let (moved, took) = timed(|| {
                    cache.note("messages", &keys, false);
                    let moved = cache.refresh(&a.store, &open);
                    if moved {
                        model(&cache.rows(), &open);
                    }
                    moved
                });
                acc.0 += cache.rows_read - read;
                acc.1 += took;
                acc.2 += u32::from(moved);
            }
        }
        // The tray's read (#EDE-65): every row of eight shapes per account (no
        // messages), at most once a second while a sync streams; off the main loop now.
        let tray_rows: usize = std::iter::once("teams").chain(crate::inbox::SHAPES.iter().copied()).map(|s| a.store.count(sync::shapes::shape_by_name(s).unwrap()).unwrap() as usize).sum();
        let (_, tray) = timed(|| {
            for _ in 0..rounds {
                let rows = AccountRows::read("acc", "u1", &a.store);
                let _ = (crate::inbox::inbox_items(&rows, &HashSet::new()), crate::inbox::needs_you_items(&rows, NOW));
            }
        });
        println!("the tray's refresh: {tray_rows} rows read, {:.1} ms (before: on the gpui thread; after: off it)", tray.as_secs_f64() * 1000.0 / rounds as f64);
        let per = |acc: (usize, Duration, u32)| (acc.0 as f64 / rounds as f64, acc.1.as_secs_f64() * 1000.0 / rounds as f64, acc.2);
        let (rows, ms, repaints) = per(own);
        println!("after, a reply streams in the open chat: {rows} rows read per delta, {ms:.2} ms per delta, {repaints}/{rounds} repaint");
        let (rows, ms, repaints) = per(other);
        println!("after, another conversation streams: {rows} rows read per delta, {ms:.2} ms per delta, {repaints}/{rounds} repaint");
    }
}
