//! #EDE-79, end to end: the person types the password into the real
//! `exponential-sealed lock` and the session unlocks. The headless
//! compositor runs the real client in a runtime directory of its own; the
//! only fake is PAM (a stub `libpam.so.0` the client finds through
//! `LD_LIBRARY_PATH`, which accepts one test password). Needs the built
//! binary: `EDE_SEALED_BIN=<path> cargo test -p ede-compositor --test sealed_unlock -- --ignored`.
//! #ERR-29: and it locks and unlocks whatever its error reporting does.

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
    let first = vec![bin.clone(), "lock".into()];
    h.with(move |ede| ede.begin_sealed_lock_with(first));
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

    // #ERR-29: the lock client reports through ede-observability now. A
    // lock that cannot start because of its error reporting is worse than
    // no report: it locks and unlocks with reporting off; on, with a
    // server that takes the connection and never answers (a crash report
    // of an earlier launch waiting to go); and on, with the crash monitor
    // unable to start. A start that waited on any of it would take the
    // config fetch's 10 s at least.
    let tarpit = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let server = format!("http://{}", tarpit.local_addr().unwrap());
    let reporting = dir.path().join("reporting");
    let (data, cache, config) = (reporting.join("data"), reporting.join("cache"), reporting.join("config"));
    let crashes = cache.join("exponential-desktop").join("crashes");
    for d in [&data, &crashes, &config] {
        std::fs::create_dir_all(d).unwrap();
    }
    std::fs::write(data.join("accounts.json"), format!(r#"{{"accounts":[{{"instanceUrl":"{server}","userId":"u-test"}}]}}"#)).unwrap();
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs();
    std::fs::write(
        crashes.join("6c8f0b2c-9c55-4c8a-9b0f-0f3c1f0e9a11.json"),
        format!(r#"{{"id":"6c8f0b2c-9c55-4c8a-9b0f-0f3c1f0e9a11","release":"exponential-ede@0.2.28","component":"compositor","timestamp":{now},"os":"Linux","os_version":null,"arch":"x86_64","kind":"SIGSEGV","reason":"SIGSEGV / SEGV_MAPERR","address":0,"thread_id":7,"thread_name":null,"frames":[{{"address":4096,"module":0,"trust":"context"}}],"modules":[{{"code_file":"/usr/bin/exponential-compositor","base":0,"size":65536,"debug_id":"b0b1b2b3-b4b5-b6b7-b8b9-babbbcbdbebf","code_id":null,"debug_file":null,"main":true}}]}}"#),
    )
    .unwrap();
    let not_a_directory = reporting.join("not-a-directory");
    std::fs::write(&not_a_directory, "").unwrap();
    let on = |cache: &std::path::Path| {
        vec![
            "EXP_DIAGNOSTICS=1".to_string(),
            format!("EXP_DATA_DIR={}", data.display()),
            format!("XDG_CACHE_HOME={}", cache.display()),
            format!("XDG_CONFIG_HOME={}", config.display()),
            "EXP_SENTRY_DSN=https://publickey@glitchtip.example/9".to_string(),
        ]
    };
    let cases = [
        ("reporting off", vec!["EXP_DIAGNOSTICS=0".to_string()]),
        ("a server that never answers", on(&cache)),
        ("no crash monitor (its directory cannot be made)", on(&not_a_directory)),
    ];
    for (case, env) in cases {
        let argv: Vec<String> = std::iter::once("env".to_string()).chain(env).chain([bin.clone(), "lock".into()]).collect();
        let started = Instant::now();
        h.with(move |ede| ede.begin_sealed_lock_with(argv));
        wait(&format!("{case}: the lock client never took the keyboard"), || {
            h.with(|ede| ede.desk.seat.get_keyboard().unwrap().current_focus().is_some_and(|f| ede.is_lock_surface(&f)))
        });
        let took = started.elapsed();
        assert!(took < Duration::from_secs(10), "{case}: the lock waited on its error reporting ({took:?})");
        assert!(is_locked(&h), "{case}");
        type_keys(&h, &[A, S, D, F, ENTER]);
        wait(&format!("{case}: the right password did not unlock"), || !is_locked(&h));
        eprintln!("{case}: locked in {took:?}, unlocked");
    }
    // The reports did try to go: the never-answering server was called.
    tarpit.set_nonblocking(true).unwrap();
    assert!(tarpit.accept().is_ok(), "reporting was on, the server was never called");
}
