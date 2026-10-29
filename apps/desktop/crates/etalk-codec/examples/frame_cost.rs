//! NEW-88: what one 20 ms Opus frame costs on this machine, on both legs of
//! a call (`voice-call.json` AUDIO): up = the caller's 16 kHz at 24 kbps with
//! in-band FEC (decoded by the call host), down = the reply's 24 kHz at
//! 32 kbps (encoded by it). CPU only, one thread; run it inside the heavy
//! lane, once per backend:
//!
//!   cargo run --release -p call-codec --example frame_cost -- <speech.wav> [frames]
//!   cargo run --release -p call-codec --features libopus --example frame_cost -- …
//!
//! The clip is 16-bit mono PCM (`crates/etalk-engine/assets/builtin-en.wav`),
//! resampled to each leg's rate and looped.

use std::path::Path;
use std::time::Instant;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let wav = args.first().expect("usage: frame_cost <speech.wav> [frames]");
    let frames: usize = args.get(1).and_then(|frames| frames.parse().ok()).unwrap_or(15_000);
    let (clip, clip_rate) = read_wav(Path::new(wav));
    println!("backend {}, {frames} frames per leg, {}", etalk_codec::BACKEND, std::env::consts::ARCH);
    println!("| leg | encode p50 µs | encode p95 µs | encode p99 µs | decode p50 µs | decode p95 µs | decode p99 µs | FEC decode p50 µs | bytes p50 |");
    println!("|---|---|---|---|---|---|---|---|---|");
    leg("up 16 kHz 24 kbps FEC", &clip, clip_rate, 16_000, 24_000, Some(10), frames);
    leg("down 24 kHz 32 kbps", &clip, clip_rate, 24_000, 32_000, None, frames);
}

fn leg(name: &str, clip: &[i16], clip_rate: u32, rate: u32, bitrate: i32, loss: Option<i32>, count: usize) {
    let pcm = resample(clip, clip_rate, rate);
    let frame = (rate / 50) as usize;
    let chunks: Vec<&[i16]> = pcm.chunks_exact(frame).collect();
    let mut encoder = etalk_codec::Encoder::new(rate, bitrate, loss).expect("encoder");
    let mut decoder = etalk_codec::Decoder::new(rate, 1).expect("decoder");
    let mut fec_decoder = etalk_codec::Decoder::new(rate, 1).expect("decoder");
    let (mut encode, mut decode, mut fec, mut sizes) = (Vec::new(), Vec::new(), Vec::new(), Vec::new());
    let mut packet = [0u8; 1500];
    let mut out = vec![0f32; frame];
    // 200 frames of warm-up, not counted.
    for index in 0..count + 200 {
        let input = chunks[index % chunks.len()];
        let at = Instant::now();
        let size = encoder.encode(input, &mut packet).expect("encode");
        let encoded = at.elapsed();
        let at = Instant::now();
        decoder.decode(&packet[..size], &mut out).expect("decode");
        let decoded = at.elapsed();
        let at = Instant::now();
        if loss.is_some() {
            fec_decoder.decode_fec(&packet[..size], &mut out).expect("decode fec");
        }
        let fec_decoded = at.elapsed();
        fec_decoder.decode(&packet[..size], &mut out).expect("decode");
        if index >= 200 {
            encode.push(encoded.as_secs_f64() * 1e6);
            decode.push(decoded.as_secs_f64() * 1e6);
            fec.push(fec_decoded.as_secs_f64() * 1e6);
            sizes.push(size as f64);
        }
    }
    let p = percentile;
    let fec = if loss.is_some() { format!("{:.1}", p(&fec, 50.0)) } else { "-".to_string() };
    println!(
        "| {name} | {:.1} | {:.1} | {:.1} | {:.1} | {:.1} | {:.1} | {fec} | {:.0} |",
        p(&encode, 50.0),
        p(&encode, 95.0),
        p(&encode, 99.0),
        p(&decode, 50.0),
        p(&decode, 95.0),
        p(&decode, 99.0),
        p(&sizes, 50.0)
    );
}

fn percentile(values: &[f64], p: f64) -> f64 {
    let mut sorted = values.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
    sorted[((p / 100.0 * (sorted.len() - 1) as f64).round() as usize).min(sorted.len() - 1)]
}

/// A 16-bit mono WAV's samples and rate (the `fmt ` and `data` chunks).
fn read_wav(path: &Path) -> (Vec<i16>, u32) {
    let bytes = std::fs::read(path).expect("read the clip");
    let mut at = 12;
    let mut rate = 0;
    while at + 8 <= bytes.len() {
        let id = &bytes[at..at + 4];
        let len = u32::from_le_bytes(bytes[at + 4..at + 8].try_into().unwrap()) as usize;
        let body = &bytes[at + 8..(at + 8 + len).min(bytes.len())];
        if id == b"fmt " {
            assert_eq!(u16::from_le_bytes([body[2], body[3]]), 1, "mono only");
            rate = u32::from_le_bytes(body[4..8].try_into().unwrap());
        } else if id == b"data" {
            return (body.chunks_exact(2).map(|pair| i16::from_le_bytes([pair[0], pair[1]])).collect(), rate);
        }
        at += 8 + len + (len & 1);
    }
    panic!("no data chunk in {}", path.display());
}

/// Linear resampling (the cost does not depend on its quality).
fn resample(pcm: &[i16], from: u32, to: u32) -> Vec<i16> {
    let len = pcm.len() as u64 * u64::from(to) / u64::from(from);
    (0..len)
        .map(|index| {
            let position = index as f64 * f64::from(from) / f64::from(to);
            let (whole, part) = (position as usize, position.fract());
            let a = f64::from(pcm[whole.min(pcm.len() - 1)]);
            let b = f64::from(pcm[(whole + 1).min(pcm.len() - 1)]);
            (a + (b - a) * part) as i16
        })
        .collect()
}
