//! A flood guard for error events (#LOGS-2): a render error repeating every
//! frame would otherwise be one event per frame. At most one event per
//! distinct message a minute, and 30 a minute overall.
//!
//! Across processes (#ERR-29): a binary that fails at start and is started
//! again every few seconds (the shell under its supervisor, a unit with
//! `Restart=always`) would send its error with every start, each process's
//! limiter being new. So a process younger than [`YOUNG`] counts what it
//! sends in `reported.json` beside the config cache: at most [`PER_HOUR`]
//! of one message an hour, whichever process sent them. A native crash
//! another launch left is counted once, however many binaries send it, and
//! under its own binary's name: two that died the same way do not share.

use std::collections::{BTreeMap, HashMap};
use std::path::Path;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use sentry::protocol::Event;

const WINDOW: Duration = Duration::from_secs(60);
const PER_WINDOW: u32 = 30;

#[derive(Default)]
pub struct Limiter {
    window: Option<Instant>,
    sent: u32,
    seen: HashMap<String, Instant>,
}

impl Limiter {
    pub fn allow(&mut self, key: &str, now: Instant) -> bool {
        if self.window.is_none_or(|w| now.duration_since(w) >= WINDOW) {
            self.window = Some(now);
            self.sent = 0;
            self.seen.retain(|_, at| now.duration_since(*at) < WINDOW);
        }
        if self.sent >= PER_WINDOW || self.seen.get(key).is_some_and(|at| now.duration_since(*at) < WINDOW) {
            return false;
        }
        self.sent += 1;
        self.seen.insert(key.to_owned(), now);
        true
    }
}

static LIMITER: Mutex<Option<Limiter>> = Mutex::new(None);

/// A process this young may be one of a restart loop.
const YOUNG: Duration = Duration::from_secs(60);
/// Events of one message an hour that young processes send between them.
const PER_HOUR: usize = 10;
const HOUR: u64 = 3600;

static STARTED: OnceLock<Instant> = OnceLock::new();

/// When this process started reporting (`init` calls it first).
pub(crate) fn started() -> Instant {
    *STARTED.get_or_init(Instant::now)
}

/// What the file knows a message by: never its text (a panic's can quote
/// what the person typed).
fn name_of(key: &str) -> String {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    key.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

/// Whether one more event of `key` may go at `now` (Unix seconds), by the
/// count in `path`; counted when it may. `once` names an event several
/// processes send (a pending crash report's id): counted the first time,
/// allowed after. An unreadable or unwritable file allows (the guard never
/// costs a report).
fn allow_across(path: &Path, key: &str, once: Option<&str>, now: u64) -> bool {
    let mut sent: BTreeMap<String, Vec<u64>> = std::fs::read_to_string(path).ok().and_then(|text| serde_json::from_str(&text).ok()).unwrap_or_default();
    // An hour back, and none from a clock that ran ahead.
    sent.retain(|_, times| {
        times.retain(|at| *at <= now + 60 && now.saturating_sub(*at) < HOUR);
        !times.is_empty()
    });
    let once = once.map(|id| name_of(&format!("once {id}")));
    if once.as_ref().is_some_and(|id| sent.contains_key(id)) {
        return true;
    }
    let times = sent.entry(name_of(key)).or_default();
    if times.len() >= PER_HOUR {
        return false;
    }
    times.push(now);
    if let Some(id) = once {
        sent.insert(id, vec![now]);
    }
    if let Ok(text) = serde_json::to_string(&sent) {
        let tmp = path.with_extension(format!("json.{}.tmp", std::process::id()));
        if std::fs::create_dir_all(path.parent().unwrap_or(Path::new("."))).is_ok() && std::fs::write(&tmp, text).is_ok() && std::fs::rename(&tmp, path).is_err() {
            let _ = std::fs::remove_file(&tmp);
        }
    }
    true
}

/// The event's identity: its first exception, else its message.
fn key(e: &Event<'_>) -> String {
    let text = match e.exception.values.first() {
        Some(ex) => format!("{}: {}", ex.ty, ex.value.as_deref().unwrap_or("")),
        None => e.message.clone().or_else(|| e.logentry.as_ref().map(|l| l.message.clone())).unwrap_or_default(),
    };
    text.chars().take(160).collect()
}

pub fn allow_event(e: &Event<'_>) -> bool {
    // A native crash another launch left (crash-monitor's tags): its
    // binary's and release's, and once for all the processes that send it.
    let pending = e.tags.get("crash").is_some_and(|kind| kind == "native");
    let (key, once) = if pending {
        let of = |tag: &str| e.tags.get(tag).map(String::as_str).unwrap_or_default();
        (format!("{} {} {}", of("component"), e.release.as_deref().unwrap_or_default(), key(e)), Some(e.event_id.to_string()))
    } else {
        (key(e), None)
    };
    let allowed = LIMITER.lock().unwrap_or_else(|p| p.into_inner()).get_or_insert_with(Limiter::default).allow(&key, Instant::now());
    if !allowed {
        return false;
    }
    if started().elapsed() >= YOUNG {
        return true;
    }
    let now = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    allow_across(&crate::config::cache_path().with_file_name("reported.json"), &key, once.as_deref(), now)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_per_message_a_minute_and_thirty_overall() {
        let mut l = Limiter::default();
        let t = Instant::now();
        assert!(l.allow("render failed", t));
        assert!(!l.allow("render failed", t + Duration::from_secs(1)));
        assert!(l.allow("bind failed", t + Duration::from_secs(1)));
        assert!(l.allow("render failed", t + Duration::from_secs(61)), "a new minute");
        let mut l = Limiter::default();
        for i in 0..PER_WINDOW {
            assert!(l.allow(&format!("e{i}"), t));
        }
        assert!(!l.allow("one more", t));
    }

    /// #ERR-29: a binary restarted in a loop sends a message ten times an
    /// hour, however many processes that is.
    #[test]
    fn a_restart_loop_sends_a_message_ten_times_an_hour() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("cache").join("reported.json");
        let now = 1_800_000_000;
        // Each call a new process: the count is the file's.
        for start in 0..PER_HOUR as u64 {
            assert!(allow_across(&path, "the ui directory could not be opened", None, now + start * 12), "start {start}");
        }
        assert!(!allow_across(&path, "the ui directory could not be opened", None, now + 200));
        assert!(allow_across(&path, "the shell crashed; starting it again", None, now + 200), "another message has its own count");
        assert!(!std::fs::read_to_string(&path).unwrap().contains("ui directory"), "never a message's text on disk");
        assert!(allow_across(&path, "the ui directory could not be opened", None, now + HOUR + 1), "an hour on");
        // A file that is no JSON never costs a report.
        std::fs::write(&path, "half a fi").unwrap();
        assert!(allow_across(&path, "the ui directory could not be opened", None, now));
        // A clock that ran ahead and came back blocks nothing for longer.
        std::fs::remove_file(&path).unwrap();
        for _ in 0..PER_HOUR {
            assert!(allow_across(&path, "ahead", None, now + 5 * HOUR));
        }
        assert!(allow_across(&path, "ahead", None, now), "the future's counts are dropped");
    }

    /// #ERR-29: seven binaries start together after a crash and each sends
    /// the report it left: one count, and every one of them may send it.
    #[test]
    fn a_pending_crash_is_counted_once_whoever_sends_it() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("reported.json");
        let now = 1_800_000_000;
        let key = "shell exponential-ede@0.2.29 SIGSEGV: SIGSEGV / SEGV_MAPERR at 0x0";
        for process in 0..7 {
            assert!(allow_across(&path, key, Some("6c8f0b2c-9c55-4c8a-9b0f-0f3c1f0e9a11"), now + process), "process {process}");
        }
        // Nine more crashes of the same kind get through this hour, the
        // loop after them does not.
        for crash in 0..PER_HOUR as u64 - 1 {
            assert!(allow_across(&path, key, Some(&format!("crash-{crash}")), now + 10 + crash), "crash {crash}");
        }
        assert!(!allow_across(&path, key, Some("one more"), now + 60));
        assert!(allow_across(&path, "sealed-lock exponential-ede@0.2.29 SIGSEGV: SIGSEGV / SEGV_MAPERR at 0x0", Some("the lock's"), now + 60), "another binary's crash has its own count");
    }
}
