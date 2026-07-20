//! Last-used window size persistence (EXP-210) — strictly LOCAL (a plain
//! JSON file in the app data dir, never synced to the server).
//!
//! The main shell window (ordinal 0) records its size on every resize
//! (debounced in [`crate::shell::Shell`], flushed on quit); the next launch
//! opens at that size instead of the fixed default. Sizes are clamped to
//! [`MIN_SIZE`] — the floor below which page layouts break — which is also
//! the `window_min_size` every shell window is opened with.

use std::path::PathBuf;

use anyhow::{anyhow, Context as _, Result};
use gpui::{px, size, Pixels, Size};
use serde::{Deserialize, Serialize};

/// Minimum shell-window content size (EXP-210: layouts break below 800×600).
pub const MIN_SIZE: Size<Pixels> = Size {
    width: px(800.),
    height: px(600.),
};

/// First-launch default (no persisted size yet) — §3.6.
pub const DEFAULT_SIZE: Size<Pixels> = Size {
    width: px(1280.),
    height: px(820.),
};

/// Logical (scale-independent) pixels, the unit gpui bounds already use.
#[derive(Serialize, Deserialize)]
struct SavedSize {
    width: f32,
    height: f32,
}

/// The app-local data dir shared with the per-window layout files. macOS:
/// `~/Library/Application Support/Exponential/…`; Linux:
/// `~/.local/share/exponential/…`.
pub(crate) fn app_data_dir() -> Option<PathBuf> {
    Some(dirs::data_local_dir()?.join(if cfg!(target_os = "macos") {
        "Exponential"
    } else {
        "exponential"
    }))
}

fn size_file() -> Option<PathBuf> {
    Some(app_data_dir()?.join("window-size.json"))
}

/// The persisted last-used size, clamped to [`MIN_SIZE`]; `None` on first
/// launch or an unreadable/garbled file (callers fall back to
/// [`DEFAULT_SIZE`]).
pub fn load_last_size() -> Option<Size<Pixels>> {
    let json = std::fs::read_to_string(size_file()?).ok()?;
    let saved: SavedSize = serde_json::from_str(&json).ok()?;
    if !saved.width.is_finite() || !saved.height.is_finite() {
        return None;
    }
    Some(size(
        px(saved.width).max(MIN_SIZE.width),
        px(saved.height).max(MIN_SIZE.height),
    ))
}

/// Persist `last` as the next launch's window size (best-effort at call
/// sites — a failed write only costs the remembered size).
pub fn save_last_size(last: Size<Pixels>) -> Result<()> {
    let path = size_file().ok_or_else(|| anyhow!("no data dir"))?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let saved = SavedSize {
        width: f32::from(last.width),
        height: f32::from(last.height),
    };
    let json = serde_json::to_string(&saved).context("serialize window size")?;
    std::fs::write(&path, json).context("write window size")?;
    Ok(())
}
