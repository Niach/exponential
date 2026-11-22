//! #EXP-1892: which of this process's writers the disk currently refuses
//! (no space left, or over quota), and since when. Writers report every
//! outcome ([`note`]); the device doctor reads [`failing`] into its
//! `disk_writes` row, which rides the heartbeat, so a full disk shows up on
//! the device page instead of as a device that silently stopped.
//!
//! In-process only: a writer that succeeds again clears its entry.

use std::collections::BTreeMap;
use std::sync::Mutex;
use std::time::SystemTime;

static FAILING: Mutex<BTreeMap<&'static str, SystemTime>> = Mutex::new(BTreeMap::new());

/// Whether `err` says the disk is full (ENOSPC) or the quota is used up.
pub fn is_disk_full(err: &std::io::Error) -> bool {
    matches!(err.kind(), std::io::ErrorKind::StorageFull | std::io::ErrorKind::QuotaExceeded)
}

/// Record one write of `writer` (`log`, `journal`, `state`, …): a full-disk
/// error starts (or keeps) its entry, a success clears it, any other error
/// leaves it as it was.
pub fn note<T>(writer: &'static str, result: &std::io::Result<T>) {
    note_at(writer, result, SystemTime::now());
}

fn note_at<T>(writer: &'static str, result: &std::io::Result<T>, now: SystemTime) {
    let mut failing = FAILING.lock().unwrap_or_else(|poison| poison.into_inner());
    match result {
        Ok(_) => {
            failing.remove(writer);
        }
        Err(err) if is_disk_full(err) => {
            failing.entry(writer).or_insert(now);
        }
        Err(_) => {}
    }
}

/// The writers the disk refuses now (sorted) and the earliest such failure;
/// `None` = every write lands.
pub fn failing() -> Option<(Vec<&'static str>, SystemTime)> {
    let failing = FAILING.lock().unwrap_or_else(|poison| poison.into_inner());
    let since = failing.values().min().copied()?;
    Some((failing.keys().copied().collect(), since))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    // The raw OS codes for "disk full" and "over quota": `libc::EDQUOT` does
    // not exist on Windows, whose equivalents std maps to the same kinds.
    #[cfg(unix)]
    const DISK_FULL: i32 = libc::ENOSPC;
    #[cfg(unix)]
    const QUOTA_EXCEEDED: i32 = libc::EDQUOT;
    #[cfg(windows)]
    const DISK_FULL: i32 = 112; // ERROR_DISK_FULL
    #[cfg(windows)]
    const QUOTA_EXCEEDED: i32 = 1295; // ERROR_DISK_QUOTA_EXCEEDED

    #[test]
    fn a_full_disk_is_noted_until_the_writer_succeeds_again() {
        let full = || std::io::Result::<()>::Err(std::io::Error::from_raw_os_error(DISK_FULL));
        let t0 = SystemTime::UNIX_EPOCH + Duration::from_secs(1_000);
        note_at("test-b", &full(), t0);
        note_at("test-a", &full(), t0 + Duration::from_secs(5));
        // A later failure keeps the first time; another error kind changes nothing.
        note_at("test-b", &full(), t0 + Duration::from_secs(60));
        note_at("test-a", &std::io::Result::<()>::Err(std::io::ErrorKind::PermissionDenied.into()), t0);
        let (writers, since) = failing().unwrap();
        assert!(writers.starts_with(&["test-a", "test-b"]), "{writers:?}");
        assert!(since <= t0);
        note_at("test-a", &Ok(()), t0);
        note_at("test-b", &Ok(()), t0);
        assert!(failing().is_none_or(|(writers, _)| !writers.iter().any(|w| w.starts_with("test-"))));
        assert!(is_disk_full(&std::io::Error::from_raw_os_error(QUOTA_EXCEEDED)));
        #[cfg(windows)]
        assert!(is_disk_full(&std::io::Error::from_raw_os_error(39))); // ERROR_HANDLE_DISK_FULL
        assert!(!is_disk_full(&std::io::ErrorKind::NotFound.into()));
    }
}
