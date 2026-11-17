//! #EDE-125: what the agent-space tests share.

use ede_compositor::{backend::headless::Harness, Config};

/// A headless compositor that can make spaces: their directories live in a
/// temp dir (dropped with the returned guard), never the runtime dir.
pub fn start_with_spaces(outputs: &[(i32, i32)]) -> (Harness, tempfile::TempDir) {
    let _ = tracing_subscriber::fmt().with_test_writer().with_env_filter("warn").try_init();
    let h = Harness::start(Config::default(), outputs);
    let dir = tempfile::tempdir().expect("temp dir");
    let spaces = dir.path().join("wayland-test.spaces");
    h.with(move |state| state.spaces.dir = Some(spaces));
    (h, dir)
}
