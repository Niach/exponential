//! #EDE-185: a process whose stderr nobody reads (a pipe here; a file on a
//! busy disk or a journald stream that fell behind does the same) never
//! waits for it on the thread that logs. The compositor's main loop did:
//! 841 ms in ext4's journal for one `EXP_FRAME_STATS` line.
//!
//! The test runs itself again as the logging process, through the real
//! `init_with`, with stderr a pipe it does not read until that process has
//! logged and timed everything.

use std::io::{BufRead, BufReader, Read, Write};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

const LOGGING_PROCESS: &str = "EDE185_LOGGING_PROCESS";
const LINES: usize = 20_000;
/// A call over this is slow (preemption on a loaded runner, at most a few
/// in a run); none may come near a blocked write.
const SLOW: Duration = Duration::from_millis(5);
const BLOCKED: Duration = Duration::from_millis(250);

#[test]
fn a_stderr_nobody_reads_never_holds_up_the_thread_that_logs() {
    if std::env::var_os(LOGGING_PROCESS).is_some() {
        return logging_process();
    }
    let home = tempfile::tempdir().unwrap();
    let mut command = Command::new(std::env::current_exe().unwrap());
    command
        .args(["--exact", "a_stderr_nobody_reads_never_holds_up_the_thread_that_logs", "--nocapture", "--test-threads=1"])
        .env(LOGGING_PROCESS, "1")
        // No account, no cache, no network: diagnostics off, the frame
        // summary on.
        .env("HOME", home.path())
        .env("XDG_CACHE_HOME", home.path().join("cache"))
        .env("XDG_CONFIG_HOME", home.path().join("config"))
        .env("XDG_DATA_HOME", home.path().join("data"))
        .env("EXP_DIAGNOSTICS", "0")
        .env("EXP_FRAME_STATS", "1")
        .env("RUST_LOG", "info")
        .env("NO_COLOR", "1")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for var in ["EXP_DATA_DIR", "EXP_SENTRY_DSN", "SENTRY_ENVIRONMENT", "SENTRY_TRACES_SAMPLE_RATE"] {
        command.env_remove(var);
    }
    let mut child = command.spawn().unwrap();
    let (said, says) = mpsc::channel::<String>();
    let stdout = BufReader::new(child.stdout.take().unwrap());
    std::thread::spawn(move || {
        // libtest's "test … … " comes first on the line.
        for line in stdout.lines().map_while(Result::ok) {
            if let Some(at) = line.find("EDE185 ") {
                if said.send(line[at..].to_owned()).is_err() {
                    break;
                }
            }
        }
    });
    let mut stderr = child.stderr.take().unwrap();

    // Nobody reads its stderr: a write there blocks once the pipe is full.
    let timed = match says.recv_timeout(Duration::from_secs(60)) {
        Ok(line) => line,
        Err(_) => {
            let _ = child.kill();
            panic!("the logging process never got through its lines: a write to stderr held it up");
        }
    };
    let field = |name: &str| -> u64 { timed.split_whitespace().find_map(|kv| kv.strip_prefix(name)?.strip_prefix('=')?.parse().ok()).unwrap_or_else(|| panic!("{name} in {timed}")) };
    eprintln!("{timed}");
    let blocked = BLOCKED.as_micros() as u64;
    assert!(field("slowest_line_us") < blocked && field("slowest_frame_us") < blocked, "{timed}");
    assert!(field("slow_lines") <= (LINES / 1000) as u64 && field("slow_frames") <= field("frames") / 100, "{timed}");

    // Now read it, and let the process drop its guard and end.
    let reader = std::thread::spawn(move || {
        let mut text = String::new();
        stderr.read_to_string(&mut text).unwrap();
        text
    });
    let mut stdin = child.stdin.take().unwrap();
    stdin.write_all(b"go\n").unwrap();
    drop(stdin);
    let done = says.recv_timeout(Duration::from_secs(60));
    let deadline = Instant::now() + Duration::from_secs(60);
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break status;
        }
        if Instant::now() > deadline {
            let _ = child.kill();
            panic!("the logging process did not end");
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    let text = reader.join().unwrap();
    assert!(status.success() && done.as_deref() == Ok("EDE185 done"), "{status:?} {done:?}\n{}", &text[text.len().saturating_sub(2000)..]);

    // What fit is there, whole and in order; the rest is counted once, at
    // the end (the guard's drop): the lines and the one frame summary.
    let numbers: Vec<usize> = text.lines().filter_map(|l| Some(l[l.find("ede185 line ")? + 12..][..5].parse().unwrap())).collect();
    assert!(numbers.len() > 100, "the pipe and the queue held some: {}", numbers.len());
    assert_eq!(numbers, (0..numbers.len()).collect::<Vec<_>>(), "in order, none missing before the gap");
    let last = text.lines().last().unwrap_or_default();
    let dropped: usize = last.strip_prefix("exponential-desktop: ").and_then(|l| l.strip_suffix(" log lines dropped")).and_then(|n| n.parse().ok()).unwrap_or_else(|| panic!("the count comes last: {last:?}"));
    assert_eq!(numbers.len() + dropped, LINES + 1, "every line written or counted, the frame summary too");
    assert_eq!(text.matches("dropped").count(), 1);
}

/// The process under test: logs and times, then waits for "go" on stdin.
fn logging_process() {
    let guard = ede_observability::init_with("test", "info", ede_observability::Output::Stderr, ede_observability::Trail::Lines);
    let pad = "x".repeat(100);
    let (mut slowest_line, mut slow_lines) = (Duration::ZERO, 0);
    for n in 0..LINES {
        let at = Instant::now();
        tracing::info!("ede185 line {n:05} {pad}");
        let took = at.elapsed();
        slowest_line = slowest_line.max(took);
        slow_lines += usize::from(took > SLOW);
    }
    // Over 5 s of frames: one `EXP_FRAME_STATS` summary goes out.
    let (mut slowest_frame, mut slow_frames, mut frames) = (Duration::ZERO, 0, 0);
    let started = Instant::now();
    while started.elapsed() < Duration::from_millis(5200) {
        let at = Instant::now();
        ede_observability::record_frame("test", Duration::from_millis(2));
        let took = at.elapsed();
        slowest_frame = slowest_frame.max(took);
        slow_frames += usize::from(took > SLOW);
        frames += 1;
        std::thread::sleep(Duration::from_millis(2));
    }
    println!(
        "EDE185 timed lines={LINES} slowest_line_us={} slow_lines={slow_lines} frames={frames} slowest_frame_us={} slow_frames={slow_frames}",
        slowest_line.as_micros(),
        slowest_frame.as_micros()
    );
    std::io::stdout().flush().unwrap();
    let mut go = String::new();
    std::io::stdin().read_line(&mut go).unwrap();
    drop(guard);
    println!("EDE185 done");
}
