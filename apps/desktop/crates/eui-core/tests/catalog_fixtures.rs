//! The fixtures ARE the contract: the Rust core replays every case of
//! `packages/exponential-ui/fixtures` with the test names of
//! `packages/exponential-ui/src/fixtures.test.ts`.

use std::collections::HashSet;
use std::sync::Arc;

use eui_core::catalog::{component_names, CORE};
use eui_core::extension::define_extension;
use eui_core::json;
use eui_core::macros::expand_macros;
use eui_core::reducer::{preorder, reduce_nested, reduce_surface, ReduceOptions, ReduceResult};
use eui_core::validate::validate_props;
use eui_core::{
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
    assert_eq!(cases.len(), 751, "#EXP-1629: + Button/size=icon-xs, Button/tone=default|muted; #EXP-1629: + Select/triggerVariant=button+icon; #EXP-1778: + Select/triggerVariant=button, Text/truncate=end|middle; + Popover/matchAnchorWidth ×2; #EXP-1762: + Composer/layout ×2; #EXP-1775: + ColorPicker/inline=true|false, ColorPicker/child; #EXP-1764: + Select/triggerIcon ×2; #EXP-1760: + IconPicker ×5 (example, allowNone, open), ColorPicker ×3 (example, open); #EXP-1765: + Select/triggerVariant=pill, Select/loading ×2; #EDE-65: + Embed/example, Composer/menuKeys ×2; #EXP-1629: + Input/variant ×5, Input/size ×2, Textarea/variant ×3, KbdGroup, Avatar/size=xl, Icon/size=xl, + KeyRecorder ×3 (example, recording=true|false), Select/triggerVariant ×2, align ×3; FEED-202: + Table/totals ×2, RecordTable/search + totals ×2; FEED-200: + Tabs/swipeable ×2; #EXP-1736: + MessageBubble/href, pressable ×2; #FEED-201: + Card/pressable, StatTile/pressable ×2; FEED-205: + Chart/valueFormat=currency, Chart/rows+valueFormat=compact; + Window; round 8: + Canvas ×6, Checkbox/indeterminate ×3, Progress valueText-only + detail, Stepper/collapse ×3 + ×2; VAPP-276: + Popover/align, Menu/side + align; #VAPP-285: + Tree; #VAPP-284: + Row/open=true|false; VAPP-277: + Sidebar/rail; VAPP-280: + SettingsRow; VAPP-279: + Tabs/variant ×2; VAPP-282: + DatePicker/inline; VAPP-286: + Slider/showValue; VAPP-217: + RecordForm, RecordTable; EXP-1674: + Button/href, Row/href; VAPP-232: + Calendar, Timeline; EXP-1280: + ChoiceTiles; N-12: + DockCard; VAPP-126: + the six layout macros; NEW-77: + VoiceWave; VAPP-162: + the round-5 five; EXP-1495: + Outline, Mosaic, Carousel counter; EXP-1548: + Table/wide; EXP-1586: + ZoomableImage; EXP-1604: + Motion; + Switch/size; VAPP-286: + Slider/showValue");
    let mut seen: Vec<String> = cases.iter().map(|c| c.node.component.clone()).collect::<HashSet<_>>().into_iter().collect();
    seen.sort();
    let mut names = component_names(false);
    names.sort();
    assert_eq!(seen, names);
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
    // The state cases: a prop that only shows in a state (EXP-1259).
    assert!(has("Button/loading=true+pendingLabel"));
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
        assert_eq!(recipe.macro_, c.input.component, "{}", c.name);
        // #VAPP-284: a foldable Row (children) takes its `group` alternative root.
        assert_eq!(recipe.part, if c.name.starts_with("Row/fold:") { "group" } else if c.name == "ColorPicker/inline=true" { "grid" } else { "root" }, "{}", c.name);
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
    let action = root.children.iter().find(|n| n.id == "empty_state.action").expect("empty_state.action");
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
    // VAPP-232: Calendar and Timeline read a records calendar view (a data
    // model the kitchen sink has not); they join it with the next web baseline
    // rewrite (a browser run). template-items.json `macro-template-*` locks them.
    // VAPP-217: a macro the HOST fills (props marked `host`) paints only once
    // filled; its catalog specimen shows it and it expands into natives the
    // sink already has. #EXP-1629: KeyRecorder joins with that rewrite too
    // (catalog-macros.json + template-items.json lock it). #EDE-65: Embed
    // (a nested surface) is locked by embed.json, not the sink.
    // IconPicker and ColorPicker (Popover, Button, Box, Icon) join with that
    // rewrite; catalog-macros.json locks them.
    let catalog = fixture("../catalog/core.catalog.json");
    let host_filled = |name: &str| catalog["components"][name]["props"].as_object().is_some_and(|p| p.values().any(|s| s["host"] == json!(true)));
    for name in component_names(false) {
        assert!(seen.contains(&name) || name == "Calendar" || name == "Timeline" || name == "KeyRecorder" || name == "Embed" || name == "IconPicker" || name == "ColorPicker" || host_filled(&name), "{name}");
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
