//! #PERF-15: ONE prop delta in the 250-component surface (the gpui painter's
//! `eui-gpui/examples/bench_delta.json`) through the core alone, with the
//! fixed measurer: `apply` (the `updateComponents`), `reduce` (the tree
//! re-reduced), `layout` (rebuild + restyle + taffy + output). 20 warm-up
//! deltas, 200 timed.
//!
//! ```text
//! cargo run --release -p eui-core --example bench_delta_core
//! ```

use std::time::Instant;

use eui_core::measure::FixedMeasure;
use eui_core::surface::{Surface, SurfaceOptions};
use eui_core::themes::builtin_theme;
use eui_core::FlatComponent;
use serde_json::{json, Value};

fn stats(mut v: Vec<f64>) -> String {
    let mean = v.iter().sum::<f64>() / v.len() as f64;
    v.sort_by(f64::total_cmp);
    let p95 = v[((v.len() as f64 * 0.95).ceil() as usize).min(v.len()) - 1];
    format!("{mean:.3} / {:.3} / {p95:.3}", v[v.len() / 2])
}

fn main() {
    let fixture: Value = serde_json::from_str(include_str!("../../eui-gpui/examples/bench_delta.json")).expect("bench_delta.json");
    let components: Vec<FlatComponent> = serde_json::from_value(fixture["components"].clone()).expect("components");
    let (w, h) = (fixture["viewport"]["width"].as_f64().unwrap() as f32, fixture["viewport"]["height"].as_f64().unwrap() as f32);
    let mut measure = FixedMeasure { sizes: Default::default(), wrap: true };
    let mut surface = Surface::new("bench", SurfaceOptions::default());
    surface.set_theme(builtin_theme("exponential"));
    surface.set_components(components);
    surface.set_viewport(w, h, None);
    surface.layout(&mut measure);

    type Case = (&'static str, fn(usize) -> Value);
    let cases: [Case; 5] = [
        ("text", |k| json!({"id": "t17", "component": "Text", "text": format!("Item 17 changed {k}"), "variant": "caption"})),
        ("same-size text", |k| json!({"id": "t17", "component": "Text", "text": format!("Item 17 caption{}", k % 2), "variant": "caption"})),
        ("paint-only style", |k| json!({"id": "c17", "component": "Box", "style": {"display": "flex", "flexDirection": "column", "gap": 4, "width": 196, "padding": 8, "borderWidth": 1, "borderColor": if k % 2 == 0 { "$color.primary" } else { "$color.border" }, "borderRadius": 8}, "children": ["t17", "r17", "b17"]})),
        ("button label", |k| json!({"id": "b17", "component": "Button", "label": format!("Open {k}"), "variant": "outline", "size": "sm"})),
        ("row title", |k| json!({"id": "r17", "component": "Row", "title": format!("Row {k}"), "meta": "17"})),
    ];
    let only = std::env::var("BENCH_DELTA").ok();
    println!("core delta ({} components, fixed measurer, ms mean / median / p95 of 200)", fixture["components"].as_array().unwrap().len());
    for (name, make) in cases {
        if only.as_deref().is_some_and(|o| o != name) {
            continue;
        }
        let (mut apply, mut reduce, mut layout, mut total) = (Vec::new(), Vec::new(), Vec::new(), Vec::new());
        let mut built = 0;
        for k in 0..220 {
            let c: FlatComponent = serde_json::from_value(make(k)).unwrap();
            let t0 = Instant::now();
            surface.update_components(vec![c]);
            let a = t0.elapsed().as_secs_f64() * 1000.0;
            let t1 = Instant::now();
            let _ = surface.root();
            let r = t1.elapsed().as_secs_f64() * 1000.0;
            let t2 = Instant::now();
            let out = surface.layout(&mut measure);
            let l = t2.elapsed().as_secs_f64() * 1000.0;
            if k >= 20 {
                apply.push(a);
                reduce.push(r);
                layout.push(l);
                total.push(t0.elapsed().as_secs_f64() * 1000.0);
                built = out.built_nodes;
            }
        }
        println!("  {name:<17} total {} · apply {} · reduce {} · layout {} · built {built}", stats(total), stats(apply), stats(reduce), stats(layout));
    }
    // A data write one Text binds (the binding dependents only).
    let mut data_case = Surface::new("bench-data", SurfaceOptions::default());
    data_case.set_theme(builtin_theme("exponential"));
    let mut comps: Vec<Value> = fixture["components"].as_array().unwrap().clone();
    for c in comps.iter_mut() {
        if c["id"] == "t17" {
            c["text"] = json!({"@path": "/t17"});
        }
    }
    data_case.set_components(serde_json::from_value(Value::Array(comps)).unwrap());
    data_case.set_data("/t17", Some(json!("bound"))).unwrap();
    data_case.set_viewport(w, h, None);
    data_case.layout(&mut measure);
    let mut total = Vec::new();
    let mut built = 0;
    for k in 0..220 {
        let t0 = Instant::now();
        data_case.set_data("/t17", Some(json!(format!("bound {k}")))).unwrap();
        let out = data_case.layout(&mut measure);
        if k >= 20 {
            total.push(t0.elapsed().as_secs_f64() * 1000.0);
            built = out.built_nodes;
        }
    }
    if only.as_deref().is_none_or(|o| o == "data") {
        println!("  {:<17} total {} · built {built}", "data write", stats(total));
    }
}
