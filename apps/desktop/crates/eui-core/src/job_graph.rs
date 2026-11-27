//! #EXP-2204 (packages/exponential-ui/docs/ci-components-contract.md §5):
//! the JobGraph layout, the TS reference's `src/job-graph.ts`; both replay
//! `fixtures/job-graph.json`. [`job_graph_layout`]: the jobs → stage columns
//! (or needs depth), the node boxes and the need edges, or (`compact`, below
//! `md`) one full-width row per node; [`job_graph_upstream`]: a node and its
//! ancestors, the set a hover or focus draws connectors for.

use std::collections::{HashMap, HashSet};

use serde_json::{json, Value};

use crate::ci_log::ci_status_of;
use crate::json::number;

/// style.json `jobGraph` (px); `row_height` = a compact list row.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct JobGraphGeometry {
    pub node_width: f64,
    pub node_height: f64,
    pub column_gap: f64,
    pub row_gap: f64,
    pub stage_height: f64,
    pub stage_gap: f64,
    pub edge_width: f64,
    pub row_height: f64,
}

pub const JOB_GRAPH: JobGraphGeometry =
    JobGraphGeometry { node_width: 200.0, node_height: 40.0, column_gap: 48.0, row_gap: 12.0, stage_height: 24.0, stage_gap: 8.0, edge_width: 1.5, row_height: 40.0 };

#[derive(Debug, Clone, PartialEq)]
pub struct JobGraphNode {
    /// The job id, or the matrix key of a collapsed matrix.
    pub id: String,
    pub name: String,
    pub status: &'static str,
    pub duration: Option<String>,
    /// A matrix: (passed, total); else None.
    pub count: Option<(usize, usize)>,
    pub members: Vec<String>,
    /// Known upstream node ids, first-seen order, never itself.
    pub needs: Vec<String>,
    pub stage: String,
    pub column: usize,
    pub row: usize,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

impl JobGraphNode {
    /// The meta text: `3/5` for a matrix, else the duration.
    pub fn meta(&self) -> Option<String> {
        match self.count {
            Some((p, t)) => Some(format!("{p}/{t}")),
            None => self.duration.clone(),
        }
    }

    pub fn to_json(&self) -> Value {
        json!({
            "id": self.id, "name": self.name, "status": self.status, "duration": self.duration,
            "count": self.count.map(|(p, t)| json!({"passed": p, "total": t})),
            "members": self.members, "needs": self.needs, "stage": self.stage,
            "column": self.column, "row": self.row,
            "x": number(self.x), "y": number(self.y), "width": number(self.width), "height": number(self.height),
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct JobGraphEdge {
    pub from: String,
    pub to: String,
    pub start: [f64; 2],
    pub c1: [f64; 2],
    pub c2: [f64; 2],
    pub end: [f64; 2],
}

impl JobGraphEdge {
    pub fn to_json(&self) -> Value {
        let p = |v: [f64; 2]| json!([number(v[0]), number(v[1])]);
        json!({"from": self.from, "to": self.to, "start": p(self.start), "c1": p(self.c1), "c2": p(self.c2), "end": p(self.end)})
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct JobGraphLayout {
    /// `graph` | `list`.
    pub mode: &'static str,
    pub width: f64,
    pub height: f64,
    /// (title, x), only with stages.
    pub stages: Vec<(String, f64)>,
    pub nodes: Vec<JobGraphNode>,
    pub edges: Vec<JobGraphEdge>,
}

impl JobGraphLayout {
    pub fn to_json(&self) -> Value {
        json!({
            "mode": self.mode, "width": number(self.width), "height": number(self.height),
            "stages": self.stages.iter().map(|(t, x)| json!({"title": t, "x": number(*x)})).collect::<Vec<_>>(),
            "nodes": self.nodes.iter().map(JobGraphNode::to_json).collect::<Vec<_>>(),
            "edges": self.edges.iter().map(JobGraphEdge::to_json).collect::<Vec<_>>(),
        })
    }
}

/// The aggregate status of a matrix: the first present, in this order.
const MATRIX_ORDER: [&str; 6] = ["failed", "running", "queued", "cancelled", "passed", "skipped"];

fn non_empty(v: Option<&Value>) -> Option<&str> {
    v.and_then(Value::as_str).filter(|s| !s.is_empty())
}

struct Draft {
    node: JobGraphNode,
    raw_needs: Vec<String>,
    statuses: Vec<&'static str>,
    matrix: bool,
}

fn blank(id: &str, name: &str, status: &'static str, duration: Option<String>, stage: String) -> JobGraphNode {
    JobGraphNode { id: id.into(), name: name.into(), status, duration, count: None, members: vec![id.into()], needs: Vec::new(), stage, column: 0, row: 0, x: 0.0, y: 0.0, width: 0.0, height: 0.0 }
}

/// The jobs → stage columns (or needs depth), the node boxes and the need
/// edges; `compact` = the list below `md` (contract §5).
pub fn job_graph_layout(jobs: Option<&Value>, compact: bool, g: &JobGraphGeometry) -> JobGraphLayout {
    // 1. Normalise and collapse matrices.
    let mut drafts: Vec<Draft> = Vec::new();
    let mut by_key: HashMap<String, usize> = HashMap::new();
    let mut node_of: HashMap<String, String> = HashMap::new();
    let mut seen: HashSet<String> = HashSet::new();
    for raw in jobs.and_then(Value::as_array).into_iter().flatten() {
        let Some(job) = raw.as_object() else { continue };
        let Some(id) = non_empty(job.get("id")) else { continue };
        if !seen.insert(id.to_string()) {
            continue;
        }
        let status = ci_status_of(job.get("status"));
        let needs: Vec<String> = job.get("needs").and_then(Value::as_array).map(|a| a.iter().filter_map(Value::as_str).map(str::to_string).collect()).unwrap_or_default();
        let stage = job.get("stage").and_then(Value::as_str).unwrap_or("").to_string();
        if let Some(matrix) = non_empty(job.get("matrix")) {
            let existing = by_key.get(matrix).copied();
            if existing.is_none() && node_of.get(matrix).is_some_and(|v| v == matrix) {
                continue; // a plain job owns that id
            }
            node_of.insert(id.to_string(), matrix.to_string());
            if let Some(i) = existing {
                let d = &mut drafts[i];
                d.node.members.push(id.to_string());
                d.statuses.push(status);
                d.raw_needs.extend(needs);
                continue;
            }
            let mut node = blank(matrix, matrix, status, None, stage);
            node.members = vec![id.to_string()];
            by_key.insert(matrix.to_string(), drafts.len());
            drafts.push(Draft { node, raw_needs: needs, statuses: vec![status], matrix: true });
            continue;
        }
        if by_key.contains_key(id) {
            continue; // a matrix owns that id
        }
        node_of.insert(id.to_string(), id.to_string());
        let name = non_empty(job.get("name")).unwrap_or(id);
        let duration = job.get("duration").and_then(Value::as_str).map(str::to_string);
        drafts.push(Draft { node: blank(id, name, status, duration, stage), raw_needs: needs, statuses: vec![status], matrix: false });
    }
    let node_ids: HashSet<String> = drafts.iter().map(|d| d.node.id.clone()).collect();
    for d in &mut drafts {
        if d.matrix {
            d.node.status = MATRIX_ORDER.iter().find(|s| d.statuses.contains(s)).copied().unwrap_or("queued");
            d.node.count = Some((d.statuses.iter().filter(|s| **s == "passed").count(), d.statuses.len()));
        }
        let mut needs: Vec<String> = Vec::new();
        for need in &d.raw_needs {
            let target = node_of.get(need).cloned().or_else(|| by_key.contains_key(need).then(|| need.clone()));
            let Some(target) = target else { continue };
            if !node_ids.contains(&target) || target == d.node.id || needs.contains(&target) {
                continue;
            }
            needs.push(target);
        }
        d.node.needs = needs;
    }
    let mut nodes: Vec<JobGraphNode> = drafts.into_iter().map(|d| d.node).collect();
    let index: HashMap<String, usize> = nodes.iter().enumerate().map(|(i, n)| (n.id.clone(), i)).collect();

    // 2. Columns: the stages in first-appearance order, else the needs depth.
    let staged = nodes.iter().any(|n| !n.stage.is_empty());
    let mut stage_titles: Vec<String> = Vec::new();
    if staged {
        for n in &mut nodes {
            let col = match stage_titles.iter().position(|t| *t == n.stage) {
                Some(c) => c,
                None => {
                    stage_titles.push(n.stage.clone());
                    stage_titles.len() - 1
                }
            };
            n.column = col;
        }
    } else {
        let mut depth: Vec<Option<usize>> = vec![None; nodes.len()];
        let mut on_path = vec![false; nodes.len()];
        fn visit(i: usize, nodes: &[JobGraphNode], index: &HashMap<String, usize>, depth: &mut [Option<usize>], on_path: &mut [bool]) -> usize {
            if let Some(d) = depth[i] {
                return d;
            }
            on_path[i] = true;
            let mut d = 0;
            for need in &nodes[i].needs {
                let j = index[need];
                if on_path[j] {
                    continue;
                }
                d = d.max(crate::deep(|| visit(j, nodes, index, depth, on_path)) + 1);
            }
            on_path[i] = false;
            depth[i] = Some(d);
            d
        }
        for i in 0..nodes.len() {
            let d = visit(i, &nodes, &index, &mut depth, &mut on_path);
            nodes[i].column = d;
        }
    }

    // Column order: column 0 first, then input order (a stable sort).
    let mut ordered = nodes;
    ordered.sort_by_key(|n| n.column);

    // 5. compact: one full-width row per node.
    if compact {
        let count = ordered.len();
        for (i, n) in ordered.iter_mut().enumerate() {
            n.row = i;
            n.x = 0.0;
            n.y = i as f64 * g.row_height;
            n.width = 0.0;
            n.height = g.row_height;
        }
        return JobGraphLayout { mode: "list", width: 0.0, height: count as f64 * g.row_height, stages: Vec::new(), nodes: ordered, edges: Vec::new() };
    }

    // 3. Geometry.
    let top = if staged { g.stage_height + g.stage_gap } else { 0.0 };
    let pitch_x = g.node_width + g.column_gap;
    let mut rows: HashMap<usize, usize> = HashMap::new();
    for n in &mut ordered {
        let r = rows.entry(n.column).or_insert(0);
        n.row = *r;
        *r += 1;
        n.x = n.column as f64 * pitch_x;
        n.y = top + n.row as f64 * (g.node_height + g.row_gap);
        n.width = g.node_width;
        n.height = g.node_height;
    }
    let columns = ordered.iter().map(|n| n.column + 1).max().unwrap_or(0);
    let max_rows = rows.values().copied().max().unwrap_or(0);
    let width = if columns > 0 { columns as f64 * pitch_x - g.column_gap } else { 0.0 };
    let height = if max_rows > 0 { top + max_rows as f64 * (g.node_height + g.row_gap) - g.row_gap } else { 0.0 };
    let stages = if staged { stage_titles.into_iter().enumerate().map(|(i, t)| (t, i as f64 * pitch_x)).collect() } else { Vec::new() };
    let at: HashMap<&str, usize> = ordered.iter().enumerate().map(|(i, n)| (n.id.as_str(), i)).collect();
    let mut edges = Vec::new();
    for n in &ordered {
        for need in &n.needs {
            let up = &ordered[at[need.as_str()]];
            let (x1, y1) = (up.x + up.width, up.y + up.height / 2.0);
            let (x2, y2) = (n.x, n.y + n.height / 2.0);
            let mx = (x1 + x2) / 2.0;
            edges.push(JobGraphEdge { from: up.id.clone(), to: n.id.clone(), start: [x1, y1], c1: [mx, y1], c2: [mx, y2], end: [x2, y2] });
        }
    }
    JobGraphLayout { mode: "graph", width, height, stages, nodes: ordered, edges }
}

/// The node `id` and every ancestor through `needs` (transitive), in the
/// layout's node order; an unknown id → [].
pub fn job_graph_upstream(layout: &JobGraphLayout, id: &str) -> Vec<String> {
    let by_id: HashMap<&str, &JobGraphNode> = layout.nodes.iter().map(|n| (n.id.as_str(), n)).collect();
    if !by_id.contains_key(id) {
        return Vec::new();
    }
    let mut set: HashSet<&str> = HashSet::from([id]);
    let mut stack = vec![id];
    while let Some(top) = stack.pop() {
        for need in &by_id[top].needs {
            if set.insert(need.as_str()) {
                stack.push(need.as_str());
            }
        }
    }
    layout.nodes.iter().filter(|n| set.contains(n.id.as_str())).map(|n| n.id.clone()).collect()
}
