//! NEW-88: any packet sequence through an Opus decoder at any rate, channel
//! count and frame size: plain decodes, FEC decodes and concealment.
//! Oracle: no panic, no memory error (ASan), a sane size and finite PCM.
//!
//! Input: `[config]` then `[op][frame][len u16 LE][packet]…`; config bits
//! 0-2 = rate, bit 3 = stereo; op % 3 = decode, FEC, lost; frame < 0x80 =
//! an Opus duration (index), else `(frame & 0x7f) * 45` samples (any size up
//! to 120 ms at 48 kHz). `examples/seeds.rs` writes seeds.
#![no_main]

use libfuzzer_sys::fuzz_target;

const RATES: [u32; 5] = [8_000, 12_000, 16_000, 24_000, 48_000];
/// 2.5, 5, 10, 20, 40, 60 ms at 48 kHz, then 80, 100, 120 ms.
const FRAMES_48K: [usize; 9] = [120, 240, 480, 960, 1920, 2880, 3840, 4800, 5760];

fuzz_target!(|data: &[u8]| {
    let Some((&config, mut rest)) = data.split_first() else { return };
    let rate = RATES[(config & 7) as usize % RATES.len()];
    let channels = if config & 8 != 0 { 2 } else { 1 };
    let mut decoder = etalk_codec::Decoder::new(rate, channels).expect("a valid decoder");
    while rest.len() >= 4 {
        let (op, frame) = (rest[0] % 3, rest[1]);
        let len = (u16::from_le_bytes([rest[2], rest[3]]) as usize).min(rest.len() - 4);
        let packet = if op == 2 { &[][..] } else { &rest[4..4 + len] };
        rest = &rest[4 + len..];
        // The wire caps a frame at 1500 bytes (voice-call.json maxFrameBytes).
        let packet = &packet[..packet.len().min(1500)];
        let frame = if frame < 0x80 {
            FRAMES_48K[frame as usize % FRAMES_48K.len()] * rate as usize / 48_000
        } else {
            (frame & 0x7f) as usize * 45
        };
        let mut out = vec![0f32; frame * channels];
        let result = if op == 1 { decoder.decode_fec(packet, &mut out) } else { decoder.decode(packet, &mut out) };
        if let Ok(size) = result {
            assert!(size <= frame, "decoded {size} samples into room for {frame}");
            for sample in &out[..size * channels] {
                assert!(sample.is_finite(), "non-finite PCM {sample} from {packet:02x?}");
            }
        }
    }
});
