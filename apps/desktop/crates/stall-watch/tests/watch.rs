//! #EXP-2091: the stall rule and the file, behind a fake clock and a fake
//! sampler; then the real sampler on this test process with a thread that
//! really is stuck.

use std::cell::Cell;
use std::fs::File;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use stall_watch::{take_unreported, Beats, Clock, PlatformSampler, Sampler, Source, Watch, KEEP, LOOK, MAX_BYTES, STALL};

/// Every `log` record of this test process: the stall path makes none.
static LOGGED: AtomicUsize = AtomicUsize::new(0);

struct CountingLogger;

impl log::Log for CountingLogger {
    fn enabled(&self, _: &log::Metadata) -> bool {
        true
    }
    fn log(&self, _: &log::Record) {
        LOGGED.fetch_add(1, Ordering::SeqCst);
    }
    fn flush(&self) {}
}

fn count_logs() {
    let _ = log::set_logger(&CountingLogger);
    log::set_max_level(log::LevelFilter::Trace);
}

#[derive(Clone)]
struct FakeClock {
    now: Rc<Cell<Duration>>,
    unix: Rc<Cell<u64>>,
}

impl FakeClock {
    fn new() -> Self {
        // 2026-11-16 17:45:16 UTC.
        Self { now: Rc::new(Cell::new(Duration::from_secs(1000))), unix: Rc::new(Cell::new(1_794_851_116)) }
    }

    fn advance(&self, by: Duration) {
        self.now.set(self.now.get() + by);
        self.unix.set(self.unix.get() + by.as_secs());
    }
}

impl Clock for FakeClock {
    fn now(&self) -> Duration {
        self.now.get()
    }
    fn unix(&self) -> u64 {
        self.unix.get()
    }
}

/// Writes a known text; counts its calls.
#[derive(Clone)]
struct FakeSampler {
    calls: Rc<Cell<usize>>,
    text: String,
}

impl FakeSampler {
    fn new(text: &str) -> Self {
        Self { calls: Rc::new(Cell::new(0)), text: text.to_string() }
    }
}

impl Sampler for FakeSampler {
    fn sample(&mut self, pid: u32, _path: &Path, out: &mut File) {
        self.calls.set(self.calls.get() + 1);
        write!(out, "fake stacks of {pid} #{}\n{}", self.calls.get(), self.text).unwrap();
    }
}

fn dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("stall-watch-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir.join("crashes")
}

fn stall_files(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .map(|entries| entries.flatten().map(|entry| entry.file_name().to_string_lossy().into_owned()).filter(|name| name.starts_with("stall-") && name.ends_with(".txt")).collect())
        .unwrap_or_default();
    names.sort();
    names
}

fn beats() -> &'static Beats {
    Box::leak(Box::new(Beats::new()))
}

/// Look every [`LOOK`] for `time`; the files written.
fn pass(watch: &mut Watch<FakeClock, FakeSampler>, clock: &FakeClock, time: Duration) -> Vec<PathBuf> {
    let mut written = Vec::new();
    for _ in 0..(time.as_secs() / LOOK.as_secs()) {
        clock.advance(LOOK);
        written.extend(watch.look());
    }
    written
}

#[test]
fn two_minutes_without_a_beat_is_exactly_one_file_with_the_samplers_output_and_no_log_line() {
    count_logs();
    let logged_before = LOGGED.load(Ordering::SeqCst);
    let dir = dir("one-file");
    let (clock, sampler, beats) = (FakeClock::new(), FakeSampler::new("main: __psynch_mutexwait\n"), beats());
    let hooked = Arc::new(Mutex::new(Vec::new()));
    let hook = hooked.clone();
    let mut watch = Watch::new(dir.clone(), "exponential-desktop@0.14.177".into(), clock.clone(), sampler.clone(), beats)
        .on_stall(Box::new(move |report| hook.lock().unwrap().push(report.clone())));

    // Never beaten = not watched, however long.
    assert!(pass(&mut watch, &clock, STALL * 3).is_empty());
    // Beating: no stall.
    for _ in 0..60 {
        beats.beat(Source::Main);
        beats.beat(Source::Heartbeat);
        assert!(pass(&mut watch, &clock, LOOK).is_empty());
    }
    // Just under two minutes: nothing yet.
    assert!(pass(&mut watch, &clock, STALL - LOOK).is_empty());
    assert_eq!(sampler.calls.get(), 0);
    assert!(stall_files(&dir).is_empty());
    // Two minutes: the file.
    let written = pass(&mut watch, &clock, LOOK);
    assert_eq!(written.len(), 1);
    // An hour more of the same stall: still that one file, one sample.
    assert!(pass(&mut watch, &clock, Duration::from_secs(3600)).is_empty());
    assert_eq!(sampler.calls.get(), 1);
    let pid = std::process::id();
    assert_eq!(stall_files(&dir), vec![format!("stall-1794851896-{pid}.txt")]);
    assert_eq!(
        std::fs::read_to_string(&written[0]).unwrap(),
        format!(
            "Exponential stall sample\nrelease: exponential-desktop@0.14.177\npid: {pid}\nat: 1794851896 (2026-11-16 17:58:16 UTC)\nstalled: main thread 120 s, heartbeat 120 s\n---\nfake stacks of {pid} #1\nmain: __psynch_mutexwait\n"
        )
    );
    // The hook saw the finished file.
    let hooked = hooked.lock().unwrap();
    assert_eq!(hooked.len(), 1);
    assert_eq!((hooked[0].path.clone(), hooked[0].stalled.as_str()), (written[0].clone(), "main thread 120 s, heartbeat 120 s"));
    assert_eq!(hooked[0].sample(11), "fake stacks");
    // Not one logger call on the way.
    assert_eq!(LOGGED.load(Ordering::SeqCst), logged_before);
}

#[test]
fn a_recovered_stall_re_arms_and_a_resting_source_is_no_stall() {
    let dir = dir("re-arm");
    let (clock, sampler, beats) = (FakeClock::new(), FakeSampler::new(""), beats());
    let mut watch = Watch::new(dir.clone(), "r".into(), clock.clone(), sampler.clone(), beats);
    beats.beat(Source::Main);
    beats.beat(Source::Heartbeat);
    assert!(pass(&mut watch, &clock, LOOK).is_empty());

    // The heartbeat alone stalls (the main thread goes on).
    let mut written = Vec::new();
    for _ in 0..(STALL.as_secs() / LOOK.as_secs()) {
        beats.beat(Source::Main);
        written.extend(pass(&mut watch, &clock, LOOK));
    }
    assert_eq!(written.len(), 1);
    assert!(std::fs::read_to_string(&written[0]).unwrap().contains("\nstalled: heartbeat 120 s, main thread 0 s\n"));

    // It recovers, then stalls again: a second file.
    beats.beat(Source::Heartbeat);
    beats.beat(Source::Main);
    assert!(pass(&mut watch, &clock, LOOK).is_empty());
    assert_eq!(pass(&mut watch, &clock, STALL).len(), 1);
    assert_eq!((sampler.calls.get(), stall_files(&dir).len()), (2, 2));

    // Its loop ended on purpose (signed out): never a stall. The main
    // thread's own stall is already on record.
    beats.rest(Source::Heartbeat);
    assert!(pass(&mut watch, &clock, STALL * 4).is_empty());
    // Back and stuck: watched again.
    beats.beat(Source::Main);
    beats.beat(Source::Heartbeat);
    assert!(pass(&mut watch, &clock, LOOK).is_empty());
    beats.rest(Source::Main);
    assert_eq!(pass(&mut watch, &clock, STALL).len(), 1);
    assert_eq!(sampler.calls.get(), 3);
}

/// The watchdog did not run itself (the machine slept, the process was
/// stopped): that stretch is no stall of the others.
#[test]
fn a_look_that_came_late_counts_as_one_step() {
    let dir = dir("late");
    let (clock, sampler, beats) = (FakeClock::new(), FakeSampler::new(""), beats());
    let mut watch = Watch::new(dir, "r".into(), clock.clone(), sampler.clone(), beats);
    beats.beat(Source::Main);
    assert!(watch.look().is_none());
    clock.advance(Duration::from_secs(8 * 3600));
    assert!(watch.look().is_none());
    assert_eq!(sampler.calls.get(), 0);
}

#[test]
fn old_files_are_pruned_and_a_long_sample_is_cut() {
    let dir = dir("prune");
    let (clock, sampler, beats) = (FakeClock::new(), FakeSampler::new(&"frame\n".repeat(MAX_BYTES as usize / 4)), beats());
    let mut watch = Watch::new(dir.clone(), "r".into(), clock.clone(), sampler.clone(), beats);
    let mut all = Vec::new();
    for _ in 0..KEEP + 2 {
        beats.beat(Source::Main);
        assert!(pass(&mut watch, &clock, LOOK).is_empty());
        all.extend(pass(&mut watch, &clock, STALL));
    }
    assert_eq!(all.len(), KEEP + 2);
    // The newest KEEP are left.
    let kept: Vec<String> = all[2..].iter().map(|path| path.file_name().unwrap().to_string_lossy().into_owned()).collect();
    assert_eq!(stall_files(&dir), kept);
    for path in &all[2..] {
        let text = std::fs::read_to_string(path).unwrap();
        assert!(text.len() as u64 <= MAX_BYTES + 64, "{}", text.len());
        assert!(text.ends_with(&format!("\n[cut at {MAX_BYTES} bytes]\n")));
    }
    // Nothing else is left behind.
    assert_eq!(std::fs::read_dir(&dir).unwrap().count(), KEEP);
}

#[test]
fn the_next_start_reports_each_file_once() {
    let dir = dir("report");
    assert!(take_unreported(&dir).is_empty(), "no crashes dir yet");
    let (clock, sampler, beats) = (FakeClock::new(), FakeSampler::new("stacks\n"), beats());
    let mut watch = Watch::new(dir.clone(), "exponential-desktop@0.14.177".into(), clock.clone(), sampler, beats);
    beats.beat(Source::Main);
    assert!(pass(&mut watch, &clock, LOOK).is_empty());
    let first = pass(&mut watch, &clock, STALL);

    let reports = take_unreported(&dir);
    assert_eq!(reports.len(), 1);
    let report = &reports[0];
    assert_eq!(report.path, first[0]);
    assert_eq!((report.release.as_str(), report.at, report.at_utc.as_str(), report.stalled.as_str()), ("exponential-desktop@0.14.177", 1_794_851_241, "2026-11-16 17:47:21", "main thread 120 s"));
    assert!(report.sample(1024).ends_with("stacks\n"));
    // The start after that has nothing new; the file stays.
    assert!(take_unreported(&dir).is_empty());
    assert!(first[0].exists());

    // A later stall is new again, and only it.
    beats.beat(Source::Main);
    assert!(pass(&mut watch, &clock, LOOK).is_empty());
    let second = pass(&mut watch, &clock, STALL);
    let reports = take_unreported(&dir);
    assert_eq!(reports.iter().map(|report| report.path.clone()).collect::<Vec<_>>(), second);
    assert!(take_unreported(&dir).is_empty());
}

/// The real thing: a thread of this process blocked for good, and the
/// platform's sampler pointed at this process. macOS: `sample`; Linux: the
/// task table, plus `eu-stack`/`gdb` where one is installed and allowed.
#[test]
fn the_platform_sampler_names_a_thread_that_really_is_stuck() {
    #[inline(never)]
    fn stuck_in_here(gate: Arc<Mutex<()>>) {
        let _never = gate.lock();
    }
    let gate = Arc::new(Mutex::new(()));
    let held = gate.lock().unwrap();
    let blocked = gate.clone();
    std::thread::Builder::new().name("stuck-thread".into()).spawn(move || stuck_in_here(blocked)).unwrap();
    // Let it reach the lock.
    std::thread::sleep(Duration::from_millis(300));

    let dir = dir("real");
    let beats = beats();
    let clock = FakeClock::new();
    let mut watch = Watch::new(dir.clone(), "stall-watch-test".into(), clock.clone(), PlatformSampler, beats);
    beats.beat(Source::Main);
    assert!(watch.look().is_none());
    let mut written = Vec::new();
    for _ in 0..(STALL.as_secs() / LOOK.as_secs()) {
        clock.advance(LOOK);
        written.extend(watch.look());
    }
    assert_eq!(written.len(), 1);
    let text = std::fs::read_to_string(&written[0]).unwrap();
    eprintln!("--- {} ({} bytes) ---\n{}", written[0].display(), text.len(), text.chars().take(6000).collect::<String>());
    assert!(text.starts_with("Exponential stall sample\nrelease: stall-watch-test\n"), "{text}");
    assert!(text.contains("\nsampler: "), "{text}");
    if cfg!(any(target_os = "linux", target_os = "macos")) {
        assert!(text.contains("stuck-thread"), "the stuck thread is in the file");
    }
    if cfg!(target_os = "macos") {
        assert!(text.contains("Call graph:"), "sample's call graph");
        assert!(text.contains("stuck_in_here"), "the frame it is stuck in");
        assert!(!dir.join(written[0].file_name().unwrap()).with_extension("sample.tmp").exists(), "sample's own file is gone");
    }
    assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1, "one file, nothing beside it");
    drop(held);
}
