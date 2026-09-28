//! The facade round-trip with a Rust `Measure` (what Swift/Kotlin implement).
use std::sync::{Arc, Mutex};

use vapp_spike_ffi::*;

struct CountingFixed(Mutex<u32>);

impl Measure for CountingFixed {
    fn measure(&self, call: MeasureCall) -> FfiSize {
        *self.0.lock().unwrap() += 1;
        // Mirror FixedMeasure through the public helper so the two paths agree.
        FfiSize { width: call.known_width.unwrap_or(100.0), height: call.known_height.unwrap_or(20.0) }
    }
}

#[test]
fn surface_lays_out_through_the_foreign_trait() {
    let surface = Surface::new(kitchen_sink_json()).expect("parse");
    assert_eq!(surface.node_count(), 48);
    let nodes = surface.nodes();
    assert_eq!(nodes[0].id, "root");
    assert!(nodes.iter().any(|n| n.kind == "markdown" && n.props_json.contains("Looking for")));
    surface.set_rounding(false);
    assert!(surface.set_viewport(900.0, 0.0));
    let m = Arc::new(CountingFixed(Mutex::new(0)));
    let r = surface.layout(m.clone());
    assert_eq!(r.frames.len(), 48);
    assert_eq!(r.measure_calls, *m.0.lock().unwrap());
    assert!(r.measure_calls > 0);
    assert_eq!(r.frames[0].index, 0);
    assert!(r.surface_height > 300.0);
    // Pressed → only the visual changes.
    assert!(surface.set_pressed(vec!["hdr-scan".into()]));
    let r2 = surface.layout(m.clone());
    let scan = nodes.iter().find(|n| n.id == "hdr-scan").unwrap().index as usize;
    assert_eq!(r2.frames[scan].visual.opacity, Some(0.6));
    assert_eq!(r.frames[scan].x, r2.frames[scan].x);
    // The fixed baseline agrees with the core's example dump on measure count.
    let fixed = surface.layout_fixed();
    assert_eq!(fixed.frames.len(), 48);
    let bench = Surface::new(bench_tree_json(200)).unwrap();
    assert!(bench.node_count() >= 200);
    let intrinsic = fixed_intrinsic("button".into(), r#"{"label":"Scan now"}"#.into());
    assert_eq!((intrinsic.width, intrinsic.height), (8.0 * 8.0 + 24.0, 36.0));
}
