//! #EXP-1868: is the slice this host's runs live in being memory-throttled?
//!
//! A cgroup over its `memory.high` is not killed: the kernel throttles every
//! process in it (`mem_cgroup_handle_over_high`, D state). From inside, that
//! reads like an outage: `docker info`, curl, ssh and gh hang while ping
//! (kernel-side) answers. On 2026-11-15 a session took mint's `expbg.slice`
//! at 17.4 of 16 GB for a prod outage. The device doctor says so instead
//! (`device_doctor::add_memory`): a session reads throttling as throttling.
//!
//! Which slices: every ancestor of this process's own cgroup that sets a
//! `memory.high`, plus the user manager's `expbg.slice` (where every
//! `systemd-run --user --scope` run lands on a desktop-first machine,
//! `.github/runners/README.md`), the first one throttled wins. Linux only;
//! read-only (never changes a cap: those are the machine owner's call).

use std::path::{Path, PathBuf};

/// `memory.pressure`'s `full avg10` (percent of the last 10 s in which
/// every task of the cgroup stalled on memory) at which a slice reads
/// throttled even below its `memory.high` (reclaim keeps it just under).
pub const FULL_PRESSURE_PERCENT: f64 = 10.0;

/// One throttled slice.
#[derive(Clone, Debug, PartialEq)]
pub struct SliceMemory {
    /// The cgroup's last path component (`expbg.slice`).
    pub name: String,
    pub current: u64,
    pub high: u64,
    /// `full avg10` from `memory.pressure` (0 when unreadable).
    pub full_avg10: f64,
}

impl SliceMemory {
    /// Over its `memory.high`, or stalling on memory most of the time.
    pub fn throttled(&self) -> bool {
        self.current > self.high || self.full_avg10 >= FULL_PRESSURE_PERCENT
    }
}

/// `full avg10=<x>` of a `memory.pressure` file.
pub fn parse_full_avg10(pressure: &str) -> Option<f64> {
    pressure
        .lines()
        .find(|line| line.starts_with("full "))?
        .split_whitespace()
        .find_map(|field| field.strip_prefix("avg10="))?
        .parse()
        .ok()
}

/// The cgroup v2 path in a `/proc/self/cgroup` file (`0::/user.slice/…`).
pub fn parse_own_cgroup(proc_cgroup: &str) -> Option<&str> {
    proc_cgroup.lines().find_map(|line| line.strip_prefix("0::"))
}

/// The slices to check under `root` (the cgroup2 mount) for a process in
/// `own` (relative cgroup path): its ancestors, nearest first, then the
/// `expbg.slice` of the `user@<uid>.service` it lives under, if any.
pub fn candidates(root: &Path, own: &str) -> Vec<PathBuf> {
    let parts: Vec<&str> = own.trim_matches('/').split('/').filter(|part| !part.is_empty()).collect();
    let mut dirs: Vec<PathBuf> = (1..=parts.len()).rev().map(|n| root.join(parts[..n].join("/"))).collect();
    if let Some(at) = parts.iter().position(|part| part.starts_with("user@") && part.ends_with(".service")) {
        let expbg = root.join(parts[..=at].join("/")).join("expbg.slice");
        if !dirs.contains(&expbg) {
            dirs.push(expbg);
        }
    }
    dirs
}

/// `dir`'s memory, `None` without a numeric `memory.high` (`max` = no limit).
pub fn read_slice(dir: &Path) -> Option<SliceMemory> {
    let read = |file: &str| std::fs::read_to_string(dir.join(file)).ok();
    let high = read("memory.high")?.trim().parse::<u64>().ok()?;
    let current = read("memory.current")?.trim().parse::<u64>().ok()?;
    let full_avg10 = read("memory.pressure").as_deref().and_then(parse_full_avg10).unwrap_or(0.0);
    let name = dir.file_name()?.to_string_lossy().into_owned();
    Some(SliceMemory { name, current, high, full_avg10 })
}

/// The first throttled slice of [`candidates`] under `root`.
pub fn throttled_under(root: &Path, own: &str) -> Option<SliceMemory> {
    candidates(root, own).iter().filter_map(|dir| read_slice(dir)).find(SliceMemory::throttled)
}

/// This host's throttled slice right now (Linux, cgroup v2), else `None`.
pub fn throttled_slice() -> Option<SliceMemory> {
    if !cfg!(target_os = "linux") {
        return None;
    }
    let proc_cgroup = std::fs::read_to_string("/proc/self/cgroup").ok()?;
    throttled_under(Path::new("/sys/fs/cgroup"), parse_own_cgroup(&proc_cgroup)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    const GB: u64 = 1_000_000_000;

    #[test]
    fn pressure_and_own_cgroup_parse() {
        let pressure = "some avg10=80.10 avg60=79.00 avg300=70.00 total=1\nfull avg10=75.32 avg60=74.00 avg300=60.00 total=2\n";
        assert_eq!(parse_full_avg10(pressure), Some(75.32));
        assert_eq!(parse_full_avg10("some avg10=1.00 avg60=0 avg300=0 total=0\n"), None);
        assert_eq!(
            parse_own_cgroup("0::/user.slice/user-1000.slice/user@1000.service/app.slice/exponential-daemon.service\n"),
            Some("/user.slice/user-1000.slice/user@1000.service/app.slice/exponential-daemon.service")
        );
    }

    #[test]
    fn the_daemons_slices_and_the_runs_slice_are_checked() {
        let root = Path::new("/sys/fs/cgroup");
        let own = "/user.slice/user-1000.slice/user@1000.service/app.slice/exponential-daemon.service";
        let dirs = candidates(root, own);
        assert_eq!(dirs[0], root.join(own.trim_start_matches('/')));
        assert_eq!(dirs[1], root.join("user.slice/user-1000.slice/user@1000.service/app.slice"));
        assert_eq!(dirs.last().unwrap(), &root.join("user.slice/user-1000.slice/user@1000.service/expbg.slice"));
        let inside = candidates(root, "/user.slice/user-1000.slice/user@1000.service/expbg.slice/run-x.scope");
        assert_eq!(inside.iter().filter(|dir| dir.ends_with("expbg.slice")).count(), 1, "never twice");
    }

    #[test]
    fn a_slice_over_its_high_reads_throttled() {
        let slice = |current, full_avg10| SliceMemory { name: "expbg.slice".into(), current, high: 16 * GB, full_avg10 };
        assert!(slice(17_400_000_000, 0.0).throttled(), "the 2026-11-15 state");
        assert!(slice(15 * GB, 75.0).throttled(), "reclaim holds it just under, everything stalls");
        assert!(!slice(12 * GB, 0.2).throttled());
    }

    /// The whole read on a fake cgroup tree: the throttled expbg.slice is
    /// found from a daemon that lives elsewhere; a `max` high never is.
    #[test]
    fn the_throttled_slice_is_found_on_a_cgroup_tree() {
        let root = std::env::temp_dir().join(format!("exp-cgroup-{}", std::process::id()));
        let user = root.join("user.slice/user-1000.slice/user@1000.service");
        let own = user.join("app.slice/exponential-daemon.service");
        let expbg = user.join("expbg.slice");
        std::fs::create_dir_all(&own).unwrap();
        std::fs::create_dir_all(&expbg).unwrap();
        std::fs::write(user.join("app.slice/memory.high"), "max\n").unwrap();
        std::fs::write(user.join("app.slice/memory.current"), "99999999999\n").unwrap();
        std::fs::write(expbg.join("memory.high"), "17179869184\n").unwrap();
        std::fs::write(expbg.join("memory.current"), "18683000000\n").unwrap();
        std::fs::write(expbg.join("memory.pressure"), "some avg10=80.00 avg60=0 avg300=0 total=0\nfull avg10=75.00 avg60=0 avg300=0 total=0\n").unwrap();
        let own_path = "/user.slice/user-1000.slice/user@1000.service/app.slice/exponential-daemon.service";
        let found = throttled_under(&root, own_path).expect("expbg.slice is throttled");
        assert_eq!(
            found,
            SliceMemory { name: "expbg.slice".into(), current: 18_683_000_000, high: 17_179_869_184, full_avg10: 75.0 }
        );
        std::fs::write(expbg.join("memory.current"), "12000000000\n").unwrap();
        std::fs::write(expbg.join("memory.pressure"), "full avg10=0.00 avg60=0 avg300=0 total=0\n").unwrap();
        assert_eq!(throttled_under(&root, own_path), None);
        let _ = std::fs::remove_dir_all(&root);
    }
}
