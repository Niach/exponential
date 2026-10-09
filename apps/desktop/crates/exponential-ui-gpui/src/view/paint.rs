//! Building the element tree: one absolutely positioned div per placed node
//! at its frame relative to its parent's (scrolled) frame, the box from the
//! node's cached `PaintStyle` (animated by [`super::motion`]), the leaf
//! content inside, children nested in PAINT order; scroll containers clip,
//! translate their descendants by the core's offsets and draw scrollbars;
//! layers paint as surface-coordinate overlays (a scrim for modal ones),
//! the non-anchored ones moved into the region the host shows.

use std::rc::Rc;
use std::time::Instant;

use exponential_ui::layout_tree::{LayerClass, NodeKind};
use exponential_ui::surface::{Layer, PlacedNode};
use gpui::{canvas, div, linear_color_stop, linear_gradient, prelude::*, px, AnyElement, Context, Div, ExternalPaths, Hsla, MouseButton, SharedString, Stateful, Window};
use serde_json::{json, Value};

use super::state::{a11y_description, a11y_label, is_text_field, role_of, NodeFlags};
use super::SurfaceView;
use crate::extension::PaintContext;
use crate::host::PaintError;
use crate::measure::{display_text, text_chrome, Shaper};
use crate::paint::icons;
use crate::paint::markdown::{self, MdPaint, MdStyles, TextSpec};
use crate::paint::natives::{self, aligned, styled_box, LeafCx};
use crate::paint::parts::{part_props, part_visual, px_prop, spacing, theme_color};
use crate::paint::PaintStyle;
use crate::text;

/// The nearest clipping ancestor with rounded corners, in PAINTED surface
/// coordinates: descendants touching its corners round theirs to match
/// (gpui's content masks are rectangular).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Clip {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub radii: [f32; 4],
}

/// A child's corner radii after a rounded clipping ancestor: a corner that
/// lies inside the ancestor's rounded corner square takes the ancestor's
/// radius minus the inset (so an image flush with a card's corner clips).
pub fn inherit_radii(own: [f32; 4], (x, y, w, h): (f32, f32, f32, f32), clip: &Clip) -> [f32; 4] {
    let (l, t) = (x - clip.x, y - clip.y);
    let (r, b) = ((clip.x + clip.w) - (x + w), (clip.y + clip.h) - (y + h));
    let corner = |own: f32, cr: f32, dx: f32, dy: f32| {
        if cr <= 0.0 || dx >= cr || dy >= cr {
            own
        } else {
            own.max(cr - dx.max(dy).max(0.0))
        }
    };
    [corner(own[0], clip.radii[0], l, t), corner(own[1], clip.radii[1], r, t), corner(own[2], clip.radii[2], r, b), corner(own[3], clip.radii[3], l, b)]
}

/// Where a node's parent puts it: the parent's content origin (its frame
/// plus its scroll offset, surface coordinates), where that origin is
/// painted, and the rounded clip in force.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Place {
    pub origin: (f32, f32),
    pub abs: (f32, f32),
    pub clip: Option<Clip>,
}

fn rounded<E: Styled>(e: E, r: [f32; 4]) -> E {
    e.rounded_tl(px(r[0])).rounded_tr(px(r[1])).rounded_br(px(r[2])).rounded_bl(px(r[3]))
}

/// The scrollbar thumb `(offset, length)` along a track `len` long.
pub fn thumb_geometry(viewport: f32, content: f32, offset: f32, len: f32) -> Option<(f32, f32)> {
    if content <= viewport + 0.5 || viewport <= 0.0 {
        return None;
    }
    let thumb = (len * viewport / content).clamp(20.0_f32.min(len), len);
    let travel = (len - thumb).max(0.0);
    let at = if content > viewport { travel * (offset / (content - viewport)).clamp(0.0, 1.0) } else { 0.0 };
    Some((at, thumb))
}

impl SurfaceView {
    fn now(&self) -> Instant {
        self.now_or_instant()
    }

    /// The style node `index` shows now (mid-transition when one runs; a
    /// ghost's slots are its own, so it shows its last style as is).
    pub(crate) fn shown_style(&self, index: u32) -> PaintStyle {
        let target = self.styles.get(index as usize).cloned().unwrap_or_default();
        if self.ghosting.get() {
            return target;
        }
        self.motion.style(index, &target, self.now())
    }

    fn owner_props(&self, index: u32) -> serde_json::Map<String, Value> {
        let o = self.cache.owner_of(index);
        self.cache.node(o).map(|n| n.props.clone()).unwrap_or_default()
    }

    /// A built-in UI string (`catalog/strings.json` id) from the surface's
    /// table: the host's override, else the English default.
    pub(crate) fn builtin_string(&self, id: &str) -> String {
        self.surface.strings().get(id).cloned().or_else(|| exponential_ui::strings::default_string(id).map(str::to_string)).unwrap_or_else(|| id.to_string())
    }

    fn states_of(&self, n: &PlacedNode) -> Vec<String> {
        let mut s = n.states.clone();
        s.extend(self.interaction.get(&n.id).copied().unwrap_or_default().states());
        s
    }

    fn checked(&self, n: &PlacedNode) -> bool {
        n.states.iter().any(|s| s == "checked") || matches!(n.props.get("checked"), Some(Value::Bool(true)))
    }

    /// One node and its subtree (`None` = hidden).
    pub(crate) fn paint_node(&self, index: u32, place: Place, window: &mut Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        let i = index as usize;
        let n = self.cache.node(index)?;
        if n.hidden {
            // An animation restarts when the node shows again.
            if self.styles.get(index as usize).is_some_and(|s| s.animation.is_some()) {
                self.anim_start.borrow_mut().remove(self.cache.ids[index as usize].as_ref());
            }
            return None;
        }
        if let Some(t) = self.paint_trace.borrow_mut().as_mut() {
            t.push(index);
        }
        let mut style = self.shown_style(index);
        if style.invisible {
            // `visibility: hidden`: the box keeps its place, nothing paints.
            return None;
        }
        let target = self.frames.get(i).copied().unwrap_or_default();
        let f = if self.ghosting.get() { target } else { self.motion.frame(index, target, self.now()) };
        // Round 2 §2: a keyframe animation composes OUTSIDE the node's own
        // transform; opacity multiplies; a leaf scales about its centre; a
        // rotation turns an Icon's glyph (gpui rotates no other box).
        let anim = if self.ghosting.get() { None } else { self.animation_frame(index, &style, window) };
        if let Some(a) = &anim {
            style.opacity = Some(style.opacity.unwrap_or(1.0) * a.opacity as f32);
            style.transform.tx += a.translate_x as f32;
            style.transform.ty += a.translate_y as f32;
            style.transform.scale *= a.scale as f32;
            style.transform.rotate += a.rotate as f32;
        }
        let tf = style.transform;
        let leaf = n.kind == NodeKind::Leaf;
        // Round 2 §2: `position: sticky` / a pinned section header moves by
        // the core's offset (inside its scroller's translation).
        let (sdx, sdy) = self.sticky.get(&index).copied().unwrap_or((0.0, 0.0));
        // Paint-only transform: the translate moves the box; a leaf's scale
        // grows it about its centre (containers keep their size).
        let (mut x, mut y, mut w, mut h) = (f.x - place.origin.0 + tf.tx + sdx, f.y - place.origin.1 + tf.ty + sdy, f.w, f.h);
        if leaf && (tf.scale - 1.0).abs() > 1e-3 && tf.scale > 0.0 {
            let (nw, nh) = (w * tf.scale, h * tf.scale);
            x -= (nw - w) / 2.0;
            y -= (nh - h) / 2.0;
            w = nw;
            h = nh;
        }
        let abs = (place.abs.0 + x, place.abs.1 + y);
        let mut radii = style.radii.map(|r| r.min(w.min(h) / 2.0).max(0.0));
        if let Some(clip) = &place.clip {
            radii = inherit_radii(radii, (abs.0, abs.1, w, h), clip);
        }
        let id = self.cache.ids[i].clone();
        let mut el = div().id(id.clone()).absolute().left(px(x)).top(px(y)).w(px(w)).h(px(h));
        // `native: true`: the platform control paints the part itself.
        let native = style.native && leaf && matches!((n.component.as_str(), n.part.as_deref()), ("Switch", Some("track")) | ("Checkbox", Some("box" | "checkbox")));
        let style = if native { PaintStyle { bg: None, border: [0.0; 4], shadows: Vec::new(), gradient: None, ..style } } else { style };
        el = el.when_some(style.bg, |d, bg| d.bg(bg));
        el = rounded(el, radii);
        if !style.shadows.is_empty() {
            el = el.shadow(style.shadows.clone());
        }
        if let Some(o) = style.opacity {
            el = el.opacity(o);
        }
        if style.clip_x && style.clip_y {
            el = el.overflow_hidden();
        } else if style.clip_x {
            el = el.overflow_x_hidden();
        } else if style.clip_y {
            el = el.overflow_y_hidden();
        }
        if let Some(c) = style.color {
            el = el.text_color(c);
        }
        let ghost = self.ghosting.get();
        if !ghost {
            el = self.accessible(el, n);
            if let Some(h) = self.focus_handles.get(&n.id) {
                el = el.track_focus(h);
            }
        }
        if !style.pointer_none && !ghost {
            el = self.interactive(el, index, n, &style, cx);
            if let Some(c) = style.cursor {
                el = el.cursor(c);
            }
        }
        if let Some(g) = &style.gradient {
            el = el.child(gradient(g, w, h, radii));
        }
        // The border paints over the background, under the children, and
        // never offsets them (an overlay, not gpui's box border).
        if style.has_border() {
            let mut b = div().absolute().top_0().left_0().w(px(w)).h(px(h)).border_color(style.border_color.unwrap_or_default());
            b = b.border_t(px(style.border[0])).border_r(px(style.border[1])).border_b(px(style.border[2])).border_l(px(style.border[3]));
            if style.dashed {
                b = b.border_dashed();
            }
            el = el.child(rounded(b, radii));
        }
        if let Some(bounds) = self.debug_bounds.as_ref().filter(|_| !ghost) {
            let (map, key) = (bounds.clone(), n.id.clone());
            el = el.child(canvas(move |b, _, _| {
                map.borrow_mut().insert(key.clone(), b);
            }, |_, _, _, _| {}).absolute().size_full());
        }
        let ink = self.inks.get(i).copied().unwrap_or(gpui::white());
        if n.component == "Extension" {
            return Some(self.paint_extension(el, index, n, w, h, place, abs, window, cx));
        }
        if leaf {
            // A painter that panics (a prop it cannot paint) leaves an empty
            // box and reaches the host (`onPaintError`); the surface stays.
            let painted = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| self.paint_leaf(index, n, &style, ink, w, h, radii, window, cx)));
            match painted {
                Ok(Some(content)) => el = el.child(content),
                Ok(None) => {}
                Err(panic) => self.paint_failed(n, &panic_message(panic.as_ref()), cx),
            }
        }
        // Children: the content origin shifts by the scroll offset.
        let (sx, sy) = self.scrolls.get(&index).map(|s| (s.offset_x, s.offset_y)).unwrap_or((0.0, 0.0));
        let clip = if style.clips() && radii.iter().any(|r| *r > 0.0) && sx == 0.0 && sy == 0.0 { Some(Clip { x: abs.0, y: abs.1, w, h, radii }) } else if style.clips() { None } else { place.clip };
        let child_place = Place { origin: (f.x + sx, f.y + sy), abs, clip };
        let kids = self.children_elements(index, child_place, window, cx);
        el = el.children(kids);
        // The shimmer band sweeps OVER the content (the web's `::after`),
        // clipped to its own box; the node's children stay unclipped.
        if let (Some(band), Some(a)) = (anim.and_then(|a| a.band), style.animation.as_ref()) {
            el = el.child(self.shimmer_band(&a.name, band as f32, w, h, radii));
        }
        if let Some(s) = self.scrolls.get(&index).copied() {
            el = self.scroll_container(el, index, n, s, w, h, ink, cx);
        }
        if self.interaction.get(&n.id).is_some_and(|s| s.focus_visible) && style.shadows.is_empty() {
            let ring = theme_color(self.surface.theme().map(|t| t.as_ref()), self.surface.mode(), "ring").unwrap_or(ink.opacity(0.5));
            el = el.child(rounded(div().absolute().top(px(-2.0)).left(px(-2.0)).w(px(w + 4.0)).h(px(h + 4.0)).border_2().border_color(ring), radii.map(|r| r + 2.0)));
        }
        Some(el.into_any_element())
    }

    /// The keyframe frame of an animated node now (`None` = not animated);
    /// asks for the next frame while it moves (reduced motion: the rest
    /// frame, never moving).
    fn animation_frame(&self, index: u32, style: &PaintStyle, window: &mut Window) -> Option<exponential_ui::animation::AnimationFrame> {
        let a = style.animation.as_ref()?;
        let reduced = a.reduced || self.surface.settings().reduced_motion;
        let now = self.now();
        let id = self.cache.ids.get(index as usize)?.clone();
        let start = *self.anim_start.borrow_mut().entry(id).or_insert(now);
        let elapsed = now.saturating_duration_since(start).as_secs_f64() * 1000.0;
        let frame = exponential_ui::animation::frame_with_timing(&a.name, &a.timing, elapsed, reduced)?;
        let total = a.timing.duration_ms * a.timing.iterations.count();
        if !reduced && a.timing.duration_ms > 0.0 && elapsed < total {
            window.request_animation_frame();
        }
        Some(frame)
    }

    /// The shimmer band at `band` box widths (−1 → 1): the set's band colour
    /// (a theme colour) through its alpha stops.
    fn shimmer_band(&self, name: &str, band: f32, w: f32, h: f32, radii: [f32; 4]) -> AnyElement {
        let def = exponential_ui::animation::ANIMATIONS.get(name).and_then(|d| d.band.clone());
        let theme = self.surface.theme().cloned();
        let color = def
            .as_ref()
            .and_then(|b| b.color.strip_prefix("$color.").map(str::to_string))
            .and_then(|c| theme_color(theme.as_deref(), self.surface.mode(), &c))
            .unwrap_or(gpui::white());
        let peak = def.as_ref().and_then(|b| b.stops.iter().map(|s| s.alpha).reduce(f64::max)).unwrap_or(0.5) as f32;
        let edge = Hsla { a: 0.0, ..color };
        let mid = Hsla { a: color.a * peak, ..color };
        let half = w / 2.0;
        let left = div().absolute().top_0().h(px(h)).left(px(band * w)).w(px(half)).bg(linear_gradient(90.0, linear_color_stop(edge, 0.0), linear_color_stop(mid, 1.0)));
        let right = div().absolute().top_0().h(px(h)).left(px(band * w + half)).w(px(half)).bg(linear_gradient(90.0, linear_color_stop(mid, 0.0), linear_color_stop(edge, 1.0)));
        rounded(div().absolute().top_0().left_0().w(px(w)).h(px(h)).overflow_hidden(), radii).child(left).child(right).into_any_element()
    }

    /// A node's role, name and description (`None` = no a11y node).
    pub(crate) fn node_a11y(&self, n: &PlacedNode) -> Option<super::AccessibleInfo> {
        let parent = n.parent.and_then(|p| self.cache.node(p)).map(|p| p.component.as_str());
        let macro_root = self.cache.macro_root(n.index);
        let role = role_of(n, parent, macro_root)?;
        // A macro root is labelled by its title (a Group described by its
        // footer), unless the author named it.
        let title = macro_root.and_then(|_| self.cache.macro_part_text(n.index, "title"));
        let footer = macro_root.filter(|m| m.name == "Group").and_then(|_| self.cache.macro_part_text(n.index, "footer"));
        // The core's `accessibility` defaults: a windowed item's place in the
        // whole list, a heading's level (a List section header = 3).
        let a = n.accessibility.as_ref();
        let count = |k: &str| a.and_then(|a| a.get(k)).and_then(Value::as_u64).map(|v| v as usize);
        let level = (role == gpui::Role::Heading).then(|| count("level").unwrap_or(3));
        let position = count("posInSet").zip(count("setSize"));
        Some(super::AccessibleInfo { role, label: a11y_label(n, self.surface.strings()).or(title), description: a11y_description(n).or(footer), level, position })
    }

    /// The Composer's built-in button names: `(send or stop, attach)`.
    pub(crate) fn composer_labels(&self, n: &PlacedNode) -> (String, String) {
        let busy = n.props.get("busy").and_then(Value::as_bool) == Some(true);
        let submit = n.props.get("submitLabel").and_then(Value::as_str).filter(|s| !s.is_empty());
        let send = if busy { self.builtin_string("stop") } else { submit.map(str::to_string).unwrap_or_else(|| self.builtin_string("send")) };
        (send, self.builtin_string("browse"))
    }

    /// A Carousel dot's name (`pageOf`, 1-based).
    pub(crate) fn carousel_dot_label(&self, page: usize, total: usize) -> String {
        exponential_ui::strings::format_string(&self.builtin_string("pageOf"), json!({"page": page + 1, "total": total}).as_object().expect("object"))
    }

    /// Role, name, description and the part states a reader announces.
    fn accessible(&self, mut el: Stateful<Div>, n: &PlacedNode) -> Stateful<Div> {
        let Some(info) = self.node_a11y(n) else { return el };
        let role = info.role;
        let flags = NodeFlags::of(n);
        el = el.role(role);
        if let Some(label) = info.label {
            el = el.aria_label(label);
        }
        if let Some(d) = info.description {
            el = el.aria_description(d);
        }
        if let Some(l) = info.level {
            el = el.aria_level(l);
        }
        if let Some((at, of)) = info.position {
            el = el.aria_position_in_set(at).aria_size_of_set(of);
        }
        match role {
            gpui::Role::Tab | gpui::Role::ListBoxOption | gpui::Role::GridCell | gpui::Role::Row => el = el.aria_selected(flags.selected),
            gpui::Role::CheckBox | gpui::Role::Switch | gpui::Role::RadioButton | gpui::Role::MenuItemCheckBox => {
                el = el.aria_toggled(if self.checked(n) { gpui::Toggled::True } else { gpui::Toggled::False });
            }
            gpui::Role::ComboBox | gpui::Role::DateInput => el = el.aria_expanded(flags.open),
            gpui::Role::Splitter => {
                let num = |k: &str| n.props.get(k).and_then(Value::as_f64);
                if let Some(v) = num("valueNow") {
                    el = el.aria_numeric_value(v);
                }
                el = el.aria_min_numeric_value(num("valueMin").unwrap_or(0.0)).aria_max_numeric_value(num("valueMax").unwrap_or(100.0));
                el = el.aria_orientation(if n.props.get("orientation").and_then(Value::as_str) == Some("horizontal") { gpui::Orientation::Horizontal } else { gpui::Orientation::Vertical });
            }
            gpui::Role::Slider => {
                let num = |k: &str| n.props.get(k).and_then(Value::as_f64);
                if let Some(v) = num("value") {
                    el = el.aria_numeric_value(v);
                }
                el = el.aria_min_numeric_value(num("min").unwrap_or(0.0)).aria_max_numeric_value(num("max").unwrap_or(100.0));
            }
            gpui::Role::Button if n.component == "Toggle" => {
                el = el.aria_toggled(if flags.selected || n.props.get("pressed").and_then(Value::as_bool) == Some(true) { gpui::Toggled::True } else { gpui::Toggled::False });
            }
            gpui::Role::Button if n.part.as_deref() == Some("trigger") => el = el.aria_expanded(flags.open),
            _ => {}
        }
        el
    }

    fn children_elements(&self, index: u32, place: Place, window: &mut Window, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let Some(n) = self.cache.node(index) else { return Vec::new() };
        let layer = n.layer;
        let mut out = Vec::with_capacity(n.children.len());
        let mut members = Vec::new();
        let pins = n.children.iter().any(|c| self.sticky.contains_key(c));
        for &c in &n.children {
            let Some(child) = self.cache.node(c) else { continue };
            if child.hidden || child.layer != layer {
                continue;
            }
            let Some(el) = self.paint_node(c, place, window, cx) else { continue };
            if pins {
                // Element (= AccessKit) order stays the DOM order; the pinned
                // child paints on top of the siblings scrolling under it.
                let f = self.frames.get(c as usize).copied().unwrap_or_default();
                let (dx, dy) = self.sticky.get(&c).copied().unwrap_or((0.0, 0.0));
                let rect = gpui::Bounds::new(gpui::point(px(f.x - place.origin.0 + dx), px(f.y - place.origin.1 + dy)), gpui::size(px(f.w), px(f.h)));
                members.push(super::pinned::Member { element: el, rect, pinned: self.sticky.contains_key(&c) });
            } else {
                out.push(el);
            }
        }
        if !members.is_empty() {
            out.push(super::pinned::PinnedStack::new(members).into_any_element());
        }
        out
    }

    /// A scroll container: wheel / trackpad scroll the core's offset (a
    /// scroll at an edge chains to the host), overlay scrollbars show the
    /// position and drag.
    #[allow(clippy::too_many_arguments)]
    fn scroll_container(&self, mut el: Stateful<Div>, index: u32, n: &PlacedNode, s: exponential_ui::surface::ScrollOutput, w: f32, h: f32, ink: Hsla, cx: &mut Context<Self>) -> Stateful<Div> {
        let overflows_y = s.scroll_y && s.content_height > h + 0.5;
        let overflows_x = s.scroll_x && s.content_width > w + 0.5;
        if !overflows_x && !overflows_y {
            return el;
        }
        let id = n.id.clone();
        el = el.on_scroll_wheel(cx.listener(move |this, ev: &gpui::ScrollWheelEvent, _window, cx| this.wheel(index, &id, ev, cx)));
        let color = ink.opacity(0.28);
        if let Some((at, len)) = overflows_y.then(|| thumb_geometry(h, s.content_height, s.offset_y, h - 4.0)).flatten() {
            let (sid, ratio) = (n.id.clone(), (s.content_height - h) / (h - 4.0 - len).max(1.0));
            let start = s.offset_y;
            let bar = div()
                .id(SharedString::from(format!("{}.scrollbar-y", n.id)))
                .absolute()
                .top(px(2.0 + at))
                .w(px(6.0))
                .h(px(len))
                .rounded_full()
                .bg(color)
                .on_mouse_down(MouseButton::Left, cx.listener(move |this, ev: &gpui::MouseDownEvent, _, cx| {
                    cx.stop_propagation();
                    this.scroll_drag_start(&sid, true, f32::from(ev.position.y), start, ratio, cx);
                }));
            el = el.child(if self.rtl { bar.left(px(2.0)) } else { bar.right(px(2.0)) });
        }
        if let Some((at, len)) = overflows_x.then(|| thumb_geometry(w, s.content_width, s.offset_x, w - 4.0)).flatten() {
            let (sid, ratio) = (n.id.clone(), (s.content_width - w) / (w - 4.0 - len).max(1.0));
            let start = s.offset_x;
            el = el.child(
                div()
                    .id(SharedString::from(format!("{}.scrollbar-x", n.id)))
                    .absolute()
                    .bottom(px(2.0))
                    .left(px(2.0 + at))
                    .h(px(6.0))
                    .w(px(len))
                    .rounded_full()
                    .bg(color)
                    .on_mouse_down(MouseButton::Left, cx.listener(move |this, ev: &gpui::MouseDownEvent, _, cx| {
                        cx.stop_propagation();
                        this.scroll_drag_start(&sid, false, f32::from(ev.position.x), start, ratio, cx);
                    })),
            );
        }
        el
    }

    /// Pointer / hover handlers of a node.
    fn interactive(&self, mut el: Stateful<Div>, index: u32, n: &PlacedNode, style: &PaintStyle, cx: &mut Context<Self>) -> Stateful<Div> {
        let id = n.id.clone();
        let owner = n.owner_component.as_deref().unwrap_or("");
        // A context Menu opens at the pointer (right click on anything in it).
        if exponential_ui::layout_tree::is_context_menu(&n.component, &n.props) {
            el = el.on_mouse_down(MouseButton::Right, cx.listener(move |this, ev: &gpui::MouseDownEvent, _, cx| {
                cx.stop_propagation();
                this.context_menu(index, ev.position, cx);
            }));
        }
        // FileUpload: OS file drops (the `dragover` state while over it).
        if owner == "FileUpload" && n.part.as_deref() == Some("dropzone") {
            let (drop_id, move_id) = (id.clone(), id.clone());
            el = el
                .on_drag_move::<ExternalPaths>(cx.listener(move |this, ev: &gpui::DragMoveEvent<ExternalPaths>, _, cx| {
                    let inside = ev.bounds.contains(&ev.event.position);
                    if this.set_interaction(&move_id, |s| s.dragover = inside) {
                        cx.notify();
                    }
                }))
                .on_drop(cx.listener(move |this, paths: &ExternalPaths, _, cx| {
                    this.set_interaction(&drop_id, |s| s.dragover = false);
                    this.files_dropped(&drop_id, paths.paths().to_vec(), cx);
                }));
        }
        // A trigger that opens on hover (Tooltip, a hover Popover): its hover
        // state is set after the platform delay; the core opens it.
        if let Some(target) = n.trigger_for.clone().filter(|t| self.opens_on_hover(t)) {
            let hid = id.clone();
            el = el.on_hover(cx.listener(move |this, hovered: &bool, window, cx| this.hover_trigger(&hid, &target, *hovered, window, cx)));
        }
        // Round 2 §1: a Resizable handle drags from an 8 px hit area centred
        // on its hairline; the core resizes from the sizes at the start.
        if owner == "Resizable" && n.part.as_deref() == Some("handle") {
            let vertical = n.props.get("orientation").and_then(Value::as_str) == Some("horizontal");
            let hit = n.props.get("hit").and_then(Value::as_f64).unwrap_or(exponential_ui::layout::RESIZE_HANDLE_HIT) as f32;
            let f = self.frames.get(index as usize).copied().unwrap_or_default();
            let cursor = if vertical { gpui::CursorStyle::ResizeRow } else { gpui::CursorStyle::ResizeColumn };
            let down_id = id.clone();
            let area = div().id(SharedString::from(format!("{id}.hit"))).absolute().cursor(cursor).on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, ev: &gpui::MouseDownEvent, window, cx| {
                    cx.stop_propagation();
                    let p = if vertical { f32::from(ev.position.y) } else { f32::from(ev.position.x) };
                    this.resize_start(&down_id, vertical, p, window, cx);
                }),
            );
            let area = if vertical { area.left_0().w(px(f.w)).top(px((f.h - hit) / 2.0)).h(px(hit)) } else { area.top_0().h(px(f.h)).left(px((f.w - hit) / 2.0)).w(px(hit)) };
            // The hit area (it covers the hairline) owns the hover state.
            let hid = id.clone();
            let area = area.on_hover(cx.listener(move |this, h: &bool, _, cx| this.hover(&hid, *h, cx)));
            return el.child(area);
        }
        let is_slider = n.component == "Slider" && n.part.as_deref() == Some("track");
        if is_slider {
            let (down_id, probe_id) = (id.clone(), id.clone());
            let bounds = self.slider_bounds.clone();
            return el
                .cursor_pointer()
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, ev: &gpui::MouseDownEvent, _, cx| {
                        cx.stop_propagation();
                        this.drag_start(&down_id, f32::from(ev.position.x), cx)
                    }),
                )
                .child(
                    canvas(
                        move |b, _, _| {
                            bounds.borrow_mut().insert(probe_id.clone(), b);
                        },
                        |_, _, _, _| {},
                    )
                    .absolute()
                    .size_full(),
                );
        }
        // A Drawer's handle (or its sheet) drags toward its edge to dismiss.
        if owner == "Drawer" && matches!(n.part.as_deref(), Some("handle" | "content")) {
            let root = if n.part.as_deref() == Some("content") { Some(index) } else { n.owner.as_deref().and_then(|o| self.layers.iter().find(|l| l.owner == o)).map(|l| l.root) };
            if let Some(root) = root {
                el = el.on_mouse_down(MouseButton::Left, cx.listener(move |this, ev: &gpui::MouseDownEvent, _, cx| this.sheet_drag_start(root, ev.position, cx)));
            }
        }
        let toast_root = owner == "Toast" && n.part.as_deref() == Some("root");
        let hoverable = n.pressable || self.cache.hover_styled.get(index as usize).copied().unwrap_or(false) || is_text_field(n) || toast_root || style.transition.is_some();
        if hoverable && n.trigger_for.as_deref().is_none_or(|t| !self.opens_on_hover(t)) {
            let hid = id.clone();
            let toast = toast_root.then(|| n.owner.clone()).flatten();
            el = el.on_hover(cx.listener(move |this, hovered: &bool, window, cx| {
                this.hover(&hid, *hovered, cx);
                if let Some(t) = &toast {
                    this.toast_hovered(t, *hovered, window, cx);
                }
            }));
        }
        if n.pressable && !is_text_field(n) {
            let (down, up, out) = (id.clone(), id.clone(), id);
            el = el
                .when(style.cursor.is_none(), |d| d.cursor_pointer())
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _, _, cx| {
                        cx.stop_propagation();
                        this.press_down(&down, cx)
                    }),
                )
                .on_mouse_up(
                    MouseButton::Left,
                    cx.listener(move |this, _, window, cx| {
                        if this.pressed.as_deref() == Some(up.as_str()) {
                            cx.stop_propagation();
                        }
                        this.press_up(&up, window, cx)
                    }),
                )
                .on_mouse_up_out(MouseButton::Left, cx.listener(move |this, _, _, cx| this.press_cancel(&out, cx)));
        }
        el
    }

    #[allow(clippy::too_many_arguments)]
    fn paint_extension(&self, el: Stateful<Div>, index: u32, n: &PlacedNode, w: f32, h: f32, place: Place, abs: (f32, f32), window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let f = self.frames.get(index as usize).copied().unwrap_or_default();
        let children = self.children_elements(index, Place { origin: (f.x, f.y), abs, clip: place.clip }, window, cx);
        let kind = n.extension_kind.clone().unwrap_or_default();
        let Some(painter) = self.painters.get(&kind).cloned() else {
            // No painter registered: the placeholder note, children kept.
            let ink = self.inks.get(index as usize).copied().unwrap_or(gpui::white());
            return el.child(div().absolute().size_full().flex().items_center().justify_center().text_size(px(12.0)).text_color(ink.opacity(0.6)).child(SharedString::from(kind))).children(children).into_any_element();
        };
        let Some(visual) = self.surface.visual(index).cloned() else { return el.children(children).into_any_element() };
        let text_style = self.texts.get(index as usize).cloned().unwrap_or_default();
        let theme = self.surface.theme().cloned();
        let this = self.this.clone();
        let ctx = PaintContext {
            node: n,
            visual: &visual,
            text_style: &text_style,
            width: w,
            height: h,
            children,
            theme: theme.as_ref(),
            mode: self.surface.mode(),
            emit: Box::new(move |event, payload, window, cx| {
                if let Some(this) = this.upgrade() {
                    let event = event.to_string();
                    this.update(cx, |this, cx| this.fire(index, &event, payload, cx));
                    let _ = window;
                }
            }),
        };
        match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| painter.paint(ctx, window, cx))) {
            Ok(painted) => el.child(painted).into_any_element(),
            Err(panic) => {
                self.paint_failed(n, &panic_message(panic.as_ref()), cx);
                el.into_any_element()
            }
        }
    }

    /// `onPaintError` (`catalog/host.json` paint), deferred out of render.
    pub(crate) fn paint_failed(&self, n: &PlacedNode, why: &str, cx: &mut gpui::App) {
        let host = self.host.clone();
        let error = PaintError { surface_id: self.surface.id.clone(), component_id: n.id.clone(), message: format!("{} failed to paint: {why}", n.component) };
        cx.defer(move |cx| host.on_paint_error(&error, cx));
    }

    /// The content of a measured leaf.
    #[allow(clippy::too_many_arguments)]
    fn paint_leaf(&self, index: u32, n: &PlacedNode, style: &PaintStyle, ink: Hsla, w: f32, h: f32, radii: [f32; 4], window: &mut Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        let i = index as usize;
        let theme = self.surface.theme().cloned();
        let owner_props = self.owner_props(index);
        let states = self.states_of(n);
        let text_style = self.texts.get(i).cloned().unwrap_or_default();
        let lcx = LeafCx {
            node: n,
            w,
            h,
            style,
            ink,
            text_style: &text_style,
            font: self.node_fonts.get(i).cloned().unwrap_or_else(|| self.default_font()),
            theme: theme.as_deref(),
            mode: self.surface.mode(),
            host: self.host.as_ref(),
            states: &states,
            owner_props: &owner_props,
            rtl: style.rtl.unwrap_or(self.rtl),
            reduced_motion: self.surface.settings().reduced_motion,
            radii,
            formatter: self.surface.formatter().as_ref(),
        };
        let part = n.part.as_deref();
        Some(match (n.component.as_str(), part) {
            // An exiting layer's field paints its text (the live input
            // belongs to the open surface).
            _ if is_text_field(n) && self.ghosting.get() => {
                let v = display_text(n.props.get("value"));
                let shown = if v.is_empty() { lcx.str("placeholder").to_string() } else { v };
                natives::text_lines(&lcx, &[shown], Some(1))
            }
            _ if is_text_field(n) => {
                if n.component == "Composer" {
                    self.paint_composer(&lcx, cx)
                } else {
                    self.field_element(&lcx)?
                }
            }
            ("Select" | "DatePicker" | "DateRangePicker" | "TimePicker", Some("trigger")) => {
                let text = lcx.str("text");
                let ph = lcx.str("placeholder");
                let is_placeholder = text.is_empty() || (!ph.is_empty() && text == ph && !has_value(n));
                natives::trigger_content(&lcx, if text.is_empty() { ph } else { text }, is_placeholder, lcx.str("icon"))
            }
            ("Text", _) => self.paint_text(&lcx, window),
            ("Markdown", _) => self.paint_markdown(&lcx, index, window, cx),
            ("Button" | "Toggle", _) => natives::button(&lcx, &n.component, window),
            ("Link", _) => natives::link(&lcx),
            ("Icon", _) => natives::icon(&lcx),
            ("Avatar", _) => natives::avatar(&lcx),
            ("Image", _) => natives::image(&lcx, window, cx),
            ("Video", _) => natives::video(&lcx),
            ("AudioPlayer", _) => natives::audio(&lcx),
            ("Spinner", _) => natives::spinner(&lcx),
            ("Ring", _) => natives::ring(&lcx),
            ("Skeleton", _) => natives::skeleton(&lcx),
            ("Chart", _) => crate::paint::chart::paint(&lcx, self.chart_hover(index), cx.listener(move |this, hovered: &Option<usize>, _, cx| this.chart_hovered(index, *hovered, cx))),
            ("TreeGuides", _) => natives::tree_guides(&lcx),
            ("Unknown", _) => natives::unknown(&lcx),
            ("Checkbox", Some("box" | "checkbox")) if style.native => gpui_component::checkbox::Checkbox::new(SharedString::from(format!("{}.native", n.id))).checked(self.checked(n)).tab_stop(false).into_any_element(),
            ("Switch", Some("track")) if style.native => gpui_component::switch::Switch::new(SharedString::from(format!("{}.native", n.id))).checked(self.checked(n)).into_any_element(),
            ("Checkbox", Some("box" | "checkbox")) => natives::check_box(&lcx, self.checked(n)),
            ("Switch", Some("track")) => natives::switch_thumb(&lcx, self.checked(n)),
            ("Radio", Some("dot")) => {
                if self.checked(n) {
                    self.radio_dot(&lcx)
                } else {
                    return None;
                }
            }
            ("Slider", Some("track")) => {
                let (min, max) = (lcx.num("min").unwrap_or(0.0), lcx.num("max").unwrap_or(100.0));
                let v = self.slider_value(index);
                let frac = if max > min { ((v - min) / (max - min)) as f32 } else { 0.0 };
                natives::slider(&lcx, frac)
            }
            ("Segmented", _) => self.paint_segmented(&lcx, index, window, cx),
            ("Box", Some("indicator")) => self.paint_indicator(&lcx, index, cx),
            ("Input" | "Select" | "DatePicker" | "Textarea" | "NumberField" | "TimePicker" | "DateRangePicker" | "ChipInput", None) => {
                let v = display_text(n.props.get("value"));
                let shown = if v.is_empty() { lcx.str("placeholder").to_string() } else { v };
                natives::text_lines(&lcx, &[shown], Some(1))
            }
            // Geometry mode only: an expanded native paints its parts.
            ("Table" | "CodeBlock" | "FileUpload", None) if n.children.is_empty() => {
                // Geometry mode (controls not expanded): a stand-in label
                // from the strings table (round 2 §3), never the kind name.
                let label = match n.component.as_str() {
                    "CodeBlock" => lcx.str("code").to_string(),
                    "Table" => self.builtin_string("table"),
                    _ => self.builtin_string("dropFiles"),
                };
                natives::text_lines(&lcx, &label.split('\n').map(str::to_string).collect::<Vec<_>>(), None)
            }
            _ => return None,
        })
    }

    /// The checked dot of a Radio, centred in its circle.
    fn radio_dot(&self, lcx: &LeafCx) -> AnyElement {
        let props = lcx.part_props("Radio", "dot", &["checked".to_string()]);
        let dot = PaintStyle::from_visual(&exponential_ui::style::visual(&props, exponential_ui::style::BoxKind::Leaf));
        let (w, h) = (lcx.w, lcx.h);
        let d = px_prop(&props, "width").filter(|d| *d < w.min(h)).unwrap_or(w.min(h) / 2.0);
        let st = PaintStyle { bg: dot.bg.or(lcx.style.color).or(Some(lcx.ink)), radii: [d / 2.0; 4], border: [0.0; 4], ..dot };
        styled_box(div().absolute().left(px((w - d) / 2.0)).top(px((h - d) / 2.0)).size(px(d)), &st, d, d).into_any_element()
    }

    /// Any `Text` leaf: its lines broken exactly as measured, with the
    /// chrome its part carries (a tab's icon and count, an accordion
    /// chevron, an option's check, a sort arrow), typed table cells and the
    /// CodeBlock's tokens.
    fn paint_text(&self, lcx: &LeafCx, window: &mut Window) -> AnyElement {
        let n = lcx.node;
        let owner = n.owner_component.as_deref().unwrap_or("");
        let part = n.part.as_deref();
        let shown_text = display_text(n.props.get("text"));
        let raw = shown_text.as_str();
        let ts = lcx.text_style;
        let (x, y, w, h) = lcx.inner();
        if owner == "Table" && part == Some("cell") {
            match lcx.str("cellType") {
                "boolean" => return natives::bool_cell(lcx, crate::paint::natives::cell_text(n.props.get("value")) == "true"),
                "badge" if !raw.is_empty() => return natives::badge_cell(lcx, raw),
                _ => {}
            }
        }
        if owner == "CodeBlock" && part == Some("code") {
            if let Some(tokens) = n.props.get("tokens").and_then(Value::as_array) {
                return self.paint_code(lcx, tokens);
            }
        }
        let count = n.props.get("count").map(|v| display_text(Some(v))).filter(|s| !s.is_empty());
        let mut shaper = Shaper::new(window, &self.fonts);
        let count_w = count.as_ref().map(|c| shaper.line_width(c, &lcx.font, ts.font_size)).unwrap_or(0.0);
        let (lead, trail) = text_chrome(owner, part, &n.props, lcx.style.gap, count_w);
        let shown = text::transform(raw, lcx.style.text_transform.as_deref()).into_owned();
        // The lines break with the measurer's `letterSpacing`.
        shaper.tracking = ts.letter_spacing.unwrap_or(0.0);
        if lead == 0.0 && trail == 0.0 {
            let clamp = n.lines.filter(|l| *l > 0).map(|l| l as usize);
            let lines = if clamp == Some(1) { Rc::new(vec![shown.replace('\n', " ")]) } else { shaper.lines(&shown, &lcx.font, ts.font_size, Some(w.max(1.0))) };
            // A Table cell centres its line in the row (the web's
            // `align-items: center`).
            if owner == "Table" && matches!(part, Some("cell" | "headerCell")) && h > ts.line_height * lines.len() as f32 {
                let pad = (h - ts.line_height * lines.len().min(clamp.unwrap_or(usize::MAX)) as f32) / 2.0;
                return div().absolute().left_0().top(px(pad)).w(px(lcx.w)).h(px(lcx.h - pad)).child(natives::text_lines(lcx, &lines, clamp)).into_any_element();
            }
            return natives::text_lines(lcx, &lines, clamp);
        }
        // A one-line text with chrome: [lead] text [trail], mirrored in RTL.
        let gap = lcx.style.gap.max(4.0);
        let mut row = lcx.row(lcx.typed(div().absolute().left(px(x)).top(px(y)).w(px(w)).h(px(h)))).items_center().gap(px(gap)).whitespace_nowrap();
        if lead > 0.0 {
            if let Some(icon) = n.props.get("icon").and_then(Value::as_str) {
                row = row.child(div().flex_none().child(icons::concept_mirrored(lcx.host, icon, 16.0, lcx.ink, lcx.rtl)));
            }
        }
        let align = if owner == "Tabs" { "center" } else { lcx.align() };
        if lcx.style.letter_spacing != 0.0 {
            row = row.child(natives::tracked_text(lcx.tracked(vec![shown], align, true)).min_w_0().flex_1().h(px(ts.line_height)));
        } else {
            row = row.child(aligned(div().min_w_0().flex_1().truncate(), align).child(SharedString::from(text::with_paragraph_direction(&shown, lcx.rtl).into_owned())));
        }
        match (owner, part) {
            ("Tabs", Some("tab")) => {
                if let Some(c) = count {
                    let muted = lcx.theme_color("mutedForeground").unwrap_or(lcx.ink);
                    row = row.child(div().flex_none().text_color(muted).child(SharedString::from(c)));
                }
                let selected = NodeFlags::of(n).selected;
                let states: Vec<String> = if selected { vec!["selected".into()] } else { vec![] };
                let ind_props = lcx.part_props("Tabs", "indicator", &states);
                let ind = PaintStyle::from_visual(&exponential_ui::style::visual(&ind_props, exponential_ui::style::BoxKind::Leaf));
                let ind_h = px_prop(&ind_props, "height").unwrap_or(0.0);
                let mut out = div().size_full().child(row.justify_center());
                if selected && ind_h > 0.0 {
                    if let Some(bg) = ind.bg {
                        out = out.child(div().absolute().left_0().bottom_0().w_full().h(px(ind_h)).bg(bg).rounded(px(ind.radius())));
                    }
                }
                return out.into_any_element();
            }
            ("Accordion", Some("trigger")) => {
                if let Some(c) = count {
                    let cp = lcx.part("Accordion", "count", &[]);
                    let muted = cp.color.or_else(|| lcx.theme_color("mutedForeground")).unwrap_or(lcx.ink);
                    row = row.child(div().flex_none().text_color(muted).child(SharedString::from(c)));
                }
                let open = NodeFlags::of(n).open;
                let chevron = icons::concept_rotated(lcx.host, "ui-chevron-down", 16.0, lcx.ink, if open { std::f32::consts::PI } else { 0.0 });
                row = row.child(div().flex_none().child(chevron));
            }
            ("Select", Some("item")) => {
                let check = lcx.str("check");
                let selected = NodeFlags::of(n).selected || lcx.bool("selected");
                row = row.child(div().flex_none().size(px(16.0)).when(selected, |d| d.child(icons::concept(lcx.host, if check.is_empty() { "ui-check" } else { check }, 16.0, lcx.ink))));
            }
            ("Table", Some("headerCell")) => {
                let icon = lcx.str("sortIcon");
                row = row.child(div().flex_none().size(px(16.0)).when(!icon.is_empty(), |d| d.child(icons::concept(lcx.host, icon, 16.0, lcx.ink))));
            }
            _ => {}
        }
        row.into_any_element()
    }

    /// A CodeBlock line: the core tokenizer's tokens, each coloured by the
    /// `CodeBlock/token {kind}` recipe.
    fn paint_code(&self, lcx: &LeafCx, tokens: &[Value]) -> AnyElement {
        let mut text = String::new();
        let mut runs = Vec::with_capacity(tokens.len());
        let mut cache: std::collections::HashMap<String, (Option<Hsla>, Option<u16>, bool)> = std::collections::HashMap::new();
        let theme = self.surface.theme().cloned();
        let mode = self.surface.mode();
        for t in tokens {
            let kind = t.get("kind").and_then(Value::as_str).unwrap_or("plain").to_string();
            let s = t.get("text").and_then(Value::as_str).unwrap_or("");
            if s.is_empty() {
                continue;
            }
            let (color, weight, italic) = cache
                .entry(kind.clone())
                .or_insert_with(|| {
                    let mut props = lcx.owner_props.clone();
                    props.insert("kind".into(), Value::String(kind.clone()));
                    let p = part_props(theme.as_deref(), mode, "CodeBlock", "token", &props, &[]);
                    let v = exponential_ui::style::visual(&p, exponential_ui::style::BoxKind::Leaf);
                    (crate::paint::color::color_of(v.color.as_deref()), v.font_weight, v.font_style.as_deref() == Some("italic"))
                })
                .to_owned();
            text.push_str(s);
            let mut font = lcx.font.clone();
            if let Some(w) = weight {
                font.weight = gpui::FontWeight(w as f32);
            }
            if italic {
                font.style = gpui::FontStyle::Italic;
            }
            runs.push(gpui::TextRun { len: s.len(), font, color: color.unwrap_or(lcx.ink), background_color: None, underline: None, strikethrough: None });
        }
        lcx.content().whitespace_nowrap().child(gpui::StyledText::new(text).with_runs(runs)).into_any_element()
    }

    fn paint_markdown(&self, lcx: &LeafCx, index: u32, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let (x, y, w, h) = lcx.inner();
        let text = lcx.str("text").to_string();
        if let Some(el) = self.host.markdown(&text, lcx.text_style, w, window, cx) {
            return div().absolute().left(px(x)).top(px(y)).w(px(w)).child(el).into_any_element();
        }
        let blocks = {
            let mut cache = self.markdown.borrow_mut();
            match cache.get(&index) {
                Some((t, b)) if *t == text => b.clone(),
                _ => {
                    let b = Rc::new(markdown::parse_for(self.host.as_ref(), &text));
                    cache.insert(index, (text.clone(), b.clone()));
                    b
                }
            }
        };
        let ts = lcx.text_style;
        let body = TextSpec { size: ts.font_size, line_height: ts.line_height, weight: ts.font_weight, family: ts.font_family.clone() };
        let styles = MdStyles::resolve(lcx.theme, lcx.mode, body, &lcx.node.props);
        let mut shaper = Shaper::new(window, &self.fonts);
        let lay = markdown::layout(&blocks, &styles, w, &mut shaper);
        let (colors, muted, border, code_block_bg) = MdPaint::colors(lcx.theme, lcx.mode, lcx.ink, &lcx.node.props);
        let host = self.host.clone();
        let paint = MdPaint {
            family: self.fonts.family(styles.body.family.as_deref()),
            heading_family: self.fonts.family(styles.heading.family.as_deref()),
            mono: self.fonts.family(styles.code.family.as_deref().or(Some(self.fonts.mono.as_ref()))),
            styles,
            colors,
            muted,
            border,
            code_block_bg,
            rtl: lcx.rtl,
            on_link: Rc::new({
                let host = host.clone();
                move |href, _window, cx| host.open_url(href, cx)
            }),
            image: Rc::new(move |src| crate::media::image_source(host.as_ref(), src)),
            node: index,
            units: (!self.ghosting.get()).then(|| self.md_pending.clone()),
            selection: self.md_selection,
            selection_bg: lcx.theme_color("ring").or_else(|| lcx.theme_color("primary")).unwrap_or(gpui::blue()).opacity(0.3),
        };
        // Units commit once painted (their text layouts can hit-test then).
        let (pending, committed) = (self.md_pending.clone(), self.md_units.clone());
        let commit = canvas(|_, _, _| {}, move |_, _, _, _| {
            if let Some(units) = pending.borrow_mut().remove(&index) {
                committed.borrow_mut().insert(index, units);
            }
        })
        .absolute()
        .size_0();
        let mut el = div().id(SharedString::from(format!("{}.md", lcx.node.id))).absolute().left(px(x)).top(px(y)).w(px(w)).h(px(h)).overflow_hidden().child(markdown::paint(&lcx.node.id, &blocks, &lay, w, &paint)).child(commit);
        if !self.ghosting.get() {
            // Selectable text: press-drag selects (the drag follows the
            // pointer anywhere in the window), Shift extends, the platform
            // copy shortcut copies (`copy_selection`).
            el = el
                .cursor_text()
                .on_mouse_down(MouseButton::Left, cx.listener(move |this, ev: &gpui::MouseDownEvent, window, cx| this.md_press(index, ev.position, ev.modifiers.shift, window, cx)));
        }
        el.into_any_element()
    }

    fn paint_composer(&self, lcx: &LeafCx, cx: &mut Context<Self>) -> AnyElement {
        let n = lcx.node;
        let (x, y, w, h) = lcx.inner();
        let send_props = lcx.part_props("Composer", "send", &[]);
        let send = px_prop(&send_props, "height").unwrap_or(36.0);
        let busy = lcx.bool("busy");
        let send_style = lcx.part("Composer", "send", &[]);
        let field_props = lcx.part_props("Composer", "field", &[]);
        let fs = px_prop(&field_props, "fontSize").unwrap_or(lcx.text_style.font_size);
        let field_ink = crate::paint::color::color_of(field_props.get("color").and_then(Value::as_str)).unwrap_or(lcx.ink);
        let gap = lcx.style.gap;
        let field_h = (h - send - gap).max(0.0);
        let empty = self.fields.get(&n.id).is_none_or(|f| f.value(cx).trim().is_empty());
        let mut out = div().absolute().left(px(x)).top(px(y)).w(px(w)).h(px(h));
        if let Some(field) = self.fields.get(&n.id) {
            let el = match &field.input {
                super::input::FieldInput::Multi(s) => gpui_component::input::Textarea::new(s).appearance(false).font(lcx.font.clone()).text_size(px(fs)).text_color(field_ink).w(px(w)).h(px(field_h)).into_any_element(),
                super::input::FieldInput::Line(s) => gpui_component::input::Input::new(s).appearance(false).w(px(w)).h(px(field_h)).into_any_element(),
            };
            out = out.child(div().absolute().left_0().top_0().w(px(w)).h(px(field_h)).child(el));
        }
        let mut bar = lcx.row(div().absolute().left_0().bottom_0().w(px(w)).h(px(send))).items_center().justify_end().gap(px(spacing(lcx.theme, "xs")));
        let idx = self.cache.index_of(&n.id).unwrap_or(0);
        if lcx.bool("attachments") {
            let att = lcx.part("Composer", "attachment", &[]);
            bar = bar.child(
                styled_box(div().id(SharedString::from(format!("{}.attach", n.id))).h(px(send.min(28.0))).px(px(8.0)).flex().items_center().justify_center().cursor_pointer(), &att, 28.0, 28.0)
                    .role(gpui::Role::Button)
                    .aria_label(self.composer_labels(n).1)
                    .on_click(cx.listener(move |this, _, _, cx| this.fire(idx, "attach", None, cx)))
                    .child(icons::concept(lcx.host, "ui-attach", 16.0, att.color.unwrap_or(lcx.ink))),
            );
        }
        let send_ink = send_style.color.unwrap_or(lcx.ink);
        let label = self.composer_labels(n).0;
        let fid = n.id.clone();
        let disabled = !busy && empty;
        bar = bar.child(
            styled_box(div().id(SharedString::from(format!("{}.send", n.id))).size(px(send)).flex().items_center().justify_center(), &send_style, send, send)
                .when(disabled, |d| d.opacity(0.5))
                .when(!disabled, |d| d.cursor_pointer())
                .role(gpui::Role::Button)
                .aria_label(SharedString::from(label))
                .on_click(cx.listener(move |this, _, window, cx| this.submit_composer(&fid, window, cx)))
                .child(icons::concept(lcx.host, if busy { "ui-stop" } else { "ui-send" }, 16.0, send_ink)),
        );
        out.child(bar).into_any_element()
    }

    fn paint_segmented(&self, lcx: &LeafCx, index: u32, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let n = lcx.node;
        let items = n.props.get("items").and_then(Value::as_array).cloned().unwrap_or_default();
        let fill = lcx.bool("fill");
        let current = n.props.get("value").cloned().unwrap_or(Value::Null);
        let chosen: Vec<String> = match &current {
            Value::Array(a) => a.iter().map(|v| display_text(Some(v))).collect(),
            Value::String(s) if lcx.str("type") == "multiple" || s.contains(',') => s.split(',').map(str::to_string).collect(),
            v => vec![display_text(Some(v))],
        };
        // Round 3: `bar` (the old TabBar) = full-width bottom destinations,
        // each item a COLUMN of icon over a caption label sharing the width.
        let bar = lcx.str("variant") == "bar";
        let item_props = lcx.part_props("Segmented", "item", &[]);
        let ih = if bar { lcx.h } else { px_prop(&item_props, "height").unwrap_or(lcx.h) };
        let pad = px_prop(&item_props, "paddingHorizontal").or_else(|| px_prop(&item_props, "padding")).unwrap_or(12.0);
        let (x, y, w, h) = lcx.inner();
        let focused_item = self.focused == Some(index) && self.keyboard;
        let roving = self.group_index(n);
        let mut row = lcx.row(div().absolute().left(px(x)).top(px(y)).w(px(w)).h(px(h))).items_center().gap(px(lcx.style.gap));
        for (k, it) in items.iter().enumerate() {
            let value = it.get("value").cloned().unwrap_or(Value::Null);
            let selected = chosen.contains(&display_text(Some(&value)));
            let disabled = matches!(it.get("disabled"), Some(Value::Bool(true)));
            let item_id = format!("{}.item.{k}", n.id);
            let hovered = self.interaction.get(&item_id).is_some_and(|s| s.hover);
            let mut st: Vec<String> = Vec::new();
            if selected {
                st.push("selected".into());
            }
            if disabled {
                st.push("disabled".into());
            }
            if hovered {
                st.push("hover".into());
            }
            let ring = focused_item && roving == k;
            if ring {
                st.push("focus-visible".into());
            }
            let s = lcx.part("Segmented", "item", &st);
            let color = s.color.unwrap_or(lcx.ink);
            let label = display_text(it.get("label"));
            if bar {
                row = row.child(self.segmented_bar_item(lcx, index, item_id, it, &label, value, &st, &s, color, ring, disabled, ih, window, cx));
                continue;
            }
            let mut item = styled_box(lcx.row(div().id(SharedString::from(item_id.clone())).h(px(ih)).px(px(pad))).items_center().justify_center().gap(px(px_prop(&item_props, "gap").unwrap_or(0.0))).whitespace_nowrap().text_color(color), &s, 80.0, ih)
                .when(fill, |d| d.flex_1())
                .role(gpui::Role::RadioButton)
                .aria_label(SharedString::from(label.clone()))
                .aria_selected(selected);
            if ring && s.shadows.is_empty() {
                let rc = lcx.theme_color("ring").unwrap_or(lcx.ink.opacity(0.5));
                item = item.border_2().border_color(rc);
            }
            if !disabled {
                let hid = item_id.clone();
                item = item
                    .cursor_pointer()
                    .on_hover(cx.listener(move |this, h: &bool, _, cx| this.hover(&hid, *h, cx)))
                    .on_click(cx.listener(move |this, _, window, cx| this.segmented_select(index, value.clone(), window, cx)));
            }
            if let Some(icon) = it.get("icon").and_then(Value::as_str) {
                item = item.child(icons::concept(self.host.as_ref(), icon, 16.0, color));
            }
            if !label.is_empty() {
                let shown = crate::text::transform(&label, lcx.style.text_transform.as_deref()).into_owned();
                if lcx.style.letter_spacing != 0.0 {
                    // Tracked as measured: the item's size and weight.
                    let size = px_prop(&item_props, "fontSize").unwrap_or(lcx.text_style.font_size);
                    let weight = item_props.get("fontWeight").and_then(Value::as_u64).map(|w| w as u16).unwrap_or(500);
                    let font = crate::measure::make_font(lcx.font.family.clone(), weight);
                    let tw = natives::tracked_width(window, &font, size, &shown, lcx.style.letter_spacing);
                    let t = natives::Tracked { font, size, ink: color, ..lcx.tracked(vec![shown], "left", false) };
                    item = item.child(natives::tracked_text(t).flex_none().w(px(tw)).h(px(lcx.text_style.line_height)));
                } else {
                    item = item.child(SharedString::from(shown));
                }
            }
            row = row.child(item);
        }
        row.into_any_element()
    }

    /// One `bar` Segmented item: a column (the `icon` part over the caption
    /// `label` part), `flexGrow 1` / `flexBasis 0`, a navigation button
    /// whose selected item is the current page.
    #[allow(clippy::too_many_arguments)]
    fn segmented_bar_item(&self, lcx: &LeafCx, index: u32, item_id: String, it: &Value, label: &str, value: Value, st: &[String], s: &PaintStyle, color: Hsla, ring: bool, disabled: bool, h: f32, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let selected = st.iter().any(|x| x == "selected");
        let icon_ink = lcx.part("Segmented", "icon", st).color.unwrap_or(color);
        let label_ink = lcx.part("Segmented", "label", st).color.unwrap_or(color);
        let label_props = lcx.part_props("Segmented", "label", st);
        let caption = crate::measure::bar_caption(lcx.theme, lcx.mode, lcx.text_style);
        let weight = label_props.get("fontWeight").and_then(Value::as_u64).map(|w| w as u16).unwrap_or(lcx.text_style.font_weight);
        let icon_size = crate::paint::parts::control(lcx.theme, "iconMd", 20.0);
        let (xxs, xs) = (spacing(lcx.theme, "xxs"), spacing(lcx.theme, "xs"));
        let mut item = styled_box(div().id(SharedString::from(item_id.clone())).flex().flex_col().items_center().justify_center().gap(px(xxs)).flex_1().flex_basis(px(0.0)).min_w(px(0.0)).h(px(h)).py(px(xs)), s, 80.0, h)
            .role(gpui::Role::Button)
            .aria_label(SharedString::from(label.to_string()))
            .aria_selected(selected);
        if ring && s.shadows.is_empty() {
            let rc = lcx.theme_color("ring").unwrap_or(lcx.ink.opacity(0.5));
            item = item.border_2().border_color(rc);
        }
        if !disabled {
            let hid = item_id.clone();
            item = item
                .cursor_pointer()
                .on_hover(cx.listener(move |this, h: &bool, _, cx| this.hover(&hid, *h, cx)))
                .on_click(cx.listener(move |this, _, window, cx| this.segmented_select(index, value.clone(), window, cx)));
        }
        if let Some(icon) = it.get("icon").and_then(Value::as_str) {
            item = item.child(icons::concept(self.host.as_ref(), icon, icon_size, icon_ink));
        }
        if !label.is_empty() {
            let font = crate::measure::make_font(lcx.font.family.clone(), weight);
            let _ = window;
            item = item.child(
                div()
                    .max_w_full()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis()
                    .font(font)
                    .text_size(px(caption.font_size))
                    .line_height(px(caption.line_height))
                    .text_color(label_ink)
                    .child(SharedString::from(label.to_string())),
            );
        }
        item.into_any_element()
    }

    fn paint_indicator(&self, lcx: &LeafCx, index: u32, cx: &mut Context<Self>) -> AnyElement {
        let count = lcx.num("count").unwrap_or(0.0).max(0.0) as usize;
        let page = lcx.num("page").unwrap_or(0.0).max(0.0) as usize;
        let owner = self.cache.owner_of(index);
        let gap = spacing(lcx.theme, "xs");
        let mut row = lcx.row(div().absolute().left_0().bottom_0().w(px(lcx.w)).h(px(lcx.h))).items_center().justify_center().gap(px(gap)).overflow_hidden();
        for i in 0..count {
            let states: Vec<String> = if i == page { vec!["selected".into()] } else { vec![] };
            let s = lcx.part("Carousel", "indicator", &states);
            let props = lcx.part_props("Carousel", "indicator", &states);
            let size = px_prop(&props, "width").unwrap_or(8.0).min(lcx.h.max(1.0));
            let fallback = if i == page { lcx.theme_color("primary") } else { lcx.theme_color("border") };
            let s = PaintStyle { bg: s.bg.or(fallback), radii: [size / 2.0; 4], ..s };
            row = row.child(
                styled_box(div().id(SharedString::from(format!("{}.dot.{i}", lcx.node.id))).flex_none().size(px(size)), &s, size, size)
                    .cursor_pointer()
                    .role(gpui::Role::Tab)
                    .aria_label(SharedString::from(self.carousel_dot_label(i, count)))
                    .aria_selected(i == page)
                    .on_click(cx.listener(move |this, _, _, cx| this.fire(owner, "change", Some(json!({"page": i})), cx))),
            );
        }
        row.into_any_element()
    }

    /// Whether a layer is placed against the viewport (not an anchor).
    fn viewport_layer(layer: &Layer) -> bool {
        layer.placement.is_none() && layer.position != "point"
    }

    /// An open layer: the scrim (modal ones), the content root at its
    /// surface-coordinate frame (viewport-placed ones moved into the region
    /// the host shows), the enter animation, outside-press dismissal.
    pub(crate) fn paint_layer(&mut self, k: usize, layer: &Layer, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        self.paint_layer_at(k, layer, None, window, cx)
    }

    /// A layer at an explicit progress (`exit` = an exiting ghost's 1 → 0
    /// fade: not hit-tested, no dismissal), else its enter animation's.
    pub(crate) fn paint_layer_at(&mut self, k: usize, layer: &Layer, exit: Option<f32>, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let root = layer.root;
        let ghost = exit.is_some();
        let shift = if Self::viewport_layer(layer) { self.visible.top } else { 0.0 };
        let progress = exit.or_else(|| self.motion.layer_progress(&layer.owner, self.now()));
        let mut container = div().absolute().left_0().top(px(shift));
        if layer.modal {
            let theme = self.surface.theme().cloned();
            let owner_props = self.cache.index_of(&layer.owner).and_then(|o| self.cache.node(o)).map(|o| o.props.clone()).unwrap_or_default();
            let overlay = PaintStyle::from_visual(&part_visual(theme.as_deref(), self.surface.mode(), &layer.kind, "overlay", &owner_props, &["open".to_string()]));
            let scrim = overlay.bg.unwrap_or(gpui::black().opacity(0.5));
            let height = if self.visible.height > 0.0 { self.visible.height } else { self.surface_height.max(self.viewport_height) };
            let scrim_el = div()
                .id(SharedString::from(format!("{}.overlay{}", layer.owner, if ghost { ".exit" } else { "" })))
                .absolute()
                .left_0()
                .top_0()
                .w(px(self.width))
                .h(px(height))
                .bg(scrim)
                .when_some(progress, |d, p| d.opacity(p));
            container = container.child(if ghost {
                scrim_el
            } else {
                scrim_el.occlude().on_mouse_down(MouseButton::Left, cx.listener(move |this, _, window, cx| {
                    cx.stop_propagation();
                    this.dismiss_layer_at(root, window, cx);
                }))
            });
        }
        if self.cache.node(root).is_some_and(|n| !n.hidden) {
            let f = self.frames.get(root as usize).copied().unwrap_or_default();
            let place = Place { origin: (f.x, f.y), abs: (f.x, f.y + shift), clip: None };
            let inner = self.paint_node(root, Place { origin: (f.x, f.y), ..place }, window, cx);
            // The root div sits at its own frame inside a wrapper at (0,0);
            // the wrapper owns the occlusion and the outside press.
            let rise = progress.map(|p| (1.0 - p) * if layer.position == "top" { -8.0 } else { 8.0 }).unwrap_or(0.0);
            let (dx, dy) = self.sheet_drag.as_ref().filter(|d| d.root == root && !ghost).map(|d| d.offset).unwrap_or((0.0, 0.0));
            let mut wrap = div().id(SharedString::from(format!("{}.{}.{k}", layer.owner, if ghost { "exit" } else { "layer" }))).absolute().left(px(f.x + dx)).top(px(f.y + rise + dy)).w(px(f.w)).h(px(f.h)).when(!ghost, |d| d.occlude()).when_some(progress, |d, p| d.opacity(p));
            if let Some(el) = inner {
                wrap = wrap.child(el);
            }
            if ghost {
                return container.child(wrap).into_any_element();
            }
            if self.opens_on_hover(&layer.owner) {
                let owner = layer.owner.clone();
                wrap = wrap.on_hover(cx.listener(move |this, hovered: &bool, window, cx| this.hover_card(&owner, *hovered, window, cx)));
            }
            let outside = !layer.modal && layer.class == LayerClass::Overlay && layer.kind != "Tooltip";
            if outside {
                wrap = wrap.on_mouse_down_out(cx.listener(move |this, ev: &gpui::MouseDownEvent, window, cx| this.outside_press(root, ev.position, window, cx)));
            }
            container = container.child(wrap);
        }
        container.into_any_element()
    }
}

/// Did the user pick something (the trigger shows a value, not the
/// placeholder)?
fn has_value(n: &PlacedNode) -> bool {
    let set = |k: &str| match n.props.get(k) {
        None | Some(Value::Null) => false,
        Some(Value::String(s)) => !s.is_empty(),
        Some(Value::Array(a)) => !a.is_empty(),
        _ => true,
    };
    set("value") || set("start") || set("end")
}

/// A linear gradient over the box: gpui paints two stops; more stops are
/// chained as segments along an axis-aligned angle (others use the end
/// stops).
fn gradient(g: &crate::paint::GradientPaint, w: f32, h: f32, radii: [f32; 4]) -> AnyElement {
    let first = g.stops[0];
    let last = *g.stops.last().unwrap_or(&first);
    if g.stops.len() == 2 || (g.angle % 90.0).abs() > 0.01 {
        return rounded(div().absolute().top_0().left_0().w(px(w)).h(px(h)).bg(linear_gradient(g.angle, linear_color_stop(first.0, first.1), linear_color_stop(last.0, last.1))), radii).into_any_element();
    }
    // Axis-aligned, 3+ stops: one 2-stop segment per pair.
    let a = g.angle.rem_euclid(360.0);
    let horizontal = (a - 90.0).abs() < 0.01 || (a - 270.0).abs() < 0.01;
    let reversed = (a - 270.0).abs() < 0.01 || a.abs() < 0.01;
    let len = if horizontal { w } else { h };
    let mut out = rounded(div().absolute().top_0().left_0().w(px(w)).h(px(h)).overflow_hidden(), radii);
    for pair in g.stops.windows(2) {
        let (s0, s1) = (pair[0], pair[1]);
        let (o0, o1) = if reversed { (1.0 - s1.1, 1.0 - s0.1) } else { (s0.1, s1.1) };
        let (c0, c1) = if reversed { (s1.0, s0.0) } else { (s0.0, s1.0) };
        let start = o0 * len;
        let size = ((o1 - o0) * len).max(0.0);
        let seg = div().absolute().bg(linear_gradient(if horizontal { 90.0 } else { 180.0 }, linear_color_stop(c0, 0.0), linear_color_stop(c1, 1.0)));
        out = out.child(if horizontal { seg.top_0().h(px(h)).left(px(start)).w(px(size)) } else { seg.left_0().w(px(w)).top(px(start)).h(px(size)) });
    }
    out.into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rounded_clips_pass_to_children_in_their_corners() {
        let card = Clip { x: 0.0, y: 0.0, w: 200.0, h: 100.0, radii: [12.0; 4] };
        assert_eq!(inherit_radii([0.0; 4], (0.0, 0.0, 200.0, 60.0), &card), [12.0, 12.0, 0.0, 0.0], "a flush header image rounds its top corners");
        assert_eq!(inherit_radii([0.0; 4], (4.0, 4.0, 192.0, 92.0), &card), [8.0; 4], "an inset child takes the radius minus the inset");
        assert_eq!(inherit_radii([0.0; 4], (20.0, 20.0, 20.0, 20.0), &card), [0.0; 4], "a child away from the corners keeps square ones");
        assert_eq!(inherit_radii([16.0, 0.0, 0.0, 0.0], (0.0, 0.0, 10.0, 10.0), &card)[0], 16.0, "its own larger radius wins");
    }

    #[test]
    fn scrollbar_thumbs_track_the_offset() {
        assert_eq!(thumb_geometry(100.0, 100.0, 0.0, 96.0), None);
        let (at, len) = thumb_geometry(100.0, 400.0, 0.0, 96.0).unwrap();
        assert_eq!((at, len), (0.0, 24.0));
        let (at, _) = thumb_geometry(100.0, 400.0, 300.0, 96.0).unwrap();
        assert_eq!(at, 72.0);
    }
}

/// A caught panic's message (`&str` / `String` payloads).
pub(crate) fn panic_message(panic: &(dyn std::any::Any + Send)) -> String {
    panic.downcast_ref::<&str>().map(|s| s.to_string()).or_else(|| panic.downcast_ref::<String>().cloned()).unwrap_or_else(|| "panicked".into())
}
