//! EXP-1604: the per-renderer pixel-parity references. Every case and frame
//! of fixtures/motion-frames.json painted by the `raster` reference at
//! 160×160 (contain-fit, transparent) into conformance/motion/parity/
//! <id>@<frame>.png. Canvas2D, SwiftUI, Compose and gpui paint the same
//! packed lists and compare against these. `UPDATE_MOTION_FIXTURES=1`
//! rewrites them; otherwise this test fails when the raster drifts.
#![cfg(feature = "raster")]

use eui_motion::{raster, DisplayList, Motion};
use std::path::PathBuf;

pub const SIZE: u32 = 160;

#[test]
fn parity_references() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../../packages/exponential-ui");
    let frames: serde_json::Value = serde_json::from_slice(&std::fs::read(root.join("fixtures/motion-frames.json")).unwrap()).unwrap();
    let dir = root.join("conformance/motion/parity");
    let update = std::env::var("UPDATE_MOTION_FIXTURES").is_ok();
    std::fs::create_dir_all(&dir).unwrap();
    let mut drift = Vec::new();
    for c in frames["cases"].as_array().unwrap() {
        let id = c["id"].as_str().unwrap();
        let motion = Motion::load(serde_json::to_string(&c["lottie"]).unwrap().as_bytes()).unwrap();
        let images = raster::decode_images(&motion, None);
        for f in c["frames"].as_array().unwrap() {
            let frame = f["frame"].as_f64().unwrap();
            let mut list = DisplayList::default();
            motion.sample(frame, &mut list);
            let png = raster::paint(&list, SIZE, SIZE, &images).encode_png().unwrap();
            let path = dir.join(format!("{id}@{frame}.png"));
            if update {
                std::fs::write(&path, &png).unwrap();
            } else if std::fs::read(&path).ok().as_deref() != Some(&png[..]) {
                drift.push(path.file_name().unwrap().to_string_lossy().to_string());
            }
        }
    }
    assert!(drift.is_empty(), "parity references drifted (UPDATE_MOTION_FIXTURES=1 rewrites them): {drift:?}");
}
