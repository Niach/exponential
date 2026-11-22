//! What the guard does for a request, given the asking process.

use std::{
    collections::HashMap,
    os::fd::{AsFd, OwnedFd},
    path::PathBuf,
};

use crate::{btf, cgroup, connect, devices, protocol::Request, signals, sys};

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
    /// `/sys/fs/cgroup`, `/proc`, the kernel's BTF and its list of LSMs
    /// (other roots in tests).
    pub cgroup_root: PathBuf,
    pub proc_root: PathBuf,
    pub btf: PathBuf,
    pub lsm: PathBuf,
    /// #EDE-157: the signal program's ids on this kernel, or why this
    /// kernel cannot run it (then a space's signals stay open), read and
    /// tried once per run.
    signals: Option<Result<signals::Ids, String>>,
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
        Self { cgroup_root: "/sys/fs/cgroup".into(), proc_root: "/proc".into(), btf: "/sys/kernel/btf/vmlinux".into(), lsm: "/sys/kernel/security/lsm".into(), signals: None, devices: None, layout: None, connects: HashMap::new(), attached: HashMap::new(), probed: None }
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

    /// #EDE-157: the signal program loads here and the kernel runs it: `bpf`
    /// among its LSMs, the hook and the kernel functions in its BTF, and a
    /// program for a slice accepted by the verifier (tried once, for the
    /// root cgroup). `Err` = a space's signals stay open, and why.
    fn signals(&mut self) -> Result<signals::Ids, String> {
        if self.signals.is_none() {
            let tried = (|| {
                let lsm = std::fs::read_to_string(&self.lsm).map_err(|e| format!("{}: {e}", self.lsm.display()))?;
                if !signals::bpf_lsm_active(&lsm) {
                    return Err(format!("this kernel runs no BPF LSM programs (no bpf in {}: {})", self.lsm.display(), lsm.trim()));
                }
                let btf = std::fs::read(&self.btf).map_err(|e| format!("{}: {e}", self.btf.display()))?;
                let ids = signals::Ids::from_btf(&btf).map_err(|e| format!("this kernel has no signal hook: {e}"))?;
                self.load_signals(&ids, 1)?;
                Ok(ids)
            })();
            self.signals = Some(tried);
        }
        self.signals.clone().unwrap()
    }

    fn load_signals(&self, ids: &signals::Ids, slice: u64) -> Result<OwnedFd, String> {
        sys::prog_load_at(sys::BPF_PROG_TYPE_LSM, sys::BPF_LSM_CGROUP, ids.hook, signals::NAME, &signals::program(ids, slice)).map_err(|e| format!("this kernel refuses the signal program: {e}"))
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
            // #EDE-157: `ok` = a space's signals to the person's processes
            // are refused here; `no` says why they stay open (spaces are
            // offered all the same).
            Request::ProbeSignals => self.signals().map(drop),
            Request::Enter { spaces } => {
                crate::protocol::spaces_dir_of(spaces, uid)?;
                let text = std::fs::read_to_string(self.proc_root.join(pid.to_string()).join("cgroup")).map_err(|e| format!("process {pid}: {e}"))?;
                let slice = cgroup::space_slice_of(&text, uid)?;
                let dir = sys::open_cgroup(&self.cgroup_root, &slice.components, uid).map_err(|e| format!("{}: {e}", slice.name()))?;
                // Ours already (another launch of the space asked first):
                // nobody but this service can attach to a user's cgroup.
                let have = |kind| sys::prog_count(dir.as_fd(), kind).map_err(|e| format!("{}: {e}", slice.name()));
                let there = [have(sys::BPF_CGROUP_DEVICE)? > 0, have(sys::BPF_CGROUP_UNIX_CONNECT)? > 0, have(sys::BPF_CGROUP_UNIX_SENDMSG)? > 0];
                // The signal program where this kernel runs it.
                let signals = self.signals().ok();
                let signals_there = match signals {
                    Some(_) => have(sys::BPF_LSM_CGROUP)? > 0,
                    None => true,
                };
                if there.iter().all(|t| *t) && signals_there {
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
                // The slice's own program (its cgroup id baked in); the
                // kernel keeps it while it is attached.
                if let (Some(ids), false) = (signals, signals_there) {
                    let id = sys::cgroup_id(dir.as_fd()).map_err(|e| format!("{}: {e}", slice.name()))?;
                    let program = self.load_signals(&ids, id)?;
                    sys::prog_attach(dir.as_fd(), program.as_fd(), sys::BPF_LSM_CGROUP).map_err(|e| format!("{}: the signal program: {e}", slice.name()))?;
                }
                eprintln!("guarded {} of user {uid} (asked by process {pid}; signals {})", slice.name(), if signals.is_some() { "closed" } else { "open" });
                self.attached.entry(uid).or_default().push(Guarded { dir, spaces: spaces.clone() });
                Ok(())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// #EDE-157: a kernel without `bpf` among its LSMs leaves a space's
    /// signals open, and the guard says why (never by loading anything).
    #[test]
    fn without_the_bpf_lsm_signals_stay_open_and_the_guard_says_why() {
        let dir = std::env::temp_dir().join(format!("ede-guard-lsm-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let lsm = dir.join("lsm");
        std::fs::write(&lsm, "lockdown,capability,landlock,yama,apparmor,ima,evm\n").unwrap();
        let mut guard = Guard { lsm: lsm.clone(), ..Guard::default() };
        let answer = guard.handle(&Request::ProbeSignals, std::process::id(), 1000);
        assert_eq!(answer, Err(format!("this kernel runs no BPF LSM programs (no bpf in {}: lockdown,capability,landlock,yama,apparmor,ima,evm)", lsm.display())));
        let mut guard = Guard { lsm: dir.join("missing"), ..Guard::default() };
        assert!(guard.handle(&Request::ProbeSignals, std::process::id(), 1000).unwrap_err().contains("missing"));
        let _ = std::fs::remove_dir_all(dir);
    }
}
