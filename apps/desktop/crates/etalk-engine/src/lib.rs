//! NEW-53: exponentialTalk's engines, the speech worker's side.
//!
//! [`run_and_exit`] is the whole worker: the host's executable re-run as
//! `<exe> __speech-worker --data-dir <dir>` (see `crates/etalk`). It reads
//! [`etalk::Request`] lines from stdin, answers with [`etalk::Event`] lines on
//! stdout, and logs to stderr (the host forwards it to its log). A host
//! that dies closes stdin; the worker sees EOF and exits.
//!
//! NEW-74: ONE engine for every language, Qwen3-TTS 0.6B Base (F16) through
//! CrispASR on the GPU, cloning zero-shot from the member's reference clip
//! (`voiceFile` + `voiceTranscript`, validated by
//! [`etalk::custom_voice_file`]) or the built-in clip of the language
//! ([`builtin`]). GPU only: the CPU runs it about two times SLOWER than real
//! time, so a machine without a usable GPU speaks nothing and reports
//! `voices: []`. Usable = the platform's GPU driver exists (checked before
//! any download), ggml finds a GPU with room for the model, and a warm-up
//! sentence synthesizes faster than [`MAX_WARMUP_RTF`]. A reply is
//! synthesized sentence by sentence and played as it arrives ([`sink`]); a
//! new `Speak` replaces the one playing, `Stop` ends it (each request
//! carries a generation number; older work stops at its next sentence).
//!
//! NEW-78: the worker also listens ([`listener`], its own thread): `Listen`
//! silences any speech first (barge-in), then opens the microphone; the
//! speaker and the transcriber share ONE CrispASR runtime ([`runtime`]).
//! A `Speak` that arrives while a Listen runs is answered `Finished {
//! stopped: true }` unplayed: never a reply into the open microphone.
//!
//! NEW-55 (`voice-call.json`): during a CALL a Speak plays (only push to
//! talk refuses it). A local call's playback is tapped as the echo
//! canceller's reference ([`echo`]) (#NEW-92: through ONE output stream for
//! the whole call, [`sink::Stream`], so the echo delay the canceller
//! converged on holds across replies); a remote call's reply is never played
//! here but streamed as `SpeechAudio` ([`StreamSink`]). When the call hears
//! [`etalk::BARGE_IN_MS`] of speech, [`barge`] stops the reply (its
//! `Finished { stopped: true }` goes out first) and only then is
//! `SpeechStarted` emitted. #NEW-91: the first audio comes under 400 ms
//! (p95 on the 4090: 84 ms German, 165 ms English) from Qwen3-TTS's own
//! streaming entry point ([`Speaker::Stream`]); where the runtime lacks it
//! (Windows) the first clause of a reply is synthesized on its own
//! ([`speech_chunks`]).
//!
//! #NEW-56: `Wake` keeps the microphone open for the wake word between
//! Listens ([`wake`], no model of its own); a heard phrase silences the voice
//! like a Listen and starts a turn the listener ends itself.
//!
//! NEW-57: `Meeting` hands the listener a meeting ([`meeting`]): the
//! microphone and the desktop audio, segmented per channel. While one is
//! asked for, a `Listen` is its flush trigger (`MeetingFlushNow`, then
//! `Empty`) and a `Call` is refused (`ListenFailed { reason: meeting }`).
//!
//! S-3 (`voice-call.json`): a LOCAL `Call` opens the call's one output stream
//! at once and RINGS on it, at most `ring_ms` ([`ring_tone`], EXP-1423: the
//! shared cadence, looped; a remote call rings on the caller, never here) while the worker warms up: the voice is
//! brought up ([`Job::Warm`]) and the listener loads the recognizer (a
//! download included) and opens the microphone. `CallListening` follows
//! once both are settled ([`listener::Links::voice_ready`]). The ring stops
//! at the first Speak (the greeting), a Stop, Hangup or the next call.
//!
//! EXP-1857: `Dial` is CALLER MODE (the IDE streams a call to ANOTHER
//! desktop over the relay): the same one output stream rings and then plays
//! the far end the host hands it (`DialPlay`, the canceller's reference),
//! and the listener sends the microphone, echo-cancelled, as `DialAudio`
//! ([`listener::Job::Dial`]). No recognizer and no voice, so nothing waits
//! for a download.

pub mod builtin;
pub mod capture;
pub mod echo;
pub mod listen;
pub mod listener;
pub mod meeting;
pub mod runtime;
pub mod sink;
pub mod wake;
pub mod wake_model;

/// EXP-1424: the wake tests' real model (livekit-wakeword's `hey_livekit`).
#[cfg(test)]
#[path = "../../etalk-wakeword/tests/common/mod.rs"]
mod wakeword_fixtures;

use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use etalk_crispasr::{artifact, models, Build, Device, StreamingTts, TtsSession};
use etalk::{Event, Request, VoiceChoice};

/// The CrispASR backend of the model.
const BACKEND: &str = "qwen3-tts";
/// The warm-up must run at least this much faster than real time (the
/// 4090 measured 0.13-0.27, the CPU ~2.0).
const MAX_WARMUP_RTF: f64 = 0.6;
/// eTalk device pass (2026-11-03): an M4 Pro warms up at 0.51-0.58, so a
/// busy machine (a build, a test suite) runs one warm-up over the bar. A
/// slow warm-up is retried on [`retry_after`]'s schedule; only this many
/// in a row (this worker) are a "too slow" verdict cached for
/// [`NO_GPU_TTL`].
const SLOW_WARMUPS_TO_CACHE: u32 = 3;
/// The slow warm-ups in a row so far.
static SLOW_WARMUPS: AtomicU32 = AtomicU32::new(0);
/// A GPU needs this much memory for the F16 model (measured peak 3.7 GB).
const MIN_GPU_BYTES: u64 = 4_000_000_000;

/// Run the worker to completion and exit the process. Call it from `main`
/// when [`etalk::requested`] answers; it never returns.
pub fn run_and_exit() -> ! {
    let Some(data_dir) = etalk::data_dir_arg() else {
        eprintln!("speech worker: missing {}", etalk::DATA_DIR_FLAG);
        std::process::exit(2);
    };
    run(data_dir);
    std::process::exit(0)
}

pub(crate) fn emit(event: &Event) {
    let Ok(line) = serde_json::to_string(event) else { return };
    let mut stdout = std::io::stdout().lock();
    let _ = writeln!(stdout, "{line}");
    let _ = stdout.flush();
}

enum Job {
    Prepare(VoiceChoice),
    Speak { id: String, text: String, generation: u64 },
    /// S-3: a call starts: bring the current voice up if it is not yet.
    Warm,
}

/// Lines a host logs at debug (`talk` reads the prefix).
pub(crate) fn debug(line: &str) {
    eprintln!("{}{line}", etalk::DEBUG_PREFIX);
}

/// S-3: the ring's level (peak, about -20 dBFS: soft next to a reply).
const RING_AMPLITUDE: f32 = 0.1;
/// Each burst fades in and out over this long (no click).
const RING_FADE_MS: u32 = 20;
/// EXP-1423: a ring is queued this long before the queued one runs out (it
/// runs out in the cadence's silence, so a late wake-up is never heard).
const RING_LEAD: Duration = Duration::from_secs(1);

/// EXP-1423: the ring's cadence (voice-call.json: `ringToneMs` on, then
/// `ringGapMs` off; `ringLowHz` + `ringHighHz` together), one period in ms.
fn ring_period_ms() -> u32 {
    let constants = &etalk::call::VOICE_CALL.constants;
    (constants.ring_tone_ms + constants.ring_gap_ms) as u32
}

/// S-3: `ms` of the ring at `rate`, from its start: the shared cadence
/// ([`etalk::call::call_ring_tone_on`]), cut at `ms`.
pub fn ring_tone(rate: u32, ms: u32) -> Vec<f32> {
    let constants = &etalk::call::VOICE_CALL.constants;
    let at = |ms: u64| (u64::from(rate) * ms / 1000) as usize;
    let (on, period, fade, total) =
        (at(constants.ring_tone_ms), at(u64::from(ring_period_ms())), at(u64::from(RING_FADE_MS)).max(1), at(u64::from(ms)));
    let (low, high) = (constants.ring_low_hz, constants.ring_high_hz);
    (0..total)
        .map(|index| {
            let phase = index % period;
            if phase >= on {
                return 0.0;
            }
            let envelope = (phase.min(on - 1 - phase) as f32 / fade as f32).min(1.0);
            let t = phase as f32 / rate as f32;
            let tone = ((std::f32::consts::TAU * low * t).sin() + (std::f32::consts::TAU * high * t).sin()) / 2.0;
            RING_AMPLITUDE * envelope * tone
        })
        .collect()
}

/// EXP-1859 (talk-listen.json `wake.multi` `tone`): the wake's short tone
/// on this machine's output, off the listening thread (the turn's
/// microphone is already open and keeps running).
fn wake_tone() {
    std::thread::spawn(|| {
        let rate = etalk_crispasr::QWEN3_TTS_RATE;
        match sink::open(rate) {
            Ok(sink) => {
                if let Err(err) = sink.push(&wake::wake_tone(rate)) {
                    eprintln!("wake word: the tone did not play: {err}");
                }
                sink.finish();
            }
            Err(err) => eprintln!("wake word: the tone's output did not open: {err}"),
        }
    });
}

/// Where an event goes: stdout in the worker, a list in the tests.
pub(crate) type Emitter = Arc<dyn Fn(&Event) + Send + Sync>;

struct Shared {
    /// Bumped by every Speak and Stop; work of an older value stops.
    generation: AtomicU64,
    /// The sink playing now, so Stop can silence it from the reader thread.
    current: Mutex<Option<Arc<dyn sink::Sink>>>,
    /// NEW-55: the reply being spoken; whoever takes it out (its own end, or
    /// a barge-in) emits its `Finished`, exactly once.
    speaking: Mutex<Option<String>>,
    /// NEW-55: whether a Listen or a (remote) Call runs.
    listening: Arc<listener::Listening>,
    /// NEW-55: a local call's echo reference.
    far: Arc<echo::FarEnd>,
    /// #NEW-92: a local call's output stream (opened by its first reply,
    /// dropped at Hangup).
    call_out: Mutex<Option<CallOut>>,
    /// #NEW-92: the sentences of the local call's last two replies, and when
    /// each ended (`None` = it plays): what the call may still hear as echo.
    spoken: Mutex<Vec<(Vec<String>, Option<Instant>)>>,
    /// S-3: the ring of a local call on its output stream (its call id).
    ring: Mutex<Option<(String, Arc<sink::Reply>)>>,
    /// S-3: the voice is settled: brought up (or known unavailable) and no
    /// bring-up or voice install runs. A call's `CallListening` waits for it.
    voice_settled: std::sync::atomic::AtomicBool,
    emit: Emitter,
}

/// #NEW-92: the open output stream of local call `call`, at `rate`.
struct CallOut {
    call: String,
    rate: u32,
    stream: Arc<dyn sink::Stream>,
}

/// #NEW-92: how long after a reply ended its words still count as echo for
/// an utterance that BEGINS by then. #NEW-98: its echo tail only (the
/// canceller's `echo::TAIL_MS` of 300 ms plus the devices' latencies, as
/// `listen::ECHO_WINDOW_MS` covers them): the utterance's closing silence
/// and its transcription no longer count, since the call reads the reply's
/// sentences as they stood when the utterance began (`listen::Call`). It
/// was 3 s and dropped "deploy it to production" answered within 3 s of
/// "Should I deploy it to production?". RETUNE from a real speaker.
pub const SPOKEN_HOLD: Duration = Duration::from_millis(500);

/// #NEW-92: the sentences of `spoken` (a reply's, and when it ended) a
/// local call may still hear at `now`: a reply still playing, or one that
/// ended less than [`SPOKEN_HOLD`] ago.
pub(crate) fn still_spoken(spoken: &[(Vec<String>, Option<Instant>)], now: Instant) -> Vec<String> {
    spoken
        .iter()
        .filter(|(_, ended)| ended.is_none_or(|ended| now.saturating_duration_since(ended) < SPOKEN_HOLD))
        .flat_map(|(sentences, _)| sentences.iter().cloned())
        .collect()
}

impl Shared {
    fn new(listening: Arc<listener::Listening>, emit: Emitter) -> Self {
        Shared {
            generation: AtomicU64::new(0),
            current: Mutex::new(None),
            speaking: Mutex::new(None),
            listening,
            far: Arc::default(),
            call_out: Mutex::new(None),
            spoken: Mutex::new(Vec::new()),
            ring: Mutex::new(None),
            voice_settled: std::sync::atomic::AtomicBool::new(false),
            emit,
        }
    }

    /// S-3: ring local call `id` on its output stream (opened now, the
    /// stream every reply of the call plays on) until [`Self::stop_ring`],
    /// at most `ms`. EXP-1423: one cadence period is queued at a time, the
    /// next [`RING_LEAD`] before it runs out. `false` = no stream.
    fn ring(&self, id: &str, ms: u32) -> bool {
        let stream = match self.call_out(etalk_crispasr::QWEN3_TTS_RATE) {
            Some(Ok(stream)) => stream,
            Some(Err(err)) => {
                eprintln!("call {id}: the audio output did not open: {err}");
                return false;
            }
            None => return false,
        };
        if ms == 0 {
            return true;
        }
        let ring = Arc::new(sink::Reply::new(stream));
        let period = ring_period_ms();
        let first = ring_tone(etalk_crispasr::QWEN3_TTS_RATE, period.min(ms));
        if let Err(err) = sink::Sink::push(&*ring, &first) {
            eprintln!("call {id}: the ring did not play: {err}");
            return true;
        }
        eprintln!("call {id}: ringing until answered (at most {ms} ms)");
        let old = self.ring.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).replace((id.to_string(), ring.clone()));
        if let Some((_, old)) = old {
            sink::Sink::stop(&*old);
        }
        let started = Instant::now();
        let id = id.to_string();
        std::thread::spawn(move || {
            let mut queued = period.min(ms);
            while queued < ms {
                let due = started + Duration::from_millis(u64::from(queued));
                std::thread::sleep(due.saturating_duration_since(Instant::now() + RING_LEAD));
                if ring.stopped() {
                    return;
                }
                let next = period.min(ms - queued);
                if let Err(err) = sink::Sink::push(&*ring, &ring_tone(etalk_crispasr::QWEN3_TTS_RATE, next)) {
                    eprintln!("call {id}: the ring stopped: {err}");
                    return;
                }
                queued += next;
            }
        });
        true
    }

    /// S-3: the ring stops (`id` = only call `id`'s; None = any).
    fn stop_ring(&self, id: Option<&str>) {
        let mut ring = self.ring.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        if ring.as_ref().is_some_and(|(call, _)| id.is_none_or(|id| id == call)) {
            if let Some((_, ring)) = ring.take() {
                sink::Sink::stop(&*ring);
            }
        }
    }

    /// #NEW-92: the local call's output stream at `rate`, opened on first use
    /// (and again after it failed). #1281 review: `None` = no local call
    /// runs (any more): checked under the stream's lock, so a Hangup that
    /// lands between the reply's start and here never leaves a stream open
    /// after its call (Hangup ends the call, THEN takes this lock).
    fn call_out(&self, rate: u32) -> Option<Result<Arc<dyn sink::Stream>, String>> {
        let mut out = self.call_out.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        let call = self.listening.local_call()?;
        if let Some(open) = out.as_ref() {
            if open.call == call && open.rate == rate && open.stream.failed().is_none() {
                return Some(Ok(open.stream.clone()));
            }
        }
        let far = self.far.clone();
        // The tap at the rate the stream hands its device (cpal: the
        // device's own, Linux: the voice's).
        let stream = match sink::open_stream(rate, Box::new(move |device_rate| echo::FarEnd::tap(&far, device_rate))) {
            Ok(stream) => stream,
            Err(err) => return Some(Err(err)),
        };
        *out = Some(CallOut { call, rate, stream: stream.clone() });
        Some(Ok(stream))
    }

    /// #NEW-92: call `id` ended (or turned remote): its stream plays out and
    /// closes. #1281 review: only its own (a late Hangup of an older call
    /// never closes the running call's); `None` = whichever is open (EOF).
    fn close_call_out(&self, id: Option<&str>) {
        let mut out = self.call_out.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        if id.is_none_or(|id| out.as_ref().is_some_and(|open| open.call == id)) {
            let stream = out.take();
            drop(out);
            drop(stream);
        }
    }

    /// EXP-1857: dialled call `id`'s far end on its output stream (the
    /// canceller's reference, like a local call's reply).
    fn dial_play(&self, id: &str, pcm: &[i16], rate: u32) {
        let samples: Vec<f32> = pcm.iter().map(|sample| f32::from(*sample) / 32768.0).collect();
        match self.call_out(rate) {
            Some(Ok(stream)) => {
                if let Err(err) = stream.push(&samples) {
                    eprintln!("call {id}: the far end did not play: {err}");
                }
            }
            Some(Err(err)) => eprintln!("call {id}: the audio output did not open: {err}"),
            None => {}
        }
    }

    /// EXP-1857: barge-in on dialled call `id`: what is queued goes.
    fn dial_flush(&self, id: &str) {
        let out = self.call_out.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some(open) = out.as_ref().filter(|open| open.call == id) {
            open.stream.flush();
        }
    }

    /// A call started: any stream but local call `keep`'s closes.
    fn keep_call_out(&self, keep: Option<&str>) {
        let mut out = self.call_out.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        if out.as_ref().is_some_and(|open| Some(open.call.as_str()) != keep) {
            let stream = out.take();
            drop(out);
            drop(stream);
        }
    }

    /// #NEW-92: what a local call may still hear of the latest reply.
    fn spoken(&self) -> Vec<String> {
        let spoken = self.spoken.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        still_spoken(&spoken, Instant::now())
    }

    /// A reply starts: its sentences join the previous reply's (whose
    /// echo can still arrive after the next one started).
    fn set_spoken(&self, sentences: Vec<String>) {
        let mut spoken = self.spoken.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        spoken.push((sentences, None));
        let over = spoken.len().saturating_sub(2);
        spoken.drain(..over);
    }

    fn speaking(&self) -> std::sync::MutexGuard<'_, Option<String>> {
        self.speaking.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}


/// #NEW-92: the reply's sentences count as playing until this drops (every
/// way `speak` returns), then for [`SPOKEN_HOLD`].
struct SpokenUntilDrop<'a>(&'a Shared);

impl Drop for SpokenUntilDrop<'_> {
    fn drop(&mut self) {
        let mut spoken = self.0.spoken.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some((_, ended)) = spoken.last_mut() {
            ended.get_or_insert_with(Instant::now);
        }
    }
}

/// S-3: the worker's `log` lines (the echo canceller's warnings) go to
/// stderr, which the host logs; there was no logger, so they went nowhere.
struct StderrLog;

impl log::Log for StderrLog {
    fn enabled(&self, metadata: &log::Metadata) -> bool {
        metadata.level() <= log::Level::Info
    }

    fn log(&self, record: &log::Record) {
        if self.enabled(record.metadata()) {
            eprintln!("{} {}", record.level(), record.args());
        }
    }

    fn flush(&self) {}
}

/// #ERR-29: set by [`report_panics`]: a panic that ends one of the
/// engine's threads ends the worker ([`EndsWorker`]). Unset in tests that
/// drive [`serve`] in their own process.
static ENDS_WORKER: AtomicBool = AtomicBool::new(false);

/// #ERR-29: held by each of the worker's long-lived threads (the engine,
/// the listener). A panic that unwinds out of one ends the worker with
/// [`etalk::PANIC_EXIT`]: the host reads the crash, reports it and starts
/// a new one, where the worker used to stay up without that engine and
/// drop every request after. An exit, no abort: the panic is the host's
/// report (its `Panicked` line), never a second, native one of a stack
/// that has unwound; on unix at once (`_exit`), with no `atexit` of the
/// model runtime while its other threads still run.
pub struct EndsWorker;

impl Drop for EndsWorker {
    fn drop(&mut self) {
        if std::thread::panicking() && ENDS_WORKER.load(Ordering::SeqCst) {
            // SAFETY: `_exit` only ends the process.
            #[cfg(unix)]
            unsafe {
                libc::_exit(etalk::PANIC_EXIT)
            }
            #[cfg(not(unix))]
            std::process::exit(etalk::PANIC_EXIT)
        }
    }
}

/// `file:line` of a panic, a registry crate's path from its crate name on
/// (never the build machine's home).
fn panic_location(location: Option<&std::panic::Location<'_>>) -> String {
    let Some(location) = location else { return "unknown".to_string() };
    let file = location.file();
    let file = file.split_once("/registry/src/").and_then(|(_, rest)| rest.split_once('/')).map_or(file, |(_, path)| path);
    format!("{file}:{}", location.line())
}

/// #ERR-29: the worker's panics reach the host: a hook that writes
/// [`Event::Panicked`] (where and on which thread), and [`EndsWorker`]
/// armed. A panic's MESSAGE can quote what was being spoken or heard, and
/// the host's log lines ride along with its error reports: it goes out as
/// debug lines only (the host's log file at `EXP_LOG=debug`, never a
/// report), in place of the default hook's output. The worker's native
/// crashes are its host's `observability` (the binary's `main`).
pub fn report_panics() {
    std::panic::set_hook(Box::new(|info| {
        let (at, thread) = (panic_location(info.location()), std::thread::current().name().unwrap_or("unnamed").to_string());
        eprintln!("panicked at {at} on thread {thread}");
        let payload = info.payload();
        let message = payload.downcast_ref::<&str>().copied().or_else(|| payload.downcast_ref::<String>().map(String::as_str)).unwrap_or("(no message)");
        message.lines().for_each(debug);
        emit(&Event::Panicked { at, thread });
    }));
    ENDS_WORKER.store(true, Ordering::SeqCst);
}

fn run(data_dir: PathBuf) {
    if log::set_logger(&StderrLog).is_ok() {
        log::set_max_level(log::LevelFilter::Info);
    }
    report_panics();
    emit(&Event::Ready { protocol: etalk::PROTOCOL });
    remove_retired_voices(&data_dir);
    let ears = data_dir.clone();
    serve(std::io::stdin().lock(), data_dir, move || listener::Ears::new(ears));
}

/// NEW-55: barge-in: the call heard [`etalk::BARGE_IN_MS`] of speech. Any
/// reply stops now (a queued one too), and the one being spoken is
/// answered `Finished { stopped: true }` here, before the caller emits
/// `SpeechStarted`.
fn barge(shared: &Shared) {
    let mut speaking = shared.speaking();
    shared.generation.fetch_add(1, Ordering::SeqCst);
    silence(shared);
    if let Some(id) = speaking.take() {
        (shared.emit)(&Event::Finished { id, stopped: true });
    }
}

/// `id` ended: its `event` goes out unless a barge-in already answered it.
fn end_speaking(shared: &Shared, id: &str, event: Event) {
    let mut speaking = shared.speaking();
    if speaking.as_deref() == Some(id) {
        *speaking = None;
        (shared.emit)(&event);
    }
}

/// Answer `input`'s requests until EOF, then wind the engines down and
/// return (the listener on `hearing`, which `make` builds on its thread).
fn serve<H: listener::Hearing>(input: impl BufRead, data_dir: PathBuf, hearing: impl FnOnce() -> H + Send + 'static) {
    let emitter: Emitter = Arc::new(emit);
    let listening = Arc::new(listener::Listening::default());
    let shared = Arc::new(Shared::new(listening.clone(), emitter.clone()));
    let (tx, rx) = mpsc::channel::<Job>();
    let engine_shared = shared.clone();
    let (engine_done, engine_ended) = mpsc::channel::<()>();
    let engine = std::thread::Builder::new()
        .name("voice-engine".into())
        .spawn(move || {
            let _ends = EndsWorker;
            engine_loop(&data_dir, rx, &engine_shared);
            let _ = engine_done.send(());
        })
        .expect("spawn the voice engine");
    let barging = shared.clone();
    let links = listener::Links {
        listening: listening.clone(),
        barge: Arc::new(move || barge(&barging)),
        far: shared.far.clone(),
        emit: emitter,
        speaking: {
            let shared = shared.clone();
            Arc::new(move || shared.speaking().is_some())
        },
        spoken: {
            let shared = shared.clone();
            Arc::new(move || shared.spoken())
        },
        voice_ready: {
            let shared = shared.clone();
            Arc::new(move || shared.voice_settled.load(Ordering::SeqCst))
        },
        tone: Arc::new(wake_tone),
    };
    let (listen, listener) = listener::spawn(hearing, links);

    for line in input.lines() {
        let Ok(line) = line else { break };
        let request = match serde_json::from_str::<Request>(&line) {
            Ok(request) => request,
            Err(err) => {
                // Never the line itself: a Speak carries the reply's text.
                eprintln!("unreadable request ({err})");
                continue;
            }
        };
        match request {
            Request::Prepare { voice } => {
                let _ = tx.send(Job::Prepare(voice));
            }
            Request::Speak { id, .. } if listening.refuses_speech() => {
                // Never into the open microphone: a reply that arrives
                // mid-Listen is dropped, as barge-in would have stopped it.
                // NEW-55: a call plays it (push to talk only refuses).
                emit(&Event::Finished { id, stopped: true });
            }
            Request::Speak { id, text } => {
                let generation = shared.generation.fetch_add(1, Ordering::SeqCst) + 1;
                // S-3: the greeting (any reply) ends the ring.
                shared.stop_ring(None);
                silence(&shared);
                let _ = tx.send(Job::Speak { id, text, generation });
            }
            Request::Stop => {
                shared.stop_ring(None);
                // eTalk device pass: answered at once like a barge-in. The
                // sentence being synthesized can't be aborted (CrispASR's
                // stream callback), so the engine's own Finished came ~2.3 s
                // after the audio stopped and the Speaking banner lingered.
                barge(&shared);
            }
            Request::Listen { id, .. } if listening.meeting().is_some() => {
                // NEW-57: push to talk during a meeting is its flush
                // trigger; the meeting keeps the microphone.
                if let Some(meeting) = listening.meeting() {
                    (shared.emit)(&Event::MeetingFlushNow { id: meeting });
                }
                (shared.emit)(&Event::Empty { id });
            }
            Request::Call { id, .. } if listening.meeting().is_some() => {
                // NEW-57: a meeting owns the microphone (voice-call.json
                // `failed.meeting`).
                (shared.emit)(&Event::ListenFailed {
                    id,
                    reason: etalk::ListenFailure::Meeting,
                    detail: "a meeting holds the microphone".into(),
                });
            }
            Request::Listen { id, language } => {
                // A call owns the microphone until Hangup: refused, nothing
                // heard, its reply keeps playing.
                if !listening.begin(&id) {
                    (shared.emit)(&Event::Empty { id });
                    continue;
                }
                // Barge-in: the person talks over the reply.
                shared.generation.fetch_add(1, Ordering::SeqCst);
                silence(&shared);
                let _ = listen.send(listener::Job::Start { id, language });
            }
            Request::EndListen { id } => {
                let _ = listen.send(listener::Job::End { id });
            }
            Request::CancelListen { id } => {
                let _ = listen.send(listener::Job::Cancel { id });
            }
            Request::Call { id, language, remote, ring_ms } => {
                // Replaces a Listen or Call; replies keep playing.
                listening.begin_call(&id, remote);
                // The call it replaced (or this one, turned remote) closes its stream.
                shared.stop_ring(None);
                shared.keep_call_out((!remote).then_some(id.as_str()));
                // S-3: a local call's stream opens now and rings while the
                // voice and the recognizer warm up.
                if !remote {
                    shared.ring(&id, ring_ms);
                }
                let _ = tx.send(Job::Warm);
                let _ = listen.send(listener::Job::Call { id, language, remote });
            }
            Request::CallAudio { id, pcm, lost_ms } => match etalk::decode_pcm(&pcm) {
                Ok(pcm) => {
                    // #EXP-1941: the loss comes first: it lies before these samples.
                    if lost_ms > 0 {
                        let _ = listen.send(listener::Job::Lost { id: id.clone(), ms: lost_ms });
                    }
                    if !pcm.is_empty() {
                        let samples = pcm.into_iter().map(|sample| f32::from(sample) / 32768.0).collect();
                        let _ = listen.send(listener::Job::Audio { id, samples });
                    }
                }
                Err(err) => eprintln!("unreadable call audio ({err})"),
            },
            Request::CallMute { id, muted } => {
                let _ = listen.send(listener::Job::Mute { id, muted });
            }
            Request::Hangup { id } => {
                // Replies play locally again at once.
                listening.end(&id);
                shared.stop_ring(Some(&id));
                shared.close_call_out(Some(&id));
                let _ = listen.send(listener::Job::Hangup { id });
            }
            Request::Wake { phrase, language, listeners } => {
                let _ = listen.send(listener::Job::Wake { phrase, language, listeners });
            }
            Request::Meeting { id, language, phrase, you_label, selection, capability } => {
                listening.begin_meeting(&id);
                let selection = selection.selection(capability);
                let _ = listen.send(listener::Job::Meeting { id, language, phrase, you_label, selection });
            }
            Request::MeetingStop { id, drain } => {
                listening.end_meeting(&id);
                let _ = listen.send(listener::Job::MeetingStop { id, drain });
            }
            Request::MeetingPause { id, paused } => {
                let _ = listen.send(listener::Job::MeetingPause { id, paused });
            }
            Request::Dial { id, .. } if listening.meeting().is_some() => {
                // NEW-57: a meeting owns the microphone.
                (shared.emit)(&Event::ListenFailed {
                    id,
                    reason: etalk::ListenFailure::Meeting,
                    detail: "a meeting holds the microphone".into(),
                });
            }
            Request::Dial { id, ring_ms } => {
                // EXP-1857: caller mode is a LOCAL call to the stream and the
                // canceller (its far end plays here), without the recognizer:
                // replaces a Listen or Call, and anything speaking stops.
                listening.begin_call(&id, false);
                shared.stop_ring(None);
                shared.keep_call_out(Some(id.as_str()));
                barge(&shared);
                shared.ring(&id, ring_ms);
                let _ = listen.send(listener::Job::Dial { id });
            }
            Request::DialPlay { id, pcm, rate } => {
                if listening.local_call().as_deref() != Some(id.as_str()) {
                    continue;
                }
                // The far end speaks: the ring is over.
                shared.stop_ring(Some(&id));
                match etalk::decode_pcm(&pcm) {
                    Ok(pcm) => shared.dial_play(&id, &pcm, rate),
                    Err(err) => eprintln!("unreadable dial audio ({err})"),
                }
            }
            Request::DialFlush { id } => shared.dial_flush(&id),
        }
    }
    // EOF: the host is gone. Silence and let the engines wind down; the
    // listener closes any open microphone. #NEW-80: running downloads stop
    // at their next read (also the listener's, on its own thread).
    etalk_crispasr::artifact::cancel_downloads();
    shared.generation.fetch_add(1, Ordering::SeqCst);
    silence(&shared);
    shared.close_call_out(None);
    drop(tx);
    drop(listen);
    // The listener closes the microphone: always waited for.
    let _ = listener.join();
    // #NEW-80: a voice download (GBs) is not waited out. The engine gets
    // ENGINE_GRACE to finish a sentence; past that the process exits and
    // takes the download with it (it writes `<id>.part`, which the next
    // download truncates, so nothing half-written is ever installed).
    if engine_ended.recv_timeout(ENGINE_GRACE).is_ok() {
        let _ = engine.join();
    } else {
        eprintln!("exiting while the voice engine is busy (a download or a load)");
    }
}

/// How long EOF waits for the voice engine before the worker exits anyway.
const ENGINE_GRACE: Duration = Duration::from_secs(2);

/// NEW-74: the Kokoro voice of NEW-53 is retired; free its ~340 MB.
fn remove_retired_voices(data_dir: &Path) {
    let kokoro = data_dir.join("talk").join("kokoro-multi-lang-v1_0");
    if kokoro.is_dir() && std::fs::remove_dir_all(&kokoro).is_ok() {
        eprintln!("removed the retired Kokoro voice");
    }
}

fn silence(shared: &Shared) {
    let sink = shared.current.lock().ok().and_then(|mut current| current.take());
    if let Some(sink) = sink {
        sink.stop();
    }
}

/// The wait after the `failures`-th failed download or load: 1 min,
/// doubling, at most 1 h. A changed or broken upstream asset is fetched
/// again on that schedule, never on every reply.
pub(crate) fn retry_after(failures: u32) -> Duration {
    Duration::from_secs(60u64.saturating_mul(1u64 << failures.saturating_sub(1).min(6))).min(Duration::from_secs(3600))
}

/// NEW-91: the first audio of a sentence after this many codec frames
/// (12.5 per second) on [`Speaker::Stream`].
const FIRST_AUDIO_FRAMES: u32 = 8;

/// NEW-91: the speaker. `Stream` = Qwen3-TTS's own streaming entry point
/// (Linux, macOS: audio every [`FIRST_AUDIO_FRAMES`] frames while the talker
/// still generates, the same voice and watermark); `Session` = the session
/// ABI, a whole utterance per call (the Windows build exports nothing else).
enum Speaker {
    Stream(StreamingTts<'static>),
    Session(TtsSession<'static>),
}

impl Speaker {
    fn set_voice(&mut self, wav: &Path, ref_text: &str, language: &str) -> Result<(), String> {
        match self {
            Speaker::Stream(tts) => tts.set_voice(wav, ref_text, language),
            Speaker::Session(tts) => tts.set_voice(wav, ref_text, language),
        }
    }

    /// Synthesize `text`, handing its audio to `on_audio` as it comes.
    fn synthesize(&mut self, text: &str, on_audio: &mut dyn FnMut(&[f32])) -> Result<(), String> {
        match self {
            Speaker::Stream(tts) => tts.synthesize(text, FIRST_AUDIO_FRAMES, on_audio),
            Speaker::Session(tts) => tts.synthesize(text).map(|pcm| on_audio(&pcm)),
        }
    }

    fn streams(&self) -> bool {
        matches!(self, Speaker::Stream(_))
    }
}

/// The engine once it is up: the runtime lives for the process.
struct Loaded {
    session: Speaker,
    sample_rate: u32,
    /// The voice installed in the session.
    voice: Option<VoiceChoice>,
}

enum State {
    /// Not tried yet, or backing off until `retry_at`.
    Idle { failures: u32, retry_at: Option<Instant> },
    /// No usable GPU on this machine: nothing is ever spoken (until the
    /// worker restarts).
    NoGpu,
    Ready(Loaded),
}

/// EXP-1930: what a worker whose voice is not up says: `VoiceRetry` while
/// it will be tried again (a slow warm-up, a failed load), never for no GPU;
/// then the empty `Voices`.
fn not_up(state: &State, now: Instant) -> Vec<Event> {
    let mut events = Vec::new();
    if let State::Idle { retry_at: Some(at), .. } = state {
        events.push(Event::VoiceRetry { in_ms: at.saturating_duration_since(now).as_millis() as u64 });
    }
    events.push(Event::Voices { voices: Vec::new() });
    events
}

fn engine_loop(data_dir: &Path, rx: Receiver<Job>, shared: &Arc<Shared>) {
    let mut state = State::Idle { failures: 0, retry_at: None };
    let mut choice = VoiceChoice { language: "en".into(), ..Default::default() };
    // The host asked for the voice: a failed bring-up is retried unasked.
    let mut wanted = false;
    loop {
        // voice.json (#1237 review): a failed download or load is retried by
        // the worker itself with backoff (1 min doubling to 1 h), never on
        // every reply: the wait ends in a Prepare of the current choice.
        let first = match state {
            State::Idle { retry_at: Some(at), .. } if wanted => {
                match rx.recv_timeout(at.saturating_duration_since(Instant::now())) {
                    Ok(job) => job,
                    Err(RecvTimeoutError::Timeout) => Job::Prepare(choice.clone()),
                    Err(RecvTimeoutError::Disconnected) => break,
                }
            }
            _ => match rx.recv() {
                Ok(job) => job,
                Err(_) => break,
            },
        };
        wanted = true;
        let job = coalesce(first, rx.try_iter(), &mut choice);
        if let State::Idle { failures, retry_at } = state {
            if retry_at.is_some_and(|at| Instant::now() < at) {
                // Backing off: the voice is not tried again yet (settled:
                // a call need not wait for it).
                shared.voice_settled.store(true, Ordering::SeqCst);
                for event in not_up(&state, Instant::now()) {
                    emit(&event);
                }
                if let Job::Speak { id, .. } = job {
                    emit(&Event::Failed { id, error: "the voice is unavailable for now".into() });
                }
                continue;
            }
            shared.voice_settled.store(false, Ordering::SeqCst);
            state = match bring_up(data_dir) {
                Ok(Some(loaded)) => {
                    emit(&Event::ModelReady);
                    // Also when a Speak swallowed the Prepare behind it.
                    emit(&Event::Voices { voices: builtin::languages() });
                    State::Ready(loaded)
                }
                Ok(None) => State::NoGpu,
                // Never a download toast: the voice is only not up yet.
                Err(NotUp::Slow(why)) => {
                    let failures = failures + 1;
                    let wait = retry_after(failures);
                    eprintln!("voice not up (attempt {failures}, next in {}s): {why}", wait.as_secs());
                    State::Idle { failures, retry_at: Some(Instant::now() + wait) }
                }
                Err(NotUp::Failed(err)) => {
                    let failures = failures + 1;
                    let wait = retry_after(failures);
                    eprintln!("voice unavailable (attempt {failures}, next in {}s): {err}", wait.as_secs());
                    emit(&Event::DownloadFailed { error: err.clone() });
                    State::Idle { failures, retry_at: Some(Instant::now() + wait) }
                }
            };
        }
        let State::Ready(loaded) = &mut state else {
            // Unavailable (no GPU, or backing off): settled all the same.
            shared.voice_settled.store(true, Ordering::SeqCst);
            for event in not_up(&state, Instant::now()) {
                emit(&event);
            }
            if let Job::Speak { id, .. } = job {
                emit(&Event::Failed { id, error: "no usable GPU for the voice".into() });
            }
            continue;
        };
        if loaded.voice.as_ref() != Some(&choice) {
            shared.voice_settled.store(false, Ordering::SeqCst);
            if let Err(err) = install_voice(data_dir, loaded, &choice) {
                eprintln!("{err}");
            }
        }
        shared.voice_settled.store(true, Ordering::SeqCst);
        match job {
            Job::Prepare(_) => emit(&Event::Voices { voices: builtin::languages() }),
            Job::Warm => {}
            Job::Speak { id, text, generation } => speak(loaded, shared, &id, &text, generation),
        }
    }
}

/// One job out of `first` and what queued up behind it: only the newest
/// Speak matters; the newest Prepare is the `choice`.
fn coalesce(first: Job, rest: impl Iterator<Item = Job>, choice: &mut VoiceChoice) -> Job {
    let mut job = first;
    // EXP-1934: before the rest: a Speak right behind a Prepare replaces it
    // as the job, and is spoken in the voice it prepared.
    if let Job::Prepare(voice) = &job {
        *choice = voice.clone();
    }
    for next in rest {
        match next {
            Job::Prepare(voice) => {
                *choice = voice;
                if !matches!(job, Job::Speak { .. }) {
                    job = Job::Prepare(choice.clone());
                }
            }
            // Any job brings the voice up.
            Job::Warm => {}
            speak => job = speak,
        }
    }
    job
}

/// Why [`bring_up`] did not finish; both are retried later.
enum NotUp {
    /// A download, verify or load failed (`DownloadFailed`).
    Failed(String),
    /// The warm-up ran over [`MAX_WARMUP_RTF`] (fewer than
    /// [`SLOW_WARMUPS_TO_CACHE`] times in a row).
    Slow(String),
}

impl From<String> for NotUp {
    fn from(err: String) -> Self {
        NotUp::Failed(err)
    }
}

/// Probe, download, verify, load and warm up. `Ok(None)` = no usable GPU
/// (permanent for this worker); `Err` = worth retrying later.
fn bring_up(data_dir: &Path) -> Result<Option<Loaded>, NotUp> {
    if !etalk_crispasr::platform::supported() || !etalk_crispasr::platform::gpu_driver_present() {
        eprintln!("no usable GPU: no GPU driver for the voice on this machine");
        return Ok(None);
    }
    let models_dir = data_dir.join("talk").join(models::MODELS_DIR);
    // NEW-78: no runtime bytes to fetch when the transcriber loaded it.
    let runtime: &[artifact::Artifact] = if runtime::loaded().is_some() { &[] } else { etalk_crispasr::platform::ARTIFACTS };
    let model_files = [models::QWEN3_TTS_0_6B_BASE, models::QWEN3_TTS_CODEC];
    let total: u64 = runtime.iter().chain(model_files.iter()).map(|artifact| artifact.size).sum();
    let runtime_total: u64 = runtime.iter().map(|artifact| artifact.size).sum();
    let mut last = Instant::now() - Duration::from_secs(1);
    let mut report = |received: u64| {
        // At most 4 events a second (the row's detail redraws on each).
        if last.elapsed() >= Duration::from_millis(250) || received == total {
            last = Instant::now();
            emit(&Event::Downloading { received, total });
        }
    };
    // The runtime first: whether there is a usable GPU at all is known
    // before the 2.2 GB of models are fetched.
    // Every library re-hashed inside `load_build` before dlopen; NEW-78: the
    // ONE runtime of the worker, shared with the transcriber.
    let runtime = runtime::get(data_dir, Build::Gpu, |received, _| report(received))?;
    let gpus = runtime.gpus();
    let Some(gpu) = gpus.iter().filter(|gpu| gpu.total_bytes >= MIN_GPU_BYTES).max_by_key(|gpu| gpu.free_bytes) else {
        let seen: Vec<String> = gpus.iter().map(|gpu| format!("{} ({} MB)", gpu.description, gpu.total_bytes / 1_000_000)).collect();
        eprintln!("no usable GPU: ggml found {}", if seen.is_empty() { "none".to_string() } else { seen.join(", ") });
        return Ok(None);
    };
    eprintln!("voice GPU: {} ({} MB, {} MB free)", gpu.description, gpu.total_bytes / 1_000_000, gpu.free_bytes / 1_000_000);
    let fingerprint = gpu_fingerprint(gpu, etalk_crispasr::platform::driver_version().as_deref());
    let cache = no_gpu_cache_path(data_dir);
    if no_gpu_cached(&cache, &fingerprint, SystemTime::now()) {
        eprintln!("no usable GPU: this GPU and driver were too slow before (cached; a driver change or 30 days re-test)");
        return Ok(None);
    }
    // #NEW-80: under the runtime's download lock (the transcriber fetches
    // into the same dir).
    runtime::fetching(|| artifact::ensure_all(&models_dir, &model_files, |received, _| report(runtime_total + received)))
        .map_err(|err| err.to_string())?;
    // The GGUFs are re-hashed before ggml parses them (~1-2 s for 2.2 GB).
    let hashing = Instant::now();
    for model in &model_files {
        artifact::verify(&models_dir, model).map_err(|err| err.to_string())?;
    }
    eprintln!("voice models verified in {} ms", hashing.elapsed().as_millis());
    let started = Instant::now();
    let model = models_dir.join(models::QWEN3_TTS_0_6B_BASE.keep[0].1);
    let codec = models_dir.join(models::QWEN3_TTS_CODEC.keep[0].1);
    let (session, sample_rate) = if runtime.streams_tts() {
        (Speaker::Stream(runtime.open_streaming_tts(&model, &codec, Device::Gpu)?), etalk_crispasr::QWEN3_TTS_RATE)
    } else {
        let session = runtime.open_tts(BACKEND, &model, &codec, Device::Gpu)?;
        let rate = session.sample_rate();
        (Speaker::Session(session), rate)
    };
    let mut loaded = Loaded { session, sample_rate, voice: None };
    let english = VoiceChoice { language: "en".into(), ..Default::default() };
    install_voice(data_dir, &mut loaded, &english)?;
    eprintln!("voice loaded in {} ms", started.elapsed().as_millis());
    // Two tries: the first call of a cold GPU pipeline compiles its shaders.
    let mut best = f64::MAX;
    for _ in 0..2 {
        let started = Instant::now();
        let mut samples = 0;
        loaded.session.synthesize("Hello, this is a quick voice check.", &mut |pcm| samples += pcm.len())?;
        let audio = samples as f64 / f64::from(sample_rate);
        let rtf = if audio > 0.0 { started.elapsed().as_secs_f64() / audio } else { f64::MAX };
        best = best.min(rtf);
        if best <= MAX_WARMUP_RTF {
            break;
        }
    }
    eprintln!("voice warm-up: real-time factor {best:.2}");
    if best > MAX_WARMUP_RTF {
        if retry_slow_warmup() {
            return Err(NotUp::Slow(format!("the warm-up ran at {best:.2} of real time (over {MAX_WARMUP_RTF}; a busy machine?)")));
        }
        eprintln!("no usable GPU: the voice runs slower than {MAX_WARMUP_RTF} of real time, {SLOW_WARMUPS_TO_CACHE} times in a row");
        remember_no_gpu(&cache, &fingerprint, SystemTime::now());
        return Ok(None);
    }
    SLOW_WARMUPS.store(0, Ordering::SeqCst);
    Ok(Some(loaded))
}

/// Count a slow warm-up: `true` = retry, not yet a verdict.
fn retry_slow_warmup() -> bool {
    SLOW_WARMUPS.fetch_add(1, Ordering::SeqCst) + 1 < SLOW_WARMUPS_TO_CACHE
}

/// How long a "too slow" verdict stands before the GPU is tried again.
const NO_GPU_TTL: Duration = Duration::from_secs(30 * 24 * 3600);

/// The GPU and driver a "too slow" verdict is about.
fn gpu_fingerprint(gpu: &etalk_crispasr::Gpu, driver: Option<&str>) -> String {
    format!(
        "{} | {} | {} MB | {} | max rtf {MAX_WARMUP_RTF}",
        etalk_crispasr::platform::RUNTIME_DIR,
        gpu.description,
        gpu.total_bytes / 1_000_000,
        driver.unwrap_or("driver unknown")
    )
}

fn no_gpu_cache_path(data_dir: &Path) -> PathBuf {
    data_dir.join("talk").join("no-gpu.txt")
}

/// The cache: the fingerprint on line one, the unix time on line two.
fn no_gpu_cached(path: &Path, fingerprint: &str, now: SystemTime) -> bool {
    let Ok(text) = std::fs::read_to_string(path) else { return false };
    let mut lines = text.lines();
    let (Some(cached), Some(at)) = (lines.next(), lines.next().and_then(|at| at.trim().parse::<u64>().ok())) else {
        return false;
    };
    let age = now.duration_since(UNIX_EPOCH).map(|now| now.as_secs().saturating_sub(at)).unwrap_or(u64::MAX);
    cached == fingerprint && age < NO_GPU_TTL.as_secs()
}

fn remember_no_gpu(path: &Path, fingerprint: &str, now: SystemTime) {
    let at = now.duration_since(UNIX_EPOCH).map(|now| now.as_secs()).unwrap_or(0);
    if let Err(err) = std::fs::write(path, format!("{fingerprint}\n{at}\n")) {
        eprintln!("could not remember the slow GPU: {err}");
    }
}

/// A synthesis error as logged and reported: never the reply's text (a
/// backend that echoed its input would otherwise put it in the log).
fn scrub(error: &str, text: &str) -> String {
    let mut error = error.to_string();
    if !text.trim().is_empty() {
        error = error.replace(text, "<reply>");
    }
    for sentence in sentences(text).into_iter().filter(|sentence| sentence.len() >= 8) {
        error = error.replace(sentence, "<reply>");
    }
    error.chars().take(200).collect()
}

/// Install the member's clip (validated) or the language's built-in clip.
fn install_voice(data_dir: &Path, loaded: &mut Loaded, choice: &VoiceChoice) -> Result<(), String> {
    let language = builtin::for_language(&choice.language);
    let custom = match etalk::custom_voice_file(&choice.file, &choice.transcript) {
        Ok(custom) => custom,
        Err(reason) => {
            // Logged once per choice (a new choice is the only way here).
            eprintln!("voice file rejected, using the built-in voice: {reason}");
            None
        }
    };
    let installed = match &custom {
        Some(path) => loaded.session.set_voice(path, choice.transcript.trim(), language.language),
        None => Err(String::new()),
    };
    if let Err(err) = installed {
        if !err.is_empty() {
            eprintln!("voice file failed to load, using the built-in voice: {err}");
        }
        let path = builtin::materialize(&data_dir.join("talk").join("voices"), language)?;
        loaded.session.set_voice(&path, language.transcript, language.language)?;
    }
    loaded.voice = Some(choice.clone());
    Ok(())
}

/// Split spoken text into sentences (each is one synthesis call, so the
/// first audio arrives after the first sentence and Stop lands between).
fn sentences(text: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut start = 0;
    for (index, ch) in text.char_indices() {
        if matches!(ch, '.' | '!' | '?') && text[index + 1..].starts_with(' ') {
            out.push(text[start..=index].trim());
            start = index + 1;
        }
    }
    let rest = text[start..].trim();
    if !rest.is_empty() {
        out.push(rest);
    }
    out.retain(|sentence| !sentence.is_empty());
    out
}

/// NEW-55: the first sentence's first clause, synthesized on its own so the
/// first audio comes sooner: split at the first `,` `;` `:` (followed by a
/// space) or ` - ` when that leaves 2 to 8 words in front. `None` = no such
/// split. The front keeps its punctuation (a dash is dropped).
pub fn first_clause(sentence: &str) -> Option<(&str, &str)> {
    let mark = sentence.char_indices().find_map(|(index, ch)| {
        let rest = &sentence[index + ch.len_utf8()..];
        match ch {
            ',' | ';' | ':' if rest.starts_with(char::is_whitespace) => Some((index + 1, index + 1)),
            '-' if index > 0 && sentence[..index].ends_with(' ') && rest.starts_with(' ') => Some((index - 1, index + 1)),
            _ => None,
        }
    })?;
    let (front, rest) = (sentence[..mark.0].trim(), sentence[mark.1..].trim());
    let words = front.split_whitespace().count();
    ((2..=8).contains(&words) && !rest.is_empty()).then_some((front, rest))
}

/// The synthesis calls of a reply (voice.json `speechChunkCases`): its
/// sentences; #NEW-91: on a speaker that does not stream (`streaming`
/// false), the first one split at its [`first_clause`].
pub fn speech_chunks(text: &str, streaming: bool) -> Vec<&str> {
    let mut parts = sentences(text);
    if !streaming {
        if let Some((front, rest)) = parts.first().and_then(|first| first_clause(first)) {
            parts.splice(0..1, [front, rest]);
        }
    }
    parts
}

/// NEW-55: a remote call's reply: every pushed sample goes to the host as
/// `SpeechAudio` (i16 LE, at most [`etalk::SPEECH_AUDIO_CHUNK_MS`] each),
/// never to a speaker. Stopped (Stop, barge-in), it sends nothing more; a
/// chunk being sent finishes before `stop` returns. EXP-1929: the audio
/// leaves at real time, at most [`REMOTE_LEAD`] ahead of the clock (the
/// relay forwards at most `maxFramesPerSecond`), and `finish` waits out the
/// lead, so the reply ends when the caller has heard it.
pub struct StreamSink {
    id: String,
    rate: u32,
    stopped: Mutex<bool>,
    emit: Emitter,
    clock: Arc<dyn Clock>,
    /// When the audio sent so far started (on `clock`), and how many samples.
    sent: Mutex<Option<(Duration, u64)>>,
}

/// EXP-1929: how far a remote reply may run ahead of real time.
pub(crate) const REMOTE_LEAD: Duration = Duration::from_millis(200);

/// The time a remote reply is paced by (a fake one in the tests).
pub(crate) trait Clock: Send + Sync {
    fn now(&self) -> Duration;
    fn sleep(&self, duration: Duration);
}

struct WallClock(Instant);

impl Clock for WallClock {
    fn now(&self) -> Duration {
        self.0.elapsed()
    }
    fn sleep(&self, duration: Duration) {
        std::thread::sleep(duration);
    }
}

impl StreamSink {
    fn new(id: &str, rate: u32, emit: Emitter) -> Self {
        Self::with_clock(id, rate, emit, Arc::new(WallClock(Instant::now())))
    }

    fn with_clock(id: &str, rate: u32, emit: Emitter, clock: Arc<dyn Clock>) -> Self {
        StreamSink { id: id.into(), rate, stopped: Mutex::new(false), emit, clock, sent: Mutex::new(None) }
    }

    fn is_stopped(&self) -> bool {
        *self.stopped.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// How far the audio sent so far runs ahead of the clock; a stream that
    /// fell behind (synthesis slower than real time, a pause between
    /// sentences) starts over from now, never sending a backlog at once.
    fn lead(&self) -> Duration {
        let now = self.clock.now();
        let mut sent = self.sent.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        let (start, samples) = *sent.get_or_insert((now, 0));
        let end = start + Duration::from_micros(samples * 1_000_000 / u64::from(self.rate.max(1)));
        if end <= now {
            *sent = Some((now, 0));
            return Duration::ZERO;
        }
        end - now
    }

    /// Wait (in steps of at most one frame, Stop ends it) until the lead is
    /// at most `lead`.
    fn wait_for_lead(&self, lead: Duration) {
        let step = Duration::from_millis(u64::from(etalk::call::VOICE_CALL.constants.frame_ms));
        loop {
            let ahead = self.lead();
            if ahead <= lead || self.is_stopped() {
                return;
            }
            self.clock.sleep((ahead - lead).min(step));
        }
    }
}

impl sink::Sink for StreamSink {
    fn push(&self, samples: &[f32]) -> Result<(), String> {
        let chunk = (self.rate * etalk::SPEECH_AUDIO_CHUNK_MS / 1000).max(1) as usize;
        for part in samples.chunks(chunk) {
            self.wait_for_lead(REMOTE_LEAD);
            let stopped = self.stopped.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
            if *stopped {
                return Ok(());
            }
            let pcm: Vec<i16> = part.iter().map(|sample| etalk::to_i16(*sample)).collect();
            (self.emit)(&Event::SpeechAudio { id: self.id.clone(), pcm: etalk::encode_pcm(&pcm), rate: self.rate });
            if let Some((_, sent)) = self.sent.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).as_mut() {
                *sent += part.len() as u64;
            }
        }
        Ok(())
    }

    fn finish(&self) {
        self.wait_for_lead(Duration::ZERO);
    }

    fn stop(&self) {
        *self.stopped.lock().unwrap_or_else(|poisoned| poisoned.into_inner()) = true;
    }
}

/// What [`speak_parts`] synthesizes with (the [`Speaker`]; tests fake it).
trait Synth {
    fn synthesize(&mut self, text: &str, on_audio: &mut dyn FnMut(&[f32])) -> Result<(), String>;
    fn streams(&self) -> bool;
}

impl Synth for Speaker {
    fn synthesize(&mut self, text: &str, on_audio: &mut dyn FnMut(&[f32])) -> Result<(), String> {
        Speaker::synthesize(self, text, on_audio)
    }
    fn streams(&self) -> bool {
        Speaker::streams(self)
    }
}

/// #EXP-2016: the most audio one synthesis call may give for `part`, in
/// samples at `rate`: [`SPEECH_SECONDS_PER_CHAR`] per character, at least
/// [`SPEECH_MIN_SECONDS`], at most [`SPEECH_MAX_SECONDS`] (the speech
/// model's own step cap, `etalk_crispasr::MAX_CODEC_STEPS`, bounds the call).
fn speech_budget(part: &str, rate: u32) -> usize {
    let seconds = (part.chars().count() as f64 * SPEECH_SECONDS_PER_CHAR).clamp(SPEECH_MIN_SECONDS, SPEECH_MAX_SECONDS);
    (seconds * f64::from(rate)) as usize
}

/// #EXP-2016: a sentence is heard at ~15 characters a second; three times
/// that is a trajectory that does not end (a degenerate sentence runs on).
const SPEECH_SECONDS_PER_CHAR: f64 = 0.2;
const SPEECH_MIN_SECONDS: f64 = 10.0;
const SPEECH_MAX_SECONDS: f64 = 60.0;

/// A reply's synthesis, sentence by sentence, each sample to `push` while
/// `current`; `started` once audio goes out (a remote call's before its
/// first push). #EXP-2016: a sentence past its [`speech_budget`] is cut
/// there and the next one goes on, so a reply the model cannot finish
/// never runs it to its ceiling. The error a reply failed with, if none
/// of it went out.
fn speak_parts(
    synth: &mut dyn Synth,
    text: &str,
    rate: u32,
    remote: bool,
    current: &dyn Fn() -> bool,
    started_now: &mut dyn FnMut(),
    push: &mut dyn FnMut(&[f32]) -> Result<(), String>,
) -> Option<String> {
    let mut started = false;
    let mut failed = None;
    for part in speech_chunks(text, synth.streams()) {
        if !current() {
            break;
        }
        let budget = speech_budget(part, rate);
        let (mut heard, mut cut) = (0usize, false);
        // NEW-91: on a streaming speaker the audio arrives in chunks while
        // the sentence is still being synthesized.
        let mut broken = false;
        let result = synth.synthesize(part, &mut |pcm| {
            if broken || cut || pcm.is_empty() || !current() {
                return;
            }
            let pcm = &pcm[..pcm.len().min(budget - heard)];
            heard += pcm.len();
            cut = heard >= budget;
            if pcm.is_empty() {
                return;
            }
            if remote && !started {
                // The stream sits between Started and Finished.
                started = true;
                started_now();
            }
            // #1231 review: a player that takes no audio (pw-play without a
            // PipeWire session) fails the reply, never reports it played.
            if let Err(err) = push(pcm) {
                eprintln!("audio output failed: {err}");
                if !started {
                    failed = Some(err);
                }
                broken = true;
                return;
            }
            if !started {
                started = true;
                started_now();
            }
        });
        if broken {
            break;
        }
        if cut {
            eprintln!("a sentence ran past its speech budget ({budget} samples): cut, the reply goes on");
        }
        if let Err(err) = result {
            let err = scrub(&err, text);
            eprintln!("synthesis failed: {err}");
            if !started {
                failed = Some(err);
            }
            break;
        }
    }
    failed
}

fn speak(loaded: &mut Loaded, shared: &Arc<Shared>, id: &str, text: &str, generation: u64) {
    let current = |shared: &Shared| shared.generation.load(Ordering::SeqCst) == generation;
    let emit = |event: Event| (shared.emit)(&event);
    if !current(shared) {
        emit(Event::Finished { id: id.into(), stopped: true });
        return;
    }
    // NEW-55: a remote call's reply streams to the caller.
    let remote = shared.listening.remote_call();
    // #NEW-92: a local call's reply plays on the call's one stream (tapped
    // as the canceller's reference); its sentences are what the call may
    // hear back as echo.
    let local_call = !remote && shared.listening.in_call();
    let opened = if remote {
        Ok(Arc::new(StreamSink::new(id, loaded.sample_rate, shared.emit.clone())) as Arc<dyn sink::Sink>)
    } else if local_call {
        shared.set_spoken(speech_chunks(text, true).into_iter().map(str::to_string).collect());
        // The call may have hung up since: then it plays like any reply.
        match shared.call_out(loaded.sample_rate) {
            Some(opened) => opened.map(|stream| Arc::new(sink::Reply::new(stream)) as Arc<dyn sink::Sink>),
            None => sink::open(loaded.sample_rate),
        }
    } else {
        sink::open(loaded.sample_rate)
    };
    let _spoken = local_call.then(|| SpokenUntilDrop(shared));
    let sink: Arc<dyn sink::Sink> = {
        match opened {
            Ok(sink) => sink,
            Err(err) => {
                eprintln!("audio output failed: {err}");
                emit(Event::Failed { id: id.into(), error: err });
                return;
            }
        }
    };
    if let Ok(mut playing) = shared.current.lock() {
        *playing = Some(sink.clone());
    }
    {
        // Checked under the barge-in's lock: a barge-in either came first
        // (the reply never starts) or finds it speaking and answers it.
        let mut speaking = shared.speaking();
        if !current(shared) {
            drop(speaking);
            sink.stop();
            clear_current(shared, &sink);
            emit(Event::Finished { id: id.into(), stopped: true });
            return;
        }
        *speaking = Some(id.to_string());
    }
    let failed = speak_parts(
        &mut loaded.session,
        text,
        loaded.sample_rate,
        remote,
        &|| current(shared),
        &mut || emit(Event::Started { id: id.into() }),
        &mut |pcm| sink.push(pcm),
    );
    if let Some(error) = failed {
        sink.stop();
        clear_current(shared, &sink);
        end_speaking(shared, id, Event::Failed { id: id.into(), error });
        return;
    }
    if current(shared) {
        sink.finish();
    } else {
        sink.stop();
    }
    let still_current = current(shared);
    clear_current(shared, &sink);
    end_speaking(shared, id, Event::Finished { id: id.into(), stopped: !still_current });
}

fn clear_current(shared: &Shared, sink: &Arc<dyn sink::Sink>) {
    if let Ok(mut playing) = shared.current.lock() {
        if playing.as_ref().is_some_and(|playing| Arc::ptr_eq(playing, sink)) {
            *playing = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// #1238 review: the worker exits on EOF while a Listen holds the
    /// microphone, and the microphone closes.
    #[test]
    fn serve_returns_on_eof_while_a_listen_is_active() {
        use std::io::Write as _;
        use std::sync::atomic::Ordering;

        let (probe, opened, _) = listener::fake::Probe::new(false);
        let (reader, mut writer) = std::io::pipe().unwrap();
        let dir = std::env::temp_dir().join(format!("voice-eof-{}", std::process::id()));
        let hearing = probe.hearing();
        let (done, served) = mpsc::channel();
        std::thread::spawn(move || {
            serve(std::io::BufReader::new(reader), dir, move || hearing);
            let _ = done.send(());
        });
        writeln!(writer, r#"{{"t":"listen","id":"u1","language":"en"}}"#).unwrap();
        opened.recv_timeout(Duration::from_secs(10)).expect("the Listen opened the microphone");
        drop(writer);
        served.recv_timeout(Duration::from_secs(10)).expect("serve returned after EOF");
        let mics = probe.opened.lock().unwrap();
        assert_eq!(mics.len(), 1);
        assert!(mics[0].load(Ordering::SeqCst), "the microphone closed");
    }

    /// EXP-1423: the ring follows the shared cadence (`ringToneCases`):
    /// soft 440 + 480 Hz for 2 s, 4 s of silence, again; no click at a
    /// burst's edges.
    #[test]
    fn the_ring_follows_the_shared_cadence() {
        let rate = 24_000;
        let ring = ring_tone(rate, 12_000);
        assert_eq!(ring.len(), 288_000);
        let peak = |from: usize, to: usize| ring[from..to].iter().fold(0.0f32, |peak, sample| peak.max(sample.abs()));
        assert!(peak(0, ring.len()) <= RING_AMPLITUDE);
        for second in 0..12 {
            let (from, to) = (second * rate as usize, (second + 1) * rate as usize);
            let on = etalk::call::call_ring_tone_on(second as i64 * 1000);
            if on {
                assert!(peak(from, to) > RING_AMPLITUDE * 0.9, "the tone in second {second}");
            } else {
                assert_eq!(peak(from, to), 0.0, "the gap in second {second}");
            }
        }
        assert!(ring[0].abs() < 1e-6 && ring[47_999].abs() < 0.01, "faded in and out");
        assert!(ring_tone(rate, 0).is_empty());
    }

    /// EXP-1934: a Prepare is the choice whatever queued up behind it: a
    /// Speak sent right after it is spoken in ITS voice, not the last one's.
    #[test]
    fn a_prepare_keeps_its_voice_whatever_follows_it() {
        let voice = |language: &str| VoiceChoice { language: language.into(), ..Default::default() };
        let speak = || Job::Speak { id: "r1".into(), text: "Hi.".into(), generation: 0 };
        let run = |jobs: Vec<Job>| {
            let mut choice = voice("previous");
            let mut jobs = jobs.into_iter();
            let job = coalesce(jobs.next().unwrap(), jobs, &mut choice);
            let kind = match job {
                Job::Prepare(prepared) => {
                    assert_eq!(prepared, choice, "a Prepare that runs is the choice");
                    "prepare"
                }
                Job::Speak { .. } => "speak",
                Job::Warm => "warm",
            };
            (kind, choice.language)
        };
        assert_eq!(run(vec![Job::Prepare(voice("a")), speak()]), ("speak", "a".into()));
        assert_eq!(run(vec![speak(), Job::Prepare(voice("a"))]), ("speak", "a".into()));
        assert_eq!(run(vec![Job::Prepare(voice("a")), Job::Warm]), ("prepare", "a".into()));
        assert_eq!(run(vec![Job::Prepare(voice("a")), Job::Prepare(voice("b")), speak()]), ("speak", "b".into()));
        assert_eq!(run(vec![Job::Warm, speak()]), ("speak", "previous".into()));
        assert_eq!(run(vec![Job::Prepare(voice("a"))]), ("prepare", "a".into()));
    }

    #[test]
    fn failed_downloads_back_off_to_an_hour() {
        assert_eq!(retry_after(1), Duration::from_secs(60));
        assert_eq!(retry_after(2), Duration::from_secs(120));
        assert_eq!(retry_after(4), Duration::from_secs(480));
        assert_eq!(retry_after(7), Duration::from_secs(3600));
        assert_eq!(retry_after(30), Duration::from_secs(3600));
    }

    #[test]
    fn a_slow_gpu_is_remembered_per_gpu_and_driver_for_30_days() {
        let dir = std::env::temp_dir().join(format!("voice-no-gpu-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("no-gpu.txt");
        let gpu = etalk_crispasr::Gpu { description: "GT 1030".into(), integrated: false, total_bytes: 4_294_967_296, free_bytes: 0 };
        let print = gpu_fingerprint(&gpu, Some("cuda 12080"));
        let now = UNIX_EPOCH + Duration::from_secs(1_800_000_000);
        assert!(!no_gpu_cached(&path, &print, now));
        remember_no_gpu(&path, &print, now);
        assert!(no_gpu_cached(&path, &print, now + Duration::from_secs(3600)));
        assert!(!no_gpu_cached(&path, &gpu_fingerprint(&gpu, Some("cuda 12090")), now), "a new driver re-tests");
        let other = etalk_crispasr::Gpu { description: "RTX 4090".into(), ..gpu.clone() };
        assert!(!no_gpu_cached(&path, &gpu_fingerprint(&other, Some("cuda 12080")), now), "another GPU re-tests");
        assert!(!no_gpu_cached(&path, &print, now + NO_GPU_TTL), "after 30 days it re-tests");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// eTalk device pass: a busy machine's slow warm-up is retried; only
    /// the third in a row is the cached verdict.
    #[test]
    fn only_three_slow_warmups_in_a_row_are_a_verdict() {
        SLOW_WARMUPS.store(0, Ordering::SeqCst);
        assert!(retry_slow_warmup());
        assert!(retry_slow_warmup());
        assert!(!retry_slow_warmup(), "the third is the verdict");
        SLOW_WARMUPS.store(0, Ordering::SeqCst);
        assert!(retry_slow_warmup(), "a fast warm-up starts the count again");
        SLOW_WARMUPS.store(0, Ordering::SeqCst);
    }

    #[test]
    fn a_synthesis_error_never_carries_the_reply() {
        let text = "Der Build ist grün. EXP-1390 ist erledigt.";
        assert_eq!(scrub("failed on: Der Build ist grün. EXP-1390 ist erledigt.", text), "failed on: <reply>");
        assert_eq!(scrub("failed at Der Build ist grün.", text), "failed at <reply>");
        assert_eq!(scrub("qwen3-tts synthesis failed", text), "qwen3-tts synthesis failed");
        assert_eq!(scrub(&"x".repeat(500), text).len(), 200);
    }

    struct FakeSink(std::sync::atomic::AtomicBool);

    impl sink::Sink for FakeSink {
        fn push(&self, _samples: &[f32]) -> Result<(), String> {
            Ok(())
        }
        fn finish(&self) {}
        fn stop(&self) {
            self.0.store(true, Ordering::SeqCst);
        }
    }

    fn recording() -> (Emitter, Arc<Mutex<Vec<Event>>>) {
        let events: Arc<Mutex<Vec<Event>>> = Arc::default();
        let sink = events.clone();
        (Arc::new(move |event: &Event| sink.lock().unwrap().push(event.clone())), events)
    }

    /// NEW-55: barge-in stops the reply and answers it, once, at once.
    #[test]
    fn barge_in_stops_the_reply_and_answers_it_first() {
        let (emitter, events) = recording();
        let shared = Shared::new(Arc::default(), emitter);
        let playing = Arc::new(FakeSink(Default::default()));
        *shared.current.lock().unwrap() = Some(playing.clone());
        *shared.speaking() = Some("m1".into());
        barge(&shared);
        assert!(playing.0.load(Ordering::SeqCst), "the sink stopped");
        assert_eq!(shared.generation.load(Ordering::SeqCst), 1, "the speaker's work is stale");
        assert_eq!(*events.lock().unwrap(), [Event::Finished { id: "m1".into(), stopped: true }]);
        // The engine winding down answers nothing more.
        end_speaking(&shared, "m1", Event::Finished { id: "m1".into(), stopped: true });
        assert_eq!(events.lock().unwrap().len(), 1);
        // Nothing playing: still stale (a queued reply never starts), no answer.
        barge(&shared);
        assert_eq!(shared.generation.load(Ordering::SeqCst), 2);
        assert_eq!(events.lock().unwrap().len(), 1);
    }

    /// NEW-55: a remote call's reply streams in chunks of at most 100 ms.
    #[test]
    fn a_remote_reply_streams_100_ms_chunks_until_stopped() {
        use sink::Sink as _;
        let (emitter, events) = recording();
        let stream = StreamSink::new("m1", 24_000, emitter);
        let pcm: Vec<f32> = (0..6000).map(|index| (index as f32 / 10.0).sin() * 0.5).collect();
        stream.push(&pcm).unwrap();
        let chunks: Vec<Vec<i16>> = events
            .lock()
            .unwrap()
            .iter()
            .map(|event| match event {
                Event::SpeechAudio { id, pcm, rate } => {
                    assert_eq!((id.as_str(), *rate), ("m1", 24_000));
                    etalk::decode_pcm(pcm).unwrap()
                }
                other => panic!("{other:?}"),
            })
            .collect();
        assert_eq!(chunks.iter().map(Vec::len).collect::<Vec<_>>(), [2400, 2400, 1200]);
        assert_eq!(chunks[0][10], etalk::to_i16(pcm[10]));
        stream.stop();
        stream.push(&pcm).unwrap();
        assert_eq!(events.lock().unwrap().len(), 3, "a stopped stream sends nothing");
    }

    /// A clock the test moves: `sleep` advances it.
    #[derive(Default)]
    struct FakeClock(Mutex<Duration>);

    impl Clock for FakeClock {
        fn now(&self) -> Duration {
            *self.0.lock().unwrap()
        }
        fn sleep(&self, duration: Duration) {
            *self.0.lock().unwrap() += duration;
        }
    }

    /// The relay's budget per socket (hub.ts `takeFrameToken`): a bucket of
    /// `maxFramesPerSecond` refilled at `maxFramesPerSecond` per second.
    struct FakeRelay {
        tokens: f64,
        at: Duration,
        forwarded: u64,
        dropped: u64,
    }

    impl FakeRelay {
        fn frame(&mut self, now: Duration) {
            let rate = f64::from(etalk::call::VOICE_CALL.constants.max_frames_per_second);
            self.tokens = (self.tokens + (now - self.at).as_secs_f64() * rate).min(rate);
            self.at = now;
            if self.tokens >= 1.0 {
                self.tokens -= 1.0;
                self.forwarded += 1;
            } else {
                self.dropped += 1;
            }
        }
    }

    /// EXP-1929: a 30 s reply synthesised far faster than real time reaches
    /// the caller whole through the relay's 60 frames/s budget, and the
    /// stream finishes when it has played, not when synthesis ended.
    #[test]
    fn a_long_remote_reply_is_paced_through_the_relays_budget() {
        use sink::Sink as _;
        let clock = Arc::new(FakeClock::default());
        let rate = f64::from(etalk::call::VOICE_CALL.constants.max_frames_per_second);
        let relay = Arc::new(Mutex::new(FakeRelay { tokens: rate, at: Duration::ZERO, forwarded: 0, dropped: 0 }));
        let frame = (24_000 * etalk::call::VOICE_CALL.constants.frame_ms / 1000) as usize;
        let emitter: Emitter = {
            let (clock, relay) = (clock.clone(), relay.clone());
            Arc::new(move |event: &Event| {
                let Event::SpeechAudio { pcm, .. } = event else { return };
                // The host frames each chunk at once (call_host), 20 ms per frame.
                for _ in 0..etalk::decode_pcm(pcm).unwrap().len() / frame {
                    relay.lock().unwrap().frame(clock.now());
                }
            })
        };
        let stream = StreamSink::with_clock("m1", 24_000, emitter, clock.clone());
        // Thirty one-second sentences, each synthesised in no time.
        for _ in 0..30 {
            stream.push(&vec![0.1; 24_000]).unwrap();
        }
        assert!(clock.now() <= Duration::from_secs(30) - Duration::from_millis(100), "the push leads the clock");
        stream.finish();
        let relay = relay.lock().unwrap();
        assert_eq!((relay.forwarded, relay.dropped), (1500, 0), "every frame of the 30 s reply arrives");
        assert!(clock.now() >= Duration::from_secs(30), "finished once played: {:?}", clock.now());
        assert!(clock.now() <= Duration::from_secs(30) + Duration::from_millis(40));
    }

    /// EXP-1929: Stop ends a paced wait at once.
    #[test]
    fn a_stopped_paced_reply_ends_at_once() {
        use sink::Sink as _;
        let clock = Arc::new(FakeClock::default());
        let (emitter, events) = recording();
        let stream = StreamSink::with_clock("m1", 24_000, emitter, clock.clone());
        stream.push(&vec![0.1; 24_000]).unwrap();
        stream.stop();
        let before = clock.now();
        stream.push(&vec![0.1; 24_000]).unwrap();
        stream.finish();
        assert_eq!(clock.now(), before, "a stopped stream waits for nothing");
        assert_eq!(events.lock().unwrap().len(), 10);
    }

    /// EXP-1930 (voice-engine-speak-2): a voice that will be tried again
    /// (a slow warm-up, a failed load) says so before its empty `Voices`; no
    /// GPU says only the empty `Voices`.
    #[test]
    fn a_voice_not_up_yet_is_told_apart_from_no_gpu() {
        let now = Instant::now();
        let slow = State::Idle { failures: 1, retry_at: Some(now + Duration::from_secs(60)) };
        assert_eq!(not_up(&slow, now), vec![Event::VoiceRetry { in_ms: 60_000 }, Event::Voices { voices: Vec::new() }]);
        assert_eq!(not_up(&State::NoGpu, now), vec![Event::Voices { voices: Vec::new() }]);
    }

    /// NEW-55: the first clause of a reply is synthesized on its own.
    #[test]
    fn the_first_clause_splits_off() {
        assert_eq!(first_clause("Three issues are open: NEW-55, NEW-56 and NEW-79."), Some(("Three issues are open:", "NEW-55, NEW-56 and NEW-79.")));
        assert_eq!(first_clause("Okay, I moved it."), None, "one word in front");
        assert_eq!(first_clause("Sure thing, I moved it."), Some(("Sure thing,", "I moved it.")));
        assert_eq!(first_clause("The build is green - merging now."), Some(("The build is green", "merging now.")));
        assert_eq!(first_clause("Done; the PR is open."), None);
        assert_eq!(first_clause("All right then; the PR is open."), Some(("All right then;", "the PR is open.")));
        assert_eq!(first_clause("One two three four five six seven eight nine, ten."), None, "nine words in front");
        assert_eq!(first_clause("One two three four five six seven eight, nine."), Some(("One two three four five six seven eight,", "nine.")));
        assert_eq!(first_clause("Okay, so the plan is set, go."), None, "only the first mark counts");
        assert_eq!(first_clause("It costs 1,000 euros, roughly."), Some(("It costs 1,000 euros,", "roughly.")), "a digit comma is no mark");
        assert_eq!(first_clause("It is 12:30 now, so lunch."), Some(("It is 12:30 now,", "so lunch.")));
        assert_eq!(first_clause("NEW-55 and NEW-56 are open."), None, "a hyphen is no dash");
        assert_eq!(first_clause("Two words,"), None, "nothing behind it");
    }

    #[test]
    fn speech_chunks_replay_voice_json() {
        let fixture: serde_json::Value =
            serde_json::from_str(include_str!("../../../../../packages/etalk/fixtures/voice.json")).unwrap();
        let cases = fixture["speechChunkCases"].as_array().unwrap();
        assert!(!cases.is_empty());
        for case in cases {
            let chunks: Vec<&str> = case["chunks"].as_array().unwrap().iter().map(|chunk| chunk.as_str().unwrap()).collect();
            assert_eq!(
                speech_chunks(case["text"].as_str().unwrap(), case["streaming"].as_bool().unwrap()),
                chunks,
                "{}",
                case["name"]
            );
        }
    }

    #[test]
    fn replies_split_into_sentences() {
        assert_eq!(sentences("Done. The PR is open! Merge it? yes"), ["Done.", "The PR is open!", "Merge it?", "yes"]);
        assert_eq!(sentences("E X P dreizehn neunzig ist erledigt."), ["E X P dreizehn neunzig ist erledigt."]);
        assert_eq!(sentences("Version 1.5 ships."), ["Version 1.5 ships."], "a decimal point is no sentence end");
        assert!(sentences("  ").is_empty());
    }

    /// #EXP-2016: a speech model that never ends a sentence (hundreds of
    /// consonants: it would run to its ceiling) is cut at the sentence's
    /// budget, the next sentence goes on, and the next reply is spoken.
    #[test]
    fn a_sentence_the_model_never_ends_is_cut_and_the_next_reply_is_spoken() {
        struct Endless {
            calls: Vec<String>,
        }
        impl Synth for Endless {
            fn synthesize(&mut self, text: &str, on_audio: &mut dyn FnMut(&[f32])) -> Result<(), String> {
                self.calls.push(text.to_string());
                // "Never" ends: 340 s of audio (4096 frames), one second at a time.
                let seconds = if text.starts_with("Endless") { 340 } else { 1 };
                for _ in 0..seconds {
                    on_audio(&[0.1; 24_000]);
                }
                Ok(())
            }
            fn streams(&self) -> bool {
                true
            }
        }
        let mut synth = Endless { calls: Vec::new() };
        let mut speak = |text: &str| {
            let (mut pushed, mut started) = (0usize, 0);
            let failed = speak_parts(&mut synth, text, 24_000, true, &|| true, &mut || started += 1, &mut |pcm| {
                pushed += pcm.len();
                Ok(())
            });
            (failed, pushed, started)
        };
        let (failed, pushed, started) = speak("Endless bcdfg. Then this.");
        assert_eq!((failed, started), (None, 1));
        assert_eq!(pushed, speech_budget("Endless bcdfg.", 24_000) + 24_000, "cut at its budget, the next sentence whole");
        assert_eq!(speech_budget("Endless bcdfg.", 24_000), 10 * 24_000, "a short sentence gets the floor");
        assert_eq!(speak("The next reply."), (None, 24_000, 1), "the next reply is spoken");
        assert_eq!(synth.calls, ["Endless bcdfg.", "Then this.", "The next reply."]);
        assert_eq!(speech_budget(&"word ".repeat(200), 24_000), 60 * 24_000, "never past the ceiling");
    }
}
