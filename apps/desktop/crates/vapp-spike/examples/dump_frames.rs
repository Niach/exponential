//! `cargo run -p vapp-spike --example dump_frames -- <width> [--rtl] [--pressed id,id] [--bench N]`
//! Prints the fixed-measure frames as JSON (rounding OFF, like a browser) for
//! the web geometry diff.

use serde_json::json;
use vapp_spike::{bench, FixedMeasure, Surface, KITCHEN_SINK_JSON};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let width: f32 = args.first().and_then(|a| a.parse().ok()).unwrap_or(900.0);
    let rtl = args.iter().any(|a| a == "--rtl");
    let pressed: Vec<String> = args
        .iter()
        .position(|a| a == "--pressed")
        .and_then(|i| args.get(i + 1))
        .map(|s| s.split(',').map(str::to_string).collect())
        .unwrap_or_default();
    let bench_n: Option<usize> = args
        .iter()
        .position(|a| a == "--bench")
        .and_then(|i| args.get(i + 1))
        .and_then(|s| s.parse().ok());

    let mut json = match bench_n {
        Some(n) => bench::bench_tree(n),
        None => KITCHEN_SINK_JSON.to_string(),
    };
    if rtl {
        let mut v: serde_json::Value = serde_json::from_str(&json).unwrap();
        v["style"]["direction"] = json!("rtl");
        json = v.to_string();
    }
    let mut surface = Surface::new(&json).expect("parse");
    surface.set_rounding(false);
    surface.set_viewport(width, 0.0);
    surface.set_pressed(&pressed);
    let result = surface.layout(&mut FixedMeasure);
    let nodes: Vec<_> = result
        .nodes
        .iter()
        .map(|n| {
            json!({
                "id": n.id, "kind": n.kind, "depth": n.depth,
                "x": n.frame.x, "y": n.frame.y, "w": n.frame.w, "h": n.frame.h,
            })
        })
        .collect();
    let out = json!({
        "width": width,
        "rtl": rtl,
        "pressed": pressed,
        "surfaceWidth": result.surface_width,
        "surfaceHeight": result.surface_height,
        "measureCalls": result.measure_calls,
        "layoutNs": result.layout_ns,
        "buildNs": surface.build_ns,
        "nodeCount": surface.node_count(),
        "nodes": nodes,
    });
    println!("{}", serde_json::to_string_pretty(&out).unwrap());
}
