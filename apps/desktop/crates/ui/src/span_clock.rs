//! #PERF-2/3/4 (benches and guards): a `tracing` subscriber that adds up
//! each span's SELF time on the thread it is installed on, the way the
//! hang watchdog attributes a hang (`observability::lag`). A bench names
//! the stage a frame spent its time in; a guard asserts a stage stayed
//! quiet (ran zero times) across a change that must not reach it.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use tracing::span::{Attributes, Id, Record};
use tracing::{Event, Metadata, Subscriber};

/// A span's name plus its first string field (`render(RailView)`).
fn label(attrs: &Attributes<'_>) -> String {
    struct First(Option<String>);
    impl tracing::field::Visit for First {
        fn record_str(&mut self, _: &tracing::field::Field, value: &str) {
            if self.0.is_none() {
                self.0 = Some(value.to_string());
            }
        }
        fn record_debug(&mut self, _: &tracing::field::Field, _: &dyn std::fmt::Debug) {}
    }
    let mut first = First(None);
    attrs.record(&mut first);
    match first.0 {
        Some(field) => format!("{}({field})", attrs.metadata().name()),
        None => attrs.metadata().name().to_string(),
    }
}

#[derive(Default)]
struct State {
    names: HashMap<u64, String>,
    stack: Vec<(u64, Instant, Duration)>,
    totals: HashMap<String, (Duration, u64)>,
}

/// Self time and entries per span label.
#[derive(Clone, Default)]
pub(crate) struct SpanClock {
    state: Rc<RefCell<State>>,
}

struct Sub {
    state: Rc<RefCell<State>>,
    next: AtomicU64,
}

// The bench installs it with `set_default` on ONE thread and only that
// thread ever reaches it.
unsafe impl Send for Sub {}
unsafe impl Sync for Sub {}

impl Subscriber for Sub {
    fn enabled(&self, _: &Metadata<'_>) -> bool {
        true
    }
    fn new_span(&self, attrs: &Attributes<'_>) -> Id {
        let id = self.next.fetch_add(1, Ordering::Relaxed) + 1;
        self.state.borrow_mut().names.insert(id, label(attrs));
        Id::from_u64(id)
    }
    fn record(&self, _: &Id, _: &Record<'_>) {}
    fn record_follows_from(&self, _: &Id, _: &Id) {}
    fn event(&self, _: &Event<'_>) {}
    fn enter(&self, id: &Id) {
        self.state.borrow_mut().stack.push((id.into_u64(), Instant::now(), Duration::ZERO));
    }
    fn exit(&self, id: &Id) {
        let mut state = self.state.borrow_mut();
        let Some(at) = state.stack.iter().rposition(|(live, ..)| *live == id.into_u64()) else { return };
        let (_, start, children) = state.stack.remove(at);
        let total = start.elapsed();
        if let Some(parent) = at.checked_sub(1).and_then(|ix| state.stack.get_mut(ix)) {
            parent.2 += total;
        }
        let name = state.names.get(&id.into_u64()).cloned().unwrap_or_default();
        let entry = state.totals.entry(name).or_default();
        entry.0 += total.saturating_sub(children);
        entry.1 += 1;
    }
    fn try_close(&self, id: Id) -> bool {
        self.state.borrow_mut().names.remove(&id.into_u64());
        true
    }
}

impl SpanClock {
    /// Collects on this thread until the guard drops.
    pub(crate) fn install() -> (Self, tracing::subscriber::DefaultGuard) {
        let clock = Self::default();
        let guard = tracing::subscriber::set_default(Sub { state: clock.state.clone(), next: AtomicU64::new(0) });
        (clock, guard)
    }

    /// Forget what was counted so far.
    pub(crate) fn reset(&self) {
        self.state.borrow_mut().totals.clear();
    }

    /// How many times `label` closed since the last reset.
    pub(crate) fn entries(&self, label: &str) -> u64 {
        self.state.borrow().totals.get(label).map_or(0, |(_, n)| *n)
    }

    /// The `n` labels with the most self time, `label ms ×count`.
    pub(crate) fn top(&self, n: usize) -> String {
        let state = self.state.borrow();
        let mut rows: Vec<(&String, &(Duration, u64))> = state.totals.iter().collect();
        rows.sort_by(|a, b| b.1 .0.cmp(&a.1 .0));
        rows.iter()
            .take(n)
            .map(|(label, (time, count))| format!("{label} {:.1}ms ×{count}", time.as_secs_f64() * 1e3))
            .collect::<Vec<_>>()
            .join(" · ")
    }
}
