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

use std::collections::HashMap;
use std::rc::Rc;

use gpui::{
    canvas, div, point, px, App, Bounds, ClickEvent, Hsla, InteractiveElement as _, IntoElement,
    ParentElement, Pixels, SharedString, StatefulInteractiveElement as _, Styled, Window,
};
use gpui::prelude::FluentBuilder as _;
use gpui_component::{h_flex, v_flex, ActiveTheme as _, Icon, Sizable as _};

use domain::issue_graph::{
    block_graph, blocks_badge_label, open_blockers_of_set, BlockCounts, GraphIssue, GraphRelation,
    IssueGraph, ISSUE_GRAPH_CYCLE_NOTE, ISSUE_GRAPH_TRUNCATED_NOTE,
};

use crate::icons::registry;
use crate::issue_chip::issue_chip;
use crate::surface::{glass_pill_button, PillSize};

/// One node's box. Wide enough for an identifier and a clipped title.
const NODE_W: f32 = 176.;
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

fn node_x(wave: usize) -> f32 {
    wave as f32 * (NODE_W + COL_GAP)
}

fn node_y(lane: usize) -> f32 {
    lane as f32 * (NODE_H + LANE_GAP)
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

/// The grid plus its notes. Empty (no nodes) renders nothing at all — the
/// callers gate on that themselves.
pub(crate) fn graph_view(
    graph: &IssueGraph,
    view_width: f32,
    on_pick: OnPickIssue,
    cx: &App,
) -> gpui::AnyElement {
    let theme = cx.theme();
    let muted = theme.muted_foreground;
    let danger = theme.danger;
    let grey = theme::tokens::glass::STROKE_STRONG.to_hsla();

    let waves = graph.nodes.iter().map(|node| node.wave).max().unwrap_or(0);
    let lanes = graph.nodes.iter().map(|node| node.lane).max().unwrap_or(0);
    let width = node_x(waves) + NODE_W;
    let height = node_y(lanes) + NODE_H;

    // Where each node sits, so the edges can be painted in one pass.
    let places: HashMap<&str, (usize, usize)> = graph
        .nodes
        .iter()
        .map(|node| (node.id.as_str(), (node.wave, node.lane)))
        .collect();
    // An issue is on a cycle when one of its edges is.
    let mut on_cycle: HashMap<&str, bool> = HashMap::new();
    let segments: Vec<(f32, f32, f32, f32, bool)> = graph
        .edges
        .iter()
        .filter_map(|edge| {
            let from = places.get(edge.from.as_str())?;
            let to = places.get(edge.to.as_str())?;
            if edge.cycle {
                on_cycle.insert(edge.from.as_str(), true);
                on_cycle.insert(edge.to.as_str(), true);
            }
            Some((
                node_x(from.0) + NODE_W,
                node_y(from.1) + NODE_H / 2.,
                node_x(to.0),
                node_y(to.1) + NODE_H / 2.,
                edge.cycle,
            ))
        })
        .collect();

    let edges = div()
        .absolute()
        .top_0()
        .left_0()
        .w(px(width))
        .h(px(height))
        .child(
            canvas(|_, _, _| (), move |bounds: Bounds<Pixels>, _, window, _| {
                let origin = bounds.origin;
                for &(x1, y1, x2, y2, cycle) in &segments {
                    let color = if cycle { danger } else { grey };
                    let start = point(origin.x + px(x1), origin.y + px(y1));
                    let end = point(origin.x + px(x2), origin.y + px(y2));
                    let mut path = gpui::PathBuilder::stroke(px(LINE));
                    path.move_to(start);
                    if (y1 - y2).abs() < 0.5 {
                        path.line_to(end);
                    } else {
                        // One quarter of the gap either side keeps the curve
                        // inside the corridor between the two waves.
                        path.curve_to(end, point(origin.x + px((x1 + x2) / 2.), start.y));
                    }
                    if let Ok(path) = path.build() {
                        window.paint_path(path, color);
                    }
                }
            })
            .size_full(),
        );

    let mut grid = div().relative().w(px(width)).h(px(height)).child(edges);
    for node in &graph.nodes {
        let outline: Option<Hsla> = if on_cycle.get(node.id.as_str()).copied().unwrap_or(false) {
            Some(danger)
        } else if node.subject {
            Some(theme.ring)
        } else {
            None
        };
        grid = grid.child(
            div()
                .absolute()
                .left(px(node_x(node.wave)))
                .top(px(node_y(node.lane)))
                .w(px(NODE_W))
                .h(px(NODE_H))
                .child(node_chip(&node.id, outline, on_pick.clone(), cx)),
        );
    }

    let note = |text: &'static str| {
        div()
            .text_xs()
            .text_color(muted)
            .child(SharedString::from(text))
    };
    v_flex()
        .min_w_0()
        .gap_1p5()
        .child(
            div()
                .id("issue-graph-scroll")
                .w(px(view_width.min(width)))
                .h(px(VIEW_H.min(height)))
                .overflow_scroll()
                .child(grid),
        )
        .when(graph.has_cycle, |this| this.child(note(ISSUE_GRAPH_CYCLE_NOTE)))
        .when(graph.truncated, |this| {
            this.child(note(ISSUE_GRAPH_TRUNCATED_NOTE))
        })
        .into_any_element()
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
