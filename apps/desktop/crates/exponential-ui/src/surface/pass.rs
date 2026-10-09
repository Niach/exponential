//! The LAYOUT pass: batched measurement (≤ 3 upcalls), the main tree and
//! every layer laid out by the engine, layers placed (viewport, edge,
//! anchor with flip + shift, point, toast stack) inside the safe area,
//! scroll containers reported, `%` radii resolved against the boxes.

use std::cell::RefCell;
use std::collections::HashSet;
use std::time::Instant;

use taffy::prelude::*;
use taffy::style::Direction;

use super::{Frame, Layer, LayoutOutput, ListOutput, NodeDelta, PlacedFrame, ScrollOutput, Surface};
use crate::engine::LeafAnswer;
use crate::layout_tree::{LayerPlacement, NodeKind};
use crate::measure::{HeightRequest, Intrinsics, LeafRequest, Measure, MeasureMemo, MeasureRequest};
use crate::overlay::{place_overlay, OverlayAlign, OverlayPlacement, OverlaySide, PlaceOptions, Rect as ORect, Size as OSize, OVERLAY_PADDING};
use crate::style::{self, BoxKind};

/// The widest a centred Dialog gets, and a side Drawer.
pub const DIALOG_MAX_WIDTH: f32 = 512.0;
pub const DRAWER_MAX_WIDTH: f32 = 360.0;
/// A top/bottom Drawer takes at most this share of the viewport height.
pub const SHEET_MAX_HEIGHT: f32 = 0.9;
/// The widest a toast gets, and the gap between stacked toasts.
pub const TOAST_MAX_WIDTH: f32 = 420.0;
pub const TOAST_GAP: f32 = 8.0;

/// One placed layer: its placement, position name and anchor frame.
type Placed = (Option<OverlayPlacement>, String, Option<Frame>);

/// A layout pass in progress (the step API).
#[derive(Debug, Clone)]
pub(super) struct PassState {
    started: Instant,
    upcalls: u32,
    rounds: u32,
    missing: Vec<usize>,
    heights: Vec<HeightRequest>,
}

/// A measured leaf, OWNED: what the step API hands a host across a lock.
#[derive(Debug, Clone, PartialEq)]
pub struct LeafData {
    pub index: u32,
    pub id: String,
    pub component: String,
    pub part: Option<String>,
    pub owner_component: Option<String>,
    pub props: serde_json::Map<String, serde_json::Value>,
    pub text_style: crate::measure::TextStyle,
    pub control: crate::measure::ControlBox,
    pub lines: Option<u32>,
}

impl LeafData {
    /// The borrowed request a [`Measure`] implementation takes.
    pub fn request(&self) -> LeafRequest<'_> {
        LeafRequest { index: self.index, id: &self.id, component: &self.component, part: self.part.as_deref(), owner_component: self.owner_component.as_deref(), props: &self.props, text_style: &self.text_style, control: self.control, lines: self.lines }
    }
}

/// One step of a resumable layout pass ([`Surface::layout_begin`]).
#[derive(Debug, Clone, PartialEq)]
pub enum LayoutStep {
    /// Measure these leaves' intrinsics; answer with `layout_intrinsics`.
    Intrinsics(Vec<LeafData>),
    /// Measure these heights; answer with `layout_heights`.
    Heights(Vec<LeafData>, Vec<HeightRequest>),
    /// The pass is complete.
    Done(Box<LayoutOutput>),
    /// No pass in progress (an answer without `layout_begin`).
    Idle,
}

/// The area layers stay inside: the viewport minus the safe-area insets.
#[derive(Debug, Clone, Copy)]
struct View {
    x: f32,
    y: f32,
    w: f32,
    h: f32,
}

impl Surface {
    fn leaf_request(&self, i: usize) -> LeafRequest<'_> {
        let n = &self.nodes[i];
        let s = &self.node_state[i];
        LeafRequest { index: i as u32, id: &n.id, component: &n.component, part: n.part.as_deref(), owner_component: n.owner_component.as_deref(), props: &n.props, text_style: &s.text, control: s.control, lines: n.lines }
    }

    fn is_measured_leaf(&self, i: usize) -> bool {
        self.live[i] && self.nodes[i].kind == NodeKind::Leaf && !self.nodes[i].hidden
    }

    /// The measurer check, the rebuild and the restyle every pass starts with.
    fn prepare(&mut self, measurer: u64) {
        if self.last_measurer.is_some_and(|m| m != measurer) {
            self.invalidate_measures();
        }
        self.last_measurer = Some(measurer);
        if self.needs_build {
            self.subtree_pending.clear();
            self.rebuild();
        } else {
            for id in std::mem::take(&mut self.subtree_pending) {
                if !self.rebuild_subtree(&id) {
                    self.rebuild();
                    self.subtree_pending.clear();
                    break;
                }
            }
        }
        self.restyle();
    }

    /// Every live measured leaf without a (current) memo entry.
    fn missing_leaves(&self) -> Vec<usize> {
        (0..self.nodes.len())
            .filter(|&i| self.is_measured_leaf(i) && (!self.memo.intrinsics.contains_key(&(i as u32)) || self.memo_versions.get(&(i as u32)).copied() != Some(self.node_state[i].content_version)))
            .collect()
    }

    fn apply_intrinsics(&mut self, missing: &[usize], answers: &[Intrinsics]) {
        for (k, &i) in missing.iter().enumerate() {
            if let Some(a) = answers.get(k) {
                self.memo.intrinsics.insert(i as u32, *a);
                self.memo_versions.insert(i as u32, self.node_state[i].content_version);
                self.engine.mark_dirty(i as u32);
            }
        }
    }

    fn apply_heights(&mut self, requests: &[HeightRequest], heights: &[f32]) {
        for (k, r) in requests.iter().enumerate() {
            if let Some(h) = heights.get(k) {
                self.memo.heights.insert((r.index, MeasureMemo::width_key(r.width)), *h);
                self.engine.mark_dirty(r.index);
            }
        }
    }

    /// Phase 2: the main tree and every layer against the memo; the height
    /// requests (deduplicated) the memo could only guess.
    fn compute_all(&mut self) -> (Vec<Frame>, Vec<Placed>, Vec<HeightRequest>) {
        let (vw, vh) = self.viewport;
        let guesses = RefCell::new(Vec::<HeightRequest>::new());
        let mut frames = vec![Frame::default(); self.nodes.len()];
        let mut placed = Vec::with_capacity(self.layers.len());
        if let Some(root) = self.main_root {
            // CSS parity (VAPP-91): a block-level surface root is as wide as
            // the surface unless its own style sets a width; the web's
            // inline-flex natives (base-css.ts) keep their content width.
            // Round 3: a `bar` Segmented always fills (base-css.ts `width: 100%`).
            let root_node = &self.nodes[root as usize];
            let inline_root = matches!(root_node.component.as_str(), "Button" | "Toggle" | "Link" | "Icon" | "Ring" | "Spinner")
                || (root_node.component == "Segmented" && root_node.props.get("variant").and_then(|v| v.as_str()) != Some("bar"));
            if !inline_root && self.engine.style(root).size.width == Dimension::auto() {
                let mut style = self.engine.style(root).clone();
                style.size.width = Dimension::percent(1.0);
                self.engine.set_style(root, style);
            }
            let available = Size { width: AvailableSpace::Definite(vw.max(0.0)), height: if vh > 0.0 { AvailableSpace::Definite(vh) } else { AvailableSpace::MaxContent } };
            self.compute(root, available, &guesses);
            self.walk_frames(root, 0.0, 0.0, &mut frames);
        }
        let main_h = self.main_root.map(|r| frames[r as usize].h).unwrap_or(0.0);
        let vh_eff = if vh > 0.0 { vh } else { self.max_height.unwrap_or(main_h) };
        let ins = self.settings.insets;
        let view = View { x: ins.left, y: ins.top, w: (vw - ins.left - ins.right).max(0.0), h: (vh_eff - ins.top - ins.bottom).max(0.0) };
        let mut toast_stack = 0.0f32;
        for k in 0..self.layers.len() {
            let p = self.place_layer(k, view, vw, vh_eff, &mut frames, &guesses, &mut toast_stack);
            placed.push(p);
        }
        let mut seen = HashSet::new();
        let pending: Vec<HeightRequest> = guesses.into_inner().into_iter().filter(|r| seen.insert((r.index, MeasureMemo::width_key(r.width)))).collect();
        (frames, placed, pending)
    }

    fn finish(&mut self, frames: Vec<Frame>, placed: Vec<Placed>, state: PassState) -> LayoutOutput {
        let (_, vh) = self.viewport;
        let bounded_h = if vh > 0.0 { Some(vh) } else { self.max_height };
        let layout_ns = state.started.elapsed().as_nanos() as u64;
        self.output(frames, placed, bounded_h, state.rounds, state.upcalls, layout_ns)
    }

    /// Prepare the tree, then run the batched passes. At most 3 upcall
    /// rounds (intrinsics, heights, one correction). The measurer is called
    /// in-process with borrowed requests; [`Self::layout_begin`] is the same
    /// pass as resumable STEPS for a host that must not be called back
    /// while the surface is locked (the FFI).
    pub fn layout(&mut self, measure: &mut dyn Measure) -> LayoutOutput {
        let mut state = PassState { started: Instant::now(), upcalls: 0, rounds: 0, missing: Vec::new(), heights: Vec::new() };
        self.prepare(measure.measure_id());
        loop {
            state.rounds += 1;
            let missing = self.missing_leaves();
            if !missing.is_empty() {
                let requests: Vec<LeafRequest> = missing.iter().map(|&i| self.leaf_request(i)).collect();
                let answers = measure.measure_intrinsics(&requests);
                state.upcalls += 1;
                self.apply_intrinsics(&missing, &answers);
            }
            let (frames, placed, pending) = self.compute_all();
            if pending.is_empty() || state.rounds >= 3 {
                return self.finish(frames, placed, state);
            }
            let leaves: Vec<LeafRequest> = pending.iter().map(|r| self.leaf_request(r.index as usize)).collect();
            let heights = measure.measure_heights(&leaves, &pending);
            state.upcalls += 1;
            self.apply_heights(&pending, &heights);
        }
    }

    fn owned_leaves(&self, indices: impl Iterator<Item = usize>) -> Vec<LeafData> {
        indices
            .map(|i| {
                let n = &self.nodes[i];
                let s = &self.node_state[i];
                LeafData { index: i as u32, id: n.id.clone(), component: n.component.clone(), part: n.part.clone(), owner_component: n.owner_component.clone(), props: n.props.clone(), text_style: s.text.clone(), control: s.control, lines: n.lines }
            })
            .collect()
    }

    /// Start a layout pass as STEPS: answer [`LayoutStep::Intrinsics`] with
    /// [`Self::layout_intrinsics`] and [`LayoutStep::Heights`] with
    /// [`Self::layout_heights`] until [`LayoutStep::Done`]. Same rounds and
    /// upcalls as [`Self::layout`]; the host measures between the steps
    /// without holding the surface.
    pub fn layout_begin(&mut self, measure_id: u64) -> LayoutStep {
        self.prepare(measure_id);
        self.step_state = Some(PassState { started: Instant::now(), upcalls: 0, rounds: 0, missing: Vec::new(), heights: Vec::new() });
        self.step_round()
    }

    fn step_round(&mut self) -> LayoutStep {
        let missing = self.missing_leaves();
        let Some(state) = self.step_state.as_mut() else { return LayoutStep::Idle };
        state.rounds += 1;
        if !missing.is_empty() {
            let leaves = self.owned_leaves(missing.iter().copied());
            self.step_state.as_mut().expect("state").missing = missing;
            return LayoutStep::Intrinsics(leaves);
        }
        self.step_compute()
    }

    fn step_compute(&mut self) -> LayoutStep {
        let (frames, placed, pending) = self.compute_all();
        let Some(mut state) = self.step_state.take() else { return LayoutStep::Idle };
        if pending.is_empty() || state.rounds >= 3 {
            return LayoutStep::Done(Box::new(self.finish(frames, placed, state)));
        }
        let leaves = self.owned_leaves(pending.iter().map(|r| r.index as usize));
        state.heights = pending.clone();
        self.step_state = Some(state);
        LayoutStep::Heights(leaves, pending)
    }

    /// Abandon a stepped pass (the host's measurer failed): the next
    /// `layout_begin` starts clean; the memo keeps what was answered.
    pub fn layout_abort(&mut self) {
        self.step_state = None;
    }

    /// Is a stepped pass in progress?
    pub fn layout_in_progress(&self) -> bool {
        self.step_state.is_some()
    }

    /// The answers to [`LayoutStep::Intrinsics`], in request order.
    pub fn layout_intrinsics(&mut self, answers: &[Intrinsics]) -> LayoutStep {
        let Some(state) = self.step_state.as_mut() else { return LayoutStep::Idle };
        state.upcalls += 1;
        let missing = std::mem::take(&mut state.missing);
        self.apply_intrinsics(&missing, answers);
        self.step_compute()
    }

    /// The answers to [`LayoutStep::Heights`], in request order.
    pub fn layout_heights(&mut self, heights: &[f32]) -> LayoutStep {
        let Some(state) = self.step_state.as_mut() else { return LayoutStep::Idle };
        state.upcalls += 1;
        let requests = std::mem::take(&mut state.heights);
        self.apply_heights(&requests, heights);
        self.step_round()
    }

    /// Lay out the tree under `root`, answering leaves from the memo.
    fn compute(&mut self, root: u32, available: Size<AvailableSpace>, guesses: &RefCell<Vec<HeightRequest>>) {
        let Surface { engine, memo, nodes, .. } = self;
        let mut measure_fn = |index: u32, known: Size<Option<f32>>, avail: Size<AvailableSpace>| -> LeafAnswer {
            if nodes[index as usize].hidden {
                return LeafAnswer::default();
            }
            let req = MeasureRequest { index, known_width: known.width, known_height: known.height, available_width: avail.width, available_height: avail.height };
            let (size, guessed, baseline) = memo.answer(&req);
            if guessed {
                guesses.borrow_mut().push(HeightRequest { index, width: size.width });
            }
            LeafAnswer { size, baseline }
        };
        engine.compute(root, available, &mut measure_fn);
    }

    fn walk_frames(&self, slot: u32, ox: f32, oy: f32, out: &mut [Frame]) {
        let l = self.engine.layout(slot);
        let x = ox + l.location.x;
        let y = oy + l.location.y;
        out[slot as usize] = Frame { x, y, w: l.size.width, h: l.size.height };
        for &c in &self.nodes[slot as usize].children {
            self.walk_frames(c, x, y, out);
        }
    }

    /// A layer root's style from its restyled flat map plus `extra` edits
    /// (size bounds, safe-area padding) — recomputed each pass so the edits
    /// never accumulate.
    fn layer_root_style(&mut self, root: u32, pad_extra: [f32; 4], edit: impl FnOnce(&mut Style)) {
        let mut flat = self.node_state[root as usize].flat.clone();
        if pad_extra.iter().any(|p| *p != 0.0) {
            let base = style::sides_px(&flat, "padding", "paddingHorizontal", "paddingVertical", ["paddingTop", "paddingRight", "paddingBottom", "paddingLeft"]).unwrap_or([0.0; 4]);
            for (k, key) in ["paddingTop", "paddingRight", "paddingBottom", "paddingLeft"].iter().enumerate() {
                flat.insert((*key).to_string(), serde_json::json!(base[k] + pad_extra[k]));
            }
        }
        let mut s = style::to_taffy(&flat, self.direction, BoxKind::Container).unwrap_or_default();
        edit(&mut s);
        self.engine.set_style(root, s);
    }

    #[allow(clippy::too_many_arguments)]
    fn place_layer(&mut self, k: usize, view: View, vw: f32, vh: f32, frames: &mut [Frame], guesses: &RefCell<Vec<HeightRequest>>, toast_stack: &mut f32) -> Placed {
        let spec = self.layers[k].clone();
        let root = spec.root;
        let pad = OVERLAY_PADDING as f32;
        let rtl = self.direction == Direction::Rtl;
        let ins = self.settings.insets;
        let finite = vh.is_finite() && vh > 0.0;
        let body_shrinks = spec.scroll_body.is_some();
        let (placement, position, anchor_frame, x, y) = match spec.placement {
            LayerPlacement::Centered => {
                let max_h = finite.then_some((view.h - 2.0 * pad).max(0.0));
                self.layer_root_style(root, [0.0; 4], |s| {
                    if let (Some(h), true) = (max_h, body_shrinks) {
                        s.max_size.height = length(h);
                    }
                });
                let width = (view.w - 2.0 * pad).clamp(0.0, DIALOG_MAX_WIDTH);
                self.compute(root, Size { width: AvailableSpace::Definite(width), height: AvailableSpace::MaxContent }, guesses);
                let size = self.engine.layout(root).size;
                let x = view.x + ((view.w - size.width) / 2.0).max(pad);
                let y = if finite { view.y + ((view.h - size.height) / 2.0).max(pad) } else { view.y + pad };
                (None, "centered".to_string(), None, x, y)
            }
            LayerPlacement::Edge(side) => {
                let vertical = matches!(side, OverlaySide::Top | OverlaySide::Bottom);
                // The sheet reaches the edge; its content stays clear of the
                // safe area.
                let extra = match side {
                    OverlaySide::Bottom => [0.0, ins.right, ins.bottom, ins.left],
                    OverlaySide::Top => [ins.top, ins.right, 0.0, ins.left],
                    OverlaySide::Left => [ins.top, 0.0, ins.bottom, ins.left],
                    OverlaySide::Right => [ins.top, ins.right, ins.bottom, 0.0],
                };
                self.layer_root_style(root, extra, |s| {
                    if vertical && finite && body_shrinks {
                        s.max_size.height = length((vh * SHEET_MAX_HEIGHT).max(0.0));
                    }
                });
                let available = if vertical {
                    Size { width: AvailableSpace::Definite(vw), height: AvailableSpace::MaxContent }
                } else {
                    Size { width: AvailableSpace::Definite((vw - 2.0 * pad).clamp(0.0, DRAWER_MAX_WIDTH)), height: if finite { AvailableSpace::Definite(vh) } else { AvailableSpace::MaxContent } }
                };
                self.compute(root, available, guesses);
                let size = self.engine.layout(root).size;
                let (x, y) = match side {
                    OverlaySide::Top | OverlaySide::Left => (0.0, 0.0),
                    OverlaySide::Bottom => (0.0, if finite { vh - size.height } else { 0.0 }),
                    OverlaySide::Right => (vw - size.width, 0.0),
                };
                (None, side.as_str().to_string(), None, x, y)
            }
            LayerPlacement::Anchored { anchor, side, align } => {
                let a = frames.get(anchor as usize).copied().unwrap_or_default();
                let side = if spec.mirror && rtl {
                    match side {
                        OverlaySide::Left => OverlaySide::Right,
                        OverlaySide::Right => OverlaySide::Left,
                        s => s,
                    }
                } else {
                    side
                };
                let size = self.shrink_to_content(root, view, pad, spec.match_anchor_width.then_some(a.w), guesses);
                let p = self.anchor_place(a, size, view, side, align, rtl);
                let pos = p.side.as_str().to_string();
                (Some(p), pos, Some(a), p.x as f32, p.y as f32)
            }
            LayerPlacement::AtPoint { x, y } => {
                let a = Frame { x, y, w: 0.0, h: 0.0 };
                let size = self.shrink_to_content(root, view, pad, None, guesses);
                let p = self.anchor_place(a, size, view, OverlaySide::Bottom, OverlayAlign::Start, rtl);
                (Some(p), "point".to_string(), Some(a), p.x as f32, p.y as f32)
            }
            LayerPlacement::Toast { .. } => {
                let max = (view.w - 2.0 * pad).clamp(0.0, TOAST_MAX_WIDTH);
                self.compute(root, Size { width: AvailableSpace::MaxContent, height: AvailableSpace::MaxContent }, guesses);
                let natural = self.engine.layout(root).size.width;
                if natural > max {
                    self.compute(root, Size { width: AvailableSpace::Definite(max), height: AvailableSpace::MaxContent }, guesses);
                }
                let size = self.engine.layout(root).size;
                let wide = self.breakpoints().get("md").is_some_and(|md| vw >= *md);
                let x = if wide {
                    if rtl {
                        view.x + pad
                    } else {
                        view.x + view.w - pad - size.width
                    }
                } else {
                    view.x + (view.w - size.width) / 2.0
                };
                let y = view.y + view.h - pad - *toast_stack - size.height;
                *toast_stack += size.height + TOAST_GAP;
                (None, "toast".to_string(), None, x, y)
            }
        };
        self.walk_frames(root, x, y, frames);
        (placement, position, anchor_frame)
    }

    /// Lay a popup out at its natural width, clamped to the view (and at
    /// least `min_width`).
    fn shrink_to_content(&mut self, root: u32, view: View, pad: f32, min_width: Option<f32>, guesses: &RefCell<Vec<HeightRequest>>) -> OSize {
        self.layer_root_style(root, [0.0; 4], |s| {
            if let Some(w) = min_width {
                s.min_size.width = length(w);
            }
        });
        let max = (view.w - 2.0 * pad).max(0.0);
        self.compute(root, Size { width: AvailableSpace::MaxContent, height: AvailableSpace::MaxContent }, guesses);
        let natural = self.engine.layout(root).size.width;
        if natural > max {
            self.compute(root, Size { width: AvailableSpace::Definite(max), height: AvailableSpace::MaxContent }, guesses);
        }
        let size = self.engine.layout(root).size;
        OSize { width: size.width as f64, height: size.height as f64 }
    }

    /// `place_overlay` inside the safe area (align start/end follow the
    /// direction).
    fn anchor_place(&self, a: Frame, size: OSize, view: View, side: OverlaySide, align: OverlayAlign, rtl: bool) -> OverlayPlacement {
        let align = match (align, rtl) {
            (OverlayAlign::Start, true) => OverlayAlign::End,
            (OverlayAlign::End, true) => OverlayAlign::Start,
            (a, _) => a,
        };
        let anchor = ORect { x: (a.x - view.x) as f64, y: (a.y - view.y) as f64, width: a.w as f64, height: a.h as f64 };
        let vp = OSize { width: view.w as f64, height: if view.h > 0.0 { view.h as f64 } else { f64::INFINITY } };
        let mut p = place_overlay(&anchor, &size, &vp, &PlaceOptions { side, align, ..PlaceOptions::default() });
        p.x += view.x as f64;
        p.y += view.y as f64;
        p
    }

    fn preorder(&self, slot: u32, out: &mut Vec<u32>) {
        out.push(slot);
        for &c in &self.nodes[slot as usize].children {
            self.preorder(c, out);
        }
    }

    /// Round 2 (§5): where a list's viewport comes from — itself (bounded
    /// with taller content on its axis), its nearest scrolling ancestor, or
    /// the host viewport — and how long it is.
    fn list_view(&self, spec: &crate::layout_tree::ListSpec, frames: &[Frame], scrolls: &[ScrollOutput]) -> crate::layout_tree::ListView {
        use crate::layout_tree::{ListView, ListViewSource};
        let h = spec.horizontal;
        let ext = |f: Frame| if h { f.w } else { f.h };
        let start = |f: Frame| if h { f.x } else { f.y };
        let scrolls_on_axis = |idx: u32| {
            scrolls.iter().any(|s| s.index == idx && if h { s.scroll_x && s.content_width > frames[idx as usize].w + 0.5 } else { s.scroll_y && s.content_height > frames[idx as usize].h + 0.5 })
        };
        let f = frames[spec.node as usize];
        if scrolls_on_axis(spec.node) {
            return ListView { source: ListViewSource::Own, viewport: ext(f) };
        }
        let mut cur = self.nodes[spec.node as usize].parent;
        while let Some(p) = cur {
            if scrolls_on_axis(p) {
                let pf = frames[p as usize];
                return ListView { source: ListViewSource::Ancestor { id: self.scroll_key(p), rel: start(f) - start(pf) }, viewport: ext(pf) };
            }
            cur = self.nodes[p as usize].parent;
        }
        let viewport = if h {
            self.viewport.0
        } else if self.viewport.1 > 0.0 {
            self.viewport.1
        } else {
            self.max_height.unwrap_or(2000.0)
        };
        ListView { source: ListViewSource::Host { rel: start(f) }, viewport }
    }

    /// Round 2: the paint offsets of `position: sticky` nodes (pinned by
    /// their insets inside the nearest scroller while their parent is in
    /// view) and of pinned List section headers.
    fn sticky_offsets(&mut self, frames: &[Frame], scrolls: &[ScrollOutput], drawn: &[u32]) -> Vec<super::StickyOutput> {
        let mut out = Vec::new();
        // Pinned section headers (on the window's memoized offsets).
        for spec in self.lists.iter().filter(|l| l.sticky && !l.headers.is_empty()) {
            let Some(window) = self.local.lists.get_mut(&spec.id) else { continue };
            let Some(view) = self.local.list_views.get(&spec.id) else { continue };
            let axis = |p: (f32, f32)| if spec.horizontal { p.0 } else { p.1 };
            let scroll = match &view.source {
                crate::layout_tree::ListViewSource::Own => self.local.scroll.get(&spec.id).copied().map(axis).unwrap_or(0.0),
                crate::layout_tree::ListViewSource::Ancestor { id, rel } => self.local.scroll.get(id).copied().map(axis).unwrap_or(0.0) - rel,
                crate::layout_tree::ListViewSource::Host { rel } => self.local.surface_scroll.map(|p| axis(p) - rel).unwrap_or_else(|| self.local.scroll.get(&spec.id).copied().map(axis).unwrap_or(0.0)),
            }
            .max(0.0);
            window.gap = spec.gap;
            let Some((pin, row_offset)) = window.sticky_header_cached(&spec.keys, &spec.headers, scroll as f64) else { continue };
            let key = &spec.keys[pin.row];
            let Some(&header) = self.nodes[spec.node as usize].children.iter().find(|c| self.nodes[**c as usize].props.get("key").and_then(serde_json::Value::as_str) == Some(key.as_str())) else { continue };
            let delta = (pin.offset - row_offset) as f32;
            // The header's frame already sits at its row offset (in flow or
            // absolute); `delta` moves it to the pinned position.
            let (dx, dy) = if spec.horizontal { (delta, 0.0) } else { (0.0, delta) };
            if dx.abs() > 0.01 || dy.abs() > 0.01 {
                out.push(super::StickyOutput { index: header, dx, dy });
            }
        }
        // `position: sticky`.
        for &i in drawn {
            let flat = &self.node_state[i as usize].flat;
            if flat.get("position").and_then(serde_json::Value::as_str) != Some("sticky") {
                continue;
            }
            // A physical side, else the `inset` shorthand (logical insets
            // are physical by now).
            let all = flat.get("inset").and_then(style::px);
            let inset = |k: &str| flat.get(k).and_then(style::px).or(all);
            let (top, bottom, left, right) = (inset("top"), inset("bottom"), inset("left"), inset("right"));
            let n = frames[i as usize];
            let parent_index = self.nodes[i as usize].parent;
            let mut parent = parent_index.map(|p| frames[p as usize]).unwrap_or(n);
            // The nearest scroller: an ancestor that scrolls, else the host
            // viewport (its offset = `set_surface_scroll`).
            let mut cur = parent_index;
            let mut port: Option<(Frame, f32, f32)> = None;
            while let Some(p) = cur {
                if let Some(s) = scrolls.iter().find(|s| s.index == p && ((s.scroll_y && s.content_height > frames[p as usize].h + 0.5) || (s.scroll_x && s.content_width > frames[p as usize].w + 0.5))) {
                    port = Some((frames[p as usize], s.offset_x, s.offset_y));
                    // A direct child of the scroller is bounded by its whole
                    // scrollable content (CSS: a sticky header in a scrolling
                    // pane stays pinned all the way down).
                    if Some(p) == parent_index {
                        parent = Frame { w: s.content_width.max(parent.w), h: s.content_height.max(parent.h), ..parent };
                    }
                    break;
                }
                cur = self.nodes[p as usize].parent;
            }
            let (pf, ox, oy) = port.unwrap_or((Frame { x: 0.0, y: 0.0, w: self.viewport.0, h: self.viewport.1 }, self.local.surface_scroll.unwrap_or_default().0, self.local.surface_scroll.unwrap_or_default().1));
            // The start edges pin against the scroll offset alone; the end
            // edges need the scroller's extent (an auto-height host surface
            // has none: bottom/right never pin there).
            let mut dy = 0.0f32;
            if let Some(t) = top {
                let edge = pf.y + oy + t;
                if n.y < edge {
                    dy = (edge.min(parent.y + parent.h - n.h) - n.y).max(0.0);
                }
            } else if let (Some(b), true) = (bottom, pf.h > 0.0) {
                let edge = pf.y + oy + pf.h - b - n.h;
                if n.y > edge {
                    dy = (edge.max(parent.y) - n.y).min(0.0);
                }
            }
            let mut dx = 0.0f32;
            if let Some(l) = left {
                let edge = pf.x + ox + l;
                if n.x < edge {
                    dx = (edge.min(parent.x + parent.w - n.w) - n.x).max(0.0);
                }
            } else if let (Some(r), true) = (right, pf.w > 0.0) {
                let edge = pf.x + ox + pf.w - r - n.w;
                if n.x > edge {
                    dx = (edge.max(parent.x) - n.x).min(0.0);
                }
            }
            if dx.abs() > 0.01 || dy.abs() > 0.01 {
                out.push(super::StickyOutput { index: i, dx, dy });
            }
        }
        out
    }

    fn output(&mut self, frames: Vec<Frame>, placed: Vec<Placed>, bounded_h: Option<f32>, rounds: u32, upcalls: u32, layout_ns: u64) -> LayoutOutput {
        let mut main = Vec::new();
        let mut drawn: Vec<u32> = Vec::new();
        if let Some(root) = self.main_root {
            self.preorder(root, &mut drawn);
        }
        for &i in &drawn {
            let f = frames[i as usize];
            main.push(PlacedFrame { index: i, x: f.x, y: f.y, w: f.w, h: f.h });
        }
        let mut layers = Vec::new();
        for (spec, (placement, position, anchor_frame)) in self.layers.clone().into_iter().zip(placed) {
            let mut order = Vec::new();
            self.preorder(spec.root, &mut order);
            let lf = order.iter().map(|&i| {
                let f = frames[i as usize];
                PlacedFrame { index: i, x: f.x, y: f.y, w: f.w, h: f.h }
            });
            let frames_out: Vec<PlacedFrame> = lf.collect();
            drawn.extend(order);
            layers.push(Layer { layer: spec.layer, kind: spec.kind.clone(), owner: spec.owner.clone(), root: spec.root, anchor_frame, placement, position, class: spec.class, modal: spec.modal, dismissible: spec.dismissible, frames: frames_out });
        }
        // `%` radii against the laid-out boxes.
        for &i in &drawn {
            let s = &self.node_state[i as usize];
            let Some(pct) = s.radius_pct else { continue };
            let f = frames[i as usize];
            let side = f.w.min(f.h);
            let mut v = s.visual.clone();
            if let Some(all) = pct.all {
                v.border_radius = Some(all * side);
            }
            if pct.corners.iter().any(Option::is_some) {
                let base = v.border_radius.unwrap_or(0.0);
                let mut radii = v.corner_radii.unwrap_or([base; 4]);
                for (k, c) in pct.corners.iter().enumerate() {
                    if let Some(c) = c {
                        radii[k] = c * side;
                    }
                }
                v.corner_radii = Some(radii);
            }
            if v != s.visual {
                self.node_state[i as usize].visual = v;
                self.visuals_dirty.insert(i);
            }
        }
        // Scroll containers: offsets clamped to the content.
        let mut scrolls = Vec::new();
        let mut reclamped = false;
        for &i in &drawn {
            let s = &self.node_state[i as usize];
            let (sx, sy) = style::is_scroll_container(&s.flat);
            if !sx && !sy {
                continue;
            }
            let l = self.engine.layout(i);
            let f = frames[i as usize];
            let content_w = l.content_size.width.max(f.w);
            let content_h = l.content_size.height.max(f.h);
            let id = self.scroll_key(i);
            let asked = self.local.scroll.get(&id).copied();
            let (ox, oy) = asked.unwrap_or((0.0, 0.0));
            let ox = if sx { ox.clamp(0.0, (content_w - f.w).max(0.0)) } else { 0.0 };
            let oy = if sy { oy.clamp(0.0, (content_h - f.h).max(0.0)) } else { 0.0 };
            // The clamped offset IS the state (content shrank, a fling
            // overshot): a windowed list re-windows at it next pass. A
            // container whose content fits is not its own scroller (a page
            // scrolls it: the host's offset positions the window) and keeps
            // the offset it was given.
            let own_x = content_w > f.w + 0.5;
            let own_y = content_h > f.h + 0.5;
            if let Some((ax, ay)) = asked {
                let (kx, ky) = (if own_x && sx { ox } else { ax }, if own_y && sy { oy } else { ay });
                if (ax - kx).abs() > 0.01 || (ay - ky).abs() > 0.01 {
                    let (ox, oy) = (kx, ky);
                    self.local.scroll.insert(id.clone(), (ox, oy));
                    if let Some(w) = self.local.lists.get_mut(&id) {
                        w.scroll_offset = oy;
                        if (ay - oy).abs() > 0.01 && self.lists.iter().any(|l| l.id == id && l.windowed) {
                            self.subtree_pending.insert(id.clone());
                            reclamped = true;
                        }
                    }
                }
            }
            scrolls.push(ScrollOutput { index: i, offset_x: ox, offset_y: oy, content_width: content_w, content_height: content_h, scroll_x: sx, scroll_y: sy });
        }
        // Windowed lists (round 2): remember the items' real extents on the
        // list's axis, then window against the list's own, its nearest
        // scrolling ancestor's or the host's viewport.
        let mut lists = Vec::new();
        let mut rewindow = false;
        for spec in self.lists.clone() {
            let view = self.list_view(&spec, &frames, &scrolls);
            self.local.list_views.insert(spec.id.clone(), view.clone());
            let axis = |p: (f32, f32)| if spec.horizontal { p.0 } else { p.1 };
            let scroll = match &view.source {
                crate::layout_tree::ListViewSource::Own => self.local.scroll.get(&spec.id).copied().map(axis).unwrap_or(0.0),
                crate::layout_tree::ListViewSource::Ancestor { id, rel } => self.local.scroll.get(id).copied().map(axis).unwrap_or(0.0) - rel,
                crate::layout_tree::ListViewSource::Host { rel } => self.local.surface_scroll.map(|p| axis(p) - rel).unwrap_or_else(|| self.local.scroll.get(&spec.id).copied().map(axis).unwrap_or(0.0)),
            }
            .max(0.0);
            let estimate = self.effective.as_ref().and_then(|t| t.tokens.control.get("row").copied()).map(|v| v as f32).unwrap_or(crate::list::DEFAULT_ESTIMATED_ITEM_HEIGHT);
            let window = self.local.lists.entry(spec.id.clone()).or_insert_with(|| crate::list::ListWindow::new(estimate, crate::list::DEFAULT_OVERSCAN, spec.gap));
            let mut changed = false;
            for child in &self.nodes[spec.node as usize].children {
                let c = &self.nodes[*child as usize];
                if matches!(c.part.as_deref(), Some("divider" | "empty" | "spacer")) {
                    continue;
                }
                let key = c.props.get("key").and_then(serde_json::Value::as_str).filter(|_| matches!(c.part.as_deref(), Some("row" | "section"))).map(str::to_string).unwrap_or_else(|| c.id.clone());
                let f = frames[*child as usize];
                changed |= window.record(&key, if spec.horizontal { f.w } else { f.h });
            }
            if spec.windowed {
                window.scroll_offset = scroll;
                let offsets = window.offsets_cached(&spec.keys);
                let range = window.visible_range(&offsets, view.viewport.max(1.0));
                let count = spec.keys.len();
                let pad = |o: &[f32], r: crate::list::VisibleRange| (o[r.start], o[count] - o[r.end.min(count)]);
                let covered = range.start >= spec.range.start && range.end <= spec.range.end;
                // Re-window when the measured extents move the window or
                // the spacers, or when the viewport now shows items the
                // window does not hold.
                if (changed && (range != spec.range || pad(&offsets, range) != pad(&spec.offsets, spec.range))) || !covered {
                    self.subtree_pending.insert(spec.id.clone());
                    rewindow |= !covered;
                }
            }
            let content_height = window.offsets_cached(&spec.keys).last().copied().unwrap_or(0.0);
            lists.push(ListOutput { id: spec.id.clone(), node: spec.node, content_height, start: spec.range.start as u32, end: spec.range.end as u32, count: spec.keys.len() as u32, windowed: spec.windowed, horizontal: spec.horizontal });
        }
        if rewindow && !self.pending.iter().any(|e| matches!(e, super::OutEvent::Relayout)) {
            self.pending.push(super::OutEvent::Relayout);
        }
        let sticky = self.sticky_offsets(&frames, &scrolls, &drawn);
        self.local.sticky_any = drawn.iter().any(|&i| self.node_state[i as usize].flat.get("position").and_then(serde_json::Value::as_str) == Some("sticky"));
        self.scrolls = scrolls.clone();
        if reclamped && !self.pending.iter().any(|e| matches!(e, super::OutEvent::Relayout)) {
            // The window moved under the host: one more pass shows the rows.
            self.pending.push(super::OutEvent::Relayout);
        }
        let root_frame = self.main_root.map(|r| frames[r as usize]).unwrap_or_default();
        self.last_frames = frames;
        let mut visual_changes: Vec<u32> = self.visuals_dirty.drain().collect();
        visual_changes.sort_unstable();
        let delta = NodeDelta {
            added: std::mem::take(&mut self.delta_added).into_iter().collect(),
            removed: std::mem::take(&mut self.delta_removed).into_iter().collect(),
            changed: std::mem::take(&mut self.delta_changed).into_iter().collect(),
            renumbered: std::mem::take(&mut self.renumbered),
        };
        LayoutOutput {
            frames: main,
            layers,
            lists,
            scrolls,
            sticky,
            toasts: self.toasts.clone(),
            visual_changes,
            delta,
            structure_version: self.structure_version,
            surface_width: root_frame.w,
            surface_height: root_frame.h,
            overflow: bounded_h.is_some_and(|h| root_frame.h > h + 0.5),
            direction: if self.direction == Direction::Rtl { "rtl".into() } else { "ltr".into() },
            breakpoint: self.breakpoint.clone(),
            measure_rounds: rounds,
            upcalls,
            restyled: std::mem::take(&mut self.restyled),
            rebuilt: std::mem::take(&mut self.rebuilt),
            built_nodes: std::mem::take(&mut self.built_nodes),
            layout_ns,
        }
    }
}
