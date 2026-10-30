//! The capture loop (its own thread): every `capture.focusPollMs` one look
//! at the screen (`epilot::screen_view`: the focused window, the work
//! displays, each display's top window); every `capture.intervalMs` the
//! work displays (and the focused window's display at once when the app or
//! title changed), nothing while paused (a denylisted or unknown focus, the
//! hotkey); each frame hashed and deduplicated per display. Kept frames go
//! to ONE low-priority encode worker (cut, fit, WebP, store) as a batch;
//! while it still works on the last batch or rests after it (the encode
//! budget) a new one is dropped and the dedupe does not advance, so the
//! next tick retries the change, re-grabbing those displays' latest frame
//! (SKIP IF BUSY). STALE: a display coming back into capture (the pause
//! ends, its top window leaves the denylist, it turns a work display again)
//! takes only frames delivered one interval after that, and once that
//! interval is over a still display is captured FRESH once (the OS stream
//! delivers only changes). LABEL and MASK read the window list nearest the
//! frame's capture time (a short ring of readings), so a frame the OS
//! captured before a focus change is labelled (and masked) as it was then.
//! RACE (#FEED-164): a grab with frames reads the window list once more
//! right after it (the newest reading, MASK's), so a denylisted window that
//! opened between the poll and the grab is masked. DISPLAY SET: a frame is
//! taken only with readings of the display epoch current right after its
//! grab (`epilot::visible::display_epoch`), so after a hot-plug no frame
//! is labelled or masked by the old display numbering.
//! The retention sweep runs every interval, paused or not.

use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use epilot::visible::{ScreenLook, VisibleWindow};
use epilot::{FocusedWindow, ScreenView};

use crate::capture::{Capturer, Frame, Want};
use crate::store::{self, Rect, Row, Store};
use crate::{fixture, hash, ring, shrink, Status};

/// The screen now (`epilot::visible::screen_look`; a fake in tests).
/// `None` = no window list: paused (fail closed).
pub(crate) type ViewSource = Box<dyn FnMut() -> Option<ScreenLook> + Send>;

/// The display epoch now (`epilot::visible::display_epoch`; a fake in
/// tests): readings of another epoch number the displays differently.
pub(crate) type EpochSource = Box<dyn Fn() -> u64 + Send>;

/// How long the window-list readings are kept for LABEL / MASK: longer than
/// any frame the OS still hands out (the slot holds at most one interval).
const READINGS_KEPT_MS: i64 = 3_000;
/// Hears Active / Paused changes.
pub(crate) type StatusSink = Arc<dyn Fn(Status) + Send + Sync>;

#[derive(Clone, Copy, Debug)]
pub(crate) struct Timing {
    pub interval: Duration,
    pub poll: Duration,
    /// `store.encodePercent`: the share of a core the encode worker may
    /// take on average (100 = no rest).
    pub encode_percent: u32,
}

impl Timing {
    pub(crate) fn from_fixture() -> Timing {
        Timing {
            interval: Duration::from_millis(fixture::interval_ms()),
            poll: Duration::from_millis(fixture::focus_poll_ms()),
            encode_percent: fixture::encode_percent(),
        }
    }
}

pub(crate) fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|since| since.as_millis() as i64)
        .unwrap_or_default()
}

/// PAUSE: the focused window holds capture paused; no focused window (no
/// window list, the focus off every display) does too (fail closed).
pub(crate) fn paused_by(focus: Option<&FocusedWindow>) -> bool {
    match focus {
        None => true,
        Some(window) => ring::denied(window.app_id.as_deref().unwrap_or(&window.app), &window.app, &window.title),
    }
}

/// The displays captured now: none while paused, else the work displays
/// whose top window is not denylisted.
pub(crate) fn capturing(view: Option<&ScreenView>, paused: bool) -> Vec<u32> {
    let Some(view) = view.filter(|_| !paused) else { return Vec::new() };
    view.work
        .iter()
        .copied()
        .filter(|display| !view.tops.iter().any(|top| top.display == *display && ring::denied(top.app_id.as_deref().unwrap_or(&top.app), &top.app, &top.title)))
        .collect()
}

/// MASK: the rects of `display`'s denylisted windows (not the top one: that
/// skips the display) in a `width` x `height` frame; `None` = one of them
/// cannot be placed (the reading's display size is not the frame's), so the
/// frame is dropped.
pub(crate) fn masks(windows: &[&VisibleWindow], display: u32, width: u32, height: u32) -> Option<Vec<Rect>> {
    let mut masks = Vec::new();
    for window in windows.iter().filter(|window| window.display == display) {
        if !ring::denied(window.app_id.as_deref().unwrap_or(&window.app), &window.app, &window.title) {
            continue;
        }
        if window.display_size != (width, height) {
            return None;
        }
        if let Some(rect) = clip(window.rect, width, height) {
            if !masks.contains(&rect) {
                masks.push(rect);
            }
        }
    }
    Some(masks)
}

/// LABEL: the display holding the focused window carries its app and
/// title, another display its top window's, else nothing.
pub(crate) fn label(view: Option<&ScreenView>, display: u32) -> (String, String) {
    let Some(view) = view else { return Default::default() };
    if let Some(window) = view.focused.as_ref().filter(|window| window.display == display) {
        return (window.app.clone(), window.title.clone());
    }
    view.tops.iter().find(|top| top.display == display).map(|top| (top.app.clone(), top.title.clone())).unwrap_or_default()
}

/// One running screen memory: the store the tool reads and the thread
/// that fills it.
pub(crate) struct Memory {
    pub data_dir: PathBuf,
    pub store: Arc<Mutex<Store>>,
    stop: mpsc::Sender<()>,
    thread: Option<JoinHandle<()>>,
    worker: Option<JoinHandle<()>>,
    #[cfg_attr(not(test), allow(dead_code))]
    worker_state: Arc<Worker>,
}

impl Memory {
    /// Open the store (sweep + orphans first) and start the loop.
    pub(crate) fn start(
        data_dir: &Path,
        capturer: Box<dyn Capturer>,
        view: ViewSource,
        epoch: EpochSource,
        timing: Timing,
        hotkey: Arc<AtomicBool>,
        status: StatusSink,
    ) -> Result<Memory, String> {
        let store = Store::open(data_dir)?;
        store.sweep(now_ms())?;
        store.remove_orphans()?;
        let store = Arc::new(Mutex::new(store));
        let (stop, stopped) = mpsc::channel();
        let worker_state = Arc::new(Worker::default());
        let (jobs, inbox) = mpsc::channel::<Vec<Job>>();
        let worker = {
            let (store, state, percent) = (store.clone(), worker_state.clone(), timing.encode_percent);
            std::thread::Builder::new()
                .name("screen-memory-encode".into())
                .spawn(move || work(inbox, &store, &state, percent))
                .map_err(|err| format!("Could not start screen memory: {err}"))?
        };
        let mut looper = Loop {
            capturer,
            view,
            epoch,
            timing,
            hotkey,
            status,
            store: store.clone(),
            gate: Gate { dedupe: ring::Dedupe::default(), worker: worker_state.clone() },
            jobs,
            shown: None,
            last_focus: None,
            retry: Vec::new(),
            captured: None,
            since: HashMap::new(),
            owed: Vec::new(),
            readings: VecDeque::new(),
        };
        let thread = std::thread::Builder::new()
            .name("screen-memory".into())
            .spawn(move || looper.run(stopped))
            .map_err(|err| format!("Could not start screen memory: {err}"))?;
        Ok(Memory { data_dir: data_dir.to_path_buf(), store, stop, thread: Some(thread), worker: Some(worker), worker_state })
    }

    /// Hold the encode worker resting for `rest` from now.
    #[cfg(test)]
    pub(crate) fn rest(&self, rest: Duration) {
        *self.worker_state.rest_until.lock().unwrap() = Some(Instant::now() + rest);
    }

    /// Whether the encode worker still works on a batch.
    #[cfg(test)]
    pub(crate) fn encoding(&self) -> bool {
        self.worker_state.busy.load(Ordering::Acquire)
    }

    /// Stop the loop and the worker (it finishes its batch) and wait for
    /// both; `forget` deletes every frame and row.
    pub(crate) fn stop(mut self, forget: bool) {
        let _ = self.stop.send(());
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
        // The loop held the worker's only sender: it ends after its batch.
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
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
    view: ViewSource,
    epoch: EpochSource,
    timing: Timing,
    hotkey: Arc<AtomicBool>,
    status: StatusSink,
    store: Arc<Mutex<Store>>,
    gate: Gate,
    jobs: mpsc::Sender<Vec<Job>>,
    shown: Option<Status>,
    /// The (app, title) of the last poll, for the focus-change capture.
    last_focus: Option<(String, String)>,
    /// Displays whose frame the busy worker skipped: the next tick grabs
    /// their latest frame even when the capturer has no newer one.
    retry: Vec<u32>,
    /// The displays captured at the last look (`None` before the first).
    captured: Option<Vec<u32>>,
    /// STALE: per display, the earliest delivery a grab takes.
    since: HashMap<u32, Instant>,
    /// FRESH: displays back in capture that still need one frame (a still
    /// screen delivers none); captured fresh once their `since` passed.
    owed: Vec<u32>,
    /// The window-list readings of the last [`READINGS_KEPT_MS`] (Unix ms,
    /// oldest first; `None` = no window list then).
    readings: VecDeque<(i64, Option<Arc<ScreenLook>>)>,
}

impl Loop {
    fn run(&mut self, stopped: mpsc::Receiver<()>) {
        let mut next_tick = Instant::now();
        let mut next_sweep = Instant::now();
        loop {
            let look = (self.view)().map(Arc::new);
            self.remember(now_ms(), look.clone());
            let view = look.as_ref().map(|look| &look.view);
            let focus = view.and_then(|view| view.focused.clone());
            let paused = self.hotkey.load(Ordering::Acquire) || paused_by(focus.as_ref());
            self.show(if paused { Status::Paused } else { Status::Active });
            let now = Instant::now();
            let capturing = capturing(view, paused);
            self.resume(&capturing, now);
            if paused {
                // Nothing, not even the tick; the next tick after the
                // focus leaves runs at once.
                self.last_focus = None;
                self.retry.clear();
            } else if now >= next_tick {
                self.tick(&capturing);
                next_tick += self.timing.interval;
                if next_tick <= now {
                    next_tick = now + self.timing.interval;
                }
            } else {
                let key = focus.as_ref().map(|window| (window.app.clone(), window.title.clone()));
                if key.is_some() && key != self.last_focus {
                    let display = focus.as_ref().expect("a focused window").display;
                    if capturing.contains(&display) {
                        let grabbed = self.capturer.grab(self.want(display));
                        let epoch = (self.epoch)();
                        match grabbed {
                            Ok(Some(frame)) => self.keep(vec![frame], epoch),
                            Ok(None) => {}
                            Err(reason) => log::debug!("[screen] no focus-change frame: {reason}"),
                        }
                    }
                }
            }
            if !paused {
                self.last_focus = focus.as_ref().map(|window| (window.app.clone(), window.title.clone()));
            }
            // RETENTION on its own clock: a long pause still ages frames out.
            if now >= next_sweep {
                if let Err(reason) = self.store.lock().unwrap().sweep(now_ms()) {
                    log::warn!("[screen] {reason}");
                }
                next_sweep = now + self.timing.interval;
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

    /// Keep this reading; drop the ones older than [`READINGS_KEPT_MS`]
    /// (the newest stays).
    fn remember(&mut self, at: i64, look: Option<Arc<ScreenLook>>) {
        self.readings.push_back((at, look));
        while self.readings.len() > 1 && self.readings.front().is_some_and(|(first, _)| *first < at - READINGS_KEPT_MS) {
            self.readings.pop_front();
        }
    }

    /// The reading nearest `at` (Unix ms; a tie takes the later one).
    fn reading_at(&self, at: i64) -> Option<&Arc<ScreenLook>> {
        self.readings.iter().rev().min_by_key(|(when, _)| (when - at).abs()).and_then(|(_, look)| look.as_ref())
    }

    /// STALE: a display back in capture takes only frames delivered one
    /// interval from now (the OS may still hold one from before), and owes
    /// a FRESH frame.
    fn resume(&mut self, capturing: &[u32], now: Instant) {
        if let Some(before) = &self.captured {
            for display in capturing.iter().filter(|display| !before.contains(display)) {
                self.since.insert(*display, now + self.timing.interval);
                if !self.owed.contains(display) {
                    self.owed.push(*display);
                }
            }
        }
        self.owed.retain(|display| capturing.contains(display));
        self.captured = Some(capturing.to_vec());
    }

    fn want(&self, display: u32) -> Want {
        Want { display, since: self.since.get(&display).copied() }
    }

    fn tick(&mut self, capturing: &[u32]) {
        // No work display (a picked one gone, every one fullscreen): no grab.
        if capturing.is_empty() {
            self.retry.clear();
            return;
        }
        let wanted: Vec<Want> = capturing.iter().map(|display| self.want(*display)).collect();
        let grabbed = self.capturer.grab_all(&wanted);
        match grabbed {
            Ok(mut frames) => {
                for display in std::mem::take(&mut self.retry) {
                    if capturing.contains(&display) && !frames.iter().any(|frame| frame.display == display) {
                        if let Ok(Some(frame)) = self.capturer.grab(self.want(display)) {
                            frames.push(frame);
                        }
                    }
                }
                // FRESH: a display back in capture whose screen stood still
                // since (no delivery) is captured now, once.
                let now = Instant::now();
                for display in self.owed.clone() {
                    let due = self.since.get(&display).is_none_or(|since| now >= *since);
                    if !due || !capturing.contains(&display) || frames.iter().any(|frame| frame.display == display) {
                        continue;
                    }
                    match self.capturer.fresh(display) {
                        Ok(Some(frame)) => frames.push(frame),
                        Ok(None) => self.owed.retain(|owed| *owed != display),
                        Err(reason) => {
                            log::debug!("[screen] no fresh frame of display {display}: {reason}");
                            self.owed.retain(|owed| *owed != display);
                        }
                    }
                }
                // Every grab of this tick is done: the epoch they share.
                let epoch = (self.epoch)();
                self.keep(frames, epoch);
            }
            Err(reason) => log::debug!("[screen] no frames this tick: {reason}"),
        }
    }

    /// Hash and dedupe the frames (grabbed under display `epoch`); the
    /// kept ones go to the worker as one batch unless it is still busy.
    fn keep(&mut self, frames: Vec<Frame>, epoch: u64) {
        // Busy: not even the hash (the gate would skip them all anyway).
        if !self.gate.worker.ready() {
            self.skip(frames.iter().map(|frame| frame.display));
            return;
        }
        if frames.is_empty() {
            return;
        }
        let grabbed = now_ms();
        // RACE: the window list once more, now that the pixels are fixed: a
        // denylisted window that opened since the poll's read is in it. One
        // read per grab with frames (a still screen grabs none), against
        // one every focus poll. None, or the display set moved on since the
        // grab: nothing is placed safely, the next tick grabs again.
        let latest = (self.view)().map(Arc::new);
        self.remember(now_ms(), latest.clone());
        if !latest.as_ref().is_some_and(|look| look.epoch == epoch) {
            self.skip(frames.iter().map(|frame| frame.display));
            return;
        }
        let frames: Vec<(Frame, u64, (String, String), Option<Rect>)> = frames
            .into_iter()
            .filter(Frame::valid)
            .filter_map(|mut frame| {
                let at = *frame.at.get_or_insert(grabbed);
                // The screen as it was when the OS captured this frame; none
                // (no window list then), a denylisted focus or top window
                // then = dropped (fail closed).
                let reading = self.reading_at(at)?.clone();
                // DISPLAY SET: a reading of another display numbering.
                if reading.epoch != epoch {
                    return None;
                }
                let view = Some(&reading.view);
                let focus = reading.view.focused.as_ref();
                if paused_by(focus) || !capturing(view, false).contains(&frame.display) {
                    return None;
                }
                // MASK: every denylisted window on the display then and now.
                let windows: Vec<&VisibleWindow> = reading.windows.iter().chain(latest.iter().flat_map(|look| look.windows.iter())).collect();
                frame.masks = masks(&windows, frame.display, frame.width, frame.height)?;
                let hash = hash::hash_rows(frame.width, frame.height, frame.stride, frame.bgra, frame.bytes(), &frame.masks);
                let label = label(view, frame.display);
                let focus = focus.filter(|window| window.display == frame.display).and_then(|window| clip(window.rect, frame.width, frame.height));
                Some((frame, hash, label, focus))
            })
            .collect();
        let keys: Vec<(u32, u64, &str, &str)> =
            frames.iter().map(|(frame, hash, (app, title), _)| (frame.display, *hash, app.as_str(), title.as_str())).collect();
        let Some(admitted) = self.gate.admit(&keys) else {
            let displays: Vec<u32> = keys.iter().map(|key| key.0).collect();
            self.skip(displays.into_iter());
            return;
        };
        // FRESH: a display with a frame through the gate owes none.
        self.owed.retain(|display| !keys.iter().any(|key| key.0 == *display));
        let batch: Vec<Job> = frames
            .into_iter()
            .zip(admitted)
            .filter(|(_, admitted)| *admitted)
            .map(|((frame, hash, (app, title), focus), _)| {
                let at = frame.at.unwrap_or(grabbed);
                let row = Row {
                    at,
                    display: frame.display,
                    app,
                    title,
                    hash: hash::hex(hash),
                    path: store::relative_path(at, frame.display),
                    width: frame.width,
                    height: frame.height,
                    focus,
                };
                Job { frame, row }
            })
            .collect();
        if !batch.is_empty() && self.jobs.send(batch).is_err() {
            self.gate.worker.busy.store(false, Ordering::Release);
        }
    }
}

impl Loop {
    /// SKIP IF BUSY: the next tick grabs these displays again.
    fn skip(&mut self, displays: impl Iterator<Item = u32>) {
        for display in displays {
            if !self.retry.contains(&display) {
                self.retry.push(display);
            }
        }
    }
}

/// One kept frame for the worker: its pixels and its index row.
struct Job {
    frame: Frame,
    row: Row,
}

/// The encode worker's state, shared with the [`Gate`].
#[derive(Default)]
pub(crate) struct Worker {
    /// A batch is queued or being encoded.
    pub busy: AtomicBool,
    /// BUDGET: the worker rests until then after its last batch.
    pub rest_until: Mutex<Option<Instant>>,
}

impl Worker {
    pub(crate) fn ready(&self) -> bool {
        !self.busy.load(Ordering::Acquire) && self.rest_until.lock().unwrap().is_none_or(|until| Instant::now() >= until)
    }
}

/// SKIP IF BUSY in front of the dedupe.
pub(crate) struct Gate {
    pub dedupe: ring::Dedupe,
    pub worker: Arc<Worker>,
}

impl Gate {
    /// Which of one grab's frames ((display, hash, app, title)) go to the
    /// worker. While it still works on a batch or rests after one: `None`,
    /// and the dedupe stays where it was, so the next tick retries the
    /// change. Else the ones the dedupe keeps, now its last kept ones, and
    /// the worker counts as busy when any go.
    pub(crate) fn admit(&mut self, frames: &[(u32, u64, &str, &str)]) -> Option<Vec<bool>> {
        if !self.worker.ready() {
            return None;
        }
        let admitted: Vec<bool> = frames.iter().map(|&(display, hash, app, title)| self.dedupe.kept(display, app, title, hash)).collect();
        for (&(display, hash, app, title), _) in frames.iter().zip(&admitted).filter(|(_, admitted)| **admitted) {
            self.dedupe.remember(display, app, title, hash);
        }
        if admitted.contains(&true) {
            self.worker.busy.store(true, Ordering::Release);
        }
        Some(admitted)
    }
}


/// The encode worker: below normal priority, one batch at a time (cut,
/// fit, WebP, store), then a rest so that its average stays within
/// `percent` of a core: a batch that took t of CPU rests t x (100 -
/// percent) / percent.
fn work(inbox: mpsc::Receiver<Vec<Job>>, store: &Mutex<Store>, state: &Worker, percent: u32) {
    lower_priority();
    let percent = f64::from(percent.clamp(1, 100));
    for batch in inbox {
        let started = thread_cpu();
        for job in batch {
            let webp = shrink::encode(&shrink::prepare(&job.frame, job.row.focus));
            drop(job.frame);
            if webp.is_empty() {
                continue;
            }
            if let Err(reason) = store.lock().unwrap().insert(&job.row, &webp) {
                log::warn!("[screen] {reason}");
            }
        }
        let rest = thread_cpu().saturating_sub(started).mul_f64((100. - percent) / percent);
        *state.rest_until.lock().unwrap() = Some(Instant::now() + rest);
        state.busy.store(false, Ordering::Release);
    }
}

/// The calling thread's CPU time (wall time where the OS has no clock).
fn thread_cpu() -> Duration {
    #[cfg(unix)]
    {
        // SAFETY: plain integers; zero is a valid timespec.
        let mut now: libc::timespec = unsafe { std::mem::zeroed() };
        if unsafe { libc::clock_gettime(libc::CLOCK_THREAD_CPUTIME_ID, &mut now) } == 0 {
            return Duration::new(now.tv_sec as u64, now.tv_nsec as u32);
        }
    }
    #[cfg(windows)]
    {
        #[repr(C)]
        #[derive(Default)]
        struct FileTime(u32, u32);
        #[link(name = "kernel32")]
        extern "system" {
            fn GetCurrentThread() -> isize;
            fn GetThreadTimes(thread: isize, creation: *mut FileTime, exit: *mut FileTime, kernel: *mut FileTime, user: *mut FileTime) -> i32;
        }
        let (mut creation, mut exit, mut kernel, mut user) = (FileTime::default(), FileTime::default(), FileTime::default(), FileTime::default());
        if unsafe { GetThreadTimes(GetCurrentThread(), &mut creation, &mut exit, &mut kernel, &mut user) } != 0 {
            let ticks = |time: &FileTime| u64::from(time.1) << 32 | u64::from(time.0);
            return Duration::from_nanos((ticks(&kernel) + ticks(&user)) * 100);
        }
    }
    static START: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();
    START.get_or_init(Instant::now).elapsed()
}

/// The calling thread below normal priority: macOS QoS utility, Linux nice
/// 10 (`setpriority` on 0 = this thread there), Windows below normal.
fn lower_priority() {
    #[cfg(target_os = "macos")]
    unsafe {
        libc::pthread_set_qos_class_self_np(libc::qos_class_t::QOS_CLASS_UTILITY, 0);
    }
    #[cfg(target_os = "linux")]
    unsafe {
        libc::setpriority(libc::PRIO_PROCESS, 0, 10);
    }
    #[cfg(target_os = "windows")]
    {
        #[link(name = "kernel32")]
        extern "system" {
            fn GetCurrentThread() -> isize;
            fn SetThreadPriority(thread: isize, priority: i32) -> i32;
        }
        const THREAD_PRIORITY_BELOW_NORMAL: i32 = -1;
        unsafe { SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_BELOW_NORMAL) };
    }
}

/// The focused rect within a `width` x `height` frame; `None` when nothing
/// of it is on the frame.
fn clip(rect: epilot::ScreenRect, width: u32, height: u32) -> Option<Rect> {
    let left = i64::from(rect.x).clamp(0, i64::from(width));
    let top = i64::from(rect.y).clamp(0, i64::from(height));
    let right = (i64::from(rect.x) + i64::from(rect.width)).clamp(0, i64::from(width));
    let bottom = (i64::from(rect.y) + i64::from(rect.height)).clamp(0, i64::from(height));
    (right > left && bottom > top).then(|| Rect { x: left as i32, y: top as i32, width: (right - left) as u32, height: (bottom - top) as u32 })
}
