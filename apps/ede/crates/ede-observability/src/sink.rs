//! #EDE-185: the log's own thread. Every line (the tracing fmt layer's,
//! the `EXP_FRAME_STATS` summaries) is handed to it through a bounded
//! queue and written there, so no thread that logs ever waits for the
//! log's output: a file on a disk that is busy, a journald stream that fell
//! behind, a pipe nobody reads. The compositor's main loop waited 841 ms
//! in ext4's journal (`do_get_write_access`) for one stderr line that way,
//! and the person's desktop stood still with it.
//!
//! A line that finds the queue full is dropped and counted; the next line
//! written is "exponential-desktop: N log lines dropped" (at the latest
//! when the thread stops). [`Worker`]'s drop writes what is queued first,
//! waiting [`FLUSH_WAIT`] at most for a stuck output; [`Sink::flush`] does
//! the same before a `std::process::exit` or an `exec`, which skip that
//! drop.

use std::io::{self, Write};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, SyncSender, TrySendError};
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::Output;

/// Lines queued before new ones drop.
pub(crate) const QUEUE: usize = 2048;
/// How long a flush (or the stop) waits for an output that is stuck.
pub(crate) const FLUSH_WAIT: Duration = Duration::from_secs(1);
/// The writer thread's name.
pub(crate) const THREAD: &str = "ede-log";

enum What {
    Line(Vec<u8>),
    /// Answered once everything before it is written.
    Flush(mpsc::Sender<()>),
    /// The same, and then the thread ends.
    Stop(mpsc::Sender<()>),
}

struct Msg {
    /// Lines dropped right before this one.
    dropped: u64,
    what: What,
}

/// What every thread logs through: a line is queued, never written here.
#[derive(Clone)]
pub(crate) struct Sink {
    tx: SyncSender<Msg>,
    /// Dropped since the last line that made it into the queue.
    dropped: Arc<AtomicU64>,
    /// Where a line goes once the thread has ended (after the [`Worker`]'s
    /// drop, at exit): written in place.
    direct: Option<Output>,
}

/// Stops the writer thread at drop, once what is queued is written.
pub(crate) struct Worker(Sink);

/// The log's thread, writing to `output`.
pub(crate) fn start(output: Output) -> io::Result<(Sink, Worker)> {
    match output {
        Output::Stdout => start_with(io::stdout(), Some(output)),
        Output::Stderr => start_with(io::stderr(), Some(output)),
    }
}

pub(crate) fn start_with<W: Write + Send + 'static>(out: W, direct: Option<Output>) -> io::Result<(Sink, Worker)> {
    let (tx, rx) = mpsc::sync_channel(QUEUE);
    std::thread::Builder::new().name(THREAD.into()).spawn(move || run(rx, out))?;
    let sink = Sink { tx, dropped: Arc::default(), direct };
    Ok((sink.clone(), Worker(sink)))
}

/// The line that says how many were dropped.
pub(crate) fn dropped_line(n: u64) -> String {
    format!("exponential-desktop: {n} log line{} dropped\n", if n == 1 { "" } else { "s" })
}

/// `bytes`, after the count of the lines dropped before them.
fn write(out: &mut impl Write, dropped: u64, bytes: &[u8]) {
    if dropped > 0 {
        let _ = out.write_all(dropped_line(dropped).as_bytes());
    }
    let _ = out.write_all(bytes);
}

fn run(rx: Receiver<Msg>, mut out: impl Write) {
    while let Ok(Msg { dropped, what }) = rx.recv() {
        match what {
            What::Line(bytes) => write(&mut out, dropped, &bytes),
            What::Flush(done) => {
                write(&mut out, dropped, &[]);
                let _ = out.flush();
                let _ = done.send(());
            }
            What::Stop(done) => {
                write(&mut out, dropped, &[]);
                // What other threads logged after the stop was asked for.
                while let Ok(Msg { dropped, what }) = rx.try_recv() {
                    match what {
                        What::Line(bytes) => write(&mut out, dropped, &bytes),
                        What::Flush(answer) | What::Stop(answer) => {
                            write(&mut out, dropped, &[]);
                            let _ = answer.send(());
                        }
                    }
                }
                let _ = out.flush();
                // Gone before the answer: a line logged once the stop has
                // returned is written in place, never queued for nobody.
                drop(rx);
                let _ = done.send(());
                return;
            }
        }
    }
}

impl Sink {
    /// Queues `what` with the count dropped before it; a refused one puts
    /// the count back.
    fn send(&self, what: What) -> Result<(), TrySendError<Msg>> {
        let dropped = self.dropped.swap(0, Ordering::Relaxed);
        self.tx.try_send(Msg { dropped, what }).inspect_err(|e| {
            let (TrySendError::Full(msg) | TrySendError::Disconnected(msg)) = e;
            self.dropped.fetch_add(msg.dropped, Ordering::Relaxed);
        })
    }

    /// One whole line (or several), queued or dropped: never a write here
    /// while the thread runs.
    pub(crate) fn line(&self, bytes: Vec<u8>) {
        match self.send(What::Line(bytes)) {
            Ok(()) => {}
            Err(TrySendError::Full(_)) => {
                self.dropped.fetch_add(1, Ordering::Relaxed);
            }
            Err(TrySendError::Disconnected(Msg { what, .. })) => {
                let What::Line(bytes) = what else { return };
                let dropped = self.dropped.swap(0, Ordering::Relaxed);
                match self.direct {
                    Some(Output::Stdout) => write(&mut io::stdout().lock(), dropped, &bytes),
                    Some(Output::Stderr) => write(&mut io::stderr().lock(), dropped, &bytes),
                    None => {}
                }
            }
        }
    }

    /// Waits up to `wait` for everything queued so far to be written; false
    /// when the output stayed stuck that long.
    pub(crate) fn flush(&self, wait: Duration) -> bool {
        self.ask(wait, What::Flush)
    }

    fn ask(&self, wait: Duration, what: fn(mpsc::Sender<()>) -> What) -> bool {
        let deadline = Instant::now() + wait;
        let (done, answered) = mpsc::channel();
        let mut what = what(done);
        loop {
            match self.send(what) {
                Ok(()) => return answered.recv_timeout(deadline.saturating_duration_since(Instant::now())).is_ok(),
                Err(TrySendError::Full(msg)) if Instant::now() < deadline => {
                    what = msg.what;
                    std::thread::sleep(Duration::from_millis(1));
                }
                Err(TrySendError::Full(_)) => return false,
                // The thread has ended: nothing is queued.
                Err(TrySendError::Disconnected(_)) => return true,
            }
        }
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        self.0.ask(FLUSH_WAIT, What::Stop);
    }
}

/// The fmt layer's writer: each event is one `write_all` of a whole line.
impl io::Write for &Sink {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.line(buf.to_vec());
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for Sink {
    type Writer = &'a Sink;

    fn make_writer(&'a self) -> Self::Writer {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    /// An output that keeps what it is given.
    #[derive(Clone, Default)]
    struct Kept(Arc<Mutex<Vec<u8>>>);

    impl Kept {
        fn text(&self) -> String {
            String::from_utf8(self.0.lock().unwrap().clone()).unwrap()
        }
    }

    impl Write for Kept {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(buf);
            Ok(buf.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    /// An output stuck in its first write until the gate opens (a disk in
    /// jbd2, a pipe nobody reads), then like [`Kept`].
    struct Stuck {
        kept: Kept,
        gate: Option<mpsc::Receiver<()>>,
        /// Told when the first write is stuck.
        stuck: Option<mpsc::Sender<()>>,
    }

    impl Write for Stuck {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            if let Some(gate) = self.gate.take() {
                if let Some(stuck) = self.stuck.take() {
                    let _ = stuck.send(());
                }
                let _ = gate.recv();
            }
            self.kept.write(buf)
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    /// A sink whose output is stuck until the returned sender is used (or
    /// dropped); it returns once the thread is inside that first write.
    fn stuck_sink() -> (Sink, Worker, Kept, mpsc::Sender<()>) {
        let kept = Kept::default();
        let (open, gate) = mpsc::channel();
        let (stuck, is_stuck) = mpsc::channel();
        let (sink, worker) = start_with(Stuck { kept: kept.clone(), gate: Some(gate), stuck: Some(stuck) }, None).unwrap();
        sink.line(b"first\n".to_vec());
        is_stuck.recv_timeout(Duration::from_secs(10)).expect("the thread took the first line");
        (sink, worker, kept, open)
    }

    fn numbered(n: usize) -> Vec<u8> {
        format!("line {n:05} {}\n", "x".repeat(80)).into_bytes()
    }

    /// The numbers of the `line N` lines, in the order written.
    fn numbers(text: &str) -> Vec<usize> {
        text.lines().filter_map(|l| l.strip_prefix("line ")).map(|l| l[..5].parse().unwrap()).collect()
    }

    /// #EDE-185: with the output stuck, the thread that logs returns at
    /// once, every time; what does not fit is dropped and counted.
    #[test]
    fn a_stuck_output_never_holds_up_the_thread_that_logs() {
        let (sink, worker, kept, open) = stuck_sink();
        let lines = 20 * QUEUE;
        let (mut slowest, mut slow) = (Duration::ZERO, 0);
        for n in 0..lines {
            let at = Instant::now();
            sink.line(numbered(n));
            let took = at.elapsed();
            slowest = slowest.max(took);
            slow += usize::from(took > Duration::from_millis(5));
        }
        // A blocking write would never return here: the output stays
        // stuck until `open`. The calls take microseconds; the bounds
        // leave room for a loaded CI runner's preemptions.
        assert!(slowest < Duration::from_millis(250) && slow <= lines / 1000, "the slowest call took {slowest:?}, {slow} over 5 ms");
        assert!(kept.text().is_empty(), "nothing was written yet: {:?}", kept.text());
        let dropped = sink.dropped.load(Ordering::Relaxed) as usize;
        assert_eq!(dropped, lines - QUEUE, "a full queue drops the rest");
        drop(open);
        drop(worker);
        let text = kept.text();
        assert_eq!(numbers(&text), (0..QUEUE).collect::<Vec<_>>(), "what fit is written, in order");
        assert!(text.starts_with("first\n"), "{}", &text[..40]);
        assert!(text.ends_with(&dropped_line(dropped as u64)), "the count at the stop: {}", &text[text.len() - 200..]);
    }

    /// Lines arrive whole and in the order logged, from one thread and from
    /// several at once.
    #[test]
    fn lines_arrive_in_order() {
        let kept = Kept::default();
        let (sink, worker) = start_with(kept.clone(), None).unwrap();
        // Paced so the queue never fills: nothing may be dropped here.
        for n in 0..5000 {
            sink.line(numbered(n));
            if n % 256 == 255 {
                assert!(sink.flush(Duration::from_secs(10)));
            }
        }
        let threads: Vec<_> = (0..4)
            .map(|t| {
                let sink = sink.clone();
                std::thread::spawn(move || {
                    for n in 0..300 {
                        sink.line(format!("thread {t} {n:04}\n").into_bytes());
                    }
                })
            })
            .collect();
        for t in threads {
            t.join().unwrap();
        }
        drop(worker);
        let text = kept.text();
        assert_eq!(numbers(&text), (0..5000).collect::<Vec<_>>());
        assert!(!text.contains("dropped"), "nothing dropped");
        for t in 0..4 {
            let own: Vec<usize> = text.lines().filter_map(|l| l.strip_prefix(&format!("thread {t} "))).map(|n| n.parse().unwrap()).collect();
            assert_eq!(own, (0..300).collect::<Vec<_>>(), "thread {t}'s lines whole and in order");
        }
    }

    /// The worker's drop writes everything queued before it returns.
    #[test]
    fn the_guard_flushes_at_drop() {
        let (sink, worker, kept, open) = stuck_sink();
        for n in 0..50 {
            sink.line(numbered(n));
        }
        assert!(kept.text().is_empty(), "nothing written when the guard drops");
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(20));
            drop(open);
        });
        drop(worker);
        assert_eq!(numbers(&kept.text()), (0..50).collect::<Vec<_>>(), "all of it, before the drop returned");
        // After the stop a line is written in place (here: nowhere), never
        // queued for a thread that is gone.
        sink.line(numbered(50));
        assert_eq!(numbers(&kept.text()).len(), 50);
        assert!(sink.flush(Duration::from_millis(10)), "nothing waits on an ended thread");
    }

    /// #EDE-185: the count of dropped lines comes right before the next
    /// line written, exactly where the gap is.
    #[test]
    fn the_next_line_written_says_how_many_were_dropped() {
        let (sink, worker, kept, open) = stuck_sink();
        for n in 0..QUEUE + 1 {
            sink.line(numbered(n));
        }
        assert_eq!(sink.dropped.load(Ordering::Relaxed), 1);
        open.send(()).unwrap();
        assert!(sink.flush(Duration::from_secs(10)), "the output is free again");
        sink.line(b"after the gap\n".to_vec());
        drop(worker);
        let text = kept.text();
        let at = text.find("after the gap\n").expect("the next line");
        assert!(text[..at].ends_with(&format!("line {:05} {}\n{}", QUEUE - 1, "x".repeat(80), dropped_line(1))), "{}", &text[at - 200..at + 14]);
        assert_eq!(dropped_line(1), "exponential-desktop: 1 log line dropped\n");
        assert_eq!(dropped_line(7), "exponential-desktop: 7 log lines dropped\n");
        assert_eq!(text.matches("dropped").count(), 1, "one gap, one count");
        assert_eq!(sink.dropped.load(Ordering::Relaxed), 0);
    }

    /// A flush waits for the output and gives up on a stuck one in time.
    #[test]
    fn a_flush_gives_up_on_a_stuck_output() {
        let (sink, worker, _kept, open) = stuck_sink();
        let started = Instant::now();
        assert!(!sink.flush(Duration::from_millis(100)), "still stuck");
        let took = started.elapsed();
        assert!(took >= Duration::from_millis(100) && took < Duration::from_secs(5), "{took:?}");
        // A full queue too: the flush retries until its time is up.
        for n in 0..2 * QUEUE {
            sink.line(numbered(n));
        }
        assert!(!sink.flush(Duration::from_millis(50)));
        drop(open);
        assert!(sink.flush(Duration::from_secs(10)), "written once the output is free");
        drop(worker);
    }
}
