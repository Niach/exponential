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
//! most [`TALL_STRIP_ROWS`] rows (narrowing it to [`MAX_TALL_WIDTH`] first
//! when it is wider than any column it can be shown in). [`render_tall`]
//! stacks the strips back into one column at any display width.

use std::sync::{Arc, Mutex};

use gpui::{
    img, px, AnyElement, ImageId, IntoElement as _, ObjectFit, ParentElement as _, RenderImage,
    Styled as _, StyledImage as _, Window,
};
use gpui_component::v_flex;

/// The biggest texture side gpui can upload (the Metal atlas cap,
/// `gpui_macos::metal_atlas::MAX_ATLAS_SIZE`). Anything larger on either
/// side goes through the strip path.
pub(crate) const MAX_TEXTURE_SIDE: u32 = 16384;

/// Rows per strip — comfortably under every backend's texture cap, and small
/// enough that one strip's BGRA upload stays a few tens of MB.
pub(crate) const TALL_STRIP_ROWS: u32 = 4096;

/// The widest a strip-decoded picture is kept: it is only ever painted in a
/// column at most ~1100 px wide (the lightbox), so 2048 px covers a 2×
/// display and cuts a wide capture's BGRA to a fraction before it is split.
pub(crate) const MAX_TALL_WIDTH: u32 = 2048;

/// The size a `source_w`×`source_h` decode is narrowed to before the split:
/// aspect-true at [`MAX_TALL_WIDTH`] when wider, unchanged otherwise.
pub(crate) fn narrowed_size(source_w: u32, source_h: u32) -> (u32, u32) {
    if source_w <= MAX_TALL_WIDTH {
        return (source_w, source_h);
    }
    let height = ((source_h as f64) * MAX_TALL_WIDTH as f64 / source_w as f64)
        .round()
        .max(1.0) as u32;
    (MAX_TALL_WIDTH, height)
}

/// Atlas keys of strips whose [`TallImage`] dropped, waiting for a window to
/// release them ([`release_dropped_strips`]). `Window::drop_image` keys on
/// the image id + frame count, never the pixels, so this 16-byte record is
/// all that outlives a strip (the terminal's `DroppedTexture` rule).
static DROPPED_STRIPS: Mutex<Vec<(ImageId, usize)>> = Mutex::new(Vec::new());

impl Drop for TallImage {
    fn drop(&mut self) {
        let Ok(mut dropped) = DROPPED_STRIPS.lock() else {
            return;
        };
        dropped.extend(
            self.strips
                .iter()
                .map(|strip| (strip.image.id, strip.image.frame_count())),
        );
    }
}

/// Release the sprite-atlas tiles of every strip whose picture dropped since
/// the last call. Runs once per frame from the shell's render — the one place
/// with the main window at hand: a per-issue `ImageCache` drops with its
/// issue view and has no window of its own. Without it every strip a closed
/// issue ever painted stayed in the atlas for the rest of the session.
pub(crate) fn release_dropped_strips(window: &mut Window) {
    let dropped: Vec<(ImageId, usize)> = match DROPPED_STRIPS.lock() {
        Ok(mut dropped) => std::mem::take(&mut *dropped),
        Err(_) => return,
    };
    for (id, frames) in dropped {
        // A pixel-free stand-in with the same id and frame count: the atlas
        // removes the very keys the strip's paints inserted.
        let frames: Vec<image::Frame> = (0..frames)
            .map(|_| image::Frame::new(image::RgbaImage::new(1, 1)))
            .collect();
        let mut tombstone = RenderImage::new(frames);
        tombstone.id = id;
        if let Err(error) = window.drop_image(Arc::new(tombstone)) {
            log::debug!("tall image strip drop failed: {error}");
        }
    }
}

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
/// Memory-lean on purpose: the decode is consumed into RGBA (no second
/// copy), narrowed to [`MAX_TALL_WIDTH`], and split from the END so each
/// strip moves out of the one buffer — the peak is the picture plus a strip,
/// never three full copies.
pub(crate) fn decode_tall(bytes: &[u8]) -> anyhow::Result<TallImage> {
    let decoded = image::load_from_memory(bytes)?;
    let (source_w, source_h) = (decoded.width(), decoded.height());
    // `into_rgba8` reuses an RGBA8 decode in place and frees any other
    // layout the moment it is converted.
    let mut rgba = decoded.into_rgba8();
    let (width, height) = narrowed_size(source_w, source_h);
    if (width, height) != (source_w, source_h) {
        rgba = image::imageops::resize(
            &rgba,
            width,
            height,
            image::imageops::FilterType::Triangle,
        );
    }
    log::info!(
        "[ui] tall image {source_w}×{source_h} → {width}×{height} in strips of {TALL_STRIP_ROWS} rows"
    );
    let mut raw = rgba.into_raw();
    // gpui's sprite pipeline expects BGRA (terminal `graphic_to_frame`).
    for pixel in raw.chunks_exact_mut(4) {
        pixel.swap(0, 2);
    }
    let stride = width as usize * 4;
    let ranges = tall_image_strip_ranges(height, TALL_STRIP_ROWS);
    let mut strips = Vec::with_capacity(ranges.len());
    for (y, rows) in ranges.iter().rev() {
        let start = *y as usize * stride;
        if raw.len() != start + *rows as usize * stride {
            anyhow::bail!("strip buffer size mismatch");
        }
        // `split_off` moves the strip's bytes out; `shrink_to_fit` hands the
        // freed tail back rather than keeping the full-size allocation.
        let pixels = raw.split_off(start);
        raw.shrink_to_fit();
        let buffer = image::RgbaImage::from_raw(width, *rows, pixels)
            .ok_or_else(|| anyhow::anyhow!("strip buffer size mismatch"))?;
        let frame = image::Frame::new(buffer);
        strips.push(TallStrip {
            image: Arc::new(RenderImage::new(vec![frame])),
            rows: *rows,
        });
    }
    strips.reverse();
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

    /// A wide capture is narrowed to the display cap before the split; a
    /// narrow one keeps its pixels.
    #[test]
    fn tall_image_narrows_only_past_the_display_width_cap() {
        assert_eq!(narrowed_size(780, 25094), (780, 25094));
        assert_eq!(narrowed_size(MAX_TALL_WIDTH, 20000), (MAX_TALL_WIDTH, 20000));
        assert_eq!(narrowed_size(4096, 20000), (MAX_TALL_WIDTH, 10000));
        assert_eq!(narrowed_size(MAX_TEXTURE_SIDE + 2, 1), (MAX_TALL_WIDTH, 1));
    }

    /// Dropping a strip-decoded picture queues its strips' atlas keys for
    /// the next frame's release, one record per strip.
    #[test]
    fn dropping_a_tall_image_queues_its_strips_for_release() {
        let height = MAX_TEXTURE_SIDE + 10;
        let buffer = image::RgbaImage::from_pixel(2, height, image::Rgba([1, 2, 3, 255]));
        let mut bytes = Vec::new();
        image::DynamicImage::ImageRgba8(buffer)
            .write_to(
                &mut std::io::Cursor::new(&mut bytes),
                image::ImageFormat::Png,
            )
            .unwrap();
        let tall = decode_tall(&bytes).unwrap();
        let ids: Vec<ImageId> = tall.strips.iter().map(|strip| strip.image.id).collect();
        assert_eq!(ids.len(), 5);
        drop(tall);
        let queued = DROPPED_STRIPS.lock().unwrap();
        for id in ids {
            assert!(queued.contains(&(id, 1)), "strip {id:?} not queued");
        }
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
