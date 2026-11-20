//! #ERR-29: a sealed lock client that dies while the session is locked is
//! an error, the level ede-observability sends to GlitchTip as an event.
//! Its own test binary: the capture is the process's one subscriber.

use std::{
    fmt::Debug,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use ede_compositor::{backend::headless::Harness, lock::LockState, Config};
use tracing::{
    field::{Field, Visit},
    Level, Subscriber,
};
use tracing_subscriber::{layer::Context, prelude::*, Layer};

type Records = Arc<Mutex<Vec<(Level, String)>>>;

/// Every event's level and message.
struct Capture(Records);

impl<S: Subscriber> Layer<S> for Capture {
    fn on_event(&self, event: &tracing::Event<'_>, _: Context<'_, S>) {
        struct Message(String);
        impl Visit for Message {
            fn record_debug(&mut self, field: &Field, value: &dyn Debug) {
                if field.name() == "message" {
                    self.0 = format!("{value:?}");
                }
            }
        }
        let mut message = Message(String::new());
        event.record(&mut message);
        self.0.lock().unwrap().push((*event.metadata().level(), message.0));
    }
}

#[test]
fn a_lock_client_that_dies_while_locked_is_an_error() {
    let records = Records::default();
    tracing_subscriber::registry().with(Capture(records.clone())).init();
    let h = Harness::start(Config::default(), &[(800, 600)]);
    h.with(|ede| ede.begin_sealed_lock_with(vec!["sh".into(), "-c".into(), "exit 3".into()]));
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        h.with(|ede| ede.sealed_tick());
        let died = records.lock().unwrap().iter().any(|(level, message)| *level == Level::ERROR && message == "the lock client ended while locked");
        if died {
            break;
        }
        assert!(Instant::now() < deadline, "no error for the dead lock client: {:?}", records.lock().unwrap());
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(h.with(|ede| matches!(ede.lock, LockState::Sealed(_))), "the session stays locked");
}
