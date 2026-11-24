//! #EXP-2091: where the threads are when the process stops.
//!
//! On 2026-11-16 the desktop app stopped logging and stayed alive for eight
//! hours with every thread blocked; nothing recorded where. The hang
//! watchdog (`observability::lag`) reports through the logger and Sentry, so
//! it is silent in exactly that case.
//!
//! Here: the watched loops [`beat`] (the main thread, the device heartbeat);
//! a watchdog thread looks every [`LOOK`] and, once a source has not beaten
//! for [`STALL`], writes the thread stacks to `{dir}/stall-<unix>-<pid>.txt`:
//! a header, then the platform [`Sampler`]'s output (macOS `sample`, Linux
//! the task table plus `eu-stack` or `gdb` when present). Once per stall (a
//! beat re-arms), at most [`MAX_BYTES`] a file, the newest [`KEEP`] kept.
//!
//! The watchdog thread takes no lock another thread holds: atomics, file
//! calls and a `posix_spawn` of its own. No logger, no Sentry, no
//! `std::process`, no `std::env`. The next start reports the file
//! ([`take_unreported`]).

use std::fs::File;
use std::io::{Read as _, Seek as _, SeekFrom, Write as _};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

mod sampler;

pub use sampler::PlatformSampler;

/// A source this long without a beat has stalled.
pub const STALL: Duration = Duration::from_secs(120);
/// The watchdog's period.
pub const LOOK: Duration = Duration::from_secs(5);
/// One look counts at most this much: a watchdog that did not run itself
/// (the machine slept, the process was stopped) saw no stall.
pub const MAX_STEP: Duration = Duration::from_secs(15);
/// Stall files kept (the newest).
pub const KEEP: usize = 3;
/// A stall file's size cap.
pub const MAX_BYTES: u64 = 2 * 1024 * 1024;

const PREFIX: &str = "stall-";
const SUFFIX: &str = ".txt";
/// The newest file name a start reported.
const SEEN: &str = "stall.seen";
const RULE: &str = "---";

/// What is watched.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    /// The UI thread's own timer.
    Main,
    /// The device heartbeat loop.
    Heartbeat,
}

const SOURCES: [Source; 2] = [Source::Main, Source::Heartbeat];

impl Source {
    pub fn label(self) -> &'static str {
        match self {
            Source::Main => "main thread",
            Source::Heartbeat => "heartbeat",
        }
    }
}

/// The sources' beat counters; 0 = not watched (never beat, or resting).
pub struct Beats([AtomicU64; 2]);

impl Beats {
    pub const fn new() -> Self {
        Self([AtomicU64::new(0), AtomicU64::new(0)])
    }

    /// The source is alive (and watched from now on).
    pub fn beat(&self, source: Source) {
        // Never back to 0 by counting.
        if self.0[source as usize].fetch_add(1, Ordering::AcqRel) == u64::MAX {
            self.0[source as usize].store(1, Ordering::Release);
        }
    }

    /// The source stopped on purpose (its loop ended): not watched until
    /// its next beat.
    pub fn rest(&self, source: Source) {
        self.0[source as usize].store(0, Ordering::Release);
    }
}

impl Default for Beats {
    fn default() -> Self {
        Self::new()
    }
}

static BEATS: Beats = Beats::new();

/// The process's own counters ([`beat`], [`rest`]).
pub fn beats() -> &'static Beats {
    &BEATS
}

/// One atomic add: free to call from any loop, watched or not.
pub fn beat(source: Source) {
    BEATS.beat(source);
}

pub fn rest(source: Source) {
    BEATS.rest(source);
}

/// The watchdog's time: monotonic for the stall, wall for the file.
pub trait Clock {
    /// Monotonic, from any origin.
    fn now(&self) -> Duration;
    /// Seconds since the Unix epoch.
    fn unix(&self) -> u64;
}

pub struct SystemClock(std::time::Instant);

impl SystemClock {
    pub fn new() -> Self {
        Self(std::time::Instant::now())
    }
}

impl Default for SystemClock {
    fn default() -> Self {
        Self::new()
    }
}

impl Clock for SystemClock {
    fn now(&self) -> Duration {
        self.0.elapsed()
    }

    fn unix(&self) -> u64 {
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |since| since.as_secs())
    }
}

/// Writes where the threads of `pid` are to `out` (appending), and says in
/// it what it used. `path` is the file `out` is (for a tool that wants a
/// file of its own beside it).
pub trait Sampler {
    fn sample(&mut self, pid: u32, path: &Path, out: &mut File);
}

#[derive(Default, Clone, Copy)]
struct Seen {
    count: u64,
    still: Duration,
    fired: bool,
}

/// Which sources stood still, for how long.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stall(pub Vec<(Source, Duration)>);

impl Stall {
    /// `main thread 120 s, heartbeat 118 s`.
    pub fn line(&self) -> String {
        self.0.iter().map(|(source, still)| format!("{} {} s", source.label(), still.as_secs())).collect::<Vec<_>>().join(", ")
    }
}

/// The stall rule alone (no clock, no file).
#[derive(Default)]
pub struct Detector {
    seen: [Seen; 2],
    last: Option<Duration>,
}

impl Detector {
    /// One look at `now`: a stall to record, once per stall. While one
    /// source's stall is on record, another reaching [`STALL`] is the same
    /// stall (the heartbeat loop runs on the main thread).
    pub fn look(&mut self, now: Duration, beats: &Beats) -> Option<Stall> {
        let step = self.last.map_or(Duration::ZERO, |last| now.saturating_sub(last)).min(MAX_STEP);
        self.last = Some(now);
        let on_record = self.seen.iter().any(|seen| seen.fired);
        let mut reached = false;
        for (seen, beat) in self.seen.iter_mut().zip(&beats.0) {
            let count = beat.load(Ordering::Acquire);
            if count == 0 || count != seen.count {
                *seen = Seen { count, ..Default::default() };
                continue;
            }
            seen.still += step;
            if seen.still >= STALL && !seen.fired {
                seen.fired = true;
                reached = true;
            }
        }
        if !reached || on_record {
            return None;
        }
        // Every watched source, the stalled first: what else stood still.
        let mut all: Vec<(Source, Duration)> = SOURCES.iter().zip(&self.seen).filter(|(_, seen)| seen.count != 0).map(|(source, seen)| (*source, seen.still)).collect();
        all.sort_by(|a, b| b.1.cmp(&a.1));
        Some(Stall(all))
    }
}

/// Runs after a stall's file is complete, on the watchdog thread: the same
/// rules (no logger, no shared lock).
pub type OnStall = Box<dyn FnMut(&Report) + Send>;

/// The watchdog: [`Detector`] + the file.
pub struct Watch<C, S> {
    dir: PathBuf,
    release: String,
    clock: C,
    sampler: S,
    beats: &'static Beats,
    detector: Detector,
    on_stall: Option<OnStall>,
}

impl<C: Clock, S: Sampler> Watch<C, S> {
    /// `dir` = the data dir's `crashes/`; `release` names the build in the
    /// file.
    pub fn new(dir: PathBuf, release: String, clock: C, sampler: S, beats: &'static Beats) -> Self {
        Self { dir, release, clock, sampler, beats, detector: Detector::default(), on_stall: None }
    }

    pub fn on_stall(mut self, hook: OnStall) -> Self {
        self.on_stall = Some(hook);
        self
    }

    /// One look; the file a stall was written to.
    pub fn look(&mut self) -> Option<PathBuf> {
        let stall = self.detector.look(self.clock.now(), self.beats)?;
        let path = self.write(&stall).ok()?;
        if let (Some(hook), Some(report)) = (self.on_stall.as_mut(), read_report(&path)) {
            hook(&report);
        }
        Some(path)
    }

    /// The watchdog thread's body.
    pub fn run(mut self) -> ! {
        loop {
            std::thread::sleep(LOOK);
            self.look();
        }
    }

    fn write(&mut self, stall: &Stall) -> std::io::Result<PathBuf> {
        std::fs::create_dir_all(&self.dir)?;
        let pid = std::process::id();
        let at = self.clock.unix();
        let path = self.dir.join(format!("{PREFIX}{at:010}-{pid}{SUFFIX}"));
        let _ = std::fs::remove_file(&path);
        // Appending: the sampler's child writes to the same file.
        let mut file = std::fs::OpenOptions::new().create(true).append(true).read(true).open(&path)?;
        write!(
            file,
            "Exponential stall sample\nrelease: {}\npid: {pid}\nat: {at} ({} UTC)\nstalled: {}\n{RULE}\n",
            self.release,
            utc(at),
            stall.line()
        )?;
        self.sampler.sample(pid, &path, &mut file);
        if file.metadata().is_ok_and(|meta| meta.len() > MAX_BYTES) {
            let _ = file.set_len(MAX_BYTES);
            let _ = write!(file, "\n[cut at {MAX_BYTES} bytes]\n");
        }
        drop(file);
        prune(&self.dir);
        Ok(path)
    }
}

/// The stall files in `dir`, oldest first (the name carries the time).
fn files(dir: &Path) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
        .map(|entries| entries.flatten().map(|entry| entry.path()).filter(|path| name(path).is_some_and(|name| name.starts_with(PREFIX) && name.ends_with(SUFFIX))).collect())
        .unwrap_or_default();
    files.sort();
    files
}

fn name(path: &Path) -> Option<&str> {
    path.file_name().and_then(|name| name.to_str())
}

fn prune(dir: &Path) {
    let files = files(dir);
    for path in &files[..files.len().saturating_sub(KEEP)] {
        let _ = std::fs::remove_file(path);
    }
}

/// A stall file, as its header tells it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Report {
    pub path: PathBuf,
    pub release: String,
    /// Seconds since the Unix epoch.
    pub at: u64,
    /// `2026-11-16 17:47:16`, UTC.
    pub at_utc: String,
    /// [`Stall::line`].
    pub stalled: String,
}

impl Report {
    /// The sampler's part of the file, at most `max` bytes of it.
    pub fn sample(&self, max: usize) -> String {
        let Ok(mut file) = File::open(&self.path) else { return String::new() };
        let mut head = vec![0; 1024];
        let read = file.read(&mut head).unwrap_or(0);
        let rule = format!("\n{RULE}\n");
        let start = String::from_utf8_lossy(&head[..read]).find(&rule).map_or(0, |at| at + rule.len());
        let mut body = Vec::new();
        if file.seek(SeekFrom::Start(start as u64)).is_ok() {
            let _ = file.take(max as u64).read_to_end(&mut body);
        }
        String::from_utf8_lossy(&body).into_owned()
    }
}

fn read_report(path: &Path) -> Option<Report> {
    let mut head = vec![0; 1024];
    let read = File::open(path).ok()?.read(&mut head).ok()?;
    let head = String::from_utf8_lossy(&head[..read]).into_owned();
    let field = |key: &str| head.lines().take_while(|line| *line != RULE).find_map(|line| line.strip_prefix(key)?.strip_prefix(": ")).map(str::to_string);
    let at = field("at")?.split(' ').next()?.parse().ok()?;
    Some(Report { path: path.to_path_buf(), release: field("release").unwrap_or_default(), at, at_utc: utc(at), stalled: field("stalled")? })
}

/// The next start's half: the stall files no start reported yet, oldest
/// first; they are reported now (a second call returns none of them).
pub fn take_unreported(dir: &Path) -> Vec<Report> {
    prune(dir);
    let seen = std::fs::read_to_string(dir.join(SEEN)).unwrap_or_default();
    let new: Vec<PathBuf> = files(dir).into_iter().filter(|path| name(path).is_some_and(|name| name > seen.trim())).collect();
    if let Some(newest) = new.last().and_then(|path| name(path)) {
        let _ = std::fs::write(dir.join(SEEN), newest);
    }
    new.iter().filter_map(|path| read_report(path)).collect()
}

/// `2026-11-16 17:47:16` (UTC) of a Unix time; no locale, no lock.
fn utc(unix: u64) -> String {
    let (days, rest) = (unix / 86_400, unix % 86_400);
    // Days to a civil date (Howard Hinnant's algorithm).
    let z = days as i64 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02} {:02}:{:02}:{:02}", rest / 3600, rest % 3600 / 60, rest % 60)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utc_reads_like_the_log() {
        assert_eq!(utc(0), "1970-01-01 00:00:00");
        // The freeze of #EXP-1892.
        assert_eq!(utc(1_794_851_116), "2026-11-16 17:45:16");
        assert_eq!(utc(1_709_251_199), "2024-02-29 23:59:59");
    }
}
