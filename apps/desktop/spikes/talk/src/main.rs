//! NEW-52: the exponentialTalk spike harness. One binary, one subcommand per
//! engine under test, every measurement printed as ONE JSON line so
//! `scripts/collect.py` can fold them into `numbers.json`. Nothing here is
//! product code: the traits of `crates/talk` are what the subcommands shape.
//!
//!   talk-spike zipformer <model-dir> <wav>...          streaming ASR (sherpa-onnx)
//!   talk-spike vad <silero.onnx> <wav>...              end-of-utterance lag (Silero)
//!   talk-spike kws <model-dir> <keywords-file> <wav>.. keyword spotting hits
//!   talk-spike kokoro <model-dir> <phrases.json> <lang> <sid> <out-dir>
//!                                                      TTS time to first audio + RTF
//!   talk-spike whistle <wav>...                        (feature `cactus`) clip ASR
//!
//! Streams are fed in 100 ms chunks the way a microphone would deliver them;
//! "audio_ms" numbers are how much audio had to be heard, "wall_ms" numbers
//! are compute. The tail (compute after the last sample) is what a person
//! waits for after they stop talking, assuming the chunks kept up.

use std::env;
use std::fs;
use std::path::Path;
use std::time::Instant;

use anyhow::{anyhow, bail, Context, Result};
use serde::Serialize;
use sherpa_onnx::{
    GenerationConfig, KeywordSpotter, KeywordSpotterConfig, OfflineTts, OfflineTtsConfig,
    OfflineTtsKokoroModelConfig, OfflineTtsModelConfig, OnlineModelConfig, OnlineRecognizer,
    OnlineRecognizerConfig, OnlineTransducerModelConfig, SileroVadModelConfig, VadModelConfig,
    VoiceActivityDetector, Wave,
};

const CHUNK_MS: usize = 100;

fn main() -> Result<()> {
    let args: Vec<String> = env::args().skip(1).collect();
    let Some((cmd, rest)) = args.split_first() else {
        bail!("usage: talk-spike <zipformer|vad|kws|kokoro|whistle> ...");
    };
    match cmd.as_str() {
        "zipformer" => zipformer(rest),
        "vad" => vad(rest),
        "kws" => kws(rest),
        "kokoro" => kokoro(rest),
        #[cfg(feature = "cactus")]
        "whistle" => whistle(rest),
        other => bail!("unknown subcommand {other}"),
    }
}

fn emit<T: Serialize>(value: &T) {
    println!("{}", serde_json::to_string(value).expect("json"));
}

fn threads() -> i32 {
    env::var("TALK_THREADS").ok().and_then(|v| v.parse().ok()).unwrap_or(4)
}

fn read_wave(path: &str) -> Result<Wave> {
    let wave = Wave::read(path).ok_or_else(|| anyhow!("cannot read {path}"))?;
    if wave.sample_rate() != 16000 {
        bail!("{path}: expected 16 kHz, got {}", wave.sample_rate());
    }
    Ok(wave)
}

fn find(dir: &Path, needle: &str, exclude: &str) -> Result<String> {
    for entry in fs::read_dir(dir).with_context(|| format!("read {}", dir.display()))? {
        let path = entry?.path();
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if name.starts_with(needle) && name.ends_with(".onnx") && !name.contains(exclude) {
            return Ok(path.to_string_lossy().into_owned());
        }
    }
    bail!("no {needle}*.onnx in {}", dir.display())
}

fn transducer(dir: &Path, int8: bool) -> Result<OnlineModelConfig> {
    let exclude = if int8 { "\u{0}" } else { ".int8" };
    let mut cfg = OnlineModelConfig::default();
    cfg.transducer = OnlineTransducerModelConfig {
        encoder: Some(find(dir, if int8 { "encoder" } else { "encoder" }, exclude)?),
        decoder: Some(find(dir, "decoder", exclude)?),
        joiner: Some(find(dir, "joiner", exclude)?),
    };
    if int8 {
        // the int8 encoder is the one WITH `.int8`; re-pick it explicitly
        cfg.transducer.encoder = Some(pick_int8(dir, "encoder")?);
        cfg.transducer.decoder = Some(pick_int8(dir, "decoder")?);
        cfg.transducer.joiner = Some(pick_int8(dir, "joiner")?);
    }
    cfg.tokens = Some(dir.join("tokens.txt").to_string_lossy().into_owned());
    cfg.num_threads = threads();
    cfg.provider = Some(env::var("TALK_PROVIDER").unwrap_or_else(|_| "cpu".into()));
    Ok(cfg)
}

fn pick_int8(dir: &Path, needle: &str) -> Result<String> {
    for entry in fs::read_dir(dir)? {
        let path = entry?.path();
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if name.starts_with(needle) && name.ends_with(".int8.onnx") {
            return Ok(path.to_string_lossy().into_owned());
        }
    }
    // KWS and some ASR bundles have no int8 decoder/joiner: fall back to fp32
    find(dir, needle, ".int8")
}

#[derive(Serialize)]
struct AsrRow<'a> {
    engine: &'a str,
    wav: &'a str,
    audio_ms: u64,
    load_ms: u64,
    first_partial_audio_ms: Option<u64>,
    first_partial_wall_ms: Option<u64>,
    first_partial: Option<String>,
    tail_ms: u64,
    compute_ms: u64,
    rtf: f64,
    text: String,
}

fn zipformer(args: &[String]) -> Result<()> {
    let [dir, wavs @ ..] = args else { bail!("zipformer <model-dir> <wav>...") };
    let int8 = env::var("TALK_INT8").is_ok();
    let t0 = Instant::now();
    let mut config = OnlineRecognizerConfig::default();
    config.model_config = transducer(Path::new(dir), int8)?;
    config.decoding_method = Some("greedy_search".into());
    config.enable_endpoint = true;
    config.rule1_min_trailing_silence = 2.4;
    config.rule2_min_trailing_silence = 1.2;
    config.rule3_min_utterance_length = 20.0;
    let recognizer = OnlineRecognizer::create(&config).ok_or_else(|| anyhow!("recognizer"))?;
    let load_ms = t0.elapsed().as_millis() as u64;
    let engine = if int8 { "zipformer-int8" } else { "zipformer" };

    for wav in wavs {
        let wave = read_wave(wav)?;
        let samples = wave.samples();
        let chunk = 16000 * CHUNK_MS / 1000;
        let stream = recognizer.create_stream();
        let mut fed = 0usize;
        let mut compute = 0u128;
        let mut first: Option<(u64, u64, String)> = None;
        let started = Instant::now();
        while fed < samples.len() {
            let end = (fed + chunk).min(samples.len());
            let t = Instant::now();
            stream.accept_waveform(16000, &samples[fed..end]);
            while recognizer.is_ready(&stream) {
                recognizer.decode(&stream);
            }
            compute += t.elapsed().as_millis();
            fed = end;
            if first.is_none() {
                if let Some(r) = recognizer.get_result(&stream) {
                    if !r.text.trim().is_empty() {
                        first = Some((
                            (fed * 1000 / 16000) as u64,
                            started.elapsed().as_millis() as u64,
                            r.text.trim().to_string(),
                        ));
                    }
                }
            }
        }
        let t = Instant::now();
        // half a second of silence, as the VAD's end-of-utterance would add
        stream.accept_waveform(16000, &vec![0f32; 8000]);
        stream.input_finished();
        while recognizer.is_ready(&stream) {
            recognizer.decode(&stream);
        }
        let text = recognizer.get_result(&stream).map(|r| r.text).unwrap_or_default();
        let tail_ms = t.elapsed().as_millis() as u64;
        compute += tail_ms as u128;
        let audio_ms = (samples.len() * 1000 / 16000) as u64;
        emit(&AsrRow {
            engine,
            wav,
            audio_ms,
            load_ms,
            first_partial_audio_ms: first.as_ref().map(|f| f.0),
            first_partial_wall_ms: first.as_ref().map(|f| f.1),
            first_partial: first.as_ref().map(|f| f.2.clone()),
            tail_ms,
            compute_ms: compute as u64,
            rtf: compute as f64 / audio_ms.max(1) as f64,
            text: text.trim().to_string(),
        });
    }
    Ok(())
}

#[derive(Serialize)]
struct VadRow<'a> {
    engine: &'a str,
    wav: &'a str,
    audio_ms: u64,
    segments: Vec<VadSegment>,
    compute_ms: u64,
}

#[derive(Serialize)]
struct VadSegment {
    start_ms: u64,
    end_ms: u64,
    /// audio heard past the segment's end before the detector emitted it
    eou_lag_ms: u64,
}

fn vad(args: &[String]) -> Result<()> {
    let [model, wavs @ ..] = args else { bail!("vad <silero.onnx> <wav>...") };
    let min_silence: f32 = env::var("TALK_VAD_SILENCE").ok().and_then(|v| v.parse().ok()).unwrap_or(0.5);
    for wav in wavs {
        let config = VadModelConfig {
            silero_vad: SileroVadModelConfig {
                model: Some(model.clone()),
                threshold: 0.5,
                min_silence_duration: min_silence,
                min_speech_duration: 0.25,
                window_size: 512,
                max_speech_duration: 20.0,
            },
            sample_rate: 16000,
            num_threads: 1,
            provider: Some("cpu".into()),
            ..Default::default()
        };
        let vad = VoiceActivityDetector::create(&config, 30.0).ok_or_else(|| anyhow!("vad"))?;
        let wave = read_wave(wav)?;
        let samples = wave.samples();
        // A person stops talking and silence follows: the clip gets 1.5 s of
        // it, so the lag measured is the detector's own, not the clip's end.
        let mut padded = samples.to_vec();
        padded.extend(std::iter::repeat(0f32).take(16000 * 3 / 2));
        let samples = &padded[..];
        let mut fed = 0usize;
        let mut segments = Vec::new();
        let mut compute = 0u128;
        while fed < samples.len() {
            let end = (fed + 512).min(samples.len());
            let t = Instant::now();
            vad.accept_waveform(&samples[fed..end]);
            if end == samples.len() {
                vad.flush();
            }
            compute += t.elapsed().as_micros();
            fed = end;
            while let Some(seg) = vad.front() {
                let start = seg.start() as usize;
                let seg_end = start + seg.n() as usize;
                segments.push(VadSegment {
                    start_ms: (start * 1000 / 16000) as u64,
                    end_ms: (seg_end * 1000 / 16000) as u64,
                    eou_lag_ms: (fed.saturating_sub(seg_end) * 1000 / 16000) as u64,
                });
                vad.pop();
            }
        }
        emit(&VadRow {
            engine: "silero-vad",
            wav,
            audio_ms: (samples.len() * 1000 / 16000) as u64,
            segments,
            compute_ms: (compute / 1000) as u64,
        });
    }
    Ok(())
}

#[derive(Serialize)]
struct KwsRow<'a> {
    engine: &'a str,
    wav: &'a str,
    audio_ms: u64,
    hits: Vec<KwsHit>,
    compute_ms: u64,
    rtf: f64,
}

#[derive(Serialize)]
struct KwsHit {
    keyword: String,
    /// audio heard when the hit was reported
    at_ms: u64,
}

fn kws(args: &[String]) -> Result<()> {
    let [dir, keywords, wavs @ ..] = args else { bail!("kws <model-dir> <keywords-file> <wav>...") };
    let dir = Path::new(dir);
    let mut config = KeywordSpotterConfig::default();
    config.model_config = transducer(dir, env::var("TALK_INT8").is_ok())?;
    config.keywords_file = Some(keywords.clone());
    if let Some(th) = env::var("TALK_KWS_THRESHOLD").ok().and_then(|v| v.parse().ok()) {
        config.keywords_threshold = th;
    }
    let spotter = KeywordSpotter::create(&config).ok_or_else(|| anyhow!("keyword spotter"))?;
    for wav in wavs {
        let wave = read_wave(wav)?;
        let samples = wave.samples();
        let stream = spotter.create_stream();
        let chunk = 16000 * CHUNK_MS / 1000;
        let mut fed = 0usize;
        let mut hits = Vec::new();
        let mut compute = 0u128;
        while fed < samples.len() {
            let end = (fed + chunk).min(samples.len());
            let t = Instant::now();
            stream.accept_waveform(16000, &samples[fed..end]);
            while spotter.is_ready(&stream) {
                spotter.decode(&stream);
                if let Some(r) = spotter.get_result(&stream) {
                    if !r.keyword.is_empty() {
                        hits.push(KwsHit { keyword: r.keyword, at_ms: (end * 1000 / 16000) as u64 });
                        spotter.reset(&stream);
                    }
                }
            }
            compute += t.elapsed().as_millis();
            fed = end;
        }
        let audio_ms = (samples.len() * 1000 / 16000) as u64;
        emit(&KwsRow {
            engine: "sherpa-kws-zipformer",
            wav,
            audio_ms,
            hits,
            compute_ms: compute as u64,
            rtf: compute as f64 / audio_ms.max(1) as f64,
        });
    }
    Ok(())
}

#[derive(Serialize)]
struct TtsRow<'a> {
    engine: &'a str,
    id: String,
    lang: String,
    chars: usize,
    load_ms: u64,
    /// wall time until the first chunk of samples came back
    ttfa_ms: u64,
    wall_ms: u64,
    audio_ms: u64,
    rtf: f64,
    out: String,
}

#[derive(serde::Deserialize)]
struct Phrases {
    phrases: Vec<Phrase>,
}

#[derive(serde::Deserialize)]
struct Phrase {
    id: String,
    lang: String,
    #[serde(rename = "ref")]
    _ref: String,
    /// what a person says aloud: identifiers spelled, numbers as words
    spoken: String,
}

fn kokoro(args: &[String]) -> Result<()> {
    let [dir, phrases, lang, sid, out] = args else {
        bail!("kokoro <model-dir> <phrases.json> <lang> <sid> <out-dir>")
    };
    let dir = Path::new(dir);
    let sid: i32 = sid.parse()?;
    fs::create_dir_all(out)?;
    let t0 = Instant::now();
    let model = OfflineTtsModelConfig {
        kokoro: OfflineTtsKokoroModelConfig {
            model: Some(dir.join("model.onnx").to_string_lossy().into_owned()),
            voices: Some(dir.join("voices.bin").to_string_lossy().into_owned()),
            tokens: Some(dir.join("tokens.txt").to_string_lossy().into_owned()),
            // 2.0: the lexicon replaces espeak-ng; data_dir stays unset on purpose
            lexicon: Some(format!(
                "{},{}",
                dir.join("lexicon-us-en.txt").to_string_lossy(),
                dir.join("lexicon-zh.txt").to_string_lossy()
            )),
            dict_dir: Some(dir.join("dict").to_string_lossy().into_owned()),
            length_scale: 1.0,
            lang: Some(lang.clone()),
            ..Default::default()
        },
        num_threads: threads(),
        provider: Some(env::var("TALK_PROVIDER").unwrap_or_else(|_| "cpu".into())),
        ..Default::default()
    };
    let config = OfflineTtsConfig { model, max_num_sentences: 1, silence_scale: 0.2, ..Default::default() };
    let tts = OfflineTts::create(&config).ok_or_else(|| anyhow!("kokoro: create failed"))?;
    let load_ms = t0.elapsed().as_millis() as u64;
    let set: Phrases = serde_json::from_str(&fs::read_to_string(phrases)?)?;
    for phrase in set.phrases.into_iter().filter(|p| &p.lang == lang) {
        let path = format!("{out}/{}.wav", phrase.id);
        let started = Instant::now();
        let first = std::sync::Arc::new(std::sync::Mutex::new(None::<u64>));
        let first_cb = first.clone();
        let gen = GenerationConfig { sid, speed: 1.0, ..Default::default() };
        let audio = tts
            .generate_with_config(
                &phrase.spoken,
                &gen,
                Some(move |_samples: &[f32], _progress: f32| {
                    let mut slot = first_cb.lock().unwrap();
                    if slot.is_none() {
                        *slot = Some(started.elapsed().as_millis() as u64);
                    }
                    true
                }),
            )
            .ok_or_else(|| anyhow!("kokoro: generate failed for {}", phrase.id))?;
        let wall_ms = started.elapsed().as_millis() as u64;
        let audio_ms = (audio.samples().len() as u64 * 1000) / audio.sample_rate().max(1) as u64;
        audio.save(&path);
        let ttfa = first.lock().unwrap().unwrap_or(wall_ms);
        emit(&TtsRow {
            engine: "kokoro-v1.0",
            id: phrase.id,
            lang: phrase.lang,
            chars: phrase.spoken.chars().count(),
            load_ms,
            ttfa_ms: ttfa,
            wall_ms,
            audio_ms,
            rtf: wall_ms as f64 / audio_ms.max(1) as f64,
            out: path,
        });
    }
    Ok(())
}

#[cfg(feature = "cactus")]
fn whistle(args: &[String]) -> Result<()> {
    use cactus_rs::whistle::{Weights, Whistle};
    let t0 = Instant::now();
    let mut whistle = Whistle::builder(Weights::fetch()?).build()?;
    let load_ms = t0.elapsed().as_millis() as u64;
    for wav in args {
        let wave = read_wave(wav)?;
        let samples = wave.samples();
        let t = Instant::now();
        let transcript = whistle.transcribe(samples)?;
        let wall = t.elapsed().as_millis() as u64;
        let audio_ms = (samples.len() * 1000 / 16000) as u64;
        emit(&AsrRow {
            engine: "whistle",
            wav,
            audio_ms,
            load_ms,
            first_partial_audio_ms: Some(audio_ms),
            first_partial_wall_ms: Some(wall),
            first_partial: None,
            tail_ms: wall,
            compute_ms: wall,
            rtf: wall as f64 / audio_ms.max(1) as f64,
            text: transcript.text().trim().to_string(),
        });
    }
    Ok(())
}
