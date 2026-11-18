//! #EDE-125: what the agent-space tests share.

use std::time::{Duration, Instant};

use ede_compositor::{backend::headless::Harness, Config};

use super::Session;

/// A headless compositor that can make spaces: their directories live in a
/// temp dir (dropped with the returned guard), never the runtime dir.
pub fn start_with_spaces(outputs: &[(i32, i32)]) -> (Harness, tempfile::TempDir) {
    let _ = tracing_subscriber::fmt().with_test_writer().with_env_filter("warn").try_init();
    let h = Harness::start(Config::default(), outputs);
    let dir = tempfile::tempdir().expect("temp dir");
    let spaces = dir.path().join("wayland-test.spaces");
    h.with(move |state| state.spaces.open(spaces));
    (h, dir)
}

pub fn wait_for(client: &mut Session, mut ok: impl FnMut(&mut Session) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !ok(client) {
        assert!(Instant::now() < deadline, "timed out");
        std::thread::sleep(Duration::from_millis(10));
        client.roundtrip();
    }
}

/// The person presses and lets go of a key (an evdev code) on their keyboard.
pub fn person_key(h: &Harness, code: u32) {
    use smithay::backend::input::KeyState;
    h.with(move |ede| {
        ede.keyboard_key((code + 8).into(), KeyState::Pressed, 0);
        ede.keyboard_key((code + 8).into(), KeyState::Released, 0);
    });
}

/// Every closed space's seat is gone from the compositor.
pub fn spaces_settled(h: &Harness) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !h.with(|state| state.spaces.settled()) {
        assert!(Instant::now() < deadline, "a closed space's seat stays shared");
        std::thread::sleep(Duration::from_millis(50));
    }
}
