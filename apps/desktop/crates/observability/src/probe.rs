//! #LOGS-3: WHY the main thread stalled, read from outside it.
//!
//! A hang's span stacks say where the main thread was, not why it did not
//! move: the same `sync.apply` that takes 2 ms on one machine took seconds
//! on another (a busy CPU, a disk it waited on, pages it faulted back in, a
//! lock). The watchdog reads the main thread's own counters at the hang's
//! edges and its scheduler state at every sample:
//! - Linux: `/proc/self/task/<tid>/stat` (state, major faults, CPU time) and
//!   the machine's load, memory, swap and pressure (`/proc/pressure/*`);
//! - macOS: `thread_info` (state, CPU time); no fault or pressure counters.
//!
//! Elsewhere every read is `None` and the report carries none of it.

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

/// The main thread's OS id (Linux tid / mach port), 0 = not marked.
static MAIN: AtomicU64 = AtomicU64::new(0);

/// Called on the main thread ([`crate::lag::mark_main_thread`]).
pub(crate) fn mark_main_thread() {
    MAIN.store(current_thread_id(), Ordering::Relaxed);
}

/// The scheduler's word for a thread at one instant.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ThreadState {
    /// On a CPU or waiting for one.
    Running,
    /// Asleep: a lock, a condvar, a pipe, the event loop.
    Waiting,
    /// Uninterruptible: the kernel is doing I/O for it (a disk read, a page
    /// fault served from disk or swap).
    Disk,
    Other,
}

impl ThreadState {
    fn letter(self) -> &'static str {
        match self {
            ThreadState::Running => "R",
            ThreadState::Waiting => "S",
            ThreadState::Disk => "D",
            ThreadState::Other => "?",
        }
    }
}

/// The main thread's counters at one instant.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ThreadCounters {
    /// User + system CPU time.
    pub cpu: Duration,
    /// Major page faults (served from disk); `None` where not counted.
    pub major_faults: Option<u64>,
}

/// One sample: the state and the counters.
pub fn sample_main() -> Option<(ThreadState, ThreadCounters)> {
    let id = MAIN.load(Ordering::Relaxed);
    (id != 0).then(|| read_thread(id)).flatten()
}

/// The machine around the process (Linux).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SystemSample {
    pub load1: Option<f64>,
    pub cpus: usize,
    pub mem_available_mb: Option<u64>,
    pub swap_used_mb: Option<u64>,
    /// `/proc/pressure/{cpu,io,memory}` "some avg10" (% of the last 10 s
    /// some task stalled on it).
    pub pressure: Vec<(&'static str, f64)>,
}

pub fn sample_system() -> SystemSample {
    let mut sample = SystemSample { cpus: std::thread::available_parallelism().map_or(1, |n| n.get()), ..Default::default() };
    if let Ok(load) = std::fs::read_to_string("/proc/loadavg") {
        sample.load1 = load.split_whitespace().next().and_then(|v| v.parse().ok());
    }
    if let Ok(meminfo) = std::fs::read_to_string("/proc/meminfo") {
        let kb = |key: &str| meminfo.lines().find_map(|line| line.strip_prefix(key)?.split_whitespace().next()?.parse::<u64>().ok());
        sample.mem_available_mb = kb("MemAvailable:").map(|v| v / 1024);
        if let (Some(total), Some(free)) = (kb("SwapTotal:"), kb("SwapFree:")) {
            sample.swap_used_mb = Some(total.saturating_sub(free) / 1024);
        }
    }
    for kind in ["cpu", "io", "memory"] {
        if let Some(avg10) = std::fs::read_to_string(format!("/proc/pressure/{kind}")).ok().as_deref().and_then(pressure_some_avg10) {
            sample.pressure.push((kind, avg10));
        }
    }
    sample
}

/// `some avg10=12.34 avg60=… total=…` → 12.34. Pure.
fn pressure_some_avg10(text: &str) -> Option<f64> {
    let line = text.lines().find(|line| line.starts_with("some "))?;
    line.split_whitespace().find_map(|field| field.strip_prefix("avg10=")?.parse().ok())
}

/// What the probe saw over one hang.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct HangProbe {
    pub wall: Duration,
    /// The main thread's CPU time inside the hang.
    pub cpu: Option<Duration>,
    pub major_faults: Option<u64>,
    /// Samples per state.
    pub states: Vec<(ThreadState, u32)>,
    pub system: SystemSample,
}

impl HangProbe {
    /// The likeliest reason, from the counters. Pure.
    pub fn cause(&self) -> &'static str {
        let total: u32 = self.states.iter().map(|(_, n)| n).sum();
        if total == 0 && self.cpu.is_none() {
            return "unknown";
        }
        let share = |state: ThreadState| self.states.iter().find(|(s, _)| *s == state).map_or(0., |(_, n)| *n as f64 / total.max(1) as f64);
        let cpu_share = self.cpu.map(|cpu| cpu.as_secs_f64() / self.wall.as_secs_f64().max(1e-3));
        // Faults served from disk: a page every few ms or more is swap or a
        // cold mapping.
        let faulting = self.major_faults.is_some_and(|n| n as f64 >= 100. * self.wall.as_secs_f64());
        if share(ThreadState::Disk) >= 0.3 || faulting {
            return if faulting { "paging" } else { "disk" };
        }
        if share(ThreadState::Waiting) >= 0.5 {
            return "blocked";
        }
        match cpu_share {
            Some(cpu) if cpu >= 0.6 => "busy",
            Some(_) => "starved",
            None => "unknown",
        }
    }

    /// One line for the event: `cpu 310 of 2200 ms · R 40 S 2 D 60 · major faults 812`.
    pub fn thread_line(&self) -> String {
        let mut parts = Vec::new();
        if let Some(cpu) = self.cpu {
            parts.push(format!("cpu {:.0} of {:.0} ms", cpu.as_secs_f64() * 1e3, self.wall.as_secs_f64() * 1e3));
        }
        if !self.states.is_empty() {
            parts.push(self.states.iter().map(|(state, n)| format!("{} {n}", state.letter())).collect::<Vec<_>>().join(" "));
        }
        if let Some(faults) = self.major_faults {
            parts.push(format!("major faults {faults}"));
        }
        parts.join(" · ")
    }

    /// `load 31.2/16 · mem 812 MB free · swap 6100 MB · pressure cpu 40 io 72 memory 55`.
    pub fn system_line(&self) -> String {
        let system = &self.system;
        let mut parts = Vec::new();
        if let Some(load) = system.load1 {
            parts.push(format!("load {load:.1}/{}", system.cpus));
        }
        if let Some(mb) = system.mem_available_mb {
            parts.push(format!("mem {mb} MB free"));
        }
        if let Some(mb) = system.swap_used_mb {
            parts.push(format!("swap {mb} MB"));
        }
        if !system.pressure.is_empty() {
            parts.push(format!("pressure {}", system.pressure.iter().map(|(kind, v)| format!("{kind} {v:.0}")).collect::<Vec<_>>().join(" ")));
        }
        parts.join(" · ")
    }
}

/// Accumulates a hang's samples (the watchdog's side).
#[derive(Debug, Default)]
pub struct HangProbeBuilder {
    start: Option<ThreadCounters>,
    last: Option<ThreadCounters>,
    states: std::collections::BTreeMap<ThreadState, u32>,
}

impl HangProbeBuilder {
    pub fn sample(&mut self) {
        let Some((state, counters)) = sample_main() else { return };
        self.start.get_or_insert(counters);
        self.last = Some(counters);
        *self.states.entry(state).or_default() += 1;
    }

    pub fn finish(mut self, wall: Duration) -> HangProbe {
        // The counters as the main thread came back.
        if let Some((_, counters)) = sample_main() {
            self.last = Some(counters);
        }
        let (cpu, major_faults) = match (self.start, self.last) {
            (Some(start), Some(last)) => (
                Some(last.cpu.saturating_sub(start.cpu)),
                last.major_faults.zip(start.major_faults).map(|(last, start)| last.saturating_sub(start)),
            ),
            _ => (None, None),
        };
        let mut states: Vec<(ThreadState, u32)> = self.states.into_iter().collect();
        states.sort_by(|a, b| b.1.cmp(&a.1));
        HangProbe { wall, cpu, major_faults, states, system: sample_system() }
    }
}

// ---------------------------------------------------------------------------
// Per platform
// ---------------------------------------------------------------------------

#[cfg(target_os = "linux")]
fn current_thread_id() -> u64 {
    // SAFETY: gettid has no preconditions.
    unsafe { libc::syscall(libc::SYS_gettid) as u64 }
}

#[cfg(target_os = "linux")]
fn read_thread(tid: u64) -> Option<(ThreadState, ThreadCounters)> {
    let stat = std::fs::read_to_string(format!("/proc/self/task/{tid}/stat")).ok()?;
    // SAFETY: sysconf has no preconditions.
    let ticks = unsafe { libc::sysconf(libc::_SC_CLK_TCK) }.max(1) as u64;
    parse_stat(&stat, ticks)
}

/// `/proc/<pid>/task/<tid>/stat`: the fields after the `(comm)` (which may
/// hold spaces) — state, then majflt at 12, utime 14, stime 15 (1-based,
/// counting pid as 1). Pure.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn parse_stat(stat: &str, ticks_per_second: u64) -> Option<(ThreadState, ThreadCounters)> {
    let rest = &stat[stat.rfind(')')? + 1..];
    let fields: Vec<&str> = rest.split_whitespace().collect();
    // fields[0] = state (field 3), so field n = fields[n - 3].
    let state = match fields.first()? {
        &"R" => ThreadState::Running,
        &"S" | &"I" => ThreadState::Waiting,
        &"D" => ThreadState::Disk,
        _ => ThreadState::Other,
    };
    let field = |n: usize| fields.get(n - 3)?.parse::<u64>().ok();
    let cpu_ticks = field(14)? + field(15)?;
    let cpu = Duration::from_micros(cpu_ticks * 1_000_000 / ticks_per_second);
    Some((state, ThreadCounters { cpu, major_faults: field(12) }))
}

#[cfg(target_os = "macos")]
fn current_thread_id() -> u64 {
    // SAFETY: both calls take the calling thread's own handle.
    unsafe { libc::pthread_mach_thread_np(libc::pthread_self()) as u64 }
}

#[cfg(target_os = "macos")]
fn read_thread(port: u64) -> Option<(ThreadState, ThreadCounters)> {
    let mut info = std::mem::MaybeUninit::<libc::thread_basic_info>::zeroed();
    let mut count = (std::mem::size_of::<libc::thread_basic_info>() / std::mem::size_of::<libc::integer_t>()) as libc::mach_msg_type_number_t;
    // SAFETY: `info` is a properly sized out-buffer and `count` its length
    // in `integer_t`s, as `thread_info(THREAD_BASIC_INFO)` requires.
    let status = unsafe { libc::thread_info(port as libc::thread_inspect_t, libc::THREAD_BASIC_INFO as libc::thread_flavor_t, info.as_mut_ptr().cast(), &mut count) };
    if status != 0 {
        return None;
    }
    // SAFETY: a successful call filled it.
    let info = unsafe { info.assume_init() };
    let time = |t: libc::time_value_t| Duration::from_secs(t.seconds as u64) + Duration::from_micros(t.microseconds as u64);
    let state = match info.run_state {
        libc::TH_STATE_RUNNING => ThreadState::Running,
        libc::TH_STATE_WAITING => ThreadState::Waiting,
        libc::TH_STATE_UNINTERRUPTIBLE => ThreadState::Disk,
        _ => ThreadState::Other,
    };
    Some((state, ThreadCounters { cpu: time(info.user_time) + time(info.system_time), major_faults: None }))
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
fn current_thread_id() -> u64 {
    0
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
fn read_thread(_: u64) -> Option<(ThreadState, ThreadCounters)> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_stat_line_reads_state_faults_and_cpu() {
        // comm with a space and parentheses, as threads name themselves.
        let stat = "4242 (exp (main) x) D 1 4242 4242 0 -1 4194304 9000 0 812 0 250 50 0 0 20 0 40 0 100 0 0";
        let (state, counters) = parse_stat(stat, 100).unwrap();
        assert_eq!(state, ThreadState::Disk);
        assert_eq!(counters.major_faults, Some(812));
        assert_eq!(counters.cpu, Duration::from_millis(3000));
    }

    #[test]
    fn pressure_reads_some_avg10() {
        let text = "some avg10=12.50 avg60=3.00 avg300=1.00 total=123\nfull avg10=1.00 avg60=0.00 avg300=0.00 total=9\n";
        assert_eq!(pressure_some_avg10(text), Some(12.5));
        assert_eq!(pressure_some_avg10("garbage"), None);
    }

    #[test]
    fn the_cause_follows_the_counters() {
        let wall = Duration::from_secs(2);
        let probe = |cpu_ms: u64, faults: Option<u64>, states: Vec<(ThreadState, u32)>| HangProbe { wall, cpu: Some(Duration::from_millis(cpu_ms)), major_faults: faults, states, system: SystemSample::default() };
        assert_eq!(probe(1900, Some(0), vec![(ThreadState::Running, 90)]).cause(), "busy");
        assert_eq!(probe(150, Some(0), vec![(ThreadState::Running, 90)]).cause(), "starved");
        assert_eq!(probe(150, Some(3), vec![(ThreadState::Disk, 60), (ThreadState::Running, 30)]).cause(), "disk");
        assert_eq!(probe(150, Some(4000), vec![(ThreadState::Running, 90)]).cause(), "paging");
        assert_eq!(probe(20, Some(0), vec![(ThreadState::Waiting, 90)]).cause(), "blocked");
        let line = probe(150, Some(3), vec![(ThreadState::Disk, 60), (ThreadState::Running, 30)]).thread_line();
        assert_eq!(line, "cpu 150 of 2000 ms · D 60 R 30 · major faults 3");
    }

    /// The probe reads this very thread on the platforms it supports.
    #[test]
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    fn the_probe_reads_a_live_thread() {
        let id = current_thread_id();
        assert_ne!(id, 0);
        let started = std::time::Instant::now();
        while started.elapsed() < Duration::from_millis(30) {
            std::hint::black_box(0u64.wrapping_add(1));
        }
        let (state, counters) = read_thread(id).expect("a reading");
        assert_eq!(state, ThreadState::Running);
        assert!(counters.cpu >= Duration::from_millis(10), "{counters:?}");
    }
}
