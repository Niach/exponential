//! EXP-862 — the IDE's tiny UI preference file (`{data_dir}/ui-prefs.json`).
//!
//! Strictly LOCAL and strictly CHROME: what the user dragged, not what the
//! product knows. Nothing here is synced, nothing here is a setting with a
//! settings row — a preference that earns a row belongs in
//! `coding::Settings`, and a preference that belongs to a WINDOW belongs in
//! [`crate::window_size`].
//!
//! EXP-877 retired its first field (the session diff pane's dragged width —
//! the pane is a full page now, and a page has no width to remember); the
//! file stayed, empty, for "the next dragged edge". EXP-1156 is that edge:
//! every sidebar column's dragged width, keyed PER PANEL by the vocabulary
//! the web client's localStorage shares (`main`, `list`, `review`,
//! `settings`, `recent`, `files`, `sourceControl` — see
//! [`crate::resize_edge::SidebarPanel::key`]). An unknown field is ignored on
//! load and an unset one is never written, so an old `ui-prefs.json` carrying
//! `diff_pane_width` simply reads as no preferences.
//!
//! Writes are DEBOUNCED. A drag fires a value per frame and a `ui-prefs.json`
//! write per frame is a hundred syscalls for one gesture, so a set updates the
//! in-memory copy immediately (every reader sees it at once) and schedules the
//! file write [`WRITE_DELAY`] later; a newer set inside that window cancels
//! the older write instead of queueing behind it.

use std::collections::BTreeMap;
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
    /// EXP-1156: each sidebar column's dragged width in logical px, keyed by
    /// [`crate::resize_edge::SidebarPanel::key`]. A missing key = the panel's
    /// token default; a reset (double-click on the edge) REMOVES the key
    /// rather than writing the default, so a later change of the default
    /// reaches everyone who never dragged. Stored UNCLAMPED: the clamp
    /// depends on the window, so it happens at read time
    /// ([`crate::resize_edge::panel_width`]) and a shrunk window never
    /// rewrites what the reader chose.
    #[serde(
        default,
        skip_serializing_if = "BTreeMap::is_empty",
        deserialize_with = "lenient_widths"
    )]
    sidebar_widths: BTreeMap<String, f32>,
}

/// EXP-1156: a hand-edited or future file must not cost the REST of the
/// widths — an entry that is not a number is dropped on its own instead of
/// failing the whole parse (which would read as "no preferences" at all).
fn lenient_widths<'de, D>(deserializer: D) -> Result<BTreeMap<String, f32>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let raw = BTreeMap::<String, serde_json::Value>::deserialize(deserializer)?;
    Ok(raw
        .into_iter()
        .filter_map(|(key, value)| Some((key, value.as_f64()? as f32)))
        .collect())
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

/// EXP-1156: the remembered width of the sidebar panel `key`, or `None` on a
/// machine that never dragged it (the caller falls back to the token
/// default). Only a finite, positive number counts; the value is UNCLAMPED.
pub(crate) fn sidebar_width(key: &str) -> Option<f32> {
    let prefs = prefs().lock().ok()?;
    stored_width(&prefs, key)
}

fn stored_width(prefs: &UiPrefs, key: &str) -> Option<f32> {
    prefs
        .sidebar_widths
        .get(key)
        .copied()
        .filter(|width| width.is_finite() && *width > 0.)
}

/// EXP-1156: remember `width` for the sidebar panel `key` (debounced, see the
/// module doc); `None` forgets it — the double-click reset. A width that is
/// not a positive number is ignored rather than persisted.
pub(crate) fn set_sidebar_width(key: &str, width: Option<f32>) {
    {
        let Ok(mut prefs) = prefs().lock() else {
            return;
        };
        if !apply_width(&mut prefs, key, width) {
            return;
        }
    }
    schedule_write();
}

/// The pure half of [`set_sidebar_width`]: true when the prefs changed (an
/// unchanged value schedules no write).
fn apply_width(prefs: &mut UiPrefs, key: &str, width: Option<f32>) -> bool {
    match width {
        Some(width) if !width.is_finite() || width <= 0. => false,
        Some(width) => prefs.sidebar_widths.insert(key.to_string(), width) != Some(width),
        None => prefs.sidebar_widths.remove(key).is_some(),
    }
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

        // EXP-877: a file written by a build that HAD a field still reads —
        // an unknown key is ignored, never a parse failure that would throw
        // away the rest of somebody's preferences.
        std::fs::create_dir_all(&dir).expect("mkdir");
        std::fs::write(&path, r#"{"diff_pane_width":512.5}"#).expect("write");
        assert_eq!(load_from(&path), Some(UiPrefs::default()));

        // An unset value is not serialized at all, so the file stays a
        // record of what was actually chosen.
        write_to(&path, &UiPrefs::default()).expect("write");
        assert_eq!(std::fs::read_to_string(&path).expect("read"), "{}");
        assert_eq!(load_from(&path), Some(UiPrefs::default()));

        std::fs::write(&path, "not json at all").expect("write");
        assert_eq!(load_from(&path), None);

        // EXP-1156: the per-panel widths round-trip under their shared keys.
        let mut prefs = UiPrefs::default();
        assert!(apply_width(&mut prefs, "main", Some(300.)));
        assert!(apply_width(&mut prefs, "sourceControl", Some(412.5)));
        write_to(&path, &prefs).expect("write");
        assert_eq!(
            std::fs::read_to_string(&path).expect("read"),
            r#"{"sidebar_widths":{"main":300.0,"sourceControl":412.5}}"#
        );
        let loaded = load_from(&path).expect("load");
        assert_eq!(loaded, prefs);
        assert_eq!(stored_width(&loaded, "main"), Some(300.));
        assert_eq!(stored_width(&loaded, "list"), None);

        // A garbage ENTRY drops alone; the rest of the widths survive it, and
        // a stored non-positive width reads as never dragged.
        std::fs::write(
            &path,
            r#"{"sidebar_widths":{"main":"wide","list":-4,"files":333},"diff_pane_width":1}"#,
        )
        .expect("write");
        let loaded = load_from(&path).expect("load");
        assert_eq!(stored_width(&loaded, "main"), None);
        assert_eq!(stored_width(&loaded, "list"), None);
        assert_eq!(stored_width(&loaded, "files"), Some(333.));

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// EXP-1156: a set only reports a change when there is one (so a drag
    /// that ends where it began schedules nothing), a reset removes the key,
    /// and a nonsense width never lands.
    #[test]
    fn a_width_sets_resets_and_refuses_nonsense() {
        let mut prefs = UiPrefs::default();
        assert!(apply_width(&mut prefs, "list", Some(400.)));
        assert!(!apply_width(&mut prefs, "list", Some(400.)), "unchanged");
        assert!(!apply_width(&mut prefs, "list", Some(f32::NAN)));
        assert!(!apply_width(&mut prefs, "list", Some(0.)));
        assert_eq!(stored_width(&prefs, "list"), Some(400.));
        assert!(apply_width(&mut prefs, "list", None), "reset removes");
        assert!(!apply_width(&mut prefs, "list", None), "nothing left to remove");
        assert_eq!(prefs, UiPrefs::default());
    }
}
