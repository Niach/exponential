//! End to end: a fake capturer into a running [`engine::Memory`], read back
//! through the `screen` endpoint's `get_screenshots`.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde_json::{json, Value};

use crate::capture::{Capturer, Frame};
use crate::engine::{self, Memory, Timing};
use crate::server::{self, Hub};
use crate::store::tests::TempDir;
use crate::{store, tool, Status};

const SIDE: u32 = 320;

/// Every grab a new 8 x 8 black/white block pattern (a fresh hash), per
/// display; counts its calls.
struct FakeCapturer {
    seed: u64,
    displays: u32,
    grabs: Arc<Mutex<Vec<&'static str>>>,
}

fn splitmix(seed: &mut u64) -> u64 {
    *seed = seed.wrapping_add(0x9e37_79b9_7f4a_7c15);
    let mut z = *seed;
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    z ^ (z >> 31)
}

impl FakeCapturer {
    fn frame(&mut self, display: u32) -> Frame {
        let bits = splitmix(&mut self.seed);
        let cell = SIDE / 8;
        let mut rgba = Vec::with_capacity((SIDE * SIDE * 4) as usize);
        for y in 0..SIDE {
            for x in 0..SIDE {
                let k = (y / cell) * 8 + x / cell;
                let v = if bits >> (63 - k) & 1 == 1 { 255 } else { 0 };
                rgba.extend([v, v, v, 255]);
            }
        }
        Frame { display, width: SIDE, height: SIDE, rgba }
    }
}

impl Capturer for FakeCapturer {
    fn grab_all(&mut self) -> Result<Vec<Frame>, String> {
        self.grabs.lock().unwrap().push("all");
        Ok((1..=self.displays).map(|display| self.frame(display)).collect())
    }

    fn grab(&mut self, display: u32) -> Result<Option<Frame>, String> {
        self.grabs.lock().unwrap().push("one");
        Ok(Some(self.frame(display)))
    }
}

fn window(app: &str, title: &str) -> computer::FocusedWindow {
    computer::FocusedWindow {
        app: app.into(),
        title: title.into(),
        display: 1,
        rect: computer::ScreenRect { x: 10, y: 20, width: 240, height: 210 },
    }
}

struct Rig {
    memory: Memory,
    focus: Arc<Mutex<Option<computer::FocusedWindow>>>,
    hotkey: Arc<AtomicBool>,
    statuses: Arc<Mutex<Vec<Status>>>,
    grabs: Arc<Mutex<Vec<&'static str>>>,
}

fn rig(dir: &TempDir, interval_ms: u64) -> Rig {
    let focus = Arc::new(Mutex::new(Some(window("Zed", "main.rs"))));
    let hotkey = Arc::new(AtomicBool::new(false));
    let statuses = Arc::new(Mutex::new(Vec::new()));
    let grabs = Arc::new(Mutex::new(Vec::new()));
    let source = focus.clone();
    let heard = statuses.clone();
    let memory = Memory::start(
        &dir.0,
        Box::new(FakeCapturer { seed: 7, displays: 2, grabs: grabs.clone() }),
        Box::new(move || source.lock().unwrap().clone()),
        Timing { interval: Duration::from_millis(interval_ms), poll: Duration::from_millis(10) },
        hotkey.clone(),
        Arc::new(move |status| heard.lock().unwrap().push(status)),
    )
    .unwrap();
    Rig { memory, focus, hotkey, statuses, grabs }
}

fn count(memory: &Memory) -> usize {
    memory.store.lock().unwrap().frames(i64::MIN, i64::MAX).unwrap().len()
}

#[test]
fn frames_flow_from_the_capturer_to_get_screenshots_within_the_cap() {
    let dir = TempDir::new();
    let rig = rig(&dir, 40);
    wait_until(|| count(&rig.memory) >= 6);
    *rig.focus.lock().unwrap() = Some(window("Zed", "lib.rs"));
    wait_until(|| count(&rig.memory) >= 10);
    assert_eq!(rig.statuses.lock().unwrap().as_slice(), [Status::Active]);

    // Through the endpoint, as a run sees it.
    let store = rig.memory.store.clone();
    let hub = Arc::new(Hub::new(Arc::new(move |name: &str, arguments: &Value| {
        tool::call(Some(&store), name, arguments, engine::now_ms())
    })));
    let token = hub.grant("voice-run");
    let url = server::serve(hub).unwrap();
    let (status, body) = server::tests::post(
        &url,
        &token,
        &json!({ "jsonrpc": "2.0", "id": 3, "method": "tools/call", "params": { "name": "get_screenshots", "arguments": {} } }),
    );
    assert!(status.starts_with("HTTP/1.1 200"), "{status}");
    let reply: Value = serde_json::from_str(&body).unwrap();
    let content = reply["result"]["content"].as_array().unwrap();
    let images: Vec<&Value> = content.iter().filter(|item| item["type"] == "image").collect();
    assert_eq!(images.len(), 4, "{content:?}");
    assert_eq!(content.len(), 8);
    for pair in content.chunks(2) {
        let line = pair[0]["text"].as_str().unwrap();
        assert!(line.contains(" · Zed — "), "{line}");
        assert!(line.ends_with("main.rs") || line.ends_with("lib.rs"), "{line}");
        assert_eq!(pair[1]["mimeType"], "image/png");
    }
    // A filter that matches nothing reads `copy.none`.
    let (_, body) = server::tests::post(
        &url,
        &token,
        &json!({ "jsonrpc": "2.0", "id": 4, "method": "tools/call", "params": { "name": "get_screenshots", "arguments": { "filter": "safari" } } }),
    );
    let reply: Value = serde_json::from_str(&body).unwrap();
    assert!(reply["result"]["content"][0]["text"].as_str().unwrap().starts_with("No screenshots between "));

    rig.memory.stop(true);
    assert!(!dir.0.join(store::FRAMES_DIR).exists());
    assert!(!dir.0.join(store::INDEX_FILE).exists());
}

#[test]
fn a_title_change_grabs_its_display_at_once_between_ticks() {
    let dir = TempDir::new();
    let rig = rig(&dir, 60_000);
    wait_until(|| count(&rig.memory) == 2);
    assert_eq!(rig.grabs.lock().unwrap().as_slice(), ["all"]);
    *rig.focus.lock().unwrap() = Some(window("Zed", "lib.rs"));
    wait_until(|| count(&rig.memory) == 3);
    assert_eq!(rig.grabs.lock().unwrap().as_slice(), ["all", "one"]);
    let rows = rig.memory.store.lock().unwrap().frames(i64::MIN, i64::MAX).unwrap();
    assert_eq!(rows.len(), 3);
    assert_eq!((rows[2].display, rows[2].title.as_str()), (1, "lib.rs"));
    rig.memory.stop(true);
}

#[test]
fn a_frame_is_cropped_to_its_focused_window_on_its_own_display() {
    let dir = TempDir::new();
    let rig = rig(&dir, 40);
    wait_until(|| count(&rig.memory) >= 2);
    let rows = rig.memory.store.lock().unwrap().frames(i64::MIN, i64::MAX).unwrap();
    let on_one = rows.iter().find(|row| row.display == 1).unwrap();
    let on_two = rows.iter().find(|row| row.display == 2).unwrap();
    assert_eq!(on_one.focus, Some(store::Rect { x: 10, y: 20, width: 240, height: 210 }));
    assert_eq!(on_two.focus, None);
    for (row, size) in [(on_one, (240, 210)), (on_two, (SIDE, SIDE))] {
        let webp = std::fs::read(dir.0.join(&row.path)).unwrap();
        let png = tool::picture(row, &webp).unwrap();
        let image = image::load_from_memory(&png).unwrap();
        assert_eq!((image.width(), image.height()), size);
    }
    // `tool.minCrop`: a focused strip (a tab-hover bar) keeps the whole frame.
    let strip = store::Row { focus: Some(store::Rect { x: 0, y: 0, width: SIDE, height: 34 }), ..on_one.clone() };
    let png = tool::picture(&strip, &std::fs::read(dir.0.join(&on_one.path)).unwrap()).unwrap();
    let image = image::load_from_memory(&png).unwrap();
    assert_eq!((image.width(), image.height()), (SIDE, SIDE));
    rig.memory.stop(false);
    assert!(dir.0.join(store::INDEX_FILE).exists());
}

/// Wait (up to 5 s) until `done`; the loop's pace varies under load.
fn wait_until(done: impl Fn() -> bool) {
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    while !done() {
        assert!(std::time::Instant::now() < deadline, "timed out");
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn a_denylisted_window_or_the_hotkey_pauses_capture_and_the_next_tick_resumes() {
    let dir = TempDir::new();
    let rig = rig(&dir, 40);
    let heard = |n: usize| rig.statuses.lock().unwrap().len() >= n;
    wait_until(|| count(&rig.memory) > 0);
    *rig.focus.lock().unwrap() = Some(window("1Password", "Vault"));
    wait_until(|| heard(2));
    let paused_at = count(&rig.memory);
    std::thread::sleep(Duration::from_millis(200));
    assert_eq!(count(&rig.memory), paused_at, "no frame while a password manager is focused");
    *rig.focus.lock().unwrap() = Some(window("Zed", "main.rs"));
    wait_until(|| count(&rig.memory) > paused_at);
    rig.hotkey.store(true, Ordering::Release);
    wait_until(|| heard(4));
    let held_at = count(&rig.memory);
    std::thread::sleep(Duration::from_millis(200));
    assert_eq!(count(&rig.memory), held_at, "no frame while the hotkey holds it");
    rig.hotkey.store(false, Ordering::Release);
    wait_until(|| heard(5));
    assert_eq!(
        rig.statuses.lock().unwrap().as_slice(),
        [Status::Active, Status::Paused, Status::Active, Status::Paused, Status::Active]
    );
    rig.memory.stop(true);
}

#[test]
fn a_grant_needs_screen_memory_on() {
    assert_eq!(crate::grant("s1").unwrap_err(), "Screen memory is off on this device.");
    let heard = Arc::new(Mutex::new(Vec::new()));
    let sink = heard.clone();
    crate::set_status_observer(move |status| sink.lock().unwrap().push(status));
    assert_eq!(heard.lock().unwrap().as_slice(), [Status::Off]);
}

/// The CPU budget (< 2 % of a core at 1 fps on two displays), by part:
/// `cargo test --release -p screen -- --ignored --nocapture cpu_budget`.
#[test]
#[ignore]
fn cpu_budget() {
    use std::time::Instant;
    // A text-like 5K frame: a white page with dark glyph cells.
    let (width, height) = (5120u32, 2880u32);
    let mut seed = 1u64;
    let mut rgba = vec![255u8; (width * height * 4) as usize];
    for line in 0..(height / 24) {
        for glyph in 0..(width / 12) {
            if splitmix(&mut seed) % 3 == 0 {
                continue;
            }
            for y in line * 24 + 6..line * 24 + 18 {
                for x in glyph * 12 + 2..glyph * 12 + 9 {
                    let at = ((y * width + x) * 4) as usize;
                    rgba[at..at + 3].copy_from_slice(&[30, 30, 40]);
                }
            }
        }
    }
    let frame = Frame { display: 1, width, height, rgba };
    let time = |runs: u32, mut f: Box<dyn FnMut() + '_>| {
        let start = Instant::now();
        for _ in 0..runs {
            f();
        }
        start.elapsed().as_secs_f64() * 1000. / f64::from(runs)
    };
    let hash_ms = time(50, Box::new(|| {
        std::hint::black_box(crate::hash::hash(frame.width, frame.height, &frame.rgba));
    }));
    let mut size = 0;
    let encode_ms = time(5, Box::new(|| size = engine::encode(&frame).len()));
    // A real screenshot when given (`SCREEN_BENCH_PNG=/path/to.png`).
    if let Ok(path) = std::env::var("SCREEN_BENCH_PNG") {
        let shot = image::open(path).unwrap().to_rgba8();
        let real = Frame { display: 1, width: shot.width(), height: shot.height(), rgba: shot.into_raw() };
        let mut bytes = 0;
        let real_hash = time(20, Box::new(|| {
            std::hint::black_box(crate::hash::hash(real.width, real.height, &real.rgba));
        }));
        let real_encode = time(5, Box::new(|| bytes = engine::encode(&real).len()));
        println!("real {}x{}: hash {real_hash:.2} ms, WebP encode {real_encode:.1} ms ({} KB)", real.width, real.height, bytes / 1024);
    }
    let focus_ms = time(200, Box::new(|| {
        std::hint::black_box(computer::focused_window());
    }));
    let dir = TempDir::new();
    let store = crate::store::Store::open(&dir.0).unwrap();
    let sweep_ms = time(100, Box::new(|| {
        store.sweep(engine::now_ms()).unwrap();
    }));
    let polls = 1000. / crate::fixture::focus_poll_ms() as f64;
    let still = 2. * hash_ms + polls * focus_ms + sweep_ms;
    println!(
        "5K hash {hash_ms:.2} ms, WebP encode {encode_ms:.1} ms ({} KB), focus poll {focus_ms:.2} ms, sweep {sweep_ms:.3} ms",
        size / 1024
    );
    println!(
        "a still screen, two 5K displays at 1 fps: {still:.1} ms/s = {:.2} % of a core; each kept 5K frame adds {:.1} %",
        still / 10.,
        encode_ms / 10.
    );
}
