//! NEW-57: Windows desktop audio (meeting.json CAPABILITY + CHANNELS).
//!
//! Build 20348+ = per_app: WASAPI process loopback
//! (`AUDIOCLIENT_ACTIVATION_TYPE_PROCESS_LOOPBACK`, the target's process
//! tree) of each app's ROOT process (the topmost ancestor running the same
//! exe); the apps are the active audio sessions (IAudioSessionManager2 →
//! pid → exe stem lowercased = the id, the file description = the name),
//! re-read every second: apps join as they start playing and leave 30 s
//! after their last active session (or when the process exits). Older
//! builds = system: plain WASAPI loopback of the default render device, ONE
//! `desktop` channel. No render device = none.

#[path = "pcm.rs"]
mod pcm;

mod procs;
mod tree;
mod wasapi;

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use super::{chunk_queue, now_ms, AppCapture, AudioApp, Capability, Chunk, ChunkSender, Selection, CAPTURE_QUEUE, DESKTOP_CHANNEL};

/// The first build with process loopback (meeting.json CAPABILITY).
const PROCESS_LOOPBACK_BUILD: u32 = 20_348;
/// The `desktop` channel's label (meeting.json `copy.desktop`).
const DESKTOP_LABEL: &str = "Desktop";
/// How often the active sessions are re-read while capturing.
const POLL: Duration = Duration::from_secs(1);
/// An app leaves this long after its last active session.
const LINGER: Duration = Duration::from_secs(30);

/// Runs `work` on a fresh thread with COM (multithreaded) initialized, so
/// the caller's apartment (the IDE's UI thread) never matters.
fn with_com<T: Send + 'static>(work: impl FnOnce() -> T + Send + 'static) -> Option<T> {
    std::thread::Builder::new()
        .name("talk-apps-com".to_string())
        .spawn(move || {
            let _com = wasapi::Com::init().ok()?;
            Some(work())
        })
        .ok()?
        .join()
        .ok()
        .flatten()
}

pub(super) fn capability() -> Capability {
    if procs::os_build() >= PROCESS_LOOPBACK_BUILD {
        return Capability::PerApp;
    }
    if with_com(wasapi::has_default_render).unwrap_or(false) {
        Capability::System
    } else {
        Capability::None
    }
}

pub(super) fn live() -> Vec<AudioApp> {
    with_com(|| {
        let pids = procs::active_session_pids().unwrap_or_default();
        let table = procs::table();
        let own = pcm::own_stem();
        let plan = tree::plan(&pids, &table, std::process::id(), own.as_deref(), &Selection::All);
        let mut apps: Vec<AudioApp> = plan
            .into_iter()
            .map(|(id, roots)| {
                let name = app_name(&id, roots.iter().next().copied());
                AudioApp { id, name }
            })
            .collect();
        apps.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()).then_with(|| a.id.cmp(&b.id)));
        apps
    })
    .unwrap_or_default()
}

/// An app's display name: its root exe's file description, else the id.
fn app_name(id: &str, pid: Option<u32>) -> String {
    let description = pid.and_then(procs::exe_path).and_then(|path| procs::description(&path));
    tree::display_name(description.as_deref(), id)
}

pub(super) fn open(selection: Selection) -> Result<AppCapture, String> {
    match capability() {
        Capability::PerApp => per_app(selection),
        // System loopback only: `include` / `exclude` behave as `all`.
        Capability::System => system(),
        Capability::None => Err("Desktop audio needs an output device.".to_string()),
    }
}

/// Stops a capture thread (and joins it) on drop.
struct Guard {
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl Drop for Guard {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn system() -> Result<AppCapture, String> {
    let (tx, rx) = chunk_queue(CAPTURE_QUEUE);
    let (ready_tx, ready_rx) = mpsc::channel();
    let stop = Arc::new(AtomicBool::new(false));
    let thread_stop = stop.clone();
    let thread = std::thread::Builder::new()
        .name("talk-apps-loopback".to_string())
        .spawn(move || {
            let _com = match wasapi::Com::init() {
                Ok(com) => com,
                Err(error) => {
                    let _ = ready_tx.send(Err(error));
                    return;
                }
            };
            let mut chunker = pcm::Chunker::default();
            let mut ready = Some(ready_tx);
            // The default device can change or vanish: reopen until stopped.
            while !thread_stop.load(Ordering::Relaxed) {
                match wasapi::Loopback::system() {
                    Ok(loopback) => {
                        if let Some(ready) = ready.take() {
                            let _ = ready.send(Ok(()));
                        }
                        let result = loopback.pump(&thread_stop, |samples| {
                            // Dated by this packet's wall clock (read as it is captured).
                            let captured_ms = now_ms();
                            chunker.push(&samples).into_iter().all(|samples| {
                                let chunk = Chunk {
                                    channel: DESKTOP_CHANNEL.to_string(),
                                    label: DESKTOP_LABEL.to_string(),
                                    samples,
                                    captured_ms,
                                };
                                tx.send(chunk)
                            })
                        });
                        if let Err(error) = result {
                            log::info!("[talk] system loopback: {error}");
                        }
                    }
                    Err(error) => match ready.take() {
                        Some(ready) => {
                            let _ = ready.send(Err(error));
                            return;
                        }
                        None => log::debug!("[talk] system loopback: {error}"),
                    },
                }
                std::thread::sleep(POLL);
            }
        })
        .map_err(|error| format!("loopback thread: {error}"))?;
    let guard = Guard { stop, thread: Some(thread) };
    match ready_rx.recv_timeout(Duration::from_secs(10)) {
        Ok(Ok(())) => Ok(AppCapture::new(rx, Box::new(guard))),
        Ok(Err(error)) => Err(error),
        Err(_) => Err("Desktop audio did not start.".to_string()),
    }
}

/// One process-tree loopback (its own thread).
struct Running {
    app: String,
    seen: Instant,
    guard: Guard,
}

impl Running {
    fn finished(&self) -> bool {
        self.guard.thread.as_ref().is_none_or(|thread| thread.is_finished())
    }
}

/// Samples of one root process (and their capture time, epoch ms), on
/// their way to that app's mixer.
type Raw = (String, u32, Vec<f32>, u64);

fn per_app(selection: Selection) -> Result<AppCapture, String> {
    let (tx, rx) = chunk_queue(CAPTURE_QUEUE);
    let (ready_tx, ready_rx) = mpsc::channel();
    let stop = Arc::new(AtomicBool::new(false));
    let thread_stop = stop.clone();
    let thread = std::thread::Builder::new()
        .name("talk-apps-sessions".to_string())
        .spawn(move || {
            let _com = match wasapi::Com::init() {
                Ok(com) => com,
                Err(error) => {
                    let _ = ready_tx.send(Err(error));
                    return;
                }
            };
            let _ = ready_tx.send(Ok(()));
            manage(selection, &thread_stop, &tx);
        })
        .map_err(|error| format!("session thread: {error}"))?;
    let guard = Guard { stop, thread: Some(thread) };
    match ready_rx.recv_timeout(Duration::from_secs(10)) {
        Ok(Ok(())) => Ok(AppCapture::new(rx, Box::new(guard))),
        Ok(Err(error)) => Err(error),
        Err(_) => Err("Desktop audio did not start.".to_string()),
    }
}

/// The session poller: starts a loopback per wanted app root, ends those
/// whose app went quiet or whose process exited, and folds every root's
/// samples into its app's channel.
fn manage(selection: Selection, stop: &AtomicBool, tx: &ChunkSender) {
    let own_pid = std::process::id();
    let own = pcm::own_stem();
    // Bounded like the chunks: a full queue drops the packet.
    let (raw_tx, raw_rx) = mpsc::sync_channel::<Raw>(CAPTURE_QUEUE);
    let mut running: HashMap<u32, Running> = HashMap::new();
    let mut names: HashMap<String, String> = HashMap::new();
    let mut mixers = pcm::Mixers::default();
    let mut polled: Option<Instant> = None;
    // Roots whose loopback ended on its own (a protected process, a failed
    // activation): not retried before this.
    let mut retry: HashMap<u32, Instant> = HashMap::new();

    'outer: while !stop.load(Ordering::Relaxed) {
        if polled.is_none_or(|at| at.elapsed() >= POLL) {
            polled = Some(Instant::now());
            let pids = procs::active_session_pids().unwrap_or_default();
            let table = procs::table();
            let plan = tree::plan(&pids, &table, own_pid, own.as_deref(), &selection);
            let now = Instant::now();
            for (app, roots) in &plan {
                for &root in roots {
                    if let Some(capture) = running.get_mut(&root) {
                        capture.seen = now;
                        continue;
                    }
                    if retry.get(&root).is_some_and(|after| now < *after) {
                        continue;
                    }
                    let name = names.entry(app.clone()).or_insert_with(|| app_name(app, Some(root))).clone();
                    mixers.add(app, &name, root);
                    let guard = spawn_loopback(app.clone(), root, raw_tx.clone());
                    running.insert(root, Running { app: app.clone(), seen: now, guard });
                    log::info!("[talk] capturing {app} (pid {root})");
                }
            }
            retry.retain(|root, after| now < *after && table.contains_key(root));
            let gone: Vec<u32> = running
                .iter()
                .filter(|(root, capture)| {
                    capture.finished() || !table.contains_key(root) || now.duration_since(capture.seen) >= LINGER
                })
                .map(|(root, _)| *root)
                .collect();
            for root in gone {
                if let Some(capture) = running.remove(&root) {
                    if capture.finished() {
                        retry.insert(root, now + LINGER);
                    }
                    drop(capture.guard);
                    // Samples of this root still queued go first.
                    while let Ok((app, source, samples, captured_ms)) = raw_rx.try_recv() {
                        if !send(tx, mixers.push(&app, names.get(&app).map_or(&app, |n| n), source, &samples, captured_ms)) {
                            break 'outer;
                        }
                    }
                    if !send(tx, mixers.remove(&capture.app, root, now_ms())) {
                        break 'outer;
                    }
                }
            }
        }
        match raw_rx.recv_timeout(Duration::from_millis(100)) {
            Ok((app, source, samples, captured_ms)) => {
                if !running.contains_key(&source) {
                    continue;
                }
                let label = names.get(&app).cloned().unwrap_or_else(|| app.clone());
                if !send(tx, mixers.push(&app, &label, source, &samples, captured_ms)) {
                    break;
                }
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }
    // Dropping the guards stops and joins every loopback.
    running.clear();
}

fn send(tx: &ChunkSender, chunks: Vec<Chunk>) -> bool {
    chunks.into_iter().all(|chunk| tx.send(chunk))
}

fn spawn_loopback(app: String, root: u32, raw: mpsc::SyncSender<Raw>) -> Guard {
    let stop = Arc::new(AtomicBool::new(false));
    let thread_stop = stop.clone();
    let thread = std::thread::Builder::new()
        .name(format!("talk-apps-{app}"))
        .spawn(move || {
            let result = wasapi::Com::init().and_then(|_com| {
                let loopback = wasapi::Loopback::process(root)?;
                // Dated by this packet's wall clock; a full queue drops it.
                loopback.pump(&thread_stop, |samples| match raw.try_send((app.clone(), root, samples, now_ms())) {
                    Ok(()) | Err(mpsc::TrySendError::Full(_)) => true,
                    Err(mpsc::TrySendError::Disconnected(_)) => false,
                })
            });
            if let Err(error) = result {
                log::debug!("[talk] loopback of {app} (pid {root}): {error}");
            }
        })
        .ok();
    if thread.is_none() {
        log::warn!("[talk] no loopback thread for pid {root}");
    }
    Guard { stop, thread }
}
