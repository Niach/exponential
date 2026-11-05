//! EXP-1654: the committed sample pictures the IDE's SPECIMEN hosts paint
//! (`packages/exponential-ui/samples/samples.json`, read its `$comment`):
//! the Exponential UI kitchen sink + specimens (an example `src` →
//! a `data:` url through the host's `resolve_url`) and the styleguide's
//! plan-page + results-tab entries (an attachment id → bytes seeded into the
//! entry's own [`ImageCache`]). Never fetched; the product never reads it.

use std::collections::HashMap;
use std::sync::LazyLock;

use crate::markdown::ImageCache;

const SAMPLES_JSON: &str = include_str!("../../../../../packages/exponential-ui/samples/samples.json");

macro_rules! sample {
    ($name:literal) => {
        ($name, include_bytes!(concat!("../../../../../packages/exponential-ui/samples/", $name)) as &[u8])
    };
}

/// Every file beside `samples.json` (a new picture = one line here).
const FILES: &[(&str, &[u8])] = &[
    sample!("alex.png"),
    sample!("cover.png"),
    sample!("data.png"),
    sample!("editor.png"),
    sample!("ios-picker.png"),
    sample!("one.png"),
    sample!("preview.png"),
    sample!("screenshot.png"),
    sample!("two.png"),
    sample!("versions.png"),
    sample!("web-picker.png"),
];

struct Samples {
    /// An exact example src → its `data:image/png;base64,…` url.
    urls: HashMap<String, String>,
    /// An attachment id → its bytes.
    attachments: HashMap<String, &'static [u8]>,
}

fn file(name: &str) -> Option<&'static [u8]> {
    FILES.iter().find(|(file, _)| *file == name).map(|(_, bytes)| *bytes)
}

fn table(json: &serde_json::Value, key: &str) -> Vec<(String, &'static [u8])> {
    json.get(key)
        .and_then(|map| map.as_object())
        .into_iter()
        .flatten()
        .filter_map(|(k, name)| {
            let bytes = name.as_str().and_then(file);
            if bytes.is_none() {
                log::warn!("[specimen pictures] {key}.{k} names a file that is not embedded: {name}");
            }
            Some((k.clone(), bytes?))
        })
        .collect()
}

static SAMPLES: LazyLock<Samples> = LazyLock::new(|| {
    let json: serde_json::Value = serde_json::from_str(SAMPLES_JSON).unwrap_or_default();
    Samples {
        urls: table(&json, "urls")
            .into_iter()
            .map(|(src, bytes)| (src, format!("data:image/png;base64,{}", eui_gpui::paint::natives::base64_encode(bytes))))
            .collect(),
        attachments: table(&json, "attachments").into_iter().collect(),
    }
});

/// A specimen host's `resolve_url`: a sample src → its bundled picture as
/// a `data:` url, anything else unchanged.
pub(crate) fn resolve_url(src: &str) -> String {
    SAMPLES.urls.get(src).cloned().unwrap_or_else(|| src.to_string())
}

/// Seed a styleguide entry's cache with every sample attachment under the
/// key the page asks for (`/api/attachments/<id>`), so it paints them
/// without a transport.
pub(crate) fn seed_attachments(cache: &mut ImageCache) {
    for (id, bytes) in &SAMPLES.attachments {
        cache.insert_bytes(format!("/api/attachments/{id}"), "image/png", bytes.to_vec());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every file samples.json names is embedded and is a PNG; every
    /// embedded file is named.
    #[test]
    fn every_sample_is_embedded_and_a_png() {
        let json: serde_json::Value = serde_json::from_str(SAMPLES_JSON).expect("samples.json parses");
        let mut named = Vec::new();
        for key in ["urls", "attachments"] {
            for (k, name) in json[key].as_object().expect(key) {
                let name = name.as_str().expect("a file name");
                let bytes = file(name).unwrap_or_else(|| panic!("{key}.{k}: {name} is not embedded"));
                assert!(bytes.starts_with(b"\x89PNG\r\n\x1a\n"), "{name} is not a PNG");
                named.push(name.to_string());
            }
        }
        for (name, _) in FILES {
            assert!(named.iter().any(|n| n == name), "{name} is embedded but unnamed");
        }
        assert_eq!(SAMPLES.urls.len(), json["urls"].as_object().unwrap().len());
        assert_eq!(SAMPLES.attachments.len(), json["attachments"].as_object().unwrap().len());
        assert!(resolve_url("https://example.com/cover.jpg").starts_with("data:image/png;base64,iVBOR"));
        assert_eq!(resolve_url("https://exponential.at/a.png"), "https://exponential.at/a.png");
        let decoded = eui_gpui::paint::natives::data_bytes(&resolve_url("https://example.com/alex.png")).unwrap();
        assert_eq!(decoded, file("alex.png").unwrap());
    }
}
