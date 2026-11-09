//! Where the lag comes from (#LOGS-2): the compositor's frame times and
//! main-loop hangs.
//!
//! - Every drawn frame goes through [`record_frame`]. One at or over
//!   [`LONG_FRAME`] is a `ui.frame.long` transaction; once a minute a
//!   `ui.frames` transaction carries the minute's p50/p95/p99/max.
//! - [`LoopWatch`] ticks on the main loop every [`TICK`]; a tick late by
//!   [`HANG`] or more is a `ui.hang` transaction for the time the loop was
//!   blocked, and a loop stuck for [`STUCK`] (a deadlock never ticks again)
//!   is a warning event from the watchdog thread.
//! - `EXP_FRAME_STATS=1` prints a summary to stderr every 5 s, Sentry or not.
//!
//! The transactions are sampled like every other (`tracesSampleRate`), and
//! at most one long-frame or hang span goes out a second.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime};

use sentry::protocol::Value;

/// A frame this long is a long frame (two refreshes at 60 Hz).
pub const LONG_FRAME: Duration = Duration::from_millis(33);
/// A main loop blocked this long is a hang.
pub const HANG: Duration = Duration::from_millis(100);
/// The main loop's heartbeat.
pub const TICK: Duration = Duration::from_millis(50);
/// No heartbeat for this long: the watchdog reports the loop stuck.
pub const STUCK: Duration = Duration::from_secs(2);

const PRINT_EVERY: Duration = Duration::from_secs(5);
const SUMMARY_EVERY: Duration = Duration::from_secs(60);
const SPAN_GAP: Duration = Duration::from_secs(1);

static STATS: AtomicBool = AtomicBool::new(false);
static SENTRY: AtomicBool = AtomicBool::new(false);
static STATE: Mutex<Option<State>> = Mutex::new(None);

pub(crate) fn init(sentry_on: bool) {
    STATS.store(std::env::var("EXP_FRAME_STATS").is_ok_and(|v| v.trim() == "1"), Ordering::Relaxed);
    SENTRY.store(sentry_on, Ordering::Relaxed);
}

/// Whether anyone listens (stderr summary or Sentry).
pub fn active() -> bool {
    STATS.load(Ordering::Relaxed) || SENTRY.load(Ordering::Relaxed)
}

/// Frame times in 0.5 ms buckets up to 100 ms (the last bucket holds
/// everything longer).
#[derive(Clone)]
pub struct Histogram {
    buckets: [u32; Self::BUCKETS],
    pub count: u32,
    pub sum_ms: f64,
    pub max_ms: f32,
    pub long: u32,
    pub hangs: u32,
    pub max_hang_ms: f32,
}

impl Default for Histogram {
    fn default() -> Self {
        Self { buckets: [0; Self::BUCKETS], count: 0, sum_ms: 0.0, max_ms: 0.0, long: 0, hangs: 0, max_hang_ms: 0.0 }
    }
}

impl Histogram {
    const BUCKETS: usize = 201;
    const BUCKET_MS: f32 = 0.5;

    pub fn frame(&mut self, ms: f32) {
        let i = ((ms / Self::BUCKET_MS) as usize).min(Self::BUCKETS - 1);
        self.buckets[i] += 1;
        self.count += 1;
        self.sum_ms += ms as f64;
        self.max_ms = self.max_ms.max(ms);
        if ms >= LONG_FRAME.as_secs_f32() * 1000.0 {
            self.long += 1;
        }
    }

    pub fn hang(&mut self, ms: f32) {
        self.hangs += 1;
        self.max_hang_ms = self.max_hang_ms.max(ms);
    }

    /// The upper edge of the bucket holding the `p`-th percentile (the
    /// maximum for the last bucket); 0 with no frames.
    pub fn percentile(&self, p: f32) -> f32 {
        if self.count == 0 {
            return 0.0;
        }
        let rank = ((p as f64 * self.count as f64) / 100.0).ceil().max(1.0) as u32;
        let mut seen = 0;
        for (i, n) in self.buckets.iter().enumerate() {
            seen += n;
            if seen >= rank {
                return if i == Self::BUCKETS - 1 { self.max_ms } else { ((i + 1) as f32 * Self::BUCKET_MS).min(self.max_ms) };
            }
        }
        self.max_ms
    }

    pub fn avg_ms(&self) -> f32 {
        if self.count == 0 {
            0.0
        } else {
            (self.sum_ms / self.count as f64) as f32
        }
    }

    /// `EXP_FRAME_STATS`'s line.
    pub fn summary(&self, over: Duration) -> String {
        let secs = over.as_secs_f32().max(0.001);
        format!(
            "frame stats {secs:.1}s: {} frames ({:.1}/s) avg {:.2} ms, p50 {:.1}, p95 {:.1}, p99 {:.1}, max {:.1} ms; {} long (>= {} ms); {} hangs (max {:.0} ms)",
            self.count,
            self.count as f32 / secs,
            self.avg_ms(),
            self.percentile(50.0),
            self.percentile(95.0),
            self.percentile(99.0),
            self.max_ms,
            self.long,
            LONG_FRAME.as_millis(),
            self.hangs,
            self.max_hang_ms,
        )
    }
}

struct State {
    printed: Instant,
    print: Histogram,
    summarised: Instant,
    summary: Histogram,
    last_span: Option<Instant>,
}

impl State {
    fn new(now: Instant) -> Self {
        Self { printed: now, print: Histogram::default(), summarised: now, summary: Histogram::default(), last_span: None }
    }

    /// One span a second at most.
    fn span_allowed(&mut self, now: Instant) -> bool {
        if self.last_span.is_some_and(|t| now.duration_since(t) < SPAN_GAP) {
            return false;
        }
        self.last_span = Some(now);
        true
    }
}

fn with_state<R>(f: impl FnOnce(&mut State, Instant) -> R) -> R {
    let now = Instant::now();
    let mut guard = STATE.lock().unwrap_or_else(|p| p.into_inner());
    f(guard.get_or_insert_with(|| State::new(now)), now)
}

/// A drawn frame on `output` that took `took` (render through submit).
pub fn record_frame(output: &str, took: Duration) {
    let stats = STATS.load(Ordering::Relaxed);
    let sentry = SENTRY.load(Ordering::Relaxed);
    if !stats && !sentry {
        return;
    }
    let ms = took.as_secs_f32() * 1000.0;
    let long = took >= LONG_FRAME;
    let (span, summary) = with_state(|s, now| {
        if stats {
            s.print.frame(ms);
        }
        if sentry {
            s.summary.frame(ms);
        }
        let span = sentry && long && s.span_allowed(now);
        (span, flush_windows(s, now, stats, sentry))
    });
    if span {
        transaction("long frame", "ui.frame.long", took, &[("output", Value::from(output)), ("frame_ms", Value::from(ms as f64))]);
    }
    if let Some((over, h)) = summary {
        send_summary(over, &h);
    }
}

/// Prints the stderr window when due; hands back the Sentry minute when due.
fn flush_windows(s: &mut State, now: Instant, stats: bool, sentry: bool) -> Option<(Duration, Histogram)> {
    if stats && now.duration_since(s.printed) >= PRINT_EVERY {
        eprintln!("exponential-desktop: {}", s.print.summary(now.duration_since(s.printed)));
        s.print = Histogram::default();
        s.printed = now;
    }
    if sentry && now.duration_since(s.summarised) >= SUMMARY_EVERY {
        let over = now.duration_since(s.summarised);
        let h = std::mem::take(&mut s.summary);
        s.summarised = now;
        if h.count > 0 || h.hangs > 0 {
            return Some((over, h));
        }
    }
    None
}

fn send_summary(over: Duration, h: &Histogram) {
    transaction(
        "frame time",
        "ui.frames",
        over,
        &[
            ("frames", Value::from(h.count)),
            ("fps", Value::from((h.count as f64) / over.as_secs_f64().max(0.001))),
            ("avg_ms", Value::from(h.avg_ms() as f64)),
            ("p50_ms", Value::from(h.percentile(50.0) as f64)),
            ("p95_ms", Value::from(h.percentile(95.0) as f64)),
            ("p99_ms", Value::from(h.percentile(99.0) as f64)),
            ("max_ms", Value::from(h.max_ms as f64)),
            ("long_frames", Value::from(h.long)),
            ("hangs", Value::from(h.hangs)),
            ("max_hang_ms", Value::from(h.max_hang_ms as f64)),
        ],
    );
}

/// A finished transaction that ran for `took` up to now.
fn transaction(name: &str, op: &str, took: Duration, data: &[(&str, Value)]) {
    let end = SystemTime::now();
    let start = end.checked_sub(took).unwrap_or(end);
    let tx = sentry::start_transaction_with_timestamp(sentry::TransactionContext::new(name, op), start);
    for (k, v) in data {
        tx.set_data(k, v.clone());
    }
    tx.finish_with_timestamp(end);
}

/// The main loop's heartbeat: [`LoopWatch::tick`] from a timer source every
/// [`TICK`]. A late tick is a hang; with Sentry on, a watchdog thread
/// reports a loop that stops ticking altogether.
pub struct LoopWatch {
    last: Instant,
    origin: Instant,
    /// Milliseconds since `origin` of the last tick (the watchdog reads it).
    beat: Arc<AtomicU64>,
}

impl LoopWatch {
    /// `None` when nobody listens (no timer, no thread).
    pub fn start() -> Option<Self> {
        if !active() {
            return None;
        }
        let origin = Instant::now();
        let beat = Arc::new(AtomicU64::new(0));
        if SENTRY.load(Ordering::Relaxed) {
            let watched = beat.clone();
            let _ = std::thread::Builder::new().name("ede-loop-watchdog".into()).spawn(move || {
                let mut reported = false;
                loop {
                    std::thread::sleep(Duration::from_millis(500));
                    let age = origin.elapsed().saturating_sub(Duration::from_millis(watched.load(Ordering::Relaxed)));
                    if age >= STUCK && !reported {
                        reported = true;
                        sentry::capture_message(&format!("main loop stuck for over {} s", STUCK.as_secs()), sentry::Level::Warning);
                    } else if age < STUCK {
                        reported = false;
                    }
                }
            });
        }
        Some(Self { last: origin, origin, beat })
    }

    pub fn tick(&mut self) {
        let now = Instant::now();
        let late = now.duration_since(self.last).saturating_sub(TICK);
        self.last = now;
        self.beat.store(now.duration_since(self.origin).as_millis() as u64, Ordering::Relaxed);
        let stats = STATS.load(Ordering::Relaxed);
        let sentry = SENTRY.load(Ordering::Relaxed);
        let hang = late >= HANG;
        let ms = late.as_secs_f32() * 1000.0;
        let (span, summary) = with_state(|s, now| {
            if hang {
                if stats {
                    s.print.hang(ms);
                }
                if sentry {
                    s.summary.hang(ms);
                }
            }
            let span = sentry && hang && s.span_allowed(now);
            (span, flush_windows(s, now, stats, sentry))
        });
        if hang && late >= Duration::from_secs(1) {
            tracing::warn!(target: "ede_hang", blocked_ms = ms as u64, "main loop blocked");
        }
        if span {
            transaction("main loop hang", "ui.hang", late, &[("blocked_ms", Value::from(ms as f64))]);
        }
        if let Some((over, h)) = summary {
            send_summary(over, &h);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percentiles_from_the_buckets() {
        let mut h = Histogram::default();
        assert_eq!(h.percentile(50.0), 0.0);
        for _ in 0..98 {
            h.frame(2.2);
        }
        h.frame(40.0);
        h.frame(250.0);
        assert_eq!(h.count, 100);
        assert_eq!(h.percentile(50.0), 2.5);
        assert_eq!(h.percentile(99.0), 40.5);
        assert_eq!(h.percentile(100.0), 250.0);
        assert_eq!(h.max_ms, 250.0);
        assert_eq!(h.long, 2);
        assert!((h.avg_ms() - (98.0 * 2.2 + 290.0) / 100.0).abs() < 0.01);
    }

    #[test]
    fn the_summary_line() {
        let mut h = Histogram::default();
        h.frame(1.0);
        h.hang(150.0);
        let line = h.summary(Duration::from_secs(5));
        assert!(line.starts_with("frame stats 5.0s: 1 frames (0.2/s) avg 1.00 ms"), "{line}");
        assert!(line.ends_with("1 hangs (max 150 ms)"), "{line}");
    }

    #[test]
    fn one_span_a_second() {
        let t = Instant::now();
        let mut s = State::new(t);
        assert!(s.span_allowed(t));
        assert!(!s.span_allowed(t + Duration::from_millis(500)));
        assert!(s.span_allowed(t + Duration::from_millis(1500)));
    }
}
