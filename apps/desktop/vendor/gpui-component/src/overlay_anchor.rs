//! Exponential patch (#EXP-1739): an anchored overlay closes when its source
//! moves, scrolls, is clipped away or leaves the tree.
//!
//! THE one mechanism behind every overlay that hangs off something on screen:
//! tooltips, context menus, popovers and dropdowns.
//!
//! - **Source:** [`report_anchor`] (or the [`AnchorSource`] element) records
//!   the source's window bounds and whether its content mask still shows it,
//!   every frame it prepaints. The record lives in the source's ELEMENT STATE,
//!   so it survives a cached view's replay (a replayed frame did not move it)
//!   and gpui drops it at the end of the first frame the source is no longer
//!   in, which is exactly "removed or hidden".
//! - **Overlay:** [`watch_anchor`] snapshots the anchor when the overlay opens
//!   and calls `on_lost` once its bounds change, it is clipped out of view or
//!   its record is gone. The returned [`AnchorGuard`] unregisters on drop:
//!   close the overlay, drop the guard.
//! - **Sweep:** the window-state layer gpui-base's `Root` mounts after the
//!   window's content carries [`sweep_layer`] (`root.rs`); it
//!   defers one check per frame past the draw, when this frame's bounds are
//!   in and a removed source's record has been dropped.
//! - **Text:** a menu opened at a point (a right click on chat text) anchors
//!   to the smallest live source under that point ([`anchor_at`]), so the
//!   text it was opened on is what it follows.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::{Rc, Weak};

use gpui::{
    AnyElement, App, Bounds, Element, ElementId, Global, GlobalElementId, InspectorElementId,
    IntoElement, LayoutId, Pixels, Point, Styled as _, Window, WindowId, canvas,
};

/// One source's last reported geometry.
struct AnchorCell {
    window: WindowId,
    bounds: Cell<Bounds<Pixels>>,
    visible: Cell<bool>,
}

/// A handle on a source's geometry record; dead once the source leaves the tree.
#[derive(Clone)]
pub struct OverlayAnchor(Weak<AnchorCell>);

impl OverlayAnchor {
    /// The source's bounds as last reported, `None` once it is gone or clipped
    /// out of view.
    pub fn bounds(&self) -> Option<Bounds<Pixels>> {
        let cell = self.0.upgrade()?;
        cell.visible.get().then(|| cell.bounds.get())
    }
}

/// The sources of every window (weak: a source's element state owns its record).
#[derive(Default)]
struct Sources(HashMap<WindowId, Vec<Weak<AnchorCell>>>);

impl Global for Sources {}

/// Records `bounds` as this frame's geometry of the overlay source `id` (a
/// child id of the element prepainting now) and returns its anchor.
pub fn report_anchor(
    id: impl Into<ElementId>,
    bounds: Bounds<Pixels>,
    window: &mut Window,
    cx: &mut App,
) -> OverlayAnchor {
    let visible = window.content_mask().bounds.intersects(&bounds);
    let window_id = window.window_handle().window_id();
    window.with_global_id(id.into(), |global_id, window| {
        window.with_element_state::<Rc<AnchorCell>, _>(global_id, |cell, _| {
            let cell = cell.unwrap_or_else(|| {
                let cell = Rc::new(AnchorCell {
                    window: window_id,
                    bounds: Cell::new(bounds),
                    visible: Cell::new(visible),
                });
                let sources = &mut cx.default_global::<Sources>().0;
                let list = sources.entry(window_id).or_default();
                list.retain(|source| source.strong_count() > 0);
                list.push(Rc::downgrade(&cell));
                cell
            });
            cell.bounds.set(bounds);
            cell.visible.set(visible);
            (OverlayAnchor(Rc::downgrade(&cell)), cell)
        })
    })
}

/// The smallest live, visible source under `point` in `window` (a menu opened
/// at the pointer follows what it was opened on).
pub fn anchor_at(point: Point<Pixels>, window: &Window, cx: &App) -> Option<OverlayAnchor> {
    let window_id = window.window_handle().window_id();
    let sources = cx.try_global::<Sources>()?.0.get(&window_id)?;
    sources
        .iter()
        .filter_map(|weak| Some((weak, weak.upgrade()?)))
        .filter(|(_, cell)| cell.window == window_id && cell.visible.get() && cell.bounds.get().contains(&point))
        .min_by(|(_, a), (_, b)| {
            let area = |cell: &AnchorCell| {
                let size = cell.bounds.get().size;
                f32::from(size.width) * f32::from(size.height)
            };
            area(a).total_cmp(&area(b))
        })
        .map(|(weak, _)| OverlayAnchor(weak.clone()))
}

type OnLost = Box<dyn FnOnce(&mut Window, &mut App)>;

struct Watch {
    anchor: OverlayAnchor,
    opened: Bounds<Pixels>,
    on_lost: Option<OnLost>,
    live: Rc<Cell<bool>>,
}

impl Watch {
    fn lost(&self) -> bool {
        self.anchor.bounds() != Some(self.opened)
    }
}

/// The open overlays' watches, per window.
#[derive(Default)]
struct Watches(HashMap<WindowId, Rc<RefCell<Vec<Watch>>>>);

impl Global for Watches {}

/// Unregisters its watch when dropped (the overlay closed on its own).
#[must_use = "the watch ends when the guard drops"]
pub struct AnchorGuard(Rc<Cell<bool>>);

impl Drop for AnchorGuard {
    fn drop(&mut self) {
        self.0.set(false);
    }
}

/// Calls `on_lost` (once, after the frame that shows it) as soon as `anchor`
/// moves from where it is now, is clipped out of view or leaves the tree.
/// `None` (and no call) when the anchor is already gone.
pub fn watch_anchor(
    anchor: OverlayAnchor,
    window: &mut Window,
    cx: &mut App,
    on_lost: impl FnOnce(&mut Window, &mut App) + 'static,
) -> Option<AnchorGuard> {
    let opened = anchor.bounds()?;
    let live = Rc::new(Cell::new(true));
    let window_id = window.window_handle().window_id();
    let watches = cx.default_global::<Watches>().0.entry(window_id).or_default().clone();
    watches.borrow_mut().push(Watch { anchor, opened, on_lost: Some(Box::new(on_lost)), live: live.clone() });
    // A frame must follow for the sweep to see the anchor again.
    window.refresh();
    Some(AnchorGuard(live))
}

/// Closes the overlays whose anchors were lost; runs once the frame is drawn.
fn sweep(window: &mut Window, cx: &mut App) {
    let window_id = window.window_handle().window_id();
    let Some(watches) = cx.try_global::<Watches>().and_then(|all| all.0.get(&window_id).cloned()) else {
        return;
    };
    let lost: Vec<OnLost> = {
        let mut watches = watches.borrow_mut();
        watches.retain(|watch| watch.live.get());
        watches
            .iter_mut()
            .filter(|watch| watch.lost())
            .filter_map(|watch| {
                watch.live.set(false);
                watch.on_lost.take()
            })
            .collect()
    };
    for on_lost in lost {
        on_lost(window, cx);
    }
    if !watches.borrow().iter().any(|watch| watch.live.get()) {
        cx.global_mut::<Watches>().0.remove(&window_id);
    }
}

/// The per-frame sweep, mounted by the window-state layer after the window's content: once
/// the frame is drawn (sources reported, removed ones dropped) the lost
/// overlays close.
pub fn sweep_layer() -> impl IntoElement {
    canvas(
        |_, window, cx| {
            let window_id = window.window_handle().window_id();
            let open = cx.try_global::<Watches>().is_some_and(|all| all.0.contains_key(&window_id));
            if open {
                let handle = window.window_handle();
                cx.defer(move |cx| {
                    let _ = handle.update(cx, |_, window, cx| sweep(window, cx));
                });
            }
        },
        |_, _, _, _| {},
    )
    .absolute()
    .size_0()
}

/// An element that reports its own bounds as an overlay source every frame
/// ([`report_anchor`]) and hands its anchor to `on_anchor`.
pub struct AnchorSource {
    id: ElementId,
    child: AnyElement,
    on_anchor: Option<Box<dyn Fn(OverlayAnchor, &mut Window, &mut App)>>,
}

/// Wraps `child` as the overlay source `id`.
pub fn anchor_source(id: impl Into<ElementId>, child: impl IntoElement) -> AnchorSource {
    AnchorSource { id: id.into(), child: child.into_any_element(), on_anchor: None }
}

impl AnchorSource {
    /// Called with the anchor every frame the source prepaints.
    pub fn on_anchor(mut self, f: impl Fn(OverlayAnchor, &mut Window, &mut App) + 'static) -> Self {
        self.on_anchor = Some(Box::new(f));
        self
    }
}

impl IntoElement for AnchorSource {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for AnchorSource {
    type RequestLayoutState = ();
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
        Some(self.id.clone())
    }

    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, ()) {
        (self.child.request_layout(window, cx), ())
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        let anchor = report_anchor("overlay-anchor", bounds, window, cx);
        if let Some(on_anchor) = &self.on_anchor {
            on_anchor(anchor, window, cx);
        }
        self.child.prepaint(window, cx);
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        _: &mut (),
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        self.child.paint(window, cx);
    }
}
