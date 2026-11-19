//! What the guard does for a request, given the asking process.

use std::{
    collections::HashMap,
    os::fd::{AsFd, OwnedFd},
    path::PathBuf,
};

use crate::{cgroup, devices, protocol::Request, sys};

/// How many space slices of one user the guard attaches to while it runs
/// (an attach is one list entry in the kernel; this bounds a user who
/// makes space-named cgroups by the thousand).
pub const MAX_SLICES_PER_USER: u32 = 64;

pub struct Guard {
    /// `/sys/fs/cgroup`, `/proc` (other roots in tests).
    pub cgroup_root: PathBuf,
    pub proc_root: PathBuf,
    devices: Option<OwnedFd>,
    attached: HashMap<u32, u32>,
}

impl Default for Guard {
    fn default() -> Self {
        Self { cgroup_root: "/sys/fs/cgroup".into(), proc_root: "/proc".into(), devices: None, attached: HashMap::new() }
    }
}

impl Guard {
    /// The device program, loaded once per run of the guard: every space
    /// slice gets the same program object.
    fn devices(&mut self) -> Result<&OwnedFd, String> {
        if self.devices.is_none() {
            let fd = sys::prog_load(sys::BPF_PROG_TYPE_CGROUP_DEVICE, sys::BPF_CGROUP_DEVICE, devices::NAME, &devices::program()).map_err(|e| format!("this kernel refuses the device program: {e}"))?;
            self.devices = Some(fd);
        }
        Ok(self.devices.as_ref().unwrap())
    }

    /// Answers `request` of process `pid` of user `uid`.
    pub fn handle(&mut self, request: &Request, pid: u32, uid: u32) -> Result<(), String> {
        match request {
            Request::Probe => self.devices().map(drop),
            Request::Enter { spaces } => {
                crate::protocol::spaces_dir_of(spaces, uid)?;
                let text = std::fs::read_to_string(self.proc_root.join(pid.to_string()).join("cgroup")).map_err(|e| format!("process {pid}: {e}"))?;
                let slice = cgroup::space_slice_of(&text, uid)?;
                let dir = sys::open_cgroup(&self.cgroup_root, &slice.components, uid).map_err(|e| format!("{}: {e}", slice.name()))?;
                // Ours already (another launch of the space asked first):
                // nobody but this service can attach to a user's cgroup.
                if sys::prog_count(dir.as_fd(), sys::BPF_CGROUP_DEVICE).map_err(|e| format!("{}: {e}", slice.name()))? > 0 {
                    return Ok(());
                }
                let count = self.attached.entry(uid).or_default();
                if *count >= MAX_SLICES_PER_USER {
                    return Err(format!("user {uid} has {MAX_SLICES_PER_USER} guarded space slices already"));
                }
                let devices = self.devices()?.try_clone().map_err(|e| e.to_string())?;
                sys::prog_attach(dir.as_fd(), devices.as_fd(), sys::BPF_CGROUP_DEVICE).map_err(|e| format!("{}: {e}", slice.name()))?;
                *self.attached.entry(uid).or_default() += 1;
                eprintln!("guarded {} of user {uid} (asked by process {pid})", slice.name());
                Ok(())
            }
        }
    }
}
