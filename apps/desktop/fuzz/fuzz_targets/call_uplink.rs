//! NEW-88: the call host's exact receive path (`etalk_codec::Uplink`, what
//! `steer::call_host` feeds every caller packet): any sequence numbers (in
//! order, lost, reordered, wrapped) with any packets.
//! Oracle: no panic, no memory error (ASan), at most two frames per packet.
//!
//! Input: `[seq u16 BE][len u16 LE][packet]…` (seeds: `examples/seeds.rs`).
#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let mut uplink = etalk_codec::Uplink::new(16_000).expect("a valid uplink");
    let mut rest = data;
    while rest.len() >= 4 {
        let seq = u16::from_be_bytes([rest[0], rest[1]]);
        let len = (u16::from_le_bytes([rest[2], rest[3]]) as usize).min(rest.len() - 4);
        // The wire frame is ≤ 1500 bytes, 3 of them header.
        let packet = &rest[4..4 + len.min(1497)];
        rest = &rest[4 + len..];
        let pcm = uplink.push(seq, packet);
        assert!(pcm.len() <= 640, "{} samples from one packet", pcm.len());
    }
});
