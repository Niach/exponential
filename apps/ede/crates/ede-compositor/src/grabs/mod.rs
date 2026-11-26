//! Pointer grabs: moving (with magnetic edges and edge tiling) and resizing.

pub mod moving;
pub mod resize;

pub use moving::MoveSurfaceGrab;
pub use resize::{ResizeEdge, ResizeSurfaceGrab};

/// linux/input-event-codes.h
pub const BTN_LEFT: u32 = 0x110;

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
