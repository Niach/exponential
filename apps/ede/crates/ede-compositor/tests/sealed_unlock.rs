//! #EDE-79, end to end: the person types the password into the real
//! `exponential-sealed lock` and the session unlocks. The headless
//! compositor runs the real client in a runtime directory of its own; the
//! only fake is PAM (a stub `libpam.so.0` the client finds through
//! `LD_LIBRARY_PATH`, which accepts one test password). Needs the built
//! binary: `EDE_SEALED_BIN=<path> cargo test -p ede-compositor --test sealed_unlock -- --ignored`.

mod common;

use std::time::{Duration, Instant};

use common::Session;
use ede_compositor::{backend::headless::Harness, lock::LockState, Config};
use ede_protocol::{Error, ErrorCode};
use smithay::backend::input::KeyState;

const RED: u32 = 0xffff_0000;
// xkb codes (evdev + 8) on the us layout.
const A: u32 = 30 + 8;
const S: u32 = 31 + 8;
const D: u32 = 32 + 8;
const F: u32 = 33 + 8;
const ENTER: u32 = 28 + 8;

/// PAM that accepts the password `asdf` and nothing else.
const PAM_STUB: &str = r#"
#include <stdlib.h>
#include <string.h>
struct pam_message { int msg_style; const char *msg; };
struct pam_response { char *resp; int resp_retcode; };
struct pam_conv { int (*conv)(int, const struct pam_message **, struct pam_response **, void *); void *appdata_ptr; };
static struct pam_conv saved;
int pam_start(const char *service, const char *user, const struct pam_conv *conv, void **handle) { saved = *conv; *handle = &saved; return 0; }
int pam_authenticate(void *handle, int flags) {
    struct pam_message ask = { 1, "Password: " };
    const struct pam_message *asks = &ask;
    struct pam_response *answer = 0;
    if (saved.conv(1, &asks, &answer, saved.appdata_ptr) != 0 || !answer || !answer->resp) return 7;
    int ok = strcmp(answer->resp, "asdf") == 0;
    free(answer->resp);
    free(answer);
    return ok ? 0 : 7;
}
int pam_acct_mgmt(void *handle, int flags) { return 0; }
int pam_end(void *handle, int status) { return 0; }
const char *pam_strerror(void *handle, int error) { return "Authentication failure"; }
"#;

fn wait(what: &str, mut ok: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(30);
    while !ok() {
        assert!(Instant::now() < deadline, "{what}");
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// The person types `keys`.
fn type_keys(h: &Harness, keys: &[u32]) {
    for &code in keys {
        for state in [KeyState::Pressed, KeyState::Released] {
            h.with(move |ede| ede.keyboard_key(code.into(), state, 0));
            std::thread::sleep(Duration::from_millis(30));
        }
    }
}

fn is_locked(h: &Harness) -> bool {
    h.with(|ede| ede.is_locked())
}

/// Does the output show the danger tone (the lock's error line)?
fn shows_an_error(h: &Harness) -> bool {
    h.capture(0).pixels.iter().any(|p| (p >> 16) & 0xff > 0xb0 && (p >> 8) & 0xff < 0x80 && p & 0xff < 0x80)
}

#[test]
#[ignore = "needs EDE_SEALED_BIN, the built exponential-sealed"]
fn the_password_typed_into_the_real_lock_client_unlocks() {
    let _ = tracing_subscriber::fmt().with_test_writer().with_env_filter("warn").try_init();
    let bin = std::env::var("EDE_SEALED_BIN").expect("EDE_SEALED_BIN");
    let dir = tempfile::tempdir().unwrap();
    let (source, runtime) = (dir.path().join("pam.c"), dir.path().join("run"));
    std::fs::write(&source, PAM_STUB).unwrap();
    std::fs::create_dir(&runtime).unwrap();
    let built = std::process::Command::new("cc").args(["-shared", "-fPIC", "-o"]).arg(dir.path().join("libpam.so.0")).arg(&source).status().unwrap();
    assert!(built.success(), "the PAM stub did not build");
    // The only test of this binary: nothing else reads the environment.
    // The lock client inherits both: its own runtime directory, and the
    // stub in front of the system's libpam.
    unsafe {
        std::env::set_var("XDG_RUNTIME_DIR", &runtime);
        std::env::set_var("LD_LIBRARY_PATH", dir.path());
    }
    let mut config = Config::default();
    config.keyboard.layout = "us".into();
    let h = Harness::start(config, &[(800, 600)]);
    let mut app = Session::connect(h.connect());
    app.keyboard();
    let t = app.toplevel("app");
    app.draw(&t, 300, 200, RED);
    wait("the window never got the keyboard", || {
        app.roundtrip();
        app.state.keyboard_focus.is_some()
    });

    h.with(|ede| ede.listen().map(|_| ())).unwrap();
    h.with(move |ede| ede.begin_sealed_lock_with(vec![bin, "lock".into()]));
    wait("the lock client never took the keyboard", || {
        h.with(|ede| ede.desk.seat.get_keyboard().unwrap().current_focus().is_some_and(|f| ede.is_lock_surface(&f)))
    });
    wait("the prompt never showed", || h.capture(0).pixels.iter().any(|p| p & 0x00ff_ffff != 0));
    assert!(!shows_an_error(&h));
    let token = h.with(|ede| match &ede.lock {
        LockState::Sealed(s) => s.token.clone(),
        _ => panic!("not sealed-locked"),
    });

    // A wrong password: still locked, and the prompt says so.
    type_keys(&h, &[S, D, F, A, ENTER]);
    wait("a wrong password showed no error", || shows_an_error(&h));
    assert!(is_locked(&h), "a wrong password unlocked");

    // The grant in another process's hands opens nothing.
    match h.ipc_with(token.clone()).unwrap().unlock() {
        Err(Error::Failed(f)) => assert_eq!(f.code, ErrorCode::Forbidden),
        other => panic!("a stolen grant: {other:?}"),
    }
    assert!(is_locked(&h), "a stolen grant unlocked the session");

    // The right one: unlocked, the grant gone, the window types again.
    // Typed straight away: the wrong one is gone from the field (#EDE-81).
    type_keys(&h, &[A, S, D, F, ENTER]);
    wait("the right password did not unlock", || !is_locked(&h));
    assert!(h.ipc_with(token).is_err(), "the unlock grant outlived the lock");
    wait("the window did not get the keyboard back", || {
        app.roundtrip();
        app.state.keyboard_focus.as_ref() == Some(&t.surface)
    });
    type_keys(&h, &[A]);
    wait("the window did not get the next key", || {
        app.roundtrip();
        app.state.keys == [A - 8]
    });
}
