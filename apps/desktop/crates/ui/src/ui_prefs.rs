//! EXP-862 — the IDE's tiny UI preference file (`{data_dir}/ui-prefs.json`).
//!
//! Strictly LOCAL and strictly CHROME: what the user dragged, not what the
//! product knows. Nothing here is synced, nothing here is a setting with a
//! settings row — a preference that earns a row belongs in
//! `coding::Settings`, and a preference that belongs to a WINDOW belongs in
//! [`crate::window_size`].
//!
//! Today it holds exactly one value, the session diff pane's width, which is
//! why it is one file rather than one file per pane: the next dragged edge
//! adds a field, not a path.
//!
//! Writes are DEBOUNCED. A drag fires a value per frame and a `ui-prefs.json`
//! write per frame is a hundred syscalls for one gesture, so a set updates the
//! in-memory copy immediately (every reader sees it at once) and schedules the
//! file write [`WRITE_DELAY`] later; a newer set inside that window cancels
//! the older write instead of queueing behind it.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use serde::{Deserialize, Serialize};

/// How long a set waits before it hits the disk (see the module doc).
const WRITE_DELAY: Duration = Duration::from_millis(500);

/// The whole file. Every field is optional: an older build's file stays
/// readable, and a value this build never writes is not invented on load.
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
struct UiPrefs {
    /// EXP-850 §11 / EXP-862: the session diff pane's dragged width, in
    /// logical px. ONE width for every session — the pane is one surface
    /// that different runs happen to fill, and a per-run width would mean
    /// the same drag has to be repeated on the next run.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    diff_pane_width: Option<f32>,
}

fn prefs_file() -> Option<PathBuf> {
    Some(crate::window_size::app_data_dir()?.join("ui-prefs.json"))
}

/// The process-wide copy, loaded from disk on first touch.
fn prefs() -> &'static Mutex<UiPrefs> {
    static PREFS: OnceLock<Mutex<UiPrefs>> = OnceLock::new();
    PREFS.get_or_init(|| {
        Mutex::new(
            prefs_file()
                .and_then(|path| load_from(&path))
                .unwrap_or_default(),
        )
    })
}

/// The newest scheduled write. A thread whose generation is no longer the
/// newest drops its write on the floor — that is the debounce.
static WRITE_GENERATION: AtomicU64 = AtomicU64::new(0);

fn load_from(path: &std::path::Path) -> Option<UiPrefs> {
    let json = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&json).ok()
}

fn write_to(path: &std::path::Path, prefs: &UiPrefs) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string(prefs).unwrap_or_else(|_| "{}".to_string());
    std::fs::write(path, json)
}

/// Schedule the current in-memory prefs to be written once the drag stops.
fn schedule_write() {
    let generation = WRITE_GENERATION.fetch_add(1, Ordering::SeqCst) + 1;
    std::thread::spawn(move || {
        std::thread::sleep(WRITE_DELAY);
        // A newer set landed while we slept — it owns the write.
        if WRITE_GENERATION.load(Ordering::SeqCst) != generation {
            return;
        }
        let Some(path) = prefs_file() else {
            return;
        };
        let snapshot = match prefs().lock() {
            Ok(prefs) => prefs.clone(),
            Err(_) => return,
        };
        // Best effort: a failed write only costs the remembered width.
        let _ = write_to(&path, &snapshot);
    });
}

/// The remembered diff-pane width, or `None` on a machine that has never
/// dragged it (the caller opens at [`crate::diff_pane::pane_width`]).
pub(crate) fn diff_pane_width() -> Option<f32> {
    prefs()
        .lock()
        .ok()
        .and_then(|prefs| prefs.diff_pane_width)
        .filter(|width| width.is_finite() && *width > 0.)
}

/// Remember `width` as the diff pane's width (debounced, see the module doc).
/// A width that is not a positive number is ignored rather than persisted.
pub(crate) fn set_diff_pane_width(width: f32) {
    if !width.is_finite() || width <= 0. {
        return;
    }
    {
        let Ok(mut prefs) = prefs().lock() else {
            return;
        };
        if prefs.diff_pane_width == Some(width) {
            return;
        }
        prefs.diff_pane_width = Some(width);
    }
    schedule_write();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|since| since.as_nanos())
            .unwrap_or_default();
        std::env::temp_dir().join(format!("exp-ui-prefs-{name}-{}-{stamp}", std::process::id()))
    }

    /// The file round-trips, an ABSENT value stays absent (an older build's
    /// file must not grow fields it never set), and garbage reads as "no
    /// preferences" rather than as a panic.
    #[test]
    fn the_prefs_file_round_trips_and_survives_garbage() {
        let dir = scratch("round-trip");
        let path = dir.join("ui-prefs.json");
        assert_eq!(load_from(&path), None, "a missing file is no preferences");

        write_to(
            &path,
            &UiPrefs {
                diff_pane_width: Some(512.5),
            },
        )
        .expect("write");
        assert_eq!(
            load_from(&path).and_then(|prefs| prefs.diff_pane_width),
            Some(512.5)
        );

        // An unset value is not serialized at all, so the file stays a
        // record of what was actually chosen.
        write_to(&path, &UiPrefs::default()).expect("write");
        assert_eq!(std::fs::read_to_string(&path).expect("read"), "{}");
        assert_eq!(load_from(&path), Some(UiPrefs::default()));

        std::fs::write(&path, "not json at all").expect("write");
        assert_eq!(load_from(&path), None);

        let _ = std::fs::remove_dir_all(&dir);
    }
}
