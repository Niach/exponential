//! EXP-998 — painting [`domain::issue_rail`] over an issue list: the blocks
//! RAIL at the right edge of the big list and of the `ListNav` column (web
//! `components/issue-rail.tsx`, the same geometry).
//!
//! AT REST only the nodes show: a small dot flush with the row's right edge
//! — a ring in the danger tone when something open is in the issue's way, a
//! filled muted dot when the issue only blocks others. That is the hint.
//! While the pointer is over the rail (a dot, or the strip any row reserves
//! for it) the host view flags itself open and the arrows appear to the
//! LEFT of the dots: every edge leaves its blocker's node on a smooth
//! S-curve out to its lane, runs the lane straight, and curves back into the
//! blocked node under an arrowhead; the edges of the hovered node go
//! foreground. A click on a dot opens the mini-graph popover the counts pill
//! used to.
//!
//! Every entry paints only inside itself (the lists are virtual), so a lane
//! that crosses a group header is the header's own slice — and a list that
//! spaces its rows starts its top-edge segments one gap higher, the tree
//! connector's bridging rule (`tree_guides.rs`).

use gpui::{
    canvas, div, point, prelude::*, px, size, AnyElement, App, Bounds, ElementId, Hsla,
    InteractiveElement, ParentElement, Pixels, SharedString, Styled, Window,
};
use gpui_component::ActiveTheme as _;

use domain::issue_graph::blocks_badge_label;
use domain::issue_rail::{rail_lane_x, rail_node_x, BlockEdge, RailNode, RailRow, RAIL_NODE_WIDTH};

use crate::issue_graph::{graph_for, graph_overlay, VIEW_W};

/// The strokes: the lane's, and the hovered edge's.
const LINE: f32 = 1.5;
const HOT_LINE: f32 = 2.;
/// How far a curve stops short of the node's centre: the dot's radius plus
/// a hairline.
const NODE_INSET: f32 = 6.;
/// The arrowhead: length along the line and half its height.
const ARROW_LENGTH: f32 = 7.;
const ARROW_HALF: f32 = 4.;
/// The node dot's diameter and ring.
const DOT: f32 = 10.;
const RING: f32 = 2.;

/// What one entry's rail paints with.
pub(crate) struct RailPaint<'a> {
    pub row: &'a RailRow,
    pub edges: &'a [BlockEdge],
    /// The rail column's width ([`domain::issue_rail::IssueRail::width`]).
    pub width: f32,
    /// The row's own right padding: the layer sits inside it, flush with
    /// the content edge.
    pub right_pad: f32,
    /// The list's row spacing (0 for flush lists): top-edge segments start
    /// that much above the row.
    pub gap: f32,
    /// The arrows show: the pointer is on the rail.
    pub open: bool,
    /// The issue whose node is hovered — its edges go foreground.
    pub hot: Option<&'a str>,
}

/// The lanes of one entry, or `None` when there is nothing to draw (no lane
/// at this entry, or the rail is closed). Absolute and paint-only.
pub(crate) fn rail_lanes_layer(paint: &RailPaint<'_>, cx: &App) -> Option<AnyElement> {
    if !paint.open || paint.row.lanes.is_empty() || paint.width <= 0. {
        return None;
    }
    let theme = cx.theme();
    let ink = theme.muted_foreground.opacity(0.45);
    let head = theme.muted_foreground.opacity(0.8);
    let hot_ink = theme.foreground;
    let danger = theme.danger;
    let width = paint.width;
    let gap = paint.gap.max(0.);
    let hot_owner = paint.hot.map(str::to_string);
    let lanes: Vec<(domain::issue_rail::RailLane, bool)> = paint
        .row
        .lanes
        .iter()
        .map(|lane| {
            let hot = hot_owner.as_deref().is_some_and(|owner| {
                lane.edges
                    .iter()
                    .filter_map(|&index| paint.edges.get(index))
                    .any(|edge| edge.from == owner || edge.to == owner)
            });
            (lane.clone(), hot)
        })
        .collect();
    Some(
        div()
            .absolute()
            .top_0()
            .bottom_0()
            .right(px(paint.right_pad))
            .w(px(width))
            .child(
                canvas(|_, _, _| (), move |bounds: Bounds<Pixels>, _, window, _| {
                    let left = f32::from(bounds.origin.x);
                    let top = f32::from(bounds.origin.y) - gap;
                    let bottom = f32::from(bounds.origin.y + bounds.size.height);
                    let mid = f32::from(bounds.origin.y) + f32::from(bounds.size.height) / 2.;
                    let half = f32::from(bounds.size.height) / 2.;
                    let node_x = left + rail_node_x(width);
                    let node_edge = node_x - NODE_INSET;
                    for (lane, hot) in &lanes {
                        let x = left + rail_lane_x(lane.lane, width);
                        let (color, line) = if *hot {
                            (hot_ink, HOT_LINE)
                        } else if lane.cycle {
                            (danger, LINE)
                        } else {
                            (ink, LINE)
                        };
                        let vertical = |window: &mut Window, from: f32, to: f32| {
                            if to <= from {
                                return;
                            }
                            window.paint_quad(gpui::fill(
                                Bounds::new(
                                    point(px(x - line / 2.), px(from)),
                                    size(px(line), px(to - from)),
                                ),
                                color,
                            ));
                        };
                        // The S-curve between the lane at the row's edge and
                        // the node's left side: vertical tangent at the lane,
                        // horizontal at the node.
                        let curve = |window: &mut Window, edge: f32| {
                            let mut path = gpui::PathBuilder::stroke(px(line));
                            let bend = edge * 0.55;
                            path.move_to(point(px(x), px(mid + edge)));
                            path.cubic_bezier_to(
                                point(px(node_edge), px(mid)),
                                point(px(x), px(mid + bend)),
                                point(px(node_edge - (node_edge - x) * 0.45), px(mid)),
                            );
                            if let Ok(path) = path.build() {
                                window.paint_path(path, color);
                            }
                        };
                        if lane.join_above {
                            curve(window, -half);
                        } else if lane.top {
                            vertical(window, top, if lane.bottom { bottom } else { mid });
                        }
                        if lane.join_below {
                            curve(window, half);
                        } else if lane.bottom && !lane.top {
                            vertical(window, mid, bottom);
                        }
                        if lane.into {
                            // The arrowhead points RIGHT, its tip on the
                            // node's left edge.
                            let tip = node_edge + 1.;
                            let fill: Hsla = if *hot {
                                hot_ink
                            } else if lane.cycle {
                                danger
                            } else {
                                head
                            };
                            let mut path = gpui::PathBuilder::fill();
                            path.add_polygon(
                                &[
                                    point(px(tip), px(mid)),
                                    point(px(tip - ARROW_LENGTH), px(mid - ARROW_HALF)),
                                    point(px(tip - ARROW_LENGTH * 0.7), px(mid)),
                                    point(px(tip - ARROW_LENGTH), px(mid + ARROW_HALF)),
                                ],
                                true,
                            );
                            if let Ok(path) = path.build() {
                                window.paint_path(path, fill);
                            }
                        }
                    }
                })
                .size_full(),
            )
            .into_any_element(),
    )
}

/// The strip a row reserves for the rail: an absolute, full-height band the
/// rail's width, whose hover opens the arrows. Hosts put it BEHIND the node
/// (the node is its own element on top). `None` when the list has no rail.
pub(crate) fn rail_strip(
    id: impl Into<ElementId>,
    width: f32,
    right_pad: f32,
    on_hover: impl Fn(&bool, &mut Window, &mut App) + 'static,
) -> Option<AnyElement> {
    if width <= 0. {
        return None;
    }
    Some(
        div()
            .id(id)
            .absolute()
            .top_0()
            .bottom_0()
            .right(px(right_pad))
            .w(px(width))
            .on_hover(on_hover)
            .into_any_element(),
    )
}

/// A row's node: the dot at the rail's right edge, the mini-graph popover's
/// trigger. Hover hands the host the issue id (or `None` on leave) so it can
/// light that node's edges; the click is the popover's. `None` when the row
/// has no open relation.
pub(crate) fn rail_node(
    id: impl Into<SharedString>,
    issue_id: &str,
    row: &RailRow,
    right_pad: f32,
    on_hover: impl Fn(&bool, &mut Window, &mut App) + 'static,
    cx: &App,
) -> Option<AnyElement> {
    let node = row.node?;
    let id: SharedString = id.into();
    let theme = cx.theme();
    let label = SharedString::from(blocks_badge_label(row.counts));
    let (border, fill) = match node {
        RailNode::Blocked => (theme.danger, theme.background),
        RailNode::Blocking => (theme.muted_foreground, theme.muted_foreground),
    };
    let dot = div()
        .id(SharedString::from(format!("{id}-dot")))
        .size(px(DOT))
        .rounded_full()
        .border(px(RING))
        .border_color(border)
        .bg(fill)
        .hover(move |style| match node {
            RailNode::Blocked => style.border_color(danger_hover(border)),
            RailNode::Blocking => style.border_color(theme.foreground).bg(theme.foreground),
        })
        .tooltip({
            let label = label.clone();
            move |window, cx| gpui_component::tooltip::Tooltip::new(label.clone()).build(window, cx)
        });
    let subject = issue_id.to_string();
    Some(
        div()
            .id(SharedString::from(format!("{id}-cell")))
            .absolute()
            .top_0()
            .bottom_0()
            .right(px(right_pad))
            .w(px(RAIL_NODE_WIDTH))
            .flex()
            .items_center()
            .justify_center()
            .cursor_pointer()
            .on_hover(on_hover)
            // A control inside a clickable row: the click opens the graph,
            // never the issue underneath.
            .on_click(|_, _, cx| cx.stop_propagation())
            .child(
                gpui_component::popover::Popover::new(SharedString::from(format!("{id}-popover")))
                    .p_2()
                    .trigger(RailDot { dot: Some(dot.into_any_element()) })
                    .content(move |_, _, cx| {
                        let graph = graph_for(&[subject.as_str()], cx);
                        graph_overlay(&graph, VIEW_W, cx)
                    }),
            )
            .into_any_element(),
    )
}

fn danger_hover(base: Hsla) -> Hsla {
    Hsla {
        l: (base.l + 0.12).min(1.),
        ..base
    }
}

/// The popover trigger wrapper: `Popover::trigger` wants a `Selectable`,
/// which a plain dot is not.
struct RailDot {
    dot: Option<AnyElement>,
}

impl gpui_component::Selectable for RailDot {
    fn selected(self, _selected: bool) -> Self {
        self
    }

    fn is_selected(&self) -> bool {
        false
    }
}

impl gpui::IntoElement for RailDot {
    type Element = AnyElement;

    fn into_element(mut self) -> Self::Element {
        self.dot.take().unwrap_or_else(|| div().into_any_element())
    }
}
