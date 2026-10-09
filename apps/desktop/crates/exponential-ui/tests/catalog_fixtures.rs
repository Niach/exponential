//! The fixtures ARE the contract: the Rust core replays every case of
//! `packages/exponential-ui/fixtures` with the test names of
//! `packages/exponential-ui/src/fixtures.test.ts`.

use std::collections::HashSet;
use std::sync::Arc;

use exponential_ui::catalog::{component_names, CORE};
use exponential_ui::extension::define_extension;
use exponential_ui::json;
use exponential_ui::macros::expand_macros;
use exponential_ui::reducer::{preorder, reduce_nested, reduce_surface, ReduceOptions, ReduceResult};
use exponential_ui::validate::validate_props;
use exponential_ui::{
    CatalogView, ExtensionDef, FlatComponent, NestedNode, UiNode, A2UI_BASIC_CATALOG_ID, CORE_CATALOG_ID, UNKNOWN_COMPONENT,
};
use serde::Deserialize;
use serde_json::{json, Value};

fn fixture(name: &str) -> Value {
    let path = format!("{}/../../../../packages/exponential-ui/fixtures/{name}", env!("CARGO_MANIFEST_DIR"));
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path}: {e}"));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("{path}: {e}"))
}

fn cases<T: for<'de> Deserialize<'de>>(file: &Value) -> Vec<T> {
    serde_json::from_value(file["cases"].clone()).expect("cases")
}

/// The first path where two documents differ (canonical form), or `None`.
fn first_diff(a: &Value, b: &Value, at: &str) -> Option<String> {
    match (a, b) {
        (Value::Object(x), Value::Object(y)) => {
            let keys: std::collections::BTreeSet<&String> = x.keys().chain(y.keys()).collect();
            for k in keys {
                match (x.get(k), y.get(k)) {
                    (Some(xv), Some(yv)) => {
                        if let Some(d) = first_diff(xv, yv, &format!("{at}.{k}")) {
                            return Some(d);
                        }
                    }
                    (xv, yv) => return Some(format!("{at}.{k}: got {} / expected {}", show(xv), show(yv))),
                }
            }
            None
        }
        (Value::Array(x), Value::Array(y)) => {
            for (i, (xv, yv)) in x.iter().zip(y).enumerate() {
                if let Some(d) = first_diff(xv, yv, &format!("{at}[{i}]")) {
                    return Some(d);
                }
            }
            (x.len() != y.len()).then(|| format!("{at}: got {} items / expected {}", x.len(), y.len()))
        }
        _ => (!json::equal(a, b)).then(|| format!("{at}: got {a} / expected {b}")),
    }
}

fn show(v: Option<&Value>) -> String {
    v.map(|v| v.to_string()).unwrap_or_else(|| "(absent)".into())
}

fn assert_same(case: &str, got: &impl serde::Serialize, expected: &Value) {
    let got = serde_json::to_value(got).unwrap();
    if !json::equal(&got, expected) {
        let diff = first_diff(&json::canonical(&got), &json::canonical(expected), "$").unwrap_or_default();
        panic!("{case}: {diff}");
    }
}

fn walk(node: &UiNode, visit: &mut dyn FnMut(&UiNode)) {
    node.walk(&mut |n| visit(n));
}

fn assert_all_native(root: &UiNode, case: &str) {
    walk(root, &mut |n| {
        let kind = CORE.components.get(&n.component).map(|d| d.kind.as_str());
        assert_eq!(kind, Some("native"), "{case}: {}", n.id);
    });
}

fn core() -> ReduceOptions {
    ReduceOptions::new(CORE_CATALOG_ID)
}

// ---------------------------------------------------------------------------
// catalog-components.json
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct ComponentCase {
    name: String,
    node: NestedNode,
}

#[test]
fn catalog_components_covers_every_visible_component_x_every_enum_value_x_both_booleans() {
    let cases: Vec<ComponentCase> = cases(&fixture("catalog-components.json"));
    assert_eq!(cases.len(), 466);
    let mut seen: Vec<String> = cases.iter().map(|c| c.node.component.clone()).collect::<HashSet<_>>().into_iter().collect();
    seen.sort();
    let deprecated: Vec<String> = CORE.components.iter().filter(|(_, d)| d.deprecated.is_some()).map(|(n, _)| n.clone()).collect();
    let mut names = component_names(false);
    names.extend(deprecated.iter().cloned());
    names.sort();
    assert_eq!(seen, names);
    // Round 3: a deprecated alias keeps its example case only.
    for name in &deprecated {
        let named: Vec<&str> = cases.iter().filter(|c| &c.node.component == name).map(|c| c.name.as_str()).collect();
        assert_eq!(named, vec![format!("{name}/example")], "{name}");
    }
    let has = |name: &str| cases.iter().any(|c| c.name == name);
    for (name, def) in &CORE.components {
        if !def.is_offered() {
            continue;
        }
        for (prop, schema) in &def.props {
            if schema.type_ == "enum" {
                let values: Vec<Value> = schema.values.clone().unwrap_or_else(|| {
                    CORE.enums[schema.enum_.as_ref().unwrap()].iter().map(|s| Value::String(s.clone())).collect()
                });
                for value in values {
                    let case = format!("{name}/{prop}={}", json::to_js_string(&value));
                    assert!(has(&case), "{case}");
                }
            }
            if schema.type_ == "boolean" {
                for value in [true, false] {
                    let case = format!("{name}/{prop}={value}");
                    assert!(has(&case), "{case}");
                }
            }
        }
    }
}

#[test]
fn catalog_components_every_case_validates_and_reduces_without_issues() {
    let cases: Vec<ComponentCase> = cases(&fixture("catalog-components.json"));
    let view = CatalogView::core();
    for c in &cases {
        let def = &CORE.components[&c.node.component];
        let props = c.node.props.clone().unwrap_or_default();
        assert_eq!(validate_props(def, &props, "props", &view), vec![], "{}", c.name);
        let result = reduce_nested(&c.node, &core());
        assert_eq!(result.issues, vec![], "{}", c.name);
        assert_all_native(&result.root, &c.name);
    }
}

// ---------------------------------------------------------------------------
// catalog-macros.json
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct MacroCase {
    name: String,
    input: NestedNode,
    expected: Value,
}

fn macro_cases() -> Vec<MacroCase> {
    cases(&fixture("catalog-macros.json"))
}

#[test]
fn catalog_macros_every_macro_case_expands_byte_for_byte() {
    let cases = macro_cases();
    assert!(!cases.is_empty());
    for c in &cases {
        let ReduceResult { root, issues, .. } = reduce_nested(&c.input, &core());
        assert_eq!(issues, vec![], "{}", c.name);
        assert_same(&c.name, &root, &c.expected);
    }
}

#[test]
fn catalog_macros_expansion_is_pure_and_reaches_natives_only_ids_stay_unique_recipes_name_the_macro() {
    let view = CatalogView::core();
    for c in &macro_cases() {
        let before = serde_json::to_value(&c.input).unwrap();
        let mut options = core();
        options.expand = false;
        let first = reduce_nested(&c.input, &options).root;
        let expanded = expand_macros(&first, &view).unwrap();
        assert_eq!(serde_json::to_value(&c.input).unwrap(), before, "{}", c.name);
        let again = expand_macros(&expanded, &view).unwrap();
        assert_same(&format!("{} idempotent", c.name), &again, &serde_json::to_value(&expanded).unwrap());
        let ids = preorder(&expanded);
        assert_eq!(ids.iter().collect::<HashSet<_>>().len(), ids.len(), "{}", c.name);
        assert_eq!(expanded.id, c.input.id);
        let recipe = expanded.recipe.as_ref().expect("recipe");
        // Round 3: a deprecated alias over a MACRO is transparent (the
        // replacement's root recipe wins); over a native the alias tags the root.
        let def = &CORE.components[&c.input.component];
        let replacement = match &def.deprecated {
            Some(r) if CORE.components[r].is_macro() => r.as_str(),
            _ => c.input.component.as_str(),
        };
        assert_eq!(recipe.macro_, replacement, "{}", c.name);
        assert_eq!(recipe.part, "root");
        assert_all_native(&expanded, &c.name);
    }
}

fn with_on(node: &NestedNode, on: Value) -> NestedNode {
    let mut out = node.clone();
    out.on = Some(serde_json::from_value(on).unwrap());
    out
}

#[test]
fn catalog_macros_every_macros_events_route_to_a_part_or_stay_on_the_root() {
    let cases = macro_cases();
    let find = |name: &str| cases.iter().find(|c| c.name == name).unwrap_or_else(|| panic!("{name}"));

    let empty_state = find("EmptyState/example");
    let mut with_handler = with_on(&empty_state.input, json!({"press": {"event": {"name": "retry"}}}));
    with_handler.props.get_or_insert_with(Default::default).insert("actionLabel".into(), json!("Retry"));
    let root = reduce_nested(&with_handler, &core()).root;
    assert!(root.on.is_none());
    let action = root.children.iter().find(|n| n.id == "empty-state.action").expect("empty-state.action");
    assert_eq!(action.on.as_ref().unwrap()["press"], json!({"event": {"name": "retry"}}));

    let pagination = find("Pagination/example");
    let paged = reduce_nested(&with_on(&pagination.input, json!({"change": {"event": {"name": "page", "context": {"list": "issues"}}}})), &core()).root;
    assert_eq!(paged.children[0].on.as_ref().unwrap()["press"], json!({"event": {"name": "page", "context": {"list": "issues", "page": 1}}}));
    assert_eq!(paged.children[2].on.as_ref().unwrap()["press"], json!({"event": {"name": "page", "context": {"list": "issues", "page": 3}}}));

    let pill = find("Chip/pressable=true");
    let pressed = reduce_nested(&with_on(&pill.input, json!({"press": {"event": {"name": "pick"}}})), &core()).root;
    assert_eq!(pressed.on.as_ref().unwrap()["press"], json!({"event": {"name": "pick"}}));
}

// ---------------------------------------------------------------------------
// catalog-basic-map.json
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct BasicCase {
    name: String,
    components: Vec<FlatComponent>,
    expected: Value,
}

fn basic_cases() -> Vec<BasicCase> {
    cases(&fixture("catalog-basic-map.json"))
}

#[test]
fn catalog_basic_map_every_basic_case_reduces_byte_for_byte() {
    let cases = basic_cases();
    assert!(cases.len() > 10);
    for c in &cases {
        let result = reduce_surface(&c.components, &ReduceOptions::new(A2UI_BASIC_CATALOG_ID));
        assert_same(&c.name, &result, &c.expected);
    }
}

#[test]
fn catalog_basic_map_the_unmapped_component_is_the_placeholder_never_an_error() {
    let cases = basic_cases();
    let c = cases.iter().find(|x| x.name.starts_with("An unmapped component")).unwrap();
    let result = reduce_surface(&c.components, &ReduceOptions::new(A2UI_BASIC_CATALOG_ID));
    let placeholder = result.root.children.iter().find(|n| n.component == UNKNOWN_COMPONENT).expect("placeholder");
    assert_eq!(Value::Object(placeholder.props.clone()), json!({"component": "Gauge", "catalogId": A2UI_BASIC_CATALOG_ID}));
    assert_eq!(c.expected["issues"].as_array().unwrap().len(), 1);
    assert_eq!(result.issues.len(), 1);
}

#[test]
fn catalog_basic_map_every_basic_component_appears_in_the_cases() {
    let seen: HashSet<String> = basic_cases().iter().flat_map(|c| c.components.iter().map(|x| x.component.clone())).collect();
    for name in [
        "Text", "Image", "Icon", "Video", "AudioPlayer", "Row", "Column", "List", "Card", "Tabs", "Modal", "Divider", "Button", "TextField",
        "CheckBox", "ChoicePicker", "Slider", "DateTimeInput",
    ] {
        assert!(seen.contains(name), "{name}");
    }
}

// ---------------------------------------------------------------------------
// catalog-extension.json
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct ExtensionCase {
    name: String,
    #[serde(rename = "catalogId")]
    catalog_id: String,
    components: Vec<FlatComponent>,
    expected: Value,
}

fn extension_fixture() -> (Arc<CatalogView>, Vec<ExtensionCase>) {
    let file = fixture("catalog-extension.json");
    let def: ExtensionDef = serde_json::from_value(file["extension"].clone()).unwrap();
    let extension = define_extension(def).expect("the example extension is valid");
    (CatalogView::with(&[extension]), cases(&file))
}

#[test]
fn catalog_extension_every_extension_case_reduces_byte_for_byte() {
    let (view, cases) = extension_fixture();
    for c in &cases {
        let result = reduce_surface(&c.components, &ReduceOptions::new(&c.catalog_id).with_view(view.clone()));
        assert_same(&c.name, &result, &c.expected);
    }
}

#[test]
fn catalog_extension_an_extension_macro_expands_through_core_macros_to_natives_plus_its_own_natives() {
    let (view, cases) = extension_fixture();
    let c = &cases[0];
    let root = reduce_surface(&c.components, &ReduceOptions::new(&c.catalog_id).with_view(view)).root;
    let mut kinds = HashSet::new();
    walk(&root, &mut |n| {
        kinds.insert(n.component.clone());
    });
    assert!(!kinds.contains("StatCard"));
    assert!(kinds.contains("TrendLine"));
    assert!(kinds.contains("Box"));
}

// ---------------------------------------------------------------------------
// kitchen-sink.json
// ---------------------------------------------------------------------------

fn kitchen_sink() -> NestedNode {
    serde_json::from_value(fixture("kitchen-sink.json")).expect("kitchen-sink.json")
}

#[test]
fn kitchen_sink_uses_every_visible_component_at_least_once() {
    fn visit(n: &NestedNode, seen: &mut HashSet<String>) {
        seen.insert(n.component.clone());
        for slot in n.slots.iter().flat_map(|s| s.values()) {
            visit(slot, seen);
        }
        for child in n.children.iter().flatten() {
            visit(child, seen);
        }
    }
    let mut seen = HashSet::new();
    visit(&kitchen_sink(), &mut seen);
    for name in component_names(false) {
        assert!(seen.contains(&name), "{name}");
    }
}

#[test]
fn kitchen_sink_reduces_to_the_committed_expansion_with_no_issues_and_unique_ids() {
    let result = reduce_nested(&kitchen_sink(), &core());
    assert_eq!(result.issues, vec![]);
    let mut expected = fixture("kitchen-sink.expanded.json");
    expected.as_object_mut().unwrap().remove("$comment");
    assert_same("kitchen sink", &result, &expected);
    let ids = preorder(&result.root);
    assert_eq!(ids.iter().collect::<HashSet<_>>().len(), ids.len());
    assert_all_native(&result.root, "kitchen sink");
}
