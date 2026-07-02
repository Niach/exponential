//! Embedded asset source (masterplan-v3 §3.2 / §3.7).
//!
//! Everything ships inside the binary via gpui's `AssetSource` — **no runtime
//! asset paths** to break inside an `.app` or AppImage. Two layers:
//!
//! 1. Our own `apps/desktop/assets/` (Inter TTFs now; the ExpIcon Lucide set
//!    lands there in Phase 3 — the `icons/**` include already covers it).
//! 2. Fallback to `gpui_component_assets::Assets`, the icon set
//!    gpui-component's `IconName` enum is generated from (its widgets load
//!    `icons/*.svg` through the app's asset source at render time).

use std::borrow::Cow;

use anyhow::anyhow;
use gpui::{App, AssetSource, Result, SharedString};

/// Our own embedded files (fonts + future icons/packaging templates).
#[derive(rust_embed::RustEmbed)]
#[folder = "../../assets"]
#[include = "fonts/**/*.ttf"]
#[include = "icons/**/*.svg"]
struct EmbeddedAssets;

/// The app-wide asset source handed to `gpui_platform::application()
/// .with_assets(…)` (§3.6): our assets first, gpui-component's icons second.
pub struct Assets;

impl AssetSource for Assets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        if path.is_empty() {
            return Ok(None);
        }

        if let Some(file) = EmbeddedAssets::get(path) {
            return Ok(Some(file.data));
        }

        // gpui-component widget icons (IconName) — same lookup contract.
        gpui_component_assets::Assets
            .load(path)
            .map_err(|_| anyhow!("could not find asset at path \"{path}\""))
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        let mut entries: Vec<SharedString> = EmbeddedAssets::iter()
            .filter_map(|p| p.starts_with(path).then(|| SharedString::from(p.to_string())))
            .collect();
        entries.extend(gpui_component_assets::Assets.list(path)?);
        entries.sort();
        entries.dedup();
        Ok(entries)
    }
}

/// Register the embedded Inter faces with the text system (§3.2: "registered
/// at startup with `cx.text_system().add_fonts(...)` so there is no runtime
/// font path"). Must run before the theme's `font_family = "Inter"` is ever
/// used by a window.
pub fn load_embedded_fonts(cx: &App) {
    let fonts: Vec<Cow<'static, [u8]>> = EmbeddedAssets::iter()
        .filter(|path| path.starts_with("fonts/") && path.ends_with(".ttf"))
        .filter_map(|path| EmbeddedAssets::get(&path).map(|file| file.data))
        .collect();

    if fonts.is_empty() {
        // Assets are compile-time embedded; an empty set means the build is
        // broken, not a user-environment problem.
        panic!("no embedded fonts found in assets/fonts — build is missing the Inter TTFs");
    }

    cx.text_system()
        .add_fonts(fonts)
        .expect("failed to register embedded Inter fonts");
}
