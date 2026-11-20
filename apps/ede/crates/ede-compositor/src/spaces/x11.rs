//! #EDE-138: an Xwayland per space. X11 apps launched into a space get a
//! display of their own (`DISPLAY=:<10 or more>`): an Xwayland whose
//! Wayland client is tagged with the space's desk, so it binds the space's
//! seat and output only (F1's global filters), and whose window manager
//! puts its windows on the space's desk. Started at the space's first
//! launch (an argv does not say whether an app is X11; an X server per idle
//! space would be memory for nothing), ended with the space.
//!
//! Every [`XwmHandler`](smithay::xwayland::XwmHandler) call names its window
//! manager; [`Ede::desk_of_xwm`] says whose it is, and the handler takes the
//! person's branch for the person's alone. What a space's window manager
//! asks that this module does not answer yet is dropped, never the
//! person's to act on.
//!
//! An Xwayland that dies while its space lives (a crash) is let go of: its
//! display number and socket are free again, and the space's next launch
//! starts another one.

use std::process::Stdio;

use smithay::{
    desktop::Window,
    reexports::{calloop::RegistrationToken, wayland_server::Client},
    utils::Rectangle,
    xwayland::{xwm::XwmId, X11Surface, X11Wm, XWayland, XWaylandEvent},
};

use smithay::wayland::selection::data_device::clear_data_device_selection;

use crate::{
    clipboard::Owner,
    desk::{DeskId, DeskTag},
    state::Ede,
};

/// The display numbers a space's Xwayland takes: from :10 on, so a space
/// never takes `:0` (the person's apps' default), also when the person's
/// Xwayland is off.
pub const FIRST_DISPLAY: u32 = 10;
/// Where the search for a free display number gives up.
const LAST_DISPLAY: u32 = 99;

/// The Xwayland of this compositor serving `display`: a child of this
/// process (smithay keeps its handle to itself).
fn xwayland_process(display: u32) -> Option<smithay::reexports::rustix::process::Pid> {
    let me = std::process::id();
    let wanted = format!(":{display}");
    std::fs::read_dir("/proc").ok()?.flatten().filter_map(|entry| entry.file_name().to_str()?.parse::<i32>().ok()).find_map(|pid| {
        let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
        let parent: u32 = stat.rsplit_once(')')?.1.split_whitespace().nth(1)?.parse().ok()?;
        let cmdline = std::fs::read(format!("/proc/{pid}/cmdline")).ok()?;
        let mut args = cmdline.split(|b| *b == 0);
        let ours = parent == me && args.next().is_some_and(|program| program.ends_with(b"Xwayland")) && args.any(|arg| arg == wanted.as_bytes());
        ours.then(|| smithay::reexports::rustix::process::Pid::from_raw(pid)).flatten()
    })
}

/// A space's Xwayland.
pub(crate) struct SpaceX11 {
    /// The `XWayland` source in the loop: removing it kills the server's
    /// Wayland client and frees its display's lock and socket.
    token: RegistrationToken,
    /// Its Wayland client (the space's desk's).
    client: Client,
    pub(crate) display: u32,
    /// Its window manager, once the server is ready.
    xwm: Option<X11Wm>,
    /// Its X authority file ([`crate::xauth`]), removed at the close.
    pub(crate) xauthority: std::path::PathBuf,
}

impl SpaceX11 {
    /// Its Wayland client.
    pub fn client(&self) -> &Client {
        &self.client
    }
}

/// Whose an X11 window manager is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum XwmDesk {
    Person,
    Space(DeskId),
    /// A closed space's, or one that is gone: nothing it asks is acted on.
    Gone,
}

impl XwmDesk {
    /// The desk it acts on, `None` for a gone one.
    pub(crate) fn desk(self) -> Option<DeskId> {
        match self {
            XwmDesk::Person => Some(DeskId::PERSON),
            XwmDesk::Space(desk) => Some(desk),
            XwmDesk::Gone => None,
        }
    }
}

impl Ede {
    /// Whose window manager `xwm` is.
    pub(crate) fn desk_of_xwm(&self, xwm: XwmId) -> XwmDesk {
        if self.xwm.as_ref().is_some_and(|wm| wm.id() == xwm) {
            return XwmDesk::Person;
        }
        match self.spaces.iter().find(|space| space.x11.as_ref().and_then(|x11| x11.xwm.as_ref()).is_some_and(|wm| wm.id() == xwm)) {
            Some(space) => XwmDesk::Space(space.desk.id),
            None => XwmDesk::Gone,
        }
    }

    /// Desk `desk`'s X11 window manager: the person's, or the space's own
    /// once its Xwayland is ready.
    pub(crate) fn desk_xwm(&mut self, desk: DeskId) -> Option<&mut X11Wm> {
        if desk == DeskId::PERSON {
            return self.xwm.as_mut();
        }
        self.spaces.get_mut(desk)?.x11.as_mut()?.xwm.as_mut()
    }

    /// The X11 window holding space `desk`'s keyboard is one of window
    /// manager `xwm`'s: only that client reads the space's clipboard.
    pub(crate) fn space_x11_focused(&self, desk: DeskId, xwm: XwmId) -> bool {
        let Some(d) = self.try_desk(desk) else { return false };
        let Some(focus) = d.seat.get_keyboard().and_then(|keyboard| keyboard.current_focus()) else { return false };
        d.space.elements().any(|w| w.x11_surface().is_some_and(|x| x.xwm_id() == Some(xwm) && x.wl_surface().as_ref() == Some(&focus)))
    }

    /// Space `desk`'s Xwayland runs and its window manager with it.
    pub fn space_xwm_ready(&self, desk: DeskId) -> bool {
        self.spaces.get(desk).and_then(|space| space.x11.as_ref()).is_some_and(|x11| x11.xwm.is_some())
    }

    /// A space's window manager, or a closed space's that is still parked.
    pub(crate) fn space_xwm_state(&mut self, xwm: XwmId) -> Option<&mut X11Wm> {
        if let Some(at) = self.spaces.closing_xwms.iter().position(|wm| wm.id() == xwm) {
            return self.spaces.closing_xwms.get_mut(at);
        }
        self.spaces.list.iter_mut().filter_map(|space| space.x11.as_mut()?.xwm.as_mut()).find(|wm| wm.id() == xwm)
    }

    /// Space `desk`'s Xwayland, started now if it has none and X11 apps
    /// run here at all ([`super::Spaces::x11`]); its display number.
    pub(crate) fn space_x11(&mut self, desk: DeskId) -> Option<u32> {
        if !self.spaces.x11 {
            return None;
        }
        if let Some(x11) = &self.spaces.get(desk)?.x11 {
            return Some(x11.display);
        }
        let tag = self.x11_tag();
        let space = self.spaces.get(desk)?;
        let dir = std::path::Path::new("/tmp/.X11-unix");
        if !dir.exists() {
            use std::os::unix::fs::PermissionsExt;
            if let Err(e) = std::fs::create_dir(dir).and_then(|()| std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o1777))) {
                tracing::warn!("{}: {e}", dir.display());
            }
        }
        let path = crate::doctor::xwayland_search_path(std::env::var_os("PATH").as_deref(), std::path::Path::new(crate::doctor::BUNDLED_XWAYLAND_DIR));
        // Its own X authority (a new cookie per server): its apps get the
        // file as `XAUTHORITY`, nothing else of the user's gets in.
        let xauthority = crate::xauth::space_path(&crate::xauth::runtime_dir(), &tag, space.id.as_str());
        let cookie = match crate::xauth::new_cookie().and_then(|cookie| crate::xauth::write(&xauthority, None, &cookie).map(|()| cookie)) {
            Ok(cookie) => cookie,
            Err(e) => {
                tracing::warn!("{}: its X authority: {e} (X11 apps won't run in it)", space.id.as_str());
                return None;
            }
        };
        let auth = [std::ffi::OsString::from("-auth"), xauthority.clone().into_os_string()];
        let spawned = (FIRST_DISPLAY..=LAST_DISPLAY).find_map(|display| {
            XWayland::spawn(&self.display_handle, display, [("PATH", path.clone())], auth.iter(), true, Stdio::null(), Stdio::null(), |data| {
                // The space's: it binds the space's seat and output alone.
                data.insert_if_missing(|| DeskTag(desk));
            })
            .ok()
            .map(|(xwayland, client)| (xwayland, client, display))
        });
        let Some((xwayland, client, display)) = spawned else {
            tracing::warn!("{}: no Xwayland (X11 apps won't run in it)", space.id.as_str());
            return None;
        };
        // Its number now that it has one: what Xlib looks up.
        if let Err(e) = crate::xauth::write(&xauthority, Some(display), &cookie) {
            tracing::warn!("{}: its X authority: {e}", space.id.as_str());
        }
        let ready_client = client.clone();
        let inserted = self.loop_handle.insert_source(xwayland, move |event, _, state| match event {
            XWaylandEvent::Ready { x11_socket, .. } => {
                let number = display;
                let Some(space) = state.spaces.list.iter_mut().find(|space| space.desk.id == desk && space.x11.as_ref().is_some_and(|x11| x11.display == display)) else { return };
                match X11Wm::start_wm(state.loop_handle.clone(), &state.display_handle, x11_socket, ready_client.clone()) {
                    Ok(wm) => {
                        tracing::info!("{}: Xwayland ready on :{number}", space.id.as_str());
                        if let Some(x11) = &mut space.x11 {
                            x11.xwm = Some(wm);
                        }
                        // What the space copied before its Xwayland ran (the
                        // agent's text for an X11 app it launches next).
                        state.xwm_selection_now(desk);
                    }
                    Err(e) => tracing::warn!("{}: X11 window manager failed: {e}", space.id.as_str()),
                }
            }
            XWaylandEvent::Error => {
                let number = display;
                tracing::warn!("space Xwayland :{number} did not start");
                state.space_x11_gone(desk, display);
            }
        });
        let token = match inserted {
            Ok(token) => token,
            Err(e) => {
                tracing::warn!("space Xwayland source: {e}");
                return None;
            }
        };
        // X's input focus goes with a seat no client binds: the space's
        // own (its X11 windows take keys through Xwayland, never a
        // wl_keyboard of this seat).
        if self.spaces.get(desk).is_some_and(|space| space.desk.x11_focus.0 == space.desk.seat) {
            let mut seat = self.seat_state.new_seat(format!("x11-focus-{}", self.spaces.get(desk)?.id.as_str()));
            seat.user_data().insert_if_missing(|| DeskTag(desk));
            seat.add_keyboard(smithay::input::keyboard::XkbConfig::default(), 600, 25).expect("default xkb keymap");
            self.spaces.get_mut(desk)?.desk.x11_focus = (seat, None);
        }
        let space = self.spaces.get_mut(desk)?;
        space.x11 = Some(SpaceX11 { token, client, display, xwm: None, xauthority });
        Some(display)
    }

    /// The X11 window of desk `desk` whose surface `focused` is, if any,
    /// takes X's input focus in its server; the one that had it gives it
    /// up. The space's twin of [`Ede::sync_x11_focus`].
    pub(crate) fn space_sync_x11_focus(&mut self, desk: DeskId, focused: Option<&smithay::reexports::wayland_server::protocol::wl_surface::WlSurface>) {
        use smithay::input::keyboard::KeyboardTarget;
        let Some(d) = self.try_desk(desk) else { return };
        let x11 = focused.and_then(|f| d.space.elements().find_map(|w| w.x11_surface().filter(|x| x.wl_surface().as_ref() == Some(f)).cloned()));
        if d.x11_focus.1 == x11 {
            return;
        }
        let seat = d.x11_focus.0.clone();
        let serial = smithay::utils::SERIAL_COUNTER.next_serial();
        let old = std::mem::replace(&mut self.desk_mut(desk).x11_focus.1, x11.clone());
        if let Some(old) = old {
            KeyboardTarget::leave(&old, &seat, self, serial);
        }
        if let Some(new) = x11 {
            KeyboardTarget::enter(&new, &seat, self, Vec::new(), serial);
            // On top in its own server too: what X finds under a point (a
            // drop target, a transient's parent) is what the space shows.
            if let Some(xwm) = self.spaces.get_mut(desk).and_then(|space| space.x11.as_mut()?.xwm.as_mut()) {
                let _ = xwm.raise_window(&new);
            }
        }
    }

    /// A space's X11 window got its surface: it takes the space's keyboard
    /// if it was given it before (its map comes before its surface).
    pub(crate) fn space_x11_associated(&mut self, desk: DeskId, wl_surface: smithay::reexports::wayland_server::protocol::wl_surface::WlSurface, surface: X11Surface) {
        let Some(d) = self.try_desk_mut(desk) else { return };
        if d.pending_x11_focus.as_ref() != Some(&surface) {
            return;
        }
        d.pending_x11_focus = None;
        let keyboard = d.seat.get_keyboard().unwrap();
        keyboard.set_focus(self, Some(wl_surface), smithay::utils::SERIAL_COUNTER.next_serial());
    }

    /// Space `desk`'s Xwayland on `display` ended (or never started): its
    /// source goes (the display number and socket are free again), its
    /// window manager with it; the next launch starts another.
    fn space_x11_gone(&mut self, desk: DeskId, display: u32) {
        let Some(space) = self.spaces.get_mut(desk) else { return };
        if space.x11.as_ref().is_none_or(|x11| x11.display != display) {
            return;
        }
        let x11 = space.x11.take().expect("checked");
        let windows: Vec<Window> = space.desk.space.elements().filter(|w| w.x11_surface().is_some()).cloned().collect();
        for window in windows {
            space.desk.space.unmap_elem(&window);
        }
        space.desk.pending_x11_focus = None;
        let focused = space.desk.x11_focus.1.take();
        let seat = space.desk.x11_focus.0.clone();
        // An X11 copy went with its server.
        if matches!(space.desk.clipboard.owner, Owner::X11(_)) {
            space.desk.clipboard.changed(Owner::Empty);
            let main = space.desk.seat.clone();
            clear_data_device_selection(&self.display_handle, &main);
        }
        // The client first: nothing it buffered reaches a handler of a
        // window manager that is gone.
        self.loop_handle.remove(x11.token);
        drop(x11.xwm);
        // Its window that had X's focus lets go of the space's X11 seat.
        if let Some(focused) = focused {
            smithay::input::keyboard::KeyboardTarget::leave(&focused, &seat, self, smithay::utils::SERIAL_COUNTER.next_serial());
        }
        self.space_schedule(desk);
    }

    /// An X11 window manager's connection closed: a closed space's is let
    /// go of, a live space's Xwayland died.
    pub(crate) fn space_xwm_disconnected(&mut self, xwm: XwmId) {
        if let Some(at) = self.spaces.closing_xwms.iter().position(|wm| wm.id() == xwm) {
            drop(self.spaces.closing_xwms.remove(at));
            return;
        }
        let died = self.spaces.iter().find_map(|space| space.x11.as_ref().filter(|x11| x11.xwm.as_ref().is_some_and(|wm| wm.id() == xwm)).map(|x11| (space.desk.id, x11.display)));
        if let Some((desk, display)) = died {
            let number = display;
            tracing::warn!("space Xwayland :{number} ended while its space lives");
            self.space_x11_gone(desk, display);
        }
    }

    /// Space `desk` closes with its Xwayland: the server goes now (its
    /// client is killed, its lock and socket removed), its window manager
    /// waits for its connection to close.
    pub(crate) fn space_x11_closed(&mut self, x11: Option<SpaceX11>) {
        let Some(x11) = x11 else { return };
        let _ = std::fs::remove_file(&x11.xauthority);
        // Its Wayland client is killed with the source, but the socket
        // closes only at the display's next dispatch, which an idle desk
        // may not have for a while: the server is told to end as well.
        if let Some(pid) = xwayland_process(x11.display) {
            let _ = smithay::reexports::rustix::process::kill_process(pid, smithay::reexports::rustix::process::Signal::TERM);
        }
        self.loop_handle.remove(x11.token);
        if let Some(wm) = x11.xwm {
            self.spaces.closing_xwms.push(wm);
        }
    }

    /// A space's X11 window asks to be shown: one without a parent fills
    /// the space's output as a space's Wayland toplevel does, a dialog
    /// (transient for another) is centred on it at its own size, and takes
    /// the space's keyboard; an override-redirect one (a menu, a tooltip)
    /// shows where it asks and takes nothing.
    pub(crate) fn space_x11_map(&mut self, desk: DeskId, surface: X11Surface) {
        let Some(size) = self.try_desk(desk).and_then(|d| d.space.outputs().next().cloned()).and_then(|o| o.current_mode()).map(|m| m.size) else { return };
        let override_redirect = surface.is_override_redirect();
        let loc = if override_redirect {
            surface.geometry().loc
        } else {
            let geometry = if surface.is_transient_for().is_some() {
                let own = surface.geometry().size;
                let (w, h) = (own.w.clamp(1, size.w), own.h.clamp(1, size.h));
                Rectangle::new(((size.w - w) / 2, (size.h - h) / 2).into(), (w, h).into())
            } else {
                Rectangle::new((0, 0).into(), (size.w, size.h).into())
            };
            let _ = surface.configure(geometry);
            if let Err(e) = surface.set_mapped(true) {
                tracing::warn!("X11 map in a space: {e}");
                return;
            }
            geometry.loc
        };
        let window = Window::new_x11_window(surface.clone());
        window.user_data().insert_if_missing(|| DeskTag(desk));
        let Some(d) = self.try_desk_mut(desk) else { return };
        d.space.map_element(window.clone(), loc, true);
        if !override_redirect {
            // Its surface may come after its map: the keyboard waits for it.
            if smithay::wayland::seat::WaylandFocus::wl_surface(&window).is_none() {
                d.pending_x11_focus = Some(surface);
            }
            self.space_focus(desk, Some(window));
        }
        self.space_schedule(desk);
    }

    /// A space's X11 window is gone from the screen; the keyboard goes to
    /// the space's window below it (never an override-redirect one).
    pub(crate) fn space_x11_unmap(&mut self, desk: DeskId, surface: &X11Surface) {
        let Some(d) = self.try_desk_mut(desk) else { return };
        if d.pending_x11_focus.as_ref() == Some(surface) {
            d.pending_x11_focus = None;
        }
        let window = d.space.elements().find(|w| w.x11_surface() == Some(surface)).cloned();
        if let Some(window) = window {
            let had_focus = smithay::wayland::seat::WaylandFocus::wl_surface(&window).is_some_and(|s| d.seat.get_keyboard().unwrap().current_focus().as_ref() == Some(&*s));
            d.space.unmap_elem(&window);
            if had_focus {
                let next = d.space.elements().rev().find(|w| w.x11_surface().is_none_or(|x| !x.is_override_redirect())).cloned();
                self.space_focus(desk, next);
            }
        }
        self.space_schedule(desk);
    }
}
