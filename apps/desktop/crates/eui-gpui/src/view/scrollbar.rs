//! #EXP-1629: a scroll container's overlay scrollbar is gpui-component's
//! own [`Scrollbar`], so an eUI surface in the IDE shows exactly the IDE's
//! scrollbars (the theme's thumb, inset and width, shown on scroll, fading
//! out after the idle delay, wider under the pointer, draggable). The core
//! owns the offset: [`CoreScroll`] mirrors it for the scrollbar and holds a
//! thumb drag's offset until the view's next render hands it to the core.

use std::cell::RefCell;
use std::rc::Rc;

use gpui::{point, size, Bounds, Pixels, Point, Size};
use gpui_component::scroll::ScrollbarHandle;

#[derive(Default)]
struct Cell {
    /// The container's node index (its `scrolls` entry).
    index: u32,
    /// gpui's sign: negative as the content moves up / left.
    offset: Point<Pixels>,
    content: Size<Pixels>,
    /// A thumb drag's offset the core has not taken yet.
    pending: Option<Point<Pixels>>,
}

/// One scroll container's offset as gpui-component's scrollbar reads and
/// drags it.
#[derive(Clone, Default)]
pub(crate) struct CoreScroll(Rc<RefCell<Cell>>);

impl CoreScroll {
    /// The core's offset and content size (px, positive) for this paint;
    /// a drag's offset still pending wins.
    pub(crate) fn sync(&self, index: u32, offset: (f32, f32), content: (f32, f32)) {
        let mut cell = self.0.borrow_mut();
        cell.index = index;
        cell.content = size(gpui::px(content.0), gpui::px(content.1));
        if cell.pending.is_none() {
            cell.offset = point(gpui::px(-offset.0), gpui::px(-offset.1));
        }
    }

    /// The dragged offset (px, positive) and the container's index, once.
    pub(crate) fn take_pending(&self) -> Option<(u32, f32, f32)> {
        let mut cell = self.0.borrow_mut();
        let p = cell.pending.take()?;
        Some((cell.index, -f32::from(p.x), -f32::from(p.y)))
    }
}

impl ScrollbarHandle for CoreScroll {
    // The scrollbar is laid out over the container (`viewport_from_layout`).
    fn viewport_bounds(&self) -> Bounds<Pixels> {
        Bounds::default()
    }

    fn offset(&self) -> Point<Pixels> {
        self.0.borrow().offset
    }

    fn set_offset(&self, offset: Point<Pixels>) {
        let mut cell = self.0.borrow_mut();
        cell.offset = offset;
        cell.pending = Some(offset);
    }

    fn content_size(&self) -> Size<Pixels> {
        self.0.borrow().content
    }
}
