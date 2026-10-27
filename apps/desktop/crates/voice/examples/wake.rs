//! #NEW-56: the wake word's measuring harness, on WAV files instead of the
//! microphone.
//!
//!   cargo run --release -p voice --example wake -- <data-dir> [--cpu]
//!       [--phrase "Hey Studio"] [--language en] [--minutes N] [--silence N] <clip.wav | dir>...
//!
//! Every clip runs through the worker's own path (`voice::wake::scan`, then
//! `voice::wake::turn` on a match, Silero and Parakeet through CrispASR) in
//! 20 ms chunks as fast as it decodes, and the process's CPU time (all
//! threads, `/proc/self/stat`) is divided by the audio's length: the share
//! of one core the wake word costs while that audio plays.
//!
//! - Each clip is padded with 0.5 s of silence before and 5 s after (its
//!   last segment closes; a turn's quiet runs out).
//! - A directory is read recursively. Clips under a directory named
//!   `positives` should wake once each (a miss is counted); every other
//!   clip is a negative, and any wake in it is a false accept.
//! - `--minutes N` loops the clips (with 0.6 s pauses) into N minutes of one
//!   continuous stream: talk that never stops, the CPU worst case.
//! - `--silence N` measures N minutes of digital silence first (VAD only).
//!
//! Recordings live in `out/wake-recordings/` (gitignored): own and
//! consenting voices only, game and desk audio, NEVER committed. Record
//! with `pw-record --rate 16000 --channels 1 out/wake-recordings/negatives/<name>.wav`.
//! Prints one JSON line per run.

use std::path::{Path, PathBuf};

use voice::listen::{Transcriber, RATE};
use voice::listener::Engines;
use voice::wake::{scan, turn, turn_ended, window_ms, WakeScanner};

/// User + system CPU seconds of this process, every thread (Linux).
fn cpu_seconds() -> f64 {
    let stat = std::fs::read_to_string("/proc/self/stat").unwrap_or_default();
    let fields: Vec<&str> = stat.rsplit_once(')').map(|(_, rest)| rest.split_whitespace().collect()).unwrap_or_default();
    // utime and stime are fields 14 and 15 (1-based), 12 and 13 after the name.
    let ticks: f64 = [11, 12].iter().filter_map(|index| fields.get(*index)?.parse::<f64>().ok()).sum();
    ticks / 100.0
}

fn wavs(path: &Path, out: &mut Vec<PathBuf>) {
    if path.is_dir() {
        let mut entries: Vec<PathBuf> = std::fs::read_dir(path).map(|dir| dir.flatten().map(|entry| entry.path()).collect()).unwrap_or_default();
        entries.sort();
        for entry in entries {
            wavs(&entry, out);
        }
    } else if path.extension().is_some_and(|extension| extension.eq_ignore_ascii_case("wav")) {
        out.push(path.to_path_buf());
    }
}

fn positive(path: &Path) -> bool {
    path.components().any(|component| component.as_os_str() == "positives")
}

#[derive(Default)]
struct Tally {
    audio_seconds: f64,
    checks: usize,
    wakes: Vec<String>,
}

/// One stream through the scan (and a turn per match), counting checks.
fn run(engines: &mut Engines, audio: &[f32], phrase: &str, language: &str, tally: &mut Tally) {
    struct Counting<'a>(&'a mut Engines, usize);
    impl Transcriber for Counting<'_> {
        fn transcribe(&mut self, pcm: &[f32], language: &str) -> Result<String, String> {
            self.1 += 1;
            self.0.transcribe(pcm, language)
        }
        fn partials(&self) -> bool {
            self.0.partials()
        }
    }
    let mut vad = engines.vad().expect("vad");
    let mut counting = Counting(engines, 0);
    let mut scanner = WakeScanner::new(window_ms(phrase));
    let mut active: Option<voice::listen::Utterance> = None;
    for chunk in audio.chunks(RATE / 50) {
        if let Some(utterance) = active.as_mut() {
            utterance.feed(chunk, &mut vad, &mut counting, &mut |_| {}).expect("feed");
            if turn_ended(utterance) {
                let utterance = active.take().expect("active");
                utterance
                    .finish(&mut counting, &mut |event| match event {
                        talk::Event::Heard { text, .. } => tally.wakes.push(text),
                        _ => tally.wakes.push(String::new()),
                    })
                    .expect("finish");
                scanner = WakeScanner::new(window_ms(phrase));
            }
            continue;
        }
        let checks = counting.1;
        if let Some(matched) = scan(&mut scanner, &mut vad, &mut counting, chunk, phrase, language, false).expect("scan") {
            active = Some(turn("bench", language, phrase, &scanner, matched, &mut counting).expect("turn"));
        }
        tally.checks += counting.1 - checks;
    }
    if let Some(utterance) = active {
        utterance
            .finish(&mut counting, &mut |event| match event {
                talk::Event::Heard { text, .. } => tally.wakes.push(text),
                _ => tally.wakes.push(String::new()),
            })
            .expect("finish");
    }
    tally.audio_seconds += audio.len() as f64 / RATE as f64;
}

fn report(kind: &str, tally: &Tally, cpu: f64, extra: serde_json::Value) {
    let mut line = serde_json::json!({
        "run": kind,
        "audioSeconds": (tally.audio_seconds * 10.0).round() / 10.0,
        "cpuSeconds": (cpu * 100.0).round() / 100.0,
        "cpuPercentOfOneCore": ((cpu / tally.audio_seconds.max(1e-9)) * 1000.0).round() / 10.0,
        "checks": tally.checks,
        "checksPerMinute": ((tally.checks as f64 / (tally.audio_seconds / 60.0).max(1e-9)) * 10.0).round() / 10.0,
        "wakes": tally.wakes.len(),
    });
    if let (Some(line), Some(extra)) = (line.as_object_mut(), extra.as_object()) {
        line.extend(extra.clone());
    }
    println!("{line}");
}

fn main() {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let mut flag = |name: &str| -> Option<String> {
        let index = args.iter().position(|arg| arg == name)?;
        args.remove(index);
        (index < args.len()).then(|| args.remove(index))
    };
    let phrase = flag("--phrase").unwrap_or_else(|| "Hey Studio".into());
    let language = flag("--language").unwrap_or_else(|| "en".into());
    let minutes: Option<f64> = flag("--minutes").and_then(|value| value.parse().ok());
    let silence: Option<f64> = flag("--silence").and_then(|value| value.parse().ok());
    let cpu = args.iter().any(|arg| arg == "--cpu");
    args.retain(|arg| arg != "--cpu");
    let [data_dir, inputs @ ..] = args.as_slice() else {
        eprintln!("usage: wake <data-dir> [--cpu] [--phrase P] [--language en] [--minutes N] [--silence N] <clip.wav | dir>...");
        std::process::exit(2);
    };
    let data_dir = PathBuf::from(data_dir);
    Engines::download(&data_dir, |_, _| {}).expect("download");
    let mut engines = Engines::load(&data_dir, cpu).expect("load");
    eprintln!("speech model on the {:?}; phrase {phrase:?} ({language})", engines.device());

    if let Some(minutes) = silence {
        let mut tally = Tally::default();
        let before = cpu_seconds();
        run(&mut engines, &vec![0.0; (minutes * 60.0 * RATE as f64) as usize], &phrase, &language, &mut tally);
        report("silence", &tally, cpu_seconds() - before, serde_json::json!({}));
    }

    let mut clips = Vec::new();
    for input in inputs {
        wavs(Path::new(input), &mut clips);
    }
    if clips.is_empty() {
        return;
    }
    let read = |path: &Path| voice::capture::read_wav(&std::fs::read(path).expect("read")).expect("a 16 kHz-readable WAV");
    if let Some(minutes) = minutes {
        // One continuous stream: the clips over and over, 0.6 s apart.
        let pause = vec![0.0; RATE * 6 / 10];
        let decoded: Vec<Vec<f32>> = clips.iter().map(|clip| read(clip)).collect();
        let target = (minutes * 60.0 * RATE as f64) as usize;
        let mut stream = Vec::with_capacity(target);
        'fill: loop {
            for clip in &decoded {
                stream.extend_from_slice(clip);
                stream.extend_from_slice(&pause);
                if stream.len() >= target {
                    break 'fill;
                }
            }
        }
        let mut tally = Tally::default();
        let before = cpu_seconds();
        run(&mut engines, &stream, &phrase, &language, &mut tally);
        let cpu = cpu_seconds() - before;
        let positives = clips.iter().any(|clip| positive(clip));
        let extra = if positives {
            serde_json::json!({ "heard": tally.wakes })
        } else {
            let per_8h = tally.wakes.len() as f64 * 8.0 * 3600.0 / tally.audio_seconds.max(1e-9);
            serde_json::json!({ "falseAccepts": tally.wakes.len(), "falseAcceptsPer8h": (per_8h * 100.0).round() / 100.0, "heard": tally.wakes })
        };
        report("continuous", &tally, cpu, extra);
        return;
    }
    let (mut negatives, mut positives) = (Tally::default(), Tally::default());
    let (mut negative_cpu, mut positive_cpu, mut misses) = (0.0, 0.0, Vec::new());
    for clip in &clips {
        // Room around each clip: its last segment closes, a turn's quiet runs out.
        let audio = [vec![0.0; RATE / 2], read(clip), vec![0.0; RATE * 5]].concat();
        let before = cpu_seconds();
        if positive(clip) {
            let wakes = positives.wakes.len();
            run(&mut engines, &audio, &phrase, &language, &mut positives);
            positive_cpu += cpu_seconds() - before;
            if positives.wakes.len() == wakes {
                misses.push(clip.display().to_string());
            }
        } else {
            run(&mut engines, &audio, &phrase, &language, &mut negatives);
            negative_cpu += cpu_seconds() - before;
        }
    }
    if negatives.audio_seconds > 0.0 {
        let per_8h = negatives.wakes.len() as f64 * 8.0 * 3600.0 / negatives.audio_seconds;
        report(
            "negatives",
            &negatives,
            negative_cpu,
            serde_json::json!({ "falseAccepts": negatives.wakes.len(), "falseAcceptsPer8h": (per_8h * 100.0).round() / 100.0, "heard": negatives.wakes }),
        );
    }
    if positives.audio_seconds > 0.0 {
        report("positives", &positives, positive_cpu, serde_json::json!({ "misses": misses, "heard": positives.wakes }));
    }
}
