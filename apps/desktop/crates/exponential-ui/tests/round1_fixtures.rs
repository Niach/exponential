//! Round 1 (docs/round-1-contract.md): the new contract fixtures the Rust
//! core replays byte for byte — `bind-time.json` (the bind pass + every
//! `set` press), `style-conditions.json` (`resolveConditions`) and
//! `code-tokens.json` (the CodeBlock tokenizer) — plus the theme settings
//! (density, contrast, system mode, breakpoint keys).

use exponential_ui::code::tokenize_code;
use exponential_ui::conditions::{default_breakpoints, resolve_conditions, ConditionContext};
use exponential_ui::data::{bind_tree, run_action, ResolveContext};
use exponential_ui::json;
use exponential_ui::reducer::{reduce_nested, ReduceOptions};
use exponential_ui::strings::DEFAULT_STRINGS;
use exponential_ui::theme::{apply_contrast, apply_density, resolve_condition_key, resolve_mode, resolve_style_values, Density, Mode, ModeSetting};
use exponential_ui::themes::builtin_theme;
use exponential_ui::{NestedNode, UiNode, CORE_CATALOG_ID};
use indexmap::IndexMap;
use serde_json::{json, Value};

const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../../../packages/exponential-ui/fixtures");

fn fixture(name: &str) -> Value {
    let path = format!("{FIXTURES}/{name}");
    serde_json::from_str(&std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path}: {e}"))).expect("json")
}

fn first_diff(a: &Value, b: &Value, at: &str) -> Option<String> {
    match (a, b) {
        (Value::Object(x), Value::Object(y)) => {
            for k in x.keys().chain(y.keys()) {
                match (x.get(k), y.get(k)) {
                    (Some(p), Some(q)) => {
                        if let Some(d) = first_diff(p, q, &format!("{at}.{k}")) {
                            return Some(d);
                        }
                    }
                    (p, q) => return Some(format!("{at}.{k}: got {p:?} / expected {q:?}")),
                }
            }
            None
        }
        (Value::Array(x), Value::Array(y)) if x.len() == y.len() => x.iter().zip(y).enumerate().find_map(|(i, (p, q))| first_diff(p, q, &format!("{at}[{i}]"))),
        _ if json::equal(a, b) => None,
        _ => Some(format!("{at}: got {a} / expected {b}")),
    }
}

fn assert_same(label: &str, got: &Value, expected: &Value) {
    if !json::equal(got, expected) {
        panic!("{label}: {}", first_diff(got, expected, "$").unwrap_or_default());
    }
}

fn find<'a>(node: &'a UiNode, id: &str) -> Option<&'a UiNode> {
    if node.id == id {
        return Some(node);
    }
    node.slots.iter().flat_map(|s| s.values()).chain(node.children.iter()).find_map(|c| find(c, id))
}

/// One bind-time case: reduce + expand (issues included), then per dataset
/// the bind pass, every press and (extra cases) the per-ROW slot cells.
fn replay_bind_case(c: &Value) -> usize {
    let name = c["name"].as_str().unwrap();
    let input: NestedNode = serde_json::from_value(c["input"].clone()).unwrap();
    let reduced = reduce_nested(&input, &ReduceOptions::new(CORE_CATALOG_ID));
    let expected_issues = c.get("issues").cloned().unwrap_or(json!([]));
    assert_same(&format!("{name} issues"), &serde_json::to_value(&reduced.issues).unwrap(), &expected_issues);
    assert_same(&format!("{name} expanded"), &serde_json::to_value(&reduced.root).unwrap(), &c["expanded"]);
    let expanded: UiNode = serde_json::from_value(c["expanded"].clone()).unwrap();
    let mut presses = 0;
    for d in c["datasets"].as_array().unwrap() {
        let data = &d["data"];
        let ctx = ResolveContext::new(data, "").with_strings(&DEFAULT_STRINGS);
        let bound = bind_tree(&expanded, &ctx);
        assert_same(&format!("{name} bound {data}"), &serde_json::to_value(&bound).unwrap(), &d["bound"]);
        for p in d["presses"].as_array().unwrap() {
            let id = p["id"].as_str().unwrap();
            let node = find(&expanded, id).unwrap_or_else(|| panic!("{name}: {id}"));
            let action = &node.on.as_ref().unwrap()["press"];
            let outcome = run_action(action, &ctx);
            assert_same(&format!("{name} press {id} {data}"), &serde_json::to_value(&outcome).unwrap(), &p["outcome"]);
            presses += 1;
        }
        for t in d.get("rowSlots").and_then(Value::as_array).into_iter().flatten() {
            let id = t["id"].as_str().unwrap();
            let bound = bound.as_ref().unwrap_or_else(|| panic!("{name}: bound tree"));
            let node = find(bound, id).unwrap_or_else(|| panic!("{name}: {id}"));
            let rows_prop = find(&expanded, id).unwrap().props.get("rows").cloned().unwrap_or(Value::Null);
            let rows = node.props.get("rows").and_then(Value::as_array).cloned().unwrap_or_default();
            for r in t["rows"].as_array().unwrap() {
                let index = r["index"].as_u64().unwrap() as usize;
                for (slot_name, expected) in r["slots"].as_object().unwrap() {
                    let slot = &node.slots.as_ref().unwrap()[slot_name];
                    let got = exponential_ui::data::bind_row_slot(slot, &rows_prop, &rows, index, &ctx);
                    assert_same(&format!("{name} row {index} {slot_name}"), &serde_json::to_value(&got).unwrap(), expected);
                }
            }
        }
    }
    presses
}

#[test]
fn bind_time_every_bound_macro_expands_binds_and_presses_like_the_reference() {
    let file = fixture("bind-time.json");
    let cases = file["cases"].as_array().unwrap();
    assert!(cases.len() > 20);
    let presses: usize = cases.iter().map(replay_bind_case).sum();
    assert!(presses > 10, "{presses} presses");
}

#[test]
fn bind_time_extra_cases_replay_issues_and_row_slots_included() {
    let file = fixture("bind-time.json");
    let extra = file["extra"].as_array().unwrap();
    let names: Vec<&str> = extra.iter().map(|c| c["name"].as_str().unwrap()).collect();
    assert_eq!(names, ["Table/slot-cell:bound-rows", "Table/slot-cell:literal-rows", "Text/accessibility:bound-label", "TabBar/set:author-function", "Pagination/page:string-data"]);
    for c in extra {
        replay_bind_case(c);
    }
    let rows = &extra[0]["datasets"][0]["rowSlots"][0]["rows"];
    let labels: Vec<Value> = rows.as_array().unwrap().iter().map(|r| {
        let slot = &r["slots"]["status"];
        if slot.is_null() { Value::Null } else { serde_json::from_value::<UiNode>(slot.clone()).ok().and_then(|n| find(&n, "cell.label").map(|l| l.props["text"].clone())).unwrap_or(Value::Null) }
    }).collect();
    assert_eq!(labels, vec![json!("ok"), Value::Null, json!(0)]);
}

#[test]
fn bind_time_the_audit_bugs_progress_pagination_collapsible() {
    // Progress `value: {path}` used to draw 0%; Pagination printed
    // `[object Object] / 5`; a bound Collapsible was always open.
    let reduce = |n: Value| reduce_nested(&serde_json::from_value(n).unwrap(), &ReduceOptions::new(CORE_CATALOG_ID)).root;
    let data = json!({"v": 30, "page": 2, "open": false});
    let ctx = ResolveContext::new(&data, "");
    let progress = bind_tree(&reduce(json!({"id": "p", "component": "Progress", "props": {"value": {"path": "/v"}, "max": 60}})), &ctx).unwrap();
    let text = serde_json::to_string(&progress).unwrap();
    assert!(text.contains("\"50%\""), "{text}");
    let pagination = bind_tree(&reduce(json!({"id": "pg", "component": "Pagination", "props": {"page": {"path": "/page"}, "totalPages": 5}})), &ctx).unwrap();
    assert_eq!(find(&pagination, "pg.label").unwrap().props["text"], json!("2 / 5"));
    let collapsible = reduce(json!({"id": "c", "component": "Collapsible", "props": {"title": "More", "open": {"path": "/open"}}, "children": [{"id": "x", "component": "Text", "props": {"text": "body"}}]}));
    assert!(find(&bind_tree(&collapsible, &ctx).unwrap(), "c.body").is_none(), "closed");
    let trigger = find(&collapsible, "c.trigger").unwrap();
    let outcome = run_action(&trigger.on.as_ref().unwrap()["press"], &ctx);
    assert_eq!(outcome.data["open"], json!(true));
    let opened = ResolveContext::new(&outcome.data, "");
    assert!(find(&bind_tree(&collapsible, &opened).unwrap(), "c.body").is_some(), "open after the press");
}

#[test]
fn style_conditions_every_case_flattens_like_resolve_conditions() {
    let file = fixture("style-conditions.json");
    let defaults: IndexMap<String, f32> = serde_json::from_value(file["breakpoints"].clone()).unwrap();
    assert_eq!(defaults, default_breakpoints());
    for c in file["cases"].as_array().unwrap() {
        let name = c["name"].as_str().unwrap();
        let style = c["style"].as_object().unwrap();
        for (ctx, expected) in c["contexts"].as_array().unwrap().iter().zip(c["expected"].as_array().unwrap()) {
            let mut context: ConditionContext = serde_json::from_value(ctx.clone()).unwrap();
            if ctx.get("breakpoints").is_none() {
                context.breakpoints = defaults.clone();
            }
            let got = resolve_conditions(style, &context);
            assert_same(&format!("{name} / {}", ctx["label"]), &Value::Object(got), expected);
        }
    }
}

#[test]
fn code_tokens_every_case_tokenizes_like_the_reference() {
    let file = fixture("code-tokens.json");
    let cases = file["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 17);
    let mut languages = std::collections::HashSet::new();
    for c in cases {
        let name = c["name"].as_str().unwrap();
        let language = c["language"].as_str().unwrap();
        languages.insert(language.to_string());
        let got = tokenize_code(c["code"].as_str().unwrap(), language);
        assert_same(name, &serde_json::to_value(&got).unwrap(), &c["expected"]);
    }
    for name in exponential_ui::generated::catalog::CODE_LANGUAGE_NAMES {
        assert!(languages.contains(*name), "{name} has a case");
    }
}

#[test]
fn theme_settings_density_contrast_system_mode_and_breakpoint_keys() {
    let neutral = builtin_theme("neutral").unwrap();
    assert_eq!(resolve_mode(ModeSetting::System, true), Mode::Dark);
    assert_eq!(resolve_mode(ModeSetting::System, false), Mode::Light);
    assert_eq!(resolve_mode(ModeSetting::Light, true), Mode::Light);
    let compact = apply_density(&neutral, Density::Compact);
    let factor = neutral.tokens.density["compact"];
    assert_eq!(compact.tokens.control["buttonMd"], (neutral.tokens.control["buttonMd"] * factor + 0.5).floor());
    assert_eq!(compact.tokens.spacing["lg"], (neutral.tokens.spacing["lg"] * factor + 0.5).floor());
    assert_eq!(compact.tokens.radius, neutral.tokens.radius, "radii do not scale");
    assert_eq!(apply_density(&neutral, Density::Default), *neutral);
    let high = apply_contrast(&neutral);
    assert_eq!(high.modes.light.color["foreground"], neutral.contrast.light.color["foreground"]);
    assert_eq!(high.modes.light.color["primary"], neutral.modes.light.color["primary"]);
    assert_eq!(resolve_condition_key(&neutral, "@media (min-width: $breakpoint.md)"), "@media (min-width: 768px)");
    assert_eq!(resolve_condition_key(&neutral, "@media (min-width: $breakpoint.huge)"), "@media (min-width: $breakpoint.huge)");
    assert_eq!(resolve_condition_key(&neutral, ":hover"), ":hover");
    let style = json!({"transitionEasing": "$ease.standard", "transition": "$motion.fast", "@media (max-width: $breakpoint.sm)": {"gap": "$spacing.md"}, "backgroundGradient": {"angle": 90, "stops": [{"color": "$color.primary", "offset": 0}, {"color": "#ffffff00", "offset": 1}]}});
    let resolved = resolve_style_values(&neutral, style.as_object().unwrap(), Mode::Light);
    assert!(json::equal(&resolved["transitionEasing"], &json!(neutral.tokens.ease["standard"])));
    assert!(json::equal(&resolved["@media (max-width: 640px)"]["gap"], &json!(neutral.tokens.spacing["md"])));
    assert_eq!(resolved["backgroundGradient"]["stops"][0]["color"], json!(neutral.modes.light.color["primary"]));
}
