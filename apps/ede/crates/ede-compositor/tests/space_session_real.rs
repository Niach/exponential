//! #EDE-138 with a REAL systemd user manager: a space's own user manager
//! and its one bus. Run where `space_slice_real.rs` runs (the `ede-scope`
//! job, `EDE_EXPECT_USER_MANAGER=1`: a skip is a failure there).
//!
//! What it shows: two launches of a space share one bus, and its user
//! manager is on it; what snapd and Ptyxis ask of "the" user manager lands
//! in the space's slice; a service the bus starts runs there too, with the
//! space's environment; the space's manager loads none of the
//! distribution's or the person's units; the close ends all of it.

mod common;

use std::{
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{Duration, Instant},
};

use common::space::start_with_spaces;
use ede_protocol::{
    process::{cgroup, space_slice, spaces_slice},
    Capability, SessionId,
};

const COMPOSITOR: &str = env!("CARGO_BIN_EXE_exponential-compositor");

fn until(what: &str, mut ok: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(20);
    while !ok() {
        assert!(Instant::now() < deadline, "{what}");
        std::thread::sleep(Duration::from_millis(20));
    }
}

fn alive(pid: u32) -> bool {
    std::fs::read_to_string(format!("/proc/{pid}/stat")).is_ok_and(|stat| !stat.rsplit(')').next().unwrap_or_default().trim_start().starts_with('Z'))
}

/// What a launch wrote to `file` (it writes `<file>.tmp` and renames it).
fn written(file: &Path) -> String {
    until(&format!("{} was never written", file.display()), || file.exists());
    std::fs::read_to_string(file).unwrap()
}

/// A launch's shell line: `script`, then it stays.
fn staying(script: String) -> Vec<String> {
    vec!["sh".into(), "-c".into(), format!("{script}; exec sleep 300")]
}

/// `> file` the way the launches write: whole or not at all.
fn into(file: &Path) -> String {
    format!("> {0}.tmp && mv {0}.tmp {0}", file.display())
}

struct Stopped(String);

impl Drop for Stopped {
    fn drop(&mut self) {
        let _ = Command::new("systemctl").args(["--user", "stop", &self.0]).stdout(Stdio::null()).stderr(Stdio::null()).status();
    }
}

#[test]
fn a_space_has_its_own_user_manager_and_one_bus() {
    if let Err(reason) = ede_compositor::spaces::slice::Units::probe() {
        assert!(std::env::var_os("EDE_EXPECT_USER_MANAGER").is_none(), "this check needs a systemd user manager: {reason}");
        eprintln!("skipped: {reason}");
        return;
    }
    let me = std::process::id();
    let _stopped = Stopped(spaces_slice(me));
    let (h, dir) = start_with_spaces(&[(800, 600)]);
    let units = h.with(|state| {
        state.start_space_units();
        if let Some(units) = &mut state.spaces.units {
            units.shim = COMPOSITOR.into();
        }
        state.spaces.units.clone()
    });
    let units = units.expect("the probe found the user manager a moment ago");
    let grant = h.ipc().mint(SessionId::new("run-1").unwrap(), vec![Capability::Space], None).unwrap();
    let mut agent = h.ipc_with(grant.token).unwrap();
    let space = agent.create_space(640, 480).unwrap();
    let space_dir = Path::new(&space.wayland_display).parent().unwrap().to_path_buf();
    let out = |name: &str| dir.path().join(name);

    // A service the space's bus can start, in the space's own data
    // directory (its `XDG_DATA_HOME`): before the bus starts, it reads it then.
    let home = dir.path().join("homes").join(format!("p{me}")).join("space-1");
    std::fs::create_dir_all(home.join("data/dbus-1/services")).unwrap();
    let echo = format!("cat /proc/self/cgroup {} ; env {}; exec sleep 30", into(&out("activated.cgroup")), into(&out("activated.env")));
    std::fs::write(home.join("data/dbus-1/services/at.exponential.TestEcho.service"), format!("[D-BUS Service]\nName=at.exponential.TestEcho\nExec=/bin/sh -c '{echo}'\n")).unwrap();

    // Two launches: one bus, the space's.
    let first = agent.launch(staying(format!("echo \"$DBUS_SESSION_BUS_ADDRESS\" {}", into(&out("first.bus")))), Default::default(), None).unwrap();
    let second = agent.launch(staying(format!("echo \"$DBUS_SESSION_BUS_ADDRESS\" {}", into(&out("second.bus")))), Default::default(), None).unwrap();
    let address = written(&out("first.bus"));
    assert_eq!(address, written(&out("second.bus")), "two launches of one space, two buses");
    assert!(address.starts_with(&format!("unix:path={};unix:abstract=/tmp/dbus-ede-", space_dir.join("bus").display())), "{address}");

    // The space's manager is on it, under the space's slice.
    let session_scope = units.space_cgroup(1).join(format!("exponential-p{me}-s1-session1.scope"));
    let manager: u32 = std::fs::read_to_string(session_scope.join("init.scope/cgroup.procs")).unwrap().trim().parse().unwrap();
    assert!(std::fs::read_to_string(format!("/proc/{manager}/cmdline")).unwrap().starts_with("/"), "a manager process");
    let owner = out("owner.txt");
    agent
        .launch(staying(format!("busctl --user --timeout=5 call org.freedesktop.DBus /org/freedesktop/DBus org.freedesktop.DBus GetNameOwner s org.freedesktop.systemd1 {}", into(&owner))), Default::default(), None)
        .unwrap();
    assert!(written(&owner).starts_with("s \":1."), "org.freedesktop.systemd1 has no owner on the space's bus");

    // What Ptyxis does for its shell: `systemd-run --user --scope`, which
    // finds `$XDG_RUNTIME_DIR/systemd/private`, the space's manager.
    let ptyxis = out("ptyxis.cgroup");
    agent.launch(staying(format!("systemd-run --user --scope --collect --quiet --unit=ptyxis-spawn-test.scope -- sh -c 'cat /proc/self/cgroup {}'", into(&ptyxis))), Default::default(), None).unwrap();
    let in_session = format!("/{}/{}/exponential-p{me}-s1-session1.scope/app.slice/", spaces_slice(me), space_slice(me, 1));
    assert!(written(&ptyxis).contains(&format!("{in_session}ptyxis-spawn-test.scope")), "{}", written(&ptyxis));

    // What `snap run` does: a scope for itself through the session bus's
    // org.freedesktop.systemd1, over the abstract socket (all a strict
    // snap may reach).
    let snap = out("snap.cgroup");
    let script = format!(
        "busctl --address=\"${{DBUS_SESSION_BUS_ADDRESS#*;}}\" call org.freedesktop.systemd1 /org/freedesktop/systemd1 org.freedesktop.systemd1.Manager StartTransientUnit 'ssa(sv)a(sa(sv))' snap.test.test-11111111-2222-3333-4444-555555555555.scope fail 1 PIDs au 1 $$ 0 >/dev/null; sleep 0.5; cat /proc/self/cgroup {}",
        into(&snap)
    );
    agent.launch(staying(script), Default::default(), None).unwrap();
    assert!(written(&snap).contains(&format!("{in_session}snap.test.test-11111111-2222-3333-4444-555555555555.scope")), "{}", written(&snap));

    // What the bus starts runs in the space's slice, with the space's environment.
    agent.launch(staying("busctl --user --timeout=2 call org.freedesktop.DBus /org/freedesktop/DBus org.freedesktop.DBus StartServiceByName su at.exponential.TestEcho 0 || true".to_string()), Default::default(), None).unwrap();
    assert!(written(&out("activated.cgroup")).contains(&format!("{in_session}dbus.service")), "{}", written(&out("activated.cgroup")));
    let activated = written(&out("activated.env"));
    assert!(activated.lines().any(|line| line == format!("XDG_RUNTIME_DIR={}", space_dir.display())), "{activated}");
    assert!(activated.lines().any(|line| line == format!("WAYLAND_DISPLAY={}", space.wayland_display)), "{activated}");

    // Nothing of the distribution's or the person's: one service, one socket.
    // (A unit only named by another, as systemd 259's mounts name
    // systemd-remount-fs.service, is `not-found`: nothing of it is there.)
    let loaded = out("units.txt");
    agent.launch(staying(format!("systemctl --user list-units --all --plain --no-legend --state=loaded --type=service,socket,timer,path {}", into(&loaded))), Default::default(), None).unwrap();
    let names: Vec<String> = written(&loaded).lines().filter_map(|line| line.split_whitespace().next().map(str::to_string)).collect();
    assert!(names.iter().all(|name| matches!(name.as_str(), "dbus.service" | "dbus.socket")), "the space's manager loaded {names:?}");
    assert!(names.contains(&"dbus.service".to_string()));

    // The close: the manager, its bus and all they started are gone.
    let bus_pids: Vec<u32> = std::fs::read_to_string(session_scope.join("app.slice/dbus.service/cgroup.procs")).unwrap().lines().filter_map(|pid| pid.parse().ok()).collect();
    assert!(!bus_pids.is_empty());
    agent.destroy_space(space.id.clone()).unwrap();
    until("the space's manager or bus outlived it", || !alive(manager) && bus_pids.iter().all(|pid| !alive(*pid)) && !alive(first) && !alive(second));
    until("the space's directory and its bus socket outlived it", || !space_dir.exists());
    until("the space's slice outlived it", || !units.space_cgroup(1).exists());
    until("the closed space was never done with", || h.with(|state| state.spaces.settled()));
    assert!(cgroup(me).is_some());
    drop(h);
    let _: PathBuf = units.all_cgroup();
}
