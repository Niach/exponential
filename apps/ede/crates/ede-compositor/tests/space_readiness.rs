//! #EXP-1514 (#EXP-2011, the flip): `space-readiness`. Every compositor
//! that makes spaces says it in its hello and answers from what holds now
//! (`spaces::readiness`): ready with the person's user manager (its tools,
//! its private socket, memory and pids delegated), the space guard and
//! polkit refusing a space's process, and no failed session since the last
//! one that started; else the block. Any person-scope grant may ask, a
//! space grant never. A compositor that makes no spaces reads the op as an
//! older one does.

mod common;

use std::{
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
};

use common::space::start_with_spaces;
use ede_compositor::spaces::slice::{Machine, Units};
use ede_protocol::{Capability, Client, Error, ErrorCode, SessionId, FEATURE_SPACES, FEATURE_SPACE_READINESS};
use ede_space_guard::client::Probed;

const GIB: u64 = 1 << 30;

/// A user manager made of stand-ins: every tool an executable file on a
/// `PATH` of its own, the private socket in a runtime directory, the
/// manager's cgroup with the controllers it was given.
struct Faked {
    dir: PathBuf,
}

impl Faked {
    const TOOLS: [&'static str; 5] = ["systemd-run", "busctl", "systemctl", "systemd", "dbus-daemon"];

    fn new(dir: &Path) -> Self {
        let dir = dir.join("faked");
        for tool in Self::TOOLS {
            let path = dir.join("bin").join(tool);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, "#!/bin/sh\nexit 0\n").unwrap();
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        std::fs::create_dir_all(dir.join("runtime/systemd")).unwrap();
        std::fs::write(dir.join("runtime/systemd/private"), "").unwrap();
        std::fs::create_dir_all(dir.join("user@1000.service/app.slice")).unwrap();
        std::fs::write(dir.join("user@1000.service/cgroup.controllers"), "cpuset cpu io memory pids\n").unwrap();
        Self { dir }
    }

    fn tool(&self, name: &str) -> PathBuf {
        self.dir.join("bin").join(name)
    }

    fn units(&self) -> Units {
        Units {
            systemd_run: self.tool("systemd-run"),
            busctl: self.tool("busctl"),
            systemctl: self.tool("systemctl"),
            version: 255,
            shim: env!("CARGO_BIN_EXE_exponential-compositor").into(),
            runtime: self.dir.join("runtime"),
            bus: None,
            compositor: std::process::id(),
            machine: Machine { cpus: 8, memory: 16 * GIB },
            app_slice: self.dir.join("user@1000.service/app.slice"),
            manager: self.tool("systemd"),
            dbus_daemon: self.tool("dbus-daemon"),
            guard: None,
        }
    }
}

fn block(root: &mut Client) -> Option<String> {
    let answer = root.space_readiness().unwrap();
    assert_eq!(answer.ready, answer.block.is_none(), "{answer:?}");
    answer.block
}

#[test]
fn every_compositor_that_makes_spaces_says_it_and_answers_why_not() {
    let (h, _dir) = start_with_spaces(&[(800, 600)]);
    let mut root = h.ipc();
    assert!(root.hello.features.iter().any(|f| f == FEATURE_SPACES) && root.hello.features.iter().any(|f| f == FEATURE_SPACE_READINESS), "{:?}", root.hello.features);
    assert!(root.hello.says_space_readiness());
    // The harness asked no user manager: no space is offered, and why.
    let answer = root.space_readiness().unwrap();
    assert_eq!((answer.ready, answer.block.as_deref(), answer.max), (false, Some("no_user_manager"), Some(4)), "no user manager here: today's four");
    // Any person-scope grant may ask, as `list_windows`.
    let watcher = root.mint(SessionId::new("watcher").unwrap(), vec![Capability::Capture], None).unwrap();
    assert_eq!(h.ipc_with(watcher.token).unwrap().space_readiness().unwrap().block.as_deref(), Some("no_user_manager"));
    // A space grant never.
    let grant = root.mint(SessionId::new("run-1").unwrap(), vec![Capability::Space], None).unwrap();
    let mut agent = h.ipc_with(grant.token).unwrap();
    assert!(agent.hello.says_space_readiness());
    assert!(agent.space_readiness().is_err_and(|err| err.has_code(ErrorCode::Forbidden)));
    assert!(root.windows().is_ok(), "the connection goes on");
}

#[test]
fn ready_from_the_real_checks_and_each_block() {
    let (h, dir) = start_with_spaces(&[(800, 600)]);
    let faked = Faked::new(dir.path());
    let units = faked.units();
    let guard = dir.path().join("guard.sock");
    h.with(move |state| {
        state.spaces.units = Some(units);
        state.spaces.guard.required = Some(guard);
        *state.spaces.guard.probe.lock().unwrap() = Some(Ok(Probed::Wayland));
        *state.spaces.guard.polkit.lock().unwrap() = Some(Ok(()));
    });
    let mut root = h.ipc();
    let answer = root.space_readiness().unwrap();
    assert!(answer.ready && answer.block.is_none(), "{answer:?}");
    assert_eq!(answer.max, Some(8), "one space per GiB of half of 16 GiB");

    // The user manager, as it is now: each tool gone.
    for tool in Faked::TOOLS {
        let (path, away) = (faked.tool(tool), faked.dir.join(format!("{tool}.away")));
        std::fs::rename(&path, &away).unwrap();
        assert_eq!(block(&mut root).as_deref(), Some("no_user_manager"), "{tool} is gone");
        assert!(h.with(|state| state.space_readiness_why()).unwrap_err().cause.contains(tool), "the cause names {tool}");
        std::fs::rename(&away, &path).unwrap();
        assert_eq!(block(&mut root), None, "{tool} is back");
    }
    // Its private socket gone (the user manager ended).
    let private = faked.dir.join("runtime/systemd/private");
    std::fs::remove_file(&private).unwrap();
    assert_eq!(block(&mut root).as_deref(), Some("no_user_manager"));
    std::fs::write(&private, "").unwrap();
    // Memory or pids not delegated: a space's limits would not hold.
    let controllers = faked.dir.join("user@1000.service/cgroup.controllers");
    for given in ["cpu pids", "cpu memory", ""] {
        std::fs::write(&controllers, given).unwrap();
        assert_eq!(block(&mut root).as_deref(), Some("no_user_manager"), "{given:?}");
    }
    std::fs::write(&controllers, "memory pids").unwrap();
    assert_eq!(block(&mut root), None);

    // The guard and polkit (#EDE-146..148): not installed, still asking,
    // refusing, polkit not refusing a space's process.
    let probe = h.with(|state| state.spaces.guard.probe.clone());
    let polkit = h.with(|state| state.spaces.guard.polkit.clone());
    let required = h.with(|state| state.spaces.guard.required.take());
    assert_eq!(block(&mut root).as_deref(), Some("guard"), "no guard installed");
    h.with(move |state| state.spaces.guard.required = required);
    for answer in [None, Some(Err("the space guard: busy".to_string()))] {
        *probe.lock().unwrap() = answer.clone();
        assert_eq!(block(&mut root).as_deref(), Some("guard"), "{answer:?}");
    }
    *probe.lock().unwrap() = Some(Ok(Probed::Older));
    assert_eq!(block(&mut root), None, "a guard from before #EDE-145 counts");
    *polkit.lock().unwrap() = Some(Err("pkcheck=0".into()));
    assert_eq!(block(&mut root).as_deref(), Some("guard"), "polkit authorizes a space's process");
    *polkit.lock().unwrap() = Some(Ok(()));

    // The latch: a space's session failed, until one starts.
    h.with(|state| state.spaces.session_failed = Some("space-1: its user manager ended".into()));
    assert_eq!(block(&mut root).as_deref(), Some("session"));
    h.with(|state| state.spaces.session_failed = None);
    assert_eq!(block(&mut root), None);

    // The order: the user manager, then the guard, then the latch.
    h.with(|state| state.spaces.session_failed = Some("latched".into()));
    *polkit.lock().unwrap() = None;
    assert_eq!(block(&mut root).as_deref(), Some("guard"));
    std::fs::remove_file(&private).unwrap();
    assert_eq!(block(&mut root).as_deref(), Some("no_user_manager"));
}

#[test]
fn a_compositor_that_makes_no_spaces_says_no_readiness() {
    let h = ede_compositor::backend::headless::Harness::start(ede_compositor::Config::default(), &[(800, 600)]);
    let mut root = h.ipc();
    assert!(!root.hello.offers_spaces() && !root.hello.says_space_readiness());
    assert!(matches!(root.space_readiness(), Err(Error::Unsupported(_))), "read as an older compositor does");
    assert!(root.windows().is_ok(), "the connection goes on");
}
