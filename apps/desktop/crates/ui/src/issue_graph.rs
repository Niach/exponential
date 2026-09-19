//! EXP-980 — the `blocks` MINI-GRAPH: the ONE view three surfaces share.
//!
//! * a list row's blocks badge opens it for that row's issue;
//! * the work header's PR-graph overlay draws it on the Issue face, where a
//!   flat "Blocked by" chip list used to sit;
//! * the blocked-start dialog draws it for everything that was picked.
//!
//! The layout is [`domain::issue_graph::block_graph`]'s: column = wave, row =
//! lane. Nothing here decides an order — this module only turns the rule's
//! `(wave, lane)` pairs into pixels, paints the edges between them (grey, RED
//! on a cycle) and draws the two notes underneath. The phones render the SAME
//! graph as a list grouped by wave.
//!
//! EXP-981 generalised the pixel half into [`grid_view`]: a NODE-KEYED grid
//! over `(wave, lane)` positions with a caller-supplied box renderer. The
//! workflow detail feeds it the server-laid-out `workflow_nodes` (whose
//! `wave`/`lane` come off the wire rather than from `block_graph`), so both
//! surfaces draw the same picture with the same edges and the same red.

use std::collections::HashMap;
use std::rc::Rc;

use gpui::{
    canvas, div, point, px, App, Bounds, ClickEvent, Hsla, InteractiveElement as _, IntoElement,
    ParentElement, Pixels, SharedString, StatefulInteractiveElement as _, Styled, Window,
};
use gpui_component::{h_flex, v_flex, ActiveTheme as _, Icon, Sizable as _};

use domain::issue_graph::{
    block_graph, blocks_badge_label, open_blockers_of_set, BlockCounts, GraphIssue, GraphRelation,
    IssueGraph, ISSUE_GRAPH_CYCLE_NOTE, ISSUE_GRAPH_TRUNCATED_NOTE,
};
use domain::workflow_view::WorkflowEdgeStyle;

use crate::icons::registry;
use crate::issue_chip::issue_chip;
use crate::surface::{glass_pill_button, PillSize};

/// One node's box. Wide enough for an identifier and a clipped title.
const NODE_W: f32 = 176.;
/// The issue graph's one-line box.
const NODE_H: f32 = 24.;
/// The space an edge crosses between two waves.
const COL_GAP: f32 = 44.;
/// The space between two lanes of the same wave.
const LANE_GAP: f32 = 8.;
/// The graph's default viewport width — past it the grid scrolls rather than
/// growing the popover off the screen. Hosts with a narrower box (the work
/// header's overlay, the dialog) pass their own.
pub(crate) const VIEW_W: f32 = 520.;
const VIEW_H: f32 = 320.;
/// The edge stroke.
const LINE: f32 = 1.;

/// What a node tap does — open that issue, in whatever way the host surface
/// navigates (a popover navigates in place, a dialog closes first).
pub(crate) type OnPickIssue = Rc<dyn Fn(&str, &mut Window, &mut App)>;

/// How one host's grid is measured: the cell, the gaps between cells, the
/// viewport it scrolls inside, and where an edge LEAVES and ENTERS a cell
/// (offsets from the cell's top-left). Boxes anchor on their side middles;
/// the workflow graph's circles anchor on the circle, not on the label
/// underneath it.
#[derive(Clone, Copy)]
pub(crate) struct GridGeometry {
    pub(crate) node_w: f32,
    pub(crate) node_h: f32,
    pub(crate) col_gap: f32,
    pub(crate) lane_gap: f32,
    pub(crate) view_w: f32,
    pub(crate) view_h: f32,
    pub(crate) edge_out: (f32, f32),
    pub(crate) edge_in: (f32, f32),
}

impl GridGeometry {
    /// The blocks mini-graph: one-line boxes, edges side to side.
    pub(crate) fn boxes(view_w: f32) -> Self {
        Self {
            node_w: NODE_W,
            node_h: NODE_H,
            col_gap: COL_GAP,
            lane_gap: LANE_GAP,
            view_w,
            view_h: VIEW_H,
            edge_out: (NODE_W, NODE_H / 2.),
            edge_in: (0., NODE_H / 2.),
        }
    }

    fn x(&self, wave: usize) -> f32 {
        wave as f32 * (self.node_w + self.col_gap)
    }

    fn y(&self, lane: usize) -> f32 {
        lane as f32 * (self.node_h + self.lane_gap)
    }
}

/// EXP-981 — one placed box of the shared grid, keyed by whatever the host
/// calls its nodes (an ISSUE id here, a `workflow_nodes` row id there).
pub(crate) struct GridNode {
    pub(crate) key: String,
    /// The column (the rule's / the server's `wave`).
    pub(crate) wave: usize,
    /// The row inside the column.
    pub(crate) lane: usize,
}

/// EXP-981 — one edge of the shared grid, between two [`GridNode::key`]s.
pub(crate) struct GridEdge {
    pub(crate) from: String,
    pub(crate) to: String,
    /// EXP-983: what the line SAYS — the ONE rule every client paints by
    /// ([`domain::workflow_view::workflow_edge_style`]). The blocks
    /// mini-graph only ever uses `Plain` and `Cycle`.
    pub(crate) style: WorkflowEdgeStyle,
}

/// The dash and the gap a SPECULATIVE edge is drawn with: the dependent
/// started before its blocker landed, so the line is not solid yet.
const DASH: f32 = 5.;
const DASH_GAP: f32 = 4.;
/// How finely a curved edge is sampled before it is dashed.
const CURVE_STEPS: usize = 24;

/// How a host paints ONE box: its node and whether any of its edges is a
/// cycle edge (the shared red rule).
pub(crate) type RenderGridNode<'a> = &'a dyn Fn(&GridNode, bool, &App) -> gpui::AnyElement;

/// The dash segments along one edge: the curve (or the straight line) is
/// sampled, then walked, alternating [`DASH`] of ink with [`DASH_GAP`] of
/// nothing. gpui paints paths, not patterns, so the dashes ARE short paths.
fn dashes(
    start: gpui::Point<Pixels>,
    end: gpui::Point<Pixels>,
    controls: Option<(gpui::Point<Pixels>, gpui::Point<Pixels>)>,
) -> Vec<(gpui::Point<Pixels>, gpui::Point<Pixels>)> {
    let at = |t: f32| match controls {
        // The cubic the solid edges curve along, sampled.
        Some((first, second)) => {
            let inverse = 1. - t;
            let blend = |p0: Pixels, p1: Pixels, p2: Pixels, p3: Pixels| {
                inverse * inverse * inverse * f32::from(p0)
                    + 3. * inverse * inverse * t * f32::from(p1)
                    + 3. * inverse * t * t * f32::from(p2)
                    + t * t * t * f32::from(p3)
            };
            point(
                px(blend(start.x, first.x, second.x, end.x)),
                px(blend(start.y, first.y, second.y, end.y)),
            )
        }
        None => point(
            px(f32::from(start.x) + (f32::from(end.x) - f32::from(start.x)) * t),
            px(f32::from(start.y) + (f32::from(end.y) - f32::from(start.y)) * t),
        ),
    };
    let steps = if controls.is_some() { CURVE_STEPS } else { 1 };
    let points: Vec<gpui::Point<Pixels>> = (0..=steps)
        .map(|step| at(step as f32 / steps as f32))
        .collect();

    let mut dashes = Vec::new();
    // How far into the current dash (or gap) the walk stands.
    let mut drawn = 0.;
    let mut inking = true;
    let mut dash_start = points[0];
    for window in points.windows(2) {
        let (from, to) = (window[0], window[1]);
        let dx = f32::from(to.x) - f32::from(from.x);
        let dy = f32::from(to.y) - f32::from(from.y);
        let length = (dx * dx + dy * dy).sqrt();
        if length <= f32::EPSILON {
            continue;
        }
        let mut walked = 0.;
        while walked < length {
            let want = if inking { DASH } else { DASH_GAP } - drawn;
            let step = want.min(length - walked);
            let t = (walked + step) / length;
            let at = point(px(f32::from(from.x) + dx * t), px(f32::from(from.y) + dy * t));
            if inking && step + drawn >= DASH - f32::EPSILON {
                dashes.push((dash_start, at));
            }
            walked += step;
            drawn += step;
            if drawn >= if inking { DASH } else { DASH_GAP } - f32::EPSILON {
                inking = !inking;
                drawn = 0.;
                dash_start = at;
            }
        }
    }
    if inking && drawn > 0. {
        dashes.push((dash_start, *points.last().expect("sampled")));
    }
    dashes
}

/// EXP-981 — the shared grid: boxes at their `(wave, lane)`, the edges
/// between them painted in one canvas pass (grey, RED on a cycle), and the
/// host's notes underneath. Nothing here decides an order or a position.
pub(crate) fn grid_view(
    nodes: &[GridNode],
    edges: &[GridEdge],
    geometry: GridGeometry,
    notes: &[SharedString],
    render_node: RenderGridNode<'_>,
    cx: &App,
) -> gpui::AnyElement {
    let theme = cx.theme();
    let muted = theme.muted_foreground;
    let danger = theme.danger;
    let success = theme.success;
    let grey = theme::tokens::glass::STROKE_STRONG.to_hsla();

    let waves = nodes.iter().map(|node| node.wave).max().unwrap_or(0);
    let lanes = nodes.iter().map(|node| node.lane).max().unwrap_or(0);
    let width = geometry.x(waves) + geometry.node_w;
    let height = geometry.y(lanes) + geometry.node_h;

    // Where each node sits, so the edges can be painted in one pass.
    let places: HashMap<&str, (usize, usize)> = nodes
        .iter()
        .map(|node| (node.key.as_str(), (node.wave, node.lane)))
        .collect();
    // A node is on a cycle when one of its edges is.
    let mut on_cycle: HashMap<&str, bool> = HashMap::new();
    // `(x1, y1, x2, y2, gap start, gap end, style)`.
    let segments: Vec<(f32, f32, f32, f32, f32, f32, WorkflowEdgeStyle)> = edges
        .iter()
        .filter_map(|edge| {
            let from = places.get(edge.from.as_str())?;
            let to = places.get(edge.to.as_str())?;
            if edge.style == WorkflowEdgeStyle::Cycle {
                on_cycle.insert(edge.from.as_str(), true);
                on_cycle.insert(edge.to.as_str(), true);
            }
            Some((
                geometry.x(from.0) + geometry.edge_out.0,
                geometry.y(from.1) + geometry.edge_out.1,
                geometry.x(to.0) + geometry.edge_in.0,
                geometry.y(to.1) + geometry.edge_in.1,
                geometry.x(from.0) + geometry.node_w,
                geometry.x(to.0),
                edge.style,
            ))
        })
        .collect();

    let painted = div()
        .absolute()
        .top_0()
        .left_0()
        .w(px(width))
        .h(px(height))
        .child(
            canvas(|_, _, _| (), move |bounds: Bounds<Pixels>, _, window, _| {
                let origin = bounds.origin;
                for &(x1, y1, x2, y2, gap_start, gap_end, style) in &segments {
                    // EXP-983: red for a cycle AND for upstream that moved,
                    // green once the blocker landed, grey otherwise.
                    let color = match style {
                        WorkflowEdgeStyle::Cycle | WorkflowEdgeStyle::Stale => danger,
                        WorkflowEdgeStyle::Landed => success,
                        _ => grey,
                    };
                    let at = |x: f32, y: f32| point(origin.x + px(x), origin.y + px(y));
                    let (start, end) = (at(x1, y1), at(x2, y2));
                    // The curve lives in the GAP between two cells: a level
                    // stub runs from the anchor to its cell's edge first, so
                    // an edge never cuts through a label beside (or under)
                    // its anchor. It leaves and arrives HORIZONTALLY, both
                    // control points on the gap's middle, so a fan of edges
                    // gathers into one bus instead of hooking at one end. A
                    // backwards (cycle) edge has no gap and curves anchor to
                    // anchor.
                    let (c1, c2) = if gap_end > gap_start {
                        (gap_start, gap_end)
                    } else {
                        (x1, x2)
                    };
                    let (curve_from, curve_to) = (at(c1, y1), at(c2, y2));
                    let middle = (c1 + c2) / 2.;
                    let controls = (at(middle, y1), at(middle, y2));
                    let curved = (y1 - y2).abs() >= 0.5;
                    if style == WorkflowEdgeStyle::Speculative {
                        // gpui's PathBuilder dashes per path, so the dashes
                        // are painted as short segments along the same route.
                        let route = [
                            (start, curve_from, None),
                            (curve_from, curve_to, curved.then_some(controls)),
                            (curve_to, end, None),
                        ];
                        for (from, to, controls) in route {
                            for (from, to) in dashes(from, to, controls) {
                                let mut path = gpui::PathBuilder::stroke(px(LINE));
                                path.move_to(from);
                                path.line_to(to);
                                if let Ok(path) = path.build() {
                                    window.paint_path(path, color);
                                }
                            }
                        }
                        continue;
                    }
                    let mut path = gpui::PathBuilder::stroke(px(LINE));
                    path.move_to(start);
                    if curved {
                        path.line_to(curve_from);
                        path.cubic_bezier_to(curve_to, controls.0, controls.1);
                    }
                    path.line_to(end);
                    if let Ok(path) = path.build() {
                        window.paint_path(path, color);
                    }
                }
            })
            .size_full(),
        );

    let mut grid = div().relative().w(px(width)).h(px(height)).child(painted);
    for node in nodes {
        let cycled = on_cycle.get(node.key.as_str()).copied().unwrap_or(false);
        grid = grid.child(
            div()
                .absolute()
                .left(px(geometry.x(node.wave)))
                .top(px(geometry.y(node.lane)))
                .w(px(geometry.node_w))
                .h(px(geometry.node_h))
                .child(render_node(node, cycled, cx)),
        );
    }

    v_flex()
        .min_w_0()
        .gap_1p5()
        .child(
            div()
                .id("exp-graph-scroll")
                .w(px(geometry.view_w.min(width)))
                .h(px(geometry.view_h.min(height)))
                .overflow_scroll()
                .child(grid),
        )
        .children(notes.iter().map(|note| {
            div()
                .text_xs()
                .text_color(muted)
                .child(note.clone())
                .into_any_element()
        }))
        .into_any_element()
}

/// The whole synced issue set as the rule reads it, plus the relation rows.
/// Every caller needs both, and both are plain collection reads.
pub(crate) fn graph_for(subject_ids: &[&str], cx: &App) -> IssueGraph {
    let Some(store) = sync::Store::try_global(cx) else {
        return IssueGraph::default();
    };
    let collections = store.collections();
    let issues = collections.issues.read(cx);
    let relations = collections.issue_relations.read(cx);
    let graph_issues: Vec<GraphIssue<'_>> = issues
        .iter()
        .map(|issue| GraphIssue {
            id: &issue.id,
            identifier: &issue.identifier,
            status: issue.status.as_wire().unwrap_or_default(),
        })
        .collect();
    let graph_relations: Vec<GraphRelation<'_>> = relations
        .iter()
        .map(|row| GraphRelation {
            kind: row.kind.as_deref().unwrap_or_default(),
            issue_id: &row.issue_id,
            related_issue_id: &row.related_issue_id,
        })
        .collect();
    block_graph(subject_ids, &graph_relations, &graph_issues)
}

/// EXP-981 — one issue's badge numbers, for the bulk bar's "Start as stack"
/// gate (an issue with nothing blocking it has no stack to cut into).
pub(crate) fn block_counts_for(issue_id: &str, cx: &App) -> BlockCounts {
    let Some(store) = sync::Store::try_global(cx) else {
        return BlockCounts::default();
    };
    let collections = store.collections();
    let issues = collections.issues.read(cx);
    let relations = collections.issue_relations.read(cx);
    let graph_issues: Vec<GraphIssue<'_>> = issues
        .iter()
        .map(|issue| GraphIssue {
            id: &issue.id,
            identifier: &issue.identifier,
            status: issue.status.as_wire().unwrap_or_default(),
        })
        .collect();
    let graph_relations: Vec<GraphRelation<'_>> = relations
        .iter()
        .map(|row| GraphRelation {
            kind: row.kind.as_deref().unwrap_or_default(),
            issue_id: &row.issue_id,
            related_issue_id: &row.related_issue_id,
        })
        .collect();
    domain::issue_graph::block_counts(&graph_relations, &graph_issues)
        .get(issue_id)
        .copied()
        .unwrap_or_default()
}

/// EXP-980 — the OPEN issues that block any of `picked` from OUTSIDE the set,
/// by identifier: what a start (one issue or a batch) has to ask about.
pub(crate) fn open_blockers_outside(picked: &[&str], cx: &App) -> Vec<String> {
    let Some(store) = sync::Store::try_global(cx) else {
        return Vec::new();
    };
    let collections = store.collections();
    let issues = collections.issues.read(cx);
    let relations = collections.issue_relations.read(cx);
    let graph_issues: Vec<GraphIssue<'_>> = issues
        .iter()
        .map(|issue| GraphIssue {
            id: &issue.id,
            identifier: &issue.identifier,
            status: issue.status.as_wire().unwrap_or_default(),
        })
        .collect();
    let graph_relations: Vec<GraphRelation<'_>> = relations
        .iter()
        .map(|row| GraphRelation {
            kind: row.kind.as_deref().unwrap_or_default(),
            issue_id: &row.issue_id,
            related_issue_id: &row.related_issue_id,
        })
        .collect();
    open_blockers_of_set(picked, &graph_relations, &graph_issues)
        .into_iter()
        .map(|issue| issue.identifier.to_string())
        .collect()
}

/// The graph inside a DIALOG: a node tap closes the dialog first, then opens
/// the issue in the window that opened it (a dialog window has no navigation
/// of its own).
pub(crate) fn graph_in_dialog(
    graph: &IssueGraph,
    view_width: f32,
    cx: &App,
) -> gpui::AnyElement {
    let on_pick: OnPickIssue = Rc::new(|issue_id: &str, window, cx| {
        let issue_id = issue_id.to_string();
        crate::native_dialog::close_then(window, cx, move |window, cx| {
            crate::navigation::navigate(
                window,
                cx,
                crate::navigation::Screen::IssueDetail { issue_id },
            );
        });
    });
    graph_view(graph, view_width, on_pick, cx)
}

/// The grid plus its notes, for the EXP-980 `blocks` graph. Empty (no nodes)
/// renders nothing at all — the callers gate on that themselves. The pixels
/// are [`grid_view`]'s; this only translates the rule's output into its
/// node-keyed input and picks the ring each node's role earns.
pub(crate) fn graph_view(
    graph: &IssueGraph,
    view_width: f32,
    on_pick: OnPickIssue,
    cx: &App,
) -> gpui::AnyElement {
    let ring = cx.theme().ring;
    let danger = cx.theme().danger;
    let nodes: Vec<GridNode> = graph
        .nodes
        .iter()
        .map(|node| GridNode {
            key: node.id.clone(),
            wave: node.wave,
            lane: node.lane,
        })
        .collect();
    let edges: Vec<GridEdge> = graph
        .edges
        .iter()
        .map(|edge| GridEdge {
            from: edge.from.clone(),
            to: edge.to.clone(),
            // The blocks mini-graph has no run to report: red on a cycle,
            // grey otherwise.
            style: if edge.cycle {
                WorkflowEdgeStyle::Cycle
            } else {
                WorkflowEdgeStyle::Plain
            },
        })
        .collect();
    let subjects: std::collections::HashSet<&str> = graph
        .nodes
        .iter()
        .filter(|node| node.subject)
        .map(|node| node.id.as_str())
        .collect();
    let mut notes: Vec<SharedString> = Vec::new();
    if graph.has_cycle {
        notes.push(SharedString::from(ISSUE_GRAPH_CYCLE_NOTE));
    }
    if graph.truncated {
        notes.push(SharedString::from(ISSUE_GRAPH_TRUNCATED_NOTE));
    }
    let render = |node: &GridNode, cycled: bool, cx: &App| {
        let outline: Option<Hsla> = if cycled {
            Some(danger)
        } else if subjects.contains(node.key.as_str()) {
            Some(ring)
        } else {
            None
        };
        node_chip(&node.key, outline, on_pick.clone(), cx)
    };
    grid_view(&nodes, &edges, GridGeometry::boxes(view_width), &notes, &render, cx)
}

/// One node: the shared issue chip (status glyph + identifier + as much title
/// as the box holds), inside the ring its role earns.
fn node_chip(
    issue_id: &str,
    outline: Option<Hsla>,
    on_pick: OnPickIssue,
    cx: &App,
) -> gpui::AnyElement {
    let row = sync::Store::try_global(cx)
        .and_then(|store| store.collections().issues.read(cx).get(issue_id).cloned());
    let identifier = row
        .as_ref()
        .map(|issue| issue.identifier.clone())
        .unwrap_or_else(|| issue_id.to_string());
    let title = row
        .as_ref()
        .map(|issue| issue.title.clone())
        .unwrap_or_default();
    let target = issue_id.to_string();
    let mut chip = issue_chip(
        SharedString::from(format!("issue-graph-node-{issue_id}")),
        identifier,
        title,
    )
    .flexible()
    .on_click(move |_: &ClickEvent, window, cx| on_pick(&target, window, cx));
    if let Some(status) = crate::issue_chip::synced_issue_status(issue_id, cx) {
        chip = chip.status(status);
    }
    div()
        .w_full()
        .h_full()
        .flex()
        .items_center()
        .rounded(px(6.))
        .border_1()
        .border_color(outline.unwrap_or_else(gpui::transparent_black))
        .child(chip)
        .into_any_element()
}

/// EXP-980 — the list row's blocks pill: the blocked-by count in the
/// destructive tone, the blocking count muted, and the two together when a
/// row is in both directions. `None` when nothing blocks and nothing is
/// blocked. Clicking it opens the mini-graph for that issue; the click never
/// reaches the row underneath.
pub(crate) fn blocks_badge(
    id: SharedString,
    issue_id: &str,
    counts: BlockCounts,
    cx: &App,
) -> Option<gpui::AnyElement> {
    if counts.blocked_by == 0 && counts.blocking == 0 {
        return None;
    }
    let theme = cx.theme();
    let label = blocks_badge_label(counts);
    let mut pill = glass_pill_button(id.clone(), PillSize::Sm, cx).tooltip(SharedString::from(
        label.clone(),
    ));
    if counts.blocked_by > 0 {
        pill = pill
            .icon(
                Icon::new(registry::RELATION_BLOCKED_BY)
                    .with_size(px(PillSize::Sm.glyph()))
                    .text_color(theme.danger),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(theme.danger)
                    .child(SharedString::from(counts.blocked_by.to_string()))
                    .into_any_element(),
            );
    }
    if counts.blocking > 0 {
        let glyph = Icon::new(registry::RELATION_BLOCKS)
            .with_size(px(PillSize::Sm.glyph()))
            .text_color(theme.muted_foreground);
        pill = if counts.blocked_by > 0 {
            pill.child(glyph.into_any_element())
        } else {
            pill.icon(glyph)
        };
        pill = pill.child(
            div()
                .text_xs()
                .text_color(theme.muted_foreground)
                .child(SharedString::from(counts.blocking.to_string()))
                .into_any_element(),
        );
    }
    let subject = issue_id.to_string();
    Some(
        // The pill is a control inside a clickable row: swallow the click so
        // opening the graph never opens the issue as well.
        div()
            .id(SharedString::from(format!("{id}-cell")))
            .flex_shrink_0()
            .on_click(|_, _, cx| cx.stop_propagation())
            .child(
                gpui_component::popover::Popover::new(SharedString::from(format!("{id}-popover")))
                    .p_2()
                    .trigger(pill)
                    .content(move |_, _, cx| {
                        let graph = graph_for(&[subject.as_str()], cx);
                        graph_overlay(&graph, VIEW_W, cx)
                    }),
            )
            .into_any_element(),
    )
}

/// The popover body: the graph, navigating in the window it is drawn in.
pub(crate) fn graph_overlay(graph: &IssueGraph, view_width: f32, cx: &App) -> gpui::AnyElement {
    let on_pick: OnPickIssue = Rc::new(|issue_id: &str, window, cx| {
        crate::navigation::navigate(
            window,
            cx,
            crate::navigation::Screen::IssueDetail {
                issue_id: issue_id.to_string(),
            },
        );
    });
    h_flex()
        .min_w_0()
        .child(graph_view(graph, view_width, on_pick, cx))
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// EXP-983 — a speculative edge is DASHED: gpui paints paths, not
    /// patterns, so the line becomes a run of short segments that still
    /// starts where the edge starts and ends where it ends.
    #[test]
    fn a_dashed_edge_is_a_run_of_segments_along_the_line() {
        let start = point(px(0.), px(0.));
        let end = point(px(40.), px(0.));
        let straight = dashes(start, end, None);
        assert!(straight.len() > 2, "40px of line is several dashes");
        assert_eq!(straight[0].0, start, "the first dash starts on the edge");
        let last = straight.last().expect("dashes");
        assert!(
            f32::from(last.1.x) <= 40.,
            "no dash runs past the end: {:?}",
            last.1
        );
        for (from, to) in &straight {
            let length = f32::from(to.x) - f32::from(from.x);
            assert!(length > 0. && length <= DASH + 0.01, "dash of {length}px");
        }

        // A curved edge is sampled first, so its dashes leave the straight
        // line between the two ends.
        let curved = dashes(
            start,
            point(px(40.), px(40.)),
            Some((point(px(20.), px(0.)), point(px(20.), px(40.)))),
        );
        assert!(curved.len() > 2);
        assert!(
            curved
                .iter()
                .any(|(from, _)| f32::from(from.y) > 0. && f32::from(from.x) > 0.),
            "the dashes follow the curve, not the chord"
        );

        // A zero-length edge simply draws nothing.
        assert!(dashes(start, start, None).is_empty());
    }
}
