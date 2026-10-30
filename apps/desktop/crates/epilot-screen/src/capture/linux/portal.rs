//! The xdg ScreenCast portal + PipeWire half of the Linux capturer.
//!
//! ONE portal session selects every monitor (`multiple`, monitors only,
//! `persist_mode` 2). The portal hands back a single-use restore token with
//! every start; it lives in `<state_dir>/screencast-restore-token` (0600)
//! and goes into the next start, so the dialog shows once.
//!
//! PipeWire runs on its own thread (`screen-pipewire`): one stream per
//! portal stream on the portal's remote. A stream's `process` keeps ONLY the
//! newest buffer dequeued (the older one goes straight back to the
//! producer), so frames between grabs cost nothing; a grab asks the thread
//! over a `pipewire::channel` and the thread converts the held buffers of
//! the asked streams to RGBA (by the negotiated SPA format) and answers.
//! Producers deliver on damage only, so a static screen costs nothing.
//!
//! `grab_all` hands out each display's latest frame ONCE (a display without
//! a new frame since the last grab is left out); `grab(display)` returns the
//! latest regardless (the focus-change capture). When every stream has
//! ended (the person stopped sharing from the system's indicator) the grabs
//! return `Err`.

use std::cell::RefCell;
use std::fs;
use std::io::Write as _;
use std::os::fd::OwnedFd;
use std::os::unix::fs::OpenOptionsExt as _;
use std::path::Path;
use std::rc::Rc;
use std::sync::mpsc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use ashpd::desktop::screencast::{CursorMode, Screencast, SelectSourcesOptions, SourceType};
use ashpd::desktop::{PersistMode, ResponseError, Session};
use ashpd::enumflags2::BitFlags;
use pipewire as pw;
use pw::spa;
use pw::spa::param::format::{FormatProperties, MediaSubtype, MediaType};
use pw::spa::param::video::{VideoFormat, VideoInfoRaw};
use pw::spa::param::ParamType;
use pw::spa::pod::Pod;
use pw::stream::{StreamFlags, StreamState};

use super::{DisplayCache, StreamRect};
use crate::capture::{Capturer, Frame, Want};

const TOKEN_FILE: &str = "screencast-restore-token";
/// How long `open` waits for the PipeWire streams to be set up.
const SETUP_TIMEOUT: Duration = Duration::from_secs(10);
/// How long a grab waits for the PipeWire thread.
const GRAB_TIMEOUT: Duration = Duration::from_secs(3);

/// To the PipeWire thread.
enum Message {
    /// Convert the held buffer of each `(stream index, seq seen, since)`
    /// whose seq is newer than the one seen and that arrived no earlier than
    /// `since` (STALE); answer one [`Grabbed`] per entry.
    Grab { wanted: Vec<(usize, u64, Option<Instant>)>, reply: mpsc::Sender<Vec<Grabbed>> },
    Quit,
}

/// One stream's answer.
struct Grabbed {
    ended: bool,
    seq: u64,
    /// STAMP: when the held buffer arrived (Unix ms).
    at: i64,
    image: Option<(u32, u32, Vec<u8>)>,
}

pub(super) struct Portal {
    // Kept alive: dropping the D-Bus connection closes the portal session.
    _proxy: Screencast,
    session: Session<Screencast>,
    rects: Vec<StreamRect>,
    numbers: Vec<u32>,
    mapped_from: Option<Vec<epilot::Display>>,
    displays: DisplayCache,
    /// Per stream: the seq `grab_all` last handed out.
    seen: Vec<u64>,
    sender: pw::channel::Sender<Message>,
    thread: Option<JoinHandle<()>>,
}

impl Portal {
    /// Start the portal session (the dialog, unless the restore token still
    /// holds) and the PipeWire streams. Blocks until the person answers.
    pub(super) fn open(state_dir: &Path) -> Result<Portal, String> {
        let token_path = state_dir.join(TOKEN_FILE);
        let token = fs::read_to_string(&token_path).ok().map(|token| token.trim().to_string()).filter(|token| !token.is_empty());
        let started = async_io::block_on(start(token.as_deref()))?;
        let Started { proxy, session, streams, token, fd } = started;
        save_token(&token_path, token.as_deref());
        if streams.is_empty() {
            close(&session);
            return Err("the portal shared no monitor".to_string());
        }
        let nodes: Vec<(u32, Option<(i32, i32)>)> = streams.iter().map(|stream| (stream.0, stream.1.size)).collect();
        let rects: Vec<StreamRect> = streams.iter().map(|stream| stream.1).collect();
        let (sender, receiver) = pw::channel::channel::<Message>();
        let (ready_tx, ready_rx) = mpsc::channel();
        let thread = std::thread::Builder::new()
            .name("screen-pipewire".to_string())
            .spawn(move || {
                if let Err(reason) = run(fd, nodes, receiver, &ready_tx) {
                    let _ = ready_tx.send(Err(reason));
                }
            })
            .map_err(|error| format!("PipeWire thread: {error}"))?;
        let ready = ready_rx.recv_timeout(SETUP_TIMEOUT).unwrap_or_else(|_| Err("PipeWire did not set up the streams".to_string()));
        if let Err(reason) = ready {
            let _ = sender.send(Message::Quit);
            close(&session);
            return Err(reason);
        }
        log::info!("[screen] ScreenCast portal: {} monitor stream(s)", rects.len());
        let count = rects.len();
        Ok(Portal {
            _proxy: proxy,
            session,
            rects,
            numbers: Vec::new(),
            mapped_from: None,
            displays: DisplayCache::default(),
            seen: vec![0; count],
            sender,
            thread: Some(thread),
        })
    }

    /// Re-number the streams when the display set changed.
    fn refresh_numbers(&mut self) {
        let displays = self.displays.get();
        if self.mapped_from.as_deref() != Some(displays) {
            self.numbers = super::number_streams(&self.rects, displays);
            self.mapped_from = Some(displays.to_vec());
        }
    }

    fn ask(&self, wanted: Vec<(usize, u64, Option<Instant>)>) -> Result<Vec<Grabbed>, String> {
        let (reply, answer) = mpsc::channel();
        if self.sender.send(Message::Grab { wanted, reply }).is_err() {
            return Err("The PipeWire thread is gone.".to_string());
        }
        answer.recv_timeout(GRAB_TIMEOUT).map_err(|_| "The PipeWire thread did not answer.".to_string())
    }
}

const ENDED: &str = "The screen sharing session ended.";

impl Capturer for Portal {
    fn grab_all(&mut self, wanted: &[Want]) -> Result<Vec<Frame>, String> {
        self.refresh_numbers();
        // Every stream is asked (an all-ended answer is the session's end);
        // only the wanted ones are converted.
        let asks = (0..self.seen.len())
            .map(|index| match wanted.iter().find(|want| self.numbers.get(index) == Some(&want.display)) {
                Some(want) => (index, self.seen[index], want.since),
                None => (index, u64::MAX, None),
            })
            .collect();
        let answers = self.ask(asks)?;
        if answers.iter().all(|answer| answer.ended) {
            return Err(ENDED.to_string());
        }
        let mut frames = Vec::new();
        for (index, answer) in answers.into_iter().enumerate() {
            let Some((width, height, rgba)) = answer.image else { continue };
            self.seen[index] = answer.seq;
            frames.push(Frame::rgba(self.numbers[index], width, height, rgba).stamped(answer.at));
        }
        Ok(frames)
    }

    fn grab(&mut self, wanted: Want) -> Result<Option<Frame>, String> {
        self.refresh_numbers();
        let Some(index) = self.numbers.iter().position(|&number| number == wanted.display) else {
            return Ok(None);
        };
        let mut answers = self.ask(vec![(index, 0, wanted.since)])?;
        let Some(answer) = answers.pop() else { return Ok(None) };
        let Some((width, height, rgba)) = answer.image else {
            return if answer.ended { Err(ENDED.to_string()) } else { Ok(None) };
        };
        self.seen[index] = answer.seq;
        Ok(Some(Frame::rgba(wanted.display, width, height, rgba).stamped(answer.at)))
    }

    /// FRESH: PipeWire too delivers on damage only, so a still screen after
    /// a resume is read live where X11 allows it (the epilot crate's root
    /// read); elsewhere (Wayland, where screen memory never captures) the
    /// next delivery counts.
    fn fresh(&mut self, display: u32) -> Result<Option<Frame>, String> {
        if !epilot::visible::window_list_supported() {
            return Ok(None);
        }
        let (width, height, rgba) = epilot::visible::screenshot(display)?;
        Ok(Some(Frame::rgba(display, width, height, rgba).stamped(crate::capture::wall_ms())))
    }
}

impl Drop for Portal {
    fn drop(&mut self) {
        let _ = self.sender.send(Message::Quit);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
        close(&self.session);
    }
}

fn close(session: &Session<Screencast>) {
    if let Err(error) = async_io::block_on(session.close()) {
        log::debug!("[screen] portal session not closed: {error}");
    }
}

/// The single-use restore token for the next start: written (0600) when the
/// portal returned one, removed when it did not.
fn save_token(path: &Path, token: Option<&str>) {
    let Some(token) = token else {
        let _ = fs::remove_file(path);
        return;
    };
    if let Some(dir) = path.parent() {
        let _ = fs::create_dir_all(dir);
    }
    let written = fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(path)
        .and_then(|mut file| file.write_all(token.as_bytes()));
    if let Err(error) = written {
        log::warn!("[screen] restore token not saved ({error}); the portal will ask again next time");
    }
}

struct Started {
    proxy: Screencast,
    session: Session<Screencast>,
    /// (PipeWire node id, the portal's rect) per stream, the portal's order.
    streams: Vec<(u32, StreamRect)>,
    token: Option<String>,
    fd: OwnedFd,
}

async fn start(token: Option<&str>) -> Result<Started, String> {
    let proxy = Screencast::new().await.map_err(|error| format!("no ScreenCast portal ({error})"))?;
    let sources = proxy.available_source_types().await.map_err(|error| format!("ScreenCast portal: {error}"))?;
    if !sources.contains(SourceType::Monitor) {
        return Err("the ScreenCast portal cannot share monitors".to_string());
    }
    // Hidden only where the portal offers it (version 2+); older ones draw it.
    let cursor = match proxy.available_cursor_modes().await {
        Ok(modes) if modes.contains(CursorMode::Hidden) => Some(CursorMode::Hidden),
        _ => None,
    };
    let session = proxy.create_session(Default::default()).await.map_err(|error| format!("ScreenCast session: {error}"))?;
    let selected = async {
        proxy
            .select_sources(
                &session,
                SelectSourcesOptions::default()
                    .set_sources(BitFlags::from_flag(SourceType::Monitor))
                    .set_multiple(true)
                    .set_cursor_mode(cursor)
                    .set_persist_mode(PersistMode::ExplicitlyRevoked)
                    .set_restore_token(token),
            )
            .await?
            .response()?;
        let streams = proxy.start(&session, None, Default::default()).await?.response()?;
        let fd = proxy.open_pipe_wire_remote(&session, Default::default()).await?;
        Ok::<_, ashpd::Error>((streams, fd))
    }
    .await;
    let (response, fd) = match selected {
        Ok(selected) => selected,
        Err(error) => {
            let _ = session.close().await;
            return Err(match error {
                ashpd::Error::Response(ResponseError::Cancelled) => DECLINED.to_string(),
                error => format!("ScreenCast portal: {error}"),
            });
        }
    };
    let streams = response
        .streams()
        .iter()
        .filter(|stream| stream.source_type().is_none_or(|kind| kind == SourceType::Monitor))
        .map(|stream| (stream.pipe_wire_node_id(), StreamRect { position: stream.position(), size: stream.size() }))
        .collect();
    let token = response.restore_token().map(str::to_string);
    Ok(Started { proxy, session, streams, token, fd })
}

// ---- the PipeWire thread ----------------------------------------------------

/// One stream's state on the PipeWire thread.
#[derive(Default)]
struct Slot {
    format: Option<VideoInfoRaw>,
    /// The newest dequeued buffer, held back from the producer until a newer
    /// one arrives (or the producer removes it).
    held: Option<*mut pw::sys::pw_buffer>,
    /// Counts the buffers held: a grab compares it with what it saw.
    seq: u64,
    /// When `held` arrived (STALE), and as Unix ms (STAMP).
    delivered: Option<Instant>,
    at: i64,
    ended: bool,
}

fn run(
    fd: OwnedFd,
    nodes: Vec<(u32, Option<(i32, i32)>)>,
    receiver: pw::channel::Receiver<Message>,
    ready: &mpsc::Sender<Result<(), String>>,
) -> Result<(), String> {
    pw::init();
    let failed = |what: &'static str| move |error: pw::Error| format!("PipeWire {what}: {error}");
    let mainloop = pw::main_loop::MainLoopRc::new(None).map_err(failed("main loop"))?;
    let context = pw::context::ContextRc::new(&mainloop, None).map_err(failed("context"))?;
    let core = context.connect_fd_rc(fd, None).map_err(failed("connection"))?;
    let _core_listener = core
        .add_listener_local()
        .error(|id, seq, res, message| log::warn!("[screen] PipeWire error on {id} (seq {seq}, {res}): {message}"))
        .register();

    let mut streams = Vec::with_capacity(nodes.len());
    for (index, (node, size)) in nodes.into_iter().enumerate() {
        let slot = Rc::new(RefCell::new(Slot::default()));
        let stream = pw::stream::StreamRc::new(
            core.clone(),
            &format!("exponential-screen-{}", index + 1),
            pw::properties::properties! {
                *pw::keys::MEDIA_TYPE => "Video",
                *pw::keys::MEDIA_CATEGORY => "Capture",
                *pw::keys::MEDIA_ROLE => "Screen",
            },
        )
        .map_err(failed("stream"))?;
        let listener = stream
            .add_local_listener_with_user_data(slot.clone())
            .state_changed(move |_, slot, _old, new| {
                if let StreamState::Error(reason) = &new {
                    log::warn!("[screen] screen stream {} failed: {reason}", index + 1);
                }
                slot.borrow_mut().ended = matches!(new, StreamState::Error(_) | StreamState::Unconnected);
            })
            .param_changed(|_, slot, id, param| {
                let Some(param) = param else { return };
                if id != ParamType::Format.as_raw() {
                    return;
                }
                let Ok((media_type, media_subtype)) = spa::param::format_utils::parse_format(param) else { return };
                if media_type != MediaType::Video || media_subtype != MediaSubtype::Raw {
                    return;
                }
                let mut info = VideoInfoRaw::default();
                if info.parse(param).is_ok() {
                    slot.borrow_mut().format = Some(info);
                }
            })
            .process(|stream, slot| {
                // Drain to the newest buffer; every older one goes back.
                let mut newest: *mut pw::sys::pw_buffer = std::ptr::null_mut();
                loop {
                    // SAFETY: a buffer of this stream, returned exactly once below.
                    let buffer = unsafe { stream.dequeue_raw_buffer() };
                    if buffer.is_null() {
                        break;
                    }
                    if !newest.is_null() {
                        unsafe { stream.queue_raw_buffer(newest) };
                    }
                    newest = buffer;
                }
                if newest.is_null() {
                    return;
                }
                // SAFETY: `newest` is dequeued and not yet returned.
                if unsafe { pixels(newest) }.is_none() {
                    unsafe { stream.queue_raw_buffer(newest) };
                    return;
                }
                let mut slot = slot.borrow_mut();
                if let Some(old) = slot.held.replace(newest) {
                    unsafe { stream.queue_raw_buffer(old) };
                }
                slot.seq += 1;
                slot.delivered = Some(Instant::now());
                slot.at = crate::capture::wall_ms();
            })
            .remove_buffer(|_, slot, buffer| {
                let mut slot = slot.borrow_mut();
                if slot.held == Some(buffer) {
                    slot.held = None;
                }
            })
            .register()
            .map_err(failed("stream listener"))?;
        let values = format_pod(size)?;
        let pod = Pod::from_bytes(&values).ok_or("PipeWire format: not a pod")?;
        stream
            .connect(spa::utils::Direction::Input, Some(node), StreamFlags::AUTOCONNECT | StreamFlags::MAP_BUFFERS, &mut [pod])
            .map_err(failed("stream connection"))?;
        // The listener first: it unhooks from the stream before the stream goes.
        streams.push((listener, stream, slot));
    }

    let slots: Vec<Rc<RefCell<Slot>>> = streams.iter().map(|(_, _, slot)| slot.clone()).collect();
    let weak_loop = mainloop.downgrade();
    let _attached = receiver.attach(mainloop.loop_(), move |message| match message {
        Message::Quit => {
            if let Some(mainloop) = weak_loop.upgrade() {
                mainloop.quit();
            }
        }
        Message::Grab { wanted, reply } => {
            let answers = wanted
                .into_iter()
                .map(|(index, seen, since)| match slots.get(index) {
                    Some(slot) => grab(&slot.borrow(), seen, since),
                    None => Grabbed { ended: true, seq: 0, at: 0, image: None },
                })
                .collect();
            let _ = reply.send(answers);
        }
    });
    let _ = ready.send(Ok(()));
    mainloop.run();

    for (_, stream, slot) in &streams {
        if let Some(held) = slot.borrow_mut().held.take() {
            unsafe { stream.queue_raw_buffer(held) };
        }
        let _ = stream.disconnect();
    }
    Ok(())
}

/// The EnumFormat we accept: raw video in every packed 8-bit RGB layout
/// [`layout`] converts, any size (the portal's as the default), any rate.
fn format_pod(size: Option<(i32, i32)>) -> Result<Vec<u8>, String> {
    let (width, height) = size.filter(|&(width, height)| width > 0 && height > 0).unwrap_or((1920, 1080));
    let object = pw::spa::pod::object!(
        pw::spa::utils::SpaTypes::ObjectParamFormat,
        pw::spa::param::ParamType::EnumFormat,
        pw::spa::pod::property!(FormatProperties::MediaType, Id, MediaType::Video),
        pw::spa::pod::property!(FormatProperties::MediaSubtype, Id, MediaSubtype::Raw),
        pw::spa::pod::property!(
            FormatProperties::VideoFormat,
            Choice,
            Enum,
            Id,
            VideoFormat::BGRx,
            VideoFormat::BGRx,
            VideoFormat::BGRA,
            VideoFormat::RGBx,
            VideoFormat::RGBA,
            VideoFormat::xRGB,
            VideoFormat::ARGB,
            VideoFormat::xBGR,
            VideoFormat::ABGR,
            VideoFormat::RGB,
            VideoFormat::BGR,
        ),
        pw::spa::pod::property!(
            FormatProperties::VideoSize,
            Choice,
            Range,
            Rectangle,
            pw::spa::utils::Rectangle { width: width as u32, height: height as u32 },
            pw::spa::utils::Rectangle { width: 1, height: 1 },
            pw::spa::utils::Rectangle { width: 16384, height: 16384 }
        ),
        pw::spa::pod::property!(
            FormatProperties::VideoFramerate,
            Choice,
            Range,
            Fraction,
            pw::spa::utils::Fraction { num: 5, denom: 1 },
            pw::spa::utils::Fraction { num: 0, denom: 1 },
            pw::spa::utils::Fraction { num: 1000, denom: 1 }
        ),
    );
    let (cursor, _) = pw::spa::pod::serialize::PodSerializer::serialize(std::io::Cursor::new(Vec::new()), &pw::spa::pod::Value::Object(object))
        .map_err(|error| format!("PipeWire format: {error:?}"))?;
    Ok(cursor.into_inner())
}

/// The first data plane of a dequeued buffer when it carries pixels.
///
/// # Safety
/// `buffer` must be a buffer of a live stream, dequeued and not returned.
unsafe fn pixels<'a>(buffer: *mut pw::sys::pw_buffer) -> Option<&'a mut spa::buffer::Data> {
    let spa_buffer = unsafe { (*buffer).buffer };
    if spa_buffer.is_null() || unsafe { (*spa_buffer).n_datas } == 0 || unsafe { (*spa_buffer).datas }.is_null() {
        return None;
    }
    // `Data` is `repr(transparent)` over `spa_data` (pipewire's own cast).
    let data = unsafe { &mut *((*spa_buffer).datas as *mut spa::buffer::Data) };
    let chunk = data.chunk();
    if chunk.size() == 0 || chunk.flags().contains(spa::buffer::ChunkFlags::CORRUPTED) {
        return None;
    }
    data.data()?;
    Some(data)
}

/// A slot's held buffer as RGBA when its seq is newer than `seen` and it
/// arrived no earlier than `since` (STALE).
fn grab(slot: &Slot, seen: u64, since: Option<Instant>) -> Grabbed {
    let mut answer = Grabbed { ended: slot.ended, seq: slot.seq, at: slot.at, image: None };
    if slot.seq <= seen || slot.delivered.is_none_or(|delivered| since.is_some_and(|since| delivered < since)) {
        return answer;
    }
    let (Some(held), Some(format)) = (slot.held, slot.format) else { return answer };
    let Some(layout) = layout(format.format()) else {
        log::debug!("[screen] unsupported screen format {:?}", format.format());
        return answer;
    };
    // SAFETY: `held` stays dequeued (and mapped) until this thread returns it.
    let Some(data) = (unsafe { pixels(held) }) else { return answer };
    let (offset, stride) = (data.chunk().offset() as usize, data.chunk().stride());
    let size = format.size();
    let Some(bytes) = data.data() else { return answer };
    answer.image = to_rgba(bytes, layout, size.width as usize, size.height as usize, stride, offset)
        .map(|rgba| (size.width, size.height, rgba));
    answer
}

/// A packed 8-bit RGB layout: bytes per pixel and the R, G, B byte indexes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Layout {
    bytes: usize,
    rgb: (usize, usize, usize),
}

fn layout(format: VideoFormat) -> Option<Layout> {
    let (bytes, rgb) = match format {
        VideoFormat::BGRx | VideoFormat::BGRA => (4, (2, 1, 0)),
        VideoFormat::RGBx | VideoFormat::RGBA => (4, (0, 1, 2)),
        VideoFormat::xRGB | VideoFormat::ARGB => (4, (1, 2, 3)),
        VideoFormat::xBGR | VideoFormat::ABGR => (4, (3, 2, 1)),
        VideoFormat::RGB => (3, (0, 1, 2)),
        VideoFormat::BGR => (3, (2, 1, 0)),
        _ => return None,
    };
    Some(Layout { bytes, rgb })
}

/// `width` x `height` pixels of `layout` starting at `offset`, rows `stride`
/// bytes apart (0 = packed) → RGBA8, alpha opaque. `None` when the plane is
/// short or bottom-up.
fn to_rgba(bytes: &[u8], layout: Layout, width: usize, height: usize, stride: i32, offset: usize) -> Option<Vec<u8>> {
    let row = width.checked_mul(layout.bytes)?;
    let stride = match stride {
        0 => row,
        stride if stride > 0 && stride as usize >= row => stride as usize,
        _ => return None,
    };
    if width == 0 || height == 0 {
        return None;
    }
    let end = offset.checked_add(stride.checked_mul(height - 1)?)?.checked_add(row)?;
    if end > bytes.len() {
        return None;
    }
    let (r, g, b) = layout.rgb;
    let mut rgba = vec![255u8; width * height * 4];
    for (y, out_row) in rgba.chunks_exact_mut(width * 4).enumerate() {
        let start = offset + y * stride;
        let line = &bytes[start..start + row];
        for (out, pixel) in out_row.chunks_exact_mut(4).zip(line.chunks_exact(layout.bytes)) {
            out[0] = pixel[r];
            out[1] = pixel[g];
            out[2] = pixel[b];
        }
    }
    Some(rgba)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_offered_format_converts_to_rgba() {
        // One pixel R=1 G=2 B=3 in each layout, padding byte 9.
        let cases: [(VideoFormat, &[u8]); 10] = [
            (VideoFormat::BGRx, &[3, 2, 1, 9]),
            (VideoFormat::BGRA, &[3, 2, 1, 9]),
            (VideoFormat::RGBx, &[1, 2, 3, 9]),
            (VideoFormat::RGBA, &[1, 2, 3, 9]),
            (VideoFormat::xRGB, &[9, 1, 2, 3]),
            (VideoFormat::ARGB, &[9, 1, 2, 3]),
            (VideoFormat::xBGR, &[9, 3, 2, 1]),
            (VideoFormat::ABGR, &[9, 3, 2, 1]),
            (VideoFormat::RGB, &[1, 2, 3]),
            (VideoFormat::BGR, &[3, 2, 1]),
        ];
        for (format, pixel) in cases {
            let layout = layout(format).unwrap();
            assert_eq!(to_rgba(pixel, layout, 1, 1, 0, 0), Some(vec![1, 2, 3, 255]), "{format:?}");
        }
        assert_eq!(layout(VideoFormat::I420), None);
    }

    #[test]
    fn rows_honour_offset_and_stride_and_short_planes_are_refused() {
        let layout = layout(VideoFormat::RGBx).unwrap();
        // offset 2, two rows of one pixel, stride 6 (2 bytes padding).
        let bytes = [0, 0, 1, 2, 3, 0, 0, 0, 4, 5, 6, 0];
        assert_eq!(to_rgba(&bytes, layout, 1, 2, 6, 2), Some(vec![1, 2, 3, 255, 4, 5, 6, 255]));
        assert_eq!(to_rgba(&bytes[..9], layout, 1, 2, 6, 2), None);
        assert_eq!(to_rgba(&bytes, layout, 1, 2, -6, 2), None);
        assert_eq!(to_rgba(&bytes, layout, 1, 2, 3, 0), None);
    }
}

/// The error `open` returns when the person declined the portal dialog.
pub(super) const DECLINED: &str = "screen sharing was declined";
