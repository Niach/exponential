//! NEW-129 live check (not committed): 16 kHz mono f32le PCM on stdin (a
//! `pw-record` of the virtual mic) through the shipped detector settings.
//!   wake_listen <classifier.onnx> <threshold>
#[path = "../tests/common/mod.rs"]
mod common;

use std::io::{Read, Write};
use std::time::{SystemTime, UNIX_EPOCH};

use etalk_wakeword::models::{DEBOUNCE, PATIENCE};
use etalk_wakeword::{Classifier, Detector, Trigger};

fn main() {
    let mut args = std::env::args().skip(1);
    let classifier = Classifier::load(args.next().expect("classifier").as_ref()).expect("classifier");
    let threshold: f32 = args.next().expect("threshold").parse().expect("threshold");
    let mut detector = Detector::new(common::front_end(), classifier, Trigger::new(threshold, PATIENCE, DEBOUNCE), 1.0);
    let (mut stdin, mut out) = (std::io::stdin().lock(), std::io::stdout().lock());
    writeln!(out, "{} READY", now()).unwrap();
    out.flush().unwrap();
    let mut bytes = [0u8; 1280];
    while stdin.read_exact(&mut bytes).is_ok() {
        let samples: Vec<f32> = bytes.chunks_exact(4).map(|b| f32::from_le_bytes(b.try_into().unwrap())).collect();
        for score in detector.push(&samples).unwrap() {
            if score.fired {
                writeln!(out, "{} FIRE {:.3} {}", now(), score.score, score.end / 1280).unwrap();
            } else if score.score >= 0.02 {
                writeln!(out, "{} score {:.3} {}", now(), score.score, score.end / 1280).unwrap();
            }
            out.flush().unwrap();
        }
    }
}

fn now() -> u128 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis()
}
