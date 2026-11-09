//! #LOGS-2: where the IDE's lag comes from.
//!
//! Three sources, all on the main (UI) thread:
//! - **frames**: gpui's own per-draw timing (`Window::draw` start/end, fed by
//!   the app through [`record_frame`]); a frame over 16 ms is long, over 33 ms
//!   it becomes a `ui.frame.long` transaction carrying the spans that ran
//!   inside it;
//! - **spans**: [`MainThreadLayer`] keeps the main thread's live span stack
//!   (for the watchdog) and a ring of the spans it closed, with self time;
//! - **hangs**: the main thread [`tick`]s every [`TICK`]; a watchdog thread
//!   that sees no tick for [`HANG`] past the cadence samples the live span
//!   stack until the tick comes back, then reports a `ui.hang`.
//!
//! `EXP_FRAME_STATS=1` prints a summary of all three to stderr every
//! [`PRINT_EVERY`], with Sentry on or off.

use std::cell::Cell;
use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant, SystemTime};

use tracing::span::{Attributes, Id};
use tracing::Subscriber;
use tracing_subscriber::layer::Context;
use tracing_subscriber::registry::LookupSpan;

/// The main thread's heartbeat cadence (the app's foreground timer).
pub const TICK: Duration = Duration::from_millis(50);
/// A main-thread stall this far past the cadence is a hang.
pub const HANG: Duration = Duration::from_millis(100);
/// Frame budgets.
pub const LONG_FRAME: Duration = Duration::from_millis(16);
pub const VERY_LONG_FRAME: Duration = Duration::from_millis(33);
/// The stderr summary's period.
pub const PRINT_EVERY: Duration = Duration::from_secs(10);
/// A hang this long is also an error-level event, not only a transaction.
const HANG_EVENT: Duration = Duration::from_secs(2);
/// Long-frame transactions: at most one per this window.
const LONG_FRAME_TX_EVERY: Duration = Duration::from_secs(10);
const RING: usize = 32_768;

thread_local! {
    static IS_MAIN: Cell<bool> = const { Cell::new(false) };
}

static ACTIVE: AtomicBool = AtomicBool::new(false);
static PRINT: AtomicBool = AtomicBool::new(false);

struct Live {
    id: u64,
    name: &'static str,
    label: Option<Arc<str>>,
    start: Instant,
    children: Duration,
}

/// One closed main-thread span.
#[derive(Clone, Debug)]
pub struct SpanRecord {
    pub name: &'static str,
    pub label: Option<Arc<str>>,
    pub start: Instant,
    pub end: Instant,
    /// Its time minus its children's.
    pub self_time: Duration,
}

struct Shared {
    stack: Mutex<Vec<Live>>,
    ring: Mutex<VecDeque<SpanRecord>>,
    stats: Mutex<Stats>,
    /// Micros since `epoch` of the last tick; 0 = none yet.
    last_tick: AtomicU64,
    /// The last tick came from a focused app (a background app's timers are
    /// throttled by the OS: their gaps are no hangs).
    foreground: AtomicBool,
    epoch: Instant,
}

fn shared() -> &'static Shared {
    static SHARED: OnceLock<Shared> = OnceLock::new();
    SHARED.get_or_init(|| Shared {
        stack: Mutex::new(Vec::new()),
        ring: Mutex::new(VecDeque::with_capacity(RING)),
        stats: Mutex::new(Stats::new(Instant::now())),
        last_tick: AtomicU64::new(0),
        foreground: AtomicBool::new(false),
        epoch: Instant::now(),
    })
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|e| e.into_inner())
}

/// Whether frames, ticks and spans are being collected at all (Sentry on or
/// `EXP_FRAME_STATS=1`). Off = every hook below returns at once.
pub fn active() -> bool {
    ACTIVE.load(Ordering::Relaxed)
}

pub(crate) fn activate(print: bool) {
    PRINT.store(print, Ordering::Relaxed);
    if !ACTIVE.swap(true, Ordering::AcqRel) {
        let _ = shared();
        std::thread::Builder::new()
            .name("exp-hang-watchdog".into())
            .spawn(watchdog)
            .ok();
    }
}

/// Called once, on the UI thread, before the event loop runs.
pub fn mark_main_thread() {
    IS_MAIN.with(|main| main.set(true));
}

fn is_main() -> bool {
    IS_MAIN.with(Cell::get)
}

/// The main thread's heartbeat (the app's [`TICK`] timer).
pub fn tick(foreground: bool) {
    if !active() {
        return;
    }
    let shared = shared();
    let now = shared.epoch.elapsed().as_micros() as u64;
    let previous = shared.last_tick.swap(now.max(1), Ordering::AcqRel);
    let was_foreground = shared.foreground.swap(foreground, Ordering::AcqRel);
    if previous != 0 && was_foreground {
        let gap = Duration::from_micros(now.saturating_sub(previous));
        lock(&shared.stats).record_tick_gap(gap.saturating_sub(TICK));
    }
}

/// One drawn frame (`Window::draw`).
/// `invalidations` = the view notifications coalesced into it (0 = a frame
/// nothing changed for: an animation frame request).
pub fn record_frame(start: Instant, end: Instant, invalidations: u64) {
    if !active() {
        return;
    }
    let shared = shared();
    let duration = end.saturating_duration_since(start);
    // The stderr summary also says what EVERY frame runs (idle redraws).
    let spans = if duration >= LONG_FRAME || PRINT.load(Ordering::Relaxed) { spans_between(start, end) } else { Vec::new() };
    let send = {
        let mut stats = lock(&shared.stats);
        stats.record_frame_with(duration, invalidations, &spans);
        let due = duration >= VERY_LONG_FRAME
            && stats.last_long_frame_tx.is_none_or(|at: Instant| at.elapsed() >= LONG_FRAME_TX_EVERY);
        if due {
            stats.last_long_frame_tx = Some(Instant::now());
        }
        due
    };
    if send && crate::sentry_enabled() {
        send_transaction("ui.frame.long", "long frame", start, end, &spans, &[]);
    }
    maybe_print();
}

fn spans_between(start: Instant, end: Instant) -> Vec<SpanRecord> {
    lock(&shared().ring)
        .iter()
        .rev()
        .take_while(|span| span.end >= start)
        .filter(|span| span.start >= start && span.end <= end)
        .cloned()
        .collect()
}

fn maybe_print() {
    if !PRINT.load(Ordering::Relaxed) {
        return;
    }
    let report = {
        let mut stats = lock(&shared().stats);
        if stats.window_start.elapsed() < PRINT_EVERY {
            return;
        }
        let report = stats.report();
        *stats = Stats::new(Instant::now());
        report
    };
    eprintln!("{report}");
}

// ---------------------------------------------------------------------------
// The span layer
// ---------------------------------------------------------------------------

/// The first field's value, short: what a span was about (`view = "Board"`).
struct Label(Option<Arc<str>>);

struct FirstField(Option<String>);

impl tracing::field::Visit for FirstField {
    fn record_str(&mut self, _field: &tracing::field::Field, value: &str) {
        if self.0.is_none() {
            self.0 = Some(value.chars().take(48).collect());
        }
    }
    // Numbers (counts, sizes) stay out of the label: it names what a span
    // was about, and a count would split one span into many rows.
    fn record_debug(&mut self, _field: &tracing::field::Field, _value: &dyn std::fmt::Debug) {}
}

/// Mirrors the main thread's spans into the live stack and the ring.
pub struct MainThreadLayer;

impl<S> tracing_subscriber::Layer<S> for MainThreadLayer
where
    S: Subscriber + for<'a> LookupSpan<'a>,
{
    fn on_new_span(&self, attrs: &Attributes<'_>, id: &Id, ctx: Context<'_, S>) {
        if !is_main() {
            return;
        }
        let mut visitor = FirstField(None);
        attrs.record(&mut visitor);
        if let Some(span) = ctx.span(id) {
            span.extensions_mut().insert(Label(visitor.0.map(|text| Arc::from(crate::scrub::scrub_text(&text)))));
        }
    }

    fn on_enter(&self, id: &Id, ctx: Context<'_, S>) {
        if !is_main() {
            return;
        }
        let Some(span) = ctx.span(id) else { return };
        let label = span.extensions().get::<Label>().and_then(|label| label.0.clone());
        lock(&shared().stack).push(Live { id: id.into_u64(), name: span.metadata().name(), label, start: Instant::now(), children: Duration::ZERO });
    }

    fn on_exit(&self, id: &Id, _ctx: Context<'_, S>) {
        if !is_main() {
            return;
        }
        let shared = shared();
        let end = Instant::now();
        let record = {
            let mut stack = lock(&shared.stack);
            let Some(position) = stack.iter().rposition(|live| live.id == id.into_u64()) else { return };
            let live = stack.remove(position);
            let total = end.saturating_duration_since(live.start);
            if let Some(parent) = position.checked_sub(1).and_then(|index| stack.get_mut(index)) {
                parent.children += total;
            }
            SpanRecord { name: live.name, label: live.label, start: live.start, end, self_time: total.saturating_sub(live.children) }
        };
        let mut ring = lock(&shared.ring);
        if ring.len() == RING {
            ring.pop_front();
        }
        ring.push_back(record);
    }
}

/// The live main-thread span stack, outermost first (`render(Shell) > …`).
pub fn current_stack() -> String {
    lock(&shared().stack)
        .iter()
        .map(|live| match &live.label {
            Some(label) => format!("{}({label})", live.name),
            None => live.name.to_string(),
        })
        .collect::<Vec<_>>()
        .join(" > ")
}

// ---------------------------------------------------------------------------
// The hang watchdog
// ---------------------------------------------------------------------------

/// One main-thread hang.
#[derive(Debug, Clone)]
pub struct Hang {
    pub start: Instant,
    pub end: Instant,
    /// The live span stacks seen while it hung, most frequent first.
    pub stacks: Vec<(String, u32)>,
    /// The main-thread spans that closed inside it, by self time.
    pub spans: Vec<SpanRecord>,
}

fn watchdog() {
    let shared = shared();
    let mut hang: Option<(Instant, HashMap<String, u32>)> = None;
    loop {
        std::thread::sleep(Duration::from_millis(20));
        let last = shared.last_tick.load(Ordering::Acquire);
        if last == 0 || !shared.foreground.load(Ordering::Acquire) {
            hang = None;
            continue;
        }
        let last_at = shared.epoch + Duration::from_micros(last);
        let since = last_at.elapsed();
        if since > TICK + HANG {
            let (_, stacks) = hang.get_or_insert_with(|| (last_at, HashMap::new()));
            let stack = current_stack();
            *stacks.entry(if stack.is_empty() { "(no span)".into() } else { stack }).or_default() += 1;
        } else if let Some((start, stacks)) = hang.take() {
            let end = shared.epoch + Duration::from_micros(shared.last_tick.load(Ordering::Acquire));
            let mut stacks: Vec<(String, u32)> = stacks.into_iter().collect();
            stacks.sort_by(|a, b| b.1.cmp(&a.1));
            let mut spans = spans_between(start, end);
            spans.sort_by(|a, b| b.self_time.cmp(&a.self_time));
            spans.truncate(20);
            report_hang(Hang { start, end, stacks, spans });
        }
    }
}

fn report_hang(hang: Hang) {
    let duration = hang.end.saturating_duration_since(hang.start);
    lock(&shared().stats).record_hang(&hang);
    if PRINT.load(Ordering::Relaxed) {
        let top = hang.stacks.first().map(|(stack, _)| stack.as_str()).unwrap_or("(no span)");
        let spans = top_spans(&hang.spans, 5);
        eprintln!("[frame-stats] HANG {:.0} ms · stack: {top} · closed spans: {spans}", ms(duration));
    }
    if !crate::sentry_enabled() {
        return;
    }
    let stacks: Vec<String> = hang.stacks.iter().take(5).map(|(stack, n)| format!("{n}× {stack}")).collect();
    send_transaction("ui.hang", "main thread hang", hang.start, hang.end, &hang.spans, &[("stacks", stacks.join("\n"))]);
    if duration >= HANG_EVENT {
        let top = hang.stacks.first().map(|(stack, _)| stack.clone()).unwrap_or_else(|| "(no span)".into());
        let mut event = sentry::protocol::Event {
            level: sentry::Level::Warning,
            message: Some(format!("Main thread hung for {:.1} s", duration.as_secs_f64())),
            fingerprint: vec!["ui.hang".into(), top.clone().into()].into(),
            ..Default::default()
        };
        event.extra.insert("duration_ms".into(), (ms(duration) as u64).into());
        event.extra.insert("stacks".into(), stacks.join("\n").into());
        event.extra.insert("spans".into(), top_spans(&hang.spans, 10).into());
        event.tags.insert("hang_stack".into(), top.chars().take(200).collect());
        sentry::Hub::main().capture_event(event);
    }
}

fn system_time(at: Instant) -> SystemTime {
    let now = Instant::now();
    SystemTime::now() - now.saturating_duration_since(at)
}

fn send_transaction(op: &str, name: &str, start: Instant, end: Instant, spans: &[SpanRecord], data: &[(&str, String)]) {
    let hub = sentry::Hub::main();
    let ctx = sentry::TransactionContext::new(name, op);
    let transaction = hub.start_transaction_with_timestamp(ctx, system_time(start));
    transaction.set_data("duration_ms", (ms(end.saturating_duration_since(start)) as u64).into());
    for (key, value) in data {
        transaction.set_data(key, value.clone().into());
    }
    let mut spans: Vec<&SpanRecord> = spans.iter().collect();
    spans.sort_by(|a, b| b.self_time.cmp(&a.self_time));
    for span in spans.into_iter().take(50) {
        let description = match &span.label {
            Some(label) => format!("{} {label}", span.name),
            None => span.name.to_string(),
        };
        let child = transaction.start_child_with_details(span.name, &description, Default::default(), system_time(span.start));
        child.set_data("self_ms", (ms(span.self_time) as u64).into());
        child.finish_with_timestamp(system_time(span.end));
    }
    transaction.finish_with_timestamp(system_time(end));
}

// ---------------------------------------------------------------------------
// The summary
// ---------------------------------------------------------------------------

fn ms(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1000.0
}

fn top_spans(spans: &[SpanRecord], n: usize) -> String {
    let mut by_name: HashMap<String, Duration> = HashMap::new();
    for span in spans {
        let key = match &span.label {
            Some(label) => format!("{}({label})", span.name),
            None => span.name.to_string(),
        };
        *by_name.entry(key).or_default() += span.self_time;
    }
    let mut top: Vec<(String, Duration)> = by_name.into_iter().collect();
    top.sort_by(|a, b| b.1.cmp(&a.1));
    top.into_iter().take(n).map(|(name, time)| format!("{name} {:.1}ms", ms(time))).collect::<Vec<_>>().join(", ")
}

#[derive(Default, Clone, Copy)]
struct SpanAgg {
    count: u32,
    total: Duration,
    max: Duration,
}

/// One summary window's numbers.
pub struct Stats {
    window_start: Instant,
    frames: Vec<Duration>,
    long: u32,
    very_long: u32,
    /// Self time of the spans inside long frames, by span.
    in_long_frames: HashMap<String, SpanAgg>,
    /// Self time of the spans of every frame, by span.
    in_all_frames: HashMap<String, SpanAgg>,
    /// Frames drawn with no invalidation (animation frame requests).
    uninvalidated: u32,
    tick_gaps: Vec<Duration>,
    hangs: Vec<(Duration, String)>,
    last_long_frame_tx: Option<Instant>,
}

impl Stats {
    pub fn new(window_start: Instant) -> Self {
        Self {
            window_start,
            frames: Vec::new(),
            long: 0,
            very_long: 0,
            in_long_frames: HashMap::new(),
            in_all_frames: HashMap::new(),
            uninvalidated: 0,
            tick_gaps: Vec::new(),
            hangs: Vec::new(),
            last_long_frame_tx: None,
        }
    }

    pub fn record_frame(&mut self, duration: Duration, spans: &[SpanRecord]) {
        self.record_frame_with(duration, 1, spans);
    }

    pub fn record_frame_with(&mut self, duration: Duration, invalidations: u64, spans: &[SpanRecord]) {
        self.frames.push(duration);
        if invalidations == 0 {
            self.uninvalidated += 1;
        }
        // What the traced spans do not cover: gpui's own layout, prepaint,
        // paint and text shaping.
        let traced: Duration = spans.iter().map(|span| span.self_time).sum();
        let untraced = SpanRecord {
            name: "(gpui layout/paint, untraced)",
            label: None,
            start: Instant::now(),
            end: Instant::now(),
            self_time: duration.saturating_sub(traced),
        };
        let with_untraced = spans.iter().chain(std::iter::once(&untraced));
        if !spans.is_empty() {
            for span in with_untraced.clone() {
                let key = match &span.label {
                    Some(label) => format!("{}({label})", span.name),
                    None => span.name.to_string(),
                };
                let agg = self.in_all_frames.entry(key).or_default();
                agg.count += 1;
                agg.total += span.self_time;
                agg.max = agg.max.max(span.self_time);
            }
        }
        if duration >= LONG_FRAME {
            self.long += 1;
            for span in with_untraced {
                let key = match &span.label {
                    Some(label) => format!("{}({label})", span.name),
                    None => span.name.to_string(),
                };
                let agg = self.in_long_frames.entry(key).or_default();
                agg.count += 1;
                agg.total += span.self_time;
                agg.max = agg.max.max(span.self_time);
            }
        }
        if duration >= VERY_LONG_FRAME {
            self.very_long += 1;
        }
    }

    pub fn record_tick_gap(&mut self, lag: Duration) {
        self.tick_gaps.push(lag);
    }

    pub fn record_hang(&mut self, hang: &Hang) {
        let top = hang.stacks.first().map(|(stack, _)| stack.clone()).unwrap_or_else(|| "(no span)".into());
        self.hangs.push((hang.end.saturating_duration_since(hang.start), top));
    }

    /// The stderr block.
    pub fn report(&self) -> String {
        let window = self.window_start.elapsed().as_secs_f64().max(0.001);
        let mut out = String::new();
        let mut frames = self.frames.clone();
        frames.sort();
        let draw_total: Duration = frames.iter().sum();
        out.push_str(&format!(
            "[frame-stats] {:.0}s · {} frames ({:.1}/s) · draw p50 {:.1} p95 {:.1} p99 {:.1} max {:.1} ms · >16ms {} · >33ms {} · main thread busy drawing {:.0}%",
            window,
            frames.len(),
            frames.len() as f64 / window,
            ms(percentile(&frames, 0.50)),
            ms(percentile(&frames, 0.95)),
            ms(percentile(&frames, 0.99)),
            ms(frames.last().copied().unwrap_or_default()),
            self.long,
            self.very_long,
            100.0 * draw_total.as_secs_f64() / window,
        ));
        let mut gaps = self.tick_gaps.clone();
        gaps.sort();
        if !gaps.is_empty() {
            out.push_str(&format!(
                "\n[frame-stats]   main-thread tick lag p50 {:.1} p95 {:.1} p99 {:.1} max {:.1} ms · hangs >{}ms: {}",
                ms(percentile(&gaps, 0.50)),
                ms(percentile(&gaps, 0.95)),
                ms(percentile(&gaps, 0.99)),
                ms(gaps.last().copied().unwrap_or_default()),
                HANG.as_millis(),
                self.hangs.len(),
            ));
        }
        if self.uninvalidated > 0 {
            out.push_str(&format!("\n[frame-stats]   frames with no invalidation (animation frames): {}", self.uninvalidated));
        }
        let mut all: Vec<(&String, &SpanAgg)> = self.in_all_frames.iter().collect();
        all.sort_by(|a, b| b.1.total.cmp(&a.1.total));
        for (name, agg) in all.into_iter().take(6) {
            out.push_str(&format!(
                "\n[frame-stats]   in all frames:  {name:<48} {:>4}× self total {:>7.1} ms · max {:>6.1} ms",
                agg.count,
                ms(agg.total),
                ms(agg.max)
            ));
        }
        let mut top: Vec<(&String, &SpanAgg)> = self.in_long_frames.iter().collect();
        top.sort_by(|a, b| b.1.total.cmp(&a.1.total));
        for (name, agg) in top.into_iter().take(8) {
            out.push_str(&format!(
                "\n[frame-stats]   in long frames: {name:<48} {:>4}× self total {:>7.1} ms · max {:>6.1} ms",
                agg.count,
                ms(agg.total),
                ms(agg.max)
            ));
        }
        let mut hangs = self.hangs.clone();
        hangs.sort_by(|a, b| b.0.cmp(&a.0));
        for (duration, stack) in hangs.into_iter().take(5) {
            out.push_str(&format!("\n[frame-stats]   hang {:>6.0} ms · {stack}", ms(duration)));
        }
        out
    }
}

fn percentile(sorted: &[Duration], q: f64) -> Duration {
    if sorted.is_empty() {
        return Duration::ZERO;
    }
    let index = ((sorted.len() as f64 - 1.0) * q).round() as usize;
    sorted[index.min(sorted.len() - 1)]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn span(name: &'static str, label: Option<&str>, self_ms: u64) -> SpanRecord {
        let now = Instant::now();
        SpanRecord { name, label: label.map(Arc::from), start: now, end: now, self_time: Duration::from_millis(self_ms) }
    }

    #[test]
    fn percentiles() {
        let sorted: Vec<Duration> = (1..=100).map(Duration::from_millis).collect();
        assert_eq!(percentile(&sorted, 0.5), Duration::from_millis(51));
        assert_eq!(percentile(&sorted, 0.99), Duration::from_millis(99));
        assert_eq!(percentile(&[], 0.5), Duration::ZERO);
    }

    #[test]
    fn the_report_names_the_spans_of_long_frames_and_the_hangs() {
        let start = Instant::now();
        let mut stats = Stats::new(start);
        for _ in 0..58 {
            stats.record_frame(Duration::from_millis(4), &[]);
        }
        stats.record_frame(Duration::from_millis(40), &[span("render", Some("Board"), 30), span("markdown.parse", None, 6)]);
        stats.record_frame(Duration::from_millis(20), &[span("render", Some("Board"), 12)]);
        stats.record_tick_gap(Duration::from_millis(2));
        stats.record_hang(&Hang { start, end: start + Duration::from_millis(250), stacks: vec![("sync.apply(issues)".into(), 9)], spans: vec![] });
        let report = stats.report();
        assert!(report.contains("60 frames"), "{report}");
        assert!(report.contains(">16ms 2 · >33ms 1"), "{report}");
        assert!(report.contains("render(Board)"), "{report}");
        assert!(report.contains("2× self total    42.0 ms"), "{report}");
        assert!(report.contains("hangs >100ms: 1"), "{report}");
        assert!(report.contains("hang    250 ms · sync.apply(issues)"), "{report}");
    }

    #[test]
    fn the_layer_tracks_the_main_thread_stack_and_self_time() {
        use tracing_subscriber::layer::SubscriberExt as _;
        ACTIVE.store(true, Ordering::Relaxed);
        let subscriber = tracing_subscriber::registry().with(MainThreadLayer);
        tracing::subscriber::with_default(subscriber, || {
            mark_main_thread();
            let outer = tracing::trace_span!("render", view = "Board");
            let _outer = outer.enter();
            {
                let inner = tracing::trace_span!("markdown.parse");
                let _inner = inner.enter();
                assert_eq!(current_stack(), "render(Board) > markdown.parse");
                std::thread::sleep(Duration::from_millis(5));
            }
            assert_eq!(current_stack(), "render(Board)");
        });
        let ring = lock(&shared().ring);
        let names: Vec<&str> = ring.iter().map(|span| span.name).collect();
        assert!(names.ends_with(&["markdown.parse", "render"]), "{names:?}");
        let render = ring.back().unwrap();
        let parse = &ring[ring.len() - 2];
        assert!(parse.self_time >= Duration::from_millis(5));
        assert!(render.self_time < parse.self_time, "the child's time is not the parent's self time");
    }
}
