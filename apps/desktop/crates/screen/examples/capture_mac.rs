//! NEW-58 live check of the macOS capturer: opens it, waits 2 s, grabs every
//! display, prints each frame and saves display 1 as a PNG. `--bench <secs>`
//! then grabs all displays once a second for that long (CPU measurement);
//! `--force` also converts display 1 every tick (a busy screen).
//!
//! cargo run -p screen --example capture_mac [-- --bench 20]

#[cfg(target_os = "macos")]
#[allow(dead_code)]
#[path = "../src/capture/mod.rs"]
mod capture;

#[cfg(target_os = "macos")]
fn main() {
    use std::time::{Duration, Instant};

    let out = std::path::Path::new("/tmp/new58-shots");
    std::fs::create_dir_all(out).expect("mkdir");
    let started = Instant::now();
    let mut capturer = match capture::open(out) {
        Ok(capturer) => capturer,
        Err(err) => {
            eprintln!("open failed: {err}");
            std::process::exit(1);
        }
    };
    println!("opened in {:?}", started.elapsed());
    std::thread::sleep(Duration::from_secs(2));
    let frames = capturer.grab_all().expect("grab_all");
    println!("grab_all: {} frame(s)", frames.len());
    for frame in &frames {
        println!(
            "display {}: {}x{} ({} bytes)",
            frame.display,
            frame.width,
            frame.height,
            frame.rgba.len()
        );
    }
    if let Some(frame) = frames.first() {
        let path = out.join("mac-capture.png");
        image::save_buffer(&path, &frame.rgba, frame.width, frame.height, image::ColorType::Rgba8)
            .expect("png");
        println!("saved {}", path.display());
    }
    let again = capturer.grab_all().expect("grab_all");
    println!("immediate second grab_all: {} fresh frame(s)", again.len());
    match capturer.grab(1).expect("grab") {
        Some(frame) => println!("grab(1): {}x{}", frame.width, frame.height),
        None => println!("grab(1): none"),
    }

    let args: Vec<String> = std::env::args().collect();
    if let Some(secs) = args
        .iter()
        .position(|a| a == "--bench")
        .and_then(|i| args.get(i + 1))
        .and_then(|s| s.parse::<u64>().ok())
    {
        let begin = Instant::now();
        let (mut ticks, mut got) = (0, 0);
        while begin.elapsed() < Duration::from_secs(secs) {
            let tick = Instant::now();
            got += capturer.grab_all().expect("grab_all").len();
            // `--force`: also convert display 1's latest every tick, the
            // cost of a screen that changes every second.
            if args.iter().any(|a| a == "--force") {
                got += capturer.grab(1).expect("grab").is_some() as usize;
            }
            ticks += 1;
            std::thread::sleep(Duration::from_secs(1).saturating_sub(tick.elapsed()));
        }
        println!("bench: {ticks} ticks, {got} frames in {:?}", begin.elapsed());
    }
}

#[cfg(not(target_os = "macos"))]
fn main() {}
