//! Step 3's icon set: the one SVG, rendered at every size a target asks for (`icons`).

use anyhow::{anyhow, Context as _, Result};
use resvg::{tiny_skia, usvg};

/// A parsed icon: render it at any size.
pub struct Icon {
    tree: usvg::Tree,
}

impl Icon {
    pub fn from_svg(svg: &[u8]) -> Result<Self> {
        let tree = usvg::Tree::from_data(svg, &usvg::Options::default()).context("the icon is not an SVG")?;
        Ok(Self { tree })
    }

    /// The icon as an RGBA square of `size` px, scaled to fit, centred.
    pub fn rgba(&self, size: u32) -> Result<Vec<u8>> {
        let mut pixmap = tiny_skia::Pixmap::new(size, size).ok_or_else(|| anyhow!("icon size {size}"))?;
        let s = self.tree.size();
        let scale = (size as f32 / s.width()).min(size as f32 / s.height());
        let dx = (size as f32 - s.width() * scale) / 2.0;
        let dy = (size as f32 - s.height() * scale) / 2.0;
        resvg::render(&self.tree, tiny_skia::Transform::from_scale(scale, scale).post_translate(dx, dy), &mut pixmap.as_mut());
        // tiny-skia keeps premultiplied alpha; PNG wants straight alpha.
        Ok(pixmap.pixels().iter().flat_map(|p| {
            let c = p.demultiply();
            [c.red(), c.green(), c.blue(), c.alpha()]
        }).collect())
    }

    /// The icon as a PNG of `size` px.
    pub fn png(&self, size: u32) -> Result<Vec<u8>> {
        let rgba = self.rgba(size)?;
        let mut out = Vec::new();
        image::codecs::png::PngEncoder::new(&mut out).write_image(&rgba, size, size, image::ExtendedColorType::Rgba8).context("encode the icon PNG")?;
        Ok(out)
    }
}

use image::ImageEncoder as _;

/// The sizes of a target's icon set (`icons.<target>`: a list, or `{density: px}`).
pub fn sizes(target: &str) -> Vec<(String, u32)> {
    let v = &crate::contract::BUNDLE["icons"][target];
    match v {
        serde_json::Value::Array(a) => a.iter().filter_map(|n| n.as_u64()).map(|n| (n.to_string(), n as u32)).collect(),
        serde_json::Value::Object(o) => o.iter().filter_map(|(k, n)| n.as_u64().map(|n| (k.clone(), n as u32))).collect(),
        _ => Vec::new(),
    }
}
