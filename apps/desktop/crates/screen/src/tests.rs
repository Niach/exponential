//! End to end: a fake capturer into a running [`engine::Memory`], read back
//! through the `screen` endpoint's `get_screenshots`.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use computer::{FocusedWindow, ScreenView, TopWindow};
use serde_json::{json, Value};

use crate::capture::{Capturer, Frame, Want};
use crate::engine::{self, Memory, Timing};
use crate::server::{self, Hub};
use crate::store::tests::TempDir;
use crate::{store, tool, Status};

const SIDE: u32 = 320;

/// Every grab a new 8 x 8 black/white block pattern (a fresh hash), per
/// display; counts its calls.
struct FakeCapturer {
    seed: u64,
    grabs: Arc<Mutex<Vec<&'static str>>>,
}

fn splitmix(seed: &mut u64) -> u64 {
    *seed = seed.wrapping_add(0x9e37_79b9_7f4a_7c15);
    let mut z = *seed;
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    z ^ (z >> 31)
}

/// An 8 x 8 block pattern of `bits` on `display`.
fn blocks(display: u32, bits: u64) -> Frame {
    let cell = SIDE / 8;
    let mut rgba = Vec::with_capacity((SIDE * SIDE * 4) as usize);
    for y in 0..SIDE {
        for x in 0..SIDE {
            let k = (y / cell) * 8 + x / cell;
            let v = if bits >> (63 - k) & 1 == 1 { 255 } else { 0 };
            rgba.extend([v, v, v, 255]);
        }
    }
    Frame::rgba(display, SIDE, SIDE, rgba)
}

fn hash_of(frame: &Frame) -> String {
    crate::hash::hex(crate::hash::hash_rows(frame.width, frame.height, frame.stride, frame.bgra, frame.bytes()))
}

impl FakeCapturer {
    fn frame(&mut self, display: u32) -> Frame {
        blocks(display, splitmix(&mut self.seed))
    }
}

impl Capturer for FakeCapturer {
    fn grab_all(&mut self, wanted: &[Want]) -> Result<Vec<Frame>, String> {
        self.grabs.lock().unwrap().push("all");
        Ok(wanted.iter().map(|want| self.frame(want.display)).collect())
    }

    fn grab(&mut self, wanted: Want) -> Result<Option<Frame>, String> {
        self.grabs.lock().unwrap().push("one");
        Ok(Some(self.frame(wanted.display)))
    }
}

fn window(app: &str, title: &str) -> FocusedWindow {
    window_on(1, app, title)
}

fn window_on(display: u32, app: &str, title: &str) -> FocusedWindow {
    FocusedWindow {
        app: app.into(),
        app_id: None,
        title: title.into(),
        display,
        rect: computer::ScreenRect { x: 10, y: 20, width: 240, height: 210 },
    }
}

/// Both displays work displays, `focus` focused, no other window.
fn view(focus: Option<FocusedWindow>, work: &[u32]) -> Option<ScreenView> {
    Some(ScreenView { focused: focus, work: work.to_vec(), tops: Vec::new() })
}

type Shared<T> = Arc<Mutex<T>>;

struct Rig {
    memory: Memory,
    view: Shared<Option<ScreenView>>,
    hotkey: Arc<AtomicBool>,
    statuses: Shared<Vec<Status>>,
    grabs: Shared<Vec<&'static str>>,
}

impl Rig {
    fn focus(&self, focus: Option<FocusedWindow>) {
        let mut view = self.view.lock().unwrap();
        let work = view.as_ref().map(|view| view.work.clone()).unwrap_or_else(|| vec![1, 2]);
        *view = Some(ScreenView { focused: focus, work, tops: view.as_ref().map(|view| view.tops.clone()).unwrap_or_default() });
    }

    fn rows(&self) -> Vec<store::Row> {
        self.memory.store.lock().unwrap().frames(i64::MIN, i64::MAX).unwrap()
    }
}

fn timing(interval_ms: u64) -> Timing {
    Timing { interval: Duration::from_millis(interval_ms), poll: Duration::from_millis(10), encode_percent: 100 }
}

fn start(dir: &TempDir, capturer: Box<dyn Capturer>, first: Option<ScreenView>, interval_ms: u64) -> Rig {
    let view = Arc::new(Mutex::new(first));
    let hotkey = Arc::new(AtomicBool::new(false));
    let statuses = Arc::new(Mutex::new(Vec::new()));
    let source = view.clone();
    let heard = statuses.clone();
    let memory = Memory::start(
        &dir.0,
        capturer,
        Box::new(move || source.lock().unwrap().clone()),
        timing(interval_ms),
        hotkey.clone(),
        Arc::new(move |status| heard.lock().unwrap().push(status)),
    )
    .unwrap();
    Rig { memory, view, hotkey, statuses, grabs: Default::default() }
}

fn rig(dir: &TempDir, interval_ms: u64) -> Rig {
    let grabs = Arc::new(Mutex::new(Vec::new()));
    let capturer = Box::new(FakeCapturer { seed: 7, grabs: grabs.clone() });
    let mut rig = start(dir, capturer, view(Some(window("Zed", "main.rs")), &[1, 2]), interval_ms);
    rig.grabs = grabs;
    rig
}

fn count(memory: &Memory) -> usize {
    memory.store.lock().unwrap().frames(i64::MIN, i64::MAX).unwrap().len()
}

#[test]
fn frames_flow_from_the_capturer_to_get_screenshots_within_the_cap() {
    let dir = TempDir::new();
    let rig = rig(&dir, 40);
    wait_until(|| count(&rig.memory) >= 6);
    rig.focus(Some(window("Zed", "lib.rs")));
    wait_until(|| count(&rig.memory) >= 10);
    assert_eq!(rig.statuses.lock().unwrap().as_slice(), [Status::Active]);

    // Through the endpoint, as a run sees it.
    let store = rig.memory.store.clone();
    let hub = Arc::new(Hub::new(Arc::new(move |name: &str, arguments: &Value| {
        tool::call(Some(&store), name, arguments, engine::now_ms(), &[1, 2])
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
        // LABEL: display 2 holds no window here, so its frames carry none.
        assert!(line.contains(" · Zed — ") || line.ends_with(" ·  — "), "{line}");
        assert_eq!(pair[1]["mimeType"], "image/webp");
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
    // The worker is done with the first batch (else the change is skipped).
    wait_until(|| !rig.memory.encoding());
    assert_eq!(rig.grabs.lock().unwrap().as_slice(), ["all"]);
    rig.focus(Some(window("Zed", "lib.rs")));
    wait_until(|| count(&rig.memory) == 3);
    assert_eq!(rig.grabs.lock().unwrap().as_slice(), ["all", "one"]);
    let rows = rig.rows();
    assert_eq!(rows.len(), 3);
    assert_eq!((rows[2].display, rows[2].title.as_str()), (1, "lib.rs"));
    rig.memory.stop(true);
}

#[test]
fn a_frame_is_stored_cut_to_its_focused_window_on_its_own_display() {
    let dir = TempDir::new();
    let rig = rig(&dir, 40);
    wait_until(|| count(&rig.memory) >= 2);
    let rows = rig.rows();
    let on_one = rows.iter().find(|row| row.display == 1).unwrap();
    let on_two = rows.iter().find(|row| row.display == 2).unwrap();
    assert_eq!(on_one.focus, Some(store::Rect { x: 10, y: 20, width: 240, height: 210 }));
    assert_eq!(on_two.focus, None);
    // The row keeps the frame's size; the file holds the cut, which the
    // tool returns as it is.
    assert_eq!((on_one.width, on_one.height), (SIDE, SIDE));
    for (row, size) in [(on_one, (240, 210)), (on_two, (SIDE, SIDE))] {
        let webp = std::fs::read(dir.0.join(&row.path)).unwrap();
        let picture = tool::decode(&webp).unwrap();
        assert_eq!((picture.width, picture.height), size);
    }
    rig.memory.stop(false);
    assert!(dir.0.join(store::INDEX_FILE).exists());
}

#[test]
fn a_busy_worker_skips_the_batch_and_the_next_tick_retries_the_change() {
    let worker = Arc::new(engine::Worker::default());
    let busy = &worker.busy;
    let mut gate = engine::Gate { dedupe: Default::default(), worker: worker.clone() };
    let (still, changed) = (0x0f0f_0f0f_0f0f_0f0f, 0xffff_0f0f_0f0f_0f0f);
    let zed = |display, hash| (display, hash, "Zed", "main.rs");
    assert_eq!(gate.admit(&[zed(1, still), zed(2, still)]), Some(vec![true, true]));
    assert!(busy.load(Ordering::Acquire));
    // Still encoding: the change is skipped and not remembered.
    assert_eq!(gate.admit(&[zed(1, changed), zed(2, still)]), None);
    busy.store(false, Ordering::Release);
    // The next tick: the change is still one; the still display stays out.
    assert_eq!(gate.admit(&[zed(1, changed), zed(2, still)]), Some(vec![true, false]));
    busy.store(false, Ordering::Release);
    // Nothing kept = the worker stays idle.
    assert_eq!(gate.admit(&[zed(1, changed), zed(2, still)]), Some(vec![false, false]));
    assert!(!busy.load(Ordering::Acquire));
    // LABEL: another display's own label counts per display.
    assert_eq!(gate.admit(&[zed(1, changed), (2, still, "Notes", "List")]), Some(vec![false, true]));
    busy.store(false, Ordering::Release);
    // BUDGET: resting after a batch counts as busy too.
    *worker.rest_until.lock().unwrap() = Some(std::time::Instant::now() + Duration::from_secs(60));
    assert_eq!(gate.admit(&[zed(1, still)]), None);
    *worker.rest_until.lock().unwrap() = Some(std::time::Instant::now());
    assert_eq!(gate.admit(&[zed(1, still)]), Some(vec![true]));
}

/// Like ScreenCaptureKit: `grab_all` hands each display's frame once per
/// change (`version`), `grab` its latest regardless.
struct OnceCapturer {
    version: Arc<std::sync::atomic::AtomicU64>,
    handed: [u64; 2],
    grabs: Arc<Mutex<Vec<&'static str>>>,
}

impl OnceCapturer {
    fn latest(&self, display: u32) -> Frame {
        let mut fake = FakeCapturer { seed: self.version.load(Ordering::Acquire) * 2 + u64::from(display), grabs: Default::default() };
        fake.frame(display)
    }
}

impl Capturer for OnceCapturer {
    fn grab_all(&mut self, wanted: &[Want]) -> Result<Vec<Frame>, String> {
        let version = self.version.load(Ordering::Acquire) + 1;
        let fresh: Vec<u32> = wanted.iter().map(|want| want.display).filter(|display| self.handed[*display as usize - 1] != version).collect();
        for display in &fresh {
            self.handed[*display as usize - 1] = version;
        }
        Ok(fresh.into_iter().map(|display| self.latest(display)).collect())
    }

    fn grab(&mut self, wanted: Want) -> Result<Option<Frame>, String> {
        self.grabs.lock().unwrap().push("one");
        Ok(Some(self.latest(wanted.display)))
    }
}

#[test]
fn a_change_skipped_while_the_worker_rests_is_stored_once_it_is_ready() {
    let dir = TempDir::new();
    let version = Arc::new(std::sync::atomic::AtomicU64::new(0));
    let grabs = Arc::new(Mutex::new(Vec::new()));
    let rig = start(
        &dir,
        Box::new(OnceCapturer { version: version.clone(), handed: [0; 2], grabs: grabs.clone() }),
        view(Some(window("Zed", "main.rs")), &[1, 2]),
        30,
    );
    let memory = &rig.memory;
    wait_until(|| count(memory) == 2);
    wait_until(|| !memory.encoding());
    // The worker rests; the screen changes once and then holds still.
    memory.rest(Duration::from_millis(300));
    version.store(1, Ordering::Release);
    std::thread::sleep(Duration::from_millis(150));
    assert_eq!(count(memory), 2, "skipped while resting");
    // The next ticks re-grab the skipped displays: the change lands.
    wait_until(|| count(memory) == 4);
    assert!(grabs.lock().unwrap().contains(&"one"));
    rig.memory.stop(true);
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
    rig.focus(Some(window("1Password", "Vault")));
    wait_until(|| heard(2));
    wait_until(|| !rig.memory.encoding());
    let paused_at = count(&rig.memory);
    std::thread::sleep(Duration::from_millis(200));
    assert_eq!(count(&rig.memory), paused_at, "no frame while a password manager is focused");
    rig.focus(Some(window("Zed", "main.rs")));
    wait_until(|| count(&rig.memory) > paused_at);
    rig.hotkey.store(true, Ordering::Release);
    wait_until(|| heard(4));
    wait_until(|| !rig.memory.encoding());
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

/// One display's slot as ScreenCaptureKit keeps it: the LAST delivered
/// frame stays until a newer one arrives, so a grab after a pause would
/// find the frame of the window that paused it.
#[derive(Default)]
struct Slot {
    bits: Option<u64>,
    delivered: Option<Instant>,
    seq: u64,
    handed: u64,
}

struct StaleCapturer {
    slot: Shared<Slot>,
}

impl StaleCapturer {
    fn take(&self, want: Want, fresh_only: bool) -> Option<Frame> {
        let mut slot = self.slot.lock().unwrap();
        if (fresh_only && slot.seq == slot.handed) || !want.takes(slot.delivered?) {
            return None;
        }
        slot.handed = slot.seq;
        Some(blocks(want.display, slot.bits?))
    }
}

impl Capturer for StaleCapturer {
    fn grab_all(&mut self, wanted: &[Want]) -> Result<Vec<Frame>, String> {
        Ok(wanted.iter().filter_map(|want| self.take(*want, true)).collect())
    }

    fn grab(&mut self, wanted: Want) -> Result<Option<Frame>, String> {
        Ok(self.take(wanted, false))
    }
}

fn deliver(slot: &Shared<Slot>, bits: u64) {
    let mut slot = slot.lock().unwrap();
    slot.bits = Some(bits);
    slot.delivered = Some(Instant::now());
    slot.seq += 1;
}

/// STALE: the vault's frame the OS still holds when the focus leaves it is
/// never stored (under the next app's label), neither by the tick nor by
/// the focus-change grab; the next frame delivered after the resume is.
#[test]
fn a_frame_delivered_while_paused_is_never_stored_after_the_resume() {
    let dir = TempDir::new();
    let slot: Shared<Slot> = Default::default();
    let (before, vault, after) = (0x00ff_00ff_00ff_00ff, 0xffff_0000_ffff_0000, 0x0f0f_f0f0_0f0f_f0f0);
    deliver(&slot, before);
    let rig = start(&dir, Box::new(StaleCapturer { slot: slot.clone() }), view(Some(window("Zed", "main.rs")), &[1]), 40);
    wait_until(|| count(&rig.memory) == 1);
    rig.focus(Some(window("1Password", "Vault")));
    wait_until(|| rig.statuses.lock().unwrap().len() >= 2);
    deliver(&slot, vault);
    // The focus leaves the vault: the slot still holds its frame.
    rig.focus(Some(window("Safari", "Docs")));
    wait_until(|| rig.statuses.lock().unwrap().len() >= 3);
    std::thread::sleep(Duration::from_millis(250));
    let vault_hash = hash_of(&blocks(1, vault));
    assert!(rig.rows().iter().all(|row| row.hash != vault_hash), "the vault frame was stored");
    assert_eq!(count(&rig.memory), 1);
    deliver(&slot, after);
    wait_until(|| count(&rig.memory) == 2);
    let rows = rig.rows();
    assert_eq!((rows[1].hash.as_str(), rows[1].app.as_str()), (hash_of(&blocks(1, after)).as_str(), "Safari"));
    rig.memory.stop(true);
}

/// PAUSE fails closed: no window list (`None`) or no focused window holds
/// capture paused; RETENTION still sweeps while paused.
#[test]
fn no_focused_window_reading_pauses_and_the_sweep_runs_while_paused() {
    for first in [None, view(None, &[1, 2])] {
        let dir = TempDir::new();
        let grabs = Arc::new(Mutex::new(Vec::new()));
        let rig = start(&dir, Box::new(FakeCapturer { seed: 1, grabs: grabs.clone() }), first, 40);
        wait_until(|| !rig.statuses.lock().unwrap().is_empty());
        let old = engine::now_ms() - crate::fixture::retention_ms() - 1_000;
        let row = store::Row {
            at: old,
            display: 1,
            app: "Zed".into(),
            title: "old".into(),
            hash: "0000000000000000".into(),
            path: store::relative_path(old, 1),
            width: 1,
            height: 1,
            focus: None,
        };
        rig.memory.store.lock().unwrap().insert(&row, b"webp").unwrap();
        wait_until(|| count(&rig.memory) == 0);
        std::thread::sleep(Duration::from_millis(200));
        assert_eq!(count(&rig.memory), 0);
        assert!(grabs.lock().unwrap().is_empty(), "no grab while paused");
        assert_eq!(rig.statuses.lock().unwrap().as_slice(), [Status::Paused]);
        rig.memory.stop(true);
    }
}

/// CAPTURE: only the work displays; none (a picked display gone) = no
/// grab. LABEL: another display carries its top window; a display whose
/// top window is denylisted is skipped.
#[test]
fn only_work_displays_are_captured_each_with_its_own_label() {
    let dir = TempDir::new();
    let rig = rig(&dir, 40);
    *rig.view.lock().unwrap() = view(Some(window("Zed", "main.rs")), &[2]);
    let only_two = |rows: &[store::Row], from: usize| rows[from..].iter().all(|row| row.display == 2);
    let at = count(&rig.memory);
    wait_until(|| count(&rig.memory) >= at + 3);
    // The first rows may predate the switch; the later ones are display 2 only.
    let skip = count(&rig.memory) - 2;
    assert!(only_two(&rig.rows(), skip));
    *rig.view.lock().unwrap() = view(Some(window("Zed", "main.rs")), &[]);
    std::thread::sleep(Duration::from_millis(60));
    wait_until(|| !rig.memory.encoding());
    let none_at = count(&rig.memory);
    std::thread::sleep(Duration::from_millis(200));
    assert_eq!(count(&rig.memory), none_at, "no work display, no frame");

    // Display 2 shows Notes on top: its frames say so.
    let notes = TopWindow { display: 2, app: "Notes".into(), title: "Shopping list".into() };
    *rig.view.lock().unwrap() = Some(ScreenView { focused: Some(window("Zed", "main.rs")), work: vec![1, 2], tops: vec![notes] });
    wait_until(|| rig.rows().iter().any(|row| row.display == 2 && row.app == "Notes"));
    assert!(rig.rows().iter().filter(|row| row.display == 2).last().is_some_and(|row| row.title == "Shopping list"));
    // A vault on top of display 2: display 1 goes on, display 2 stops.
    let vault = TopWindow { display: 2, app: "Bitwarden".into(), title: "Vault".into() };
    *rig.view.lock().unwrap() = Some(ScreenView { focused: Some(window_on(1, "Zed", "main.rs")), work: vec![1, 2], tops: vec![vault] });
    std::thread::sleep(Duration::from_millis(60));
    wait_until(|| !rig.memory.encoding());
    let twos = rig.rows().iter().filter(|row| row.display == 2).count();
    let ones = rig.rows().iter().filter(|row| row.display == 1).count();
    wait_until(|| rig.rows().iter().filter(|row| row.display == 1).count() > ones + 2);
    assert_eq!(rig.rows().iter().filter(|row| row.display == 2).count(), twos, "the vault's display is skipped");
    assert!(rig.rows().iter().all(|row| row.app != "Bitwarden"));
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

#[test]
fn a_leftover_buffer_is_found_and_forgotten() {
    let dir = TempDir::new();
    assert!(!crate::leftover(&dir.0));
    drop(store::Store::open(&dir.0).unwrap());
    assert!(crate::leftover(&dir.0));
    crate::forget(&dir.0);
    assert!(!crate::leftover(&dir.0));
}

/// A text-like `width` x `height` frame (a white page of dark glyph cells)
/// with a dark band at `variant`'s height: every variant hashes apart.
fn page(width: u32, height: u32, variant: u32) -> Vec<u8> {
    let mut seed = u64::from(variant) + 1;
    let mut rgba = vec![255u8; (width * height * 4) as usize];
    let band = (variant * 7 % 10) * height / 10;
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
    for y in band..(band + height / 5).min(height) {
        for x in 0..width {
            let at = ((y * width + x) * 4) as usize;
            rgba[at..at + 3].copy_from_slice(&[40, 60, 90]);
        }
    }
    rgba
}

/// This process's CPU time (user + system, every thread), seconds.
#[cfg(unix)]
fn cpu_seconds() -> f64 {
    let mut usage: libc::rusage = unsafe { std::mem::zeroed() };
    unsafe { libc::getrusage(libc::RUSAGE_SELF, &mut usage) };
    let seconds = |t: libc::timeval| t.tv_sec as f64 + t.tv_usec as f64 / 1e6;
    seconds(usage.ru_utime) + seconds(usage.ru_stime)
}

/// Pixels lent without a copy, as ScreenCaptureKit's are.
struct Lent(Arc<Vec<u8>>);

impl crate::capture::Pixels for Lent {
    fn bytes(&self) -> &[u8] {
        &self.0
    }
}

/// Two displays whose content changes on every grab (a page scrolling),
/// lent like the macOS capturer's buffers.
struct ChangingCapturer {
    width: u32,
    height: u32,
    variants: Vec<Arc<Vec<u8>>>,
    next: usize,
}

impl ChangingCapturer {
    fn frame(&mut self, display: u32) -> Frame {
        self.next += 1;
        let source = self.variants[self.next % self.variants.len()].clone();
        Frame::lent(display, self.width, self.height, self.width as usize * 4, false, Box::new(Lent(source)))
    }
}

impl Capturer for ChangingCapturer {
    fn grab_all(&mut self, wanted: &[Want]) -> Result<Vec<Frame>, String> {
        Ok(wanted.iter().map(|want| self.frame(want.display)).collect())
    }

    fn grab(&mut self, wanted: Want) -> Result<Option<Frame>, String> {
        Ok(Some(self.frame(wanted.display)))
    }
}

/// The CPU budget (< 2 % of a core at 1 fps on two displays, also while the
/// screen changes every second), by part and then end to end:
/// `cargo test --release -p screen --lib -- --ignored --nocapture cpu_budget`
/// (`SCREEN_BENCH_SECONDS`, default 20; `SCREEN_BENCH_PNG=<png>` adds a real
/// screenshot's parts).
#[test]
#[ignore]
#[cfg(unix)]
fn cpu_budget() {
    let time = |runs: u32, mut f: Box<dyn FnMut() + '_>| {
        let start = Instant::now();
        for _ in 0..runs {
            f();
        }
        start.elapsed().as_secs_f64() * 1000. / f64::from(runs)
    };
    let parts = |name: &str, frame: &Frame| {
        let window = store::Rect { x: frame.width as i32 / 10, y: frame.height as i32 / 10, width: frame.width * 8 / 10, height: frame.height * 8 / 10 };
        let hash_ms = time(20, Box::new(|| {
            std::hint::black_box(crate::hash::hash_rows(frame.width, frame.height, frame.stride, frame.bgra, frame.bytes()));
        }));
        for (label, focus) in [("whole", None), ("window", Some(window))] {
            let picture = crate::shrink::prepare(frame, focus);
            let fit_ms = time(10, Box::new(|| {
                std::hint::black_box(crate::shrink::prepare(frame, focus));
            }));
            let mut bytes = 0;
            let encode_ms = time(10, Box::new(|| bytes = crate::shrink::encode(&picture).len()));
            println!(
                "{name} {}x{} {label}: hash {hash_ms:.2} ms, cut+fit {fit_ms:.1} ms to {}x{}, WebP {encode_ms:.1} ms ({} KB)",
                frame.width, frame.height, picture.width, picture.height, bytes / 1024
            );
        }
    };
    if let Ok(path) = std::env::var("SCREEN_BENCH_PNG") {
        let shot = image::open(path).unwrap().to_rgba8();
        parts("real", &Frame::rgba(1, shot.width(), shot.height(), shot.into_raw()));
    }
    let view_ms = time(200, Box::new(|| {
        std::hint::black_box(computer::screen_view());
    }));
    println!("screen view poll {view_ms:.2} ms");
    let seconds: u64 = std::env::var("SCREEN_BENCH_SECONDS").ok().and_then(|s| s.parse().ok()).unwrap_or(20);
    for (width, height) in [(3456u32, 2234u32), (5120, 2880)] {
        let variants: Vec<Arc<Vec<u8>>> = (0..5).map(|variant| Arc::new(page(width, height, variant))).collect();
        parts("page", &Frame::rgba(1, width, height, variants[0].to_vec()));
        for percent in [100, crate::fixture::encode_percent()] {
            // End to end: the real loop, worker and store, 1 fps, two displays
            // changing on every grab, the real screen view poll.
            let dir = TempDir::new();
            let focus = FocusedWindow {
                app: "Safari".into(),
                app_id: None,
                title: "A long page".into(),
                display: 1,
                rect: computer::ScreenRect { x: width as i32 / 10, y: 0, width: width * 8 / 10, height },
            };
            let memory = Memory::start(
                &dir.0,
                Box::new(ChangingCapturer { width, height, variants: variants.clone(), next: 0 }),
                Box::new(move || {
                    std::hint::black_box(computer::screen_view());
                    view(Some(focus.clone()), &[1, 2])
                }),
                Timing { encode_percent: percent, ..Timing::from_fixture() },
                Arc::new(AtomicBool::new(false)),
                Arc::new(|_| {}),
            )
            .unwrap();
            std::thread::sleep(Duration::from_millis(1500));
            let (cpu, start) = (cpu_seconds(), Instant::now());
            let before = count(&memory);
            std::thread::sleep(Duration::from_secs(seconds));
            let used = cpu_seconds() - cpu;
            let wall = start.elapsed().as_secs_f64();
            let stored = count(&memory) - before;
            memory.stop(true);
            println!(
                "two {width}x{height} displays changing every second, encodePercent {percent}, {wall:.0} s: {:.2} % of a core, {stored} frames stored of {} grabbed",
                used / wall * 100.,
                2. * wall.round()
            );
        }
    }
}
