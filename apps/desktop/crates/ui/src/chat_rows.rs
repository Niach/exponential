//! #PERF-3 / #PERF-10: the Chats bands of one team, kept INCREMENTALLY.
//!
//! A chat row (`domain::session_tree::chat_list_rows`) reads its own tree
//! only: its sessions, their digest messages and issue links (and the
//! issues those name), the root's read mark. A write moves the trees whose
//! inputs it wrote and nothing else, so those trees' rows are derived again
//! and grouped with the rest (`domain::session_tree::group_chat_row_refs`),
//! never the whole team: the rail derived every one of 1.7k sessions per
//! synced row (tens of ms, the `render(RailView)` hangs).
//!
//! Per write kind, the trees derived again:
//! - `sessions`: the trees each written row left and joined;
//! - `session_issues`: the trees of the sessions a written link named,
//!   before and after the write;
//! - `issues`: the trees of the sessions linking a written issue;
//! - `session_reads`: the tree of the session a written mark named.
//!
//! A WHOLE derive ([`full_rows`]'s) happens only where nothing per row can
//! say what moved: the first derive, another store (an account switch),
//! another team (a team switch), or a change log that no longer reaches
//! back to the last derive (a wholesale refetch of a collection).

use std::collections::{HashMap, HashSet};

use domain::rows::{MessageRow, SessionIssueRow, SessionReadRow, SessionRow};
use domain::session_tree::{chat_list_rows, group_chat_rows, ChatsListGroup, ChatsListInput, ChatsListKey, ChatsListRow, TreeIssue};
use gpui::App;
use sync::protocol::RowKey;
use sync::Store;

/// One team's rows, by root, and the trees and links they were derived from.
#[derive(Default)]
pub(crate) struct ChatRows {
    team: String,
    store: Option<gpui::EntityId>,
    /// The `sessions`, `session_issues`, `issues` and `session_reads`
    /// revisions the rows are current to.
    revisions: [u64; 4],
    rows: HashMap<String, (ChatsListKey, ChatsListRow, i64)>,
    /// A team session → its root.
    root_of: HashMap<String, String>,
    /// A root → its tree's sessions.
    trees: HashMap<String, HashSet<String>>,
    /// Every link: id → (session, issue) (a deleted link still names its
    /// session's tree).
    links: HashMap<String, (String, String)>,
    /// Every read mark: id → its session.
    reads: HashMap<String, String>,
    /// The trees the last derive read (benches and guards).
    pub(crate) derived_trees: usize,
    /// The last derive was a whole one.
    pub(crate) whole: bool,
}

fn single_ids(keys: Vec<&RowKey>) -> Vec<String> {
    keys.into_iter()
        .filter_map(|key| match key {
            RowKey::Single(id) => Some(id.clone()),
            _ => None,
        })
        .collect()
}

fn sort_links(links: &mut [&SessionIssueRow]) {
    links.sort_by(|a, b| {
        a.sort_order
            .unwrap_or(0.0)
            .total_cmp(&b.sort_order.unwrap_or(0.0))
            .then_with(|| a.created_at.cmp(&b.created_at))
            .then_with(|| a.id.cmp(&b.id))
    });
}

/// `chat_list_rows` over `rows` (a team's sessions, or one tree of them):
/// their links, the issues those name, their digest messages, their reads.
fn rows_of(team_id: &str, rows: &[&SessionRow], cx: &App) -> Vec<(ChatsListKey, ChatsListRow)> {
    let collections = Store::global(cx).collections();
    let ids: HashSet<&str> = rows.iter().map(|session| session.id.as_str()).collect();
    let mut links: Vec<&SessionIssueRow> = collections.session_issues.read(cx).iter().filter(|link| ids.contains(link.session_id.as_str())).collect();
    sort_links(&mut links);
    let all_issues = collections.issues.read(cx);
    let mut seen: HashSet<&str> = HashSet::new();
    let issues: Vec<TreeIssue> = links
        .iter()
        .filter(|link| seen.insert(link.issue_id.as_str()))
        .filter_map(|link| all_issues.get(&link.issue_id))
        .map(TreeIssue::from_issue)
        .collect();
    let keyed = crate::session_data::digest_message_lists(cx, |owner| ids.contains(owner));
    let messages: Vec<&MessageRow> = keyed.iter().flat_map(|rows| rows.iter()).collect();
    let session_reads = collections.session_reads.read(cx);
    let reads: Vec<&SessionReadRow> = session_reads.iter().filter(|read| ids.contains(read.session_id.as_str())).collect();
    chat_list_rows(ChatsListInput {
        sessions: rows,
        session_issues: &links,
        issues: &issues,
        messages: &messages,
        team_id: Some(team_id),
        reads: &reads,
        now_ms: None,
    })
}

/// [`rows_of`] for ONE tree, its inputs looked up by session (the links'
/// index, the digests' cache) instead of a pass over every row.
fn tree_rows(team_id: &str, rows: &[&SessionRow], cx: &App) -> Vec<(ChatsListKey, ChatsListRow)> {
    let collections = Store::global(cx).collections();
    let index = crate::session_data::links_index(cx);
    let mut links: Vec<&SessionIssueRow> = rows.iter().flat_map(|session| index.0.get(&session.id).into_iter().flatten()).collect();
    sort_links(&mut links);
    let all_issues = collections.issues.read(cx);
    let mut seen: HashSet<&str> = HashSet::new();
    let issues: Vec<TreeIssue> = links
        .iter()
        .filter(|link| seen.insert(link.issue_id.as_str()))
        .filter_map(|link| all_issues.get(&link.issue_id))
        .map(TreeIssue::from_issue)
        .collect();
    let keyed = crate::session_data::digest_messages_of(rows, cx);
    let messages: Vec<&MessageRow> = keyed.iter().flat_map(|rows| rows.iter()).collect();
    let ids: HashSet<&str> = rows.iter().map(|session| session.id.as_str()).collect();
    let session_reads = collections.session_reads.read(cx);
    let reads: Vec<&SessionReadRow> = session_reads.iter().filter(|read| ids.contains(read.session_id.as_str())).collect();
    chat_list_rows(ChatsListInput {
        sessions: rows,
        session_issues: &links,
        issues: &issues,
        messages: &messages,
        team_id: Some(team_id),
        reads: &reads,
        now_ms: None,
    })
}

/// The reference: every row of the team derived at once, grouped
/// (`partition_chats`' answer).
pub(crate) fn full_rows(team_id: &str, now_ms: i64, cx: &App) -> Vec<ChatsListGroup> {
    let sessions = Store::global(cx).collections().sessions.read(cx);
    let rows: Vec<&SessionRow> = sessions.iter().filter(|session| session.team_id.as_deref() == Some(team_id)).collect();
    group_chat_rows(rows_of(team_id, &rows, cx), Some(now_ms))
}

impl ChatRows {
    /// The team's bands as of the store now.
    pub(crate) fn derive(&mut self, team_id: &str, now_ms: i64, cx: &App) -> Vec<ChatsListGroup> {
        let _span = tracing::trace_span!("chat_rows.derive").entered();
        let Some(store) = Store::try_global(cx) else { return Vec::new() };
        let c = store.collections();
        let sessions = c.sessions.read(cx);
        let links = c.session_issues.read(cx);
        let issues = c.issues.read(cx);
        let reads = c.session_reads.read(cx);
        let revisions = [sessions.revision(), links.revision(), issues.revision(), reads.revision()];
        let same = self.team == team_id && self.store == Some(c.sessions.entity_id());
        let changed = same
            .then(|| {
                Some((
                    single_ids(sessions.changed_since(self.revisions[0])?),
                    single_ids(links.changed_since(self.revisions[1])?),
                    single_ids(issues.changed_since(self.revisions[2])?),
                    single_ids(reads.changed_since(self.revisions[3])?),
                ))
            })
            .flatten();
        match changed {
            Some((written_sessions, written_links, written_issues, written_reads)) => {
                self.whole = false;
                let mut touched: HashSet<String> = HashSet::new();
                // The trees each written session left and joined.
                for id in &written_sessions {
                    if let Some(root) = self.root_of.remove(id) {
                        if let Some(tree) = self.trees.get_mut(&root) {
                            tree.remove(id);
                            if tree.is_empty() {
                                self.trees.remove(&root);
                            }
                        }
                        touched.insert(root);
                    }
                    if let Some(row) = sessions.get(id).filter(|row| row.team_id.as_deref() == Some(team_id)) {
                        let root = row.root_id_or_self().to_string();
                        self.root_of.insert(id.clone(), root.clone());
                        self.trees.entry(root.clone()).or_default().insert(id.clone());
                        touched.insert(root);
                    }
                }
                let mut sessions_touched: HashSet<String> = HashSet::new();
                // A link's session, before and after the write.
                for id in &written_links {
                    if let Some((session, _)) = self.links.remove(id) {
                        sessions_touched.insert(session);
                    }
                    if let Some(link) = links.get(id) {
                        self.links.insert(id.clone(), (link.session_id.clone(), link.issue_id.clone()));
                        sessions_touched.insert(link.session_id.clone());
                    }
                }
                // The sessions linking a written issue.
                if !written_issues.is_empty() {
                    let written: HashSet<&str> = written_issues.iter().map(String::as_str).collect();
                    for (session, issue) in self.links.values() {
                        if written.contains(issue.as_str()) {
                            sessions_touched.insert(session.clone());
                        }
                    }
                }
                // A read mark's session, before and after the write.
                for id in &written_reads {
                    if let Some(session) = self.reads.remove(id) {
                        sessions_touched.insert(session);
                    }
                    if let Some(read) = reads.get(id) {
                        self.reads.insert(id.clone(), read.session_id.clone());
                        sessions_touched.insert(read.session_id.clone());
                    }
                }
                touched.extend(sessions_touched.iter().filter_map(|session| self.root_of.get(session).cloned()));
                self.derived_trees = touched.len();
                for root in touched {
                    self.rows.remove(&root);
                    let Some(tree) = self.trees.get(&root) else { continue };
                    let mut members: Vec<&SessionRow> = tree.iter().filter_map(|id| sessions.get(id)).collect();
                    members.sort_by(|a, b| a.id.cmp(&b.id));
                    for (key, row) in tree_rows(team_id, &members, cx) {
                        let at = domain::session_tree::chat_row_activity_ms(&row);
                        self.rows.insert(row.session_id.clone(), (key, row, at));
                    }
                }
            }
            None => {
                // The first derive, another store or team, or a log that no
                // longer reaches back: everything.
                let _span = tracing::trace_span!("chat_rows.whole").entered();
                self.whole = true;
                let rows: Vec<&SessionRow> = sessions.iter().filter(|session| session.team_id.as_deref() == Some(team_id)).collect();
                self.root_of.clear();
                self.trees.clear();
                for row in &rows {
                    let root = row.root_id_or_self().to_string();
                    self.root_of.insert(row.id.clone(), root.clone());
                    self.trees.entry(root).or_default().insert(row.id.clone());
                }
                self.links = links.iter().map(|link| (link.id.clone(), (link.session_id.clone(), link.issue_id.clone()))).collect();
                self.reads = reads.iter().map(|read| (read.id.clone(), read.session_id.clone())).collect();
                self.derived_trees = self.trees.len();
                self.rows = rows_of(team_id, &rows, cx)
                    .into_iter()
                    .map(|(key, row)| {
                        let at = domain::session_tree::chat_row_activity_ms(&row);
                        (row.session_id.clone(), (key, row, at))
                    })
                    .collect();
            }
        }
        self.team = team_id.to_string();
        self.store = Some(c.sessions.entity_id());
        self.revisions = revisions;
        domain::session_tree::group_chat_rows_at(self.rows.values().map(|(key, row, at)| (*key, row, *at)), Some(now_ms))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn seed<T: sync::ShapeRow + serde::de::DeserializeOwned>(collection: &gpui::Entity<sync::Collection<T>>, rows: serde_json::Value, cx: &mut App) {
        collection.update(cx, |c, cx| {
            for row in rows.as_array().unwrap() {
                let row: T = serde_json::from_value(row.clone()).unwrap();
                c.seed(row.key(), row);
            }
            cx.notify();
        });
    }

    fn remove(id: &str, cx: &mut App) {
        let sessions = Store::global(cx).collections().sessions.clone();
        sessions.update(cx, |c, cx| {
            c.remove_for_test(&RowKey::Single(id.into()));
            cx.notify();
        });
    }

    fn session(id: &str, root: Option<&str>, team: &str, updated: &str, extra: serde_json::Value) -> serde_json::Value {
        let mut row = json!({
            "id": id, "team_id": team, "root_id": root.unwrap_or(id), "parent_id": root, "status": "idle", "shared": true,
            "created_at": "2026-11-01T09:00:00.000Z", "updated_at": updated,
        });
        for (key, value) in extra.as_object().unwrap() {
            row[key] = value.clone();
        }
        row
    }

    fn remove_from<T: sync::ShapeRow>(collection: &gpui::Entity<sync::Collection<T>>, id: &str, cx: &mut App) {
        collection.update(cx, |c, cx| {
            c.remove_for_test(&RowKey::Single(id.into()));
            cx.notify();
        });
    }

    /// #PERF-3 / #PERF-10: the incremental bands equal a whole derive after
    /// every kind of write the rows read, and each write derives only the
    /// trees it touched.
    #[gpui::test]
    fn incremental_rows_equal_a_full_derive(cx: &mut gpui::TestAppContext) {
        const NOW: i64 = 1_794_528_000_000; // 2026-11-12
        cx.update(|cx| {
            let store = Store::open(cx, None, None);
            cx.set_global(store);
            let c = Store::global(cx).collections().clone();
            let mut rows = Vec::new();
            for n in 0..40 {
                let root = format!("r{n:02}");
                // Every fourth tree: its first prompt and an agent reply (a
                // title off the digest, an unread reply).
                let digest = if n % 4 == 0 {
                    json!({"digest": {"first": {"id": format!("m{n}"), "authorKind": "user", "body": format!("Prompt {n}"), "createdAt": "2026-11-10T09:00:00.000Z"}, "agentAt": "2026-11-11T09:00:00.000Z", "agentTimes": ["2026-11-11T09:00:00.000Z"]}})
                } else {
                    json!({})
                };
                rows.push(session(&root, None, "t1", &format!("2026-11-{:02}T10:00:00.000Z", 1 + n % 11), digest));
                if n % 3 == 0 {
                    rows.push(session(&format!("c{n:02}"), Some(&root), "t1", "2026-11-05T10:00:00.000Z", json!({"busy": n % 2 == 0})));
                }
            }
            rows.push(session("x1", None, "t2", "2026-11-11T10:00:00.000Z", json!({})));
            seed(&c.sessions, serde_json::Value::Array(rows), cx);
            seed(&c.boards, json!([{"id": "b1", "team_id": "t1", "name": "App", "slug": "app", "prefix": "APP"}]), cx);
            seed(&c.issues, json!([
                {"id": "i1", "board_id": "b1", "number": 1, "identifier": "APP-1", "title": "Crash on launch", "status": "in_progress"},
                {"id": "i2", "board_id": "b1", "number": 2, "identifier": "APP-2", "title": "Dark mode", "status": "backlog"}
            ]), cx);
            seed(&c.session_issues, json!([
                {"id": "l1", "session_id": "r01", "issue_id": "i1", "created_at": "2026-11-01T09:00:00.000Z"},
                {"id": "l2", "session_id": "r02", "issue_id": "i2", "created_at": "2026-11-01T09:00:00.000Z"},
                {"id": "l3", "session_id": "c03", "issue_id": "i1", "created_at": "2026-11-01T09:00:00.000Z"}
            ]), cx);
            seed(&c.session_reads, json!([{"id": "rd1", "session_id": "r04", "user_id": "u1", "read_at": "2026-11-12T00:00:00.000Z"}]), cx);
        });
        let mut rows = ChatRows::default();
        let check = |rows: &mut ChatRows, cx: &mut gpui::TestAppContext, what: &str| -> (usize, bool) {
            cx.update(|cx| {
                let incremental = rows.derive("t1", NOW, cx);
                assert_eq!(incremental, full_rows("t1", NOW, cx), "after {what}");
                (rows.derived_trees, rows.whole)
            })
        };
        assert!(check(&mut rows, cx, "the first derive").1);
        let write = |cx: &mut gpui::TestAppContext, value: serde_json::Value| {
            cx.update(|cx| {
                let c = Store::global(cx).collections().clone();
                seed(&c.sessions, json!([value]), cx);
            })
        };
        let with = |cx: &mut gpui::TestAppContext, f: &dyn Fn(&sync::Collections, &mut App)| {
            cx.update(|cx| {
                let c = Store::global(cx).collections().clone();
                f(&c, cx);
            })
        };
        // sessions: a child starts waiting on a person (Recent → Needs you).
        write(cx, session("c03", Some("r03"), "t1", "2026-11-12T08:00:00.000Z", json!({"needs_input": true})));
        assert_eq!(check(&mut rows, cx, "a child needing input"), (1, false));
        // A reorder: an old root's activity jumps to the top of Recent.
        write(cx, session("r39", None, "t1", "2026-11-12T09:00:00.000Z", json!({})));
        assert_eq!(check(&mut rows, cx, "a reorder"), (1, false));
        // A root starts working (Recent → Running).
        write(cx, session("r05", None, "t1", "2026-11-12T09:30:00.000Z", json!({"busy": true})));
        assert_eq!(check(&mut rows, cx, "a root working"), (1, false));
        // Inserts: a new tree, and a new child of an existing one.
        write(cx, session("r40", None, "t1", "2026-11-12T10:00:00.000Z", json!({})));
        write(cx, session("c40", Some("r06"), "t1", "2026-11-12T10:01:00.000Z", json!({"status": "queued"})));
        assert_eq!(check(&mut rows, cx, "inserts"), (2, false));
        // A session moving between trees (its root changes).
        write(cx, session("c09", Some("r10"), "t1", "2026-11-12T10:02:00.000Z", json!({"needs_input": true})));
        assert_eq!(check(&mut rows, cx, "a move between trees"), (2, false));
        // A session leaving the team, and one joining it.
        write(cx, session("c12", Some("r12"), "t2", "2026-11-12T10:03:00.000Z", json!({})));
        write(cx, session("x1", None, "t1", "2026-11-12T10:04:00.000Z", json!({})));
        assert!(!check(&mut rows, cx, "team moves").1);
        // Deletes: a child, then a whole root.
        cx.update(|cx| remove("c15", cx));
        assert_eq!(check(&mut rows, cx, "a child deleted"), (1, false));
        cx.update(|cx| remove("r20", cx));
        assert_eq!(check(&mut rows, cx, "a root deleted"), (1, false));
        // #PERF-10 links: one added to a root, one removed from a child.
        with(cx, &|c, cx| seed(&c.session_issues, json!([{"id": "l4", "session_id": "r07", "issue_id": "i2", "created_at": "2026-11-12T09:00:00.000Z"}]), cx));
        assert_eq!(check(&mut rows, cx, "a link added"), (1, false));
        with(cx, &|c, cx| remove_from(&c.session_issues, "l3", cx));
        assert_eq!(check(&mut rows, cx, "a link removed"), (1, false));
        // An issue renamed (r01's title), then its status moving.
        with(cx, &|c, cx| seed(&c.issues, json!([{"id": "i1", "board_id": "b1", "number": 1, "identifier": "APP-1", "title": "Crash on cold launch", "status": "in_progress"}]), cx));
        assert_eq!(check(&mut rows, cx, "an issue renamed"), (1, false));
        with(cx, &|c, cx| seed(&c.issues, json!([{"id": "i2", "board_id": "b1", "number": 2, "identifier": "APP-2", "title": "Dark mode", "status": "done"}]), cx));
        assert_eq!(check(&mut rows, cx, "an issue's status"), (2, false));
        // Read marks: one set on an unread tree, one moved past its reply.
        with(cx, &|c, cx| seed(&c.session_reads, json!([{"id": "rd2", "session_id": "r08", "user_id": "u1", "read_at": "2026-11-12T00:00:00.000Z"}]), cx));
        assert_eq!(check(&mut rows, cx, "a read mark set"), (1, false));
        with(cx, &|c, cx| seed(&c.session_reads, json!([{"id": "rd1", "session_id": "r04", "user_id": "u1", "read_at": "2026-11-01T00:00:00.000Z"}]), cx));
        assert_eq!(check(&mut rows, cx, "a read mark moved back"), (1, false));
        with(cx, &|c, cx| remove_from(&c.session_reads, "rd2", cx));
        assert_eq!(check(&mut rows, cx, "a read mark removed"), (1, false));
        // Another team asks: a whole derive of that team.
        cx.update(|cx| assert_eq!(rows.derive("t2", NOW, cx), full_rows("t2", NOW, cx)));
        assert!(rows.whole);
    }
}

#[cfg(test)]
mod agent_unread_tests {
    use super::*;
    use serde_json::json;

    fn seed<T: sync::ShapeRow + serde::de::DeserializeOwned>(collection: &gpui::Entity<sync::Collection<T>>, rows: serde_json::Value, cx: &mut App) {
        collection.update(cx, |c, cx| {
            for row in rows.as_array().unwrap() {
                let row: T = serde_json::from_value(row.clone()).unwrap();
                c.seed(row.key(), row);
            }
            cx.notify();
        });
    }

    /// #PERF-10: the rail's agent counts equal the Chats model's agent
    /// rows (the model it no longer derives per write), as read marks move.
    #[gpui::test]
    fn agent_unread_counts_equal_the_chats_model(cx: &mut gpui::TestAppContext) {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("accounts.json"),
            json!({"accounts": [{"id": "acc", "instanceUrl": "https://example.test", "userId": "u1", "email": "me@example.test", "name": "Me"}]}).to_string(),
        )
        .unwrap();
        let db = dir.path().join("local.sqlite");
        let (_changes, rx) = flume::unbounded::<(String, Vec<String>)>();
        let data = dir.path().to_path_buf();
        cx.update(|cx| {
            cx.set_global(crate::AuthContext { auth: api::AuthStore::load(data.clone()), client: std::sync::Arc::new(api::AuthClient::new()), data_dir: data });
            let store = Store::open(cx, None, None);
            cx.set_global(store);
            Store::global(cx).clone().connect_local("acc", &db, rx, cx).unwrap();
        });
        cx.run_until_parked();
        assert_eq!(cx.update(|cx| crate::queries::active_account(cx).map(|a| a.user_id)).as_deref(), Some("u1"));
        let replies = |times: &[&str]| json!({"first": {"id": "p", "authorKind": "user", "body": "Hi", "createdAt": "2026-11-10T09:00:00.000Z"}, "agentAt": times.last(), "agentTimes": times});
        cx.update(|cx| {
            let c = Store::global(cx).collections().clone();
            seed(&c.teams, json!([{"id": "t1", "name": "One", "slug": "one"}]), cx);
            seed(&c.agents, json!([{"id": "a1", "team_id": "t1", "name": "Reviewer"}, {"id": "a2", "team_id": "t1", "name": "Planner"}, {"id": "a3", "team_id": "t1", "name": "Quiet"}]), cx);
            seed(&c.sessions, json!([
                {"id": "v1", "team_id": "t1", "root_id": "v1", "agent_id": "a1", "user_id": "u1", "status": "idle", "created_at": "2026-11-10T09:00:00.000Z",
                 "digest": replies(&["2026-11-11T09:00:00.000Z", "2026-11-11T10:00:00.000Z", "2026-11-11T11:00:00.000Z"])},
                {"id": "v2", "team_id": "t1", "root_id": "v2", "agent_id": "a2", "user_id": "u1", "status": "idle", "created_at": "2026-11-10T09:00:00.000Z",
                 "digest": replies(&["2026-11-11T09:00:00.000Z"])},
                {"id": "w1", "team_id": "t1", "root_id": "w1", "agent_id": "a1", "user_id": "u2", "status": "idle", "created_at": "2026-11-10T09:00:00.000Z",
                 "digest": replies(&["2026-11-12T09:00:00.000Z"])}
            ]), cx);
            seed(&c.session_reads, json!([{"id": "rd1", "session_id": "v1", "user_id": "u1", "read_at": "2026-11-11T09:30:00.000Z"}]), cx);
        });
        let both = |cx: &mut gpui::TestAppContext| {
            cx.update(|cx| {
                let counts = crate::session_data::agent_unread_counts("t1", cx);
                let model: HashMap<String, usize> =
                    crate::session_data::chats_model("t1", cx).agent_rows.iter().map(|row| (row.agent_id.clone(), row.unread_count)).collect();
                (counts, model)
            })
        };
        let (counts, model) = both(cx);
        assert_eq!(counts, model);
        assert!(counts.values().any(|n| *n > 0), "{counts:?}");
        cx.update(|cx| {
            let c = Store::global(cx).collections().clone();
            seed(&c.session_reads, json!([{"id": "rd1", "session_id": "v1", "user_id": "u1", "read_at": "2026-11-12T00:00:00.000Z"}, {"id": "rd2", "session_id": "v2", "user_id": "u1", "read_at": "2026-11-01T00:00:00.000Z"}]), cx);
        });
        let (counts, model) = both(cx);
        assert_eq!(counts, model, "after the marks moved");
        drop(dir);
    }
}
