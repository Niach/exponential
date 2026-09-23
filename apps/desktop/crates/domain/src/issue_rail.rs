//! EXP-998 — the BLOCKS RAIL: the git-graph-style column at the right edge
//! of an issue list that draws every open `blocks` relation between two
//! VISIBLE rows as an arrow from the blocker to the issue it blocks (web
//! `apps/web/src/lib/issue-rail.ts`, the same rule and the same tests). It
//! replaces the per-row counts pill on the desktop and the web; a phone keeps
//! the pill.
//!
//! The rule is pure over the list's VISIBLE ENTRIES in order — issue rows,
//! group headers and the like — because, like the tree connector
//! ([`crate::tree_guides`]), every entry can only paint inside itself: an
//! edge that crosses a group header is drawn as one slice per entry it
//! passes. A row hidden by a fold is simply absent, so an edge to it is not
//! drawn; the row's node still tells (its ring) that something is in its way,
//! and the mini-graph behind it shows what.
//!
//! Lanes are assigned git-graph style: every edge is an interval over entry
//! indices, walked in order, taking the lowest lane free at its start. Two
//! edges may SHARE a lane when they only touch at one row (a chain A → B → C
//! reads as one line with an arrowhead at each blocked node); overlapping
//! edges never do. Lane 0 is nearest the nodes, which sit flush with the
//! rail's RIGHT edge; lanes run leftwards from there.

use std::collections::{HashMap, HashSet};

use crate::issue_graph::{open_edges, BlockCounts, GraphIssue, GraphRelation};

/// The node column, at the rail's RIGHT edge: the dot centred in it.
pub const RAIL_NODE_WIDTH: f32 = 24.;
/// One lane's width; lanes run leftwards from the node column.
pub const RAIL_LANE_PITCH: f32 = 10.;
/// The blank between the cell before the rail and the outermost lane — at
/// rest, the space between that cell and the dot.
pub const RAIL_GUTTER: f32 = 8.;

/// One open `blocks` edge, blocker → blocked, computed ONCE per query over
/// every synced issue (a blocker on another board is still in the way).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BlockEdge {
    pub from: String,
    pub to: String,
    /// `EXP-1 blocks EXP-2` — the hover label. Byte-identical ×4.
    pub label: String,
    /// Part of a blocking cycle — drawn red.
    pub cycle: bool,
}

/// `EXP-1 blocks EXP-2`. Byte-identical ×4.
pub fn rail_edge_label(from: &str, to: &str) -> String {
    format!("{from} blocks {to}")
}

/// Every open `blocks` edge, deduplicated, with its label and whether it
/// sits on a cycle (strongly connected components over the open edges: an
/// edge whose ends share a component is cyclic).
pub fn block_edges<'a>(
    relations: &[GraphRelation<'a>],
    issues: &[GraphIssue<'a>],
) -> Vec<BlockEdge> {
    let open = open_edges(relations, issues, &HashSet::new());
    let identifier_of: HashMap<&str, &str> = issues
        .iter()
        .map(|issue| (issue.id, issue.identifier))
        .collect();
    let component = components(&open);
    open.into_iter()
        .map(|(from, to)| BlockEdge {
            from: from.to_string(),
            to: to.to_string(),
            label: rail_edge_label(
                identifier_of.get(from).copied().unwrap_or(from),
                identifier_of.get(to).copied().unwrap_or(to),
            ),
            cycle: component.get(from) == component.get(to),
        })
        .collect()
}

/// Tarjan's strongly connected components, iteratively (a deep chain must
/// not overflow the stack): node → component number.
fn components<'a>(edges: &[(&'a str, &'a str)]) -> HashMap<&'a str, usize> {
    let mut out: HashMap<&str, Vec<&str>> = HashMap::new();
    let mut order: Vec<&str> = Vec::new();
    for &(from, to) in edges {
        if !out.contains_key(from) {
            order.push(from);
        }
        out.entry(from).or_default().push(to);
        if !out.contains_key(to) {
            order.push(to);
            out.insert(to, Vec::new());
        }
    }
    let mut index: HashMap<&str, usize> = HashMap::new();
    let mut low: HashMap<&str, usize> = HashMap::new();
    let mut component: HashMap<&str, usize> = HashMap::new();
    let mut stack: Vec<&str> = Vec::new();
    let mut on_stack: HashSet<&str> = HashSet::new();
    let mut next = 0usize;
    let mut components = 0usize;
    for &start in &order {
        if index.contains_key(start) {
            continue;
        }
        let mut frames: Vec<(&str, usize)> = vec![(start, 0)];
        index.insert(start, next);
        low.insert(start, next);
        next += 1;
        stack.push(start);
        on_stack.insert(start);
        while let Some(&(id, at)) = frames.last() {
            let targets = out.get(id).map(Vec::as_slice).unwrap_or(&[]);
            if at < targets.len() {
                let target = targets[at];
                frames.last_mut().expect("frame").1 += 1;
                if !index.contains_key(target) {
                    index.insert(target, next);
                    low.insert(target, next);
                    next += 1;
                    stack.push(target);
                    on_stack.insert(target);
                    frames.push((target, 0));
                } else if on_stack.contains(target) {
                    let candidate = index[target];
                    let entry = low.entry(id).or_insert(candidate);
                    *entry = (*entry).min(candidate);
                }
                continue;
            }
            frames.pop();
            if let Some(&(parent, _)) = frames.last() {
                let child_low = low[id];
                let entry = low.entry(parent).or_insert(child_low);
                *entry = (*entry).min(child_low);
            }
            if low[id] == index[id] {
                loop {
                    let member = stack.pop().expect("member");
                    on_stack.remove(member);
                    component.insert(member, components);
                    if member == id {
                        break;
                    }
                }
                components += 1;
            }
        }
    }
    component
}

/// One visible entry of the list, top to bottom.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RailEntry<'a> {
    Row(&'a str),
    /// A group header, a "Show more" button: lanes pass, nothing else.
    Gap,
}

/// What a lane draws at ONE entry.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RailLane {
    pub lane: usize,
    /// A vertical from the entry's top edge to its centre.
    pub top: bool,
    /// A vertical from the entry's centre to its bottom edge.
    pub bottom: bool,
    /// The lane joins this row's node under an arrowhead: this row is
    /// blocked by the edge's other end.
    pub into: bool,
    /// The lane leaves this row's node: this row blocks the other end.
    pub out: bool,
    /// The edge whose other end sits ABOVE joins the node here — the line
    /// from the top edge bends into the node.
    pub join_above: bool,
    /// The edge whose other end sits BELOW joins the node here.
    pub join_below: bool,
    /// Part of a blocking cycle — drawn red.
    pub cycle: bool,
    /// Indices into the [`BlockEdge`] list of the edges using this lane at
    /// this entry — the hover target and its label.
    pub edges: Vec<usize>,
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
    /// By lane, ascending.
    pub lanes: Vec<RailLane>,
}

impl RailRow {
    /// Nothing to draw for this entry.
    pub fn is_empty(&self) -> bool {
        self.node.is_none() && self.lanes.is_empty()
    }

    /// The edges that touch this row's node.
    pub fn node_edges(&self) -> Vec<usize> {
        let mut out: Vec<usize> = Vec::new();
        for lane in self.lanes.iter().filter(|lane| lane.into || lane.out) {
            for &edge in &lane.edges {
                if !out.contains(&edge) {
                    out.push(edge);
                }
            }
        }
        out
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct IssueRail {
    /// One per entry, in order. A gap entry carries only pass-through lanes.
    pub entries: Vec<RailRow>,
    /// How many lanes the rail needs; 0 = no arrow to draw.
    pub lane_count: usize,
    /// Any row with a node at all — the rail column exists when true.
    pub has_nodes: bool,
}

impl IssueRail {
    /// The rail column's width: 0 without a node anywhere.
    pub fn width(&self) -> f32 {
        rail_width(self.lane_count, self.has_nodes)
    }
}

/// The rail column's width for a lane count.
pub fn rail_width(lane_count: usize, has_nodes: bool) -> f32 {
    if !has_nodes {
        return 0.;
    }
    RAIL_GUTTER + RAIL_LANE_PITCH * lane_count as f32 + RAIL_NODE_WIDTH
}

/// The x of the node dot's centre inside a rail `width` wide.
pub fn rail_node_x(width: f32) -> f32 {
    width - RAIL_NODE_WIDTH / 2.
}

/// The x of lane `lane`'s centre inside a rail `width` wide.
pub fn rail_lane_x(lane: usize, width: f32) -> f32 {
    width - RAIL_NODE_WIDTH - RAIL_LANE_PITCH * lane as f32 - RAIL_LANE_PITCH / 2.
}

/// The rail for `entries`, given the open edges and the per-row counts
/// (both from [`crate::issue_graph`]'s one open-edge rule).
pub fn issue_rail(
    entries: &[RailEntry<'_>],
    edges: &[BlockEdge],
    counts: &HashMap<String, BlockCounts>,
) -> IssueRail {
    let mut at: HashMap<&str, usize> = HashMap::new();
    for (index, entry) in entries.iter().enumerate() {
        if let RailEntry::Row(id) = entry {
            at.entry(id).or_insert(index);
        }
    }

    // The drawable edges: both ends visible. Ordered by their upper end, then
    // their lower end, then by key — so the lane walk is deterministic.
    struct Span {
        edge: usize,
        from: usize,
        to: usize,
        lo: usize,
        hi: usize,
        cycle: bool,
    }
    let mut spans: Vec<Span> = edges
        .iter()
        .enumerate()
        .filter_map(|(index, edge)| {
            let from = *at.get(edge.from.as_str())?;
            let to = *at.get(edge.to.as_str())?;
            Some(Span {
                edge: index,
                from,
                to,
                lo: from.min(to),
                hi: from.max(to),
                cycle: edge.cycle,
            })
        })
        .collect();
    spans.sort_by(|a, b| {
        a.lo.cmp(&b.lo).then(a.hi.cmp(&b.hi)).then_with(|| {
            let ka = (&edges[a.edge].from, &edges[a.edge].to);
            let kb = (&edges[b.edge].from, &edges[b.edge].to);
            ka.cmp(&kb)
        })
    });

    // Greedy lanes: the lowest lane whose last edge ENDED at or above this
    // edge's start (touching at one row is sharing, overlapping is not).
    let mut lane_end: Vec<usize> = Vec::new();
    let lanes: Vec<usize> = spans
        .iter()
        .map(|span| match lane_end.iter().position(|&end| end <= span.lo) {
            Some(lane) => {
                lane_end[lane] = span.hi;
                lane
            }
            None => {
                lane_end.push(span.hi);
                lane_end.len() - 1
            }
        })
        .collect();

    let mut rows: Vec<RailRow> = vec![RailRow::default(); entries.len()];
    for (span, &lane) in spans.iter().zip(&lanes) {
        for entry in span.lo..=span.hi {
            let row = &mut rows[entry];
            let slot = match row.lanes.iter().position(|slot| slot.lane == lane) {
                Some(position) => &mut row.lanes[position],
                None => {
                    row.lanes.push(RailLane {
                        lane,
                        ..RailLane::default()
                    });
                    row.lanes.sort_by_key(|slot| slot.lane);
                    let position = row
                        .lanes
                        .iter()
                        .position(|slot| slot.lane == lane)
                        .expect("just pushed");
                    &mut row.lanes[position]
                }
            };
            slot.edges.push(span.edge);
            slot.cycle |= span.cycle;
            if entry > span.lo {
                slot.top = true;
            }
            if entry < span.hi {
                slot.bottom = true;
            }
            if entry == span.from {
                slot.out = true;
            }
            if entry == span.to {
                slot.into = true;
            }
            if span.lo != span.hi {
                if entry == span.lo {
                    slot.join_below = true;
                }
                if entry == span.hi {
                    slot.join_above = true;
                }
            }
        }
    }

    let mut has_nodes = false;
    for (index, entry) in entries.iter().enumerate() {
        let RailEntry::Row(id) = entry else {
            continue;
        };
        let Some(&count) = counts.get(*id) else {
            continue;
        };
        if count.blocked_by == 0 && count.blocking == 0 {
            continue;
        }
        let row = &mut rows[index];
        row.counts = count;
        row.node = Some(if count.blocked_by > 0 {
            RailNode::Blocked
        } else {
            RailNode::Blocking
        });
        has_nodes = true;
    }

    IssueRail {
        entries: rows,
        lane_count: lane_end.len(),
        has_nodes,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::issue_graph::block_counts;

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
        issue_rail(
            entries,
            &block_edges(relations, issues),
            &block_counts(relations, issues),
        )
    }

    /// `[lane, top, bottom, into, out]` — the web test's tuple.
    fn shape(lane: &RailLane) -> (usize, bool, bool, bool, bool) {
        (lane.lane, lane.top, lane.bottom, lane.into, lane.out)
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
        assert_eq!(out.lane_count, 0);
        assert_eq!(out.width(), 0.);
        assert!(out.entries.iter().all(RailRow::is_empty));
    }

    #[test]
    fn runs_one_lane_from_the_blocker_down_into_the_blocked_row() {
        let out = rail(
            &[RailEntry::Row("a"), RailEntry::Gap, RailEntry::Row("b")],
            &[blocks("a", "b")],
            &[A, B],
        );
        assert_eq!(out.lane_count, 1);
        assert!(out.has_nodes);
        assert_eq!(out.width(), RAIL_GUTTER + RAIL_LANE_PITCH + RAIL_NODE_WIDTH);
        let [a, header, b] = out.entries.as_slice() else {
            panic!("three entries");
        };
        assert_eq!(a.node, Some(RailNode::Blocking));
        assert_eq!(
            a.lanes.iter().map(shape).collect::<Vec<_>>(),
            vec![(0, false, true, false, true)]
        );
        // The header only carries the line through.
        assert_eq!(header.node, None);
        assert_eq!(
            header.lanes.iter().map(shape).collect::<Vec<_>>(),
            vec![(0, true, true, false, false)]
        );
        assert_eq!(b.node, Some(RailNode::Blocked));
        assert_eq!(
            b.counts,
            BlockCounts {
                blocked_by: 1,
                blocking: 0
            }
        );
        assert_eq!(
            b.lanes.iter().map(shape).collect::<Vec<_>>(),
            vec![(0, true, false, true, false)]
        );
        assert_eq!(b.lanes[0].edges, vec![0]);
    }

    #[test]
    fn labels_every_edge_blocker_first() {
        let edges = block_edges(&[blocks("a", "b")], &[A, B]);
        assert_eq!(edges[0].label, "EXP-A blocks EXP-B");
        assert!(!edges[0].cycle);
    }

    #[test]
    fn points_the_arrow_up_when_the_blocker_sits_below() {
        let out = rail(
            &[RailEntry::Row("b"), RailEntry::Row("a")],
            &[blocks("a", "b")],
            &[A, B],
        );
        let [b, a] = out.entries.as_slice() else {
            panic!("two entries");
        };
        assert_eq!(
            b.lanes.iter().map(shape).collect::<Vec<_>>(),
            vec![(0, false, true, true, false)]
        );
        assert_eq!(
            a.lanes.iter().map(shape).collect::<Vec<_>>(),
            vec![(0, true, false, false, true)]
        );
    }

    #[test]
    fn gives_overlapping_edges_their_own_lanes_shortest_innermost() {
        let out = rail(
            &[RailEntry::Row("a"), RailEntry::Row("b"), RailEntry::Row("c")],
            &[blocks("a", "b"), blocks("a", "c")],
            &[A, B, C],
        );
        assert_eq!(out.lane_count, 2);
        let [a, b, c] = out.entries.as_slice() else {
            panic!("three entries");
        };
        assert_eq!(
            a.lanes.iter().map(shape).collect::<Vec<_>>(),
            vec![(0, false, true, false, true), (1, false, true, false, true)]
        );
        assert_eq!(
            b.lanes.iter().map(shape).collect::<Vec<_>>(),
            vec![(0, true, false, true, false), (1, true, true, false, false)]
        );
        assert_eq!(
            c.lanes.iter().map(shape).collect::<Vec<_>>(),
            vec![(1, true, false, true, false)]
        );
    }

    #[test]
    fn shares_a_lane_along_a_chain_with_an_arrowhead_at_every_blocked_node() {
        let out = rail(
            &[RailEntry::Row("a"), RailEntry::Row("b"), RailEntry::Row("c")],
            &[blocks("a", "b"), blocks("b", "c")],
            &[A, B, C],
        );
        assert_eq!(out.lane_count, 1);
        let [a, b, c] = out.entries.as_slice() else {
            panic!("three entries");
        };
        assert_eq!(b.node, Some(RailNode::Blocked));
        assert_eq!(
            b.lanes.iter().map(shape).collect::<Vec<_>>(),
            vec![(0, true, true, true, true)]
        );
        assert_eq!(b.lanes[0].edges, vec![0, 1]);
        // Both edges bend into b's node: one from above, one from below —
        // no straight pass-through.
        assert_eq!((b.lanes[0].join_above, b.lanes[0].join_below), (true, true));
        assert_eq!((a.lanes[0].join_above, a.lanes[0].join_below), (false, true));
        assert_eq!((c.lanes[0].join_above, c.lanes[0].join_below), (true, false));
        assert_eq!(b.node_edges(), vec![0, 1]);
    }

    #[test]
    fn keeps_a_node_for_an_edge_whose_other_end_is_not_in_the_list() {
        let z = issue("z", "EXP-Z", "backlog");
        let out = rail(
            &[RailEntry::Row("b")],
            &[blocks("a", "b"), blocks("b", "z")],
            &[A, B, z],
        );
        assert_eq!(out.lane_count, 0);
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
        assert!(out.entries[0].lanes.is_empty());
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

    #[test]
    fn marks_a_blocking_cycle_on_every_edge_of_it() {
        let out = rail(
            &[RailEntry::Row("a"), RailEntry::Row("b"), RailEntry::Row("c")],
            &[blocks("a", "b"), blocks("b", "a"), blocks("b", "c")],
            &[A, B, C],
        );
        let [a, b, c] = out.entries.as_slice() else {
            panic!("three entries");
        };
        assert_eq!(
            a.lanes.iter().map(|lane| lane.cycle).collect::<Vec<_>>(),
            vec![true, true]
        );
        assert_eq!(
            b.lanes.iter().map(|lane| lane.cycle).collect::<Vec<_>>(),
            vec![true, true]
        );
        // b → c is not on the cycle.
        assert_eq!(
            c.lanes.iter().map(|lane| lane.cycle).collect::<Vec<_>>(),
            vec![false]
        );
    }

    #[test]
    fn lays_the_lanes_out_leftwards_from_the_node_column() {
        let width = RAIL_GUTTER + RAIL_LANE_PITCH * 3. + RAIL_NODE_WIDTH;
        assert_eq!(rail_node_x(width), width - RAIL_NODE_WIDTH / 2.);
        assert_eq!(
            rail_lane_x(0, width),
            width - RAIL_NODE_WIDTH - RAIL_LANE_PITCH / 2.
        );
        assert_eq!(rail_lane_x(2, width), RAIL_GUTTER + RAIL_LANE_PITCH / 2.);
        assert_eq!(rail_edge_label("EXP-1", "EXP-2"), "EXP-1 blocks EXP-2");
    }
}
