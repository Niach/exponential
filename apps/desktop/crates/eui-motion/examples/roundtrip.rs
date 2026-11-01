//! Dialect round trip over a directory of Lottie files:
//! d1 = decompile(L); L2 = compile(d1); d2 = decompile(L2);
//! d1 == d2 byte for byte, and L, L2 sample to the same display lists.
//!
//!   cargo run --release -p eui-motion --example roundtrip -- <dir>
//! (tests/dialect.rs runs the same check over the committed corpus).

use eui_motion::{dialect, DisplayList, Limits, Motion};

fn main() {
    let dir = std::env::args().nth(1).expect("dir");
    let mut files: Vec<_> = std::fs::read_dir(&dir).unwrap().filter_map(|e| e.ok()).map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "json")).collect();
    files.sort();
    let unlimited = Limits { max_bytes: usize::MAX, max_shapes: usize::MAX, max_seconds: f64::MAX, max_vertices: usize::MAX };
    let (mut ok, mut bad) = (0, 0);
    for p in files {
        let name = p.file_stem().unwrap().to_string_lossy().to_string();
        let bytes = std::fs::read(&p).unwrap();
        let l: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        let d1 = dialect::decompile(&l);
        let l2 = match dialect::compile(&d1) {
            Ok(v) => v,
            Err(e) => {
                bad += 1;
                println!("{name}: COMPILE {:?}", e.iter().take(3).map(|i| format!("{} {} {}", i.code, i.path, i.message)).collect::<Vec<_>>());
                continue;
            }
        };
        let d2 = dialect::decompile(&l2);
        let (s1, s2) = (serde_json::to_string(&d1).unwrap(), serde_json::to_string(&d2).unwrap());
        let stable = s1 == s2;
        let m1 = Motion::load_with(&bytes, &unlimited, &Default::default());
        let m2 = Motion::load_with(serde_json::to_string(&l2).unwrap().as_bytes(), &unlimited, &Default::default());
        let mut same = true;
        if let (Ok(m1), Ok(m2)) = (&m1, &m2) {
            let (a, b) = m1.frames();
            for t in [0.0, 0.25, 0.5, 0.75, 0.99] {
                let f = a + (b - a) * t;
                let (mut x, mut y) = (DisplayList::default(), DisplayList::default());
                m1.sample(f, &mut x);
                m2.sample(f, &mut y);
                let (mut px, mut py) = (Vec::new(), Vec::new());
                x.encode(&mut px);
                y.encode(&mut py);
                if px.len() != py.len() || px.iter().zip(&py).any(|(a, b)| (a - b).abs() > 1e-3 * (1.0 + a.abs())) {
                    same = false;
                    let at = px.iter().zip(&py).position(|(a, b)| (a - b).abs() > 1e-3 * (1.0 + a.abs())).unwrap_or(px.len().min(py.len()));
                    println!("  frame {f}: len {} vs {}, first diff at {at}: {:?} vs {:?}", px.len(), py.len(), &px[at.saturating_sub(4)..(at + 6).min(px.len())], &py[at.saturating_sub(4)..(at + 6).min(py.len())]);
                    break;
                }
            }
        } else {
            same = false;
        }
        if stable && same {
            ok += 1;
        } else {
            bad += 1;
            let at = s1.bytes().zip(s2.bytes()).position(|(a, b)| a != b).unwrap_or(s1.len().min(s2.len()));
            println!("{name}: stable={stable} same_render={same} {}", if stable { String::new() } else { format!("\n  d1: …{}\n  d2: …{}", &s1[at.saturating_sub(80)..(at + 80).min(s1.len())], &s2[at.saturating_sub(80)..(at + 80).min(s2.len())]) });
        }
    }
    println!("{ok} ok, {bad} failing");
}
