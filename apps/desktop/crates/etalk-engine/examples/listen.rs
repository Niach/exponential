//! NEW-78: the transcriber on WAV files instead of the microphone.
//!
//!   cargo run -p voice --example listen -- <data-dir> <en|de> [--cpu] <clip.wav>...
//!
//! Downloads and verifies the runtime and the models on first use, loads
//! Parakeet (on the GPU unless `--cpu` or none has room) and runs every clip
//! through the worker's own pipeline (`etalk_engine::listen::Utterance`), fed in
//! 20 ms chunks as fast as it decodes. Prints one JSON line per clip:
//! the transcript, how many partials it produced, the compute spent while
//! the clip was "held" and the tail after "release" (what the person waits
//! for). `docs/talk-spike.md` has the numbers.

use std::time::Instant;

use etalk_engine::listen::{Utterance, RATE};
use etalk_engine::listener::Engines;

fn main() {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let cpu = args.iter().any(|arg| arg == "--cpu");
    args.retain(|arg| arg != "--cpu");
    let [data_dir, language, clips @ ..] = args.as_slice() else {
        eprintln!("usage: listen <data-dir> <en|de> [--cpu] <clip.wav>...");
        std::process::exit(2);
    };
    let data_dir = std::path::PathBuf::from(data_dir);
    let mut last = 0;
    Engines::download(&data_dir, |received, total| {
        let percent = if total > 0 { received * 100 / total } else { 100 };
        if percent >= last + 10 || percent == 100 && last != 100 {
            last = percent;
            eprintln!("download {percent}% of {} MB", total / 1_000_000);
        }
    })
    .expect("download");
    let started = Instant::now();
    let mut engines = Engines::load(&data_dir, cpu).expect("load");
    let load_ms = started.elapsed().as_millis();
    for clip in clips {
        let bytes = std::fs::read(clip).expect("read the clip");
        let samples = etalk_engine::capture::read_wav(&bytes).expect("a WAV");
        let mut vad = engines.vad().expect("vad");
        let mut utterance = Utterance::new("clip", language.as_str());
        let mut partials = Vec::new();
        let mut heard = String::new();
        let held = Instant::now();
        for chunk in samples.chunks(RATE / 50) {
            utterance
                .feed(chunk, &mut vad, &mut engines, &mut |event| {
                    if let etalk::Event::Partial { text, .. } = event {
                        partials.push(text);
                    }
                })
                .expect("feed");
        }
        let held_ms = held.elapsed().as_millis();
        let released = Instant::now();
        utterance
            .finish(&mut engines, &mut |event| match event {
                etalk::Event::Heard { text, .. } => heard = text,
                etalk::Event::Empty { .. } => heard.clear(),
                _ => {}
            })
            .expect("finish");
        let tail_ms = released.elapsed().as_millis();
        println!(
            "{}",
            serde_json::json!({
                "clip": clip,
                "device": format!("{:?}", engines.device()),
                "audioMs": samples.len() * 1000 / RATE,
                "loadMs": load_ms,
                "heldComputeMs": held_ms,
                "tailMs": tail_ms,
                "partials": partials.len(),
                "firstPartial": partials.first(),
                "text": heard,
            })
        );
    }
}
