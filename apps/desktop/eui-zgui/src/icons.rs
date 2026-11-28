//! Icons from the shared registry: a concept (`packages/icons/icons.json`
//! `semantic`) → its Lucide SVG in `apps/desktop/assets/icons` (the IDE's
//! and `exponential-shot`'s rule), else the name itself as a Lucide glyph.
//! The SVG is handed to zgui's `vector` element as a whole document; its
//! `currentColor` strokes follow the element's `color`.

use std::collections::HashMap;
use std::sync::OnceLock;

fn concepts() -> &'static HashMap<String, String> {
    static MAP: OnceLock<HashMap<String, String>> = OnceLock::new();
    MAP.get_or_init(|| {
        let raw = std::fs::read_to_string(crate::repo().join("packages/icons/icons.json")).unwrap_or_default();
        let v: serde_json::Value = serde_json::from_str(&raw).unwrap_or_default();
        v["semantic"].as_object().map(|m| m.iter().filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string()))).collect()).unwrap_or_default()
    })
}

/// The SVG document of an icon concept, `None` = not in the registry.
pub fn svg(name: &str) -> Option<String> {
    static CACHE: OnceLock<std::sync::Mutex<HashMap<String, Option<String>>>> = OnceLock::new();
    let cache = CACHE.get_or_init(Default::default);
    if let Some(hit) = cache.lock().unwrap().get(name) {
        return hit.clone();
    }
    let lucide = concepts().get(name).cloned().unwrap_or_else(|| name.strip_prefix("ui-").unwrap_or(name).to_string());
    let path = crate::repo().join(format!("apps/desktop/assets/icons/{lucide}.svg"));
    let doc = std::fs::read_to_string(path).ok();
    cache.lock().unwrap().insert(name.to_string(), doc.clone());
    doc
}
