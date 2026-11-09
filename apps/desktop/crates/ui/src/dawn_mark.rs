//! #LOGS-5: the Home dawn's brand mark, rasterized OFF the main thread.
//!
//! gpui rasterizes an SVG inside `paint_svg`, on the main thread: the dawn's
//! 440 pt logo is a 1760² device-px tile on a retina display (440 × 2 × the
//! smooth-SVG factor 2), and its mask + clip cost resvg ~50 ms, the first
//! Home frame. gpui has no way to pre-fill its sprite atlas, so a worker
//! thread renders the tile exactly as gpui would and wraps the alpha in an
//! SVG that only embeds that raster at 1:1 (nearest sampling): gpui's
//! rasterization of the wrapper is byte-identical to the logo's own tile
//! (`wrapper_raster_is_the_logo_tile`) and costs a PNG decode, ~10 ms. Only
//! the rows the dawn shows are embedded; the rest of the tile stays clear.
//!
//! A window that painted the mark before its wrapper was ready keeps the
//! plain logo path: its atlas already holds that tile.
use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, Mutex},
    thread::JoinHandle,
};

use base64::Engine as _;
use gpui::{App, Bounds, Hsla, Pixels, SharedString, Window, WindowId, SMOOTH_SVG_SCALE_FACTOR};
use gpui_component::IconNamed as _;
use resvg::{tiny_skia, usvg};

use crate::icons::ExpIcon;

/// Every wrapper by its tile width (device px), and the windows that drew
/// the plain logo.
#[derive(Default)]
struct Marks {
    jobs: HashMap<i32, Job>,
    plain: HashSet<WindowId>,
}

enum Job {
    Pending(JoinHandle<Option<Arc<[u8]>>>),
    Done(Option<Arc<[u8]>>),
}

static MARKS: Mutex<Option<Marks>> = Mutex::new(None);

/// The rows of the mark the dawn shows, plus a margin bilinear sampling at
/// the clip's edge can never reach past.
fn shown_rows(tile_h: u32, shown: f32) -> u32 {
    ((tile_h as f32 * shown).ceil() as u32 + tile_h / 25).min(tile_h)
}

/// Start the wrapper for a retina window at app start: Home is the landing
/// screen, so its first frame wants it.
pub(crate) fn warm(size: f32, shown: f32, cx: &App) {
    let Some(logo) = logo(cx) else { return };
    let width = (size * 2. * SMOOTH_SVG_SCALE_FACTOR).ceil() as i32;
    let mut marks = MARKS.lock().unwrap();
    let marks = marks.get_or_insert_with(Marks::default);
    marks.jobs.entry(width).or_insert_with(|| spawn(logo, width, shown));
}

fn logo(cx: &App) -> Option<Arc<[u8]>> {
    let bytes = cx.asset_source().load(&ExpIcon::Logo.path()).ok()??;
    Some(Arc::from(&*bytes))
}

fn spawn(logo: Arc<[u8]>, width: i32, shown: f32) -> Job {
    Job::Pending(std::thread::spawn(move || wrapper(&logo, width, shown).map(Arc::from)))
}

/// Paint the mark into `bounds` (the box the logo `Icon` had) in `color`.
pub(crate) fn paint(bounds: Bounds<Pixels>, color: Hsla, shown: f32, window: &mut Window, cx: &mut App) {
    let width = tile_width(bounds, window.scale_factor());
    let id = window.window_handle().window_id();
    let wrapper = {
        let mut marks = MARKS.lock().unwrap();
        let marks = marks.get_or_insert_with(Marks::default);
        if marks.plain.contains(&id) {
            None
        } else {
            let ready = match marks.jobs.remove(&width) {
                // Started at app start: at most the rest of its run is waited for.
                Some(Job::Pending(job)) => job.join().ok().flatten(),
                Some(Job::Done(done)) => done,
                None => {
                    if let Some(logo) = logo(cx) {
                        marks.jobs.insert(width, spawn(logo, width, shown));
                    }
                    marks.plain.insert(id);
                    return paint_plain(bounds, color, window, cx);
                }
            };
            marks.jobs.insert(width, Job::Done(ready.clone()));
            if ready.is_none() {
                marks.plain.insert(id);
            }
            ready
        }
    };
    match wrapper {
        Some(svg) => {
            let path = SharedString::from(format!("exp-dawn-mark-{width}"));
            window.paint_svg(bounds, path, Some(&svg), Default::default(), color, cx).ok();
        }
        None => paint_plain(bounds, color, window, cx),
    }
}

fn paint_plain(bounds: Bounds<Pixels>, color: Hsla, window: &mut Window, cx: &mut App) {
    window.paint_svg(bounds, ExpIcon::Logo.path(), None, Default::default(), color, cx).ok();
}

/// The tile width gpui's `paint_svg` derives from `bounds` (its
/// `snap_bounds`: each edge rounded half toward zero to a device pixel).
fn tile_width(bounds: Bounds<Pixels>, scale: f32) -> i32 {
    let snap = |v: Pixels| {
        let v = f32::from(v) * scale;
        (v.abs() - 0.5).ceil().copysign(v)
    };
    let left = snap(bounds.left());
    let right = snap(bounds.right()).max(left);
    ((right - left) * SMOOTH_SVG_SCALE_FACTOR).ceil() as i32
}

/// gpui's `rasterize_tree` for a `SvgSize::Size` of `width`.
fn raster(svg: &[u8], width: i32) -> Option<tiny_skia::Pixmap> {
    const MAX_SIZE: f32 = 8192.;
    let tree = usvg::Tree::from_data(svg, &usvg::Options::default()).ok()?;
    let size = tree.size();
    let mut scale = width as f32 / size.width();
    if size.width() * scale > MAX_SIZE {
        scale *= MAX_SIZE / (size.width() * scale);
    }
    if size.height() * scale > MAX_SIZE {
        scale *= MAX_SIZE / (size.height() * scale);
    }
    let mut pixmap = tiny_skia::Pixmap::new((size.width() * scale) as u32, (size.height() * scale) as u32)?;
    resvg::render(&tree, tiny_skia::Transform::from_scale(scale, scale), &mut pixmap.as_mut());
    Some(pixmap)
}

/// The wrapper SVG: the logo's tile at `width`, its `shown` top rows
/// embedded 1:1 as an indexed PNG whose palette IS the alpha.
fn wrapper(logo: &[u8], width: i32, shown: f32) -> Option<Vec<u8>> {
    let pixmap = raster(logo, width)?;
    let (w, h) = (pixmap.width(), pixmap.height());
    // gpui scales the wrapper by `width / w`: anything but 1:1 resamples.
    if w as i32 != width {
        return None;
    }
    let rows = shown_rows(h, shown);
    let alpha: Vec<u8> = pixmap.pixels()[..(w * rows) as usize].iter().map(|p| p.alpha()).collect();
    let mut png = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut png, w, rows);
        encoder.set_color(png::ColorType::Indexed);
        encoder.set_depth(png::BitDepth::Eight);
        encoder.set_palette(vec![0u8; 256 * 3]);
        encoder.set_trns((0..=255u8).collect::<Vec<_>>());
        let mut writer = encoder.write_header().ok()?;
        writer.write_image_data(&alpha).ok()?;
    }
    let png = base64::engine::general_purpose::STANDARD.encode(&png);
    Some(
        format!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" width="{w}" height="{h}" viewBox="0 0 {w} {h}"><image width="{w}" height="{rows}" image-rendering="optimizeSpeed" xlink:href="data:image/png;base64,{png}"/></svg>"#
        )
        .into_bytes(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::px;

    const LOGO: &[u8] = include_bytes!("../../../assets/icons/logo.svg");

    /// The wrapper's raster equals the logo's tile on every row the dawn
    /// shows, at retina, 1× and an odd width.
    #[test]
    fn wrapper_raster_is_the_logo_tile() {
        for width in [1760, 880, 1001] {
            let logo = raster(LOGO, width).unwrap();
            let svg = wrapper(LOGO, width, 0.5).unwrap();
            let mark = raster(&svg, width).unwrap();
            assert_eq!((mark.width(), mark.height()), (logo.width(), logo.height()));
            let rows = shown_rows(logo.height(), 0.5) as usize * logo.width() as usize;
            let alpha = |p: &tiny_skia::Pixmap| p.pixels().iter().map(|p| p.alpha()).collect::<Vec<_>>();
            let (logo, mark) = (alpha(&logo), alpha(&mark));
            assert!(logo[..rows] == mark[..rows], "width {width}");
            assert!(mark[rows..].iter().all(|&a| a == 0));
        }
    }

    #[test]
    fn tile_width_matches_snap_bounds() {
        let at = |x: f32| Bounds::new(gpui::point(px(x), px(0.)), gpui::size(px(440.), px(440.)));
        assert_eq!(tile_width(at(500.), 2.), 1760);
        assert_eq!(tile_width(at(500.25), 2.), 1760);
        assert_eq!(tile_width(at(500.5), 1.), 880);
    }
}
