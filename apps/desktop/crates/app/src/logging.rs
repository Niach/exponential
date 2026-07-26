//! File logging for `exp-desktop` (EXP-300).
//!
//! Until this existed the desktop binary installed **no `log` sink at all**:
//! five crates in this workspace call `log::warn!`/`log::error!` and every one
//! of those records was discarded. That is why EXP-300 — the app lingering as
//! a live process after Quit — left zero evidence behind: gpui's own
//! `timed out waiting on app_will_quit` and `terminal`'s "thread still blocked
//! after 1s; detaching" both fire exactly on the failing path, and both went
//! nowhere.
//!
//! Deliberately hand-rolled rather than pulling in `env_logger`: `log` is the
//! only logging crate vendored in this tree, and the requirements here are
//! narrow enough that a sink is cheaper than a dependency.
//!
//! Two properties matter more than features:
//!
//! 1. **Every record is flushed immediately.** The failure this exists to
//!    diagnose ends in a hang or a `_exit` from the quit watchdog
//!    ([`ui::arm_quit_watchdog`]) — neither runs destructors, so anything
//!    sitting in a `BufWriter` at that moment is lost precisely when it is
//!    the thing we needed to read.
//! 2. **Failing to log never takes the app down.** Every IO error here is
//!    swallowed; a read-only data dir must not cost the user their editor.

use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;

/// Roll the log over at this size, keeping one previous generation. Big
/// enough to cover a long session, small enough to attach to an issue.
const MAX_BYTES: u64 = 8 * 1024 * 1024;

struct FileLogger {
    /// `None` once the sink is unusable (disk full, permissions) — we degrade
    /// to silence rather than failing the launch.
    file: Mutex<Option<File>>,
}

impl log::Log for FileLogger {
    fn enabled(&self, _: &log::Metadata<'_>) -> bool {
        // Level filtering is handled by `log::set_max_level`.
        true
    }

    fn log(&self, record: &log::Record<'_>) {
        if !self.enabled(record.metadata()) {
            return;
        }
        let line = format!(
            "{} {:<5} [{}] {}\n",
            timestamp(),
            record.level(),
            record.target(),
            record.args()
        );
        // In debug builds mirror to stderr so `cargo run` stays usable. On
        // Windows the release binary is the GUI subsystem and has no stderr,
        // which is exactly why the file sink is the primary destination.
        #[cfg(debug_assertions)]
        {
            eprint!("{line}");
        }
        if let Ok(mut guard) = self.file.lock() {
            if let Some(file) = guard.as_mut() {
                // Flush per record — see the module note; a buffered tail is
                // worthless after a hang or a `_exit`.
                if file.write_all(line.as_bytes()).is_err() || file.flush().is_err() {
                    *guard = None;
                }
            }
        }
    }

    fn flush(&self) {
        if let Ok(mut guard) = self.file.lock() {
            if let Some(file) = guard.as_mut() {
                let _ = file.flush();
            }
        }
    }
}

/// `YYYY-MM-DD HH:MM:SS.mmm` in UTC, computed from the epoch directly.
///
/// A timestamp is the whole point of a log line here (EXP-300 is about *how
/// long* a teardown took), but no date crate is vendored for the desktop
/// binary and this is not worth adding one for — the civil-date conversion is
/// a dozen lines of arithmetic.
fn timestamp() -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    format_timestamp(now.as_secs(), now.subsec_millis())
}

/// Split from [`timestamp`] purely so the calendar arithmetic is testable
/// without a clock.
fn format_timestamp(secs: u64, millis: u32) -> String {
    let (days, rem) = (secs / 86_400, secs % 86_400);
    let (h, m, s) = (rem / 3600, (rem % 3600) / 60, rem % 60);

    // Days since 1970-01-01 → civil date (Howard Hinnant's `civil_from_days`).
    let z = days as i64 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let mth = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = era * 400 + yoe + i64::from(mth <= 2);

    format!("{y:04}-{mth:02}-{d:02} {h:02}:{m:02}:{s:02}.{millis:03}")
}

/// `<data_dir>/logs/exp-desktop.log` — beside the account store, so the
/// staging build keeps its own (`default_data_dir` already branches on the
/// `staging` feature).
pub fn log_path() -> PathBuf {
    api::default_data_dir().join("logs").join("exp-desktop.log")
}

/// Install the file sink. Call FIRST in `main` — records emitted before this
/// are dropped on the floor by `log`'s no-op default.
///
/// Level defaults to `info` and is overridable per-launch with `EXP_LOG`
/// (`error`/`warn`/`info`/`debug`/`trace`), which is how you turn on the
/// chatty `[sync]` debug records when chasing a teardown.
pub fn init() {
    let level = std::env::var("EXP_LOG")
        .ok()
        .and_then(|raw| raw.parse::<log::LevelFilter>().ok())
        .unwrap_or(log::LevelFilter::Info);

    let path = log_path();
    if let Some(dir) = path.parent() {
        let _ = fs::create_dir_all(dir);
    }
    // Keep one previous generation so a launch that follows a bad quit does
    // not immediately bury the evidence from that quit.
    if fs::metadata(&path).is_ok_and(|meta| meta.len() > MAX_BYTES) {
        let _ = fs::rename(&path, path.with_extension("log.1"));
    }

    let file = OpenOptions::new().create(true).append(true).open(&path).ok();
    let logger = FileLogger {
        file: Mutex::new(file),
    };
    if log::set_boxed_logger(Box::new(logger)).is_ok() {
        log::set_max_level(level);
        log::info!(
            "exp-desktop {} starting (pid {}, log {})",
            env!("CARGO_PKG_VERSION"),
            std::process::id(),
            path.display()
        );
    }
}

#[cfg(test)]
mod tests {
    use super::{format_timestamp, FileLogger};
    use std::sync::Mutex;

    /// The sink must land on disk on EVERY record, not at drop: the failure it
    /// exists to capture ends in a hang or the watchdog's `_exit`, and neither
    /// runs destructors. A buffered implementation would pass a naive test and
    /// still lose the one line that mattered.
    #[test]
    fn writes_through_on_every_record() {
        use log::Log;

        let path = std::env::temp_dir().join(format!("exp-log-test-{}.log", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .expect("open temp log");
        let logger = FileLogger {
            file: Mutex::new(Some(file)),
        };

        logger.log(
            &log::Record::builder()
                .args(format_args!("terminate path entered"))
                .level(log::Level::Warn)
                .target("ui::quit")
                .build(),
        );

        // Read WITHOUT dropping the logger — proves the bytes are already
        // durable rather than sitting in a buffer awaiting teardown.
        let body = std::fs::read_to_string(&path).expect("read temp log");
        assert!(body.contains("WARN "), "level missing: {body:?}");
        assert!(body.contains("[ui::quit]"), "target missing: {body:?}");
        assert!(
            body.contains("terminate path entered"),
            "message missing: {body:?}"
        );
        assert!(body.ends_with('\n'), "record not newline-terminated");

        drop(logger);
        let _ = std::fs::remove_file(&path);
    }

    /// The civil-date conversion is hand-rolled (no date crate is vendored for
    /// the desktop binary), so pin it against known epoch instants — including
    /// leap years and the century rule `civil_from_days` exists to get right.
    #[test]
    fn formats_known_instants() {
        for (secs, millis, want) in [
            (0u64, 0u32, "1970-01-01 00:00:00.000"),
            (1, 500, "1970-01-01 00:00:01.500"),
            (86_399, 999, "1970-01-01 23:59:59.999"),
            (86_400, 0, "1970-01-02 00:00:00.000"),
            // 2000-02-29 — leap year by the 400 rule.
            (951_782_400, 0, "2000-02-29 00:00:00.000"),
            // 2100 is NOT a leap year (the 100 rule): 2100-03-01 follows 02-28.
            (4_107_456_000, 0, "2100-02-28 00:00:00.000"),
            (4_107_542_400, 0, "2100-03-01 00:00:00.000"),
            (1_000_000_000, 0, "2001-09-09 01:46:40.000"),
            (1_700_000_000, 0, "2023-11-14 22:13:20.000"),
        ] {
            assert_eq!(format_timestamp(secs, millis), want, "secs={secs}");
        }
    }
}
