//! #EDE-162: what the spaces cost the thread that serves the person's
//! frames. With `EXP_FRAME_STATS=1` every space tick and capture pass is
//! timed in thread CPU (what that thread spent, not the wall clock a
//! loaded machine stretches), and the loop's own lateness is sampled; a
//! line goes to stderr every 5 s, written by the log's thread
//! (`ede_observability::line`, #EDE-185). These numbers stay apart from the
//! person's frame times (`ede_observability`'s histogram and Sentry's
//! frames): space work is not a frame of the person's.

use std::time::{Duration, Instant};

use smithay::reexports::rustix::time::{clock_gettime, ClockId};

const PRINT_EVERY: Duration = Duration::from_secs(5);
/// How often the loop's lateness is sampled while stats are on.
pub const PROBE: Duration = Duration::from_millis(10);

/// This thread's CPU time so far.
pub fn thread_cpu() -> Duration {
    let t = clock_gettime(ClockId::ThreadCPUTime);
    Duration::new(t.tv_sec as u64, t.tv_nsec as u32)
}

pub(crate) struct Stats {
    pub on: bool,
    window: Instant,
    /// µs of thread CPU per tick.
    ticks: Vec<u32>,
    /// µs of thread CPU and of wall time per capture pass (on the GPU: its
    /// submit).
    passes: Vec<(u32, u32)>,
    /// GPU passes: µs of thread CPU to read one back, µs from submit to
    /// its fence.
    gpu: Vec<(u32, u32)>,
    /// Captures that waited for their turn.
    waits: u32,
    /// Commits a space's app made faster than its clock: held for a tick.
    paced: u32,
    /// µs a timer of the loop fired late.
    lateness: Vec<u32>,
}

impl Default for Stats {
    fn default() -> Self {
        let on = std::env::var("EXP_FRAME_STATS").is_ok_and(|v| v.trim() == "1");
        Self { on, window: Instant::now(), ticks: Vec::new(), passes: Vec::new(), gpu: Vec::new(), waits: 0, paced: 0, lateness: Vec::new() }
    }
}

fn micros(d: Duration) -> u32 {
    d.as_micros().min(u32::MAX as u128) as u32
}

impl Stats {
    pub fn tick(&mut self, cpu: Duration) {
        if self.on {
            self.ticks.push(micros(cpu));
            self.flush();
        }
    }

    pub fn pass(&mut self, cpu: Duration, wall: Duration) {
        if self.on {
            self.passes.push((micros(cpu), micros(wall)));
            self.flush();
        }
    }

    #[cfg_attr(not(feature = "gpu"), allow(dead_code))]
    pub fn gpu_done(&mut self, cpu: Duration, latency: Duration) {
        if self.on {
            self.gpu.push((micros(cpu), micros(latency)));
            self.flush();
        }
    }

    pub fn paced(&mut self) {
        if self.on {
            self.paced += 1;
        }
    }

    pub fn waited(&mut self) {
        if self.on {
            self.waits += 1;
        }
    }

    pub fn late(&mut self, by: Duration) {
        if self.on {
            self.lateness.push(micros(by));
            self.flush();
        }
    }

    fn flush(&mut self) {
        let over = self.window.elapsed();
        if over < PRINT_EVERY {
            return;
        }
        ede_observability::line(&format!("exponential-desktop: {}", self.summary(over)));
        let on = self.on;
        *self = Self { on, ..Self::default() };
    }

    /// The stderr line of one window.
    pub fn summary(&self, over: Duration) -> String {
        let secs = over.as_secs_f32().max(0.001);
        let cpu: Vec<u32> = self.passes.iter().map(|p| p.0).collect();
        let wall: Vec<u32> = self.passes.iter().map(|p| p.1).collect();
        let sum = |v: &[u32]| v.iter().map(|&x| x as u64).sum::<u64>() as f32 / 1000.0;
        let read: Vec<u32> = self.gpu.iter().map(|p| p.0).collect();
        let latency: Vec<u32> = self.gpu.iter().map(|p| p.1).collect();
        format!(
            "space stats {secs:.1}s: {} ticks ({:.1}/s) cpu {}; {} captures cpu {}, wall {} ({:.1} ms/s); {} on the GPU: read cpu {}, fence after {}; {} waited; {} commits paced; loop late {}",
            self.ticks.len(),
            self.ticks.len() as f32 / secs,
            spread(&self.ticks),
            self.passes.len(),
            spread(&cpu),
            spread(&wall),
            (sum(&cpu) + sum(&read)) / secs,
            self.gpu.len(),
            spread(&read),
            spread(&latency),
            self.waits,
            self.paced,
            spread(&self.lateness),
        )
    }
}

/// `p50 / p99 / max` of µs samples, exact (a window holds a few thousand).
pub fn spread(samples: &[u32]) -> String {
    if samples.is_empty() {
        return "-".into();
    }
    let mut v = samples.to_vec();
    v.sort_unstable();
    let at = |p: f32| v[(((p / 100.0) * v.len() as f32).ceil() as usize).clamp(1, v.len()) - 1];
    format!("p50 {} p99 {} max {} µs", at(50.0), at(99.0), v[v.len() - 1])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_spread_is_exact_and_a_window_names_ticks_captures_and_waits() {
        assert_eq!(spread(&[]), "-");
        assert_eq!(spread(&[7]), "p50 7 p99 7 max 7 µs");
        let hundred: Vec<u32> = (1..=100).collect();
        assert_eq!(spread(&hundred), "p50 50 p99 99 max 100 µs");
        let s = Stats { on: true, window: Instant::now(), ticks: vec![10, 20], passes: vec![(1500, 2000), (2500, 3000)], gpu: vec![(200, 900)], waits: 3, paced: 5, lateness: vec![40] };
        let line = s.summary(Duration::from_secs(2));
        assert!(line.starts_with("space stats 2.0s: 2 ticks (1.0/s) cpu p50 10 p99 20 max 20 µs; 2 captures cpu p50 1500 p99 2500 max 2500 µs"), "{line}");
        assert!(line.contains("(2.1 ms/s); 1 on the GPU: read cpu p50 200 p99 200 max 200 µs, fence after p50 900"), "{line}");
        assert!(line.contains("; 3 waited; 5 commits paced; loop late p50 40"), "{line}");
    }

    /// #EDE-185: the compositor never writes stderr or stdout itself (a
    /// write there held its loop 841 ms on a busy disk): its lines go
    /// through tracing or `ede_observability::line`, which the log's thread
    /// writes. Allowed: what runs before the loop or instead of it (main's
    /// arguments and last error, the doctor's report) and the `space-*`
    /// dispatches, which are other processes.
    #[test]
    fn the_compositor_never_writes_its_output_itself() {
        let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let allowed = ["main.rs", "doctor.rs", "spaces/slice.rs"];
        let writes = ["eprintln!", "println!", "eprint!", "print!(", "dbg!(", "io::stderr()", "io::stdout()"];
        let (mut found, mut files, mut dirs) = (Vec::new(), 0, vec![src.clone()]);
        while let Some(dir) = dirs.pop() {
            for entry in std::fs::read_dir(&dir).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    dirs.push(path);
                    continue;
                }
                let rel = path.strip_prefix(&src).unwrap().to_string_lossy().into_owned();
                if !rel.ends_with(".rs") || allowed.contains(&rel.as_str()) {
                    continue;
                }
                files += 1;
                let source = std::fs::read_to_string(&path).unwrap();
                // A file's tests, at its end, print what they like.
                let code = source.split("#[cfg(test)]\nmod tests").next().unwrap();
                for (i, line) in code.lines().enumerate() {
                    let line = line.trim_start();
                    if !line.starts_with("//") && writes.iter().any(|w| line.contains(w)) {
                        found.push(format!("{rel}:{}: {line}", i + 1));
                    }
                }
            }
        }
        assert!(files > 50, "the compositor's sources: {files}");
        assert!(found.is_empty(), "written on the compositor's own threads:\n{}", found.join("\n"));
    }

    #[test]
    fn thread_cpu_moves_with_work_on_this_thread() {
        let before = thread_cpu();
        let mut x = 0u64;
        for i in 0..2_000_000u64 {
            x = x.wrapping_mul(31).wrapping_add(i);
        }
        std::hint::black_box(x);
        assert!(thread_cpu() > before);
    }
}
