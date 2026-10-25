//! NEW-53: exponentialTalk's engines, the speech worker's side.
//!
//! [`run_and_exit`] is the whole worker: the host's executable re-run as
//! `<exe> __speech-worker --data-dir <dir>` (see `crates/talk`). It reads
//! [`talk::Request`] lines from stdin, answers with [`talk::Event`] lines on
//! stdout, and logs to stderr (the host forwards it to its log). A host
//! that dies closes stdin; the worker sees EOF and exits.
//!
//! Slice (b): the Kokoro voice ([`model::KOKORO`], English, speaker 0),
//! synthesized sentence by sentence through sherpa-onnx and played as it
//! arrives ([`sink`]). A new `Speak` replaces the one playing; `Stop` ends
//! it. Each request carries a generation number; synthesis and playback of
//! an older generation stop at their next chunk.

pub mod model;
pub mod sink;

use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use talk::{Event, Request};

/// The Kokoro speaker id (the voice the owner heard and chose).
const SPEAKER: i32 = 0;

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

fn emit(event: &Event) {
    let Ok(line) = serde_json::to_string(event) else { return };
    let mut stdout = std::io::stdout().lock();
    let _ = writeln!(stdout, "{line}");
    let _ = stdout.flush();
}

enum Job {
    Prepare,
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
    let shared = Arc::new(Shared { generation: AtomicU64::new(0), current: Mutex::new(None) });
    let (tx, rx) = mpsc::channel::<Job>();
    let engine_shared = shared.clone();
    let engine = std::thread::Builder::new()
        .name("voice-engine".into())
        .spawn(move || engine_loop(&data_dir, rx, &engine_shared))
        .expect("spawn the voice engine");

    for line in std::io::stdin().lock().lines() {
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
            Request::Prepare => {
                let _ = tx.send(Job::Prepare);
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
        }
    }
    // EOF: the host is gone. Silence and let the engine wind down.
    shared.generation.fetch_add(1, Ordering::SeqCst);
    silence(&shared);
    drop(tx);
    let _ = engine.join();
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
fn retry_after(failures: u32) -> Duration {
    Duration::from_secs(60u64.saturating_mul(1u64 << failures.saturating_sub(1).min(6))).min(Duration::from_secs(3600))
}

fn engine_loop(data_dir: &Path, rx: Receiver<Job>, shared: &Arc<Shared>) {
    let mut tts: Option<sherpa_onnx::OfflineTts> = None;
    let mut failures = 0u32;
    let mut retry_at: Option<Instant> = None;
    while let Ok(first) = rx.recv() {
        // Only the newest Speak matters: drain what queued up behind it.
        let mut job = first;
        while let Ok(next) = rx.try_recv() {
            if matches!(next, Job::Speak { .. }) || matches!(job, Job::Prepare) {
                job = next;
            }
        }
        if tts.is_none() {
            if retry_at.is_some_and(|at| Instant::now() < at) {
                // Backing off: the voice is not tried again yet.
                if let Job::Speak { id, .. } = job {
                    emit(&Event::Failed { id, error: "the voice is unavailable for now".into() });
                }
                continue;
            }
            match load(data_dir) {
                Ok(engine) => {
                    tts = Some(engine);
                    failures = 0;
                    retry_at = None;
                    emit(&Event::ModelReady);
                }
                Err(err) => {
                    failures += 1;
                    let wait = retry_after(failures);
                    retry_at = Some(Instant::now() + wait);
                    eprintln!("voice unavailable (attempt {failures}, next in {}s): {err}", wait.as_secs());
                    emit(&Event::DownloadFailed { error: err.clone() });
                    if let Job::Speak { id, .. } = job {
                        emit(&Event::Failed { id, error: err });
                    }
                    continue;
                }
            }
        }
        if let (Job::Speak { id, text, generation }, Some(engine)) = (job, tts.as_ref()) {
            speak(engine, shared, &id, &text, generation);
        }
    }
}

fn load(data_dir: &Path) -> Result<sherpa_onnx::OfflineTts, String> {
    let dir = model::ensure(data_dir, &model::KOKORO, |received, total| {
        emit(&Event::Downloading { received, total });
    })?;
    let path = |name: &str| Some(dir.join(name).to_string_lossy().into_owned());
    let config = sherpa_onnx::OfflineTtsConfig {
        model: sherpa_onnx::OfflineTtsModelConfig {
            kokoro: sherpa_onnx::OfflineTtsKokoroModelConfig {
                model: path("model.onnx"),
                voices: path("voices.bin"),
                tokens: path("tokens.txt"),
                // sherpa-onnx 2.0: the lexicon replaces espeak-ng.
                lexicon: path("lexicon-us-en.txt"),
                length_scale: 1.0,
                lang: Some("en".into()),
                ..Default::default()
            },
            num_threads: synthesis_threads(),
            provider: Some("cpu".into()),
            ..Default::default()
        },
        // One sentence per callback: the first audio after one sentence.
        max_num_sentences: 1,
        silence_scale: 0.2,
        ..Default::default()
    };
    sherpa_onnx::OfflineTts::create(&config).ok_or_else(|| "the voice model failed to load".to_string())
}

fn synthesis_threads() -> i32 {
    std::thread::available_parallelism().map(|n| (n.get() / 2).clamp(1, 4) as i32).unwrap_or(2)
}

fn speak(engine: &sherpa_onnx::OfflineTts, shared: &Arc<Shared>, id: &str, text: &str, generation: u64) {
    if shared.generation.load(Ordering::SeqCst) != generation {
        emit(&Event::Finished { id: id.into(), stopped: true });
        return;
    }
    let sink = match sink::open(engine.sample_rate() as u32) {
        Ok(sink) => sink,
        Err(err) => {
            emit(&Event::Failed { id: id.into(), error: err });
            return;
        }
    };
    if let Ok(mut current) = shared.current.lock() {
        *current = Some(sink.clone());
    }
    let started = Arc::new(Mutex::new(false));
    let callback_shared = shared.clone();
    let callback_sink = sink.clone();
    let callback_started = started.clone();
    let callback_id = id.to_string();
    let config = sherpa_onnx::GenerationConfig { sid: SPEAKER, speed: 1.0, ..Default::default() };
    let _ = engine.generate_with_config(
        text,
        &config,
        Some(move |samples: &[f32], _progress: f32| {
            if callback_shared.generation.load(Ordering::SeqCst) != generation {
                return false;
            }
            if let Ok(mut started) = callback_started.lock() {
                if !*started && !samples.is_empty() {
                    *started = true;
                    emit(&Event::Started { id: callback_id.clone() });
                }
            }
            callback_sink.push(samples);
            true
        }),
    );
    let current_generation = shared.generation.load(Ordering::SeqCst) == generation;
    if current_generation {
        sink.finish();
    } else {
        sink.stop();
    }
    let still_current = shared.generation.load(Ordering::SeqCst) == generation;
    if let Ok(mut current) = shared.current.lock() {
        if current.as_ref().is_some_and(|playing| Arc::ptr_eq(playing, &sink)) {
            *current = None;
        }
    }
    emit(&Event::Finished { id: id.into(), stopped: !still_current });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn failed_downloads_back_off_to_an_hour() {
        assert_eq!(retry_after(1), Duration::from_secs(60));
        assert_eq!(retry_after(2), Duration::from_secs(120));
        assert_eq!(retry_after(4), Duration::from_secs(480));
        assert_eq!(retry_after(7), Duration::from_secs(3600));
        assert_eq!(retry_after(30), Duration::from_secs(3600));
    }
}
