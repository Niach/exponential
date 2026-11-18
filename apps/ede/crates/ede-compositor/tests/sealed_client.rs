//! #EDE-78: the real `exponential-sealed lock` against the headless
//! compositor, in a runtime directory of its own. Needs the built binary:
//! `EDE_SEALED_BIN=<path> cargo test -p ede-compositor --test sealed_client -- --ignored`.

use std::time::{Duration, Instant};

use ede_compositor::{backend::headless::Harness, Config};

/// Does output `index` show anything but black?
fn shows(h: &Harness, index: usize) -> bool {
    h.capture(index).pixels.iter().any(|p| p & 0x00ff_ffff != 0)
}

fn wait(what: &str, mut ok: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(30);
    while !ok() {
        assert!(Instant::now() < deadline, "{what}");
        std::thread::sleep(Duration::from_millis(100));
    }
}

#[test]
#[ignore = "needs EDE_SEALED_BIN, the built exponential-sealed"]
fn the_lock_client_covers_every_output_and_follows_them() {
    let _ = tracing_subscriber::fmt().with_test_writer().with_env_filter("warn").try_init();
    let bin = std::env::var("EDE_SEALED_BIN").expect("EDE_SEALED_BIN");
    let runtime = tempfile::tempdir().unwrap();
    // The only test of this binary: nothing else reads the environment.
    unsafe { std::env::set_var("XDG_RUNTIME_DIR", runtime.path()) };
    let h = Harness::start(Config::default(), &[(800, 600), (1024, 768)]);
    h.with(|ede| ede.listen().map(|_| ())).unwrap();
    h.with(move |ede| ede.begin_sealed_lock_with(vec![bin, "lock".into()]));
    wait("the first output never showed the prompt", || shows(&h, 0));
    wait("the second output stayed black", || shows(&h, 1));

    // One goes: the prompt stays on the other, once.
    h.with_backend(|ede, backend| backend.remove_output(ede, 1));
    wait("the lock client kept the surface of an output that went", || h.with(|ede| ede.sealed_lock_surfaces().len()) == 1);
    assert!(shows(&h, 0));

    // A new one comes: it shows the prompt too.
    h.with_backend(|ede, backend| {
        backend.add_output(ede, "HEADLESS-3", (1280, 720));
    });
    wait("a new output stayed black", || shows(&h, 1));
    assert!(shows(&h, 0));
    assert!(h.with(|ede| ede.is_locked()));
}
