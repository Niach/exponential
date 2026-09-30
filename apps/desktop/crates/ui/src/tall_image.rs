//! EXP-1128 — pictures too big for ONE GPU texture.
//!
//! gpui uploads a decoded image as a single atlas texture, and the atlas
//! clamps every side to [`MAX_TEXTURE_SIDE`] (Metal's `MAX_ATLAS_SIZE`; wgpu
//! clamps to the adapter's `max_texture_dimension_2d` the same way). A
//! bigger picture — a 780×25094 full-page screenshot — fails its atlas
//! allocation, `img` only logs the paint error, and the tile paints NOTHING
//! while its cache slot says Ready.
//!
//! So the [`crate::markdown::ImageCache`] decodes such a picture itself, on
//! the background executor, and cuts it into horizontal [`TallStrip`]s of at
//! most [`TALL_STRIP_ROWS`] rows (narrowing it to [`MAX_TEXTURE_SIDE`] first
//! when it is also too wide). [`render_tall`] stacks the strips back into one
//! column at any display width.

use std::sync::Arc;

use gpui::{
    img, px, AnyElement, IntoElement as _, ObjectFit, ParentElement as _, RenderImage, Styled as _,
    StyledImage as _,
};
use gpui_component::v_flex;

/// The biggest texture side gpui can upload (the Metal atlas cap,
/// `gpui_macos::metal_atlas::MAX_ATLAS_SIZE`). Anything larger on either
/// side goes through the strip path.
pub(crate) const MAX_TEXTURE_SIDE: u32 = 16384;

/// Rows per strip — comfortably under every backend's texture cap, and small
/// enough that one strip's BGRA upload stays a few tens of MB.
pub(crate) const TALL_STRIP_ROWS: u32 = 4096;

/// A decoded picture split into stacked strips, each its own texture.
pub(crate) struct TallImage {
    /// The (possibly narrowed) decoded size the strips add up to.
    pub width: u32,
    pub height: u32,
    pub strips: Vec<TallStrip>,
}

/// One horizontal slice of a [`TallImage`], `rows` pixels tall.
pub(crate) struct TallStrip {
    pub image: Arc<RenderImage>,
    pub rows: u32,
}

/// `(y, rows)` chunks covering `height` rows, `rows` at a time; the last one
/// carries the remainder.
pub(crate) fn tall_image_strip_ranges(height: u32, rows: u32) -> Vec<(u32, u32)> {
    if rows == 0 {
        return Vec::new();
    }
    let mut ranges = Vec::new();
    let mut y = 0;
    while y < height {
        let chunk = rows.min(height - y);
        ranges.push((y, chunk));
        y += chunk;
    }
    ranges
}

/// Whether `bytes` need the strip path: the header probe says a side is over
/// [`MAX_TEXTURE_SIDE`]. A failed probe keeps the ordinary path.
pub(crate) fn needs_strips(bytes: &[u8]) -> bool {
    match imagesize::blob_size(bytes) {
        Ok(size) => size.width.max(size.height) > MAX_TEXTURE_SIDE as usize,
        Err(_) => false,
    }
}

/// Decode `bytes` and cut them into strips. Runs on the BACKGROUND executor —
/// a full decode of a 25k-row screenshot is far too slow for the foreground.
pub(crate) fn decode_tall(bytes: &[u8]) -> anyhow::Result<TallImage> {
    let decoded = image::load_from_memory(bytes)?;
    let (source_w, source_h) = (decoded.width(), decoded.height());
    let mut rgba = decoded.to_rgba8();
    if rgba.width() > MAX_TEXTURE_SIDE {
        let height = ((rgba.height() as f64) * MAX_TEXTURE_SIDE as f64 / rgba.width() as f64)
            .round()
            .max(1.0) as u32;
        rgba = image::imageops::resize(
            &rgba,
            MAX_TEXTURE_SIDE,
            height,
            image::imageops::FilterType::Triangle,
        );
    }
    let (width, height) = (rgba.width(), rgba.height());
    log::info!(
        "[ui] tall image {source_w}×{source_h} → {width}×{height} in strips of {TALL_STRIP_ROWS} rows"
    );
    let raw = rgba.into_raw();
    let stride = width as usize * 4;
    let mut strips = Vec::new();
    for (y, rows) in tall_image_strip_ranges(height, TALL_STRIP_ROWS) {
        let start = y as usize * stride;
        let end = start + rows as usize * stride;
        let mut pixels = raw[start..end].to_vec();
        // gpui's sprite pipeline expects BGRA (terminal `graphic_to_frame`).
        for pixel in pixels.chunks_exact_mut(4) {
            pixel.swap(0, 2);
        }
        let buffer = image::RgbaImage::from_raw(width, rows, pixels)
            .ok_or_else(|| anyhow::anyhow!("strip buffer size mismatch"))?;
        let frame = image::Frame::new(buffer);
        strips.push(TallStrip {
            image: Arc::new(RenderImage::new(vec![frame])),
            rows,
        });
    }
    Ok(TallImage {
        width,
        height,
        strips,
    })
}

/// The strips stacked into one column `display_width` wide, each at its
/// aspect-true height. `Fill` so gpui never letterboxes a strip (the box is
/// already exact).
pub(crate) fn render_tall(tall: &TallImage, display_width: f32) -> AnyElement {
    let scale = display_width / tall.width.max(1) as f32;
    v_flex()
        .flex_shrink_0()
        .w(px(display_width))
        .children(tall.strips.iter().map(|strip| {
            img(strip.image.clone())
                .flex_shrink_0()
                .w(px(display_width))
                .h(px(strip.rows as f32 * scale))
                .object_fit(ObjectFit::Fill)
        }))
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tall_image_strip_ranges_cover_the_height() {
        assert!(tall_image_strip_ranges(0, TALL_STRIP_ROWS).is_empty());
        assert_eq!(
            tall_image_strip_ranges(4096, TALL_STRIP_ROWS),
            vec![(0, 4096)]
        );
        let ranges = tall_image_strip_ranges(25094, TALL_STRIP_ROWS);
        assert_eq!(ranges.len(), 7);
        assert_eq!(ranges.last(), Some(&(6 * 4096, 518)));
        assert_eq!(ranges.iter().map(|(_, rows)| rows).sum::<u32>(), 25094);
        for pair in ranges.windows(2) {
            assert_eq!(pair[0].0 + pair[0].1, pair[1].0);
        }
    }

    #[test]
    fn tall_image_decode_splits_an_oversized_png() {
        // 2×(MAX+10) rows: over the texture cap, so it strips.
        let height = MAX_TEXTURE_SIDE + 10;
        let buffer = image::RgbaImage::from_pixel(2, height, image::Rgba([10, 20, 30, 255]));
        let mut bytes = Vec::new();
        image::DynamicImage::ImageRgba8(buffer)
            .write_to(
                &mut std::io::Cursor::new(&mut bytes),
                image::ImageFormat::Png,
            )
            .unwrap();
        assert!(needs_strips(&bytes));
        let tall = decode_tall(&bytes).unwrap();
        assert_eq!((tall.width, tall.height), (2, height));
        assert_eq!(tall.strips.len(), 5);
        assert_eq!(tall.strips.last().unwrap().rows, 10);
    }

    #[test]
    fn tall_image_small_pictures_keep_the_plain_path() {
        let buffer = image::RgbaImage::from_pixel(4, 4, image::Rgba([0, 0, 0, 255]));
        let mut bytes = Vec::new();
        image::DynamicImage::ImageRgba8(buffer)
            .write_to(
                &mut std::io::Cursor::new(&mut bytes),
                image::ImageFormat::Png,
            )
            .unwrap();
        assert!(!needs_strips(&bytes));
        assert!(!needs_strips(b"not an image"));
    }
}
