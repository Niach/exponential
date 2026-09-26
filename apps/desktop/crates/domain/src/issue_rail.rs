//! EXP-998 / EXP-1057 — the BLOCKS RAIL: the column at the right edge of an
//! issue list (web `apps/web/src/lib/issue-rail.ts`, the same rule and the
//! same tests). It replaces the per-row counts pill on the desktop and the
//! web; a phone keeps the pill.
//!
//! EXP-1057: the rail draws DOTS only — no lanes, no arrows between rows. A
//! row with an open `blocks` relation gets a node (a ring when something open
//! is in its way, a dot when it only blocks others); hovering it opens the
//! mini-graph, which shows what. The rule is pure over the list's VISIBLE
//! ENTRIES in order (issue rows, group headers and the like), one slice per
//! entry, so a virtual list can paint each row alone.

use std::collections::HashMap;

use crate::issue_graph::{geometry, BlockCounts};

/// The node column, at the rail's RIGHT edge: the dot centred in it.
pub const RAIL_NODE_WIDTH: f32 = geometry::RAIL_NODE_WIDTH;
/// The blank between the cell before the rail and the node column.
pub const RAIL_GUTTER: f32 = geometry::RAIL_GUTTER;

/// One visible entry of the list, top to bottom.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RailEntry<'a> {
    Row(&'a str),
    /// A group header, a "Show more" button: never a node.
    Gap,
}

/// The node dot: `Blocked` = an open blocker exists (a ring), `Blocking` =
/// only in something else's way (a dot).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RailNode {
    Blocked,
    Blocking,
}

/// One entry's slice of the rail.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RailRow {
    /// `None` = no open relation (a gap entry always).
    pub node: Option<RailNode>,
    /// Open blockers / open blocked issues, visible or not — the label.
    pub counts: BlockCounts,
}

impl RailRow {
    /// Nothing to draw for this entry.
    pub fn is_empty(&self) -> bool {
        self.node.is_none()
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct IssueRail {
    /// One per entry, in order.
    pub entries: Vec<RailRow>,
    /// Any row with a node at all — the rail column exists when true.
    pub has_nodes: bool,
}

impl IssueRail {
    /// The rail column's width: 0 without a node anywhere.
    pub fn width(&self) -> f32 {
        rail_width(self.has_nodes)
    }
}

/// The rail column's width: the gutter plus the node column, or nothing.
pub fn rail_width(has_nodes: bool) -> f32 {
    if has_nodes {
        RAIL_GUTTER + RAIL_NODE_WIDTH
    } else {
        0.
    }
}

/// The rail for `entries`, given the per-row counts ([`crate::issue_graph`]'s
/// one open-edge rule).
pub fn issue_rail(entries: &[RailEntry<'_>], counts: &HashMap<String, BlockCounts>) -> IssueRail {
    let mut has_nodes = false;
    let rows: Vec<RailRow> = entries
        .iter()
        .map(|entry| {
            let RailEntry::Row(id) = entry else {
                return RailRow::default();
            };
            let Some(&count) = counts.get(*id) else {
                return RailRow::default();
            };
            if count.blocked_by == 0 && count.blocking == 0 {
                return RailRow::default();
            }
            has_nodes = true;
            RailRow {
                node: Some(if count.blocked_by > 0 {
                    RailNode::Blocked
                } else {
                    RailNode::Blocking
                }),
                counts: count,
            }
        })
        .collect();
    IssueRail {
        entries: rows,
        has_nodes,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::issue_graph::{block_counts, GraphIssue, GraphRelation};

    fn issue<'a>(id: &'a str, identifier: &'a str, status: &'a str) -> GraphIssue<'a> {
        GraphIssue {
            id,
            identifier,
            status,
        }
    }

    fn blocks<'a>(from: &'a str, to: &'a str) -> GraphRelation<'a> {
        GraphRelation {
            kind: "blocks",
            issue_id: from,
            related_issue_id: to,
        }
    }

    fn rail(
        entries: &[RailEntry<'_>],
        relations: &[GraphRelation<'_>],
        issues: &[GraphIssue<'_>],
    ) -> IssueRail {
        issue_rail(entries, &block_counts(relations, issues))
    }

    const A: GraphIssue<'static> = GraphIssue {
        id: "a",
        identifier: "EXP-A",
        status: "backlog",
    };
    const B: GraphIssue<'static> = GraphIssue {
        id: "b",
        identifier: "EXP-B",
        status: "backlog",
    };
    const C: GraphIssue<'static> = GraphIssue {
        id: "c",
        identifier: "EXP-C",
        status: "backlog",
    };

    #[test]
    fn draws_nothing_for_a_list_with_no_open_blocks_relation() {
        let out = rail(
            &[RailEntry::Row("a"), RailEntry::Row("b")],
            &[GraphRelation {
                kind: "parent",
                issue_id: "a",
                related_issue_id: "b",
            }],
            &[A, B],
        );
        assert!(!out.has_nodes);
        assert_eq!(out.width(), 0.);
        assert!(out.entries.iter().all(RailRow::is_empty));
    }

    #[test]
    fn marks_the_blocker_and_the_blocked_row_and_nothing_between() {
        let out = rail(
            &[RailEntry::Row("a"), RailEntry::Gap, RailEntry::Row("b")],
            &[blocks("a", "b")],
            &[A, B],
        );
        assert!(out.has_nodes);
        assert_eq!(out.width(), RAIL_GUTTER + RAIL_NODE_WIDTH);
        let [a, header, b] = out.entries.as_slice() else {
            panic!("three entries");
        };
        assert_eq!(a.node, Some(RailNode::Blocking));
        assert_eq!(
            a.counts,
            BlockCounts {
                blocked_by: 0,
                blocking: 1
            }
        );
        assert!(header.is_empty());
        assert_eq!(b.node, Some(RailNode::Blocked));
        assert_eq!(
            b.counts,
            BlockCounts {
                blocked_by: 1,
                blocking: 0
            }
        );
    }

    #[test]
    fn a_row_both_blocked_and_blocking_is_blocked() {
        let out = rail(
            &[RailEntry::Row("a"), RailEntry::Row("b"), RailEntry::Row("c")],
            &[blocks("a", "b"), blocks("b", "c")],
            &[A, B, C],
        );
        let nodes: Vec<Option<RailNode>> = out.entries.iter().map(|row| row.node).collect();
        assert_eq!(
            nodes,
            vec![
                Some(RailNode::Blocking),
                Some(RailNode::Blocked),
                Some(RailNode::Blocked)
            ]
        );
    }

    #[test]
    fn keeps_a_node_for_an_edge_whose_other_end_is_not_in_the_list() {
        let z = issue("z", "EXP-Z", "backlog");
        let out = rail(
            &[RailEntry::Row("b")],
            &[blocks("a", "b"), blocks("b", "z")],
            &[A, B, z],
        );
        assert!(out.has_nodes);
        assert_eq!(out.width(), RAIL_GUTTER + RAIL_NODE_WIDTH);
        assert_eq!(out.entries[0].node, Some(RailNode::Blocked));
        assert_eq!(
            out.entries[0].counts,
            BlockCounts {
                blocked_by: 1,
                blocking: 1
            }
        );
    }

    #[test]
    fn drops_a_closed_end_like_the_counts_badge() {
        let done = issue("a", "EXP-A", "done");
        let out = rail(
            &[RailEntry::Row("a"), RailEntry::Row("b")],
            &[blocks("a", "b")],
            &[done, B],
        );
        assert!(!out.has_nodes);
    }
}
