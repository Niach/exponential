//! EXP-1602: the pixel conformance of eui-motion against lottie-web.
//!
//! For every corpus file and reference frame
//! (`packages/exponential-ui/conformance/motion/{corpus,refs}`, rendered by
//! `render-refs.ts` with lottie-web's SVG renderer in headless Chromium) the
//! test samples the same frame, paints it with the tiny-skia reference
//! raster at the reference's size and compares the two with SSIM:
//!
//! - both images are straight RGBA; each is composited over WHITE and over
//!   BLACK, reduced to luma (Rec. 601: 0.299 R + 0.587 G + 0.114 B, 0..255),
//!   and compared with the classic SSIM of Wang et al. 2004 (11×11 Gaussian
//!   window, σ = 1.5, C1 = (0.01·255)², C2 = (0.03·255)², mean over the
//!   windows that fit entirely inside the image); the case's score is the
//!   MIN of the two backgrounds, so a wrong alpha cannot hide on either;
//! - a case passes at SSIM ≥ [`THRESHOLD`].
//!
//! Every case passes OR is listed in `known-gaps.json` with a reason. That
//! list only shrinks: a listed case that now passes fails the test (remove
//! it). `report.json` is rewritten on every run (per case: file, frame,
//! ssim, pass).
//!
//! - `UPDATE_MOTION_REPORT=1`: rewrite `known-gaps.json` from the current
//!   results (failing cases keep their recorded reason; new ones get the
//!   validator's notes as a provisional reason);
//! - `MOTION_CONTACT_DIR=/tmp/x`: also write `ref | ours | diff` contact
//!   sheets (over white; diff = |ref − ours| × 4) per case;
//! - `MOTION_ONLY=<substring>`: run only matching files.
//!
//! Run: `cargo test -p eui-motion --features raster --test corpus` (add
//! `--release` for speed; the debug build takes a while).
#![cfg(feature = "raster")]

use eui_motion::{raster, DisplayList, Motion};
use serde_json::{json, Map, Value};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

const THRESHOLD: f64 = 0.98;

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../../packages/exponential-ui/conformance/motion")
}

/// Straight RGBA8, row major.
struct Rgba {
    w: usize,
    h: usize,
    px: Vec<[u8; 4]>,
}

fn load_png(path: &Path) -> Rgba {
    let img = image::open(path).unwrap_or_else(|e| panic!("{}: {e}", path.display())).to_rgba8();
    let (w, h) = (img.width() as usize, img.height() as usize);
    Rgba { w, h, px: img.pixels().map(|p| p.0).collect() }
}

fn from_pixmap(p: &tiny_skia::Pixmap) -> Rgba {
    Rgba {
        w: p.width() as usize,
        h: p.height() as usize,
        px: p
            .pixels()
            .iter()
            .map(|c| {
                let c = c.demultiply();
                [c.red(), c.green(), c.blue(), c.alpha()]
            })
            .collect(),
    }
}

/// `img` over a solid grey level `bg` (0 or 255), as RGB floats.
fn over(img: &Rgba, bg: f64) -> Vec<[f64; 3]> {
    img.px
        .iter()
        .map(|p| {
            let a = p[3] as f64 / 255.0;
            [0, 1, 2].map(|i| p[i] as f64 * a + bg * (1.0 - a))
        })
        .collect()
}

fn luma(rgb: &[[f64; 3]]) -> Vec<f64> {
    rgb.iter().map(|c| 0.299 * c[0] + 0.587 * c[1] + 0.114 * c[2]).collect()
}

/// Valid-region separable Gaussian blur (11 taps, σ 1.5).
fn blur(src: &[f64], w: usize, h: usize, k: &[f64]) -> (Vec<f64>, usize, usize) {
    let n = k.len();
    let (ow, oh) = (w + 1 - n, h + 1 - n);
    let mut tmp = vec![0.0; ow * h];
    for y in 0..h {
        for x in 0..ow {
            let row = &src[y * w + x..y * w + x + n];
            tmp[y * ow + x] = row.iter().zip(k).map(|(a, b)| a * b).sum();
        }
    }
    let mut out = vec![0.0; ow * oh];
    for y in 0..oh {
        for x in 0..ow {
            let mut s = 0.0;
            for (i, kv) in k.iter().enumerate() {
                s += tmp[(y + i) * ow + x] * kv;
            }
            out[y * ow + x] = s;
        }
    }
    (out, ow, oh)
}

fn ssim_gray(a: &[f64], b: &[f64], w: usize, h: usize) -> f64 {
    const N: usize = 11;
    if w < N || h < N {
        // Too small for the window: 1 − normalised mean absolute error.
        let mae: f64 = a.iter().zip(b).map(|(x, y)| (x - y).abs()).sum::<f64>() / a.len().max(1) as f64;
        return 1.0 - mae / 255.0;
    }
    let sigma = 1.5f64;
    let mut k: Vec<f64> = (0..N).map(|i| (-((i as f64 - 5.0).powi(2)) / (2.0 * sigma * sigma)).exp()).collect();
    let sum: f64 = k.iter().sum();
    k.iter_mut().for_each(|v| *v /= sum);
    let aa: Vec<f64> = a.iter().map(|v| v * v).collect();
    let bb: Vec<f64> = b.iter().map(|v| v * v).collect();
    let ab: Vec<f64> = a.iter().zip(b).map(|(x, y)| x * y).collect();
    let (ma, ow, oh) = blur(a, w, h, &k);
    let (mb, _, _) = blur(b, w, h, &k);
    let (saa, _, _) = blur(&aa, w, h, &k);
    let (sbb, _, _) = blur(&bb, w, h, &k);
    let (sab, _, _) = blur(&ab, w, h, &k);
    let c1 = (0.01f64 * 255.0).powi(2);
    let c2 = (0.03f64 * 255.0).powi(2);
    let mut total = 0.0;
    for i in 0..ow * oh {
        let (mx, my) = (ma[i], mb[i]);
        let vx = saa[i] - mx * mx;
        let vy = sbb[i] - my * my;
        let cxy = sab[i] - mx * my;
        total += ((2.0 * mx * my + c1) * (2.0 * cxy + c2)) / ((mx * mx + my * my + c1) * (vx + vy + c2));
    }
    total / (ow * oh) as f64
}

/// The case score: min SSIM of the two backgrounds.
fn ssim(reference: &Rgba, ours: &Rgba) -> f64 {
    assert_eq!((reference.w, reference.h), (ours.w, ours.h));
    [0.0, 255.0]
        .iter()
        .map(|&bg| ssim_gray(&luma(&over(reference, bg)), &luma(&over(ours, bg)), reference.w, reference.h))
        .fold(f64::INFINITY, f64::min)
}

fn contact_sheet(reference: &Rgba, ours: &Rgba, path: &Path) {
    let (w, h) = (reference.w, reference.h);
    let (r, o) = (over(reference, 255.0), over(ours, 255.0));
    let gap = 4;
    let mut img = image::RgbaImage::from_pixel((w * 3 + gap * 2) as u32, h as u32, image::Rgba([128, 128, 128, 255]));
    for y in 0..h {
        for x in 0..w {
            let i = y * w + x;
            let px = |c: [f64; 3]| image::Rgba([c[0].round() as u8, c[1].round() as u8, c[2].round() as u8, 255]);
            img.put_pixel(x as u32, y as u32, px(r[i]));
            img.put_pixel((x + w + gap) as u32, y as u32, px(o[i]));
            let d = [0, 1, 2].map(|c| 255.0 - ((r[i][c] - o[i][c]).abs() * 4.0).min(255.0));
            img.put_pixel((x + 2 * (w + gap)) as u32, y as u32, px(d));
        }
    }
    img.save(path).unwrap();
}

struct Outcome {
    file: String,
    frame: i64,
    ssim: f64,
    note: Option<String>,
}

fn run_file(name: &str, sizes: &Value, contact: Option<&Path>) -> Vec<Outcome> {
    let dir = root();
    let w = sizes["width"].as_u64().unwrap() as u32;
    let h = sizes["height"].as_u64().unwrap() as u32;
    let frames: Vec<i64> = sizes["frames"].as_array().unwrap().iter().map(|f| f.as_i64().unwrap()).collect();
    let bytes = std::fs::read(dir.join("corpus").join(format!("{name}.json"))).unwrap();
    let fail = |note: String| frames.iter().map(|&frame| Outcome { file: name.into(), frame, ssim: 0.0, note: Some(note.clone()) }).collect();
    let motion = match Motion::load(&bytes) {
        Ok(m) => m,
        Err(issues) => return fail(format!("refused: {}", issues.iter().map(|i| i.code.as_str()).collect::<Vec<_>>().join(", "))),
    };
    let notes: Vec<String> = {
        let mut v: Vec<String> = motion.warnings.iter().map(|i| i.code.clone()).collect();
        v.dedup();
        v.sort();
        v.dedup();
        v
    };
    let images = raster::decode_images(&motion, Some(&dir.join("corpus")));
    let mut out = Vec::new();
    for frame in frames {
        let reference = load_png(&dir.join("refs").join(format!("{name}@{frame}.png")));
        let painted = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let mut list = DisplayList::default();
            motion.sample(frame as f64, &mut list);
            from_pixmap(&raster::paint(&list, w, h, &images))
        }));
        match painted {
            Ok(ours) => {
                let s = ssim(&reference, &ours);
                if let Some(c) = contact {
                    contact_sheet(&reference, &ours, &c.join(format!("{:.4}-{name}@{frame}.png", s)));
                }
                let note = (!notes.is_empty()).then(|| format!("validator notes: {}", notes.join(", ")));
                out.push(Outcome { file: name.into(), frame, ssim: s, note });
            }
            Err(e) => {
                let msg = e.downcast_ref::<String>().cloned().or_else(|| e.downcast_ref::<&str>().map(|s| s.to_string())).unwrap_or_default();
                out.push(Outcome { file: name.into(), frame, ssim: 0.0, note: Some(format!("panic: {msg}")) });
            }
        }
    }
    out
}

fn write_json(path: &Path, v: &Value) {
    std::fs::write(path, format!("{}\n", serde_json::to_string_pretty(v).unwrap())).unwrap();
}

#[test]
fn corpus_matches_lottie_web() {
    let dir = root();
    let index: Value = serde_json::from_slice(&std::fs::read(dir.join("refs/index.json")).expect("refs/index.json (run render-refs.ts)")).unwrap();
    let only = std::env::var("MOTION_ONLY").ok();
    let contact = std::env::var("MOTION_CONTACT_DIR").ok().map(PathBuf::from);
    if let Some(c) = &contact {
        std::fs::create_dir_all(c).unwrap();
    }
    let files: Vec<(String, Value)> = index["files"]
        .as_object()
        .unwrap()
        .iter()
        .filter(|(k, _)| only.as_deref().is_none_or(|o| k.contains(o)))
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();

    // A work queue over the files, one worker per core.
    let queue = Mutex::new(files.clone().into_iter());
    let results = Mutex::new(Vec::new());
    let workers = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4).min(16);
    std::thread::scope(|s| {
        for _ in 0..workers {
            s.spawn(|| loop {
                let Some((name, sizes)) = queue.lock().unwrap().next() else { break };
                let r = run_file(&name, &sizes, contact.as_deref());
                results.lock().unwrap().extend(r);
            });
        }
    });
    let mut results = results.into_inner().unwrap();
    results.sort_by(|a, b| (a.file.as_str(), a.frame).cmp(&(b.file.as_str(), b.frame)));

    let key = |o: &Outcome| format!("{}@{}", o.file, o.frame);
    let gaps_path = dir.join("known-gaps.json");
    let gaps: BTreeMap<String, String> = std::fs::read(&gaps_path)
        .ok()
        .and_then(|b| serde_json::from_slice::<Value>(&b).ok())
        .and_then(|v| v.get("gaps").cloned())
        .and_then(|g| serde_json::from_value(g).ok())
        .unwrap_or_default();

    let passed = results.iter().filter(|o| o.ssim >= THRESHOLD).count();
    if only.is_none() {
        let cases: Vec<Value> = results
            .iter()
            .map(|o| json!({"file": o.file, "frame": o.frame, "ssim": (o.ssim * 1e4).round() / 1e4, "pass": o.ssim >= THRESHOLD}))
            .collect();
        write_json(
            &dir.join("report.json"),
            &json!({
                "$comment": "AUTO-GENERATED by apps/desktop/crates/eui-motion/tests/corpus.rs on every run: eui-motion (tiny-skia raster) vs lottie-web (SVG, Chromium) per corpus frame. ssim = min over the white and black backgrounds of the 11×11 Gaussian SSIM of the luma; pass = ssim ≥ threshold.",
                "threshold": THRESHOLD,
                "summary": {"cases": results.len(), "passed": passed, "knownGaps": gaps.len()},
                "cases": cases,
            }),
        );
    }

    if std::env::var("UPDATE_MOTION_REPORT").is_ok() {
        let mut next = if only.is_some() { gaps.clone() } else { BTreeMap::new() };
        for o in &results {
            let k = key(o);
            if o.ssim >= THRESHOLD {
                next.remove(&k);
            } else {
                let reason = gaps.get(&k).cloned().unwrap_or_else(|| {
                    format!("ssim {:.4}; {}", o.ssim, o.note.clone().unwrap_or_else(|| "not triaged".into()))
                });
                next.insert(k, reason);
            }
        }
        let mut m = Map::new();
        m.insert("$comment".into(), json!("EXP-1602: corpus frames below the SSIM bar (tests/corpus.rs), each with the reason. This list only SHRINKS: a listed frame that passes fails the test until it is removed. UPDATE_MOTION_REPORT=1 rewrites it from the current results (reasons of still-failing frames are kept)."));
        m.insert("gaps".into(), serde_json::to_value(&next).unwrap());
        write_json(&gaps_path, &Value::Object(m));
        return;
    }

    let mut problems = Vec::new();
    for o in &results {
        let k = key(o);
        let pass = o.ssim >= THRESHOLD;
        match (pass, gaps.contains_key(&k)) {
            (false, false) => problems.push(format!("{k}: ssim {:.4} < {THRESHOLD} ({})", o.ssim, o.note.as_deref().unwrap_or("-"))),
            (true, true) => problems.push(format!("{k}: now passes (ssim {:.4}): remove it from known-gaps.json", o.ssim)),
            _ => {}
        }
    }
    if only.is_none() {
        let present: std::collections::HashSet<String> = results.iter().map(key).collect();
        for k in gaps.keys().filter(|k| !present.contains(*k)) {
            problems.push(format!("{k}: in known-gaps.json but not a corpus case: remove it"));
        }
    }
    eprintln!("motion corpus: {passed}/{} frames pass (SSIM ≥ {THRESHOLD}), {} known gaps", results.len(), gaps.len());
    assert!(problems.is_empty(), "{} problem(s):\n{}", problems.len(), problems.join("\n"));
}
