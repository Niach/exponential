//! NEW-57 live check for per-app desktop audio: prints the capability, the
//! permission and the apps playing now, then captures `Selection::All` for
//! 10 s (play something meanwhile, e.g. `say …`) and prints per-channel
//! sample counts and RMS.
//!
//! `cargo run -p talk --example app_audio`

use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use etalk::apps::{self, Selection};

fn main() {
    println!("capability: {:?}", apps::capability());
    println!("permission: {:?}", apps::permission());
    println!("live: {:?}", apps::live());
    let capture = match apps::open(Selection::All) {
        Ok(capture) => capture,
        Err(error) => {
            println!("open failed: {error}");
            return;
        }
    };
    let mut channels: BTreeMap<String, (String, usize, f64)> = BTreeMap::new();
    let start = Instant::now();
    let mut last_report = Instant::now();
    while start.elapsed() < Duration::from_secs(10) {
        if let Ok(chunk) = capture.recv_timeout(Duration::from_millis(200)) {
            let entry = channels.entry(chunk.channel.clone()).or_insert((chunk.label.clone(), 0, 0.0));
            entry.1 += chunk.samples.len();
            entry.2 += chunk.samples.iter().map(|s| f64::from(*s) * f64::from(*s)).sum::<f64>();
        }
        if last_report.elapsed() >= Duration::from_secs(2) {
            last_report = Instant::now();
            println!("t={:.0}s live: {:?}", start.elapsed().as_secs_f64(), apps::live());
        }
    }
    drop(capture);
    if channels.is_empty() {
        println!("no chunks");
    }
    for (id, (label, count, sum)) in channels {
        let rms = if count > 0 { (sum / count as f64).sqrt() } else { 0.0 };
        println!(
            "channel {id} ({label}): {count} samples = {:.2} s @16 kHz, rms {rms:.4}",
            count as f64 / 16_000.0
        );
    }
    println!("permission after: {:?}", apps::permission());
}
