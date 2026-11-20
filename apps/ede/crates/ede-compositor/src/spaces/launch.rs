//! #EDE-126: launch into a space. The compositor starts the program, so
//! what was launched ends with its space also after its launcher crashed,
//! and the compositor alone decides the environment
//! ([`Ede::space_command`]): the space's socket and runtime directory,
//! a profile of the space's own, never the person's displays, session bus
//! or token.
//!
//! #EDE-138: with a systemd user manager a launch runs in a scope of its
//! space's own slice ([`super::slice`]): under the person's limits, out
//! of their login session scope, and ended with the space also when it
//! started a session of its own.
//!
//! What a launch is NOT: contained. It runs as the person's user with
//! their `HOME`; without a user manager a process that starts a session
//! of its own outlives the group kill. Grants, filters and slices guard
//! against accidents, not against a hostile holder of a space token.

use std::{
    collections::BTreeMap,
    io::{Seek, Write},
    os::unix::{
        fs::{DirBuilderExt, OpenOptionsExt},
        process::CommandExt,
    },
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};

use ede_protocol::{ErrorCode, Failure, Launched};
use smithay::reexports::rustix::process::{kill_process_group, set_parent_process_death_signal, Pid, Signal};

use super::Spaces;
use crate::{desk::DeskId, state::Ede};

/// What all spaces together may have running where launches do not run in
/// systemd scopes; where they do, one space may have `[spaces] launches`
/// (#EDE-138).
pub const MAX_LAUNCHED: usize = 32;
/// After a space closed its processes get this long to end by themselves.
const TERM_GRACE: Duration = Duration::from_secs(2);
/// The person's session: never in a launch's environment, and refused in a
/// caller's `env`.
const DROPPED: [&str; 6] = ["DISPLAY", "WAYLAND_SOCKET", "XDG_ACTIVATION_TOKEN", "DESKTOP_STARTUP_ID", "DBUS_SESSION_BUS_ADDRESS", "EDE_TOKEN"];
/// The compositor's to set: with [`DROPPED`], refused in a caller's `env`.
const SET: [&str; 5] = ["WAYLAND_DISPLAY", "XDG_RUNTIME_DIR", "TMUX_TMPDIR", "SCREENDIR", "GNUPGHOME"];
/// What a launch takes of the compositor's environment (#EDE-138): who
/// the person is, their language, where programs and their files are, the
/// desktop's name and the toolkits' hints. NOTHING else: no agent's socket
/// (`SSH_AUTH_SOCK`, `GPG_AGENT_INFO`, a keyring's), no terminal
/// multiplexer of the person's (`TMUX`, `STY`), no service manager's
/// (`NOTIFY_SOCKET`), no container engine's (`DOCKER_HOST`), whatever the
/// login put there.
const KEPT: [&str; 26] = [
    "PATH", "HOME", "USER", "LOGNAME", "SHELL", "LANG", "LANGUAGE", "TZ",
    "XDG_CURRENT_DESKTOP", "XDG_SESSION_DESKTOP", "XDG_SESSION_TYPE", "XDG_DATA_DIRS", "XDG_CONFIG_DIRS", "XDG_MENU_PREFIX",
    "MOZ_ENABLE_WAYLAND", "QT_QPA_PLATFORM", "QT_QPA_PLATFORMTHEME", "QT_WAYLAND_DISABLE_WINDOWDECORATION", "ELECTRON_OZONE_PLATFORM_HINT", "_JAVA_AWT_WM_NONREPARENTING", "GDK_BACKEND", "SDL_VIDEODRIVER",
    "XCURSOR_THEME", "XCURSOR_SIZE", "XCURSOR_PATH", "GTK_THEME",
];

/// Whether the compositor's variable `name` goes into a launch: [`KEPT`],
/// and the locale's `LC_*`.
fn kept(name: &str) -> bool {
    KEPT.contains(&name) || name.starts_with("LC_")
}
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
    /// Its process id, which is also its group's ([`Ede::space_of_process`]).
    pub(crate) fn pid(&self) -> u32 {
        self.child.id()
    }

    /// The space it was launched into.
    pub(crate) fn desk(&self) -> DeskId {
        self.desk
    }

    fn signal(&self, signal: Signal) {
        if let Some(group) = Pid::from_raw(self.child.id() as i32) {
            let _ = kill_process_group(group, signal);
        }
    }
}


/// The caller's variables for a launch in a scope, as its standard input:
/// a JSON object in a file of the space's directory that has no name any
/// more. `space-exec` reads it inside the scope, so no program before the
/// scope (the launch's first two stages, `busctl`) runs with a variable
/// the caller chose, and none is on an argument.
fn callers_environment(dir: &Path, launch: u32, env: &BTreeMap<String, String>) -> std::io::Result<std::fs::File> {
    let path = dir.join(format!(".launch-{launch}"));
    let mut file = std::fs::OpenOptions::new().read(true).write(true).create_new(true).mode(0o600).open(&path)?;
    let written = file.write_all(serde_json::to_string(env).expect("an environment is JSON").as_bytes()).and_then(|()| file.rewind());
    std::fs::remove_file(&path)?;
    written.map(|()| file)
}

/// Its own process group, ended with the compositor: a launch, and a
/// space's user manager.
pub(crate) fn own_group_dying_with_us(cmd: &mut Command) {
    cmd.process_group(0);
    // SAFETY: between fork and exec only prctl runs, which is
    // async-signal-safe: a compositor that dies takes its launches along.
    unsafe {
        cmd.pre_exec(|| Ok(set_parent_process_death_signal(Some(Signal::TERM))?));
    }
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
        // #EDE-138: first what is in the slices, whatever group or session
        // it is in by now (a launch that left its group would be waited
        // for without end below): everything at once, then the user
        // manager takes the units away, which is waited for, for a moment.
        // The user manager may linger after the person's logout: nothing
        // else would end what a space left.
        if let Some(units) = self.units.as_ref().filter(|_| self.sliced) {
            if !units.end() {
                tracing::warn!("the slice of all spaces was not stopped within {:?}", super::slice::EXIT_STOP);
            }
        }
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

    /// What a program in space `desk` starts with (a launch, the space's
    /// own user manager): an allowlist of the compositor's environment,
    /// not the compositor's minus a list (what the login put there, a tmux
    /// server or an ssh agent, stays out); the space's socket, runtime
    /// directory and bus; a tmux server and screen sessions of its own
    /// (else an agent's `tmux` opens a window in the PERSON's server,
    /// outside the space); and its `profile`.
    pub(crate) fn space_environment(&self, desk: DeskId, profile: Profile) -> Result<Vec<(std::ffi::OsString, std::ffi::OsString)>, Failure> {
        let fail = |code, message: &str| Failure::new(code, message);
        let space = self.spaces.get(desk).ok_or_else(|| fail(ErrorCode::NotFound, "no such space"))?;
        let mut env: Vec<(std::ffi::OsString, std::ffi::OsString)> = std::env::vars_os().filter(|(name, _)| name.to_str().is_some_and(kept)).collect();
        let mut set = |name: &str, value: &dyn AsRef<std::ffi::OsStr>| env.push((name.into(), value.as_ref().to_os_string()));
        let screens = space.dir.join("screen");
        std::fs::DirBuilder::new().recursive(true).mode(0o700).create(&screens).map_err(|e| fail(ErrorCode::Unavailable, &format!("the space's directory: {e}")))?;
        set("WAYLAND_DISPLAY", &space.socket());
        set("XDG_RUNTIME_DIR", &space.dir);
        set("TMUX_TMPDIR", &space.dir);
        set("SCREENDIR", &screens);
        // Its own X display, never the person's.
        #[cfg(feature = "xwayland")]
        if let Some(x11) = &space.x11 {
            set("DISPLAY", &format!(":{}", x11.display));
        }
        if self.spaces.units.is_some() {
            set("DBUS_SESSION_BUS_ADDRESS", &space.session.address(&space.dir));
        }
        // #EDE-162: lavapipe alone in software; on the GPU its own driver and lavapipe.
        for (name, value) in self.space_vulkan_environment(space.software_rendering) {
            set(name, &value);
        }
        if profile == Profile::Space {
            let home = self.spaces.home_of(space);
            // gpg finds the PERSON's agent and keys by `HOME` otherwise: a
            // signed commit in a space would sign with their key.
            for (var, sub) in PROFILE.into_iter().chain([("GNUPGHOME", "gnupg")]) {
                let dir = home.join(sub);
                std::fs::DirBuilder::new().recursive(true).mode(0o700).create(&dir).map_err(|e| fail(ErrorCode::Unavailable, &format!("the space's home: {e}")))?;
                set(var, &dir);
            }
        }
        Ok(env)
    }

    /// `argv` as a command for a client of space `desk`, the one place
    /// that decides what a launch into a space gets: the compositor's
    /// environment without the person's session ([`DROPPED`]), the space's
    /// socket as `WAYLAND_DISPLAY`, the space's own directory as
    /// `XDG_RUNTIME_DIR` (so nothing found through the runtime directory
    /// is the person's: the bus, PipeWire, per-user sockets), a session
    /// bus of its own when `dbus-run-session` is installed and none
    /// otherwise, and its `profile`. The caller's `env` goes on top. With
    /// a user manager ([`Spaces::units`]) all of that starts in a scope of
    /// the space's slice.
    /// What a launch into space `desk` is refused for: no such space, an
    /// empty argv, a variable that is the compositor's to set, a program
    /// or a directory that is not there. Its working directory otherwise.
    pub(crate) fn space_launch_valid(&self, desk: DeskId, argv: &[String], env: &BTreeMap<String, String>, cwd: Option<&str>) -> Result<PathBuf, Failure> {
        let fail = |code, message: &str| Failure::new(code, message);
        self.spaces.get(desk).ok_or_else(|| fail(ErrorCode::NotFound, "no such space"))?;
        let Some(program) = argv.first() else { return Err(fail(ErrorCode::Invalid, "argv is empty")) };
        if let Some(name) = env.keys().find(|name| DROPPED.contains(&name.as_str()) || SET.contains(&name.as_str()) || super::render::SET.contains(&name.as_str()) || PROFILE.iter().any(|(var, _)| var == name)) {
            return Err(fail(ErrorCode::Invalid, &format!("{name} is the compositor's to set")));
        }
        if !found(program) {
            return Err(fail(ErrorCode::NotFound, &format!("no such program: {program}")));
        }
        let cwd = cwd.map(PathBuf::from).or_else(|| std::env::var_os("HOME").map(PathBuf::from)).unwrap_or_else(|| "/".into());
        if !cwd.is_dir() {
            return Err(fail(ErrorCode::NotFound, &format!("no such directory: {}", cwd.display())));
        }
        Ok(cwd)
    }

    pub fn space_command(&self, desk: DeskId, argv: &[String], env: &BTreeMap<String, String>, cwd: Option<&str>, profile: Profile) -> Result<Command, Failure> {
        let fail = |code, message: &str| Failure::new(code, message);
        let cwd = self.space_launch_valid(desk, argv, env, cwd)?;
        let space = self.spaces.get(desk).ok_or_else(|| fail(ErrorCode::NotFound, "no such space"))?;
        // A space with its own session has its bus; else one per launch
        // when `dbus-run-session` is there (no user manager, as before).
        let mut inner: Vec<String> = Vec::with_capacity(argv.len() + 2);
        if self.spaces.units.is_none() && found("dbus-run-session") {
            inner.extend(["dbus-run-session".to_string(), "--".to_string()]);
        }
        inner.extend(argv.iter().cloned());
        // #EDE-146..148: with the space guard installed nothing starts in a
        // space that the guard cannot guard: not without a user manager (no
        // slice to guard), not while its socket is gone (it is stopped or
        // removed). A guard that is there but says no stops the launch in
        // its scope (`space-enter`).
        if let Some(socket) = &self.spaces.guard.required {
            if self.spaces.units.is_none() {
                return Err(fail(ErrorCode::Unavailable, "the space guard needs the systemd user manager: nothing starts in a space without both"));
            }
            if !socket.exists() {
                return Err(fail(ErrorCode::Unavailable, &format!("the space guard is not running ({}): nothing starts in a space without it", socket.display())));
            }
        }
        // In a scope the caller's variables go in through the launch's
        // standard input, for the last of its stages alone.
        let (mut cmd, callers) = match &self.spaces.units {
            // Never outside a scope once launches run in scopes: the
            // person's login session is no place for a space's process.
            Some(units) if !units.reachable() => return Err(fail(ErrorCode::Unavailable, "the systemd user manager is gone: nothing starts in a space without it")),
            Some(units) => {
                let launch = self.spaces.launch_serial.get() + 1;
                self.spaces.launch_serial.set(launch);
                let callers = callers_environment(&space.dir, launch, env).map_err(|e| fail(ErrorCode::Unavailable, &format!("the launch's environment: {e}")))?;
                (units.command(&self.config.spaces, desk.number(), launch, &space.dir, &inner), Some(callers))
            }
            None => {
                let mut cmd = Command::new(&inner[0]);
                cmd.args(&inner[1..]);
                (cmd, None)
            }
        };
        cmd.env_clear().envs(self.space_environment(desk, profile)?);
        match callers {
            Some(callers) => cmd.stdin(callers),
            None => cmd.envs(env).stdin(Stdio::null()),
        };
        cmd.current_dir(cwd);
        own_group_dying_with_us(&mut cmd);
        Ok(cmd)
    }

    /// `launch` of a space connection: starts `argv` in its space `desk`.
    pub(crate) fn space_launch(&mut self, desk: DeskId, argv: Vec<String>, env: BTreeMap<String, String>, cwd: Option<String>) -> Result<Launched, Failure> {
        // What is refused is refused before anything starts.
        self.space_launch_valid(desk, &argv, &env, cwd.as_deref())?;
        // The space's Xwayland, at its first launch: the launch gets its display.
        #[cfg(feature = "xwayland")]
        self.space_x11(desk);
        let mut cmd = self.space_command(desk, &argv, &env, cwd.as_deref(), self.launch_profile(desk))?;
        self.space_launches_tick();
        match &self.spaces.units {
            None if self.spaces.launches.len() >= MAX_LAUNCHED => return Err(Failure::new(ErrorCode::Limit, &format!("{MAX_LAUNCHED} launched processes at most"))),
            Some(_) if self.spaces.launches.iter().filter(|launch| launch.desk == desk).count() >= self.config.spaces.launches as usize => {
                return Err(Failure::new(ErrorCode::Limit, &format!("{} launched processes in this space at most", self.config.spaces.launches)));
            }
            _ => {}
        }
        // The space's session: started at its first launch, the launch waits
        // for its bus in its own process.
        self.space_session(desk)?;
        let child = cmd.spawn().map_err(|e| Failure::new(if e.kind() == std::io::ErrorKind::NotFound { ErrorCode::NotFound } else { ErrorCode::Unavailable }, &format!("{}: {e}", argv[0])))?;
        let pid = child.id();
        let scoped = self.spaces.units.is_some();
        if let Some(space) = self.spaces.get_mut(desk).filter(|_| scoped) {
            space.sliced = true;
            self.spaces.sliced = true;
        }
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
        self.space_slices_tick();
        self.write_space_guard_state();
        let mut ended = Vec::new();
        self.spaces.launches.retain_mut(|launch| {
            if launch.closed.is_some_and(|at| at.elapsed() > TERM_GRACE) && !std::mem::replace(&mut launch.killed, true) {
                launch.signal(Signal::KILL);
            }
            match launch.child.try_wait() {
                Ok(None) => true,
                Ok(Some(status)) => {
                    ended.push((launch.pid(), status));
                    false
                }
                Err(_) => false,
            }
        });
        // #EDE-162: how a GPU space's launches ended (`frames::net`).
        for (pid, status) in ended {
            self.space_net_ended(pid, status);
        }
    }
}
