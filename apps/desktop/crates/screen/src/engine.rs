//! The capture loop (its own thread): every `capture.intervalMs` every
//! display, every `capture.focusPollMs` a look at the focused window (and
//! its display at once when the app or title changed), nothing while
//! paused; each frame hashed and deduplicated per display, a kept one
//! encoded to WebP and stored. The retention sweep runs every tick.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use computer::FocusedWindow;

use crate::capture::{Capturer, Frame};
use crate::store::{self, Rect, Row, Store};
use crate::{fixture, hash, ring, Status};

/// The focused window now (`computer::focused_window`; a fake in tests).
pub(crate) type FocusSource = Box<dyn FnMut() -> Option<FocusedWindow> + Send>;
/// Hears Active / Paused changes.
pub(crate) type StatusSink = Arc<dyn Fn(Status) + Send + Sync>;

#[derive(Clone, Copy, Debug)]
pub(crate) struct Timing {
    pub interval: Duration,
    pub poll: Duration,
}

impl Timing {
    pub(crate) fn from_fixture() -> Timing {
        Timing {
            interval: Duration::from_millis(fixture::interval_ms()),
            poll: Duration::from_millis(fixture::focus_poll_ms()),
        }
    }
}

pub(crate) fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|since| since.as_millis() as i64)
        .unwrap_or_default()
}

/// One running screen memory: the store the tool reads and the thread
/// that fills it.
pub(crate) struct Memory {
    pub data_dir: PathBuf,
    pub store: Arc<Mutex<Store>>,
    stop: mpsc::Sender<()>,
    thread: Option<JoinHandle<()>>,
}

impl Memory {
    /// Open the store (sweep + orphans first) and start the loop.
    pub(crate) fn start(
        data_dir: &Path,
        capturer: Box<dyn Capturer>,
        focus: FocusSource,
        timing: Timing,
        hotkey: Arc<AtomicBool>,
        status: StatusSink,
    ) -> Result<Memory, String> {
        let store = Store::open(data_dir)?;
        store.sweep(now_ms())?;
        store.remove_orphans()?;
        let store = Arc::new(Mutex::new(store));
        let (stop, stopped) = mpsc::channel();
        let mut looper = Loop {
            capturer,
            focus,
            timing,
            hotkey,
            status,
            store: store.clone(),
            dedupe: ring::Dedupe::default(),
            shown: None,
            last_focus: None,
        };
        let thread = std::thread::Builder::new()
            .name("screen-memory".into())
            .spawn(move || looper.run(stopped))
            .map_err(|err| format!("Could not start screen memory: {err}"))?;
        Ok(Memory { data_dir: data_dir.to_path_buf(), store, stop, thread: Some(thread) })
    }

    /// Stop the loop and wait for it; `forget` deletes every frame and row.
    pub(crate) fn stop(mut self, forget: bool) {
        let _ = self.stop.send(());
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
        let data_dir = self.data_dir.clone();
        drop(self);
        if forget {
            store::forget(&data_dir);
        }
    }
}

struct Loop {
    capturer: Box<dyn Capturer>,
    focus: FocusSource,
    timing: Timing,
    hotkey: Arc<AtomicBool>,
    status: StatusSink,
    store: Arc<Mutex<Store>>,
    dedupe: ring::Dedupe,
    shown: Option<Status>,
    /// The (app, title) of the last poll, for the focus-change capture.
    last_focus: Option<(String, String)>,
}

impl Loop {
    fn run(&mut self, stopped: mpsc::Receiver<()>) {
        let mut next_tick = Instant::now();
        loop {
            let focus = (self.focus)();
            let paused = self.hotkey.load(Ordering::Acquire)
                || focus.as_ref().is_some_and(|window| ring::denied(&window.app, &window.app, &window.title));
            self.show(if paused { Status::Paused } else { Status::Active });
            let now = Instant::now();
            if paused {
                // Nothing, not even the tick; the next tick after the
                // focus leaves runs at once.
                self.last_focus = None;
            } else if now >= next_tick {
                self.tick(focus.as_ref());
                next_tick += self.timing.interval;
                if next_tick <= now {
                    next_tick = now + self.timing.interval;
                }
            } else {
                let key = focus.as_ref().map(|window| (window.app.clone(), window.title.clone()));
                if key.is_some() && key != self.last_focus {
                    let window = focus.as_ref().expect("a focused window");
                    match self.capturer.grab(window.display) {
                        Ok(Some(frame)) => self.keep(frame, focus.as_ref(), now_ms()),
                        Ok(None) => {}
                        Err(reason) => log::debug!("[screen] no focus-change frame: {reason}"),
                    }
                }
            }
            if !paused {
                self.last_focus = focus.as_ref().map(|window| (window.app.clone(), window.title.clone()));
            }
            let wait = self.timing.poll.min(next_tick.saturating_duration_since(Instant::now()));
            match stopped.recv_timeout(wait.max(Duration::from_millis(1))) {
                Err(RecvTimeoutError::Timeout) => {}
                Ok(()) | Err(RecvTimeoutError::Disconnected) => break,
            }
        }
    }

    fn show(&mut self, status: Status) {
        if self.shown != Some(status) {
            self.shown = Some(status);
            (self.status)(status);
        }
    }

    fn tick(&mut self, focus: Option<&FocusedWindow>) {
        match self.capturer.grab_all() {
            Ok(frames) => {
                let at = now_ms();
                for frame in frames {
                    self.keep(frame, focus, at);
                }
            }
            Err(reason) => log::debug!("[screen] no frames this tick: {reason}"),
        }
        if let Err(reason) = self.store.lock().unwrap().sweep(now_ms()) {
            log::warn!("[screen] {reason}");
        }
    }

    /// Hash, dedupe, and store a kept frame.
    fn keep(&mut self, frame: Frame, focus: Option<&FocusedWindow>, at: i64) {
        if frame.width == 0 || frame.height == 0 || frame.rgba.len() < frame.width as usize * frame.height as usize * 4 {
            return;
        }
        let (app, title) = focus.map(|window| (window.app.as_str(), window.title.as_str())).unwrap_or(("", ""));
        let hash = hash::hash(frame.width, frame.height, &frame.rgba);
        if !self.dedupe.keep(frame.display, app, title, hash) {
            return;
        }
        let focus_rect = focus.filter(|window| window.display == frame.display).and_then(|window| clip(window.rect, frame.width, frame.height));
        let webp = encode(&frame);
        if webp.is_empty() {
            return;
        }
        let row = Row {
            at,
            display: frame.display,
            app: app.to_string(),
            title: title.to_string(),
            hash: hash::hex(hash),
            path: store::relative_path(at, frame.display),
            width: frame.width,
            height: frame.height,
            focus: focus_rect,
        };
        if let Err(reason) = self.store.lock().unwrap().insert(&row, &webp) {
            log::warn!("[screen] {reason}");
        }
    }
}

/// The focused rect within a `width` x `height` frame; `None` when nothing
/// of it is on the frame.
fn clip(rect: computer::ScreenRect, width: u32, height: u32) -> Option<Rect> {
    let left = i64::from(rect.x).clamp(0, i64::from(width));
    let top = i64::from(rect.y).clamp(0, i64::from(height));
    let right = (i64::from(rect.x) + i64::from(rect.width)).clamp(0, i64::from(width));
    let bottom = (i64::from(rect.y) + i64::from(rect.height)).clamp(0, i64::from(height));
    (right > left && bottom > top).then(|| Rect { x: left as i32, y: top as i32, width: (right - left) as u32, height: (bottom - top) as u32 })
}

/// Lossy WebP at `store.webpQuality`, full resolution. `method` 0 of 0-6
/// (the default is 4): a kept frame is the loop's one real cost, and 0
/// encodes a real 3456x2234 screen in ~77 ms against ~130 ms at method 2,
/// for ~13 % larger files, which ten minutes of retention can afford.
pub(crate) fn encode(frame: &Frame) -> Vec<u8> {
    let encoder = webp::Encoder::from_rgba(&frame.rgba, frame.width, frame.height);
    let mut config = webp::WebPConfig::new().expect("libwebp config");
    config.lossless = 0;
    config.quality = fixture::webp_quality();
    config.method = 0;
    match encoder.encode_advanced(&config) {
        Ok(memory) => memory.to_vec(),
        Err(err) => {
            log::warn!("[screen] WebP encoding failed: {err:?}");
            Vec::new()
        }
    }
}
