//! VAPP-91: the Rust core's conformance runner. Replays EVERY suite of
//! `packages/exponential-ui/conformance/manifest.json` against the core,
//! counting cases exactly as the manifest's `unit` says (`src/conformance.ts`),
//! and writes the report (`conformance/report.schema.json`) to
//! `$EXPONENTIAL_UI_CONFORMANCE_REPORT`, default
//! `<CARGO_TARGET_DIR or target>/exponential-ui-conformance/exponential-ui.json`.
//! Check it with `bun run --filter @exponential-at/ui conformance:check <report>`.

mod support;

use std::collections::{BTreeMap, HashMap};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::Arc;

use exponential_ui::catalog::CatalogView;
use exponential_ui::extension::define_extension;
use exponential_ui::geometry::control_geometry;
use exponential_ui::json;
use exponential_ui::measure::FixedMeasure;
use exponential_ui::overlay::{place_overlay, OverlaySide, PlaceOptions, Rect, Size};
use exponential_ui::reducer::{reduce_nested, reduce_surface, ReduceOptions};
use exponential_ui::surface::{Surface, SurfaceOptions};
use exponential_ui::theme::{load_theme, resolve_recipe, try_load_theme, Mode, RecipeQuery, ThemeOptions};
use exponential_ui::themes::{builtin_refs, builtin_theme, BUILTIN_THEME_IDS};
use exponential_ui::types::{ExtensionDef, FlatComponent, NestedNode};
use exponential_ui::{Props, A2UI_BASIC_CATALOG_ID, CORE_CATALOG_ID, UNKNOWN_COMPONENT};
use serde_json::{json, Value};
use support::{fixture, read, same, Case};

const MODES: [Mode; 2] = [Mode::Light, Mode::Dark];

/// One case: a panic is a failure, never the end of the run.
fn case(name: impl Into<String>, f: impl FnOnce() -> Result<(), String>) -> Case {
    let name = name.into();
    let result = catch_unwind(AssertUnwindSafe(f)).unwrap_or_else(|p| {
        let msg = p.downcast_ref::<String>().cloned().or_else(|| p.downcast_ref::<&str>().map(|s| s.to_string())).unwrap_or_else(|| "panicked".into());
        Err(format!("panic: {msg}"))
    });
    (name, result)
}

fn ensure(ok: bool, msg: impl FnOnce() -> String) -> Result<(), String> {
    if ok {
        Ok(())
    } else {
        Err(msg())
    }
}

fn to_value<T: serde::Serialize>(v: &T) -> Value {
    serde_json::to_value(v).expect("serializable")
}

fn obj(v: &Value) -> Props {
    v.as_object().cloned().unwrap_or_default()
}

fn strings(v: Option<&Value>) -> Vec<String> {
    v.and_then(Value::as_array).map(|a| a.iter().filter_map(|s| s.as_str().map(str::to_string)).collect()).unwrap_or_default()
}

/// Lay a surface out with the fixed measure: no Unknown, frames present.
fn paint(surface: &mut Surface, width: f32) -> Result<(), String> {
    surface.set_viewport(width, 0.0, None);
    let out = surface.layout(&mut FixedMeasure { sizes: HashMap::new(), wrap: true });
    ensure(!out.frames.is_empty(), || "no frames".into())?;
    if let Some(n) = surface.nodes().iter().find(|n| n.component == UNKNOWN_COMPONENT) {
        return Err(format!("Unknown placeholder at {}", n.id));
    }
    Ok(())
}

fn catalog_suite() -> Vec<Case> {
    let f = fixture("catalog-components.json");
    let core = ReduceOptions::new(CORE_CATALOG_ID);
    f["cases"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| {
            case(c["name"].as_str().unwrap(), || {
                let node: NestedNode = serde_json::from_value(c["node"].clone()).map_err(|e| e.to_string())?;
                let result = reduce_nested(&node, &core);
                ensure(result.issues.is_empty(), || format!("issues {:?}", result.issues))?;
                let mut surface = Surface::new("catalog", SurfaceOptions::default());
                let outcome = surface.set_nested(node);
                ensure(outcome.issues.is_empty(), || format!("surface issues {:?}", outcome.issues))?;
                paint(&mut surface, 400.0)
            })
        })
        .collect()
}

fn macros_suite() -> Vec<Case> {
    let f = fixture("catalog-macros.json");
    let core = ReduceOptions::new(CORE_CATALOG_ID);
    f["cases"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| {
            case(c["name"].as_str().unwrap(), || {
                let input: NestedNode = serde_json::from_value(c["input"].clone()).map_err(|e| e.to_string())?;
                let result = reduce_nested(&input, &core);
                ensure(result.issues.is_empty(), || format!("issues {:?}", result.issues))?;
                same(&to_value(&result.root), &c["expected"])
            })
        })
        .collect()
}

fn reduce_cases(file: &str, view: Option<Arc<CatalogView>>, catalog: impl Fn(&Value) -> String) -> Vec<Case> {
    let f = fixture(file);
    f["cases"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| {
            case(c["name"].as_str().unwrap(), || {
                let components: Vec<FlatComponent> = serde_json::from_value(c["components"].clone()).map_err(|e| e.to_string())?;
                let mut options = ReduceOptions::new(&catalog(c));
                if let Some(view) = &view {
                    options = options.with_view(view.clone());
                }
                same(&to_value(&reduce_surface(&components, &options)), &c["expected"])
            })
        })
        .collect()
}

fn extension_suite() -> Vec<Case> {
    let f = fixture("catalog-extension.json");
    let def: ExtensionDef = serde_json::from_value(f["extension"].clone()).unwrap();
    let ext = define_extension(def).expect("the example extension is valid");
    let view = CatalogView::with(std::slice::from_ref(&ext));
    let mut cases = reduce_cases("catalog-extension.json", Some(view), |c| c["catalogId"].as_str().unwrap().to_string());
    // Its natives reach the extension painter: an `Extension` leaf.
    if let Some((name, result)) = cases.first_mut() {
        if result.is_ok() {
            let c = &f["cases"][0];
            *result = case(name.clone(), || {
                let components: Vec<FlatComponent> = serde_json::from_value(c["components"].clone()).map_err(|e| e.to_string())?;
                let mut surface = Surface::new("ext", SurfaceOptions { catalog_id: ext.id.clone(), extensions: vec![ext.clone()], ..SurfaceOptions::default() });
                surface.set_components(components);
                paint(&mut surface, 400.0)?;
                ensure(surface.nodes().iter().any(|n| n.component == "Extension"), || "no Extension leaf".into())
            })
            .1;
        }
    }
    cases
}

fn theme_resolved_suite() -> Vec<Case> {
    let f = fixture("theme-resolved.json");
    f["themes"]
        .as_object()
        .unwrap()
        .iter()
        .map(|(id, expected)| case(id.clone(), || same(&to_value(&*builtin_theme(id).ok_or("not a built-in")?), expected)))
        .collect()
}

fn theme_recipes_suite() -> Vec<Case> {
    let f = fixture("theme-recipes.json");
    f["cases"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| {
            let component = c["component"].as_str().unwrap();
            let part = c["part"].as_str().unwrap();
            case(format!("{component}/{part} {}", c["props"]), || {
                let props = obj(&c["props"]);
                for (theme_id, by_mode) in c["visuals"].as_object().unwrap() {
                    let t = builtin_theme(theme_id).ok_or("not a built-in")?;
                    for (mode, by_state) in by_mode.as_object().unwrap() {
                        let m = Mode::parse(mode).ok_or("bad mode")?;
                        for (state, style) in by_state.as_object().unwrap() {
                            let states = if state == "default" { vec![] } else { vec![state.clone()] };
                            let got = Value::Object(resolve_recipe(&t, &RecipeQuery::new(component, part, props.clone(), states), m));
                            same(&got, style).map_err(|e| format!("{theme_id}/{mode}/{state}: {e}"))?;
                        }
                    }
                }
                Ok(())
            })
        })
        .collect()
}

fn theme_extends_suite() -> Vec<Case> {
    let refs = builtin_refs();
    let options = ThemeOptions::core(&refs);
    let f = fixture("theme-extends.json");
    f["cases"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| {
            case(c["name"].as_str().unwrap(), || {
                let t = load_theme(&c["theme"], &options).map_err(|e| e.to_string())?;
                ensure(t.chain == strings(c["expected"].get("chain")), || format!("chain {:?}", t.chain))?;
                for p in c["expected"]["probes"].as_array().unwrap() {
                    let q = RecipeQuery::new(p["component"].as_str().unwrap(), p["part"].as_str().unwrap(), obj(&p["props"]), strings(p.get("states")));
                    let got = Value::Object(resolve_recipe(&t, &q, Mode::parse(p["mode"].as_str().unwrap()).ok_or("bad mode")?));
                    same(&got, &p["style"])?;
                }
                Ok(())
            })
        })
        .collect()
}

fn theme_invalid_suite() -> Vec<Case> {
    let refs = builtin_refs();
    let options = ThemeOptions::core(&refs);
    let f = fixture("theme-invalid.json");
    f["cases"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| {
            case(c["name"].as_str().unwrap(), || match try_load_theme(&c["theme"], &options) {
                Ok(_) => Err("loaded".into()),
                Err(issues) => same(&to_value(&issues), &c["issues"]),
            })
        })
        .collect()
}

fn control_geometry_suite() -> Vec<Case> {
    let f = fixture("control-geometry.json");
    let mut out = Vec::new();
    for (theme_id, by_component) in f["themes"].as_object().unwrap() {
        let t = builtin_theme(theme_id);
        for (component, entry) in by_component.as_object().unwrap() {
            for (name, c) in entry["cases"].as_object().unwrap() {
                out.push(case(format!("{theme_id} {component} {name}"), || {
                    let t = t.clone().ok_or("not a built-in")?;
                    let got = control_geometry(&t, component, &obj(&c["props"]), &[]);
                    let got = Value::Object(got.iter().map(|(k, v)| (k.clone(), json::number(*v))).collect());
                    same(&got, &c["geometry"])
                }));
            }
        }
    }
    out
}

fn layout_suite() -> Vec<Case> {
    let fx = fixture("layout-geometry.json");
    let sizes: HashMap<String, (f32, f32)> =
        fx["measures"].as_object().unwrap().iter().map(|(id, m)| (id.clone(), (m["w"].as_f64().unwrap() as f32, m["h"].as_f64().unwrap() as f32))).collect();
    fx["cases"]
        .as_object()
        .unwrap()
        .iter()
        .map(|(name, c)| {
            case(name.clone(), || {
                let mut tree: NestedNode = serde_json::from_value(fx["surface"].clone()).map_err(|e| e.to_string())?;
                tree.style.get_or_insert_with(Default::default).insert("direction".into(), c["direction"].clone());
                let mut surface = Surface::new("geometry", SurfaceOptions { theme: None, ..SurfaceOptions::default() });
                let outcome = surface.set_nested(tree);
                ensure(outcome.issues.is_empty(), || format!("issues {:?}", outcome.issues))?;
                surface.set_viewport(c["width"].as_f64().unwrap() as f32, 0.0, None);
                let out = surface.layout(&mut FixedMeasure::with_sizes(sizes.clone()));
                let nodes = surface.nodes();
                let expected = c["frames"].as_array().unwrap();
                ensure(out.frames.len() == expected.len(), || format!("{} frames, expected {}", out.frames.len(), expected.len()))?;
                for (i, want) in expected.iter().enumerate() {
                    let got = out.frames[i];
                    let node = &nodes[got.index as usize];
                    ensure(node.id == want["id"].as_str().unwrap(), || format!("order at {i}: {}", node.id))?;
                    for (k, v) in [("x", got.x), ("y", got.y), ("w", got.w), ("h", got.h)] {
                        let e = want[k].as_f64().unwrap() as f32;
                        ensure((v - e).abs() <= 0.001, || format!("{}.{k} = {v}, expected {e}", node.id))?;
                    }
                }
                Ok(())
            })
        })
        .collect()
}

fn overlay_suite() -> Vec<Case> {
    let fx = fixture("overlay-geometry.json");
    let tolerance = fx["tolerancePx"].as_f64().unwrap_or(0.0);
    fx["cases"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| {
            case(c["name"].as_str().unwrap(), || {
                let rect = |v: &Value| Rect { x: v["x"].as_f64().unwrap(), y: v["y"].as_f64().unwrap(), width: v["width"].as_f64().unwrap(), height: v["height"].as_f64().unwrap() };
                let size = |v: &Value| Size { width: v["width"].as_f64().unwrap(), height: v["height"].as_f64().unwrap() };
                let side = OverlaySide::parse(c["side"].as_str().unwrap()).ok_or("bad side")?;
                let got = place_overlay(&rect(&c["anchor"]), &size(&c["size"]), &size(&c["viewport"]), &PlaceOptions::side(side));
                let e = &c["expected"];
                ensure((got.x - e["x"].as_f64().unwrap()).abs() <= tolerance, || format!("x {}", got.x))?;
                ensure((got.y - e["y"].as_f64().unwrap()).abs() <= tolerance, || format!("y {}", got.y))?;
                ensure(got.side.as_str() == e["side"].as_str().unwrap(), || format!("side {}", got.side.as_str()))?;
                ensure(got.flipped == e["flipped"].as_bool().unwrap(), || format!("flipped {}", got.flipped))
            })
        })
        .collect()
}

fn replay_suite() -> Vec<Case> {
    let sink: NestedNode = serde_json::from_value(fixture("kitchen-sink.json")).unwrap();
    let mut expected = fixture("kitchen-sink.expanded.json");
    expected.as_object_mut().unwrap().remove("$comment");
    let mut out = Vec::new();
    for id in BUILTIN_THEME_IDS {
        for mode in MODES {
            out.push(case(format!("{id}/{}", mode.as_str()), || {
                let reduced = reduce_nested(&sink, &ReduceOptions::new(CORE_CATALOG_ID));
                same(&to_value(&reduced), &expected)?;
                let mut surface = Surface::new("ks", SurfaceOptions { theme: Some(builtin_theme(id).ok_or("not a built-in")?), mode, ..SurfaceOptions::default() });
                let outcome = surface.set_nested(sink.clone());
                ensure(outcome.issues.is_empty(), || format!("issues {:?}", outcome.issues))?;
                surface
                    .apply(&json!({"version": "v0.9", "updateDataModel": {"surfaceId": "ks", "value": {"posts": [{"title": "One"}, {"title": "Two"}], "ui": {"confirmOpen": false}}}}))
                    .map_err(|e| e.to_string())?;
                for width in [390.0, 900.0] {
                    paint(&mut surface, width)?;
                    let frames = surface.layout(&mut FixedMeasure::default()).frames;
                    ensure(frames.iter().any(|f| f.w > 0.0 && f.h > 0.0), || format!("{width}: every frame empty"))?;
                }
                Ok(())
            }));
        }
    }
    out
}

fn suite(id: &str) -> Vec<Case> {
    match id {
        "catalog" => catalog_suite(),
        "macros" => macros_suite(),
        "basic-map" => reduce_cases("catalog-basic-map.json", None, |_| A2UI_BASIC_CATALOG_ID.to_string()),
        "extension" => extension_suite(),
        "theme-resolved" => theme_resolved_suite(),
        "theme-recipes" => theme_recipes_suite(),
        "theme-extends" => theme_extends_suite(),
        "theme-invalid" => theme_invalid_suite(),
        "control-geometry" => control_geometry_suite(),
        "layout" => layout_suite(),
        "overlay" => overlay_suite(),
        "replay" => replay_suite(),
        "host-transport" => support::transport_cases(),
        "host-policy" => support::policy_cases(),
        "host-router" => support::router_cases(),
        other => panic!("the runner does not know the suite {other}: add it"),
    }
}

fn report_path() -> std::path::PathBuf {
    if let Ok(p) = std::env::var("EXPONENTIAL_UI_CONFORMANCE_REPORT") {
        return p.into();
    }
    let target = std::env::var("CARGO_TARGET_DIR").unwrap_or_else(|_| concat!(env!("CARGO_MANIFEST_DIR"), "/../../target").to_string());
    std::path::Path::new(&target).join("exponential-ui-conformance").join("exponential-ui.json")
}

#[test]
fn the_core_passes_every_conformance_suite_and_writes_its_report() {
    // A failing case's panic message must not drown the summary.
    let hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    let manifest = read("conformance/manifest.json");
    let mut suites = BTreeMap::new();
    let mut problems = Vec::new();
    for s in manifest["suites"].as_array().unwrap() {
        let id = s["id"].as_str().unwrap();
        let cases = suite(id);
        let failed: Vec<String> = cases.iter().filter_map(|(name, r)| r.as_ref().err().map(|e| format!("{name}: {e}"))).collect();
        let want = s["cases"].as_u64().unwrap() as usize;
        if cases.len() != want {
            problems.push(format!("{id}: ran {} of {want} cases", cases.len()));
        }
        if !failed.is_empty() {
            problems.push(format!("{id}: {} failed\n  {}", failed.len(), failed.join("\n  ")));
        }
        suites.insert(id.to_string(), json!({"cases": cases.len(), "passed": cases.len() - failed.len(), "failed": failed}));
    }
    std::panic::set_hook(hook);
    let report = json!({
        "renderer": "exponential-ui",
        "platform": "rust",
        "version": env!("CARGO_PKG_VERSION"),
        "conformanceVersion": manifest["version"],
        "suites": suites,
    });
    let path = report_path();
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, serde_json::to_string_pretty(&report).unwrap() + "\n").unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    eprintln!("conformance report: {}", path.display());
    assert!(problems.is_empty(), "not conformant:\n{}", problems.join("\n"));
}
