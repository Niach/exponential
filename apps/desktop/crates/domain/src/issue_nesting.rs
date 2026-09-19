//! EXP-980 — sub-issues nest under their parent in every issue list. The ROOT
//! issue decides the group and the sort position; its sub-issues follow it,
//! whatever status they are in themselves.
//!
//! ONE pure rule over ids, mirrored ×4 (web `apps/web/src/lib/issue-nesting.ts`,
//! iOS `IssueNesting.swift`, Android `IssueNesting.kt`) and locked by the
//! contract fixture `packages/domain-contract/fixtures/issue-nesting.json` —
//! same cases, same test names. The elbow connectors are the shared
//! [`crate::tree_guides`].
//!
//! 1. A canonical `parent` row is `issue_id` = the PARENT, `related_issue_id` =
//!    the CHILD (EXP-736). Only rows whose BOTH ends are in the list count: a
//!    parent on another board, unsynced or filtered out leaves the child a root.
//! 2. A child with several listed parents nests under the one with the lowest
//!    identifier (plain string order).
//! 3. An issue on a parent CYCLE keeps no parent (walking up from it returns to
//!    it), so a cycle can never swallow rows; whatever hangs off it still nests.
//! 4. Siblings keep the LIST's order: group order first, then the position
//!    inside their own group — the caller's comparator already ran.

use std::collections::HashMap;

/// The synced `issue_relations` columns this rule reads, borrowed off whatever
/// row the caller holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NestingRelation<'a> {
    pub kind: &'a str,
    pub issue_id: &'a str,
    pub related_issue_id: &'a str,
}

/// One row of a nested list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NestedIssueRow {
    pub id: String,
    /// 0 = a root row, 1 = its sub-issue, …
    pub depth: usize,
}

/// `groups` = the list's groups in order, each the ids in display order.
/// Returns the same number of groups; one a nesting emptied comes back empty
/// (the caller hides it).
pub fn nest_issue_rows<G, F>(
    groups: &[G],
    relations: &[NestingRelation<'_>],
    identifier_of: F,
) -> Vec<Vec<NestedIssueRow>>
where
    G: AsRef<[String]>,
    F: Fn(&str) -> String,
{
    let mut position: HashMap<&str, usize> = HashMap::new();
    for group in groups {
        for id in group.as_ref() {
            let next = position.len();
            position.entry(id.as_str()).or_insert(next);
        }
    }

    let mut parent_of: HashMap<&str, &str> = HashMap::new();
    for relation in relations {
        if relation.kind != "parent" {
            continue;
        }
        let (parent, child) = (relation.issue_id, relation.related_issue_id);
        if parent == child {
            continue;
        }
        if !position.contains_key(parent) || !position.contains_key(child) {
            continue;
        }
        let better = match parent_of.get(child) {
            Some(current) => identifier_of(parent) < identifier_of(current),
            None => true,
        };
        if better {
            parent_of.insert(child, parent);
        }
    }

    // Rule 3: drop the parent of every issue that is its own ancestor. Judged
    // against the untouched map first, so EVERY member of a cycle becomes a
    // root, whatever order the rows arrived in.
    let on_cycle: Vec<&str> = parent_of
        .keys()
        .copied()
        .filter(|&start| {
            let mut seen: Vec<&str> = vec![start];
            let mut cursor = parent_of.get(start).copied();
            while let Some(id) = cursor {
                if seen.contains(&id) {
                    break;
                }
                seen.push(id);
                cursor = parent_of.get(id).copied();
            }
            cursor == Some(start)
        })
        .collect();
    for id in on_cycle {
        parent_of.remove(id);
    }

    let mut children_of: HashMap<&str, Vec<&str>> = HashMap::new();
    for (child, parent) in &parent_of {
        children_of.entry(*parent).or_default().push(*child);
    }
    for siblings in children_of.values_mut() {
        siblings.sort_by_key(|id| position.get(*id).copied().unwrap_or(0));
    }

    fn emit<'a>(
        id: &'a str,
        depth: usize,
        children_of: &HashMap<&'a str, Vec<&'a str>>,
        emitted: &mut Vec<&'a str>,
        out: &mut Vec<NestedIssueRow>,
    ) {
        if emitted.contains(&id) {
            return;
        }
        emitted.push(id);
        out.push(NestedIssueRow { id: id.to_string(), depth });
        let children: Vec<&'a str> = children_of
            .get(id)
            .map(|siblings| siblings.to_vec())
            .unwrap_or_default();
        for child in children {
            emit(child, depth + 1, children_of, emitted, out);
        }
    }

    let mut emitted: Vec<&str> = Vec::new();
    groups
        .iter()
        .map(|group| {
            let mut out: Vec<NestedIssueRow> = Vec::new();
            for id in group.as_ref() {
                if !parent_of.contains_key(id.as_str()) {
                    emit(id.as_str(), 0, &children_of, &mut emitted, &mut out);
                }
            }
            out
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;

    /// The ONE contract fixture, replayed byte-for-byte on every client (web
    /// `issue-nesting.test.ts`, iOS `IssueNestingTests`, Android
    /// `IssueNestingTest`) — the SAME case names everywhere.
    const FIXTURE: &str =
        include_str!("../../../../../packages/domain-contract/fixtures/issue-nesting.json");

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct FixtureRelation {
        #[serde(rename = "type")]
        kind: String,
        issue_id: String,
        related_issue_id: String,
    }

    #[derive(Deserialize)]
    struct FixtureRow {
        id: String,
        depth: usize,
    }

    #[derive(Deserialize)]
    struct FixtureCase {
        name: String,
        groups: Vec<Vec<String>>,
        identifiers: HashMap<String, String>,
        relations: Vec<FixtureRelation>,
        expected: Vec<Vec<FixtureRow>>,
    }

    /// Every fixture case, replayed with its own name in the failure message
    /// (a Rust test name cannot be a sentence — the fixture's name IS the case
    /// identity across the four clients).
    #[test]
    fn issue_nesting_contract_fixture() {
        let cases: Vec<FixtureCase> =
            serde_json::from_str(FIXTURE).expect("the issue-nesting fixture parses");
        assert!(!cases.is_empty());
        for case in &cases {
            let relations: Vec<NestingRelation<'_>> = case
                .relations
                .iter()
                .map(|relation| NestingRelation {
                    kind: &relation.kind,
                    issue_id: &relation.issue_id,
                    related_issue_id: &relation.related_issue_id,
                })
                .collect();
            let nested = nest_issue_rows(&case.groups, &relations, |id| {
                case.identifiers.get(id).cloned().unwrap_or_else(|| id.to_string())
            });
            let shape: Vec<Vec<(&str, usize)>> = nested
                .iter()
                .map(|group| group.iter().map(|row| (row.id.as_str(), row.depth)).collect())
                .collect();
            let expected: Vec<Vec<(&str, usize)>> = case
                .expected
                .iter()
                .map(|group| group.iter().map(|row| (row.id.as_str(), row.depth)).collect())
                .collect();
            assert_eq!(shape, expected, "case: {}", case.name);
        }
    }
}
