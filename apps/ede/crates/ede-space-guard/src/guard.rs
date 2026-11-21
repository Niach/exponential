//! What the guard does for a request, given the asking process.

use std::{
    collections::HashMap,
    os::fd::{AsFd, OwnedFd},
    path::PathBuf,
};

use crate::{btf, cgroup, connect, devices, protocol::Request, sys};

/// How many LIVE space slices of one user carry the guard's programs (an
/// attach is one list entry in the kernel; this bounds a user who makes
/// space-named cgroups by the thousand). A slice whose cgroup is gone no
/// longer counts.
pub const MAX_SLICES_PER_USER: usize = 64;
/// How many directories of spaces of one user have connect programs of
/// their own (one per compositor of theirs), counting those with a live
/// guarded slice.
pub const MAX_SPACES_DIRS_PER_USER: usize = 4;

/// A space slice guarded and alive: its directory and its spaces' root.
struct Guarded {
    dir: OwnedFd,
    spaces: String,
}

pub struct Guard {
    /// `/sys/fs/cgroup`, `/proc` and the kernel's BTF (other roots in tests).
    pub cgroup_root: PathBuf,
    pub proc_root: PathBuf,
    pub btf: PathBuf,
    devices: Option<OwnedFd>,
    /// This kernel's layout of a connect's address, read once.
    layout: Option<Result<connect::Layout, String>>,
    /// The connect programs (connect, sendmsg) per (user, directory of spaces).
    connects: HashMap<(u32, String), [OwnedFd; 2]>,
    /// Per user, the space slices guarded and alive.
    attached: HashMap<u32, Vec<Guarded>>,
    /// The probe's answer, once given.
    probed: Option<Result<(), String>>,
}

impl Default for Guard {
    fn default() -> Self {
        Self { cgroup_root: "/sys/fs/cgroup".into(), proc_root: "/proc".into(), btf: "/sys/kernel/btf/vmlinux".into(), devices: None, layout: None, connects: HashMap::new(), attached: HashMap::new(), probed: None }
    }
}

impl Guard {
    /// The device program, loaded once per run of the guard (with the
    /// dynamic majors of this boot): every space slice gets the same
    /// program object.
    fn devices(&mut self) -> Result<&OwnedFd, String> {
        if self.devices.is_none() {
            let table = devices::table(&std::fs::read_to_string(self.proc_root.join("devices")).unwrap_or_default());
            let fd = sys::prog_load(sys::BPF_PROG_TYPE_CGROUP_DEVICE, sys::BPF_CGROUP_DEVICE, devices::NAME, &devices::program(&table)).map_err(|e| format!("this kernel refuses the device program: {e}"))?;
            self.devices = Some(fd);
        }
        Ok(self.devices.as_ref().unwrap())
    }

    /// Where this kernel keeps a connect's address and its length: read
    /// from its BTF once per run.
    fn layout(&mut self) -> Result<connect::Layout, String> {
        if self.layout.is_none() {
            let read = std::fs::read(&self.btf).map_err(|e| format!("{}: {e}", self.btf.display())).and_then(|btf| {
                btf::member_offsets(&btf, "bpf_sock_addr_kern", &["uaddr", "uaddrlen"]).map_err(|e| format!("this kernel has no unix connect hook: {e}"))
            });
            self.layout = Some(read.map(|offsets| connect::Layout { uaddr: offsets[0], uaddrlen: offsets[1] }));
        }
        self.layout.clone().unwrap()
    }

    /// Loads the connect program of `policy` for connect() and for sendto().
    fn load_connect(&mut self, policy: &connect::Policy) -> Result<[OwnedFd; 2], String> {
        let program = policy.program(self.layout()?)?;
        let load = |attach| sys::prog_load(sys::BPF_PROG_TYPE_CGROUP_SOCK_ADDR, attach, connect::NAME, &program).map_err(|e| format!("this kernel refuses the connect program: {e}"));
        Ok([load(sys::BPF_CGROUP_UNIX_CONNECT)?, load(sys::BPF_CGROUP_UNIX_SENDMSG)?])
    }

    /// The live guarded slices of user `uid` (gone ones are dropped, and
    /// with them the connect programs of a directory of spaces none of the
    /// live ones has).
    fn live(&mut self, uid: u32) -> usize {
        let slices = self.attached.entry(uid).or_default();
        slices.retain(|slice| !sys::cgroup_gone(slice.dir.as_fd()));
        let roots: Vec<String> = slices.iter().map(|slice| slice.spaces.clone()).collect();
        self.connects.retain(|(user, spaces), _| *user != uid || roots.contains(spaces));
        roots.len()
    }

    /// Answers `request` of process `pid` of user `uid`.
    pub fn handle(&mut self, request: &Request, pid: u32, uid: u32) -> Result<(), String> {
        match request {
            // Both programs load on this kernel; asked once per run (the
            // connect program for a directory of spaces of the asker's).
            // This guard refuses the person's Wayland socket (#EDE-145):
            // asked so, it answers the same.
            Request::Probe | Request::ProbeWayland => {
                if self.probed.is_none() {
                    let devices = self.devices().map(drop);
                    let connect = self.load_connect(&connect::Policy { uid, spaces: format!("/run/user/{uid}/exponential-desktop/probe.spaces") }).map(drop);
                    self.probed = Some(devices.and(connect));
                }
                self.probed.clone().unwrap()
            }
            Request::Enter { spaces } => {
                crate::protocol::spaces_dir_of(spaces, uid)?;
                let text = std::fs::read_to_string(self.proc_root.join(pid.to_string()).join("cgroup")).map_err(|e| format!("process {pid}: {e}"))?;
                let slice = cgroup::space_slice_of(&text, uid)?;
                let dir = sys::open_cgroup(&self.cgroup_root, &slice.components, uid).map_err(|e| format!("{}: {e}", slice.name()))?;
                // Ours already (another launch of the space asked first):
                // nobody but this service can attach to a user's cgroup.
                let have = |kind| sys::prog_count(dir.as_fd(), kind).map_err(|e| format!("{}: {e}", slice.name()));
                let there = [have(sys::BPF_CGROUP_DEVICE)? > 0, have(sys::BPF_CGROUP_UNIX_CONNECT)? > 0, have(sys::BPF_CGROUP_UNIX_SENDMSG)? > 0];
                if there.iter().all(|t| *t) {
                    return Ok(());
                }
                if self.live(uid) >= MAX_SLICES_PER_USER {
                    return Err(format!("user {uid} has {MAX_SLICES_PER_USER} guarded space slices alive already"));
                }
                // The device program first (it needs nothing of the kernel
                // but cgroup2), then the connect programs; a slice without
                // all three is not guarded and its launch does not run.
                if !there[0] {
                    let devices = self.devices()?.try_clone().map_err(|e| e.to_string())?;
                    sys::prog_attach(dir.as_fd(), devices.as_fd(), sys::BPF_CGROUP_DEVICE).map_err(|e| format!("{}: {e}", slice.name()))?;
                }
                let key = (uid, spaces.clone());
                if !self.connects.contains_key(&key) {
                    if self.connects.keys().filter(|(user, _)| *user == uid).count() >= MAX_SPACES_DIRS_PER_USER {
                        return Err(format!("user {uid} has {MAX_SPACES_DIRS_PER_USER} directories of spaces guarded already"));
                    }
                    let programs = self.load_connect(&connect::Policy { uid, spaces: spaces.clone() })?;
                    self.connects.insert(key.clone(), programs);
                }
                for (i, attach) in [(1, sys::BPF_CGROUP_UNIX_CONNECT), (2, sys::BPF_CGROUP_UNIX_SENDMSG)] {
                    if !there[i] {
                        let program = self.connects[&key][i - 1].try_clone().map_err(|e| e.to_string())?;
                        sys::prog_attach(dir.as_fd(), program.as_fd(), attach).map_err(|e| format!("{}: {e}", slice.name()))?;
                    }
                }
                eprintln!("guarded {} of user {uid} (asked by process {pid})", slice.name());
                self.attached.entry(uid).or_default().push(Guarded { dir, spaces: spaces.clone() });
                Ok(())
            }
        }
    }
}
