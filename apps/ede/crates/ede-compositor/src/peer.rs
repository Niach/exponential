//! #EDE-172 (P2): who is on the other end of a socket, taken when the
//! connection comes in (the control socket's and the person's Wayland
//! socket's), before its first request is served.
//!
//! A pid alone is no identity (it is reused) and an executable's name is
//! anyone's (a copy, a rename). A [`Peer`] holds the process itself (a
//! pidfd: `SO_PEERPIDFD`, else `SO_PEERCRED` + `pidfd_open` checked against
//! the process's start), the file it executed (`(st_dev, st_ino)` of
//! `/proc/<pid>/exe`) and whether it runs in the compositor's user
//! namespace. [`Installed`] names the root-owned binaries of eDE: a process
//! is one of them only when its executable IS that file (or that file
//! replaced by an upgrade while it ran), in this user namespace. A copy, a
//! rename, a bind mount of another file, or the genuine file run from a
//! child user namespace is not.
//!
//! What this does not stop (design 2): the genuine binary run with code
//! injected (`LD_PRELOAD`, a debugger as its parent). Peer identity names a
//! caller; it never carries a grant on its own.

use std::{
    os::{
        fd::{AsRawFd, BorrowedFd, FromRawFd, OwnedFd},
        unix::fs::MetadataExt,
    },
    path::{Path, PathBuf},
};

use smithay::reexports::rustix;

/// eDE's binaries whose processes the compositor tells apart.
pub const SEALED: &str = "exponential-sealed";
pub const SHELL: &str = "exponential-shell";
/// #EDE-172 (P5'): the portal backend and `exponential-ctl`.
pub const PORTAL: &str = "xdg-desktop-portal-exponential";
pub const CTL: &str = "exponential-ctl";

/// The file a process executed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Exe {
    pub dev: u64,
    pub ino: u64,
    /// `/proc/<pid>/exe` as it reads; a file replaced or removed while the
    /// process ran ends with ` (deleted)`.
    pub path: PathBuf,
    /// The file's owner and mode bits.
    pub uid: u32,
    pub mode: u32,
}

/// A connected process, as the compositor took it.
#[derive(Debug)]
pub struct Peer {
    pub pid: u32,
    pub uid: u32,
    /// That very process, for as long as anyone holds this; `None` on a
    /// kernel without pidfds (the identity below was then read by pid).
    pub pidfd: Option<OwnedFd>,
    /// Clock ticks after boot (`/proc/<pid>/stat`, field 22).
    pub start: u64,
    pub exe: Option<Exe>,
    /// It runs in the compositor's user namespace.
    pub same_userns: bool,
    /// Its parent's pid and executable path, for a prompt (#EDE-172 P5).
    pub parent: Option<(u32, Option<PathBuf>)>,
}

/// `SO_PEERPIDFD` (Linux 6.5); not in the libc crate everywhere.
const SO_PEERPIDFD: libc::c_int = 77;

impl Peer {
    /// The process that connected `socket` (its `SO_PEERCRED`), read now.
    /// `None` = no credentials, or the process was gone before it was read.
    pub fn of_socket(socket: BorrowedFd<'_>) -> Option<Peer> {
        let cred = rustix::net::sockopt::socket_peercred(socket).ok()?;
        let pid = cred.pid.as_raw_nonzero().get() as u32;
        let uid = cred.uid.as_raw();
        let pidfd = peer_pidfd(socket).or_else(|| open_pidfd(pid));
        Self::read(pid, uid, pidfd)
    }

    /// Process `pid`, read now (a test's child; a connection's peer goes
    /// through [`Peer::of_socket`]).
    pub fn of_pid(pid: u32) -> Option<Peer> {
        let uid = std::fs::metadata(format!("/proc/{pid}")).ok()?.uid();
        Self::read(pid, uid, open_pidfd(pid))
    }

    fn read(pid: u32, uid: u32, pidfd: Option<OwnedFd>) -> Option<Peer> {
        let (start, ppid) = stat_of(pid)?;
        let exe = exe_of(pid);
        let same_userns = same_ns(pid, "user");
        let parent = (ppid > 0).then(|| (ppid, std::fs::read_link(format!("/proc/{ppid}/exe")).ok()));
        let peer = Peer { pid, uid, pidfd, start, exe, same_userns, parent };
        // Read by pid: the process must still be the one the pidfd holds
        // (a pid is reused only after its process is gone).
        if peer.pidfd.as_ref().is_some_and(exited) || stat_of(pid).is_none_or(|(again, _)| again != start) {
            return None;
        }
        Some(peer)
    }

    /// The process is still running (its pidfd says so; without one, its
    /// pid has the same start).
    pub fn alive(&self) -> bool {
        match &self.pidfd {
            Some(fd) => !exited(fd),
            None => stat_of(self.pid).is_some_and(|(start, _)| start == self.start),
        }
    }

    /// Its executable's path without ` (deleted)`.
    pub fn exe_path(&self) -> Option<PathBuf> {
        let path = self.exe.as_ref()?.path.to_string_lossy().into_owned();
        Some(PathBuf::from(path.strip_suffix(" (deleted)").unwrap_or(&path)))
    }
}

/// `SO_PEERPIDFD`: the connecting process itself (Linux 6.5).
fn peer_pidfd(socket: BorrowedFd<'_>) -> Option<OwnedFd> {
    let mut fd: libc::c_int = -1;
    let mut len = std::mem::size_of::<libc::c_int>() as libc::socklen_t;
    // Safety: getsockopt writes at most `len` bytes into `fd`.
    let got = unsafe { libc::getsockopt(socket.as_raw_fd(), libc::SOL_SOCKET, SO_PEERPIDFD, (&mut fd as *mut libc::c_int).cast(), &mut len) };
    // Safety: a descriptor the kernel just made for this process.
    (got == 0 && fd >= 0).then(|| unsafe { OwnedFd::from_raw_fd(fd) })
}

/// `pidfd_open(pid)`; whether it is still the connecting process is
/// checked against the start read before and after ([`Peer::read`]).
fn open_pidfd(pid: u32) -> Option<OwnedFd> {
    let pid = rustix::process::Pid::from_raw(pid as i32)?;
    rustix::process::pidfd_open(pid, rustix::process::PidfdFlags::empty()).ok()
}

/// A pidfd is readable once its process exited.
fn exited(pidfd: &OwnedFd) -> bool {
    let mut fds = [rustix::event::PollFd::new(pidfd, rustix::event::PollFlags::IN)];
    rustix::event::poll(&mut fds, Some(&rustix::event::Timespec { tv_sec: 0, tv_nsec: 0 })).is_ok_and(|n| n > 0)
}

/// `(start, ppid)` from `/proc/<pid>/stat` (the fields after the command,
/// which may hold spaces and parentheses itself).
fn stat_of(pid: u32) -> Option<(u64, u32)> {
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    let rest = &stat[stat.rfind(')')? + 2..];
    let fields: Vec<&str> = rest.split(' ').collect();
    // `rest` starts at field 3 (state): ppid is field 4, starttime 22.
    Some((fields.get(19)?.parse().ok()?, fields.get(1)?.parse().ok()?))
}

fn exe_of(pid: u32) -> Option<Exe> {
    let link = format!("/proc/{pid}/exe");
    let meta = std::fs::metadata(&link).ok()?;
    let path = std::fs::read_link(&link).ok()?;
    Some(Exe { dev: meta.dev(), ino: meta.ino(), path, uid: meta.uid(), mode: meta.mode() })
}

/// Process `pid` is in this process's namespace of `kind`.
fn same_ns(pid: u32, kind: &str) -> bool {
    let ours = std::fs::metadata(format!("/proc/self/ns/{kind}"));
    let theirs = std::fs::metadata(format!("/proc/{pid}/ns/{kind}"));
    matches!((ours, theirs), (Ok(a), Ok(b)) if (a.dev(), a.ino()) == (b.dev(), b.ino()))
}

/// The root-owned binaries the compositor knows by file, per name. Each
/// path is looked at again at every check: a package upgrade replaces the
/// file, and the processes started from the new one are it too.
#[derive(Debug, Clone, Default)]
pub struct Installed {
    known: Vec<(String, PathBuf)>,
}

impl Installed {
    /// eDE's [`SEALED`], [`SHELL`], [`PORTAL`] and [`CTL`] where a package
    /// puts them: next to this compositor, in `/usr/bin` and in
    /// `/usr/libexec` ([`root_owned`] decides at each check).
    pub fn system() -> Installed {
        let mut dirs: Vec<PathBuf> = std::env::current_exe().ok().and_then(|p| Some(p.parent()?.to_path_buf())).into_iter().collect();
        dirs.push(PathBuf::from("/usr/bin"));
        dirs.push(PathBuf::from("/usr/libexec"));
        let mut paths = Vec::new();
        for name in [SEALED, SHELL, PORTAL, CTL] {
            for dir in &dirs {
                paths.push((name.to_string(), dir.join(name)));
            }
        }
        Installed::of(&paths)
    }

    /// `paths` (`(name, path)`), each by its real path.
    pub fn of(paths: &[(String, PathBuf)]) -> Installed {
        let mut known: Vec<(String, PathBuf)> = Vec::new();
        for (name, path) in paths {
            let real = std::fs::canonicalize(path).unwrap_or_else(|_| path.clone());
            if !known.iter().any(|(n, p)| n == name && *p == real) {
                known.push((name.clone(), real));
            }
        }
        Installed { known }
    }

    /// Is `peer` the installed binary `name`, in this user namespace? Its
    /// executable is the root-owned file at that path now, or (an upgrade
    /// replaced it while the process ran) a root-owned file that was there.
    pub fn is(&self, peer: &Peer, name: &str) -> bool {
        let Some(exe) = peer.exe.as_ref().filter(|_| peer.same_userns) else { return false };
        self.known.iter().filter(|(n, _)| n == name).any(|(_, path)| {
            root_owned(path).is_some_and(|id| id == (exe.dev, exe.ino)) || (exe.uid == 0 && exe.mode & 0o022 == 0 && exe.path == PathBuf::from(format!("{} (deleted)", path.display())))
        })
    }

    /// The paths that hold a root-owned file now.
    pub fn paths(&self) -> Vec<&Path> {
        self.known.iter().filter(|(_, p)| root_owned(p).is_some()).map(|(_, p)| p.as_path()).collect()
    }
}

/// `(st_dev, st_ino)` of `path` when it is a regular file that root owns,
/// under directories root owns, and nothing on the way is writable by the
/// group or others.
pub fn root_owned(path: &Path) -> Option<(u64, u64)> {
    let meta = std::fs::metadata(path).ok()?;
    if !meta.is_file() || meta.uid() != 0 || meta.mode() & 0o022 != 0 {
        return None;
    }
    let mut dir = path.parent();
    while let Some(d) = dir {
        let m = std::fs::metadata(d).ok()?;
        if m.uid() != 0 || m.mode() & 0o022 != 0 {
            return None;
        }
        dir = d.parent();
    }
    Some((meta.dev(), meta.ino()))
}
