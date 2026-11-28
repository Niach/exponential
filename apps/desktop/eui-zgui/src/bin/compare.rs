//! Pixel comparison of two renders (the gpui reference vs eui-zgui).
//!
//! ```text
//! compare <a.png> <b.png> [diff.png]                # one pair → JSON
//! compare dirs <gpui-dir> <zgui-dir> <out-dir>      # every <id>.png in both → table.md + results.json + diffs + side-by-sides
//! ```
//! A pixel DIFFERS when any channel's delta is > 8 (of 255). Sizes that
//! differ compare over the union: a pixel only one image has differs.
//! Pass threshold (fixed before the first run): <= 1.0 % differing pixels.

use image::{Rgba, RgbaImage};
use serde_json::json;

const DELTA: u8 = 8;
const PASS_PERCENT: f64 = 1.0;

struct Result {
    /// Informational: pixels with no match within 1 px in the other image (anti-aliasing / sub-pixel tolerant).
    shifted: u64,
    differing: u64,
    total: u64,
    max_delta: u8,
    diff: RgbaImage,
}

fn compare(a: &RgbaImage, b: &RgbaImage) -> Result {
    let (w, h) = (a.width().max(b.width()), a.height().max(b.height()));
    let mut diff = RgbaImage::new(w, h);
    let (mut differing, mut max_delta, mut shifted) = (0u64, 0u8, 0u64);
    let near = |x: u32, y: u32, p: &Rgba<u8>, img: &RgbaImage| -> bool {
        for dy in -1i32..=1 {
            for dx in -1i32..=1 {
                let (nx, ny) = (x as i32 + dx, y as i32 + dy);
                if nx >= 0 && ny >= 0 && (nx as u32) < img.width() && (ny as u32) < img.height() {
                    let q = img.get_pixel(nx as u32, ny as u32);
                    if (0..3).all(|c| p[c].abs_diff(q[c]) <= DELTA) {
                        return true;
                    }
                }
            }
        }
        false
    };
    for y in 0..h {
        for x in 0..w {
            let pa = (x < a.width() && y < a.height()).then(|| a.get_pixel(x, y));
            let pb = (x < b.width() && y < b.height()).then(|| b.get_pixel(x, y));
            let d = match (pa, pb) {
                (Some(pa), Some(pb)) => (0..3).map(|c| pa[c].abs_diff(pb[c])).max().unwrap(),
                _ => 255,
            };
            max_delta = max_delta.max(d);
            let grey = pa.map(|p| ((p[0] as u16 + p[1] as u16 + p[2] as u16) / 3 / 4) as u8).unwrap_or(0);
            if d > DELTA {
                differing += 1;
                if !(pa.is_some_and(|p| near(x, y, p, b)) && pb.is_some_and(|p| near(x, y, p, a))) {
                    shifted += 1;
                }
                diff.put_pixel(x, y, Rgba([255, 40u8.saturating_add(grey), 40, 255]));
            } else {
                diff.put_pixel(x, y, Rgba([grey, grey, grey, 255]));
            }
        }
    }
    Result { shifted, differing, total: w as u64 * h as u64, max_delta, diff }
}

fn load(p: &str) -> RgbaImage {
    image::open(p).unwrap_or_else(|e| panic!("{p}: {e}")).to_rgba8()
}

fn side_by_side(a: &RgbaImage, b: &RgbaImage, d: &RgbaImage) -> RgbaImage {
    let gap = 16;
    let h = a.height().max(b.height()).max(d.height());
    let mut out = RgbaImage::from_pixel(a.width() + b.width() + d.width() + 2 * gap, h, Rgba([40, 40, 48, 255]));
    image::imageops::overlay(&mut out, a, 0, 0);
    image::imageops::overlay(&mut out, b, (a.width() + gap) as i64, 0);
    image::imageops::overlay(&mut out, d, (a.width() + b.width() + 2 * gap) as i64, 0);
    out
}

fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    if a.first().map(String::as_str) == Some("dirs") {
        let (gd, zd, od) = (&a[1], &a[2], &a[3]);
        std::fs::create_dir_all(od).unwrap();
        let mut ids: Vec<String> = std::fs::read_dir(gd).unwrap().flatten().filter_map(|e| e.file_name().to_str().and_then(|n| n.strip_suffix(".png")).map(str::to_string)).collect();
        ids.sort();
        let mut table = format!("| component | size gpui | size zgui | differing px | % | max delta | pass (<= {PASS_PERCENT} %) | % beyond a 1 px shift (info) |\n|---|---|---|---|---|---|---|---|\n");
        let mut results = Vec::new();
        for id in ids {
            let zp = format!("{zd}/{id}.png");
            if !std::path::Path::new(&zp).exists() {
                table.push_str(&format!("| {id} | | missing | | | | no |\n"));
                continue;
            }
            let (g, z) = (load(&format!("{gd}/{id}.png")), load(&zp));
            let r = compare(&g, &z);
            let pct = r.differing as f64 * 100.0 / r.total as f64;
            r.diff.save(format!("{od}/{id}.diff.png")).unwrap();
            side_by_side(&g, &z, &r.diff).save(format!("{od}/{id}.side.png")).unwrap();
            let pass = pct <= PASS_PERCENT;
            let spct = r.shifted as f64 * 100.0 / r.total as f64;
            table.push_str(&format!("| {} | {}x{} | {}x{} | {} | {:.2} | {} | {} | {:.2} |\n", id.trim_start_matches("exponential-ui-"), g.width(), g.height(), z.width(), z.height(), r.differing, pct, r.max_delta, if pass { "yes" } else { "no" }, spct));
            results.push(json!({"shifted_percent": spct, "id": id, "gpui": [g.width(), g.height()], "zgui": [z.width(), z.height()], "differing": r.differing, "percent": pct, "max_delta": r.max_delta, "pass": pass}));
        }
        std::fs::write(format!("{od}/table.md"), &table).unwrap();
        std::fs::write(format!("{od}/results.json"), serde_json::to_string_pretty(&results).unwrap()).unwrap();
        print!("{table}");
        return;
    }
    if a.len() < 2 {
        eprintln!("usage: compare <a.png> <b.png> [diff.png] | compare dirs <gpui-dir> <zgui-dir> <out-dir>");
        std::process::exit(2);
    }
    let r = compare(&load(&a[0]), &load(&a[1]));
    if let Some(p) = a.get(2) {
        r.diff.save(p).unwrap();
    }
    let pct = r.differing as f64 * 100.0 / r.total as f64;
    println!("{}", json!({"shifted_percent": r.shifted as f64 * 100.0 / r.total as f64, "differing": r.differing, "total": r.total, "percent": pct, "max_delta": r.max_delta, "pass": pct <= PASS_PERCENT}));
}
