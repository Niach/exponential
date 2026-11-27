//! Pointer grabs: moving (with magnetic edges and edge tiling) and resizing.

pub mod moving;
pub mod resize;

pub use moving::MoveSurfaceGrab;
pub use resize::{ResizeEdge, ResizeSurfaceGrab};

/// linux/input-event-codes.h
pub const BTN_LEFT: u32 = 0x110;
/// #EDE-243: Alt+right-drag resizes.
pub const BTN_RIGHT: u32 = 0x111;

use smithay::{
    desktop::Window,
    utils::SERIAL_COUNTER,
};

use crate::state::Ede;

/// Ends a move or resize of `window` that is running (an X11 client
/// maximizing or going fullscreen mid-drag, #EDE-222): the drag would pull
/// it off its new geometry with the next motion.
pub fn end_grab_of(data: &mut Ede, window: &Window) {
    let pointer = data.desk.seat.get_pointer().unwrap();
    let ours = pointer
        .with_grab(|_, g| {
            g.downcast_ref::<MoveSurfaceGrab>().is_some_and(|m| &m.window == window)
                || g.downcast_ref::<ResizeSurfaceGrab>().is_some_and(|r| r.window() == window)
        })
        .unwrap_or(false);
    if ours {
        pointer.unset_grab(data, SERIAL_COUNTER.next_serial(), smithay::backend::input::InputTime::now());
    }
}

/// Muffin's tile preview: while a move hovers a screen edge, the tile a
/// release there would fill, drawn under the moving window.
pub struct TilePreview {
    /// The window being moved and the tile's rectangle (title bar included).
    pub shown: Option<(Window, crate::layout::Rect)>,
    /// The fill and its four border lines: the same ids from frame to frame,
    /// so only a changed rectangle is damage.
    pub ids: [smithay::backend::renderer::element::Id; 5],
}

impl Default for TilePreview {
    fn default() -> Self {
        Self { shown: None, ids: std::array::from_fn(|_| smithay::backend::renderer::element::Id::new()) }
    }
}

impl TilePreview {
    /// Shows `rect` for `window` (`None` = hides it, if it is `window`'s).
    pub fn show(&mut self, window: &Window, rect: Option<crate::layout::Rect>) {
        match rect {
            Some(r) => self.shown = Some((window.clone(), r)),
            None => {
                if self.shown.as_ref().is_some_and(|(w, _)| w == window) {
                    self.shown = None;
                }
            }
        }
    }

    /// The preview drawn under `window`, if it is the one being moved.
    pub fn of(&self, window: &Window) -> Option<crate::layout::Rect> {
        self.shown.as_ref().filter(|(w, _)| w == window).map(|(_, r)| *r)
    }
}
