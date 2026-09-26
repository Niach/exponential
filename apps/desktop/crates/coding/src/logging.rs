//! EXP-1099: the desktop's and the CLI's ONE logger, plus the heartbeat's
//! failure policy both device loops share.
//!
//! * [`RotatingFile`]: a hand-rolled, size-capped appender
//!   (`{data_dir}/logs/<name>`, rotated at [`MAX_LOG_BYTES`] into `.1..=.3`).
//!   A failed open never panics: the logger simply has no file.
//! * [`ExpLogger`]: the `log::Log` both binaries install. File sink, plus an
//!   optional stderr sink (the CLI keeps its `[level] msg` terminal output).
//!   Workspace crates log at the `EXP_LOG` level; third-party crates are
//!   capped at `warn` so the file stays about us.
//! * [`install_panic_hook`]: a panic ALSO lands in the log file (with a
//!   backtrace) before the default hook prints it.
//! * [`HeartbeatHealth`]: warn-level heartbeat failures with the HTTP status,
//!   one `info` on recovery, and the optional-payload back-off: a beat whose
//!   `agentAccounts`/`agentUsage`/`mcpReadiness` payload drew a 4xx is followed
//!   by BARE beats, so `last_seen_at` still lands.

use std::fs::{File, OpenOptions};
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// Rotate once the live file would pass 5 MB.
pub const MAX_LOG_BYTES: u64 = 5 * 1024 * 1024;
/// Rotated files kept beside the live one (`<name>.1` newest … `.3` oldest).
pub const KEEP_ROTATED: usize = 3;

/// `{data_dir}/logs` — every log file lives here.
pub fn logs_dir(data_dir: &Path) -> PathBuf {
    data_dir.join("logs")
}

/// `EXP_LOG` → a level (`trace|debug|info|warn|error|off`, default `info`).
pub fn parse_level(value: Option<&str>) -> log::LevelFilter {
    match value.map(|value| value.trim().to_ascii_lowercase()).as_deref() {
        Some("trace") => log::LevelFilter::Trace,
        Some("debug") => log::LevelFilter::Debug,
        Some("warn") | Some("warning") => log::LevelFilter::Warn,
        Some("error") => log::LevelFilter::Error,
        Some("off") => log::LevelFilter::Off,
        _ => log::LevelFilter::Info,
    }
}

/// The level `EXP_LOG` asks for.
pub fn level_from_env() -> log::LevelFilter {
    parse_level(std::env::var("EXP_LOG").ok().as_deref())
}

// ---------------------------------------------------------------------------
// The rotating file
// ---------------------------------------------------------------------------

struct OpenFile {
    file: File,
    len: u64,
}

/// A size-capped append-only file with numbered rotations.
pub struct RotatingFile {
    path: PathBuf,
    max_bytes: u64,
    keep: usize,
    state: Mutex<Option<OpenFile>>,
}

impl RotatingFile {
    /// Open (creating `path`'s directory) for appending. Never fails: an
    /// unopenable path yields a file that drops every line.
    pub fn open(path: PathBuf, max_bytes: u64, keep: usize) -> Self {
        let state = open_append(&path);
        Self {
            path,
            max_bytes: max_bytes.max(1),
            keep,
            state: Mutex::new(state),
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Whether a file is actually open.
    pub fn is_open(&self) -> bool {
        self.lock().is_some()
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Option<OpenFile>> {
        self.state.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Append one line (a trailing newline is added when missing), rotating
    /// first when it would push the file past the cap.
    pub fn write_line(&self, line: &str) {
        let mut state = self.lock();
        let needed = line.len() as u64 + u64::from(!line.ends_with('\n'));
        if state
            .as_ref()
            .is_some_and(|open| open.len > 0 && open.len + needed > self.max_bytes)
        {
            *state = None; // close before renaming (Windows)
            *state = self.rotate();
        }
        let Some(open) = state.as_mut() else {
            return;
        };
        let mut ok = open.file.write_all(line.as_bytes()).is_ok();
        if ok && !line.ends_with('\n') {
            ok = open.file.write_all(b"\n").is_ok();
        }
        if ok {
            open.len += needed;
        }
    }

    /// Shift `.1..keep` up one (dropping the oldest), move the live file to
    /// `.1` and open a fresh one. A failed rename truncates instead, so the
    /// cap holds either way.
    fn rotate(&self) -> Option<OpenFile> {
        if self.keep == 0 {
            return truncate(&self.path);
        }
        let _ = std::fs::remove_file(rotated_path(&self.path, self.keep));
        for index in (1..self.keep).rev() {
            let from = rotated_path(&self.path, index);
            if from.exists() {
                let _ = std::fs::rename(&from, rotated_path(&self.path, index + 1));
            }
        }
        if std::fs::rename(&self.path, rotated_path(&self.path, 1)).is_err() {
            return truncate(&self.path);
        }
        open_append(&self.path)
    }
}

/// `<path>.<index>`.
pub fn rotated_path(path: &Path, index: usize) -> PathBuf {
    let mut name = path.as_os_str().to_owned();
    name.push(format!(".{index}"));
    PathBuf::from(name)
}

fn open_append(path: &Path) -> Option<OpenFile> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).ok()?;
    }
    let file = OpenOptions::new().create(true).append(true).open(path).ok()?;
    let len = file.metadata().map(|meta| meta.len()).unwrap_or(0);
    Some(OpenFile { file, len })
}

fn truncate(path: &Path) -> Option<OpenFile> {
    let file = File::create(path).ok()?;
    Some(OpenFile { file, len: 0 })
}

// ---------------------------------------------------------------------------
// The logger
// ---------------------------------------------------------------------------

/// Crate roots logged at the full `EXP_LOG` level; everything else is capped
/// at `warn` (gpui, reqwest, … would otherwise fill the file).
const WORKSPACE_TARGETS: &[&str] = &[
    "app",
    "exp_desktop",
    "exponential",
    "exp_cli",
    "ui",
    "coding",
    "api",
    "domain",
    "steer",
    "engine",
    "sync",
    "theme",
    "updater",
    "terminal",
    "gpui_markdown_editor",
];

fn is_workspace_target(target: &str) -> bool {
    let root = target.split("::").next().unwrap_or(target);
    WORKSPACE_TARGETS.contains(&root)
}

/// The line format: `<UTC ms timestamp> <LEVEL> <target>: <message>`.
pub fn format_line(level: log::Level, target: &str, message: &std::fmt::Arguments<'_>) -> String {
    format!(
        "{} {:<5} {target}: {message}",
        chrono::Utc::now().format("%Y-%m-%dT%H:%M:%S%.3fZ"),
        level.as_str()
    )
}

/// The installed logger: a file sink plus an optional stderr sink.
pub struct ExpLogger {
    level: log::LevelFilter,
    file: Option<RotatingFile>,
    /// Stderr sink: the formatter and the minimum level it prints.
    stderr: Option<(fn(&log::Record), log::LevelFilter)>,
}

impl ExpLogger {
    pub fn new(level: log::LevelFilter) -> Self {
        Self {
            level,
            file: None,
            stderr: None,
        }
    }

    /// Add the rotating `{data_dir}/logs/<file_name>` sink.
    pub fn with_file(mut self, data_dir: &Path, file_name: &str) -> Self {
        self.file = Some(RotatingFile::open(
            logs_dir(data_dir).join(file_name),
            MAX_LOG_BYTES,
            KEEP_ROTATED,
        ));
        self
    }

    /// Echo records at `min` or more severe (and within the logger's level)
    /// through `print`.
    pub fn with_stderr(mut self, print: fn(&log::Record), min: log::LevelFilter) -> Self {
        self.stderr = Some((print, min));
        self
    }

    pub fn level(&self) -> log::LevelFilter {
        self.level
    }

    /// The file this logger writes, when one is open.
    pub fn file_path(&self) -> Option<&Path> {
        self.file.as_ref().filter(|file| file.is_open()).map(RotatingFile::path)
    }

    /// Write one line to the FILE only (a startup banner that must not reach
    /// an interactive terminal).
    pub fn file_only(&self, level: log::Level, target: &str, message: std::fmt::Arguments<'_>) {
        if let Some(file) = &self.file {
            file.write_line(&format_line(level, target, &message));
        }
    }
}

impl log::Log for ExpLogger {
    fn enabled(&self, metadata: &log::Metadata) -> bool {
        let cap = if is_workspace_target(metadata.target()) {
            self.level
        } else {
            self.level.min(log::LevelFilter::Warn)
        };
        metadata.level() <= cap
    }

    fn log(&self, record: &log::Record) {
        if !self.enabled(record.metadata()) {
            return;
        }
        if let Some(file) = &self.file {
            file.write_line(&format_line(record.level(), record.target(), record.args()));
        }
        if let Some((print, min)) = self.stderr {
            if record.level() <= min {
                print(record);
            }
        }
    }

    fn flush(&self) {}
}

/// Install `logger` as THE process logger (leaked: it lives for the process).
/// Returns the installed reference, or `None` when a logger already exists.
pub fn install(logger: ExpLogger) -> Option<&'static ExpLogger> {
    let level = logger.level;
    let logger: &'static ExpLogger = Box::leak(Box::new(logger));
    log::set_logger(logger).ok()?;
    log::set_max_level(level);
    Some(logger)
}

/// A panic payload's message (`&str` / `String` payloads; else a placeholder).
pub fn panic_message(payload: &(dyn std::any::Any + Send)) -> String {
    if let Some(text) = payload.downcast_ref::<&str>() {
        (*text).to_string()
    } else if let Some(text) = payload.downcast_ref::<String>() {
        text.clone()
    } else {
        "non-string panic payload".to_string()
    }
}

/// Route every panic through `log::error!` (location, thread, message and a
/// backtrace) and then the previous hook (stderr), so a panic on a detached
/// task leaves a trace in the log file.
pub fn install_panic_hook() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let location = info
            .location()
            .map(|at| format!("{}:{}", at.file(), at.line()))
            .unwrap_or_else(|| "unknown location".to_string());
        let thread = std::thread::current();
        log::error!(
            target: "coding::panic",
            "panic on thread {:?} at {location}: {}\n{}",
            thread.name().unwrap_or("<unnamed>"),
            panic_message(info.payload()),
            std::backtrace::Backtrace::force_capture()
        );
        previous(info);
    }));
}

// ---------------------------------------------------------------------------
// The heartbeat failure policy (desktop device_sync + CLI daemon)
// ---------------------------------------------------------------------------

/// Beats between warn lines once a failure streak is long (the first
/// [`WARN_FIRST`] failures always warn).
const WARN_EVERY: u32 = 10;
const WARN_FIRST: u32 = 3;
/// The bare-beat stretch after a payload rejection doubles per repeat, up to
/// this many beats (~10 min at 30s).
const MAX_BARE_BEATS: u32 = 20;

/// One device loop's heartbeat health.
#[derive(Debug, Default)]
pub struct HeartbeatHealth {
    /// Consecutive failed beats.
    failures: u32,
    /// Bare beats (no optional payload) still owed.
    bare_beats_left: u32,
    /// Consecutive payload rejections (reset by an accepted FULL beat).
    payload_rejections: u32,
}

impl HeartbeatHealth {
    pub fn new() -> Self {
        Self::default()
    }

    /// Called once per beat: whether this beat may carry the optional
    /// payload (`agentAccounts`/`agentUsage`/`mcpReadiness`).
    pub fn begin_beat(&mut self) -> bool {
        if self.bare_beats_left > 0 {
            self.bare_beats_left -= 1;
            false
        } else {
            true
        }
    }

    pub fn failures(&self) -> u32 {
        self.failures
    }

    /// An accepted beat. `carried_optional` = it carried the optional
    /// payload (only such a beat proves the payload is fine again).
    pub fn on_ok(&mut self, tag: &str, carried_optional: bool) {
        if self.failures > 0 {
            log::info!("{tag}: heartbeat recovered after {} failure(s)", self.failures);
        }
        self.failures = 0;
        if carried_optional {
            self.payload_rejections = 0;
        }
    }

    /// A failed beat: log it (warn, damped on long streaks) and, when the
    /// server refused a beat carrying the optional payload with a 4xx, owe
    /// bare beats so `last_seen_at` still lands.
    pub fn on_err(&mut self, tag: &str, err: &api::ApiError, carried_optional: bool) {
        self.failures = self.failures.saturating_add(1);
        let payload_rejected = carried_optional && is_payload_rejection(err);
        let detail = describe_error(err);
        if self.failures <= WARN_FIRST || self.failures % WARN_EVERY == 0 {
            log::warn!(
                "{tag}: heartbeat failed ({} in a row): {detail}{}",
                self.failures,
                if payload_rejected {
                    " — the next beat goes out without the agent/MCP payload"
                } else {
                    ""
                }
            );
        } else {
            log::debug!("{tag}: heartbeat failed ({} in a row): {detail}", self.failures);
        }
        if payload_rejected {
            self.payload_rejections = self.payload_rejections.saturating_add(1);
            let shift = self.payload_rejections.saturating_sub(1).min(5);
            self.bare_beats_left = (1u32 << shift).min(MAX_BARE_BEATS);
        }
    }
}

/// A 4xx that is not 401 (the credential) or 426 (the build): the BODY was
/// refused, and the optional payload is the part that varies.
pub fn is_payload_rejection(err: &api::ApiError) -> bool {
    matches!(err, api::ApiError::Http { status, .. }
        if (400..=499).contains(status) && *status != 401 && *status != 426)
}

/// The failure detail a log line names (status + message).
pub fn describe_error(err: &api::ApiError) -> String {
    match err {
        api::ApiError::Http { status, message } => format!("HTTP {status}: {message}"),
        api::ApiError::Unauthorized => "HTTP 401 (credential rejected)".to_string(),
        api::ApiError::UpgradeRequired => "HTTP 426 (this build is below the server's minimum)".to_string(),
        api::ApiError::Transport { message, offline } => format!(
            "{}: {message}",
            if *offline { "offline" } else { "transport error" }
        ),
        other => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "exp-logging-{tag}-{}-{}",
            std::process::id(),
            chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn rotates_past_the_cap_and_keeps_three() {
        let dir = temp_dir("rotate");
        let path = dir.join("logs").join("exponential.log");
        let file = RotatingFile::open(path.clone(), 100, 3);
        assert!(file.is_open());
        // 30-byte lines: 3 per file before the cap.
        for index in 0..3 {
            file.write_line(&format!("line-{index:02} {}", "x".repeat(20)));
        }
        assert!(!rotated_path(&path, 1).exists());
        file.write_line(&format!("line-03 {}", "x".repeat(20)));
        assert!(rotated_path(&path, 1).exists(), "the 4th line rotates");
        let first = std::fs::read_to_string(rotated_path(&path, 1)).unwrap();
        assert!(first.starts_with("line-00"));
        // Enough lines for many rotations.
        for index in 4..40 {
            file.write_line(&format!("line-{index:02} {}", "x".repeat(20)));
        }
        for index in 1..=3 {
            assert!(rotated_path(&path, index).exists(), ".{index} kept");
        }
        assert!(!rotated_path(&path, 4).exists(), "keep=3 drops the oldest");
        let oldest = std::fs::read_to_string(rotated_path(&path, 3)).unwrap();
        assert!(!oldest.contains("line-00"), "the first lines were dropped");
        for candidate in [path.clone(), rotated_path(&path, 1), rotated_path(&path, 2)] {
            let len = std::fs::metadata(&candidate).unwrap().len();
            assert!(len <= 100, "{} is {len} bytes", candidate.display());
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn appends_to_an_existing_file_and_counts_its_size() {
        let dir = temp_dir("append");
        let path = dir.join("app.log");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(&path, "x".repeat(90)).unwrap();
        let file = RotatingFile::open(path.clone(), 100, 3);
        file.write_line("0123456789012");
        assert!(rotated_path(&path, 1).exists(), "the pre-existing size counts");
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "0123456789012\n");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_unopenable_path_never_panics() {
        let dir = temp_dir("blocked");
        std::fs::create_dir_all(&dir).unwrap();
        // A FILE where the logs directory should be.
        std::fs::write(dir.join("logs"), "not a dir").unwrap();
        let logger = ExpLogger::new(log::LevelFilter::Info).with_file(&dir, "exponential.log");
        assert!(logger.file_path().is_none());
        logger.file_only(log::Level::Info, "coding", format_args!("dropped"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn line_format_names_level_and_target() {
        let line = format_line(log::Level::Warn, "ui::device_sync", &format_args!("hello"));
        let (stamp, rest) = line.split_once(' ').unwrap();
        assert!(stamp.ends_with('Z') && stamp.contains('T') && stamp.contains('.'));
        assert_eq!(rest, "WARN  ui::device_sync: hello");
    }

    #[test]
    fn third_party_targets_are_capped_at_warn() {
        use log::Log as _;
        let logger = ExpLogger::new(log::LevelFilter::Debug);
        let meta = |level, target| log::Metadata::builder().level(level).target(target).build();
        assert!(logger.enabled(&meta(log::Level::Debug, "ui::device_sync")));
        assert!(logger.enabled(&meta(log::Level::Info, "exponential::commands::daemon")));
        assert!(!logger.enabled(&meta(log::Level::Info, "reqwest::connect")));
        assert!(logger.enabled(&meta(log::Level::Warn, "reqwest::connect")));
        assert!(!logger.enabled(&meta(log::Level::Trace, "coding")));
    }

    #[test]
    fn exp_log_parses_like_the_cli() {
        assert_eq!(parse_level(None), log::LevelFilter::Info);
        assert_eq!(parse_level(Some("debug")), log::LevelFilter::Debug);
        assert_eq!(parse_level(Some("TRACE")), log::LevelFilter::Trace);
        assert_eq!(parse_level(Some("warn")), log::LevelFilter::Warn);
        assert_eq!(parse_level(Some("nonsense")), log::LevelFilter::Info);
    }

    fn http(status: u16) -> api::ApiError {
        api::ApiError::Http {
            status,
            message: "Invalid input".to_string(),
        }
    }

    #[test]
    fn a_payload_4xx_makes_the_next_beat_bare_then_resumes() {
        let mut health = HeartbeatHealth::new();
        assert!(health.begin_beat());
        health.on_err("t", &http(400), true);
        assert!(!health.begin_beat(), "the next beat drops the optional payload");
        health.on_ok("t", false);
        assert_eq!(health.failures(), 0);
        assert!(health.begin_beat(), "a landed bare beat resumes the full payload");
        health.on_ok("t", true);
        // A later rejection starts over at one bare beat.
        health.on_err("t", &http(422), true);
        assert!(!health.begin_beat());
        assert!(health.begin_beat());
    }

    #[test]
    fn repeated_payload_rejections_back_off_longer() {
        let mut health = HeartbeatHealth::new();
        let bare_after_rejection = |health: &mut HeartbeatHealth| {
            health.on_err("t", &http(400), true);
            let mut bare = 0;
            while !health.begin_beat() {
                bare += 1;
                health.on_ok("t", false);
            }
            bare
        };
        assert!(health.begin_beat());
        assert_eq!(bare_after_rejection(&mut health), 1);
        assert_eq!(bare_after_rejection(&mut health), 2);
        assert_eq!(bare_after_rejection(&mut health), 4);
        for _ in 0..6 {
            bare_after_rejection(&mut health);
        }
        assert_eq!(bare_after_rejection(&mut health), MAX_BARE_BEATS);
        // An accepted FULL beat clears the streak.
        health.on_ok("t", true);
        assert_eq!(bare_after_rejection(&mut health), 1);
    }

    #[test]
    fn other_failures_keep_the_payload() {
        let mut health = HeartbeatHealth::new();
        for err in [
            api::ApiError::Unauthorized,
            api::ApiError::UpgradeRequired,
            http(401),
            http(426),
            http(500),
            api::ApiError::Transport {
                message: "dns".to_string(),
                offline: true,
            },
        ] {
            health.on_err("t", &err, true);
            assert!(health.begin_beat(), "{err:?} is not a payload rejection");
        }
        // A 4xx on a beat that carried nothing is not the payload's fault.
        health.on_err("t", &http(400), false);
        assert!(health.begin_beat());
        assert_eq!(health.failures(), 7);
    }

    #[test]
    fn describe_names_the_status() {
        assert_eq!(describe_error(&http(400)), "HTTP 400: Invalid input");
        assert!(describe_error(&api::ApiError::Unauthorized).contains("401"));
        assert!(describe_error(&api::ApiError::UpgradeRequired).contains("426"));
    }

    #[test]
    fn panic_messages_extract() {
        let caught = std::panic::catch_unwind(|| panic!("boom {}", 1)).unwrap_err();
        assert_eq!(panic_message(caught.as_ref()), "boom 1");
        let caught = std::panic::catch_unwind(|| panic!("static")).unwrap_err();
        assert_eq!(panic_message(caught.as_ref()), "static");
    }
}
