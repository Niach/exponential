//! VAPP-92: the theme fixtures ARE the contract — the Rust core replays every
//! case of `packages/exponential-ui/fixtures/theme-*.json` and
//! `control-geometry.json` with the test names of `src/theme-fixtures.test.ts`.

use exponential_ui::geometry::{check_geometry, control_geometry, GEOMETRY_KEYS, verify_painter_geometry, GeometryCase, MeasuredBox, PainterOverride};
use exponential_ui::json;
use exponential_ui::theme::{load_theme, resolve_recipe, try_load_theme, Mode, RecipeQuery, ResolvedTheme, ThemeOptions, ThemeRef};
use exponential_ui::themes::{builtin_refs, builtin_theme};
use exponential_ui::Props;
use indexmap::IndexMap;
use serde_json::Value;

const PKG: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../../../packages/exponential-ui");

fn read(rel: &str) -> Value {
    let path = format!("{PKG}/{rel}");
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path}: {e}"));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("{path}: {e}"))
}

fn fixture(name: &str) -> Value {
    read(&format!("fixtures/{name}"))
}

/// The first key path where two documents differ (canonical numbers).
fn first_diff(a: &Value, b: &Value, path: &str) -> Option<String> {
    match (a, b) {
        (Value::Object(x), Value::Object(y)) => {
            for k in x.keys().chain(y.keys()) {
                let at = format!("{path}.{k}");
                match (x.get(k), y.get(k)) {
                    (Some(p), Some(q)) => {
                        if let Some(d) = first_diff(p, q, &at) {
                            return Some(d);
                        }
                    }
                    (p, q) => return Some(format!("{at}: {p:?} vs {q:?}")),
                }
            }
            None
        }
        (Value::Array(x), Value::Array(y)) if x.len() == y.len() => {
            x.iter().zip(y).enumerate().find_map(|(i, (p, q))| first_diff(p, q, &format!("{path}[{i}]")))
        }
        _ if json::equal(a, b) => None,
        _ => Some(format!("{path}: {a} vs {b}")),
    }
}

fn assert_same(actual: &Value, expected: &Value, label: &str) {
    if !json::equal(actual, expected) {
        panic!("{label}: first difference at {}", first_diff(actual, expected, "").unwrap_or_default());
    }
}

fn obj(v: &Value) -> Props {
    v.as_object().cloned().unwrap_or_default()
}

fn strings(v: Option<&Value>) -> Vec<String> {
    v.and_then(Value::as_array).map(|a| a.iter().filter_map(|s| s.as_str().map(str::to_string)).collect()).unwrap_or_default()
}

fn theme(id: &str) -> std::sync::Arc<ResolvedTheme> {
    builtin_theme(id).unwrap_or_else(|| panic!("unknown built-in {id}"))
}

// ---------------------------------------------------------------------------
// theme-resolved.json
// ---------------------------------------------------------------------------

#[test]
fn every_built_in_resolves_to_the_recorded_theme_byte_for_byte() {
    let f = fixture("theme-resolved.json");
    let themes = f["themes"].as_object().unwrap();
    let mut ids: Vec<&str> = themes.keys().map(String::as_str).collect();
    ids.sort();
    assert_eq!(ids, ["exponential", "neutral", "playful"]);
    for (id, expected) in themes {
        assert_same(&serde_json::to_value(&*theme(id)).unwrap(), expected, id);
    }
}

#[test]
fn the_built_in_sources_load_to_the_recorded_themes() {
    // Proves the LOADER, not only the embedding: the three source files
    // through `load_theme` over a registry of sources.
    let f = fixture("theme-resolved.json");
    let ids = ["neutral", "exponential", "playful"];
    let registry: Vec<ThemeRef> = ids.iter().map(|id| ThemeRef::Source(read(&format!("themes/{id}.theme.json")))).collect();
    let options = ThemeOptions::core(&registry);
    for (i, id) in ids.iter().enumerate() {
        let ThemeRef::Source(source) = &registry[i] else { unreachable!() };
        let loaded = load_theme(source, &options).unwrap_or_else(|e| panic!("{e}"));
        assert_same(&serde_json::to_value(&loaded).unwrap(), &f["themes"][id], id);
    }
}

// ---------------------------------------------------------------------------
// theme-recipes.json
// ---------------------------------------------------------------------------

fn recipe_cases() -> Vec<Value> {
    fixture("theme-recipes.json")["cases"].as_array().unwrap().clone()
}

#[test]
fn every_theme_component_part_props_resolves_to_the_recorded_visuals() {
    let cases = recipe_cases();
    assert!(cases.len() > 100);
    let mut checked = 0;
    for c in &cases {
        let component = c["component"].as_str().unwrap();
        let part = c["part"].as_str().unwrap();
        let props = obj(&c["props"]);
        for (theme_id, by_mode) in c["visuals"].as_object().unwrap() {
            let t = theme(theme_id);
            for (mode, by_state) in by_mode.as_object().unwrap() {
                let mode = Mode::parse(mode).unwrap();
                for (state, style) in by_state.as_object().unwrap() {
                    let states = if state == "default" { vec![] } else { vec![state.clone()] };
                    let q = RecipeQuery::new(component, part, props.clone(), states);
                    let got = Value::Object(resolve_recipe(&t, &q, mode));
                    assert_same(&got, style, &format!("{theme_id}/{}/{state} {component}/{part} {}", mode.as_str(), c["props"]));
                    checked += 1;
                }
            }
        }
    }
    assert!(checked > cases.len());
}

#[test]
fn the_three_themes_give_the_kitchen_sink_controls_different_looks() {
    let cases = recipe_cases();
    let button = cases
        .iter()
        .find(|c| c["component"] == "Button" && c["part"] == "root" && c["props"]["variant"] == "default" && c["props"]["size"] == "default")
        .unwrap();
    let mut looks: Vec<Value> = button["visuals"].as_object().unwrap().values().map(|m| json::canonical(&m["light"]["default"])).collect();
    looks.dedup();
    let mut unique: Vec<Value> = Vec::new();
    for l in looks {
        if !unique.contains(&l) {
            unique.push(l);
        }
    }
    assert_eq!(unique.len(), 3);
    let card = cases.iter().find(|c| c["component"] == "Card" && c["part"] == "root").unwrap();
    assert!(json::equal(&card["visuals"]["playful"]["light"]["default"]["borderWidth"], &Value::from(0)));
    assert!(json::equal(&card["visuals"]["neutral"]["light"]["default"]["borderWidth"], &Value::from(1)));
}

// ---------------------------------------------------------------------------
// theme-extends.json
// ---------------------------------------------------------------------------

#[test]
fn every_extends_case_loads_over_the_built_ins_and_answers_its_probes_byte_for_byte() {
    let refs = builtin_refs();
    let options = ThemeOptions::core(&refs);
    let f = fixture("theme-extends.json");
    for c in f["cases"].as_array().unwrap() {
        let name = c["name"].as_str().unwrap();
        let t = load_theme(&c["theme"], &options).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(t.chain, strings(c["expected"].get("chain")), "{name}");
        for p in c["expected"]["probes"].as_array().unwrap() {
            let component = p["component"].as_str().unwrap();
            let part = p["part"].as_str().unwrap();
            let q = RecipeQuery::new(component, part, obj(&p["props"]), strings(p.get("states")));
            let got = Value::Object(resolve_recipe(&t, &q, Mode::parse(p["mode"].as_str().unwrap()).unwrap()));
            assert_same(&got, &p["style"], &format!("{name}: {component}/{part}"));
        }
    }
}

// ---------------------------------------------------------------------------
// theme-invalid.json
// ---------------------------------------------------------------------------

#[test]
fn every_invalid_theme_raises_exactly_the_recorded_issues_never_a_throw() {
    let refs = builtin_refs();
    let options = ThemeOptions::core(&refs);
    let f = fixture("theme-invalid.json");
    for c in f["cases"].as_array().unwrap() {
        let name = c["name"].as_str().unwrap();
        let issues = match try_load_theme(&c["theme"], &options) {
            Ok(_) => panic!("{name}: loaded"),
            Err(issues) => issues,
        };
        assert!(!issues.is_empty(), "{name}");
        for i in &issues {
            assert!(!i.path.is_empty(), "{name}");
        }
        assert_same(&serde_json::to_value(&issues).unwrap(), &c["issues"], name);
    }
}

// ---------------------------------------------------------------------------
// control-geometry.json
// ---------------------------------------------------------------------------

fn geometry_value(g: &IndexMap<String, f64>) -> Value {
    Value::Object(g.iter().map(|(k, v)| (k.clone(), json::number(*v))).collect())
}

#[test]
fn every_controls_box_matches_the_recorded_geometry() {
    let f = fixture("control-geometry.json");
    let mut checked = 0;
    for (theme_id, by_component) in f["themes"].as_object().unwrap() {
        let t = theme(theme_id);
        for (component, entry) in by_component.as_object().unwrap() {
            for (name, c) in entry["cases"].as_object().unwrap() {
                let got = control_geometry(&t, component, &obj(&c["props"]), &[]);
                // The keys come out in GEOMETRY_KEYS order.
                let order: Vec<usize> = got.keys().map(|k| GEOMETRY_KEYS.iter().position(|g| g == k).unwrap()).collect();
                assert!(order.windows(2).all(|w| w[0] < w[1]), "{theme_id} {component} {name}");
                assert_same(&geometry_value(&got), &c["geometry"], &format!("{theme_id} {component} {name}"));
                checked += 1;
            }
        }
    }
    assert!(checked > 0);
}

struct ThemedSwitch;
impl PainterOverride for ThemedSwitch {
    fn component(&self) -> &str {
        "Switch"
    }
    fn measure(&self, theme: &ResolvedTheme, props: &Props, _states: &[String]) -> MeasuredBox {
        let g = control_geometry(theme, "Switch", props, &[]);
        MeasuredBox {
            width: g.get("width").copied(),
            height: g.get("height").copied(),
            border_radius: g.get("borderRadius").copied(),
            border_width: g.get("borderWidth").copied(),
            ..Default::default()
        }
    }
}

struct FixedSwitch;
impl PainterOverride for FixedSwitch {
    fn component(&self) -> &str {
        "Switch"
    }
    fn measure(&self, _: &ResolvedTheme, _: &Props, _: &[String]) -> MeasuredBox {
        MeasuredBox { width: Some(51.0), height: Some(31.0), ..Default::default() }
    }
}

#[test]
fn a_painter_override_for_switch_passes_the_geometry_fixture() {
    let f = fixture("control-geometry.json");
    let themes = f["themes"].as_object().unwrap();
    let cases: Vec<GeometryCase> = themes["neutral"]["Switch"]["cases"]
        .as_object()
        .unwrap()
        .values()
        .map(|c| GeometryCase { props: obj(&c["props"]), states: strings(c.get("states")) })
        .collect();
    assert!(!cases.is_empty());
    for theme_id in themes.keys() {
        assert_eq!(verify_painter_geometry(&ThemedSwitch, &theme(theme_id), &cases), [], "{theme_id}");
    }
    // One that ignores the theme fails loudly.
    let failures = verify_painter_geometry(&FixedSwitch, &theme("neutral"), &cases);
    assert_eq!(failures.len(), cases.len());
    let keys: Vec<&str> = failures[0].issues.iter().map(|i| i.key.as_str()).collect();
    assert_eq!(keys, ["width", "height", "borderRadius"]);
    let expected: IndexMap<String, f64> = [("height".to_string(), 20.0)].into_iter().collect();
    assert_eq!(check_geometry(&expected, &MeasuredBox { height: Some(20.4), ..Default::default() }, 0.5), []);
}
