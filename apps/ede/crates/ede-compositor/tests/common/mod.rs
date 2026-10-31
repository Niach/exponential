//! A minimal Wayland client for the headless tests: xdg toplevels and layer
//! surfaces filled with one solid colour from a wl_shm buffer.

#![allow(dead_code)]

use std::{io::Write, os::fd::AsFd, os::unix::net::UnixStream};

use wayland_client::{
    delegate_noop,
    protocol::{wl_buffer, wl_compositor, wl_keyboard, wl_output, wl_registry, wl_seat, wl_shm, wl_shm_pool, wl_surface},
    Connection, Dispatch, EventQueue, QueueHandle, WEnum,
};
use wayland_protocols_misc::zwp_virtual_keyboard_v1::client::{zwp_virtual_keyboard_manager_v1, zwp_virtual_keyboard_v1};
use wayland_protocols::{
    ext::session_lock::v1::client::{ext_session_lock_manager_v1, ext_session_lock_surface_v1, ext_session_lock_v1},
    wp::idle_inhibit::zv1::client::{zwp_idle_inhibit_manager_v1, zwp_idle_inhibitor_v1},
    xdg::shell::client::{xdg_surface, xdg_toplevel, xdg_wm_base},
};
use wayland_protocols_wlr::layer_shell::v1::client::{zwlr_layer_shell_v1, zwlr_layer_surface_v1};

#[derive(Default)]
pub struct Client {
    pub compositor: Option<wl_compositor::WlCompositor>,
    pub shm: Option<wl_shm::WlShm>,
    pub wm_base: Option<xdg_wm_base::XdgWmBase>,
    pub layer_shell: Option<zwlr_layer_shell_v1::ZwlrLayerShellV1>,
    pub outputs: Vec<wl_output::WlOutput>,
    pub toplevels: Vec<ToplevelState>,
    pub layers: Vec<LayerState>,
    pub lock_manager: Option<ext_session_lock_manager_v1::ExtSessionLockManagerV1>,
    pub idle_inhibit_manager: Option<zwp_idle_inhibit_manager_v1::ZwpIdleInhibitManagerV1>,
    pub seat: Option<wl_seat::WlSeat>,
    /// Every global the compositor advertised.
    pub globals: Vec<String>,
    /// `locked` / `finished` on our lock object.
    pub locked: bool,
    pub finished: bool,
    pub lock_surfaces: Vec<LayerState>,
    /// The surface our keyboard is on, and the keys it received (evdev codes).
    pub keyboard_focus: Option<wl_surface::WlSurface>,
    pub keys: Vec<u32>,
    /// The keymap the compositor sent (format, fd, size), for a virtual
    /// keyboard to upload.
    pub keymap: Option<(u32, std::os::fd::OwnedFd, u32)>,
    pub virtual_keyboard_manager: Option<zwp_virtual_keyboard_manager_v1::ZwpVirtualKeyboardManagerV1>,
}

#[derive(Default, Debug, Clone)]
pub struct ToplevelState {
    /// The last configure's size (0 = client's choice) and states.
    pub size: (i32, i32),
    pub states: Vec<xdg_toplevel::State>,
    pub pending_serial: Option<u32>,
    pub configures: u32,
    pub closed: bool,
}

#[derive(Default, Debug, Clone)]
pub struct LayerState {
    pub size: (u32, u32),
    pub pending_serial: Option<u32>,
}

pub struct Toplevel {
    pub index: usize,
    pub surface: wl_surface::WlSurface,
    pub xdg_surface: xdg_surface::XdgSurface,
    pub toplevel: xdg_toplevel::XdgToplevel,
}

pub struct Layer {
    pub index: usize,
    pub surface: wl_surface::WlSurface,
    pub layer: zwlr_layer_surface_v1::ZwlrLayerSurfaceV1,
}

pub struct LockSurface {
    pub index: usize,
    pub surface: wl_surface::WlSurface,
    pub lock_surface: ext_session_lock_surface_v1::ExtSessionLockSurfaceV1,
}

pub struct Session {
    pub conn: Connection,
    pub queue: EventQueue<Client>,
    pub qh: QueueHandle<Client>,
    pub state: Client,
}

impl Session {
    pub fn connect(stream: UnixStream) -> Self {
        let conn = Connection::from_socket(stream).expect("connect");
        let queue = conn.new_event_queue();
        let qh = queue.handle();
        conn.display().get_registry(&qh, ());
        let mut s = Self { conn, queue, qh, state: Client::default() };
        s.roundtrip();
        s.roundtrip();
        assert!(s.state.compositor.is_some() && s.state.shm.is_some() && s.state.wm_base.is_some(), "core globals advertised");
        s
    }

    pub fn roundtrip(&mut self) {
        self.queue.roundtrip(&mut self.state).expect("roundtrip");
    }

    pub fn toplevel(&mut self, title: &str) -> Toplevel {
        let surface = self.state.compositor.as_ref().unwrap().create_surface(&self.qh, ());
        let index = self.state.toplevels.len();
        self.state.toplevels.push(ToplevelState::default());
        let xdg_surface = self.state.wm_base.as_ref().unwrap().get_xdg_surface(&surface, &self.qh, index);
        let toplevel = xdg_surface.get_toplevel(&self.qh, index);
        toplevel.set_title(title.into());
        surface.commit();
        self.roundtrip();
        Toplevel { index, surface, xdg_surface, toplevel }
    }

    /// Acks the last configure and commits a `w`×`h` buffer of `argb`.
    pub fn draw(&mut self, t: &Toplevel, w: i32, h: i32, argb: u32) {
        if let Some(serial) = self.state.toplevels[t.index].pending_serial.take() {
            t.xdg_surface.ack_configure(serial);
        }
        let buffer = self.buffer(w, h, argb);
        t.surface.attach(Some(&buffer), 0, 0);
        t.surface.damage_buffer(0, 0, w, h);
        t.surface.commit();
        self.roundtrip();
    }

    /// Draws at the size the compositor last asked for (falls back to `w`×`h`).
    pub fn draw_configured(&mut self, t: &Toplevel, w: i32, h: i32, argb: u32) {
        let (cw, ch) = self.state.toplevels[t.index].size;
        let (w, h) = if cw > 0 && ch > 0 { (cw, ch) } else { (w, h) };
        self.draw(t, w, h, argb);
    }

    pub fn layer(&mut self, namespace: &str, anchor: zwlr_layer_surface_v1::Anchor, size: (u32, u32), exclusive: i32) -> Layer {
        let surface = self.state.compositor.as_ref().unwrap().create_surface(&self.qh, ());
        let index = self.state.layers.len();
        self.state.layers.push(LayerState::default());
        let layer = self.state.layer_shell.as_ref().expect("layer shell advertised").get_layer_surface(
            &surface,
            None,
            zwlr_layer_shell_v1::Layer::Top,
            namespace.into(),
            &self.qh,
            index,
        );
        layer.set_anchor(anchor);
        layer.set_size(size.0, size.1);
        layer.set_exclusive_zone(exclusive);
        surface.commit();
        self.roundtrip();
        Layer { index, surface, layer }
    }

    pub fn draw_layer(&mut self, l: &Layer, argb: u32) {
        let st = self.state.layers[l.index].clone();
        if let Some(serial) = st.pending_serial {
            l.layer.ack_configure(serial);
        }
        let (w, h) = (st.size.0 as i32, st.size.1 as i32);
        let buffer = self.buffer(w, h, argb);
        l.surface.attach(Some(&buffer), 0, 0);
        l.surface.damage_buffer(0, 0, w, h);
        l.surface.commit();
        self.roundtrip();
    }

    /// The placeholder lock screen: `ext_session_lock_manager_v1.lock`.
    pub fn lock(&mut self) -> ext_session_lock_v1::ExtSessionLockV1 {
        let lock = self.state.lock_manager.as_ref().expect("session lock advertised").lock(&self.qh, ());
        self.roundtrip();
        lock
    }

    /// One lock surface per output we know of.
    pub fn lock_surfaces(&mut self, lock: &ext_session_lock_v1::ExtSessionLockV1) -> Vec<LockSurface> {
        let outputs = self.state.outputs.clone();
        let made = outputs
            .iter()
            .map(|output| {
                let surface = self.state.compositor.as_ref().unwrap().create_surface(&self.qh, ());
                let index = self.state.lock_surfaces.len();
                self.state.lock_surfaces.push(LayerState::default());
                let lock_surface = lock.get_lock_surface(&surface, output, &self.qh, index);
                LockSurface { index, surface, lock_surface }
            })
            .collect();
        self.roundtrip();
        made
    }

    /// Acks the configure and fills the lock surface with `argb`.
    pub fn draw_lock(&mut self, l: &LockSurface, argb: u32) {
        let st = self.state.lock_surfaces[l.index].clone();
        if let Some(serial) = st.pending_serial {
            l.lock_surface.ack_configure(serial);
        }
        let (w, h) = (st.size.0 as i32, st.size.1 as i32);
        let buffer = self.buffer(w, h, argb);
        l.surface.attach(Some(&buffer), 0, 0);
        l.surface.damage_buffer(0, 0, w, h);
        l.surface.commit();
        self.roundtrip();
    }

    /// Binds the seat's keyboard so `keyboard_focus` and `keys` fill in.
    pub fn keyboard(&mut self) -> wl_keyboard::WlKeyboard {
        let keyboard = self.state.seat.as_ref().expect("seat advertised").get_keyboard(&self.qh, ());
        self.roundtrip();
        keyboard
    }

    /// A virtual keyboard with the seat's keymap (call [`Session::keyboard`] first).
    pub fn virtual_keyboard(&mut self) -> zwp_virtual_keyboard_v1::ZwpVirtualKeyboardV1 {
        let seat = self.state.seat.clone().expect("seat advertised");
        let vk = self.state.virtual_keyboard_manager.as_ref().expect("virtual keyboard advertised").create_virtual_keyboard(&seat, &self.qh, ());
        let (format, fd, size) = self.state.keymap.as_ref().expect("keymap received");
        vk.keymap(*format, fd.as_fd(), *size);
        self.roundtrip();
        vk
    }

    pub fn inhibit_idle(&mut self, surface: &wl_surface::WlSurface) -> zwp_idle_inhibitor_v1::ZwpIdleInhibitorV1 {
        let inhibitor = self.state.idle_inhibit_manager.as_ref().expect("idle inhibit advertised").create_inhibitor(surface, &self.qh, ());
        self.roundtrip();
        inhibitor
    }

    fn buffer(&mut self, w: i32, h: i32, argb: u32) -> wl_buffer::WlBuffer {
        let mut file = tempfile::tempfile().expect("shm file");
        let bytes: Vec<u8> = std::iter::repeat_n(argb.to_le_bytes(), (w * h) as usize).flatten().collect();
        file.write_all(&bytes).unwrap();
        let pool = self.state.shm.as_ref().unwrap().create_pool(file.as_fd(), w * h * 4, &self.qh, ());
        let buffer = pool.create_buffer(0, w, h, w * 4, wl_shm::Format::Argb8888, &self.qh, ());
        pool.destroy();
        buffer
    }
}

impl Dispatch<wl_registry::WlRegistry, ()> for Client {
    fn event(state: &mut Self, registry: &wl_registry::WlRegistry, event: wl_registry::Event, _: &(), _: &Connection, qh: &QueueHandle<Self>) {
        if let wl_registry::Event::Global { name, interface, version } = event {
            state.globals.push(interface.clone());
            match interface.as_str() {
                "ext_session_lock_manager_v1" => state.lock_manager = Some(registry.bind(name, 1, qh, ())),
                "zwp_idle_inhibit_manager_v1" => state.idle_inhibit_manager = Some(registry.bind(name, 1, qh, ())),
                "zwp_virtual_keyboard_manager_v1" => state.virtual_keyboard_manager = Some(registry.bind(name, 1, qh, ())),
                "wl_seat" => state.seat = Some(registry.bind(name, version.min(7), qh, ())),
                "wl_compositor" => state.compositor = Some(registry.bind(name, version.min(5), qh, ())),
                "wl_shm" => state.shm = Some(registry.bind(name, 1, qh, ())),
                "xdg_wm_base" => state.wm_base = Some(registry.bind(name, version.min(5), qh, ())),
                "zwlr_layer_shell_v1" => state.layer_shell = Some(registry.bind(name, version.min(4), qh, ())),
                "wl_output" => state.outputs.push(registry.bind(name, version.min(4), qh, ())),
                _ => {}
            }
        }
    }
}

impl Dispatch<xdg_wm_base::XdgWmBase, ()> for Client {
    fn event(_: &mut Self, wm_base: &xdg_wm_base::XdgWmBase, event: xdg_wm_base::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {
        if let xdg_wm_base::Event::Ping { serial } = event {
            wm_base.pong(serial);
        }
    }
}

impl Dispatch<xdg_surface::XdgSurface, usize> for Client {
    fn event(state: &mut Self, _: &xdg_surface::XdgSurface, event: xdg_surface::Event, index: &usize, _: &Connection, _: &QueueHandle<Self>) {
        if let xdg_surface::Event::Configure { serial } = event {
            let t = &mut state.toplevels[*index];
            t.pending_serial = Some(serial);
            t.configures += 1;
        }
    }
}

impl Dispatch<xdg_toplevel::XdgToplevel, usize> for Client {
    fn event(state: &mut Self, _: &xdg_toplevel::XdgToplevel, event: xdg_toplevel::Event, index: &usize, _: &Connection, _: &QueueHandle<Self>) {
        let t = &mut state.toplevels[*index];
        match event {
            xdg_toplevel::Event::Configure { width, height, states } => {
                t.size = (width, height);
                t.states = states
                    .chunks_exact(4)
                    .filter_map(|c| xdg_toplevel::State::try_from(u32::from_ne_bytes([c[0], c[1], c[2], c[3]])).ok())
                    .collect();
            }
            xdg_toplevel::Event::Close => t.closed = true,
            _ => {}
        }
    }
}

impl Dispatch<zwlr_layer_surface_v1::ZwlrLayerSurfaceV1, usize> for Client {
    fn event(
        state: &mut Self,
        _: &zwlr_layer_surface_v1::ZwlrLayerSurfaceV1,
        event: zwlr_layer_surface_v1::Event,
        index: &usize,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let zwlr_layer_surface_v1::Event::Configure { serial, width, height } = event {
            let l = &mut state.layers[*index];
            l.pending_serial = Some(serial);
            l.size = (width, height);
        }
    }
}

impl Dispatch<ext_session_lock_v1::ExtSessionLockV1, ()> for Client {
    fn event(state: &mut Self, _: &ext_session_lock_v1::ExtSessionLockV1, event: ext_session_lock_v1::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {
        match event {
            ext_session_lock_v1::Event::Locked => state.locked = true,
            ext_session_lock_v1::Event::Finished => state.finished = true,
            _ => {}
        }
    }
}

impl Dispatch<ext_session_lock_surface_v1::ExtSessionLockSurfaceV1, usize> for Client {
    fn event(
        state: &mut Self,
        _: &ext_session_lock_surface_v1::ExtSessionLockSurfaceV1,
        event: ext_session_lock_surface_v1::Event,
        index: &usize,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let ext_session_lock_surface_v1::Event::Configure { serial, width, height } = event {
            let l = &mut state.lock_surfaces[*index];
            l.pending_serial = Some(serial);
            l.size = (width, height);
        }
    }
}

impl Dispatch<wl_keyboard::WlKeyboard, ()> for Client {
    fn event(state: &mut Self, _: &wl_keyboard::WlKeyboard, event: wl_keyboard::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {
        match event {
            wl_keyboard::Event::Keymap { format: WEnum::Value(format), fd, size } => state.keymap = Some((format as u32, fd, size)),
            wl_keyboard::Event::Enter { surface, .. } => state.keyboard_focus = Some(surface),
            wl_keyboard::Event::Leave { .. } => state.keyboard_focus = None,
            wl_keyboard::Event::Key { key, state: WEnum::Value(wl_keyboard::KeyState::Pressed), .. } => state.keys.push(key),
            _ => {}
        }
    }
}

delegate_noop!(Client: ignore wl_seat::WlSeat);
delegate_noop!(Client: ignore zwp_virtual_keyboard_manager_v1::ZwpVirtualKeyboardManagerV1);
delegate_noop!(Client: ignore zwp_virtual_keyboard_v1::ZwpVirtualKeyboardV1);
delegate_noop!(Client: ignore ext_session_lock_manager_v1::ExtSessionLockManagerV1);
delegate_noop!(Client: ignore zwp_idle_inhibit_manager_v1::ZwpIdleInhibitManagerV1);
delegate_noop!(Client: ignore zwp_idle_inhibitor_v1::ZwpIdleInhibitorV1);
delegate_noop!(Client: ignore wl_compositor::WlCompositor);
delegate_noop!(Client: ignore wl_surface::WlSurface);
delegate_noop!(Client: ignore wl_shm::WlShm);
delegate_noop!(Client: ignore wl_shm_pool::WlShmPool);
delegate_noop!(Client: ignore wl_buffer::WlBuffer);
delegate_noop!(Client: ignore wl_output::WlOutput);
delegate_noop!(Client: ignore zwlr_layer_shell_v1::ZwlrLayerShellV1);

