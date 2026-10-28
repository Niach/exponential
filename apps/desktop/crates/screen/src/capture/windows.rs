//! NEW-58: the Windows capturer (screen-memory.json CAPTURE): Windows
//! Graphics Capture, one session per monitor, each on its own thread
//! (`windows-capture`'s free-threaded start) keeping its latest frame behind
//! a Mutex. The yellow border is off where the OS allows it
//! (`IsBorderRequired = false`, Windows 11), the cursor is off, and the
//! session's minimum update interval is the fixture's `capture.intervalMs`.
//!
//! Display numbers are 1-based in `EnumDisplayMonitors` order, which is the
//! computer crate's order (`computer::displays`); `Monitor::enumerate` walks
//! the same API. A monitor set change (plug, unplug) rebuilds the sessions on
//! the next grab; a session that ended (secure desktop, device lost) is
//! restarted, at most once every `RESTART_BACKOFF`.

use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use windows_capture::capture::{CaptureControl, Context, GraphicsCaptureApiHandler};
use windows_capture::frame::Frame as WgcFrame;
use windows_capture::graphics_capture_api::{GraphicsCaptureApi, InternalCaptureControl};
use windows_capture::monitor::Monitor;
use windows_capture::settings::{
    ColorFormat, CursorCaptureSettings, DirtyRegionSettings, DrawBorderSettings, MinimumUpdateIntervalSettings,
    SecondaryWindowSettings, Settings,
};

use super::{Capturer, Frame, Want};

/// screen-memory.json `capture.intervalMs`.
const INTERVAL_MS: u64 = 1000;
/// How long a session that ended or failed to start waits before a retry.
const RESTART_BACKOFF: Duration = Duration::from_secs(5);

/// One monitor's latest frame, RGBA8, tightly packed.
#[derive(Default)]
struct Latest {
    width: u32,
    height: u32,
    rgba: Vec<u8>,
    /// Bumped on every delivered frame (1 = the first).
    seq: u64,
    /// STALE: when it arrived; STAMP: when it was captured (Unix ms).
    delivered: Option<Instant>,
    at: i64,
}

/// STAMP: a frame's SystemRelativeTime (QPC in 100 ns units) as Unix ms:
/// now minus how long ago it was on the performance counter.
fn captured_at(timestamp: i64) -> i64 {
    #[link(name = "kernel32")]
    extern "system" {
        fn QueryPerformanceCounter(count: *mut i64) -> i32;
        fn QueryPerformanceFrequency(frequency: *mut i64) -> i32;
    }
    let now = super::wall_ms();
    let (mut count, mut frequency) = (0i64, 0i64);
    // SAFETY: two out-pointers to live integers.
    if unsafe { QueryPerformanceCounter(&mut count) } == 0 || unsafe { QueryPerformanceFrequency(&mut frequency) } == 0 || frequency <= 0 {
        return now;
    }
    let counter = (i128::from(count) * 10_000_000 / i128::from(frequency)) as i64;
    let ago = (counter - timestamp) / 10_000;
    if (0..10_000).contains(&ago) {
        now - ago
    } else {
        now
    }
}

type Slot = Arc<Mutex<Option<Latest>>>;

/// The per-session WGC handler: copies every delivered frame into its slot.
struct Handler {
    slot: Slot,
    /// The buffer the next frame is copied into outside the lock, then
    /// swapped with the slot's, so a grab never waits on a full-frame copy.
    spare: Vec<u8>,
}

impl GraphicsCaptureApiHandler for Handler {
    type Flags = Slot;
    type Error = String;

    fn new(ctx: Context<Self::Flags>) -> Result<Self, Self::Error> {
        Ok(Self { slot: ctx.flags, spare: Vec::new() })
    }

    fn on_frame_arrived(&mut self, frame: &mut WgcFrame, _control: InternalCaptureControl) -> Result<(), Self::Error> {
        let delivered = Instant::now();
        let at = captured_at(frame.timestamp().Duration);
        let mut buffer = frame.buffer().map_err(|e| format!("frame buffer: {e}"))?;
        let (width, height) = (buffer.width(), buffer.height());
        // Rgba8 is requested from the frame pool, so no BGRA swizzle here.
        let pixels = buffer.as_nopadding_buffer().map_err(|e| format!("frame pixels: {e}"))?;
        self.spare.clear();
        self.spare.extend_from_slice(pixels);
        let mut slot = self.slot.lock().unwrap_or_else(|p| p.into_inner());
        let latest = slot.get_or_insert_with(Latest::default);
        latest.width = width;
        latest.height = height;
        latest.seq += 1;
        latest.delivered = Some(delivered);
        latest.at = at;
        std::mem::swap(&mut latest.rgba, &mut self.spare);
        Ok(())
    }
}

/// One monitor's capture.
struct Session {
    /// The `HMONITOR` as an integer (raw handles are not `Send`).
    monitor: usize,
    slot: Slot,
    control: Option<CaptureControl<Handler, String>>,
    /// When the last start was attempted (for the restart backoff).
    started: Instant,
    /// Why the last start failed, if it did.
    error: Option<String>,
    /// The `Latest::seq` `grab_all` last handed out (0 = none yet).
    handed: u64,
}

impl Session {
    fn start(monitor: Monitor) -> Self {
        let slot: Slot = Arc::new(Mutex::new(None));
        let mut session = Self {
            monitor: monitor.as_raw_hmonitor() as usize,
            slot,
            control: None,
            started: Instant::now(),
            error: None,
            handed: 0,
        };
        session.spawn(monitor);
        session
    }

    fn spawn(&mut self, monitor: Monitor) {
        self.started = Instant::now();
        let settings = Settings::new(
            monitor,
            if GraphicsCaptureApi::is_cursor_settings_supported().unwrap_or(false) {
                CursorCaptureSettings::WithoutCursor
            } else {
                CursorCaptureSettings::Default
            },
            // Windows 11: the yellow border is optional; Windows 10 always
            // draws it and refuses the setting.
            if GraphicsCaptureApi::is_border_settings_supported().unwrap_or(false) {
                DrawBorderSettings::WithoutBorder
            } else {
                DrawBorderSettings::Default
            },
            SecondaryWindowSettings::Default,
            if GraphicsCaptureApi::is_minimum_update_interval_supported().unwrap_or(false) {
                MinimumUpdateIntervalSettings::Custom(Duration::from_millis(INTERVAL_MS))
            } else {
                MinimumUpdateIntervalSettings::Default
            },
            DirtyRegionSettings::Default,
            ColorFormat::Rgba8,
            self.slot.clone(),
        );
        match Handler::start_free_threaded(settings) {
            Ok(control) => {
                self.control = Some(control);
                self.error = None;
            }
            Err(e) => {
                log::warn!("screen: capture of monitor {:#x} failed to start: {e}", self.monitor);
                self.control = None;
                self.error = Some(e.to_string());
            }
        }
    }

    fn running(&self) -> bool {
        self.control.as_ref().is_some_and(|c| !c.is_finished())
    }

    /// Restart an ended or failed session once the backoff has passed.
    fn revive(&mut self) {
        if self.running() || self.started.elapsed() < RESTART_BACKOFF {
            return;
        }
        if let Some(control) = self.control.take() {
            if let Err(e) = control.stop() {
                log::warn!("screen: capture of monitor {:#x} ended: {e}", self.monitor);
            }
        }
        // A new session counts its frames from 1 again.
        self.handed = 0;
        let slot = Arc::new(Mutex::new(None));
        self.slot = slot;
        self.spawn(Monitor::from_raw_hmonitor(self.monitor as *mut std::ffi::c_void));
    }

    /// The latest frame, whether or not it was handed out before (`grab`);
    /// `None` when it arrived before `want.since` (STALE).
    fn latest(&self, want: Want) -> Option<Frame> {
        let slot = self.slot.lock().unwrap_or_else(|p| p.into_inner());
        let latest = slot.as_ref()?;
        if latest.width == 0 || latest.height == 0 || latest.rgba.is_empty() || !want.takes(latest.delivered?) {
            return None;
        }
        Some(Frame::rgba(want.display, latest.width, latest.height, latest.rgba.clone()).stamped(latest.at))
    }

    /// The latest frame only when it arrived after the one `grab_all` last
    /// handed out (and not before `want.since`): a static screen (WGC
    /// delivers nothing) costs no copy.
    fn fresh(&mut self, want: Want) -> Option<Frame> {
        let slot = self.slot.lock().unwrap_or_else(|p| p.into_inner());
        let latest = slot.as_ref()?;
        if latest.seq == self.handed || latest.width == 0 || latest.height == 0 || latest.rgba.is_empty() || !want.takes(latest.delivered?) {
            return None;
        }
        self.handed = latest.seq;
        Some(Frame::rgba(want.display, latest.width, latest.height, latest.rgba.clone()).stamped(latest.at))
    }

    fn stop(&mut self) {
        if let Some(control) = self.control.take() {
            if let Err(e) = control.stop() {
                log::debug!("screen: stopping capture of monitor {:#x}: {e}", self.monitor);
            }
        }
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        self.stop();
    }
}

struct Wgc {
    /// Index i = display i + 1 (EnumDisplayMonitors order).
    sessions: Vec<Session>,
}

impl Wgc {
    /// Rebuild on a monitor set change, revive ended sessions.
    fn refresh(&mut self) -> Result<(), String> {
        let monitors = Monitor::enumerate().map_err(|e| format!("Could not list the displays: {e}"))?;
        let handles: Vec<usize> = monitors.iter().map(|m| m.as_raw_hmonitor() as usize).collect();
        let current: Vec<usize> = self.sessions.iter().map(|s| s.monitor).collect();
        if handles != current {
            self.sessions.clear();
            self.sessions = monitors.into_iter().map(Session::start).collect();
            return Ok(());
        }
        for session in &mut self.sessions {
            session.revive();
        }
        Ok(())
    }

    /// Err only when there are displays and not one of them captures.
    fn check(&self) -> Result<(), String> {
        if self.sessions.is_empty() || self.sessions.iter().any(Session::running) {
            return Ok(());
        }
        let reason = self.sessions.iter().find_map(|s| s.error.clone()).unwrap_or_else(|| "the capture ended".into());
        Err(format!("Screen capture is not running: {reason}"))
    }
}

impl Capturer for Wgc {
    /// Only the wanted displays with a frame new since the last `grab_all`.
    fn grab_all(&mut self, wanted: &[Want]) -> Result<Vec<Frame>, String> {
        self.refresh()?;
        self.check()?;
        let sessions = &mut self.sessions;
        Ok(wanted
            .iter()
            .filter_map(|want| sessions.get_mut(want.display.checked_sub(1)? as usize)?.fresh(*want))
            .collect())
    }

    fn grab(&mut self, wanted: Want) -> Result<Option<Frame>, String> {
        self.refresh()?;
        let Some(session) = wanted.display.checked_sub(1).and_then(|i| self.sessions.get(i as usize)) else {
            return Ok(None);
        };
        Ok(session.latest(wanted))
    }
}

pub(super) fn open(_state_dir: &Path) -> Result<Box<dyn Capturer>, String> {
    match GraphicsCaptureApi::is_supported() {
        Ok(true) => {}
        Ok(false) => {
            return Err(
                "Screen memory needs Windows Graphics Capture (Windows 10 version 2004 or later).".to_string()
            );
        }
        Err(e) => return Err(format!("Screen memory could not check for Windows Graphics Capture: {e}")),
    }
    let mut capturer = Wgc { sessions: Vec::new() };
    capturer.refresh()?;
    Ok(Box::new(capturer))
}
