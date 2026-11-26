//! Headless: screen copy for the person's own tools (`src/screencopy.rs`).
//! grim's `zwlr_screencopy_manager_v1` (a whole output, a region at a
//! fractional scale) and `ext-image-copy-capture-v1` copy what the
//! redacting capture draws, pixel for pixel; a buffer of the wrong size
//! fails; a space's client, another desk's and a sandboxed one never see
//! the globals.

mod common;

use std::{
    io::Write,
    os::{fd::AsFd, unix::fs::FileExt, unix::net::UnixStream},
    sync::Arc,
    time::{Duration, Instant},
};

use common::Session;
use ede_compositor::{
    backend::headless::{Frame, Harness},
    peer::Peer,
    state::ClientState,
    Config,
};
use wayland_client::{
    delegate_noop,
    protocol::{wl_buffer, wl_output, wl_registry, wl_shm, wl_shm_pool},
    Connection, Dispatch, EventQueue, QueueHandle, WEnum,
};
use wayland_protocols::ext::{
    image_capture_source::v1::client::{ext_image_capture_source_v1, ext_output_image_capture_source_manager_v1},
    image_copy_capture::v1::client::{ext_image_copy_capture_frame_v1, ext_image_copy_capture_manager_v1, ext_image_copy_capture_session_v1},
};
use wayland_protocols_wlr::screencopy::v1::client::{zwlr_screencopy_frame_v1, zwlr_screencopy_manager_v1};

const BLUE: u32 = 0xff00_00ff;
const RED: u32 = 0xffff_0000;
const WINDOW: (i32, i32) = (300, 200);
const BUTTON: (i32, i32, i32, i32) = (200, 140, 80, 40);

const GLOBALS: [&str; 3] = ["ext_output_image_capture_source_manager_v1", "ext_image_copy_capture_manager_v1", "zwlr_screencopy_manager_v1"];

fn harness(scale: f64, mode: (i32, i32)) -> Harness {
    let _ = tracing_subscriber::fmt().with_test_writer().with_env_filter("warn").try_init();
    Harness::start(Config::parse(&format!("[[outputs]]\nname = \"HEADLESS-1\"\nscale = {scale}\n")).unwrap(), &[mode])
}

/// A window with a red button on the output, so a copy has something to
/// get wrong.
fn app(h: &Harness) -> Session {
    let mut app = Session::connect(h.connect());
    let t = app.toplevel("button");
    app.draw_with_rect(&t, WINDOW, BLUE, BUTTON, RED);
    let deadline = Instant::now() + Duration::from_secs(5);
    while h.with(|e| e.desk.space.elements().count()) != 1 {
        assert!(Instant::now() < deadline, "the window never mapped");
        app.roundtrip();
        std::thread::sleep(Duration::from_millis(10));
    }
    app
}

/// What a screen copy tool hears.
#[derive(Default)]
struct Tool {
    globals: Vec<String>,
    shm: Option<wl_shm::WlShm>,
    output: Option<wl_output::WlOutput>,
    wlr: Option<zwlr_screencopy_manager_v1::ZwlrScreencopyManagerV1>,
    sources: Option<ext_output_image_capture_source_manager_v1::ExtOutputImageCaptureSourceManagerV1>,
    copies: Option<ext_image_copy_capture_manager_v1::ExtImageCopyCaptureManagerV1>,
    /// wlr: `buffer` (format, w, h, stride), `buffer_done`, `flags`,
    /// `damage`, `ready`, `failed`.
    wlr_buffer: Option<(u32, u32, u32, u32)>,
    wlr_buffer_done: bool,
    wlr_flags: Option<u32>,
    wlr_damage: Vec<(u32, u32, u32, u32)>,
    wlr_ready: bool,
    wlr_failed: bool,
    /// ext session: `buffer_size`, `shm_format`s, `done`, `stopped`.
    size: Option<(u32, u32)>,
    formats: Vec<u32>,
    done: bool,
    stopped: bool,
    /// ext frame: `transform`, `damage`, `ready`, `failed`.
    transform: Option<u32>,
    damage: Vec<(i32, i32, i32, i32)>,
    ready: bool,
    failed: Option<u32>,
}

struct Client {
    queue: EventQueue<Tool>,
    qh: QueueHandle<Tool>,
    tool: Tool,
}

impl Client {
    fn connect(stream: UnixStream) -> Self {
        let conn = Connection::from_socket(stream).expect("connect");
        let queue = conn.new_event_queue();
        let qh = queue.handle();
        conn.display().get_registry(&qh, ());
        let mut c = Self { queue, qh, tool: Tool::default() };
        c.roundtrip();
        c.roundtrip();
        c
    }

    fn roundtrip(&mut self) {
        self.queue.roundtrip(&mut self.tool).expect("roundtrip");
    }

    fn until(&mut self, what: &str, done: impl Fn(&Tool) -> bool) {
        let deadline = Instant::now() + Duration::from_secs(5);
        while !done(&self.tool) {
            assert!(Instant::now() < deadline, "timed out waiting for {what}");
            self.roundtrip();
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    /// A wl_shm buffer of `w`x`h` with `stride`, zeroed, and its file to
    /// read the copy back from.
    fn buffer(&mut self, w: i32, h: i32, stride: i32, format: wl_shm::Format) -> (wl_buffer::WlBuffer, std::fs::File) {
        let mut file = tempfile::tempfile().expect("shm file");
        file.write_all(&vec![0u8; (stride * h) as usize]).unwrap();
        let pool = self.tool.shm.as_ref().unwrap().create_pool(file.as_fd(), stride * h, &self.qh, ());
        let buffer = pool.create_buffer(0, w, h, stride, format, &self.qh, ());
        pool.destroy();
        (buffer, file)
    }
}

/// The pixels a buffer of `w`x`h` holds, as `0xAARRGGBB` words.
fn read_back(file: &std::fs::File, w: i32, h: i32, stride: i32) -> Frame {
    let mut bytes = vec![0u8; (stride * h) as usize];
    file.read_exact_at(&mut bytes, 0).unwrap();
    let pixels = (0..h).flat_map(|y| (0..w).map(move |x| (x, y))).map(|(x, y)| {
        let at = (y * stride + x * 4) as usize;
        u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap())
    });
    Frame { width: w, height: h, pixels: pixels.collect() }
}

/// `frame`'s pixels inside (x, y, w, h), ignoring the alpha byte (an
/// Xrgb8888 copy's alpha is not the picture).
fn crop_rgb(frame: &Frame, (x, y, w, h): (i32, i32, i32, i32)) -> Vec<u32> {
    (y..y + h).flat_map(|yy| (x..x + w).map(move |xx| (xx, yy))).map(|(xx, yy)| frame.at(xx, yy) & 0x00ff_ffff).collect()
}

fn rgb(frame: &Frame) -> Vec<u32> {
    frame.pixels.iter().map(|p| p & 0x00ff_ffff).collect()
}

/// wlr: a copy of the whole output is the redacting capture, pixel for pixel.
#[test]
fn grim_copies_the_whole_output() {
    let h = harness(1.0, (800, 600));
    let _app = app(&h);
    let mut grim = Client::connect(h.connect());
    for global in GLOBALS {
        assert!(grim.tool.globals.iter().any(|g| g == global), "the person's client sees {global}");
    }
    let output = grim.tool.output.clone().unwrap();
    let frame = grim.tool.wlr.as_ref().unwrap().capture_output(0, &output, &grim.qh, ());
    grim.until("buffer_done", |t| t.wlr_buffer_done);
    let (format, w, hh, stride) = grim.tool.wlr_buffer.unwrap();
    assert_eq!((format, w, hh, stride), (u32::from(wl_shm::Format::Xrgb8888), 800, 600, 3200));
    let (buffer, file) = grim.buffer(800, 600, 3200, wl_shm::Format::Xrgb8888);
    frame.copy(&buffer);
    grim.until("ready", |t| t.wlr_ready || t.wlr_failed);
    assert!(grim.tool.wlr_ready && !grim.tool.wlr_failed);
    assert_eq!(grim.tool.wlr_flags, Some(0));
    let copied = read_back(&file, 800, 600, 3200);
    let shown = h.capture_shared(0);
    assert_eq!(rgb(&copied), rgb(&shown), "the copy is the capture");
    assert!(copied.pixels.iter().any(|p| p & 0x00ff_ffff == RED & 0x00ff_ffff), "the button is in it");

    // One frame, one copy: a second is a protocol error.
    let (again, _file) = grim.buffer(800, 600, 3200, wl_shm::Format::Xrgb8888);
    frame.copy(&again);
    assert!(grim.queue.roundtrip(&mut grim.tool).is_err(), "already_used");
}

/// wlr: a region in logical coordinates on a 1.25 and a 2x output is its
/// pixels of the output; `copy_with_damage` carries full damage. A
/// region partly off the output is clipped to it.
#[test]
fn grim_copies_a_region_at_a_fractional_scale() {
    for (scale, mode, region, pixels) in [
        (1.25, (1000, 750), (40, 32, 400, 300), (50, 40, 500, 375)),
        (2.0, (1200, 900), (100, 50, 200, 150), (200, 100, 400, 300)),
        (2.0, (1200, 900), (500, 400, 400, 400), (1000, 800, 200, 100)),
    ] {
        let h = harness(scale, mode);
        let _app = app(&h);
        let mut grim = Client::connect(h.connect());
        let output = grim.tool.output.clone().unwrap();
        let (x, y, w, hh) = region;
        let frame = grim.tool.wlr.as_ref().unwrap().capture_output_region(0, &output, x, y, w, hh, &grim.qh, ());
        grim.until("buffer_done", |t| t.wlr_buffer_done);
        let (_, bw, bh, stride) = grim.tool.wlr_buffer.unwrap();
        assert_eq!((bw as i32, bh as i32), (pixels.2, pixels.3), "scale {scale}: region {region:?}");
        // An Argb8888 buffer with a padded stride is taken too.
        let stride = stride as i32 + 64;
        let (buffer, file) = grim.buffer(pixels.2, pixels.3, stride, wl_shm::Format::Argb8888);
        frame.copy_with_damage(&buffer);
        grim.until("ready", |t| t.wlr_ready || t.wlr_failed);
        assert!(grim.tool.wlr_ready, "scale {scale}: copied");
        assert_eq!(grim.tool.wlr_damage, [(0, 0, pixels.2 as u32, pixels.3 as u32)]);
        let copied = read_back(&file, pixels.2, pixels.3, stride);
        assert_eq!(rgb(&copied), crop_rgb(&h.capture_shared(0), pixels), "scale {scale}: the region's pixels");
    }
}

/// wlr: a buffer that is not the announced size fails the frame (the
/// connection stays).
#[test]
fn grim_with_a_wrong_buffer_gets_failed() {
    let h = harness(1.0, (800, 600));
    let mut grim = Client::connect(h.connect());
    let output = grim.tool.output.clone().unwrap();
    let frame = grim.tool.wlr.as_ref().unwrap().capture_output(0, &output, &grim.qh, ());
    grim.until("buffer_done", |t| t.wlr_buffer_done);
    let (buffer, _file) = grim.buffer(400, 300, 1600, wl_shm::Format::Xrgb8888);
    frame.copy(&buffer);
    grim.until("failed", |t| t.wlr_ready || t.wlr_failed);
    assert!(grim.tool.wlr_failed && !grim.tool.wlr_ready);
    grim.roundtrip();
}

/// ext-image-copy-capture: an output source's session announces the mode
/// and the shm formats; a frame is the capture, fully damaged.
#[test]
fn an_ext_session_copies_one_frame() {
    let h = harness(1.5, (1200, 900));
    let _app = app(&h);
    let mut tool = Client::connect(h.connect());
    let output = tool.tool.output.clone().unwrap();
    let source = tool.tool.sources.as_ref().unwrap().create_source(&output, &tool.qh, ());
    let session = tool.tool.copies.as_ref().unwrap().create_session(&source, ext_image_copy_capture_manager_v1::Options::empty(), &tool.qh, ());
    tool.until("done", |t| t.done || t.stopped);
    assert!(!tool.tool.stopped);
    assert_eq!(tool.tool.size, Some((1200, 900)));
    for format in [wl_shm::Format::Argb8888, wl_shm::Format::Xrgb8888] {
        assert!(tool.tool.formats.contains(&u32::from(format)), "{format:?}");
    }

    let (buffer, file) = tool.buffer(1200, 900, 1200 * 4, wl_shm::Format::Argb8888);
    let frame = session.create_frame(&tool.qh, ());
    frame.attach_buffer(&buffer);
    frame.damage_buffer(0, 0, 1200, 900);
    frame.capture();
    tool.until("ready", |t| t.ready || t.failed.is_some());
    assert!(tool.tool.ready, "failed: {:?}", tool.tool.failed);
    assert_eq!(tool.tool.transform, Some(0), "normal");
    assert_eq!(tool.tool.damage, [(0, 0, 1200, 900)]);
    let copied = read_back(&file, 1200, 900, 1200 * 4);
    assert_eq!(copied.pixels, h.capture_shared(0).pixels, "the copy is the capture, alpha included");
    frame.destroy();

    // A second frame on the session with a buffer too small: failed with
    // buffer_constraints.
    let (small, _file) = tool.buffer(600, 450, 600 * 4, wl_shm::Format::Argb8888);
    let frame = session.create_frame(&tool.qh, ());
    tool.tool.ready = false;
    frame.attach_buffer(&small);
    frame.capture();
    tool.until("failed", |t| t.ready || t.failed.is_some());
    assert_eq!(tool.tool.failed, Some(u32::from(ext_image_copy_capture_frame_v1::FailureReason::BufferConstraints)));
}

/// Who never sees the globals: another desk's client, an agent space's,
/// and a sandboxed one of the person's (another user namespace); one in
/// the compositor's user namespace does.
#[test]
fn sandboxed_and_space_clients_see_no_screen_copy() {
    let (h, _dir) = common::space::start_with_spaces(&[(800, 600)]);
    let sees = |c: &Client| GLOBALS.iter().filter(|g| c.tool.globals.iter().any(|s| s == *g)).count();

    assert_eq!(sees(&Client::connect(h.connect())), 3, "the person's own client");
    assert_eq!(sees(&Client::connect(h.connect_foreign())), 0, "another desk's");

    let socket = h.with(|state| {
        let desk = state.create_space(ede_protocol::SessionId::new("run-1").unwrap(), 640, 480).expect("a space");
        state.spaces.get(desk).unwrap().socket()
    });
    let space = Client::connect(UnixStream::connect(&socket).expect("the space's socket"));
    assert!(!space.tool.globals.is_empty(), "the space's client is connected");
    assert_eq!(sees(&space), 0, "an agent space's");

    let with_peer = |same_userns: bool| {
        let (ours, theirs) = UnixStream::pair().unwrap();
        h.with(move |state| {
            let peer = Peer { pid: std::process::id(), uid: 0, pidfd: None, start: 0, exe: None, same_userns, parent: None };
            let client = ClientState { peer: Some(Arc::new(peer)), ..Default::default() };
            state.display_handle.insert_client(theirs, Arc::new(client)).expect("insert client");
        });
        Client::connect(ours)
    };
    assert_eq!(sees(&with_peer(false)), 0, "a Flatpak");
    assert_eq!(sees(&with_peer(true)), 3, "grim from a terminal");
}

impl Dispatch<wl_registry::WlRegistry, ()> for Tool {
    fn event(state: &mut Self, registry: &wl_registry::WlRegistry, event: wl_registry::Event, _: &(), _: &Connection, qh: &QueueHandle<Self>) {
        if let wl_registry::Event::Global { name, interface, version } = event {
            match interface.as_str() {
                "wl_shm" => state.shm = Some(registry.bind(name, 1, qh, ())),
                "wl_output" if state.output.is_none() => state.output = Some(registry.bind(name, version.min(4), qh, ())),
                "zwlr_screencopy_manager_v1" => state.wlr = Some(registry.bind(name, version.min(3), qh, ())),
                "ext_output_image_capture_source_manager_v1" => state.sources = Some(registry.bind(name, 1, qh, ())),
                "ext_image_copy_capture_manager_v1" => state.copies = Some(registry.bind(name, 1, qh, ())),
                _ => {}
            }
            state.globals.push(interface);
        }
    }
}

impl Dispatch<zwlr_screencopy_frame_v1::ZwlrScreencopyFrameV1, ()> for Tool {
    fn event(state: &mut Self, _: &zwlr_screencopy_frame_v1::ZwlrScreencopyFrameV1, event: zwlr_screencopy_frame_v1::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {
        use zwlr_screencopy_frame_v1::Event;
        match event {
            Event::Buffer { format, width, height, stride } => {
                let format = match format {
                    WEnum::Value(f) => u32::from(f),
                    WEnum::Unknown(f) => f,
                };
                state.wlr_buffer = Some((format, width, height, stride));
            }
            Event::BufferDone => state.wlr_buffer_done = true,
            Event::Flags { flags } => state.wlr_flags = Some(flags.into_result().map_or(u32::MAX, |f| f.bits())),
            Event::Damage { x, y, width, height } => state.wlr_damage.push((x, y, width, height)),
            Event::Ready { .. } => state.wlr_ready = true,
            Event::Failed => state.wlr_failed = true,
            _ => {}
        }
    }
}

impl Dispatch<ext_image_copy_capture_session_v1::ExtImageCopyCaptureSessionV1, ()> for Tool {
    fn event(state: &mut Self, _: &ext_image_copy_capture_session_v1::ExtImageCopyCaptureSessionV1, event: ext_image_copy_capture_session_v1::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {
        use ext_image_copy_capture_session_v1::Event;
        match event {
            Event::BufferSize { width, height } => state.size = Some((width, height)),
            Event::ShmFormat { format } => state.formats.push(match format {
                WEnum::Value(f) => u32::from(f),
                WEnum::Unknown(f) => f,
            }),
            Event::Done => state.done = true,
            Event::Stopped => state.stopped = true,
            _ => {}
        }
    }
}

impl Dispatch<ext_image_copy_capture_frame_v1::ExtImageCopyCaptureFrameV1, ()> for Tool {
    fn event(state: &mut Self, _: &ext_image_copy_capture_frame_v1::ExtImageCopyCaptureFrameV1, event: ext_image_copy_capture_frame_v1::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {
        use ext_image_copy_capture_frame_v1::Event;
        match event {
            Event::Transform { transform } => state.transform = Some(match transform {
                WEnum::Value(t) => u32::from(t),
                WEnum::Unknown(t) => t,
            }),
            Event::Damage { x, y, width, height } => state.damage.push((x, y, width, height)),
            Event::Ready => state.ready = true,
            Event::Failed { reason } => state.failed = Some(match reason {
                WEnum::Value(r) => u32::from(r),
                WEnum::Unknown(r) => r,
            }),
            _ => {}
        }
    }
}

delegate_noop!(Tool: ignore wl_output::WlOutput);
delegate_noop!(Tool: ignore wl_shm::WlShm);
delegate_noop!(Tool: ignore wl_buffer::WlBuffer);
delegate_noop!(Tool: wl_shm_pool::WlShmPool);
delegate_noop!(Tool: zwlr_screencopy_manager_v1::ZwlrScreencopyManagerV1);
delegate_noop!(Tool: ext_output_image_capture_source_manager_v1::ExtOutputImageCaptureSourceManagerV1);
delegate_noop!(Tool: ext_image_capture_source_v1::ExtImageCaptureSourceV1);
delegate_noop!(Tool: ext_image_copy_capture_manager_v1::ExtImageCopyCaptureManagerV1);
