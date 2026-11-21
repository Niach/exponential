//! #EDE-138: a systemd slice per space. What a space launches runs in a
//! scope of the space's own slice in the person's user manager: out of the
//! person's login session scope, under limits the person sets
//! (`[spaces]` in `compositor.toml`), and ended with the space, whatever
//! it forked, whichever session it started.
//!
//! ```text
//! app.slice/app-exponential_p<pid>.slice          all spaces: CPU, memory
//!   └─ app-exponential_p<pid>-s<n>.slice           one space: memory, tasks
//!        └─ exponential-p<pid>-s<n>-<k>.scope      launch k
//! ```
//!
//! The compositor links no D-Bus library and never waits for the user
//! manager in its event loop. A launch is four programs in ONE process
//! (so the launch's group and its death signal stay what they were):
//!
//! 1. `exponential-compositor space-run <what> -- argv` ([`run_main`]),
//!    still in the compositor's cgroup: it makes the slices when the
//!    space has none (`busctl`), then becomes
//! 2. `systemd-run --user --scope`, which asks the person's user manager
//!    for the scope and, inside it, becomes
//! 3. `exponential-compositor space-enter <spaces> -- …` ([`enter_main`]),
//!    the first program of every scope into a space (with `--guard` it
//!    asks the space guard first, #EDE-146..148), which becomes
//! 4. `exponential-compositor space-exec` ([`exec_main`]), which takes
//!    the caller's environment from its standard input and becomes the
//!    program.
//!
//! Nothing the caller chose is in the first two: its environment (a
//! `LD_PRELOAD`, a library path) reaches no process outside the scope and
//! no argument anywhere.
//!
//! A launch the compositor wrapped never runs outside its scope: when the
//! user manager refuses, the program does not start. Where there is no
//! user manager to ask (or a systemd older than [`SYSTEMD_FLOOR`], or no
//! unified cgroup hierarchy), no launch is wrapped ([`Units::probe`] says
//! why) and all of them run as before, in the person's login session and
//! without limits. That is decided once: a compositor that found its user
//! manager never starts a launch unwrapped, and one that then loses it
//! answers `unavailable` ([`Units::reachable`]). Whether spaces are offered
//! to people at all is [`super::readiness`]'s answer.
//!
//! Like the grants, this guards against accidents. Every cgroup under the
//! user manager belongs to the user: a process that works at it raises
//! its own limits or moves itself to another cgroup.

use std::{
    collections::BTreeMap,
    io::Read,
    os::{
        fd::{FromRawFd, IntoRawFd, OwnedFd},
        unix::process::CommandExt,
    },
    path::{Path, PathBuf},
    process::{Child, Command, Output, Stdio},
    time::{Duration, Instant},
};

use ede_protocol::process::{launch_scope, space_slice, spaces_slice};
use serde::{Deserialize, Serialize};
use smithay::reexports::rustix::io::{fcntl_setfd, FdFlags};

use crate::config;

/// Where systemd's manager is, merged `/usr` or not.
const MANAGERS: [&str; 2] = ["/usr/lib/systemd/systemd", "/lib/systemd/systemd"];
/// How long a launch waits for its space's bus at the space's start.
const BUS_SECONDS: u64 = 5;

/// The oldest systemd whose user manager is asked: 252 delegates the cpu
/// controller to it, so the CPU limit is real (Debian 12 has 252, Ubuntu
/// 22.04 has 249 and runs its spaces as before).
pub const SYSTEMD_FLOOR: u32 = 252;
/// `systemd-run --expand-environment=` exists from here on, and from 258
/// a scope's argv has its `$VARIABLES` replaced unless it is `no`.
const NO_EXPAND_SINCE: u32 = 254;
/// A scope's processes get this long between SIGTERM and SIGKILL.
const STOP_SECONDS: u32 = 2;
/// How long one question to the user manager may take.
const ASK_SECONDS: u32 = 5;
/// How long the end of a compositor waits for the user manager to take
/// the slice of all its spaces away.
pub const EXIT_STOP: Duration = Duration::from_secs(3);
const GIB: u64 = 1 << 30;
/// Every memory limit is a multiple of this: the largest page there is.
const MEMORY_STEP: u64 = 64 << 10;
/// What a still space takes: its manager and bus, the helpers a toolkit
/// starts, one browser page. Measured on mint (#EXP-1514, 3.6): 0.5 GB with
/// Chromium, 0.9 GB with Firefox. A new space is made only while this fits.
pub const STILL_SPACE: u64 = GIB;
/// The most spaces there may be, whatever the memory.
pub const MAX_SPACES: u32 = 32;

/// The person's user manager and the tools that talk to it; in
/// [`super::Spaces::units`] when launches run in scopes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Units {
    pub systemd_run: PathBuf,
    pub busctl: PathBuf,
    pub systemctl: PathBuf,
    /// systemd's version (`systemd-run --version`).
    pub version: u32,
    /// The program a launch starts as (`space-run`): the compositor's own
    /// running image, also after a package upgrade replaced the file.
    pub shim: PathBuf,
    /// The person's runtime directory, where their user manager listens.
    pub runtime: PathBuf,
    /// The person's session bus (`busctl --user`); `None` = the one in
    /// their runtime directory.
    pub bus: Option<String>,
    /// The compositor's process id, in every unit's name.
    pub compositor: u32,
    /// What the limits are fractions of.
    pub machine: Machine,
    /// The cgroup directory of the user manager's `app.slice`, as a scope
    /// of the compositor's own found it ([`Units::probe`]): where the
    /// slices' cgroups are. Never taken from a launch.
    pub app_slice: PathBuf,
    /// `systemd`, run as a space's own user manager ([`super::session`]).
    pub manager: PathBuf,
    /// `dbus-daemon`, the space's bus under that manager.
    pub dbus_daemon: PathBuf,
    /// #EDE-146..148: the space guard's socket when the guard is installed
    /// ([`super::guard`]). Then every scope into a space asks it first and
    /// nothing runs in a space it did not guard, for the whole run.
    pub guard: Option<PathBuf>,
}

/// The machine's size.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Machine {
    pub cpus: u64,
    /// Bytes of RAM.
    pub memory: u64,
}

impl Machine {
    pub fn read() -> Self {
        let cpus = std::thread::available_parallelism().map_or(1, |n| n.get() as u64);
        let memory = std::fs::read_to_string("/proc/meminfo").ok().and_then(|text| parse_mem_total(&text)).unwrap_or(4 * GIB);
        Self { cpus, memory }
    }
}

/// `MemTotal:       32786188 kB` of `/proc/meminfo`, in bytes.
pub fn parse_mem_total(meminfo: &str) -> Option<u64> {
    let kb: u64 = meminfo.lines().find_map(|line| line.strip_prefix("MemTotal:"))?.trim().strip_suffix("kB")?.trim().parse().ok()?;
    Some(kb * 1024)
}

/// `systemd 255 (255.4-1ubuntu8.18)` → 255.
pub fn parse_systemd_version(output: &str) -> Option<u32> {
    output.lines().next()?.strip_prefix("systemd ")?.split_whitespace().next()?.parse().ok()
}

/// The cgroup directory of the user manager's `app.slice`, from the
/// `/proc/self/cgroup` of a scope it made there
/// (`0::/user.slice/user-1000.slice/user@1000.service/app.slice/run-r1.scope`).
/// `None` without a unified hierarchy (cgroup v1 or hybrid: the user
/// manager has no memory or cpu controller there, and no `cgroup.kill`),
/// or when the scope is anywhere else.
pub fn parse_app_slice(cgroup: &str) -> Option<PathBuf> {
    let path = cgroup.lines().find_map(|line| line.strip_prefix("0::"))?;
    let (slice, scope) = path.trim_end().rsplit_once('/')?;
    if !scope.ends_with(".scope") || !slice.ends_with("/app.slice") {
        return None;
    }
    Some(Path::new("/sys/fs/cgroup").join(slice.trim_start_matches('/')))
}

/// A slice to make, with its limits: the names and values of
/// `StartTransientUnit`'s properties, every one a `t` (u64).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Slice {
    pub name: String,
    pub properties: Vec<(String, u64)>,
}

/// The limits of all spaces together and of one space, from the person's
/// `[spaces]` and the machine's size.
///
/// The CPU limit is a quota. The person's apps run in their login session
/// scope, a sibling of the whole user manager, so no weight inside the
/// user manager can keep spaces below half of a busy machine; the weight
/// only puts them behind the person's user services.
pub fn limits(spaces: &config::Spaces, machine: Machine) -> (Vec<(String, u64)>, Vec<(String, u64)>) {
    let percent = |of: u64, percent: u32| (of as u128 * percent as u128 / 100) as u64;
    // The kernel keeps a memory limit in whole pages (4, 16 or 64 KiB):
    // rounded here, what is asked for is what `memory.max` then reads.
    let pages = |bytes: u64| bytes / MEMORY_STEP * MEMORY_STEP;
    let all_memory = pages(percent(machine.memory, spaces.memory_max));
    let space_memory = pages(percent(machine.memory, spaces.space_memory_max).max(2 * GIB).min(all_memory));
    let all = vec![
        // Microseconds of CPU per second of time: 1_000_000 is one core.
        ("CPUQuotaPerSecUSec".to_string(), percent(machine.cpus * 1_000_000, spaces.cpu_quota).max(10_000)),
        ("CPUWeight".to_string(), 10),
        ("MemoryMax".to_string(), all_memory),
        ("MemoryHigh".to_string(), pages(all_memory / 10 * 9)),
        // `memory.max` does not count swap: without this a space at its
        // limit would fill the swap the person's apps share.
        ("MemorySwapMax".to_string(), pages(all_memory / 4)),
    ];
    let space = vec![("MemoryMax".to_string(), space_memory), ("MemoryHigh".to_string(), pages(space_memory / 10 * 9)), ("TasksMax".to_string(), u64::from(spaces.tasks_max))];
    (all, space)
}

/// How many spaces there may be: the person's `[spaces] max`, else one per
/// GiB of what all spaces together may use (rounded), at least 1 and at
/// most [`MAX_SPACES`]: 8 GB of RAM = 4, 16 GB = 8, 64 GB = 31.
pub fn max_spaces(spaces: &config::Spaces, machine: Machine) -> u32 {
    if let Some(max) = spaces.max {
        return max.clamp(1, MAX_SPACES);
    }
    let all = limits(spaces, machine).0.iter().find(|(name, _)| name == "MemoryMax").map_or(0, |(_, bytes)| *bytes);
    ((all + GIB / 2) / GIB).clamp(1, u64::from(MAX_SPACES)) as u32
}

/// `anon` + `shmem` of a cgroup's `memory.stat`: what its processes hold
/// that only swap could take away.
pub fn held(memory_stat: &str) -> u64 {
    memory_stat.lines().filter_map(|line| line.split_once(' ')).filter(|(name, _)| matches!(*name, "anon" | "shmem")).filter_map(|(_, bytes)| bytes.trim().parse::<u64>().ok()).sum()
}

/// Whether a new space fits: what the spaces hold now (`memory.stat` of the
/// slice of all spaces; none = nothing yet) plus one still space under the
/// slice's `MemoryHigh`. Above it every process of every space is
/// throttled for as long as nothing frees memory, and every agent's calls
/// time out. `Err` is the refusal's sentence.
pub fn room(spaces: &config::Spaces, machine: Machine, memory_stat: Option<&str>) -> Result<(), String> {
    room_freeing(spaces, machine, memory_stat, 0)
}

/// [`room`] once spaces holding `freed` bytes have closed (#EXP-1514: a
/// full desktop gives way; their slices stop after the close, so what
/// they held still shows in `memory.stat`).
pub fn room_freeing(spaces: &config::Spaces, machine: Machine, memory_stat: Option<&str>, freed: u64) -> Result<(), String> {
    let high = limits(spaces, machine).0.iter().find(|(name, _)| name == "MemoryHigh").map_or(0, |(_, bytes)| *bytes);
    let held = memory_stat.map_or(0, held).saturating_sub(freed);
    if held + STILL_SPACE <= high {
        return Ok(());
    }
    let mib = |bytes: u64| bytes >> 20;
    Err(format!("no memory for another space: the agent spaces hold {} MiB of their {} MiB", mib(held), mib(high)))
}

/// What the spaces may take, in words for the log and the doctor: the
/// count, and that the spaces share their memory (one may take more than
/// its part while the others are still).
pub fn described(spaces: &config::Spaces, machine: Machine) -> String {
    let (all, one) = limits(spaces, machine);
    let value = |list: &[(String, u64)], name: &str| list.iter().find(|(n, _)| n == name).map_or(0, |(_, v)| *v);
    let gib = |bytes: u64| format!("{:.1} GiB", bytes as f64 / GIB as f64);
    format!(
        "{} spaces at most, {} launches each; together {} of memory, one up to {}: they share it, a new space is made only while {} is free under {}",
        max_spaces(spaces, machine),
        spaces.launches,
        gib(value(&all, "MemoryMax")),
        gib(value(&one, "MemoryMax")),
        gib(STILL_SPACE),
        gib(value(&all, "MemoryHigh")),
    )
}

/// What `space-run` is told: one launch of one space.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Run {
    pub systemd_run: PathBuf,
    pub busctl: PathBuf,
    pub runtime: PathBuf,
    pub bus: Option<String>,
    /// The slice of all spaces, then the space's; the scope goes into the last.
    pub slices: Vec<Slice>,
    /// The cgroup directory of the space's slice: there = the slices are made.
    pub cgroup: PathBuf,
    /// The launch's scope.
    pub unit: String,
    /// A scope whose cgroup is handed to its program (`Delegate=yes`): the
    /// space's own user manager.
    #[serde(default)]
    pub delegate: bool,
    /// The space's bus: the program starts once it answers (a launch at the
    /// space's start waits for its manager in its own process).
    #[serde(default)]
    pub wait_bus: Option<PathBuf>,
    pub no_expand: bool,
    /// The space's directory: the program's `XDG_RUNTIME_DIR`.
    pub space_dir: PathBuf,
    /// The space guard `space-enter` asks before anything runs ([`Units::guard`]).
    #[serde(default)]
    pub guard: Option<PathBuf>,
}

impl Units {
    /// The user manager of this session, asked once at the session's
    /// start; `Err` says why launches run without a scope. It asks for a
    /// real scope, so what is found here works, and the scope says where
    /// the user manager's cgroups are.
    pub fn probe() -> Result<Self, String> {
        let mut units = Self::find()?;
        let mut scope = Command::new(&units.systemd_run);
        // In `app.slice` as asked, under a name of the compositor's: a
        // scope without one is `run-<id>.scope`, which a drop-in of the
        // person's (`run-.scope.d`) may send anywhere.
        static PROBES: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        let probe = PROBES.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let unit = format!("--unit=exponential-p{}-probe{probe}.scope", units.compositor);
        scope.args(["--user", "--scope", "--collect", "--quiet", "--slice=app.slice", &unit]);
        if units.no_expand() {
            scope.arg("--expand-environment=no");
        }
        scope.args(["--", "cat", "/proc/self/cgroup"]);
        let placed = match output_within(scope, Duration::from_secs(u64::from(ASK_SECONDS))) {
            Some(output) if output.status.success() => String::from_utf8_lossy(&output.stdout).into_owned(),
            Some(_) => return Err("the user manager refuses a scope".into()),
            None => return Err("the user manager does not answer".into()),
        };
        // Limits and `cgroup.kill` need the unified hierarchy (cgroup v2);
        // a hybrid one also has a `0::` line, but no controllers there.
        if !Path::new("/sys/fs/cgroup/cgroup.controllers").exists() {
            return Err("no unified cgroup hierarchy (cgroup v2)".into());
        }
        units.app_slice = parse_app_slice(&placed).filter(|dir| dir.is_dir()).ok_or_else(|| format!("the user manager does not put a scope under its app.slice ({})", placed.trim()))?;
        // The slices are made over the person's session bus: a launch
        // must not find out that it is missing.
        let mut ping = Command::new(&units.busctl);
        ping.args(["--user", &format!("--timeout={ASK_SECONDS}"), "call", "org.freedesktop.systemd1", "/org/freedesktop/systemd1", "org.freedesktop.DBus.Peer", "Ping"]);
        match output_within(ping, Duration::from_secs(u64::from(ASK_SECONDS) + 1)) {
            Some(output) if output.status.success() => Ok(units),
            _ => Err("the user manager is not on the session bus".into()),
        }
    }

    /// What is installed and running, without asking the user manager for
    /// anything; [`Units::app_slice`] is not known yet.
    pub fn find() -> Result<Self, String> {
        let runtime = PathBuf::from(std::env::var_os("XDG_RUNTIME_DIR").ok_or("no XDG_RUNTIME_DIR")?);
        if !runtime.join("systemd/private").exists() {
            return Err(format!("no systemd user manager in {}", runtime.display()));
        }
        let tool = |name: &str| on_path(name).ok_or(format!("{name} is not installed"));
        let (systemd_run, busctl, systemctl, dbus_daemon) = (tool("systemd-run")?, tool("busctl")?, tool("systemctl")?, tool("dbus-daemon")?);
        let manager = MANAGERS.iter().map(PathBuf::from).find(|path| path.is_file()).ok_or("systemd's manager (/usr/lib/systemd/systemd) is not there")?;
        let mut asked = Command::new(&systemd_run);
        asked.arg("--version");
        let output = output_within(asked, Duration::from_secs(u64::from(ASK_SECONDS))).ok_or_else(|| format!("{} --version does not answer", systemd_run.display()))?;
        let version = parse_systemd_version(&String::from_utf8_lossy(&output.stdout)).ok_or("systemd-run --version says no version")?;
        if version < SYSTEMD_FLOOR {
            return Err(format!("systemd {version} is older than {SYSTEMD_FLOOR}"));
        }
        Ok(Self {
            systemd_run,
            busctl,
            systemctl,
            version,
            shim: "/proc/self/exe".into(),
            runtime,
            bus: std::env::var("DBUS_SESSION_BUS_ADDRESS").ok().filter(|address| !address.is_empty()),
            compositor: std::process::id(),
            machine: Machine::read(),
            app_slice: PathBuf::new(),
            manager,
            dbus_daemon,
            guard: None,
        })
    }

    fn no_expand(&self) -> bool {
        self.version >= NO_EXPAND_SINCE
    }

    /// The user manager still listens. A launch asks before it starts: a
    /// compositor that puts its launches into scopes starts none outside
    /// one, so without the user manager it starts nothing.
    pub fn reachable(&self) -> bool {
        self.runtime.join("systemd/private").exists()
    }

    /// The cgroup directory of the slice of all spaces.
    pub fn all_cgroup(&self) -> PathBuf {
        self.app_slice.join(spaces_slice(self.compositor))
    }

    /// The cgroup directory of space number `space`'s slice.
    pub fn space_cgroup(&self, space: u32) -> PathBuf {
        self.all_cgroup().join(space_slice(self.compositor, space))
    }

    /// Whether a new space fits ([`room`]), by what the slice of all spaces
    /// holds now: one small file, read in the event loop.
    pub fn room(&self, spaces: &config::Spaces) -> Result<(), String> {
        let stat = std::fs::read_to_string(self.all_cgroup().join("memory.stat")).ok();
        room(spaces, self.machine, stat.as_deref())
    }

    /// `inner` (a program and its arguments) as launch number `launch` of
    /// space number `space`: the `space-run` that puts it into its scope.
    /// The command has the compositor's environment; the caller sets what
    /// the space gets on it and hands the launch caller's own variables to
    /// its standard input as a JSON object ([`exec_main`] reads it in the
    /// scope). None of either is on an argument, here or later.
    pub fn command(&self, spaces: &config::Spaces, space: u32, launch: u32, space_dir: &Path, inner: &[String]) -> Command {
        self.scoped(spaces, space, launch_scope(self.compositor, space, launch), false, Some(space_dir.join("bus")), space_dir, inner)
    }

    /// The space's own user manager (`inner`), start number `start`, in a
    /// `Delegate=yes` scope of its slice ([`super::session`]).
    pub fn session_command(&self, spaces: &config::Spaces, space: u32, start: u32, space_dir: &Path, inner: &[String]) -> Command {
        self.scoped(spaces, space, super::session::session_scope(self.compositor, space, start), true, None, space_dir, inner)
    }

    #[allow(clippy::too_many_arguments)]
    fn scoped(&self, spaces: &config::Spaces, space: u32, unit: String, delegate: bool, wait_bus: Option<PathBuf>, space_dir: &Path, inner: &[String]) -> Command {
        let (all, one) = limits(spaces, self.machine);
        let run = Run {
            systemd_run: self.systemd_run.clone(),
            busctl: self.busctl.clone(),
            runtime: self.runtime.clone(),
            bus: self.bus.clone(),
            slices: vec![Slice { name: spaces_slice(self.compositor), properties: all }, Slice { name: space_slice(self.compositor, space), properties: one }],
            cgroup: self.space_cgroup(space),
            unit,
            delegate,
            wait_bus,
            no_expand: self.no_expand(),
            space_dir: space_dir.to_path_buf(),
            guard: self.guard.clone(),
        };
        let mut cmd = Command::new(&self.shim);
        cmd.arg0("exponential-compositor").arg("space-run").arg(serde_json::to_string(&run).expect("a launch is JSON")).arg("--").args(inner);
        cmd
    }

    /// `systemctl --user <args>` at the person's user manager.
    fn systemctl(&self, args: &[&str]) -> Command {
        let mut cmd = Command::new(&self.systemctl);
        cmd.arg("--user").args(args).env("XDG_RUNTIME_DIR", &self.runtime).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
        cmd
    }

    /// Tells the user manager to stop `unit` with all in it, and does not
    /// wait: the caller reaps the child ([`super::Spaces`]' tick).
    pub(crate) fn stop(&self, unit: &str) -> Option<Child> {
        self.systemctl(&["stop", unit]).spawn().inspect_err(|e| tracing::warn!("systemctl stop {unit}: {e}")).ok()
    }

    /// Stops `unit` and waits until it is gone, for at most `limit`: only
    /// at the session's start and at the compositor's end, never in the
    /// event loop.
    pub fn stop_now(&self, unit: &str, limit: Duration) -> bool {
        output_within(self.systemctl(&["stop", unit]), limit).is_some_and(|output| output.status.success())
    }

    /// Ends the slice of all spaces of the compositor whose process id
    /// this has: everything in its cgroup at once (whatever group or
    /// session it is in by now), then the user manager takes the units
    /// away, which is waited for, for [`EXIT_STOP`] at most. The user
    /// manager may linger after the person's logout: nothing else would
    /// end what a space left. `false` = the slice is still there.
    pub fn end(&self) -> bool {
        let (dir, deadline) = (self.all_cgroup(), Instant::now() + EXIT_STOP);
        kill_cgroup(&dir);
        let stopped = self.stop_now(&spaces_slice(self.compositor), EXIT_STOP);
        // A cgroup goes when its last process has: gone = nothing is left.
        while dir.is_dir() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(10));
        }
        stopped && !dir.is_dir()
    }

    /// The slices of all spaces that a compositor of this user left
    /// behind: one that is gone (its process id is nobody's or another
    /// program's), or an earlier one with this process id. Their spaces'
    /// processes still run. At the session's start, before any space.
    pub fn stale(&self) -> Vec<String> {
        let mut list = self.systemctl(&["list-units", "--all", "--plain", "--no-legend", "app-exponential_p*.slice"]);
        list.stdout(Stdio::piped());
        let Some(output) = output_within(list, Duration::from_secs(u64::from(ASK_SECONDS))) else { return Vec::new() };
        let alive = |pid: u32| pid != self.compositor && is_compositor(pid);
        String::from_utf8_lossy(&output.stdout)
            .lines()
            .filter_map(|line| line.split_whitespace().next())
            .filter(|unit| parse_spaces_slice(unit).is_some_and(|pid| !alive(pid)))
            .map(str::to_string)
            .collect()
    }
}

/// Whether process `pid` is a running exponential-compositor (its name, as
/// the kernel keeps 15 characters of it).
pub fn is_compositor(pid: u32) -> bool {
    std::fs::read_to_string(format!("/proc/{pid}/comm")).is_ok_and(|comm| "exponential-compositor".starts_with(comm.trim()) && comm.trim().len() >= 15)
}

/// `app-exponential_p41.slice` → 41: a compositor's slice of all spaces.
pub fn parse_spaces_slice(unit: &str) -> Option<u32> {
    let pid = unit.strip_prefix("app-exponential_p")?.strip_suffix(".slice")?;
    (!pid.is_empty() && pid.len() <= 10 && pid.bytes().all(|b| b.is_ascii_digit())).then(|| pid.parse().ok()).flatten()
}

/// Ends every process in the cgroup `dir` and below at once (the
/// kernel's `cgroup.kill`, Linux 5.14): what a stop left running, a scope
/// that asked not to be killed with it. `false` = nothing there (any
/// more), or a kernel without it.
pub(crate) fn kill_cgroup(dir: &Path) -> bool {
    dir.is_dir() && std::fs::write(dir.join("cgroup.kill"), "1").is_ok()
}

/// `name` on `PATH`.
fn on_path(name: &str) -> Option<PathBuf> {
    use std::os::unix::fs::PermissionsExt;
    std::env::split_paths(&std::env::var_os("PATH")?).map(|dir| dir.join(name)).find(|path| path.metadata().is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0))
}

/// Runs `cmd` for at most `limit` and returns what it wrote to its
/// standard output (when the caller piped it; a few lines at most, less
/// than a pipe holds); `None` = it could not start or did not end (it is
/// killed).
fn output_within(mut cmd: Command, limit: Duration) -> Option<Output> {
    let mut child = cmd.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::null()).spawn().ok()?;
    let deadline = Instant::now() + limit;
    loop {
        match child.try_wait() {
            Ok(Some(_)) => return child.wait_with_output().ok(),
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(5)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    }
}

/// `busctl --user call … Manager <method> <signature> <slice> …` with the
/// slice's limits: `StartTransientUnit` makes it, `SetUnitProperties` sets
/// them on one that is there.
fn busctl(run: &Run, slice: &Slice, make: bool) -> Command {
    let mut cmd = Command::new(&run.busctl);
    cmd.args(["--user", &format!("--timeout={ASK_SECONDS}"), "call", "org.freedesktop.systemd1", "/org/freedesktop/systemd1", "org.freedesktop.systemd1.Manager"]);
    if make {
        cmd.args(["StartTransientUnit", "ssa(sv)a(sa(sv))", &slice.name, "fail"]);
    } else {
        cmd.args(["SetUnitProperties", "sba(sv)", &slice.name, "true"]);
    }
    cmd.arg(slice.properties.len().to_string());
    for (name, value) in &slice.properties {
        cmd.args([name.as_str(), "t", &value.to_string()]);
    }
    if make {
        // No auxiliary units.
        cmd.arg("0");
    }
    cmd.env("XDG_RUNTIME_DIR", &run.runtime).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::piped());
    match &run.bus {
        Some(bus) => cmd.env("DBUS_SESSION_BUS_ADDRESS", bus),
        None => cmd.env_remove("DBUS_SESSION_BUS_ADDRESS"),
    };
    cmd
}

/// Makes `slice` with its limits, or sets them on the one that is there
/// (another launch of the space made it a moment ago, or the slice of all
/// spaces is an earlier space's: the person's limits of now apply).
fn ensure(run: &Run, slice: &Slice) -> Result<(), String> {
    let ask = |make: bool| busctl(run, slice, make).output().map_err(|e| format!("{}: {e}", run.busctl.display()));
    if ask(true)?.status.success() {
        return Ok(());
    }
    let set = ask(false)?;
    if set.status.success() {
        return Ok(());
    }
    Err(format!("the user manager refuses {}: {}", slice.name, String::from_utf8_lossy(&set.stderr).trim()))
}

/// `systemd-run --user --scope … -- argv` for `run`: the scope exists
/// before `argv` runs, in the same process. systemd-run itself needs the
/// PERSON's runtime directory (their user manager is in it), the program
/// the space's: `--setenv` is the one variable on an argument.
pub fn scope(run: &Run, argv: &[String]) -> Command {
    let mut cmd = Command::new(&run.systemd_run);
    cmd.args(["--user", "--scope", "--collect", "--quiet"]);
    if let Some(slice) = run.slices.last() {
        cmd.arg(format!("--slice={}", slice.name));
    }
    cmd.arg(format!("--unit={}", run.unit)).args(["-p", &format!("TimeoutStopSec={STOP_SECONDS}")]);
    if run.delegate {
        cmd.args(["-p", "Delegate=yes"]);
    }
    if run.no_expand {
        cmd.arg("--expand-environment=no");
    }
    cmd.arg(format!("--setenv=XDG_RUNTIME_DIR={}", run.space_dir.display())).arg("--").args(argv);
    cmd.env("XDG_RUNTIME_DIR", &run.runtime);
    cmd
}

/// Whether the bus at socket `bus` answers with its user manager on it
/// (`org.freedesktop.systemd1` has an owner: what `snap run` asks for)
/// within `limit`. Before that, the systemd package's `Exec=/bin/false`
/// activation file for that name would answer `snap run`.
fn bus_answers(busctl: &Path, bus: &Path, limit: Duration) -> bool {
    let deadline = Instant::now() + limit;
    loop {
        let mut ask = Command::new(busctl);
        ask.args([&format!("--address=unix:path={}", bus.display()), "--timeout=1", "call", "org.freedesktop.DBus", "/org/freedesktop/DBus", "org.freedesktop.DBus", "GetNameOwner", "s", "org.freedesktop.systemd1"]);
        if bus.exists() && output_within(ask, Duration::from_secs(2)).is_some_and(|output| output.status.success()) {
            return true;
        }
        if Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

fn failed(stage: &str, reason: String) -> ! {
    eprintln!("exponential-compositor {stage}: {reason}");
    std::process::exit(127)
}

/// `exponential-compositor space-run <run as JSON> -- argv…`: never
/// returns. Exit 127 with the reason when the program could not be put
/// into its scope; it has not run then. Its standard input goes through
/// to [`exec_main`] untouched.
pub fn run_main(args: impl Iterator<Item = String>) -> ! {
    let args: Vec<String> = args.collect();
    let (Some(spec), Some("--"), Some(argv)) = (args.first(), args.get(1).map(String::as_str), args.get(2..).filter(|argv| !argv.is_empty())) else {
        failed("space-run", "usage: space-run <launch> -- program [arguments]".into())
    };
    let run: Run = serde_json::from_str(spec).unwrap_or_else(|e| failed("space-run", format!("the launch is not JSON: {e}")));
    // A launch waits for its space's bus, which its manager brings (that
    // one is started first and makes the slices).
    if let Some(bus) = &run.wait_bus {
        if !bus_answers(&run.busctl, bus, Duration::from_secs(BUS_SECONDS)) {
            failed("space-run", format!("the space's session did not start within {BUS_SECONDS} s"));
        }
    }
    // The space's first start (or its slice was taken away since): the
    // slice of all spaces gets the person's limits of now, the space's
    // own is made. Asked of what is there, not remembered.
    if !run.cgroup.is_dir() {
        for slice in &run.slices {
            if let Err(reason) = ensure(&run, slice) {
                failed("space-run", reason);
            }
        }
    }
    if !run.space_dir.is_dir() {
        failed("space-run", "the space closed".into());
    }
    // The next program but one is this one again, inside the scope. It is
    // found through a descriptor that systemd-run inherits: the running
    // image, whatever a package upgrade did to the file since.
    let image = std::fs::File::open("/proc/self/exe").and_then(|image| fcntl_setfd(&image, FdFlags::empty()).map(|()| image.into_raw_fd()).map_err(Into::into));
    let image = image.unwrap_or_else(|e| failed("space-run", format!("this program's image: {e}")));
    let image_path = format!("/proc/self/fd/{image}");
    let spaces = run.space_dir.parent().unwrap_or(&run.space_dir).to_string_lossy().into_owned();
    let mut inside = vec![image_path.clone(), "space-enter".to_string()];
    if let Some(guard) = &run.guard {
        inside.extend(["--guard".to_string(), guard.to_string_lossy().into_owned()]);
    }
    inside.extend([spaces, "--".to_string(), image_path, "space-exec".to_string(), image.to_string(), "--".to_string()]);
    inside.extend(argv.iter().cloned());
    failed("space-run", format!("{}: {}", run.systemd_run.display(), scope(&run, &inside).exec()))
}

/// `exponential-compositor space-enter [--guard <socket>] <spaces> -- argv…`:
/// the first program in every systemd scope into a space (a launch's, and
/// the space's own user manager's), `<spaces>` = the compositor's
/// directory of spaces. With `--guard` it first asks the space guard
/// (#EDE-146..148) to guard the space slice it is in, and becomes `argv`
/// only when the guard says so: a space's first program runs with the
/// guard's programs on its slice, or not at all. Never returns.
pub fn enter_main(args: impl Iterator<Item = String>) -> ! {
    let mut args: Vec<String> = args.collect();
    let guard = match args.first().map(String::as_str) {
        Some("--guard") if args.len() > 1 => {
            let socket = args.remove(1);
            args.remove(0);
            Some(PathBuf::from(socket))
        }
        _ => None,
    };
    let (Some(spaces), Some("--"), Some(argv)) = (args.first(), args.get(1).map(String::as_str), args.get(2..).filter(|argv| !argv.is_empty())) else {
        failed("space-enter", "usage: space-enter [--guard <socket>] <spaces> -- program [arguments]".into())
    };
    if let Some(socket) = guard {
        if let Err(reason) = ede_space_guard::client::ask(&socket, &ede_space_guard::Request::Enter { spaces: spaces.clone() }) {
            failed("space-enter", reason);
        }
    }
    failed("space-enter", format!("{}: {}", argv[0], Command::new(&argv[0]).args(&argv[1..]).exec()))
}

/// Does the space guard answer at `socket` (it loads its programs on this
/// kernel), and does it refuse the person's Wayland socket too (#EDE-145;
/// a guard from before does not)? `Err` says why not. Blocks for at most
/// the guard client's timeout: in the session's probe thread only.
pub fn guard_probe(socket: &Path) -> Result<ede_space_guard::client::Probed, String> {
    if !socket.exists() {
        return Err("the space guard is not installed (exponential-space-guard.socket)".into());
    }
    ede_space_guard::client::probe(socket)
}

/// #EDE-146: polkit refuses a space's programs. Asked of polkit itself: a
/// probe in a scope of the slice of all spaces (no connect program there,
/// so it reaches polkit) checks the package's probe action, which anyone is
/// allowed and the package's rule answers with a challenge for a space's
/// process: `pkcheck` exits 2 exactly when the rule was read. `Err` says
/// why not. Blocks for at most 5 s: in the session's probe thread only.
pub fn polkit_probe(units: &Units) -> Result<(), String> {
    let mut probe = Command::new(&units.systemd_run);
    probe.args(["--user", "--scope", "--quiet", "--collect", &format!("--slice={}", spaces_slice(units.compositor)), &format!("--unit=exponential-p{}-polkit.scope", units.compositor), "-p", "RuntimeMaxSec=10"]);
    // From systemd 258 on a scope's argv has its `$…` replaced: `$$` would
    // reach the shell as `$`.
    if units.no_expand() {
        probe.arg("--expand-environment=no");
    }
    // The shell is the subject (in the scope); its line tells pkcheck's
    // answer apart from systemd-run's own failure. Both by their paths:
    // nothing on the person's PATH stands in for them.
    probe.args(["--", "/bin/sh", "-c", &format!("/usr/bin/pkcheck --action-id {POLKIT_PROBE_ACTION} --process $$ >/dev/null 2>&1; echo pkcheck=$?")]).env("XDG_RUNTIME_DIR", &units.runtime);
    let Some(out) = output_within(probe, Duration::from_secs(5)) else { return Err("the polkit probe did not answer".into()) };
    match String::from_utf8_lossy(&out.stdout).trim() {
        "pkcheck=2" => Ok(()),
        "pkcheck=0" => Err(format!("polkit authorizes a space's programs: {POLKIT_RULE} is missing or not read")),
        "pkcheck=1" => Err("polkit refuses the probe outright: another rule answers before the package's".into()),
        "pkcheck=3" => Err("the polkit probe was dismissed".into()),
        "" => Err(format!("the polkit probe could not start ({})", out.status)),
        other => Err(format!("the polkit probe ended with {other} (no pkcheck, no polkit, or no probe action)")),
    }
}

/// The package's polkit rule (#EDE-146).
pub const POLKIT_RULE: &str = "/usr/share/polkit-1/rules.d/00-00-exponential-agent-spaces.rules";
/// The package's probe action, which the rule answers with a challenge.
pub const POLKIT_PROBE_ACTION: &str = "at.exponential.desktop.space-probe";

/// `… space-exec <descriptor> -- argv…`, in the launch's scope: closes
/// the descriptor it was found through, reads the caller's environment
/// from its standard input (a JSON object; nothing = none), and becomes
/// the program with it, its standard input closed as a launch's always
/// was. Never returns.
pub fn exec_main(args: impl Iterator<Item = String>) -> ! {
    let args: Vec<String> = args.collect();
    let (Some(image), Some("--"), Some(argv)) = (args.first().and_then(|fd| fd.parse::<i32>().ok()), args.get(1).map(String::as_str), args.get(2..).filter(|argv| !argv.is_empty())) else {
        failed("space-exec", "usage: space-exec <descriptor> -- program [arguments]".into())
    };
    if image > 2 {
        // SAFETY: the descriptor `space-run` opened for this exec and
        // named here; nothing else in this fresh process holds it.
        drop(unsafe { OwnedFd::from_raw_fd(image) });
    }
    let mut text = String::new();
    if let Err(e) = std::io::stdin().read_to_string(&mut text) {
        failed("space-exec", format!("the caller's environment: {e}"));
    }
    let env: BTreeMap<String, String> = if text.trim().is_empty() { BTreeMap::new() } else { serde_json::from_str(&text).unwrap_or_else(|e| failed("space-exec", format!("the caller's environment is not JSON: {e}"))) };
    failed("space-exec", format!("{}: {}", argv[0], Command::new(&argv[0]).args(&argv[1..]).envs(env).stdin(Stdio::null()).exec()))
}

/// `exponential-compositor space-end <pid>`: ends what the agent spaces of
/// the compositor with that process id left in their slice
/// ([`Units::end`]). exponential-session runs it when its compositor has
/// ended, however it ended (the session's SIGTERM, a crash): a compositor
/// that is killed stops nothing itself. Exit 0 also when there is no user
/// manager or no such slice.
pub fn end_main(mut args: impl Iterator<Item = String>) -> ! {
    let Some(compositor) = args.next().and_then(|pid| pid.parse::<u32>().ok()) else {
        failed("space-end", "usage: space-end <the compositor's process id>".into())
    };
    if let Ok(units) = Units::probe() {
        let units = Units { compositor, ..units };
        if units.all_cgroup().is_dir() && !units.end() {
            failed("space-end", format!("{} was not stopped within {EXIT_STOP:?}", spaces_slice(compositor)));
        }
    }
    std::process::exit(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn units() -> Units {
        Units {
            systemd_run: "/usr/bin/systemd-run".into(),
            busctl: "/usr/bin/busctl".into(),
            systemctl: "/usr/bin/systemctl".into(),
            version: 255,
            shim: "/proc/self/exe".into(),
            runtime: "/run/user/1000".into(),
            bus: Some("unix:path=/run/user/1000/bus".into()),
            compositor: 41,
            machine: Machine { cpus: 8, memory: 16 * GIB },
            app_slice: "/sys/fs/cgroup/user.slice/user-1000.slice/user@1000.service/app.slice".into(),
            manager: "/usr/lib/systemd/systemd".into(),
            dbus_daemon: "/usr/bin/dbus-daemon".into(),
            guard: None,
        }
    }

    /// #EDE-146: the polkit probe asks polkit from a scope of the slice of
    /// all spaces and reads pkcheck's answer: 1 (not authorized) = the rule
    /// is read; 0 = it is not; anything else, or no answer, says why.
    #[test]
    fn the_polkit_probe_reads_pkchecks_answer_from_a_scope_of_the_spaces_slice() {
        let dir = std::env::temp_dir().join(format!("ede-polkit-probe-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let fake = dir.join("systemd-run");
        // A stand-in that writes down its arguments, prints what the probe's
        // shell would, and exits as systemd-run would.
        let answer = |line: &str, code: u8| {
            use std::os::unix::fs::PermissionsExt;
            std::fs::write(&fake, format!("#!/bin/sh\nprintf '%s\\n' \"$@\" > '{}'\nprintf '{line}'\nexit {code}\n", dir.join("args").display())).unwrap();
            std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o755)).unwrap();
            polkit_probe(&Units { systemd_run: fake.clone(), ..units() })
        };
        assert_eq!(answer("pkcheck=2\\n", 0), Ok(()));
        let args = std::fs::read_to_string(dir.join("args")).unwrap();
        assert_eq!(
            args.lines().collect::<Vec<_>>(),
            ["--user", "--scope", "--quiet", "--collect", "--slice=app-exponential_p41.slice", "--unit=exponential-p41-polkit.scope", "-p", "RuntimeMaxSec=10", "--expand-environment=no", "--", "/bin/sh", "-c", "/usr/bin/pkcheck --action-id at.exponential.desktop.space-probe --process $$ >/dev/null 2>&1; echo pkcheck=$?"]
        );
        assert_eq!(answer("pkcheck=0\\n", 0), Err(format!("polkit authorizes a space's programs: {POLKIT_RULE} is missing or not read")));
        // A plain NO (another rule, a broken one) does not prove ours.
        assert!(answer("pkcheck=1\\n", 0).unwrap_err().contains("another rule"));
        assert!(answer("pkcheck=127\\n", 0).unwrap_err().contains("no pkcheck"));
        // systemd-run's own failure (exit 1, the shell never ran) is no answer.
        assert!(answer("", 1).unwrap_err().contains("could not start"));
        // systemd before 254 has no --expand-environment (and expands nothing).
        let _ = answer("pkcheck=2\\n", 0);
        assert_eq!(polkit_probe(&Units { systemd_run: fake.clone(), version: 253, ..units() }), Ok(()));
        assert!(!std::fs::read_to_string(dir.join("args")).unwrap().contains("--expand-environment"));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    fn args(cmd: &Command) -> Vec<String> {
        cmd.get_args().map(|arg| arg.to_string_lossy().into_owned()).collect()
    }

    fn pages(bytes: u64) -> u64 {
        bytes / MEMORY_STEP * MEMORY_STEP
    }

    #[test]
    fn the_machine_and_systemd_as_their_files_tell() {
        assert_eq!(parse_mem_total("MemFree: 1 kB\nMemTotal:       32786188 kB\nSwapTotal: 0 kB\n"), Some(32786188 * 1024));
        assert_eq!(parse_mem_total("MemTotal: many kB\n"), None);
        assert_eq!(parse_systemd_version("systemd 255 (255.4-1ubuntu8.18)\n+PAM +AUDIT\n"), Some(255));
        assert_eq!(parse_systemd_version("systemd 259\n"), Some(259));
        assert_eq!(parse_systemd_version("systemd-run: unrecognized option\n"), None);
        assert_eq!(parse_spaces_slice("app-exponential_p41.slice"), Some(41));
        for other in ["app-exponential_p41-s3.slice", "app-exponential.slice", "app-exponential_p.slice", "app-exponential_px.slice", "app-exponential_p41.scope", "app-exponential-p41.slice"] {
            assert_eq!(parse_spaces_slice(other), None, "{other}");
        }
    }

    /// Where the user manager's cgroups are is read from a scope of the
    /// compositor's own, and only a unified hierarchy will do.
    #[test]
    fn the_probes_scope_says_where_app_slice_is() {
        let app = |cgroup: &str| parse_app_slice(cgroup).map(|dir| dir.to_string_lossy().into_owned());
        assert_eq!(app("0::/user.slice/user-1000.slice/user@1000.service/app.slice/run-r5c0d.scope\n"), Some("/sys/fs/cgroup/user.slice/user-1000.slice/user@1000.service/app.slice".into()));
        assert_eq!(app("0::/user.slice/user-1001.slice/user@1001.service/app.slice/run-p9-i10.scope"), Some("/sys/fs/cgroup/user.slice/user-1001.slice/user@1001.service/app.slice".into()));
        assert_eq!(app("12:memory:/user.slice\n1:name=systemd:/user.slice/user-1000.slice/user@1000.service/app.slice/run-r1.scope\n"), None, "cgroup v1: no unified line");
        assert_eq!(app("0::/user.slice/user-1000.slice/session-3.scope\n"), None, "the scope is not the user manager's");
        assert_eq!(app("0::/user.slice/user-1000.slice/user@1000.service/app.slice/x.service\n"), None);
        assert_eq!(app(""), None);
        let units = units();
        assert_eq!(units.all_cgroup(), units.app_slice.join("app-exponential_p41.slice"));
        assert_eq!(units.space_cgroup(3), units.app_slice.join("app-exponential_p41.slice/app-exponential_p41-s3.slice"));
        assert!(!kill_cgroup(Path::new("/nonexistent/cgroup")));
    }

    /// One space per GiB of what all spaces may use, unless the person
    /// says; a new one only while a still space fits under `MemoryHigh`.
    #[test]
    fn how_many_spaces_and_whether_one_more_fits() {
        let spaces = config::Spaces::default();
        let machine = |gib: u64| Machine { cpus: 8, memory: gib * GIB };
        // MemTotal is a little under the nominal size.
        let total = |gb: u64| Machine { cpus: 8, memory: gb * GIB / 100 * 97 };
        assert_eq!([4, 8, 16, 32, 64].map(|gb| max_spaces(&spaces, total(gb))), [2, 4, 8, 16, 31]);
        assert_eq!(max_spaces(&spaces, machine(256)), 32, "never more than 32");
        assert_eq!(max_spaces(&spaces, machine(2)), 1);
        assert_eq!(max_spaces(&spaces, Machine { cpus: 1, memory: GIB / 2 }), 1, "at least one");
        assert_eq!(max_spaces(&config::Spaces { max: Some(3), ..spaces.clone() }, machine(64)), 3, "the person's number");

        assert_eq!(held("anon 1048576\nfile 99999999\nkernel 5\nshmem 2097152\nshmem_thp 7\n"), 3 << 20);
        assert_eq!(held(""), 0);
        // 16 GiB: all spaces 8 GiB, MemoryHigh 7.2 GiB; one more fits while
        // they hold at most 6.2 GiB.
        let high = pages(8 * GIB / 10 * 9);
        assert_eq!(room(&spaces, machine(16), None), Ok(()), "nothing made yet");
        assert_eq!(room(&spaces, machine(16), Some(&format!("anon {}\nshmem 0\n", high - STILL_SPACE))), Ok(()));
        let refused = room(&spaces, machine(16), Some(&format!("anon {}\nshmem 4096\n", high - STILL_SPACE)));
        assert_eq!(refused, Err(format!("no memory for another space: the agent spaces hold {} MiB of their {} MiB", (high - STILL_SPACE + 4096) >> 20, high >> 20)));
        assert_eq!(described(&spaces, machine(16)), "8 spaces at most, 32 launches each; together 8.0 GiB of memory, one up to 4.8 GiB: they share it, a new space is made only while 1.0 GiB is free under 7.2 GiB");
    }

    /// Half of the cores and half of the memory for all spaces together by
    /// default; one space at least 2 GiB, never more than all of them.
    #[test]
    fn the_limits_are_fractions_of_the_machine() {
        let value = |list: &[(String, u64)], name: &str| list.iter().find(|(n, _)| n == name).map(|(_, v)| *v);
        let (all, one) = limits(&config::Spaces::default(), Machine { cpus: 8, memory: 16 * GIB });
        assert_eq!(value(&all, "CPUQuotaPerSecUSec"), Some(4_000_000), "four of eight cores");
        assert_eq!(value(&all, "CPUWeight"), Some(10));
        assert_eq!(value(&all, "MemoryMax"), Some(8 * GIB));
        assert_eq!(value(&all, "MemoryHigh"), Some(pages(8 * GIB / 10 * 9)));
        assert_eq!(value(&all, "MemorySwapMax"), Some(2 * GIB));
        assert_eq!(value(&one, "MemoryMax"), Some(pages(16 * GIB * 30 / 100)));
        assert!(all.iter().chain(&one).filter(|(name, _)| name.starts_with("Memory")).all(|(_, bytes)| bytes % MEMORY_STEP == 0), "whole pages: {all:?} {one:?}");
        assert_eq!(value(&one, "TasksMax"), Some(4096));
        // A small machine: one space gets its 2 GiB, capped by what all get.
        let (all, one) = limits(&config::Spaces::default(), Machine { cpus: 2, memory: 4 * GIB });
        assert_eq!((value(&all, "MemoryMax"), value(&one, "MemoryMax")), (Some(2 * GIB), Some(2 * GIB)));
        assert_eq!(value(&all, "CPUQuotaPerSecUSec"), Some(1_000_000), "one of two cores");
        let (all, one) = limits(&config::Spaces::default(), Machine { cpus: 4, memory: 2 * GIB });
        assert_eq!((value(&all, "MemoryMax"), value(&one, "MemoryMax")), (Some(GIB), Some(GIB)));
        // The person's own numbers.
        let own = config::Spaces { cpu_quota: 25, memory_max: 20, space_memory_max: 10, tasks_max: 512, ..Default::default() };
        let (all, one) = limits(&own, Machine { cpus: 32, memory: 128 * GIB });
        assert_eq!(value(&all, "CPUQuotaPerSecUSec"), Some(8_000_000));
        assert_eq!(value(&all, "MemoryMax"), Some(pages(128 * GIB * 20 / 100)));
        assert_eq!(value(&one, "MemoryMax"), Some(pages(128 * GIB * 10 / 100)));
        assert_eq!(value(&one, "TasksMax"), Some(512));
    }

    /// A launch is `space-run` with what it needs as one JSON argument,
    /// then the program as it was given: nothing of an environment.
    #[test]
    fn a_launch_is_space_run_with_its_slices_and_its_scope() {
        let inner = ["dbus-run-session".to_string(), "--".into(), "foot".into(), "-e".into(), "$HOME".into()];
        let cmd = units().command(&config::Spaces::default(), 3, 7, Path::new("/run/user/1000/exponential-desktop/wayland-1.spaces/space-3"), &inner);
        assert_eq!(cmd.get_program(), "/proc/self/exe", "the running image, also after an upgrade replaced the file");
        let argv = args(&cmd);
        assert_eq!((argv[0].as_str(), argv[2].as_str()), ("space-run", "--"));
        assert_eq!(argv[3..], inner);
        let run: Run = serde_json::from_str(&argv[1]).unwrap();
        assert_eq!(run.slices.iter().map(|s| s.name.as_str()).collect::<Vec<_>>(), ["app-exponential_p41.slice", "app-exponential_p41-s3.slice"], "the slice of all spaces first");
        assert_eq!(run.unit, "exponential-p41-s3-7.scope");
        assert_eq!(run.cgroup, units().space_cgroup(3));
        assert!(run.no_expand, "systemd 255 knows the switch");
        assert_eq!((run.delegate, run.wait_bus.as_deref()), (false, Some(Path::new("/run/user/1000/exponential-desktop/wayland-1.spaces/space-3/bus"))), "a launch waits for its space's bus");
        assert_eq!(cmd.get_envs().count(), 0, "the caller sets the space's environment");

        let scope = scope(&run, &inner);
        assert_eq!(
            args(&scope),
            ["--user", "--scope", "--collect", "--quiet", "--slice=app-exponential_p41-s3.slice", "--unit=exponential-p41-s3-7.scope", "-p", "TimeoutStopSec=2", "--expand-environment=no", "--setenv=XDG_RUNTIME_DIR=/run/user/1000/exponential-desktop/wayland-1.spaces/space-3", "--", "dbus-run-session", "--", "foot", "-e", "$HOME"]
        );
        let envs: Vec<_> = scope.get_envs().collect();
        assert_eq!(envs, [(std::ffi::OsStr::new("XDG_RUNTIME_DIR"), Some(std::ffi::OsStr::new("/run/user/1000")))], "systemd-run finds the PERSON's user manager");

        // 252 and 253 do not know the switch (and do not expand).
        let old = Units { version: 253, ..units() };
        let run: Run = serde_json::from_str(&args(&old.command(&config::Spaces::default(), 1, 1, Path::new("/s"), &inner))[1]).unwrap();
        assert!(!run.no_expand && !args(&super::scope(&run, &inner)).iter().any(|arg| arg.starts_with("--expand-environment")));
    }

    /// The space's own user manager: a scope of the space's slice whose
    /// cgroup is its own to manage, waiting for no bus (it is the bus's).
    #[test]
    fn a_spaces_manager_has_a_delegated_scope() {
        let cmd = units().session_command(&config::Spaces::default(), 3, 2, Path::new("/s/space-3"), &["/usr/lib/systemd/systemd".to_string(), "--user".to_string()]);
        let run: Run = serde_json::from_str(&args(&cmd)[1]).unwrap();
        assert_eq!((run.unit.as_str(), run.delegate, run.wait_bus.as_deref()), ("exponential-p41-s3-session2.scope", true, None));
        let scope = args(&scope(&run, &["systemd".to_string()]));
        assert!(scope.windows(2).any(|pair| pair == ["-p", "Delegate=yes"]), "{scope:?}");
        assert_eq!(args(&cmd)[3..], ["/usr/lib/systemd/systemd", "--user"]);
    }

    #[test]
    fn a_slice_is_made_or_gets_its_limits_set_over_the_persons_bus() {
        let run: Run = serde_json::from_str(&args(&units().command(&config::Spaces::default(), 3, 1, Path::new("/s"), &["true".to_string()]))[1]).unwrap();
        let make = busctl(&run, &run.slices[1], true);
        let argv = args(&make);
        assert_eq!(argv[..10], ["--user", "--timeout=5", "call", "org.freedesktop.systemd1", "/org/freedesktop/systemd1", "org.freedesktop.systemd1.Manager", "StartTransientUnit", "ssa(sv)a(sa(sv))", "app-exponential_p41-s3.slice", "fail"]);
        let memory = pages(16 * GIB * 30 / 100);
        assert_eq!(argv[10..], ["3", "MemoryMax", "t", &memory.to_string(), "MemoryHigh", "t", &pages(memory / 10 * 9).to_string(), "TasksMax", "t", "4096", "0"]);
        let set = args(&busctl(&run, &run.slices[0], false));
        assert_eq!(set[6..11], ["SetUnitProperties", "sba(sv)", "app-exponential_p41.slice", "true", "5"]);
        assert_eq!(set.last().map(String::as_str), Some(&*(2 * GIB).to_string()), "no auxiliary units after the properties");
        let env = |cmd: &Command, name: &str| cmd.get_envs().find(|(n, _)| *n == name).map(|(_, v)| v.map(|v| v.to_string_lossy().into_owned()));
        assert_eq!(env(&make, "XDG_RUNTIME_DIR"), Some(Some("/run/user/1000".into())));
        assert_eq!(env(&make, "DBUS_SESSION_BUS_ADDRESS"), Some(Some("unix:path=/run/user/1000/bus".into())));
        let no_bus = Run { bus: None, ..run.clone() };
        assert_eq!(env(&busctl(&no_bus, &run.slices[0], true), "DBUS_SESSION_BUS_ADDRESS"), Some(None), "never the space's: the one in the runtime directory");
    }
}
