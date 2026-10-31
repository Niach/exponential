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
    backend::{
        allocator::Fourcc,
        renderer::{damage::OutputDamageTracker, pixman::PixmanRenderer, Bind, Offscreen},
    },
    output::{Mode, Output, PhysicalProperties, Subpixel},
    reexports::{
        calloop::{
            channel::{self, Event as ChannelEvent},
            timer::{TimeoutAction, Timer},
            EventLoop,
        },
        wayland_server::Display,
    },
    utils::{Size, Transform},
};

use crate::{
    config::Config,
    state::{ClientState, Ede},
};

pub struct Headless {
    pub renderer: PixmanRenderer,
    pub outputs: Vec<Output>,
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

impl Headless {
    /// Adds a virtual output of `size` to the right of the existing ones.
    pub fn add_output(&mut self, state: &mut Ede, name: &str, size: (i32, i32)) -> Output {
        let output = Output::new(
            name.to_string(),
            PhysicalProperties { size: (0, 0).into(), subpixel: Subpixel::Unknown, make: "Exponential".into(), model: "eDE headless".into() },
        );
        let mode = Mode { size: size.into(), refresh: 60_000 };
        let _global = output.create_global::<Ede>(&state.display_handle);
        output.change_current_state(Some(mode), Some(Transform::Normal), None, None);
        output.set_preferred(mode);
        state.add_output(&output);
        self.outputs.push(output.clone());
        output
    }

    /// Unplugs output `index`.
    pub fn remove_output(&mut self, state: &mut Ede, index: usize) {
        let output = self.outputs.remove(index);
        state.remove_output(&output);
    }

    /// A full repaint of output `index`.
    pub fn capture(&mut self, state: &Ede, index: usize) -> Result<Frame, String> {
        let output = self.outputs.get(index).ok_or("no such output")?.clone();
        let mode = output.current_mode().ok_or("output has no mode")?;
        let size: Size<i32, smithay::utils::Buffer> = (mode.size.w, mode.size.h).into();
        let mut image = Offscreen::<smithay::reexports::pixman::Image<'static, 'static>>::create_buffer(&mut self.renderer, Fourcc::Argb8888, size)
            .map_err(|e| e.to_string())?;
        {
            let mut fb = self.renderer.bind(&mut image).map_err(|e| e.to_string())?;
            let mut damage_tracker = OutputDamageTracker::from_output(&output);
            super::render(state, &output, &mut self.renderer, &mut fb, 0, &mut damage_tracker).map_err(|e| format!("{e:?}"))?;
        }
        let (w, h) = (mode.size.w, mode.size.h);
        let stride = image.stride() / 4;
        // Safety: the image is ours, `h` rows of `stride` u32s, and no
        // renderer holds it any more (the framebuffer above is dropped).
        let data = unsafe { std::slice::from_raw_parts(image.data(), stride * h as usize) };
        let pixels = data.chunks(stride).flat_map(|row| row[..w as usize].iter().copied()).collect();
        Ok(Frame { width: w, height: h, pixels })
    }
}

type Command = Box<dyn FnOnce(&mut Ede, &mut Headless) + Send>;

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
                let mut headless = Headless { renderer: PixmanRenderer::new().expect("pixman"), outputs: Vec::new() };
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
                event_loop
                    .handle()
                    .insert_source(Timer::from_duration(Duration::from_millis(16)), move |_, _, state| {
                        for output in h.borrow().outputs.clone() {
                            super::frame_done(state, &output);
                        }
                        TimeoutAction::ToDuration(Duration::from_millis(16))
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

    pub fn capture(&self, output: usize) -> Frame {
        self.with_backend(move |state, headless| headless.capture(state, output)).expect("capture")
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
