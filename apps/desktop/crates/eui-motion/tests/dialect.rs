//! EXP-1603: the dialect round trip over the conformance corpus:
//! d1 = decompile(L); L2 = compile(d1); decompile(L2) == d1 byte for byte,
//! and L, L2 sample to the same display lists (5 frames each).
#![cfg(feature = "decompile")]

use eui_motion::{dialect, DisplayList, Limits, Motion};
use std::path::PathBuf;

#[test]
fn corpus_round_trips() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../../packages/exponential-ui/conformance/motion/corpus");
    let mut files: Vec<_> = std::fs::read_dir(&dir).unwrap().filter_map(|e| e.ok()).map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "json")).collect();
    files.sort();
    assert!(files.len() > 40);
    let unlimited = Limits { max_bytes: usize::MAX, max_shapes: usize::MAX, max_seconds: f64::MAX, max_vertices: usize::MAX };
    let mut problems = Vec::new();
    for p in files {
        let name = p.file_stem().unwrap().to_string_lossy().to_string();
        let bytes = std::fs::read(&p).unwrap();
        let l: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        let d1 = dialect::decompile(&l);
        let l2 = match dialect::compile(&d1) {
            Ok(v) => v,
            Err(e) => {
                problems.push(format!("{name}: compile {:?}", e.first()));
                continue;
            }
        };
        let d2 = dialect::decompile(&l2);
        if serde_json::to_string(&d1).unwrap() != serde_json::to_string(&d2).unwrap() {
            problems.push(format!("{name}: decompile(compile(d)) != d"));
            continue;
        }
        let m1 = Motion::load_with(&bytes, &unlimited, &Default::default()).unwrap();
        let m2 = Motion::load_with(serde_json::to_string(&l2).unwrap().as_bytes(), &unlimited, &Default::default()).unwrap();
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
                problems.push(format!("{name}: frame {f} renders differently"));
                break;
            }
        }
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}
