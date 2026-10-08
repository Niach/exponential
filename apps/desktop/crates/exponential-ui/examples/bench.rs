//! Release timings for the README: `cargo run --release -p exponential-ui --example bench`.
//! - a ~200-node card grid: warm passes alternating 390/900 px;
//! - a 2,000-row windowed list: scroll steps (re-window) and hover churn
//!   (one restyle) at 390 px;
//! - round 2: the shared 100,000-row bench (`fixtures/bench-list.json`).

use std::time::Instant;

use exponential_ui::list::ScrollAlign;
use exponential_ui::themes::builtin_theme;
use exponential_ui::FlatComponent;
use serde_json::{json, Value};

use exponential_ui::bench::{bench_list_tree, bench_tree};
use exponential_ui::measure::FixedMeasure;
use exponential_ui::surface::{Surface, SurfaceOptions};

fn median(mut v: Vec<u64>) -> u64 {
    v.sort_unstable();
    v[v.len() / 2]
}

fn main() {
    let mut measure = FixedMeasure { sizes: Default::default(), wrap: true };

    let mut grid = Surface::new("grid", SurfaceOptions::default());
    grid.set_nested(bench_tree(200));
    grid.set_viewport(390.0, 0.0, None);
    grid.layout(&mut measure);
    let mut passes = Vec::new();
    for i in 0..200 {
        grid.set_viewport(if i % 2 == 0 { 900.0 } else { 390.0 }, 0.0, None);
        let t = Instant::now();
        grid.layout(&mut measure);
        passes.push(t.elapsed().as_nanos() as u64);
    }
    println!("grid ({} nodes): warm width change, median {:.1} µs", grid.node_count(), median(passes) as f64 / 1000.0);

    let mut list = Surface::new("list", SurfaceOptions::default());
    list.set_nested(bench_list_tree(2_000));
    list.set_viewport(390.0, 844.0, None);
    for _ in 0..3 {
        list.layout(&mut measure);
    }
    let mut scrolls = Vec::new();
    let mut hovers = Vec::new();
    let mut restyled = 0u32;
    for step in 0..400u32 {
        list.scroll("list", (step % 200) as f32 * 300.0);
        let t = Instant::now();
        let out = list.layout(&mut measure);
        scrolls.push(t.elapsed().as_nanos() as u64);
        restyled = restyled.max(out.restyled);
        let row = format!("row-{}", (step % 200) * 3 + 2);
        if list.index_of(&row).is_some() {
            list.set_states(&row, vec!["hover".into()]);
            let t = Instant::now();
            list.layout(&mut measure);
            hovers.push(t.elapsed().as_nanos() as u64);
            list.set_states(&row, vec![]);
            list.layout(&mut measure);
        }
    }
    println!(
        "list (2,000 rows, {} live nodes): scroll step median {:.1} µs (max restyled {restyled}), hover median {:.1} µs",
        list.node_count(),
        median(scrolls) as f64 / 1000.0,
        median(hovers) as f64 / 1000.0
    );

    bench_100k(&mut measure);
}

/// Round 2 (docs/round-2-contract.md §5): `fixtures/bench-list.json`.
fn bench_100k(measure: &mut FixedMeasure) {
    let fixture: Value = serde_json::from_str(include_str!("../../../../../packages/exponential-ui/fixtures/bench-list.json")).expect("bench-list.json");
    let count = fixture["rows"]["count"].as_u64().unwrap() as usize;
    let rows: Vec<Value> = (0..count).map(|i| json!({"id": format!("r{i}"), "title": format!("Row {i}"), "meta": (i % 97).to_string()})).collect();
    let components: Vec<FlatComponent> = serde_json::from_value(fixture["components"].clone()).unwrap();
    let (w, h) = (fixture["viewport"]["width"].as_f64().unwrap() as f32, fixture["viewport"]["height"].as_f64().unwrap() as f32);
    let theme = builtin_theme(fixture["theme"].as_str().unwrap());
    let t = Instant::now();
    let mut s = Surface::new("bench", SurfaceOptions { theme, catalog_id: fixture["catalogId"].as_str().unwrap().to_string(), ..SurfaceOptions::default() });
    s.set_data("", Some(json!({"rows": rows})));
    s.set_components(components);
    s.set_viewport(w, h, None);
    s.layout(measure);
    let first_paint = t.elapsed().as_secs_f64() * 1000.0;
    let steps = fixture["steps"]["scroll"]["count"].as_u64().unwrap() as usize;
    let mut total = 0.0;
    for k in 1..=steps {
        s.scroll("root", k as f32 * h);
        let t = Instant::now();
        s.layout(measure);
        total += t.elapsed().as_secs_f64() * 1000.0;
    }
    let index = fixture["steps"]["scrollToIndex"]["index"].as_u64().unwrap() as usize;
    let t = Instant::now();
    s.scroll_to_index("root", index, ScrollAlign::Start);
    let out = s.layout(measure);
    let jump = t.elapsed().as_secs_f64() * 1000.0;
    let l = &out.lists[0];
    let rendered = l.end - l.start;
    assert!(s.index_of(&format!("bench-row.r{index}")).is_some(), "item {index} rendered after the jump");
    println!(
        "list (100,000 ListRows, bench-list.json): firstPaintMs {first_paint:.1}, scrollStepMs {:.3}, scrollToIndexMs {jump:.3}, renderedItems {rendered}",
        total / steps as f64
    );
}
