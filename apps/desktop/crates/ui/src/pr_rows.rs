//! EXP-1248 — THE pull-request row, ×4 (web `@exp/ui` `pr-row.tsx`, fixture
//! `list-item.json`): ONE line, [ring lead · mono identifier · title · quiet
//! word]. No branch line, no PR number, no counts, no age, no inline Merge.
//! The Threads page, PR trees and the Guide's stack card all draw it (EXP-1294:
//! a Threads row swaps the ring for its own `lead`). Two shapes: a
//! TREE nests with tree guides ([`pr_list`]), a linear STACK never nests, it
//! hangs off one rail down to its base branch ([`stack_rail`]).

use std::rc::Rc;

use gpui::prelude::FluentBuilder as _;
use gpui::{
    div, px, AnyElement, App, InteractiveElement as _, IntoElement, ParentElement, SharedString,
    StatefulInteractiveElement as _, Styled, Window,
};
use gpui_component::ActiveTheme as _;

use domain::list_item::{PrNodeRing, PrNodeState, BASE, GAP, MARK, PR_NODE, PR_ROW, RAIL};

use crate::run_rows::RunRowAction;

/// The mono identifier's minimum width, so titles line up down a list (web
/// `min-w-[4.5rem]`).
const IDENTIFIER_MIN_W: f32 = 72.;

/// The ring's stroke (web `border-[1.5px]`).
const RING_STROKE: f32 = 1.5;

/// The current member's filled centre (web `size-1.5`).
const FILL: f32 = 6.;

/// The quiet word on a stack's top row ×4 (web `word="stack"`).
pub(crate) const STACK_WORD: &str = "stack";

/// EXP-1318: the merged-PR glyph's tint ×4 = the web's `text-purple-400`
/// (`pr-merged`), never the blue of a done status.
pub(crate) const PR_MERGED_TINT: theme::Srgb8 = theme::Srgb8 { r: 192, g: 132, b: 252, a: 255 };

/// A hover callback: `true` entering the row, `false` leaving it.
pub(crate) type PrRowHover = Box<dyn Fn(&bool, &mut Window, &mut App)>;

/// The ring lead: emerald ring = an open PR, filled centre = the current
/// stack member, muted ring = the base branch. A [`MARK`]-wide box like the
/// run mark, so tree guides land on its centre.
pub(crate) fn pr_node(state: PrNodeState, cx: &App) -> gpui::Div {
    let ring = match state.ring() {
        PrNodeRing::Emerald => theme::tokens::GREEN.to_hsla(),
        PrNodeRing::Muted => cx.theme().muted_foreground.opacity(0.6),
    };
    div()
        .flex_shrink_0()
        .size(px(MARK))
        .flex()
        .items_center()
        .justify_center()
        .child(
            div()
                .size(px(PR_NODE))
                .rounded_full()
                .border(px(RING_STROKE))
                .border_color(ring)
                .flex()
                .items_center()
                .justify_center()
                .when(state.filled(), |this| {
                    this.child(div().size(px(FILL)).rounded_full().bg(ring))
                }),
        )
}

/// Which stack-rail segments a row draws: into the node from above, out of
/// it below. They stop at the ring, never cross it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct PrRail {
    pub(crate) above: bool,
    pub(crate) below: bool,
}

/// Everything ONE [`pr_row`] draws.
pub(crate) struct PrRowSpec {
    pub(crate) id: SharedString,
    pub(crate) node: PrNodeState,
    /// EXP-1294: a lead drawn INSTEAD of the ring (a Threads row's PR glyph
    /// or agent mark), in the same [`MARK`]-wide box.
    pub(crate) lead: Option<AnyElement>,
    pub(crate) identifier: Option<SharedString>,
    /// The PR's issue title, or the base branch's name on a `Base` row.
    pub(crate) title: SharedString,
    /// A quiet trailing word (`stack` on a stack's top row).
    pub(crate) word: Option<SharedString>,
    /// Tree shape: the indent AND the connector ([`domain::tree_guides`]).
    pub(crate) guides: domain::tree_guides::Guides,
    /// Stack shape: the rail through the node centres.
    pub(crate) rail: PrRail,
    /// The member on screen: the active row wash.
    pub(crate) active: bool,
    /// `None` = an inert row (the base branch).
    pub(crate) on_click: Option<RunRowAction>,
    pub(crate) on_hover: Option<PrRowHover>,
    /// After the word: an external-link glyph, the ghost merge-through.
    pub(crate) trailing: Option<AnyElement>,
    /// EXP-1323: a Threads row with news the viewer has not read — the
    /// title goes semibold.
    pub(crate) strong: bool,
    /// EXP-1323: a Threads row's live caption, the muted line under the
    /// title (the row grows to [`CAPTION_ROW`]).
    pub(crate) caption: Option<SharedString>,
}

/// EXP-1323: a row carrying a caption line (web `h-[52px]`).
pub(crate) const CAPTION_ROW: f32 = 52.;

impl PrRowSpec {
    /// A plain open-PR row: no word, no nesting, no rail, nothing trailing.
    pub(crate) fn open(id: impl Into<SharedString>, title: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            node: PrNodeState::Open,
            lead: None,
            identifier: None,
            title: title.into(),
            word: None,
            guides: domain::tree_guides::Guides::default(),
            rail: PrRail::default(),
            active: false,
            on_click: None,
            on_hover: None,
            trailing: None,
            strong: false,
            caption: None,
        }
    }
}

/// The node's centre x for a row at `depth` (the rail's x).
fn node_centre(depth: usize) -> f32 {
    domain::list_item::lead_x(depth) + MARK / 2.
}

/// THE pull-request row: one line at [`PR_ROW`].
pub(crate) fn pr_row(spec: PrRowSpec, cx: &App) -> AnyElement {
    let PrRowSpec {
        id,
        node,
        lead,
        identifier,
        title,
        word,
        guides,
        rail,
        active,
        on_click,
        on_hover,
        trailing,
        strong,
        caption,
    } = spec;
    let theme = cx.theme();
    let muted = theme.muted_foreground;
    let row_hover = theme.list_hover;
    let row_active = theme.list_active;
    let rail_color = theme::tokens::glass::STROKE_STRONG.to_hsla();
    let depth = guides.depth();
    let rail_x = node_centre(depth) - RAIL / 2.;
    let half = PR_NODE / 2.;
    let base = node == PrNodeState::Base;
    let interactive = on_click.is_some();

    let title_el = div().flex_1().min_w_0().truncate().child(title);
    let title_el = if base {
        title_el
            .text_xs()
            .font_family(theme::terminal::FONT_FAMILY)
            .text_color(muted)
    } else {
        title_el
            .text_sm()
            .text_color(theme.foreground)
            .when(strong, |this| this.font_weight(gpui::FontWeight::SEMIBOLD))
    };
    let row_h = if caption.is_some() { CAPTION_ROW } else { PR_ROW };
    let title_el = match caption {
        Some(caption) => gpui_component::v_flex()
            .flex_1()
            .min_w_0()
            .child(title_el)
            .child(div().min_w_0().truncate().text_xs().text_color(muted).child(caption))
            .into_any_element(),
        None => title_el.into_any_element(),
    };

    let mut row = div()
        .id(id)
        .relative()
        .flex()
        .flex_row()
        .items_center()
        .w_full()
        .min_w_0()
        .flex_shrink_0()
        .h(px(row_h))
        .gap(px(GAP))
        .pr_3()
        .pl(px(domain::list_item::lead_x(depth)))
        .rounded(px(theme::tokens::radius::MD))
        .when(active, |this| this.bg(row_active))
        .when(interactive, |this| {
            this.cursor_pointer()
                .hover(move |style| style.bg(if active { row_active } else { row_hover }))
        })
        .children(crate::tree_guides::guide_layer(&guides, BASE, 0.))
        .when(rail.above, |this| {
            this.child(
                div()
                    .absolute()
                    .top_0()
                    .left(px(rail_x))
                    .w(px(RAIL))
                    .h(px(row_h / 2. - half))
                    .bg(rail_color),
            )
        })
        .when(rail.below, |this| {
            this.child(
                div()
                    .absolute()
                    .bottom_0()
                    .left(px(rail_x))
                    .w(px(RAIL))
                    .h(px(row_h / 2. - half))
                    .bg(rail_color),
            )
        })
        .child(match lead {
            Some(lead) => div()
                .flex_shrink_0()
                .size(px(MARK))
                .flex()
                .items_center()
                .justify_center()
                .child(lead)
                .into_any_element(),
            None => pr_node(node, cx).into_any_element(),
        })
        .children(identifier.map(|identifier| {
            div()
                .flex_shrink_0()
                .min_w(px(IDENTIFIER_MIN_W))
                .text_xs()
                .font_family(theme::terminal::FONT_FAMILY)
                .text_color(muted)
                .child(identifier)
        }))
        .child(title_el)
        .children(word.map(|word| {
            div()
                .flex_shrink_0()
                .text_size(px(11.5))
                .text_color(muted)
                .child(word)
        }))
        .children(trailing);
    if let Some(on_click) = on_click {
        row = row.on_click(move |event, window, cx| on_click(event, window, cx));
    }
    if let Some(on_hover) = on_hover {
        row = row.on_hover(move |hovered, window, cx| on_hover(hovered, window, cx));
    }
    row.into_any_element()
}

/// One row of a PR tree (or a flat run of single PRs).
pub(crate) struct PrListRow {
    pub(crate) key: String,
    pub(crate) identifier: Option<SharedString>,
    pub(crate) title: SharedString,
    pub(crate) depth: usize,
    pub(crate) node: PrNodeState,
    pub(crate) word: Option<SharedString>,
    pub(crate) active: bool,
    pub(crate) on_open: Option<RunRowAction>,
    pub(crate) trailing: Option<AnyElement>,
}

/// A PR TREE: rows nest with tree guides, gapless (nothing for a connector
/// to bridge).
pub(crate) fn pr_list(id_prefix: &str, rows: Vec<PrListRow>, cx: &App) -> AnyElement {
    let guides = domain::tree_guides::guides_for(&rows.iter().map(|row| row.depth).collect::<Vec<_>>());
    gpui_component::v_flex()
        .w_full()
        .min_w_0()
        .children(rows.into_iter().enumerate().map(|(index, row)| {
            pr_row(
                PrRowSpec {
                    id: SharedString::from(format!("{id_prefix}-{}", row.key)),
                    node: row.node,
                    lead: None,
                    identifier: row.identifier,
                    title: row.title,
                    word: row.word,
                    guides: guides.get(index).cloned().unwrap_or_default(),
                    rail: PrRail::default(),
                    active: row.active,
                    on_click: row.on_open,
                    on_hover: None,
                    trailing: row.trailing,
                    strong: false,
                    caption: None,
                },
                cx,
            )
        }))
        .into_any_element()
}

/// One member of a linear stack.
pub(crate) struct StackRailMember {
    pub(crate) key: String,
    pub(crate) identifier: SharedString,
    pub(crate) title: SharedString,
    /// The member on screen: the filled node + the active wash.
    pub(crate) current: bool,
    pub(crate) on_open: Option<RunRowAction>,
    /// An external-link glyph or the like (never the ghost, which the rail
    /// adds itself).
    pub(crate) trailing: Option<AnyElement>,
}

/// A callback naming a member by its `key`.
pub(crate) type StackMemberAction = Rc<dyn Fn(&str, &mut Window, &mut App)>;

/// Everything [`stack_rail`] draws.
pub(crate) struct StackRailSpec {
    pub(crate) id_prefix: SharedString,
    /// TOP member first.
    pub(crate) members: Vec<StackRailMember>,
    /// The trailing muted base-branch row.
    pub(crate) base_branch: SharedString,
    /// The quiet word on the top row ([`STACK_WORD`]; none on the Guide).
    pub(crate) word: Option<SharedString>,
    /// The member under the pointer — the CALLER's view state (`on_hover`
    /// writes it), so only that row offers the ghost.
    pub(crate) hovered: Option<String>,
    /// `Some(key)` entering a member row, `None` leaving it.
    pub(crate) on_hover: Option<Rc<dyn Fn(Option<String>, &mut Window, &mut App)>>,
    /// `Some` = the hovered row offers a ghost "Merge through here".
    pub(crate) on_merge_through: Option<StackMemberAction>,
}

/// A STACK, top-first, on one rail down to its base-branch row. The current
/// member wears the active wash; with `on_merge_through`, the HOVERED member
/// offers a ghost "Merge through here" (`contract.diffUi.mergeThrough`).
pub(crate) fn stack_rail(spec: StackRailSpec, cx: &App) -> AnyElement {
    let StackRailSpec {
        id_prefix,
        members,
        base_branch,
        word,
        hovered,
        on_hover,
        on_merge_through,
    } = spec;
    let has_members = !members.is_empty();
    let mut column = gpui_component::v_flex().w_full().min_w_0();
    for (index, member) in members.into_iter().enumerate() {
        let StackRailMember {
            key,
            identifier,
            title,
            current,
            on_open,
            trailing,
        } = member;
        let ghost = on_merge_through
            .as_ref()
            .filter(|_| hovered.as_deref() == Some(key.as_str()))
            .map(|merge| {
                let merge = merge.clone();
                let key = key.clone();
                crate::controls::text_button(
                    SharedString::from(format!("{id_prefix}-merge-through-{key}")),
                    domain::pr_stack::MERGE_THROUGH_LABEL,
                    crate::controls::TextButtonVariant::Text,
                    cx,
                )
                .flex_shrink_0()
                .on_click(move |_, window, cx| {
                    // The row itself opens the member — merging must not.
                    cx.stop_propagation();
                    merge(&key, window, cx);
                })
                .into_any_element()
            });
        let hover: Option<PrRowHover> = on_hover.as_ref().map(|on_hover| {
            let on_hover = on_hover.clone();
            let key = key.clone();
            Box::new(move |entered: &bool, window: &mut Window, cx: &mut App| {
                on_hover(entered.then(|| key.clone()), window, cx);
            }) as PrRowHover
        });
        let trailing = match (trailing, ghost) {
            (Some(trailing), Some(ghost)) => Some(
                gpui_component::h_flex()
                    .flex_shrink_0()
                    .gap_2()
                    .child(ghost)
                    .child(trailing)
                    .into_any_element(),
            ),
            (trailing, ghost) => ghost.or(trailing),
        };
        column = column.child(pr_row(
            PrRowSpec {
                id: SharedString::from(format!("{id_prefix}-{key}")),
                node: if current {
                    PrNodeState::Current
                } else {
                    PrNodeState::Open
                },
                lead: None,
                identifier: Some(identifier),
                title,
                word: if index == 0 { word.clone() } else { None },
                guides: domain::tree_guides::Guides::default(),
                rail: PrRail {
                    above: index > 0,
                    below: true,
                },
                active: current,
                on_click: on_open,
                on_hover: hover,
                trailing,
                strong: false,
                caption: None,
            },
            cx,
        ));
    }
    column
        .child(pr_row(
            PrRowSpec {
                rail: PrRail {
                    above: has_members,
                    below: false,
                },
                node: PrNodeState::Base,
                ..PrRowSpec::open(format!("{id_prefix}-base"), base_branch)
            },
            cx,
        ))
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// EXP-1248: the node sits in the run mark's box, so a tree's elbow and
    /// the stack rail both land on its centre (`list-item.json` geometry).
    #[test]
    fn the_node_centre_is_the_gutter_centre() {
        assert_eq!(node_centre(0), BASE + MARK / 2.);
        assert_eq!(node_centre(2), BASE + 2. * domain::list_item::INDENT + MARK / 2.);
        assert_eq!(MARK, crate::tree_guides::LEVEL_PITCH);
        assert!(PR_NODE < MARK);
    }
}
