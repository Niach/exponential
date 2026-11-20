//! #EDE-138 with a REAL systemd user manager: where a launch into a space
//! ends up, what limits it, and that nothing of it outlives its space or
//! its compositor.
//!
//! The gate's Linux runner is a container without systemd, so this runs in
//! the `ede-scope` job (a hosted runner with a user manager), which sets
//! `EDE_EXPECT_USER_MANAGER=1`: there a skip is a failure. Elsewhere, a
//! machine without a user manager (or with a systemd before 252) skips.
//!
//! One test: it stands for one compositor (the process id is in every unit
//! name), and the end of its harness stops the slice of all its spaces.
//! The job turns lingering on for its user, as the installed daemon does:
//! the user manager then outlives a logout, so nothing but the end of the
//! compositor (its own, or the one exponential-session gives it) stops
//! what a space left behind.

mod common;

use std::{
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};

use common::space::start_with_spaces;
use ede_compositor::spaces::slice::limits;
use ede_protocol::{
    process::{cgroup, space_slice, spaces_slice},
    Capability, Membership, ProcessSpace, SessionId, SpaceId,
};

/// The compositor's binary: a launch's stages and `space-end`.
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

fn pid_in(file: &Path) -> u32 {
    until(&format!("{} was never written", file.display()), || std::fs::read_to_string(file).is_ok_and(|text| text.trim().parse::<u32>().is_ok()));
    std::fs::read_to_string(file).unwrap().trim().parse().unwrap()
}

fn systemctl(args: &[&str]) -> bool {
    Command::new("systemctl").arg("--user").args(args).stdout(Stdio::null()).stderr(Stdio::null()).status().is_ok_and(|status| status.success())
}

/// Whatever this test leaves when it fails half way: gone.
struct Stopped(Vec<String>);

impl Drop for Stopped {
    fn drop(&mut self) {
        for unit in &self.0 {
            systemctl(&["stop", unit]);
        }
    }
}

/// `systemd-run --user --scope` as this test asks it of the user manager
/// itself: with `--expand-environment=no` where systemd has it (from 258 a
/// scope's argv has `$$` and the like replaced on the client's side).
fn scope_run(systemd: u32) -> Command {
    let mut run = Command::new("systemd-run");
    run.args(["--user", "--scope", "--collect", "--quiet"]);
    if systemd >= 254 {
        run.arg("--expand-environment=no");
    }
    run.stdin(Stdio::null());
    run
}

/// What a compositor with process id `compositor` (nobody's) would have
/// left: a space's slice with a scope that asked to be left alone at a
/// stop, and in it a shell that ignores SIGTERM. Its process id is
/// written to `file`.
fn left_behind(systemd: u32, compositor: u32, file: &Path) -> Child {
    let script = format!("trap '' TERM; echo $$ > {0}.tmp && mv {0}.tmp {0}; sleep 300; sleep 300", file.display());
    scope_run(systemd).args(["-p", "KillMode=none", &format!("--slice={}", space_slice(compositor, 1)), &format!("--unit=exponential-p{compositor}-s1-1.scope"), "--", "sh", "-c", &script]).spawn().unwrap()
}

#[test]
fn a_launch_runs_in_its_spaces_slice_under_the_persons_limits_and_ends_with_it() {
    let me = std::process::id();
    // Two compositors that are gone (process ids above any there is).
    let (crashed, ended) = (4_194_997, 4_194_998);
    let (all, one) = (spaces_slice(me), space_slice(me, 1));
    let _stopped = Stopped(vec![all.clone(), spaces_slice(crashed), spaces_slice(ended)]);
    let systemd = match ede_compositor::spaces::slice::Units::probe() {
        Ok(units) => units.version,
        Err(reason) => {
            assert!(std::env::var_os("EDE_EXPECT_USER_MANAGER").is_none(), "this check needs a systemd user manager: {reason}");
            eprintln!("skipped: {reason}");
            return;
        }
    };
    let (h, dir) = start_with_spaces(&[(800, 600)]);

    // What a crashed compositor left is there when this one starts.
    let crashed_file = dir.path().join("crashed.pid");
    let mut crashed_scope = left_behind(systemd, crashed, &crashed_file);
    let crashed_pid = pid_in(&crashed_file);

    // The session's start finds the user manager and stops what is stale.
    let config = ede_compositor::config::Spaces { tasks_max: 777, ..Default::default() };
    let units = {
        let config = config.clone();
        h.with(move |state| {
            state.config.spaces = config;
            state.start_space_units();
            // A launch's stages are the compositor's binary, not this test.
            if let Some(units) = &mut state.spaces.units {
                units.shim = COMPOSITOR.into();
            }
            state.spaces.units.clone()
        })
    };
    let units = units.expect("the session's start found the user manager a moment after the probe did");
    if std::env::var_os("EDE_EXPECT_USER_MANAGER").is_some() {
        let user = smithay::reexports::rustix::process::getuid().as_raw().to_string();
        let linger = Command::new("loginctl").args(["show-user", &user, "--property=Linger", "--value"]).output().unwrap();
        assert_eq!(String::from_utf8_lossy(&linger.stdout).trim(), "yes", "this job runs with lingering on: the user manager outlives a logout");
    }
    until("what a crashed compositor left still runs after the next one's start", || !alive(crashed_pid) && !units.app_slice.join(spaces_slice(crashed)).exists());
    let _ = crashed_scope.wait();

    let (expected_all, expected_one) = limits(&config, units.machine);
    let limit = |list: &[(String, u64)], name: &str| list.iter().find(|(n, _)| n == name).map(|(_, v)| *v).unwrap();
    let grant = h.ipc().mint(SessionId::new("run-1").unwrap(), vec![Capability::Space], None).unwrap();
    let mut agent = h.ipc_with(grant.token).unwrap();
    let space = agent.create_space(640, 480).unwrap();
    let runtime = Path::new(&space.wayland_display).parent().unwrap().to_path_buf();
    let mut root = h.ipc();

    // A launch: in a scope of the space's slice, out of the compositor's
    // own cgroup (in a real session: the person's login session scope),
    // with the caller's variables and the space's runtime directory. (The
    // real systemd-run hands the last stage its descriptor and its input.)
    let env_file = dir.path().join("first.env");
    let script = format!("env > {0}.tmp && mv {0}.tmp {0}; exec sleep 300", env_file.display());
    let pid = agent.launch(vec!["sh".into(), "-c".into(), script], [("EDE_TEST_MARK".to_string(), "the-callers".to_string())].into(), None).unwrap();
    let in_slice = format!("/{all}/{one}/exponential-p{me}-s1-");
    until("the launch never reached a scope of its space's slice", || cgroup(pid).is_some_and(|c| c.contains(&in_slice)));
    let own = cgroup(me).unwrap();
    let launched = cgroup(pid).unwrap();
    assert_ne!(launched, own, "the launch stayed in the compositor's cgroup");
    assert!(!launched.contains("/session-"), "the launch is in a login session scope: {launched}");
    until("the launched program never ran", || env_file.exists());
    let env = std::fs::read_to_string(&env_file).unwrap();
    assert!(env.lines().any(|line| line == "EDE_TEST_MARK=the-callers") && env.lines().any(|line| line == format!("XDG_RUNTIME_DIR={}", runtime.display())), "{env}");

    // The person's limits, as the kernel holds them, where the session's
    // start said the user manager's cgroups are.
    let (space_dir, all_dir): (PathBuf, PathBuf) = (units.space_cgroup(1), units.all_cgroup());
    let read = |dir: &Path, file: &str| std::fs::read_to_string(dir.join(file)).unwrap_or_else(|e| panic!("{}/{file}: {e}", dir.display())).trim().to_string();
    assert_eq!(read(&space_dir, "memory.max"), limit(&expected_one, "MemoryMax").to_string());
    assert_eq!(read(&space_dir, "memory.high"), limit(&expected_one, "MemoryHigh").to_string());
    assert_eq!(read(&space_dir, "pids.max"), "777");
    assert_eq!(read(&all_dir, "memory.max"), limit(&expected_all, "MemoryMax").to_string());
    assert_eq!(read(&all_dir, "memory.swap.max"), limit(&expected_all, "MemorySwapMax").to_string());
    assert_eq!(read(&all_dir, "cpu.max"), format!("{} 100000", limit(&expected_all, "CPUQuotaPerSecUSec") / 10), "microseconds of CPU per 100 ms");
    assert_eq!(read(&all_dir, "cpu.weight"), "10");

    // Whose process it is: the cgroup says so first.
    let space_id = SpaceId::new("space-1").unwrap();
    assert_eq!(root.space_of_process(pid).unwrap(), Some(ProcessSpace { space: space_id.clone(), by: Membership::Cgroup, pid }));
    assert_eq!(root.space_of_process(me).unwrap(), None, "the compositor's own process");

    // #EDE-143: an orphan in a session of its own with an empty environment,
    // started from a launch. Its parent is gone, it left the launch's group,
    // its environment names nothing and it is no client of the space: the
    // four older signs say nothing. Its cgroup is still the launch's scope.
    let orphan_file = dir.path().join("orphan.pid");
    let script = format!("setsid -f env -i /bin/sh -c 'echo $$ > {0}.tmp && mv {0}.tmp {0}; exec sleep 300'; exec sleep 300", orphan_file.display());
    let parent = agent.launch(vec!["sh".into(), "-c".into(), script], Default::default(), None).unwrap();
    let orphan = pid_in(&orphan_file);
    until("the orphan never became sleep", || std::fs::read_to_string(format!("/proc/{orphan}/comm")).is_ok_and(|comm| comm.trim() == "sleep"));
    let lineage = || ede_protocol::process::lineage(orphan).iter().map(|stat| stat.pid).collect::<Vec<u32>>();
    until("the orphan still descends from its launch", || !lineage().contains(&parent));
    assert!(!lineage().contains(&me), "the orphan descends from the compositor: {:?}", lineage());
    assert_eq!(root.space_of_process(orphan).unwrap(), Some(ProcessSpace { space: space_id.clone(), by: Membership::Cgroup, pid: orphan }));
    let without_the_cgroup = h.with(move |state| {
        let units = state.spaces.units.take();
        let found = state.space_of_process(orphan);
        state.spaces.units = units;
        found
    });
    assert_eq!(without_the_cgroup, None, "another sign still ties the orphan to its space: this check proves nothing then");

    // A scope in the space's slice that asked to be left alone at a stop,
    // with a child that ignores SIGTERM in a session of its own.
    let stubborn_file = dir.path().join("stubborn.pid");
    let stubborn_unit = format!("exponential-p{me}-s1-stubborn.scope");
    let script = format!("trap '' TERM; setsid sh -c 'trap \"\" TERM; echo $$ > {0}.tmp && mv {0}.tmp {0}; sleep 300; sleep 300' & wait", stubborn_file.display());
    let mut stubborn_scope = scope_run(systemd).args(["-p", "KillMode=none", &format!("--slice={one}"), &format!("--unit={stubborn_unit}"), "--", "sh", "-c", &script]).spawn().unwrap();
    let stubborn = pid_in(&stubborn_file);
    assert!(cgroup(stubborn).unwrap().contains(&format!("/{one}/{stubborn_unit}")));

    // The space closes: everything in its slice ends, whatever it asked
    // for, and the slice is gone.
    agent.destroy_space(space.id.clone()).unwrap();
    until("a process of the closed space still runs", || !alive(pid) && !alive(parent) && !alive(orphan) && !alive(stubborn));
    let _ = stubborn_scope.wait();
    until("the closed space's slice is still there", || !space_dir.exists() && !systemctl(&["is-active", "--quiet", &one]));
    until("the closed space was never done with", || h.with(|state| state.spaces.settled()));

    // A compositor that ended without stopping anything (the session's
    // SIGTERM, a crash): exponential-session runs `space-end` for it.
    let ended_file = dir.path().join("ended.pid");
    let mut ended_scope = left_behind(systemd, ended, &ended_file);
    let ended_pid = pid_in(&ended_file);
    let ended_dir = units.app_slice.join(spaces_slice(ended));
    assert!(ended_dir.is_dir() && alive(ended_pid));
    let status = Command::new(COMPOSITOR).args(["space-end", &ended.to_string()]).status().unwrap();
    assert!(status.success(), "space-end: {status}");
    assert!(!alive(ended_pid), "what an ended compositor's space left still runs after space-end");
    let _ = ended_scope.wait();
    until("the ended compositor's slice is still there", || !ended_dir.exists() && !systemctl(&["is-active", "--quiet", &spaces_slice(ended)]));

    // The compositor ends with a space still open, and in it a process
    // that left its launch's group for a session of its own and ignores
    // SIGTERM. The user manager may linger after the person's logout, so
    // the compositor's own end is what stops the slice of all its spaces:
    // when it has ended, nothing of the slice is left.
    let grant = h.ipc().mint(SessionId::new("run-2").unwrap(), vec![Capability::Space], None).unwrap();
    let mut second = h.ipc_with(grant.token).unwrap();
    second.create_space(640, 480).unwrap();
    let left_file = dir.path().join("left.pid");
    let script = format!("setsid sh -c 'trap \"\" TERM; echo $$ > {0}.tmp && mv {0}.tmp {0}; sleep 300; sleep 300' & exec sleep 300", left_file.display());
    let launch = second.launch(vec!["sh".into(), "-c".into(), script], Default::default(), None).unwrap();
    let left = pid_in(&left_file);
    until("the second space's process never reached its slice", || cgroup(left).is_some_and(|c| c.contains(&format!("/{all}/{}/", space_slice(me, 2)))));
    drop(h);
    assert!(!alive(left) && !alive(launch), "a process of a space outlived the compositor");
    until("the slice of all spaces outlived the compositor", || !all_dir.exists() && !systemctl(&["is-active", "--quiet", &all]));
    assert!(!units.app_slice.join("app-exponential.slice").exists(), "a parent slice is left behind");
}
