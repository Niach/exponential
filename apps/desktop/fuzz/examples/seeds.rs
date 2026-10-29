//! NEW-88: seed corpora for the Opus fuzz targets (`apps/desktop/fuzz`),
//! real packets of every mode the encoder makes (SILK, hybrid, CELT; FEC on
//! and off; mono and stereo; every rate), coded from a speech clip.
//!
//!   cargo run --release --example seeds -- <speech.wav> <corpus>   (in apps/desktop/fuzz; run.sh does it)
//!
//! The clip is 16-bit mono PCM (`crates/etalk-engine/assets/builtin-en.wav`); the
//! seeds are written to `<corpus>/opus_decode` and `<corpus>/call_uplink`
//! (gitignored, never committed). An encoder panic while seeding is
//! printed as `ENCODER PANIC: <config>` (opus-rs 0.1.40: in-band FEC at
//! 24/48 kHz and 20-28 kbps, restsend/opus-rs; the call never encodes so).

use std::path::{Path, PathBuf};

use opus_rs::{Application, OpusEncoder};

const RATES: [u32; 5] = [8_000, 12_000, 16_000, 24_000, 48_000];

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [wav, corpus] = &args[..] else { panic!("usage: seeds <speech.wav> <corpus-dir>") };
    let (clip, clip_rate) = read_wav(Path::new(wav));
    let corpus = PathBuf::from(corpus);
    let decode_dir = corpus.join("opus_decode");
    let uplink_dir = corpus.join("call_uplink");
    std::fs::create_dir_all(&decode_dir).unwrap();
    std::fs::create_dir_all(&uplink_dir).unwrap();
    let mut written = 0;
    for (rate_index, rate) in RATES.into_iter().enumerate() {
        let pcm = resample(&clip, clip_rate, rate);
        for stereo in [false, true] {
            for (app_name, app) in [("voip", Application::Voip), ("audio", Application::Audio), ("lowdelay", Application::RestrictedLowDelay)] {
                for (bitrate, fec, hybrid) in [(12_000, false, false), (24_000, true, false), (32_000, false, true), (64_000, false, false)] {
                    let channels = if stereo { 2 } else { 1 };
                    let Ok(mut encoder) = OpusEncoder::new(rate as i32, channels, app) else { continue };
                    encoder.bitrate_bps = bitrate;
                    if fec {
                        encoder.use_inband_fec = true;
                        encoder.packet_loss_perc = 10;
                    }
                    if hybrid && encoder.enable_hybrid_mode().is_err() {
                        continue;
                    }
                    for frame_ms in [10u32, 20, 40, 60] {
                        let frame = (rate * frame_ms / 1000) as usize;
                        let name = format!("{rate}-{}-{app_name}-{bitrate}-{}{}-{frame_ms}ms", channels, if fec { "fec" } else { "nofec" }, if hybrid { "-hybrid" } else { "" });
                        // An encoder panic is a finding (NEW-88): report it, go on.
                        let Ok(packets) = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| encode(&mut encoder, &pcm, frame, channels, 12))) else {
                            eprintln!("ENCODER PANIC: {name}");
                            continue;
                        };
                        if packets.is_empty() {
                            continue;
                        }
                        // opus_decode: decodes, a FEC of the next and a loss mid-way.
                        let mut seed = vec![rate_index as u8 | if stereo { 8 } else { 0 }];
                        let frame_index = [480, 960, 1920, 2880].iter().position(|n| *n == frame_ms as usize * 48).unwrap() as u8 + 2;
                        for (index, packet) in packets.iter().enumerate() {
                            let op = match index { 5 => 2, 6 => 1, _ => 0 };
                            seed.extend([op, frame_index]);
                            seed.extend((packet.len() as u16).to_le_bytes());
                            seed.extend(packet);
                            if op == 1 {
                                seed.extend([0, frame_index]);
                                seed.extend((packet.len() as u16).to_le_bytes());
                                seed.extend(packet);
                            }
                        }
                        std::fs::write(decode_dir.join(name), &seed).unwrap();
                        written += 1;
                        // call_uplink: what a caller sends (16 kHz mono 20 ms), seq gaps.
                        if rate == 16_000 && !stereo && frame_ms == 20 {
                            let mut seed = Vec::new();
                            for (index, packet) in packets.iter().enumerate() {
                                if index == 4 {
                                    continue;
                                }
                                let seq = (index as u16).wrapping_add(65_530);
                                seed.extend(seq.to_be_bytes());
                                seed.extend((packet.len() as u16).to_le_bytes());
                                seed.extend(packet);
                            }
                            std::fs::write(uplink_dir.join(format!("{app_name}-{bitrate}-{}", if fec { "fec" } else { "nofec" })), &seed).unwrap();
                            written += 1;
                        }
                    }
                }
            }
        }
    }
    println!("{written} seeds in {}", corpus.display());
}

fn encode(encoder: &mut OpusEncoder, pcm: &[i16], frame: usize, channels: usize, count: usize) -> Vec<Vec<u8>> {
    let interleaved: Vec<i16> = pcm.iter().flat_map(|sample| std::iter::repeat_n(*sample, channels)).collect();
    // Skip the clip's leading silence: start a third in.
    let start = (pcm.len() / 3) * channels;
    let mut packet = [0u8; 1500];
    interleaved[start..]
        .chunks_exact(frame * channels)
        .take(count)
        .filter_map(|input| encoder.encode_i16(input, frame, &mut packet).ok().map(|size| packet[..size].to_vec()))
        .collect()
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

/// Linear resampling (seeds only need plausible speech).
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
