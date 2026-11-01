//! Pointer grabs: moving (with magnetic edges and edge tiling) and resizing.

pub mod moving;
pub mod resize;

pub use moving::MoveSurfaceGrab;
pub use resize::ResizeSurfaceGrab;

/// linux/input-event-codes.h
pub const BTN_LEFT: u32 = 0x110;
