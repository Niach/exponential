//! A flood guard for error events (#LOGS-2): a render error repeating every
//! frame would otherwise be one event per frame. At most one event per
//! distinct message a minute, and 30 a minute overall.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use sentry::protocol::Event;

const WINDOW: Duration = Duration::from_secs(60);
const PER_WINDOW: u32 = 30;

#[derive(Default)]
pub struct Limiter {
    window: Option<Instant>,
    sent: u32,
    seen: HashMap<String, Instant>,
}

impl Limiter {
    pub fn allow(&mut self, key: &str, now: Instant) -> bool {
        if self.window.is_none_or(|w| now.duration_since(w) >= WINDOW) {
            self.window = Some(now);
            self.sent = 0;
            self.seen.retain(|_, at| now.duration_since(*at) < WINDOW);
        }
        if self.sent >= PER_WINDOW || self.seen.get(key).is_some_and(|at| now.duration_since(*at) < WINDOW) {
            return false;
        }
        self.sent += 1;
        self.seen.insert(key.to_owned(), now);
        true
    }
}

static LIMITER: Mutex<Option<Limiter>> = Mutex::new(None);

/// The event's identity: its first exception, else its message.
fn key(e: &Event<'_>) -> String {
    let text = match e.exception.values.first() {
        Some(ex) => format!("{}: {}", ex.ty, ex.value.as_deref().unwrap_or("")),
        None => e.message.clone().or_else(|| e.logentry.as_ref().map(|l| l.message.clone())).unwrap_or_default(),
    };
    text.chars().take(160).collect()
}

pub fn allow_event(e: &Event<'_>) -> bool {
    let mut guard = LIMITER.lock().unwrap_or_else(|p| p.into_inner());
    guard.get_or_insert_with(Limiter::default).allow(&key(e), Instant::now())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_per_message_a_minute_and_thirty_overall() {
        let mut l = Limiter::default();
        let t = Instant::now();
        assert!(l.allow("render failed", t));
        assert!(!l.allow("render failed", t + Duration::from_secs(1)));
        assert!(l.allow("bind failed", t + Duration::from_secs(1)));
        assert!(l.allow("render failed", t + Duration::from_secs(61)), "a new minute");
        let mut l = Limiter::default();
        for i in 0..PER_WINDOW {
            assert!(l.allow(&format!("e{i}"), t));
        }
        assert!(!l.allow("one more", t));
    }
}
