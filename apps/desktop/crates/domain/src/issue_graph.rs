//! EXP-980 — the `blocks` graph behind the list badge, the mini-graph overlay
//! and the blocked-start dialog.
//!
//! ONE pure rule, mirrored ×4 (web `apps/web/src/lib/issue-graph.ts`, iOS
//! `IssueGraph.swift`, Android `IssueGraph.kt`) and locked by the contract
//! fixture `packages/domain-contract/fixtures/issue-graph.json` — same cases,
//! same test names. EXP-1057: [`geometry`] = the ONE pixel look, locked by
//! `fixtures/issue-graph-geometry.json`.
//!
//! A canonical `blocks` row is `issue_id` BLOCKS `related_issue_id` (EXP-736).
//! An edge counts only while BOTH ends are synced and OPEN (anchor status not
//! done / cancelled / duplicate): a finished blocker is in nobody's way, and a
//! finished issue is blocked by nothing. A SUBJECT is always kept, open or not.

use std::collections::{HashMap, HashSet};

const CLOSED_ANCHORS: [&str; 3] = ["done", "cancelled", "duplicate"];

/// The most nodes one graph draws; the rest is cut and `truncated` says so.
pub const ISSUE_GRAPH_MAX_NODES: usize = 60;

/// Under a graph the node cap cut. Byte-identical ×4.
pub const ISSUE_GRAPH_TRUNCATED_NOTE: &str = "Showing the nearest 60 issues.";
/// Under a graph that holds a cycle. Byte-identical ×4.
pub const ISSUE_GRAPH_CYCLE_NOTE: &str = "Red issues block each other in a cycle.";

/// The synced `issue_relations` columns this rule reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GraphRelation<'a> {
    pub kind: &'a str,
    pub issue_id: &'a str,
    pub related_issue_id: &'a str,
}

/// What an issue row must carry to be laid out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GraphIssue<'a> {
    pub id: &'a str,
    pub identifier: &'a str,
    /// The dual-written ANCHOR enum (`issues.status`).
    pub status: &'a str,
}

/// The badge numbers of one row.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct BlockCounts {
    /// Open issues that block this one.
    pub blocked_by: usize,
    /// Open issues this one blocks.
    pub blocking: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IssueGraphNode {
    pub id: String,
    /// The column: 0 = blocked by nothing in the graph; every edge points to a
    /// higher wave (cycle edges aside).
    pub wave: usize,
    /// The row inside the wave, by identifier.
    pub lane: usize,
    pub subject: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IssueGraphEdge {
    /// The blocker.
    pub from: String,
    /// The blocked issue.
    pub to: String,
    /// Part of a blocking cycle: drawn red, and nothing on it can start.
    pub cycle: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct IssueGraph {
    pub nodes: Vec<IssueGraphNode>,
    pub edges: Vec<IssueGraphEdge>,
    pub has_cycle: bool,
    pub truncated: bool,
}

impl IssueGraph {
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// The direct blockers of `id` INSIDE this graph (the edges pointing at
    /// it), in edge order — what the phones list under a node.
    pub fn blockers_of(&self, id: &str) -> Vec<&IssueGraphEdge> {
        self.edges.iter().filter(|edge| edge.to == id).collect()
    }
}

/// The badge's accessible label: `Blocked by 2`, `Blocking 1` or
/// `Blocked by 2, blocking 1`. Byte-identical ×4.
pub fn blocks_badge_label(counts: BlockCounts) -> String {
    let mut parts: Vec<String> = Vec::new();
    if counts.blocked_by > 0 {
        parts.push(format!("Blocked by {}", counts.blocked_by));
    }
    if counts.blocking > 0 {
        let word = if parts.is_empty() { "Blocking" } else { "blocking" };
        parts.push(format!("{word} {}", counts.blocking));
    }
    parts.join(", ")
}

fn is_closed(status: &str) -> bool {
    CLOSED_ANCHORS.contains(&status)
}

/// The usable `blocks` edges, deduplicated, in relation order. `keep` names the
/// ids that survive whatever their status (the subjects). EXP-998: the rail's
/// [`crate::issue_rail::block_edges`] reads the same list.
pub(crate) fn open_edges<'a>(
    relations: &[GraphRelation<'a>],
    issues: &[GraphIssue<'a>],
    keep: &HashSet<&str>,
) -> Vec<(&'a str, &'a str)> {
    let by_id: HashMap<&str, &GraphIssue<'a>> =
        issues.iter().map(|issue| (issue.id, issue)).collect();
    let usable = |id: &str| -> bool {
        by_id
            .get(id)
            .is_some_and(|issue| keep.contains(id) || !is_closed(issue.status))
    };
    let mut seen: HashSet<(&str, &str)> = HashSet::new();
    let mut edges: Vec<(&'a str, &'a str)> = Vec::new();
    for relation in relations {
        if relation.kind != "blocks" {
            continue;
        }
        let (from, to) = (relation.issue_id, relation.related_issue_id);
        if from == to || !usable(from) || !usable(to) {
            continue;
        }
        if seen.insert((from, to)) {
            edges.push((from, to));
        }
    }
    edges
}

/// The badge numbers of every issue that has any; an issue with neither count
/// is absent.
pub fn block_counts<'a>(
    relations: &[GraphRelation<'a>],
    issues: &[GraphIssue<'a>],
) -> HashMap<String, BlockCounts> {
    let mut counts: HashMap<String, BlockCounts> = HashMap::new();
    for (from, to) in open_edges(relations, issues, &HashSet::new()) {
        counts.entry(from.to_string()).or_default().blocking += 1;
        counts.entry(to.to_string()).or_default().blocked_by += 1;
    }
    counts
}

/// The open issues that block any of `ids` from OUTSIDE the set, by identifier:
/// what a batch start has to ask about (a blocker picked into the same batch is
/// not in its way).
pub fn open_blockers_of_set<'a>(
    ids: &[&str],
    relations: &[GraphRelation<'a>],
    issues: &[GraphIssue<'a>],
) -> Vec<GraphIssue<'a>> {
    let picked: HashSet<&str> = ids.iter().copied().collect();
    let by_id: HashMap<&str, GraphIssue<'a>> =
        issues.iter().map(|issue| (issue.id, *issue)).collect();
    let mut seen: HashSet<&str> = HashSet::new();
    let mut out: Vec<GraphIssue<'a>> = Vec::new();
    for (from, to) in open_edges(relations, issues, &HashSet::new()) {
        if !picked.contains(to) || picked.contains(from) || !seen.insert(from) {
            continue;
        }
        if let Some(blocker) = by_id.get(from) {
            out.push(*blocker);
        }
    }
    out.sort_by(|a, b| a.identifier.cmp(b.identifier));
    out
}

/// The graph around `subject_ids`: the subjects, everything that transitively
/// blocks them and everything they transitively block, with every open edge
/// among those nodes.
///
/// Layout: an edge is a CYCLE edge when its blocker is reachable from its
/// blocked end; `wave` is the longest path over the remaining (acyclic) edges,
/// `lane` the identifier order inside a wave. Nodes come back by (wave, lane),
/// edges by (from, to) identifier.
pub fn block_graph<'a>(
    subject_ids: &[&str],
    relations: &[GraphRelation<'a>],
    issues: &[GraphIssue<'a>],
) -> IssueGraph {
    let by_id: HashMap<&str, GraphIssue<'a>> =
        issues.iter().map(|issue| (issue.id, *issue)).collect();
    let identifier_of = |id: &str| -> String {
        by_id
            .get(id)
            .map_or_else(|| id.to_string(), |issue| issue.identifier.to_string())
    };
    let by_identifier =
        |a: &str, b: &str| identifier_of(a).cmp(&identifier_of(b)).then_with(|| a.cmp(b));

    let mut subjects: Vec<&'a str> = Vec::new();
    for id in subject_ids {
        if let Some(issue) = by_id.get(id) {
            if !subjects.contains(&issue.id) {
                subjects.push(issue.id);
            }
        }
    }
    subjects.sort_by(|a, b| by_identifier(a, b));
    let subject_set: HashSet<&str> = subjects.iter().copied().collect();

    let all = open_edges(relations, issues, &subject_set);
    let mut blockers_of: HashMap<&str, Vec<&'a str>> = HashMap::new();
    let mut blocked_by: HashMap<&str, Vec<&'a str>> = HashMap::new();
    for &(from, to) in &all {
        blockers_of.entry(to).or_default().push(from);
        blocked_by.entry(from).or_default().push(to);
    }

    // The closure, blockers first and then the blocked side, level by level and
    // in identifier order, so the node cap cuts the same nodes everywhere.
    let mut picked: Vec<&'a str> = Vec::new();
    let mut truncated = false;
    for id in &subjects {
        if picked.len() < ISSUE_GRAPH_MAX_NODES {
            picked.push(id);
        } else {
            truncated = true;
        }
    }
    for next in [&blockers_of, &blocked_by] {
        let mut frontier: Vec<&'a str> = subjects.clone();
        while !frontier.is_empty() {
            let mut found: Vec<&'a str> = Vec::new();
            for id in &frontier {
                for other in next.get(id).map(Vec::as_slice).unwrap_or_default() {
                    if !picked.contains(other) && !found.contains(other) {
                        found.push(other);
                    }
                }
            }
            found.sort_by(|a, b| by_identifier(a, b));
            let mut admitted: Vec<&'a str> = Vec::new();
            for id in found {
                if picked.len() >= ISSUE_GRAPH_MAX_NODES {
                    truncated = true;
                    continue;
                }
                picked.push(id);
                admitted.push(id);
            }
            frontier = admitted;
        }
    }
    let index_of: HashMap<&str, usize> =
        picked.iter().enumerate().map(|(index, id)| (*id, index)).collect();

    let inside: Vec<(&'a str, &'a str)> = all
        .into_iter()
        .filter(|(from, to)| index_of.contains_key(from) && index_of.contains_key(to))
        .collect();
    let mut out: HashMap<usize, Vec<usize>> = HashMap::new();
    for (from, to) in &inside {
        out.entry(index_of[from]).or_default().push(index_of[to]);
    }
    let reaches = |start: usize, goal: usize| -> bool {
        let mut seen: HashSet<usize> = HashSet::from([start]);
        let mut stack: Vec<usize> = vec![start];
        while let Some(id) = stack.pop() {
            if id == goal {
                return true;
            }
            for other in out.get(&id).map(Vec::as_slice).unwrap_or_default() {
                if seen.insert(*other) {
                    stack.push(*other);
                }
            }
        }
        false
    };

    let mut edges: Vec<IssueGraphEdge> = inside
        .iter()
        .map(|(from, to)| IssueGraphEdge {
            from: from.to_string(),
            to: to.to_string(),
            cycle: reaches(index_of[to], index_of[from]),
        })
        .collect();
    edges.sort_by(|a, b| {
        by_identifier(&a.from, &b.from).then_with(|| by_identifier(&a.to, &b.to))
    });

    // Longest path over the acyclic edges (Kahn).
    let mut wave: Vec<usize> = vec![0; picked.len()];
    let mut pending: Vec<usize> = vec![0; picked.len()];
    let mut forward: Vec<Vec<usize>> = vec![Vec::new(); picked.len()];
    for edge in &edges {
        if edge.cycle {
            continue;
        }
        let (from, to) = (index_of[edge.from.as_str()], index_of[edge.to.as_str()]);
        pending[to] += 1;
        forward[from].push(to);
    }
    let mut ready: Vec<usize> = (0..picked.len()).filter(|index| pending[*index] == 0).collect();
    while let Some(index) = ready.pop() {
        for other in forward[index].clone() {
            wave[other] = wave[other].max(wave[index] + 1);
            pending[other] -= 1;
            if pending[other] == 0 {
                ready.push(other);
            }
        }
    }

    let mut ordered: Vec<usize> = (0..picked.len()).collect();
    ordered.sort_by(|a, b| {
        wave[*a]
            .cmp(&wave[*b])
            .then_with(|| by_identifier(picked[*a], picked[*b]))
    });
    let mut lane_at: HashMap<usize, usize> = HashMap::new();
    let nodes: Vec<IssueGraphNode> = ordered
        .into_iter()
        .map(|index| {
            let w = wave[index];
            let lane = lane_at.entry(w).or_default();
            let node = IssueGraphNode {
                id: picked[index].to_string(),
                wave: w,
                lane: *lane,
                subject: subject_set.contains(picked[index]),
            };
            *lane += 1;
            node
        })
        .collect();

    let has_cycle = edges.iter().any(|edge| edge.cycle);
    IssueGraph { nodes, edges, has_cycle, truncated }
}

/// EXP-1057 — THE mini-graph look, identical ×4 (web `ISSUE_GRAPH_GEOMETRY`,
/// iOS `IssueGraph.Geometry`, Android `IssueGraph.Geometry`), locked by
/// `packages/domain-contract/fixtures/issue-graph-geometry.json`. Points
/// (desktop px = web px). A node box sits at `inset + wave * (node + gap)`;
/// the grid is the boxes plus `inset` on every side, so rings never clip.
pub mod geometry {
    pub const NODE_WIDTH: f32 = 176.;
    pub const NODE_HEIGHT: f32 = 28.;
    pub const WAVE_GAP: f32 = 40.;
    pub const LANE_GAP: f32 = 8.;
    pub const INSET: f32 = 4.;
    pub const MAX_VIEW_WIDTH: f32 = 520.;
    pub const MAX_VIEW_HEIGHT: f32 = 320.;
    pub const EDGE_STROKE: f32 = 1.25;
    pub const RING_WIDTH: f32 = 1.;
    pub const NODE_RADIUS: f32 = 6.;
    pub const RAIL_GUTTER: f32 = 8.;
    pub const RAIL_NODE_WIDTH: f32 = 24.;
    pub const RAIL_DOT: f32 = 10.;
    pub const RAIL_DOT_RING: f32 = 2.;

    /// The grid's natural size (insets included) and the viewport it shows
    /// before scrolling.
    #[derive(Debug, Clone, Copy, Default, PartialEq)]
    pub struct GraphSize {
        pub width: f32,
        pub height: f32,
        pub view_width: f32,
        pub view_height: f32,
    }

    /// One edge as a cubic: `(start, control1, control2, end)`, each `(x, y)`.
    pub type GraphEdgeCurve = ((f32, f32), (f32, f32), (f32, f32), (f32, f32));

    /// A node box's top-left inside the grid.
    pub fn origin(wave: usize, lane: usize) -> (f32, f32) {
        (
            INSET + wave as f32 * (NODE_WIDTH + WAVE_GAP),
            INSET + lane as f32 * (NODE_HEIGHT + LANE_GAP),
        )
    }

    /// The grid for `waves` × `lanes`; nothing at all without a node.
    pub fn size(waves: usize, lanes: usize) -> GraphSize {
        if waves == 0 || lanes == 0 {
            return GraphSize::default();
        }
        let width = 2. * INSET + waves as f32 * (NODE_WIDTH + WAVE_GAP) - WAVE_GAP;
        let height = 2. * INSET + lanes as f32 * (NODE_HEIGHT + LANE_GAP) - LANE_GAP;
        GraphSize {
            width,
            height,
            view_width: width.min(MAX_VIEW_WIDTH),
            view_height: height.min(MAX_VIEW_HEIGHT),
        }
    }

    /// How far a curve's control points sit from its ends: forward, the
    /// gap's middle; backward (a cycle), `max(gap / 2, |dx| / 2)`.
    pub fn bend(start_x: f32, end_x: f32, gap: f32) -> f32 {
        if end_x > start_x {
            (end_x - start_x) / 2.
        } else {
            (gap / 2.).max((end_x - start_x).abs() / 2.)
        }
    }

    /// The blocker's right-middle → the blocked box's left-middle.
    pub fn edge(from: (usize, usize), to: (usize, usize)) -> GraphEdgeCurve {
        let (ax, ay) = origin(from.0, from.1);
        let (bx, by) = origin(to.0, to.1);
        let start = (ax + NODE_WIDTH, ay + NODE_HEIGHT / 2.);
        let end = (bx, by + NODE_HEIGHT / 2.);
        let bend = bend(start.0, end.0, WAVE_GAP);
        (start, (start.0 + bend, start.1), (end.0 - bend, end.1), end)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;

    /// The ONE contract fixture, replayed byte-for-byte on every client (web
    /// `issue-graph.test.ts`, iOS `IssueGraphTests`, Android `IssueGraphTest`)
    /// — the SAME case names everywhere.
    const FIXTURE: &str =
        include_str!("../../../../../packages/domain-contract/fixtures/issue-graph.json");

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct FixtureIssue {
        id: String,
        identifier: String,
        status: String,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct FixtureRelation {
        #[serde(rename = "type")]
        kind: String,
        issue_id: String,
        related_issue_id: String,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct FixtureCounts {
        blocked_by: usize,
        blocking: usize,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct FixtureNode {
        id: String,
        wave: usize,
        lane: usize,
        subject: bool,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct FixtureEdge {
        from: String,
        to: String,
        cycle: bool,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct FixtureGraph {
        nodes: Vec<FixtureNode>,
        edges: Vec<FixtureEdge>,
        has_cycle: bool,
        truncated: bool,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct FixtureCase {
        name: String,
        issues: Vec<FixtureIssue>,
        relations: Vec<FixtureRelation>,
        #[serde(default)]
        expected_counts: Option<HashMap<String, FixtureCounts>>,
        #[serde(default)]
        picked: Option<Vec<String>>,
        #[serde(default)]
        expected_set_blockers: Option<Vec<String>>,
        #[serde(default)]
        subjects: Option<Vec<String>>,
        #[serde(default)]
        expected_graph: Option<FixtureGraph>,
    }

    /// Every fixture case, replayed with its own name in the failure message
    /// (a Rust test name cannot be a sentence — the fixture's name IS the case
    /// identity across the four clients).
    #[test]
    fn issue_graph_contract_fixture() {
        let cases: Vec<FixtureCase> =
            serde_json::from_str(FIXTURE).expect("the issue-graph fixture parses");
        assert!(!cases.is_empty());
        for case in &cases {
            let issues: Vec<GraphIssue<'_>> = case
                .issues
                .iter()
                .map(|issue| GraphIssue {
                    id: &issue.id,
                    identifier: &issue.identifier,
                    status: &issue.status,
                })
                .collect();
            let relations: Vec<GraphRelation<'_>> = case
                .relations
                .iter()
                .map(|relation| GraphRelation {
                    kind: &relation.kind,
                    issue_id: &relation.issue_id,
                    related_issue_id: &relation.related_issue_id,
                })
                .collect();

            if let Some(expected) = &case.expected_counts {
                let counts = block_counts(&relations, &issues);
                let mut got: Vec<(String, usize, usize)> = counts
                    .iter()
                    .map(|(id, counts)| (id.clone(), counts.blocked_by, counts.blocking))
                    .collect();
                got.sort();
                let mut want: Vec<(String, usize, usize)> = expected
                    .iter()
                    .map(|(id, counts)| (id.clone(), counts.blocked_by, counts.blocking))
                    .collect();
                want.sort();
                assert_eq!(got, want, "case: {}", case.name);
            }

            if let Some(expected) = &case.expected_set_blockers {
                let picked: Vec<&str> = case
                    .picked
                    .as_deref()
                    .unwrap_or_default()
                    .iter()
                    .map(String::as_str)
                    .collect();
                let ids: Vec<&str> = open_blockers_of_set(&picked, &relations, &issues)
                    .into_iter()
                    .map(|issue| issue.id)
                    .collect();
                assert_eq!(ids, *expected, "case: {}", case.name);
            }

            if let Some(expected) = &case.expected_graph {
                let subjects: Vec<&str> = case
                    .subjects
                    .as_deref()
                    .unwrap_or_default()
                    .iter()
                    .map(String::as_str)
                    .collect();
                let graph = block_graph(&subjects, &relations, &issues);
                let nodes: Vec<(&str, usize, usize, bool)> = graph
                    .nodes
                    .iter()
                    .map(|node| (node.id.as_str(), node.wave, node.lane, node.subject))
                    .collect();
                let want_nodes: Vec<(&str, usize, usize, bool)> = expected
                    .nodes
                    .iter()
                    .map(|node| (node.id.as_str(), node.wave, node.lane, node.subject))
                    .collect();
                assert_eq!(nodes, want_nodes, "case: {}", case.name);
                let edges: Vec<(&str, &str, bool)> = graph
                    .edges
                    .iter()
                    .map(|edge| (edge.from.as_str(), edge.to.as_str(), edge.cycle))
                    .collect();
                let want_edges: Vec<(&str, &str, bool)> = expected
                    .edges
                    .iter()
                    .map(|edge| (edge.from.as_str(), edge.to.as_str(), edge.cycle))
                    .collect();
                assert_eq!(edges, want_edges, "case: {}", case.name);
                assert_eq!(graph.has_cycle, expected.has_cycle, "case: {}", case.name);
                assert_eq!(graph.truncated, expected.truncated, "case: {}", case.name);
            }
        }
    }

    const GEOMETRY_FIXTURE: &str = include_str!(
        "../../../../../packages/domain-contract/fixtures/issue-graph-geometry.json"
    );

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct GeometryPoint {
        x: f32,
        y: f32,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct GeometryCell {
        wave: usize,
        lane: usize,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct GeometrySize {
        name: String,
        waves: usize,
        lanes: usize,
        width: f32,
        height: f32,
        view_width: f32,
        view_height: f32,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct GeometryOrigin {
        wave: usize,
        lane: usize,
        x: f32,
        y: f32,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct GeometryEdge {
        name: String,
        from: GeometryCell,
        to: GeometryCell,
        start: GeometryPoint,
        control1: GeometryPoint,
        control2: GeometryPoint,
        end: GeometryPoint,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct GeometryFixture {
        constants: HashMap<String, f32>,
        sizes: Vec<GeometrySize>,
        origins: Vec<GeometryOrigin>,
        edges: Vec<GeometryEdge>,
    }

    /// EXP-1057 — the geometry fixture: constants, sizes, origins and edges,
    /// each case under its fixture name.
    #[test]
    fn issue_graph_geometry_contract_fixture() {
        use geometry::*;
        let fixture: GeometryFixture =
            serde_json::from_str(GEOMETRY_FIXTURE).expect("the geometry fixture parses");
        let constants = [
            ("nodeWidth", NODE_WIDTH),
            ("nodeHeight", NODE_HEIGHT),
            ("waveGap", WAVE_GAP),
            ("laneGap", LANE_GAP),
            ("inset", INSET),
            ("maxViewWidth", MAX_VIEW_WIDTH),
            ("maxViewHeight", MAX_VIEW_HEIGHT),
            ("edgeStroke", EDGE_STROKE),
            ("ringWidth", RING_WIDTH),
            ("nodeRadius", NODE_RADIUS),
            ("railGutter", RAIL_GUTTER),
            ("railNodeWidth", RAIL_NODE_WIDTH),
            ("railDot", RAIL_DOT),
            ("railDotRing", RAIL_DOT_RING),
        ];
        assert_eq!(fixture.constants.len(), constants.len(), "every constant mirrored");
        for (name, value) in constants {
            assert_eq!(fixture.constants.get(name), Some(&value), "constant: {name}");
        }
        for case in &fixture.sizes {
            assert_eq!(
                size(case.waves, case.lanes),
                GraphSize {
                    width: case.width,
                    height: case.height,
                    view_width: case.view_width,
                    view_height: case.view_height,
                },
                "size: {}",
                case.name
            );
        }
        for case in &fixture.origins {
            assert_eq!(
                origin(case.wave, case.lane),
                (case.x, case.y),
                "origin: {} {}",
                case.wave,
                case.lane
            );
        }
        for case in &fixture.edges {
            let point = |p: &GeometryPoint| (p.x, p.y);
            assert_eq!(
                edge((case.from.wave, case.from.lane), (case.to.wave, case.to.lane)),
                (
                    point(&case.start),
                    point(&case.control1),
                    point(&case.control2),
                    point(&case.end)
                ),
                "edge: {}",
                case.name
            );
        }
    }

    /// `blocks badge label` → `names the side that has a count`.
    #[test]
    fn names_the_side_that_has_a_count() {
        assert_eq!(
            blocks_badge_label(BlockCounts { blocked_by: 2, blocking: 0 }),
            "Blocked by 2"
        );
        assert_eq!(
            blocks_badge_label(BlockCounts { blocked_by: 0, blocking: 1 }),
            "Blocking 1"
        );
        assert_eq!(
            blocks_badge_label(BlockCounts { blocked_by: 2, blocking: 1 }),
            "Blocked by 2, blocking 1"
        );
        assert_eq!(blocks_badge_label(BlockCounts::default()), "");
    }

    /// `issue graph node cap` → `cuts the closure at the node cap, nearest
    /// blockers first`.
    #[test]
    fn cuts_the_closure_at_the_node_cap_nearest_blockers_first() {
        let total = ISSUE_GRAPH_MAX_NODES + 5;
        let ids: Vec<String> = (0..total).map(|index| format!("n{index}")).collect();
        let identifiers: Vec<String> = (0..total)
            .map(|index| format!("EXP-{}", 1000 + index))
            .collect();
        let issues: Vec<GraphIssue<'_>> = (0..total)
            .map(|index| GraphIssue {
                id: &ids[index],
                identifier: &identifiers[index],
                status: "backlog",
            })
            .collect();
        // A chain n(total-1) → … → n1 → n0; the subject is the most blocked end.
        let relations: Vec<GraphRelation<'_>> = (1..total)
            .map(|index| GraphRelation {
                kind: "blocks",
                issue_id: &ids[index],
                related_issue_id: &ids[index - 1],
            })
            .collect();
        let graph = block_graph(&[ids[0].as_str()], &relations, &issues);
        assert!(graph.truncated);
        assert_eq!(graph.nodes.len(), ISSUE_GRAPH_MAX_NODES);
        assert_eq!(
            graph.nodes.last(),
            Some(&IssueGraphNode {
                id: "n0".to_string(),
                wave: ISSUE_GRAPH_MAX_NODES - 1,
                lane: 0,
                subject: true,
            })
        );
    }

    /// The two notes are byte-identical ×4, and the truncated one names the
    /// cap it cuts at.
    #[test]
    fn the_graph_notes_stay_byte_identical() {
        assert_eq!(
            ISSUE_GRAPH_TRUNCATED_NOTE,
            format!("Showing the nearest {ISSUE_GRAPH_MAX_NODES} issues.")
        );
        assert_eq!(ISSUE_GRAPH_CYCLE_NOTE, "Red issues block each other in a cycle.");
    }
}
