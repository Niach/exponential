//! EXP-1097: what the issue detail DRAWS for its relations, on every client —
//! the "Sub-issue of" parent line above the title, the Linear-style
//! Sub-issues section (completion ring + `done/total`, rows, only a `+`), and
//! ONE foldable band per remaining relation side (Blocked by / Blocking /
//! Duplicate of / Duplicated by / Related). The byte-identical twin of web
//! `apps/web/src/lib/issue-relations-view.ts`, FIXTURE-LOCKED ×4
//! (`packages/domain-contract/fixtures/issue-relations-view.json`), copy
//! included.
//!
//! Storage stays canonical-direction ([`crate::relations`]): `parent` =
//! issueId is the parent of relatedIssueId, `blocks` = issueId blocks
//! relatedIssueId, `duplicate` = issueId duplicates relatedIssueId, `related`
//! = symmetric. A row whose other end is not synced is dropped (it would have
//! no identifier to draw); an unknown type folds into Related.

use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

/// A band's side key — its fold state is stored per key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RelationBandKey {
    BlockedBy,
    Blocking,
    DuplicateOf,
    DuplicatedBy,
    Related,
}

impl RelationBandKey {
    pub fn title(self) -> &'static str {
        match self {
            Self::BlockedBy => copy::BLOCKED_BY,
            Self::Blocking => copy::BLOCKING,
            Self::DuplicateOf => copy::DUPLICATE_OF,
            Self::DuplicatedBy => copy::DUPLICATED_BY,
            Self::Related => copy::RELATED,
        }
    }
}

const BAND_ORDER: [RelationBandKey; 5] = [
    RelationBandKey::BlockedBy,
    RelationBandKey::Blocking,
    RelationBandKey::DuplicateOf,
    RelationBandKey::DuplicatedBy,
    RelationBandKey::Related,
];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RelationsViewIssue {
    pub id: String,
    pub identifier: String,
    pub title: String,
    /// The dual-written ANCHOR enum (`issues.status`).
    pub status: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RelationsViewRelation {
    #[serde(rename = "type")]
    pub kind: String,
    pub issue_id: String,
    pub related_issue_id: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RelationsViewInput {
    pub subject_id: String,
    pub relations: Vec<RelationsViewRelation>,
    /// Every synced issue the relations may name (the team's issues).
    pub issues: Vec<RelationsViewIssue>,
    /// Bands the user folded/unfolded this session, overriding the default.
    pub toggled: Vec<RelationBandKey>,
    /// Bands whose "Show N more" was pressed.
    pub show_all: Vec<RelationBandKey>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RelationsViewRow {
    pub id: String,
    pub identifier: String,
    pub title: String,
    pub status: String,
    pub open: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RelationsViewBand {
    pub key: RelationBandKey,
    pub title: String,
    pub count: usize,
    pub open_count: usize,
    pub expanded: bool,
    /// The rows drawn right now (none while folded).
    pub rows: Vec<RelationsViewRow>,
    /// "Show N more", `None` when nothing is hidden.
    pub more: Option<String>,
    /// "Show less", only once "Show N more" was pressed and rows exceed the cap.
    pub less: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RelationsViewSubIssues {
    pub rows: Vec<RelationsViewRow>,
    pub done: usize,
    pub total: usize,
    /// `2/5`, `None` when there are no sub-issues.
    pub progress: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RelationsView {
    /// The "Sub-issue of" line's parent, `None` when the subject has none.
    pub parent: Option<RelationsViewRow>,
    pub sub_issues: RelationsViewSubIssues,
    pub bands: Vec<RelationsViewBand>,
    /// Total rows across the bands (the phone sheet's "Relations" count).
    pub relation_count: usize,
}

/// Copy (byte-identical ×4).
pub mod copy {
    pub const SUB_ISSUES: &str = "Sub-issues";
    pub const SUB_ISSUE_OF: &str = "Sub-issue of";
    pub const ADD_SUB_ISSUES: &str = "Add sub-issues";
    pub const RELATIONS: &str = "Relations";
    pub const ADD: &str = "Add";
    pub const BLOCKED_BY: &str = "Blocked by";
    pub const BLOCKING: &str = "Blocking";
    pub const DUPLICATE_OF: &str = "Duplicate of";
    pub const DUPLICATED_BY: &str = "Duplicated by";
    pub const RELATED: &str = "Related";
    pub const SHOW_LESS: &str = "Show less";

    /// The fixture's `copy` key → const table, in fixture order.
    pub const TABLE: &[(&str, &str)] = &[
        ("subIssues", SUB_ISSUES),
        ("subIssueOf", SUB_ISSUE_OF),
        ("addSubIssues", ADD_SUB_ISSUES),
        ("relations", RELATIONS),
        ("add", ADD),
        ("blockedBy", BLOCKED_BY),
        ("blocking", BLOCKING),
        ("duplicateOf", DUPLICATE_OF),
        ("duplicatedBy", DUPLICATED_BY),
        ("related", RELATED),
        ("showLess", SHOW_LESS),
    ];
}

pub fn relations_show_more(count: usize) -> String {
    format!("Show {count} more")
}

pub fn relations_progress(done: usize, total: usize) -> String {
    format!("{done}/{total}")
}

/// Rows a band shows before "Show N more".
pub const RELATIONS_BAND_CAP: usize = 3;

fn is_closed_anchor(status: &str) -> bool {
    matches!(status, "done" | "cancelled" | "duplicate")
}

/// The identifier's trailing number, for a natural `EXP-9 < EXP-10` order.
fn identifier_number(identifier: &str) -> u128 {
    let digits = identifier
        .bytes()
        .rev()
        .take_while(u8::is_ascii_digit)
        .count();
    if digits == 0 {
        return u128::MAX;
    }
    identifier[identifier.len() - digits..]
        .parse()
        .unwrap_or(u128::MAX)
}

fn by_identifier(a: &RelationsViewRow, b: &RelationsViewRow) -> Ordering {
    identifier_number(&a.identifier)
        .cmp(&identifier_number(&b.identifier))
        .then_with(|| a.identifier.cmp(&b.identifier))
}

/// Open rows first, each half in identifier order.
fn open_first(a: &RelationsViewRow, b: &RelationsViewRow) -> Ordering {
    if a.open != b.open {
        return if a.open { Ordering::Less } else { Ordering::Greater };
    }
    by_identifier(a, b)
}

fn to_row(issue: &RelationsViewIssue) -> RelationsViewRow {
    RelationsViewRow {
        id: issue.id.clone(),
        identifier: issue.identifier.clone(),
        title: issue.title.clone(),
        status: issue.status.clone(),
        open: !is_closed_anchor(&issue.status),
    }
}

pub fn issue_relations_view(input: &RelationsViewInput) -> RelationsView {
    let by_id: HashMap<&str, &RelationsViewIssue> =
        input.issues.iter().map(|issue| (issue.id.as_str(), issue)).collect();
    let subject = input.subject_id.as_str();
    let mut parent: Option<RelationsViewRow> = None;
    let mut children: Vec<RelationsViewRow> = Vec::new();
    let mut buckets: HashMap<RelationBandKey, Vec<RelationsViewRow>> = HashMap::new();
    let mut seen_children: HashSet<String> = HashSet::new();
    let mut seen: HashSet<(RelationBandKey, String)> = HashSet::new();

    for relation in &input.relations {
        let forward = relation.issue_id == subject;
        let inverse = relation.related_issue_id == subject;
        if forward == inverse {
            continue; // not about the subject (or a self-loop)
        }
        let other_id = if forward {
            relation.related_issue_id.as_str()
        } else {
            relation.issue_id.as_str()
        };
        let Some(other) = by_id.get(other_id) else {
            continue;
        };
        let row = to_row(other);
        let key = match relation.kind.as_str() {
            "parent" => {
                if forward {
                    if seen_children.insert(row.id.clone()) {
                        children.push(row);
                    }
                } else if parent
                    .as_ref()
                    .is_none_or(|current| by_identifier(&row, current) == Ordering::Less)
                {
                    parent = Some(row);
                }
                continue;
            }
            "blocks" if forward => RelationBandKey::Blocking,
            "blocks" => RelationBandKey::BlockedBy,
            "duplicate" if forward => RelationBandKey::DuplicateOf,
            "duplicate" => RelationBandKey::DuplicatedBy,
            _ => RelationBandKey::Related,
        };
        if seen.insert((key, row.id.clone())) {
            buckets.entry(key).or_default().push(row);
        }
    }

    children.sort_by(by_identifier);
    let done = children.iter().filter(|row| !row.open).count();

    let bands: Vec<RelationsViewBand> = BAND_ORDER
        .iter()
        .filter_map(|key| buckets.remove(key).map(|rows| (*key, rows)))
        .map(|(key, mut all)| {
            all.sort_by(open_first);
            let open_count = all.iter().filter(|row| row.open).count();
            // Blockers are the actionable relation: they open by default
            // while one is still open. Duplicates and Related stay folded.
            let open_by_default = matches!(key, RelationBandKey::BlockedBy | RelationBandKey::Blocking)
                && open_count > 0;
            let expanded = if input.toggled.contains(&key) {
                !open_by_default
            } else {
                open_by_default
            };
            let everything = input.show_all.contains(&key);
            let overflow = all.len() > RELATIONS_BAND_CAP;
            let count = all.len();
            let rows = if !expanded {
                Vec::new()
            } else if everything || !overflow {
                all
            } else {
                all.truncate(RELATIONS_BAND_CAP);
                all
            };
            RelationsViewBand {
                key,
                title: key.title().to_string(),
                count,
                open_count,
                expanded,
                rows,
                more: (expanded && overflow && !everything)
                    .then(|| relations_show_more(count - RELATIONS_BAND_CAP)),
                less: (expanded && overflow && everything).then(|| copy::SHOW_LESS.to_string()),
            }
        })
        .collect();

    let total = children.len();
    let relation_count = bands.iter().map(|band| band.count).sum();
    RelationsView {
        parent,
        sub_issues: RelationsViewSubIssues {
            rows: children,
            done,
            total,
            progress: (total > 0).then(|| relations_progress(done, total)),
        },
        bands,
        relation_count,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    /// The ONE contract fixture, replayed byte-for-byte on every client (web
    /// `issue-relations-view.test.ts`, iOS + Android twins) — the SAME case
    /// names everywhere.
    const FIXTURE: &str =
        include_str!("../../../../../packages/domain-contract/fixtures/issue-relations-view.json");

    #[derive(Deserialize)]
    struct Case {
        name: String,
        input: RelationsViewInput,
        expected: Value,
    }

    #[derive(Deserialize)]
    struct Fixture {
        copy: serde_json::Map<String, Value>,
        cases: Vec<Case>,
    }

    fn fixture() -> Fixture {
        serde_json::from_str(FIXTURE).expect("the issue-relations-view fixture parses")
    }

    #[test]
    fn relations_view_copy_table() {
        let mut derived: Vec<(String, String)> = copy::TABLE
            .iter()
            .map(|(key, value)| (key.to_string(), value.to_string()))
            .collect();
        derived.extend([
            ("showMore(4)".into(), relations_show_more(4)),
            ("progress(2,5)".into(), relations_progress(2, 5)),
            ("cap".into(), RELATIONS_BAND_CAP.to_string()),
        ]);
        let fixture = fixture();
        assert_eq!(derived.len(), fixture.copy.len(), "every copy key mirrored");
        for (key, value) in &derived {
            assert_eq!(
                fixture.copy.get(key).and_then(Value::as_str),
                Some(value.as_str()),
                "copy: {key}"
            );
        }
    }

    #[test]
    fn relations_view_contract_fixture() {
        let cases = fixture().cases;
        assert!(!cases.is_empty());
        for case in &cases {
            let actual = serde_json::to_value(issue_relations_view(&case.input)).unwrap();
            assert_eq!(actual, case.expected, "case: {}", case.name);
        }
    }

    #[test]
    fn band_titles_match_the_copy() {
        assert_eq!(RelationBandKey::BlockedBy.title(), "Blocked by");
        assert_eq!(RelationBandKey::DuplicatedBy.title(), "Duplicated by");
    }
}
