//! #EDE-140: a minimal Wayland client for the live eDE test
//! (`headless_ede_drives_and_captures`): xdg toplevels filled with one
//! colour from a wl_shm buffer, and what its keyboard is sent.

use std::io::Write;
use std::os::fd::AsFd;
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};

use wayland_client::protocol::{wl_buffer, wl_compositor, wl_keyboard, wl_registry, wl_seat, wl_shm, wl_shm_pool, wl_surface};
use wayland_client::{delegate_noop, Connection, Dispatch, EventQueue, QueueHandle, WEnum};
use wayland_protocols::xdg::shell::client::{xdg_surface, xdg_toplevel, xdg_wm_base};

#[derive(Default)]
pub(super) struct Heard {
    compositor: Option<wl_compositor::WlCompositor>,
    shm: Option<wl_shm::WlShm>,
    wm_base: Option<xdg_wm_base::XdgWmBase>,
    seat: Option<wl_seat::WlSeat>,
    /// The last configure not acked yet.
    configure: Option<u32>,
    /// The surface the keyboard is on and every key pressed into the
    /// client (evdev codes).
    pub(super) focus: Option<wl_surface::WlSurface>,
    pub(super) keys: Vec<u32>,
}

pub(super) struct App {
    queue: EventQueue<Heard>,
    qh: QueueHandle<Heard>,
    pub(super) heard: Heard,
    /// Where the buffers' files are made (unlinked at once).
    scratch: PathBuf,
    buffers: u32,
    _keyboard: wl_keyboard::WlKeyboard,
    _conn: Connection,
}

pub(super) struct Window {
    pub(super) surface: wl_surface::WlSurface,
    xdg_surface: xdg_surface::XdgSurface,
    toplevel: xdg_toplevel::XdgToplevel,
}

impl App {
    /// A client of the compositor listening on `socket`, with a keyboard.
    pub(super) fn connect(socket: &Path) -> App {
        let conn = Connection::from_socket(UnixStream::connect(socket).expect("the compositor's Wayland socket")).expect("a Wayland connection");
        let mut queue = conn.new_event_queue();
        let qh = queue.handle();
        conn.display().get_registry(&qh, ());
        let mut heard = Heard::default();
        // The globals, then the seat's capabilities.
        for _ in 0..2 {
            queue.roundtrip(&mut heard).expect("roundtrip");
        }
        let keyboard = heard.seat.as_ref().expect("a seat").get_keyboard(&qh, ());
        let scratch = socket.parent().expect("a runtime directory").to_path_buf();
        let mut app = App { queue, qh, heard, scratch, buffers: 0, _keyboard: keyboard, _conn: conn };
        app.roundtrip();
        app
    }

    pub(super) fn roundtrip(&mut self) {
        self.queue.roundtrip(&mut self.heard).expect("roundtrip");
    }

    /// A mapped `width`×`height` toplevel of one colour (`argb`).
    pub(super) fn window(&mut self, title: &str, app_id: &str, (width, height): (i32, i32), argb: u32) -> Window {
        let surface = self.heard.compositor.as_ref().expect("wl_compositor").create_surface(&self.qh, ());
        let xdg_surface = self.heard.wm_base.as_ref().expect("xdg_wm_base").get_xdg_surface(&surface, &self.qh, ());
        let toplevel = xdg_surface.get_toplevel(&self.qh, ());
        toplevel.set_title(title.into());
        toplevel.set_app_id(app_id.into());
        surface.commit();
        self.roundtrip();
        xdg_surface.ack_configure(self.heard.configure.take().expect("a first configure"));

        self.buffers += 1;
        let path = self.scratch.join(format!("epilot-test-buffer-{}", self.buffers));
        let mut file = std::fs::File::options().read(true).write(true).create_new(true).open(&path).expect("a buffer file");
        let _ = std::fs::remove_file(&path);
        file.write_all(&argb.to_le_bytes().repeat((width * height) as usize)).expect("the buffer's pixels");
        let pool = self.heard.shm.as_ref().expect("wl_shm").create_pool(file.as_fd(), width * height * 4, &self.qh, ());
        let buffer = pool.create_buffer(0, width, height, width * 4, wl_shm::Format::Argb8888, &self.qh, ());
        pool.destroy();
        surface.attach(Some(&buffer), 0, 0);
        surface.damage(0, 0, width, height);
        surface.commit();
        self.roundtrip();
        Window { surface, xdg_surface, toplevel }
    }

    pub(super) fn close(&mut self, window: Window) {
        window.toplevel.destroy();
        window.xdg_surface.destroy();
        window.surface.destroy();
        self.roundtrip();
    }

    /// Is the keyboard on `window`?
    pub(super) fn focused(&mut self, window: &Window) -> bool {
        self.roundtrip();
        self.heard.focus.as_ref() == Some(&window.surface)
    }

    /// The keys pressed into the client so far.
    pub(super) fn keys(&mut self) -> Vec<u32> {
        self.roundtrip();
        self.heard.keys.clone()
    }
}

impl Dispatch<wl_registry::WlRegistry, ()> for Heard {
    fn event(heard: &mut Self, registry: &wl_registry::WlRegistry, event: wl_registry::Event, _: &(), _: &Connection, qh: &QueueHandle<Self>) {
        let wl_registry::Event::Global { name, interface, version } = event else { return };
        match interface.as_str() {
            "wl_compositor" => heard.compositor = Some(registry.bind(name, version.min(4), qh, ())),
            "wl_shm" => heard.shm = Some(registry.bind(name, 1, qh, ())),
            "xdg_wm_base" => heard.wm_base = Some(registry.bind(name, version.min(3), qh, ())),
            "wl_seat" => heard.seat = Some(registry.bind(name, version.min(5), qh, ())),
            _ => {}
        }
    }
}

impl Dispatch<xdg_wm_base::XdgWmBase, ()> for Heard {
    fn event(_: &mut Self, wm_base: &xdg_wm_base::XdgWmBase, event: xdg_wm_base::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {
        if let xdg_wm_base::Event::Ping { serial } = event {
            wm_base.pong(serial);
        }
    }
}

impl Dispatch<xdg_surface::XdgSurface, ()> for Heard {
    fn event(heard: &mut Self, _: &xdg_surface::XdgSurface, event: xdg_surface::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {
        if let xdg_surface::Event::Configure { serial } = event {
            heard.configure = Some(serial);
        }
    }
}

impl Dispatch<wl_keyboard::WlKeyboard, ()> for Heard {
    fn event(heard: &mut Self, _: &wl_keyboard::WlKeyboard, event: wl_keyboard::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {
        match event {
            wl_keyboard::Event::Enter { surface, .. } => heard.focus = Some(surface),
            wl_keyboard::Event::Leave { .. } => heard.focus = None,
            wl_keyboard::Event::Key { key, state: WEnum::Value(wl_keyboard::KeyState::Pressed), .. } => heard.keys.push(key),
            _ => {}
        }
    }
}

delegate_noop!(Heard: ignore wl_compositor::WlCompositor);
delegate_noop!(Heard: ignore wl_surface::WlSurface);
delegate_noop!(Heard: ignore wl_shm::WlShm);
delegate_noop!(Heard: ignore wl_shm_pool::WlShmPool);
delegate_noop!(Heard: ignore wl_buffer::WlBuffer);
delegate_noop!(Heard: ignore wl_seat::WlSeat);
delegate_noop!(Heard: ignore xdg_toplevel::XdgToplevel);
