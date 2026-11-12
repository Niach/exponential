//! The compositor state every backend drives: the Wayland globals, the space
//! of floating windows, the seat. Backends (winit, headless) only add outputs,
//! feed input through [`crate::input`] and render [`Ede::space`].

use std::{cell::RefCell, ffi::OsString, sync::Arc, time::Instant};

use smithay::{
    desktop::{layer_map_for_output, PopupManager, Space, Window, WindowSurfaceType},
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
    utils::{Logical, Point, Rectangle, Size, SERIAL_COUNTER},
    wayland::{
        compositor::{CompositorClientState, CompositorState},
        output::OutputManagerState,
        selection::data_device::DataDeviceState,
        shell::{wlr_layer::WlrLayerShellState, xdg::XdgShellState},
        shm::ShmState,
        socket::ListeningSocketSource,
    },
};

use crate::{
    config::Config,
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

    /// Mapped windows, back to front.
    pub space: Space<Window>,
    /// Toplevels that exist but have not committed a buffer yet: they are
    /// placed when their first frame arrives, so the size is known.
    pub unmapped: Vec<Window>,
    /// Windows in the taskbar only (#EDE-48, [`crate::minimize`]): out of
    /// the space, their place and workspace remembered.
    pub minimized: Vec<Window>,
    pub popups: PopupManager,

    pub compositor_state: CompositorState,
    pub xdg_shell_state: XdgShellState,
    pub layer_shell_state: WlrLayerShellState,
    pub shm_state: ShmState,
    pub output_manager_state: OutputManagerState,
    pub seat_state: SeatState<Ede>,
    pub data_device_state: DataDeviceState,
    pub seat: Seat<Ede>,
    /// What the pointer looks like (a client's cursor surface, hidden, or
    /// the default arrow); backends that draw their own cursor read it.
    pub cursor_status: CursorImageStatus,

    /// Set by a backend that owns a seat session: switch to VT n.
    pub vt_switch: Option<Box<dyn FnMut(i32)>>,
    /// Commands started once clients can connect (after Xwayland is up).
    pub autostart: Vec<Vec<String>>,
    pub dmabuf_state: smithay::wayland::dmabuf::DmabufState,
    /// Set by the backend that owns the renderer: can it import this buffer?
    pub dmabuf_importer: Option<DmabufImporter>,
    /// Explicit sync (drm-syncobj), when the primary GPU supports it.
    #[cfg(feature = "udev")]
    pub syncobj_state: Option<smithay::wayland::drm_syncobj::DrmSyncobjState>,

    #[cfg(feature = "xwayland")]
    pub xwayland_shell_state: smithay::wayland::xwayland_shell::XWaylandShellState,
    /// The X11 window manager once Xwayland is up.
    #[cfg(feature = "xwayland")]
    pub xwm: Option<smithay::xwayland::X11Wm>,
    /// Xwayland's display number (`DISPLAY=:N`).
    #[cfg(feature = "xwayland")]
    pub xdisplay: Option<u32>,
    /// An X11 window that was focused before it had a wl_surface.
    #[cfg(feature = "xwayland")]
    pub pending_x11_focus: Option<smithay::xwayland::X11Surface>,

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
    /// The virtual keyboard whose keymap the seat holds (until a real key).
    pub virtual_keymap: Option<smithay::input::keyboard::KeyboardSource>,
    /// ede-protocol: the control socket, its grants and clients.
    pub ipc: crate::ipc::Ipc,
    /// The seat's CLIPBOARD serial, owner and reads ([`crate::clipboard`]).
    pub clipboard: crate::clipboard::Clipboard,
    /// Alt+Tab, Super, the media keys: what the shell answers ([`crate::hotkeys`]).
    pub hotkeys: crate::hotkeys::Hotkeys,
    /// The active workspace and the parked windows ([`crate::workspaces`]).
    pub workspaces: crate::workspaces::Workspaces,
    /// wp-fractional-scale + wp-viewporter: HiDPI at 1.5× and the like.
    pub fractional_scale_state: smithay::wayland::fractional_scale::FractionalScaleManagerState,
    pub viewporter_state: smithay::wayland::viewporter::ViewporterState,
    /// Set by a backend that can change an output's mode (udev: a modeset).
    pub set_mode: Option<Box<dyn FnMut(&Output, smithay::output::Mode) -> Result<(), String>>>,
    /// xdg-decoration: who draws a window's frame ([`crate::frames`]).
    pub xdg_decoration_state: smithay::wayland::shell::xdg::decoration::XdgDecorationState,
    /// The recovery screen's state ([`crate::recovery`]).
    pub recovery: crate::recovery::Recovery,
    /// Why the backend stopped the loop (#EDE-40: the outputs never drew);
    /// the run ends with this error.
    pub fatal: Option<String>,
    /// The last press on a title bar, for the double-click.
    pub title_click: Option<(Window, Instant)>,
    /// `--debug-captures` (headless and nested only): captures are not redacted.
    pub debug_captures: bool,
    /// Counts presented frames ([`crate::backend::frame_done`]); a capture
    /// with `since` equal to it carries no pixels.
    pub frame_serial: u64,
    /// Where `reload_config` reads from; `None` in safe mode.
    pub config_path: Option<std::path::PathBuf>,
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
        let layer_shell_state = WlrLayerShellState::new::<Self>(&dh);
        let shm_state = ShmState::new::<Self>(&dh, vec![]);
        let output_manager_state = OutputManagerState::new_with_xdg_output::<Self>(&dh);
        let data_device_state = DataDeviceState::new::<Self>(&dh);
        let mut seat_state = SeatState::new();

        let mut seat: Seat<Self> = seat_state.new_wl_seat(&dh, "seat0");
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

        let idle_notifier = smithay::wayland::idle_notify::IdleNotifierState::new(&dh, loop_handle.clone());
        crate::idle::start_timer(&loop_handle);

        Self {
            start_time: Instant::now(),
            display_handle: dh.clone(),
            loop_handle,
            loop_signal,
            socket_name: None,
            config,
            space: Space::default(),
            unmapped: Vec::new(),
            minimized: Vec::new(),
            popups: PopupManager::default(),
            compositor_state,
            xdg_shell_state,
            layer_shell_state,
            shm_state,
            output_manager_state,
            seat_state,
            data_device_state,
            seat,
            cursor_status: CursorImageStatus::default_named(),
            vt_switch: None,
            autostart: Vec::new(),
            dmabuf_state: smithay::wayland::dmabuf::DmabufState::new(),
            dmabuf_importer: None,
            #[cfg(feature = "udev")]
            syncobj_state: None,
            #[cfg(feature = "xwayland")]
            xwayland_shell_state: smithay::wayland::xwayland_shell::XWaylandShellState::new::<Self>(&dh),
            #[cfg(feature = "xwayland")]
            xwm: None,
            #[cfg(feature = "xwayland")]
            xdisplay: None,
            #[cfg(feature = "xwayland")]
            pending_x11_focus: None,
            session_lock_state: smithay::wayland::session_lock::SessionLockManagerState::new::<Self, _>(&dh, |_| true),
            lock: crate::lock::LockState::default(),
            idle_notifier,
            idle_inhibit_state: smithay::wayland::idle_inhibit::IdleInhibitManagerState::new::<Self>(&dh),
            idle: crate::idle::Idle::default(),
            input_method_state: smithay::wayland::input_method::InputMethodManagerState::new::<Self, _>(&dh, |_| true),
            text_input_state: smithay::wayland::text_input::TextInputManagerState::new::<Self>(&dh),
            virtual_keyboard_state: smithay::wayland::virtual_keyboard::VirtualKeyboardManagerState::new::<Self, _>(&dh, |_| true),
            virtual_keymap: None,
            ipc: crate::ipc::Ipc::default(),
            clipboard: Default::default(),
            hotkeys: crate::hotkeys::Hotkeys::default(),
            workspaces: crate::workspaces::Workspaces::default(),
            fractional_scale_state: smithay::wayland::fractional_scale::FractionalScaleManagerState::new::<Self>(&dh),
            viewporter_state: smithay::wayland::viewporter::ViewporterState::new::<Self>(&dh),
            set_mode: None,
            xdg_decoration_state: smithay::wayland::shell::xdg::decoration::XdgDecorationState::new::<Self>(&dh),
            recovery: crate::recovery::Recovery::default(),
            fatal: None,
            title_click: None,
            debug_captures: false,
            frame_serial: 0,
            config_path: None,
        }
    }

    /// Opens the next free `$XDG_RUNTIME_DIR/wayland-N` socket.
    pub fn listen(&mut self) -> std::io::Result<OsString> {
        let source = ListeningSocketSource::new_auto().map_err(std::io::Error::other)?;
        let name = source.socket_name().to_os_string();
        self.loop_handle
            .insert_source(source, |stream, _, state| {
                if let Err(e) = state.display_handle.insert_client(stream, Arc::new(ClientState::default())) {
                    tracing::warn!("client refused: {e}");
                }
            })
            .map_err(|e| std::io::Error::other(e.to_string()))?;
        self.socket_name = Some(name.clone());
        if let Some(runtime) = std::env::var_os("XDG_RUNTIME_DIR") {
            match self.ipc_listen(std::path::Path::new(&runtime), &name.to_string_lossy()) {
                Ok(path) => tracing::info!("ede-protocol on {}", path.display()),
                Err(e) => tracing::warn!("ede-protocol socket: {e}"),
            }
        }
        Ok(name)
    }

    /// Starts Xwayland if enabled, then the autostart commands.
    pub fn start_session(&mut self) {
        #[cfg(feature = "xwayland")]
        if self.config.xwayland {
            // Autostart runs from the Ready/Error event.
            self.start_xwayland();
            return;
        }
        self.run_autostart();
    }

    pub fn run_autostart(&mut self) {
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
        let (program, args) = argv.split_first()?;
        let mut cmd = std::process::Command::new(program);
        cmd.args(args).env_remove("DISPLAY").env_remove("WAYLAND_DISPLAY");
        if let Some(socket) = &self.socket_name {
            cmd.env("WAYLAND_DISPLAY", socket);
        }
        #[cfg(feature = "xwayland")]
        if let Some(n) = self.xdisplay {
            cmd.env("DISPLAY", format!(":{n}"));
        }
        for (k, v) in env {
            cmd.env(k, v);
        }
        match cmd.spawn() {
            Ok(child) => Some(child),
            Err(e) => {
                tracing::warn!("{program}: {e}");
                None
            }
        }
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
            None => (self.space.outputs().filter_map(|o| self.space.output_geometry(o)).map(|g| g.loc.x + g.size.w).max().unwrap_or(0), 0),
        };
        self.space.map_output(output, at);
        layer_map_for_output(output).arrange();
        self.emit(ede_protocol::events::OUTPUT_CHANGED, serde_json::json!({ "output": output.name() }));
    }

    /// Unplugged: the rest close ranks left to right, and the windows that
    /// were on it (or end up on no output) reopen on the first one.
    pub fn remove_output(&mut self, output: &Output) {
        let center = |space: &Space<Window>, w: &Window| {
            space.element_geometry(w).map(|g| g.loc.to_f64() + g.size.to_f64().to_point().downscale(2.0))
        };
        let gone = self.space.output_geometry(output);
        let on_it: Vec<Window> = self
            .space
            .elements()
            .filter(|w| gone.zip(center(&self.space, w)).is_some_and(|(g, c)| g.to_f64().contains(c)))
            .cloned()
            .collect();
        self.space.unmap_output(output);
        self.lock_output_removed(output);
        let outputs: Vec<Output> = self.space.outputs().cloned().collect();
        let mut x = 0;
        for o in outputs {
            let w = self.space.output_geometry(&o).map(|g| g.size.w).unwrap_or(0);
            self.space.map_output(&o, (x, 0));
            layer_map_for_output(&o).arrange();
            x += w;
        }
        self.rehome_lost_windows(&on_it);
    }

    /// Windows in `also`, and any whose centre is on no output, reopen on
    /// the first output (an output went away or shrank under them).
    pub fn rehome_lost_windows(&mut self, also: &[Window]) {
        let center = |space: &Space<Window>, w: &Window| {
            space.element_geometry(w).map(|g| g.loc.to_f64() + g.size.to_f64().to_point().downscale(2.0))
        };
        let Some(area) = self.space.outputs().next().and_then(|o| self.work_area(o)) else { return };
        let orphans: Vec<Window> = self
            .space
            .elements()
            .filter(|w| also.contains(w) || center(&self.space, w).is_some_and(|c| self.space.output_under(c).next().is_none()))
            .cloned()
            .collect();
        for w in orphans {
            // A tiled window becomes floating again at its old size.
            let mut size = self.restore_size(&w).unwrap_or(w.geometry().size);
            self.untile(&w, None);
            let fh = self.frame_height(&w);
            size.h += fh;
            let taken: Vec<_> = self.space.elements().filter(|o| *o != &w).filter_map(|o| self.space.element_location(o)).collect();
            let mut loc = layout::place_new(size, area, &taken, 32);
            loc.y += fh;
            self.move_window(&w, loc, false);
        }
    }

    /// The output's area minus exclusive layer-shell zones (a panel), inset
    /// by `[windows] panel_margin` on every side (the IDE look, #EDE-48),
    /// in space coordinates.
    pub fn work_area(&self, output: &Output) -> Option<Rect> {
        let geo = self.space.output_geometry(output)?;
        let zone = layer_map_for_output(output).non_exclusive_zone();
        Some(layout::inset(Rectangle::new(geo.loc + zone.loc, zone.size), self.config.windows.panel_margin))
    }

    pub fn output_at(&self, p: Point<f64, Logical>) -> Option<Output> {
        self.space.output_under(p).next().or_else(|| self.space.outputs().next()).cloned()
    }

    pub fn window_for_surface(&self, surface: &WlSurface) -> Option<Window> {
        self.space.elements().chain(self.unmapped.iter()).find(|w| w.wl_surface().is_some_and(|s| *s == *surface)).cloned()
    }

    /// The topmost surface under `pos`: overlay/top layers, then windows, then
    /// bottom/background layers, with that surface's origin.
    pub fn surface_under(&self, pos: Point<f64, Logical>) -> Option<(WlSurface, Point<f64, Logical>)> {
        use smithay::wayland::shell::wlr_layer::Layer;
        let output = self.space.output_under(pos).next()?;
        let output_geo = self.space.output_geometry(output)?;
        if self.is_locked() {
            // Only the lock client, nothing behind it.
            let lock = self.lock_surface_on(output)?;
            return Some((lock, output_geo.loc.to_f64()));
        }
        let layers = layer_map_for_output(output);
        let layer_hit = |layer| {
            layers.layer_under(layer, pos - output_geo.loc.to_f64()).and_then(|l| {
                let loc = layers.layer_geometry(l)?.loc + output_geo.loc;
                l.surface_under(pos - loc.to_f64(), WindowSurfaceType::ALL).map(|(s, p)| (s, (p + loc).to_f64()))
            })
        };
        // A layer's popups are drawn above the whole layer (the scene), so
        // they take the hit first.
        let popup_hit = |layer| {
            layers.layers_on(layer).rev().find_map(|l| {
                let loc = layers.layer_geometry(l)?.loc + output_geo.loc;
                l.surface_under(pos - loc.to_f64(), WindowSurfaceType::POPUP | WindowSurfaceType::SUBSURFACE).map(|(s, p)| (s, (p + loc).to_f64()))
            })
        };
        popup_hit(Layer::Overlay)
            .or_else(|| popup_hit(Layer::Top))
            .or_else(|| layer_hit(Layer::Overlay))
            .or_else(|| layer_hit(Layer::Top))
            .or_else(|| {
                self.space.element_under(pos).and_then(|(window, location)| {
                    window
                        .surface_under(pos - location.to_f64(), WindowSurfaceType::ALL)
                        .map(|(s, p)| (s, (p + location).to_f64()))
                })
            })
            .or_else(|| layer_hit(Layer::Bottom))
            .or_else(|| layer_hit(Layer::Background))
    }

    /// Raises `window` and gives it keyboard focus; `None` clears focus.
    pub fn focus(&mut self, window: Option<Window>) {
        if self.is_locked() {
            // Windows (a new one mapping, a click) never take focus from
            // the lock screen.
            return;
        }
        let serial = SERIAL_COUNTER.next_serial();
        let keyboard = self.seat.get_keyboard().unwrap();
        if let Some(window) = &window {
            self.space.raise_element(window, true);
            #[cfg(feature = "xwayland")]
            if let (Some(xwm), Some(x11)) = (self.xwm.as_mut(), window.x11_surface()) {
                let _ = xwm.raise_window(x11);
            }
        } else {
            for w in self.space.elements() {
                w.set_activated(false);
            }
        }
        let surface = window.as_ref().and_then(|w| w.wl_surface()).map(|s| s.into_owned());
        // A fresh X11 window has no wl_surface until Xwayland associates
        // one; focus follows then (see the xwayland shell handler).
        #[cfg(feature = "xwayland")]
        {
            self.pending_x11_focus = window.as_ref().filter(|_| surface.is_none()).and_then(|w| w.x11_surface().cloned());
        }
        keyboard.set_focus(self, surface, serial);
        if let Some(w) = &window {
            self.window_event(ede_protocol::events::WINDOW_FOCUSED, w);
        }
        for w in self.space.elements() {
            if let Some(t) = w.toplevel() {
                t.send_pending_configure();
            }
        }
    }

    pub fn focused_window(&self) -> Option<Window> {
        let focus = self.seat.get_keyboard()?.current_focus()?;
        self.window_for_surface(&focus)
    }

    /// Moves `window` to `loc` in the space; X11 windows also learn where
    /// they are (their position is part of their state).
    pub fn move_window(&mut self, window: &Window, loc: Point<i32, Logical>, activate: bool) {
        self.space.map_element(window.clone(), loc, activate);
        #[cfg(feature = "xwayland")]
        if let Some(x11) = window.x11_surface() {
            let _ = x11.configure(Some(Rectangle::new(loc, x11.last_configure().size)));
        }
    }

    /// First buffer arrived: pick a spot on the pointer's output and map.
    pub fn place_new_window(&mut self, window: Window) {
        self.unmapped.retain(|w| w != &window);
        let pointer = self.seat.get_pointer().unwrap().current_location();
        let mut st = window_state(&window);
        st.workspace = self.workspaces.active;
        set_window_state(&window, st);
        let fh = self.frame_height(&window);
        let loc = match self.output_at(pointer).and_then(|o| self.work_area(&o)) {
            Some(area) => {
                let taken: Vec<_> = self.space.elements().filter_map(|w| self.space.element_location(w)).collect();
                let mut size = window.geometry().size;
                size.h += fh;
                let mut at = layout::place_new(size, area, &taken, 32);
                at.y += fh;
                at
            }
            None => (0, fh).into(),
        };
        self.move_window(&window, loc, true);
        self.send_preferred_scale(&window);
        self.window_event(ede_protocol::events::WINDOW_OPENED, &window);
        self.focus(Some(window));
    }

    /// Tells a window's surface the fractional scale of the output under
    /// its centre (wp-fractional-scale), so HiDPI clients draw crisply.
    pub fn send_preferred_scale(&self, window: &Window) {
        let Some(surface) = window.wl_surface() else { return };
        let geo = self.space.element_geometry(window).unwrap_or_else(|| Rectangle::from_size(window.geometry().size));
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
        let Some(geo) = self.space.element_geometry(window) else { return };
        let center = geo.loc.to_f64() + geo.size.to_f64().to_point().downscale(2.0);
        let Some(area) = self.output_at(center).and_then(|o| self.work_area(&o)) else { return };
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
        self.space.map_element(window.clone(), rect.loc, true);
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
            self.space.map_element(window.clone(), loc, true);
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
            let Some(geo) = self.space.element_geometry(window).or_else(|| {
                let p = self.seat.get_pointer().unwrap().current_location();
                Some(Rectangle::new(p.to_i32_round(), window.geometry().size))
            }) else {
                return;
            };
            let center = geo.loc.to_f64() + geo.size.to_f64().to_point().downscale(2.0);
            let Some(out) = self.output_at(center).and_then(|o| self.space.output_geometry(&o)) else { return };
            st.fullscreen = Some(geo);
            set_window_state(window, st);
            self.configure_window(window, Some(out), None, true);
            self.space.map_element(window.clone(), out.loc, true);
            self.focus(Some(window.clone()));
        } else {
            let back = st.fullscreen.take();
            set_window_state(window, st);
            match (st.tile, back) {
                (Some(tile), _) => {
                    let center = back.map(|b| b.loc.to_f64()).unwrap_or_default();
                    if let Some(area) = self.output_at(center).and_then(|o| self.work_area(&o)) {
                        let rect = self.without_frame(window, tile.rect(area));
                        self.configure_window(window, Some(rect), Some(tile), false);
                        self.space.map_element(window.clone(), rect.loc, true);
                    }
                }
                (None, Some(back)) => {
                    self.configure_window(window, Some(back), None, false);
                    self.space.map_element(window.clone(), back.loc, true);
                }
                (None, None) => {}
            }
        }
    }

    /// The output background until the shell draws a wallpaper; black
    /// while locked (behind the lock surface, or instead of it).
    pub fn clear_color(&self) -> [f32; 4] {
        if self.is_locked() {
            return [0.0, 0.0, 0.0, 1.0];
        }
        self.config.background_rgba().unwrap_or([0.04, 0.04, 0.04, 1.0])
    }

    /// The window filling `output` fullscreen, if any (topmost wins).
    pub fn fullscreen_on(&self, output: &Output) -> Option<Window> {
        let out = self.space.output_geometry(output)?;
        self.space
            .elements()
            .rev()
            .find(|w| window_state(w).fullscreen.is_some() && self.space.element_location(w) == Some(out.loc))
            .cloned()
    }

    /// The floating size a window returns to, if it is tiled.
    pub fn restore_size(&self, window: &Window) -> Option<Size<i32, Logical>> {
        window_state(window).restore.map(|r| r.size)
    }

    /// Asks the focused window to close (Alt+F4).
    pub fn close_focused(&mut self) {
        let Some(w) = self.focused_window() else { return };
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
}

impl ClientData for ClientState {
    fn initialized(&self, _client_id: ClientId) {}
    fn disconnected(&self, _client_id: ClientId, _reason: DisconnectReason) {}
}
