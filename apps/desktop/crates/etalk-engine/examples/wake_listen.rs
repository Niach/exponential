//! #NEW-126: what the wake word costs while it LISTENS, and what it still
//! hears, on WAV files instead of the microphone (no device is opened).
//!
//!   cargo run --release -p etalk-engine --example wake_listen -- <data-dir> idle
//!       [--phrase "Hey spezl"] [--seconds 120] [--settle 15] [--rate 48000] [--chunk-ms 10] [room.wav]
//!   cargo run --release -p etalk-engine --example wake_listen -- <data-dir> clips
//!       [--phrase "Hey spezl"] [--classifier <file.onnx> --threshold 0.5] [--lead 4] <clip.wav | dir>...
//!
//! `idle`: the worker's own listening thread (`etalk_engine::listener`, the
//! real engines) hears a room for `--seconds`, in real time, from a stand-in
//! microphone: `room.wav` looped (default: a quiet room, noise at about
//! -55 dBFS), delivered as a capture device does, `--chunk-ms` of `--rate`
//! audio at a time through the capture's own resampler (cpal on macOS and
//! Windows: 48 kHz in 10 ms callbacks; `--rate 16000 --chunk-ms 64` is
//! Linux's `pw-record`). Prints the process's CPU time (user + system, every
//! thread) over the window: the share of one core listening takes. A phrase
//! with a published model fetches it on first use and is measured once the
//! model listens (`--settle` seconds after the microphone opened).
//!
//! `clips`: every clip, after `--lead` seconds of that quiet room, through
//! the scan (`etalk_engine::wake::scan`: Silero and the phrase's model) in
//! 20 ms captures as fast as it decodes: where each clip wakes (samples from
//! its start). `--classifier` scores with another classifier than the
//! phrase's own (the tests' `hey_livekit.onnx`). Clips under a directory
//! named `positives` should wake, every other clip should not.
//!
//! Prints one JSON line.
//!
//! macOS: the speech runtime loads only from the running executable's
//! bundle, so run the built example from a stand-in one
//! (`WL.app/Contents/MacOS/wake_listen`, with `WL.app/Contents/Frameworks/`
//! copied from an installed Exponential.app); a bare `cargo run` never
//! listens there.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::sync::Arc;
use std::time::{Duration, Instant};

use etalk::{Event, ListenFailure};
use etalk_engine::capture::{offer, read_wav, Resampler, CAPACITY};
use etalk_engine::listen::RATE;
use etalk_engine::listener::{self, Ears, Engines, Hearing, Job, Links, Listening, Mic, SileroVad};
use etalk_engine::wake::{scan, ModelDetector, WakeScanner};

/// A quiet room: low-passed noise at about -55 dBFS, at `rate`.
fn room(rate: usize, seconds: usize) -> Vec<f32> {
    let mut state = 0x2545_f491_4f6c_dd1du64;
    let mut low = 0.0f32;
    // One pole at about 230 Hz, whatever the rate.
    let keep = (-std::f32::consts::TAU * 230.0 / rate as f32).exp();
    (0..rate * seconds)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            let white = (state >> 40) as f32 / (1u64 << 23) as f32 - 1.0;
            low = keep * low + (1.0 - keep) * white;
            0.02 * low + 0.0007 * white
        })
        .collect()
}

/// User + system CPU seconds of this process, every thread.
#[cfg(unix)]
fn cpu_seconds() -> f64 {
    let mut usage = std::mem::MaybeUninit::<libc::rusage>::zeroed();
    // SAFETY: getrusage fills the struct it is given.
    let usage = unsafe {
        libc::getrusage(libc::RUSAGE_SELF, usage.as_mut_ptr());
        usage.assume_init()
    };
    let seconds = |time: libc::timeval| time.tv_sec as f64 + time.tv_usec as f64 / 1e6;
    seconds(usage.ru_utime) + seconds(usage.ru_stime)
}

#[cfg(not(unix))]
fn cpu_seconds() -> f64 {
    f64::NAN
}

/// The stand-in microphone: `audio` (at `rate`) looped in real time,
/// `chunk` samples at a time through the capture's resampler.
struct FileMic {
    samples: Receiver<Vec<f32>>,
    closed: Arc<AtomicBool>,
}

impl FileMic {
    fn open(audio: Arc<Vec<f32>>, rate: usize, chunk: usize) -> Self {
        let (tx, samples) = std::sync::mpsc::sync_channel(CAPACITY);
        let closed = Arc::new(AtomicBool::new(false));
        let stop = closed.clone();
        std::thread::Builder::new()
            .name("voice-capture".into())
            .spawn(move || {
                let mut resampler = Resampler::new(rate as u32, RATE as u32);
                let (mut at, mut sent, mut dropped) = (0usize, 0u64, 0u64);
                let started = Instant::now();
                while !stop.load(Ordering::SeqCst) {
                    let captured: Vec<f32> = (0..chunk).map(|index| audio[(at + index) % audio.len()]).collect();
                    at = (at + chunk) % audio.len();
                    let mut out = Vec::with_capacity(captured.len());
                    resampler.process(&captured, &mut out);
                    if !offer(&tx, out, &mut dropped) {
                        return;
                    }
                    sent += chunk as u64;
                    let due = Duration::from_secs_f64(sent as f64 / rate as f64);
                    std::thread::sleep(due.saturating_sub(started.elapsed()));
                }
            })
            .expect("the capture thread");
        FileMic { samples, closed }
    }
}

impl Drop for FileMic {
    fn drop(&mut self) {
        self.closed.store(true, Ordering::SeqCst);
    }
}

impl Mic for FileMic {
    fn recv_timeout(&self, timeout: Duration) -> Result<Vec<f32>, RecvTimeoutError> {
        self.samples.recv_timeout(timeout)
    }
}

/// The worker's ears with the stand-in microphone.
struct FileEars {
    ears: Ears,
    audio: Arc<Vec<f32>>,
    rate: usize,
    chunk: usize,
}

impl Hearing for FileEars {
    type Engines = Engines;
    type Vad = SileroVad;
    type Mic = FileMic;

    fn prepare(&mut self, _microphone: bool) -> Result<(), (ListenFailure, String)> {
        // No device opens here: no permission to ask for.
        self.ears.prepare(false)
    }

    fn downloading(&self) -> bool {
        self.ears.downloading()
    }

    fn poll(&mut self) {
        self.ears.poll()
    }

    fn vad(&mut self) -> Result<SileroVad, (ListenFailure, String)> {
        self.ears.vad()
    }

    fn mic(&mut self) -> Result<FileMic, (ListenFailure, String)> {
        Ok(FileMic::open(self.audio.clone(), self.rate, self.chunk))
    }

    fn engines(&mut self) -> &mut Engines {
        self.ears.engines()
    }

    fn wake_model(&mut self, phrase: &str) -> Option<etalk_wakeword::Detector> {
        self.ears.wake_model(phrase)
    }
}

fn wakeword_dir(data_dir: &Path) -> PathBuf {
    data_dir.join("talk").join("models").join("wakeword")
}

fn idle(data_dir: PathBuf, phrase: String, seconds: u64, settle: u64, rate: usize, chunk_ms: usize, wav: Option<PathBuf>) {
    let audio = match &wav {
        // `read_wav` answers 16 kHz: the stand-in then delivers that.
        Some(path) => read_wav(&std::fs::read(path).expect("read the room")).expect("a WAV"),
        None => room(rate, 30),
    };
    let rate = if wav.is_some() { RATE } else { rate };
    let has_model = etalk_wakeword::models::model_for(&phrase).is_some();
    let listening = Arc::new(AtomicBool::new(false));
    let woke = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let mut links = Links::new(Arc::new(Listening::default()));
    links.emit = {
        let (listening, woke) = (listening.clone(), woke.clone());
        Arc::new(move |event: &Event| match event {
            Event::WakeListening { listening: now, reason } => {
                eprintln!("wake word listening: {now} ({reason:?})");
                listening.store(*now, Ordering::SeqCst);
            }
            Event::Woke { .. } => {
                woke.fetch_add(1, Ordering::SeqCst);
            }
            _ => {}
        })
    };
    let ears_dir = data_dir.clone();
    let chunk = (rate * chunk_ms / 1000).max(1);
    let (jobs, thread) =
        listener::spawn(move || FileEars { ears: Ears::new(ears_dir), audio: Arc::new(audio), rate, chunk }, links);
    jobs.send(Job::Wake { phrase: phrase.clone(), language: "en".into() }).expect("the listening thread");
    let deadline = Instant::now() + Duration::from_secs(1800);
    let model_ready = || !has_model || std::fs::read_dir(wakeword_dir(&data_dir)).is_ok_and(|dir| dir.flatten().filter(|entry| entry.path().extension().is_some_and(|ext| ext == "onnx")).count() >= 3);
    while !(listening.load(Ordering::SeqCst) && model_ready()) {
        assert!(Instant::now() < deadline, "the wake word never listened");
        std::thread::sleep(Duration::from_millis(200));
    }
    // The scan asks for the model every 5 s; one that was ready listens at once.
    std::thread::sleep(Duration::from_secs(settle));
    let (started, before) = (Instant::now(), cpu_seconds());
    std::thread::sleep(Duration::from_secs(seconds));
    let (wall, cpu) = (started.elapsed().as_secs_f64(), cpu_seconds() - before);
    println!(
        "{}",
        serde_json::json!({
            "run": "idle",
            "phrase": phrase,
            "detector": if has_model { "model" } else { "transcript" },
            "microphone": format!("{rate} Hz in {chunk_ms} ms chunks"),
            "seconds": (wall * 10.0).round() / 10.0,
            "cpuSeconds": (cpu * 100.0).round() / 100.0,
            "cpuPercentOfOneCore": (cpu / wall * 1000.0).round() / 10.0,
            "woke": woke.load(Ordering::SeqCst),
        })
    );
    drop(jobs);
    let _ = thread.join();
}

fn wavs(path: &Path, out: &mut Vec<PathBuf>) {
    if path.is_dir() {
        let mut entries: Vec<PathBuf> = std::fs::read_dir(path).map(|dir| dir.flatten().map(|entry| entry.path()).collect()).unwrap_or_default();
        entries.sort();
        entries.iter().for_each(|entry| wavs(entry, out));
    } else if path.extension().is_some_and(|extension| extension.eq_ignore_ascii_case("wav")) {
        out.push(path.to_path_buf());
    }
}

fn clips(data_dir: PathBuf, phrase: String, classifier: Option<(PathBuf, f32)>, lead: usize, inputs: &[String]) {
    Engines::download(&data_dir, |_, _| {}).expect("download");
    let mut engines = Engines::load(&data_dir, false).expect("load");
    let mut models = etalk_engine::wake_model::WakeModels::default();
    let mut detector = || -> etalk_wakeword::Detector {
        use etalk_wakeword::models::{DEBOUNCE, PATIENCE};
        match &classifier {
            Some((path, threshold)) => {
                let dir = wakeword_dir(&data_dir);
                let front = etalk_wakeword::FrontEnd::load(&dir.join("melspectrogram.onnx"), &dir.join("embedding_model.onnx")).expect("the front end (run `idle` once: it fetches it)");
                let classifier = etalk_wakeword::Classifier::load(path).expect("classifier");
                etalk_wakeword::Detector::new(front, classifier, etalk_wakeword::Trigger::new(*threshold, PATIENCE, DEBOUNCE), 1.0)
            }
            None => {
                let deadline = Instant::now() + Duration::from_secs(600);
                loop {
                    if let Some(detector) = models.detector(&data_dir, &phrase) {
                        break detector;
                    }
                    assert!(Instant::now() < deadline, "no model for {phrase:?}");
                    std::thread::sleep(Duration::from_millis(500));
                }
            }
        }
    };
    let mut paths = Vec::new();
    inputs.iter().for_each(|input| wavs(Path::new(input), &mut paths));
    let quiet = room(RATE, lead.max(1));
    let (mut results, mut accepted, mut positives, mut false_accepts) = (Vec::new(), 0, 0, 0);
    for path in &paths {
        let clip = read_wav(&std::fs::read(path).expect("read")).expect("a WAV");
        let audio = [&quiet[..lead * RATE], clip.as_slice(), &quiet[..RATE * 2]].concat();
        let mut vad = engines.vad().expect("vad");
        let mut scanner = WakeScanner::for_model();
        let mut model = ModelDetector(detector());
        let mut wakes = Vec::new();
        for (index, chunk) in audio.chunks(RATE / 50).enumerate() {
            if scan(&mut scanner, &mut model, &mut vad, &mut engines, chunk, &phrase, "en", false).expect("scan").is_some() {
                wakes.push((index + 1) * (RATE / 50) - lead * RATE);
            }
        }
        let positive = path.components().any(|component| component.as_os_str() == "positives");
        positives += usize::from(positive);
        accepted += usize::from(positive && !wakes.is_empty());
        false_accepts += if positive { 0 } else { wakes.len() };
        results.push(serde_json::json!({ "clip": path.display().to_string(), "positive": positive, "wakes": wakes }));
    }
    println!(
        "{}",
        serde_json::json!({
            "run": "clips",
            "phrase": phrase,
            "clips": paths.len(),
            "positives": positives,
            "accepted": accepted,
            "negatives": paths.len() - positives,
            "falseAccepts": false_accepts,
            "results": results,
        })
    );
}

fn main() {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let mut flag = |name: &str| -> Option<String> {
        let index = args.iter().position(|arg| arg == name)?;
        args.remove(index);
        (index < args.len()).then(|| args.remove(index))
    };
    let phrase = flag("--phrase").unwrap_or_else(|| "Hey spezl".into());
    let number = |value: Option<String>, default: usize| value.and_then(|value| value.parse().ok()).unwrap_or(default);
    let seconds = number(flag("--seconds"), 120) as u64;
    let settle = number(flag("--settle"), 15) as u64;
    let rate = number(flag("--rate"), 48_000);
    let chunk_ms = number(flag("--chunk-ms"), 10);
    let lead = number(flag("--lead"), 4);
    let threshold: f32 = flag("--threshold").and_then(|value| value.parse().ok()).unwrap_or(etalk_wakeword::models::THRESHOLD);
    let classifier = flag("--classifier").map(|path| (PathBuf::from(path), threshold));
    match args.as_slice() {
        [data_dir, mode, rest @ ..] if mode == "idle" && rest.len() <= 1 => {
            idle(PathBuf::from(data_dir), phrase, seconds, settle, rate, chunk_ms, rest.first().map(PathBuf::from))
        }
        [data_dir, mode, inputs @ ..] if mode == "clips" && !inputs.is_empty() => clips(PathBuf::from(data_dir), phrase, classifier, lead, inputs),
        _ => {
            eprintln!("usage: wake_listen <data-dir> idle [--phrase P] [--seconds N] [--settle N] [--rate 48000] [--chunk-ms 10] [room.wav]");
            eprintln!("       wake_listen <data-dir> clips [--phrase P] [--classifier F --threshold T] [--lead N] <clip.wav | dir>...");
            std::process::exit(2);
        }
    }
}
