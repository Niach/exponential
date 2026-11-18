//! Headless: virtual outputs drawn in software (pixman) into memory. No GPU,
//! no display server, so CI can run a real compositor, connect real Wayland
//! clients and look at the pixels.

use std::{
    os::unix::net::UnixStream,
    sync::{mpsc, Arc},
    thread::JoinHandle,
    time::Duration,
};

use smithay::{
    backend::renderer::pixman::PixmanRenderer,
    output::{Mode, Output, PhysicalProperties, Subpixel},
    reexports::{
        calloop::{
            channel::{self, Event as ChannelEvent},
            timer::{TimeoutAction, Timer},
            EventLoop,
        },
        wayland_server::Display,
    },
    utils::Transform,
};

use crate::{
    config::Config,
    state::{ClientState, Ede},
};

pub struct Headless {
    pub renderer: PixmanRenderer,
    pub outputs: Vec<Output>,
    /// The `wl_output` global of each output, in the same order.
    pub globals: Vec<smithay::reexports::wayland_server::backend::GlobalId>,
}

/// One rendered output: `pixels` are `0xAARRGGBB`, row-major.
#[derive(Debug, Clone)]
pub struct Frame {
    pub width: i32,
    pub height: i32,
    pub pixels: Vec<u32>,
}

impl Frame {
    pub fn at(&self, x: i32, y: i32) -> u32 {
        self.pixels[(y * self.width + x) as usize]
    }
}

/// The extra modes a headless output offers besides its own size.
pub const VIRTUAL_MODES: [(i32, i32); 3] = [(1024, 768), (1600, 900), (1920, 1080)];

impl Headless {
    /// Adds a virtual output of `size` to the right of the existing ones.
    pub fn add_output(&mut self, state: &mut Ede, name: &str, size: (i32, i32)) -> Output {
        let output = Output::new(
            name.to_string(),
            PhysicalProperties { size: (0, 0).into(), subpixel: Subpixel::Unknown, make: "Exponential".into(), model: "Desktop (headless)".into(), serial_number: "Unknown".into() },
        );
        let mode = Mode { size: size.into(), refresh: 60_000 };
        let global = state.person_output_global(&output);
        output.change_current_state(Some(mode), Some(Transform::Normal), None, None);
        output.set_preferred(mode);
        // Like a VM's virtual GPU, a few more sizes to switch to (#EDE-65:
        // a VirtualBox window resize is a mode change under the shell).
        for (w, h) in VIRTUAL_MODES {
            if (w, h) != size {
                output.add_mode(Mode { size: (w, h).into(), refresh: 60_000 });
            }
        }
        state.add_output(&output);
        self.outputs.push(output.clone());
        self.globals.push(global);
        output
    }

    /// Unplugs output `index`.
    pub fn remove_output(&mut self, state: &mut Ede, index: usize) {
        let output = self.outputs.remove(index);
        state.remove_output(&output);
        // Clients see it go, as they do when a monitor is unplugged.
        state.display_handle.remove_global::<Ede>(self.globals.remove(index));
    }

    /// A full repaint of output `index`, as the monitor shows it.
    pub fn capture(&mut self, state: &mut Ede, index: usize) -> Result<Frame, String> {
        self.draw(state, index, false)
    }

    /// Output `index` as a screen capture sees it: protected windows black,
    /// a locked session all black.
    pub fn capture_shared(&mut self, state: &mut Ede, index: usize) -> Result<Frame, String> {
        self.draw(state, index, true)
    }

    fn draw(&mut self, state: &mut Ede, index: usize, shared: bool) -> Result<Frame, String> {
        let output = self.outputs.get(index).ok_or("no such output")?.clone();
        super::draw_output(&mut self.renderer, state, &output, shared)
    }
}

/// The headless frame clock: every tick ends a frame on each output; it
/// counts as presented when the output's scene changed since the last tick
/// (#EDE-100: a `since` capture of an idle output is unchanged).
pub struct FrameClock {
    renderer: PixmanRenderer,
    trackers: std::collections::HashMap<String, smithay::backend::renderer::damage::OutputDamageTracker>,
}

impl FrameClock {
    /// How often it ticks (60 Hz).
    pub const TICK: Duration = Duration::from_millis(16);

    pub fn new() -> Result<Self, String> {
        Ok(Self { renderer: PixmanRenderer::new().map_err(|e| e.to_string())?, trackers: Default::default() })
    }

    pub fn tick(&mut self, state: &mut Ede, outputs: &[Output]) {
        self.trackers.retain(|name, _| outputs.iter().any(|o| o.name() == *name));
        for output in outputs {
            let tracker = self.trackers.entry(output.name()).or_insert_with(|| smithay::backend::renderer::damage::OutputDamageTracker::from_output(output));
            let elements = super::output_elements(state, output, &mut self.renderer);
            let presented = matches!(tracker.damage_output(1, &elements), Ok((Some(damage), _)) if !damage.is_empty());
            super::frame_done(state, output, presented);
        }
    }
}

type Command = Box<dyn FnOnce(&mut Ede, &mut Headless) + Send>;

/// A desk no compositor ever has: what [`Harness::connect_foreign`] is on.
pub const FOREIGN_DESK: crate::desk::DeskId = crate::desk::DeskId::numbered(u32::MAX);

/// A headless compositor on its own thread. Tests talk Wayland to it over
/// [`Harness::connect`] and inspect it through [`Harness::with`].
pub struct Harness {
    tx: channel::Sender<Command>,
    thread: Option<JoinHandle<()>>,
}

impl Harness {
    /// Starts a compositor with one virtual output per entry of `outputs`.
    pub fn start(config: Config, outputs: &[(i32, i32)]) -> Self {
        let (tx, rx) = channel::channel::<Command>();
        let (ready_tx, ready_rx) = mpsc::channel();
        let outputs = outputs.to_vec();
        let thread = std::thread::Builder::new()
            .name("ede-headless".into())
            .spawn(move || {
                let mut event_loop: EventLoop<'static, Ede> = EventLoop::try_new().expect("event loop");
                let display: Display<Ede> = Display::new().expect("display");
                let mut state = Ede::new(display, event_loop.handle(), event_loop.get_signal(), config);
                let mut headless = Headless { renderer: PixmanRenderer::new().expect("pixman"), outputs: Vec::new(), globals: Vec::new() };
                for (i, size) in outputs.into_iter().enumerate() {
                    headless.add_output(&mut state, &format!("HEADLESS-{}", i + 1), size);
                }
                let headless = std::rc::Rc::new(std::cell::RefCell::new(headless));

                let h = headless.clone();
                event_loop
                    .handle()
                    .insert_source(rx, move |event, _, state| match event {
                        ChannelEvent::Msg(cmd) => cmd(state, &mut h.borrow_mut()),
                        ChannelEvent::Closed => state.loop_signal.stop(),
                    })
                    .expect("command channel");

                // 60 Hz frame clock: without frame callbacks clients stall.
                let h = headless.clone();
                let mut clock = FrameClock::new().expect("pixman");
                event_loop
                    .handle()
                    .insert_source(Timer::from_duration(FrameClock::TICK), move |_, _, state| {
                        let outputs = h.borrow().outputs.clone();
                        clock.tick(state, &outputs);
                        TimeoutAction::ToDuration(FrameClock::TICK)
                    })
                    .expect("frame timer");

                ready_tx.send(()).ok();
                event_loop
                    .run(None, &mut state, |state| {
                        let _ = state.display_handle.flush_clients();
                    })
                    .expect("event loop");
            })
            .expect("spawn the compositor thread");
        ready_rx.recv().expect("compositor thread started");
        Self { tx, thread: Some(thread) }
    }

    /// Runs `f` on the compositor thread and returns its result.
    pub fn with<R: Send + 'static>(&self, f: impl FnOnce(&mut Ede) -> R + Send + 'static) -> R {
        self.with_backend(move |state, _| f(state))
    }

    pub fn with_backend<R: Send + 'static>(&self, f: impl FnOnce(&mut Ede, &mut Headless) -> R + Send + 'static) -> R {
        let (tx, rx) = mpsc::channel();
        self.tx
            .send(Box::new(move |state, headless| {
                tx.send(f(state, headless)).ok();
            }))
            .expect("compositor thread gone");
        rx.recv().expect("compositor thread dropped the command")
    }

    /// A new in-process client connection.
    pub fn connect(&self) -> UnixStream {
        let (ours, theirs) = UnixStream::pair().expect("socketpair");
        self.with(move |state| {
            state.display_handle.insert_client(theirs, Arc::new(ClientState::default())).expect("insert client");
        });
        ours
    }

    /// A new in-process client connection that belongs to [`FOREIGN_DESK`],
    /// not to the person: it sees none of the person's globals (#EDE-125).
    pub fn connect_foreign(&self) -> UnixStream {
        let (ours, theirs) = UnixStream::pair().expect("socketpair");
        self.with(move |state| {
            let client = ClientState { desk: FOREIGN_DESK, ..Default::default() };
            state.display_handle.insert_client(theirs, Arc::new(client)).expect("insert client");
        });
        ours
    }

    /// An `ede-protocol` connection with the root grant (every capability).
    pub fn ipc(&self) -> ede_protocol::Client {
        let (ours, theirs) = UnixStream::pair().expect("socketpair");
        let token = self.with(move |state| {
            state.ipc_insert(theirs);
            state.ipc.root_grant().token
        });
        ede_protocol::Client::over(ours, token).expect("ede-protocol hello")
    }

    /// An `ede-protocol` connection with `token`.
    pub fn ipc_with(&self, token: ede_protocol::Token) -> Result<ede_protocol::Client, ede_protocol::Error> {
        let (ours, theirs) = UnixStream::pair().expect("socketpair");
        self.with(move |state| state.ipc_insert(theirs));
        ede_protocol::Client::over(ours, token)
    }

    pub fn capture(&self, output: usize) -> Frame {
        self.with_backend(move |state, headless| headless.capture(state, output)).expect("capture")
    }

    pub fn capture_shared(&self, output: usize) -> Frame {
        self.with_backend(move |state, headless| headless.capture_shared(state, output)).expect("capture")
    }
}

impl Drop for Harness {
    fn drop(&mut self) {
        let _ = self.tx.send(Box::new(|state, _| state.loop_signal.stop()));
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}
