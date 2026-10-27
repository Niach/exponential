//! NEW-88 + #NEW-92: barge-in against the agent's own voice, on real audio
//! devices.
//!
//!   cargo run --release -p voice --example echo_check -- <data-dir> \
//!       --sink <pipewire node> --source <pipewire node> [--seconds 30] [--cpu] [--reply <wav>]... \
//!       [--interject <wav> --interject-at <s> [--interject-every <n>] [--interject-gain <dB>]] [--no-stop] [--dump <dir>] [--frames <csv>]
//!   cargo run --release -p voice --example echo_check -- <data-dir> --replay <dump dir> [--cpu] [--frames <csv>]
//!
//! A local call's replies play through `--sink` the product's way: ONE
//! stream for the whole run ([`voice::sink::open_stream_on`], silence between
//! replies), tapped into the far end, each reply a [`Reply`] on it. `--source`
//! is recorded (`pw-record`, the capture's own arguments) and runs the
//! product's path: [`Echo::cancel`], the call's barge-in gate
//! ([`Call::set_echo`] with [`Echo::state`]), Silero, Parakeet, and the echo
//! text rule ([`voice::listen::echo_of`] against the playing reply's
//! sentences). A SpeechStarted while only the reply plays is a SELF-TRIP.
//! Next to it the same cancelled audio runs through an UNGATED call (NEW-88's
//! rule: 250 ms of speech) for comparison.
//!
//! `--interject <wav>` plays another voice into the same sink (its own
//! `pw-play`, mixed by the sink) `--interject-at` seconds into every
//! `--interject-every`-th reply (default every 2nd, starting with the 2nd):
//! a barge-in during it is a real one, and the reply then stops like the
//! worker's (`--no-stop` keeps every reply playing to its end). `--dump`
//! writes the microphone and the reference (`near.f32`, `far.f32`, 16 kHz;
//! local only, never committed); `--replay` runs a dump through the same
//! path without devices (no interjections, nothing stops). `--frames` writes
//! one CSV row per 32 ms frame (levels, VAD, the gate's state, the
//! canceller's numbers) for tuning.
//!
//! `--reply` takes 16-bit mono WAVs (default: the built-in voices' clips,
//! their transcripts as the sentences). The data dir must hold the runtime
//! and the listening models (`Engines::download` fetches them).

use std::io::Write as _;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use voice::capture::read_wav;
use voice::echo::{dbfs, Echo, FarEnd};
use voice::listen::{Call, Vad, FRAME, RATE};
use voice::listener::Engines;
use voice::sink::{Reply, Sink};

/// The clips play at the rate `read_wav` gives.
const REPLY_RATE: u32 = RATE as u32;
/// Silence between two replies.
const GAP: Duration = Duration::from_millis(400);

struct Clip {
    samples: Vec<f32>,
    sentences: Vec<String>,
}

/// What happened when, on the run's clock (seconds).
#[derive(Default)]
struct Timeline {
    /// [start, end) of every reply, and whether it was stopped.
    replies: Vec<(f32, f32, bool)>,
    /// [start, end) of every interjection.
    interjections: Vec<(f32, f32)>,
}

fn main() {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let flag = |args: &mut Vec<String>, name: &str| -> Option<String> {
        let at = args.iter().position(|arg| arg == name)?;
        args.remove(at);
        Some(args.remove(at))
    };
    let switch = |args: &mut Vec<String>, name: &str| -> bool {
        let found = args.iter().any(|arg| arg == name);
        args.retain(|arg| arg != name);
        found
    };
    let replay = flag(&mut args, "--replay").map(PathBuf::from);
    let sink = flag(&mut args, "--sink");
    let source = flag(&mut args, "--source");
    let seconds: u64 = flag(&mut args, "--seconds").map(|seconds| seconds.parse().expect("--seconds")).unwrap_or(30);
    let dump = flag(&mut args, "--dump").map(PathBuf::from);
    let frames_csv = flag(&mut args, "--frames").map(PathBuf::from);
    let interject = flag(&mut args, "--interject").map(PathBuf::from);
    let interject_at: f32 = flag(&mut args, "--interject-at").map(|at| at.parse().expect("--interject-at")).unwrap_or(3.0);
    let interject_gain: f32 = flag(&mut args, "--interject-gain").map(|db| db.parse::<f32>().expect("--interject-gain <dB>")).map_or(1.0, |db| 10f32.powf(db / 20.0));
    let interject_every: usize = flag(&mut args, "--interject-every").map(|every| every.parse().expect("--interject-every")).unwrap_or(2);
    let mut replies = Vec::new();
    while let Some(path) = flag(&mut args, "--reply") {
        replies.push(PathBuf::from(path));
    }
    let cpu = switch(&mut args, "--cpu");
    let no_stop = switch(&mut args, "--no-stop") || replay.is_some();
    let data_dir = PathBuf::from(args.first().expect("usage: echo_check <data-dir> (--sink <node> --source <node> | --replay <dir>)"));
    let clips: Vec<Clip> = if replies.is_empty() {
        voice::builtin::BUILTIN
            .iter()
            .map(|builtin| Clip {
                samples: read_wav(builtin.wav).expect("a built-in clip"),
                sentences: voice::speech_chunks(builtin.transcript, true).into_iter().map(str::to_string).collect(),
            })
            .collect()
    } else {
        replies
            .iter()
            .map(|path| Clip { samples: read_wav(&std::fs::read(path).expect("read a reply")).expect("a 16-bit mono WAV"), sentences: Vec::new() })
            .collect()
    };

    let replay_sentences: Vec<String> = clips.iter().flat_map(|clip| clip.sentences.clone()).collect();
    let mut engines = Engines::load(&data_dir, cpu).expect("load the listening models");
    let mut vad_gated = engines.vad().expect("vad");
    let mut vad_ungated = engines.vad().expect("vad");
    let mut vad_probe = engines.vad().expect("vad");
    let far = Arc::new(FarEnd::default());
    let mut echo = Echo::new(far.clone());
    let mut gated = Call::new("gated", "en");
    let mut ungated = Call::new("ungated", "en");
    let (mut near_all, mut far_all) = (Vec::new(), Vec::new());
    let mut frames_out = frames_csv.as_ref().map(|path| {
        let mut file = std::io::BufWriter::new(std::fs::File::create(path).expect("--frames file"));
        writeln!(file, "t,far_db,near_db,out_db,vad,reference_db,floor_db,erle_db,locked,double_talk").unwrap();
        file
    });

    // The reply's sentences while it plays (the worker's `Links::spoken`).
    let spoken: Arc<Mutex<(Vec<String>, Option<Instant>)>> = Arc::default();
    let timeline: Arc<Mutex<Timeline>> = Arc::default();
    if let Some(dir) = &replay {
        // The live run's timeline, on the microphone's clock.
        if let Ok(text) = std::fs::read_to_string(dir.join("timeline.txt")) {
            let mut timeline = timeline.lock().unwrap();
            for line in text.lines() {
                let parts: Vec<&str> = line.split(' ').collect();
                let at = |index: usize| parts[index].parse::<f32>().expect("a timeline number");
                match parts[0] {
                    "reply" => timeline.replies.push((at(1), at(2), parts.get(3) == Some(&"stopped"))),
                    "interjection" => timeline.interjections.push((at(1), at(2))),
                    _ => {}
                }
            }
        }
    }
    let started = Instant::now();
    let barged = Arc::new(AtomicBool::new(false));
    let current: Arc<Mutex<Option<Arc<Reply>>>> = Arc::default();

    // The microphone: recorded live, or read back from a dump.
    // Each microphone chunk, with its reference when replayed.
    let (tx, rx) = std::sync::mpsc::channel::<(Vec<f32>, Option<Vec<f32>>)>();
    let mut recorder: Option<Child> = None;
    let mut player = None;
    let finished = Arc::new(AtomicBool::new(false));
    if let Some(dir) = &replay {
        let read = |name: &str| -> Vec<f32> {
            std::fs::read(dir.join(name)).expect("a dump").chunks_exact(4).map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]])).collect()
        };
        let (near, reference) = (read("near.f32"), read("far.f32"));
        for (near, reference) in near.chunks(320).zip(reference.chunks(320)) {
            let _ = tx.send((near.to_vec(), Some(reference.to_vec())));
        }
        drop(tx);
    } else {
        let sink = sink.clone().expect("--sink <pipewire node>");
        let source = source.clone().expect("--source <pipewire node>");
        // `<sink>.monitor` records what that sink plays (a loopback run).
        let mut record = Command::new("pw-record");
        match source.strip_suffix(".monitor") {
            Some(sink) => record.args(["--target", sink, "-P", "{ stream.capture.sink=true }"]),
            None => record.args(["--target", &source]),
        };
        let mut child = record
            .args(["--rate", &RATE.to_string(), "--channels", "1", "--format", "f32", "--latency", "20ms", "-"])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("pw-record");
        let mut stdout = child.stdout.take().expect("piped");
        recorder = Some(child);
        std::thread::spawn(move || {
            use std::io::Read as _;
            let mut bytes = [0u8; 4 * 320];
            loop {
                if stdout.read_exact(&mut bytes).is_err() {
                    return;
                }
                let samples = bytes.chunks_exact(4).map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]])).collect();
                if tx.send((samples, None)).is_err() {
                    return;
                }
            }
        });

        // The replies: one stream, after 1.5 s of room tone, for `seconds`.
        let interjection = interject.as_ref().map(|path| {
            let samples = read_wav(&std::fs::read(path).expect("read --interject")).expect("a 16-bit mono WAV");
            samples.into_iter().map(|sample| sample * interject_gain).collect::<Vec<f32>>()
        });
        let (far, spoken, timeline, barged, current, finished) =
            (far.clone(), spoken.clone(), timeline.clone(), barged.clone(), current.clone(), finished.clone());
        player = Some(std::thread::spawn(move || {
            let stream = voice::sink::open_stream_on(REPLY_RATE, Box::new(move |rate| FarEnd::tap(&far, rate)), &sink).expect("open the call's stream");
            std::thread::sleep(Duration::from_millis(1500));
            let until = Instant::now() + Duration::from_secs(seconds);
            let mut played = 0;
            while Instant::now() < until {
                let clip = &clips[played % clips.len()];
                let reply = Arc::new(Reply::new(stream.clone()));
                *spoken.lock().unwrap() = (clip.sentences.clone(), None);
                *current.lock().unwrap() = Some(reply.clone());
                barged.store(false, Ordering::SeqCst);
                let at = started.elapsed().as_secs_f32();
                // Queued at once (the stream paces it), like a fast GPU's sentences.
                reply.push(&clip.samples).expect("play");
                let mut mixer = None;
                if let Some(samples) = interjection.as_ref().filter(|_| played % interject_every == interject_every - 1) {
                    while started.elapsed().as_secs_f32() < at + interject_at {
                        std::thread::sleep(Duration::from_millis(5));
                    }
                    mixer = Some(play_once(&sink, samples, &timeline, started));
                }
                reply.finish();
                let stopped = barged.load(Ordering::SeqCst);
                timeline.lock().unwrap().replies.push((at, started.elapsed().as_secs_f32(), stopped));
                spoken.lock().unwrap().1 = Some(Instant::now());
                if let Some(mixer) = mixer {
                    let _ = mixer.join();
                }
                played += 1;
                std::thread::sleep(GAP);
            }
            // The last echo dies away.
            std::thread::sleep(Duration::from_millis(1500));
            finished.store(true, Ordering::SeqCst);
            played
        }));
    }

    let mut events: Vec<(&str, f32, talk::Event)> = Vec::new();
    let mut fed = 0usize;
    // The microphone's clock on the run's: when its first chunk arrived
    // (pw-record's own latency is below a frame).
    let mut mic_start: Option<f32> = None;
    let mut probe_frames = 0usize;
    // Near and reference as they came in, the canceller's output as it came
    // out (it may return a few samples more or less per chunk).
    let (mut window_in, mut window_out): (Vec<(f32, f32)>, Vec<f32>) = (Vec::new(), Vec::new());
    loop {
        let (near, replayed) = match rx.recv_timeout(Duration::from_secs(2)) {
            Ok(chunk) => chunk,
            Err(_) if replay.is_some() => break,
            Err(_) => panic!("the microphone stopped"),
        };
        if replay.is_none() && finished.load(Ordering::SeqCst) {
            break;
        }
        if let Some(reference) = &replayed {
            far.push(reference);
        }
        let cleaned = echo.cancel(&near);
        let reference = echo.reference().to_vec();
        if dump.is_some() {
            near_all.extend_from_slice(&near);
            far_all.extend_from_slice(&reference);
        }
        let mic_start = *mic_start.get_or_insert_with(|| if replay.is_some() { 0.0 } else { started.elapsed().as_secs_f32() - near.len() as f32 / RATE as f32 });
        let at = mic_start + fed as f32 / RATE as f32;
        fed += near.len();
        let state = echo.state();
        // Per-frame numbers for `--frames` (their own VAD on the same audio).
        if let Some(out) = frames_out.as_mut() {
            window_in.extend(near.iter().copied().zip(reference.iter().copied()));
            window_out.extend_from_slice(&cleaned);
            while window_in.len() >= FRAME && window_out.len() >= FRAME {
                let cleaned_frame: Vec<f32> = window_out.drain(..FRAME).collect();
                let frame: Vec<(f32, f32, f32)> = window_in.drain(..FRAME).zip(cleaned_frame.iter().copied()).map(|((n, f), o)| (n, f, o)).collect();
                // One frame in, one probability out: the row's numbers line up.
                let probability = vad_probe.push(&cleaned_frame).expect("vad").first().copied().unwrap_or(0.0);
                let level = |pick: fn(&(f32, f32, f32)) -> f32| dbfs(&frame.iter().map(pick).collect::<Vec<_>>());
                let metrics = echo.metrics();
                let (erle, locked, double_talk) = metrics.map_or((0.0, false, false), |m| {
                    (m.canceller.erle_db, matches!(m.delay.status, decibri_aec::DelayStatus::Locked(_)), m.canceller.double_talk)
                });
                writeln!(
                    out,
                    "{:.3},{:.1},{:.1},{:.1},{:.3},{:.1},{:.1},{:.1},{},{}",
                    mic_start + probe_frames as f32 * FRAME as f32 / RATE as f32,
                    level(|s| s.1),
                    level(|s| s.0),
                    level(|s| s.2),
                    probability,
                    state.reference_db,
                    gated.echo_gate().floor(),
                    erle,
                    u8::from(locked),
                    u8::from(double_talk)
                )
                .unwrap();
                probe_frames += 1;
            }
        }
        gated.set_echo(state);
        let reply_sentences = {
            let spoken = spoken.lock().unwrap();
            match spoken.1 {
                Some(ended) if ended.elapsed() >= Duration::from_secs(3) => Vec::new(),
                _ => spoken.0.clone(),
            }
        };
        let sentences = if replay.is_some() { &replay_sentences } else { &reply_sentences };
        // The product's echo text rule runs inside the gated call; the
        // check names what it dropped (what was heard after a reply that
        // repeats it and never barged in over it).
        gated.set_spoken(sentences.clone());
        for (name, call, vad) in [("gated", &mut gated, &mut vad_gated), ("ungated", &mut ungated, &mut vad_ungated)] {
            call.feed(&cleaned, vad, &mut engines, &mut |event| {
                if name == "gated" {
                    if matches!(event, talk::Event::SpeechStarted { .. }) && !no_stop {
                        // The worker's barge-in: the reply stops.
                        barged.store(true, Ordering::SeqCst);
                        if let Some(reply) = current.lock().unwrap().as_ref() {
                            reply.stop();
                        }
                    }
                }
                if !matches!(event, talk::Event::Level { .. } | talk::Event::Partial { .. }) {
                    events.push((name, at, event));
                }
            })
            .expect("feed");
        }
    }
    let played = player.map(|player| player.join().expect("player")).unwrap_or(0);
    if let Some(dir) = &dump {
        std::fs::create_dir_all(dir).expect("dump dir");
        let start = mic_start.unwrap_or(0.0);
        let timeline = timeline.lock().unwrap();
        let mut text = String::new();
        for (from, to, stopped) in &timeline.replies {
            text.push_str(&format!("reply {} {} {}\n", from - start, to - start, if *stopped { "stopped" } else { "played" }));
        }
        for (from, to) in &timeline.interjections {
            text.push_str(&format!("interjection {} {}\n", from - start, to - start));
        }
        std::fs::write(dir.join("timeline.txt"), text).expect("dump");
        for (name, samples) in [("near.f32", &near_all), ("far.f32", &far_all)] {
            let bytes: Vec<u8> = samples.iter().flat_map(|sample| sample.to_le_bytes()).collect();
            std::fs::write(dir.join(name), bytes).expect("dump");
        }
    }
    if let Some(mut recorder) = recorder {
        let _ = recorder.kill();
    }

    let timeline = timeline.lock().unwrap();
    // An interjection's barge-in may land up to its end plus the 250 ms closing silence.
    let during_interjection = |at: f32| timeline.interjections.iter().any(|(start, end)| at >= *start && at <= end + 0.5);
    match &replay {
        Some(dir) => println!("replay of {} ({:.1} s)", dir.display(), fed as f32 / RATE as f32),
        None => println!(
            "sink {}, source {}, {played} replies in {seconds} s on one stream, {} interjections, canceller tail {} ms",
            sink.as_deref().unwrap_or_default(),
            source.as_deref().unwrap_or_default(),
            timeline.interjections.len(),
            voice::echo::TAIL_MS
        ),
    }
    if let Some(metrics) = echo.metrics() {
        println!(
            "canceller: delay {:?} samples, ERLE {:.1} dB, divergence resets {}; the gate's echo floor {:.1} dB",
            metrics.delay_samples,
            metrics.canceller.erle_db,
            metrics.canceller.divergence_resets,
            gated.echo_gate().floor()
        );
    }
    for name in ["gated", "ungated"] {
        let starts: Vec<f32> = events
            .iter()
            .filter(|(path, _, event)| *path == name && matches!(event, talk::Event::SpeechStarted { .. }))
            .map(|(_, at, _)| *at)
            .collect();
        let real = timeline.interjections.iter().filter(|(start, end)| starts.iter().any(|at| at >= start && *at <= end + 0.5)).count();
        let self_trips = starts.iter().filter(|at| !during_interjection(**at)).count();
        println!(
            "{name}: {self_trips} self-trips (SpeechStarted with only the reply playing) over {played} replies; {real} of {} interjections barged in",
            timeline.interjections.len()
        );
        for (_, at, event) in events.iter().filter(|(path, _, _)| *path == name) {
            let mark = if during_interjection(*at) { "interjection" } else { "" };
            match event {
                talk::Event::SpeechStarted { .. } => println!("  {at:6.2}s SpeechStarted {mark}"),
                talk::Event::Said { text, .. } => println!("  {at:6.2}s Said {text:?} {mark}"),
                other => println!("  {at:6.2}s {other:?}"),
            }
        }
    }
    println!("dropped as the reply's echo (the echo text rule): {}", gated.echo_dropped());
    for (start, end, stopped) in timeline.replies.iter() {
        println!("  reply {start:6.2}-{end:6.2}s{}", if *stopped { " (barged in)" } else { "" });
    }
    for (start, end) in timeline.interjections.iter() {
        println!("  interjection {start:6.2}-{end:6.2}s");
    }
}

/// Play `samples` once on `sink` (its own `pw-play`, mixed by the sink) and
/// note when, on the run's clock.
fn play_once(sink: &str, samples: &[f32], timeline: &Arc<Mutex<Timeline>>, started: Instant) -> std::thread::JoinHandle<()> {
    let (sink, samples, timeline) = (sink.to_string(), samples.to_vec(), timeline.clone());
    std::thread::spawn(move || {
        let mut child = Command::new("pw-play")
            .args(["--target", &sink, "--format", "f32", "--rate", &REPLY_RATE.to_string(), "--channels", "1", "-"])
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("pw-play");
        let at = started.elapsed().as_secs_f32();
        let bytes: Vec<u8> = samples.iter().flat_map(|sample| sample.to_le_bytes()).collect();
        let _ = child.stdin.as_mut().expect("piped").write_all(&bytes);
        drop(child.stdin.take());
        let _ = child.wait();
        timeline.lock().unwrap().interjections.push((at, at + samples.len() as f32 / RATE as f32));
    })
}
