//! ScreenCast's PipeWire side: one video source node per shared output or
//! window. A thread per stream captures over its own `ede-protocol`
//! connection at the stream's framerate (only while someone watches) and
//! wakes the PipeWire loop, which copies the frame into the next buffer.
//! Captures are drawn in software by the compositor for now, so this is
//! a copy per frame; dmabuf zero-copy is a follow-up.

use std::{
    cell::RefCell,
    collections::HashMap,
    rc::Rc,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc, Mutex,
    },
    time::{Duration, Instant},
};

use ede_protocol::{CaptureTarget, ErrorCode};
use pipewire as pw;
use pw::{
    properties::properties,
    spa::{
        self,
        param::{
            format::{FormatProperties, MediaSubtype, MediaType},
            video::{VideoFormat, VideoInfoRaw},
            ParamType,
        },
        pod::{serialize::PodSerializer, ChoiceValue, Object, Pod, Property, PropertyFlags, Value},
        utils::{Choice, ChoiceEnum, ChoiceFlags},
        utils::{Fraction, Rectangle, SpaTypes},
    },
    stream::{StreamFlags, StreamRc, StreamState},
};

use crate::Connect;

struct Frame {
    width: u32,
    height: u32,
    pixels: Vec<u8>,
}

enum Cmd {
    Create { key: u64, target: CaptureTarget, size: (u32, u32), fps: u32, reply: mpsc::Sender<Result<u32, String>> },
    Frame(u64),
    Ended(u64),
    Destroy(u64),
}

/// The PipeWire thread.
pub struct Pw {
    tx: pw::channel::Sender<Cmd>,
}

struct Entry {
    stream: StreamRc,
    _listener: pw::stream::StreamListener<Data>,
    stop: Arc<AtomicBool>,
}

struct Data {
    latest: Arc<Mutex<Option<Frame>>>,
    size: (u32, u32),
    fps: u32,
    reply: Option<mpsc::Sender<Result<u32, String>>>,
    streaming: Arc<AtomicBool>,
}

impl Pw {
    /// Connects to PipeWire on a new thread. `on_end` runs (on that thread)
    /// when a stream's source went away (its window closed).
    pub fn start(connect: Connect, on_end: Arc<dyn Fn(u64) + Send + Sync>) -> Result<Pw, String> {
        let (tx, rx) = pw::channel::channel::<Cmd>();
        let (ready_tx, ready_rx) = mpsc::channel();
        let own_tx = tx.clone();
        std::thread::Builder::new()
            .name("ede-portal-pipewire".into())
            .spawn(move || {
                pw::init();
                let setup = (|| {
                    let mainloop = pw::main_loop::MainLoopRc::new(None)?;
                    let context = pw::context::ContextRc::new(&mainloop, None)?;
                    let core = context.connect_rc(None)?;
                    Ok::<_, pw::Error>((mainloop, context, core))
                })();
                let (mainloop, _context, core) = match setup {
                    Ok(s) => s,
                    Err(e) => {
                        ready_tx.send(Err(format!("PipeWire: {e}"))).ok();
                        return;
                    }
                };
                let streams: Rc<RefCell<HashMap<u64, Entry>>> = Rc::default();
                let _attached = rx.attach(mainloop.loop_(), move |cmd| match cmd {
                    Cmd::Create { key, target, size, fps, reply } => {
                        match create(&core, key, target, size, fps, reply.clone(), connect.clone(), own_tx.clone()) {
                            Ok(entry) => {
                                streams.borrow_mut().insert(key, entry);
                            }
                            Err(e) => {
                                reply.send(Err(e)).ok();
                            }
                        }
                    }
                    Cmd::Frame(key) => {
                        if let Some(e) = streams.borrow().get(&key) {
                            let _ = e.stream.trigger_process();
                        }
                    }
                    Cmd::Ended(key) => {
                        if let Some(e) = streams.borrow_mut().remove(&key) {
                            e.stop.store(true, Ordering::Relaxed);
                            let _ = e.stream.disconnect();
                            on_end(key);
                        }
                    }
                    Cmd::Destroy(key) => {
                        if let Some(e) = streams.borrow_mut().remove(&key) {
                            e.stop.store(true, Ordering::Relaxed);
                            let _ = e.stream.disconnect();
                        }
                    }
                });
                ready_tx.send(Ok(())).ok();
                mainloop.run();
            })
            .map_err(|e| e.to_string())?;
        ready_rx.recv_timeout(Duration::from_secs(10)).map_err(|_| "PipeWire did not start".to_string())??;
        Ok(Pw { tx })
    }

    /// A new source node for `target`; returns its node id.
    pub fn create(&self, key: u64, target: CaptureTarget, size: (u32, u32), fps: u32) -> Result<u32, String> {
        let (reply, answer) = mpsc::channel();
        self.tx.send(Cmd::Create { key, target, size, fps, reply }).map_err(|_| "the PipeWire thread is gone".to_string())?;
        answer.recv_timeout(Duration::from_secs(10)).map_err(|_| "PipeWire did not set up the stream".to_string())?
    }

    pub fn destroy(&self, key: u64) {
        let _ = self.tx.send(Cmd::Destroy(key));
    }
}

#[allow(clippy::too_many_arguments)]
fn create(
    core: &pw::core::CoreRc,
    key: u64,
    target: CaptureTarget,
    size: (u32, u32),
    fps: u32,
    reply: mpsc::Sender<Result<u32, String>>,
    connect: Connect,
    tx: pw::channel::Sender<Cmd>,
) -> Result<Entry, String> {
    let fps = fps.clamp(1, 240);
    let stream = StreamRc::new(
        core.clone(),
        "ede-screencast",
        properties! {
            *pw::keys::MEDIA_CLASS => "Video/Source",
            *pw::keys::MEDIA_TYPE => "Video",
            *pw::keys::MEDIA_CATEGORY => "Capture",
            *pw::keys::MEDIA_ROLE => "Screen",
            *pw::keys::NODE_NAME => "ede-screencast",
        },
    )
    .map_err(|e| e.to_string())?;
    let latest = Arc::new(Mutex::new(None));
    let streaming = Arc::new(AtomicBool::new(false));
    let data = Data { latest: latest.clone(), size, fps, reply: Some(reply), streaming: streaming.clone() };
    let listener = stream
        .add_local_listener_with_user_data(data)
        .state_changed(|stream, data, old, new| {
            tracing::debug!("stream {} {old:?} -> {new:?}", stream.node_id());
            data.streaming.store(matches!(new, StreamState::Streaming), Ordering::Relaxed);
            match new {
                StreamState::Paused | StreamState::Streaming => {
                    if let Some(reply) = data.reply.take() {
                        reply.send(Ok(stream.node_id())).ok();
                    }
                }
                StreamState::Error(e) => {
                    if let Some(reply) = data.reply.take() {
                        reply.send(Err(format!("PipeWire stream: {e}"))).ok();
                    }
                }
                _ => {}
            }
        })
        .param_changed(|stream, data, id, param| {
            let Some(param) = param else { return };
            if id != ParamType::Format.as_raw() {
                return;
            }
            let mut info = VideoInfoRaw::default();
            if info.parse(param).is_err() {
                return;
            }
            data.size = (info.size().width, info.size().height);
            let buffers = buffers_pod(data.size.0 * 4, data.size.0 * 4 * data.size.1);
            let _ = stream.update_params(&mut [Pod::from_bytes(&buffers).unwrap()]);
        })
        .process(|stream, data| {
            let Some(frame) = data.latest.lock().unwrap().take() else { return };
            if (frame.width, frame.height) != data.size {
                // The window was resized: offer the new size, skip this one.
                let format = format_pod(frame.width, frame.height, data.fps);
                let _ = stream.update_params(&mut [Pod::from_bytes(&format).unwrap()]);
                data.size = (frame.width, frame.height);
                return;
            }
            let Some(mut buffer) = stream.dequeue_buffer() else { return };
            let datas = buffer.datas_mut();
            let Some(d) = datas.first_mut() else { return };
            let stride = frame.width as usize * 4;
            let Some(out) = d.data() else { return };
            let n = (stride * frame.height as usize).min(out.len());
            out[..n].copy_from_slice(&frame.pixels[..n]);
            let chunk = d.chunk_mut();
            *chunk.offset_mut() = 0;
            *chunk.stride_mut() = stride as i32;
            *chunk.size_mut() = n as u32;
        })
        .register()
        .map_err(|e| e.to_string())?;

    let format = format_pod(size.0, size.1, fps);
    stream
        .connect(
            spa::utils::Direction::Output,
            None,
            StreamFlags::DRIVER | StreamFlags::MAP_BUFFERS,
            &mut [Pod::from_bytes(&format).unwrap()],
        )
        .map_err(|e| e.to_string())?;

    let stop = Arc::new(AtomicBool::new(false));
    let thread_stop = stop.clone();
    std::thread::Builder::new()
        .name("ede-portal-capture".into())
        .spawn(move || capture_loop(connect, target, fps, key, latest, streaming, thread_stop, tx))
        .map_err(|e| e.to_string())?;
    Ok(Entry { stream, _listener: listener, stop })
}

#[allow(clippy::too_many_arguments)]
fn capture_loop(
    connect: Connect,
    target: CaptureTarget,
    fps: u32,
    key: u64,
    latest: Arc<Mutex<Option<Frame>>>,
    streaming: Arc<AtomicBool>,
    stop: Arc<AtomicBool>,
    tx: pw::channel::Sender<Cmd>,
) {
    let period = Duration::from_secs(1) / fps;
    let mut client = None;
    while !stop.load(Ordering::Relaxed) {
        let started = Instant::now();
        if streaming.load(Ordering::Relaxed) {
            if client.is_none() {
                client = connect().ok();
            }
            if let Some(c) = client.as_mut() {
                match c.capture(target.clone()) {
                    Ok(capture) => {
                        let (w, h) = (capture.info.width as u32, capture.info.height as u32);
                        *latest.lock().unwrap() = Some(Frame { width: w, height: h, pixels: capture.pixels });
                        let _ = tx.send(Cmd::Frame(key));
                    }
                    Err(ede_protocol::Error::Failed(f)) if f.code == ErrorCode::NotFound => {
                        let _ = tx.send(Cmd::Ended(key));
                        return;
                    }
                    // Locked, or the window became protected: no frame.
                    Err(ede_protocol::Error::Failed(f)) => tracing::debug!("capture refused: {f}"),
                    Err(e) => {
                        tracing::debug!("capture: {e}");
                        client = None;
                    }
                }
            }
        }
        std::thread::sleep(period.saturating_sub(started.elapsed()));
    }
}

fn serialize(object: Object) -> Vec<u8> {
    PodSerializer::serialize(std::io::Cursor::new(Vec::new()), &Value::Object(object)).unwrap().0.into_inner()
}

/// BGRx (the little-endian `argb8888` words a capture carries) at one size,
/// variable framerate up to `fps`.
fn format_pod(width: u32, height: u32, fps: u32) -> Vec<u8> {
    serialize(spa::pod::object!(
        SpaTypes::ObjectParamFormat,
        ParamType::EnumFormat,
        spa::pod::property!(FormatProperties::MediaType, Id, MediaType::Video),
        spa::pod::property!(FormatProperties::MediaSubtype, Id, MediaSubtype::Raw),
        spa::pod::property!(FormatProperties::VideoFormat, Id, VideoFormat::BGRx),
        spa::pod::property!(FormatProperties::VideoSize, Rectangle, Rectangle { width, height }),
        spa::pod::property!(FormatProperties::VideoFramerate, Fraction, Fraction { num: 0, denom: 1 }),
        spa::pod::property!(
            FormatProperties::VideoMaxFramerate,
            Choice,
            Range,
            Fraction,
            Fraction { num: fps, denom: 1 },
            Fraction { num: 1, denom: 1 },
            Fraction { num: fps, denom: 1 }
        ),
    ))
}

fn buffers_pod(stride: u32, size: u32) -> Vec<u8> {
    let int = |key: u32, v: i32| Property { key, flags: PropertyFlags::empty(), value: Value::Int(v) };
    serialize(Object {
        type_: spa::sys::SPA_TYPE_OBJECT_ParamBuffers,
        id: spa::sys::SPA_PARAM_Buffers,
        properties: vec![
            int(spa::sys::SPA_PARAM_BUFFERS_buffers, 4),
            int(spa::sys::SPA_PARAM_BUFFERS_blocks, 1),
            int(spa::sys::SPA_PARAM_BUFFERS_size, size as i32),
            int(spa::sys::SPA_PARAM_BUFFERS_stride, stride as i32),
            Property {
                key: spa::sys::SPA_PARAM_BUFFERS_dataType,
                flags: PropertyFlags::empty(),
                value: Value::Choice(ChoiceValue::Int(Choice(
                    ChoiceFlags::empty(),
                    ChoiceEnum::Flags { default: 1 << spa::sys::SPA_DATA_MemFd, flags: vec![1 << spa::sys::SPA_DATA_MemFd] },
                ))),
            },
        ],
    })
}
