//! NEW-58 live check of the macOS capturer: opens it, waits 2 s, grabs every
//! display, prints each frame and saves display 1 as a PNG. `--bench <secs>`
//! then grabs all displays once a second for that long (CPU measurement);
//! `--force` also lends display 1 every tick (a busy screen); `--fresh` also
//! takes one FRESH screenshot of display 1 every tick (its cost after a
//! resume). Prints the process's CPU share.
//!
//! cargo run -p epilot-screen --example capture_mac [-- --bench 20]

#[cfg(target_os = "macos")]
#[allow(dead_code)]
#[path = "../src/capture/mod.rs"]
mod capture;

/// The capture module's one outside type (a frame's masks).
#[cfg(target_os = "macos")]
#[allow(dead_code)]
mod store {
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub struct Rect {
        pub x: i32,
        pub y: i32,
        pub width: u32,
        pub height: u32,
    }
}

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
    let all: Vec<capture::Want> = (1..=8).map(|display| capture::Want { display, since: None }).collect();
    let frames = capturer.grab_all(&all).expect("grab_all");
    println!("grab_all: {} frame(s)", frames.len());
    for frame in &frames {
        println!(
            "display {}: {}x{} ({} bytes, stride {}, bgra {})",
            frame.display,
            frame.width,
            frame.height,
            frame.bytes().len(),
            frame.stride,
            frame.bgra
        );
    }
    if let Some(frame) = frames.first() {
        let path = out.join("mac-capture.png");
        let rgba: Vec<u8> = frame
            .bytes()
            .chunks(frame.stride)
            .take(frame.height as usize)
            .flat_map(|row| row[..frame.width as usize * 4].chunks_exact(4).flat_map(|px| [px[2], px[1], px[0], 255]))
            .collect();
        image::save_buffer(&path, &rgba, frame.width, frame.height, image::ColorType::Rgba8)
            .expect("png");
        println!("saved {}", path.display());
    }
    let again = capturer.grab_all(&all).expect("grab_all");
    println!("immediate second grab_all: {} fresh frame(s)", again.len());
    match capturer.grab(capture::Want { display: 1, since: None }).expect("grab") {
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
        let cpu = || {
            let mut usage: libc::rusage = unsafe { std::mem::zeroed() };
            unsafe { libc::getrusage(libc::RUSAGE_SELF, &mut usage) };
            let seconds = |t: libc::timeval| t.tv_sec as f64 + t.tv_usec as f64 / 1e6;
            seconds(usage.ru_utime) + seconds(usage.ru_stime)
        };
        let cpu_before = cpu();
        let begin = Instant::now();
        let (mut ticks, mut got) = (0, 0);
        while begin.elapsed() < Duration::from_secs(secs) {
            let tick = Instant::now();
            got += capturer.grab_all(&all).expect("grab_all").len();
            // `--force`: also lend display 1's latest every tick, the
            // cost of a screen that changes every second.
            if args.iter().any(|a| a == "--force") {
                got += capturer.grab(capture::Want { display: 1, since: None }).expect("grab").is_some() as usize;
            }
            if args.iter().any(|a| a == "--fresh") {
                got += capturer.fresh(1).expect("fresh").is_some() as usize;
            }
            ticks += 1;
            std::thread::sleep(Duration::from_secs(1).saturating_sub(tick.elapsed()));
        }
        let wall = begin.elapsed();
        println!("bench: {ticks} ticks, {got} frames in {wall:?}, {:.2} % of a core", (cpu() - cpu_before) / wall.as_secs_f64() * 100.);
    }
}

#[cfg(not(target_os = "macos"))]
fn main() {}
