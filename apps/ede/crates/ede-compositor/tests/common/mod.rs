//! A minimal Wayland client for the headless tests: xdg toplevels and layer
//! surfaces filled with one solid colour from a wl_shm buffer.

#![allow(dead_code)]

use std::{io::Write, os::fd::AsFd, os::unix::net::UnixStream};

use wayland_client::{
    delegate_noop,
    protocol::{wl_buffer, wl_compositor, wl_output, wl_registry, wl_shm, wl_shm_pool, wl_surface},
    Connection, Dispatch, EventQueue, QueueHandle,
};
use wayland_protocols::xdg::shell::client::{xdg_surface, xdg_toplevel, xdg_wm_base};
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
            match interface.as_str() {
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

delegate_noop!(Client: ignore wl_compositor::WlCompositor);
delegate_noop!(Client: ignore wl_surface::WlSurface);
delegate_noop!(Client: ignore wl_shm::WlShm);
delegate_noop!(Client: ignore wl_shm_pool::WlShmPool);
delegate_noop!(Client: ignore wl_buffer::WlBuffer);
delegate_noop!(Client: ignore wl_output::WlOutput);
delegate_noop!(Client: ignore zwlr_layer_shell_v1::ZwlrLayerShellV1);

