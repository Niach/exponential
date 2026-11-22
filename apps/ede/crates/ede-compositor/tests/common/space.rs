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
    let homes = dir.path().join("homes");
    h.with(move |state| {
        state.spaces.open(spaces);
        state.spaces.open_homes(homes);
    });
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

/// The person holds a key down, or lets it go.
pub fn person_key_state(h: &Harness, code: u32, pressed: bool) {
    use smithay::backend::input::KeyState;
    h.with(move |ede| ede.keyboard_key((code + 8).into(), if pressed { KeyState::Pressed } else { KeyState::Released }, 0));
}

/// Every closed space's seat is gone from the compositor.
pub fn spaces_settled(h: &Harness) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !h.with(|state| state.spaces.settled()) {
        assert!(Instant::now() < deadline, "a closed space's seat stays shared");
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// What an agent's work in a space must leave as it was: the person's
/// windows with their focus and place, which of them are activated, the
/// pointer, the cursor, the clipboard, and what the shell was told.
#[derive(Debug, PartialEq)]
pub struct PersonSnapshot {
    pub windows: Vec<ede_protocol::WindowInfo>,
    pub activated: Vec<bool>,
    pub pointer: (f64, f64),
    pub cursor: String,
    pub clipboard: (u64, String),
}

pub fn person_snapshot(h: &Harness) -> PersonSnapshot {
    use smithay::reexports::wayland_protocols::xdg::shell::server::xdg_toplevel::State;
    h.with(|state| {
        let at = state.desk.seat.get_pointer().unwrap().current_location();
        PersonSnapshot {
            windows: state.window_infos(),
            activated: state.desk.space.elements().filter_map(|w| w.toplevel().map(|t| t.with_pending_state(|s| s.states.contains(State::Activated)))).collect(),
            pointer: (at.x, at.y),
            cursor: format!("{:?}", state.desk.cursor_status),
            clipboard: (state.desk.clipboard.serial, format!("{:?}", state.desk.clipboard.owner)),
        }
    })
}

/// The person's desk is as `before` saw it.
pub fn assert_person_untouched(h: &Harness, before: &PersonSnapshot) {
    assert_eq!(&person_snapshot(h), before, "the person's desk moved");
}

/// Marks the shell's event stream and returns what came before the mark.
pub fn events_until_mark(h: &Harness, shell: &mut ede_protocol::Client, mark: &'static str) -> Vec<String> {
    h.with(move |state| state.emit(mark, serde_json::Value::Null));
    let mut kinds = Vec::new();
    loop {
        let kind = shell.next_event().expect("the shell's events").kind;
        if kind == mark {
            return kinds;
        }
        kinds.push(kind);
    }
}
