//! NEW-53: exponentialTalk's engines, the speech worker's side.
//!
//! [`run_and_exit`] is the whole worker: the host's executable re-run as
//! `<exe> __speech-worker --data-dir <dir>` (see `crates/talk`). It reads
//! [`talk::Request`] lines from stdin, answers with [`talk::Event`] lines on
//! stdout, and logs to stderr (the host forwards it to its log). A host
//! that dies closes stdin; the worker sees EOF and exits.
//!
//! NEW-74: ONE engine for every language, Qwen3-TTS 0.6B Base (F16) through
//! CrispASR on the GPU, cloning zero-shot from the member's reference clip
//! (`voiceFile` + `voiceTranscript`, validated by
//! [`talk::custom_voice_file`]) or the built-in clip of the language
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

pub mod builtin;
pub mod capture;
pub mod listen;
pub mod listener;
pub mod runtime;
pub mod sink;

use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crispasr::{artifact, models, Build, Device, TtsSession};
use talk::{Event, Request, VoiceChoice};

/// The CrispASR backend of the model.
const BACKEND: &str = "qwen3-tts";
/// The warm-up must run at least this much faster than real time (the
/// 4090 measured 0.13-0.27, the CPU ~2.0).
const MAX_WARMUP_RTF: f64 = 0.6;
/// A GPU needs this much memory for the F16 model (measured peak 3.7 GB).
const MIN_GPU_BYTES: u64 = 4_000_000_000;

/// Run the worker to completion and exit the process. Call it from `main`
/// when [`talk::requested`] answers; it never returns.
pub fn run_and_exit() -> ! {
    let Some(data_dir) = talk::data_dir_arg() else {
        eprintln!("speech worker: missing {}", talk::DATA_DIR_FLAG);
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
}

struct Shared {
    /// Bumped by every Speak and Stop; work of an older value stops.
    generation: AtomicU64,
    /// The sink playing now, so Stop can silence it from the reader thread.
    current: Mutex<Option<Arc<dyn sink::Sink>>>,
}

fn run(data_dir: PathBuf) {
    emit(&Event::Ready { protocol: talk::PROTOCOL });
    remove_retired_voices(&data_dir);
    let ears = data_dir.clone();
    serve(std::io::stdin().lock(), data_dir, move || listener::Ears::new(ears));
}

/// Answer `input`'s requests until EOF, then wind the engines down and
/// return (the listener on `hearing`, which `make` builds on its thread).
fn serve<H: listener::Hearing>(input: impl BufRead, data_dir: PathBuf, hearing: impl FnOnce() -> H + Send + 'static) {
    let shared = Arc::new(Shared { generation: AtomicU64::new(0), current: Mutex::new(None) });
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
    let listening = Arc::new(listener::Listening::default());
    let (listen, listener) = listener::spawn(hearing, listening.clone());

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
            Request::Speak { id, .. } if listening.active() => {
                // Never into the open microphone: a reply that arrives
                // mid-Listen is dropped, as barge-in would have stopped it.
                emit(&Event::Finished { id, stopped: true });
            }
            Request::Speak { id, text } => {
                let generation = shared.generation.fetch_add(1, Ordering::SeqCst) + 1;
                silence(&shared);
                let _ = tx.send(Job::Speak { id, text, generation });
            }
            Request::Stop => {
                shared.generation.fetch_add(1, Ordering::SeqCst);
                silence(&shared);
            }
            Request::Listen { id, language } => {
                // Barge-in: the person talks over the reply.
                listening.begin(&id);
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
        }
    }
    // EOF: the host is gone. Silence and let the engines wind down; the
    // listener closes any open microphone.
    shared.generation.fetch_add(1, Ordering::SeqCst);
    silence(&shared);
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

/// The engine once it is up: the runtime lives for the process.
struct Loaded {
    session: TtsSession<'static>,
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
                speak => job = speak,
            }
        }
        if let Job::Prepare(voice) = &job {
            choice = voice.clone();
        }
        if let State::Idle { failures, retry_at } = state {
            if retry_at.is_some_and(|at| Instant::now() < at) {
                // Backing off: the voice is not tried again yet.
                emit(&Event::Voices { voices: Vec::new() });
                if let Job::Speak { id, .. } = job {
                    emit(&Event::Failed { id, error: "the voice is unavailable for now".into() });
                }
                continue;
            }
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
            emit(&Event::Voices { voices: Vec::new() });
            if let Job::Speak { id, .. } = job {
                emit(&Event::Failed { id, error: "no usable GPU for the voice".into() });
            }
            continue;
        };
        if loaded.voice.as_ref() != Some(&choice) {
            if let Err(err) = install_voice(data_dir, loaded, &choice) {
                eprintln!("{err}");
            }
        }
        match job {
            Job::Prepare(_) => emit(&Event::Voices { voices: builtin::languages() }),
            Job::Speak { id, text, generation } => speak(loaded, shared, &id, &text, generation),
        }
    }
}

/// Probe, download, verify, load and warm up. `Ok(None)` = no usable GPU
/// (permanent for this worker); `Err` = a failure worth retrying later.
fn bring_up(data_dir: &Path) -> Result<Option<Loaded>, String> {
    if !crispasr::platform::supported() || !crispasr::platform::gpu_driver_present() {
        eprintln!("no usable GPU: no GPU driver for the voice on this machine");
        return Ok(None);
    }
    let models_dir = data_dir.join("talk").join(models::MODELS_DIR);
    // NEW-78: no runtime bytes to fetch when the transcriber loaded it.
    let runtime: &[artifact::Artifact] = if runtime::loaded().is_some() { &[] } else { crispasr::platform::ARTIFACTS };
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
    let fingerprint = gpu_fingerprint(gpu, crispasr::platform::driver_version().as_deref());
    let cache = no_gpu_cache_path(data_dir);
    if no_gpu_cached(&cache, &fingerprint, SystemTime::now()) {
        eprintln!("no usable GPU: this GPU and driver were too slow before (cached; a driver change or 30 days re-test)");
        return Ok(None);
    }
    artifact::ensure_all(&models_dir, &model_files, |received, _| report(runtime_total + received))
        .map_err(|err| err.to_string())?;
    // The GGUFs are re-hashed before ggml parses them (~1-2 s for 2.2 GB).
    let hashing = Instant::now();
    for model in &model_files {
        artifact::verify(&models_dir, model).map_err(|err| err.to_string())?;
    }
    eprintln!("voice models verified in {} ms", hashing.elapsed().as_millis());
    let started = Instant::now();
    let session = runtime.open_tts(
        BACKEND,
        &models_dir.join(models::QWEN3_TTS_0_6B_BASE.keep[0].1),
        &models_dir.join(models::QWEN3_TTS_CODEC.keep[0].1),
        Device::Gpu,
    )?;
    let sample_rate = session.sample_rate();
    let mut loaded = Loaded { session, sample_rate, voice: None };
    let english = VoiceChoice { language: "en".into(), ..Default::default() };
    install_voice(data_dir, &mut loaded, &english)?;
    eprintln!("voice loaded in {} ms", started.elapsed().as_millis());
    // Two tries: the first call of a cold GPU pipeline compiles its shaders.
    let mut best = f64::MAX;
    for _ in 0..2 {
        let started = Instant::now();
        let pcm = loaded.session.synthesize("Hello, this is a quick voice check.")?;
        let audio = pcm.len() as f64 / f64::from(sample_rate);
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
fn gpu_fingerprint(gpu: &crispasr::Gpu, driver: Option<&str>) -> String {
    format!(
        "{} | {} | {} MB | {} | max rtf {MAX_WARMUP_RTF}",
        crispasr::platform::RUNTIME_DIR,
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
    let custom = match talk::custom_voice_file(&choice.file, &choice.transcript) {
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

fn speak(loaded: &mut Loaded, shared: &Arc<Shared>, id: &str, text: &str, generation: u64) {
    let current = |shared: &Shared| shared.generation.load(Ordering::SeqCst) == generation;
    if !current(shared) {
        emit(&Event::Finished { id: id.into(), stopped: true });
        return;
    }
    let sink = match sink::open(loaded.sample_rate) {
        Ok(sink) => sink,
        Err(err) => {
            eprintln!("audio output failed: {err}");
            emit(&Event::Failed { id: id.into(), error: err });
            return;
        }
    };
    if let Ok(mut playing) = shared.current.lock() {
        *playing = Some(sink.clone());
    }
    let mut started = false;
    for sentence in sentences(text) {
        if !current(shared) {
            break;
        }
        match loaded.session.synthesize(sentence) {
            Ok(pcm) => {
                if !current(shared) {
                    break;
                }
                // #1231 review: a player that takes no audio (pw-play
                // without a PipeWire session) fails the reply, never
                // reports it played.
                if let Err(err) = sink.push(&pcm) {
                    eprintln!("audio output failed: {err}");
                    if !started {
                        sink.stop();
                        clear_current(shared, &sink);
                        emit(&Event::Failed { id: id.into(), error: err });
                        return;
                    }
                    break;
                }
                if !started && !pcm.is_empty() {
                    started = true;
                    emit(&Event::Started { id: id.into() });
                }
            }
            Err(err) => {
                let err = scrub(&err, text);
                eprintln!("synthesis failed: {err}");
                if !started {
                    sink.stop();
                    clear_current(shared, &sink);
                    emit(&Event::Failed { id: id.into(), error: err });
                    return;
                }
                break;
            }
        }
    }
    if current(shared) {
        sink.finish();
    } else {
        sink.stop();
    }
    let still_current = current(shared);
    clear_current(shared, &sink);
    emit(&Event::Finished { id: id.into(), stopped: !still_current });
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
        let gpu = crispasr::Gpu { description: "GT 1030".into(), integrated: false, total_bytes: 4_294_967_296, free_bytes: 0 };
        let print = gpu_fingerprint(&gpu, Some("cuda 12080"));
        let now = UNIX_EPOCH + Duration::from_secs(1_800_000_000);
        assert!(!no_gpu_cached(&path, &print, now));
        remember_no_gpu(&path, &print, now);
        assert!(no_gpu_cached(&path, &print, now + Duration::from_secs(3600)));
        assert!(!no_gpu_cached(&path, &gpu_fingerprint(&gpu, Some("cuda 12090")), now), "a new driver re-tests");
        let other = crispasr::Gpu { description: "RTX 4090".into(), ..gpu.clone() };
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

    #[test]
    fn replies_split_into_sentences() {
        assert_eq!(sentences("Done. The PR is open! Merge it? yes"), ["Done.", "The PR is open!", "Merge it?", "yes"]);
        assert_eq!(sentences("E X P dreizehn neunzig ist erledigt."), ["E X P dreizehn neunzig ist erledigt."]);
        assert_eq!(sentences("Version 1.5 ships."), ["Version 1.5 ships."], "a decimal point is no sentence end");
        assert!(sentences("  ").is_empty());
    }
}
