//! #EXP-1406: the on-disk count of an interrupted session's recent Continues
//! (`<data_dir>/continue-attempts/<session id>`), so "once per absence"
//! survives the process: the in-memory settle in `pickup` is gone when a
//! Continue takes the daemon down with it, and the restarted daemon would
//! continue the same row again, forever.
//!
//! One small file per session: the caller holds that session's claim, so
//! nothing else writes it. Best-effort like the run registry: an unreadable
//! file or a read-only data dir counts as no attempts, never a refused Continue.

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// Continues allowed within [`WINDOW_SECS`] of each other.
pub const MAX_CONTINUES: u32 = 3;
/// A Continue this long after the last one is a new absence (an update or a
/// reboot hours later is no crash loop).
pub const WINDOW_SECS: u64 = 30 * 60;
/// Counts untouched this long are swept when another one is written.
const KEEP_SECS: u64 = 7 * 24 * 60 * 60;

fn dir(data_dir: &Path) -> PathBuf {
    data_dir.join("continue-attempts")
}

/// A row id is a uuid; anything else never becomes a path.
fn is_plain_id(session_id: &str) -> bool {
    !session_id.is_empty() && session_id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
}

fn read(path: &Path) -> Option<(u32, u64)> {
    let text = std::fs::read_to_string(path).ok()?;
    let (count, at) = text.trim().split_once(' ')?;
    Some((count.parse().ok()?, at.parse().ok()?))
}

fn sweep(dir: &Path, now: u64) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        if read(&entry.path()).is_none_or(|(_, at)| now.saturating_sub(at) > KEEP_SECS) {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

/// Count a Continue of `session_id` about to spawn; false = the cap is reached,
/// so do not continue it. Written before the spawn, so a crash in it counts.
pub fn note(data_dir: &Path, session_id: &str) -> bool {
    let now = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    note_at(data_dir, session_id, now)
}

fn note_at(data_dir: &Path, session_id: &str, now: u64) -> bool {
    if !is_plain_id(session_id) {
        return true;
    }
    let dir = dir(data_dir);
    let path = dir.join(session_id);
    let count = match read(&path) {
        Some((count, at)) if now.saturating_sub(at) <= WINDOW_SECS => count,
        _ => 0,
    };
    if count >= MAX_CONTINUES {
        return false;
    }
    let _ = std::fs::create_dir_all(&dir);
    sweep(&dir, now);
    let _ = std::fs::write(&path, format!("{} {now}", count + 1));
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("continue-attempts-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn the_cap_holds_across_processes_and_a_later_absence_starts_over() {
        let data = temp_dir("cap");
        let start = 1_000_000;
        for attempt in 0..MAX_CONTINUES {
            assert!(note_at(&data, "s1", start + u64::from(attempt) * 60));
        }
        assert!(!note_at(&data, "s1", start + 600));
        assert!(!note_at(&data, "s1", start + 600), "a refusal never resets it");
        assert!(note_at(&data, "s2", start + 600), "another session counts for itself");
        assert!(note_at(&data, "s1", start + 120 + WINDOW_SECS + 1));
        let _ = std::fs::remove_dir_all(&data);
    }

    #[test]
    fn a_corrupt_count_or_an_odd_id_never_refuses() {
        let data = temp_dir("corrupt");
        std::fs::create_dir_all(dir(&data)).unwrap();
        std::fs::write(dir(&data).join("s1"), "garbage").unwrap();
        assert!(note_at(&data, "s1", 1_000));
        for _ in 0..=MAX_CONTINUES {
            assert!(note_at(&data, "../up", 1_000));
        }
        assert!(!data.join("up").exists());
        let _ = std::fs::remove_dir_all(&data);
    }

    #[test]
    fn old_counts_are_swept() {
        let data = temp_dir("sweep");
        assert!(note_at(&data, "old", 1_000));
        assert!(note_at(&data, "new", 1_000 + KEEP_SECS + 1));
        assert!(!dir(&data).join("old").exists());
        let _ = std::fs::remove_dir_all(&data);
    }
}
