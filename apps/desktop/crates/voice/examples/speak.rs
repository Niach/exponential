//! NEW-53: the worker end to end, outside the IDE.
//!
//!   cargo run -p voice --example speak -- <data-dir> <en|de> "Text to say." [voice.wav "its transcript"]
//!
//! Re-runs itself as the speech worker (the IDE's pattern), downloads and
//! verifies the voice on first use, speaks the text and prints every event
//! with its time since the request.

use std::sync::mpsc;
use std::time::Instant;

fn main() {
    if talk::requested() {
        voice::run_and_exit();
    }
    let mut args = std::env::args().skip(1);
    let data_dir = args.next().expect("usage: speak <data-dir> <en|de> <text> [voice.wav transcript]");
    let language = args.next().unwrap_or_else(|| "en".into());
    let text = args.next().unwrap_or_else(|| "Exponential voice check.".into());
    let voice = talk::VoiceChoice { language, file: args.next().unwrap_or_default(), transcript: args.next().unwrap_or_default() };
    let (tx, rx) = mpsc::channel();
    let client = talk::SpeechClient::new(std::env::current_exe().unwrap(), data_dir.into(), move |event| {
        let _ = tx.send(event);
    });
    let started = Instant::now();
    client.prepare(voice);
    client.speak("example", text);
    let mut last_progress = 0u64;
    while let Ok(event) = rx.recv() {
        if let talk::Event::Downloading { received, total } = &event {
            let percent = if *total > 0 { received * 100 / total } else { 0 };
            if percent < last_progress + 10 && percent != 100 {
                continue;
            }
            last_progress = percent;
        }
        println!("{:>7} ms  {event:?}", started.elapsed().as_millis());
        if matches!(event, talk::Event::Finished { .. } | talk::Event::Failed { .. } | talk::Event::Exited) {
            break;
        }
    }
    client.shutdown();
}
