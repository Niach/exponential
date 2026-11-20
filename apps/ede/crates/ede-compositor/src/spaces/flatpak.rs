//! #EDE-138: a Flatpak started in a space comes back into the space.
//!
//! `flatpak run` asks the person's user manager for its scope at a path
//! of its own (`/run/user/<uid>/systemd/private`, whatever
//! `XDG_RUNTIME_DIR` and the session bus say), so its
//! `app-flatpak-<app>-<instance>.scope` is in the person's `app.slice`:
//! outside the space's slice and its limits, and not ended with it. The
//! compositor finds such a scope on its 1 s tick (a scope whose first
//! process was started by a space's process, or is a launch into a space)
//! and asks the space's own user manager for a scope of its own holding
//! the same processes: they move into the space's slice (the person's
//! `app.slice`, their common ancestor, is the person's own cgroup). What
//! is still outside when the space closes is ended by the kernel.

use std::{
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
};

use ede_protocol::process::{cgroup, space_in_cgroup, stat};

use super::slice::kill_cgroup;
use crate::{desk::DeskId, state::Ede};

/// A Flatpak's scope in the person's `app.slice` that belongs to a space.
#[derive(Debug, PartialEq, Eq)]
pub struct Escaped {
    pub space: u32,
    /// Its cgroup directory.
    pub cgroup: PathBuf,
    /// The processes in it now.
    pub pids: Vec<u32>,
}

/// The Flatpak scopes in `app_slice` (the person's `app.slice` cgroup
/// directory) whose first process belongs to one of compositor
/// `compositor`'s spaces: its parent runs in that space's slice, or it is
/// a launch into the space (`launches`: process id and space number).
/// `cgroup_of` reads `/proc/<pid>/cgroup` (a test's in the tests).
pub fn escaped(app_slice: &Path, compositor: u32, launches: &[(u32, u32)], cgroup_of: impl Fn(u32) -> Option<String>) -> Vec<Escaped> {
    let Ok(entries) = std::fs::read_dir(app_slice) else { return Vec::new() };
    let mut found = Vec::new();
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(name) = name.to_str() else { continue };
        if !(name.starts_with("app-flatpak-") && name.ends_with(".scope")) {
            continue;
        }
        let dir = entry.path();
        let pids: Vec<u32> = std::fs::read_to_string(dir.join("cgroup.procs")).unwrap_or_default().lines().filter_map(|pid| pid.trim().parse().ok()).collect();
        let Some(first) = pids.iter().min().copied() else { continue };
        let space = launches.iter().find(|(pid, _)| *pid == first).map(|(_, space)| *space).or_else(|| {
            let parent = stat(first)?.ppid;
            space_in_cgroup(&cgroup_of(parent)?).filter(|(owner, _)| *owner == compositor).and_then(|(_, space)| space.as_str().strip_prefix("space-")?.parse().ok())
        });
        if let Some(space) = space {
            found.push(Escaped { space, cgroup: dir, pids });
        }
    }
    found
}

/// What the compositor keeps of the Flatpaks it brought back.
#[derive(Default)]
pub(crate) struct Adoptions {
    /// `busctl` asking a space's manager, until it is reaped.
    pending: Vec<(PathBuf, Child)>,
    /// How many were asked for: each scope's name has its number.
    serial: u32,
}

impl Ede {
    /// Once a second: every Flatpak scope of a space outside its slice
    /// is asked into the space's own user manager. Nothing here waits.
    pub(crate) fn space_flatpaks_tick(&mut self) {
        let Some(units) = &self.spaces.units else { return };
        self.spaces.adoptions.pending.retain_mut(|(_, child)| matches!(child.try_wait(), Ok(None)));
        let launches: Vec<(u32, u32)> = self.spaces.launches.iter().map(|launch| (launch.pid(), launch.desk().number())).collect();
        for found in escaped(&units.app_slice, units.compositor, &launches, cgroup) {
            if self.spaces.adoptions.pending.iter().any(|(dir, _)| *dir == found.cgroup) {
                continue;
            }
            let Some(space) = self.spaces.get(DeskId::numbered(found.space)) else { continue };
            let bus = space.dir.join("bus");
            self.spaces.adoptions.serial += 1;
            let unit = format!("flatpak-{}.scope", self.spaces.adoptions.serial);
            let mut ask = Command::new(&units.busctl);
            ask.arg(format!("--address=unix:path={}", bus.display()))
                .args(["--timeout=5", "call", "org.freedesktop.systemd1", "/org/freedesktop/systemd1", "org.freedesktop.systemd1.Manager", "StartTransientUnit", "ssa(sv)a(sa(sv))", &unit, "fail", "1", "PIDs", "au"])
                .arg(found.pids.len().to_string())
                .args(found.pids.iter().map(u32::to_string))
                .arg("0")
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null());
            match ask.spawn() {
                Ok(child) => {
                    tracing::info!("space-{}: its Flatpak {} comes into its slice", found.space, found.cgroup.file_name().unwrap_or_default().to_string_lossy());
                    self.spaces.adoptions.pending.push((found.cgroup, child));
                }
                Err(e) => tracing::warn!("{}: {e}", units.busctl.display()),
            }
        }
    }

    /// Space number `space` closes: what of its Flatpaks is still outside
    /// its slice is ended at once.
    pub(crate) fn space_flatpaks_closed(&mut self, space: u32) {
        let Some(units) = &self.spaces.units else { return };
        let launches: Vec<(u32, u32)> = self.spaces.launches.iter().map(|launch| (launch.pid(), launch.desk().number())).collect();
        for found in escaped(&units.app_slice, units.compositor, &launches, cgroup).into_iter().filter(|found| found.space == space) {
            if kill_cgroup(&found.cgroup) {
                tracing::info!("space-{space}: its Flatpak {} was still outside its slice and ended with it", found.cgroup.file_name().unwrap_or_default().to_string_lossy());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Only a Flatpak scope whose first process a space started is the
    /// space's: a launch, or a child of a process in the space's slice.
    #[test]
    fn a_flatpak_scope_is_a_spaces_by_its_first_process() {
        let dir = tempfile::tempdir().unwrap();
        let app = dir.path();
        let scope = |name: &str, pids: &[u32]| {
            std::fs::create_dir_all(app.join(name)).unwrap();
            std::fs::write(app.join(name).join("cgroup.procs"), pids.iter().map(|pid| format!("{pid}\n")).collect::<String>()).unwrap();
        };
        // This test process's parent stands for a space's process.
        let me = std::process::id();
        let parent = stat(me).unwrap().ppid;
        let mut child = Command::new("sleep").arg("30").spawn().unwrap();
        scope("app-flatpak-org.example.A-11.scope", &[child.id()]);
        scope("app-flatpak-org.example.B-22.scope", &[me]);
        scope("app-flatpak-org.example.C-33.scope", &[]);
        scope("app-gnome-firefox-44.scope", &[child.id()]);
        let in_space = |pid: u32| -> Option<String> {
            Some(if pid == me { "0::/user.slice/u/app.slice/app-exponential_p9.slice/app-exponential_p9-s3.slice/x.scope\n".into() } else if pid == parent { "0::/user.slice/u/session-2.scope\n".into() } else { cgroup(pid)? })
        };
        let found = escaped(app, 9, &[(me, 5)], in_space);
        let mut found: Vec<(u32, String, Vec<u32>)> = found.into_iter().map(|e| (e.space, e.cgroup.file_name().unwrap().to_string_lossy().into_owned(), e.pids)).collect();
        found.sort();
        // A: its first process's parent (this test) runs in space 3. B: its
        // first process is a launch into space 5. C: empty. Not a Flatpak's: skipped.
        assert_eq!(found, [(3, "app-flatpak-org.example.A-11.scope".to_string(), vec![child.id()]), (5, "app-flatpak-org.example.B-22.scope".to_string(), vec![me])]);
        // Another compositor's spaces are not this one's.
        assert!(escaped(app, 10, &[], in_space).is_empty());
        let _ = child.kill();
        let _ = child.wait();
    }
}
