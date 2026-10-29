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
//! at once and RINGS on it for `ring_ms` ([`ring_tone`]; a remote call rings
//! on the caller, never here) while the worker warms up: the voice is
//! brought up ([`Job::Warm`]) and the listener loads the recognizer (a
//! download included) and opens the microphone. `CallListening` follows
//! once both are settled ([`listener::Links::voice_ready`]). The ring stops
//! at the first Speak (the greeting), a Stop, Hangup or the next call.

pub mod builtin;
pub mod capture;
pub mod echo;
pub mod listen;
pub mod listener;
pub mod meeting;
pub mod runtime;
pub mod sink;
pub mod wake;

use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
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
/// S-3: the ring's pitch and cadence: a burst, then a pause (two bursts in
/// voice-call.json's 3 s `ringMs`).
const RING_HZ: f32 = 425.0;
const RING_ON_MS: u32 = 1_000;
const RING_OFF_MS: u32 = 500;
/// Each burst fades in and out over this long (no click).
const RING_FADE_MS: u32 = 20;

/// S-3: `ms` of the ring at `rate`: [`RING_HZ`] bursts of [`RING_ON_MS`]
/// with [`RING_OFF_MS`] of silence after each, cut at `ms`.
pub fn ring_tone(rate: u32, ms: u32) -> Vec<f32> {
    let at = |ms: u32| (u64::from(rate) * u64::from(ms) / 1000) as usize;
    let (on, period, fade, total) = (at(RING_ON_MS), at(RING_ON_MS + RING_OFF_MS), at(RING_FADE_MS).max(1), at(ms));
    (0..total)
        .map(|index| {
            let phase = index % period;
            if phase >= on {
                return 0.0;
            }
            let envelope = (phase.min(on - 1 - phase) as f32 / fade as f32).min(1.0);
            let t = index as f32 / rate as f32;
            RING_AMPLITUDE * envelope * (std::f32::consts::TAU * RING_HZ * t).sin()
        })
        .collect()
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

/// #NEW-92: how long after a reply ended its words still count as echo
/// (its echo tail, the segment's closing silence, a CPU transcription).
pub(crate) const SPOKEN_HOLD: Duration = Duration::from_secs(3);

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

    /// S-3: ring local call `id` for `ms` on its output stream (opened now,
    /// the stream every reply of the call plays on). `false` = no stream.
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
        if let Err(err) = sink::Sink::push(&*ring, &ring_tone(etalk_crispasr::QWEN3_TTS_RATE, ms)) {
            eprintln!("call {id}: the ring did not play: {err}");
            return true;
        }
        eprintln!("call {id}: ringing for {ms} ms");
        let old = self.ring.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).replace((id.to_string(), ring));
        if let Some((_, old)) = old {
            sink::Sink::stop(&*old);
        }
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
        spoken
            .iter()
            .filter(|(_, ended)| ended.is_none_or(|ended| ended.elapsed() < SPOKEN_HOLD))
            .flat_map(|(sentences, _)| sentences.iter().cloned())
            .collect()
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

fn run(data_dir: PathBuf) {
    if log::set_logger(&StderrLog).is_ok() {
        log::set_max_level(log::LevelFilter::Info);
    }
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
                shared.generation.fetch_add(1, Ordering::SeqCst);
                silence(&shared);
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
            Request::CallAudio { id, pcm } => match etalk::decode_pcm(&pcm) {
                Ok(pcm) => {
                    let samples = pcm.into_iter().map(|sample| f32::from(sample) / 32768.0).collect();
                    let _ = listen.send(listener::Job::Audio { id, samples });
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
            Request::Wake { phrase, language } => {
                let _ = listen.send(listener::Job::Wake { phrase, language });
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
        // Only the newest Speak matters; the newest Prepare is the choice.
        let mut job = first;
        while let Ok(next) = rx.try_recv() {
            match next {
                Job::Prepare(voice) => {
                    choice = voice;
                    if !matches!(job, Job::Speak { .. }) {
                        job = Job::Prepare(choice.clone());
                    }
                }
                // Any job brings the voice up.
                Job::Warm => {}
                speak => job = speak,
            }
        }
        if let Job::Prepare(voice) = &job {
            choice = voice.clone();
        }
        if let State::Idle { failures, retry_at } = state {
            if retry_at.is_some_and(|at| Instant::now() < at) {
                // Backing off: the voice is not tried again yet (settled:
                // a call need not wait for it).
                shared.voice_settled.store(true, Ordering::SeqCst);
                emit(&Event::Voices { voices: Vec::new() });
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
                Err(err) => {
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
            emit(&Event::Voices { voices: Vec::new() });
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

/// Probe, download, verify, load and warm up. `Ok(None)` = no usable GPU
/// (permanent for this worker); `Err` = a failure worth retrying later.
fn bring_up(data_dir: &Path) -> Result<Option<Loaded>, String> {
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
        eprintln!("no usable GPU: the voice runs slower than {MAX_WARMUP_RTF} of real time");
        remember_no_gpu(&cache, &fingerprint, SystemTime::now());
        return Ok(None);
    }
    Ok(Some(loaded))
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
/// chunk being sent finishes before `stop` returns.
pub struct StreamSink {
    id: String,
    rate: u32,
    stopped: Mutex<bool>,
    emit: Emitter,
}

impl StreamSink {
    fn new(id: &str, rate: u32, emit: Emitter) -> Self {
        StreamSink { id: id.into(), rate, stopped: Mutex::new(false), emit }
    }
}

impl sink::Sink for StreamSink {
    fn push(&self, samples: &[f32]) -> Result<(), String> {
        let chunk = (self.rate * etalk::SPEECH_AUDIO_CHUNK_MS / 1000).max(1) as usize;
        for part in samples.chunks(chunk) {
            let stopped = self.stopped.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
            if *stopped {
                return Ok(());
            }
            let pcm: Vec<i16> = part.iter().map(|sample| etalk::to_i16(*sample)).collect();
            (self.emit)(&Event::SpeechAudio { id: self.id.clone(), pcm: etalk::encode_pcm(&pcm), rate: self.rate });
        }
        Ok(())
    }

    fn finish(&self) {}

    fn stop(&self) {
        *self.stopped.lock().unwrap_or_else(|poisoned| poisoned.into_inner()) = true;
    }
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
    let mut started = false;
    let mut failed = None;
    for part in speech_chunks(text, loaded.session.streams()) {
        if !current(shared) {
            break;
        }
        // NEW-91: on a streaming speaker the audio arrives in chunks while
        // the sentence is still being synthesized.
        let mut broken = false;
        let result = loaded.session.synthesize(part, &mut |pcm| {
            if broken || pcm.is_empty() || !current(shared) {
                return;
            }
            if remote && !started {
                // The stream sits between Started and Finished.
                started = true;
                emit(Event::Started { id: id.into() });
            }
            // #1231 review: a player that takes no audio (pw-play without a
            // PipeWire session) fails the reply, never reports it played.
            if let Err(err) = sink.push(pcm) {
                eprintln!("audio output failed: {err}");
                if !started {
                    failed = Some(err);
                }
                broken = true;
                return;
            }
            if !started {
                started = true;
                emit(Event::Started { id: id.into() });
            }
        });
        if broken {
            break;
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

    /// S-3: the ring: soft 425 Hz bursts, 1 s on and 0.5 s off, cut at
    /// `ring_ms` (two bursts in 3 s), no click at a burst's edges.
    #[test]
    fn the_ring_is_two_soft_bursts_in_three_seconds() {
        let rate = 24_000;
        let ring = ring_tone(rate, 3_000);
        assert_eq!(ring.len(), 72_000);
        let peak = |from: usize, to: usize| ring[from..to].iter().fold(0.0f32, |peak, sample| peak.max(sample.abs()));
        assert!(peak(0, ring.len()) <= RING_AMPLITUDE);
        assert!(peak(6_000, 18_000) > RING_AMPLITUDE * 0.9, "the first burst");
        assert_eq!(peak(24_000, 36_000), 0.0, "the pause");
        assert!(peak(42_000, 54_000) > RING_AMPLITUDE * 0.9, "the second burst");
        assert_eq!(peak(60_000, 72_000), 0.0, "the pause after it");
        assert!(ring[0].abs() < 1e-6 && ring[23_999].abs() < 0.01, "faded in and out");
        assert!(ring_tone(rate, 0).is_empty());
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
}
