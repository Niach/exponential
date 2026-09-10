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
