//! A minimal Wayland client for the headless tests: xdg toplevels and layer
//! surfaces filled with one solid colour from a wl_shm buffer.

#![allow(dead_code)]

pub mod space;

use std::{io::Write, os::fd::AsFd, os::unix::net::UnixStream};

use wayland_client::{
    delegate_noop,
    protocol::{wl_buffer, wl_callback, wl_compositor, wl_data_device, wl_data_device_manager, wl_data_offer, wl_data_source, wl_keyboard, wl_output, wl_pointer, wl_region, wl_registry, wl_seat, wl_shm, wl_shm_pool, wl_surface},
    Connection, Dispatch, EventQueue, QueueHandle, WEnum,
};
use wayland_protocols_misc::zwp_virtual_keyboard_v1::client::{zwp_virtual_keyboard_manager_v1, zwp_virtual_keyboard_v1};
use wayland_protocols::xdg::decoration::zv1::client::{zxdg_decoration_manager_v1, zxdg_toplevel_decoration_v1};
use wayland_protocols::{
    ext::idle_notify::v1::client::{ext_idle_notification_v1, ext_idle_notifier_v1},
    ext::session_lock::v1::client::{ext_session_lock_manager_v1, ext_session_lock_surface_v1, ext_session_lock_v1},
    wp::fractional_scale::v1::client::{wp_fractional_scale_manager_v1, wp_fractional_scale_v1},
    wp::idle_inhibit::zv1::client::{zwp_idle_inhibit_manager_v1, zwp_idle_inhibitor_v1},
    xdg::shell::client::{xdg_popup, xdg_positioner, xdg_surface, xdg_toplevel, xdg_wm_base},
};
use wayland_protocols_wlr::layer_shell::v1::client::{zwlr_layer_shell_v1, zwlr_layer_surface_v1};

#[derive(Default)]
pub struct Client {
    pub compositor: Option<wl_compositor::WlCompositor>,
    pub shm: Option<wl_shm::WlShm>,
    pub wm_base: Option<xdg_wm_base::XdgWmBase>,
    pub layer_shell: Option<zwlr_layer_shell_v1::ZwlrLayerShellV1>,
    pub outputs: Vec<wl_output::WlOutput>,
    /// The name and the current mode each bound wl_output announced.
    pub output_names: Vec<String>,
    pub output_modes: Vec<(i32, i32)>,
    pub toplevels: Vec<ToplevelState>,
    pub layers: Vec<LayerState>,
    pub lock_manager: Option<ext_session_lock_manager_v1::ExtSessionLockManagerV1>,
    pub idle_inhibit_manager: Option<zwp_idle_inhibit_manager_v1::ZwpIdleInhibitManagerV1>,
    pub seat: Option<wl_seat::WlSeat>,
    /// The name each bound wl_seat announced.
    pub seat_names: Vec<String>,
    pub decoration_manager: Option<zxdg_decoration_manager_v1::ZxdgDecorationManagerV1>,
    /// The decoration mode the compositor configured per toplevel index.
    pub decoration_modes: std::collections::HashMap<usize, u32>,
    /// Every global the compositor advertised.
    pub globals: Vec<String>,
    /// Their registry names, and the names it took back (`global_remove`).
    pub global_names: Vec<u32>,
    pub removed: Vec<u32>,
    /// `locked` / `finished` on our lock object.
    pub locked: bool,
    pub finished: bool,
    pub lock_surfaces: Vec<LayerState>,
    /// The surface our keyboard is on, and the keys it received (evdev codes).
    pub keyboard_focus: Option<wl_surface::WlSurface>,
    pub keys: Vec<u32>,
    /// The keys it was told were let go, and how often it was sent a keymap.
    pub key_releases: Vec<u32>,
    pub keymaps: u32,
    /// The depressed modifiers of every `modifiers` event (Shift = 1).
    pub modifiers: Vec<u32>,
    /// What our pointer heard: enters, motions, and each button with its state.
    pub pointer_enters: u32,
    pub pointer_motions: u32,
    pub buttons: Vec<(u32, bool)>,
    pub idle_notifier: Option<ext_idle_notifier_v1::ExtIdleNotifierV1>,
    /// `idled` and `resumed` on our idle notifications.
    pub idled: u32,
    pub resumed: u32,
    /// The keymap the compositor sent (format, fd, size), for a virtual
    /// keyboard to upload.
    pub keymap: Option<(u32, std::os::fd::OwnedFd, u32)>,
    pub virtual_keyboard_manager: Option<zwp_virtual_keyboard_manager_v1::ZwpVirtualKeyboardManagerV1>,
    /// The serial of the last button press our pointer received (what a
    /// client-side title bar hands back with `move` and `resize`).
    pub press_serial: Option<u32>,
    pub data_device_manager: Option<wl_data_device_manager::WlDataDeviceManager>,
    /// Every offer the data device announced, with its types.
    pub offers: Vec<(wl_data_offer::WlDataOffer, Vec<String>)>,
    /// How often a drag entered one of our surfaces.
    pub drag_enters: u32,
    /// The offer the data device last named the selection (`None` = cleared).
    pub selection: Option<wl_data_offer::WlDataOffer>,
    /// What our own source serves per type; a type missing here is never
    /// answered (a hung client: its fd is kept open in `held`).
    pub source_data: Vec<(String, Vec<u8>)>,
    pub held: Vec<std::os::fd::OwnedFd>,
    pub source_cancelled: bool,
    pub fractional_scale_manager: Option<wp_fractional_scale_manager_v1::WpFractionalScaleManagerV1>,
    /// Every preferred scale a surface of ours was told, in 120ths.
    pub preferred_scales: Vec<u32>,
    /// Every popup configure: x, y (in its parent's geometry), width, height.
    pub popup_configures: Vec<(i32, i32, i32, i32)>,
    /// Frame callbacks received through [`Session::count_frames`].
    pub frames: u32,
    /// The callback [`Session::count_frames`] waits on (another surface's
    /// earlier one is not counted).
    pub frame_pending: Option<wl_callback::WlCallback>,
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

pub struct Popup {
    pub surface: wl_surface::WlSurface,
    pub xdg_surface: xdg_surface::XdgSurface,
    pub popup: xdg_popup::XdgPopup,
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

    /// Asks for a server-side frame on `t` (xdg-decoration, #EDE-16);
    /// `None` = ask the compositor to decide.
    pub fn decorate(&mut self, t: &Toplevel, mode: Option<zxdg_toplevel_decoration_v1::Mode>) -> zxdg_toplevel_decoration_v1::ZxdgToplevelDecorationV1 {
        let manager = self.state.decoration_manager.as_ref().expect("xdg-decoration advertised");
        let decoration = manager.get_toplevel_decoration(&t.toplevel, &self.qh, t.index);
        match mode {
            Some(m) => decoration.set_mode(m),
            None => decoration.unset_mode(),
        }
        t.surface.commit();
        self.roundtrip();
        decoration
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

    /// A toplevel that names `parent` before its first commit (a dialog).
    pub fn child_toplevel(&mut self, parent: &Toplevel, title: &str) -> Toplevel {
        let surface = self.state.compositor.as_ref().unwrap().create_surface(&self.qh, ());
        let index = self.state.toplevels.len();
        self.state.toplevels.push(ToplevelState::default());
        let xdg_surface = self.state.wm_base.as_ref().unwrap().get_xdg_surface(&surface, &self.qh, index);
        let toplevel = xdg_surface.get_toplevel(&self.qh, index);
        toplevel.set_parent(Some(&parent.toplevel));
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

    /// A layer surface on `on` with no anchors (the compositor centres it)
    /// and the keyboard interactivity a prompt or a shell surface asks for.
    pub fn keyboard_layer(&mut self, namespace: &str, on: zwlr_layer_shell_v1::Layer, size: (u32, u32), keyboard: zwlr_layer_surface_v1::KeyboardInteractivity) -> Layer {
        let surface = self.state.compositor.as_ref().unwrap().create_surface(&self.qh, ());
        let index = self.state.layers.len();
        self.state.layers.push(LayerState::default());
        let layer = self.state.layer_shell.as_ref().expect("layer shell advertised").get_layer_surface(&surface, None, on, namespace.into(), &self.qh, index);
        layer.set_size(size.0, size.1);
        layer.set_keyboard_interactivity(keyboard);
        surface.commit();
        self.roundtrip();
        Layer { index, surface, layer }
    }

    /// A drawn `w`×`h` `xdg_popup` under the layer surface `l` (the shell's
    /// start menu under its taskbar); `grab` = it asks for the grab, that
    /// is for the keyboard (#EDE-86).
    pub fn layer_popup(&mut self, l: &Layer, (w, h): (i32, i32), grab: bool, argb: u32) -> Popup {
        let surface = self.state.compositor.as_ref().unwrap().create_surface(&self.qh, ());
        let index = self.state.toplevels.len();
        self.state.toplevels.push(ToplevelState::default());
        let wm_base = self.state.wm_base.clone().unwrap();
        let xdg_surface = wm_base.get_xdg_surface(&surface, &self.qh, index);
        let positioner = wm_base.create_positioner(&self.qh, ());
        positioner.set_size(w, h);
        positioner.set_anchor_rect(0, 0, 1, 1);
        let popup = xdg_surface.get_popup(None, &positioner, &self.qh, ());
        l.layer.get_popup(&popup);
        positioner.destroy();
        if grab {
            // No press to name: a hotkey opened it.
            popup.grab(self.state.seat.as_ref().expect("seat advertised"), 0);
        }
        surface.commit();
        self.roundtrip();
        if let Some(serial) = self.state.toplevels[index].pending_serial.take() {
            xdg_surface.ack_configure(serial);
        }
        let buffer = self.buffer(w, h, argb);
        surface.attach(Some(&buffer), 0, 0);
        surface.damage_buffer(0, 0, w, h);
        surface.commit();
        self.roundtrip();
        Popup { surface, xdg_surface, popup }
    }

    pub fn close_popup(&mut self, p: Popup) {
        p.popup.destroy();
        p.xdg_surface.destroy();
        p.surface.destroy();
        self.roundtrip();
    }

    /// Frame callbacks `surface` gets during `period`, asked for again after
    /// each one like a client that animates (#EDE-100).
    pub fn count_frames(&mut self, surface: &wl_surface::WlSurface, period: std::time::Duration) -> u32 {
        let (start, until) = (self.state.frames, std::time::Instant::now() + period);
        self.state.frame_pending = None;
        while std::time::Instant::now() < until {
            if self.state.frame_pending.is_none() {
                self.state.frame_pending = Some(surface.frame(&self.qh, ()));
                surface.commit();
            }
            std::thread::sleep(std::time::Duration::from_millis(4));
            self.roundtrip();
        }
        self.state.frames - start
    }

    /// An empty input region: no pointer event reaches `surface`, what is
    /// under it gets them (#EXP-1912: the computer-use banner).
    pub fn no_input(&mut self, surface: &wl_surface::WlSurface) {
        self.input_region(surface, None);
    }

    /// The input region of `surface` is `rect` (x, y, w, h) alone (#EXP-1912:
    /// the call pill inside its layer surface); `None` = empty.
    pub fn input_region(&mut self, surface: &wl_surface::WlSurface, rect: Option<(i32, i32, i32, i32)>) {
        let region = self.state.compositor.as_ref().unwrap().create_region(&self.qh, ());
        if let Some((x, y, w, h)) = rect {
            region.add(x, y, w, h);
        }
        surface.set_input_region(Some(&region));
        region.destroy();
        surface.commit();
        self.roundtrip();
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

    /// Binds the seat's pointer so `press_serial` fills in.
    pub fn pointer(&mut self) -> wl_pointer::WlPointer {
        let pointer = self.state.seat.as_ref().expect("seat advertised").get_pointer(&self.qh, ());
        self.roundtrip();
        pointer
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
        if let wl_registry::Event::GlobalRemove { name } = event {
            state.removed.push(name);
        }
        if let wl_registry::Event::Global { name, interface, version } = event {
            state.globals.push(interface.clone());
            state.global_names.push(name);
            match interface.as_str() {
                "ext_session_lock_manager_v1" => state.lock_manager = Some(registry.bind(name, 1, qh, ())),
                "zwp_idle_inhibit_manager_v1" => state.idle_inhibit_manager = Some(registry.bind(name, 1, qh, ())),
                "zwp_virtual_keyboard_manager_v1" => state.virtual_keyboard_manager = Some(registry.bind(name, 1, qh, ())),
                "ext_idle_notifier_v1" => state.idle_notifier = Some(registry.bind(name, 1, qh, ())),
                "wl_seat" => state.seat = Some(registry.bind(name, version.min(7), qh, ())),
                "wl_compositor" => state.compositor = Some(registry.bind(name, version.min(5), qh, ())),
                "wl_shm" => state.shm = Some(registry.bind(name, 1, qh, ())),
                "xdg_wm_base" => state.wm_base = Some(registry.bind(name, version.min(5), qh, ())),
                "zwlr_layer_shell_v1" => state.layer_shell = Some(registry.bind(name, version.min(4), qh, ())),
                "wl_output" => state.outputs.push(registry.bind(name, version.min(4), qh, ())),
                "wl_data_device_manager" => state.data_device_manager = Some(registry.bind(name, version.min(3), qh, ())),
                "zxdg_decoration_manager_v1" => state.decoration_manager = Some(registry.bind(name, 1, qh, ())),
                "wp_fractional_scale_manager_v1" => state.fractional_scale_manager = Some(registry.bind(name, 1, qh, ())),
                _ => {}
            }
        }
    }
}

impl Dispatch<zxdg_decoration_manager_v1::ZxdgDecorationManagerV1, ()> for Client {
    fn event(_: &mut Self, _: &zxdg_decoration_manager_v1::ZxdgDecorationManagerV1, _: zxdg_decoration_manager_v1::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {}
}

impl Dispatch<zxdg_toplevel_decoration_v1::ZxdgToplevelDecorationV1, usize> for Client {
    fn event(state: &mut Self, _: &zxdg_toplevel_decoration_v1::ZxdgToplevelDecorationV1, event: zxdg_toplevel_decoration_v1::Event, index: &usize, _: &Connection, _: &QueueHandle<Self>) {
        if let zxdg_toplevel_decoration_v1::Event::Configure { mode } = event {
            if let wayland_client::WEnum::Value(m) = mode {
                state.decoration_modes.insert(*index, m as u32);
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

impl Dispatch<wl_seat::WlSeat, ()> for Client {
    fn event(state: &mut Self, _: &wl_seat::WlSeat, event: wl_seat::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {
        if let wl_seat::Event::Name { name } = event {
            state.seat_names.push(name);
        }
    }
}

impl Dispatch<wl_output::WlOutput, ()> for Client {
    fn event(state: &mut Self, _: &wl_output::WlOutput, event: wl_output::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {
        match event {
            wl_output::Event::Name { name } => state.output_names.push(name),
            wl_output::Event::Mode { flags: WEnum::Value(flags), width, height, .. } if flags.contains(wl_output::Mode::Current) => state.output_modes.push((width, height)),
            _ => {}
        }
    }
}

impl Dispatch<wl_keyboard::WlKeyboard, ()> for Client {
    fn event(state: &mut Self, _: &wl_keyboard::WlKeyboard, event: wl_keyboard::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {
        match event {
            wl_keyboard::Event::Keymap { format: WEnum::Value(format), fd, size } => {
                state.keymaps += 1;
                state.keymap = Some((format as u32, fd, size));
            }
            wl_keyboard::Event::Key { key, state: WEnum::Value(wl_keyboard::KeyState::Released), .. } => state.key_releases.push(key),
            wl_keyboard::Event::Enter { surface, .. } => state.keyboard_focus = Some(surface),
            wl_keyboard::Event::Leave { .. } => state.keyboard_focus = None,
            wl_keyboard::Event::Key { key, state: WEnum::Value(wl_keyboard::KeyState::Pressed), .. } => state.keys.push(key),
            wl_keyboard::Event::Modifiers { mods_depressed, .. } => state.modifiers.push(mods_depressed),
            _ => {}
        }
    }
}

impl Dispatch<wl_pointer::WlPointer, ()> for Client {
    fn event(state: &mut Self, _: &wl_pointer::WlPointer, event: wl_pointer::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {
        match event {
            wl_pointer::Event::Enter { .. } => state.pointer_enters += 1,
            wl_pointer::Event::Motion { .. } => state.pointer_motions += 1,
            wl_pointer::Event::Button { serial, button, state: WEnum::Value(pressed), .. } => {
                let pressed = pressed == wl_pointer::ButtonState::Pressed;
                state.buttons.push((button, pressed));
                if pressed {
                    state.press_serial = Some(serial);
                }
            }
            _ => {}
        }
    }
}

impl Session {
    /// The seat's data device (call [`Session::keyboard`] first: the
    /// selection goes to the keyboard-focused client only).
    pub fn data_device(&mut self) -> wl_data_device::WlDataDevice {
        let seat = self.state.seat.clone().expect("seat advertised");
        let device = self.state.data_device_manager.as_ref().expect("data device manager advertised").get_data_device(&seat, &self.qh, ());
        self.roundtrip();
        device
    }

    /// Starts a drag of one text type from `origin` with the last press.
    pub fn start_drag(&mut self, device: &wl_data_device::WlDataDevice, origin: &wl_surface::WlSurface) -> wl_data_source::WlDataSource {
        let source = self.state.data_device_manager.as_ref().unwrap().create_data_source(&self.qh, ());
        source.offer("text/plain".into());
        device.start_drag(Some(&source), origin, None, self.state.press_serial.expect("a press"));
        self.roundtrip();
        source
    }

    /// An idle notification on our seat after `ms` without input.
    pub fn idle_notification(&mut self, ms: u32) -> ext_idle_notification_v1::ExtIdleNotificationV1 {
        let seat = self.state.seat.clone().expect("seat advertised");
        let notification = self.state.idle_notifier.as_ref().expect("idle notify advertised").get_idle_notification(ms, &seat, &self.qh, ());
        self.roundtrip();
        notification
    }

    /// Sets the clipboard to a source offering `types` (`None` data = never answered).
    pub fn set_selection(&mut self, device: &wl_data_device::WlDataDevice, types: &[(&str, Option<&[u8]>)]) -> wl_data_source::WlDataSource {
        let source = self.state.data_device_manager.as_ref().unwrap().create_data_source(&self.qh, ());
        self.state.source_data.clear();
        self.state.source_cancelled = false;
        for (mime, data) in types {
            source.offer(mime.to_string());
            if let Some(data) = data {
                self.state.source_data.push((mime.to_string(), data.to_vec()));
            }
        }
        device.set_selection(Some(&source), 0);
        self.roundtrip();
        source
    }

    /// The types of the current selection's offer.
    pub fn selection_types(&self) -> Option<Vec<String>> {
        let offer = self.state.selection.as_ref()?;
        self.state.offers.iter().find(|(o, _)| o == offer).map(|(_, types)| types.clone())
    }

    /// Reads `mime` of the current selection to the end, pumping events
    /// while the compositor writes.
    pub fn read_selection(&mut self, mime: &str) -> Vec<u8> {
        use std::io::Read;
        let offer = self.state.selection.clone().expect("a selection");
        let (mut read, write) = std::io::pipe().expect("pipe");
        offer.receive(mime.into(), write.as_fd());
        drop(write);
        self.roundtrip();
        let mut out = Vec::new();
        read.read_to_end(&mut out).expect("read the selection");
        out
    }
}

impl Dispatch<wl_data_device::WlDataDevice, ()> for Client {
    fn event(state: &mut Self, _: &wl_data_device::WlDataDevice, event: wl_data_device::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {
        match event {
            wl_data_device::Event::DataOffer { id } => state.offers.push((id, Vec::new())),
            wl_data_device::Event::Selection { id } => state.selection = id,
            wl_data_device::Event::Enter { .. } => state.drag_enters += 1,
            _ => {}
        }
    }

    wayland_client::event_created_child!(Client, wl_data_device::WlDataDevice, [
        wl_data_device::EVT_DATA_OFFER_OPCODE => (wl_data_offer::WlDataOffer, ()),
    ]);
}

impl Dispatch<wl_data_offer::WlDataOffer, ()> for Client {
    fn event(state: &mut Self, offer: &wl_data_offer::WlDataOffer, event: wl_data_offer::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {
        if let wl_data_offer::Event::Offer { mime_type } = event {
            if let Some((_, types)) = state.offers.iter_mut().find(|(o, _)| o == offer) {
                types.push(mime_type);
            }
        }
    }
}

impl Dispatch<wl_data_source::WlDataSource, ()> for Client {
    fn event(state: &mut Self, _: &wl_data_source::WlDataSource, event: wl_data_source::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {
        match event {
            wl_data_source::Event::Send { mime_type, fd } => match state.source_data.iter().find(|(m, _)| *m == mime_type) {
                Some((_, data)) => {
                    let _ = std::fs::File::from(fd).write_all(data);
                }
                None => state.held.push(fd),
            },
            wl_data_source::Event::Cancelled => state.source_cancelled = true,
            _ => {}
        }
    }
}

impl Dispatch<ext_idle_notification_v1::ExtIdleNotificationV1, ()> for Client {
    fn event(state: &mut Self, _: &ext_idle_notification_v1::ExtIdleNotificationV1, event: ext_idle_notification_v1::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {
        match event {
            ext_idle_notification_v1::Event::Idled => state.idled += 1,
            ext_idle_notification_v1::Event::Resumed => state.resumed += 1,
            _ => {}
        }
    }
}

impl Dispatch<wl_callback::WlCallback, ()> for Client {
    fn event(state: &mut Self, callback: &wl_callback::WlCallback, event: wl_callback::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {
        if matches!(event, wl_callback::Event::Done { .. }) && state.frame_pending.as_ref() == Some(callback) {
            state.frames += 1;
            state.frame_pending = None;
        }
    }
}

delegate_noop!(Client: ignore wl_data_device_manager::WlDataDeviceManager);
delegate_noop!(Client: ignore ext_idle_notifier_v1::ExtIdleNotifierV1);
delegate_noop!(Client: ignore zwp_virtual_keyboard_manager_v1::ZwpVirtualKeyboardManagerV1);
delegate_noop!(Client: ignore zwp_virtual_keyboard_v1::ZwpVirtualKeyboardV1);
delegate_noop!(Client: ignore ext_session_lock_manager_v1::ExtSessionLockManagerV1);
delegate_noop!(Client: ignore zwp_idle_inhibit_manager_v1::ZwpIdleInhibitManagerV1);
delegate_noop!(Client: ignore zwp_idle_inhibitor_v1::ZwpIdleInhibitorV1);
delegate_noop!(Client: ignore wl_compositor::WlCompositor);
delegate_noop!(Client: ignore wl_surface::WlSurface);
delegate_noop!(Client: wl_region::WlRegion);
delegate_noop!(Client: ignore wl_shm::WlShm);
delegate_noop!(Client: ignore wl_shm_pool::WlShmPool);
delegate_noop!(Client: ignore wl_buffer::WlBuffer);
delegate_noop!(Client: ignore zwlr_layer_shell_v1::ZwlrLayerShellV1);
delegate_noop!(Client: xdg_positioner::XdgPositioner);
delegate_noop!(Client: ignore wp_fractional_scale_manager_v1::WpFractionalScaleManagerV1);

impl Dispatch<wp_fractional_scale_v1::WpFractionalScaleV1, ()> for Client {
    fn event(state: &mut Self, _: &wp_fractional_scale_v1::WpFractionalScaleV1, event: wp_fractional_scale_v1::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {
        if let wp_fractional_scale_v1::Event::PreferredScale { scale } = event {
            state.preferred_scales.push(scale);
        }
    }
}

impl Dispatch<xdg_popup::XdgPopup, ()> for Client {
    fn event(state: &mut Self, _: &xdg_popup::XdgPopup, event: xdg_popup::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {
        if let xdg_popup::Event::Configure { x, y, width, height } = event {
            state.popup_configures.push((x, y, width, height));
        }
    }
}

impl Session {
    /// The preferred scale of `surface` (in 120ths), as the compositor says it now.
    pub fn preferred_scale(&mut self, surface: &wl_surface::WlSurface) -> Option<u32> {
        let told = self.state.preferred_scales.len();
        let scale = self.state.fractional_scale_manager.as_ref().expect("fractional scale advertised").get_fractional_scale(surface, &self.qh, ());
        self.roundtrip();
        scale.destroy();
        self.state.preferred_scales[told..].last().copied()
    }

    /// A drawn `w`x`h` `xdg_popup` of the window `parent`, its top left
    /// corner asked for at `at` in the window (the compositor may slide
    /// it); `grab` = it asks for the grab. Its configure, when one came,
    /// is the last of [`Client::popup_configures`].
    pub fn window_popup(&mut self, parent: &Toplevel, at: (i32, i32), (w, h): (i32, i32), grab: bool, argb: u32) -> Popup {
        let surface = self.state.compositor.as_ref().unwrap().create_surface(&self.qh, ());
        let index = self.state.toplevels.len();
        self.state.toplevels.push(ToplevelState::default());
        let wm_base = self.state.wm_base.clone().unwrap();
        let xdg_surface = wm_base.get_xdg_surface(&surface, &self.qh, index);
        let positioner = wm_base.create_positioner(&self.qh, ());
        positioner.set_size(w, h);
        positioner.set_anchor_rect(at.0, at.1, 1, 1);
        positioner.set_anchor(xdg_positioner::Anchor::TopLeft);
        positioner.set_gravity(xdg_positioner::Gravity::BottomRight);
        positioner.set_constraint_adjustment(xdg_positioner::ConstraintAdjustment::SlideX | xdg_positioner::ConstraintAdjustment::SlideY);
        let popup = xdg_surface.get_popup(Some(&parent.xdg_surface), &positioner, &self.qh, ());
        positioner.destroy();
        if grab {
            popup.grab(self.state.seat.as_ref().expect("seat advertised"), 0);
        }
        surface.commit();
        self.roundtrip();
        if let Some(serial) = self.state.toplevels[index].pending_serial.take() {
            xdg_surface.ack_configure(serial);
            let buffer = self.buffer(w, h, argb);
            surface.attach(Some(&buffer), 0, 0);
            surface.damage_buffer(0, 0, w, h);
            surface.commit();
            self.roundtrip();
        }
        Popup { surface, xdg_surface, popup }
    }
}

