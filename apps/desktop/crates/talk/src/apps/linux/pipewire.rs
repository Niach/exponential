//! Per-app capture through PipeWire: every `Stream/Output/Audio` node is
//! bound for its full properties (the registry's global properties carry
//! neither `application.process.binary` nor the pid), and each wanted app
//! node gets its own capture stream TARGETED at it (`target.object` = the
//! node's serial; `node.dont-reconnect` + `node.dont-fallback` so a vanished
//! target never falls back to the microphone). The graph converts to mono
//! f32 at 16 kHz; [`pcm::Converter`] covers any other negotiated format.
//! Nodes come and go with the registry: apps join as they start playing
//! and leave when their last stream goes.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::mpsc;
use std::time::Duration;

use pipewire as pw;
use pw::spa;
use pw::spa::param::format::{MediaSubtype, MediaType};
use pw::spa::param::ParamType;
use pw::spa::pod::Pod;
use pw::stream::{StreamFlags, StreamState};
use pw::types::ObjectType;

use super::super::{AppCapture, AudioApp, Chunk, Selection, SAMPLE_RATE};
use super::nodes::{self, Own, STREAM_OUTPUT};
use super::pcm;

fn failed(what: &'static str) -> impl Fn(pw::Error) -> String {
    move |error| format!("PipeWire {what}: {error}")
}

/// A PipeWire daemon answers (connecting is cheap and prompts nothing).
pub(super) fn available() -> bool {
    pw::init();
    let Ok(mainloop) = pw::main_loop::MainLoopRc::new(None) else { return false };
    let Ok(context) = pw::context::ContextRc::new(&mainloop, None) else { return false };
    context.connect_rc(None).is_ok()
}

/// The apps with a playback stream now: one registry roundtrip binds every
/// playback node, a second one collects their info.
pub(super) fn live() -> Result<Vec<AudioApp>, String> {
    pw::init();
    let mainloop = pw::main_loop::MainLoopRc::new(None).map_err(failed("main loop"))?;
    let context = pw::context::ContextRc::new(&mainloop, None).map_err(failed("context"))?;
    let core = context.connect_rc(None).map_err(failed("connection"))?;
    let registry = core.get_registry_rc().map_err(failed("registry"))?;
    let own = Own::current();

    let found: Rc<RefCell<Vec<AudioApp>>> = Rc::default();
    let bound: Rc<RefCell<Vec<(pw::node::Node, pw::node::NodeListener)>>> = Rc::default();
    let weak_registry = registry.downgrade();
    let _registry_listener = registry
        .add_listener_local()
        .global({
            let found = found.clone();
            let bound = bound.clone();
            move |global| {
                if !is_playback(global) {
                    return;
                }
                let Some(registry) = weak_registry.upgrade() else { return };
                let Ok(node) = registry.bind::<pw::node::Node, _>(global) else { return };
                let found = found.clone();
                let own = own.clone();
                let listener = node
                    .add_listener_local()
                    .info(move |info| {
                        let Some(props) = info.props() else { return };
                        if let Some(app) = nodes::app_of(|key| props.get(key), &own) {
                            found.borrow_mut().push(app);
                        }
                    })
                    .register();
                bound.borrow_mut().push((node, listener));
            }
        })
        .register();

    let pending = Rc::new(Cell::new(core.sync(0).map_err(failed("sync"))?));
    let second = Rc::new(Cell::new(false));
    let weak_loop = mainloop.downgrade();
    let _core_listener = core
        .add_listener_local()
        .done({
            let core = core.downgrade();
            let weak_loop = mainloop.downgrade();
            move |id, seq| {
                if id != pw::core::PW_ID_CORE || seq != pending.get() {
                    return;
                }
                if !second.get() {
                    // Every playback node is bound now; one more roundtrip
                    // and their info events are in.
                    second.set(true);
                    if let Some(next) = core.upgrade().and_then(|core| core.sync(0).ok()) {
                        pending.set(next);
                        return;
                    }
                }
                if let Some(mainloop) = weak_loop.upgrade() {
                    mainloop.quit();
                }
            }
        })
        .register();
    let timeout = mainloop.loop_().add_timer(move |_| {
        if let Some(mainloop) = weak_loop.upgrade() {
            mainloop.quit();
        }
    });
    let _ = timeout.update_timer(Some(Duration::from_secs(2)), None);
    mainloop.run();

    let apps = std::mem::take(&mut *found.borrow_mut());
    Ok(nodes::group(apps))
}

fn is_playback(global: &pw::registry::GlobalObject<&spa::utils::dict::DictRef>) -> bool {
    global.type_ == ObjectType::Node && global.props.and_then(|props| props.get("media.class")) == Some(STREAM_OUTPUT)
}

/// Stops the capture thread (and joins it) when the [`AppCapture`] drops.
struct Guard {
    quit: pw::channel::Sender<()>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl Drop for Guard {
    fn drop(&mut self) {
        let _ = self.quit.send(());
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

pub(super) fn open(selection: Selection) -> Result<AppCapture, String> {
    let (tx, rx) = mpsc::channel();
    let (quit, quit_rx) = pw::channel::channel::<()>();
    let (ready_tx, ready_rx) = mpsc::channel();
    let thread = std::thread::Builder::new()
        .name("talk-apps-pipewire".to_string())
        .spawn(move || {
            if let Err(error) = run(selection, tx, quit_rx, &ready_tx) {
                log::warn!("[talk] PipeWire app capture: {error}");
                let _ = ready_tx.send(Err(error));
            }
        })
        .map_err(|error| format!("PipeWire thread: {error}"))?;
    let guard = Guard { quit, thread: Some(thread) };
    match ready_rx.recv_timeout(Duration::from_secs(5)) {
        Ok(Ok(())) => Ok(AppCapture::new(rx, Box::new(guard))),
        Ok(Err(error)) => Err(error),
        Err(_) => Err("PipeWire did not answer.".to_string()),
    }
}

/// One capture stream's state (the stream listener's user data).
struct StreamData {
    app: String,
    label: String,
    node: u32,
    converter: Option<pcm::Converter>,
    mixers: Rc<RefCell<pcm::Mixers>>,
    tx: mpsc::Sender<Chunk>,
}

struct Capture {
    app: String,
    // The listener first: it unhooks from the stream before the stream goes.
    _listener: pw::stream::StreamListener<StreamData>,
    stream: pw::stream::StreamRc,
}

struct State {
    core: pw::core::CoreRc,
    selection: Selection,
    own: Own,
    tx: mpsc::Sender<Chunk>,
    mixers: Rc<RefCell<pcm::Mixers>>,
    /// Every bound playback node (kept until it leaves the registry).
    bound: RefCell<HashMap<u32, (pw::node::Node, pw::node::NodeListener)>>,
    captures: RefCell<HashMap<u32, Capture>>,
}

fn run(
    selection: Selection,
    tx: mpsc::Sender<Chunk>,
    quit: pw::channel::Receiver<()>,
    ready: &mpsc::Sender<Result<(), String>>,
) -> Result<(), String> {
    pw::init();
    let mainloop = pw::main_loop::MainLoopRc::new(None).map_err(failed("main loop"))?;
    let context = pw::context::ContextRc::new(&mainloop, None).map_err(failed("context"))?;
    let core = context.connect_rc(None).map_err(failed("connection"))?;
    let _core_listener = core
        .add_listener_local()
        .error(|id, seq, res, message| log::debug!("[talk] PipeWire error on {id} (seq {seq}, {res}): {message}"))
        .register();
    let registry = core.get_registry_rc().map_err(failed("registry"))?;
    let state = Rc::new(State {
        core: core.clone(),
        selection,
        own: Own::current(),
        tx,
        mixers: Rc::default(),
        bound: RefCell::default(),
        captures: RefCell::default(),
    });

    let weak_registry = registry.downgrade();
    let weak_state = Rc::downgrade(&state);
    let _registry_listener = registry
        .add_listener_local()
        .global({
            let weak_state = weak_state.clone();
            move |global| {
                if !is_playback(global) {
                    return;
                }
                let (Some(registry), Some(state)) = (weak_registry.upgrade(), weak_state.upgrade()) else { return };
                let Ok(node) = registry.bind::<pw::node::Node, _>(global) else { return };
                let id = global.id;
                let weak_state = weak_state.clone();
                let listener = node
                    .add_listener_local()
                    .info(move |info| {
                        if let (Some(state), Some(props)) = (weak_state.upgrade(), info.props()) {
                            state.node_info(id, props);
                        }
                    })
                    .register();
                state.bound.borrow_mut().insert(id, (node, listener));
            }
        })
        .global_remove(move |id| {
            if let Some(state) = weak_state.upgrade() {
                state.node_removed(id);
            }
        })
        .register();

    let weak_loop = mainloop.downgrade();
    let _quit = quit.attach(mainloop.loop_(), move |()| {
        if let Some(mainloop) = weak_loop.upgrade() {
            mainloop.quit();
        }
    });
    let _ = ready.send(Ok(()));
    mainloop.run();

    let captures: Vec<Capture> = state.captures.borrow_mut().drain().map(|(_, capture)| capture).collect();
    for capture in &captures {
        let _ = capture.stream.disconnect();
    }
    drop(captures);
    state.bound.borrow_mut().clear();
    Ok(())
}

impl State {
    /// A playback node's properties arrived: capture it if its app is
    /// listened to (once per node).
    fn node_info(&self, id: u32, props: &spa::utils::dict::DictRef) {
        if self.captures.borrow().contains_key(&id) {
            return;
        }
        let Some(app) = nodes::app_of(|key| props.get(key), &self.own) else { return };
        if !self.selection.wants(&app.id) {
            return;
        }
        let target = props.get("object.serial").map(str::to_string).unwrap_or_else(|| id.to_string());
        match self.capture(id, &app, &target) {
            Ok(capture) => {
                self.mixers.borrow_mut().add(&app.id, &app.name, id);
                self.captures.borrow_mut().insert(id, capture);
                log::info!("[talk] capturing {} (node {id})", app.id);
            }
            Err(error) => log::warn!("[talk] capture of {} (node {id}): {error}", app.id),
        }
    }

    fn node_removed(&self, id: u32) {
        let capture = self.captures.borrow_mut().remove(&id);
        if let Some(capture) = capture {
            let chunks = self.mixers.borrow_mut().remove(&capture.app, id);
            for chunk in chunks {
                let _ = self.tx.send(chunk);
            }
            let _ = capture.stream.disconnect();
            drop(capture);
        }
        let bound = self.bound.borrow_mut().remove(&id);
        drop(bound);
    }

    fn capture(&self, node: u32, app: &AudioApp, target: &str) -> Result<Capture, String> {
        let mut props = pw::properties::PropertiesBox::new();
        props.insert(*pw::keys::MEDIA_TYPE, "Audio");
        props.insert(*pw::keys::MEDIA_CATEGORY, "Capture");
        props.insert(*pw::keys::MEDIA_ROLE, "Communication");
        props.insert(*pw::keys::NODE_NAME, format!("exponential-meeting-{}", app.id));
        props.insert("target.object", target);
        props.insert("node.dont-reconnect", "true");
        props.insert("node.dont-fallback", "true");
        let stream = pw::stream::StreamRc::new(self.core.clone(), "exponential-meeting", props)
            .map_err(failed("stream"))?;
        let data = StreamData {
            app: app.id.clone(),
            label: app.name.clone(),
            node,
            converter: None,
            mixers: self.mixers.clone(),
            tx: self.tx.clone(),
        };
        let listener = stream
            .add_local_listener_with_user_data(data)
            .state_changed(|_, data, _old, new| {
                if let StreamState::Error(reason) = &new {
                    log::warn!("[talk] capture of {} failed: {reason}", data.app);
                }
            })
            .param_changed(|_, data, id, param| {
                let Some(param) = param else { return };
                if id != ParamType::Format.as_raw() {
                    return;
                }
                let Ok((media_type, media_subtype)) = spa::param::format_utils::parse_format(param) else { return };
                if media_type != MediaType::Audio || media_subtype != MediaSubtype::Raw {
                    return;
                }
                let mut info = spa::param::audio::AudioInfoRaw::new();
                if info.parse(param).is_ok() {
                    let (rate, channels) = (info.rate(), info.channels() as usize);
                    if !data.converter.as_ref().is_some_and(|c| c.matches(rate, channels)) {
                        data.converter = Some(pcm::Converter::new(rate, channels));
                    }
                }
            })
            .process(|stream, data| {
                let Some(mut buffer) = stream.dequeue_buffer() else { return };
                let datas = buffer.datas_mut();
                let Some(first) = datas.first_mut() else { return };
                let (offset, size) = (first.chunk().offset() as usize, first.chunk().size() as usize);
                let Some(bytes) = first.data() else { return };
                let start = offset.min(bytes.len());
                let end = (offset + size).min(bytes.len());
                let samples = pcm::f32_le(&bytes[start..end]);
                let converter = data.converter.get_or_insert_with(|| pcm::Converter::new(SAMPLE_RATE, 1));
                let mono = converter.convert(&samples);
                let chunks = data.mixers.borrow_mut().push(&data.app, &data.label, data.node, &mono);
                for chunk in chunks {
                    let _ = data.tx.send(chunk);
                }
            })
            .register()
            .map_err(failed("stream listener"))?;
        let values = format_pod()?;
        let pod = Pod::from_bytes(&values).ok_or("PipeWire format: not a pod")?;
        stream
            .connect(
                spa::utils::Direction::Input,
                None,
                StreamFlags::AUTOCONNECT | StreamFlags::MAP_BUFFERS,
                &mut [pod],
            )
            .map_err(failed("stream connection"))?;
        Ok(Capture { app: app.id.clone(), _listener: listener, stream })
    }
}

/// The format we ask for: mono f32 at 16 kHz (the stream's adapter
/// converts whatever the app plays).
fn format_pod() -> Result<Vec<u8>, String> {
    let mut info = spa::param::audio::AudioInfoRaw::new();
    info.set_format(spa::param::audio::AudioFormat::F32LE);
    info.set_rate(SAMPLE_RATE);
    info.set_channels(1);
    let mut position = [0u32; 64];
    position[0] = spa::sys::SPA_AUDIO_CHANNEL_MONO;
    info.set_position(position);
    let object = spa::pod::Object {
        type_: spa::utils::SpaTypes::ObjectParamFormat.as_raw(),
        id: ParamType::EnumFormat.as_raw(),
        properties: info.into(),
    };
    let (cursor, _) =
        spa::pod::serialize::PodSerializer::serialize(std::io::Cursor::new(Vec::new()), &spa::pod::Value::Object(object))
            .map_err(|error| format!("PipeWire format: {error:?}"))?;
    Ok(cursor.into_inner())
}
