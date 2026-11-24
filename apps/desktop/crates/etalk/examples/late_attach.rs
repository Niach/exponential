//! #NEW-127 measurement: how many ms of an app's START are missing from its
//! desktop channel when it begins playing while a capture already runs
//! (meeting.json CHANNELS, `desktopAudio: "all"`). Desktop channels only:
//! the microphone is never opened.
//!
//! Every run starts a FRESH process (`afplay <clip>`, or `say` for the
//! issue's two-utterance case) at a random phase, keeps what its channel
//! delivered and compares it with the clip: `missing` = the clip's voiced
//! span minus the captured voiced span (the end always arrives), `lag` = the
//! same by cross-correlating the capture's head against the clip. Only
//! numbers are printed, never audio or text.
//!
//! ```text
//! say -o /tmp/clip.wav --data-format=LEI16@16000 "Hey spezl. Write down the action items."
//! cargo run --release -p etalk --example late_attach -- runs /tmp/clip.wav 10
//! cargo run --release -p etalk --example late_attach -- two /tmp/first.wav /tmp/second.wav "<first text>" "<second text>"
//! cargo run --release -p etalk --example late_attach -- idle 60
//! cargo run --release -p etalk --example late_attach -- idle 60 /tmp/clip.wav   # one app playing all along
//! ```
//!
//! Linux: `LATE_ATTACH_PLAYER=pw-play` (or `paplay`, `aplay`) names the
//! player; its channel is matched by that name.

use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use etalk::apps::{self, AppCapture, Capability, Chunk, Selection, SAMPLE_RATE};

/// A 10 ms frame at 16 kHz.
const FRAME: usize = 160;
/// A frame is voiced above this share of its signal's loudest frame.
const VOICED: f32 = 0.05;
/// The capture's head that is searched for in the clip.
const HEAD: usize = SAMPLE_RATE as usize / 2;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mode = args.first().map(String::as_str).unwrap_or("");
    println!("capability: {:?}, permission: {:?}", apps::capability(), apps::permission());
    if apps::capability() == Capability::None {
        println!("no desktop audio on this machine");
        std::process::exit(2);
    }
    // Never raise a permission prompt from a measurement.
    if apps::permission() == Some(false) {
        println!("the Audio Recording permission is not granted; not opening a tap");
        std::process::exit(2);
    }
    match mode {
        "runs" if args.len() >= 2 => {
            let runs = args.get(2).and_then(|n| n.parse().ok()).unwrap_or(10);
            runs_mode(&args[1], runs);
        }
        "two" if args.len() >= 5 => two_mode(&args[1], &args[2], &args[3], &args[4]),
        "idle" => idle_mode(args.get(1).and_then(|n| n.parse().ok()).unwrap_or(60), args.get(2).map(String::as_str)),
        _ => {
            println!("usage: late_attach runs <clip.wav> [n] | two <first.wav> <second.wav> <first text> <second text> | idle [seconds] [clip.wav]");
            std::process::exit(2);
        }
    }
}

fn player() -> String {
    std::env::var("LATE_ATTACH_PLAYER").unwrap_or_else(|_| "afplay".to_string())
}

fn open() -> AppCapture {
    match apps::open(Selection::All) {
        Ok(capture) => capture,
        Err(error) => {
            println!("open failed: {error}");
            std::process::exit(2);
        }
    }
}

fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |since| since.as_millis() as u64)
}

/// A phase that differs from run to run (no rand in this crate).
fn jitter_ms(run: usize) -> u64 {
    (now_ms().wrapping_mul(2_654_435_761).wrapping_add(run as u64 * 7_919)) % 1_000
}

fn spawn(program: &str, args: &[&str]) -> Child {
    Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap_or_else(|error| {
            println!("could not start {program}: {error}");
            std::process::exit(2);
        })
}

/// Whether `chunk` is the channel of the process this run started.
fn is_channel(chunk: &Chunk, name: &str) -> bool {
    let name = name.to_lowercase();
    chunk.channel.to_lowercase().contains(&name) || chunk.label.to_lowercase().contains(&name)
}

/// Read chunks for `wait`, keeping those of `name`'s channel (every other
/// app's audio is dropped unread).
fn collect(capture: &AppCapture, name: &str, wait: Duration, into: &mut Vec<Chunk>) {
    let until = Instant::now() + wait;
    loop {
        let left = until.saturating_duration_since(Instant::now());
        if left.is_zero() {
            return;
        }
        if let Ok(chunk) = capture.recv_timeout(left.min(Duration::from_millis(50))) {
            if is_channel(&chunk, name) {
                into.push(chunk);
            }
        }
    }
}

/// Play one fresh process to its end and return its channel's chunks and
/// when it was started (epoch ms).
fn play(capture: &AppCapture, program: &str, args: &[&str]) -> (Vec<Chunk>, u64) {
    let mut chunks = Vec::new();
    let started = now_ms();
    let mut child = spawn(program, args);
    loop {
        collect(capture, program, Duration::from_millis(50), &mut chunks);
        if matches!(child.try_wait(), Ok(Some(_)) | Err(_)) {
            break;
        }
    }
    collect(capture, program, Duration::from_millis(800), &mut chunks);
    (chunks, started)
}

struct Measure {
    /// The clip's voiced span minus the captured one, ms.
    missing_ms: i64,
    /// Where the capture's head sits in the clip's voiced span, ms
    /// (`None` = no confident match).
    lag_ms: Option<i64>,
    /// The first chunk's capture time after the process start, ms.
    first_chunk_ms: Option<i64>,
    captured_ms: i64,
    /// Audio delivered, and the wall time from its first to its last sample
    /// (they differ when cycles were dropped or the rate was wrong), ms.
    audio_ms: i64,
    wall_ms: i64,
}

fn measure(reference: &[f32], chunks: &[Chunk], started: u64) -> Measure {
    let captured: Vec<f32> = chunks.iter().flat_map(|chunk| chunk.samples.iter().copied()).collect();
    let (ref_first, ref_last) = voiced(reference).unwrap_or((0, 0));
    let ref_span = (ref_last - ref_first) as i64;
    let first_chunk_ms = chunks.first().map(|chunk| {
        let length = chunk.samples.len() as u64 * 1_000 / u64::from(SAMPLE_RATE);
        chunk.captured_ms as i64 - length as i64 - started as i64
    });
    let audio_ms = to_ms(captured.len() as i64);
    let wall_ms = match (chunks.first(), chunks.last()) {
        (Some(first), Some(last)) => {
            last.captured_ms as i64 - first.captured_ms as i64 + to_ms(first.samples.len() as i64)
        }
        _ => 0,
    };
    let Some((first, last)) = voiced(&captured) else {
        return Measure { missing_ms: to_ms(ref_span), lag_ms: None, first_chunk_ms, captured_ms: 0, audio_ms, wall_ms };
    };
    let span = (last - first) as i64;
    let lag_ms = lag(reference, &captured[first..]).map(|at| to_ms(at as i64 - ref_first as i64));
    Measure { missing_ms: to_ms(ref_span - span), lag_ms, first_chunk_ms, captured_ms: to_ms(span), audio_ms, wall_ms }
}

fn to_ms(samples: i64) -> i64 {
    samples * 1_000 / i64::from(SAMPLE_RATE)
}

/// The first and one-past-last sample of the voiced span.
fn voiced(samples: &[f32]) -> Option<(usize, usize)> {
    let energy: Vec<f32> =
        samples.chunks(FRAME).map(|frame| (frame.iter().map(|s| s * s).sum::<f32>() / frame.len() as f32).sqrt()).collect();
    let peak = energy.iter().copied().fold(0.0f32, f32::max);
    if peak < 1e-4 {
        return None;
    }
    let first = energy.iter().position(|e| *e > peak * VOICED)?;
    let last = energy.iter().rposition(|e| *e > peak * VOICED)?;
    Some((first * FRAME, ((last + 1) * FRAME).min(samples.len())))
}

/// Where `head`'s first [`HEAD`] samples sit in `reference` (normalized
/// cross-correlation; `None` below 0.5).
fn lag(reference: &[f32], head: &[f32]) -> Option<usize> {
    let head = &head[..head.len().min(HEAD)];
    if head.len() < FRAME * 10 || reference.len() < head.len() {
        return None;
    }
    let head_energy: f32 = head.iter().map(|s| s * s).sum();
    let mut best = (0.0f32, 0usize);
    for at in 0..=reference.len() - head.len() {
        let window = &reference[at..at + head.len()];
        let dot: f32 = window.iter().zip(head).map(|(a, b)| a * b).sum();
        let energy: f32 = window.iter().map(|s| s * s).sum();
        let score = dot / (energy * head_energy).sqrt().max(1e-9);
        if score > best.0 {
            best = (score, at);
        }
    }
    (best.0 >= 0.5).then_some(best.1)
}

fn report(tag: &str, measure: &Measure) {
    println!(
        "{tag}: missing {} ms, lag {}, first chunk {} after start, voiced {} ms, {} ms of audio in {} ms",
        measure.missing_ms,
        measure.lag_ms.map_or("-".to_string(), |ms| format!("{ms} ms")),
        measure.first_chunk_ms.map_or("never".to_string(), |ms| format!("{ms} ms")),
        measure.captured_ms,
        measure.audio_ms,
        measure.wall_ms,
    );
}

fn summary(tag: &str, mut values: Vec<i64>) {
    if values.is_empty() {
        return;
    }
    values.sort_unstable();
    let mean = values.iter().sum::<i64>() / values.len() as i64;
    println!(
        "{tag}: n {}, min {}, median {}, mean {mean}, max {} ms",
        values.len(),
        values[0],
        values[values.len() / 2],
        values[values.len() - 1]
    );
}

fn runs_mode(clip: &str, runs: usize) {
    let reference = read_wav(clip);
    let player = player();
    let capture = open();
    let mut missing = Vec::new();
    for run in 1..=runs {
        // Past the old channel's end, at a phase of its own.
        collect(&capture, &player, Duration::from_millis(4_000 + jitter_ms(run)), &mut Vec::new());
        let (chunks, started) = play(&capture, &player, &[clip]);
        let measure = measure(&reference, &chunks, started);
        report(&format!("run {run}"), &measure);
        missing.push(measure.missing_ms);
    }
    summary("missing", missing);
}

/// The issue's case: two `say` utterances as separate processes.
fn two_mode(first_clip: &str, second_clip: &str, first_text: &str, second_text: &str) {
    let capture = open();
    collect(&capture, "say", Duration::from_millis(3_000 + jitter_ms(0)), &mut Vec::new());
    for (tag, clip, text) in [("first", first_clip, first_text), ("second", second_clip, second_text)] {
        let reference = read_wav(clip);
        let (chunks, started) = play(&capture, "say", &[text]);
        report(tag, &measure(&reference, &chunks, started));
        collect(&capture, "say", Duration::from_millis(1_000), &mut Vec::new());
    }
}

/// CPU of this process while a capture runs and nothing new starts
/// (`clip` = one app playing it in a loop all along).
fn idle_mode(seconds: u64, clip: Option<&str>) {
    let player = player();
    let capture = open();
    let mut child = clip.map(|clip| spawn(&player, &[clip]));
    collect(&capture, &player, Duration::from_secs(3), &mut Vec::new());
    let (cpu_before, at) = (cpu_seconds(), Instant::now());
    let mut chunks = 0usize;
    while at.elapsed() < Duration::from_secs(seconds) {
        if let (Some(clip), Some(running)) = (clip, child.as_mut()) {
            if matches!(running.try_wait(), Ok(Some(_)) | Err(_)) {
                child = Some(spawn(&player, &[clip]));
            }
        }
        if capture.recv_timeout(Duration::from_millis(50)).is_ok() {
            chunks += 1;
        }
    }
    let cpu = cpu_seconds() - cpu_before;
    let wall = at.elapsed().as_secs_f64();
    println!("idle: {:.2} % of one core over {wall:.0} s ({cpu:.3} s CPU), {chunks} chunk(s) read", cpu / wall * 100.0);
    if let Some(mut child) = child {
        let _ = child.kill();
        let _ = child.wait();
    }
}

/// This process's user + system CPU time, seconds.
#[cfg(unix)]
fn cpu_seconds() -> f64 {
    #[repr(C)]
    struct Timeval {
        seconds: std::ffi::c_long,
        // suseconds_t: i32 on macOS (padded to 16 bytes), long on Linux.
        micros: std::ffi::c_long,
    }
    #[repr(C)]
    struct Rusage {
        user: Timeval,
        system: Timeval,
        rest: [std::ffi::c_long; 14],
    }
    extern "C" {
        fn getrusage(who: i32, usage: *mut Rusage) -> i32;
    }
    let mut usage: Rusage = unsafe { std::mem::zeroed() };
    if unsafe { getrusage(0, &mut usage) } != 0 {
        return 0.0;
    }
    let micros = |time: &Timeval| time.seconds as f64 + (time.micros as i32) as f64 / 1e6;
    micros(&usage.user) + micros(&usage.system)
}

#[cfg(not(unix))]
fn cpu_seconds() -> f64 {
    0.0
}

/// A 16 kHz mono 16-bit WAV's samples (`say -o x.wav --data-format=LEI16@16000`).
fn read_wav(path: &str) -> Vec<f32> {
    let bytes = std::fs::read(path).unwrap_or_else(|error| {
        println!("could not read {path}: {error}");
        std::process::exit(2);
    });
    let mut at = 12;
    while at + 8 <= bytes.len() {
        let size = u32::from_le_bytes([bytes[at + 4], bytes[at + 5], bytes[at + 6], bytes[at + 7]]) as usize;
        if &bytes[at..at + 4] == b"data" {
            let end = (at + 8 + size).min(bytes.len());
            return bytes[at + 8..end].chunks_exact(2).map(|pair| f32::from(i16::from_le_bytes([pair[0], pair[1]])) / 32_768.0).collect();
        }
        at += 8 + size + (size & 1);
    }
    println!("{path} has no data chunk");
    std::process::exit(2);
}
