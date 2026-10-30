//! EXP-1424: what the model detector costs: CPU time per second of audio
//! (= the share of one core it takes while listening), on livekit-wakeword's
//! `hey_livekit` classifier (the same front end every model runs; a
//! classifier of another size changes only its last step).
//!
//!   cargo run --release -p etalk-wakeword --example wake_model [-- <seconds> [<classifier.onnx>]]
//!
//! Streams `<seconds>` (default 60) of speech-like audio (the positive and
//! negative clips, a tone and noise, looped) in 20 ms captures, as the
//! listener does.

#[path = "../tests/common/mod.rs"]
mod common;

use std::time::{Duration, Instant};

use etalk_wakeword::{Classifier, Detector, Trigger, RATE};

/// The process's user CPU time so far.
fn cpu() -> Duration {
    let mut usage = std::mem::MaybeUninit::<libc::rusage>::zeroed();
    // SAFETY: getrusage fills the struct it is given.
    let usage = unsafe {
        libc::getrusage(libc::RUSAGE_SELF, usage.as_mut_ptr());
        usage.assume_init()
    };
    Duration::from_secs(usage.ru_utime.tv_sec as u64) + Duration::from_micros(usage.ru_utime.tv_usec as u64)
}

fn main() {
    let mut args = std::env::args().skip(1);
    let seconds: usize = args.next().and_then(|arg| arg.parse().ok()).unwrap_or(60);
    let classifier = match args.next() {
        Some(path) => Classifier::load(path.as_ref()).expect("classifier"),
        None => common::hey_livekit(),
    };
    let loaded = Instant::now();
    let front = common::front_end();
    eprintln!("models loaded in {} ms", loaded.elapsed().as_millis());

    let fixtures = common::fixtures().join("tests/fixtures");
    let (positive, _) = common::wav(&fixtures.join("positive.wav"));
    let (negative, _) = common::wav(&fixtures.join("negative.wav"));
    let tone: Vec<f32> = (0..RATE).map(|i| 0.2 * (std::f32::consts::TAU * 220.0 * i as f32 / RATE as f32).sin()).collect();
    let noise: Vec<f32> = (0..RATE as u32).map(|i| ((i.wrapping_mul(2_654_435_761) >> 16) as f32 / 65_536.0 - 0.5) * 0.01).collect();
    let pattern = [positive, negative, tone, noise].concat();
    let audio: Vec<f32> = pattern.iter().copied().cycle().take(seconds * RATE).collect();

    let mut detector = Detector::new(front, classifier, Trigger::new(0.5, 2, 25), 1.0);
    let (wall, user) = (Instant::now(), cpu());
    let mut detections = 0;
    let mut slowest = Duration::ZERO;
    for capture in audio.chunks(RATE / 50) {
        let step = Instant::now();
        detections += detector.push(capture).unwrap().iter().filter(|score| score.fired).count();
        slowest = slowest.max(step.elapsed());
    }
    let (wall, user) = (wall.elapsed(), cpu() - user);
    println!(
        "{seconds} s of audio: {:.1} ms CPU per second of audio ({:.2} % of one core), wall {:.2} s, slowest 20 ms capture {:.2} ms, {detections} detections",
        user.as_secs_f64() * 1000.0 / seconds as f64,
        user.as_secs_f64() * 100.0 / seconds as f64,
        wall.as_secs_f64(),
        slowest.as_secs_f64() * 1000.0,
    );
}
