//! The compositor state every backend drives: the Wayland globals, the space
//! of floating windows, the seat. Backends (winit, headless) only add outputs,
//! feed input through [`crate::input`] and render [`Ede::space`].

use std::{cell::RefCell, ffi::OsString, sync::Arc, time::Instant};

use smithay::{
    desktop::{layer_map_for_output, PopupManager, Space, Window, WindowSurfaceType},
    input::{keyboard::XkbConfig, Seat, SeatState},
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
    pub popups: PopupManager,

    pub compositor_state: CompositorState,
    pub xdg_shell_state: XdgShellState,
    pub layer_shell_state: WlrLayerShellState,
    pub shm_state: ShmState,
    pub output_manager_state: OutputManagerState,
    pub seat_state: SeatState<Ede>,
    pub data_device_state: DataDeviceState,
    pub seat: Seat<Ede>,
}

/// Per-window floating state, kept in the window's user data.
#[derive(Debug, Default, Clone, Copy)]
pub struct WindowState {
    pub tile: Option<Tile>,
    /// The floating geometry to go back to when the window leaves a tile.
    pub restore: Option<Rect>,
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

        Self {
            start_time: Instant::now(),
            display_handle: dh,
            loop_handle,
            loop_signal,
            socket_name: None,
            config,
            space: Space::default(),
            unmapped: Vec::new(),
            popups: PopupManager::default(),
            compositor_state,
            xdg_shell_state,
            layer_shell_state,
            shm_state,
            output_manager_state,
            seat_state,
            data_device_state,
            seat,
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
        Ok(name)
    }

    /// Lays outputs out left to right in the order they were added.
    pub fn add_output(&mut self, output: &Output) {
        let x = self.space.outputs().filter_map(|o| self.space.output_geometry(o)).map(|g| g.loc.x + g.size.w).max().unwrap_or(0);
        self.space.map_output(output, (x, 0));
        layer_map_for_output(output).arrange();
    }

    /// The output's area minus exclusive layer-shell zones (a panel), in
    /// space coordinates.
    pub fn work_area(&self, output: &Output) -> Option<Rect> {
        let geo = self.space.output_geometry(output)?;
        let zone = layer_map_for_output(output).non_exclusive_zone();
        Some(Rectangle::new(geo.loc + zone.loc, zone.size))
    }

    pub fn output_at(&self, p: Point<f64, Logical>) -> Option<Output> {
        self.space.output_under(p).next().or_else(|| self.space.outputs().next()).cloned()
    }

    pub fn window_for_surface(&self, surface: &WlSurface) -> Option<Window> {
        self.space
            .elements()
            .chain(self.unmapped.iter())
            .find(|w| w.toplevel().is_some_and(|t| t.wl_surface() == surface))
            .cloned()
    }

    /// The topmost surface under `pos`: overlay/top layers, then windows, then
    /// bottom/background layers, with that surface's origin.
    pub fn surface_under(&self, pos: Point<f64, Logical>) -> Option<(WlSurface, Point<f64, Logical>)> {
        use smithay::wayland::shell::wlr_layer::Layer;
        let output = self.space.output_under(pos).next()?;
        let output_geo = self.space.output_geometry(output)?;
        let layers = layer_map_for_output(output);
        let layer_hit = |layer| {
            layers.layer_under(layer, pos - output_geo.loc.to_f64()).and_then(|l| {
                let loc = layers.layer_geometry(l)?.loc + output_geo.loc;
                l.surface_under(pos - loc.to_f64(), WindowSurfaceType::ALL).map(|(s, p)| (s, (p + loc).to_f64()))
            })
        };
        layer_hit(Layer::Overlay)
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
        let serial = SERIAL_COUNTER.next_serial();
        let keyboard = self.seat.get_keyboard().unwrap();
        if let Some(window) = &window {
            self.space.raise_element(window, true);
        } else {
            for w in self.space.elements() {
                w.set_activated(false);
            }
        }
        let surface = window.as_ref().and_then(|w| w.toplevel()).map(|t| t.wl_surface().clone());
        keyboard.set_focus(self, surface, serial);
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

    /// First buffer arrived: pick a spot on the pointer's output and map.
    pub fn place_new_window(&mut self, window: Window) {
        self.unmapped.retain(|w| w != &window);
        let pointer = self.seat.get_pointer().unwrap().current_location();
        let Some(area) = self.output_at(pointer).and_then(|o| self.work_area(&o)) else {
            self.space.map_element(window.clone(), (0, 0), true);
            self.focus(Some(window));
            return;
        };
        let taken: Vec<_> = self.space.elements().filter_map(|w| self.space.element_location(w)).collect();
        let loc = layout::place_new(window.geometry().size, area, &taken, 32);
        self.space.map_element(window.clone(), loc, true);
        self.focus(Some(window));
    }

    /// Tiles `window` into `tile` on the output under its centre, remembering
    /// the floating geometry the first time.
    pub fn tile(&mut self, window: &Window, tile: Tile) {
        let Some(geo) = self.space.element_geometry(window) else { return };
        let center = geo.loc.to_f64() + geo.size.to_f64().to_point().downscale(2.0);
        let Some(area) = self.output_at(center).and_then(|o| self.work_area(&o)) else { return };
        let mut st = window_state(window);
        if st.tile.is_none() {
            st.restore = Some(geo);
        }
        st.tile = Some(tile);
        set_window_state(window, st);
        let rect = tile.rect(area);
        if let Some(t) = window.toplevel() {
            t.with_pending_state(|s| {
                s.size = Some(rect.size);
                for st in TILE_STATES {
                    s.states.unset(st);
                }
                s.states.unset(xdg_toplevel::State::Maximized);
                match tile {
                    Tile::Maximized => {
                        s.states.set(xdg_toplevel::State::Maximized);
                    }
                    t => {
                        for st in tile_edges(t) {
                            s.states.set(st);
                        }
                    }
                }
            });
            t.send_pending_configure();
        }
        self.space.map_element(window.clone(), rect.loc, true);
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
        if let Some(t) = window.toplevel() {
            t.with_pending_state(|s| {
                s.size = restore.map(|r| r.size);
                for st in TILE_STATES {
                    s.states.unset(st);
                }
                s.states.unset(xdg_toplevel::State::Maximized);
            });
            t.send_pending_configure();
        }
        if let Some(loc) = at.or(restore.map(|r| r.loc)) {
            self.space.map_element(window.clone(), loc, true);
        }
    }

    /// The floating size a window returns to, if it is tiled.
    pub fn restore_size(&self, window: &Window) -> Option<Size<i32, Logical>> {
        window_state(window).restore.map(|r| r.size)
    }

    /// Asks the focused window to close (Alt+F4).
    pub fn close_focused(&mut self) {
        if let Some(t) = self.focused_window().and_then(|w| w.toplevel().cloned()) {
            t.send_close();
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
