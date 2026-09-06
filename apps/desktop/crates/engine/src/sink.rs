//! EXP-746 — the publish seam.
//!
//! Abstracted for ONE reason: `steer::ActivitySender::test_pair` is
//! `#[cfg(test)] pub(crate)`, so an out-of-crate test cannot record what the
//! mapper published. Widening steer's test seam would be a worse change than
//! this two-line trait.
//!
//! Landed in P0 with real bodies.

use std::sync::{Arc, Mutex};

/// Where a mapped, already-redacted [`steer::ActivityEvent`] goes.
/// Fire-and-forget in both impls, exactly like the publisher's own channel.
pub trait EventSink: Send + Sync + 'static {
    fn send(&self, event: steer::ActivityEvent);
}

impl EventSink for steer::ActivitySender {
    fn send(&self, event: steer::ActivityEvent) {
        steer::ActivitySender::send(self, event)
    }
}

/// The test double: every event in order, behind a mutex.
#[derive(Default)]
pub struct RecordingSink(Mutex<Vec<steer::ActivityEvent>>);

impl RecordingSink {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    /// Everything recorded so far, leaving the buffer empty.
    pub fn drain(&self) -> Vec<steer::ActivityEvent> {
        match self.0.lock() {
            Ok(mut events) => std::mem::take(&mut *events),
            Err(poisoned) => std::mem::take(&mut *poisoned.into_inner()),
        }
    }

    /// Everything recorded so far, leaving the buffer intact.
    pub fn snapshot(&self) -> Vec<steer::ActivityEvent> {
        match self.0.lock() {
            Ok(events) => events.clone(),
            Err(poisoned) => poisoned.into_inner().clone(),
        }
    }
}

impl EventSink for RecordingSink {
    fn send(&self, event: steer::ActivityEvent) {
        match self.0.lock() {
            Ok(mut events) => events.push(event),
            Err(poisoned) => poisoned.into_inner().push(event),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_recording_sink_keeps_events_in_order() {
        let sink = RecordingSink::new();
        sink.send(steer::ActivityEvent::narration("first"));
        sink.send(steer::ActivityEvent::narration("second"));
        assert_eq!(sink.snapshot().len(), 2);
        let drained = sink.drain();
        assert_eq!(drained.len(), 2);
        assert!(sink.snapshot().is_empty());
        match &drained[0] {
            steer::ActivityEvent::Narration { text, .. } => assert_eq!(text, "first"),
            other => panic!("expected a narration, got {other:?}"),
        }
    }
}
