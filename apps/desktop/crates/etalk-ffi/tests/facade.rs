//! #EXP-1441: voice-call.json replayed through the facade's types (what the
//! Swift and Kotlin suites replay through the bindings), plus the codec.

use etalk_ffi::*;
use serde_json::Value;

fn fixture() -> Value {
    serde_json::from_str(&call_spec_json()).unwrap()
}

fn platform(value: &Value) -> TalkCallPlatform {
    match value.as_str().unwrap() {
        "desktop" => TalkCallPlatform::Desktop,
        "phone" => TalkCallPlatform::Phone,
        _ => TalkCallPlatform::Web,
    }
}

fn speech(value: &str) -> TalkCallSpeech {
    match value {
        "ok" => TalkCallSpeech::Ok,
        "noGpu" => TalkCallSpeech::NoGpu,
        "failed" => TalkCallSpeech::Failed,
        "preparing" => TalkCallSpeech::Preparing,
        other => panic!("unknown speech {other}"),
    }
}

fn button(value: &Value) -> TalkCallButton {
    match value.as_str().unwrap() {
        "mute" => TalkCallButton::Mute,
        "speaker" => TalkCallButton::Speaker,
        "end" => TalkCallButton::End,
        other => panic!("unknown button {other}"),
    }
}

#[test]
fn availability_and_speech_match_the_fixture() {
    let fixture = fixture();
    for case in fixture["availabilityCases"].as_array().unwrap() {
        let input = &case["input"];
        let reason = call_unavailable(TalkCallAvailabilityInput {
            agent: input["agent"].as_bool().unwrap(),
            platform: platform(&input["platform"]),
            device: input["device"].as_str().map(str::to_string),
            online: input["online"].as_bool().unwrap(),
            here: input["here"].as_bool().unwrap(),
            speech: input["speech"].as_str().map(speech),
            busy: input["busy"].as_bool().unwrap(),
            meeting: input["meeting"].as_bool().unwrap_or(false),
        });
        assert_eq!(reason.as_deref(), case["expected"]["reason"].as_str(), "{}", case["name"]);
        assert_eq!(reason.is_none(), case["expected"]["enabled"].as_bool().unwrap(), "{}", case["name"]);
    }
    for case in fixture["speechCases"].as_array().unwrap() {
        let state = case["state"].as_str().map(str::to_string);
        assert_eq!(call_speech_of(state), case["expected"].as_str().map(speech), "{}", case["name"]);
    }
}

#[test]
fn the_flow_matches_the_fixture_through_the_wire_form() {
    let cases = fixture()["flowCases"].as_array().unwrap().clone();
    assert!(cases.len() >= 41);
    for case in &cases {
        let name = &case["name"];
        let mut state = call_idle();
        for value in case["events"].as_array().unwrap() {
            let event = call_event_of_wire(value.to_string()).unwrap_or_else(|| panic!("{name}: {value}"));
            state = call_step(state, event);
        }
        let expected = &case["view"];
        match call_view(state.clone(), platform(&case["platform"])) {
            None => assert!(expected.is_null(), "{name}: hidden, expected {expected}"),
            Some(view) => {
                assert_eq!(view.activity, expected["activity"].as_str().unwrap(), "{name}");
                assert_eq!(view.status.as_deref(), expected["status"].as_str(), "{name}");
                assert_eq!(view.caption, expected["caption"].as_str().unwrap(), "{name}");
                assert_eq!(view.muted, expected["muted"].as_bool().unwrap(), "{name}");
                assert_eq!(view.speaker, expected["speaker"].as_bool().unwrap(), "{name}");
                let buttons: Vec<TalkCallButton> = expected["buttons"].as_array().unwrap().iter().map(button).collect();
                assert_eq!(view.buttons, buttons, "{name}");
                assert_eq!(view.end, expected["end"].as_str().unwrap(), "{name}");
            }
        }
        assert_eq!(state.toast.as_deref(), case["toast"].as_str(), "{name}");
    }
}

#[test]
fn the_ring_and_timer_match_the_fixture() {
    let fixture = fixture();
    for case in fixture["ringCases"].as_array().unwrap() {
        assert_eq!(call_ring_ms(case["random"].as_f64().unwrap()), case["ringMs"].as_u64().unwrap(), "{case}");
    }
    for case in fixture["answerCases"].as_array().unwrap() {
        assert_eq!(call_answer_ms(case["random"].as_f64().unwrap(), case["readyMs"].as_u64()), case["answeredMs"].as_u64(), "{case}");
    }
    for case in fixture["ringToneCases"].as_array().unwrap() {
        assert_eq!(call_ring_tone_on(case["ms"].as_i64().unwrap()), case["on"].as_bool().unwrap(), "{case}");
    }
    for case in fixture["timerCases"].as_array().unwrap() {
        assert_eq!(call_timer(case["ms"].as_u64().unwrap()), case["text"].as_str().unwrap(), "{case}");
    }
    for _ in 0..50 {
        assert!((0.0..1.0).contains(&call_ring_random()));
    }
    let constants = call_constants();
    assert_eq!(constants.min_ring_ms, fixture["constants"]["minRingMs"].as_u64().unwrap());
    assert_eq!(constants.kind_down, 2);
    assert_eq!(call_greeting(Some("de".into())), "Hallo?");
    assert_eq!(call_greeting(None), "Hello?");
}

#[test]
fn a_host_event_reaches_the_caller_as_the_same_event() {
    let wire = call_wire_event(TalkCallEvent::Queued { count: 2, text: "x".into() }).unwrap();
    assert_eq!(call_event_of_wire(wire), Some(TalkCallEvent::Queued { count: 2, text: "x".into() }));
    assert_eq!(call_wire_event(TalkCallEvent::Mute), None);
    assert_eq!(call_event_of_wire("not json".into()), None);
    assert_eq!(call_event_of_wire(r#"{"t":"nope"}"#.into()), None);
}

/// One 20 ms frame of a 440 Hz tone at `rate`.
fn tone(rate: u32, frame: usize, offset: usize) -> Vec<i16> {
    (0..frame).map(|i| ((((offset + i) as f32) * 440.0 * std::f32::consts::TAU / rate as f32).sin() * 8000.0) as i16).collect()
}

#[test]
fn the_codec_round_trips_a_call() {
    let c = call_constants();
    let encoder = TalkEncoder::new(c.up_rate, (c.up_kbps * 1000) as i32, Some(10)).unwrap();
    let frame = encoder.frame_samples() as usize;
    assert_eq!(frame, (c.up_rate * c.frame_ms / 1000) as usize);
    let decoder = TalkDecoder::new(c.up_rate).unwrap();
    let uplink = TalkUplink::new(c.up_rate).unwrap();
    let mut energy = 0f64;
    for n in 0..25u16 {
        let packet = encoder.encode(tone(c.up_rate, frame, n as usize * frame)).unwrap();
        assert!(!packet.is_empty() && packet.len() <= c.max_frame_bytes as usize);
        let pcm = decoder.decode(packet.clone()).unwrap();
        assert_eq!(pcm.len(), frame);
        energy += pcm.iter().map(|s| (*s as f64).powi(2)).sum::<f64>();
        // Packet 10 lost: the uplink recovers it from 11's FEC (two frames out).
        if n != 10 {
            let out = uplink.push(n, packet);
            assert_eq!(out.len(), if n == 11 { 2 * frame } else { frame }, "packet {n}");
        }
    }
    assert!(energy > 0.0, "the tone decodes to sound");
    assert_eq!(decoder.decode(Vec::new()).unwrap().len(), frame, "an empty packet conceals");
    assert!(matches!(encoder.encode(vec![0; 7]), Err(TalkError::Invalid { .. })));
    assert!(matches!(TalkEncoder::new(44_100, 24_000, None), Err(TalkError::Invalid { .. })));
    assert!(TalkDecoder::new(c.down_rate).is_ok());
    assert_eq!(codec_backend(), "opus-rs");
}
