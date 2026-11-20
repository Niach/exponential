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
use ede_protocol::{Capability, Client, SessionId, SpaceInfo};

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
            "#!/bin/sh\n{log}\n[ -e \"$d/refuse-slices\" ] && {{ echo 'Call failed: Access denied' >&2; exit 1; }}\nunit=$9\n{place}\nif [ \"$7\" = StartTransientUnit ]; then\n  [ -d \"$place\" ] && {{ echo \"Call failed: Unit $unit was already loaded or has a fragment file.\" >&2; exit 1; }}\n  mkdir -p \"$place\"\nfi\nexit 0\n"
        );
        let systemctl = format!(
            "#!/bin/sh\n{log}\n[ \"$2\" = list-units ] && [ -e \"$d/units\" ] && cat \"$d/units\"\nif [ \"$2\" = stop ]; then\n  [ -e \"$d/slow\" ] && sleep 3\n  unit=$3\n  {place}\n  [ -e \"$d/stubborn\" ] || rm -rf \"$place\"\nfi\nexit 0\n"
        );
        for (name, script) in [("systemd-run", systemd_run), ("busctl", busctl), ("systemctl", systemctl)] {
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
        }
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
    assert_eq!(log.len(), 3, "{log:#?}");
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
    // Then the scope, asked of the PERSON's user manager. In it the last
    // stage of the launch, found through a descriptor, then the program.
    let scope = format!(
        "systemd-run [--user] [--scope] [--collect] [--quiet] [--slice=app-exponential_p{me}-s1.slice] [--unit=exponential-p{me}-s1-1.scope] [-p] [TimeoutStopSec=2] [--expand-environment=no] [--setenv=XDG_RUNTIME_DIR={}] [--] [/proc/self/fd/",
        space_dir.display()
    );
    // The first program in the scope is `space-enter` with the
    // compositor's directory of spaces (#EDE-146's guard goes there).
    let enter = format!("] [space-enter] [{}] [--] [/proc/self/fd/", space_dir.parent().unwrap().display());
    assert!(log[2].starts_with(&scope) && log[2].contains(&enter) && log[2].contains("] [space-exec] [") && log[2].contains(" [sh] [-c] "), "{}", log[2]);
    assert!(log[2].ends_with(&format!(" XDG_RUNTIME_DIR={} BUS= MARK=", runtime.display())), "systemd-run reaches the person's user manager, with no bus of the person's and nothing of the caller's: {}", log[2]);
    assert!(!log.iter().any(|line| line.contains("secret-never-on-an-argument") || line.contains("the-callers")), "the caller's environment is on an argument or in a program before the scope");
    // The program has it, with the space's runtime directory, and its
    // standard input is nothing, as a launch's always was.
    let env = std::fs::read_to_string(&out).unwrap();
    for line in [format!("XDG_RUNTIME_DIR={}", space_dir.display()), format!("WAYLAND_DISPLAY={}", space.wayland_display), "TOKEN=secret-never-on-an-argument".to_string(), format!("{MARK}=the-callers"), "STDIN=/dev/null".to_string()] {
        assert!(env.lines().any(|l| l == line), "{line} is not what the program has: {env}");
    }
    assert!(!std::fs::read_dir(&space_dir).unwrap().any(|entry| entry.unwrap().file_name().to_string_lossy().starts_with(".launch")), "the caller's environment is still a file");
    // One process from the spawn to the program: the launch's stages are gone.
    let cmdline = String::from_utf8_lossy(&std::fs::read(format!("/proc/{pid}/cmdline")).unwrap()).into_owned();
    assert!(!cmdline.contains("space-run") && !cmdline.contains("space-enter") && !cmdline.contains("space-exec"), "the launch is still {cmdline}");

    // The space's later launches ask for their scope only: its slice is there.
    let second = dir.path().join("second.env");
    agent.launch(writes_its_environment(&second), Default::default(), None).unwrap();
    until("the second program never ran", || second.exists());
    let log = tools.log();
    assert_eq!(log.len(), 4, "{log:#?}");
    assert!(log[3].starts_with(&format!("systemd-run [--user] [--scope] [--collect] [--quiet] [--slice=app-exponential_p{me}-s1.slice] [--unit=exponential-p{me}-s1-2.scope] ")), "{}", log[3]);

    // A second space: the slice of all spaces is there, so its limits are
    // set again (the person's of now); the space's own slice is made.
    let (mut other, other_space) = self::agent(&h, "run-2");
    let third = dir.path().join("third.env");
    let other_pid = other.launch(writes_its_environment(&third), Default::default(), None).unwrap();
    until("the other space's program never ran", || third.exists());
    let log = tools.log();
    assert_eq!(log.len(), 8, "{log:#?}");
    assert!(log[4].contains(&format!(" [StartTransientUnit] [ssa(sv)a(sa(sv))] [app-exponential_p{me}.slice] ")), "{}", log[4]);
    assert!(log[5].contains(&format!(" [SetUnitProperties] [sba(sv)] [app-exponential_p{me}.slice] [true] [5] [CPUQuotaPerSecUSec] [t] [4000000] ")) && !log[5].contains("] [0] XDG"), "{}", log[5]);
    assert!(log[6].contains(&format!(" [StartTransientUnit] [ssa(sv)a(sa(sv))] [app-exponential_p{me}-s2.slice] ")), "{}", log[6]);
    assert!(log[7].contains(&format!(" [--slice=app-exponential_p{me}-s2.slice] [--unit=exponential-p{me}-s2-3.scope] ")), "{}", log[7]);

    // The close tells the user manager to stop the space's slice and does
    // not wait for it: the stand-in takes 3 s, the compositor answers at once.
    tools.flag("slow", true);
    let asked = Instant::now();
    agent.destroy_space(space.id.clone()).unwrap();
    h.ipc().windows().unwrap();
    assert!(asked.elapsed() < Duration::from_secs(2), "the close waited for the user manager: {:?}", asked.elapsed());
    until("the user manager was never told to stop the space's slice", || tools.log().len() == 9);
    assert!(tools.log()[8].starts_with(&format!("systemctl [--user] [stop] [app-exponential_p{me}-s1.slice] XDG_RUNTIME_DIR={} BUS=", runtime.display())), "{}", tools.log()[8]);
    until("the closed space's launches outlived it", || !alive(pid));
    until("the closed space's slice was never done with", || h.with(|state| state.spaces.settled()));
    assert!(!first_cgroup.exists() && tools.log().len() == 9, "a slice that the stop ended is not ended again: {:#?}", tools.log());
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
    let (mut agent, _space) = agent(&h, "run-1");

    tools.flag("refuse-scope", true);
    let out = dir.path().join("refused-scope.env");
    let pid = agent.launch(writes_its_environment(&out), Default::default(), None).unwrap();
    until("a launch without its scope did not end", || !alive(pid));
    assert!(!out.exists(), "the program ran without a scope");
    assert!(tools.log().iter().any(|line| line.starts_with("systemd-run ")));
    until("the ended launch still counts", || h.with(|state| state.spaces.launched()) == 0);

    // No slices: the scope is not even asked for. (A new space: the first
    // one's slice is there.)
    let (mut other, _other_space) = self::agent(&h, "run-2");
    tools.flag("refuse-scope", false);
    tools.flag("refuse-slices", true);
    let asked = tools.log().len();
    let out = dir.path().join("refused-slices.env");
    let pid = other.launch(writes_its_environment(&out), Default::default(), None).unwrap();
    until("a launch without its slices did not end", || !alive(pid));
    assert!(!out.exists(), "the program ran without its slices");
    let log = tools.log().split_off(asked);
    assert_eq!(log.len(), 2, "made, then set, then nothing more: {log:#?}");
    assert!(log[0].contains("[StartTransientUnit]") && log[1].contains("[SetUnitProperties]") && !log.iter().any(|line| line.starts_with("systemd-run ")), "{log:#?}");

    // The user manager is gone: a compositor whose launches run in scopes
    // starts nothing, and says so. It does not fall back to a launch in
    // the person's login session. With the user manager back it launches.
    tools.flag("refuse-slices", false);
    until("the ended launches still count", || h.with(|state| state.spaces.launched()) == 0);
    let socket = tools.runtime().join("systemd/private");
    std::fs::remove_file(&socket).unwrap();
    let asked = tools.log().len();
    let out = dir.path().join("no-manager.env");
    match other.launch(writes_its_environment(&out), Default::default(), None) {
        Err(ede_protocol::Error::Failed(failure)) => assert_eq!(failure.code, ede_protocol::ErrorCode::Unavailable, "{failure:?}"),
        answer => panic!("a launch without a user manager answered {answer:?}"),
    }
    assert_eq!((tools.log().len(), h.with(|state| state.spaces.launched())), (asked, 0), "something was started");
    std::fs::write(&socket, "").unwrap();
    other.launch(writes_its_environment(&out), Default::default(), None).unwrap();
    until("the launch with the user manager back never ran", || out.exists());
    drop(h);
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
