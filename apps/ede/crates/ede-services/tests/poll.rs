//! #EDE-135: the data source's poll. A hung domain stops no other domain's
//! events, nothing is polled while nobody listens, and a `get` never eats
//! the change the next poll would have broadcast.

use std::{
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    thread,
    time::{Duration, Instant},
};

use ede_services::{
    socket::{serve, Client},
    ActionSpec, Registry, Service,
};
use serde_json::{json, Value};

/// A domain whose snapshot runs a command that never ends (an `lpstat`
/// against an unreachable CUPS server).
struct Hung;
impl Service for Hung {
    fn name(&self) -> &'static str {
        "hung"
    }
    fn description(&self) -> &'static str {
        "a command that hangs"
    }
    fn snapshot(&self) -> Value {
        json!({"available": true, "out": ede_services::domains::command("sleep", &["60"])})
    }
    fn actions(&self) -> Vec<ActionSpec> {
        vec![]
    }
    fn call(&self, _: &str, _: &Value) -> Result<Value, String> {
        Err("no".into())
    }
}

/// A domain whose state the test moves; counts its snapshots.
#[derive(Clone, Default)]
struct Lamp {
    on: Arc<AtomicU64>,
    looks: Arc<AtomicU64>,
}
impl Service for Lamp {
    fn name(&self) -> &'static str {
        "lamp"
    }
    fn description(&self) -> &'static str {
        "a lamp"
    }
    fn snapshot(&self) -> Value {
        self.looks.fetch_add(1, Ordering::SeqCst);
        json!({"available": true, "on": self.on.load(Ordering::SeqCst)})
    }
    fn actions(&self) -> Vec<ActionSpec> {
        vec![]
    }
    fn call(&self, _: &str, _: &Value) -> Result<Value, String> {
        Err("no".into())
    }
}

fn start(services: Vec<Box<dyn Service>>) -> (tempfile::TempDir, std::path::PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("services.sock");
    let registry = Arc::new(Registry::new(services));
    let p = path.clone();
    thread::spawn(move || serve(&p, registry));
    let deadline = Instant::now() + Duration::from_secs(5);
    while !path.exists() && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(20));
    }
    (dir, path)
}

/// The next `services.changed` of `domain` within `t`.
fn event_of(client: &mut Client, domain: &str, t: Duration) -> Option<Value> {
    let deadline = Instant::now() + t;
    while let Some(left) = deadline.checked_duration_since(Instant::now()) {
        client.set_read_timeout(Some(left.max(Duration::from_millis(1)))).unwrap();
        match client.next_event() {
            Ok(e) if e["domain"] == domain => return Some(e),
            Ok(_) => continue,
            Err(_) => return None,
        }
    }
    None
}

#[test]
fn a_hung_domain_stops_no_other_domains_events() {
    let lamp = Lamp::default();
    let (_dir, path) = start(vec![Box::new(Hung), Box::new(lamp.clone())]);
    let mut client = Client::connect(&path).unwrap();
    client.subscribe().unwrap();
    lamp.on.store(1, Ordering::SeqCst);
    let event = event_of(&mut client, "lamp", Duration::from_secs(6)).expect("the lamp's state reached the subscriber");
    assert_eq!(event["snapshot"]["on"], 1);
}

#[test]
fn nothing_is_polled_while_nobody_listens() {
    let lamp = Lamp::default();
    let (_dir, _path) = start(vec![Box::new(lamp.clone())]);
    thread::sleep(Duration::from_secs(3));
    assert_eq!(lamp.looks.load(Ordering::SeqCst), 0, "the poll looked with no subscriber");
}

#[test]
fn a_get_does_not_eat_the_change_the_poll_broadcasts() {
    let lamp = Lamp::default();
    let (_dir, path) = start(vec![Box::new(lamp.clone())]);
    let mut panel = Client::connect(&path).unwrap();
    panel.subscribe().unwrap();
    // The poll has looked (and maybe broadcast its first look); nothing moves.
    let _ = event_of(&mut panel, "lamp", Duration::from_millis(2500));
    // An agent turns the lamp on and reads it back before the next poll.
    lamp.on.store(1, Ordering::SeqCst);
    let mut agent = Client::connect(&path).unwrap();
    assert_eq!(agent.get("lamp").unwrap()["on"], 1);
    let event = event_of(&mut panel, "lamp", Duration::from_secs(5)).expect("the panel heard the change");
    assert_eq!(event["snapshot"]["on"], 1);
}

#[test]
fn a_subscriber_that_gets_after_subscribing_keeps_the_events_heard_meanwhile() {
    let lamp = Lamp::default();
    let other = Lamp { on: Arc::new(AtomicU64::new(0)), looks: Arc::new(AtomicU64::new(0)) };
    struct Named(Lamp, &'static str);
    impl Service for Named {
        fn name(&self) -> &'static str {
            self.1
        }
        fn description(&self) -> &'static str {
            "a lamp"
        }
        fn snapshot(&self) -> Value {
            self.0.snapshot()
        }
        fn actions(&self) -> Vec<ActionSpec> {
            vec![]
        }
        fn call(&self, _: &str, _: &Value) -> Result<Value, String> {
            Err("no".into())
        }
    }
    let (_dir, path) = start(vec![Box::new(lamp.clone()), Box::new(Named(other.clone(), "slow"))]);
    let mut shell = Client::connect(&path).unwrap();
    shell.subscribe().unwrap();
    // The poll's first broadcast arrives while the shell reads another domain.
    thread::sleep(Duration::from_millis(500));
    assert_eq!(shell.get("slow").unwrap()["on"], 0);
    let event = event_of(&mut shell, "lamp", Duration::from_secs(5)).expect("the event heard during the get stayed");
    assert_eq!(event["snapshot"]["on"], 0);
}
