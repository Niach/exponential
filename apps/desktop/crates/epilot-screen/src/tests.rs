//! End to end: a fake capturer into a running [`engine::Memory`], read back
//! through the `screen` endpoint's `get_screenshots`.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use epilot::visible::{ScreenLook, VisibleWindow};
use epilot::{FocusedWindow, ScreenView, TopWindow};
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
    crate::hash::hex(crate::hash::hash_rows(frame.width, frame.height, frame.stride, frame.bgra, frame.bytes(), &frame.masks))
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
        rect: epilot::ScreenRect { x: 10, y: 20, width: 240, height: 210 },
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
    /// Every window of the window list (the looks' `windows`).
    windows: Shared<Vec<VisibleWindow>>,
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
    start_listing(dir, capturer, first, Vec::new(), interval_ms)
}

/// [`start`] with `listed` as every window of the window list from the
/// first look on.
fn start_listing(dir: &TempDir, capturer: Box<dyn Capturer>, first: Option<ScreenView>, listed: Vec<VisibleWindow>, interval_ms: u64) -> Rig {
    start_rig(dir, capturer, first, Arc::new(Mutex::new(listed)), Arc::new(AtomicU64::new(0)), interval_ms)
}

/// [`start_listing`] over a window list and a display epoch the capturer
/// may share.
fn start_rig(dir: &TempDir, capturer: Box<dyn Capturer>, first: Option<ScreenView>, windows: Shared<Vec<VisibleWindow>>, epoch: Arc<AtomicU64>, interval_ms: u64) -> Rig {
    start_epochs(dir, capturer, first, windows, (epoch.clone(), epoch), interval_ms)
}

/// [`start_rig`] whose window list reads one epoch and whose grabs another
/// (`(read, now)`).
fn start_epochs(
    dir: &TempDir,
    capturer: Box<dyn Capturer>,
    first: Option<ScreenView>,
    windows: Shared<Vec<VisibleWindow>>,
    (read_epoch, now_epoch): (Arc<AtomicU64>, Arc<AtomicU64>),
    interval_ms: u64,
) -> Rig {
    let view = Arc::new(Mutex::new(first));
    let hotkey = Arc::new(AtomicBool::new(false));
    let statuses = Arc::new(Mutex::new(Vec::new()));
    let source = view.clone();
    let listed = windows.clone();
    let heard = statuses.clone();
    let memory = Memory::start(
        &dir.0,
        capturer,
        Box::new(move || {
            let epoch = read_epoch.load(Ordering::Acquire);
            source.lock().unwrap().clone().map(|view| ScreenLook { view, windows: listed.lock().unwrap().clone(), epoch })
        }),
        Box::new(move || now_epoch.load(Ordering::Acquire)),
        timing(interval_ms),
        hotkey.clone(),
        Arc::new(move |status| heard.lock().unwrap().push(status)),
    )
    .unwrap();
    Rig { memory, view, windows, hotkey, statuses, grabs: Default::default() }
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
    let notes = TopWindow { display: 2, app: "Notes".into(), app_id: None, title: "Shopping list".into() };
    *rig.view.lock().unwrap() = Some(ScreenView { focused: Some(window("Zed", "main.rs")), work: vec![1, 2], tops: vec![notes] });
    wait_until(|| rig.rows().iter().any(|row| row.display == 2 && row.app == "Notes"));
    assert!(rig.rows().iter().filter(|row| row.display == 2).last().is_some_and(|row| row.title == "Shopping list"));
    // A vault on top of display 2: display 1 goes on, display 2 stops.
    let vault = TopWindow { display: 2, app: "Bitwarden".into(), app_id: None, title: "Vault".into() };
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

/// #FEED-164: a display whose top window is denylisted BY ITS APP ID (the
/// macOS bundle id; the name says nothing) is skipped, like PAUSE and MASK.
#[test]
fn a_top_window_denylisted_by_its_app_id_skips_its_display() {
    let top = |app_id: Option<&str>| TopWindow { display: 2, app: "Desktop".into(), app_id: app_id.map(Into::into), title: "Vault".into() };
    let with = |top| ScreenView { focused: Some(window("Zed", "main.rs")), work: vec![1, 2], tops: vec![top] };
    assert_eq!(engine::capturing(Some(&with(top(None))), false), [1, 2]);
    assert_eq!(engine::capturing(Some(&with(top(Some("com.bitwarden.desktop")))), false), [1]);
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
#[cfg(unix)]
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
#[cfg(unix)]
struct Lent(Arc<Vec<u8>>);

#[cfg(unix)]
impl crate::capture::Pixels for Lent {
    fn bytes(&self) -> &[u8] {
        &self.0
    }
}

/// Two displays whose content changes on every grab (a page scrolling),
/// lent like the macOS capturer's buffers.
#[cfg(unix)]
struct ChangingCapturer {
    width: u32,
    height: u32,
    variants: Vec<Arc<Vec<u8>>>,
    next: usize,
}

#[cfg(unix)]
impl ChangingCapturer {
    fn frame(&mut self, display: u32) -> Frame {
        self.next += 1;
        let source = self.variants[self.next % self.variants.len()].clone();
        Frame::lent(display, self.width, self.height, self.width as usize * 4, false, Box::new(Lent(source)))
    }
}

#[cfg(unix)]
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
/// `cargo test --release -p epilot-screen --lib -- --ignored --nocapture cpu_budget`
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
            std::hint::black_box(crate::hash::hash_rows(frame.width, frame.height, frame.stride, frame.bgra, frame.bytes(), &[]));
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
        std::hint::black_box(epilot::visible::screen_look());
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
                rect: epilot::ScreenRect { x: width as i32 / 10, y: 0, width: width * 8 / 10, height },
            };
            let memory = Memory::start(
                &dir.0,
                Box::new(ChangingCapturer { width, height, variants: variants.clone(), next: 0 }),
                Box::new(move || {
                    std::hint::black_box(epilot::visible::screen_look());
                    view(Some(focus.clone()), &[1, 2]).map(|view| ScreenLook { view, windows: Vec::new(), epoch: 0 })
                }),
                Box::new(|| 0),
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

/// The same frame on every grab (a still screen): white, or `bits`.
struct StillCapturer {
    bits: u64,
}

impl Capturer for StillCapturer {
    fn grab_all(&mut self, wanted: &[Want]) -> Result<Vec<Frame>, String> {
        Ok(wanted.iter().map(|want| blocks(want.display, self.bits)).collect())
    }

    fn grab(&mut self, wanted: Want) -> Result<Option<Frame>, String> {
        Ok(Some(blocks(wanted.display, self.bits)))
    }
}

fn listed(display: u32, app: &str, title: &str, size: u32, rect: epilot::ScreenRect) -> VisibleWindow {
    VisibleWindow { display, display_size: (size, size), app: app.into(), app_id: None, title: title.into(), rect }
}

/// MASK: a vault that is NOT on top (behind the focused editor) is filled
/// black before the hash and in the stored picture; one that cannot be
/// placed (the reading's display size is not the frame's) drops that
/// display's frames, the other display goes on.
#[test]
fn a_denylisted_window_behind_the_focus_is_masked_or_its_display_skipped() {
    let dir = TempDir::new();
    let quadrant = epilot::ScreenRect { x: 0, y: 0, width: 160, height: 160 };
    let windows = vec![listed(1, "Zed", "main.rs", SIDE, window("Zed", "main.rs").rect), listed(1, "Bitwarden", "Vault", SIDE, quadrant)];
    let rig = start_listing(&dir, Box::new(StillCapturer { bits: u64::MAX }), view(Some(window("Zed", "main.rs")), &[1]), windows, 40);
    wait_until(|| count(&rig.memory) == 1);
    let row = rig.rows().remove(0);
    // The top-left 4 x 4 blocks of 8 x 8 (160 px) black, the rest white.
    let masked = (0..64).fold(0u64, |bits, k| if k / 8 < 4 && k % 8 < 4 { bits } else { bits | 1 << (63 - k) });
    assert_eq!(row.hash, hash_of(&blocks(1, masked)));
    // The stored picture (cut to the editor at (10, 20)): black inside the
    // vault's rect, white past it.
    let webp = std::fs::read(rig.memory.store.lock().unwrap().file(&row)).unwrap();
    let picture = webp::Decoder::new(&webp).decode().unwrap();
    let width = picture.width() as usize;
    let channels = picture.len() / (width * picture.height() as usize);
    let at = |x: usize, y: usize| picture[(y * width + x) * channels];
    assert!(at(2, 2) < 24 && at(140, 130) < 24, "the vault is black");
    assert!(at(200, 180) > 230, "the editor stays");
    rig.memory.stop(true);

    // Two displays; the vault's reading says display 1 is 640 px, the frame
    // is 320: display 1 is skipped, display 2 kept.
    let dir = TempDir::new();
    let grabs = Arc::new(Mutex::new(Vec::new()));
    let windows = vec![listed(1, "1Password", "Vault", 640, quadrant)];
    let rig = start_listing(&dir, Box::new(FakeCapturer { seed: 5, grabs }), view(Some(window_on(2, "Zed", "main.rs")), &[1, 2]), windows, 40);
    wait_until(|| count(&rig.memory) >= 3);
    assert!(rig.rows().iter().all(|row| row.display == 2), "display 1 is skipped");
    rig.memory.stop(true);
}

/// Every grab a new frame the OS captured `lag_ms` before it; the stamps
/// it handed out.
struct LaggingCapturer {
    inner: FakeCapturer,
    lag_ms: i64,
    stamps: Shared<Vec<i64>>,
}

impl LaggingCapturer {
    fn frame(&mut self, display: u32) -> Frame {
        let at = engine::now_ms() - self.lag_ms;
        self.stamps.lock().unwrap().push(at);
        self.inner.frame(display).stamped(at)
    }
}

impl Capturer for LaggingCapturer {
    fn grab_all(&mut self, wanted: &[Want]) -> Result<Vec<Frame>, String> {
        Ok(wanted.iter().map(|want| self.frame(want.display)).collect())
    }

    fn grab(&mut self, wanted: Want) -> Result<Option<Frame>, String> {
        Ok(Some(self.frame(wanted.display)))
    }
}

/// LABEL by capture time + STAMP: a frame the OS captured before a title
/// change but handed out after it carries the OLD title (the reading
/// nearest its capture time), and every row's `at` is the capturer's stamp.
#[test]
fn a_frame_is_labelled_by_the_reading_nearest_its_capture_time() {
    let dir = TempDir::new();
    let stamps: Shared<Vec<i64>> = Default::default();
    let capturer = LaggingCapturer { inner: FakeCapturer { seed: 9, grabs: Default::default() }, lag_ms: 400, stamps: stamps.clone() };
    let rig = start(&dir, Box::new(capturer), view(Some(window("Zed", "before.rs")), &[1]), 40);
    // Long enough for the readings to cover the lag.
    std::thread::sleep(Duration::from_millis(600));
    let switched = engine::now_ms();
    rig.focus(Some(window("Zed", "after.rs")));
    wait_until(|| rig.rows().iter().filter(|row| row.at > switched + 100).count() >= 2);
    let rows = rig.rows();
    rig.memory.stop(true);
    let late_before: Vec<&store::Row> = rows.iter().filter(|row| row.at < switched - 30 && row.at > switched - 400).collect();
    assert!(!late_before.is_empty(), "frames captured just before the change");
    for row in &rows {
        if row.at < switched - 30 {
            assert_eq!(row.title, "before.rs", "captured at {} before the change at {switched}", row.at);
        } else if row.at > switched + 30 {
            assert_eq!(row.title, "after.rs");
        }
    }
    let stamps = stamps.lock().unwrap();
    assert!(rows.iter().all(|row| stamps.contains(&row.at)), "every row carries its capture stamp");
}

/// PAUSE by the bundle id in the production path: a localized Passwords
/// app (its name matches nothing) is held paused by `com.apple.Passwords`.
#[test]
fn a_denylisted_bundle_id_pauses_whatever_the_app_is_called() {
    let dir = TempDir::new();
    let grabs = Arc::new(Mutex::new(Vec::new()));
    let passwords = FocusedWindow { app_id: Some("com.apple.Passwords".into()), ..window("Kennwörter", "Alle Objekte") };
    let rig = start(&dir, Box::new(FakeCapturer { seed: 2, grabs: grabs.clone() }), view(Some(passwords), &[1]), 40);
    wait_until(|| !rig.statuses.lock().unwrap().is_empty());
    std::thread::sleep(Duration::from_millis(150));
    assert_eq!(rig.statuses.lock().unwrap().as_slice(), [Status::Paused]);
    assert!(grabs.lock().unwrap().is_empty());
    assert_eq!(count(&rig.memory), 0);
    rig.focus(Some(FocusedWindow { app_id: Some("com.apple.Notes".into()), ..window("Notizen", "Liste") }));
    wait_until(|| count(&rig.memory) >= 1);
    assert_eq!(rig.statuses.lock().unwrap().as_slice(), [Status::Paused, Status::Active]);
    rig.memory.stop(true);
}

/// A stream that delivers only on change (like [`StaleCapturer`]) plus a
/// live screenshot of `screen` (FRESH); counts the screenshots.
struct FreshCapturer {
    stale: StaleCapturer,
    screen: Shared<u64>,
    shots: Shared<u32>,
}

impl Capturer for FreshCapturer {
    fn grab_all(&mut self, wanted: &[Want]) -> Result<Vec<Frame>, String> {
        self.stale.grab_all(wanted)
    }

    fn grab(&mut self, wanted: Want) -> Result<Option<Frame>, String> {
        self.stale.grab(wanted)
    }

    fn fresh(&mut self, display: u32) -> Result<Option<Frame>, String> {
        *self.shots.lock().unwrap() += 1;
        Ok(Some(blocks(display, *self.screen.lock().unwrap()).stamped(engine::now_ms())))
    }
}

/// FRESH: a screen that stood still across a pause (the stream delivers
/// nothing new) is captured once after the resume, under the new label;
/// never the frame the stream still holds from before.
#[test]
fn a_still_screen_is_stored_once_after_a_resume() {
    let dir = TempDir::new();
    let slot: Shared<Slot> = Default::default();
    let (screen, shots): (Shared<u64>, Shared<u32>) = Default::default();
    let (before, docs) = (0x00ff_00ff_00ff_00ff, 0x0f0f_f0f0_0f0f_f0f0);
    deliver(&slot, before);
    let capturer = FreshCapturer { stale: StaleCapturer { slot: slot.clone() }, screen: screen.clone(), shots: shots.clone() };
    let rig = start(&dir, Box::new(capturer), view(Some(window("Zed", "main.rs")), &[1]), 40);
    wait_until(|| count(&rig.memory) == 1);
    assert_eq!(*shots.lock().unwrap(), 0, "no screenshot before any resume");
    rig.focus(Some(window("1Password", "Vault")));
    wait_until(|| rig.statuses.lock().unwrap().len() >= 2);
    // The person reads Docs; nothing on screen changes after that.
    *screen.lock().unwrap() = docs;
    rig.focus(Some(window("Safari", "Docs")));
    wait_until(|| count(&rig.memory) == 2);
    std::thread::sleep(Duration::from_millis(250));
    let rows = rig.rows();
    assert_eq!(rows.len(), 2);
    assert_eq!((rows[1].hash.as_str(), rows[1].app.as_str()), (hash_of(&blocks(1, docs)).as_str(), "Safari"));
    assert_eq!(*shots.lock().unwrap(), 1, "one screenshot per resume");
    rig.memory.stop(true);
}

/// Opens only when told to (a portal dialog the person has not answered);
/// says when it is dropped.
struct Dropped(Arc<AtomicBool>);

impl Capturer for Dropped {
    fn grab_all(&mut self, _: &[Want]) -> Result<Vec<Frame>, String> {
        Ok(Vec::new())
    }

    fn grab(&mut self, _: Want) -> Result<Option<Frame>, String> {
        Ok(None)
    }
}

impl Drop for Dropped {
    fn drop(&mut self) {
        self.0.store(true, Ordering::Release);
    }
}

/// A switch-off while the start still waits on its capturer (the Linux
/// portal's dialog) does not wait for it: the stop and the delete return at
/// once, and the capturer the start then gets is closed without capturing.
#[test]
fn a_stop_cancels_a_start_still_waiting_on_its_capturer() {
    let dir = TempDir::new();
    let ticket = crate::ticket();
    let (answer, dialog) = std::sync::mpsc::channel::<()>();
    let dropped = Arc::new(AtomicBool::new(false));
    let (path, closed) = (dir.0.clone(), dropped.clone());
    let starting = std::thread::spawn(move || {
        crate::start_inner(
            path,
            ticket,
            move || {
                dialog.recv().unwrap();
                Ok(Box::new(Dropped(closed)) as Box<dyn Capturer>)
            },
            Box::new(|| None),
            Box::new(|| 0),
        )
    });
    std::thread::sleep(Duration::from_millis(50));
    let asked = Instant::now();
    crate::forget(&dir.0);
    assert!(asked.elapsed() < Duration::from_millis(500), "the stop waited for the start");
    answer.send(()).unwrap();
    assert_eq!(starting.join().unwrap(), Err(crate::CANCELLED.to_string()));
    assert!(dropped.load(Ordering::Acquire), "the late capturer is closed");
    assert!(!crate::leftover(&dir.0), "nothing was stored");
    assert!(crate::grant("cancelled").is_err(), "screen memory stays off");
}

/// The same frame on every grab, captured `lag_ms` before it.
struct LaggingStill {
    bits: u64,
    lag_ms: i64,
}

impl Capturer for LaggingStill {
    fn grab_all(&mut self, wanted: &[Want]) -> Result<Vec<Frame>, String> {
        Ok(wanted.iter().map(|want| blocks(want.display, self.bits).stamped(engine::now_ms() - self.lag_ms)).collect())
    }

    fn grab(&mut self, wanted: Want) -> Result<Option<Frame>, String> {
        Ok(Some(blocks(wanted.display, self.bits).stamped(engine::now_ms() - self.lag_ms)))
    }
}

/// The hash of an all-white frame with the top-left 160 x 160 quadrant
/// black (a masked vault there).
fn masked_quadrant(bits: u64) -> String {
    let masked = (0..64).fold(bits, |bits, k| if k / 8 < 4 && k % 8 < 4 { bits & !(1 << (63 - k)) } else { bits });
    hash_of(&blocks(1, masked))
}

const QUADRANT: epilot::ScreenRect = epilot::ScreenRect { x: 0, y: 0, width: 160, height: 160 };

/// #FEED-164 MASK: a vault that is only in the NEWEST reading (it opened
/// after the frame's capture time, whose nearest reading lacks it) is
/// masked all the same.
#[test]
fn a_denylisted_window_only_in_the_newest_reading_is_masked() {
    let dir = TempDir::new();
    let editor = vec![listed(1, "Zed", "main.rs", SIDE, window("Zed", "main.rs").rect)];
    let rig = start_listing(&dir, Box::new(LaggingStill { bits: u64::MAX, lag_ms: 400 }), view(Some(window("Zed", "main.rs")), &[1]), editor, 40);
    wait_until(|| count(&rig.memory) == 1);
    // Long enough for the readings to cover the lag.
    std::thread::sleep(Duration::from_millis(600));
    let opened = engine::now_ms();
    rig.windows.lock().unwrap().push(listed(1, "Bitwarden", "Vault", SIDE, QUADRANT));
    wait_until(|| count(&rig.memory) == 2);
    let rows = rig.rows();
    rig.memory.stop(true);
    assert_eq!(rows[0].hash, hash_of(&blocks(1, u64::MAX)), "before the vault: unmasked");
    assert!(rows[1].at < opened, "captured before the vault opened ({} vs {opened})", rows[1].at);
    assert_eq!(rows[1].hash, masked_quadrant(u64::MAX), "masked by the newest reading");
}

/// #FEED-164 PAUSE per frame: a frame captured while a vault held the focus
/// is dropped although the focus left it by the time it is handed out.
#[test]
fn a_frame_captured_while_a_vault_was_focused_is_dropped() {
    let dir = TempDir::new();
    let stamps: Shared<Vec<i64>> = Default::default();
    let capturer = LaggingCapturer { inner: FakeCapturer { seed: 4, grabs: Default::default() }, lag_ms: 400, stamps: stamps.clone() };
    let rig = start(&dir, Box::new(capturer), view(Some(window("1Password", "Vault")), &[1]), 40);
    std::thread::sleep(Duration::from_millis(600));
    assert_eq!(count(&rig.memory), 0, "paused");
    let left = engine::now_ms();
    rig.focus(Some(window("Zed", "main.rs")));
    wait_until(|| rig.rows().iter().filter(|row| row.at > left + 100).count() >= 2);
    let rows = rig.rows();
    rig.memory.stop(true);
    assert!(stamps.lock().unwrap().iter().any(|at| *at < left - 50), "frames captured while the vault was focused");
    assert!(rows.iter().all(|row| row.at >= left - 30), "none of them stored: {:?}", rows.iter().map(|row| row.at - left).collect::<Vec<_>>());
}

/// #FEED-164 work display per frame: a frame of a display that was not a
/// work display when it was captured is dropped, although it is one now.
#[test]
fn a_frame_captured_before_its_display_turned_work_is_dropped() {
    let dir = TempDir::new();
    let stamps: Shared<Vec<i64>> = Default::default();
    let capturer = LaggingCapturer { inner: FakeCapturer { seed: 6, grabs: Default::default() }, lag_ms: 400, stamps };
    let rig = start(&dir, Box::new(capturer), view(Some(window("Zed", "main.rs")), &[1]), 40);
    std::thread::sleep(Duration::from_millis(600));
    let widened = engine::now_ms();
    *rig.view.lock().unwrap() = view(Some(window("Zed", "main.rs")), &[1, 2]);
    wait_until(|| rig.rows().iter().filter(|row| row.display == 2 && row.at > widened + 100).count() >= 2);
    let rows = rig.rows();
    rig.memory.stop(true);
    assert!(rows.iter().any(|row| row.display == 1 && row.at < widened), "display 1 went on");
    assert!(rows.iter().filter(|row| row.display == 2).all(|row| row.at >= widened - 30), "display 2 only from when it is a work display");
}

/// A still white screen; once `open` is set, its next grab opens a vault in
/// the window list (after the poll's read, before the pixels) and hands out
/// one `bits` frame, then nothing.
struct OpeningCapturer {
    open: Arc<AtomicBool>,
    windows: Shared<Vec<VisibleWindow>>,
    bits: u64,
    done: bool,
}

impl Capturer for OpeningCapturer {
    fn grab_all(&mut self, wanted: &[Want]) -> Result<Vec<Frame>, String> {
        if self.done {
            return Ok(Vec::new());
        }
        if !self.open.load(Ordering::Acquire) {
            return Ok(wanted.iter().map(|want| blocks(want.display, u64::MAX)).collect());
        }
        self.done = true;
        self.windows.lock().unwrap().push(listed(1, "Bitwarden", "Vault", SIDE, QUADRANT));
        Ok(wanted.iter().map(|want| blocks(want.display, self.bits)).collect())
    }

    fn grab(&mut self, _: Want) -> Result<Option<Frame>, String> {
        Ok(None)
    }
}

/// #FEED-164 RACE: a vault that opens between the window-list read and the
/// grab of the same iteration is masked in that grab's frame.
#[test]
fn a_vault_opening_between_the_read_and_the_grab_is_masked() {
    let dir = TempDir::new();
    let open = Arc::new(AtomicBool::new(false));
    let windows: Shared<Vec<VisibleWindow>> = Arc::new(Mutex::new(vec![listed(1, "Zed", "main.rs", SIDE, window("Zed", "main.rs").rect)]));
    let bits = 0xff00_ff00_ff00_ff00;
    let capturer = OpeningCapturer { open: open.clone(), windows: windows.clone(), bits, done: false };
    let rig = start_rig(&dir, Box::new(capturer), view(Some(window("Zed", "main.rs")), &[1]), windows, Arc::new(AtomicU64::new(0)), 40);
    wait_until(|| count(&rig.memory) == 1);
    open.store(true, Ordering::Release);
    wait_until(|| count(&rig.memory) == 2);
    let rows = rig.rows();
    rig.memory.stop(true);
    assert_eq!(rows[1].hash, masked_quadrant(bits));
}

/// [`LaggingCapturer`] that bumps the display epoch on its first grab once
/// `bump` is set (it saw the monitor set change).
struct HotPlugCapturer {
    inner: LaggingCapturer,
    bump: Arc<AtomicBool>,
    epoch: Arc<AtomicU64>,
}

impl Capturer for HotPlugCapturer {
    fn grab_all(&mut self, wanted: &[Want]) -> Result<Vec<Frame>, String> {
        if self.bump.swap(false, Ordering::AcqRel) {
            self.epoch.fetch_add(1, Ordering::AcqRel);
        }
        self.inner.grab_all(wanted)
    }

    fn grab(&mut self, wanted: Want) -> Result<Option<Frame>, String> {
        self.inner.grab(wanted)
    }
}

/// #FEED-164 DISPLAY SET: after the capturer sees a hot-plug, a frame whose
/// nearest reading predates it (another display numbering) is dropped; the
/// frames captured after it are stored.
#[test]
fn a_frame_read_against_the_display_set_before_a_hot_plug_is_dropped() {
    let dir = TempDir::new();
    let stamps: Shared<Vec<i64>> = Default::default();
    let (bump, epoch) = (Arc::new(AtomicBool::new(false)), Arc::new(AtomicU64::new(0)));
    let capturer = HotPlugCapturer {
        inner: LaggingCapturer { inner: FakeCapturer { seed: 8, grabs: Default::default() }, lag_ms: 400, stamps: stamps.clone() },
        bump: bump.clone(),
        epoch: epoch.clone(),
    };
    let rig = start_rig(&dir, Box::new(capturer), view(Some(window("Zed", "main.rs")), &[1]), Default::default(), epoch, 40);
    std::thread::sleep(Duration::from_millis(600));
    let plugged = engine::now_ms();
    bump.store(true, Ordering::Release);
    wait_until(|| rig.rows().iter().filter(|row| row.at > plugged + 100).count() >= 2);
    let rows = rig.rows();
    rig.memory.stop(true);
    let stale = |at: i64| at > plugged - 350 && at < plugged - 50;
    assert!(rows.iter().any(|row| row.at < plugged - 450), "frames before the hot-plug were stored");
    assert!(stamps.lock().unwrap().iter().any(|at| stale(*at)), "frames captured before it were handed out after it");
    assert!(!rows.iter().any(|row| stale(row.at)), "none of them stored");
}

/// #FEED-164 DISPLAY SET: a re-read after the grab that numbers the displays
/// differently places no mask, so the grab is skipped even for frames whose
/// own reading still matches the grab's epoch.
#[test]
fn a_frame_is_dropped_when_the_re_read_after_its_grab_is_of_another_display_set() {
    let dir = TempDir::new();
    let stamps: Shared<Vec<i64>> = Default::default();
    let (listed, grabbed) = (Arc::new(AtomicU64::new(0)), Arc::new(AtomicU64::new(0)));
    let capturer = LaggingCapturer { inner: FakeCapturer { seed: 10, grabs: Default::default() }, lag_ms: 400, stamps: stamps.clone() };
    let rig = start_epochs(&dir, Box::new(capturer), view(Some(window("Zed", "main.rs")), &[1]), Default::default(), (listed.clone(), grabbed), 40);
    std::thread::sleep(Duration::from_millis(600));
    let plugged = engine::now_ms();
    listed.store(1, Ordering::Release);
    std::thread::sleep(Duration::from_millis(600));
    let rows = rig.rows();
    rig.memory.stop(true);
    // Captured before the change (their reading = epoch 0), handed out after.
    let late = |at: i64| at > plugged - 350;
    assert!(rows.iter().any(|row| row.at < plugged - 450), "frames before the change were stored");
    assert!(stamps.lock().unwrap().iter().any(|at| late(*at) && *at < plugged - 50), "frames captured before it were handed out after it");
    assert!(!rows.iter().any(|row| late(row.at)), "none stored after the change");
}
