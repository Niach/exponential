//! EXP-1601 spike harness: for every Lottie file in a directory, validate,
//! sample every frame (timing the display-list build), paint three frames
//! with the raster (timing the paint at 600×400 @2x) and write PNGs.
//!
//!   cargo run --release -p eui-motion --features raster --example spike -- <dir> <out>

use eui_motion::{display::DisplayList, raster, validate, Limits, Motion};
use std::collections::BTreeMap;
use std::time::Instant;

fn main() {
    let mut args = std::env::args().skip(1);
    let dir = args.next().expect("corpus dir");
    let out = args.next().expect("out dir");
    std::fs::create_dir_all(&out).unwrap();
    let mut entries: Vec<_> = std::fs::read_dir(&dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "json"))
        .collect();
    entries.sort();
    let mut gaps: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut rows = Vec::new();
    // Limits off for the census: the corpus includes files over 2 MB.
    let unlimited = Limits {
        max_bytes: usize::MAX,
        max_shapes: usize::MAX,
        max_seconds: f64::MAX,
        max_vertices: usize::MAX,
    };
    for path in entries {
        let name = path.file_stem().unwrap().to_string_lossy().to_string();
        let bytes = std::fs::read(&path).unwrap();
        let strict = validate::check(&bytes, &Limits::default());
        let motion = match Motion::load_with(&bytes, &unlimited, &Default::default()) {
            Ok(m) => m,
            Err(e) => {
                println!("{name}: REFUSED {:?}", e.iter().map(|i| &i.code).collect::<Vec<_>>());
                continue;
            }
        };
        for w in &motion.warnings {
            let v = gaps.entry(w.code.clone()).or_default();
            if !v.contains(&name) {
                v.push(name.clone());
            }
        }
        let (a, b) = motion.frames();
        let mut list = DisplayList::default();
        let mut packed = Vec::new();
        let n = (b - a).ceil().max(1.0) as usize;
        let mut worst = 0f64;
        let mut verts = 0usize;
        let mut ops = 0usize;
        let t0 = Instant::now();
        for i in 0..n {
            let s = Instant::now();
            motion.sample(a + i as f64, &mut list);
            list.encode(&mut packed);
            worst = worst.max(s.elapsed().as_secs_f64());
            verts = verts.max(list.vertex_count());
            ops = ops.max(list.ops.len());
        }
        let mean = t0.elapsed().as_secs_f64() / n as f64;
        let mut paint_worst = 0f64;
        for (k, f) in [0.0, 0.5, 0.9].iter().enumerate() {
            motion.sample(a + (b - a) * f, &mut list);
            let s = Instant::now();
            let pix = raster::paint(&list, 1200, 800, &[]);
            paint_worst = paint_worst.max(s.elapsed().as_secs_f64());
            pix.save_png(format!("{out}/{name}-{k}.png")).unwrap();
        }
        rows.push(format!(
            "| {name} | {} | {n} | {:.3} | {:.3} | {:.2} | {ops} | {verts} | {} |",
            bytes.len() / 1024,
            mean * 1e3,
            worst * 1e3,
            paint_worst * 1e3,
            if strict.errors.is_empty() {
                "ok".to_string()
            } else {
                strict.errors.iter().map(|i| i.code.as_str()).collect::<Vec<_>>().join(",")
            }
        ));
    }
    println!("| file | KB | frames | sample mean ms | sample worst ms | paint worst ms (1200×800) | ops | vertices | limits |");
    println!("|---|---|---|---|---|---|---|---|---|");
    for r in rows {
        println!("{r}");
    }
    println!("\n## gaps");
    for (code, files) in gaps {
        println!("- {code}: {}", files.join(", "));
    }
}
