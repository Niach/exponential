//! One eUI surface mirrored into zgui: [`Doc`] owns an `eui_core::Surface`
//! and a [`ZMeasure`]; [`Doc::pass`] runs the core's layout and writes what
//! changed into per-slot signals; [`Doc::view`] is the zgui view that reads
//! them. Every slot = one absolutely positioned element at the core's
//! frame, its inline style = the resolved visual; a container's children
//! are a keyed `For` over the slots the core placed; a scroll container
//! moves ONE inner element (`transform`), so a scroll re-styles one node
//! plus the rows the window brought in.
//!
//! A `Doc` is plain data: any number of them live in one process and one
//! zgui document (criterion 3: the spike's mapper could mount only one).

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;

use eui_core::measure::TextStyle;
use eui_core::style::Visual;
use eui_core::surface::{LayoutOutput, PlacedNode, Surface, SurfaceOptions};
use eui_core::theme::{Mode, ResolvedTheme};
use eui_core::layout_tree::NodeKind;
use serde_json::{Map, Value};
use zgui::prelude::*;
use zgui::reactive::{LocalStorage, RwSignal};
use zgui::view::AnyView;
use zgui::view::ForProps;

use crate::measure::ZMeasure;
use crate::paint::{self, LeafCx, Piece, PieceKind};

/// A child row's identity: the slot, its generation (a reused slot is a
/// new element), whether it scrolls (an inner element) and is a leaf.
type Key = (u32, u32, bool);

type Sig<T> = RwSignal<T, LocalStorage>;

#[derive(Clone, Copy)]
struct Slot {
    css: Sig<String>,
    inner: Sig<String>,
    pieces: Sig<Vec<Piece>>,
    kids: Sig<Vec<Key>>,
}

impl Slot {
    fn new() -> Slot {
        Slot { css: RwSignal::new_local(String::new()), inner: RwSignal::new_local(String::new()), pieces: RwSignal::new_local(Vec::new()), kids: RwSignal::new_local(Vec::new()) }
    }
}

#[derive(Default, Clone)]
struct Cached {
    rel: (f32, f32, f32, f32),
    css_key: u64,
    generation: u32,
    scroll: Option<(f32, f32)>,
    kids: Vec<Key>,
}

/// Counters for one pass.
#[derive(Debug, Default, Clone, Copy)]
pub struct PassStats {
    pub layout_ms: f64,
    pub mirror_ms: f64,
    pub restyled: u32,
    pub nodes: u32,
}

pub struct Doc {
    pub surface: Surface,
    pub measure: ZMeasure,
    theme: Option<Arc<ResolvedTheme>>,
    mode: Mode,
    nodes: Vec<Option<PlacedNode>>,
    cached: Vec<Cached>,
    slots: Rc<RefCell<Vec<Slot>>>,
    root: Sig<Vec<Key>>,
    root_css: Sig<String>,
    scrim: Sig<String>,
    pub width: f32,
    pub height: f32,
    first: bool,
    recycle: bool,
    pub last: Option<LayoutOutput>,
}

impl Doc {
    pub fn new(id: &str, fonts: &zgui::app::Fonts, theme: Option<Arc<ResolvedTheme>>, mode: Mode, width: f32, height: f32) -> Doc {
        let mut surface = Surface::new(id, SurfaceOptions { theme: theme.clone(), mode, ..Default::default() });
        surface.set_viewport(width, height, None);
        let mut measure = ZMeasure::new(fonts);
        measure.theme = theme.clone();
        measure.mode = mode;
        Doc {
            surface,
            measure,
            theme,
            mode,
            nodes: Vec::new(),
            cached: Vec::new(),
            slots: Rc::new(RefCell::new(Vec::new())),
            root: RwSignal::new_local(Vec::new()),
            root_css: RwSignal::new_local(String::new()),
            scrim: RwSignal::new_local(String::new()),
            width,
            height,
            first: true,
            recycle: std::env::var("EUI_ZGUI_RECYCLE").map(|v| v != "0").unwrap_or(true),
            last: None,
        }
    }

    pub fn background(&self) -> String {
        paint::theme_color(self.theme.as_deref(), self.mode, "background").unwrap_or_else(|| "#0a0a0a".into())
    }

    fn slot(&self, i: u32) -> Slot {
        let mut slots = self.slots.borrow_mut();
        while slots.len() <= i as usize {
            slots.push(Slot::new());
        }
        slots[i as usize]
    }

    /// One core pass (layout through the zgui measurer), mirrored into the
    /// signals. The next zgui frame paints it.
    pub fn pass(&mut self) -> PassStats {
        let t = std::time::Instant::now();
        let out = self.surface.layout(&mut self.measure);
        let layout_ms = t.elapsed().as_secs_f64() * 1000.0;
        let t = std::time::Instant::now();
        let mut stats = PassStats { layout_ms, ..Default::default() };

        // Patch the node cache with the delta.
        let d = &out.delta;
        if self.first || d.renumbered {
            let all = self.surface.nodes();
            self.nodes = vec![None; all.len()];
            for n in all {
                let i = n.index as usize;
                if self.nodes.len() <= i {
                    self.nodes.resize(i + 1, None);
                }
                if !n.removed {
                    self.nodes[i] = Some(n);
                }
            }
            for c in self.cached.iter_mut() {
                c.generation += 1;
                c.css_key = 0;
            }
        } else {
            for r in &d.removed {
                if let Some(n) = self.nodes.get_mut(*r as usize) {
                    *n = None;
                }
            }
            let fetch: Vec<u32> = d.added.iter().chain(d.changed.iter()).copied().collect();
            if !fetch.is_empty() {
                for n in self.surface.nodes_at(&fetch) {
                    let i = n.index as usize;
                    if self.nodes.len() <= i {
                        self.nodes.resize(i + 1, None);
                    }
                    // Recycling (default): a slot the core reuses keeps its zgui
                    // element (every part of it is signal-driven); off = a fresh one.
                    if d.added.contains(&n.index) && !self.recycle {
                        if let Some(c) = self.cached.get_mut(i) {
                            c.generation += 1;
                        }
                    }
                    if let Some(c) = self.cached.get_mut(i) {
                        c.css_key = 0;
                    }
                    self.nodes[i] = if n.removed { None } else { Some(n) };
                }
            }
        }
        for v in &out.visual_changes {
            if let Some(c) = self.cached.get_mut(*v as usize) {
                c.css_key = 0;
            }
        }
        if self.cached.len() < self.nodes.len() {
            self.cached.resize(self.nodes.len(), Cached::default());
        }
        self.first = false;

        // Frames by slot (main tree + every layer), scroll offsets.
        let mut frames: Vec<Option<(f32, f32, f32, f32)>> = vec![None; self.nodes.len()];
        for fr in out.frames.iter().chain(out.layers.iter().flat_map(|l| l.frames.iter())) {
            if let Some(slot) = frames.get_mut(fr.index as usize) {
                *slot = Some((fr.x, fr.y, fr.w, fr.h));
            }
        }
        let scrolls: HashMap<u32, (f32, f32)> = out.scrolls.iter().map(|s| (s.index, (s.offset_x, s.offset_y))).collect();
        let sticky: HashMap<u32, (f32, f32)> = out.sticky.iter().map(|s| (s.index, (s.dx, s.dy))).collect();

        // Mirror every placed node.
        let mut root_keys = Vec::new();
        let main_root = out.frames.first().map(|f| f.index);
        let layer_roots: Vec<u32> = out.layers.iter().filter(|l| !l.hosted).map(|l| l.root).collect();
        let mut inks: Vec<Option<String>> = vec![None; self.nodes.len()];
        for i in 0..self.nodes.len() {
            let Some(n) = self.nodes[i].as_ref() else { continue };
            let Some(fr) = frames[i] else { continue };
            if n.hidden {
                continue;
            }
            stats.nodes += 1;
            let vis = self.surface.visual(n.index).cloned().unwrap_or_default();
            // The parent's padding-box origin (scroll lives in the inner element).
            let (px, py) = match n.parent.and_then(|p| frames.get(p as usize).copied().flatten().map(|pf| (p, pf))) {
                Some((p, pf)) => {
                    let pv = self.surface.visual(p).cloned().unwrap_or_default();
                    let (bl, bt) = paint::border_offset(&pv);
                    (pf.0 + bl, pf.1 + bt)
                }
                None => (0.0, 0.0),
            };
            let (sx, sy) = sticky.get(&n.index).copied().unwrap_or((0.0, 0.0));
            let rel = (fr.0 - px + sx, fr.1 - py + sy, fr.2, fr.3);
            let scroll = scrolls.get(&n.index).copied().filter(|_| n.kind == NodeKind::Container);
            let ink = vis.color.clone().or_else(|| n.parent.and_then(|p| inks.get(p as usize).cloned().flatten()));
            inks[i] = ink;
            let leaf = n.kind == NodeKind::Leaf;
            // Re-derived only when the frame moved or the core reported the node (delta, visual change: `css_key` reset to 0).
            let key = hash_of(&rel_bits(rel)) | 1;
            let slot = self.slot(n.index);
            let c = &mut self.cached[i];
            if c.css_key != key {
                c.css_key = key;
                c.rel = rel;
                stats.restyled += 1;
                let css = paint::node_css(&vis, rel, leaf);
                if slot.css.with_untracked(|old| *old != css) {
                    slot.css.set(css);
                }
                if leaf {
                    let ts = self.surface.text_style(n.index).cloned().unwrap_or_else(|| text_style_of(&vis));
                    let owner_props = n.owner.as_deref().and_then(|o| self.surface.index_of(o)).and_then(|o| self.nodes.get(o as usize).cloned().flatten()).map(|o| o.props).unwrap_or_default();
                    let pieces = {
                        let lcx = LeafCx { node: n, vis: &vis, ts: &ts, theme: self.theme.as_deref(), mode: self.mode, owner_props: &owner_props, w: fr.2, h: fr.3 };
                        paint::leaf_pieces(&lcx, &mut self.measure)
                    };
                    if slot.pieces.with_untracked(|old| *old != pieces) {
                        slot.pieces.set(pieces);
                    }
                }
            }
            let c = &mut self.cached[i];
            if c.scroll != scroll {
                if let Some((x, y)) = scroll {
                    let inner = format!("position:absolute;left:0px;top:0px;width:1px;height:1px;transform:translate({}px,{}px);", -x, -y);
                    slot.inner.set(inner);
                }
                if c.scroll.is_some() != scroll.is_some() {
                    c.generation += 1;
                }
                c.scroll = scroll;
            }
            if Some(n.index) == main_root || (n.parent.is_none() && layer_roots.contains(&n.index)) {
                root_keys.push((n.index, self.cached[i].generation, scroll.is_some()));
            }
        }
        // Children lists (only placed, shown children).
        for i in 0..self.nodes.len() {
            let Some(n) = self.nodes[i].as_ref() else { continue };
            if frames[i].is_none() || n.kind == NodeKind::Leaf {
                continue;
            }
            let kids: Vec<Key> = n
                .children
                .iter()
                .filter(|k| frames.get(**k as usize).copied().flatten().is_some() && self.nodes.get(**k as usize).and_then(|n| n.as_ref()).is_some_and(|n| !n.hidden))
                .map(|k| (*k, self.cached[*k as usize].generation, self.cached[*k as usize].scroll.is_some()))
                .collect();
            if self.cached[i].kids != kids {
                self.cached[i].kids = kids.clone();
                self.slot(n.index).kids.set(kids);
            }
        }
        // Layer scrims (a modal layer dims the surface under it).
        let mut scrims = String::new();
        for l in out.layers.iter().filter(|l| l.modal && !l.hosted) {
            let owner_props = self.surface.index_of(&l.owner).and_then(|o| self.nodes.get(o as usize).cloned().flatten()).map(|o| o.props).unwrap_or_default();
            let bg = paint::part_visual(self.theme.as_deref(), self.mode, &l.kind, "overlay", &owner_props, &["open".to_string()]).background_color.unwrap_or("#00000080".into());
            scrims = bg;
        }
        let w = self.width;
        let h = if self.height > 0.0 { self.height } else { out.surface_height };
        let root_css = format!("position:absolute;left:0px;top:0px;width:{w}px;height:{h}px;overflow:hidden;background-color:{};color:{};{}", self.background(), paint::theme_color(self.theme.as_deref(), self.mode, "foreground").unwrap_or("#fafafa".into()), if scrims.is_empty() { String::new() } else { format!("--scrim:{scrims};") });
        if self.root_css.with_untracked(|o| *o != root_css) {
            self.root_css.set(root_css);
        }
        // Layer roots paint after the main tree, a scrim box before each modal one.
        if self.root.with_untracked(|o| *o != root_keys) {
            self.root.set(root_keys);
        }
        self.scrim.set(if scrims.is_empty() { String::new() } else { format!("position:absolute;left:0px;top:0px;width:{w}px;height:{h}px;background-color:{scrims};") });
        stats.mirror_ms = t.elapsed().as_secs_f64() * 1000.0;
        self.last = Some(out);
        stats
    }

    /// The zgui view of this surface (mount it once; passes update it).
    pub fn view(&self) -> AnyView {
        let slots = self.slots.clone();
        let root = self.root;
        let root_css = self.root_css;
        let scrim = self.scrim;
        let main = root;
        AnyView::new(
            zgui::elements::r#box()
                .style_text(move || Some(root_css.get()))
                .child(keyed(move || main.get().into_iter().take(1).collect(), slots.clone()))
                .child(zgui::elements::r#box().style_text(move || Some(scrim.get())))
                .child(keyed(move || main.get().into_iter().skip(1).collect(), slots)),
        )
    }
}

fn keyed(each: impl Fn() -> Vec<Key> + 'static, slots: Rc<RefCell<Vec<Slot>>>) -> AnyView {
    AnyView::new(ForProps::builder().each(each).key(|k: &Key| *k).children(move |k: Key| node_view(k, slots.clone())).build().render())
}

fn node_view((index, _, scrolls): Key, slots: Rc<RefCell<Vec<Slot>>>) -> AnyView {
    let s = slots.borrow()[index as usize];
    let el = zgui::elements::r#box().style_text(move || Some(s.css.get()));
    let pieces = s.pieces;
    let el = el.child(ForProps::builder().each(move || pieces.get().into_iter().enumerate().collect::<Vec<_>>()).key(|(i, p): &(usize, Piece)| (*i, hash_of(p))).children(|(_, p): (usize, Piece)| piece_view(p)).build().render());
    let kids = keyed(move || s.kids.get(), slots);
    if scrolls {
        AnyView::new(el.child(zgui::elements::r#box().style_text(move || Some(s.inner.get())).child(kids)))
    } else {
        AnyView::new(el.child(kids))
    }
}

fn piece_view(p: Piece) -> AnyView {
    match p.kind {
        PieceKind::Text(runs) if runs.len() == 1 && runs[0].0.is_none() => AnyView::new(zgui::elements::text().style_text(Some(p.css)).child(runs.into_iter().next().unwrap().1)),
        PieceKind::Text(runs) => {
            let mut el = zgui::elements::text().style_text(Some(p.css));
            for (css, t) in runs {
                el = match css {
                    None => el.child(t),
                    Some(css) => el.child(zgui::elements::text().style_text(Some(format!("display:inline;{css}"))).child(t)),
                };
            }
            AnyView::new(el)
        }
        PieceKind::Svg(doc) => AnyView::new(zgui::elements::vector().style_text(Some(p.css)).document(&doc)),
        PieceKind::Box => AnyView::new(zgui::elements::r#box().style_text(Some(p.css))),
    }
}

fn hash_of<T: std::hash::Hash + ?Sized>(t: &T) -> u64 {
    use std::hash::Hasher;
    let mut h = std::hash::DefaultHasher::new();
    t.hash(&mut h);
    h.finish()
}

fn rel_bits(r: (f32, f32, f32, f32)) -> [u32; 4] {
    [r.0.to_bits(), r.1.to_bits(), r.2.to_bits(), r.3.to_bits()]
}

fn vis_key(v: &Visual) -> String {
    serde_json::to_string(v).unwrap_or_default()
}

fn text_style_of(v: &Visual) -> TextStyle {
    TextStyle { font_size: v.font_size.unwrap_or(14.0), font_weight: v.font_weight.unwrap_or(400), line_height: v.line_height.unwrap_or(20.0), font_family: v.font_family.clone(), ..Default::default() }
}

#[allow(dead_code)]
fn owner_of(_: &Map<String, Value>) {}
