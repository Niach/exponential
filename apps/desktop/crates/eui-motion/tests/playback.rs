//! EXP-1604: the shared playback clock, pinned by
//! packages/exponential-ui/fixtures/motion-playback.json (replayed over
//! WASM in TS and over uniffi in Swift and Kotlin). `UPDATE_MOTION_FIXTURES=1`
//! rewrites the expected frames from the engine.

use eui_motion::playback::{Playback, Timeline};
use serde_json::{json, Value};
use std::path::PathBuf;

fn path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../../packages/exponential-ui/fixtures/motion-playback.json")
}

fn cases() -> Vec<(&'static str, Value, Vec<f64>)> {
    let t = [0.0, 250.0, 500.0, 999.0, 1000.0, 1500.0, 2250.0, 5000.0];
    vec![
        ("default-loop", json!({}), t.to_vec()),
        ("once-holds-last", json!({"loop": false}), t.to_vec()),
        ("plays-2", json!({"plays": 2}), t.to_vec()),
        ("reverse", json!({"direction": "reverse"}), t.to_vec()),
        ("alternate", json!({"direction": "alternate"}), t.to_vec()),
        ("speed-2", json!({"speed": 2}), t.to_vec()),
        ("segment-ms", json!({"segment": [200, 600]}), t.to_vec()),
        ("marker", json!({"marker": "pop"}), t.to_vec()),
        ("paused-poster", json!({"autoplay": false, "poster": 500}), t.to_vec()),
        ("bad-values-default", json!({"speed": -3, "direction": "sideways", "segment": [900, 100]}), t.to_vec()),
    ]
}

#[test]
fn playback_fixture() {
    // A 1 s composition at 30 fps with a marker `pop` at frames 10–15.
    let timeline = Timeline { frame_rate: 30.0, in_point: 0.0, out_point: 30.0, markers: vec![("pop".into(), 10.0, 5.0)] };
    let mut out = Vec::new();
    for (id, props, times) in cases() {
        let p = Playback::from_json(&props);
        let ticks: Vec<Value> = times.iter().map(|t| { let k = timeline.tick(&p, *t); json!({"elapsedMs": t, "frame": k.frame, "done": k.done}) }).collect();
        out.push(json!({"id": id, "props": props, "poster": timeline.poster(&p), "progressHalf": timeline.at_progress(&p, 0.5), "ticks": ticks}));
    }
    let doc = json!({
        "$comment": "EXP-1604: eUI Motion's playback clock (apps/desktop/crates/eui-motion/src/playback.rs), the ONE clock every renderer uses: Playback::from_json(props) then tick(elapsedMs) -> {frame, done}, poster(), at_progress(0.5). Timeline: 30 fps, in 0, out 30, marker `pop` = frames 10..15 (lottie `markers` [{cm:\"pop\", tm:10, dr:5}]). Replayed by eui-motion tests/playback.rs (generator: UPDATE_MOTION_FIXTURES=1), over WASM (packages/exponential-ui src/motion) and over uniffi (Swift, Kotlin) with the `lottie` below.",
        "lottie": {"v": "5.12.0", "fr": 30, "ip": 0, "op": 30, "w": 10, "h": 10, "layers": [], "markers": [{"cm": "pop", "tm": 10, "dr": 5}]},
        "cases": out,
    });
    let text = serde_json::to_string_pretty(&doc).unwrap() + "\n";
    if std::env::var("UPDATE_MOTION_FIXTURES").is_ok() {
        std::fs::write(path(), &text).unwrap();
    }
    let on_disk = std::fs::read_to_string(path()).expect("fixtures/motion-playback.json (UPDATE_MOTION_FIXTURES=1 writes it)");
    assert_eq!(on_disk, text, "motion-playback.json drifted from the engine: rerun with UPDATE_MOTION_FIXTURES=1");
}
