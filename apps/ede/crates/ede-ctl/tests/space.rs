//! #EDE-126: `exponential-ctl space`, `launch` and `capture --space`
//! against a headless compositor on a runtime directory of its own.

use std::{path::Path, process::Command};

use ede_compositor::{backend::headless::Harness, Config};

/// Runs `exponential-ctl ARGS` there; `token` = a space's, in `EDE_TOKEN`.
fn ctl(runtime: &Path, token: Option<&str>, args: &[&str]) -> (bool, String, String) {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_exponential-ctl"));
    cmd.args(args).env("XDG_RUNTIME_DIR", runtime).env("WAYLAND_DISPLAY", "wayland-test").env_remove("EDE_TOKEN");
    if let Some(token) = token {
        cmd.env("EDE_TOKEN", token);
    }
    let out = cmd.output().expect("exponential-ctl runs");
    (out.status.success(), String::from_utf8_lossy(&out.stdout).into_owned(), String::from_utf8_lossy(&out.stderr).into_owned())
}

fn ok(run: (bool, String, String)) -> String {
    assert!(run.0, "exponential-ctl failed: {}", run.2);
    run.1
}

fn json(line: &str) -> serde_json::Value {
    serde_json::from_str(line.trim()).unwrap_or_else(|e| panic!("not JSON ({e}): {line}"))
}

#[test]
fn the_space_commands_make_drive_and_close_a_space() {
    let h = Harness::start(Config::default(), &[(800, 600)]);
    let dir = tempfile::tempdir().unwrap();
    let runtime = dir.path().to_path_buf();
    let spaces = runtime.join("exponential-desktop/wayland-test.spaces");
    h.with({
        let runtime = runtime.clone();
        move |state| {
            state.ipc_listen(&runtime, "wayland-test").expect("the control socket");
            state.spaces.open(spaces);
        }
    });

    // create: the space, and the token of the grant that owns it.
    let made = json(&ok(ctl(&runtime, None, &["space", "create", "640x480", "--session", "ctl-test"])));
    assert_eq!((made["id"].as_str(), made["session_id"].as_str(), made["width"].as_i64(), made["height"].as_i64()), (Some("space-1"), Some("ctl-test"), Some(640), Some(480)));
    let token = made["token"].as_str().expect("the grant's token").to_string();
    let socket = made["wayland_display"].as_str().unwrap().to_string();
    assert!(Path::new(&socket).exists());
    // list, with the root token: every space, without the token.
    let listed = json(&ok(ctl(&runtime, None, &["space", "list"])));
    assert_eq!((listed["id"].as_str(), listed.get("token")), (Some("space-1"), None));

    // With the space's token the old commands act in the space.
    let outputs = ok(ctl(&runtime, Some(&token), &["outputs"]));
    assert_eq!(json(&outputs)["name"], "space-1");
    assert_eq!(ok(ctl(&runtime, Some(&token), &["windows"])), "");
    ok(ctl(&runtime, Some(&token), &["key", "30"]));
    ok(ctl(&runtime, Some(&token), &["pointer", "10", "10", "--click"]));
    let png = runtime.join("own.png");
    let shot = json(&ok(ctl(&runtime, Some(&token), &["capture", "--png", png.to_str().unwrap()])));
    assert_eq!((shot["width"].as_i64(), shot["height"].as_i64()), (Some(640), Some(480)));
    assert!(png.metadata().is_ok_and(|m| m.len() > 0));
    // capture --space: the root's way to a space's screen.
    let by_id = json(&ok(ctl(&runtime, None, &["capture", "--space", "space-1"])));
    assert_eq!(by_id["width"], 640);
    // The person's output is not the space token's.
    assert!(!ctl(&runtime, Some(&token), &["capture", "--output", "HEADLESS-1"]).0);

    // launch: only with the space's token; --cwd and --env reach the program.
    let seen = runtime.join("seen");
    let line = format!("echo \"$PWD $CTL_TEST $WAYLAND_DISPLAY\" > {0}.tmp; mv {0}.tmp {0}; exec sleep 30", seen.display());
    let launch = ["launch", "--cwd", "/tmp", "--env", "CTL_TEST=yes", "--", "sh", "-c", &line];
    let refused = ctl(&runtime, None, &launch);
    assert!(!refused.0 && refused.2.contains("space grant"), "{}", refused.2);
    let pid = json(&ok(ctl(&runtime, Some(&token), &launch)))["pid"].as_u64().expect("a pid");
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while !seen.exists() {
        assert!(std::time::Instant::now() < deadline, "the launched program never ran");
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    assert_eq!(std::fs::read_to_string(&seen).unwrap().trim(), format!("/tmp yes {socket}"));
    assert!(Path::new(&format!("/proc/{pid}")).exists());
    assert!(!ctl(&runtime, Some(&token), &["launch", "--", "no-such-program-ede-126"]).0);

    // destroy: the space, its socket and what it launched go.
    assert!(!ctl(&runtime, None, &["space", "destroy", "space-9"]).0);
    ok(ctl(&runtime, None, &["space", "destroy", "space-1"]));
    assert_eq!(ok(ctl(&runtime, None, &["space", "list"])), "");
    assert!(!Path::new(&socket).exists());
    assert!(!ctl(&runtime, Some(&token), &["outputs"]).0, "the space's token still lists a space");
    // Bad arguments are the usage, never a call.
    assert!(!ctl(&runtime, None, &["space"]).0 && !ctl(&runtime, None, &["launch", "sh"]).0);
}
