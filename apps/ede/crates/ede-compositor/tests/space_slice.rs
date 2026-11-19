//! #EDE-138 headless: a launch into a space runs in a systemd scope of the
//! space's own slice. Here with stand-ins for `busctl`, `systemd-run` and
//! `systemctl` that write down what they were asked: what the compositor
//! asks of the person's user manager, in which order, with whose runtime
//! directory, that nothing the caller chose reaches a program before the
//! scope, and that the compositor never waits for an answer in its event
//! loop. `space_slice_real.rs` asks a real user manager.

mod common;

use std::{
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use common::space::start_with_spaces;
use ede_compositor::{
    backend::headless::Harness,
    spaces::slice::{Machine, Units},
};
use ede_protocol::{Capability, Client, SessionId, SpaceId, SpaceInfo};

const GIB: u64 = 1 << 30;
/// A variable only the launch's caller sets.
const MARK: &str = "EDE_TEST_MARK";

/// A memory limit is asked for in whole pages (of 64 KiB, the largest).
fn pages(bytes: u64) -> u64 {
    bytes / (64 << 10) * (64 << 10)
}

/// The stand-ins, in a directory of their own. Each appends one line to
/// `log` there: its name, every argument in brackets, the two variables
/// that say whose user manager it would reach, and the caller's mark if
/// it got that far. `cgroup/` stands for the user manager's `app.slice`:
/// a slice that is made has its directory there, a stop removes it
/// (unless `stubborn` is there: something in it ignores the stop).
struct Tools {
    dir: PathBuf,
}

impl Tools {
    fn new(dir: &Path) -> Self {
        let dir = dir.join("tools");
        std::fs::create_dir_all(dir.join("cgroup")).unwrap();
        // The person's runtime directory with their user manager's socket.
        std::fs::create_dir_all(dir.join("persons-runtime/systemd")).unwrap();
        std::fs::write(dir.join("persons-runtime/systemd/private"), "").unwrap();
        let log = r#"d=$(dirname "$0"); { printf '%s' "$(basename "$0")"; for a; do printf ' [%s]' "$a"; done; printf ' XDG_RUNTIME_DIR=%s BUS=%s MARK=%s\n' "$XDG_RUNTIME_DIR" "$DBUS_SESSION_BUS_ADDRESS" "$EDE_TEST_MARK"; } >> "$d/log""#;
        // The cgroup directory of slice `$unit`: a space's is in its compositor's.
        let place = r#"case "$unit" in *-s[0-9]*.slice) place="$d/cgroup/${unit%-s*}.slice/$unit";; *) place="$d/cgroup/$unit";; esac"#;
        // A scope: the variables of `--setenv`, then the program in this process.
        let systemd_run = format!(
            "#!/bin/sh\n{log}\n[ -e \"$d/refuse-scope\" ] && {{ echo 'Failed to start transient scope unit' >&2; exit 1; }}\nwhile [ $# -gt 0 ]; do case \"$1\" in --setenv=*) export \"${{1#--setenv=}}\";; --) shift; break;; esac; shift; done\nexec \"$@\"\n"
        );
        // `StartTransientUnit` fails for a slice that is there, as the real
        // one does; `refuse-slices` fails every call.
        let busctl = format!(
            "#!/bin/sh\ncase \"$*\" in *GetNameOwner*) exit 0;; esac\n{log}\n[ -e \"$d/refuse-slices\" ] && {{ echo 'Call failed: Access denied' >&2; exit 1; }}\nunit=$9\n{place}\nif [ \"$7\" = StartTransientUnit ]; then\n  [ -d \"$place\" ] && {{ echo \"Call failed: Unit $unit was already loaded or has a fragment file.\" >&2; exit 1; }}\n  mkdir -p \"$place\"\nfi\nexit 0\n"
        );
        let systemctl = format!(
            "#!/bin/sh\n{log}\n[ \"$2\" = list-units ] && [ -e \"$d/units\" ] && cat \"$d/units\"\nif [ \"$2\" = stop ]; then\n  [ -e \"$d/slow\" ] && sleep 3\n  unit=$3\n  {place}\n  [ -e \"$d/stubborn\" ] || rm -rf \"$place\"\nfi\nexit 0\n"
        );
        // The space's own user manager: brings the bus (a file stands for
        // its socket), writes its environment and its process id, stays.
        let systemd = format!(
            "#!/bin/sh\n{log}\n[ -e \"$d/refuse-manager\" ] && exit 1\nspace=$(basename \"$XDG_RUNTIME_DIR\")\nenv > \"$d/manager-$space.env\"\necho $$ > \"$d/manager-$space.pid\"\n: > \"$XDG_RUNTIME_DIR/bus\"\nexec sleep 300\n"
        );
        for (name, script) in [("systemd-run", systemd_run), ("busctl", busctl), ("systemctl", systemctl), ("systemd", systemd)] {
            let path = dir.join(name);
            std::fs::write(&path, script).unwrap();
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        Self { dir }
    }

    fn runtime(&self) -> PathBuf {
        self.dir.join("persons-runtime")
    }

    fn units(&self) -> Units {
        Units {
            systemd_run: self.dir.join("systemd-run"),
            busctl: self.dir.join("busctl"),
            systemctl: self.dir.join("systemctl"),
            version: 255,
            shim: env!("CARGO_BIN_EXE_exponential-compositor").into(),
            runtime: self.runtime(),
            bus: Some("unix:path=/persons/bus".into()),
            compositor: std::process::id(),
            machine: Machine { cpus: 8, memory: 16 * GIB },
            app_slice: self.dir.join("cgroup"),
            manager: self.dir.join("systemd"),
            dbus_daemon: "/usr/bin/dbus-daemon".into(),
            guard: None,
        }
    }

    /// The process id of space `space`'s user manager (the latest start).
    fn manager(&self, space: &str) -> u32 {
        let file = self.dir.join(format!("manager-{space}.pid"));
        until("the space's user manager never started", || std::fs::read_to_string(&file).is_ok_and(|pid| pid.trim().parse::<u32>().is_ok()));
        let pid = std::fs::read_to_string(&file).unwrap().trim().parse().unwrap();
        std::fs::remove_file(file).unwrap();
        pid
    }

    fn flag(&self, name: &str, on: bool) {
        if on {
            std::fs::write(self.dir.join(name), "").unwrap();
        } else {
            std::fs::remove_file(self.dir.join(name)).unwrap();
        }
    }

    fn log(&self) -> Vec<String> {
        std::fs::read_to_string(self.dir.join("log")).unwrap_or_default().lines().map(str::to_string).collect()
    }
}

fn until(what: &str, mut ok: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(15);
    while !ok() {
        assert!(Instant::now() < deadline, "{what}");
        std::thread::sleep(Duration::from_millis(20));
    }
}

fn agent(h: &Harness, session: &str) -> (Client, SpaceInfo) {
    let grant = h.ipc().mint(SessionId::new(session).unwrap(), vec![Capability::Space], None).unwrap();
    let mut agent = h.ipc_with(grant.token).unwrap();
    let space = agent.create_space(640, 480).unwrap();
    (agent, space)
}

/// A program that writes its environment and what its standard input is
/// to `out` and stays.
fn writes_its_environment(out: &Path) -> Vec<String> {
    vec!["sh".into(), "-c".into(), format!("{{ env; echo STDIN=$(readlink /proc/$$/fd/0); }} > '{0}.tmp' && mv '{0}.tmp' '{0}'; exec sleep 60", out.display())]
}

fn alive(pid: u32) -> bool {
    std::fs::read_to_string(format!("/proc/{pid}/stat")).is_ok_and(|stat| !stat.rsplit(')').next().unwrap_or_default().trim_start().starts_with('Z'))
}

#[test]
fn a_launch_asks_the_persons_user_manager_for_its_slices_and_its_scope() {
    let (h, dir) = start_with_spaces(&[(800, 600)]);
    let tools = Tools::new(dir.path());
    let (runtime, units) = (tools.runtime(), tools.units());
    let (all_cgroup, first_cgroup, second_cgroup) = (units.all_cgroup(), units.space_cgroup(1), units.space_cgroup(2));
    h.with(move |state| state.spaces.units = Some(units));
    let me = std::process::id();
    let (mut agent, space) = agent(&h, "run-1");
    let space_dir = Path::new(&space.wayland_display).parent().unwrap().to_path_buf();

    let out = dir.path().join("first.env");
    let callers = [("TOKEN".to_string(), "secret-never-on-an-argument".to_string()), (MARK.to_string(), "the-callers".to_string())];
    let pid = agent.launch(writes_its_environment(&out), callers.into(), None).unwrap();
    until("the launched program never ran", || out.exists());
    let log = tools.log();
    assert_eq!(log.len(), 5, "{log:#?}");
    // Whose user manager, and nothing of the caller's: its variables reach
    // no program that runs before the scope.
    let whose = format!(" XDG_RUNTIME_DIR={} BUS=unix:path=/persons/bus MARK=", runtime.display());

    // The slice of all spaces first, with the limits of an 8-core 16 GiB machine.
    assert_eq!(
        log[0],
        format!(
            "busctl [--user] [--timeout=5] [call] [org.freedesktop.systemd1] [/org/freedesktop/systemd1] [org.freedesktop.systemd1.Manager] [StartTransientUnit] [ssa(sv)a(sa(sv))] [app-exponential_p{me}.slice] [fail] [5] [CPUQuotaPerSecUSec] [t] [4000000] [CPUWeight] [t] [10] [MemoryMax] [t] [{}] [MemoryHigh] [t] [{}] [MemorySwapMax] [t] [{}] [0]{whose}",
            8 * GIB,
            pages(8 * GIB / 10 * 9),
            2 * GIB
        )
    );
    assert_eq!(
        log[1],
        format!(
            "busctl [--user] [--timeout=5] [call] [org.freedesktop.systemd1] [/org/freedesktop/systemd1] [org.freedesktop.systemd1.Manager] [StartTransientUnit] [ssa(sv)a(sa(sv))] [app-exponential_p{me}-s1.slice] [fail] [3] [MemoryMax] [t] [{}] [MemoryHigh] [t] [{}] [TasksMax] [t] [4096] [0]{whose}",
            pages(16 * GIB * 30 / 100),
            pages(pages(16 * GIB * 30 / 100) / 10 * 9)
        )
    );
    // The first program in every scope is `space-enter` with the
    // compositor's directory of spaces (#EDE-146's guard goes there).
    let enter = format!("] [space-enter] [{}] [--] [/proc/self/fd/", space_dir.parent().unwrap().display());
    // The space's first launch starts its own user manager, in a scope of
    // its own whose cgroup is the manager's to manage.
    let session = format!(
        "systemd-run [--user] [--scope] [--collect] [--quiet] [--slice=app-exponential_p{me}-s1.slice] [--unit=exponential-p{me}-s1-session1.scope] [-p] [TimeoutStopSec=2] [-p] [Delegate=yes] [--expand-environment=no] [--setenv=XDG_RUNTIME_DIR={}] [--] [/proc/self/fd/",
        space_dir.display()
    );
    assert!(log[2].starts_with(&session) && log[2].contains(&enter) && log[2].contains("] [space-exec] [") && log[2].contains(&format!("] [--] [{}] [--user] XDG_RUNTIME_DIR=", tools.dir.join("systemd").display())), "{}", log[2]);
    let address = format!("unix:path={};unix:abstract=/tmp/dbus-ede-", space_dir.join("bus").display());
    assert!(log[3].starts_with(&format!("systemd [--user] XDG_RUNTIME_DIR={} BUS={address}", space_dir.display())), "the manager has the space's runtime directory and its bus: {}", log[3]);
    let manager_env = std::fs::read_to_string(tools.dir.join("manager-space-1.env")).unwrap();
    for line in [format!("SYSTEMD_UNIT_PATH={}", space_dir.join("units").display()), format!("SYSTEMD_GENERATOR_PATH={}", space_dir.join("nothing").display()), format!("SYSTEMD_ENVIRONMENT_GENERATOR_PATH={}", space_dir.join("nothing").display())] {
        assert!(manager_env.lines().any(|l| l == line), "{line} is not the manager's: {manager_env}");
    }
    for unit in ["default.target", "basic.target", "dbus.socket", "dbus.service"] {
        assert!(space_dir.join("units").join(unit).is_file(), "{unit}");
    }
    assert!(std::fs::read_to_string(space_dir.join("dbus.conf")).unwrap().contains("<listen>unix:abstract=/tmp/dbus-ede-"));
    let home_config = manager_env.lines().find_map(|line| line.strip_prefix("XDG_CONFIG_HOME=")).unwrap();
    assert!(Path::new(home_config).join("xdg-desktop-portal/exponential-portals.conf").is_file(), "the space's portals");
    // Then the launch's scope, asked of the PERSON's user manager once the
    // space's bus answers. In it the launch's last stage, found through a
    // descriptor, then the program: no `dbus-run-session` (the space has a bus).
    let scope = format!(
        "systemd-run [--user] [--scope] [--collect] [--quiet] [--slice=app-exponential_p{me}-s1.slice] [--unit=exponential-p{me}-s1-1.scope] [-p] [TimeoutStopSec=2] [--expand-environment=no] [--setenv=XDG_RUNTIME_DIR={}] [--] [/proc/self/fd/",
        space_dir.display()
    );
    assert!(log[4].starts_with(&scope) && log[4].contains(&enter) && log[4].contains("] [space-exec] [") && log[4].contains("] [--] [sh] [-c] ") && !log[4].contains("dbus-run-session"), "{}", log[4]);
    assert!(log[4].contains(&format!(" XDG_RUNTIME_DIR={} BUS={address}", runtime.display())) && log[4].ends_with(" MARK="), "systemd-run reaches the person's user manager, and has nothing of the caller's: {}", log[4]);
    assert!(!log.iter().any(|line| line.contains("secret-never-on-an-argument") || line.contains("the-callers")), "the caller's environment is on an argument or in a program before the scope");
    // The program has it, with the space's runtime directory, and its
    // standard input is nothing, as a launch's always was.
    let env = std::fs::read_to_string(&out).unwrap();
    for line in [format!("XDG_RUNTIME_DIR={}", space_dir.display()), format!("WAYLAND_DISPLAY={}", space.wayland_display), "TOKEN=secret-never-on-an-argument".to_string(), format!("{MARK}=the-callers"), "STDIN=/dev/null".to_string()] {
        assert!(env.lines().any(|l| l == line), "{line} is not what the program has: {env}");
    }
    assert!(env.lines().any(|l| l.starts_with(&format!("DBUS_SESSION_BUS_ADDRESS={address}"))), "the space's bus: {env}");
    assert!(!std::fs::read_dir(&space_dir).unwrap().any(|entry| entry.unwrap().file_name().to_string_lossy().starts_with(".launch")), "the caller's environment is still a file");
    // One process from the spawn to the program: the launch's stages are gone.
    let cmdline = String::from_utf8_lossy(&std::fs::read(format!("/proc/{pid}/cmdline")).unwrap()).into_owned();
    assert!(!cmdline.contains("space-run") && !cmdline.contains("space-enter") && !cmdline.contains("space-exec"), "the launch is still {cmdline}");

    // The space's later launches ask for their scope only: its slice is there.
    let second = dir.path().join("second.env");
    agent.launch(writes_its_environment(&second), Default::default(), None).unwrap();
    until("the second program never ran", || second.exists());
    let log = tools.log();
    assert_eq!(log.len(), 6, "one manager per space: {log:#?}");
    assert!(log[5].starts_with(&format!("systemd-run [--user] [--scope] [--collect] [--quiet] [--slice=app-exponential_p{me}-s1.slice] [--unit=exponential-p{me}-s1-2.scope] ")), "{}", log[5]);

    // A second space: the slice of all spaces is there, so its limits are
    // set again (the person's of now); the space's own slice is made.
    let (mut other, other_space) = self::agent(&h, "run-2");
    let third = dir.path().join("third.env");
    let other_pid = other.launch(writes_its_environment(&third), Default::default(), None).unwrap();
    until("the other space's program never ran", || third.exists());
    let log = tools.log();
    assert_eq!(log.len(), 12, "{log:#?}");
    assert!(log[6].contains(&format!(" [StartTransientUnit] [ssa(sv)a(sa(sv))] [app-exponential_p{me}.slice] ")), "{}", log[6]);
    assert!(log[7].contains(&format!(" [SetUnitProperties] [sba(sv)] [app-exponential_p{me}.slice] [true] [5] [CPUQuotaPerSecUSec] [t] [4000000] ")) && !log[7].contains("] [0] XDG"), "{}", log[7]);
    assert!(log[8].contains(&format!(" [StartTransientUnit] [ssa(sv)a(sa(sv))] [app-exponential_p{me}-s2.slice] ")), "{}", log[8]);
    assert!(log[9].contains(&format!(" [--unit=exponential-p{me}-s2-session1.scope] ")) && log[10].starts_with("systemd [--user] "), "{log:#?}");
    assert!(log[11].contains(&format!(" [--slice=app-exponential_p{me}-s2.slice] [--unit=exponential-p{me}-s2-3.scope] ")), "{}", log[11]);

    // The close tells the user manager to stop the space's slice and does
    // not wait for it: the stand-in takes 3 s, the compositor answers at once.
    tools.flag("slow", true);
    let asked = Instant::now();
    agent.destroy_space(space.id.clone()).unwrap();
    h.ipc().windows().unwrap();
    assert!(asked.elapsed() < Duration::from_secs(2), "the close waited for the user manager: {:?}", asked.elapsed());
    until("the user manager was never told to stop the space's slice", || tools.log().len() == 13);
    assert!(tools.log()[12].starts_with(&format!("systemctl [--user] [stop] [app-exponential_p{me}-s1.slice] XDG_RUNTIME_DIR={} BUS=", runtime.display())), "{}", tools.log()[12]);
    until("the closed space's launches outlived it", || !alive(pid));
    until("the closed space's slice was never done with", || h.with(|state| state.spaces.settled()));
    assert!(!first_cgroup.exists() && tools.log().len() == 13, "a slice that the stop ended is not ended again: {:#?}", tools.log());
    tools.flag("slow", false);

    // A slice that outlives its stop (a scope in it asked to be left
    // alone): after the grace the kernel ends what is in its cgroup, and
    // the slice is stopped once more.
    tools.flag("stubborn", true);
    other.destroy_space(other_space.id.clone()).unwrap();
    until("the kernel was never told to end what the stop left", || std::fs::read_to_string(second_cgroup.join("cgroup.kill")).is_ok_and(|told| told == "1"));
    until("the outlived slice was not stopped once more", || tools.log().iter().filter(|line| line.starts_with(&format!("systemctl [--user] [stop] [app-exponential_p{me}-s2.slice] "))).count() == 2);
    until("the other space's launch outlived it", || !alive(other_pid));
    until("the outlived slice was never done with", || h.with(|state| state.spaces.settled()));

    // The compositor ends: everything in the slice of all its spaces at
    // once, and the user manager is told to take it away (waited for).
    assert!(!all_cgroup.join("cgroup.kill").exists());
    drop(h);
    assert_eq!(std::fs::read_to_string(all_cgroup.join("cgroup.kill")).unwrap(), "1");
    assert!(tools.log().last().unwrap().starts_with(&format!("systemctl [--user] [stop] [app-exponential_p{me}.slice] ")), "{:#?}", tools.log());
}

/// `space-enter`, the first program of every scope into a space, becomes
/// its program as it is: same process, same environment, same arguments.
/// (#EDE-146 puts its guard in front.)
#[test]
fn space_enter_becomes_its_program() {
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_exponential-compositor"))
        .args(["space-enter", "/run/user/1000/exponential-desktop/wayland-1.spaces", "--", "sh", "-c", "echo $$ $EDE_TEST_MARK \"$0\"", "a b"])
        .env("EDE_TEST_MARK", "kept")
        .output()
        .unwrap();
    assert!(out.status.success(), "{out:?}");
    let child = String::from_utf8(out.stdout).unwrap();
    let mut words = child.trim().splitn(3, ' ');
    assert!(words.next().is_some_and(|pid| pid.parse::<u32>().is_ok()));
    assert_eq!((words.next(), words.next()), (Some("kept"), Some("a b")));
    let wrong = std::process::Command::new(env!("CARGO_BIN_EXE_exponential-compositor")).args(["space-enter", "/x"]).output().unwrap();
    assert_eq!(wrong.status.code(), Some(127), "no program: a usage error, nothing runs");
    // One process: what `space-enter` becomes has its process id.
    let mut child = std::process::Command::new(env!("CARGO_BIN_EXE_exponential-compositor")).args(["space-enter", "/x", "--", "sleep", "30"]).spawn().unwrap();
    until("space-enter never became its program", || std::fs::read_to_string(format!("/proc/{}/comm", child.id())).is_ok_and(|comm| comm.trim() == "sleep"));
    let _ = child.kill();
    let _ = child.wait();
}

/// A launch that the compositor wrapped never runs outside a scope: when
/// the user manager refuses the scope or the slices, the program does not
/// start, and without the user manager nothing is started at all.
#[test]
fn a_launch_that_gets_no_scope_does_not_run() {
    let (h, dir) = start_with_spaces(&[(800, 600)]);
    let tools = Tools::new(dir.path());
    let units = tools.units();
    h.with(move |state| state.spaces.units = Some(units));
    let me = std::process::id();
    let (mut agent, _space) = agent(&h, "run-1");

    // No scope: neither the space's manager nor the launch runs.
    tools.flag("refuse-scope", true);
    let out = dir.path().join("refused-scope.env");
    let pid = agent.launch(writes_its_environment(&out), Default::default(), None).unwrap();
    until("a launch without its scope did not end", || !alive(pid));
    assert!(!out.exists(), "the program ran without a scope");
    assert!(tools.log().iter().any(|line| line.contains(&format!("[--unit=exponential-p{me}-s1-session1.scope]"))));
    assert!(!tools.log().iter().any(|line| line.starts_with("systemd [")), "the manager ran without its scope");
    until("the ended launch still counts", || h.with(|state| state.spaces.launched()) == 0);

    // No slices: no scope is even asked for, for the manager or the launch.
    let (mut other, _other_space) = self::agent(&h, "run-2");
    tools.flag("refuse-scope", false);
    tools.flag("refuse-slices", true);
    let asked = tools.log().len();
    let out = dir.path().join("refused-slices.env");
    let pid = other.launch(writes_its_environment(&out), Default::default(), None).unwrap();
    until("a launch without its slices did not end", || !alive(pid));
    assert!(!out.exists(), "the program ran without its slices");
    let log = tools.log().split_off(asked);
    assert!(log.iter().any(|line| line.contains("[StartTransientUnit]")) && log.iter().any(|line| line.contains("[SetUnitProperties]")), "made, then set: {log:#?}");
    assert!(!log.iter().any(|line| line.contains(&format!("[--slice=app-exponential_p{me}-s2.slice]"))), "a scope was asked for without its slices: {log:#?}");
    tools.flag("refuse-slices", false);

    // The user manager is gone: a compositor whose launches run in scopes
    // starts nothing, and says so. It does not fall back to a launch in
    // the person's login session. With the user manager back it launches.
    let (mut third, _third_space) = self::agent(&h, "run-3");
    let socket = tools.runtime().join("systemd/private");
    std::fs::remove_file(&socket).unwrap();
    let out = dir.path().join("no-manager.env");
    match third.launch(writes_its_environment(&out), Default::default(), None) {
        Err(ede_protocol::Error::Failed(failure)) => assert_eq!(failure.code, ede_protocol::ErrorCode::Unavailable, "{failure:?}"),
        answer => panic!("a launch without a user manager answered {answer:?}"),
    }
    assert!(!tools.log().iter().any(|line| line.contains(&format!("app-exponential_p{me}-s3"))), "something of the third space was started: {:#?}", tools.log());
    std::fs::write(&socket, "").unwrap();
    third.launch(writes_its_environment(&out), Default::default(), None).unwrap();
    until("the launch with the user manager back never ran", || out.exists());
    drop(h);
}

/// A space whose user manager ends gets it once more; when that one ends
/// too, the space closes (its session's next call reads "new and empty"),
/// instead of keeping a desk on which nothing can start.
#[test]
fn a_space_whose_manager_ends_twice_closes() {
    let (h, dir) = start_with_spaces(&[(800, 600)]);
    let tools = Tools::new(dir.path());
    let units = tools.units();
    h.with(move |state| state.spaces.units = Some(units));
    let me = std::process::id();
    let (mut agent, space) = agent(&h, "run-1");
    let out = dir.path().join("first.env");
    agent.launch(writes_its_environment(&out), Default::default(), None).unwrap();
    until("the launch never ran", || out.exists());
    let kill = |pid: u32| assert!(std::process::Command::new("kill").args(["-KILL", &pid.to_string()]).status().unwrap().success());

    kill(tools.manager("space-1"));
    until("the ended manager was not started once more", || tools.log().iter().any(|line| line.contains(&format!("[--unit=exponential-p{me}-s1-session2.scope]"))));
    let again = tools.manager("space-1");
    assert_eq!(agent.spaces().unwrap(), [space.clone()], "the space lives on with its second manager");
    let second = dir.path().join("second.env");
    agent.launch(writes_its_environment(&second), Default::default(), None).unwrap();
    until("a launch after the restart never ran", || second.exists());

    kill(again);
    until("a space whose manager ended twice is still there", || agent.spaces().unwrap().is_empty());
    assert!(!tools.log().iter().any(|line| line.contains("-session3.scope")), "a third start");
    match agent.launch(vec!["true".into()], Default::default(), None) {
        Err(ede_protocol::Error::Failed(failure)) => assert_eq!(failure.code, ede_protocol::ErrorCode::NotFound, "{failure:?}"),
        answer => panic!("a launch into the closed space answered {answer:?}"),
    }
    assert_ne!(agent.create_space(640, 480).unwrap().id, space.id, "a new space, new and empty");
    drop(h);
}

/// #EDE-138: with slices the count of spaces is the person's (or one per
/// GiB of the spaces' memory: 8 on this 16 GiB stand-in), a new space is
/// made only while a still one fits under the slice's `MemoryHigh`, and a
/// space may launch `[spaces] launches` processes. A session's own space
/// is found again before any of it.
#[test]
fn spaces_and_launches_are_counted_and_fenced_by_memory() {
    let (h, dir) = start_with_spaces(&[(800, 600)]);
    let tools = Tools::new(dir.path());
    let units = tools.units();
    let all_cgroup = units.all_cgroup();
    h.with(move |state| {
        state.config.spaces.launches = 2;
        state.spaces.units = Some(units);
    });
    let mut agents: Vec<(Client, SpaceInfo)> = (1..=8).map(|n| agent(&h, &format!("run-{n}"))).collect();
    let grant = h.ipc().mint(SessionId::new("run-9").unwrap(), vec![Capability::Space], None).unwrap();
    let mut ninth = h.ipc_with(grant.token).unwrap();
    let refused = |reply: Result<SpaceInfo, ede_protocol::Error>| match reply {
        Err(ede_protocol::Error::Failed(failure)) => (failure.code, failure.message),
        other => panic!("expected a refusal, got {other:?}"),
    };
    assert_eq!(refused(ninth.create_space(320, 240)), (ede_protocol::ErrorCode::Limit, "8 spaces at most".to_string()));

    // Room: the spaces hold so much that one still space no longer fits.
    agents.pop().unwrap().0.destroy_space(SpaceId::new("space-8").unwrap()).unwrap();
    std::fs::create_dir_all(&all_cgroup).unwrap();
    let high = pages(8 * GIB / 10 * 9);
    std::fs::write(all_cgroup.join("memory.stat"), format!("anon {}\nfile 9999999999\nshmem {}\n", high - GIB, 1 << 20)).unwrap();
    let (code, message) = refused(ninth.create_space(320, 240));
    assert_eq!(code, ede_protocol::ErrorCode::Limit);
    assert_eq!(message, format!("no memory for another space: the agent spaces hold {} MiB of their {} MiB", (high - GIB + (1 << 20)) >> 20, high >> 20));
    // A session's own space is found again whatever the memory.
    let (first, first_space) = &mut agents[0];
    assert_eq!(&first.create_space(320, 240).unwrap(), first_space);
    std::fs::write(all_cgroup.join("memory.stat"), format!("anon {}\nshmem 0\n", high - GIB)).unwrap();
    ninth.create_space(320, 240).expect("room again");

    // Two launches in a space, then `limit`; another space launches on.
    let sleep = || vec!["sleep".to_string(), "60".to_string()];
    for _ in 0..2 {
        first.launch(sleep(), Default::default(), None).unwrap();
    }
    match first.launch(sleep(), Default::default(), None) {
        Err(ede_protocol::Error::Failed(failure)) => assert_eq!((failure.code, failure.message.as_str()), (ede_protocol::ErrorCode::Limit, "2 launched processes in this space at most")),
        other => panic!("a third launch answered {other:?}"),
    }
    agents[1].0.launch(sleep(), Default::default(), None).expect("another space launches");
    drop(h);
}

/// #EDE-138: a space's home is on disk, beside the other spaces' of its
/// compositor, and what a compositor that is gone left there goes when the
/// next one opens the place: a `p<pid>` of a process that is no running
/// compositor, or of an earlier life of this process id.
#[test]
fn the_homes_of_a_gone_compositor_go() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("spaces");
    let me = std::process::id();
    let compositor = dir.path().join("exponential-compositor");
    std::fs::write(&compositor, "#!/bin/sh\nsleep 60\n:\n").unwrap();
    std::fs::set_permissions(&compositor, std::fs::Permissions::from_mode(0o755)).unwrap();
    let mut running = std::process::Command::new(&compositor).spawn().unwrap();
    until("the stand-in compositor never started", || std::fs::read_to_string(format!("/proc/{}/comm", running.id())).is_ok_and(|comm| comm.trim() == "exponential-com"));
    let mut other_program = std::process::Command::new("sleep").arg("60").spawn().unwrap();
    for pid in [me, running.id(), other_program.id(), 4_194_999] {
        std::fs::create_dir_all(root.join(format!("p{pid}/space-1/config"))).unwrap();
    }
    std::fs::create_dir_all(root.join("not-a-compositors")).unwrap();
    let mut spaces = ede_compositor::spaces::Spaces::default();
    spaces.open_homes(root.clone());
    let mut left: Vec<String> = std::fs::read_dir(&root).unwrap().map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned()).collect();
    left.sort();
    let mut expected = vec![format!("p{me}"), format!("p{}", running.id()), "not-a-compositors".to_string()];
    expected.sort();
    assert_eq!(left, expected, "a running compositor's homes stay, and nothing that is not a compositor's");
    assert_eq!(std::fs::read_dir(root.join(format!("p{me}"))).unwrap().count(), 0, "an earlier life's homes are gone");
    assert_eq!(spaces.homes.as_deref(), Some(root.join(format!("p{me}")).as_path()));
    drop(spaces);
    assert!(!root.join(format!("p{me}")).exists(), "the compositor's end takes its homes along");
    for child in [&mut running, &mut other_program] {
        let _ = child.kill();
        let _ = child.wait();
    }
}

/// What a compositor that is gone left behind: slices of all spaces whose
/// process id is nobody's, another program's, or this compositor's own
/// from an earlier life. A running compositor's are not touched.
#[test]
fn the_slices_of_a_gone_compositor_are_found() {
    let dir = tempfile::tempdir().unwrap();
    let tools = Tools::new(dir.path());
    let units = tools.units();
    let me = std::process::id();
    // A running compositor: its process is named so.
    let compositor = dir.path().join("exponential-compositor");
    std::fs::write(&compositor, "#!/bin/sh\nsleep 60\n:\n").unwrap();
    std::fs::set_permissions(&compositor, std::fs::Permissions::from_mode(0o755)).unwrap();
    let mut running = std::process::Command::new(&compositor).spawn().unwrap();
    until("the stand-in compositor never started", || std::fs::read_to_string(format!("/proc/{}/comm", running.id())).is_ok_and(|comm| comm.trim() == "exponential-com"));
    let mut other_program = std::process::Command::new("sleep").arg("60").spawn().unwrap();
    std::fs::write(
        tools.dir.join("units"),
        format!(
            "app-exponential_p{me}.slice loaded active active Slice /app/exponential_p{me}\napp-exponential_p{}.slice loaded active active x\napp-exponential_p{}.slice loaded active active x\napp-exponential_p4194999.slice loaded active active x\napp-exponential_p{me}-s3.slice loaded active active a space's own\napp-exponential.slice loaded active active x\n",
            running.id(),
            other_program.id()
        ),
    )
    .unwrap();
    assert_eq!(units.stale(), [format!("app-exponential_p{me}.slice"), format!("app-exponential_p{}.slice", other_program.id()), "app-exponential_p4194999.slice".to_string()]);
    assert_eq!(tools.log()[0].split(" XDG_RUNTIME_DIR").next(), Some("systemctl [--user] [list-units] [--all] [--plain] [--no-legend] [app-exponential_p*.slice]"));
    for child in [&mut running, &mut other_program] {
        let _ = child.kill();
        let _ = child.wait();
    }
}

/// #EDE-146..148: a space guard of the test's own on a socket: answers
/// every request with `answer`, and hands the test each request with the
/// asking process's id.
fn fake_guard(dir: &Path, answer: &'static str) -> (PathBuf, std::sync::mpsc::Receiver<(String, u32)>) {
    use std::io::{BufRead, Write};
    use std::os::fd::AsFd;
    let socket = dir.join(format!("guard-{}.sock", answer.split_whitespace().next().unwrap()));
    let listener = std::os::unix::net::UnixListener::bind(&socket).unwrap();
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(stream) = stream else { return };
            let mut line = String::new();
            let _ = std::io::BufReader::new(&stream).read_line(&mut line);
            let (pid, _) = ede_space_guard::sys::peer(stream.as_fd()).unwrap();
            let _ = (&stream).write_all(format!("{answer}\n").as_bytes());
            if tx.send((line, pid)).is_err() {
                return;
            }
        }
    });
    (socket, rx)
}

/// `space-enter --guard` asks the guard to guard the space slice it is in
/// and becomes its program only on `ok`; a refusal, or no guard, and the
/// program does not run.
#[test]
fn space_enter_runs_its_program_only_when_the_guard_says_so() {
    let dir = tempfile::tempdir().unwrap();
    let spaces = "/run/user/1000/exponential-desktop/wayland-1.spaces";
    let enter = |socket: &Path| std::process::Command::new(env!("CARGO_BIN_EXE_exponential-compositor")).args(["space-enter", "--guard"]).arg(socket).args([spaces, "--", "sh", "-c", "echo ran $$"]).output().unwrap();

    let (yes, asked) = fake_guard(dir.path(), "ok");
    let out = enter(&yes);
    assert!(out.status.success(), "{out:?}");
    let (line, pid) = asked.recv_timeout(Duration::from_secs(5)).unwrap();
    assert_eq!(line, format!("enter {spaces}\n"));
    // Asked by the process that then became the program.
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), format!("ran {pid}"));

    let (no, _asked) = fake_guard(dir.path(), "no this kernel refuses the device program");
    let out = enter(&no);
    assert_eq!(out.status.code(), Some(127), "{out:?}");
    assert!(out.stdout.is_empty(), "the program ran: {out:?}");
    assert!(String::from_utf8_lossy(&out.stderr).contains("the space guard: this kernel refuses the device program"), "{out:?}");

    let out = enter(&dir.path().join("no-guard-here.sock"));
    assert_eq!(out.status.code(), Some(127), "{out:?}");
    assert!(out.stdout.is_empty(), "the program ran without its guard: {out:?}");
}

/// A compositor whose guard answered puts it in front of every launch:
/// the launch's own process asks from inside its scope, and the program
/// runs only after the guard said so.
#[test]
fn a_guarded_compositors_launch_asks_the_guard_from_inside_its_scope() {
    let (h, dir) = start_with_spaces(&[(800, 600)]);
    let tools = Tools::new(dir.path());
    let (socket, asked) = fake_guard(dir.path(), "ok");
    let units = Units { guard: Some(socket.clone()), ..tools.units() };
    h.with(move |state| state.spaces.units = Some(units));
    let (mut agent, space) = agent(&h, "run-1");
    let spaces_dir = Path::new(&space.wayland_display).parent().unwrap().parent().unwrap().to_path_buf();

    let out = dir.path().join("guarded.env");
    let pid = agent.launch(writes_its_environment(&out), Default::default(), None).unwrap();
    until("the guarded program never ran", || out.exists());
    let (line, asker) = asked.recv_timeout(Duration::from_secs(5)).unwrap();
    assert_eq!((line, asker), (format!("enter {}\n", spaces_dir.display()), pid), "asked by someone else than the launch, or for something else");
    let log = tools.log();
    assert!(log[2].contains(&format!("] [space-enter] [--guard] [{}] [{}] [--] [", socket.display(), spaces_dir.display())), "{}", log[2]);

    // The guard says no: the program does not run.
    let (refusing, _asked) = fake_guard(dir.path(), "no not a space slice");
    let units = Units { guard: Some(refusing), ..tools.units() };
    h.with(move |state| state.spaces.units = Some(units));
    let out = dir.path().join("refused.env");
    let pid = agent.launch(writes_its_environment(&out), Default::default(), None).unwrap();
    until("a launch the guard refused did not end", || !alive(pid));
    assert!(!out.exists(), "the program ran although the guard said no");
    drop(h);
}
