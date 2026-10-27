//! NEW-55: the call's latency budget, measured on this machine.
//!
//!   cargo run --release -p voice --example call_latency -- <data-dir> [runs]
//!
//! The data dir must already hold the runtime and the four models
//! (`talk/crispasr-*`, `talk/models`): nothing is downloaded here. Loads the
//! speaker (Qwen3-TTS) AND the transcriber (Parakeet + Silero) once, like a
//! call on the voice device, then measures:
//!
//! 1. TTS time to first audio (resident): the first clause alone vs the
//!    whole first sentence, en + de, the built-in voices;
//! 2. end of speech → `Said` through `voice::listen::Call` (what a remote
//!    call's `CallAudio` feeds), TTS-made utterances in 20 ms chunks at
//!    real-time pace;
//! 3. speech start → first `Partial`;
//! 4. Opus encode + decode per 20 ms frame (16 kHz up, 24 kHz down).

use std::path::PathBuf;
use std::time::{Duration, Instant};

use crispasr::{models, Build, Device};
use voice::capture::Resampler;
use voice::listen::{Call, Transcriber, RATE};
use voice::listener::Engines;

const REPLIES: &[(&str, &[&str])] = &[
    (
        "en",
        &[
            "Three issues are open: NEW-55, NEW-56 and NEW-79.",
            "Done, I moved it to review.",
            "I looked at the failing build, and the problem is a missing migration that the new voice settings table needs.",
        ],
    ),
    (
        "de",
        &[
            "Drei Issues sind offen: NEW-55, NEW-56 und NEW-79.",
            "Erledigt, ich habe es nach Review verschoben.",
            "Ich habe mir den fehlgeschlagenen Build angesehen, und das Problem ist eine fehlende Migration für die neue Tabelle der Spracheinstellungen.",
        ],
    ),
];

const UTTERANCES: &[(&str, &[&str])] = &[
    (
        "en",
        &[
            "What is still open on the voice board?",
            "Move NEW-55 to review and assign it to me.",
            "Can you summarize the last pull request?",
            "Start a session on the flaky test.",
            "Thanks, that's all for now.",
        ],
    ),
    (
        "de",
        &[
            "Was ist auf dem Voice-Board noch offen?",
            "Schieb NEW-55 in Review und weise es mir zu.",
            "Kannst du den letzten Pull Request zusammenfassen?",
            "Starte eine Session für den wackeligen Test.",
            "Danke, das war's fürs Erste.",
        ],
    ),
];

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let data_dir = PathBuf::from(args.first().expect("usage: call_latency <data-dir> [runs]"));
    let runs: usize = args.get(1).and_then(|runs| runs.parse().ok()).unwrap_or(20);

    // ---- load both engines, like a call on the voice device ----
    let started = Instant::now();
    let runtime = voice::runtime::get(&data_dir, Build::Gpu, |_, _| {}).expect("runtime (must be on disk)");
    let models_dir = data_dir.join("talk").join(models::MODELS_DIR);
    for model in [models::QWEN3_TTS_0_6B_BASE, models::QWEN3_TTS_CODEC] {
        crispasr::artifact::verify(&models_dir, &model).expect("tts model on disk");
    }
    let mut tts = runtime
        .open_tts(
            "qwen3-tts",
            &models_dir.join(models::QWEN3_TTS_0_6B_BASE.keep[0].1),
            &models_dir.join(models::QWEN3_TTS_CODEC.keep[0].1),
            Device::Gpu,
        )
        .expect("open tts");
    let tts_rate = tts.sample_rate();
    let mut engines = Engines::load(&data_dir, false).expect("load the transcriber");
    eprintln!("loaded tts ({tts_rate} Hz) + asr ({:?}) in {} ms", engines.device(), started.elapsed().as_millis());
    assert_eq!(engines.device(), Device::Gpu, "the transcriber must be on the GPU");

    let voices = data_dir.join("talk").join("voices");
    let set_voice = |tts: &mut crispasr::TtsSession<'_>, language: &str| {
        let builtin = voice::builtin::for_language(language);
        let path = voice::builtin::materialize(&voices, builtin).expect("materialize");
        tts.set_voice(&path, builtin.transcript, builtin.language).expect("set voice");
    };
    // Warm-up per language (not counted): the first calls compile kernels.
    for (language, _) in REPLIES {
        set_voice(&mut tts, language);
        for _ in 0..3 {
            tts.synthesize("Hello, this is a quick voice check.").expect("warm-up");
        }
    }
    println!("\n## GPU memory, both engines resident (pid {})", std::process::id());
    print_gpu_memory();

    // ---- 1. TTS time to first audio ----
    println!("\n## 1. TTS time to first audio (resident, {runs} runs each)");
    println!("| lang | first synthesis call | words | audio ms | p50 ms | p95 ms | max ms |");
    println!("|---|---|---|---|---|---|---|");
    for (language, replies) in REPLIES {
        set_voice(&mut tts, language);
        for reply in *replies {
            let sentence = first_sentence(reply);
            let clause = voice::first_clause(sentence).map(|(front, _)| front);
            let mut variants = vec![("sentence", sentence)];
            if let Some(clause) = clause {
                variants.insert(0, ("clause", clause));
            }
            for (kind, text) in variants {
                let mut times = Vec::new();
                let mut audio_ms = Vec::new();
                for _ in 0..runs {
                    let at = Instant::now();
                    let pcm = tts.synthesize(text).expect("synthesize");
                    times.push(at.elapsed().as_secs_f64() * 1000.0);
                    audio_ms.push(pcm.len() as f64 * 1000.0 / f64::from(tts_rate));
                }
                let worker = if clause.is_none() || kind == "clause" { " (= worker's first call)" } else { "" };
                println!(
                    "| {language} | {kind}{worker}: \"{text}\" | {} | {:.0} | {:.0} | {:.0} | {:.0} |",
                    text.split_whitespace().count(),
                    percentile(&audio_ms, 50.0),
                    percentile(&times, 50.0),
                    percentile(&times, 95.0),
                    percentile(&times, 100.0),
                );
            }
        }
    }

    // ---- utterances for the call: TTS-made, resampled to 16 kHz ----
    let mut clips: Vec<(String, String, Vec<f32>)> = Vec::new();
    for (language, lines) in UTTERANCES {
        set_voice(&mut tts, language);
        for line in *lines {
            let pcm = tts.synthesize(line).expect("synthesize utterance");
            let mut out = Vec::new();
            Resampler::new(tts_rate, RATE as u32).process(&pcm, &mut out);
            clips.push((language.to_string(), line.to_string(), out));
        }
    }
    let downlink_pcm: Vec<f32> = {
        set_voice(&mut tts, "en");
        tts.synthesize(REPLIES[0].1[2]).expect("downlink sample")
    };
    drop(tts);

    // ---- 2 + 3. a call: end of speech → Said, speech start → Partial ----
    println!("\n## 2+3. Call: real-time 20 ms chunks through voice::listen::Call ({} clips × reps)", clips.len());
    let reps = runs.div_ceil(clips.len()).max(2);
    let mut said_ms = Vec::new();
    let mut decode_ms = Vec::new();
    let mut partial_ms = Vec::new();
    let mut partial_gap_ms = Vec::new();
    let mut started_ms = Vec::new();
    let mut multi = 0;
    for rep in 0..reps {
        for (language, line, clip) in &clips {
            let run = call_once(&mut engines, language, clip);
            if rep == 0 {
                eprintln!("{language} \"{line}\" → {:?} (segments {})", run.said_text, run.said_count);
            }
            if run.said_count != 1 {
                multi += 1;
            }
            if let Some(ms) = run.said_after_end_ms {
                said_ms.push(ms);
            }
            if let Some(ms) = run.final_decode_ms {
                decode_ms.push(ms);
            }
            if let Some(ms) = run.first_partial_ms {
                partial_ms.push(ms);
            }
            if let Some(ms) = run.speech_started_ms {
                started_ms.push(ms);
            }
            partial_gap_ms.extend(run.partial_gaps_ms);
        }
    }
    println!("| metric | n | p50 ms | p95 ms | max ms |");
    println!("|---|---|---|---|---|");
    for (name, values) in [
        ("last speech sample → Said", &said_ms),
        ("  of which the final decode (Parakeet)", &decode_ms),
        ("speech start → first Partial", &partial_ms),
        ("Partial → next Partial", &partial_gap_ms),
        ("speech start → SpeechStarted (barge-in)", &started_ms),
    ] {
        println!(
            "| {name} | {} | {:.0} | {:.0} | {:.0} |",
            values.len(),
            percentile(values, 50.0),
            percentile(values, 95.0),
            percentile(values, 100.0)
        );
    }
    println!("runs with != 1 Said segment: {multi} of {}", reps * clips.len());

    // ---- 4. Opus per 20 ms frame ----
    println!("\n## 4. Opus per 20 ms frame (CPU, one thread)");
    println!("| leg | encode p50 µs | encode p95 µs | decode p50 µs | decode p95 µs | bytes/frame p50 |");
    println!("|---|---|---|---|---|---|");
    let up: Vec<f32> = clips.iter().flat_map(|(_, _, clip)| clip.iter().copied()).collect();
    opus_leg("up 16 kHz 24 kbps FEC", &up, 16_000, 24_000, true);
    opus_leg("down 24 kHz 32 kbps", &downlink_pcm, 24_000, 32_000, false);
}

/// The worker's sentence split (`.`/`!`/`?` + space), first sentence only.
fn first_sentence(text: &str) -> &str {
    for (index, ch) in text.char_indices() {
        if matches!(ch, '.' | '!' | '?') && text[index + 1..].starts_with(' ') {
            return text[..=index].trim();
        }
    }
    text.trim()
}

struct Timed<'a> {
    engines: &'a mut Engines,
    calls: Vec<(Instant, Duration)>,
}

impl Transcriber for Timed<'_> {
    fn transcribe(&mut self, pcm: &[f32], language: &str) -> Result<String, String> {
        let at = Instant::now();
        let out = self.engines.transcribe(pcm, language);
        self.calls.push((at, at.elapsed()));
        out
    }
    fn partials(&self) -> bool {
        self.engines.partials()
    }
}

#[derive(Default)]
struct CallRun {
    said_text: Vec<String>,
    said_count: usize,
    said_after_end_ms: Option<f64>,
    final_decode_ms: Option<f64>,
    first_partial_ms: Option<f64>,
    partial_gaps_ms: Vec<f64>,
    speech_started_ms: Option<f64>,
}

/// One utterance through a fresh call: 0.6 s of near-silence, the clip,
/// 1.5 s of near-silence, in 20 ms chunks on a real-time schedule (a chunk
/// is handed over when its last sample would have been captured).
fn call_once(engines: &mut Engines, language: &str, clip: &[f32]) -> CallRun {
    let lead = RATE * 600 / 1000;
    let (speech_start, speech_end) = speech_bounds(clip);
    let mut audio = noise(lead, 1);
    audio.extend_from_slice(clip);
    audio.extend(noise(RATE * 3 / 2, 2));
    let start_sample = lead + speech_start;
    let end_sample = lead + speech_end;

    let mut vad = engines.vad().expect("vad");
    let mut timed = Timed { engines, calls: Vec::new() };
    let mut call = Call::new("call", language);
    let mut events: Vec<(Instant, talk::Event)> = Vec::new();
    let chunk = RATE / 50;
    let t0 = Instant::now();
    let at = |sample: usize| t0 + Duration::from_nanos(sample as u64 * 1_000_000_000 / RATE as u64);
    for (index, samples) in audio.chunks(chunk).enumerate() {
        let due = at((index + 1) * chunk);
        let now = Instant::now();
        if due > now {
            std::thread::sleep(due - now);
        }
        call.feed(samples, &mut vad, &mut timed, &mut |event| {
            if !matches!(event, talk::Event::Level { .. }) {
                events.push((Instant::now(), event));
            }
        })
        .expect("feed");
    }
    let ms = |from: Instant, to: Instant| to.saturating_duration_since(from).as_secs_f64() * 1000.0;
    let mut run = CallRun::default();
    let mut last_partial: Option<Instant> = None;
    for (when, event) in &events {
        match event {
            talk::Event::Said { text, .. } => {
                run.said_count += 1;
                run.said_text.push(text.clone());
                if *when >= at(end_sample) {
                    run.said_after_end_ms = Some(ms(at(end_sample), *when));
                    // The decode that produced it: the last transcribe started before it.
                    run.final_decode_ms = timed
                        .calls
                        .iter()
                        .rev()
                        .find(|(started, took)| *started + *took <= *when)
                        .map(|(_, took)| took.as_secs_f64() * 1000.0);
                }
            }
            talk::Event::Partial { .. } => {
                if run.first_partial_ms.is_none() {
                    run.first_partial_ms = Some(ms(at(start_sample), *when));
                }
                if let Some(previous) = last_partial {
                    run.partial_gaps_ms.push(ms(previous, *when));
                }
                last_partial = Some(*when);
            }
            talk::Event::SpeechStarted { .. } => {
                if run.speech_started_ms.is_none() {
                    run.speech_started_ms = Some(ms(at(start_sample), *when));
                }
            }
            _ => {}
        }
    }
    run
}

/// First and one-past-last sample of speech: 10 ms windows within 35 dB
/// of the loudest one.
fn speech_bounds(clip: &[f32]) -> (usize, usize) {
    let window = RATE / 100;
    let rms: Vec<f32> =
        clip.chunks(window).map(|part| (part.iter().map(|sample| sample * sample).sum::<f32>() / part.len() as f32).sqrt()).collect();
    let peak = rms.iter().copied().fold(0.0, f32::max);
    let floor = peak * 10f32.powf(-35.0 / 20.0);
    let first = rms.iter().position(|value| *value > floor).unwrap_or(0);
    let last = rms.iter().rposition(|value| *value > floor).unwrap_or(rms.len() - 1);
    (first * window, ((last + 1) * window).min(clip.len()))
}

/// Room-tone at about -66 dBFS (deterministic).
fn noise(len: usize, seed: u32) -> Vec<f32> {
    let mut state = 0x9E37_79B9u32 ^ seed;
    (0..len)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            (state as f32 / u32::MAX as f32 - 0.5) * 0.001
        })
        .collect()
}

fn opus_leg(name: &str, pcm: &[f32], rate: u32, bitrate: i32, fec: bool) {
    let mut encoder = opus_rs::OpusEncoder::new(rate as i32, 1, opus_rs::Application::Voip).expect("encoder");
    encoder.bitrate_bps = bitrate;
    if fec {
        encoder.use_inband_fec = true;
        encoder.packet_loss_perc = 10;
    }
    let mut decoder = opus_rs::OpusDecoder::new(rate as i32, 1).expect("decoder");
    let frame = (rate / 50) as usize;
    let pcm: Vec<i16> = pcm.iter().map(|sample| talk::to_i16(*sample)).collect();
    let mut encode = Vec::new();
    let mut decode = Vec::new();
    let mut sizes = Vec::new();
    let mut packet = [0u8; 1500];
    let mut out = vec![0f32; frame];
    // At least 3000 frames (60 s of audio), the clip looped.
    let frames: Vec<&[i16]> = pcm.chunks_exact(frame).collect();
    for index in 0..3000.max(frames.len()) {
        let input = frames[index % frames.len()];
        let at = Instant::now();
        let size = encoder.encode_i16(input, frame, &mut packet).expect("encode");
        encode.push(at.elapsed().as_secs_f64() * 1e6);
        sizes.push(size as f64);
        let at = Instant::now();
        decoder.decode(&packet[..size], frame, &mut out).expect("decode");
        decode.push(at.elapsed().as_secs_f64() * 1e6);
    }
    println!(
        "| {name} | {:.0} | {:.0} | {:.0} | {:.0} | {:.0} |",
        percentile(&encode, 50.0),
        percentile(&encode, 95.0),
        percentile(&decode, 50.0),
        percentile(&decode, 95.0),
        percentile(&sizes, 50.0)
    );
}

fn percentile(values: &[f64], p: f64) -> f64 {
    if values.is_empty() {
        return f64::NAN;
    }
    let mut sorted = values.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let rank = (p / 100.0 * (sorted.len() - 1) as f64).round() as usize;
    sorted[rank.min(sorted.len() - 1)]
}

fn print_gpu_memory() {
    let pid = std::process::id().to_string();
    if let Ok(out) = std::process::Command::new("nvidia-smi")
        .args(["--query-compute-apps=pid,process_name,used_memory", "--format=csv,noheader"])
        .output()
    {
        let text = String::from_utf8_lossy(&out.stdout);
        let ours: Vec<&str> = text.lines().filter(|line| line.trim_start().starts_with(&pid)).collect();
        println!("this process: {}", if ours.is_empty() { "not listed".to_string() } else { ours.join("; ") });
        println!("all compute apps:\n{}", text.trim_end());
    }
    if let Ok(out) = std::process::Command::new("nvidia-smi").args(["--query-gpu=name,memory.used,memory.total", "--format=csv,noheader"]).output() {
        println!("gpu: {}", String::from_utf8_lossy(&out.stdout).trim());
    }
}
