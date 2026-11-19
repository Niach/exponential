//! #EDE-126: launch into a space. The compositor starts the program, so
//! what was launched ends with its space also after its launcher crashed,
//! and the compositor alone decides the environment
//! ([`Ede::space_command`]): the space's socket and runtime directory,
//! a profile of the space's own, never the person's displays, session bus
//! or token.
//!
//! What a launch is NOT: contained. It runs as the person's user with
//! their `HOME`; a process that starts a session of its own outlives the
//! group kill. Grants and filters guard against accidents, not against a
//! hostile holder of a space token.

use std::{
    collections::BTreeMap,
    os::unix::{fs::DirBuilderExt, process::CommandExt},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};

use ede_protocol::{ErrorCode, Failure, Launched};
use smithay::reexports::rustix::process::{kill_process_group, set_parent_process_death_signal, Pid, Signal};

use super::Spaces;
use crate::{desk::DeskId, state::Ede};

/// What all spaces together may have running.
pub const MAX_LAUNCHED: usize = 32;
/// After a space closed its processes get this long to end by themselves.
const TERM_GRACE: Duration = Duration::from_secs(2);
/// The person's session: dropped from a launch's environment.
const DROPPED: [&str; 6] = ["DISPLAY", "WAYLAND_SOCKET", "XDG_ACTIVATION_TOKEN", "DESKTOP_STARTUP_ID", "DBUS_SESSION_BUS_ADDRESS", "EDE_TOKEN"];
/// The compositor's to set: with [`DROPPED`], refused in a caller's `env`.
const SET: [&str; 2] = ["WAYLAND_DISPLAY", "XDG_RUNTIME_DIR"];
/// A profile: where an app keeps its settings, caches, data and state.
const PROFILE: [(&str, &str); 4] = [("XDG_CONFIG_HOME", "config"), ("XDG_CACHE_HOME", "cache"), ("XDG_DATA_HOME", "data"), ("XDG_STATE_HOME", "state")];

/// Whose profile a launched app runs with. The compositor's own policy
/// decides ([`Ede::launch_profile`]); a launch request cannot name it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Profile {
    /// The space's own, fresh and signed out, removed when the space
    /// closes. An app that is already running on the person's desk starts
    /// a second instance here instead of joining that one (its window
    /// would open on the person's desk and take their keyboard).
    Space,
    /// The person's: signed in as them. Nothing chooses it yet.
    Person,
}

/// One launched process (the head of its own process group).
pub(crate) struct Launch {
    desk: DeskId,
    child: Child,
    /// Its space closed then: the group was told to end.
    closed: Option<Instant>,
    killed: bool,
}

impl Launch {
    fn signal(&self, signal: Signal) {
        if let Some(group) = Pid::from_raw(self.child.id() as i32) {
            let _ = kill_process_group(group, signal);
        }
    }
}

/// A space's home: `home` in its own directory, so it goes the way that
/// directory goes at the close (tried again while something of a dying
/// process is still mounted or written in it).
pub(crate) fn home_of(dir: &Path) -> PathBuf {
    dir.join("home")
}

/// `program` as the launch will find it: a path as it is, a name on `PATH`.
fn found(program: &str) -> bool {
    use std::os::unix::fs::PermissionsExt;
    let runs = |path: &Path| path.metadata().is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0);
    if program.contains('/') {
        return runs(Path::new(program));
    }
    std::env::var_os("PATH").is_some_and(|path| std::env::split_paths(&path).any(|dir| runs(&dir.join(program))))
}

impl Spaces {
    /// The processes launched into spaces that still run (or wait to be reaped).
    pub fn launched(&self) -> usize {
        self.launches.len()
    }

    /// The compositor ends: nothing launched outlives it.
    pub(crate) fn kill_launches(&mut self) {
        for mut launch in self.launches.drain(..) {
            launch.signal(Signal::KILL);
            let _ = launch.child.wait();
        }
    }
}

impl Ede {
    /// The profile a launch into a space gets: the space's own. The one
    /// place where a later policy (the person's switch per run) chooses
    /// [`Profile::Person`]; never the launch request.
    fn launch_profile(&self, _desk: DeskId) -> Profile {
        Profile::Space
    }

    /// `argv` as a command for a client of space `desk`, the one place
    /// that decides what a launch into a space gets: the compositor's
    /// environment without the person's session ([`DROPPED`]), the space's
    /// socket as `WAYLAND_DISPLAY`, the space's own directory as
    /// `XDG_RUNTIME_DIR` (so nothing found through the runtime directory
    /// is the person's: the bus, PipeWire, per-user sockets), a session
    /// bus of its own when `dbus-run-session` is installed and none
    /// otherwise, and its `profile`. The caller's `env` goes on top.
    pub fn space_command(&self, desk: DeskId, argv: &[String], env: &BTreeMap<String, String>, cwd: Option<&str>, profile: Profile) -> Result<Command, Failure> {
        let fail = |code, message: &str| Failure::new(code, message);
        let space = self.spaces.get(desk).ok_or_else(|| fail(ErrorCode::NotFound, "no such space"))?;
        let Some(program) = argv.first() else { return Err(fail(ErrorCode::Invalid, "argv is empty")) };
        if let Some(name) = env.keys().find(|name| DROPPED.contains(&name.as_str()) || SET.contains(&name.as_str()) || PROFILE.iter().any(|(var, _)| var == name)) {
            return Err(fail(ErrorCode::Invalid, &format!("{name} is the compositor's to set")));
        }
        if !found(program) {
            return Err(fail(ErrorCode::NotFound, &format!("no such program: {program}")));
        }
        let cwd = cwd.map(PathBuf::from).or_else(|| std::env::var_os("HOME").map(PathBuf::from)).unwrap_or_else(|| "/".into());
        if !cwd.is_dir() {
            return Err(fail(ErrorCode::NotFound, &format!("no such directory: {}", cwd.display())));
        }
        let mut cmd = if found("dbus-run-session") {
            let mut cmd = Command::new("dbus-run-session");
            cmd.arg("--").args(argv);
            cmd
        } else {
            let mut cmd = Command::new(program);
            cmd.args(&argv[1..]);
            cmd
        };
        for name in DROPPED {
            cmd.env_remove(name);
        }
        cmd.env("WAYLAND_DISPLAY", space.socket()).env("XDG_RUNTIME_DIR", &space.dir);
        if profile == Profile::Space {
            let home = home_of(&space.dir);
            for (var, sub) in PROFILE {
                let dir = home.join(sub);
                std::fs::DirBuilder::new().recursive(true).mode(0o700).create(&dir).map_err(|e| fail(ErrorCode::Unavailable, &format!("the space's home: {e}")))?;
                cmd.env(var, dir);
            }
        }
        cmd.envs(env).current_dir(cwd).stdin(Stdio::null()).process_group(0);
        // SAFETY: between fork and exec only prctl runs, which is
        // async-signal-safe: a compositor that dies takes its launches along.
        unsafe {
            cmd.pre_exec(|| Ok(set_parent_process_death_signal(Some(Signal::TERM))?));
        }
        Ok(cmd)
    }

    /// `launch` of a space connection: starts `argv` in its space `desk`.
    pub(crate) fn space_launch(&mut self, desk: DeskId, argv: Vec<String>, env: BTreeMap<String, String>, cwd: Option<String>) -> Result<Launched, Failure> {
        let mut cmd = self.space_command(desk, &argv, &env, cwd.as_deref(), self.launch_profile(desk))?;
        self.space_launches_tick();
        if self.spaces.launches.len() >= MAX_LAUNCHED {
            return Err(Failure::new(ErrorCode::Limit, &format!("{MAX_LAUNCHED} launched processes at most")));
        }
        let child = cmd.spawn().map_err(|e| Failure::new(if e.kind() == std::io::ErrorKind::NotFound { ErrorCode::NotFound } else { ErrorCode::Unavailable }, &format!("{}: {e}", argv[0])))?;
        let pid = child.id();
        self.spaces.launches.push(Launch { desk, child, closed: None, killed: false });
        Ok(Launched { pid })
    }

    /// Space `desk` closes: what was launched into it is told to end. Its
    /// home goes with the space's directory.
    pub(crate) fn space_launches_closed(&mut self, desk: DeskId) {
        for launch in self.spaces.launches.iter_mut().filter(|launch| launch.desk == desk) {
            launch.signal(Signal::TERM);
            launch.closed = Some(Instant::now());
        }
    }

    /// Once a second (and before a launch counts): reaps what ended, and
    /// kills what outlived its space by [`TERM_GRACE`].
    pub(crate) fn space_launches_tick(&mut self) {
        self.spaces.launches.retain_mut(|launch| {
            if launch.closed.is_some_and(|at| at.elapsed() > TERM_GRACE) && !std::mem::replace(&mut launch.killed, true) {
                launch.signal(Signal::KILL);
            }
            matches!(launch.child.try_wait(), Ok(None))
        });
    }
}
