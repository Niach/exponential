//! EXP-818: the session TREE — a run started by another run through
//! `exponential_sessions_start` carries `parent_session_id`, and every
//! session list (the rail, the Agent page, Devices on the phones) nests it
//! under its parent instead of listing it as a stranger.
//!
//! ONE pure rule, mirrored ×4 (web `lib/session-tree.ts`, iOS
//! `SessionTree.swift`, Android `SessionTree.kt`) with the same four tests:
//!
//! 1. The caller's order is the ROOT order — the tree never re-sorts roots.
//! 2. A row is a child iff its parent names ANOTHER row of the input; a
//!    parent that is not listed leaves the child a root at depth 0.
//! 3. Children follow their parent directly, oldest start first (then id),
//!    recursively — depth grows by one per level.
//! 4. A cycle (defensive) breaks at the first repeat.
//!
//! EXP-996/EXP-1049 layer the TREE proper on top ([`session_tree`], the web
//! `lib/sessions/session-tree.ts` twin, same rules and same test names):
//! resume successions COLLAPSE into one node, children nest under their
//! parent's succession, and [`visible_session_tree_rows`] is the flattening
//! every client paints its connector over. [`session_needs_you`] is the red
//! needs-you mark, byte-locked ×4 by `fixtures/session-tree-marks.json`.

use std::collections::{HashMap, HashSet};

/// One row of the flattened tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TreeRow<T> {
    pub session: T,
    /// 0 for a root, +1 per nesting level.
    pub depth: usize,
    /// Whether at least one child is nested right below.
    pub has_children: bool,
}

/// Flatten `sessions` into nested order. `id`/`parent`/`started_at` read the
/// three columns off whatever row type the caller holds (ISO-8601 UTC
/// strings compare lexicographically in time order).
pub fn nest_sessions<T, I, P, S>(sessions: Vec<T>, id: I, parent: P, started_at: S) -> Vec<TreeRow<T>>
where
    I: Fn(&T) -> &str,
    P: Fn(&T) -> Option<&str>,
    S: Fn(&T) -> Option<&str>,
{
    let ids: HashSet<String> = sessions.iter().map(|s| id(s).to_string()).collect();
    let is_child = |s: &T| -> bool {
        parent(s).is_some_and(|p| p != id(s) && ids.contains(p))
    };
    let mut children_of: HashMap<String, Vec<usize>> = HashMap::new();
    for (index, session) in sessions.iter().enumerate() {
        if is_child(session) {
            let key = parent(session).unwrap_or_default().to_string();
            children_of.entry(key).or_default().push(index);
        }
    }
    for list in children_of.values_mut() {
        list.sort_by(|a, b| {
            let (sa, sb) = (&sessions[*a], &sessions[*b]);
            started_at(sa)
                .unwrap_or_default()
                .cmp(started_at(sb).unwrap_or_default())
                .then_with(|| id(sa).cmp(id(sb)))
        });
    }
    let mut order: Vec<(usize, usize, bool)> = Vec::with_capacity(sessions.len());
    let mut placed = vec![false; sessions.len()];
    fn visit(
        index: usize,
        depth: usize,
        sessions_ids: &[String],
        children_of: &HashMap<String, Vec<usize>>,
        placed: &mut [bool],
        order: &mut Vec<(usize, usize, bool)>,
    ) {
        if placed[index] {
            return;
        }
        placed[index] = true;
        let children: Vec<usize> = children_of
            .get(&sessions_ids[index])
            .map(|c| c.iter().copied().filter(|child| !placed[*child]).collect())
            .unwrap_or_default();
        order.push((index, depth, !children.is_empty()));
        for child in children {
            visit(child, depth + 1, sessions_ids, children_of, placed, order);
        }
    }
    let session_ids: Vec<String> = sessions.iter().map(|s| id(s).to_string()).collect();
    for (index, session) in sessions.iter().enumerate() {
        if !is_child(session) {
            visit(index, 0, &session_ids, &children_of, &mut placed, &mut order);
        }
    }
    // A child whose ancestry cycled without a root — keep it, at depth 0.
    for index in 0..sessions.len() {
        visit(index, 0, &session_ids, &children_of, &mut placed, &mut order);
    }
    let mut slots: Vec<Option<T>> = sessions.into_iter().map(Some).collect();
    order
        .into_iter()
        .map(|(index, depth, has_children)| TreeRow {
            session: slots[index].take().expect("each index placed once"),
            depth,
            has_children,
        })
        .collect()
}

/// The ids of every row nested (at any depth) under `id`.
pub fn descendant_ids<T, I>(rows: &[TreeRow<T>], id: &str, row_id: I) -> Vec<String>
where
    I: Fn(&T) -> &str,
{
    let Some(start) = rows.iter().position(|row| row_id(&row.session) == id) else {
        return Vec::new();
    };
    let depth = rows[start].depth;
    rows[start + 1..]
        .iter()
        .take_while(|row| row.depth > depth)
        .map(|row| row_id(&row.session).to_string())
        .collect()
}

// ---------------------------------------------------------------------------
// EXP-996 — the session TREE proper (web `lib/sessions/session-tree.ts`)
// ---------------------------------------------------------------------------

/// EXP-996 — the row fields the tree reads, lifted off whatever row type the
/// caller holds.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SessionFacts {
    pub id: String,
    pub resumed_from_id: Option<String>,
    pub parent_session_id: Option<String>,
    /// ISO-8601 UTC, compared lexicographically (this file's convention);
    /// `""` = unknown, which sorts OLDEST — the web's `stamp` of 0.
    pub created_at: String,
    pub updated_at: String,
}

/// The facts of a synced `coding_sessions` row — every desktop caller's
/// mapper, so no two surfaces can read the columns differently.
pub fn coding_session_facts(session: &crate::rows::CodingSession) -> SessionFacts {
    SessionFacts {
        id: session.id.clone(),
        resumed_from_id: session.resumed_from_id.clone(),
        parent_session_id: session.parent_session_id.clone(),
        created_at: session.created_at.clone().unwrap_or_default(),
        updated_at: session.updated_at.clone().unwrap_or_default(),
    }
}

/// ONE run — every row of its resume succession (EXP-974) folded together,
/// with the runs it started nested below.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionNode<T> {
    /// The NEWEST row of the succession: the node's identity and its key.
    pub id: String,
    /// The succession oldest-first (`run_chain`'s order), the newest last; one
    /// entry when the run was never resumed.
    pub chain: Vec<T>,
    pub children: Vec<SessionNode<T>>,
    /// The newest `updated_at` across the chain AND the whole subtree, so
    /// folding a parent never moves it.
    pub last_activity_at: String,
}

impl<T> SessionNode<T> {
    /// The row the node DRAWS — the newest of its resume succession.
    pub fn session(&self) -> &T {
        self.chain.last().expect("a chain always holds its own row")
    }
}

/// A node's stable identity — the key a collapsed set and a list row use
/// (web `sessionTreeNodeKey`): the id of its newest row.
pub fn session_tree_node_key<T>(node: &SessionNode<T>) -> String {
    node.id.clone()
}

/// A row is LIVE until the server ends it (`running` and `in_review` both
/// are; `needs_input`/`blocked` are flags on a live row).
pub fn session_row_is_live(status: &str) -> bool {
    status != "ended"
}

/// EXP-996 — the sessions list as a TREE, the web `sessionTree`'s twin: the
/// ONE selector every session list draws from (the rail's Running section, the
/// IDE's Recent list, the phones' session screens). Rules, in this order:
///
/// 1. Resume successions COLLAPSE into one node keyed by their newest row.
/// 2. Children nest under their `parent_session_id`, following the parent's
///    whole succession (EXP-906).
/// 3. Top-level nodes sort by last activity, newest first; children keep
///    creation order.
/// 4. An orphan child (parent swept, another team, not synced) sits at top
///    level; so does a row naming itself, and a cycle breaks where it closes.
///
/// Pure: no clock, no IO; every tie breaks on the node key, so two clients
/// agree row for row.
pub fn session_tree<T, F>(sessions: Vec<T>, facts: F) -> Vec<SessionNode<T>>
where
    F: Fn(&T) -> SessionFacts,
{
    let facts: Vec<SessionFacts> = sessions.iter().map(&facts).collect();
    let index_of: HashMap<&str, usize> = facts
        .iter()
        .enumerate()
        .map(|(index, row)| (row.id.as_str(), index))
        .collect();

    // 1. Resume successions collapse. OLDEST row first, so the primary
    //    succession (the newest-successor walk) claims its members before an
    //    older fork sibling does; whatever is left becomes its own node.
    let mut ordered: Vec<usize> = (0..facts.len()).collect();
    ordered.sort_by(|a, b| {
        facts[*a]
            .created_at
            .cmp(&facts[*b].created_at)
            .then_with(|| facts[*a].id.cmp(&facts[*b].id))
    });
    let mut canonical_of: HashMap<usize, usize> = HashMap::new();
    // (canonical, chain oldest-first), in discovery order.
    let mut chains: Vec<(usize, Vec<usize>)> = Vec::new();
    for &start in &ordered {
        if canonical_of.contains_key(&start) {
            continue;
        }
        let mut chain: Vec<usize> = resume_chain(&facts, &index_of, start)
            .into_iter()
            .filter(|member| !canonical_of.contains_key(member))
            .collect();
        if chain.is_empty() {
            chain.push(start);
        }
        let canonical = *chain.last().expect("non-empty");
        for &member in &chain {
            canonical_of.insert(member, canonical);
        }
        chains.push((canonical, chain));
    }

    // 2. Children nest under their parent's SUCCESSION — the whole chain
    //    answers for the newest `parent_session_id` it names.
    let mut parent_of: HashMap<usize, usize> = HashMap::new();
    for (canonical, chain) in &chains {
        let named = chain
            .iter()
            .rev()
            .find_map(|member| facts[*member].parent_session_id.as_deref());
        let parent = named
            .and_then(|id| index_of.get(id))
            .and_then(|index| canonical_of.get(index))
            .copied();
        // Rule 4: a parent that is gone leaves the child at top level, and so
        // does a row naming its own succession.
        let Some(parent) = parent.filter(|parent| parent != canonical) else {
            continue;
        };
        parent_of.insert(*canonical, parent);
    }

    let mut children_of: HashMap<usize, Vec<usize>> = HashMap::new();
    let mut roots: Vec<usize> = Vec::new();
    for (canonical, _) in &chains {
        match ancestor(*canonical, &parent_of) {
            Some(parent) => children_of.entry(parent).or_default().push(*canonical),
            None => roots.push(*canonical),
        }
    }
    // Rule 3: children keep CREATION order, ties on their id.
    for list in children_of.values_mut() {
        list.sort_by(|a, b| {
            facts[*a]
                .created_at
                .cmp(&facts[*b].created_at)
                .then_with(|| facts[*a].id.cmp(&facts[*b].id))
        });
    }

    let chain_of: HashMap<usize, Vec<usize>> = chains.into_iter().collect();
    let mut slots: Vec<Option<T>> = sessions.into_iter().map(Some).collect();
    let mut out: Vec<SessionNode<T>> = roots
        .iter()
        .map(|canonical| build_node(*canonical, &chain_of, &children_of, &facts, &mut slots))
        .collect();

    // 3. Top-level nodes sort by last activity, newest FIRST.
    out.sort_by(|a, b| {
        b.last_activity_at
            .cmp(&a.last_activity_at)
            .then_with(|| a.id.cmp(&b.id))
    });
    out
}

/// The resume succession `start` belongs to, oldest first — `queries::run_chain`
/// / the web `runChain`, over indices: BACKWARDS through `resumed_from_id` to
/// the first row, FORWARDS to the newest successor at every fork (`created_at`
/// then id, descending). Cycle-safe both ways. It lives here rather than in
/// the ui crate's copy because the tree must collapse successions on every
/// client, not just the one with a query layer.
fn resume_chain(
    facts: &[SessionFacts],
    index_of: &HashMap<&str, usize>,
    start: usize,
) -> Vec<usize> {
    let mut seen: HashSet<usize> = HashSet::new();
    seen.insert(start);
    let mut before: Vec<usize> = Vec::new();
    let mut cursor = start;
    while let Some(previous_id) = facts[cursor].resumed_from_id.as_deref() {
        let Some(&previous) = index_of.get(previous_id) else {
            break;
        };
        if !seen.insert(previous) {
            break;
        }
        before.push(previous);
        cursor = previous;
    }
    before.reverse();

    let mut after: Vec<usize> = Vec::new();
    let mut cursor = start;
    loop {
        let next = (0..facts.len())
            .filter(|index| {
                facts[*index].resumed_from_id.as_deref() == Some(facts[cursor].id.as_str())
            })
            .filter(|index| !seen.contains(index))
            .max_by(|a, b| {
                facts[*a]
                    .created_at
                    .cmp(&facts[*b].created_at)
                    .then_with(|| facts[*a].id.cmp(&facts[*b].id))
            });
        let Some(next) = next else {
            break;
        };
        seen.insert(next);
        after.push(next);
        cursor = next;
    }

    before
        .into_iter()
        .chain(std::iter::once(start))
        .chain(after)
        .collect()
}

/// The top of `index`' parent walk, or `None` when it is already a root.
/// Breaks a cycle by returning `None`, so the row stays where it is.
fn ancestor(index: usize, parent_of: &HashMap<usize, usize>) -> Option<usize> {
    let parent = *parent_of.get(&index)?;
    let mut seen: HashSet<usize> = HashSet::new();
    seen.insert(index);
    let mut cursor = Some(parent);
    while let Some(current) = cursor {
        if !seen.insert(current) {
            return None;
        }
        cursor = parent_of.get(&current).copied();
    }
    Some(parent)
}

/// One session node and its subtree, rows moved out of `slots`. A node's
/// activity counts its whole subtree's.
fn build_node<T>(
    canonical: usize,
    chain_of: &HashMap<usize, Vec<usize>>,
    children_of: &HashMap<usize, Vec<usize>>,
    facts: &[SessionFacts],
    slots: &mut Vec<Option<T>>,
) -> SessionNode<T> {
    let chain = chain_of.get(&canonical).cloned().unwrap_or_default();
    let mut last_activity_at = chain
        .iter()
        .map(|member| facts[*member].updated_at.clone())
        .max()
        .unwrap_or_default();
    let children: Vec<SessionNode<T>> = children_of
        .get(&canonical)
        .map(|kids| {
            kids.iter()
                .map(|kid| build_node(*kid, chain_of, children_of, facts, slots))
                .collect()
        })
        .unwrap_or_default();
    for child in &children {
        if child.last_activity_at > last_activity_at {
            last_activity_at = child.last_activity_at.clone();
        }
    }
    SessionNode {
        id: facts[canonical].id.clone(),
        chain: chain
            .iter()
            .map(|member| slots[*member].take().expect("each row placed once"))
            .collect(),
        children,
        last_activity_at,
    }
}

/// One row of a DRAWN session tree: the node, how deep it sits and whether it
/// can fold — what [`crate::tree_guides::guides_for`] paints its connector
/// over.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionTreeFlatRow<'a, T> {
    pub node: &'a SessionNode<T>,
    pub key: String,
    pub depth: usize,
    pub has_children: bool,
}

/// Every node of the tree, depth-first (web `flattenSessionTree`, iOS
/// `flatten`, Android `flattenSessionTree`). What a caller wants when it
/// needs the runs a tree holds and not its structure — counting them, or
/// finding the one it should reveal.
pub fn flatten_session_tree<T>(nodes: &[SessionNode<T>]) -> Vec<&SessionNode<T>> {
    fn walk<'a, T>(nodes: &'a [SessionNode<T>], out: &mut Vec<&'a SessionNode<T>>) {
        for node in nodes {
            out.push(node);
            walk(&node.children, out);
        }
    }
    let mut out = Vec::new();
    walk(nodes, &mut out);
    out
}

/// The tree flattened top to bottom, skipping everything under a COLLAPSED
/// node (keyed by [`session_tree_node_key`]). Mirrored ×4 with
/// [`session_tree`] itself — every client flattens before it paints.
pub fn visible_session_tree_rows<'a, T>(
    nodes: &'a [SessionNode<T>],
    collapsed: &HashSet<String>,
) -> Vec<SessionTreeFlatRow<'a, T>> {
    fn walk<'a, T>(
        nodes: &'a [SessionNode<T>],
        depth: usize,
        collapsed: &HashSet<String>,
        out: &mut Vec<SessionTreeFlatRow<'a, T>>,
    ) {
        for node in nodes {
            let key = session_tree_node_key(node);
            let hidden = collapsed.contains(&key);
            out.push(SessionTreeFlatRow {
                node,
                key,
                depth,
                has_children: !node.children.is_empty(),
            });
            if !hidden {
                walk(&node.children, depth + 1, collapsed, out);
            }
        }
    }
    let mut out = Vec::new();
    walk(nodes, 0, collapsed, &mut out);
    out
}

// ---------------------------------------------------------------------------
// EXP-1108: a run row's needs-you MARK, byte-locked x4 by
// `packages/domain-contract/fixtures/session-tree-marks.json` (web
// `lib/sessions/session-tree.ts` `sessionNeedsYou`).

/// The red needs-you dot: a LIVE row with an open question. The amber
/// needs-input/blocked flags are a separate mark, never this one.
pub fn session_needs_you(status: &str, has_pending_question: bool) -> bool {
    session_row_is_live(status) && has_pending_question
}

#[cfg(test)]
mod marks_tests {
    use super::*;

    #[derive(serde::Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Fixture {
        needs_you: Vec<NeedsYouCase>,
    }

    #[derive(serde::Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct NeedsYouCase {
        name: String,
        status: String,
        pending_question: Option<serde_json::Value>,
        needs_you: bool,
    }

    fn fixture() -> Fixture {
        serde_json::from_str(include_str!(
            "../../../../../packages/domain-contract/fixtures/session-tree-marks.json"
        ))
        .expect("session-tree-marks.json parses")
    }

    #[test]
    fn needs_you_matches_the_fixture() {
        let fixture = fixture();
        assert!(!fixture.needs_you.is_empty());
        for case in fixture.needs_you {
            let has_question = case.pending_question.as_ref().is_some_and(|q| !q.is_null());
            assert_eq!(session_needs_you(&case.status, has_question), case.needs_you, "{}", case.name);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Clone, PartialEq, Eq)]
    struct Row {
        id: &'static str,
        parent: Option<&'static str>,
        started_at: &'static str,
    }

    fn row(id: &'static str, parent: Option<&'static str>) -> Row {
        Row { id, parent, started_at: "2026-09-10T10:00:00Z" }
    }

    fn row_at(id: &'static str, parent: Option<&'static str>, started_at: &'static str) -> Row {
        Row { id, parent, started_at }
    }

    fn nest(rows: Vec<Row>) -> Vec<TreeRow<Row>> {
        nest_sessions(rows, |r| r.id, |r| r.parent, |r| Some(r.started_at))
    }

    fn shape(rows: &[TreeRow<Row>]) -> Vec<String> {
        rows.iter()
            .map(|r| format!("{}@{}{}", r.session.id, r.depth, if r.has_children { "+" } else { "" }))
            .collect()
    }

    #[test]
    fn nest_sessions_keeps_the_callers_root_order() {
        let rows = nest(vec![row("b", None), row("a", None), row("c", None)]);
        assert_eq!(shape(&rows), ["b@0", "a@0", "c@0"]);
    }

    #[test]
    fn nest_sessions_nests_a_child_only_under_a_parent_that_is_listed() {
        let rows = nest(vec![row("p", None), row("c", Some("p")), row("orphan", Some("gone"))]);
        assert_eq!(shape(&rows), ["p@0+", "c@1", "orphan@0"]);
    }

    #[test]
    fn nest_sessions_lists_children_right_after_their_parent_oldest_first_recursively() {
        let rows = nest(vec![
            row_at("late", Some("p"), "2026-09-10T12:00:00Z"),
            row("p", None),
            row_at("grand", Some("early"), "2026-09-10T13:00:00Z"),
            row_at("early", Some("p"), "2026-09-10T11:00:00Z"),
            row("z", None),
        ]);
        assert_eq!(shape(&rows), ["p@0+", "early@1+", "grand@2", "late@1", "z@0"]);
        assert_eq!(descendant_ids(&rows, "p", |r| r.id), ["early", "grand", "late"]);
        assert_eq!(descendant_ids(&rows, "early", |r| r.id), ["grand"]);
        assert!(descendant_ids(&rows, "z", |r| r.id).is_empty());
    }

    #[test]
    fn nest_sessions_breaks_a_cycle_where_it_first_appears() {
        let rows = nest(vec![row("a", Some("b")), row("b", Some("a")), row("self", Some("self"))]);
        assert_eq!(shape(&rows), ["self@0", "a@0+", "b@1"]);
    }
}

/// EXP-996 — the web `session-tree.test.ts` table, case for case (the same
/// names, snake_cased), plus the cases only a second implementation can catch.
#[cfg(test)]
mod tree_tests {
    use super::*;

    /// The web test's `row()`: the columns the tree reads, nothing else.
    #[derive(Debug, Clone, PartialEq, Eq)]
    struct Run {
        id: &'static str,
        resumed_from_id: Option<&'static str>,
        parent_session_id: Option<&'static str>,
        /// `created_at` == `updated_at`, like the web fixture's `at()`.
        at: &'static str,
    }

    fn run(id: &'static str) -> Run {
        Run {
            id,
            resumed_from_id: None,
            parent_session_id: None,
            at: "2026-09-01T10:00:00Z",
        }
    }

    impl Run {
        fn at(mut self, at: &'static str) -> Self {
            self.at = at;
            self
        }
        fn resuming(mut self, id: &'static str) -> Self {
            self.resumed_from_id = Some(id);
            self
        }
        fn under(mut self, id: &'static str) -> Self {
            self.parent_session_id = Some(id);
            self
        }
    }

    fn facts(run: &Run) -> SessionFacts {
        SessionFacts {
            id: run.id.to_string(),
            resumed_from_id: run.resumed_from_id.map(str::to_string),
            parent_session_id: run.parent_session_id.map(str::to_string),
            created_at: run.at.to_string(),
            updated_at: run.at.to_string(),
        }
    }

    fn tree(runs: Vec<Run>) -> Vec<SessionNode<Run>> {
        session_tree(runs, facts)
    }

    /// The web test's `ids()`, as one line: keys in order, children in
    /// parentheses (`p(c1 c2)`).
    fn shape(nodes: &[SessionNode<Run>]) -> String {
        nodes
            .iter()
            .map(|node| {
                let children = shape(&node.children);
                let key = session_tree_node_key(node);
                if children.is_empty() {
                    key
                } else {
                    format!("{key}({children})")
                }
            })
            .collect::<Vec<_>>()
            .join(" ")
    }

    #[test]
    fn lists_unrelated_sessions_at_top_level_newest_activity_first() {
        let nodes = tree(
            vec![
                run("a").at("2026-09-01T10:00:00Z"),
                run("b").at("2026-09-01T11:00:00Z"),
            ],
        );
        assert_eq!(shape(&nodes), "b a");
    }

    #[test]
    fn nests_a_child_under_its_parent_session_id() {
        let nodes = tree(
            vec![run("c").under("p").at("2026-09-01T10:30:00Z"), run("p")],
        );
        assert_eq!(shape(&nodes), "p(c)");
    }

    #[test]
    fn collapses_a_resume_succession_into_one_node_keyed_by_its_newest_row() {
        let nodes = tree(
            vec![
                run("r1").at("2026-09-01T10:00:00Z"),
                run("r2").resuming("r1").at("2026-09-01T12:00:00Z"),
            ],
        );
        assert_eq!(shape(&nodes), "r2");
        let node = &nodes[0];
        assert_eq!(
            node.chain.iter().map(|run| run.id).collect::<Vec<_>>(),
            ["r1", "r2"]
        );
        assert_eq!(node.session().id, "r2");
    }

    #[test]
    fn follows_a_child_to_its_parents_resume_succession() {
        let nodes = tree(
            vec![
                run("p1"),
                run("p2").resuming("p1").at("2026-09-01T11:00:00Z"),
                run("c").under("p1"),
            ],
        );
        assert_eq!(shape(&nodes), "p2(c)");
    }

    #[test]
    fn puts_an_orphan_child_whose_parent_is_gone_at_top_level() {
        let nodes = tree(vec![run("o").under("gone")]);
        assert_eq!(shape(&nodes), "o");
    }

    #[test]
    fn keeps_children_in_creation_order_under_their_parent() {
        let nodes = tree(
            vec![
                run("c2").under("p").at("2026-09-01T10:20:00Z"),
                run("p"),
                run("c1").under("p").at("2026-09-01T10:10:00Z"),
            ],
        );
        assert_eq!(shape(&nodes), "p(c1 c2)");
    }

    /// A fork: the primary succession claims its members oldest-first, so the
    /// OLDER sibling is left over as a node of its own.
    #[test]
    fn an_older_fork_sibling_becomes_its_own_node() {
        let nodes = tree(
            vec![
                run("r1").at("2026-09-01T10:00:00Z"),
                run("r2").resuming("r1").at("2026-09-01T11:00:00Z"),
                run("r3").resuming("r1").at("2026-09-01T12:00:00Z"),
            ],
        );
        assert_eq!(shape(&nodes), "r3 r2");
    }

    /// A parent with one child and a lone run.
    fn sample() -> Vec<Run> {
        vec![
            run("p").at("2026-09-01T11:00:00Z"),
            run("c").under("p").at("2026-09-01T10:30:00Z"),
            run("n2").at("2026-09-01T10:00:00Z"),
        ]
    }

    fn flattened(collapsed: &[&str]) -> Vec<(String, usize, bool)> {
        let nodes = tree(sample());
        let collapsed: HashSet<String> = collapsed.iter().map(|key| key.to_string()).collect();
        visible_session_tree_rows(&nodes, &collapsed)
            .into_iter()
            .map(|row| (row.key, row.depth, row.has_children))
            .collect()
    }

    #[test]
    fn walks_children_depth_first() {
        let nodes = tree(sample());
        assert_eq!(
            flatten_session_tree(&nodes)
                .into_iter()
                .map(|node| node.id.clone())
                .collect::<Vec<_>>(),
            vec!["p".to_string(), "c".to_string(), "n2".to_string()]
        );
    }

    #[test]
    fn flattens_children_with_their_depths() {
        assert_eq!(
            flattened(&[])
                .into_iter()
                .map(|(key, depth, _)| (key, depth))
                .collect::<Vec<_>>(),
            vec![("p".to_string(), 0), ("c".to_string(), 1), ("n2".to_string(), 0)]
        );
    }

    #[test]
    fn hides_everything_under_a_collapsed_node() {
        let rows = flattened(&["p"]);
        assert_eq!(
            rows.iter().map(|(key, _, _)| key.as_str()).collect::<Vec<_>>(),
            ["p", "n2"]
        );
        assert!(rows.iter().find(|(key, _, _)| key == "p").unwrap().2);
    }
}
