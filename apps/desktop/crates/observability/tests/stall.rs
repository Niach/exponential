//! #EXP-2091: a stall's way out of the process. The watchdog writes the
//! thread stacks and the stall's event with no logger call; the next start
//! names the file in the log (once) and the next client sends the event
//! through the crash reports' path.

use std::cell::Cell;
use std::fs::File;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Mutex;
use std::time::Duration;

use observability::stall::{Beats, Clock, Sampler, Source, Watch, LOOK, STALL};

/// Every `log` record of this test process.
static LOGGED: Mutex<Vec<(log::Level, String)>> = Mutex::new(Vec::new());
/// The tests share the logger.
static ONE_AT_A_TIME: Mutex<()> = Mutex::new(());

struct Recorder;

impl log::Log for Recorder {
    fn enabled(&self, _: &log::Metadata) -> bool {
        true
    }
    fn log(&self, record: &log::Record) {
        LOGGED.lock().unwrap().push((record.level(), record.args().to_string()));
    }
    fn flush(&self) {}
}

fn logged() -> Vec<(log::Level, String)> {
    std::mem::take(&mut *LOGGED.lock().unwrap())
}

#[derive(Clone)]
struct FakeClock(Rc<Cell<u64>>);

impl Clock for FakeClock {
    fn now(&self) -> Duration {
        Duration::from_secs(self.0.get())
    }
    fn unix(&self) -> u64 {
        // 2026-11-16 17:45:16 UTC at 0.
        1_794_851_116 + self.0.get()
    }
}

struct FakeSampler;

impl Sampler for FakeSampler {
    fn sample(&mut self, _pid: u32, _path: &Path, out: &mut File) {
        write!(out, "Thread_1 main: write (in libsystem_kernel.dylib)\n/Users/someone/Library/x Bearer abc.def\n").unwrap();
    }
}

fn data_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("observability-stall-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// The main thread stalls for two minutes under a watchdog wired like
/// [`observability::watch_stalls`] wires it; the stall file.
fn stall(data_dir: &Path) -> PathBuf {
    let beats: &'static Beats = Box::leak(Box::new(Beats::new()));
    let clock = FakeClock(Rc::new(Cell::new(0)));
    let hook_dir = data_dir.to_path_buf();
    let mut watch = Watch::new(data_dir.join("crashes"), "exponential-desktop@0.14.177".into(), clock.clone(), FakeSampler, beats)
        .on_stall(Box::new(move |report| observability::keep_stall_event(&hook_dir, false, Some("/Users/someone"), report)));
    beats.beat(Source::Main);
    let mut written = Vec::new();
    for _ in 0..=(STALL.as_secs() / LOOK.as_secs()) {
        written.extend(watch.look());
        clock.0.set(clock.0.get() + LOOK.as_secs());
    }
    assert_eq!(written.len(), 1);
    written.remove(0)
}

fn events(crashes: &Path) -> Vec<PathBuf> {
    std::fs::read_dir(crashes).unwrap().flatten().map(|entry| entry.path()).filter(|path| path.extension().is_some_and(|ext| ext == "panic")).collect()
}

#[test]
fn a_stall_is_a_file_and_an_event_without_the_logger_and_the_next_start_reports_both() {
    let _one = ONE_AT_A_TIME.lock().unwrap_or_else(|e| e.into_inner());
    let _ = log::set_logger(&Recorder);
    log::set_max_level(log::LevelFilter::Trace);
    let data_dir = data_dir("report");
    let crashes = data_dir.join("crashes");
    logged();

    let file = stall(&data_dir);
    assert_eq!(logged(), vec![], "the stall path makes no logger call");
    assert!(std::fs::read_to_string(&file).unwrap().contains("\nstalled: main thread 120 s\n---\nThread_1 main: write"));
    assert_eq!(events(&crashes).len(), 1, "the event waits beside the panics");

    // The next start: one log line naming the file, once.
    assert_eq!(observability::report_stalls(&crashes), 1);
    assert_eq!(
        logged(),
        vec![(
            log::Level::Warn,
            format!("exponential-desktop@0.14.177 stalled at 2026-11-16 17:47:16 UTC (main thread 120 s): its thread stacks are in {}", file.display())
        )]
    );
    assert_eq!(observability::report_stalls(&crashes), 0);
    assert_eq!(logged(), vec![]);

    // The next client: the crash reports' path hands the event over, and
    // deletes it once the tunnel took it. The file stays.
    let mut sent = Vec::new();
    assert_eq!(crash_monitor::send_pending(&crashes, |event| sent.push(event), || true), 1);
    let event = &sent[0];
    assert_eq!(event.message.as_deref(), Some("Stalled: main thread 120 s"));
    assert_eq!(event.level, sentry::Level::Error);
    assert_eq!(event.release.as_deref(), Some("exponential-desktop@0.14.177"));
    assert_eq!(event.fingerprint.iter().map(|part| &**part).collect::<Vec<&str>>(), ["stall", "main thread"]);
    assert_eq!(event.extra["file"], &*file.file_name().unwrap().to_string_lossy());
    assert_eq!(event.extra["sample"], "Thread_1 main: write (in libsystem_kernel.dylib)\n~/Library/x Bearer abc.def\n");
    // What the client's `before_send` makes of it.
    assert_eq!(observability::scrub::scrub_event(event.clone()).extra["sample"], "Thread_1 main: write (in libsystem_kernel.dylib)\n~/Library/x Bearer [Filtered]\n");
    assert!(events(&crashes).is_empty());
    assert_eq!(crash_monitor::send_pending(&crashes, |_| panic!("sent twice"), || true), 0);
    assert!(file.exists());
}

#[test]
fn with_diagnostics_off_the_file_is_written_and_no_event() {
    let _one = ONE_AT_A_TIME.lock().unwrap_or_else(|e| e.into_inner());
    let data_dir = data_dir("off");
    std::fs::write(data_dir.join("diagnostics.json"), r#"{"sendDiagnostics":false}"#).unwrap();
    let file = stall(&data_dir);
    assert!(file.exists());
    assert!(events(&data_dir.join("crashes")).is_empty());
}
