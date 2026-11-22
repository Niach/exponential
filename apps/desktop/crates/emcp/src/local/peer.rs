//! #EDE-172 (P8): who is on the other end of a local socket, from the
//! kernel, never from anything the peer says. Linux: `SO_PEERCRED` (the pid
//! and uid at connect) and `SO_PEERPIDFD` (a pidfd of that very process,
//! Linux 6.5; else `pidfd_open` on the pid), so a peer that exits during
//! its admission is seen to. macOS: `LOCAL_PEERPID` and `getpeereid`.

use std::io;
use std::os::fd::RawFd;
#[cfg(target_os = "linux")]
use std::os::fd::{AsRawFd as _, FromRawFd as _, OwnedFd};

/// The process on the other end.
pub(crate) struct Peer {
    pub pid: u32,
    pub uid: u32,
    /// A handle on the peer that a reused pid cannot fool.
    #[cfg(target_os = "linux")]
    pidfd: Option<OwnedFd>,
}

impl Peer {
    /// Whether the process is still there (a zombie counts: its pid is not
    /// free until it is reaped). Without a pidfd the caller compares start
    /// times instead.
    pub(crate) fn alive(&self) -> bool {
        #[cfg(target_os = "linux")]
        if let Some(pidfd) = &self.pidfd {
            // SAFETY: signal 0 on our own pidfd checks existence only.
            return unsafe {
                libc::syscall(libc::SYS_pidfd_send_signal, pidfd.as_raw_fd(), 0, std::ptr::null::<libc::c_void>(), 0)
            } == 0;
        }
        true
    }
}

/// The peer of the connected socket `fd`.
#[cfg(target_os = "linux")]
pub(crate) fn of(fd: RawFd) -> io::Result<Peer> {
    let mut cred = libc::ucred { pid: 0, uid: 0, gid: 0 };
    let mut len = std::mem::size_of::<libc::ucred>() as libc::socklen_t;
    // SAFETY: `getsockopt` writes at most `len` bytes into `cred`.
    let got = unsafe { libc::getsockopt(fd, libc::SOL_SOCKET, libc::SO_PEERCRED, &mut cred as *mut _ as *mut libc::c_void, &mut len) };
    if got != 0 {
        return Err(io::Error::last_os_error());
    }
    if cred.pid <= 0 {
        return Err(io::Error::other("the peer has no process id here"));
    }
    let pid = cred.pid as u32;
    Ok(Peer { pid, uid: cred.uid, pidfd: peer_pidfd(fd).or_else(|| pidfd_open(pid)) })
}

/// `SO_PEERPIDFD`: a pidfd of the process that connected (Linux 6.5).
#[cfg(target_os = "linux")]
fn peer_pidfd(fd: RawFd) -> Option<OwnedFd> {
    let mut pidfd: libc::c_int = -1;
    let mut len = std::mem::size_of::<libc::c_int>() as libc::socklen_t;
    // SAFETY: as above; a returned descriptor is ours to own.
    let got = unsafe { libc::getsockopt(fd, libc::SOL_SOCKET, libc::SO_PEERPIDFD, &mut pidfd as *mut _ as *mut libc::c_void, &mut len) };
    (got == 0 && pidfd >= 0).then(|| unsafe { OwnedFd::from_raw_fd(pidfd) })
}

/// `pidfd_open(2)` (Linux 5.3).
#[cfg(target_os = "linux")]
fn pidfd_open(pid: u32) -> Option<OwnedFd> {
    // SAFETY: a plain syscall; a returned descriptor is ours to own.
    let fd = unsafe { libc::syscall(libc::SYS_pidfd_open, pid as libc::pid_t, 0) };
    (fd >= 0).then(|| unsafe { OwnedFd::from_raw_fd(fd as RawFd) })
}

/// The peer of the connected socket `fd`.
#[cfg(target_os = "macos")]
pub(crate) fn of(fd: RawFd) -> io::Result<Peer> {
    let mut pid: libc::pid_t = 0;
    let mut len = std::mem::size_of::<libc::pid_t>() as libc::socklen_t;
    // SAFETY: `getsockopt` writes at most `len` bytes into `pid`.
    let got = unsafe { libc::getsockopt(fd, libc::SOL_LOCAL, libc::LOCAL_PEERPID, &mut pid as *mut _ as *mut libc::c_void, &mut len) };
    if got != 0 {
        return Err(io::Error::last_os_error());
    }
    if pid <= 0 {
        return Err(io::Error::other("the peer has no process id"));
    }
    let (mut uid, mut gid) = (0, 0);
    // SAFETY: `getpeereid` fills the two ids.
    if unsafe { libc::getpeereid(fd, &mut uid, &mut gid) } != 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(Peer { pid: pid as u32, uid })
}

/// This process's effective uid.
pub(crate) fn own_uid() -> u32 {
    // SAFETY: `geteuid` cannot fail.
    unsafe { libc::geteuid() }
}

/// What `pid` runs, for the log (best effort).
pub(crate) fn exe_of(pid: u32) -> String {
    #[cfg(target_os = "linux")]
    {
        std::fs::read_link(format!("/proc/{pid}/exe")).map(|path| path.display().to_string()).unwrap_or_else(|_| "?".to_string())
    }
    #[cfg(target_os = "macos")]
    {
        let mut buf = vec![0u8; libc::PROC_PIDPATHINFO_MAXSIZE as usize];
        // SAFETY: `proc_pidpath` writes at most `buf.len()` bytes.
        let len = unsafe { libc::proc_pidpath(pid as libc::c_int, buf.as_mut_ptr() as *mut libc::c_void, buf.len() as u32) };
        if len <= 0 {
            return "?".to_string();
        }
        String::from_utf8_lossy(&buf[..len as usize]).into_owned()
    }
}
