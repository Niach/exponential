//! #LOGS-4 — an animation frame repaints only the glyph that animates.
//!
//! gpui re-renders EVERY ancestor view of a notified view, and a dirty
//! cached view re-renders its whole subtree (`window.refreshing`). A working
//! mark inside the cached dock therefore re-rendered the issue page, its
//! timeline and every markdown block on each of its frames, and a mark in
//! the rail rebuilt the whole sidebar document.
//!
//! So a long-lived glyph ([`stepped_glyph`]) inside a [`glyph_scope`] does
//! not paint in place: it reserves its box, and records where it sits (its
//! bounds and clip) and how it paints. One [`GlyphLayer`] view per window,
//! painted after the content, paints every recorded glyph at its animation's
//! frame and wakes itself on the shared step grid. A tick notifies only the
//! layer: the window's root re-renders (cheap), the cached regions around the
//! glyphs are reused as they were.
//!
//! A scope is the outside of a cached region ([`GlyphRegion`]): its glyphs
//! stay recorded while the region is reused (nothing inside it ran), are
//! recorded again when the region re-renders (its [`scope_marker`] ran), and
//! go with the scope when it leaves the tree. A glyph outside any scope, an
//! unfocused or hidden window and Reduce motion paint in place as
//! [`SteppedAnimationExt`](super::SteppedAnimationExt) does.

use std::collections::HashMap;
use std::rc::Rc;
use std::time::{Duration, Instant};

use gpui::{
    div, px, AnyElement, AnyView, Animation, App, AvailableSpace, Bounds, ContentMask, Context,
    Element, ElementId, EntityId, GlobalElementId, InspectorElementId, IntoElement, LayoutId,
    ParentElement, Pixels, Render, SharedString, Size, Style, Styled as _, Window, WindowId,
};

use super::animation::{aligned_start, step_epoch, until_next_step, until_shared_tick};

/// How a glyph paints at an animation progress (0…1, eased).
pub type GlyphPaint = Rc<dyn Fn(f32, &mut Window, &mut App) -> AnyElement>;

#[derive(Clone)]
struct Glyph {
    bounds: Bounds<Pixels>,
    mask: ContentMask<Pixels>,
    start: Instant,
    animation: Rc<Animation>,
    step: Duration,
    paint: GlyphPaint,
    /// The ink and text size where it sits (an icon without its own colour
    /// or size reads them).
    text: gpui::TextStyleRefinement,
}

#[derive(Default)]
struct Scope {
    glyphs: Vec<Glyph>,
    /// Recorded during the current pass of the scope.
    pending: Vec<Glyph>,
    /// The region inside re-rendered during the current pass.
    rerendered: bool,
    /// The layer frame this scope was last in the tree for.
    seen: u64,
}

#[derive(Default)]
struct WindowGlyphs {
    scopes: HashMap<SharedString, Scope>,
    stack: Vec<SharedString>,
    frame: u64,
    armed_until: Option<Instant>,
}

#[derive(Default)]
struct GlyphLayers(HashMap<WindowId, WindowGlyphs>);

impl gpui::Global for GlyphLayers {}

fn glyphs<'a>(window: &Window, cx: &'a mut App) -> &'a mut WindowGlyphs {
    cx.default_global::<GlyphLayers>().0.entry(window.window_handle().window_id()).or_default()
}

/// The eased progress of `animation` `elapsed` into it (a loop wraps).
fn progress(animation: &Animation, elapsed: Duration) -> f32 {
    let mut delta = elapsed.as_secs_f32() / animation.duration.as_secs_f32().max(f32::EPSILON);
    if delta > 1.0 {
        delta = if animation.oneshot { 1.0 } else { delta % 1.0 };
    }
    (animation.easing)(delta)
}

/// Whether a glyph shows its static face: Reduce motion, or a window that
/// is not the focused one (#LOGS-4: a background IDE draws nothing). gpui
/// refreshes a window when its focus changes, so the loop resumes by itself.
pub fn animation_paused(window: &Window, cx: &App) -> bool {
    cx.reduce_motion() || !window.is_window_active()
}

// ---------------------------------------------------------------------------
// The glyph
// ---------------------------------------------------------------------------

/// A `size` glyph looping `animation`, repainted every `step` (snapped to
/// the shared grid): `paint` draws it at a progress. Inside a
/// [`glyph_scope`] the [`GlyphLayer`] paints it; elsewhere it paints in
/// place.
pub fn stepped_glyph(
    id: impl Into<ElementId>,
    size: Size<Pixels>,
    animation: Animation,
    step: Duration,
    paint: impl Fn(f32, &mut Window, &mut App) -> AnyElement + 'static,
) -> SteppedGlyph {
    SteppedGlyph { id: id.into(), size, animation: Rc::new(animation), step, paint: Rc::new(paint) }
}

pub struct SteppedGlyph {
    id: ElementId,
    size: Size<Pixels>,
    animation: Rc<Animation>,
    step: Duration,
    paint: GlyphPaint,
}

impl IntoElement for SteppedGlyph {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

struct GlyphState {
    start: Instant,
    armed_until: Option<Instant>,
}

impl Element for SteppedGlyph {
    type RequestLayoutState = ();
    type PrepaintState = Option<AnyElement>;

    fn id(&self) -> Option<ElementId> {
        Some(self.id.clone())
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(&mut self, _: Option<&GlobalElementId>, _: Option<&InspectorElementId>, window: &mut Window, cx: &mut App) -> (LayoutId, ()) {
        let mut style = Style::default();
        style.size = self.size.map(|d| d.into());
        style.flex_shrink = 0.;
        (window.request_layout(style, [], cx), ())
    }

    fn prepaint(&mut self, global_id: Option<&GlobalElementId>, _: Option<&InspectorElementId>, bounds: Bounds<Pixels>, _: &mut (), window: &mut Window, cx: &mut App) -> Option<AnyElement> {
        let _span = tracing::enabled!(tracing::Level::TRACE).then(|| {
            let id = format!("{:?}", self.id);
            tracing::trace_span!("anim.step", id = id.as_str()).entered()
        });
        let now = cx.background_executor().now();
        let epoch = step_epoch(now, cx);
        let step = self.step;
        let paused = animation_paused(window, cx);
        let scoped = !paused && glyphs(window, cx).stack.last().is_some();
        let (start, arm) = window.with_element_state(global_id.unwrap(), |state: Option<GlyphState>, _| {
            let mut state = state.unwrap_or_else(|| GlyphState { start: aligned_start(now, step, epoch), armed_until: None });
            let arm = !paused && !scoped && state.armed_until.is_none_or(|until| now >= until);
            if arm {
                state.armed_until = Some(now + until_shared_tick(now, now + until_next_step(now.saturating_duration_since(epoch), step), epoch));
            }
            ((state.start, arm.then(|| state.armed_until.unwrap() - now)), state)
        });
        if scoped {
            let style = window.text_style();
            let text = gpui::TextStyleRefinement { color: Some(style.color), font_size: Some(style.font_size), ..Default::default() };
            let glyph = Glyph { bounds, mask: window.content_mask(), start, animation: self.animation.clone(), step, paint: self.paint.clone(), text };
            let layer = glyphs(window, cx);
            let scope = layer.stack.last().cloned().expect("scoped");
            layer.scopes.entry(scope).or_default().pending.push(glyph);
            return None;
        }
        let delta = if paused { (self.animation.easing)(if self.animation.oneshot { 1. } else { 0. }) } else { progress(&self.animation, now.saturating_duration_since(start)) };
        let mut element = (self.paint)(delta, window, cx);
        element.prepaint_as_root(bounds.origin, bounds.size.map(AvailableSpace::Definite), window, cx);
        if let Some(wait) = arm {
            let view = window.current_view();
            notify_after(view, wait, cx);
        }
        Some(element)
    }

    fn paint(&mut self, _: Option<&GlobalElementId>, _: Option<&InspectorElementId>, _: Bounds<Pixels>, _: &mut (), element: &mut Option<AnyElement>, window: &mut Window, cx: &mut App) {
        if let Some(element) = element {
            element.paint(window, cx);
        }
    }
}

fn notify_after(view: EntityId, wait: Duration, cx: &mut App) {
    cx.spawn(async move |cx| {
        cx.background_executor().timer(wait).await;
        cx.update(|cx| cx.notify(view));
    })
    .detach();
}

// ---------------------------------------------------------------------------
// Scopes
// ---------------------------------------------------------------------------

/// The outside of a cached region: the glyphs `child` (a [`GlyphRegion`],
/// cached) records go to the [`GlyphLayer`].
pub fn glyph_scope(scope: impl Into<SharedString>, child: impl IntoElement) -> GlyphScope {
    GlyphScope { scope: scope.into(), child: Some(child.into_any_element()) }
}

pub struct GlyphScope {
    scope: SharedString,
    child: Option<AnyElement>,
}

impl IntoElement for GlyphScope {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for GlyphScope {
    type RequestLayoutState = AnyElement;
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(&mut self, _: Option<&GlobalElementId>, _: Option<&InspectorElementId>, window: &mut Window, cx: &mut App) -> (LayoutId, AnyElement) {
        let mut child = self.child.take().expect("laid out once");
        (child.request_layout(window, cx), child)
    }

    fn prepaint(&mut self, _: Option<&GlobalElementId>, _: Option<&InspectorElementId>, _: Bounds<Pixels>, child: &mut AnyElement, window: &mut Window, cx: &mut App) {
        {
            let layer = glyphs(window, cx);
            let frame = layer.frame;
            let scope = layer.scopes.entry(self.scope.clone()).or_default();
            scope.pending.clear();
            scope.rerendered = false;
            scope.seen = frame;
            layer.stack.push(self.scope.clone());
        }
        child.prepaint(window, cx);
        let layer = glyphs(window, cx);
        layer.stack.pop();
        let scope = layer.scopes.entry(self.scope.clone()).or_default();
        // A reused region ran nothing inside: its glyphs stand.
        if scope.rerendered {
            scope.glyphs = std::mem::take(&mut scope.pending);
        }
    }

    fn paint(&mut self, _: Option<&GlobalElementId>, _: Option<&InspectorElementId>, _: Bounds<Pixels>, child: &mut AnyElement, _: &mut (), window: &mut Window, cx: &mut App) {
        child.paint(window, cx);
    }
}

/// Marks its scope's region as re-rendered (it runs only when the region
/// does): its glyphs are recorded afresh, none = gone.
fn scope_marker() -> ScopeMarker {
    ScopeMarker
}

struct ScopeMarker;

impl IntoElement for ScopeMarker {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for ScopeMarker {
    type RequestLayoutState = ();
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(&mut self, _: Option<&GlobalElementId>, _: Option<&InspectorElementId>, window: &mut Window, cx: &mut App) -> (LayoutId, ()) {
        let mut style = Style::default();
        style.position = gpui::Position::Absolute;
        (window.request_layout(style, [], cx), ())
    }

    fn prepaint(&mut self, _: Option<&GlobalElementId>, _: Option<&InspectorElementId>, _: Bounds<Pixels>, _: &mut (), window: &mut Window, cx: &mut App) {
        let layer = glyphs(window, cx);
        if let Some(scope) = layer.stack.last().cloned() {
            layer.scopes.entry(scope).or_default().rerendered = true;
        }
    }

    fn paint(&mut self, _: Option<&GlobalElementId>, _: Option<&InspectorElementId>, _: Bounds<Pixels>, _: &mut (), _: &mut (), _: &mut Window, _: &mut App) {}
}

/// A cached region's view: its `children` views, filling it, and the
/// [`scope_marker`]. Render it `.cached(..)` inside a [`glyph_scope`]; a
/// notify of any view inside re-renders it (gpui dirties ancestors).
pub struct GlyphRegion {
    children: Vec<AnyView>,
}

impl GlyphRegion {
    pub fn new(children: Vec<AnyView>) -> Self {
        Self { children }
    }
}

impl Render for GlyphRegion {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().size_full().child(scope_marker()).children(self.children.iter().cloned())
    }
}

// ---------------------------------------------------------------------------
// The layer
// ---------------------------------------------------------------------------

/// Paints the window's scoped glyphs; render it after the content (before
/// any modal layer), once per window.
#[derive(Default)]
pub struct GlyphLayer;

impl Render for GlyphLayer {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        LayerElement
    }
}

struct LayerElement;

impl IntoElement for LayerElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for LayerElement {
    type RequestLayoutState = ();
    type PrepaintState = Vec<(ContentMask<Pixels>, gpui::TextStyleRefinement, AnyElement)>;

    fn id(&self) -> Option<ElementId> {
        Some("glyph-layer".into())
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(&mut self, _: Option<&GlobalElementId>, _: Option<&InspectorElementId>, window: &mut Window, cx: &mut App) -> (LayoutId, ()) {
        let mut style = Style::default();
        style.position = gpui::Position::Absolute;
        style.size = gpui::size(px(0.), px(0.)).map(|d| d.into());
        (window.request_layout(style, [], cx), ())
    }

    fn prepaint(&mut self, _: Option<&GlobalElementId>, _: Option<&InspectorElementId>, _: Bounds<Pixels>, _: &mut (), window: &mut Window, cx: &mut App) -> Vec<(ContentMask<Pixels>, gpui::TextStyleRefinement, AnyElement)> {
        let now = cx.background_executor().now();
        let epoch = step_epoch(now, cx);
        let (live, armed) = {
            let layer = glyphs(window, cx);
            let frame = layer.frame;
            layer.frame += 1;
            // A scope that was not in the tree this frame took its glyphs.
            layer.scopes.retain(|_, scope| scope.seen == frame);
            let live: Vec<Glyph> = layer.scopes.values().flat_map(|scope| scope.glyphs.iter().cloned()).collect();
            (live, layer.armed_until)
        };
        let mut next: Option<Duration> = None;
        let mut painted = Vec::with_capacity(live.len());
        for (i, glyph) in live.into_iter().enumerate() {
            let delta = progress(&glyph.animation, now.saturating_duration_since(glyph.start));
            let mut element = window.with_text_style(Some(glyph.text.clone()), |window| (glyph.paint)(delta, window, cx));
            window.with_element_namespace(ElementId::Integer(i as u64), |window| {
                window.with_text_style(Some(glyph.text.clone()), |window| {
                    window.with_content_mask(Some(glyph.mask.clone()), |window| {
                        element.prepaint_as_root(glyph.bounds.origin, glyph.bounds.size.map(AvailableSpace::Definite), window, cx);
                    });
                });
            });
            painted.push((glyph.mask, glyph.text, element));
            let due = now + until_next_step(now.saturating_duration_since(epoch), glyph.step);
            let wait = until_shared_tick(now, due, epoch);
            next = Some(next.map_or(wait, |next| next.min(wait)));
        }
        if let Some(wait) = next {
            if armed.is_none_or(|until| now >= until) {
                glyphs(window, cx).armed_until = Some(now + wait);
                notify_after(window.current_view(), wait, cx);
            }
        }
        painted
    }

    fn paint(&mut self, _: Option<&GlobalElementId>, _: Option<&InspectorElementId>, _: Bounds<Pixels>, _: &mut (), painted: &mut Vec<(ContentMask<Pixels>, gpui::TextStyleRefinement, AnyElement)>, window: &mut Window, cx: &mut App) {
        for (i, (mask, text, element)) in painted.iter_mut().enumerate() {
            window.with_element_namespace(ElementId::Integer(i as u64), |window| {
                window.with_text_style(Some(text.clone()), |window| {
                    window.with_content_mask(Some(mask.clone()), |window| element.paint(window, cx));
                });
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use gpui::{AppContext as _, Entity, StyleRefinement, TestAppContext, VisualTestContext};

    use super::*;

    const STEP: Duration = Duration::from_millis(90);

    struct Inner {
        renders: Rc<Cell<usize>>,
        paints: Rc<Cell<usize>>,
        working: bool,
    }

    impl Render for Inner {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            self.renders.set(self.renders.get() + 1);
            let paints = self.paints.clone();
            div().size_full().children(self.working.then(|| {
                stepped_glyph("mark", gpui::size(px(12.), px(12.)), Animation::new(Duration::from_millis(720)).repeat(), STEP, move |_, _, _| {
                    paints.set(paints.get() + 1);
                    div().size_full().into_any_element()
                })
            }))
        }
    }

    struct Root {
        region: Entity<GlyphRegion>,
        layer: Entity<GlyphLayer>,
    }

    impl Render for Root {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            div()
                .size_full()
                .child(div().size(px(100.)).child(glyph_scope("region", self.region.clone().cached(StyleRefinement::default().size_full()))))
                .child(self.layer.clone())
        }
    }

    fn open(cx: &mut TestAppContext) -> (Entity<Inner>, Rc<Cell<usize>>, Rc<Cell<usize>>, &mut VisualTestContext) {
        let renders = Rc::new(Cell::new(0));
        let paints = Rc::new(Cell::new(0));
        let (inner, renders2, paints2) = (Rc::new(Cell::new(None)), renders.clone(), paints.clone());
        let slot = inner.clone();
        let (_, cx) = cx.add_window_view(move |_, cx| {
            let inner = cx.new(|_| Inner { renders: renders2, paints: paints2, working: true });
            slot.set(Some(inner.clone()));
            let region = cx.new(|_| GlyphRegion::new(vec![inner.into()]));
            let layer = cx.new(|_| GlyphLayer);
            Root { region, layer }
        });
        cx.update(|window, _| window.activate_window());
        cx.run_until_parked();
        (inner.take().unwrap(), renders, paints, cx)
    }

    /// The heart of #LOGS-4: a glyph's frames repaint the glyph through the
    /// layer; the cached region around it never renders again.
    #[gpui::test]
    async fn a_scoped_glyph_animates_without_re_rendering_its_region(cx: &mut TestAppContext) {
        let (_, renders, paints, cx) = open(cx);
        let (renders_before, paints_before) = (renders.get(), paints.get());
        cx.executor().advance_clock(Duration::from_secs(1));
        cx.run_until_parked();
        assert_eq!(renders.get(), renders_before, "the region is reused");
        let frames = paints.get() - paints_before;
        assert!((10..=12).contains(&frames), "the glyph repainted once per 90 ms frame, {frames}");
    }

    /// A region that re-renders without its glyph takes it off the layer.
    #[gpui::test]
    async fn a_glyph_that_leaves_its_region_leaves_the_layer(cx: &mut TestAppContext) {
        let (inner, _, paints, cx) = open(cx);
        inner.update(cx, |inner, cx| {
            inner.working = false;
            cx.notify();
        });
        cx.run_until_parked();
        let before = paints.get();
        cx.executor().advance_clock(Duration::from_secs(1));
        cx.run_until_parked();
        assert_eq!(paints.get(), before, "nothing left to paint");
    }

    /// An unfocused window paints the static face and schedules nothing.
    #[gpui::test]
    async fn an_unfocused_window_does_not_animate(cx: &mut TestAppContext) {
        let (_, _, paints, cx) = open(cx);
        cx.deactivate_window();
        cx.run_until_parked();
        let before = paints.get();
        cx.executor().advance_clock(Duration::from_secs(1));
        cx.run_until_parked();
        assert_eq!(paints.get(), before, "no frames while unfocused");
    }
}
