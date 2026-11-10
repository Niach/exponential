//! #EXP-1629: `fixtures/skeleton-rows.json` in the layout core: a
//! Skeleton's `rows` are the app's placeholder (feedback-states.json
//! `placeholder`): the row height, a 12 px dot, the 56 px lead bar and the
//! title bar (8 px at radius 4), 12 px apart and in from the edges, and NO
//! hairline between rows. gpui paints these nodes; SwiftUI and Compose get
//! them through the FFI.

use std::collections::HashMap;

use eui_core::measure::FixedMeasure;
use eui_core::surface::{Surface, SurfaceOptions};
use eui_core::theme::{recipe_style, RecipeQuery};
use eui_core::types::NestedNode;
use serde_json::{json, Map, Value};

const PKG: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../../../packages/exponential-ui");

fn fixture() -> Value {
    let path = format!("{PKG}/fixtures/skeleton-rows.json");
    serde_json::from_str(&std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path}: {e}"))).expect("the fixture")
}

fn num(v: &Value) -> f64 {
    v.as_f64().expect("a number")
}

#[test]
fn every_case_lays_the_contract_rows_out() {
    let f = fixture();
    assert_eq!(f["row"]["hairline"], json!(false));
    for c in f["cases"].as_array().unwrap() {
        let name = c["name"].as_str().unwrap();
        let tree: NestedNode = serde_json::from_value(json!({"id": "root", "component": "Box", "style": {"display": "flex", "flexDirection": "column", "width": c["width"]},
            "children": [{"id": "sk", "component": "Skeleton", "props": c["skeleton"]}]}))
        .unwrap();
        let mut s = Surface::new("t", SurfaceOptions::default());
        let outcome = s.set_nested(tree);
        assert!(outcome.issues.is_empty(), "{name}: {:?}", outcome.issues);
        s.set_viewport(600.0, 800.0, None);
        let out = s.layout(&mut FixedMeasure { sizes: HashMap::new(), wrap: true });
        let root = out.frames.iter().find(|fr| fr.index == s.index_of("root").unwrap()).copied().unwrap();
        for (id, want) in c["frames"].as_object().unwrap() {
            let i = s.index_of(id).unwrap_or_else(|| panic!("{name}: no node {id}"));
            let fr = out.frames.iter().find(|fr| fr.index == i).copied().unwrap_or_else(|| panic!("{name}: no frame {id}"));
            let got = [fr.x - root.x, fr.y - root.y, fr.w, fr.h].map(f64::from);
            let want: Vec<f64> = want.as_array().unwrap().iter().map(num).collect();
            assert_eq!(got.to_vec(), want, "{name}: {id}");
        }
        let rows = c["skeleton"]["rows"].as_u64().unwrap();
        for r in 0..rows {
            let row = s.layout_node(s.index_of(&format!("sk.row.{r}")).unwrap()).unwrap();
            for key in ["borderTopWidth", "borderWidth", "borderBottomWidth", "borderColor"] {
                assert!(!row.base_style.contains_key(key), "{name}: row {r} has no {key}");
            }
        }
        let dots = s.nodes().into_iter().filter(|n| n.part.as_deref() == Some("dot") && n.owner.as_deref() == Some("sk")).count() as u64;
        assert_eq!(dots, if c["skeleton"]["dot"] == json!(false) { 0 } else { rows }, "{name}");
    }
}

#[test]
fn every_built_in_theme_gives_the_rows_the_contract_recipe() {
    let f = fixture();
    let p = &f["placeholder"];
    let props: Map<String, Value> = serde_json::from_value(json!({"rounded": false, "dot": true})).unwrap();
    for id in eui_core::themes::BUILTIN_THEME_IDS {
        let t = eui_core::themes::builtin_theme(id).unwrap();
        let part = |name: &str| recipe_style(&t, &RecipeQuery::new("Skeleton", name, props.clone(), vec![]));
        let spacing = |v: &Value| match v {
            Value::String(s) => t.tokens.spacing[s.trim_start_matches("$spacing.")],
            other => num(other),
        };
        let row = part("row");
        assert_eq!((spacing(&row["gap"]), spacing(&row["paddingHorizontal"])), (num(&f["row"]["gap"]), num(&f["row"]["paddingHorizontal"])), "{id} row");
        assert!(!row.contains_key("borderTopWidth") && !row.contains_key("borderWidth"), "{id}: a borderless row");
        let dot = part("dot");
        assert_eq!((num(&dot["width"]), num(&dot["height"]), &dot["backgroundColor"]), (num(&p["dot"]), num(&p["dot"]), &f["fill"]), "{id} dot");
        let bar = part("bar");
        assert_eq!((num(&bar["height"]), num(&bar["borderRadius"]), &bar["backgroundColor"]), (num(&p["barHeight"]), num(&p["radius"]), &f["fill"]), "{id} bar");
    }
    let low_high = [eui_core::layout::SKELETON_OPACITY_LOW, eui_core::layout::SKELETON_OPACITY_HIGH];
    assert_eq!(low_high.to_vec(), p["opacity"].as_array().unwrap().iter().map(num).collect::<Vec<_>>());
    assert_eq!(eui_core::layout::SKELETON_PULSE_MS, num(&p["pulseMs"]));
    assert_eq!(eui_core::layout::SKELETON_REDUCED_OPACITY, num(&p["reducedOpacity"]));
}
