//! The compositor state every backend drives: the Wayland globals, the space
//! of floating windows, the seat. Backends (winit, headless) only add outputs,
//! feed input through [`crate::input`] and render [`Ede::space`].

use std::{cell::RefCell, ffi::OsString, sync::Arc, time::Instant};

use smithay::{
    desktop::{layer_map_for_output, LayerMap, PopupManager, Space, Window, WindowSurfaceType},
    wayland::seat::WaylandFocus,
    input::{keyboard::XkbConfig, pointer::CursorImageStatus, Seat, SeatState},
    output::Output,
    reexports::{
        calloop::{generic::Generic, Interest, LoopHandle, LoopSignal, Mode, PostAction},
        wayland_protocols::xdg::shell::server::xdg_toplevel,
        wayland_server::{
            backend::{ClientData, ClientId, DisconnectReason},
            protocol::wl_surface::WlSurface,
            Display, DisplayHandle,
        },
    },
    utils::{IsAlive, Logical, Point, Rectangle, Size, SERIAL_COUNTER},
    wayland::{
        compositor::{CompositorClientState, CompositorState},
        output::OutputManagerState,
        selection::data_device::DataDeviceState,
        shell::{wlr_layer::{Layer as WlrLayer, WlrLayerShellState}, xdg::XdgShellState},
        shm::ShmState,
        socket::ListeningSocketSource,
    },
};

use crate::{
    config::Config,
    desk::{on_person, DeskId},
    handlers::layer_shell::is_sealed_prompt,
    layout::{self, Rect, Tile},
};

pub struct Ede {
    pub start_time: Instant,
    pub display_handle: DisplayHandle,
    pub loop_handle: LoopHandle<'static, Ede>,
    pub loop_signal: LoopSignal,
    /// `wayland-N` once [`Ede::listen`] ran; `None` for in-process clients only.
    pub socket_name: Option<OsString>,
    pub config: Config,

    pub popups: PopupManager,

    pub compositor_state: CompositorState,
    pub xdg_shell_state: XdgShellState,
    pub layer_shell_state: WlrLayerShellState,
    pub shm_state: ShmState,
    pub output_manager_state: OutputManagerState,
    pub seat_state: SeatState<Ede>,
    pub data_device_state: DataDeviceState,
    /// The person's desk: seat, windows, clipboard, cursor ([`crate::desk`]).
    pub desk: crate::desk::Desk,
    /// The agent spaces: each a desk of its own beside the person's
    /// ([`crate::spaces`]). The person's desk is in no collection.
    pub spaces: crate::spaces::Spaces,

    /// Set by a backend that owns a seat session: switch to VT n.
    pub vt_switch: Option<Box<dyn FnMut(i32)>>,
    /// Commands started once clients can connect (after Xwayland is up).
    pub autostart: Vec<Vec<String>>,
    pub dmabuf_state: smithay::wayland::dmabuf::DmabufState,
    /// Set by the backend that owns the renderer: can it import this buffer?
    pub dmabuf_importer: Option<DmabufImporter>,
    /// Draws captures on the renderer that draws the outputs (udev, winit:
    /// GLES, which reads the GPU clients' dmabufs pixman cannot, #EDE-100);
    /// `None` = pixman (headless).
    pub capture_renderer: Option<crate::backend::CaptureRenderer>,
    /// Explicit sync (drm-syncobj), when the primary GPU (udev) or the
    /// spaces' GPU (#EDE-162) supports it.
    #[cfg(feature = "gpu")]
    pub syncobj_state: Option<smithay::wayland::drm_syncobj::DrmSyncobjState>,

    #[cfg(feature = "xwayland")]
    pub xwayland_shell_state: smithay::wayland::xwayland_shell::XWaylandShellState,
    /// The X11 window manager once Xwayland is up.
    #[cfg(feature = "xwayland")]
    pub xwm: Option<smithay::xwayland::X11Wm>,
    /// Xwayland's display number (`DISPLAY=:N`).
    #[cfg(feature = "xwayland")]
    pub xdisplay: Option<u32>,
    /// #EDE-138: the person's Xwayland's source, X authority and deaths.
    #[cfg(feature = "xwayland")]
    pub(crate) person_x11: crate::handlers::xwayland::PersonX11,

    /// ext-session-lock: while locked only the lock client is shown and
    /// gets input ([`crate::lock`]).
    pub session_lock_state: smithay::wayland::session_lock::SessionLockManagerState,
    pub lock: crate::lock::LockState,
    /// ext-idle-notify (the shell's blanking/suspend), zwp-idle-inhibit, and
    /// the compositor's own idle lock ([`crate::idle`]).
    pub idle_notifier: smithay::wayland::idle_notify::IdleNotifierState<Ede>,
    pub idle_inhibit_state: smithay::wayland::idle_inhibit::IdleInhibitManagerState,
    pub idle: crate::idle::Idle,
    /// input-method-v2 + text-input-v3 (ibus, fcitx5) and the virtual
    /// keyboard IMEs forward keys through.
    pub input_method_state: smithay::wayland::input_method::InputMethodManagerState,
    pub text_input_state: smithay::wayland::text_input::TextInputManagerState,
    pub virtual_keyboard_state: smithay::wayland::virtual_keyboard::VirtualKeyboardManagerState,
    /// ede-protocol: the control socket, its grants and clients.
    pub ipc: crate::ipc::Ipc,
    /// Alt+Tab, Super, the media keys: what the shell answers ([`crate::hotkeys`]).
    pub hotkeys: crate::hotkeys::Hotkeys,
    /// wp-fractional-scale + wp-viewporter: HiDPI at 1.5× and the like.
    pub fractional_scale_state: smithay::wayland::fractional_scale::FractionalScaleManagerState,
    pub viewporter_state: smithay::wayland::viewporter::ViewporterState,
    /// Set by a backend that can change an output's mode (udev: a modeset).
    pub set_mode: Option<Box<dyn FnMut(&Output, smithay::output::Mode) -> Result<(), String>>>,
    /// xdg-decoration: who draws a window's frame ([`crate::frames`]).
    pub xdg_decoration_state: smithay::wayland::shell::xdg::decoration::XdgDecorationState,
    /// xdg-activation and the person's launch tokens ([`crate::launch`]).
    pub xdg_activation_state: smithay::wayland::xdg_activation::XdgActivationState,
    /// The recovery screen's state ([`crate::recovery`]).
    pub recovery: crate::recovery::Recovery,
    /// Why the backend stopped the loop (#EDE-40: the outputs never drew);
    /// the run ends with this error.
    pub fatal: Option<String>,
    /// `--debug-captures` (headless and nested only): captures are not redacted.
    pub debug_captures: bool,
    /// A test's own process standing in for `exponential-sealed` (as
    /// [`Ede::begin_sealed_lock_as`] does for the lock): a layer surface it
    /// maps meanwhile is a sealed one ([`crate::sealed::origin_of`]). It
    /// decides what counts as sealed, so only code inside this process
    /// sets it (the headless tests): no protocol request, no config key
    /// and no environment variable ever does. Always `None` in a session.
    pub sealed_stand_in: Option<u32>,
    /// #EXP-2024: a test's own process standing in for `exponential-shell`:
    /// a layer it maps meanwhile with the space view's namespace is the
    /// shell's pinned view ([`crate::sealed::origin_of`]), a popup it names
    /// meanwhile the Agent spaces list's namespace is the shell's list
    /// (#EXP-2041, [`crate::popup_namespace`]). As `sealed_stand_in`, only
    /// the headless tests set it; `None` in a session.
    pub shell_stand_in: Option<u32>,
    /// #EDE-172: eDE's root-owned binaries, by file ([`crate::peer`]): a
    /// sealed or shell surface is one of these processes', never a name's.
    pub installed: crate::peer::Installed,
    /// Counts presented frames ([`crate::backend::frame_done`]); a capture
    /// with `since` equal to it carries no pixels.
    pub frame_serial: u64,
    /// Where `reload_config` reads from; `None` in safe mode.
    pub config_path: Option<std::path::PathBuf>,
    /// #EDE-172 (P4): the session's programs the compositor starts itself
    /// (`[session] helpers = "compositor"`, [`crate::helpers`]).
    pub helpers: crate::helpers::Helpers,
}

/// Tries a client's dmabuf on the backend's renderer.
pub type DmabufImporter = Box<dyn FnMut(&smithay::backend::allocator::dmabuf::Dmabuf) -> bool>;

/// Per-window floating state, kept in the window's user data.
#[derive(Debug, Default, Clone, Copy)]
pub struct WindowState {
    pub tile: Option<Tile>,
    /// The floating geometry to go back to when the window leaves a tile.
    pub restore: Option<Rect>,
    /// Fullscreen, with the geometry to go back to.
    pub fullscreen: Option<Rect>,
    /// The workspace it lives on (0-based, [`crate::workspaces`]).
    pub workspace: usize,
    /// Where it sat when it was parked (its workspace left the screen).
    pub parked_at: Option<Point<i32, Logical>>,
    /// The compositor draws its title bar ([`crate::frames`]).
    pub decorated: bool,
    /// In the taskbar only ([`crate::minimize`]); `parked_at` keeps its place.
    pub minimized: bool,
}

pub fn window_state(window: &Window) -> WindowState {
    *window.user_data().get_or_insert(|| RefCell::new(WindowState::default())).borrow()
}

pub fn set_window_state(window: &Window, state: WindowState) {
    *window.user_data().get_or_insert(|| RefCell::new(WindowState::default())).borrow_mut() = state;
}

impl Ede {
    pub fn new(display: Display<Ede>, loop_handle: LoopHandle<'static, Ede>, loop_signal: LoopSignal, config: Config) -> Self {
        let dh = display.handle();

        let compositor_state = CompositorState::new::<Self>(&dh);
        let xdg_shell_state = XdgShellState::new::<Self>(&dh);
        let layer_shell_state = WlrLayerShellState::new_with_filter::<Self, _>(&dh, on_person);
        let shm_state = ShmState::new::<Self>(&dh, vec![]);
        let output_manager_state = OutputManagerState::new_with_xdg_output::<Self>(&dh);
        let data_device_state = DataDeviceState::new::<Self>(&dh);
        let mut seat_state = SeatState::new();

        // #EDE-125: the person's seat is announced to the person's clients only.
        let mut seat: Seat<Self> = seat_state.new_wl_seat_with_filter(&dh, "seat0", on_person);
        #[cfg(feature = "xwayland")]
        let mut x11_focus_seat = seat_state.new_seat("x11-focus");
        // smithay's Wayland enter wants a keyboard on the seat; no client
        // ever binds this one (no wl_seat global), so it sends nothing.
        #[cfg(feature = "xwayland")]
        x11_focus_seat.add_keyboard(XkbConfig::default(), 600, 25).expect("default xkb keymap");
        let kb = &config.keyboard;
        let xkb = XkbConfig { layout: &kb.layout, variant: &kb.variant, options: Some(kb.options.clone()).filter(|o| !o.is_empty()), ..Default::default() };
        if let Err(e) = seat.add_keyboard(xkb, kb.repeat_delay, kb.repeat_rate) {
            tracing::warn!("keyboard layout {:?} rejected ({e:?}); using the system default", kb.layout);
            seat.add_keyboard(XkbConfig::default(), kb.repeat_delay, kb.repeat_rate).expect("default xkb keymap");
        }
        seat.add_pointer();

        loop_handle
            .insert_source(Generic::new(display, Interest::READ, Mode::Level), |_, display, state| {
                // Safety: the display is never dropped while the source lives.
                unsafe { display.get_mut().dispatch_clients(state).expect("dispatch clients") };
                Ok(PostAction::Continue)
            })
            .expect("insert the display source");

        // #EXP-2041: the shell names its Agent spaces list (black in captures).
        crate::popup_namespace::create_global(&dh);
        let idle_notifier = smithay::wayland::idle_notify::IdleNotifierState::new(&dh, loop_handle.clone());
        crate::idle::start_timer(&loop_handle);
        let helpers = crate::helpers::Helpers::new(config.session.helpers == crate::config::Helpers::Compositor);

        Self {
            start_time: Instant::now(),
            display_handle: dh.clone(),
            loop_handle,
            loop_signal,
            socket_name: None,
            config,
            popups: PopupManager::default(),
            compositor_state,
            xdg_shell_state,
            layer_shell_state,
            shm_state,
            output_manager_state,
            seat_state,
            data_device_state,
            desk: crate::desk::Desk {
                id: crate::desk::DeskId::PERSON,
                seat,
                cursor_status: CursorImageStatus::default_named(),
                virtual_keymap: None,
                #[cfg(feature = "xwayland")]
                pending_x11_focus: None,
                #[cfg(feature = "xwayland")]
                x11_focus: (x11_focus_seat, None),
                keysyms: Default::default(),
                hold: Default::default(),
                driven: Default::default(),
                clipboard: Default::default(),
                space: Space::default(),
                unmapped: Vec::new(),
                minimized: Vec::new(),
                workspaces: crate::workspaces::Workspaces::default(),
                title_click: None,
            },
            spaces: Default::default(),
            vt_switch: None,
            autostart: Vec::new(),
            dmabuf_state: smithay::wayland::dmabuf::DmabufState::new(),
            dmabuf_importer: None,
            capture_renderer: None,
            #[cfg(feature = "gpu")]
            syncobj_state: None,
            #[cfg(feature = "xwayland")]
            xwayland_shell_state: smithay::wayland::xwayland_shell::XWaylandShellState::new::<Self>(&dh),
            #[cfg(feature = "xwayland")]
            xwm: None,
            #[cfg(feature = "xwayland")]
            xdisplay: None,
            #[cfg(feature = "xwayland")]
            person_x11: Default::default(),
            session_lock_state: smithay::wayland::session_lock::SessionLockManagerState::new::<Self, _>(&dh, on_person),
            lock: crate::lock::LockState::default(),
            idle_notifier,
            idle_inhibit_state: smithay::wayland::idle_inhibit::IdleInhibitManagerState::new::<Self>(&dh),
            idle: crate::idle::Idle::default(),
            input_method_state: smithay::wayland::input_method::InputMethodManagerState::new::<Self, _>(&dh, on_person),
            text_input_state: smithay::wayland::text_input::TextInputManagerState::new::<Self>(&dh),
            virtual_keyboard_state: smithay::wayland::virtual_keyboard::VirtualKeyboardManagerState::new::<Self, _>(&dh, on_person),
            ipc: crate::ipc::Ipc::default(),
            hotkeys: crate::hotkeys::Hotkeys::default(),
            fractional_scale_state: smithay::wayland::fractional_scale::FractionalScaleManagerState::new::<Self>(&dh),
            viewporter_state: smithay::wayland::viewporter::ViewporterState::new::<Self>(&dh),
            set_mode: None,
            xdg_decoration_state: smithay::wayland::shell::xdg::decoration::XdgDecorationState::new_with_filter::<Self, _>(&dh, on_person),
            xdg_activation_state: smithay::wayland::xdg_activation::XdgActivationState::new::<Self>(&dh),
            recovery: crate::recovery::Recovery::default(),
            fatal: None,
            debug_captures: false,
            sealed_stand_in: None,
            shell_stand_in: None,
            installed: crate::peer::Installed::system(),
            frame_serial: 0,
            config_path: None,
            helpers,
        }
    }

    /// A client of the person's Wayland socket. #EDE-172: who connected is
    /// taken now, before its first request ([`crate::peer::Peer`]).
    pub fn insert_person_client(&mut self, stream: std::os::unix::net::UnixStream) {
        let peer = crate::peer::Peer::of_socket(std::os::fd::AsFd::as_fd(&stream)).map(Arc::new);
        if let Err(e) = self.display_handle.insert_client(stream, Arc::new(ClientState { peer, ..ClientState::default() })) {
            tracing::warn!("client refused: {e}");
        }
    }

    /// Opens the next free `$XDG_RUNTIME_DIR/wayland-N` socket.
    pub fn listen(&mut self) -> std::io::Result<OsString> {
        let source = ListeningSocketSource::new_auto().map_err(std::io::Error::other)?;
        let name = source.socket_name().to_os_string();
        self.loop_handle
            .insert_source(source, |stream, _, state| state.insert_person_client(stream))
            .map_err(|e| std::io::Error::other(e.to_string()))?;
        self.socket_name = Some(name.clone());
        if let Some(runtime) = std::env::var_os("XDG_RUNTIME_DIR") {
            match self.ipc_listen(std::path::Path::new(&runtime), &name.to_string_lossy()) {
                Ok(path) => tracing::info!("ede-protocol on {}", path.display()),
                Err(e) => tracing::warn!("ede-protocol socket: {e}"),
            }
            // The agent spaces' directories live beside it; nothing of an
            // earlier compositor of this name is kept (#EDE-125).
            self.spaces.open(ede_protocol::spaces_path(std::path::Path::new(&runtime), &name.to_string_lossy()));
            // #EDE-138: their homes on disk, not in the runtime directory.
            let cache = std::env::var_os("XDG_CACHE_HOME").filter(|dir| !dir.is_empty()).map(std::path::PathBuf::from).or_else(|| std::env::var_os("HOME").map(|home| std::path::PathBuf::from(home).join(".cache")));
            if let Some(cache) = cache {
                self.spaces.open_homes(cache.join("exponential-desktop").join("spaces"));
            }
        }
        Ok(name)
    }

    /// Starts Xwayland if enabled, then the autostart commands.
    pub fn start_session(&mut self) {
        self.start_space_units();
        // #EDE-138: X11 apps in spaces too, each space with its own Xwayland.
        self.spaces.x11 = cfg!(feature = "xwayland") && self.config.xwayland;
        #[cfg(feature = "xwayland")]
        if self.config.xwayland {
            // Autostart runs from the Ready/Error event.
            self.start_xwayland();
            return;
        }
        self.run_autostart();
    }

    pub fn run_autostart(&mut self) {
        // #EDE-172 (P4): the session's helpers first (the autostart's
        // `exponential-session --ready` then finds their units conditioned off).
        self.start_helpers();
        for argv in std::mem::take(&mut self.autostart) {
            self.spawn(&argv);
        }
    }

    /// Starts `argv` as our client: our Wayland socket, our Xwayland (never
    /// the parent session's displays).
    pub fn spawn(&self, argv: &[String]) {
        if let Some(child) = self.spawn_with_env(argv, &[]) {
            // Reaped by the kernel's init when it ends; nothing waits on it.
            drop(child);
        }
    }

    /// [`Self::spawn`] with extra environment, keeping the child.
    pub fn spawn_with_env(&self, argv: &[String], env: &[(&str, &str)]) -> Option<std::process::Child> {
        let mut cmd = self.client_command(argv)?;
        for (k, v) in env {
            cmd.env(k, v);
        }
        match cmd.spawn() {
            Ok(child) => Some(child),
            Err(e) => {
                tracing::warn!("{}: {e}", argv[0]);
                None
            }
        }
    }

    /// `argv` as a command for a client of ours ([`Self::spawn`]).
    pub fn client_command(&self, argv: &[String]) -> Option<std::process::Command> {
        let (program, args) = argv.split_first()?;
        let mut cmd = std::process::Command::new(program);
        cmd.args(args).env_remove("DISPLAY").env_remove("WAYLAND_DISPLAY").env_remove("XAUTHORITY");
        if let Some(socket) = &self.socket_name {
            cmd.env("WAYLAND_DISPLAY", socket);
        }
        // #EDE-172 (P4): the helpers' units stay off, a helper run by hand knows it is none.
        if self.helpers.on {
            cmd.env(ede_protocol::process::HELPERS_ENV, "1");
        }
        #[cfg(feature = "xwayland")]
        if let Some(n) = self.xdisplay {
            cmd.env("DISPLAY", format!(":{n}"));
            // #EDE-138: the cookie our Xwayland asks for.
            if let Some(xauthority) = self.xauthority(DeskId::PERSON) {
                cmd.env("XAUTHORITY", xauthority);
            }
        }
        Some(cmd)
    }

    /// The `wl_output` global of one of the person's outputs: announced to
    /// the person's clients only (#EDE-125). Every backend makes its
    /// outputs' globals here.
    pub fn person_output_global(&self, output: &Output) -> smithay::reexports::wayland_server::backend::GlobalId {
        output.create_global_with_filter::<Ede, _>(&self.display_handle, on_person)
    }

    /// Lays outputs out left to right in the order they were added, unless
    /// `[[outputs]]` in the config names a position; a configured scale
    /// applies too ([`crate::outputs`]).
    pub fn add_output(&mut self, output: &Output) {
        let settings = self.config.output(&output.name()).cloned();
        if let Some(scale) = settings.as_ref().and_then(|s| s.scale) {
            output.change_current_state(None, None, Some(smithay::output::Scale::Fractional(scale)), None);
        }
        let at = match settings.as_ref().and_then(|s| s.position) {
            Some([x, y]) => (x, y),
            None => (self.desk.space.outputs().filter_map(|o| self.desk.space.output_geometry(o)).map(|g| g.loc.x + g.size.w).max().unwrap_or(0), 0),
        };
        self.place_output(output, at.into());
        layer_map_for_output(output).arrange();
        self.sealed_rehome();
        self.output_changed(output.name());
    }

    /// Unplugged: the rest close ranks left to right, and the windows that
    /// were on it (or end up on no output) reopen on the first one
    /// ([`Self::relayout`]).
    pub fn remove_output(&mut self, output: &Output) {
        let before = self.laid_out();
        self.desk.space.unmap_output(output);
        self.lock_output_removed(output);
        self.relayout(&before);
    }

    /// Windows in `also`, and any whose centre is on no output, reopen on
    /// the first output (an output went away or shrank under them).
    pub fn rehome_lost_windows(&mut self, also: &[Window]) {
        let center = |space: &Space<Window>, w: &Window| {
            space.element_geometry(w).map(|g| g.loc.to_f64() + g.size.to_f64().to_point().downscale(2.0))
        };
        let Some(first) = self.desk.space.outputs().next().cloned() else { return };
        let orphans: Vec<Window> = self
            .desk.space
            .elements()
            .filter(|w| also.contains(w) || center(&self.desk.space, w).is_some_and(|c| self.desk.space.output_under(c).next().is_none()))
            .cloned()
            .collect();
        for w in orphans {
            self.rehome(&w, &first);
        }
    }

    /// `window` reopens in a free spot of `output`'s work area, where it
    /// stands in the stack: a tiled one floating again at its old size, a
    /// fullscreen one filling `output`, leaving fullscreen to that spot
    /// (#EDE-139: never to the place on an output that went away).
    pub fn rehome(&mut self, window: &Window, output: &Output) {
        let st = window_state(window);
        let size = st.restore.or(st.fullscreen).map_or(window.geometry().size, |r| r.size);
        let Some(loc) = self.free_spot(window, size, output) else { return };
        if st.fullscreen.is_some() {
            set_window_state(window, WindowState { fullscreen: Some(Rectangle::new(loc, size)), tile: None, restore: None, ..st });
            self.fit_fullscreen(window, output);
            return;
        }
        self.untile(window, None);
        self.relocate_window(window, loc);
    }

    /// Where a `size` window (its title bar not counted) opens in
    /// `output`'s work area beside the other windows.
    pub(crate) fn free_spot(&self, window: &Window, size: Size<i32, Logical>, output: &Output) -> Option<Point<i32, Logical>> {
        let area = self.work_area(output)?;
        let fh = self.frame_height(window);
        let taken: Vec<_> = self.desk.space.elements().filter(|o| *o != window).filter_map(|o| self.desk.space.element_location(o)).collect();
        let mut loc = layout::place_new((size.w, size.h + fh).into(), area, &taken, 32);
        loc.y += fh;
        Some(loc)
    }

    /// `r` moved by `delta` along with `output`; one `output` no longer
    /// holds (it shrank) gets a free spot on it instead.
    pub(crate) fn moved_along(&self, window: &Window, r: Rect, delta: Point<i32, Logical>, output: &Output) -> Rect {
        let r = Rectangle::new(r.loc + delta, r.size);
        let center = r.loc.to_f64() + r.size.to_f64().to_point().downscale(2.0);
        if self.desk.space.output_geometry(output).is_some_and(|g| g.to_f64().contains(center)) {
            return r;
        }
        self.free_spot(window, r.size, output).map_or(r, |loc| Rectangle::new(loc, r.size))
    }

    /// The output's area minus exclusive layer-shell zones (a panel), inset
    /// by `[windows] panel_margin` on every side (the IDE look, #EDE-48),
    /// in space coordinates.
    pub fn work_area(&self, output: &Output) -> Option<Rect> {
        let geo = self.desk.space.output_geometry(output)?;
        let zone = layer_map_for_output(output).non_exclusive_zone();
        Some(layout::inset(Rectangle::new(geo.loc + zone.loc, zone.size), self.config.windows.panel_margin))
    }

    pub fn output_at(&self, p: Point<f64, Logical>) -> Option<Output> {
        self.desk.space.output_under(p).next().or_else(|| self.desk.space.outputs().next()).cloned()
    }

    pub fn window_for_surface(&self, surface: &WlSurface) -> Option<Window> {
        self.desk.space.elements().chain(self.desk.unmapped.iter()).find(|w| w.wl_surface().is_some_and(|s| *s == *surface)).cloned()
    }

    /// The topmost surface under `pos`: overlay/top layers, then windows, then
    /// bottom/background layers, with that surface's origin.
    pub fn surface_under(&self, pos: Point<f64, Logical>) -> Option<(WlSurface, Point<f64, Logical>)> {
        let output = self.desk.space.output_under(pos).next()?;
        let output_geo = self.desk.space.output_geometry(output)?;
        if self.is_locked() {
            // Only the lock client, nothing behind it.
            let lock = self.lock_surface_on(output)?;
            return Some((lock, output_geo.loc.to_f64()));
        }
        if let Some(window) = self.fullscreen_on(output) {
            // #EDE-103: the output draws only that window (and its popups):
            // nothing hidden behind it takes the pointer. #EDE-107: but a
            // sealed prompt, drawn above it.
            let prompt = sealed_prompt_under(&layer_map_for_output(output), output_geo.loc, pos);
            if prompt.is_some() {
                return prompt;
            }
            let location = self.desk.space.element_location(&window)? - window.geometry().loc;
            return window.surface_under(pos - location.to_f64(), WindowSurfaceType::ALL).map(|(s, p)| (s, (p + location).to_f64()));
        }
        let layers = layer_map_for_output(output);
        let above = layers_above_windows(&layers, output_geo.loc, pos);
        if above.is_some() {
            return above;
        }
        // #EDE-103: a server-side bar (or its resize band) is the
        // compositor's: the window drawn under it takes neither press nor hover.
        if self.window_frame_under(pos).is_some() {
            return None;
        }
        self.desk.space
            .element_under(pos)
            .and_then(|(window, location)| window.surface_under(pos - location.to_f64(), WindowSurfaceType::ALL).map(|(s, p)| (s, (p + location).to_f64())))
            .or_else(|| layer_surface_under(&layers, output_geo.loc, WlrLayer::Bottom, pos))
            .or_else(|| layer_surface_under(&layers, output_geo.loc, WlrLayer::Background, pos))
    }

    /// Whether the shell draws a layer surface, or a popup of one, above
    /// the windows at `pos` (a toast, the start menu). A press there is the
    /// shell's, never a title bar's under it (#EDE-119). A fullscreen window
    /// hides those layers (#EDE-103).
    pub fn layer_above_windows(&self, pos: Point<f64, Logical>) -> bool {
        let Some(output) = self.desk.space.output_under(pos).next() else { return false };
        let Some(output_geo) = self.desk.space.output_geometry(output) else { return false };
        if self.is_locked() || self.fullscreen_on(output).is_some() {
            return false;
        }
        layers_above_windows(&layer_map_for_output(output), output_geo.loc, pos).is_some()
    }

    /// Raises `window` and gives it keyboard focus; `None` clears focus.
    pub fn focus(&mut self, desk: DeskId, window: Option<Window>) {
        if self.desk_locked(desk) {
            // Windows (a new one mapping, a click) never take focus from
            // the lock screen.
            return;
        }
        if self.exclusive_prompt().is_some() {
            // Nor from an Exclusive prompt (#EDE-82): what had the keyboard
            // before it gets it back when the prompt closes.
            return;
        }
        let serial = SERIAL_COUNTER.next_serial();
        let keyboard = self.desk(desk).seat.get_keyboard().unwrap();
        if let Some(window) = &window {
            self.desk_mut(desk).space.raise_element(window, true);
            #[cfg(feature = "xwayland")]
            if let (Some(xwm), Some(x11)) = (self.xwm.as_mut(), window.x11_surface()) {
                let _ = xwm.raise_window(x11);
            }
        } else {
            for w in self.desk(desk).space.elements() {
                w.set_activated(false);
            }
        }
        // A fresh X11 window has no wl_surface until Xwayland associates
        // one; focus follows then (see the xwayland shell handler). One
        // back from the taskbar still names the surface Xwayland destroyed
        // when it was minimized (#EDE-122): it waits for its new one too.
        let surface = window.as_ref().and_then(|w| w.wl_surface()).map(|s| s.into_owned()).filter(|s| s.alive());
        #[cfg(feature = "xwayland")]
        {
            self.desk_mut(desk).pending_x11_focus = window.as_ref().filter(|_| surface.is_none()).and_then(|w| w.x11_surface().cloned());
        }
        #[cfg(feature = "xwayland")]
        let unset = surface.is_none();
        keyboard.set_focus(self, surface, serial);
        // The seat reports no change when the keyboard goes to nothing, so
        // X's own focus is told here (#EDE-122): else the X11 window that
        // gets the keyboard back next would still count as holding it.
        #[cfg(feature = "xwayland")]
        if unset {
            self.sync_x11_focus(None);
        }
        if let Some(w) = &window {
            self.window_event(ede_protocol::events::WINDOW_FOCUSED, w);
        }
        for w in self.desk(desk).space.elements() {
            if let Some(t) = w.toplevel() {
                t.send_pending_configure();
            }
        }
    }

    pub fn focused_window(&self, desk: DeskId) -> Option<Window> {
        let focus = self.desk(desk).seat.get_keyboard()?.current_focus()?;
        self.window_for_surface(&focus)
    }

    /// Moves `window` to `loc` in the space; X11 windows also learn where
    /// they are (their position is part of their state).
    pub fn move_window(&mut self, window: &Window, loc: Point<i32, Logical>, activate: bool) {
        self.desk.space.map_element(window.clone(), loc, activate);
        #[cfg(feature = "xwayland")]
        if let Some(x11) = window.x11_surface() {
            let _ = x11.configure(Some(Rectangle::new(loc, x11.last_configure().size)));
        }
    }

    /// [`Self::move_window`] where it stands in the stack: who is in front
    /// and who is activated stay (a display change, #EDE-139).
    pub fn relocate_window(&mut self, window: &Window, loc: Point<i32, Logical>) {
        self.remap(window, loc);
        #[cfg(feature = "xwayland")]
        if let Some(x11) = window.x11_surface() {
            let _ = x11.configure(Some(Rectangle::new(loc, x11.last_configure().size)));
        }
    }

    /// First buffer arrived: pick a spot on the pointer's output and map.
    pub fn place_new_window(&mut self, desk: DeskId, window: Window) {
        self.desk_mut(desk).unmapped.retain(|w| w != &window);
        let pointer = self.desk(desk).seat.get_pointer().unwrap().current_location();
        let mut st = window_state(&window);
        st.workspace = self.desk(desk).workspaces.active;
        set_window_state(&window, st);
        let fh = self.frame_height(&window);
        let loc = match self.output_at(pointer).and_then(|o| self.work_area(&o)) {
            Some(area) => {
                let taken: Vec<_> = self.desk(desk).space.elements().filter_map(|w| self.desk(desk).space.element_location(w)).collect();
                let mut size = window.geometry().size;
                size.h += fh;
                let mut at = layout::place_new(size, area, &taken, 32);
                at.y += fh;
                at
            }
            None => (0, fh).into(),
        };
        // #EDE-82, #EDE-76: only a window of the focused app, or one the
        // person launched, takes the keyboard; never from a prompt.
        let person = self.person_started(&window);
        let takes = self.takes_keyboard(&window) || (person && self.exclusive_prompt().is_none());
        self.move_window(&window, loc, takes);
        self.send_preferred_scale(&window);
        self.window_event(ede_protocol::events::WINDOW_OPENED, &window);
        if takes {
            self.focus(desk, Some(window));
        } else {
            self.keep_behind(&window);
        }
    }

    /// Tells a window's surface the fractional scale of the output under
    /// its centre (wp-fractional-scale), so HiDPI clients draw crisply.
    pub fn send_preferred_scale(&self, window: &Window) {
        let Some(surface) = window.wl_surface() else { return };
        let geo = self.desk.space.element_geometry(window).unwrap_or_else(|| Rectangle::from_size(window.geometry().size));
        let center = geo.loc.to_f64() + geo.size.to_f64().to_point().downscale(2.0);
        let Some(output) = self.output_at(center) else { return };
        let scale = output.current_scale().fractional_scale();
        smithay::wayland::compositor::with_states(&surface, |states| {
            smithay::wayland::fractional_scale::with_fractional_scale(states, |f| f.set_preferred_scale(scale));
        });
    }

    /// Gives `window` the geometry `rect` with the xdg states of `tile`
    /// (`None` = floating), on either shell.
    fn configure_window(&self, window: &Window, rect: Option<Rect>, tile: Option<Tile>, fullscreen: bool) {
        if let Some(t) = window.toplevel() {
            t.with_pending_state(|s| {
                s.size = rect.map(|r| r.size);
                for st in TILE_STATES {
                    s.states.unset(st);
                }
                s.states.unset(xdg_toplevel::State::Maximized);
                s.states.unset(xdg_toplevel::State::Fullscreen);
                if fullscreen {
                    s.states.set(xdg_toplevel::State::Fullscreen);
                } else if tile == Some(Tile::Maximized) {
                    s.states.set(xdg_toplevel::State::Maximized);
                } else if let Some(t) = tile {
                    for st in tile_edges(t) {
                        s.states.set(st);
                    }
                }
            });
            t.send_pending_configure();
        }
        #[cfg(feature = "xwayland")]
        if let Some(x11) = window.x11_surface() {
            let _ = x11.set_fullscreen(fullscreen);
            let _ = x11.set_maximized(tile == Some(Tile::Maximized));
            if let Some(rect) = rect {
                let _ = x11.configure(Some(rect));
            }
        }
    }

    /// Tiles `window` into `tile` on the output under its centre, remembering
    /// the floating geometry the first time.
    pub fn tile(&mut self, window: &Window, tile: Tile) {
        let Some(geo) = self.desk.space.element_geometry(window) else { return };
        let center = geo.loc.to_f64() + geo.size.to_f64().to_point().downscale(2.0);
        if let Some(output) = self.output_at(center) {
            self.tile_on(window, tile, &output);
        }
    }

    /// [`Self::tile`] on `output`, whichever output its old tile's centre
    /// lies on now (#EDE-139: the outputs moved under it).
    pub fn tile_on(&mut self, window: &Window, tile: Tile, output: &Output) {
        let Some(geo) = self.desk.space.element_geometry(window) else { return };
        let Some(area) = self.work_area(output) else { return };
        let mut st = window_state(window);
        if st.fullscreen.is_some() {
            return;
        }
        if st.tile.is_none() {
            st.restore = Some(geo);
        }
        st.tile = Some(tile);
        set_window_state(window, st);
        let rect = self.without_frame(window, tile.rect(area));
        self.configure_window(window, Some(rect), Some(tile), false);
        self.remap(window, rect.loc);
    }

    /// `output`'s work area changed (a panel's exclusive zone came, changed
    /// or went, #EDE-139): its tiled and maximized windows fill the new one,
    /// in place (who is in front stays).
    pub fn refit_tiles(&mut self, output: &Output) {
        for w in self.desk.space.elements().cloned().collect::<Vec<_>>() {
            let Some(tile) = window_state(&w).tile else { continue };
            let Some(geo) = self.desk.space.element_geometry(&w) else { continue };
            let center = geo.loc.to_f64() + geo.size.to_f64().to_point().downscale(2.0);
            if self.desk.space.output_under(center).next() == Some(output) {
                self.tile(&w, tile);
            }
        }
    }

    /// `rect` minus the title bar at its top (what the window itself gets).
    pub fn without_frame(&self, window: &Window, rect: Rect) -> Rect {
        let fh = self.frame_height(window);
        Rectangle::new((rect.loc.x, rect.loc.y + fh).into(), (rect.size.w, (rect.size.h - fh).max(1)).into())
    }

    /// Back to the floating geometry it had before tiling; `at` overrides the
    /// location (a drag pulling it off a tile).
    pub fn untile(&mut self, window: &Window, at: Option<Point<i32, Logical>>) {
        let mut st = window_state(window);
        if st.tile.take().is_none() {
            return;
        }
        let restore = st.restore.take();
        set_window_state(window, st);
        let loc = at.or(restore.map(|r| r.loc));
        let rect = restore.map(|r| Rectangle::new(loc.unwrap_or(r.loc), r.size));
        self.configure_window(window, rect, None, false);
        if let Some(loc) = loc {
            self.remap(window, loc);
        }
    }

    /// Fullscreen covers the whole output, panel included, and the output
    /// then draws only that window (games, video).
    pub fn set_fullscreen(&mut self, window: &Window, on: bool) {
        let mut st = window_state(window);
        if on == st.fullscreen.is_some() {
            return;
        }
        if on {
            let Some(geo) = self.desk.space.element_geometry(window).or_else(|| {
                let p = self.desk.seat.get_pointer().unwrap().current_location();
                Some(Rectangle::new(p.to_i32_round(), window.geometry().size))
            }) else {
                return;
            };
            let center = geo.loc.to_f64() + geo.size.to_f64().to_point().downscale(2.0);
            let Some(out) = self.output_at(center).and_then(|o| self.desk.space.output_geometry(&o)) else { return };
            st.fullscreen = Some(geo);
            set_window_state(window, st);
            self.configure_window(window, Some(out), None, true);
            // #EDE-82: a window in the background goes fullscreen where it
            // is, behind the one being typed into.
            let takes = self.takes_keyboard(window);
            self.remap(window, out.loc);
            if takes {
                self.focus(self.desk_of_window(window), Some(window.clone()));
            }
        } else {
            let back = st.fullscreen.take();
            set_window_state(window, st);
            match (st.tile, back) {
                (Some(tile), _) => {
                    let center = back.map(|b| b.loc.to_f64()).unwrap_or_default();
                    if let Some(area) = self.output_at(center).and_then(|o| self.work_area(&o)) {
                        let rect = self.without_frame(window, tile.rect(area));
                        self.configure_window(window, Some(rect), Some(tile), false);
                        self.remap(window, rect.loc);
                    }
                }
                (None, Some(back)) => {
                    self.configure_window(window, Some(back), None, false);
                    self.remap(window, back.loc);
                }
                (None, None) => {}
            }
        }
    }

    /// A fullscreen `window` covers `output` as it is now (its mode, scale
    /// or place changed, or the window came over from another, #EDE-139).
    pub fn fit_fullscreen(&mut self, window: &Window, output: &Output) {
        let Some(out) = self.desk.space.output_geometry(output) else { return };
        self.configure_window(window, Some(out), None, true);
        self.remap(window, out.loc);
    }

    /// The output background until the shell draws a wallpaper; black
    /// while locked (behind the lock surface, or instead of it).
    pub fn clear_color(&self) -> [f32; 4] {
        if self.is_locked() {
            return [0.0, 0.0, 0.0, 1.0];
        }
        self.config.background_rgba().unwrap_or([0.04, 0.04, 0.04, 1.0])
    }

    /// The window filling `output` fullscreen: the topmost window on it,
    /// when that one is fullscreen (#EDE-82: one behind another window
    /// never hides what is typed into). X11 menus and tooltips do not count.
    pub fn fullscreen_on(&self, output: &Output) -> Option<Window> {
        let out = self.desk.space.output_geometry(output)?;
        let top = self.desk.space.elements().rev().find(|w| {
            #[cfg(feature = "xwayland")]
            if w.x11_surface().is_some_and(|x| x.is_override_redirect()) {
                return false;
            }
            self.desk.space.element_geometry(w).is_some_and(|g| g.overlaps(out))
        })?;
        (window_state(top).fullscreen.is_some() && self.desk.space.element_location(top) == Some(out.loc)).then(|| top.clone())
    }

    /// The floating size a window returns to, if it is tiled.
    pub fn restore_size(&self, window: &Window) -> Option<Size<i32, Logical>> {
        window_state(window).restore.map(|r| r.size)
    }

    /// Asks the focused window to close (Alt+F4).
    pub fn close_focused(&mut self) {
        let Some(w) = self.focused_window(DeskId::PERSON) else { return };
        self.close_window(&w);
    }

    /// Asks `w` to close (it may ask the person first).
    pub fn close_window(&mut self, w: &Window) {
        if let Some(t) = w.toplevel() {
            t.send_close();
        }
        #[cfg(feature = "xwayland")]
        if let Some(x11) = w.x11_surface() {
            let _ = x11.close();
        }
    }
}

/// The topmost surface of `layer` under `pos`, with its origin. `origin` is
/// the output's place in the space.
fn layer_surface_under(layers: &LayerMap, origin: Point<i32, Logical>, layer: WlrLayer, pos: Point<f64, Logical>) -> Option<(WlSurface, Point<f64, Logical>)> {
    layers.layer_under(layer, pos - origin.to_f64()).and_then(|l| {
        let loc = layers.layer_geometry(l)?.loc + origin;
        l.surface_under(pos - loc.to_f64(), WindowSurfaceType::ALL).map(|(s, p)| (s, (p + loc).to_f64()))
    })
}

/// The topmost sealed prompt ([`is_sealed_prompt`]) under `pos`, or its
/// popup: what a fullscreen window does not hide (#EDE-107).
fn sealed_prompt_under(layers: &LayerMap, origin: Point<i32, Logical>, pos: Point<f64, Logical>) -> Option<(WlSurface, Point<f64, Logical>)> {
    layers.layers().rev().filter(|l| is_sealed_prompt(l)).find_map(|l| {
        let loc = layers.layer_geometry(l)?.loc + origin;
        l.surface_under(pos - loc.to_f64(), WindowSurfaceType::ALL).map(|(s, p)| (s, (p + loc).to_f64()))
    })
}

/// What the shell draws above every window at `pos`: a popup of an overlay
/// or top layer (the scene draws a layer's popups above the whole layer, so
/// they take the hit first), then those layers themselves.
fn layers_above_windows(layers: &LayerMap, origin: Point<i32, Logical>, pos: Point<f64, Logical>) -> Option<(WlSurface, Point<f64, Logical>)> {
    let popup = |layer| {
        layers.layers_on(layer).rev().find_map(|l| {
            let loc = layers.layer_geometry(l)?.loc + origin;
            l.surface_under(pos - loc.to_f64(), WindowSurfaceType::POPUP | WindowSurfaceType::SUBSURFACE).map(|(s, p)| (s, (p + loc).to_f64()))
        })
    };
    popup(WlrLayer::Overlay)
        .or_else(|| popup(WlrLayer::Top))
        .or_else(|| layer_surface_under(layers, origin, WlrLayer::Overlay, pos))
        .or_else(|| layer_surface_under(layers, origin, WlrLayer::Top, pos))
}

const TILE_STATES: [xdg_toplevel::State; 4] =
    [xdg_toplevel::State::TiledLeft, xdg_toplevel::State::TiledRight, xdg_toplevel::State::TiledTop, xdg_toplevel::State::TiledBottom];

/// The edges a tile touches neighbours on (xdg `tiled_*` = no rounded corners
/// or shadow on that side).
fn tile_edges(tile: Tile) -> Vec<xdg_toplevel::State> {
    use xdg_toplevel::State::*;
    match tile {
        Tile::Maximized => vec![],
        Tile::Left => vec![TiledLeft, TiledTop, TiledBottom],
        Tile::Right => vec![TiledRight, TiledTop, TiledBottom],
        Tile::TopLeft => vec![TiledLeft, TiledTop],
        Tile::TopRight => vec![TiledRight, TiledTop],
        Tile::BottomLeft => vec![TiledLeft, TiledBottom],
        Tile::BottomRight => vec![TiledRight, TiledBottom],
    }
}

#[derive(Default)]
pub struct ClientState {
    pub compositor_state: CompositorClientState,
    /// The desk its socket belongs to; the person's unless said otherwise.
    pub desk: DeskId,
    /// #EDE-162: its space draws on the GPU (decided when it connected): it
    /// sees the spaces' linux-dmabuf and drm-syncobj ([`crate::desk::on_gpu_space`]).
    pub gpu: bool,
    /// #EDE-172: the process that connected the person's socket, as it
    /// connected ([`crate::peer::Peer::of_socket`]); `None` for the
    /// compositor's own clients (Xwayland, a space's, a test's pair).
    pub peer: Option<Arc<crate::peer::Peer>>,
    /// #EDE-172 (P4): the compositor made this client for one of its
    /// helpers ([`crate::helpers`]): what it is, by construction.
    pub helper: Option<crate::helpers::Kind>,
    /// That helper's process, once it was started (0 = not known: the
    /// shell its supervisor starts). A socket pair's credentials name the
    /// compositor itself.
    pub helper_pid: std::sync::atomic::AtomicU32,
}

/// #EDE-172 (P4): the process behind `client`: the helper's the compositor
/// recorded for a client it made ([`ClientState::helper_pid`]), else its
/// socket's credentials; 0 = not known.
pub fn client_pid(dh: &DisplayHandle, client: &smithay::reexports::wayland_server::Client) -> u32 {
    match client.get_data::<ClientState>() {
        Some(state) if state.helper.is_some() => state.helper_pid.load(std::sync::atomic::Ordering::Relaxed),
        _ => client.get_credentials(dh).map(|c| c.pid as u32).unwrap_or(0),
    }
}

impl ClientData for ClientState {
    fn initialized(&self, _client_id: ClientId) {}
    fn disconnected(&self, _client_id: ClientId, _reason: DisconnectReason) {}
}
