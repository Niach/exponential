//! Live plan-picker detection against the REAL `claude` TUI (EXP-150).
//!
//! Ignored by default: it spawns `claude` from PATH (network + a few tokens
//! on haiku) inside the repo checkout (an already-trusted cwd on a dev
//! machine). Run manually when touching the detector:
//!
//! ```sh
//! cargo test -p steer --test plan_picker_live -- --ignored --nocapture
//! ```
//!
//! What it proves end-to-end: the plan-approval picker is detected on the
//! live grid WHILE it is pending (the transcript can't show it then — claude
//! flushes the `ExitPlanMode` entry only after the answer), and the watcher
//! resolves once the picker is answered.

use std::time::{Duration, Instant};

use steer::plan_picker::{PlanPickerWatcher, Transition};
use terminal::{display_offset, SpawnSpec, Terminal};

fn pump_for_transition(
    term: &mut Terminal,
    watcher: &mut PlanPickerWatcher,
    timeout: Duration,
) -> Option<Transition> {
    let wake = term.wake_rx();
    let deadline = Instant::now() + timeout;
    let mut last_tick = Instant::now() - Duration::from_secs(1);
    loop {
        term.pump();
        // The trust dialog (fresh/untrusted cwd) would stall the run —
        // accept its default so the test works on a first-run machine too.
        if term
            .screen_lines()
            .iter()
            .any(|line| line.contains("trust this folder"))
        {
            term.write(b"\r");
        }
        // Poll roughly like the emitter (it ticks at 1s; 300ms keeps the
        // test snappy while still exercising the 2-tick debounce).
        if last_tick.elapsed() >= Duration::from_millis(300) {
            last_tick = Instant::now();
            let offset = display_offset(&term.term());
            if let Some(transition) = watcher.tick(&term.screen_lines(), offset) {
                return Some(transition);
            }
        }
        if Instant::now() >= deadline {
            println!("--- screen at timeout ---");
            for line in term.screen_lines() {
                println!("|{line}");
            }
            return None;
        }
        let _ = wake.recv_timeout(Duration::from_millis(50));
    }
}

#[test]
#[ignore = "spawns a real `claude` TUI (PATH + network + tokens) — run manually"]
fn live_picker_detected_while_pending_and_resolved_on_answer() {
    let spec = SpawnSpec::new("claude")
        .args([
            "--model",
            "haiku",
            "--permission-mode",
            "plan",
            "Immediately call ExitPlanMode with the one-line plan: noop probe plan. \
             Do not read any files, do not use any other tools.",
        ])
        .env("TERM", "xterm-256color")
        // The repo checkout — a dev machine's already-trusted cwd (the PTY
        // default is $HOME, which triggers the first-run trust dialog).
        .cwd(concat!(env!("CARGO_MANIFEST_DIR"), "/../../../.."));
    let mut term = Terminal::spawn(&spec, 110, 42).expect("spawn claude");
    let mut watcher = PlanPickerWatcher::new();

    // The picker must be detected while it is PENDING — this is the whole
    // point: nothing has hit the transcript yet at this moment.
    let shown = pump_for_transition(&mut term, &mut watcher, Duration::from_secs(180))
        .expect("plan picker never detected");
    let snapshot = match shown {
        Transition::Show(snapshot) => snapshot,
        other => panic!("expected Show first, got {other:?}"),
    };
    println!(
        "detected pending picker with options: {:?}",
        snapshot
            .options
            .iter()
            .map(|o| format!("{} => {}", o.key, o.label))
            .collect::<Vec<_>>()
    );
    assert!(snapshot.options.len() >= 2, "picker should offer ≥2 options");
    assert_eq!(snapshot.options[0].key, "1");

    // Answer it like a steering client would: the digit selects, a separate
    // Enter submits (EXP-97). The watcher must then resolve.
    term.write(b"1");
    std::thread::sleep(Duration::from_millis(200));
    term.write(b"\r");
    let resolved = pump_for_transition(&mut term, &mut watcher, Duration::from_secs(90))
        .expect("picker never resolved after answering");
    assert_eq!(resolved, Transition::Resolved);
}
