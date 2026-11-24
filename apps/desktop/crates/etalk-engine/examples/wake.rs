//! #NEW-56: the wake word's measuring harness, on WAV files instead of the
//! microphone.
//!
//!   cargo run --release -p voice --example wake -- <data-dir> [--cpu]
//!       [--phrase "Hey Studio"]... [--language en] [--together] [--minutes N] [--silence N] [--checks-out F] <clip.wav | dir>...
//!
//! Every clip runs through the worker's own path (`etalk_engine::wake::scan`, then
//! `etalk_engine::wake::turn` on a match, Silero and Parakeet through CrispASR) in
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
//! - `--checks-out <file>` appends every negative check as a JSON line
//!   (`clip`, `at`, `windowMs`, `heard`): the transcripts the match ran on,
//!   to replay other phrases or tolerances offline.
//! - `--phrase` repeats: every phrase is its own run over the same clips
//!   (its own window and turns), one JSON line each.
//! - `--together` (EXP-1859): the phrases are ONE set of listeners instead,
//!   heard at once like the voice device hears its agents (one run, one
//!   JSON line; a wake names its `phrase`). A positive clip counts as heard
//!   whichever phrase woke: check `heard[].phrase` against the clip.
//!
//! Every wake names its clip and second (`wakes`), so a false accept can be
//! listened back; `nearMisses` are the checks one edit past the phrase's
//! tolerance (`etalk_engine::wake::wake_allowed`), the margin the match has left.
//!
//! Recordings live in `out/wake-recordings/` (gitignored): own and
//! consenting voices only, game and desk audio, NEVER committed. Record
//! with `pw-record --rate 16000 --channels 1 out/wake-recordings/negatives/<name>.wav`.
//! The corpus set (LibriSpeech, MLS, CC0 game audio, Qwen3-TTS positives)
//! is built by `apps/desktop/spikes/talk/scripts/wake_corpus.py` (the sweep
//! over `--checks-out`: `wake_sweep.py`); its numbers are in
//! `docs/talk-spike.md` (Wake word false accepts).
//! Prints one JSON line per run.

use std::path::{Path, PathBuf};

use etalk_engine::listen::{Transcriber, RATE};
use etalk_engine::listener::Engines;
use etalk_engine::wake::{multi_window_ms, scan, turn, turn_ended, wake_allowed, wake_distance, Listener, TranscriptMatcher, WakeScanner};

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
    /// One per wake: where it fired and what the turn heard.
    wakes: Vec<serde_json::Value>,
    /// Checks one edit past the tolerance: (where, what the check heard).
    near_misses: Vec<serde_json::Value>,
    /// Every check's transcript (positives only: what a miss heard).
    checks_heard: Option<Vec<String>>,
    /// Every check with where it ran (`--checks-out`).
    checks_out: Option<Vec<serde_json::Value>>,
}

/// One stream through the scan (and a turn per match), counting checks.
/// `clip` names the stream in `wakes` and `nearMisses`.
fn run(engines: &mut Engines, audio: &[f32], listeners: &[Listener], clip: &str, tally: &mut Tally) {
    /// Counts transcriptions and keeps the texts of the scan's checks.
    struct Counting<'a>(&'a mut Engines, usize, Vec<String>);
    impl Transcriber for Counting<'_> {
        fn transcribe(&mut self, pcm: &[f32], language: &str) -> Result<String, String> {
            self.1 += 1;
            let text = self.0.transcribe(pcm, language)?;
            self.2.push(text.clone());
            Ok(text)
        }
        fn partials(&self) -> bool {
            self.0.partials()
        }
    }
    let mut vad = engines.vad().expect("vad");
    let mut counting = Counting(engines, 0, Vec::new());
    let window = multi_window_ms(listeners.iter().map(|listener| listener.phrase.as_str()));
    let mut scanner = WakeScanner::new(window);
    let mut active: Option<(f64, String, etalk_engine::listen::Utterance)> = None;
    let mut woken = String::new();
    let wake = |at: f64, check: String, phrase: &str, event: etalk::Event, wakes: &mut Vec<serde_json::Value>| {
        let heard = match event {
            etalk::Event::Heard { text, .. } => text,
            _ => String::new(),
        };
        wakes.push(serde_json::json!({ "clip": clip, "at": (at * 10.0).round() / 10.0, "phrase": phrase, "check": check, "heard": heard }));
    };
    let chunk_samples = RATE / 50;
    for (index, chunk) in audio.chunks(chunk_samples).enumerate() {
        let at = (index * chunk_samples) as f64 / RATE as f64;
        if let Some((_, _, utterance)) = active.as_mut() {
            utterance.feed(chunk, &mut vad, &mut counting, &mut |_| {}).expect("feed");
            if turn_ended(utterance) {
                let (woke, check, utterance) = active.take().expect("active");
                utterance.finish(&mut counting, &mut |event| wake(woke, check.clone(), &woken, event, &mut tally.wakes)).expect("finish");
                scanner = WakeScanner::new(window);
            }
            continue;
        }
        let checks = counting.1;
        counting.2.clear();
        let matched = scan(&mut scanner, &mut TranscriptMatcher, &mut vad, &mut counting, chunk, listeners, false).expect("scan");
        tally.checks += counting.1 - checks;
        if let Some(checks_heard) = tally.checks_heard.as_mut() {
            checks_heard.extend(counting.2.iter().cloned());
        }
        if let Some(out) = tally.checks_out.as_mut() {
            out.extend(counting.2.iter().map(|heard| serde_json::json!({ "clip": clip, "at": (at * 10.0).round() / 10.0, "windowMs": window, "heard": heard })));
        }
        for heard in &counting.2 {
            for listener in listeners {
                if wake_distance(&listener.phrase, heard).is_some_and(|(_, distance)| distance == wake_allowed(&listener.phrase) + 1) {
                    tally.near_misses.push(serde_json::json!({ "clip": clip, "at": (at * 10.0).round() / 10.0, "phrase": listener.phrase, "check": heard }));
                }
            }
        }
        if let Some(matched) = matched {
            let check = counting.2.last().cloned().unwrap_or_default();
            woken = listeners[matched.2].phrase.clone();
            active = Some((at, check, turn("bench", listeners, &scanner, matched, &mut counting).expect("turn")));
        }
    }
    if let Some((woke, check, utterance)) = active {
        utterance.finish(&mut counting, &mut |event| wake(woke, check.clone(), &woken, event, &mut tally.wakes)).expect("finish");
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
        "nearMisses": tally.near_misses.len(),
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
    let mut phrases = Vec::new();
    while let Some(phrase) = flag("--phrase") {
        phrases.push(phrase);
    }
    if phrases.is_empty() {
        phrases.push("Hey Studio".into());
    }
    let language = flag("--language").unwrap_or_else(|| "en".into());
    let minutes: Option<f64> = flag("--minutes").and_then(|value| value.parse().ok());
    let silence: Option<f64> = flag("--silence").and_then(|value| value.parse().ok());
    let checks_out = flag("--checks-out").map(PathBuf::from);
    let together = match args.iter().position(|arg| arg == "--together") {
        Some(index) => {
            args.remove(index);
            true
        }
        None => false,
    };
    let cpu = args.iter().any(|arg| arg == "--cpu");
    args.retain(|arg| arg != "--cpu");
    let [data_dir, inputs @ ..] = args.as_slice() else {
        eprintln!("usage: wake <data-dir> [--cpu] [--phrase P]... [--language en] [--minutes N] [--silence N] [--checks-out F] <clip.wav | dir>...");
        std::process::exit(2);
    };
    let data_dir = PathBuf::from(data_dir);
    Engines::download(&data_dir, |_, _| {}).expect("download");
    let mut engines = Engines::load(&data_dir, cpu).expect("load");
    eprintln!("speech model on the {:?}; phrases {phrases:?} ({language})", engines.device());
    let mut clips = Vec::new();
    for input in inputs {
        wavs(Path::new(input), &mut clips);
    }
    if together {
        // EXP-1859: every phrase listened for at once, as one set.
        let listeners: Vec<Listener> = phrases.iter().map(|phrase| Listener::new(phrase.as_str(), phrase.as_str(), &language)).collect();
        measure(&mut engines, &listeners, minutes, silence, &clips, checks_out.as_deref());
        return;
    }
    for phrase in &phrases {
        measure(&mut engines, &[Listener::new(phrase.as_str(), phrase.as_str(), &language)], minutes, silence, &clips, checks_out.as_deref());
    }
}

fn measure(engines: &mut Engines, listeners: &[Listener], minutes: Option<f64>, silence: Option<f64>, clips: &[PathBuf], checks_out: Option<&Path>) {
    let phrases: Vec<&str> = listeners.iter().map(|listener| listener.phrase.as_str()).collect();
    let base = serde_json::json!({
        "phrase": phrases.join(" | "),
        "phrases": phrases.len(),
        "language": listeners[0].language,
        "allowedEdits": listeners.iter().map(|listener| wake_allowed(&listener.phrase)).collect::<Vec<_>>(),
        "windowMs": multi_window_ms(phrases.iter().copied()),
    });
    let with = |extra: serde_json::Value| {
        let mut out = base.clone();
        if let (Some(out), Some(extra)) = (out.as_object_mut(), extra.as_object()) {
            out.extend(extra.clone());
        }
        out
    };
    if let Some(minutes) = silence {
        let mut tally = Tally::default();
        let before = cpu_seconds();
        run(engines, &vec![0.0; (minutes * 60.0 * RATE as f64) as usize], listeners, "silence", &mut tally);
        report("silence", &tally, cpu_seconds() - before, with(serde_json::json!({})));
    }
    if clips.is_empty() {
        return;
    }
    let read = |path: &Path| etalk_engine::capture::read_wav(&std::fs::read(path).expect("read")).expect("a 16 kHz-readable WAV");
    let negatives_extra = |tally: &Tally| {
        let per_8h = tally.wakes.len() as f64 * 8.0 * 3600.0 / tally.audio_seconds.max(1e-9);
        serde_json::json!({
            "falseAccepts": tally.wakes.len(),
            "falseAcceptsPer8h": (per_8h * 100.0).round() / 100.0,
            "heard": tally.wakes,
            "nearMissChecks": tally.near_misses,
        })
    };
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
        run(engines, &stream, listeners, "continuous", &mut tally);
        let cpu = cpu_seconds() - before;
        let extra = if clips.iter().any(|clip| positive(clip)) {
            serde_json::json!({ "heard": tally.wakes })
        } else {
            negatives_extra(&tally)
        };
        report("continuous", &tally, cpu, with(extra));
        return;
    }
    let mut negatives = Tally { checks_out: checks_out.map(|_| Vec::new()), ..Tally::default() };
    let mut positives = Tally { checks_heard: Some(Vec::new()), ..Tally::default() };
    let (mut negative_cpu, mut positive_cpu, mut misses) = (0.0, 0.0, Vec::new());
    for clip in clips {
        // Room around each clip: its last segment closes, a turn's quiet runs out.
        let audio = [vec![0.0; RATE / 2], read(clip), vec![0.0; RATE * 5]].concat();
        let name = clip.display().to_string();
        let before = cpu_seconds();
        if positive(clip) {
            let wakes = positives.wakes.len();
            positives.checks_heard = Some(Vec::new());
            run(engines, &audio, listeners, &name, &mut positives);
            positive_cpu += cpu_seconds() - before;
            if positives.wakes.len() == wakes {
                misses.push(serde_json::json!({ "clip": name, "checks": positives.checks_heard.take().unwrap_or_default() }));
            }
        } else {
            run(engines, &audio, listeners, &name, &mut negatives);
            negative_cpu += cpu_seconds() - before;
        }
    }
    if let (Some(path), Some(checks)) = (checks_out, negatives.checks_out.take()) {
        use std::io::Write;
        let mut file = std::fs::OpenOptions::new().create(true).append(true).open(path).expect("checks-out");
        for check in checks {
            writeln!(file, "{check}").expect("write checks-out");
        }
    }
    if negatives.audio_seconds > 0.0 {
        report("negatives", &negatives, negative_cpu, with(negatives_extra(&negatives)));
    }
    if positives.audio_seconds > 0.0 {
        let clips = clips.iter().filter(|clip| positive(clip)).count();
        report(
            "positives",
            &positives,
            positive_cpu,
            with(serde_json::json!({ "clips": clips, "trueAccepts": clips - misses.len(), "misses": misses, "heard": positives.wakes })),
        );
    }
}
