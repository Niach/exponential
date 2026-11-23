//! #ERR-29: a sealed lock client that dies while the session is locked is
//! an error, the level ede-observability sends to GlitchTip as an event.
//! #EDE-210: one that cannot start is a warning once, not every second.
//! Its own test binary: the capture is the process's one subscriber; each
//! test reads the events of its own compositor's thread.

use std::{
    fmt::Debug,
    os::unix::fs::PermissionsExt,
    sync::{Arc, Mutex, OnceLock},
    thread::ThreadId,
    time::{Duration, Instant},
};

use ede_compositor::{backend::headless::Harness, lock::LockState, Config};
use tracing::{
    field::{Field, Visit},
    Level, Subscriber,
};
use tracing_subscriber::{layer::Context, prelude::*, Layer};

type Records = Arc<Mutex<Vec<(ThreadId, Level, String)>>>;

/// Every event's thread, level and message.
struct Capture(Records);

/// The process's capture, installed by the first test that asks.
fn capture() -> Records {
    static RECORDS: OnceLock<Records> = OnceLock::new();
    RECORDS
        .get_or_init(|| {
            let records = Records::default();
            tracing_subscriber::registry().with(Capture(records.clone())).init();
            records
        })
        .clone()
}

/// What the compositor on `thread` logged: level and message.
fn logged(records: &Records, thread: ThreadId) -> Vec<(Level, String)> {
    records.lock().unwrap().iter().filter(|(t, ..)| *t == thread).map(|(_, level, message)| (*level, message.clone())).collect()
}

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
        self.0.lock().unwrap().push((std::thread::current().id(), *event.metadata().level(), message.0));
    }
}

#[test]
fn a_lock_client_that_dies_while_locked_is_an_error() {
    let records = capture();
    let h = Harness::start(Config::default(), &[(800, 600)]);
    let thread = h.with(|_| std::thread::current().id());
    h.with(|ede| ede.begin_sealed_lock_with(vec!["sh".into(), "-c".into(), "exit 3".into()]));
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        h.with(|ede| ede.sealed_tick());
        let died = logged(&records, thread).iter().any(|(level, message)| *level == Level::ERROR && message == "the lock client ended while locked");
        if died {
            break;
        }
        assert!(Instant::now() < deadline, "no error for the dead lock client: {:?}", logged(&records, thread));
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(h.with(|ede| matches!(ede.lock, LockState::Sealed(_))), "the session stays locked");
}

/// #EDE-210: a lock client that cannot start any more (its binary gone, an
/// upgrade under the lock) is tried again on every tick and logged at the
/// first failure with its reason, not on every try; the session stays
/// locked and black meanwhile, and once the binary is back it starts.
#[test]
fn a_lock_client_that_cannot_start_is_logged_once_not_every_second() {
    let records = capture();
    let dir = tempfile::tempdir().unwrap();
    let client = dir.path().join("exponential-sealed");
    // Its first start works: it removes its own binary and ends.
    std::fs::write(&client, "#!/bin/sh\nrm -f -- \"$0\"\nexit 1\n").unwrap();
    std::fs::set_permissions(&client, std::fs::Permissions::from_mode(0o755)).unwrap();
    let h = Harness::start(Config::default(), &[(800, 600)]);
    let thread = h.with(|_| std::thread::current().id());
    let argv = vec![client.to_string_lossy().into_owned()];
    h.with(move |ede| ede.begin_sealed_lock_with(argv));
    let cannot_start = |records: &Records| logged(records, thread).into_iter().filter(|(_, message)| message.contains("cannot start")).collect::<Vec<_>>();
    let deadline = Instant::now() + Duration::from_secs(10);
    while cannot_start(&records).is_empty() {
        assert!(Instant::now() < deadline, "the gone lock client was never tried again: {:?}", logged(&records, thread));
        std::thread::sleep(Duration::from_millis(50));
    }

    // A hundred more tries (the timer's and these): still the one line.
    for _ in 0..100 {
        h.with(|ede| ede.sealed_tick());
        std::thread::sleep(Duration::from_millis(10));
    }
    std::thread::sleep(Duration::from_secs(2));
    let lines = cannot_start(&records);
    assert_eq!(lines.len(), 1, "a start that keeps failing is logged once in its first 30 s: {lines:?}");
    let (level, line) = &lines[0];
    assert_eq!(*level, Level::WARN);
    assert!(line.starts_with("the lock client cannot start: ") && line.contains("No such file"), "the first failure says why: {line}");
    let tries = h.with(|ede| match &ede.lock {
        LockState::Sealed(s) => s.failed_starts.tries,
        _ => panic!("the session must stay locked"),
    });
    assert!(tries > 100, "every tick tries again: {tries}");
    assert!(h.capture(0).pixels.iter().all(|p| p & 0x00ff_ffff == 0), "the lock stays black");

    // The binary is back: the next tick starts it, and says after how many tries.
    std::fs::write(&client, "#!/bin/sh\nexec sleep 30\n").unwrap();
    std::fs::set_permissions(&client, std::fs::Permissions::from_mode(0o755)).unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while !logged(&records, thread).iter().any(|(level, message)| *level == Level::INFO && message.starts_with("the lock client started after ")) {
        assert!(Instant::now() < deadline, "the lock client did not start once its binary was back: {:?}", logged(&records, thread));
        h.with(|ede| ede.sealed_tick());
        std::thread::sleep(Duration::from_millis(50));
    }
    assert!(h.with(|ede| matches!(&ede.lock, LockState::Sealed(s) if s.child.is_some() && s.failed_starts.tries == 0)));
}
