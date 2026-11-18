//! #EDE-90: a desk is what belongs to one person (or, later, one agent
//! space): the seat and what it holds down, the windows and their
//! workspaces, the clipboard, the cursor.
//! [`crate::state::Ede`] holds the person's as a field ([`Ede::desk`]) and
//! the agent spaces' beside it ([`crate::spaces`]). What acts on a desk
//! takes its [`DeskId`] (focus, the `drive_*` paths, the clipboard,
//! placement, the hold, the protected and lock checks) and finds the desk
//! with [`Ede::desk`].
//!
//! [`Ede::desk`]: crate::state::Ede::desk

use std::time::Instant;

use smithay::{
    desktop::{Space, Window},
    input::{pointer::CursorImageStatus, Seat},
    reexports::wayland_server::Client,
};

use crate::state::{ClientState, Ede};

/// Which desk: the person's (the default), or another one's number (#EDE-90).
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DeskId(u32);

impl DeskId {
    pub const PERSON: DeskId = DeskId(0);

    /// The id of desk `n` beside the person's; 0 is the person's.
    pub(crate) const fn numbered(n: u32) -> DeskId {
        DeskId(n)
    }
}

/// The desk something belongs to, kept where a lookup would come too late
/// or find nothing: in the client data of Xwayland (#EDE-125).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeskTag(pub DeskId);

/// The desk a Wayland client belongs to: the one its socket gave it
/// ([`ClientState::desk`]), or the tag its Xwayland was started with. A
/// client with neither is nobody's: it is on no desk, the person's included.
pub fn desk_of_client(client: &Client) -> Option<DeskId> {
    if let Some(state) = client.get_data::<ClientState>() {
        return Some(state.desk);
    }
    #[cfg(feature = "xwayland")]
    if let Some(x) = client.get_data::<smithay::xwayland::XWaylandClientData>() {
        return x.user_data().get::<DeskTag>().map(|tag| tag.0);
    }
    None
}

/// The filter of every global that is the person's alone: their seat and
/// outputs, the layer shell, the session lock, input methods, the virtual
/// keyboard, decorations, and GPU buffers.
pub fn on_person(client: &Client) -> bool {
    desk_of_client(client) == Some(DeskId::PERSON)
}

/// The filter of a global that belongs to desk `id` alone.
pub fn on_desk(id: DeskId) -> impl Fn(&Client) -> bool + Send + Sync + 'static {
    move |client| desk_of_client(client) == Some(id)
}

pub struct Desk {
    pub id: DeskId,
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

impl Ede {
    /// The desk `id` names.
    ///
    /// A space's id is good only while the space lives: this is called
    /// with [`DeskId::PERSON`], or with an id found in the same call
    /// (nothing in between can close a space). What keeps an id across a
    /// turn of the loop asks [`Ede::try_desk`] and returns on `None`.
    pub fn desk(&self, id: DeskId) -> &Desk {
        self.try_desk(id).expect("the person's desk or a live space")
    }

    pub fn desk_mut(&mut self, id: DeskId) -> &mut Desk {
        self.try_desk_mut(id).expect("the person's desk or a live space")
    }

    /// The desk a window lives on: the person's, the only one.
    pub fn desk_of_window(&self, _window: &smithay::desktop::Window) -> DeskId {
        self.desk.id
    }

    /// The desk an ede-protocol connection drives: the person's, the only one.
    pub(crate) fn desk_of_conn(&self, _conn: u64) -> DeskId {
        self.desk.id
    }

    /// The session lock covers the person's desk.
    pub fn desk_locked(&self, id: DeskId) -> bool {
        id == DeskId::PERSON && self.is_locked()
    }
}
