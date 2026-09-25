//! EXP-998 / EXP-1057 — painting [`domain::issue_rail`] over an issue list:
//! the blocks RAIL at the right edge of the big list and of the `ListNav`
//! column (web `components/issue-rail.tsx`, the same geometry).
//!
//! The rail draws DOTS only (EXP-1057 dropped the lanes and arrows): a small
//! dot flush with the row's right edge — a ring in the danger tone when
//! something open is in the issue's way, a filled muted dot when the issue
//! only blocks others. HOVERING a dot opens the mini-graph beside it: a
//! [`HoverCard`] with a short open delay (a pointer sweeping down the rail
//! opens nothing) and a grace close delay, so the pointer can travel from the
//! dot into the card and stay there; leaving both closes it. The card hangs
//! off the dot's top-RIGHT corner and grows LEFTWARDS, so a dot at the
//! window's right edge never pushes it off screen.

use std::time::Duration;

use gpui::{div, prelude::*, px, Anchor, AnyElement, App, SharedString, Styled};
use gpui_component::hover_card::HoverCard;
use gpui_component::ActiveTheme as _;

use domain::issue_graph::{blocks_badge_label, geometry};
use domain::issue_rail::{RailNode, RailRow, RAIL_NODE_WIDTH};

use crate::issue_graph::{graph_for, graph_overlay, VIEW_W};

/// How long the pointer rests on a dot before the graph opens.
const OPEN_DELAY: Duration = Duration::from_millis(120);
/// The grace a pointer gets to cross from the dot into the card (and back).
const CLOSE_DELAY: Duration = Duration::from_millis(180);

/// A row's node: the dot at the rail's right edge, the mini-graph's hover
/// trigger. `None` when the row has no open relation.
pub(crate) fn rail_node(
    id: impl Into<SharedString>,
    issue_id: &str,
    row: &RailRow,
    right_pad: f32,
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
    let foreground = theme.foreground;
    let dot = div()
        .id(SharedString::from(format!("{id}-dot")))
        .size(px(geometry::RAIL_DOT))
        .rounded_full()
        .border(px(geometry::RAIL_DOT_RING))
        .border_color(border)
        .bg(fill)
        .hover(move |style| match node {
            RailNode::Blocked => style.border_color(lighten(border)),
            RailNode::Blocking => style.border_color(foreground).bg(foreground),
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
            // A control inside a clickable row: a click never opens the
            // issue underneath (the hover already opened the graph).
            .on_click(|_, _, cx| cx.stop_propagation())
            .child(
                HoverCard::new(SharedString::from(format!("{id}-graph")))
                    .anchor(Anchor::TopRight)
                    .open_delay(OPEN_DELAY)
                    .close_delay(CLOSE_DELAY)
                    .p_2()
                    .trigger(dot)
                    .content(move |_, _, cx| {
                        let graph = graph_for(&[subject.as_str()], cx);
                        gpui_component::v_flex()
                            .gap_1p5()
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(cx.theme().muted_foreground)
                                    .child(label.clone()),
                            )
                            .child(graph_overlay(&graph, VIEW_W, cx))
                    }),
            )
            .into_any_element(),
    )
}

fn lighten(base: gpui::Hsla) -> gpui::Hsla {
    gpui::Hsla {
        l: (base.l + 0.12).min(1.),
        ..base
    }
}
