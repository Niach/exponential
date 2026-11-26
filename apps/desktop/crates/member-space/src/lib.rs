//! #EXP-2161 (`packages/domain-contract/fixtures/member-space.json`,
//! ISOLATION): the closed-off environment one member's space daemon runs in.
//!
//! Direct syscalls, no runtime dependency. The host daemon starts this very
//! binary as a LAUNCHER ([`LAUNCHER_ARG`], a clean environment, the [`Spec`]
//! as the first line of its stdin). The launcher forks a child that enters
//! its own user, mount, PID, network, IPC, UTS and cgroup namespaces and
//! builds a private root by ALLOWLIST:
//!
//! - read-only: `/usr`, `/bin`, `/sbin`, `/lib*`, `/etc`, `/nix`, and each
//!   tool of the spec under `/exp/bin`, the space's `host/` dir at `/exp/host`;
//! - read-write: the space's own `data/` at `/space` (its HOME);
//! - private: `/tmp`, `/var/tmp`, `/run`, `/dev` (null, zero, full, random,
//!   urandom, tty, a fresh devpts and shm) and `/proc` (its own PID namespace);
//! - nothing else exists: no `/home`, `/root`, `/run/user`, `/var`, `/opt`,
//!   `/srv`, `/mnt`, `/sys`; so no host data dir, no other space, no
//!   `docker.sock`, no session bus, no SSH agent.
//!
//! Inside: no capabilities, `no_new_privs`, no new namespaces, a seccomp
//! filter over the kernel's rarely needed corners, no inherited descriptor
//! but the two output pipes; PID 1 is this binary EXEC'd from the space's own
//! read-only copy and not dumpable, so no process there names the host's
//! binary or the host's control pipe. The launcher and its child stay
//! outside; the child ends the space when the launcher is told to, is gone,
//! or has another parent than the host daemon.
//!
//! THE NETWORK is the space's own network namespace, so no service on the
//! host's loopback and no abstract socket of the host exists there at all.
//! Its one device is a TUN the launcher terminates in user space
//! ([`net`]): outbound connections to public addresses only, plus the one
//! address of the Exponential instance; never the host itself, its LAN,
//! link-local or metadata addresses ([`net::allowed`]).
//!
//! Every step fails closed: a machine where one of them does not work has no
//! spaces ([`probe`]), and nothing is ever started without all of them.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[cfg(target_os = "linux")]
mod linux;
pub mod net;

/// The launcher's argv[1].
pub const LAUNCHER_ARG: &str = "__member-space";
/// The init of a space (PID 1), inside.
pub const INIT_ARG: &str = "__member-space-init";
/// Run by [`probe`] inside a throwaway space.
pub const PROBE_ARG: &str = "__member-space-probe";

/// Where things are inside a space.
pub const SPACE_HOME: &str = "/space";
pub const HOST_DIR: &str = "/exp/host";
pub const TOOLS_DIR: &str = "/exp/bin";
/// The program of a space (this binary).
pub const PROGRAM: &str = "/exp/bin/exponential";

/// What a launcher starts. `data` and `host` are the two halves of a space's
/// directory: the member's code writes `data` and only ever reads `host`
/// (the host daemon writes there, so no symlink of the member's is followed
/// by the host).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Spec {
    pub data: PathBuf,
    pub host: PathBuf,
    /// Host paths of single-file programs, read-only at `/exp/bin/<name>`.
    pub tools: Vec<(String, PathBuf)>,
    /// argv of [`PROGRAM`] (which must be one of `tools`).
    pub args: Vec<String>,
    pub env: Vec<(String, String)>,
    /// The instance's host and port: reachable whatever its address.
    pub instance: Option<(String, u16)>,
    /// Nothing else is reachable but these and the instance, resolved by the
    /// host when the space starts (its last save); None = public addresses.
    #[serde(default)]
    pub only: Option<Vec<(String, u16)>>,
    /// How long SIGTERM has before SIGKILL.
    pub grace_secs: u64,
}

/// The environment a space starts with: nothing is inherited but the
/// locale and the time zone, and only well-formed values of them.
pub fn environment(host: impl Iterator<Item = (String, String)>) -> Vec<(String, String)> {
    let mut env: Vec<(String, String)> = [
        ("PATH", "/exp/bin:/usr/local/bin:/usr/bin:/bin:/usr/local/sbin:/usr/sbin:/sbin"),
        ("HOME", SPACE_HOME),
        ("USER", "space"),
        ("LOGNAME", "space"),
        ("SHELL", "/bin/sh"),
        ("TERM", "xterm-256color"),
        ("TMPDIR", "/tmp"),
        ("EXP_DATA_DIR", "/space/exponential"),
        ("XDG_CONFIG_HOME", "/space/.config"),
        ("XDG_DATA_HOME", "/space/.local/share"),
        ("XDG_CACHE_HOME", "/space/.cache"),
        ("XDG_STATE_HOME", "/space/.local/state"),
    ]
    .into_iter()
    .map(|(key, value)| (key.to_string(), value.to_string()))
    .collect();
    const INHERITED: [&str; 5] = ["LANG", "LC_ALL", "LC_CTYPE", "LANGUAGE", "TZ"];
    for (key, value) in host {
        let plain = value.len() <= 64 && value.bytes().all(|b| b.is_ascii_alphanumeric() || b"._-:@/+".contains(&b));
        if INHERITED.contains(&key.as_str()) && plain {
            env.push((key, value));
        }
    }
    env
}

/// Whether a tool can be shown in a space: one file the kernel runs itself
/// (an ELF binary). A script's package tree is not, it would have to be
/// mounted whole.
pub fn bindable(path: &Path) -> bool {
    use std::io::Read as _;
    let mut magic = [0u8; 4];
    std::fs::File::open(path).and_then(|mut file| file.read_exact(&mut magic)).is_ok() && magic == *b"\x7fELF"
}

/// Why this machine cannot run spaces at all (asked before anything is
/// started). `None` = worth probing.
pub fn unsupported() -> Option<&'static str> {
    #[cfg(not(target_os = "linux"))]
    {
        Some("Linux servers only")
    }
    #[cfg(target_os = "linux")]
    {
        linux::unsupported()
    }
}

/// Starts a real space in a throwaway directory and has it check, from the
/// inside, that it is closed off. `exe` = this binary. BLOCKING (seconds).
pub fn probe(exe: &Path) -> Result<(), String> {
    if let Some(reason) = unsupported() {
        return Err(reason.to_string());
    }
    #[cfg(target_os = "linux")]
    {
        linux::probe(exe)
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = exe;
        Err("Linux servers only".into())
    }
}

/// Deletes a space's directory, whatever its member left in it: links are
/// never followed, closed modes and any depth are no obstacle. Only once
/// [`quiet`] holds its lock.
pub fn remove_tree(dir: &Path) -> std::io::Result<()> {
    #[cfg(target_os = "linux")]
    {
        linux::remove_tree(dir)
    }
    #[cfg(not(target_os = "linux"))]
    {
        std::fs::remove_dir_all(dir)
    }
}

/// The lock of a space that runs nowhere (`host` = its `host/` directory):
/// held, no launcher of it lives and none can start. Err = it is alive.
#[cfg(target_os = "linux")]
pub fn quiet(host: &Path) -> Result<std::os::fd::OwnedFd, String> {
    linux::exclusive(host)
}

/// No space runs on another OS.
#[cfg(not(target_os = "linux"))]
pub fn quiet(_host: &Path) -> Result<(), String> {
    Ok(())
}

/// Runs the launcher or the probe when this process was started as one
/// (first thing in `main`, before any thread). Never returns then.
pub fn run_if_requested() {
    let arg = std::env::args().nth(1);
    match arg.as_deref() {
        Some(LAUNCHER_ARG) => {
            #[cfg(target_os = "linux")]
            linux::launcher_main();
            #[cfg(not(target_os = "linux"))]
            std::process::exit(1);
        }
        Some(INIT_ARG) => {
            #[cfg(target_os = "linux")]
            linux::init_main();
            #[cfg(not(target_os = "linux"))]
            std::process::exit(1);
        }
        Some(PROBE_ARG) => {
            #[cfg(target_os = "linux")]
            linux::probe_main();
            #[cfg(not(target_os = "linux"))]
            std::process::exit(1);
        }
        _ => {}
    }
}

/// A command that starts `spec` through the launcher: a clean environment,
/// its own process group (a Ctrl-C at the host's terminal reaches the host,
/// which stops its spaces), stdin for the spec line and the stop line. The
/// caller writes [`spec_line`], keeps stdin open while the space runs, and
/// closes it (or writes `stop`) to end it; the host's death closes it too.
#[cfg(unix)]
pub fn command(exe: &Path) -> std::process::Command {
    use std::os::unix::process::CommandExt as _;
    use std::process::Stdio;
    let mut command = std::process::Command::new(exe);
    command
        .arg(LAUNCHER_ARG)
        .env_clear()
        .current_dir("/")
        .process_group(0)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    command
}

pub fn spec_line(spec: &Spec) -> String {
    format!("{}\n", serde_json::to_string(spec).unwrap_or_default())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_environment_is_an_allowlist() {
        let host = [
            ("AWS_SECRET_ACCESS_KEY", "x"),
            ("SSH_AUTH_SOCK", "/run/user/1000/ssh"),
            ("DBUS_SESSION_BUS_ADDRESS", "unix:path=/run/user/1000/bus"),
            ("EXP_TOKEN", "expu_x"),
            ("ANTHROPIC_API_KEY", "sk-x"),
            ("PATH", "/home/me/bin"),
            ("HOME", "/home/me"),
            ("LD_PRELOAD", "/home/me/x.so"),
            ("LANG", "de_DE.UTF-8"),
            ("TZ", "Europe/Berlin"),
            ("LC_ALL", "a value with spaces"),
        ];
        let env = environment(host.into_iter().map(|(k, v)| (k.to_string(), v.to_string())));
        let keys: Vec<&str> = env.iter().map(|(k, _)| k.as_str()).collect();
        assert_eq!(
            keys,
            [
                "PATH", "HOME", "USER", "LOGNAME", "SHELL", "TERM", "TMPDIR", "EXP_DATA_DIR", "XDG_CONFIG_HOME",
                "XDG_DATA_HOME", "XDG_CACHE_HOME", "XDG_STATE_HOME", "LANG", "TZ"
            ]
        );
        assert!(env.iter().all(|(_, v)| !v.contains("/home/")));
    }

    #[test]
    fn only_a_single_binary_is_bindable() {
        assert!(!bindable(Path::new("/etc/passwd")));
        assert!(!bindable(Path::new("/nonexistent")));
        #[cfg(target_os = "linux")]
        assert!(bindable(&std::env::current_exe().unwrap()));
    }
}
