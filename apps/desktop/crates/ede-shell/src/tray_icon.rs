//! The tray's icons, pixel-clean like Cinnamon's systray (#EDE-237): ONE
//! icon size per panel height and output scale, every icon rasterized here
//! at exactly that many device pixels, so the taskbar draws it 1:1 and never
//! scales a bitmap (a 64 px pixmap squeezed into 18 px, a 16 px SVG blown up
//! by the renderer, a symbolic icon in the theme's dark ink: 0.2.38).
//!
//! Cinnamon's rule (`panel.js` `_clampPanelZoneColorIconSize` +
//! `toStandardIconSize`, the `org.cinnamon` `panel-zone-*icon-sizes`
//! defaults for the right zone: full colour 24, symbolic 16): a full-colour
//! icon is 24 px unless the panel is lower than that (then the largest
//! standard size that fits it), a symbolic one 16 px clamped to 10 ..
//! min(50, panel). The slot is the full-colour size whatever the icon: a
//! symbolic icon sits centred in it, so every item is the same width.
//! The size is logical; `scale` (the panel's output scale, fractional
//! included) turns it into device pixels.

use std::path::{Path, PathBuf};

use base64::Engine as _;

/// The taskbar's height (`taskbar.express`: `size: {height: 48}`).
pub const PANEL_HEIGHT: f32 = 48.0;

/// `org.cinnamon` `panel-zone-icon-sizes`' right zone.
const FULLCOLOR_DEFAULT: u32 = 24;
/// `org.cinnamon` `panel-zone-symbolic-icon-sizes`' right zone.
const SYMBOLIC_DEFAULT: u32 = 16;

/// The ink a symbolic icon is drawn in: the dark theme's `foreground`
/// (the taskbar is dark), as GTK recolours symbolic icons with the text colour.
pub const SYMBOLIC_INK: [u8; 3] = [0xfa, 0xfa, 0xfa];

/// Cinnamon's `toStandardIconSize`.
fn standard_size(max: f32) -> u32 {
    match max.floor() as u32 {
        0..=21 => 16,
        22..=23 => 22,
        24..=31 => 24,
        32..=47 => 32,
        _ => 48,
    }
}

/// The logical icon size on a panel `panel_height` high (Cinnamon's rule).
pub fn icon_size(panel_height: f32, symbolic: bool) -> u32 {
    if symbolic {
        SYMBOLIC_DEFAULT.clamp(10, (panel_height.floor() as u32).clamp(10, 50))
    } else if FULLCOLOR_DEFAULT as f32 > panel_height {
        standard_size(panel_height)
    } else {
        FULLCOLOR_DEFAULT
    }
}

/// Logical `size` at `scale` in device pixels.
pub fn device_px(size: u32, scale: f64) -> u32 {
    ((size as f64) * scale.max(0.5)).round().max(1.0) as u32
}

/// What an item says about its icon (`IconName`, `IconThemePath`,
/// `IconPixmap`: ARGB32 in network byte order).
#[derive(Debug, Default, Clone)]
pub struct Source {
    pub name: String,
    pub theme_path: String,
    pub pixmaps: Vec<(i32, i32, Vec<u8>)>,
}

/// GTK's rule: an icon named `…-symbolic` is a symbolic one.
pub fn is_symbolic(name: &str) -> bool {
    Path::new(name).file_stem().and_then(|s| s.to_str()).is_some_and(|s| s.ends_with("-symbolic"))
}

/// The item's icon as a PNG data URL of exactly `device_px(slot, scale)`
/// square pixels (`slot` = the full-colour size), or `None` (the taskbar
/// draws its fallback glyph). A named icon goes first, then the pixmaps,
/// as Cinnamon's xapp-sn-watcher does.
pub fn render(source: &Source, panel_height: f32, scale: f64) -> Option<String> {
    let slot = icon_size(panel_height, false);
    let canvas = device_px(slot, scale);
    let named = (!source.name.is_empty()).then(|| {
        let symbolic = is_symbolic(&source.name);
        let glyph = device_px(icon_size(panel_height, symbolic), scale).min(canvas);
        let file = find_icon(&source.name, &source.theme_path, glyph)?;
        let mut img = load_file(&file, glyph)?;
        if symbolic {
            recolor(&mut img, SYMBOLIC_INK);
        }
        Some(centred(&img, canvas))
    });
    let img = named.flatten().or_else(|| {
        let (w, h, argb) = best_pixmap(&source.pixmaps, canvas)?;
        let rgba = argb.chunks_exact(4).flat_map(|p| [p[1], p[2], p[3], p[0]]).collect();
        let img = image::RgbaImage::from_raw(w as u32, h as u32, rgba)?;
        Some(centred(&fit(&img, canvas), canvas))
    })?;
    png_data_url(&img)
}

/// The pixmap that scales least for a `px` square: the exact size, else the
/// smallest larger one (scaled down), else the largest (scaled up).
pub fn best_pixmap(pixmaps: &[(i32, i32, Vec<u8>)], px: u32) -> Option<&(i32, i32, Vec<u8>)> {
    let valid = || pixmaps.iter().filter(|(w, h, b)| *w > 0 && *h > 0 && *w <= 4096 && *h <= 4096 && b.len() == (*w * *h * 4) as usize);
    let side = |p: &(i32, i32, Vec<u8>)| p.0.max(p.1) as u32;
    valid().find(|p| side(p) == px).or_else(|| valid().filter(|p| side(p) > px).min_by_key(|p| side(p))).or_else(|| valid().max_by_key(|p| side(p)))
}

/// `img` scaled so its longer side is `px` (aspect kept); as is when it is.
fn fit(img: &image::RgbaImage, px: u32) -> image::RgbaImage {
    let (w, h) = img.dimensions();
    if w.max(h) == px {
        return img.clone();
    }
    let (nw, nh) = if w >= h { (px, ((h as f64) * px as f64 / w as f64).round().max(1.0) as u32) } else { (((w as f64) * px as f64 / h as f64).round().max(1.0) as u32, px) };
    // Lanczos down (sharp), Catmull-Rom up (no ringing on a small pixmap).
    let filter = if px < w.max(h) { image::imageops::FilterType::Lanczos3 } else { image::imageops::FilterType::CatmullRom };
    image::imageops::resize(img, nw, nh, filter)
}

/// `img` centred on a transparent `px` square, on whole pixels.
fn centred(img: &image::RgbaImage, px: u32) -> image::RgbaImage {
    let mut out = image::RgbaImage::new(px, px);
    let (w, h) = img.dimensions();
    let x = (px.saturating_sub(w) / 2) as i64;
    let y = (px.saturating_sub(h) / 2) as i64;
    image::imageops::overlay(&mut out, img, x, y);
    out
}

/// A symbolic icon in `ink`: every pixel takes the colour, keeping its alpha.
fn recolor(img: &mut image::RgbaImage, ink: [u8; 3]) {
    for p in img.pixels_mut() {
        p.0 = [ink[0], ink[1], ink[2], p.0[3]];
    }
}

/// A file as a `px` image: an SVG rasterized at `px`, a raster scaled to it.
fn load_file(file: &Path, px: u32) -> Option<image::RgbaImage> {
    let bytes = std::fs::read(file).ok()?;
    if file.extension().is_some_and(|x| x.eq_ignore_ascii_case("svg")) {
        return rasterize_svg(&bytes, px);
    }
    let img = image::load_from_memory(&bytes).ok()?.to_rgba8();
    Some(fit(&img, px))
}

/// An SVG drawn at `px` (its longer side), straight RGBA.
pub fn rasterize_svg(bytes: &[u8], px: u32) -> Option<image::RgbaImage> {
    let tree = resvg::usvg::Tree::from_data(bytes, &resvg::usvg::Options::default()).ok()?;
    let size = tree.size();
    let k = px as f32 / size.width().max(size.height());
    let (w, h) = (((size.width() * k).round() as u32).max(1), ((size.height() * k).round() as u32).max(1));
    let mut pixmap = resvg::tiny_skia::Pixmap::new(w, h)?;
    resvg::render(&tree, resvg::tiny_skia::Transform::from_scale(k, k), &mut pixmap.as_mut());
    let rgba = pixmap
        .pixels()
        .iter()
        .flat_map(|p| {
            let c = p.demultiply();
            [c.red(), c.green(), c.blue(), c.alpha()]
        })
        .collect();
    image::RgbaImage::from_raw(w, h, rgba)
}

fn png_data_url(img: &image::RgbaImage) -> Option<String> {
    let mut png = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut png, img.width(), img.height());
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder.write_header().ok()?.write_image_data(img.as_raw()).ok()?;
    }
    Some(format!("data:image/png;base64,{}", base64::engine::general_purpose::STANDARD.encode(png)))
}

/// The file for icon `name` best drawn at `px`: an absolute path as is;
/// else the item's `IconThemePath`, then the icon themes. In a theme, an
/// exact-size PNG beats an SVG, which beats the nearest larger PNG, then
/// the largest smaller one.
pub fn find_icon(name: &str, theme_path: &str, px: u32) -> Option<PathBuf> {
    if name.starts_with('/') {
        return Path::new(name).is_file().then(|| PathBuf::from(name));
    }
    let name = name.strip_suffix(".png").or_else(|| name.strip_suffix(".svg")).unwrap_or(name);
    let mut roots: Vec<PathBuf> = Vec::new();
    if !theme_path.is_empty() {
        roots.push(PathBuf::from(theme_path));
    }
    for theme in uicore_data::apps::icon_themes() {
        for root in uicore_data::apps::icon_theme_roots() {
            roots.push(root.join(&theme));
        }
    }
    for root in roots.iter().filter(|r| r.is_dir()) {
        if let Some(found) = best_in(root, name, px) {
            return Some(found);
        }
    }
    let pixmaps = Path::new("/usr/share/pixmaps");
    ["svg", "png"].iter().map(|x| pixmaps.join(format!("{name}.{x}"))).find(|p| p.is_file())
}

/// The nominal size of an icon theme directory (`24x24`, `24`, `24x24@2`
/// = 48, `scalable` = none), from the path below the theme root.
fn dir_size(rel: &Path) -> Option<u32> {
    rel.components().filter_map(|c| c.as_os_str().to_str()).find_map(|c| {
        let (base, mult) = match c.split_once('@') {
            Some((b, m)) => (b, m.trim_end_matches('x').parse::<u32>().ok()?),
            None => (c, 1),
        };
        let n = base.split('x').next()?.parse::<u32>().ok()?;
        Some(n * mult)
    })
}

/// The best file for `name` at `px` under `root`: the root itself (a flat
/// `IconThemePath`) and the directories two levels down (`<size>/<context>`
/// and `<context>/<size>` themes), probed by name, never listed.
fn best_in(root: &Path, name: &str, px: u32) -> Option<PathBuf> {
    let subdirs = |d: &Path| -> Vec<PathBuf> { std::fs::read_dir(d).map(|it| it.flatten().map(|e| e.path()).filter(|p| p.is_dir()).collect()).unwrap_or_default() };
    let mut dirs = vec![root.to_path_buf()];
    for d in subdirs(root) {
        dirs.extend(subdirs(&d));
        dirs.push(d);
    }
    // (rank, path): lower rank wins.
    let mut best: Option<(u64, PathBuf)> = None;
    for dir in dirs {
        for ext in ["png", "svg"] {
            let p = dir.join(format!("{name}.{ext}"));
            if !p.is_file() {
                continue;
            }
            let size = p.strip_prefix(root).ok().and_then(dir_size);
            let rank = match (ext, size) {
                ("png", Some(s)) if s == px => 0,
                ("svg", Some(s)) if s == px => 1,
                ("svg", _) => 2,
                ("png", Some(s)) if s > px => 10 + (s - px) as u64,
                ("png", Some(s)) => 10_000 + (px - s) as u64,
                // A bare PNG (a flat IconThemePath): its pixels tell.
                _ => image::image_dimensions(&p).map(|(w, h)| w.max(h)).map_or(50_000, |s| if s >= px { 10 + (s - px) as u64 } else { 10_000 + (px - s) as u64 }),
            };
            if best.as_ref().is_none_or(|(r, _)| rank < *r) {
                best = Some((rank, p));
            }
        }
    }
    best.map(|(_, p)| p)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cinnamon_sizes_per_panel_height() {
        // The taskbar: 24 full colour, 16 symbolic (Cinnamon's right zone).
        assert_eq!(icon_size(PANEL_HEIGHT, false), 24);
        assert_eq!(icon_size(PANEL_HEIGHT, true), 16);
        // Cinnamon's 40 px default panel: the same.
        assert_eq!(icon_size(40.0, false), 24);
        // A panel lower than 24: the largest standard size that fits.
        assert_eq!(icon_size(23.0, false), 22);
        assert_eq!(icon_size(20.0, false), 16);
        assert_eq!(icon_size(12.0, true), 12);
        assert_eq!(icon_size(8.0, true), 10);
    }

    #[test]
    fn device_pixels_follow_the_scale() {
        assert_eq!(device_px(24, 1.0), 24);
        assert_eq!(device_px(24, 1.25), 30);
        assert_eq!(device_px(24, 1.5), 36);
        assert_eq!(device_px(24, 2.0), 48);
        assert_eq!(device_px(16, 1.25), 20);
    }

    #[test]
    fn the_taskbar_draws_the_icons_at_the_rule_s_size() {
        // taskbar.express's tray Images are literal sizes: they must be the rule's.
        let doc = include_str!("../../../../ede/ui/taskbar.express");
        assert!(doc.contains("size: {height: 48}"), "PANEL_HEIGHT is the taskbar's height");
        let size = icon_size(PANEL_HEIGHT, false);
        for line in doc.lines().filter(|l| l.starts_with("tray_") && l.contains("Image(")) {
            assert!(line.contains(&format!("width={size}, height={size}")), "{line}");
        }
    }

    fn pm(side: i32) -> (i32, i32, Vec<u8>) {
        (side, side, vec![0xff; (side * side * 4) as usize])
    }

    #[test]
    fn the_pixmap_that_scales_least() {
        let set = vec![pm(16), pm(22), pm(48), pm(64)];
        assert_eq!(best_pixmap(&set, 22).unwrap().0, 22);
        assert_eq!(best_pixmap(&set, 30).unwrap().0, 48);
        assert_eq!(best_pixmap(&set, 96).unwrap().0, 64);
        // A broken one is never picked.
        assert_eq!(best_pixmap(&[(24, 24, vec![0; 3]), pm(16)], 24).unwrap().0, 16);
        assert!(best_pixmap(&[], 24).is_none());
    }

    fn decode(url: &str) -> image::RgbaImage {
        let b64 = url.strip_prefix("data:image/png;base64,").unwrap();
        image::load_from_memory(&base64::engine::general_purpose::STANDARD.decode(b64).unwrap()).unwrap().to_rgba8()
    }

    #[test]
    fn a_pixmap_is_drawn_at_the_device_size() {
        // Discord's one 24 px pixmap at 1.25: 30 device pixels, no more.
        let source = Source { pixmaps: vec![pm(24)], ..Default::default() };
        let img = decode(&render(&source, PANEL_HEIGHT, 1.25).unwrap());
        assert_eq!(img.dimensions(), (30, 30));
        let img = decode(&render(&source, PANEL_HEIGHT, 1.0).unwrap());
        assert_eq!(img.dimensions(), (24, 24));
        assert_eq!(img.get_pixel(0, 0).0, [0xff; 4], "the exact size, untouched");
    }

    #[test]
    fn a_wide_pixmap_is_centred() {
        let source = Source { pixmaps: vec![(48, 24, vec![0xff; 48 * 24 * 4])], ..Default::default() };
        let img = decode(&render(&source, PANEL_HEIGHT, 1.0).unwrap());
        assert_eq!(img.dimensions(), (24, 24));
        assert_eq!(img.get_pixel(12, 2).0[3], 0, "above the band: clear");
        assert_eq!(img.get_pixel(12, 12).0[3], 0xff);
        assert_eq!(img.get_pixel(12, 21).0[3], 0, "below the band: clear");
    }

    const SQUARE: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16"><rect width="16" height="16" fill="#2e3436"/></svg>"##;

    #[test]
    fn a_symbolic_icon_is_inked_and_centred_in_the_slot() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("net-symbolic.svg");
        std::fs::write(&file, SQUARE).unwrap();
        let source = Source { name: file.display().to_string(), ..Default::default() };
        // 2x: a 48 px slot, the 16 px glyph = 32 px centred (8 px around).
        let img = decode(&render(&source, PANEL_HEIGHT, 2.0).unwrap());
        assert_eq!(img.dimensions(), (48, 48));
        assert_eq!(img.get_pixel(7, 24).0[3], 0);
        assert_eq!(img.get_pixel(8, 24).0, [0xfa, 0xfa, 0xfa, 0xff], "the theme's ink, not the SVG's dark");
        assert_eq!(img.get_pixel(39, 24).0[3], 0xff);
        assert_eq!(img.get_pixel(40, 24).0[3], 0);
    }

    #[test]
    fn a_full_colour_svg_fills_the_slot_in_its_own_colours() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("app.svg");
        std::fs::write(&file, SQUARE).unwrap();
        let source = Source { name: file.display().to_string(), ..Default::default() };
        let img = decode(&render(&source, PANEL_HEIGHT, 1.25).unwrap());
        assert_eq!(img.dimensions(), (30, 30));
        assert_eq!(img.get_pixel(0, 0).0, [0x2e, 0x34, 0x36, 0xff]);
        assert_eq!(img.get_pixel(29, 29).0, [0x2e, 0x34, 0x36, 0xff]);
    }

    #[test]
    fn a_theme_path_s_best_file_for_the_size() {
        // Steam's IconThemePath: one flat 48 px PNG; Toolbox-like sized dirs.
        let dir = tempfile::tempdir().unwrap();
        let png = |p: &Path, side: u32| {
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            image::RgbaImage::new(side, side).save(p).unwrap();
        };
        png(&dir.path().join("steam_tray_mono.png"), 48);
        assert_eq!(find_icon("steam_tray_mono", &dir.path().display().to_string(), 30).unwrap(), dir.path().join("steam_tray_mono.png"));
        png(&dir.path().join("hicolor/22x22/apps/x.png"), 22);
        png(&dir.path().join("hicolor/32x32/apps/x.png"), 32);
        png(&dir.path().join("hicolor/64x64/apps/x.png"), 64);
        let root = dir.path().display().to_string();
        assert!(find_icon("x", &root, 30).unwrap().ends_with("32x32/apps/x.png"), "the nearest larger");
        assert!(find_icon("x", &root, 22).unwrap().ends_with("22x22/apps/x.png"), "the exact one");
        std::fs::create_dir_all(dir.path().join("hicolor/scalable/apps")).unwrap();
        std::fs::write(dir.path().join("hicolor/scalable/apps/x.svg"), SQUARE).unwrap();
        assert!(find_icon("x", &root, 30).unwrap().ends_with("scalable/apps/x.svg"), "an SVG beats scaling a PNG");
        assert!(find_icon("x", &root, 22).unwrap().ends_with("22x22/apps/x.png"), "an exact PNG beats the SVG");
        assert_eq!(dir_size(Path::new("24x24@2/panel/x.svg")), Some(48));
        assert_eq!(dir_size(Path::new("status/24/x.svg")), Some(24));
    }

    #[test]
    fn symbolic_by_name() {
        assert!(is_symbolic("blueman-tray-symbolic"));
        assert!(is_symbolic("/usr/share/icons/x/net-symbolic.svg"));
        assert!(!is_symbolic("steam_tray_mono"));
    }
}
