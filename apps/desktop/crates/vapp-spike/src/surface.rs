//! A `Surface` = one parsed tree + one taffy tree, kept across passes so only
//! nodes whose resolved style changed are restyled (taffy caches the rest).

use std::collections::HashSet;
use std::time::Instant;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use taffy::prelude::*;
use taffy::style::Direction;

use crate::measure::{Measure, MeasureRequest};
use crate::style::{self, StyleContext, TextStyle, Visual};
use crate::tree::{flatten, FlatNode, Kind, NodeSpec};

/// Absolute frame in surface coordinates (points/dp, unrounded unless
/// rounding is on).
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct Frame {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

/// One laid-out node. The list is in PRE-ORDER: paint in this order (parents
/// under children, absolute nodes last among siblings by fixture rule) and
/// read it in this order for accessibility.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlacedNode {
    pub index: u32,
    pub id: String,
    pub kind: Kind,
    pub depth: u32,
    pub frame: Frame,
    pub visual: Visual,
    pub text_style: TextStyle,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LayoutResult {
    pub nodes: Vec<PlacedNode>,
    pub measure_calls: u32,
    /// Wall time of `compute_layout_with_measure` alone (includes the host's
    /// measure callbacks, which is the number the spike wants).
    pub layout_ns: u64,
    pub surface_width: f32,
    pub surface_height: f32,
}

pub struct Surface {
    nodes: Vec<FlatNode>,
    tree: TaffyTree<u32>,
    taffy_ids: Vec<NodeId>,
    root: NodeId,
    resolved: Vec<Map<String, Value>>,
    visuals: Vec<Visual>,
    text_styles: Vec<TextStyle>,
    direction: Direction,
    viewport: (f32, f32),
    pressed: HashSet<String>,
    /// Nanoseconds spent parsing + building the tree in `new`.
    pub build_ns: u64,
}

// SAFETY: taffy's `CompactLength` carries a `*const ()` slot for `calc()`
// expressions, which makes `Style` (and so the tree) `!Send`. This crate never
// builds a calc value — every length is a plain number/percent/auto — so the
// slot is always the null/tag variant and the tree is plain owned data. The
// UniFFI facade needs the surface behind a `Mutex` on any thread.
unsafe impl Send for Surface {}

impl Surface {
    pub fn new(tree_json: &str) -> Result<Self, String> {
        let started = Instant::now();
        let spec: NodeSpec = serde_json::from_str(tree_json).map_err(|e| e.to_string())?;
        let nodes = flatten(&spec)?;
        let mut tree: TaffyTree<u32> = TaffyTree::with_capacity(nodes.len());
        let mut taffy_ids = Vec::with_capacity(nodes.len());
        // Two passes: create every node (leaves carry their index as context),
        // then wire children in the authored order.
        for n in &nodes {
            let id = tree
                .new_leaf_with_context(Style::default(), n.index)
                .map_err(|e| format!("{e:?}"))?;
            taffy_ids.push(id);
        }
        for n in &nodes {
            if !n.children.is_empty() {
                let kids: Vec<NodeId> = n.children.iter().map(|c| taffy_ids[*c as usize]).collect();
                tree.set_children(taffy_ids[n.index as usize], &kids)
                    .map_err(|e| format!("{e:?}"))?;
            }
        }
        let root = taffy_ids[0];
        let mut surface = Surface {
            resolved: vec![Map::new(); nodes.len()],
            visuals: vec![Visual::default(); nodes.len()],
            text_styles: vec![
                TextStyle { font_size: 14.0, font_weight: 400, line_height: 20.0 };
                nodes.len()
            ],
            nodes,
            tree,
            taffy_ids,
            root,
            direction: Direction::Ltr,
            viewport: (0.0, 0.0),
            pressed: HashSet::new(),
            build_ns: 0,
        };
        surface.restyle(true)?;
        surface.build_ns = started.elapsed().as_nanos() as u64;
        Ok(surface)
    }

    pub fn nodes(&self) -> &[FlatNode] {
        &self.nodes
    }

    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    /// Android accumulates in dp and rounds once itself; the browser never
    /// rounds. Off = fractional frames.
    pub fn set_rounding(&mut self, on: bool) {
        if on {
            self.tree.enable_rounding();
        } else {
            self.tree.disable_rounding();
        }
    }

    /// Surface size. Height ≤ 0 means "as tall as the content" (a scrolling
    /// host). Returns whether anything changed.
    pub fn set_viewport(&mut self, width: f32, height: f32) -> bool {
        if self.viewport == (width, height) {
            return false;
        }
        self.viewport = (width, height);
        // Only `@media` depends on the width; restyle touches changed nodes only.
        let _ = self.restyle(false);
        true
    }

    pub fn set_pressed(&mut self, ids: &[String]) -> bool {
        let next: HashSet<String> = ids.iter().cloned().collect();
        if next == self.pressed {
            return false;
        }
        self.pressed = next;
        let _ = self.restyle(false);
        true
    }

    /// Re-resolve every node; `set_style` only where the resolved map changed
    /// (`force` = first build). The root decides the surface direction, and
    /// every node gets it because taffy does not inherit `direction`.
    fn restyle(&mut self, force: bool) -> Result<(), String> {
        let root_ctx = StyleContext { surface_width: self.viewport.0, pressed: false };
        let root_resolved = style::resolve(&self.nodes[0].style, root_ctx);
        let direction = style::direction_of(&root_resolved);
        let direction_changed = direction != self.direction;
        self.direction = direction;
        for i in 0..self.nodes.len() {
            let node = &self.nodes[i];
            let ctx = StyleContext {
                surface_width: self.viewport.0,
                pressed: self.pressed.contains(&node.id),
            };
            let resolved = style::resolve(&node.style, ctx);
            if !force && !direction_changed && resolved == self.resolved[i] {
                continue;
            }
            let taffy_style = style::to_taffy(&resolved, direction)
                .map_err(|e| format!("node {}: {e}", node.id))?;
            self.tree
                .set_style(self.taffy_ids[i], taffy_style)
                .map_err(|e| format!("{e:?}"))?;
            let visual = style::visual(&resolved);
            self.text_styles[i] = TextStyle::for_node(node.kind, &node.props, &visual);
            self.visuals[i] = visual;
            self.resolved[i] = resolved;
        }
        Ok(())
    }

    pub fn layout(&mut self, measure: &mut dyn Measure) -> LayoutResult {
        let (vw, vh) = self.viewport;
        let available = Size {
            width: AvailableSpace::Definite(vw),
            height: if vh > 0.0 { AvailableSpace::Definite(vh) } else { AvailableSpace::MaxContent },
        };
        let nodes = &self.nodes;
        let text_styles = &self.text_styles;
        let mut measure_calls = 0u32;
        let started = Instant::now();
        self.tree
            .compute_layout_with_measure(
                self.root,
                available,
                |known, avail, _node_id, ctx: Option<&mut u32>, _style| {
                    let Some(index) = ctx.map(|i| *i as usize) else {
                        return Size::ZERO;
                    };
                    let node = &nodes[index];
                    if node.kind.is_container() {
                        return Size::ZERO;
                    }
                    measure_calls += 1;
                    let req = MeasureRequest {
                        index: node.index,
                        id: &node.id,
                        kind: node.kind,
                        props: &node.props,
                        text_style: text_styles[index],
                        known_width: known.width,
                        known_height: known.height,
                        available_width: avail.width,
                        available_height: avail.height,
                    };
                    measure.measure(&req)
                },
            )
            .expect("taffy layout");
        let layout_ns = started.elapsed().as_nanos() as u64;

        let mut placed = Vec::with_capacity(self.nodes.len());
        self.collect(0, 0.0, 0.0, &mut placed);
        let root = self.tree.layout(self.root).expect("root layout");
        LayoutResult {
            nodes: placed,
            measure_calls,
            layout_ns,
            surface_width: root.size.width,
            surface_height: root.size.height,
        }
    }

    fn collect(&self, index: usize, ox: f32, oy: f32, out: &mut Vec<PlacedNode>) {
        let node = &self.nodes[index];
        let l = self.tree.layout(self.taffy_ids[index]).expect("layout");
        let x = ox + l.location.x;
        let y = oy + l.location.y;
        out.push(PlacedNode {
            index: node.index,
            id: node.id.clone(),
            kind: node.kind,
            depth: node.depth,
            frame: Frame { x, y, w: l.size.width, h: l.size.height },
            visual: self.visuals[index].clone(),
            text_style: self.text_styles[index],
        });
        for child in &node.children {
            self.collect(*child as usize, x, y, out);
        }
    }
}
