//! The kernel calls: `bpf(2)` (load, attach, query, detach), a cgroup
//! directory opened without following a link, a socket peer's credentials.

use std::{
    ffi::CString,
    io,
    os::fd::{AsRawFd, BorrowedFd, FromRawFd, OwnedFd},
};

pub const BPF_PROG_LOAD: i32 = 5;
pub const BPF_PROG_ATTACH: i32 = 8;
pub const BPF_PROG_DETACH: i32 = 9;
pub const BPF_PROG_QUERY: i32 = 16;
pub const BPF_PROG_TYPE_CGROUP_DEVICE: u32 = 15;
pub const BPF_PROG_TYPE_CGROUP_SOCK_ADDR: u32 = 18;
pub const BPF_CGROUP_DEVICE: u32 = 6;
pub const BPF_CGROUP_UNIX_CONNECT: u32 = 49;
pub const BPF_CGROUP_UNIX_SENDMSG: u32 = 50;
/// Programs of this cgroup AND its descendants' all run; nobody without
/// the program's descriptor can detach it.
pub const BPF_F_ALLOW_MULTI: u32 = 1 << 1;
/// `CGROUP2_SUPER_MAGIC`.
const CGROUP2: i64 = 0x6367_7270;
/// Every `bpf(2)` call gets a zeroed buffer this long: larger than `union
/// bpf_attr` of any kernel the guard runs on (144 bytes in Linux 6.8's
/// headers, under 200 in 6.17's), so a field a newer kernel writes back
/// (6.17's `query.revision` at byte 56, whatever the size passed) lands in
/// it. The kernel reads the bytes it knows and wants the rest zero.
pub const ATTR_SIZE: usize = 256;

fn attr() -> [u8; ATTR_SIZE] {
    [0u8; ATTR_SIZE]
}

fn bpf(cmd: i32, attr: &mut [u8]) -> io::Result<i64> {
    // SAFETY: `attr` is a zeroed `union bpf_attr` prefix of the length
    // passed; the kernel reads (and for a query writes) only within it.
    let r = unsafe { libc::syscall(libc::SYS_bpf, cmd, attr.as_mut_ptr(), attr.len()) };
    if r < 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(r)
    }
}

fn put32(attr: &mut [u8], at: usize, value: u32) {
    attr[at..at + 4].copy_from_slice(&value.to_ne_bytes());
}

fn put64(attr: &mut [u8], at: usize, value: u64) {
    attr[at..at + 8].copy_from_slice(&value.to_ne_bytes());
}

/// Loads a program; the verifier's log is in the error when it refuses.
pub fn prog_load(prog_type: u32, expected_attach_type: u32, name: &str, insns: &[u8]) -> io::Result<OwnedFd> {
    let license = CString::new("GPL").unwrap();
    let mut log = vec![0u8; 1 << 16];
    let mut attr = attr();
    put32(&mut attr, 0, prog_type);
    put32(&mut attr, 4, (insns.len() / 8) as u32);
    put64(&mut attr, 8, insns.as_ptr() as u64);
    put64(&mut attr, 16, license.as_ptr() as u64);
    let mut name_bytes = [0u8; 16];
    for (dst, src) in name_bytes.iter_mut().zip(name.bytes().take(15)) {
        *dst = src;
    }
    attr[48..64].copy_from_slice(&name_bytes);
    put32(&mut attr, 68, expected_attach_type);
    match bpf(BPF_PROG_LOAD, &mut attr) {
        // SAFETY: the kernel returned a new descriptor that nothing else owns.
        Ok(fd) => Ok(unsafe { OwnedFd::from_raw_fd(fd as i32) }),
        Err(first) => {
            // Again with the verifier's log, to say why.
            put32(&mut attr, 24, 1);
            put32(&mut attr, 28, log.len() as u32);
            put64(&mut attr, 32, log.as_mut_ptr() as u64);
            let _ = bpf(BPF_PROG_LOAD, &mut attr);
            let end = log.iter().position(|b| *b == 0).unwrap_or(log.len());
            let text = String::from_utf8_lossy(&log[..end]);
            let tail: String = text.lines().rev().take(4).collect::<Vec<_>>().into_iter().rev().collect::<Vec<_>>().join(" | ");
            Err(io::Error::new(first.kind(), format!("{name}: {first}{}", if tail.is_empty() { String::new() } else { format!(" ({tail})") })))
        }
    }
}

/// Attaches `prog` to the cgroup `cgroup` with [`BPF_F_ALLOW_MULTI`].
pub fn prog_attach(cgroup: BorrowedFd, prog: BorrowedFd, attach_type: u32) -> io::Result<()> {
    let mut attr = attr();
    put32(&mut attr, 0, cgroup.as_raw_fd() as u32);
    put32(&mut attr, 4, prog.as_raw_fd() as u32);
    put32(&mut attr, 8, attach_type);
    put32(&mut attr, 12, BPF_F_ALLOW_MULTI);
    bpf(BPF_PROG_ATTACH, &mut attr).map(drop)
}

/// How many programs are attached to `cgroup` itself (not inherited)
/// for `attach_type`.
pub fn prog_count(cgroup: BorrowedFd, attach_type: u32) -> io::Result<u32> {
    let mut attr = attr();
    put32(&mut attr, 0, cgroup.as_raw_fd() as u32);
    put32(&mut attr, 4, attach_type);
    bpf(BPF_PROG_QUERY, &mut attr)?;
    Ok(u32::from_ne_bytes(attr[24..28].try_into().unwrap()))
}

/// Detaches without naming a program: what a process of the person's uid
/// can try. Refused for a cgroup whose programs were attached with
/// [`BPF_F_ALLOW_MULTI`].
pub fn prog_detach_any(cgroup: BorrowedFd, attach_type: u32) -> io::Result<()> {
    let mut attr = attr();
    put32(&mut attr, 0, cgroup.as_raw_fd() as u32);
    put32(&mut attr, 8, attach_type);
    bpf(BPF_PROG_DETACH, &mut attr).map(drop)
}

/// Opens `root/components…` one directory at a time, following no link,
/// and checks that it is a cgroup2 directory owned by `uid`.
pub fn open_cgroup(root: &std::path::Path, components: &[String], uid: u32) -> io::Result<OwnedFd> {
    let flags = libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC;
    let root = CString::new(root.as_os_str().as_encoded_bytes()).map_err(|_| io::Error::from(io::ErrorKind::InvalidInput))?;
    // SAFETY: a NUL-terminated path; the result is checked.
    let fd = unsafe { libc::open(root.as_ptr(), flags) };
    if fd < 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: a new descriptor of ours.
    let mut dir = unsafe { OwnedFd::from_raw_fd(fd) };
    for component in components {
        if component.is_empty() || component.contains('/') || component == "." || component == ".." {
            return Err(io::Error::from(io::ErrorKind::InvalidInput));
        }
        let name = CString::new(component.as_str()).map_err(|_| io::Error::from(io::ErrorKind::InvalidInput))?;
        // SAFETY: as above, relative to a directory we hold.
        let fd = unsafe { libc::openat(dir.as_raw_fd(), name.as_ptr(), flags) };
        if fd < 0 {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: a new descriptor of ours.
        dir = unsafe { OwnedFd::from_raw_fd(fd) };
    }
    // SAFETY: plain out-structs for fstatfs/fstat on our descriptor.
    let (fs, st) = unsafe {
        let mut fs: libc::statfs = std::mem::zeroed();
        let mut st: libc::stat = std::mem::zeroed();
        if libc::fstatfs(dir.as_raw_fd(), &mut fs) < 0 || libc::fstat(dir.as_raw_fd(), &mut st) < 0 {
            return Err(io::Error::last_os_error());
        }
        (fs, st)
    };
    if fs.f_type as i64 != CGROUP2 {
        return Err(io::Error::other("not a cgroup2 directory"));
    }
    if st.st_uid != uid {
        return Err(io::Error::other(format!("the cgroup belongs to uid {}, not {uid}", st.st_uid)));
    }
    Ok(dir)
}

/// Is the cgroup of directory `dir` gone (removed since it was opened)?
/// A removed cgroup's directory has no files any more.
pub fn cgroup_gone(dir: BorrowedFd) -> bool {
    let name = CString::new("cgroup.events").unwrap();
    // SAFETY: a NUL-terminated name relative to a directory we hold.
    let fd = unsafe { libc::openat(dir.as_raw_fd(), name.as_ptr(), libc::O_RDONLY | libc::O_CLOEXEC) };
    if fd < 0 {
        return io::Error::last_os_error().raw_os_error() == Some(libc::ENOENT);
    }
    // SAFETY: a new descriptor of ours.
    drop(unsafe { OwnedFd::from_raw_fd(fd) });
    false
}

/// The process id and user id of the other end of a unix socket.
pub fn peer(socket: BorrowedFd) -> io::Result<(u32, u32)> {
    // SAFETY: SO_PEERCRED fills a `struct ucred` of the size passed.
    unsafe {
        let mut cred: libc::ucred = std::mem::zeroed();
        let mut len = std::mem::size_of::<libc::ucred>() as libc::socklen_t;
        if libc::getsockopt(socket.as_raw_fd(), libc::SOL_SOCKET, libc::SO_PEERCRED, (&mut cred as *mut libc::ucred).cast(), &mut len) < 0 {
            return Err(io::Error::last_os_error());
        }
        Ok((cred.pid as u32, cred.uid))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Linux 6.8's `union bpf_attr` (this machine's headers: 144 bytes)
    /// and the furthest field a kernel up to 6.17 writes back (the query's
    /// `revision`, bytes 56..64) fit, with room for newer ones.
    #[test]
    fn every_bpf_attr_is_larger_than_any_kernels_union() {
        assert!(ATTR_SIZE >= 144 && ATTR_SIZE >= 56 + 8 && ATTR_SIZE % 8 == 0);
        assert_eq!(attr().len(), ATTR_SIZE);
    }
}
