//! #EXP-1941: voice-call.json `seal` through the facade, as Swift and Kotlin
//! call it. No fixed eph secret crosses the binding, so the vectors replay
//! inside Rust (`call_seal_selftest`); one live round trip goes through the
//! two exported objects.

use std::sync::Arc;

use exponential_net_ffi::*;

const FIXTURE: &str = include_str!("../../../../../packages/etalk/fixtures/voice-call.json");
const ROOT: &str = "11111111-1111-4111-8111-111111111111";

fn reason<T>(result: Result<T, EnetError>) -> String {
    match result {
        Ok(_) => "accepted".into(),
        Err(EnetError::Crypto { reason }) => reason,
        Err(other) => other.to_string(),
    }
}

fn finish(peer_hello: Vec<u8>) -> EnetCallFinish {
    EnetCallFinish {
        secret: vec![7; 32],
        call_id: "22222222-2222-4222-8222-222222222222".into(),
        agent_id: "33333333-3333-4333-8333-333333333333".into(),
        device_id: "desk-7f3a".into(),
        peer_hello,
        root_id: Some(ROOT.into()),
        epoch: 3,
        pad: call_seal_pad(),
    }
}

/// A caller that knew no root and a host, hello for hello.
fn call() -> (Arc<EnetCallHandshake>, Arc<EnetCallSeal>, Vec<u8>) {
    let caller = EnetCallHandshake::new(EnetCallRole::Caller, None, 0).unwrap();
    let host = EnetCallHandshake::new(EnetCallRole::Host, Some(ROOT.into()), 3).unwrap();
    let host = host.finish(finish(caller.hello().unwrap())).unwrap();
    let hello = host.host_hello().unwrap();
    (caller, host, hello)
}

#[test]
fn the_selftest_replays_the_fixture_and_names_what_fails() {
    call_seal_selftest(FIXTURE.into()).unwrap_or_else(|failed| panic!("{failed}"));
    let failed = reason(call_seal_selftest(FIXTURE.replace("021df2d28ad2384e35f042fbb30b425d", "031df2d28ad2384e35f042fbb30b425d")));
    assert!(failed.contains("hostHelloTag"), "{failed}");
    assert!(call_seal_selftest("{}".into()).is_err());
}

#[test]
fn a_live_round_trip_through_the_two_objects() {
    let (caller_side, host, host_hello) = call();
    assert_eq!((caller_side.hello().unwrap().len(), host_hello.len()), (54, 70));
    // A host seals nothing before a caller frame opened.
    assert_eq!(reason(host.seal_audio(0, 0, vec![1, 2, 3])), "call seal: unproven");
    assert_eq!(reason(host.seal_control("{}".into())), "call seal: unproven");
    let caller = caller_side.finish(finish(host_hello.clone())).unwrap();
    assert_eq!(reason(caller_side.finish(finish(host_hello))), "call seal: finished", "a second finish");

    let up = caller.seal_audio(9, 1, vec![0xf8, 0xff, 0xfe]).unwrap();
    assert_eq!(up.len(), 27 + call_seal_pad().audio_up as usize);
    assert_eq!(host.open_audio(up).unwrap(), EnetOpenedAudio { seq: 9, flags: 1, packet: vec![0xf8, 0xff, 0xfe], missing: 0 });
    let caption = r#"{"t":"call_event","event":{"t":"said","text":"hello"}}"#;
    assert_eq!(caller.open_control(host.seal_control(caption.into()).unwrap()).unwrap().trim_end(), caption);
    assert_eq!(reason(caller.seal_audio(10, 0, vec![0; 121])), "call seal: too_large");
    assert_eq!(host.open_audio(caller.seal_audio(10, 0, vec![]).unwrap()).unwrap().missing, 1, "an empty frame is one missing");
}

#[test]
fn a_host_hello_with_a_flipped_tag_bit_is_refused_for_good() {
    let (caller, _host, mut hello) = call();
    hello[69] ^= 1;
    assert_eq!(reason(caller.finish(finish(hello.clone()))), "call seal: tag");
    hello[69] ^= 1;
    assert_eq!(reason(caller.finish(finish(hello))), "call seal: finished", "a failed finish used the handshake up");
    let (caller, _host, hello) = call();
    assert_eq!(reason(caller.finish(EnetCallFinish { root_id: Some("11111111-1111-4111-8111-111111111112".into()), ..finish(hello) })), "call seal: root");
}

/// Measured for PR 3 (`--nocapture`): one frame sealed and opened through the
/// facade's objects. A phone adds its own FFI hop per call.
#[test]
fn ns_per_frame_through_the_facade() {
    let (caller, host, hello) = call();
    let caller = caller.finish(finish(hello)).unwrap();
    let packet = vec![0x5a; 60];
    let started = std::time::Instant::now();
    const FRAMES: u32 = 20_000;
    for seq in 0..FRAMES {
        let frame = caller.seal_audio(seq as u16, 0, packet.clone()).unwrap();
        assert_eq!(host.open_audio(frame).unwrap().missing, 0);
    }
    println!("call seal: {} ns per frame (seal + open, 147 bytes)", started.elapsed().as_nanos() / u128::from(FRAMES));
}
