//! #EDE-172 (P4): the compositor starts the session's own programs itself
//! (`[session] helpers = "compositor"`; dark until eDE's flip, P14):
//!
//! - **The list** ([`session_specs`]): the shell under its supervisor
//!   (`exponential-ui supervise -- exponential-shell`), the polkit agent
//!   (`exponential-sealed polkit`), the portal and the services daemon
//!   (`exponential-services serve`, its intake at fd 3: [`crate::services`]).
//! - **Each in a scope of its own** (`systemd-run --user --scope
//!   --slice=session.slice`, part of the session's targets as the units
//!   were) where the person's user manager answers, else in the
//!   compositor's cgroup. Their systemd units stay installed, conditioned
//!   off by `EDE_HELPERS_SPAWNED`, which every process the compositor
//!   starts gets and `exponential-session --ready` exports.
//! - **Their way in is inherited, never read**: fd 3 is a channel the
//!   compositor made (`EDE_CONTROL_FD=3`, [`ede_protocol::Client::inherited`]),
//!   fd 4 a Wayland connection it made (`WAYLAND_SOCKET=4`), so it knows
//!   that client by construction ([`Kind`]), not by its executable. The
//!   supervisor asks for a Wayland connection per shell it starts
//!   ([`ede_protocol::Request::OpenWayland`]) and hands each shell a channel
//!   of its own.
//! - **A pinned environment** ([`pinned_env`]): what loads code into a
//!   process from a path the person's other programs can write (the
//!   dynamic loader's variables, Vulkan's layers, PipeWire's modules) does
//!   not reach a helper.
//! - **Started again** when one ends, as the lock client is: quick deaths
//!   count up to [`MAX_RESPAWNS`], then the next start waits (5 s,
//!   doubling to a minute); one that lived [`HEALTHY`] counts anew.
//!
//! Each helper refuses ptrace itself once its first roundtrips are done
//! ([`ede_protocol::process::refuse_ptrace`]), and the compositor does at
//! its start.

use std::{
    ffi::OsString,
    io::Write,
    os::{fd::OwnedFd, unix::net::UnixStream},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{atomic::AtomicU32, Arc},
    time::{Duration, Instant},
};

use ede_protocol::{Capability, Token};

use crate::{
    sealed::{HEALTHY, MAX_RESPAWNS},
    state::{ClientState, Ede},
};

/// What a helper's compositor-made Wayland client is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// The shell: its pinned space view and its Agent spaces list are black
    /// in captures of the person's output.
    Shell,
    /// `exponential-sealed` (the polkit agent, the lock): its surfaces are
    /// protected.
    Sealed,
}

/// The control channel a helper starts with at fd 3.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Channel {
    /// The root grant (the shell's supervisor).
    Root,
    /// A bound child of root for `session`, holding `capabilities` (none for
    /// the polkit agent: `space_of_process` and the outputs take any grant).
    Child { session: String, capabilities: Vec<Capability> },
    /// #EDE-172 (P13): no control channel but the services daemon's intake
    /// ([`crate::services`], [`ede_protocol::intake`]).
    Intake,
}

/// How a helper gets its Wayland connection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Wayland {
    /// At fd 4 when it starts.
    AtStart(Kind),
    /// One per program it starts, asked for on its channel (the supervisor,
    /// for each shell).
    Asked(Kind),
}

/// One program the compositor runs for the session.
#[derive(Debug, Clone)]
pub struct Spec {
    /// The name its scope and its log go by (its unit's name).
    pub name: String,
    pub argv: Vec<String>,
    pub channel: Option<Channel>,
    pub wayland: Option<Wayland>,
    /// Set on top of the pinned environment (what its unit set).
    pub env: Vec<(String, String)>,
}

/// What every helper's environment leaves out: the dynamic loader's
/// variables, the GL and GBM driver paths, PipeWire's module paths (a
/// library from a directory of the person's, in a process that holds
/// their desk), and what the compositor sets itself.
pub const DROPPED: [&str; 20] = [
    "LD_PRELOAD",
    "LD_AUDIT",
    "LD_LIBRARY_PATH",
    "GCONV_PATH",
    "__EGL_VENDOR_LIBRARY_DIRS",
    "__EGL_VENDOR_LIBRARY_FILENAMES",
    "LIBGL_DRIVERS_PATH",
    "GBM_BACKENDS_PATH",
    "SPA_PLUGIN_DIR",
    "PIPEWIRE_MODULE_DIR",
    "PIPEWIRE_CONFIG_DIR",
    "WAYLAND_SOCKET",
    "EDE_CONTROL_FD",
    "EDE_TOKEN",
    "EDE_TOKEN_FD",
    "WAYLAND_DISPLAY",
    "DISPLAY",
    "XAUTHORITY",
    "XDG_ACTIVATION_TOKEN",
    "DESKTOP_STARTUP_ID",
];

/// Every Vulkan loader override goes too (its drivers, layers, their paths).
pub const DROPPED_PREFIX: &str = "VK_";

/// What every helper's environment gets instead: no implicit Vulkan layer
/// but Mesa's own device_select (gpui draws on Vulkan, and on a machine
/// with two GPUs that layer picks one), and only from the system's layer
/// directories.
pub const PINNED: [(&str, &str); 3] = [
    ("VK_LOADER_LAYERS_DISABLE", "~implicit~"),
    ("VK_LOADER_LAYERS_ALLOW", "VK_LAYER_MESA_device_select"),
    ("VK_IMPLICIT_LAYER_PATH", "/usr/local/share/vulkan/implicit_layer.d:/usr/share/vulkan/implicit_layer.d:/etc/vulkan/implicit_layer.d"),
];

/// The system's PipeWire client configuration: the portal's (its streams),
/// none of the person's drop-ins and their modules.
pub const PIPEWIRE_SYSTEM: &str = "/usr/share/pipewire";

/// What the compositor sets itself, never the person's to get back.
const OURS: [&str; 9] = ["WAYLAND_SOCKET", "EDE_CONTROL_FD", "EDE_TOKEN", "EDE_TOKEN_FD", "WAYLAND_DISPLAY", "DISPLAY", "XAUTHORITY", "XDG_ACTIVATION_TOKEN", "DESKTOP_STARTUP_ID"];

fn dropped(name: &str) -> bool {
    DROPPED.contains(&name) || name.starts_with(DROPPED_PREFIX)
}

/// `base` as a helper gets it: [`DROPPED`] and every `VK_` variable out,
/// [`PINNED`] in, and what was dropped (but the compositor's own) saved in
/// [`ede_protocol::process::PERSON_ENV`] for the apps the shell launches
/// as the person ([`ede_protocol::process::as_the_person`]).
pub fn pinned_env(base: impl IntoIterator<Item = (OsString, OsString)>) -> Vec<(OsString, OsString)> {
    let (mut env, out): (Vec<(OsString, OsString)>, Vec<(OsString, OsString)>) = base.into_iter().filter(|(k, _)| k != ede_protocol::process::PERSON_ENV).partition(|(k, _)| !dropped(&k.to_string_lossy()));
    let person: std::collections::BTreeMap<String, String> = out.into_iter().filter(|(k, _)| !OURS.contains(&k.to_string_lossy().as_ref())).map(|(k, v)| (k.to_string_lossy().into_owned(), v.to_string_lossy().into_owned())).collect();
    env.extend(PINNED.iter().map(|(k, v)| (OsString::from(k), OsString::from(v))));
    env.push((ede_protocol::process::PERSON_ENV.into(), serde_json::to_string(&person).unwrap_or_default().into()));
    env
}

/// Where eDE's programs are: next to this compositor, else `/usr/bin`
/// (`/usr/libexec` for the portal), else `PATH`.
fn installed(name: &str, libexec: bool) -> String {
    let here = std::env::current_exe().ok().and_then(|p| Some(p.parent()?.to_path_buf()));
    let mut dirs: Vec<PathBuf> = here.iter().cloned().collect();
    if libexec {
        dirs.extend(here.iter().filter_map(|d| Some(d.parent()?.join("libexec"))));
        dirs.push(PathBuf::from("/usr/libexec"));
    }
    dirs.push(PathBuf::from("/usr/bin"));
    dirs.iter().map(|d| d.join(name)).find(|p| p.is_file()).map(|p| p.to_string_lossy().into_owned()).unwrap_or_else(|| name.to_string())
}

/// The session's helpers, as their units ran them.
pub fn session_specs() -> Vec<Spec> {
    let mut portal_env = Vec::new();
    if Path::new(PIPEWIRE_SYSTEM).join("client.conf").is_file() {
        portal_env.push(("PIPEWIRE_CONFIG_DIR".to_string(), PIPEWIRE_SYSTEM.to_string()));
    }
    vec![
        Spec {
            name: "exponential-shell".into(),
            argv: vec![installed("exponential-ui", false), "supervise".into(), "--".into(), installed("exponential-shell", false)],
            channel: Some(Channel::Root),
            wayland: Some(Wayland::Asked(Kind::Shell)),
            env: vec![("RUST_LOG".into(), "info,wgpu_core=warn,wgpu_hal=warn".into())],
        },
        Spec {
            name: "exponential-polkit-agent".into(),
            argv: vec![installed("exponential-sealed", false), "polkit".into()],
            channel: Some(Channel::Child { session: "exponential-polkit-agent".into(), capabilities: vec![] }),
            wayland: Some(Wayland::AtStart(Kind::Sealed)),
            env: vec![],
        },
        // #EDE-172 P7 hands the portal its channel ({capture, input:person,
        // ui}); until then it mints from the token file, as before.
        Spec { name: "xdg-desktop-portal-exponential".into(), argv: vec![installed("xdg-desktop-portal-exponential", true)], channel: None, wayland: None, env: portal_env },
        // #EDE-172 (P13): no services.sock; connections come over its intake.
        Spec { name: "exponential-services".into(), argv: vec![installed("exponential-services", false), "serve".into()], channel: Some(Channel::Intake), wayland: None, env: vec![("RUST_LOG".into(), "info".into())] },
    ]
}

/// A running (or waiting) helper.
#[derive(Debug)]
pub struct Running {
    pub spec: Spec,
    pub child: Option<Child>,
    /// Its channel's connection while it runs.
    pub conn: Option<u64>,
    /// The bound child grant its channel holds (none for root's).
    pub grant: Option<Token>,
    /// Its scope while it runs in one.
    pub unit: Option<String>,
    pub started: Instant,
    pub respawns: u32,
    /// A helper that keeps dying is not started again before this.
    pub retry_at: Instant,
    /// How many times it was started: in its scope's name.
    pub starts: u32,
}

/// The compositor's helpers ([`Ede::helpers`]).
#[derive(Debug, Default)]
pub struct Helpers {
    /// `[session] helpers = "compositor"` (or a test's [`Ede::start_helpers_with`]).
    pub on: bool,
    pub running: Vec<Running>,
    /// Their output goes to the journal under their names (the session);
    /// a test's go where the test's go.
    pub journal: bool,
    /// What their environment is made from; `None` = this process's.
    pub base_env: Option<Vec<(OsString, OsString)>>,
}

impl Helpers {
    pub fn new(on: bool) -> Self {
        Self { on, running: Vec::new(), journal: false, base_env: None }
    }

    /// The pid of the helper `name` while it runs.
    pub fn pid(&self, name: &str) -> Option<u32> {
        self.running.iter().find(|r| r.spec.name == name).and_then(|r| r.child.as_ref()).map(Child::id)
    }
}

impl Drop for Helpers {
    /// The helpers end with the compositor (their parent-death signal says
    /// so too): asked first, then killed.
    fn drop(&mut self) {
        let mut children: Vec<Child> = self.running.iter_mut().filter_map(|r| r.child.take()).collect();
        for child in &children {
            // Safety: signalling our own child by pid.
            unsafe { libc::kill(child.id() as libc::pid_t, libc::SIGTERM) };
        }
        let deadline = Instant::now() + Duration::from_millis(500);
        while Instant::now() < deadline && children.iter_mut().any(|c| c.try_wait().ok().flatten().is_none()) {
            std::thread::sleep(Duration::from_millis(10));
        }
        for mut child in children {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

/// A stream of the journal's for `identifier`'s output (what systemd gives
/// a unit's stdout); `None` = no journal here.
fn journal_stream(identifier: &str) -> Option<OwnedFd> {
    let stream = UnixStream::connect("/run/systemd/journal/stdout").ok()?;
    // identifier, unit, priority (info), level prefix, syslog, kmsg, console.
    (&stream).write_all(format!("{identifier}\n\n6\n1\n0\n0\n0\n").as_bytes()).ok()?;
    let _ = stream.shutdown(std::net::Shutdown::Read);
    Some(stream.into())
}

impl Ede {
    /// Starts the session's helpers when `[session] helpers = "compositor"`;
    /// once, before the autostart commands.
    pub fn start_helpers(&mut self) {
        if !self.helpers.on || !self.helpers.running.is_empty() {
            return;
        }
        self.helpers.journal = true;
        self.start_helpers_with(session_specs());
    }

    /// Starts `specs` as the compositor's helpers (a test's own list).
    pub fn start_helpers_with(&mut self, specs: Vec<Spec>) {
        self.helpers.on = true;
        for spec in specs {
            tracing::info!(helper = spec.name, "the compositor starts {}", spec.argv.join(" "));
            let now = Instant::now();
            self.helpers.running.push(Running { spec, child: None, conn: None, grant: None, unit: None, started: now, respawns: 0, retry_at: now, starts: 0 });
            let i = self.helpers.running.len() - 1;
            self.start_helper(i);
        }
    }

    /// A Wayland client of the person's desk that the compositor made for
    /// a helper of `kind` (#EDE-172 P4): the client's end, and the end for
    /// the helper's `WAYLAND_SOCKET`.
    pub fn helper_wayland_client(&mut self, kind: Kind) -> Option<(smithay::reexports::wayland_server::Client, OwnedFd)> {
        let (ours, theirs) = UnixStream::pair().map_err(|e| tracing::warn!("a helper's Wayland connection: {e}")).ok()?;
        let state = ClientState { helper: Some(kind), helper_pid: AtomicU32::new(0), ..ClientState::default() };
        let client = self.display_handle.insert_client(ours, Arc::new(state)).map_err(|e| tracing::warn!("a helper's Wayland connection: {e}")).ok()?;
        Some((client, theirs.into()))
    }

    /// The command for helper `argv`, `name`'s: in a scope of its own when
    /// the person's user manager answers (`scoped`), with the pinned
    /// environment, its output in the journal, and the compositor's death
    /// its end.
    pub fn helper_command(&self, name: &str, argv: &[String], env: &[(String, String)], unit: Option<&str>) -> Option<Command> {
        let (program, args) = argv.split_first()?;
        let mut cmd = match (unit, self.spaces.units.as_ref()) {
            (Some(unit), Some(units)) => {
                let mut cmd = Command::new(&units.systemd_run);
                cmd.args(["--user", "--scope", "--collect", "--quiet", "--slice=session.slice", &format!("--unit={unit}")]);
                // Part of the session as the units were: it ends with it.
                cmd.args(["-p", "PartOf=graphical-session.target", "-p", "PartOf=exponential-session.target"]);
                if units.version >= 254 {
                    cmd.arg("--expand-environment=no");
                }
                cmd.arg("--").arg(program).args(args);
                cmd
            }
            _ => {
                let mut cmd = Command::new(program);
                cmd.args(args);
                cmd
            }
        };
        let base = self.helpers.base_env.clone().unwrap_or_else(|| std::env::vars_os().collect());
        cmd.env_clear().envs(pinned_env(base));
        // The displays, as every client of ours gets them.
        if let Some(socket) = &self.socket_name {
            cmd.env("WAYLAND_DISPLAY", socket);
        }
        #[cfg(feature = "xwayland")]
        if let Some(n) = self.xdisplay {
            cmd.env("DISPLAY", format!(":{n}"));
            if let Some(xauthority) = self.xauthority(crate::desk::DeskId::PERSON) {
                cmd.env("XAUTHORITY", xauthority);
            }
        }
        cmd.env(ede_protocol::process::HELPERS_ENV, "1");
        for (k, v) in env {
            cmd.env(k, v);
        }
        cmd.stdin(Stdio::null());
        match self.helpers.journal.then(|| journal_stream(name)).flatten() {
            Some(out) => match out.try_clone() {
                Ok(err) => {
                    cmd.stdout(out).stderr(err);
                }
                Err(_) => {
                    cmd.stdout(out);
                }
            },
            None => {}
        }
        use std::os::unix::process::CommandExt;
        // Safety: prctl only, between fork and exec. It holds across exec.
        unsafe {
            cmd.pre_exec(|| {
                if libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGTERM) != 0 {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            });
        }
        Some(cmd)
    }

    /// Starts helper `i`: its channel, its Wayland connection, its process.
    /// One that cannot start is tried again on the next tick.
    fn start_helper(&mut self, i: usize) {
        let spec = self.helpers.running[i].spec.clone();
        self.helpers.running[i].starts += 1;
        let starts = self.helpers.running[i].starts;
        // A scope the user manager refuses must not keep the panel away:
        // after a few quick deaths in a row it starts in the compositor's cgroup.
        let unit = (self.helpers.running[i].respawns < 3).then(|| format!("{}-p{}-{starts}.scope", spec.name, std::process::id()));
        let channel = match &spec.channel {
            // #EDE-172 (P13): the services daemon's intake, at fd 3 too.
            Some(Channel::Intake) => match self.services_intake() {
                Some(fd) => Some((None, fd, None)),
                None => {
                    tracing::error!(helper = spec.name, "its intake could not be made; trying again");
                    return;
                }
            },
            Some(channel) => match self.ipc_helper_channel(channel) {
                Some((conn, fd, grant)) => Some((Some(conn), fd, grant)),
                None => {
                    tracing::error!(helper = spec.name, "its channel could not be made; trying again");
                    return;
                }
            },
            None => None,
        };
        let wayland = match spec.wayland {
            Some(Wayland::AtStart(kind)) => self.helper_wayland_client(kind),
            _ => None,
        };
        let Some(mut cmd) = self.helper_command(&spec.name, &spec.argv, &spec.env, unit.as_deref().filter(|_| self.spaces.units.is_some())) else { return };
        let (conn, fd, grant) = match channel {
            Some((conn, fd, grant)) => (conn, Some(fd), grant),
            None => (None, None, None),
        };
        let (client, wayland_fd) = match wayland {
            Some((client, fd)) => (Some(client), Some(fd)),
            None => (None, None),
        };
        let started = ede_protocol::process::hand_over(&mut cmd, fd, wayland_fd).and_then(|()| cmd.spawn());
        drop(cmd);
        let r = &mut self.helpers.running[i];
        match started {
            Ok(child) => {
                tracing::info!(helper = spec.name, pid = child.id(), unit = unit.as_deref().filter(|_| self.spaces.units.is_some()).unwrap_or("none"), "helper started");
                if let Some(state) = client.as_ref().and_then(|c| c.get_data::<ClientState>()) {
                    state.helper_pid.store(child.id(), std::sync::atomic::Ordering::Relaxed);
                }
                r.child = Some(child);
                r.conn = conn;
                r.grant = grant;
                r.unit = unit.filter(|_| self.spaces.units.is_some());
                r.started = Instant::now();
            }
            Err(e) => {
                tracing::error!(helper = spec.name, "{}: {e}", spec.argv[0]);
                if let Some(client) = client {
                    self.display_handle.backend_handle().kill_client(client.id(), smithay::reexports::wayland_server::backend::DisconnectReason::ConnectionClosed);
                }
                if let Some(conn) = conn {
                    self.ipc_end_channel(conn, grant.as_ref());
                }
                let r = &mut self.helpers.running[i];
                r.respawns += 1;
                r.started = Instant::now();
            }
        }
    }

    /// Once a second: a helper that ended is started again (its channel
    /// and what it held go first), at once unless it keeps dying.
    pub fn helpers_tick(&mut self) {
        if !self.helpers.on {
            return;
        }
        for i in 0..self.helpers.running.len() {
            let r = &mut self.helpers.running[i];
            if let Some(child) = r.child.as_mut() {
                let status = match child.try_wait() {
                    Ok(None) => continue,
                    Ok(Some(status)) => status.to_string(),
                    Err(e) => e.to_string(),
                };
                tracing::error!(helper = r.spec.name, %status, "a helper of the session ended; starting it again");
                r.child = None;
                if r.started.elapsed() >= HEALTHY {
                    r.respawns = 0;
                }
                r.respawns += 1;
                if r.respawns > MAX_RESPAWNS {
                    let pause = Duration::from_secs(60.min(5 << (r.respawns - MAX_RESPAWNS - 1).min(4)));
                    tracing::error!(helper = r.spec.name, ?pause, "a helper keeps dying");
                    r.retry_at = Instant::now() + pause;
                }
                let (conn, grant) = (r.conn.take(), r.grant.take());
                r.unit = None;
                if let Some(conn) = conn {
                    self.ipc_end_channel(conn, grant.as_ref());
                }
            }
            let r = &self.helpers.running[i];
            if r.child.is_some() || (r.respawns > MAX_RESPAWNS && Instant::now() < r.retry_at) {
                continue;
            }
            self.start_helper(i);
        }
    }

    /// `open_wayland` on connection `conn` (#EDE-172 P4): a Wayland
    /// connection for the program the helper on that channel starts next;
    /// `None` = it is no helper's channel that asks for one.
    pub fn helper_wayland_for(&mut self, conn: u64) -> Option<OwnedFd> {
        let kind = self.helpers.running.iter().find(|r| r.conn == Some(conn) && r.child.is_some()).and_then(|r| match r.spec.wayland {
            Some(Wayland::Asked(kind)) => Some(kind),
            _ => None,
        })?;
        self.helper_wayland_client(kind).map(|(_, fd)| fd)
    }
}
