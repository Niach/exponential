//! EXP-2099: the usage-wall resets this device's sessions wait for
//! (`<data_dir>/wall-resume/<session id>`, unix ms), so a run the wall
//! stopped carries on after a restart or an idle release too. Written and
//! cleared by the engine (`eagent::engine::wall_resume`); read by the
//! pickup for a session with no engine ([`due`]).
//!
//! One small file per session, like `continue_attempts`: only the process
//! holding the session's claim writes it. Best-effort.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

/// A reset passed longer ago than this is stale (the session ended, or
/// moved away): swept.
const KEEP_MS: i64 = 7 * 24 * 60 * 60 * 1000;

fn dir(data_dir: &Path) -> PathBuf {
    data_dir.join("wall-resume")
}

/// A row id is a uuid; anything else never becomes a path.
fn path(data_dir: &Path, session_id: &str) -> Option<PathBuf> {
    let plain = !session_id.is_empty() && session_id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-');
    plain.then(|| dir(data_dir).join(session_id))
}

fn read(path: &Path) -> Option<i64> {
    std::fs::read_to_string(path).ok()?.trim().parse().ok()
}

pub fn get(data_dir: &Path, session_id: &str) -> Option<i64> {
    read(&path(data_dir, session_id)?)
}

/// Keep (or, for `None`, drop) the reset `session_id` waits for.
pub fn set(data_dir: &Path, session_id: &str, resets_at_ms: Option<i64>) {
    let Some(path) = path(data_dir, session_id) else { return };
    match resets_at_ms {
        Some(at) => {
            let _ = std::fs::create_dir_all(dir(data_dir));
            if let Err(err) = std::fs::write(&path, at.to_string()) {
                log::warn!("wall resume: {} not written: {err}", path.display());
            }
        }
        None => {
            let _ = std::fs::remove_file(path);
        }
    }
}

/// The sessions whose reset has passed (by the walls' clock-skew grace).
pub fn due(data_dir: &Path, now_ms: i64) -> HashSet<String> {
    let Ok(entries) = std::fs::read_dir(dir(data_dir)) else {
        return HashSet::new();
    };
    let mut due = HashSet::new();
    for entry in entries.flatten() {
        let Some(at) = read(&entry.path()) else { continue };
        if now_ms - at > KEEP_MS {
            let _ = std::fs::remove_file(entry.path());
        } else if eagent::wire::frames::rate_limit_expired(Some(at), now_ms) {
            due.extend(entry.file_name().into_string().ok().filter(|id| path(data_dir, id).is_some()));
        }
    }
    due
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_reset_is_kept_due_after_its_grace_and_swept_when_stale() {
        let data_dir = std::env::temp_dir().join(format!("wall-resume-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&data_dir);
        let at = 1_700_000_000_000;
        set(&data_dir, "s-1", Some(at));
        set(&data_dir, "../evil", Some(at));
        assert_eq!(get(&data_dir, "s-1"), Some(at));
        assert!(due(&data_dir, at).is_empty());
        assert_eq!(due(&data_dir, at + 60_001), HashSet::from(["s-1".to_string()]));
        assert!(due(&data_dir, at + KEEP_MS + 1).is_empty());
        assert_eq!(get(&data_dir, "s-1"), None);
        set(&data_dir, "s-1", Some(at));
        set(&data_dir, "s-1", None);
        assert_eq!(get(&data_dir, "s-1"), None);
        let _ = std::fs::remove_dir_all(&data_dir);
    }
}
