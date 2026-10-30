//! Siblings with pinned (`position: sticky` / sticky section header) members:
//! laid out and prepainted in DOM order, so the AccessKit tree, focus and
//! reading order stay the core's, but the pinned ones PAINT last (on top of
//! the siblings scrolling under them). Prepaint clips the other siblings'
//! hitboxes out of the pinned boxes, so a press there lands on the pinned one.

use gpui::{point, px, relative, size, AnyElement, App, Bounds, ContentMask, Element, ElementId, GlobalElementId, InspectorElementId, IntoElement, LayoutId, Pixels, Position, Style, Window};

/// One child: its element, its box relative to the stack's origin and
/// whether it is pinned.
pub(crate) struct Member {
    pub element: AnyElement,
    pub rect: Bounds<Pixels>,
    pub pinned: bool,
}

pub(crate) struct PinnedStack {
    members: Vec<Member>,
}

impl PinnedStack {
    pub(crate) fn new(members: Vec<Member>) -> Self {
        Self { members }
    }
}

/// The part of the plane a sibling keeps beside a pinned box: of the four
/// half-planes outside `pin`, the one holding most of `rect` (a row under a
/// top-pinned header keeps what lies below the header).
pub(crate) fn outside(rect: Bounds<Pixels>, pin: Bounds<Pixels>, within: Bounds<Pixels>) -> Bounds<Pixels> {
    let (l, t, r, b) = (within.left(), within.top(), within.right(), within.bottom());
    let side = |x0: Pixels, y0: Pixels, x1: Pixels, y1: Pixels| Bounds::from_corners(point(x0.max(l), y0.max(t)), point(x1.min(r).max(x0.max(l)), y1.min(b).max(y0.max(t))));
    let candidates = [side(l, t, r, pin.top()), side(l, pin.bottom(), r, b), side(l, t, pin.left(), b), side(pin.right(), t, r, b)];
    let area = |c: &Bounds<Pixels>| {
        let i = c.intersect(&rect);
        f32::from(i.size.width).max(0.0) * f32::from(i.size.height).max(0.0)
    };
    candidates.into_iter().max_by(|a, b| area(a).total_cmp(&area(b))).unwrap_or(within)
}

impl IntoElement for PinnedStack {
    type Element = Self;

    fn into_element(self) -> Self {
        self
    }
}

impl Element for PinnedStack {
    type RequestLayoutState = ();
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }

    fn request_layout(&mut self, _: Option<&GlobalElementId>, _: Option<&InspectorElementId>, window: &mut Window, cx: &mut App) -> (LayoutId, ()) {
        let ids: Vec<LayoutId> = self.members.iter_mut().map(|m| m.element.request_layout(window, cx)).collect();
        let mut style = Style { position: Position::Absolute, ..Style::default() };
        style.inset.top = px(0.0).into();
        style.inset.left = px(0.0).into();
        style.size = size(relative(1.0).into(), relative(1.0).into());
        (window.request_layout(style, ids, cx), ())
    }

    fn prepaint(&mut self, _: Option<&GlobalElementId>, _: Option<&InspectorElementId>, bounds: Bounds<Pixels>, _: &mut (), window: &mut Window, cx: &mut App) {
        let at = |r: Bounds<Pixels>| Bounds { origin: r.origin + bounds.origin, size: r.size };
        let pins: Vec<Bounds<Pixels>> = self.members.iter().filter(|m| m.pinned).map(|m| at(m.rect)).collect();
        for m in &mut self.members {
            if m.pinned {
                let _ = m.element.prepaint(window, cx);
                continue;
            }
            let rect = at(m.rect);
            let mut mask = window.content_mask().bounds;
            for pin in pins.iter().filter(|p| p.intersects(&rect)) {
                mask = outside(rect, *pin, mask);
            }
            window.with_content_mask(Some(ContentMask { bounds: mask }), |window| {
                let _ = m.element.prepaint(window, cx);
            });
        }
    }

    fn paint(&mut self, _: Option<&GlobalElementId>, _: Option<&InspectorElementId>, _: Bounds<Pixels>, _: &mut (), _: &mut (), window: &mut Window, cx: &mut App) {
        for pinned in [false, true] {
            for m in self.members.iter_mut().filter(|m| m.pinned == pinned) {
                m.element.paint(window, cx);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn b(x: f32, y: f32, w: f32, h: f32) -> Bounds<Pixels> {
        Bounds::new(point(px(x), px(y)), size(px(w), px(h)))
    }

    #[test]
    fn a_row_under_a_top_header_keeps_what_lies_below_it() {
        let m = outside(b(0.0, 10.0, 300.0, 40.0), b(0.0, 0.0, 300.0, 24.0), b(0.0, 0.0, 300.0, 400.0));
        assert_eq!(m, b(0.0, 24.0, 300.0, 376.0));
    }

    #[test]
    fn a_cell_beside_a_pinned_first_column_keeps_its_right_side() {
        let m = outside(b(40.0, 0.0, 80.0, 30.0), b(0.0, 0.0, 60.0, 30.0), b(0.0, 0.0, 400.0, 30.0));
        assert_eq!(m, b(60.0, 0.0, 340.0, 30.0));
    }
}
