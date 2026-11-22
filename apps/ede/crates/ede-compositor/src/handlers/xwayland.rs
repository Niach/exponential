//! Xwayland: X11 apps (and most games) as ordinary eDE windows. smithay's
//! X11Wm is the window manager; mapped X11 windows join the same space and
//! get the same placement, focus, tiling, fullscreen and Alt+F4 as Wayland
//! ones. Override-redirect windows (menus, tooltips) map where they ask.
//! Clipboard flows both ways through the Wayland data device.

use std::{
    os::unix::io::OwnedFd,
    path::PathBuf,
    process::Stdio,
    time::{Duration, Instant},
};

use smithay::{
    desktop::Window,
    reexports::{
        calloop::{
            timer::{TimeoutAction, Timer},
            RegistrationToken,
        },
        wayland_server::protocol::wl_surface::WlSurface,
    },
    input::{
        keyboard::KeyboardTarget,
        pointer::{Focus, GrabStartData},
    },
    utils::{IsAlive, Logical, Rectangle, SERIAL_COUNTER},
    wayland::{
        selection::{
            data_device::{clear_data_device_selection, current_data_device_selection_userdata, request_data_device_client_selection, set_data_device_selection},
            SelectionTarget,
        },
        xwayland_shell::{XWaylandShellHandler, XWaylandShellState},
    },
    xwayland::{
        xwm::{Reorder, ResizeEdge as X11ResizeEdge, XwmId},
        X11Surface, X11Wm, XWayland, XWaylandEvent, XwmHandler,
    },
};

use crate::{
    desk::DeskId,
    spaces::x11::XwmDesk,
    grabs::{MoveSurfaceGrab, BTN_LEFT},
    layout::Tile,
    state::{set_window_state, window_state, Ede},
};

/// #EDE-138: the person's Xwayland, started again when it dies.
const RESTART_DELAYS: [Duration; 7] = [Duration::from_secs(1), Duration::from_secs(2), Duration::from_secs(4), Duration::from_secs(8), Duration::from_secs(16), Duration::from_secs(30), Duration::from_secs(60)];
/// A server that ran this long before it died was no crash loop: the next
/// start waits the shortest delay again.
const RAN_WELL: Duration = Duration::from_secs(60);
/// Deaths in a row (each within [`RAN_WELL`] of its start) after which it
/// is not started again: X11 apps then wait for the next login.
const GIVE_UP: u32 = 8;

/// The person's Xwayland: its source in the loop, its X authority, its
/// deaths.
#[derive(Default)]
pub(crate) struct PersonX11 {
    /// Removing it frees the display's lock and sockets.
    token: Option<RegistrationToken>,
    /// `$XDG_RUNTIME_DIR/xauth_<tag>` ([`crate::xauth`]): the same path for
    /// the compositor's life, so the `XAUTHORITY` the session was given
    /// stays right across a restart.
    pub(crate) xauthority: Option<PathBuf>,
    /// The compositor's tag in the cookie files' names (its socket's).
    tag: Option<String>,
    /// One cookie for the compositor's life, every server's.
    cookie: Option<[u8; 16]>,
    /// The deaths in a row.
    deaths: u32,
    /// When the server that runs now became ready.
    ready_at: Option<Instant>,
}

impl Drop for PersonX11 {
    fn drop(&mut self) {
        if let Some(path) = &self.xauthority {
            let _ = std::fs::remove_file(path);
        }
        if let Some(tag) = &self.tag {
            crate::xauth::remove_spaces(&crate::xauth::runtime_dir(), tag);
        }
    }
}

impl Ede {
    /// Starts Xwayland, on the display it had if it ran before (else the
    /// next free one), admitting only clients with the session's cookie;
    /// `DISPLAY` and `XAUTHORITY` are set for children once it is ready.
    pub fn start_xwayland(&mut self) {
        // X sockets live in /tmp/.X11-unix, which systemd-tmpfiles creates on
        // a desktop but a container or minimal system may lack.
        let dir = std::path::Path::new("/tmp/.X11-unix");
        if !dir.exists() {
            use std::os::unix::fs::PermissionsExt;
            if let Err(e) = std::fs::create_dir(dir).and_then(|()| std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o1777))) {
                tracing::warn!("{}: {e}", dir.display());
            }
        }
        // #EDE-138: its X authority. Without the file it starts as before
        // (open to every process of the user) rather than not at all.
        let tag = self.x11_tag();
        let xauthority = self.person_x11.xauthority.get_or_insert_with(|| crate::xauth::person_path(&crate::xauth::runtime_dir(), &tag)).clone();
        let cookie = match self.person_x11.cookie {
            Some(cookie) => Ok(cookie),
            None => crate::xauth::new_cookie(),
        };
        let auth = match cookie.and_then(|cookie| crate::xauth::write(&xauthority, self.xdisplay, &cookie).map(|()| cookie)) {
            Ok(cookie) => {
                self.person_x11.cookie = Some(cookie);
                vec![std::ffi::OsString::from("-auth"), xauthority.clone().into_os_string()]
            }
            Err(e) => {
                tracing::error!("{}: {e}: Xwayland starts without X authority", xauthority.display());
                Vec::new()
            }
        };
        // smithay runs `Xwayland` from PATH: the .deb's own 24.1+ goes first.
        // The rest of the environment passes through (GPU drivers found via
        // LD_LIBRARY_PATH and the like).
        let path = crate::doctor::xwayland_search_path(std::env::var_os("PATH").as_deref(), std::path::Path::new(crate::doctor::BUNDLED_XWAYLAND_DIR));
        let spawned = XWayland::spawn_with_inherited_env(&self.display_handle, self.xdisplay, [("PATH", path)], auth.iter(), true, Stdio::null(), Stdio::inherit(), |data| {
            // #EDE-125: the person's Xwayland; without the tag it would be nobody's.
            data.insert_if_missing(|| crate::desk::DeskTag(DeskId::PERSON));
        });
        let (xwayland, client) = match spawned {
            Ok(x) => x,
            Err(e) if self.xdisplay.is_some() => {
                tracing::error!("Xwayland did not start again on :{}: {e}", self.xdisplay.unwrap_or_default());
                self.person_xwayland_ended();
                return;
            }
            Err(e) => {
                tracing::warn!("Xwayland not started (X11 apps won't run): {e}");
                self.run_autostart();
                return;
            }
        };
        if let (Some(cookie), false) = (self.person_x11.cookie, auth.is_empty()) {
            if let Err(e) = crate::xauth::write(&xauthority, Some(xwayland.display_number()), &cookie) {
                tracing::error!("{}: {e}", xauthority.display());
            }
        }
        let inserted = self.loop_handle.insert_source(xwayland, move |event, _, state| match event {
            XWaylandEvent::Ready { x11_socket, display_number } => {
                match X11Wm::start_wm(state.loop_handle.clone(), &state.display_handle, x11_socket, client.clone()) {
                    Ok(wm) => {
                        tracing::info!("Xwayland ready on :{display_number}");
                        state.xwm = Some(wm);
                        state.xdisplay = Some(display_number);
                        state.person_x11.ready_at = Some(Instant::now());
                        // What the person copied while it was not there.
                        state.xwm_selection_now(DeskId::PERSON);
                    }
                    Err(e) => tracing::error!("X11 window manager failed: {e}"),
                }
                state.run_autostart();
            }
            XWaylandEvent::Error => {
                tracing::warn!("Xwayland crashed on startup");
                state.person_xwayland_ended();
                state.run_autostart();
            }
        });
        match inserted {
            Ok(token) => self.person_x11.token = Some(token),
            Err(e) => tracing::error!("Xwayland source: {e}"),
        }
    }

    /// The X authority file for `desk`'s programs: the person's (once
    /// their Xwayland was started), or a space's while it has one.
    pub fn xauthority(&self, desk: DeskId) -> Option<PathBuf> {
        if desk == DeskId::PERSON {
            return self.person_x11.xauthority.clone().filter(|_| self.person_x11.cookie.is_some());
        }
        self.spaces.get(desk)?.x11.as_ref().map(|x11| x11.xauthority.clone())
    }

    /// This compositor's tag in the cookie files' names ([`crate::xauth`]),
    /// fixed at its first use; what a crashed one on the same socket left
    /// of its spaces' files goes then.
    pub(crate) fn x11_tag(&mut self) -> String {
        if let Some(tag) = &self.person_x11.tag {
            return tag.clone();
        }
        let tag = crate::xauth::tag(self.socket_name.as_deref().and_then(|name| name.to_str()));
        crate::xauth::remove_spaces(&crate::xauth::runtime_dir(), &tag);
        self.person_x11.tag = Some(tag.clone());
        tag
    }

    /// #EDE-138: the person's Xwayland ended (it died, or it never became
    /// ready): its windows go as if each were unmapped, its copy with it,
    /// and it starts again on the same display with the same cookie file,
    /// later the more often it died young. A log line and an error report
    /// each time.
    fn person_xwayland_ended(&mut self) {
        let display = self.xdisplay;
        let surfaces: Vec<X11Surface> = self
            .desk
            .space
            .elements()
            .chain(self.desk.workspaces.parked.iter())
            .chain(self.desk.minimized.iter())
            .filter_map(|w| w.x11_surface().cloned())
            .collect();
        for surface in &surfaces {
            self.person_x11_unmap(surface);
        }
        self.desk.pending_x11_focus = None;
        if let Some(old) = self.desk.x11_focus.1.take() {
            let seat = self.desk.x11_focus.0.clone();
            KeyboardTarget::leave(&old, &seat, self, SERIAL_COUNTER.next_serial());
        }
        if matches!(self.desk.clipboard.owner, crate::clipboard::Owner::X11(_)) {
            self.desk.clipboard.changed(crate::clipboard::Owner::Empty);
            clear_data_device_selection(&self.display_handle, &self.desk.seat);
        }
        // The client first: nothing it buffered reaches a window manager
        // that is gone; the display's lock and sockets are free again.
        if let Some(token) = self.person_x11.token.take() {
            self.loop_handle.remove(token);
        }
        self.xwm = None;
        let x11 = &mut self.person_x11;
        if x11.ready_at.take().is_some_and(|at| at.elapsed() >= RAN_WELL) {
            x11.deaths = 0;
        }
        x11.deaths += 1;
        let deaths = x11.deaths;
        let number = display.map_or_else(|| "?".to_string(), |n| n.to_string());
        if deaths > GIVE_UP {
            tracing::error!("the person's Xwayland on :{number} ended {deaths} times in a row; it is not started again (X11 apps run again after the next login)");
            return;
        }
        let delay = RESTART_DELAYS[(deaths as usize - 1).min(RESTART_DELAYS.len() - 1)];
        tracing::error!("the person's Xwayland on :{number} ended ({} X11 windows; {deaths} in a row); it starts again in {delay:?}", surfaces.len());
        let timer = self.loop_handle.insert_source(Timer::from_duration(delay), |_, _, state| {
            state.start_xwayland();
            TimeoutAction::Drop
        });
        if let Err(e) = timer {
            tracing::error!("Xwayland restart: {e}");
        }
    }

    /// #EDE-88: X gives keys to its own input focus (by default the window
    /// under X's pointer), so the X11 window whose surface now has the
    /// keyboard takes X's focus too, and the one that lost it gives it up.
    pub(crate) fn sync_x11_focus(&mut self, focused: Option<&WlSurface>) {
        let x11 = focused.and_then(|f| {
            self.desk.space.elements().find_map(|w| w.x11_surface().filter(|x| x.wl_surface().as_ref() == Some(f)).cloned())
        });
        if self.desk.x11_focus.1 == x11 {
            return;
        }
        let seat = self.desk.x11_focus.0.clone();
        let serial = SERIAL_COUNTER.next_serial();
        if let Some(old) = std::mem::replace(&mut self.desk.x11_focus.1, x11.clone()) {
            KeyboardTarget::leave(&old, &seat, self, serial);
        }
        if let Some(new) = x11 {
            KeyboardTarget::enter(&new, &seat, self, Vec::new(), serial);
        }
    }

    /// The person's X11 window `surface` is gone from the screen (unmapped,
    /// or its server ended).
    pub(crate) fn person_x11_unmap(&mut self, surface: &X11Surface) {
        // Parked on another workspace: forget it there.
        if let Some(parked) = self.desk.workspaces.parked.iter().find(|w| w.x11_surface() == Some(surface)).cloned() {
            self.window_event(ede_protocol::events::WINDOW_CLOSED, &parked);
            self.forget_parked(&parked);
        }
        if let Some(minimized) = self.desk.minimized.iter().find(|w| w.x11_surface() == Some(surface)).cloned() {
            self.window_event(ede_protocol::events::WINDOW_CLOSED, &minimized);
            self.forget_minimized(&minimized);
        }
        if let Some(window) = self.x11_window(surface) {
            let desk = self.desk_of_window(&window);
            let had_focus = self.focused_window(desk).as_ref() == Some(&window);
            if !surface.is_override_redirect() {
                self.window_event(ede_protocol::events::WINDOW_CLOSED, &window);
            }
            self.desk.space.unmap_elem(&window);
            if had_focus {
                let next = self.desk.space.elements().rev().find(|w| w.x11_surface().is_none_or(|x| !x.is_override_redirect())).cloned();
                self.focus(desk, next);
            }
        }
        if !surface.is_override_redirect() {
            let _ = surface.set_mapped(false);
        }
    }

    fn x11_window(&self, surface: &X11Surface) -> Option<Window> {
        self.desk.space.elements().find(|w| w.x11_surface() == Some(surface)).cloned()
    }
}

impl XWaylandShellHandler for Ede {
    fn xwayland_shell_state(&mut self) -> &mut XWaylandShellState {
        &mut self.xwayland_shell_state
    }

    fn surface_associated(&mut self, xwm: XwmId, wl_surface: WlSurface, surface: X11Surface) {
        // #EDE-138: a space's X11 window takes its space's keyboard.
        match self.desk_of_xwm(xwm) {
            XwmDesk::Person => {}
            XwmDesk::Space(desk) => return self.space_x11_associated(desk, wl_surface, surface),
            XwmDesk::Gone => return,
        }
        let keyboard = self.desk.seat.get_keyboard().unwrap();
        // #EDE-122: restored right after its minimize, the window was given
        // the keyboard on the surface Xwayland then destroyed: the keyboard
        // is on nothing and X's focus still names the window, so its new
        // surface takes it.
        let orphaned = self.desk.x11_focus.1.as_ref() == Some(&surface) && keyboard.current_focus().is_some_and(|f| !f.alive());
        if self.desk.pending_x11_focus.as_ref() == Some(&surface) || orphaned {
            self.desk.pending_x11_focus = None;
            keyboard.set_focus(self, Some(wl_surface), SERIAL_COUNTER.next_serial());
        }
    }
}

impl XwmHandler for Ede {
    fn xwm_state(&mut self, xwm: XwmId) -> &mut X11Wm {
        // #EDE-138: the person's, or a space's (also a closed space's until
        // its connection is gone); smithay asks for no other.
        if self.xwm.as_ref().is_some_and(|wm| wm.id() == xwm) {
            return self.xwm.as_mut().expect("checked");
        }
        self.space_xwm_state(xwm).expect("X11 events only arrive for a window manager that runs")
    }

    /// #EDE-138: an Xwayland ended: the person's (it starts again) or a
    /// space's (its space closed, or it died).
    fn disconnected(&mut self, xwm: XwmId) {
        if self.xwm.as_ref().is_some_and(|wm| wm.id() == xwm) {
            return self.person_xwayland_ended();
        }
        self.space_xwm_disconnected(xwm);
    }

    /// #EDE-125: the person's X11 clients drag on the person's seat. smithay
    /// would take the seat whose button went down last, an agent's in its
    /// space included.
    fn dnd_seat(&mut self, xwm: XwmId, seat: &smithay::input::Seat<Self>) -> bool {
        // #EDE-138: a space's X11 clients drag on the space's seat.
        match self.desk_of_xwm(xwm) {
            XwmDesk::Person => *seat == self.desk.seat,
            XwmDesk::Space(desk) => self.try_desk(desk).is_some_and(|d| *seat == d.seat),
            XwmDesk::Gone => false,
        }
    }

    fn new_window(&mut self, _xwm: XwmId, _window: X11Surface) {}
    fn new_override_redirect_window(&mut self, _xwm: XwmId, _window: X11Surface) {}

    fn map_window_request(&mut self, xwm: XwmId, surface: X11Surface) {
        match self.desk_of_xwm(xwm) {
            XwmDesk::Person => {}
            XwmDesk::Space(desk) => {
                self.space_x11_map(desk, surface);
                return;
            }
            XwmDesk::Gone => return,
        }
        if let Err(e) = surface.set_mapped(true) {
            tracing::warn!("X11 map: {e}");
            return;
        }
        let wants_fullscreen = surface.is_fullscreen();
        // #EDE-88: every X11 window gets our title bar unless its Motif
        // hints ask for no decorations (smithay's `is_decorated` = "the
        // client draws its own") or it is no application window (a dock,
        // a splash, a menu that is not override-redirect). Set on the
        // Window that maps, before it is placed (the frame counts there).
        use smithay::xwayland::xwm::WmWindowType as T;
        let decorated = !surface.is_decorated() && matches!(surface.window_type(), None | Some(T::Normal | T::Dialog | T::Utility));
        let window = Window::new_x11_window(surface);
        let mut st = window_state(&window);
        st.decorated = decorated;
        set_window_state(&window, st);
        // Same rules as a Wayland toplevel's first frame.
        self.place_new_window(self.desk_of_window(&window), window.clone());
        if wants_fullscreen {
            self.set_fullscreen(&window, true);
        }
    }

    fn mapped_override_redirect_window(&mut self, xwm: XwmId, surface: X11Surface) {
        match self.desk_of_xwm(xwm) {
            XwmDesk::Person => {}
            XwmDesk::Space(desk) => {
                self.space_x11_map(desk, surface);
                return;
            }
            XwmDesk::Gone => return,
        }
        let loc = surface.last_configure().loc;
        self.desk.space.map_element(Window::new_x11_window(surface), loc, true);
    }

    fn unmapped_window(&mut self, xwm: XwmId, surface: X11Surface) {
        match self.desk_of_xwm(xwm) {
            XwmDesk::Person => {}
            XwmDesk::Space(desk) => {
                self.space_x11_unmap(desk, &surface);
                return;
            }
            XwmDesk::Gone => return,
        }
        self.person_x11_unmap(&surface);
    }

    fn destroyed_window(&mut self, _xwm: XwmId, _window: X11Surface) {}

    fn configure_request(
        &mut self,
        xwm: XwmId,
        surface: X11Surface,
        x: Option<i32>,
        y: Option<i32>,
        w: Option<u32>,
        h: Option<u32>,
        _reorder: Option<Reorder>,
    ) {
        match self.desk_of_xwm(xwm) {
            XwmDesk::Person => {}
            // A space's window keeps what its space gave it; before it maps
            // it may ask (an override-redirect one places itself).
            XwmDesk::Space(desk) => {
                let mapped = self.try_desk(desk).is_some_and(|d| d.space.elements().any(|w| w.x11_surface() == Some(&surface)));
                if !mapped || surface.is_override_redirect() {
                    let mut geo = surface.last_configure();
                    geo.loc = (x.unwrap_or(geo.loc.x), y.unwrap_or(geo.loc.y)).into();
                    geo.size = (w.map_or(geo.size.w, |w| w as i32), h.map_or(geo.size.h, |h| h as i32)).into();
                    let _ = surface.configure(geo);
                } else {
                    let _ = surface.configure(None);
                }
                return;
            }
            XwmDesk::Gone => return,
        }
        // Tiled and fullscreen windows keep their geometry; floating ones
        // may resize, and move only while not yet mapped.
        let mapped = self.x11_window(&surface);
        if let Some(window) = &mapped {
            let st = window_state(window);
            if st.tile.is_some() || st.fullscreen.is_some() {
                let _ = surface.configure(self.desk.space.element_geometry(window));
                return;
            }
        }
        let mut geo = surface.last_configure();
        if mapped.is_none() {
            geo.loc = (x.unwrap_or(geo.loc.x), y.unwrap_or(geo.loc.y)).into();
        }
        if let Some(w) = w {
            geo.size.w = w as i32;
        }
        if let Some(h) = h {
            geo.size.h = h as i32;
        }
        let _ = surface.configure(geo);
    }

    fn configure_notify(&mut self, xwm: XwmId, surface: X11Surface, geometry: Rectangle<i32, Logical>, _above: Option<u32>) {
        match self.desk_of_xwm(xwm) {
            XwmDesk::Person => {}
            XwmDesk::Space(_desk) => {
                let _ = (surface, geometry);
                return;
            }
            XwmDesk::Gone => return,
        }
        if let Some(window) = self.x11_window(&surface) {
            if surface.is_override_redirect() {
                self.desk.space.map_element(window, geometry.loc, false);
            }
        }
    }

    fn maximize_request(&mut self, xwm: XwmId, surface: X11Surface) {
        match self.desk_of_xwm(xwm) {
            XwmDesk::Person => {}
            XwmDesk::Space(_desk) => {
                let _ = surface;
                return;
            }
            XwmDesk::Gone => return,
        }
        if let Some(w) = self.x11_window(&surface) {
            self.tile(&w, Tile::Maximized);
        }
    }

    fn unmaximize_request(&mut self, xwm: XwmId, surface: X11Surface) {
        match self.desk_of_xwm(xwm) {
            XwmDesk::Person => {}
            XwmDesk::Space(_desk) => {
                let _ = surface;
                return;
            }
            XwmDesk::Gone => return,
        }
        if let Some(w) = self.x11_window(&surface) {
            self.untile(&w, None);
        }
    }

    fn fullscreen_request(&mut self, xwm: XwmId, surface: X11Surface) {
        match self.desk_of_xwm(xwm) {
            XwmDesk::Person => {}
            XwmDesk::Space(_desk) => {
                let _ = surface;
                return;
            }
            XwmDesk::Gone => return,
        }
        if let Some(w) = self.x11_window(&surface) {
            self.set_fullscreen(&w, true);
        }
    }

    fn unfullscreen_request(&mut self, xwm: XwmId, surface: X11Surface) {
        match self.desk_of_xwm(xwm) {
            XwmDesk::Person => {}
            XwmDesk::Space(_desk) => {
                let _ = surface;
                return;
            }
            XwmDesk::Gone => return,
        }
        if let Some(w) = self.x11_window(&surface) {
            self.set_fullscreen(&w, false);
        }
    }

    // _NET_WM_STATE_HIDDEN / WM_CHANGE_STATE Iconic (#EDE-48): the app's own
    // minimize goes into the taskbar; the X client asking back restores.
    fn minimize_request(&mut self, xwm: XwmId, surface: X11Surface) {
        match self.desk_of_xwm(xwm) {
            XwmDesk::Person => {}
            XwmDesk::Space(_desk) => {
                let _ = surface;
                return;
            }
            XwmDesk::Gone => return,
        }
        if let Some(w) = self.x11_window(&surface) {
            self.minimize_window(&w);
        }
    }

    fn unminimize_request(&mut self, xwm: XwmId, surface: X11Surface) {
        match self.desk_of_xwm(xwm) {
            XwmDesk::Person => {}
            XwmDesk::Space(_desk) => {
                let _ = surface;
                return;
            }
            XwmDesk::Gone => return,
        }
        if let Some(w) = self.desk.minimized.iter().find(|w| w.x11_surface() == Some(&surface)).cloned() {
            self.restore_window(&w);
            // #EDE-82: a client's own request brings it in front only when
            // its app is the one typed into.
            if self.takes_keyboard(&w) {
                self.focus(self.desk_of_window(&w), Some(w));
            } else {
                self.keep_behind(&w);
            }
        }
    }

    fn resize_request(&mut self, _xwm: XwmId, _surface: X11Surface, _button: u32, _edges: X11ResizeEdge) {
        // X11 windows resize from their own frame or by the app; interactive
        // edge resizing for them comes with server-side decorations.
    }

    fn move_request(&mut self, xwm: XwmId, surface: X11Surface, _button: u32) {
        match self.desk_of_xwm(xwm) {
            XwmDesk::Person => {}
            XwmDesk::Space(_desk) => {
                let _ = surface;
                return;
            }
            XwmDesk::Gone => return,
        }
        let Some(window) = self.x11_window(&surface) else { return };
        let Some(loc) = self.desk.space.element_location(&window) else { return };
        let pointer = self.desk.seat.get_pointer().unwrap();
        let start = pointer
            .grab_start_data()
            .unwrap_or(GrabStartData { focus: None, button: BTN_LEFT, location: pointer.current_location() });
        pointer.set_grab(self, MoveSurfaceGrab::new(start, window, loc), SERIAL_COUNTER.next_serial(), Focus::Clear);
    }

    fn allow_selection_access(&mut self, xwm: XwmId, _selection: SelectionTarget) -> bool {
        match self.desk_of_xwm(xwm) {
            // Only the focused X11 client may read the clipboard.
            XwmDesk::Person => self.focused_window(DeskId::PERSON).and_then(|w| w.x11_surface().and_then(|x| x.xwm_id())) == Some(xwm),
            // #EDE-138: a space's, the space's (never the person's).
            XwmDesk::Space(desk) => self.space_x11_focused(desk, xwm),
            XwmDesk::Gone => false,
        }
    }

    /// An X11 client reads its desk's clipboard (#EDE-138: a space's
    /// client the space's).
    fn send_selection(&mut self, xwm: XwmId, selection: SelectionTarget, mime_type: String, fd: OwnedFd) {
        let Some(desk) = self.desk_of_xwm(xwm).desk() else { return };
        if selection != SelectionTarget::Clipboard {
            return;
        }
        let Some(seat) = self.try_desk(desk).map(|d| d.seat.clone()) else { return };
        // Ours ([`crate::clipboard`]) is served from memory; a Wayland
        // client's through its source.
        let ours = match current_data_device_selection_userdata(&seat).as_deref() {
            Some(crate::clipboard::Selection::Ours(owned)) => Some(owned.clone()),
            _ => None,
        };
        if let Some(owned) = ours {
            self.serve_owned(desk, &owned, &mime_type, fd);
        } else if let Err(e) = request_data_device_client_selection(&seat, mime_type, fd) {
            tracing::warn!("clipboard to X11: {e:?}");
        }
    }

    /// An X11 client copied: its desk's clipboard is the X11 client's.
    fn new_selection(&mut self, xwm: XwmId, selection: SelectionTarget, mime_types: Vec<String>) {
        let Some(desk) = self.desk_of_xwm(xwm).desk() else { return };
        if selection != SelectionTarget::Clipboard {
            return;
        }
        let Some(d) = self.try_desk_mut(desk) else { return };
        d.clipboard.changed(crate::clipboard::Owner::X11(mime_types.clone()));
        let seat = d.seat.clone();
        set_data_device_selection(&self.display_handle, &seat, mime_types, crate::clipboard::Selection::X11);
    }

    fn cleared_selection(&mut self, xwm: XwmId, selection: SelectionTarget) {
        let Some(desk) = self.desk_of_xwm(xwm).desk() else { return };
        let Some(d) = self.try_desk_mut(desk) else { return };
        let x11 = matches!(current_data_device_selection_userdata(&d.seat).as_deref(), Some(crate::clipboard::Selection::X11));
        if selection == SelectionTarget::Clipboard && x11 {
            d.clipboard.changed(crate::clipboard::Owner::Empty);
            let seat = d.seat.clone();
            clear_data_device_selection(&self.display_handle, &seat);
        }
    }
}

/// Xwayland's own client data carries its compositor state.
pub fn client_compositor_state(client: &smithay::reexports::wayland_server::Client) -> Option<&smithay::wayland::compositor::CompositorClientState> {
    client.get_data::<smithay::xwayland::XWaylandClientData>().map(|d| &d.compositor_state)
}
