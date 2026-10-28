//! NEW-58: the macOS capturer (screen-memory.json CAPTURE): ScreenCaptureKit,
//! one persistent `SCStream` per display at `minimumFrameInterval` =
//! `capture.intervalMs`, the IDE's own app excluded from every filter.
//!
//! Each stream's output keeps only its LATEST pixel buffer (retained, never
//! copied on the callback queue); a grab LENDS it as a BGRA frame under a
//! read-only lock (released when the frame drops), never copied: the ring
//! hashes it in place and only a kept frame's window is copied, on the
//! encode worker. ScreenCaptureKit delivers a new buffer only when the
//! display changed, so a static screen costs nothing between grabs.
//!
//! `grab_all` hands out each display's latest frame ONCE: a display without a
//! new frame since the last grab is left out (the trait allows it; the ring's
//! dedupe would drop it anyway). `grab(display)` returns that display's latest
//! frame regardless (the focus-change capture).
//!
//! The display set (`CGGetActiveDisplayList`: the computer crate's order,
//! with each display's physical pixel size) is read on every grab, a few
//! microseconds; a change rebuilds only the streams that changed. A stream
//! the system stopped (display gone, permission revoked) is rebuilt at most
//! every `RETRY` seconds.

use std::path::Path;
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant};

use block2::RcBlock;
use dispatch2::{DispatchQueue, DispatchRetained};
use objc2::rc::Retained;
use objc2::runtime::{NSObject, NSObjectProtocol, ProtocolObject};
use objc2::{define_class, msg_send, AllocAnyThread, DefinedClass};
use objc2_core_foundation::CFRetained;
use objc2_core_graphics::{
    CGDirectDisplayID, CGDisplayCopyDisplayMode, CGDisplayMode, CGGetActiveDisplayList,
    CGPreflightScreenCaptureAccess,
};
use objc2_core_media::{CMSampleBuffer, CMTime};
use objc2_core_video::{
    kCVPixelFormatType_32BGRA, CVPixelBuffer, CVPixelBufferGetBaseAddress,
    CVPixelBufferGetBytesPerRow, CVPixelBufferGetHeight, CVPixelBufferGetPixelFormatType,
    CVPixelBufferGetWidth, CVPixelBufferLockBaseAddress, CVPixelBufferLockFlags,
    CVPixelBufferUnlockBaseAddress,
};
use objc2_foundation::{NSArray, NSError};
use objc2_screen_capture_kit::{
    SCContentFilter, SCDisplay, SCRunningApplication, SCShareableContent, SCStream,
    SCStreamConfiguration, SCStreamDelegate, SCStreamOutput, SCStreamOutputType,
};

use super::{Capturer, Frame, Pixels};

/// screen-memory.json `capture.intervalMs`.
const INTERVAL_MS: i64 = 1000;
/// How long to wait for ScreenCaptureKit's completion handlers.
const TIMEOUT: Duration = Duration::from_secs(5);
/// The least time between two rebuilds of failed or stopped streams.
const RETRY: Duration = Duration::from_secs(5);
const NOT_GRANTED: &str = "Screen Recording is not granted.";

pub(super) fn open(_state_dir: &Path) -> Result<Box<dyn Capturer>, String> {
    if !CGPreflightScreenCaptureAccess() {
        return Err(NOT_GRANTED.to_string());
    }
    let mut capturer = MacCapturer {
        streams: Vec::new(),
        layout: Vec::new(),
        next_retry: Instant::now(),
    };
    capturer.sync()?;
    if capturer.streams.is_empty() {
        return Err("ScreenCaptureKit started no display stream.".to_string());
    }
    Ok(Box::new(capturer))
}

/// One active display: its id and physical pixel size.
#[derive(Clone, Copy, PartialEq, Eq)]
struct Layout {
    id: CGDirectDisplayID,
    width: usize,
    height: usize,
}

struct MacCapturer {
    /// In `layout` order; a display whose stream failed is missing.
    streams: Vec<DisplayStream>,
    layout: Vec<Layout>,
    next_retry: Instant,
}

// SAFETY: SCStream, the output object and the dispatch queue are only
// started, stopped and released from here; ScreenCaptureKit allows that from
// any thread. The state shared with the callback queue sits behind a Mutex.
unsafe impl Send for MacCapturer {}

impl Capturer for MacCapturer {
    fn grab_all(&mut self) -> Result<Vec<Frame>, String> {
        self.sync()?;
        Ok(self.streams.iter().filter_map(|s| s.take(true)).collect())
    }

    fn grab(&mut self, display: u32) -> Result<Option<Frame>, String> {
        self.sync()?;
        Ok(self
            .streams
            .iter()
            .find(|s| s.number == display)
            .and_then(|s| s.take(false)))
    }
}

impl MacCapturer {
    /// Keep one running stream per active display. `Err` only when no
    /// display can be captured at all.
    fn sync(&mut self) -> Result<(), String> {
        let layout = active_layout()?;
        let changed = layout != self.layout;
        let healthy = self.streams.len() == layout.len()
            && self.streams.iter().all(|s| !s.stopped());
        if !changed && (healthy || Instant::now() < self.next_retry) {
            return Ok(());
        }
        self.next_retry = Instant::now() + RETRY;

        let mut old = std::mem::take(&mut self.streams);
        let mut kept: Vec<Option<DisplayStream>> = layout
            .iter()
            .map(|want| {
                old.iter()
                    .position(|s| s.layout == *want && !s.stopped())
                    .map(|pos| old.swap_remove(pos))
            })
            .collect();
        drop(old);

        let mut error = None;
        if kept.iter().any(Option::is_none) {
            match shareable_content() {
                Ok(content) => {
                    let own = own_application(&content);
                    let displays = unsafe { content.displays() };
                    for (want, slot) in layout.iter().zip(kept.iter_mut()) {
                        if slot.is_some() {
                            continue;
                        }
                        let Some(display) =
                            displays.iter().find(|d| unsafe { d.displayID() } == want.id)
                        else {
                            continue;
                        };
                        match DisplayStream::start(&display, own.as_deref(), *want) {
                            Ok(stream) => *slot = Some(stream),
                            Err(err) => {
                                log::warn!("screen: display {} stream: {err}", want.id);
                                error = Some(err);
                            }
                        }
                    }
                }
                Err(err) => {
                    log::warn!("screen: shareable content: {err}");
                    error = Some(err);
                }
            }
        }

        self.streams = kept
            .into_iter()
            .enumerate()
            .filter_map(|(index, stream)| {
                let mut stream = stream?;
                stream.number = index as u32 + 1;
                Some(stream)
            })
            .collect();
        self.layout = layout;
        match error {
            Some(err) if self.streams.is_empty() => Err(err),
            _ => Ok(()),
        }
    }
}

/// The active displays in CoreGraphics order (the computer crate's
/// `CGDisplay::active_displays`), each with its physical pixel size.
fn active_layout() -> Result<Vec<Layout>, String> {
    let mut ids = [0 as CGDirectDisplayID; 32];
    let mut count = 0u32;
    let err = unsafe { CGGetActiveDisplayList(ids.len() as u32, ids.as_mut_ptr(), &mut count) };
    if err.0 != 0 {
        return Err(format!("displays: CGError {}", err.0));
    }
    Ok(ids[..(count as usize).min(ids.len())]
        .iter()
        .map(|&id| {
            let mode = CGDisplayCopyDisplayMode(id);
            Layout {
                id,
                width: CGDisplayMode::pixel_width(mode.as_deref()),
                height: CGDisplayMode::pixel_height(mode.as_deref()),
            }
        })
        .collect())
}

fn shareable_content() -> Result<Retained<SCShareableContent>, String> {
    let (tx, rx) = mpsc::channel::<Result<Unsync<Retained<SCShareableContent>>, String>>();
    let block = RcBlock::new(move |content: *mut SCShareableContent, error: *mut NSError| {
        let result = match unsafe { Retained::retain(content) } {
            Some(content) => Ok(Unsync(content)),
            None => Err(describe(error)),
        };
        let _ = tx.send(result);
    });
    unsafe { SCShareableContent::getShareableContentWithCompletionHandler(&block) };
    match rx.recv_timeout(TIMEOUT) {
        Ok(Ok(content)) => Ok(content.0),
        Ok(Err(_)) | Err(_) if !CGPreflightScreenCaptureAccess() => Err(NOT_GRANTED.to_string()),
        Ok(Err(err)) => Err(err),
        Err(_) => Err("ScreenCaptureKit did not answer.".to_string()),
    }
}

/// This process as ScreenCaptureKit lists it (absent while it owns no
/// window, e.g. a command-line example), to keep the IDE out of its own
/// screen memory.
fn own_application(content: &SCShareableContent) -> Option<Retained<SCRunningApplication>> {
    let pid = std::process::id() as i32;
    unsafe { content.applications() }
        .iter()
        .find(|app| unsafe { app.processID() } == pid)
}

fn describe(error: *mut NSError) -> String {
    match unsafe { error.as_ref() } {
        Some(error) => error.localizedDescription().to_string(),
        None => "ScreenCaptureKit failed without an error.".to_string(),
    }
}

/// A value moved to or from a ScreenCaptureKit queue (only ever owned by
/// one thread at a time).
struct Unsync<T>(T);
// SAFETY: see the type's comment; CoreFoundation and Objective-C reference
// counting is thread-safe.
unsafe impl<T> Send for Unsync<T> {}

/// What the callback queue shares with the capturer.
#[derive(Default)]
struct Slot {
    latest: Option<Unsync<CFRetained<CVPixelBuffer>>>,
    /// Bumped per delivered frame; `taken` = the seq handed out last.
    seq: u64,
    taken: u64,
    stopped: bool,
}

struct OutputIvars {
    slot: Arc<Mutex<Slot>>,
}

define_class!(
    // SAFETY: NSObject has no subclassing requirements; no Drop impl.
    #[unsafe(super(NSObject))]
    #[name = "ExpScreenMemoryStreamOutput"]
    #[ivars = OutputIvars]
    struct StreamOutput;

    unsafe impl NSObjectProtocol for StreamOutput {}

    unsafe impl SCStreamOutput for StreamOutput {
        #[unsafe(method(stream:didOutputSampleBuffer:ofType:))]
        fn did_output(&self, _stream: &SCStream, sample: &CMSampleBuffer, kind: SCStreamOutputType) {
            if kind != SCStreamOutputType::Screen {
                return;
            }
            // Idle / blank frames carry no image buffer: keep the last one.
            let Some(buffer) = (unsafe { sample.image_buffer() }) else {
                return;
            };
            if let Ok(mut slot) = self.ivars().slot.lock() {
                slot.latest = Some(Unsync(buffer));
                slot.seq += 1;
            }
        }
    }

    unsafe impl SCStreamDelegate for StreamOutput {
        #[unsafe(method(stream:didStopWithError:))]
        fn did_stop(&self, _stream: &SCStream, error: &NSError) {
            log::warn!("screen: stream stopped: {}", error.localizedDescription());
            if let Ok(mut slot) = self.ivars().slot.lock() {
                slot.stopped = true;
            }
        }
    }
);

impl StreamOutput {
    fn new(slot: Arc<Mutex<Slot>>) -> Retained<Self> {
        let this = Self::alloc().set_ivars(OutputIvars { slot });
        unsafe { msg_send![super(this), init] }
    }
}

struct DisplayStream {
    layout: Layout,
    /// 1-based, the computer crate's order.
    number: u32,
    slot: Arc<Mutex<Slot>>,
    stream: Retained<SCStream>,
    output: Retained<StreamOutput>,
    _queue: DispatchRetained<DispatchQueue>,
}

impl DisplayStream {
    fn start(
        display: &SCDisplay,
        own: Option<&SCRunningApplication>,
        layout: Layout,
    ) -> Result<Self, String> {
        let excluded = match own {
            Some(app) => NSArray::from_slice(&[app]),
            None => NSArray::new(),
        };
        let filter = unsafe {
            SCContentFilter::initWithDisplay_excludingApplications_exceptingWindows(
                SCContentFilter::alloc(),
                display,
                &excluded,
                &NSArray::new(),
            )
        };
        let config = unsafe { SCStreamConfiguration::new() };
        unsafe {
            config.setWidth(layout.width);
            config.setHeight(layout.height);
            config.setScalesToFit(false);
            config.setPixelFormat(kCVPixelFormatType_32BGRA);
            config.setMinimumFrameInterval(CMTime::new(INTERVAL_MS, 1000));
            config.setShowsCursor(false);
            // The fewest surfaces ScreenCaptureKit allows (3) plus one: the
            // slot holds the latest, a kept frame lends one to the encode
            // worker while it is cut.
            config.setQueueDepth(4);
        }

        let slot = Arc::new(Mutex::new(Slot::default()));
        let output = StreamOutput::new(slot.clone());
        let stream = unsafe {
            SCStream::initWithFilter_configuration_delegate(
                SCStream::alloc(),
                &filter,
                &config,
                Some(ProtocolObject::from_ref(&*output)),
            )
        };
        let queue = DispatchQueue::new("at.exponential.screen-memory", None);
        unsafe {
            stream.addStreamOutput_type_sampleHandlerQueue_error(
                ProtocolObject::from_ref(&*output),
                SCStreamOutputType::Screen,
                Some(&queue),
            )
        }
        .map_err(|err| err.localizedDescription().to_string())?;

        let (tx, rx) = mpsc::channel::<Option<String>>();
        let block = RcBlock::new(move |error: *mut NSError| {
            let _ = tx.send((!error.is_null()).then(|| describe(error)));
        });
        unsafe { stream.startCaptureWithCompletionHandler(Some(&block)) };
        let this = Self {
            layout,
            number: 0,
            slot,
            stream,
            output,
            _queue: queue,
        };
        match rx.recv_timeout(TIMEOUT) {
            Ok(None) => Ok(this),
            Ok(Some(err)) => Err(err),
            Err(_) => Err("ScreenCaptureKit did not start the stream.".to_string()),
        }
    }

    fn stopped(&self) -> bool {
        self.slot.lock().map(|slot| slot.stopped).unwrap_or(true)
    }

    /// The latest frame; `fresh_only` = `None` when it was handed out
    /// already.
    fn take(&self, fresh_only: bool) -> Option<Frame> {
        let buffer = {
            let mut slot = self.slot.lock().ok()?;
            if fresh_only && slot.seq == slot.taken {
                return None;
            }
            slot.taken = slot.seq;
            slot.latest.as_ref()?.0.clone()
        };
        to_frame(buffer, self.number)
    }
}

impl Drop for DisplayStream {
    fn drop(&mut self) {
        // Stop without blocking; the block keeps the stream and its output
        // alive until ScreenCaptureKit is done with them.
        let keep = Unsync((self.stream.clone(), self.output.clone()));
        let block = RcBlock::new(move |_error: *mut NSError| {
            let _ = &keep;
        });
        unsafe { self.stream.stopCaptureWithCompletionHandler(Some(&block)) };
    }
}

/// A pixel buffer locked read-only while a frame borrows its bytes.
struct Locked {
    buffer: CFRetained<CVPixelBuffer>,
    base: *const u8,
    len: usize,
}

// SAFETY: the buffer is retained and locked for the value's life;
// CoreVideo's lock count and CoreFoundation's reference count are
// thread-safe, so it may unlock and release on another thread.
unsafe impl Send for Locked {}

impl Pixels for Locked {
    fn bytes(&self) -> &[u8] {
        // SAFETY: locked; base..base + len = bytesPerRow * height bytes.
        unsafe { std::slice::from_raw_parts(self.base, self.len) }
    }
}

impl Drop for Locked {
    fn drop(&mut self) {
        unsafe { CVPixelBufferUnlockBaseAddress(&self.buffer, READ_ONLY) };
    }
}

const READ_ONLY: CVPixelBufferLockFlags = CVPixelBufferLockFlags(1);

/// The buffer lent as a BGRA frame (rows padded to `bytesPerRow`).
fn to_frame(buffer: CFRetained<CVPixelBuffer>, display: u32) -> Option<Frame> {
    if CVPixelBufferGetPixelFormatType(&buffer) != kCVPixelFormatType_32BGRA {
        return None;
    }
    if unsafe { CVPixelBufferLockBaseAddress(&buffer, READ_ONLY) } != 0 {
        return None;
    }
    let width = CVPixelBufferGetWidth(&buffer);
    let height = CVPixelBufferGetHeight(&buffer);
    let stride = CVPixelBufferGetBytesPerRow(&buffer);
    let base = CVPixelBufferGetBaseAddress(&buffer) as *const u8;
    let locked = Locked { len: stride * height, base, buffer };
    if base.is_null() || width == 0 || height == 0 || stride < width * 4 {
        return None;
    }
    Some(Frame::lent(display, width as u32, height as u32, stride, true, Box::new(locked)))
}
