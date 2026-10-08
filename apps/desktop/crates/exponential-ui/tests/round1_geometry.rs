//! `fixtures/layout-geometry-round1.json`: the round-1 RESPONSIVE section of
//! the kitchen sink (a Grid going 1 → 2 → 3 columns, a Sidebar whose column
//! hides below `md`, nodes shown only on narrow or wide surfaces, a box with
//! state/orientation/hover/motion blocks, a responsive Drawer, a scrolling
//! chip strip) laid out by the core at 390 / 600 / 700 (the `sm` band) /
//! 768 (EXACTLY `md`: `min-width` is `>=`, `max-width` is `<`) / 900 / 1280
//! px, LTR and RTL, theme `neutral`, every leaf a fixed box (the fixed
//! measure: 8 px per character on one 20 px line, the controls' recipe
//! boxes; `measures` per case). The gpui painter replays it
//! (`exponential-ui-gpui/tests/render.rs`); NO TypeScript/React test reads it
//! yet, so it locks the core and its painters, not the web renderer.
//!
//! Regenerate: `EXP_UI_WRITE_FIXTURES=1 cargo test -p exponential-ui --test round1_geometry`.

use std::collections::HashMap;

use exponential_ui::measure::{FixedMeasure, HeightRequest, Intrinsics, LeafRequest, Measure};
use exponential_ui::surface::{Surface, SurfaceOptions};
use exponential_ui::types::NestedNode;
use exponential_ui::{json, themes};
use indexmap::IndexMap;
use serde_json::{json as j, Value};

const PKG: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../../../packages/exponential-ui");
const OUT: &str = "fixtures/layout-geometry-round1.json";
const WIDTHS: [f32; 6] = [390.0, 600.0, 700.0, 768.0, 900.0, 1280.0];

fn read(rel: &str) -> Value {
    let path = format!("{PKG}/{rel}");
    serde_json::from_str(&std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path}: {e}"))).expect("json")
}

/// The responsive section, wrapped in a full-width root.
fn surface_tree(direction: &str) -> Value {
    let ks = read("fixtures/kitchen-sink.json");
    let section = ks["children"].as_array().unwrap().iter().find(|c| c["id"] == "responsive").cloned().expect("the responsive section");
    j!({"id": "root", "component": "Box", "style": {"display": "flex", "flexDirection": "column", "width": "100%", "padding": 16, "direction": direction}, "children": [section]})
}

/// The fixed measure, recording every leaf's box.
struct Recording {
    inner: FixedMeasure,
    seen: IndexMap<String, (f32, f32)>,
}

impl Measure for Recording {
    fn measure_id(&self) -> u64 {
        1
    }
    fn measure_intrinsics(&mut self, leaves: &[LeafRequest]) -> Vec<Intrinsics> {
        let out = self.inner.measure_intrinsics(leaves);
        for (l, i) in leaves.iter().zip(&out) {
            self.seen.insert(l.id.to_string(), (i.max_content_width, i.height_at_max_content));
        }
        out
    }
    fn measure_heights(&mut self, leaves: &[LeafRequest], requests: &[HeightRequest]) -> Vec<f32> {
        self.inner.measure_heights(leaves, requests)
    }
}

/// One case: the surface at a width and direction → (frames, measures).
fn run(width: f32, direction: &str, hover: bool, sizes: Option<HashMap<String, (f32, f32)>>) -> (Vec<Value>, IndexMap<String, (f32, f32)>) {
    let tree: NestedNode = serde_json::from_value(surface_tree(direction)).unwrap();
    let mut s = Surface::new("geo", SurfaceOptions { theme: themes::builtin_theme("neutral"), expand_controls: Some(false), ..SurfaceOptions::default() });
    let outcome = s.set_nested(tree);
    assert!(outcome.issues.is_empty(), "{:?}", outcome.issues);
    s.set_data("", Some(j!({"ui": {"sidebarCollapsed": false, "filtersOpen": false}})));
    s.set_pointer(hover, false);
    s.set_viewport(width, 0.0, None);
    let mut m = Recording { inner: FixedMeasure { sizes: sizes.unwrap_or_default(), wrap: false }, seen: IndexMap::new() };
    let out = s.layout(&mut m);
    assert!(out.upcalls <= 1, "fixed boxes never need a height round");
    let nodes = s.nodes();
    let frames = out
        .frames
        .iter()
        .map(|f| {
            let r = |v: f32| json::number((v as f64 * 1000.0).round() / 1000.0);
            j!({"id": nodes[f.index as usize].id, "x": r(f.x), "y": r(f.y), "w": r(f.w), "h": r(f.h)})
        })
        .collect();
    (frames, m.seen)
}

fn case_name(width: f32, direction: &str) -> String {
    if direction == "rtl" {
        format!("{width}-rtl")
    } else {
        format!("{width}")
    }
}

fn render() -> Value {
    let mut cases = serde_json::Map::new();
    for width in WIDTHS {
        for direction in ["ltr", "rtl"] {
            let hover = width >= 900.0;
            let (frames, seen) = run(width, direction, hover, None);
            let measures: serde_json::Map<String, Value> = seen.iter().map(|(id, (w, h))| (id.clone(), j!({"w": json::number(*w as f64), "h": json::number(*h as f64)}))).collect();
            cases.insert(
                case_name(width, direction),
                j!({"width": json::number(width as f64), "direction": direction, "context": {"hover": hover, "reducedMotion": false, "height": null}, "measures": measures, "frames": frames}),
            );
        }
    }
    j!({
        "$comment": "Round 1 (docs/round-1-contract.md §2): the RESPONSIVE layout contract, written by the Rust core (apps/desktop/crates/exponential-ui tests/round1_geometry.rs; regenerate with EXP_UI_WRITE_FIXTURES=1). `surface` = the kitchen sink's `responsive` section in a full-width root (padding 16, the case's `direction`), data `data`, theme `neutral` (light), form controls NOT expanded (every native is one measured leaf). Every leaf is a fixed box of `cases[c].measures[id]` (the fixed measure: 8 px per character on ONE 20 px line, the controls' recipe boxes); `context` = the condition inputs (`hover`: a hover-capable pointer, `reducedMotion`, `height: null` = the surface height is the content's, so `orientation` is landscape). `frames` = every drawn node in paint order, x/y/w/h relative to the root, rounded to 1/1000 px. Breakpoints: neutral's 640/768/1024/1280 (`min-width: N` holds from N on, `max-width: N` below N; the 768 case sits exactly on md). A renderer lays the same surface out at each case's width and matches every frame within `tolerancePx`. Replayed by the core (round1_geometry.rs) and the gpui painter (exponential-ui-gpui tests/render.rs); no TypeScript/React test reads it yet.",
        "theme": "neutral",
        "mode": "light",
        "tolerancePx": 1,
        "data": {"ui": {"sidebarCollapsed": false, "filtersOpen": false}},
        "surface": surface_tree("ltr"),
        "cases": cases,
    })
}

#[test]
fn layout_geometry_round1_matches_the_committed_fixture() {
    let rendered = render();
    let path = format!("{PKG}/{OUT}");
    if std::env::var_os("EXP_UI_WRITE_FIXTURES").is_some() {
        std::fs::write(&path, format!("{}\n", serde_json::to_string_pretty(&rendered).unwrap())).unwrap();
    }
    let committed = read(OUT);
    let cases = committed["cases"].as_object().unwrap();
    assert_eq!(cases.len(), 2 * WIDTHS.len());
    for (name, case) in cases {
        let width = case["width"].as_f64().unwrap() as f32;
        let direction = case["direction"].as_str().unwrap();
        let hover = case["context"]["hover"].as_bool().unwrap();
        let sizes: HashMap<String, (f32, f32)> = case["measures"].as_object().unwrap().iter().map(|(id, m)| (id.clone(), (m["w"].as_f64().unwrap() as f32, m["h"].as_f64().unwrap() as f32))).collect();
        // Replayed with the RECORDED measures (what a renderer does).
        let (frames, _) = run(width, direction, hover, Some(sizes));
        let expected = case["frames"].as_array().unwrap();
        assert_eq!(frames.len(), expected.len(), "{name}: node count");
        for (got, want) in frames.iter().zip(expected) {
            assert_eq!(got["id"], want["id"], "{name}: paint order");
            for k in ["x", "y", "w", "h"] {
                let (g, w) = (got[k].as_f64().unwrap(), want[k].as_f64().unwrap());
                assert!((g - w).abs() <= 0.001, "{name}: {}.{k} = {g}, expected {w}", want["id"]);
            }
        }
    }
    assert!(json::equal(&committed["surface"], &rendered["surface"]), "the surface drifted from kitchen-sink.json: regenerate");
}

#[test]
fn layout_geometry_round1_the_responsive_rules_show() {
    let committed = read(OUT);
    let frame = |case: &str, id: &str| -> Option<Value> { committed["cases"][case]["frames"].as_array().unwrap().iter().find(|f| f["id"] == id).cloned() };
    let x = |case: &str, id: &str| frame(case, id).unwrap()["x"].as_f64().unwrap();
    let w = |case: &str, id: &str| frame(case, id).unwrap()["w"].as_f64().unwrap();
    // The grid: 1 column on phones, 2 from sm (640), 3 from lg (1024).
    assert_eq!(x("390", "resp-card-1"), x("390", "resp-card-2"));
    assert_eq!(x("600", "resp-card-1"), x("600", "resp-card-2"), "600 < sm: still one column");
    assert!(x("900", "resp-card-2") > x("900", "resp-card-1"));
    assert_eq!(x("900", "resp-card-3"), x("900", "resp-card-1"), "two columns: the third wraps");
    assert!(x("1280", "resp-card-3") > x("1280", "resp-card-2"), "three columns from lg");
    // The sm band (640–767): two columns already, the sidebar still hidden.
    assert!(x("700", "resp-card-2") > x("700", "resp-card-1"), "700 ≥ sm: two columns");
    assert_eq!(w("700", "resp-shell.sidebar"), 0.0, "700 < md");
    // The sidebar column hides below md (768) and shows FROM it (exactly 768
    // included: `max-width: 768` means < 768).
    assert_eq!(w("600", "resp-shell.sidebar"), 0.0);
    assert_eq!(w("768", "resp-shell.sidebar"), 200.0, "the exact breakpoint is md");
    assert_eq!(w("900", "resp-shell.sidebar"), 200.0);
    // Shown only narrow / only wide.
    assert_eq!(w("390", "resp-only-wide"), 0.0);
    assert!(w("390", "resp-only-narrow") > 0.0);
    assert!(w("1280", "resp-only-wide") > 0.0);
    assert_eq!(w("1280", "resp-only-narrow"), 0.0);
    // RTL mirrors: the sidebar sits at the right edge.
    let root_w = w("900-rtl", "root");
    let sb = frame("900-rtl", "resp-shell.sidebar").unwrap();
    assert!((sb["x"].as_f64().unwrap() + sb["w"].as_f64().unwrap() - (root_w - 16.0 - 1.0)).abs() < 1.5);
}
