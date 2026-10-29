//! NEW-57 live check for a meeting's channels: captures the desktop audio
//! (`etalk::apps::open(All)`, and the microphone with `--mic`) for
//! `--seconds` (default 20) through the worker's own per-channel pipeline
//! (`etalk_engine::meeting::Meeting`) and prints every event as JSON. With
//! `--say` it plays two phrases through macOS `say` meanwhile (an app of
//! its own: channel `say`).
//!
//!   cargo run -p voice --example meeting -- [--data-dir <dir>] [--mic] [--say] [--seconds 20]
//!
//! With `--data-dir` holding the downloaded speech models (Silero +
//! Parakeet, `<dir>/talk/…`) it transcribes; without them it segments on
//! loudness alone and names each segment by its length (no ASR).

use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use etalk::apps::{self, Selection};
use etalk_engine::listen::{measure, Transcriber, Vad, FRAME};
use etalk_engine::meeting::Meeting;

/// Speech = a frame above -40 dBFS (no model).
#[derive(Default)]
struct EnergyVad(Vec<f32>);

impl Vad for EnergyVad {
    fn push(&mut self, samples: &[f32]) -> Result<Vec<f32>, String> {
        self.0.extend_from_slice(samples);
        let whole = self.0.len() / FRAME * FRAME;
        let out = self.0[..whole].chunks(FRAME).map(|frame| if measure(frame).0 > 0.4 { 0.9 } else { 0.05 }).collect();
        self.0.drain(..whole);
        Ok(out)
    }
}

/// No ASR: a segment's length and peak.
struct Lengths;

impl Transcriber for Lengths {
    fn transcribe(&mut self, pcm: &[f32], _language: &str) -> Result<String, String> {
        let peak = pcm.iter().fold(0f32, |peak, sample| peak.max(sample.abs()));
        Ok(format!("[{:.2} s of speech, peak {peak:.2}]", pcm.len() as f32 / 16_000.0))
    }

    fn partials(&self) -> bool {
        false
    }
}

fn now_ms() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |since| since.as_millis() as i64)
}

enum Engines {
    Real(etalk_engine::listener::Engines),
    Fake,
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let flag = |name: &str| args.iter().any(|arg| arg == name);
    let value = |name: &str| args.iter().position(|arg| arg == name).and_then(|at| args.get(at + 1)).cloned();
    let seconds: u64 = value("--seconds").and_then(|value| value.parse().ok()).unwrap_or(20);
    let mut engines = match value("--data-dir").map(std::path::PathBuf::from) {
        Some(dir) if etalk_engine::listener::Engines::files_ready(&dir) => {
            Engines::Real(etalk_engine::listener::Engines::load(&dir, false).expect("load the speech models"))
        }
        Some(dir) => {
            eprintln!("no speech models under {}: segmentation only (no ASR)", dir.display());
            Engines::Fake
        }
        None => {
            eprintln!("no --data-dir: segmentation only (no ASR)");
            Engines::Fake
        }
    };
    eprintln!("capability {:?}, permission {:?}", apps::capability(), apps::permission());
    let capture = apps::open(Selection::All).expect("open the desktop audio");
    let mic = flag("--mic").then(|| etalk_engine::capture::open().map_err(|(reason, detail)| format!("{reason:?}: {detail}")).expect("open the microphone"));
    let say = flag("--say").then(|| {
        std::thread::spawn(|| {
            std::thread::sleep(Duration::from_secs(2));
            let _ = std::process::Command::new("say").arg("The release is blocked on the Android build. Can we ship on Friday?").status();
            std::thread::sleep(Duration::from_secs(2));
            let _ = std::process::Command::new("say").args(["-v", "Samantha", "Yes, if the fix lands by Thursday noon."]).status();
        })
    });

    let mut fake: Meeting<EnergyVad> = Meeting::new("live", "en", "Hey Studio", etalk_engine::meeting::ring_enabled());
    let mut real: Meeting<etalk_engine::listener::SileroVad> = Meeting::new("live", "en", "Hey Studio", etalk_engine::meeting::ring_enabled());
    let mut print = |event: etalk::Event| println!("{}", serde_json::to_string(&event).unwrap());
    let started = Instant::now();
    let mut feed = |channel: &str, label: &str, samples: &[f32], now: i64, engines: &mut Engines| {
        match engines {
            Engines::Fake => {
                if fake.needs(channel) {
                    fake.add(channel, label, EnergyVad::default());
                }
                fake.feed(channel, label, samples, now, &mut Lengths, &mut print).expect("feed");
            }
            Engines::Real(engines) => {
                if real.needs(channel) {
                    real.add(channel, label, engines.vad().expect("a VAD"));
                }
                real.feed(channel, label, samples, now, engines, &mut print).expect("feed");
            }
        }
    };
    while started.elapsed() < Duration::from_secs(seconds) {
        if let Some(mic) = mic.as_ref() {
            while let Ok(samples) = mic.recv_timeout(Duration::ZERO) {
                feed(etalk::YOU_CHANNEL, "You", &samples, now_ms(), &mut engines);
            }
        }
        match capture.recv_timeout(Duration::from_millis(20)) {
            Ok(chunk) => feed(&chunk.channel, &chunk.label, &chunk.samples, chunk.captured_ms as i64, &mut engines),
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }
    let now = now_ms();
    match &mut engines {
        Engines::Fake => fake.idle(now + etalk_engine::meeting::CHANNEL_IDLE_MS, &mut Lengths, &mut print).expect("idle"),
        Engines::Real(engines) => real.idle(now + etalk_engine::meeting::CHANNEL_IDLE_MS, engines, &mut print).expect("idle"),
    }
    let dir = std::env::temp_dir().join(etalk_engine::meeting::DEBUG_DIR);
    let written = match engines {
        Engines::Fake => fake.stop(Some(&dir)),
        Engines::Real(_) => real.stop(Some(&dir)),
    };
    for file in written {
        eprintln!("debug ring: {}", file.display());
    }
    if let Some(say) = say {
        let _ = say.join();
    }
}
