//! The layout ENGINE: taffy's algorithms (flexbox, grid, block) over the
//! surface's own node slots (taffy's "custom tree" API), so the core
//! controls what `TaffyTree` hides:
//!
//! - a measured leaf answers with its size AND its first BASELINE
//!   (`alignItems: baseline` aligns text by its first line, not its bottom);
//! - nodes live in STABLE SLOTS (the surface's node indices): a re-windowed
//!   list or an opened overlay touches only the slots that changed, and
//!   `mark_dirty` clears the layout cache of a node and its ancestors only;
//! - `content_size` per node (scroll containers report their scrollable
//!   extent).

use taffy::prelude::*;
use taffy::{
    compute_block_layout, compute_cached_layout, compute_flexbox_layout, compute_grid_layout, compute_hidden_layout, compute_leaf_layout,
    compute_root_layout, round_layout, Cache, CacheTree, LayoutInput, LayoutOutput,
};

/// What a measured leaf answers for one taffy request.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct LeafAnswer {
    pub size: Size<f32>,
    /// The first baseline, from the top of the border box; `None` = the
    /// bottom edge (CSS synthesizes it there).
    pub baseline: Option<f32>,
}

#[derive(Debug, Clone)]
struct ENode {
    style: Style,
    cache: Cache,
    unrounded: Layout,
    rounded: Layout,
    children: Vec<u32>,
    parent: Option<u32>,
    /// A measured leaf (its size comes from the measure callback).
    measured: bool,
}

impl Default for ENode {
    fn default() -> Self {
        ENode {
            style: Style::default(),
            cache: Cache::new(),
            unrounded: Layout::with_order(0),
            rounded: Layout::with_order(0),
            children: Vec::new(),
            parent: None,
            measured: false,
        }
    }
}

/// One tree of slots; several roots (the main tree, every overlay layer).
#[derive(Debug, Clone, Default)]
pub struct Engine {
    nodes: Vec<ENode>,
    rounding: bool,
}

/// The measure callback: `(slot, known size, available space) → answer`.
pub type MeasureFn<'a> = dyn FnMut(u32, Size<Option<f32>>, Size<AvailableSpace>) -> LeafAnswer + 'a;

struct View<'t, 'm> {
    engine: &'t mut Engine,
    measure: &'t mut MeasureFn<'m>,
}

pub struct ChildIter<'a>(std::slice::Iter<'a, u32>);

impl Iterator for ChildIter<'_> {
    type Item = NodeId;
    fn next(&mut self) -> Option<NodeId> {
        self.0.next().map(|i| NodeId::from(*i as usize))
    }
}

fn slot(id: NodeId) -> usize {
    usize::from(id)
}

impl taffy::TraversePartialTree for View<'_, '_> {
    type ChildIter<'a>
        = ChildIter<'a>
    where
        Self: 'a;

    fn child_ids(&self, node: NodeId) -> Self::ChildIter<'_> {
        ChildIter(self.engine.nodes[slot(node)].children.iter())
    }

    fn child_count(&self, node: NodeId) -> usize {
        self.engine.nodes[slot(node)].children.len()
    }

    fn get_child_id(&self, node: NodeId, index: usize) -> NodeId {
        NodeId::from(self.engine.nodes[slot(node)].children[index] as usize)
    }
}

impl taffy::TraverseTree for View<'_, '_> {}

impl taffy::LayoutPartialTree for View<'_, '_> {
    type CoreContainerStyle<'a>
        = &'a Style
    where
        Self: 'a;
    type CustomIdent = String;

    fn get_core_container_style(&self, node: NodeId) -> Self::CoreContainerStyle<'_> {
        &self.engine.nodes[slot(node)].style
    }

    fn set_unrounded_layout(&mut self, node: NodeId, layout: &Layout) {
        self.engine.nodes[slot(node)].unrounded = *layout;
    }

    fn resolve_calc_value(&self, _val: *const (), _basis: f32) -> f32 {
        0.0
    }

    fn compute_child_layout(&mut self, node: NodeId, inputs: LayoutInput) -> LayoutOutput {
        if inputs.run_mode == taffy::RunMode::PerformHiddenLayout {
            return compute_hidden_layout(self, node);
        }
        compute_cached_layout(self, node, inputs, |view, node, inputs| {
            let n = &view.engine.nodes[slot(node)];
            let display = n.style.display;
            let has_children = !n.children.is_empty();
            match (display, has_children) {
                (Display::None, _) => compute_hidden_layout(view, node),
                (Display::Block, true) => compute_block_layout(view, node, inputs, None),
                (Display::Flex, true) => compute_flexbox_layout(view, node, inputs),
                (Display::Grid, true) => compute_grid_layout(view, node, inputs),
                (_, false) => {
                    let index = slot(node) as u32;
                    let View { engine, measure } = view;
                    let n = &engine.nodes[slot(node)];
                    let measured = n.measured;
                    let mut baseline = None;
                    let mut output = compute_leaf_layout(inputs, &n.style, |_, _| 0.0, |known, available| {
                        if !measured {
                            return Size::ZERO;
                        }
                        let answer = measure(index, known, available);
                        baseline = answer.baseline;
                        answer.size
                    });
                    if let Some(b) = baseline {
                        output.first_baselines.y = Some(b);
                    }
                    output
                }
            }
        })
    }
}

impl CacheTree for View<'_, '_> {
    fn cache_get(&self, node: NodeId, input: &LayoutInput) -> Option<LayoutOutput> {
        self.engine.nodes[slot(node)].cache.get(input)
    }

    fn cache_store(&mut self, node: NodeId, input: &LayoutInput, output: LayoutOutput) {
        self.engine.nodes[slot(node)].cache.store(input, output)
    }

    fn cache_clear(&mut self, node: NodeId) {
        self.engine.nodes[slot(node)].cache.clear();
    }
}

impl taffy::LayoutFlexboxContainer for View<'_, '_> {
    type FlexboxContainerStyle<'a>
        = &'a Style
    where
        Self: 'a;
    type FlexboxItemStyle<'a>
        = &'a Style
    where
        Self: 'a;

    fn get_flexbox_container_style(&self, node: NodeId) -> Self::FlexboxContainerStyle<'_> {
        &self.engine.nodes[slot(node)].style
    }

    fn get_flexbox_child_style(&self, child: NodeId) -> Self::FlexboxItemStyle<'_> {
        &self.engine.nodes[slot(child)].style
    }
}

impl taffy::LayoutGridContainer for View<'_, '_> {
    type GridContainerStyle<'a>
        = &'a Style
    where
        Self: 'a;
    type GridItemStyle<'a>
        = &'a Style
    where
        Self: 'a;

    fn get_grid_container_style(&self, node: NodeId) -> Self::GridContainerStyle<'_> {
        &self.engine.nodes[slot(node)].style
    }

    fn get_grid_child_style(&self, child: NodeId) -> Self::GridItemStyle<'_> {
        &self.engine.nodes[slot(child)].style
    }
}

impl taffy::LayoutBlockContainer for View<'_, '_> {
    type BlockContainerStyle<'a>
        = &'a Style
    where
        Self: 'a;
    type BlockItemStyle<'a>
        = &'a Style
    where
        Self: 'a;

    fn get_block_container_style(&self, node: NodeId) -> Self::BlockContainerStyle<'_> {
        &self.engine.nodes[slot(node)].style
    }

    fn get_block_child_style(&self, child: NodeId) -> Self::BlockItemStyle<'_> {
        &self.engine.nodes[slot(child)].style
    }
}

impl taffy::RoundTree for View<'_, '_> {
    fn get_unrounded_layout(&self, node: NodeId) -> Layout {
        self.engine.nodes[slot(node)].unrounded
    }

    fn set_final_layout(&mut self, node: NodeId, layout: &Layout) {
        self.engine.nodes[slot(node)].rounded = *layout;
    }
}

impl Engine {
    pub fn new() -> Engine {
        Engine::default()
    }

    /// Off = fractional frames (the default).
    pub fn set_rounding(&mut self, on: bool) {
        self.rounding = on;
    }

    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// Grow (or shrink) to `n` slots; new slots are empty leaves.
    pub fn resize(&mut self, n: usize) {
        self.nodes.resize_with(n, ENode::default);
    }

    pub fn clear(&mut self) {
        self.nodes.clear();
    }

    /// Replace a slot's style; its cache and its ancestors' are cleared when
    /// the style differs.
    pub fn set_style(&mut self, i: u32, style: Style) -> bool {
        if self.nodes[i as usize].style == style {
            return false;
        }
        self.nodes[i as usize].style = style;
        self.mark_dirty(i);
        true
    }

    pub fn style(&self, i: u32) -> &Style {
        &self.nodes[i as usize].style
    }

    /// Set a slot's children (parent pointers follow); dirty when changed.
    pub fn set_children(&mut self, i: u32, children: &[u32]) {
        if self.nodes[i as usize].children == children {
            return;
        }
        for c in std::mem::take(&mut self.nodes[i as usize].children) {
            if self.nodes[c as usize].parent == Some(i) {
                self.nodes[c as usize].parent = None;
            }
        }
        for &c in children {
            self.nodes[c as usize].parent = Some(i);
        }
        self.nodes[i as usize].children = children.to_vec();
        self.mark_dirty(i);
    }

    pub fn children(&self, i: u32) -> &[u32] {
        &self.nodes[i as usize].children
    }

    pub fn set_measured(&mut self, i: u32, measured: bool) {
        if self.nodes[i as usize].measured != measured {
            self.nodes[i as usize].measured = measured;
            self.mark_dirty(i);
        }
    }

    /// Forget the cached layout of `i` and every ancestor.
    pub fn mark_dirty(&mut self, i: u32) {
        let mut cur = Some(i);
        while let Some(n) = cur {
            let node = &mut self.nodes[n as usize];
            node.cache.clear();
            cur = node.parent;
        }
    }

    pub fn mark_all_dirty(&mut self) {
        for n in &mut self.nodes {
            n.cache.clear();
        }
    }

    /// Lay out the tree under `root` against the available space.
    pub fn compute(&mut self, root: u32, available: Size<AvailableSpace>, measure: &mut MeasureFn) {
        let rounding = self.rounding;
        let mut view = View { engine: self, measure };
        compute_root_layout(&mut view, NodeId::from(root as usize), available);
        if rounding {
            round_layout(&mut view, NodeId::from(root as usize));
        }
    }

    /// The final layout of a slot (relative to its parent).
    pub fn layout(&self, i: u32) -> &Layout {
        let n = &self.nodes[i as usize];
        if self.rounding {
            &n.rounded
        } else {
            &n.unrounded
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn baseline_alignment_uses_the_measured_first_baseline() {
        let mut e = Engine::new();
        e.resize(3);
        e.set_style(0, Style { display: Display::Flex, align_items: Some(AlignItems::BASELINE), ..Style::default() });
        e.set_children(0, &[1, 2]);
        e.set_measured(1, true);
        e.set_measured(2, true);
        // A 40 px control whose text baseline sits at 26, beside a 20 px
        // text line whose baseline is 15: the text moves down by 11.
        let mut measure = |i: u32, _k: Size<Option<f32>>, _a: Size<AvailableSpace>| match i {
            1 => LeafAnswer { size: Size { width: 80.0, height: 40.0 }, baseline: Some(26.0) },
            _ => LeafAnswer { size: Size { width: 50.0, height: 20.0 }, baseline: Some(15.0) },
        };
        e.compute(0, Size { width: AvailableSpace::Definite(300.0), height: AvailableSpace::MaxContent }, &mut measure);
        assert_eq!(e.layout(1).location.y, 0.0);
        assert_eq!(e.layout(2).location.y, 11.0);
        // Without baselines the bottoms align (CSS synthesizes them there).
        let mut bottoms = |i: u32, _k: Size<Option<f32>>, _a: Size<AvailableSpace>| match i {
            1 => LeafAnswer { size: Size { width: 80.0, height: 40.0 }, baseline: None },
            _ => LeafAnswer { size: Size { width: 50.0, height: 20.0 }, baseline: None },
        };
        e.mark_all_dirty();
        e.compute(0, Size { width: AvailableSpace::Definite(300.0), height: AvailableSpace::MaxContent }, &mut bottoms);
        assert_eq!(e.layout(2).location.y, 20.0);
    }

    #[test]
    fn dirty_marking_climbs_to_the_root_only() {
        let mut e = Engine::new();
        e.resize(4);
        e.set_style(0, Style { display: Display::Flex, flex_direction: FlexDirection::Column, ..Style::default() });
        e.set_children(0, &[1, 2]);
        e.set_style(2, Style { display: Display::Flex, ..Style::default() });
        e.set_children(2, &[3]);
        for i in [1, 3] {
            e.set_measured(i, true);
        }
        let calls = std::cell::Cell::new(0);
        let mut measure = |_i: u32, _k: Size<Option<f32>>, _a: Size<AvailableSpace>| {
            calls.set(calls.get() + 1);
            LeafAnswer { size: Size { width: 10.0, height: 10.0 }, baseline: None }
        };
        e.compute(0, Size { width: AvailableSpace::Definite(100.0), height: AvailableSpace::MaxContent }, &mut measure);
        let first = calls.get();
        e.compute(0, Size { width: AvailableSpace::Definite(100.0), height: AvailableSpace::MaxContent }, &mut measure);
        assert_eq!(calls.get(), first, "a clean tree answers from its caches");
        e.mark_dirty(3);
        e.compute(0, Size { width: AvailableSpace::Definite(100.0), height: AvailableSpace::MaxContent }, &mut measure);
        assert!(calls.get() > first, "the dirty leaf is measured again");
    }
}
