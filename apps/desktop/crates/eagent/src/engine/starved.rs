//! #FEED-296: is a starting agent stalled on memory by its cgroup?
//!
//! A cgroup over its `memory.high` is not killed: the kernel throttles every
//! process in it (D state in `mem_cgroup_handle_over_high`). Measured on
//! mint (2026-11-23): a `claude` started inside a scope 100 MB over its
//! `memory.high` took 9.3 s to answer `initialize` (0.3 s normally); one
//! whose cgroup stalled ~80 % of the time (`full avg10`) answered nothing
//! in 240 s. CPU load alone did not slow it (0.35 s beside 96 busy loops),
//! nor did the transcript's size (0.24 s at 0.4 MB, 0.42 s at 68 MB). On
//! that day mint's `expbg.slice`, where every agent run lives, had spent
//! 5.4 of 47 hours with all its tasks stalled on memory.
//!
//! A start stalled like that is not a wedged CLI: its silence clock pauses
//! while it is starved ([`StartClock`]), and the failure, if it comes, says
//! the machine is short of memory instead of blaming the agent. Linux only;
//! read-only.

use std::path::Path;
use std::time::Duration;

/// `memory.pressure`'s `full avg10` (percent of the last 10 s in which every
/// task of the cgroup stalled on memory) at which a cgroup reads starved
/// even below its `memory.high` (reclaim keeps it just under). The same
/// threshold the device doctor uses (`coding::memory_pressure`).
pub const FULL_PRESSURE_PERCENT: f64 = 10.0;

/// The cgroup (its last path component, `expbg.slice`) that is throttling
/// `pid` on memory right now, if any. `None` off Linux or when unreadable.
pub fn memory_starved(pid: u32) -> Option<String> {
    if !cfg!(target_os = "linux") {
        return None;
    }
    let cgroup = std::fs::read_to_string(format!("/proc/{pid}/cgroup")).ok()?;
    starved_under(Path::new("/sys/fs/cgroup"), own_cgroup(&cgroup)?)
}

/// The cgroup v2 path in a `/proc/<pid>/cgroup` file (`0::/user.slice/…`).
fn own_cgroup(proc_cgroup: &str) -> Option<&str> {
    proc_cgroup.lines().find_map(|line| line.strip_prefix("0::"))
}

/// [`memory_starved`] over a cgroup tree at `root`: from the process's own
/// cgroup up, the first one over its `memory.high` names the throttling
/// (that is the cap a person set); else its own cgroup when that one is
/// stalling on memory ([`FULL_PRESSURE_PERCENT`]).
pub fn starved_under(root: &Path, cgroup: &str) -> Option<String> {
    let parts: Vec<&str> = cgroup.split('/').filter(|part| !part.is_empty()).collect();
    let dirs: Vec<std::path::PathBuf> =
        (1..=parts.len()).rev().map(|depth| root.join(parts[..depth].join("/"))).collect();
    let name = |dir: &Path| dir.file_name().map(|name| name.to_string_lossy().into_owned());
    let over_high = dirs.iter().find(|dir| {
        let read = |file: &str| std::fs::read_to_string(dir.join(file)).ok();
        let high = read("memory.high").and_then(|high| high.trim().parse::<u64>().ok());
        let current = read("memory.current").and_then(|current| current.trim().parse::<u64>().ok());
        matches!((current, high), (Some(current), Some(high)) if current > high)
    });
    if let Some(dir) = over_high {
        return name(dir);
    }
    // Pressure only on its OWN cgroup: a slice's is the sum of everyone
    // in it (another session's build stalling there is not this start's).
    let own = dirs.first()?;
    std::fs::read_to_string(own.join("memory.pressure"))
        .ok()
        .and_then(|pressure| full_avg10(&pressure))
        .filter(|full| *full >= FULL_PRESSURE_PERCENT)
        .and_then(|_| name(own))
}

/// `full avg10=<x>` of a `memory.pressure` file.
fn full_avg10(pressure: &str) -> Option<f64> {
    pressure
        .lines()
        .find(|line| line.starts_with("full "))?
        .split_whitespace()
        .find_map(|field| field.strip_prefix("avg10="))?
        .parse()
        .ok()
}

/// How a startup wait ended ([`StartClock::tick`]).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StartEnd {
    /// Nothing heard for the whole window of time the CLI could run
    /// (`starved` = the cgroup that throttled it meanwhile, if one did).
    Silent { starved: Option<String> },
    /// It spoke but never answered `initialize` within the window: alive,
    /// just slow (a slow MCP handshake); the session seeds from `init`.
    Late,
}

/// #FEED-296: the startup wait's clock. `window` counts only the time the
/// CLI could actually run: ticks while its cgroup throttles it on memory do
/// not count, up to `cap` of wall time in all.
#[derive(Clone, Debug)]
pub struct StartClock {
    window: Duration,
    cap: Duration,
    elapsed: Duration,
    quiet: Duration,
    starved: Option<String>,
}

impl StartClock {
    pub fn new(window: Duration, cap: Duration) -> StartClock {
        StartClock { window, cap: cap.max(window), elapsed: Duration::ZERO, quiet: Duration::ZERO, starved: None }
    }

    /// `step` passed; `heard` = any stdout line so far; `starved` = the
    /// cgroup throttling it right now. `Some` = the wait is over.
    pub fn tick(&mut self, step: Duration, heard: bool, starved: Option<String>) -> Option<StartEnd> {
        self.elapsed += step;
        if heard {
            return (self.elapsed >= self.window).then_some(StartEnd::Late);
        }
        match starved {
            Some(cgroup) => self.starved = Some(cgroup),
            None => self.quiet += step,
        }
        (self.quiet >= self.window || self.elapsed >= self.cap)
            .then(|| StartEnd::Silent { starved: self.starved.clone() })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tree(name: &str) -> std::path::PathBuf {
        let root = std::env::temp_dir().join(format!("eagent-starved-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        root
    }

    fn cgroup(root: &Path, path: &str, current: &str, high: &str, full: &str) {
        let dir = root.join(path);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("memory.current"), current).unwrap();
        std::fs::write(dir.join("memory.high"), high).unwrap();
        std::fs::write(
            dir.join("memory.pressure"),
            format!("some avg10=0.00 avg60=0.00 avg300=0.00 total=0\nfull avg10={full} avg60=0.00 avg300=0.00 total=0\n"),
        )
        .unwrap();
    }

    /// The slice over its cap names the throttling, not the run's scope.
    #[test]
    fn the_slice_over_its_memory_high_is_named() {
        let root = tree("over");
        let scope = "user.slice/user@1000.service/expbg.slice/run-exp-s1-1x0.scope";
        cgroup(&root, "user.slice", "30000000000\n", "max\n", "0.00");
        cgroup(&root, "user.slice/user@1000.service", "20000000000\n", "max\n", "0.00");
        cgroup(&root, "user.slice/user@1000.service/expbg.slice", "17400000000\n", "17179869184\n", "79.40");
        cgroup(&root, scope, "300000000\n", "max\n", "83.00");
        assert_eq!(starved_under(&root, &format!("/{scope}")).as_deref(), Some("expbg.slice"));
        // Back under its cap and calm: nothing starves the start.
        cgroup(&root, "user.slice/user@1000.service/expbg.slice", "12000000000\n", "17179869184\n", "0.00");
        cgroup(&root, scope, "300000000\n", "max\n", "0.00");
        assert_eq!(starved_under(&root, &format!("/{scope}")), None);
        // The slice stalling on someone else's work: not this start's.
        cgroup(&root, "user.slice/user@1000.service/expbg.slice", "12000000000\n", "17179869184\n", "39.76");
        assert_eq!(starved_under(&root, &format!("/{scope}")), None);
        // Under its cap but stalling (reclaim holds it just below).
        cgroup(&root, scope, "300000000\n", "max\n", "12.50");
        assert_eq!(starved_under(&root, &format!("/{scope}")).as_deref(), Some("run-exp-s1-1x0.scope"));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn the_own_cgroup_is_read_off_proc() {
        assert_eq!(own_cgroup("0::/user.slice/x.scope\n"), Some("/user.slice/x.scope"));
        assert_eq!(own_cgroup("1:name=systemd:/x\n"), None);
        assert_eq!(starved_under(Path::new("/nonexistent-cgroup-root"), "/a/b"), None);
    }

    /// #FEED-296 regression: under memory throttling a claude start was
    /// killed after 90 s of silence it never had the memory to break, then
    /// again, and the start failed. Starved ticks do not count against the
    /// window; the cap still ends a start that never recovers.
    #[test]
    fn starved_time_does_not_count_against_the_startup_window() {
        let step = Duration::from_secs(5);
        let mut clock = StartClock::new(Duration::from_secs(90), Duration::from_secs(240));
        for _ in 0..30 {
            assert_eq!(clock.tick(step, false, Some("expbg.slice".into())), None, "150 s starved: still waiting");
        }
        for _ in 0..17 {
            assert_eq!(clock.tick(step, false, None), None);
        }
        // 240 s of wall time: the cap ends it, naming the throttling.
        assert_eq!(clock.tick(step, false, None), Some(StartEnd::Silent { starved: Some("expbg.slice".into()) }));

        let mut calm = StartClock::new(Duration::from_secs(90), Duration::from_secs(240));
        for _ in 0..17 {
            assert_eq!(calm.tick(step, false, None), None);
        }
        assert_eq!(calm.tick(step, false, None), Some(StartEnd::Silent { starved: None }), "90 s of real silence");

        let mut alive = StartClock::new(Duration::from_secs(90), Duration::from_secs(240));
        for _ in 0..17 {
            assert_eq!(alive.tick(step, true, None), None);
        }
        assert_eq!(alive.tick(step, true, None), Some(StartEnd::Late), "it spoke: slow, not wedged");
    }
}
