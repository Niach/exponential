//! NEW-88: barge-in against the agent's own voice, on real audio devices.
//!
//!   cargo run --release -p voice --example echo_check -- <data-dir> \
//!       --sink <pipewire node> --source <pipewire node> [--seconds 30] [--cpu] [--reply <wav>]...
//!
//! A local call's reply plays through `--sink` (one `pw-play` per utterance,
//! like the worker's sink, behind the worker's [`TapSink`]) while nobody
//! speaks, and `--source` is recorded (`pw-record`, the capture's own
//! arguments). The microphone runs two ways at once through the call's own
//! pipeline (`voice::listen::Call`, Silero, Parakeet): RAW (no canceller:
//! does the echo reach the microphone at all?) and the product's path
//! through the canceller (`Echo::cancel`: the far end's reference for the
//! chunk, then [`EchoCanceller`]). `--dump <dir>` writes both streams
//! (`near.f32`, `far.f32`, 16 kHz; local only, never committed). A SpeechStarted on the cancelled path while nobody
//! speaks is a barge-in tripped by the agent's own voice. Prints the counts,
//! what was "heard", and the echo's level before and after the canceller.
//!
//! `--reply` takes 16-bit mono WAVs (default: the built-in voices'
//! clips). The data dir must hold the runtime and the listening models
//! (`Engines::download` fetches them); `--sink`/`--source` take any
//! PipeWire node name (`pactl list short sinks|sources`), e.g. a null sink
//! and its monitor for a loopback run.

use std::io::Write as _;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use voice::capture::read_wav;
use voice::echo::{EchoCanceller, FarEnd, TapSink};
use voice::listen::{Call, RATE};
use voice::listener::Engines;
use voice::sink::Sink;

/// The clips play at the rate `read_wav` gives (the worker's 24 kHz voice
/// reaches the canceller resampled to the same 16 kHz).
const REPLY_RATE: u32 = RATE as u32;
/// Silence between two utterances of the reply.
const GAP: Duration = Duration::from_millis(400);

fn main() {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let flag = |args: &mut Vec<String>, name: &str| -> Option<String> {
        let at = args.iter().position(|arg| arg == name)?;
        args.remove(at);
        Some(args.remove(at))
    };
    let sink = flag(&mut args, "--sink").expect("--sink <pipewire node>");
    let source = flag(&mut args, "--source").expect("--source <pipewire node>");
    let seconds: u64 = flag(&mut args, "--seconds").map(|seconds| seconds.parse().expect("--seconds")).unwrap_or(30);
    let dump = flag(&mut args, "--dump").map(PathBuf::from);
    let mut replies = Vec::new();
    while let Some(path) = flag(&mut args, "--reply") {
        replies.push(PathBuf::from(path));
    }
    let cpu = args.iter().any(|arg| arg == "--cpu");
    args.retain(|arg| arg != "--cpu");
    let data_dir = PathBuf::from(args.first().expect("usage: echo_check <data-dir> --sink <node> --source <node>"));
    if replies.is_empty() {
        let assets = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets");
        replies = vec![assets.join("builtin-en.wav"), assets.join("builtin-de.wav")];
    }
    let clips: Vec<Vec<f32>> = replies.iter().map(|path| read_wav(&std::fs::read(path).expect("read a reply")).expect("a 16-bit mono WAV")).collect();

    let mut engines = Engines::load(&data_dir, cpu).expect("load the listening models");
    let mut vad_raw = engines.vad().expect("vad");
    let mut vad_aec = engines.vad().expect("vad");
    let far = Arc::new(FarEnd::default());
    // `Echo::cancel` spelled out (take the reference, cancel), so `--dump`
    // can keep both streams.
    far.set_enabled(true);
    let mut canceller = EchoCanceller::new();
    let (mut near_all, mut far_all) = (Vec::new(), Vec::new());
    let mut raw = Call::new("raw", "en");
    let mut aec = Call::new("aec", "en");

    // `<sink>.monitor` records what that sink plays (a loopback run).
    let mut record = Command::new("pw-record");
    match source.strip_suffix(".monitor") {
        Some(sink) => record.args(["--target", sink, "-P", "{ stream.capture.sink=true }"]),
        None => record.args(["--target", &source]),
    };
    let mut recorder = record
        .args(["--rate", &RATE.to_string(), "--channels", "1", "--format", "f32", "--latency", "20ms", "-"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("pw-record");
    let mut stdout = recorder.stdout.take().expect("piped");
    let (tx, rx) = std::sync::mpsc::channel::<Vec<f32>>();
    std::thread::spawn(move || {
        use std::io::Read as _;
        let mut bytes = [0u8; 4 * 320];
        loop {
            if stdout.read_exact(&mut bytes).is_err() {
                return;
            }
            let samples = bytes.chunks_exact(4).map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]])).collect();
            if tx.send(samples).is_err() {
                return;
            }
        }
    });

    // The reply: one player per utterance, back to back, behind a TapSink,
    // after 1.5 s of room tone (the noise floor), for `seconds`.
    let playing = Arc::new(Mutex::new(false));
    let player = {
        let (far, sink, playing) = (far.clone(), sink.clone(), playing.clone());
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(1500));
            *playing.lock().unwrap() = true;
            let until = Instant::now() + Duration::from_secs(seconds);
            let mut played = 0;
            while Instant::now() < until {
                let clip = &clips[played % clips.len()];
                let out = Arc::new(TapSink::new(Arc::new(Player::open(&sink)), far.clone(), REPLY_RATE));
                out.push(clip).expect("play");
                out.finish();
                played += 1;
                std::thread::sleep(GAP);
            }
            // The last echo dies away.
            std::thread::sleep(Duration::from_millis(800));
            *playing.lock().unwrap() = false;
            played
        })
    };

    let mut events: Vec<(&str, f32, talk::Event)> = Vec::new();
    let (mut floor, mut mic_during, mut out_during) = (Energy::default(), Energy::default(), Energy::default());
    let started = Instant::now();
    let mut ever_played = false;
    loop {
        let Ok(near) = rx.recv_timeout(Duration::from_secs(2)) else { panic!("the microphone stopped") };
        let now = *playing.lock().unwrap();
        ever_played |= now;
        let reference = far.take(near.len());
        let cleaned = canceller.process(&near, &reference);
        if dump.is_some() {
            near_all.extend_from_slice(&near);
            far_all.extend_from_slice(&reference);
        }
        let at = started.elapsed().as_secs_f32();
        if now {
            mic_during.add(&near);
            out_during.add(&cleaned);
        } else if !ever_played {
            floor.add(&near);
        }
        for (name, call, vad, samples) in [("raw", &mut raw, &mut vad_raw, &near[..]), ("aec", &mut aec, &mut vad_aec, &cleaned[..])] {
            call.feed(samples, vad, &mut engines, &mut |event| {
                if !matches!(event, talk::Event::Level { .. } | talk::Event::Partial { .. }) {
                    events.push((name, at, event));
                }
            })
            .expect("feed");
        }
        if ever_played && !now {
            break;
        }
    }
    let played = player.join().expect("player");
    if let Some(dir) = &dump {
        std::fs::create_dir_all(dir).expect("dump dir");
        for (name, samples) in [("near.f32", &near_all), ("far.f32", &far_all)] {
            let bytes: Vec<u8> = samples.iter().flat_map(|sample| sample.to_le_bytes()).collect();
            std::fs::write(dir.join(name), bytes).expect("dump");
        }
    }
    let _ = recorder.kill();

    println!("sink {sink}, source {source}, {played} utterances in {seconds} s, canceller tail {} ms", voice::echo::TAIL_MS);
    println!("room tone (before the reply): {:.1} dBFS", floor.dbfs());
    println!("microphone during the reply: {:.1} dBFS (raw), {:.1} dBFS after the canceller ({:.1} dB removed)", mic_during.dbfs(), out_during.dbfs(), mic_during.dbfs() - out_during.dbfs());
    for name in ["raw", "aec"] {
        let started = events.iter().filter(|(path, _, event)| *path == name && matches!(event, talk::Event::SpeechStarted { .. })).count();
        println!("{name}: {started} SpeechStarted (barge-in) while nobody spoke");
        for (_, at, event) in events.iter().filter(|(path, _, _)| *path == name) {
            match event {
                talk::Event::SpeechStarted { .. } => println!("  {at:6.2}s SpeechStarted"),
                talk::Event::Said { text, .. } => println!("  {at:6.2}s Said {text:?}"),
                other => println!("  {at:6.2}s {other:?}"),
            }
        }
    }
}

/// `pw-play` on one node, the worker sink's arguments plus `--target`.
struct Player(Mutex<Option<Child>>);

impl Player {
    fn open(target: &str) -> Self {
        let child = Command::new("pw-play")
            .args(["--target", target, "--format", "f32", "--rate", &REPLY_RATE.to_string(), "--channels", "1", "-"])
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("pw-play");
        Player(Mutex::new(Some(child)))
    }
}

impl Sink for Player {
    fn push(&self, samples: &[f32]) -> Result<(), String> {
        let mut child = self.0.lock().unwrap();
        let stdin = child.as_mut().and_then(|child| child.stdin.as_mut()).ok_or("closed")?;
        let bytes: Vec<u8> = samples.iter().flat_map(|sample| sample.to_le_bytes()).collect();
        stdin.write_all(&bytes).map_err(|err| err.to_string())
    }

    fn finish(&self) {
        if let Some(mut child) = self.0.lock().unwrap().take() {
            drop(child.stdin.take());
            let _ = child.wait();
        }
    }

    fn stop(&self) {
        if let Some(mut child) = self.0.lock().unwrap().take() {
            let _ = child.kill();
        }
    }
}

#[derive(Default)]
struct Energy {
    sum: f64,
    count: usize,
}

impl Energy {
    fn add(&mut self, samples: &[f32]) {
        self.sum += samples.iter().map(|sample| f64::from(*sample) * f64::from(*sample)).sum::<f64>();
        self.count += samples.len();
    }

    fn dbfs(&self) -> f64 {
        10.0 * (self.sum / self.count.max(1) as f64).max(1e-12).log10()
    }
}
