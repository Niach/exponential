//! #EXP-2144: the newer build this process knows and does not run yet, and
//! since when: what the readiness block's `App update` row says
//! (device-doctor.json). One per process.

use std::sync::Mutex;
use std::time::SystemTime;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pending {
    pub version: String,
    /// Since when the device has known a newer build than its own (a still
    /// newer release does not restart it).
    pub since: SystemTime,
}

static PENDING: Mutex<Option<Pending>> = Mutex::new(None);

fn lock() -> std::sync::MutexGuard<'static, Option<Pending>> {
    PENDING.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// A newer build, `version`, is known as of `now`.
pub fn note(version: &str, now: SystemTime) {
    let mut pending = lock();
    let since = pending.as_ref().map_or(now, |known| known.since);
    *pending = Some(Pending { version: version.to_string(), since });
}

/// No newer build is known.
pub fn clear() {
    *lock() = None;
}

pub fn get() -> Option<Pending> {
    lock().clone()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn a_newer_release_keeps_the_since() {
        let at = SystemTime::UNIX_EPOCH + Duration::from_secs(100);
        note("1.0.1", at);
        note("1.0.2", at + Duration::from_secs(900));
        assert_eq!(get(), Some(Pending { version: "1.0.2".into(), since: at }));
        clear();
        assert_eq!(get(), None);
    }
}
