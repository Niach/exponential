//! UI zoom (EXP-85) — a per-install scale factor for the whole IDE chrome.
//!
//! gpui already has the mechanism built in: the Tailwind-style spacing/text
//! helpers (`p_2`, `h_8`, `text_sm`, …) are all REM-based, and
//! gpui-component's `Root` sets the window rem size from
//! `theme.font_size` on every render. So scaling the theme's base font size
//! scales text AND rem-sized boxes together — the same trick Zed's UI zoom
//! uses. Explicit `px(…)` values (the terminal grid, the diff view's code
//! text, hairline borders) deliberately stay fixed.
//!
//! The factor persists as the top-level `uiScale` key in the same local
//! per-install `settings.json` the coding launcher uses (both writers
//! merge-preserve foreign keys — see `coding::Settings::save`). Never synced.

use gpui::{px, App};
use gpui_component::theme::Theme;
use serde_json::{json, Value};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::AuthContext;

/// Slider floor — a notch of "smaller" headroom under the default.
pub const MIN_SCALE: f32 = 0.8;
/// Slider ceiling — 150% is plenty before px-fixed chrome looks off.
pub const MAX_SCALE: f32 = 1.5;
/// Snap step (5%): keeps persisted values tidy and the slider discrete.
pub const SCALE_STEP: f32 = 0.05;
pub const DEFAULT_SCALE: f32 = 1.0;

const KEY: &str = "uiScale";

/// Load + apply the persisted scale at startup. Call from `ui::init`, AFTER
/// `theme::init` (which resets `theme.font_size` to the base constant).
pub fn init(cx: &mut App) {
    let scale = load(&settings_file(cx));
    if scale != DEFAULT_SCALE {
        apply(scale, cx);
    }
}

/// The applied scale, recovered from the live theme (the single source of
/// truth — stays correct even if another surface changed it).
pub fn current(cx: &App) -> f32 {
    normalize(f32::from(Theme::global(cx).font_size) / theme::FONT_SIZE_PX)
}

/// Re-derive the theme's base font size from the constant (idempotent) and
/// redraw every window — `Root::render` picks the new rem size up from
/// `theme.font_size` on the next frame.
pub fn apply(scale: f32, cx: &mut App) {
    let scale = normalize(scale);
    Theme::global_mut(cx).font_size = px(theme::FONT_SIZE_PX * scale);
    cx.refresh_windows();
}

/// Canonical settings file: `{data_dir}/settings.json` — shared with the
/// coding launcher settings.
pub fn settings_file(cx: &App) -> PathBuf {
    let data_dir = cx
        .try_global::<AuthContext>()
        .map(|auth| auth.data_dir.clone())
        .unwrap_or_else(api::default_data_dir);
    coding::Settings::default_path(&data_dir)
}

/// Missing/corrupt file or a non-numeric key → the default; anything else is
/// normalized into the slider's closed range so a hand-edited file can never
/// produce an unreadable UI.
pub fn load(path: &Path) -> f32 {
    fs::read_to_string(path)
        .ok()
        .and_then(|raw| serde_json::from_str::<Value>(&raw).ok())
        .and_then(|root| root.get(KEY).and_then(Value::as_f64))
        .map(|raw| normalize(raw as f32))
        .unwrap_or(DEFAULT_SCALE)
}

/// Persist, merging over the existing JSON object so keys owned by other
/// subsystems (the coding launcher settings) survive.
pub fn save(path: &Path, scale: f32) -> io::Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let mut root = fs::read_to_string(path)
        .ok()
        .and_then(|raw| serde_json::from_str::<Value>(&raw).ok())
        .filter(Value::is_object)
        .unwrap_or_else(|| Value::Object(Default::default()));
    if let Some(target) = root.as_object_mut() {
        target.insert(KEY.to_string(), json!(normalize(scale)));
    }
    let mut rendered = serde_json::to_string_pretty(&root).expect("render settings json");
    rendered.push('\n');
    fs::write(path, rendered)
}

/// Clamp into `[MIN_SCALE, MAX_SCALE]` and snap to [`SCALE_STEP`]; non-finite
/// values fall back to the default.
pub fn normalize(raw: f32) -> f32 {
    if !raw.is_finite() {
        return DEFAULT_SCALE;
    }
    let clamped = raw.clamp(MIN_SCALE, MAX_SCALE);
    (clamped / SCALE_STEP).round() * SCALE_STEP
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TempDir(PathBuf);

    impl TempDir {
        fn new(tag: &str) -> Self {
            let mut path = std::env::temp_dir();
            let nanos = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            path.push(format!("exp-ui-scale-{tag}-{}-{nanos}", std::process::id()));
            fs::create_dir_all(&path).unwrap();
            Self(path)
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn approx(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-4
    }

    #[test]
    fn normalize_clamps_snaps_and_defaults() {
        assert!(approx(normalize(1.0), 1.0));
        assert!(approx(normalize(1.1), 1.1));
        assert!(approx(normalize(1.12), 1.1), "snaps to the 5% step");
        assert!(approx(normalize(0.1), MIN_SCALE), "clamps the floor");
        assert!(approx(normalize(9.0), MAX_SCALE), "clamps the ceiling");
        assert!(approx(normalize(f32::NAN), DEFAULT_SCALE));
    }

    #[test]
    fn missing_or_corrupt_file_loads_the_default() {
        let dir = TempDir::new("missing");
        let path = dir.0.join("settings.json");
        assert!(approx(load(&path), DEFAULT_SCALE));
        fs::write(&path, "{not json").unwrap();
        assert!(approx(load(&path), DEFAULT_SCALE));
        fs::write(&path, r#"{"uiScale":"big"}"#).unwrap();
        assert!(approx(load(&path), DEFAULT_SCALE));
    }

    #[test]
    fn save_load_round_trips_and_normalizes() {
        let dir = TempDir::new("roundtrip");
        let path = dir.0.join("settings.json");
        save(&path, 1.1).unwrap();
        assert!(approx(load(&path), 1.1));
        // Out-of-range hand edits normalize on load.
        fs::write(&path, r#"{"uiScale":12.0}"#).unwrap();
        assert!(approx(load(&path), MAX_SCALE));
    }

    #[test]
    fn save_preserves_the_coding_launcher_keys() {
        let dir = TempDir::new("merge");
        let path = dir.0.join("settings.json");
        fs::write(&path, r#"{"claudePath":"/opt/claude","deviceId":"dev-1"}"#).unwrap();
        save(&path, 1.25).unwrap();
        let root: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(root["claudePath"], "/opt/claude");
        assert_eq!(root["deviceId"], "dev-1");
        assert!(approx(root["uiScale"].as_f64().unwrap() as f32, 1.25));
        // …and the coding settings loader tolerates our foreign key.
        let settings = coding::Settings::load(&path);
        assert_eq!(settings.claude_path, "/opt/claude");
    }
}
