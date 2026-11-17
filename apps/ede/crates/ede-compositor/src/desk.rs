//! #EDE-90: a desk is what belongs to one person (or, later, one agent
//! space): the seat and what it holds down, the windows and their
//! workspaces, the clipboard, the cursor.
//! [`crate::state::Ede`] holds exactly one, the person's ([`Ede::desk`]).
//!
//! [`Ede::desk`]: crate::state::Ede::desk

use std::time::Instant;

use smithay::{
    desktop::{Space, Window},
    input::{pointer::CursorImageStatus, Seat},
};

use crate::state::Ede;

pub struct Desk {
    pub seat: Seat<Ede>,
    /// What the pointer looks like (a client's cursor surface, hidden, or
    /// the default arrow); backends that draw their own cursor read it.
    pub cursor_status: CursorImageStatus,
    /// The virtual keyboard whose keymap the seat holds (until a real key).
    pub virtual_keymap: Option<smithay::input::keyboard::KeyboardSource>,
    /// An X11 window that was focused before it had a wl_surface.
    #[cfg(feature = "xwayland")]
    pub pending_x11_focus: Option<smithay::xwayland::X11Surface>,
    /// #EDE-88: the X side of the keyboard focus. X delivers keys to its
    /// own input focus, not to the surface Wayland focused, so the X11
    /// window holding the keyboard is entered as an X11 keyboard target
    /// (SetInputFocus or WM_TAKE_FOCUS, then `_NET_ACTIVE_WINDOW`)
    /// through this seat, which no client binds: the Wayland enter itself
    /// stays with `seat` ([`Ede::sync_x11_focus`]).
    #[cfg(feature = "xwayland")]
    pub x11_focus: (Seat<Ede>, Option<smithay::xwayland::X11Surface>),
    /// The modifiers held for `InputEvent::Keysym` (#EDE-22).
    pub keysyms: crate::keysym::Keysyms,
    /// `hold_input` (#EDE-23).
    pub hold: crate::hold::Hold,
    /// The keys and buttons each connection drove down (#EDE-73).
    pub driven: crate::protected::Driven,
    /// The seat's CLIPBOARD serial, owner and reads ([`crate::clipboard`]).
    pub clipboard: crate::clipboard::Clipboard,
    /// Mapped windows, back to front.
    pub space: Space<Window>,
    /// Toplevels that exist but have not committed a buffer yet: they are
    /// placed when their first frame arrives, so the size is known.
    pub unmapped: Vec<Window>,
    /// Windows in the taskbar only (#EDE-48, [`crate::minimize`]): out of
    /// the space, their place and workspace remembered.
    pub minimized: Vec<Window>,
    /// The active workspace and the parked windows ([`crate::workspaces`]).
    pub workspaces: crate::workspaces::Workspaces,
    /// The last press on a title bar, for the double-click.
    pub title_click: Option<(Window, Instant)>,
}
