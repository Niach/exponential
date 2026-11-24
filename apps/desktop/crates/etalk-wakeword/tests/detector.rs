//! The detector end to end on livekit-wakeword's own model and recordings.

mod common;

use etalk_wakeword::{Detector, Trigger, FRAME_SAMPLES, RATE};

fn detector() -> Detector {
    Detector::new(common::front_end(), common::hey_livekit(), Trigger::new(0.5, 1, 25), 1.0)
}

fn clip(name: &str) -> Vec<f32> {
    let (samples, rate) = common::wav(&common::fixtures().join("tests/fixtures").join(name));
    assert_eq!(rate as usize, RATE);
    common::padded(&samples, 1.0)
}

fn run(samples: &[f32], chunk: usize) -> (f32, usize) {
    let mut detector = detector();
    let mut peak = 0.0f32;
    let mut fired = 0;
    for part in samples.chunks(chunk) {
        for score in detector.push(part).unwrap() {
            peak = peak.max(score.score);
            fired += usize::from(score.fired);
        }
    }
    (peak, fired)
}

#[test]
fn the_phrase_fires_once() {
    let (peak, fired) = run(&clip("positive.wav"), 320);
    assert!(peak >= 0.5, "peak {peak}");
    assert_eq!(fired, 1, "one detection (the debounce holds the rest)");
}

/// livekit-wakeword's own check: the negative clip's window (its last
/// frame, as `WakeWordModel::predict` scores a 2 s chunk) stays under the
/// threshold, as the positive one's reaches it.
#[test]
fn the_clips_score_like_livekit_wakeword() {
    let last = |name: &str| {
        let (samples, _) = common::wav(&common::fixtures().join("tests/fixtures").join(name));
        detector().push(&samples).unwrap().last().unwrap().score
    };
    assert!(last("positive.wav") >= 0.5);
    assert!(last("negative.wav") < 0.5);
}

#[test]
fn silence_noise_and_a_tone_never_fire() {
    let noise: Vec<f32> = (0..3 * RATE as u32).map(|i| ((i.wrapping_mul(2_654_435_761) >> 16) as f32 / 65_536.0 - 0.5) * 0.01).collect();
    let tone: Vec<f32> = (0..3 * RATE).map(|i| 0.3 * (std::f32::consts::TAU * 440.0 * i as f32 / RATE as f32).sin()).collect();
    for (name, audio) in [("silence", vec![0.0; 3 * RATE]), ("noise", noise), ("tone", tone)] {
        let (peak, fired) = run(&audio, 512);
        assert!(peak < 0.5 && fired == 0, "{name}: {peak}");
    }
}

#[test]
fn chunking_never_changes_a_score() {
    let audio = clip("positive.wav");
    let scores = |chunk: usize| {
        let mut detector = detector();
        audio.chunks(chunk).flat_map(|part| detector.push(part).unwrap()).collect::<Vec<_>>()
    };
    let whole = scores(FRAME_SAMPLES);
    assert_eq!(whole.len(), audio.len() / FRAME_SAMPLES);
    for chunk in [160, 512, 4000] {
        assert_eq!(scores(chunk), whole, "chunk {chunk}");
    }
}

#[test]
fn a_detection_says_where_the_phrase_ended() {
    let audio = clip("positive.wav");
    let mut detector = detector();
    let end = audio.chunks(512).find_map(|part| detector.detect(part).unwrap()).expect("detected");
    // The phrase sits inside the clip, after the second of padding.
    assert!(end > RATE && end <= audio.len() - RATE / 2, "{end}");
    assert_eq!(end % FRAME_SAMPLES, 0);
}

/// #NEW-126: frames at rest cost no model, and the frames scored after them
/// score exactly as in a stream scored whole, however long the rest (shorter
/// than the classifier's window, longer than what is kept).
#[test]
fn a_rest_never_changes_the_scores_after_it() {
    let phrase = clip("positive.wav");
    let noise = |seconds: f32| -> Vec<f32> {
        (0..(seconds * RATE as f32) as u32).map(|i| ((i.wrapping_mul(2_654_435_761) >> 16) as f32 / 65_536.0 - 0.5) * 0.01).collect()
    };
    for seconds in [0.3, 1.0, 1.9, 2.5, 7.3] {
        let quiet = noise(seconds);
        let audio = [quiet.as_slice(), phrase.as_slice()].concat();
        let mut whole = detector();
        let expected: Vec<_> = audio.chunks(512).flat_map(|part| whole.push(part).unwrap()).collect();
        let mut rested = detector();
        quiet.chunks(512).for_each(|part| rested.rest(part));
        assert_eq!(rested.samples(), quiet.len());
        let scored: Vec<_> = phrase.chunks(512).flat_map(|part| rested.push(part).unwrap()).collect();
        assert!(!scored.is_empty() && scored.iter().any(|score| score.fired), "{seconds} s: the phrase fires");
        assert_eq!(scored, expected[expected.len() - scored.len()..], "{seconds} s of rest");
        assert_eq!(scored[0].end, (quiet.len() / FRAME_SAMPLES + 1) * FRAME_SAMPLES, "{seconds} s: the frames at rest count");
    }
}
