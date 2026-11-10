//! EXP-1654: the committed sample pictures the IDE's SPECIMEN hosts paint
//! (`packages/exponential-ui/samples/samples.json`, read its `$comment`):
//! the Exponential UI kitchen sink + specimens (an example `src` →
//! a `data:` url through the host's `resolve_url`) and the dev-only
//! `crate::specimens` plan-page + results-tab entries (an attachment id → bytes seeded into the
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

/// Seed a specimen's cache with every sample attachment under the
/// key the page asks for (`/api/attachments/<id>`), so it paints them
/// without a transport.
pub(crate) fn seed_attachments(cache: &mut ImageCache) {
    for (id, bytes) in &SAMPLES.attachments {
        cache.insert_bytes(format!("/api/attachments/{id}"), "image/png", bytes.to_vec());
    }
}

/// The `plan-card` specimen's stand-in picture = the web's SVG: a diagonal
/// gradient `from` → `to` (top left → bottom right) under a 35% white bar
/// and a 12% white panel, drawn on an 800 × 500 grid scaled to `w` × `h`.
pub(crate) fn picture_png(from: [u8; 3], to: [u8; 3], w: u32, h: u32) -> Vec<u8> {
    // `(x, y, w, h, radius, white alpha)` on the 800 × 500 grid.
    const SHAPES: [(f32, f32, f32, f32, f32, f32); 2] =
        [(48., 48., 320., 28., 6., 0.35), (48., 100., 704., 352., 12., 0.12)];
    let in_rounded_rect = |px: f32, py: f32, (x, y, w, h, r): (f32, f32, f32, f32, f32)| {
        if px < x || py < y || px > x + w || py > y + h {
            return false;
        }
        let dx = (x + r - px).max(px - (x + w - r)).max(0.);
        let dy = (y + r - py).max(py - (y + h - r)).max(0.);
        dx * dx + dy * dy <= r * r
    };
    let (sx, sy) = (w as f32 / 800., h as f32 / 500.);
    let image = image::RgbaImage::from_fn(w, h, |x, y| {
        let (cx, cy) = (x as f32 + 0.5, y as f32 + 0.5);
        let t = ((cx / w as f32) + (cy / h as f32)) / 2.;
        let mut rgb: [f32; 3] = std::array::from_fn(|i| from[i] as f32 + (to[i] as f32 - from[i] as f32) * t);
        for (bx, by, bw, bh, br, alpha) in SHAPES {
            if in_rounded_rect(cx / sx, cy / sy, (bx, by, bw, bh, br)) {
                rgb = rgb.map(|c| c + (255. - c) * alpha);
            }
        }
        let [r, g, b] = rgb.map(|c| c.round().clamp(0., 255.) as u8);
        image::Rgba([r, g, b, 255])
    });
    let mut bytes = Vec::new();
    image::DynamicImage::ImageRgba8(image)
        .write_to(&mut std::io::Cursor::new(&mut bytes), image::ImageFormat::Png)
        .expect("png encodes");
    bytes
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
