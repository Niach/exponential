//! Embedded assets: the IDE's fonts and its Lucide icon files
//! (`apps/desktop/assets`, the set `packages/icons` generates), served
//! through gpui's `AssetSource` like the IDE does; gpui-component's own
//! icons behind them. Icons resolve by CONCEPT (`nav-settings`) through
//! `packages/icons/icons.json`, a raw Lucide name works too.

use std::{borrow::Cow, collections::HashMap, sync::OnceLock};

use gpui::{AssetSource, Result, SharedString};

#[derive(rust_embed::RustEmbed)]
#[folder = "../../assets"]
#[include = "fonts/**/*.ttf"]
#[include = "icons/**/*.svg"]
struct Embedded;

const ICONS_JSON: &str = include_str!("../../../../packages/icons/icons.json");

pub struct Assets;

impl AssetSource for Assets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        if path.is_empty() {
            return Ok(None);
        }
        if let Some(file) = Embedded::get(path) {
            return Ok(Some(file.data));
        }
        gpui_component_assets::Assets.load(path)
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        let mut entries: Vec<SharedString> = Embedded::iter().filter(|p| p.starts_with(path)).map(|p| SharedString::from(p.to_string())).collect();
        entries.extend(gpui_component_assets::Assets.list(path)?);
        entries.sort();
        entries.dedup();
        Ok(entries)
    }
}

/// Every embedded font file's bytes.
pub fn fonts() -> Vec<Cow<'static, [u8]>> {
    Embedded::iter().filter(|p| p.ends_with(".ttf")).filter_map(|p| Embedded::get(&p).map(|f| f.data)).collect()
}

fn concepts() -> &'static HashMap<String, String> {
    static MAP: OnceLock<HashMap<String, String>> = OnceLock::new();
    MAP.get_or_init(|| {
        let v: serde_json::Value = serde_json::from_str(ICONS_JSON).unwrap_or_default();
        v["semantic"].as_object().map(|m| m.iter().filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string()))).collect()).unwrap_or_default()
    })
}

/// The asset path of an icon named by concept or by Lucide name.
pub fn icon_path(name: &str) -> Option<SharedString> {
    let lucide = concepts().get(name).map(String::as_str).unwrap_or(name);
    let path = format!("icons/{lucide}.svg");
    Embedded::get(&path).is_some().then(|| SharedString::from(path))
}
