//! The log's own thread (#EDE-185). No line is written on the thread that
//! logs it: the tracing fmt layer and [`line`] (the `EXP_FRAME_STATS`
//! summaries) hand each line to a bounded queue, and one writer thread
//! (`ede-log`) writes them to stderr (stdout for the session), in order.
//!
//! A `write(2)` to a log file or a journald stream can wait in the
//! filesystem for a second while another process loads the disk (ext4's
//! jbd2, `do_get_write_access`): on the compositor's main thread that froze
//! the person's desktop for that second. Now only the writer thread waits.
//! A full queue drops the line instead of waiting; the drops are counted
//! and said in a line of their own ("N log lines dropped") where they
//! happened: before the next line that got through, or at the end.
//!
//! [`crate::Guard`]'s drop writes what is still queued (up to
//! [`EXIT_FLUSH`]), and so does every `exit()` through an `atexit` handler
//! (`std::process::exit` skips the guard's drop). Panics and native crashes
//! still write directly.

use std::io::{self, Write};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, SyncSender, TrySendError};
use std::sync::{Arc, Condvar, Mutex, OnceLock, PoisonError};
use std::time::{Duration, Instant};

use crate::Output;

/// Lines the queue holds while the output is slow (a few hundred KB).
pub(crate) const QUEUE: usize = 4096;
/// How long an exit waits for the queued lines.
pub(crate) const EXIT_FLUSH: Duration = Duration::from_secs(1);

/// The process's log thread, once [`install`]ed (`None`: it could not start).
static LOG: OnceLock<Option<Log>> = OnceLock::new();

/// One queued line and the drops just before it.
struct Entry {
    dropped: u64,
    line: Vec<u8>,
}

#[derive(Default)]
struct Shared {
    /// Lines dropped since the last one queued.
    dropped: AtomicU64,
    /// Entries queued so far.
    queued: AtomicU64,
    /// Entries the thread has written.
    written: Mutex<u64>,
    wrote: Condvar,
    /// What the last flush that gave up waited for: a later one waits only
    /// for lines queued since (the exit's after the guard's, not twice).
    abandoned: AtomicU64,
}

/// A handle on a log thread; the thread ends with the last handle.
#[derive(Clone)]
pub(crate) struct Log {
    tx: SyncSender<Entry>,
    shared: Arc<Shared>,
}

impl Log {
    /// A thread writing to `sink`, with `capacity` lines queued at most.
    pub(crate) fn start(sink: impl Write + Send + 'static, capacity: usize) -> io::Result<Log> {
        let (tx, rx) = mpsc::sync_channel(capacity);
        let shared = Arc::new(Shared::default());
        let thread = shared.clone();
        std::thread::Builder::new().name("ede-log".into()).spawn(move || drain(rx, sink, &thread))?;
        Ok(Log { tx, shared })
    }

    /// Queues `line` (whole lines, `\n`-terminated), or counts it dropped
    /// when the queue is full. Never waits.
    pub(crate) fn send(&self, line: Vec<u8>) {
        self.enqueue(line, 1);
    }

    /// `counts`: what the entry adds to the drops when it does not fit.
    fn enqueue(&self, line: Vec<u8>, counts: u64) {
        let dropped = self.shared.dropped.swap(0, Ordering::AcqRel);
        match self.tx.try_send(Entry { dropped, line }) {
            Ok(()) => {
                self.shared.queued.fetch_add(1, Ordering::AcqRel);
            }
            Err(TrySendError::Full(entry) | TrySendError::Disconnected(entry)) => {
                self.shared.dropped.fetch_add(entry.dropped + counts, Ordering::AcqRel);
            }
        }
    }

    /// Waits up to `timeout` until the lines queued so far are written and
    /// the drops said; `false` while the output stays stuck.
    pub(crate) fn flush(&self, timeout: Duration) -> bool {
        let deadline = Instant::now() + timeout;
        if !self.wait(self.shared.queued.load(Ordering::Acquire), deadline) {
            return false;
        }
        if self.shared.dropped.load(Ordering::Acquire) == 0 {
            return true;
        }
        // The drops after the last line, in a line of their own.
        self.enqueue(Vec::new(), 0);
        self.wait(self.shared.queued.load(Ordering::Acquire), deadline)
    }

    fn wait(&self, target: u64, deadline: Instant) -> bool {
        let written = self.shared.written.lock().unwrap_or_else(PoisonError::into_inner);
        if *written >= target {
            return true;
        }
        if target <= self.shared.abandoned.load(Ordering::Acquire) {
            return false;
        }
        let timeout = deadline.saturating_duration_since(Instant::now());
        let (written, _) = self.shared.wrote.wait_timeout_while(written, timeout, |written| *written < target).unwrap_or_else(PoisonError::into_inner);
        if *written < target {
            self.shared.abandoned.fetch_max(target, Ordering::AcqRel);
            return false;
        }
        true
    }

    /// Lines dropped and not said yet.
    #[cfg(test)]
    pub(crate) fn dropped(&self) -> u64 {
        self.shared.dropped.load(Ordering::Acquire)
    }
}

/// The line that says `n` lines were dropped.
fn dropped_line(n: u64) -> String {
    format!("exponential-desktop: {n} log line{} dropped\n", if n == 1 { "" } else { "s" })
}

/// The writer thread: every entry in order. A failed write is the
/// output's (a closed stderr), never a reason to stop.
fn drain(rx: Receiver<Entry>, mut sink: impl Write, shared: &Shared) {
    for entry in rx {
        if entry.dropped > 0 {
            let _ = sink.write_all(dropped_line(entry.dropped).as_bytes());
        }
        if !entry.line.is_empty() {
            let _ = sink.write_all(&entry.line);
        }
        let _ = sink.flush();
        *shared.written.lock().unwrap_or_else(PoisonError::into_inner) += 1;
        shared.wrote.notify_all();
    }
}

/// The fmt layer's writer: each event's text is queued as one entry.
pub(crate) struct Writer(pub(crate) Log);

impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for Writer {
    type Writer = Line<'a>;

    fn make_writer(&'a self) -> Line<'a> {
        Line { log: &self.0, buf: Vec::new() }
    }
}

/// One event's text, queued whole once the fmt layer has written it.
pub(crate) struct Line<'a> {
    log: &'a Log,
    buf: Vec<u8>,
}

impl Write for Line<'_> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.buf.extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl Drop for Line<'_> {
    fn drop(&mut self) {
        if !self.buf.is_empty() {
            self.log.send(std::mem::take(&mut self.buf));
        }
    }
}

/// The process's log thread, writing to `output` (the first call's).
/// `None` when no thread could start: the log writes directly then.
pub(crate) fn install(output: Output) -> Option<&'static Log> {
    install_with(|| match output {
        Output::Stdout => Log::start(io::stdout(), QUEUE),
        Output::Stderr => Log::start(io::stderr(), QUEUE),
    })
}

pub(crate) fn install_with(start: impl FnOnce() -> io::Result<Log>) -> Option<&'static Log> {
    LOG.get_or_init(|| {
        let log = start().ok()?;
        extern "C" {
            fn atexit(callback: extern "C" fn()) -> std::ffi::c_int;
        }
        // SAFETY: registers a plain function, which waits (bounded) on our
        // own queue and never unwinds.
        unsafe { atexit(flush_at_exit) };
        Some(log)
    })
    .as_ref()
}

/// The process's log thread, if [`install`]ed.
fn installed() -> Option<&'static Log> {
    LOG.get().and_then(Option::as_ref)
}

extern "C" fn flush_at_exit() {
    flush(EXIT_FLUSH);
}

/// Waits up to `timeout` for the process's queued lines.
pub(crate) fn flush(timeout: Duration) -> bool {
    installed().is_none_or(|log| log.flush(timeout))
}

/// A line of its own on the log (no level, time or target), through the
/// log's thread; on stderr directly before [`crate::init_with`].
pub fn line(args: std::fmt::Arguments<'_>) {
    match installed() {
        Some(log) => log.send(format!("{args}\n").into_bytes()),
        None => eprintln!("{args}"),
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::sync::mpsc::Sender;

    /// What the thread wrote.
    #[derive(Clone, Default)]
    struct Collect(Arc<Mutex<Vec<u8>>>);

    impl Collect {
        fn lines(&self) -> Vec<String> {
            String::from_utf8(self.0.lock().unwrap().clone()).unwrap().lines().map(str::to_owned).collect()
        }
    }

    impl Write for Collect {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(bytes);
            Ok(bytes.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    /// An output that takes nothing until it is let go: each write says
    /// it started, then waits until the test drops its go.
    struct Stuck {
        out: Collect,
        started: Sender<()>,
        go: Receiver<()>,
    }

    impl Write for Stuck {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            let _ = self.started.send(());
            let _ = self.go.recv();
            self.out.write(bytes)
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    /// A log on a [`Stuck`] output holding `first` in its write.
    fn stuck(capacity: usize, first: &str) -> (Log, Collect, Sender<()>) {
        let out = Collect::default();
        let (started, started_rx) = mpsc::channel();
        let (go, go_rx) = mpsc::channel();
        let log = Log::start(Stuck { out: out.clone(), started, go: go_rx }, capacity).unwrap();
        log.send(format!("{first}\n").into_bytes());
        started_rx.recv().unwrap();
        (log, out, go)
    }

    /// The median and the worst of `n` runs of `f`.
    pub(crate) fn timed(n: usize, mut f: impl FnMut(usize)) -> (Duration, Duration) {
        let mut took: Vec<Duration> = (0..n)
            .map(|i| {
                let at = Instant::now();
                f(i);
                at.elapsed()
            })
            .collect();
        took.sort();
        (took[n / 2], took[n - 1])
    }

    /// Generous for a loaded CI machine: a blocked write takes as long as
    /// the output is stuck (forever here), a queued line microseconds.
    pub(crate) fn assert_never_waited((median, worst): (Duration, Duration)) {
        assert!(median < Duration::from_millis(1) && worst < Duration::from_millis(250), "median {median:?}, worst {worst:?}");
    }

    /// #EDE-185: with stderr a pipe nobody reads, the thread that logs
    /// (through tracing's fmt layer, as `init_with` sets it up) never
    /// waits for it: the full pipe holds the writer thread, the full queue
    /// drops lines and counts them. Once someone reads, every line is
    /// there, in order, or counted.
    #[test]
    fn a_pipe_nobody_reads_never_blocks_the_logging_thread() {
        let (mut reader, pipe) = io::pipe().unwrap();
        let log = Log::start(pipe, 64).unwrap();
        let subscriber = tracing_subscriber::fmt().with_writer(Writer(log.clone())).with_ansi(false).finish();
        let sent = 20_000;
        // ~2.6 MB of lines against a 64 KiB pipe and 64 queued lines.
        let took = tracing::subscriber::with_default(subscriber, || timed(sent, |n| tracing::info!(n, "{}", "x".repeat(100))));
        eprintln!("{sent} lines into a pipe nobody reads: median {:?}, worst {:?}", took.0, took.1);
        assert_never_waited(took);
        let dropped = log.dropped();
        assert!(dropped > sent as u64 / 2, "{dropped} of {sent} dropped");

        let read = std::thread::spawn(move || {
            let mut text = String::new();
            io::Read::read_to_string(&mut reader, &mut text).unwrap();
            text
        });
        assert!(log.flush(Duration::from_secs(10)));
        drop(log);
        let text = read.join().unwrap();
        let logged: Vec<u64> = text.lines().filter(|l| l.contains(" INFO ")).map(|l| l.rsplit("n=").next().unwrap().parse().unwrap()).collect();
        let said: u64 = text.lines().filter_map(|l| l.strip_prefix("exponential-desktop: ")?.split_once(" log line")?.0.parse::<u64>().ok()).sum();
        assert_eq!(logged.len() as u64 + said, sent as u64, "{} written + {said} said dropped", logged.len());
        assert!(logged.windows(2).all(|w| w[0] < w[1]), "in order");
    }

    /// The writer stuck: a send still returns at once, and what does not
    /// fit is counted.
    #[test]
    fn a_stuck_writer_never_blocks_send() {
        let (log, _, _go) = stuck(8, "held");
        let took = timed(10_000, |n| log.send(format!("line {n}\n").into_bytes()));
        assert_never_waited(took);
        assert_eq!(log.dropped(), 10_000 - 8);
    }

    /// The writer free: every line, in order, none dropped.
    #[test]
    fn lines_arrive_in_order() {
        let out = Collect::default();
        let log = Log::start(out.clone(), QUEUE).unwrap();
        for n in 0..1000 {
            log.send(format!("line {n}\n").into_bytes());
        }
        assert!(log.flush(Duration::from_secs(5)));
        assert_eq!(out.lines(), (0..1000).map(|n| format!("line {n}")).collect::<Vec<_>>());
        assert_eq!(log.dropped(), 0);
    }

    /// #EDE-185: the drops are counted and said where they happened:
    /// before the next line that got through, or at the end by a flush.
    #[test]
    fn a_dropped_line_is_counted_and_said_in_the_next_line() {
        let (log, out, go) = stuck(4, "0");
        for n in 1..=7 {
            log.send(format!("{n}\n").into_bytes());
        }
        // 1-4 queued, 5-7 dropped.
        assert_eq!(log.dropped(), 3);
        drop(go);
        let deadline = Instant::now() + Duration::from_secs(5);
        while out.lines().len() < 5 && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(1));
        }
        log.send(b"8\n".to_vec());
        assert!(log.flush(Duration::from_secs(5)));
        assert_eq!(out.lines(), ["0", "1", "2", "3", "4", "exponential-desktop: 3 log lines dropped", "8"]);
        assert_eq!(log.dropped(), 0);

        // No line after the drops: the flush says them.
        let (log, out, go) = stuck(1, "a");
        log.send(b"b\n".to_vec());
        log.send(b"c\n".to_vec());
        assert_eq!(log.dropped(), 1);
        drop(go);
        assert!(log.flush(Duration::from_secs(5)));
        assert_eq!(out.lines(), ["a", "b", "exponential-desktop: 1 log line dropped"]);
    }

    /// The guard's drop writes what is still queued.
    #[test]
    fn the_guard_flushes_at_drop() {
        struct Slow(Collect);
        impl Write for Slow {
            fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
                std::thread::sleep(Duration::from_millis(5));
                self.0.write(bytes)
            }
            fn flush(&mut self) -> io::Result<()> {
                Ok(())
            }
        }
        let out = Collect::default();
        let log = Log::start(Slow(out.clone()), QUEUE).unwrap();
        for n in 0..10 {
            log.send(format!("line {n}\n").into_bytes());
        }
        assert!(out.lines().len() < 10, "still queued");
        drop(crate::Guard { sentry: None, log: Some(log) });
        assert_eq!(out.lines(), (0..10).map(|n| format!("line {n}")).collect::<Vec<_>>());
    }

    /// A flush gives up at its timeout while the output is stuck, and the
    /// next one (the exit's after the guard's) does not wait again for the
    /// same lines.
    #[test]
    fn a_stuck_flush_gives_up_once() {
        let (log, _, _go) = stuck(8, "held");
        log.send(b"queued\n".to_vec());
        let at = Instant::now();
        assert!(!log.flush(Duration::from_millis(100)));
        assert!(at.elapsed() >= Duration::from_millis(100));
        let at = Instant::now();
        assert!(!log.flush(Duration::from_secs(5)));
        assert!(at.elapsed() < Duration::from_secs(1), "{:?}", at.elapsed());
    }
}
