//! `fixtures/embed.json` (#EDE-65 lane M): Embed, a nested surface drawn in
//! place, sandboxed. `eui_core::embed` replays `parse` and `errorRoot`; the
//! core's prop resolution keeps a literal `surface` verbatim and resolves a
//! top-level binding; and the whole `render` / `events` contract replays on
//! core surfaces alone (each Embed mounts its card as its own `Surface`, an
//! inner `event` re-fires on the host Embed as `action`, an inner host
//! function is denied) — the reference the painters' mounts follow.

use std::collections::HashMap;

use eui_core::embed::{data_key_path, embed_card, embed_error_root, embed_error_root_value, embed_mount, embed_payload, embed_surface_id, EMBED_EVENT};
use eui_core::measure::FixedMeasure;
use eui_core::reducer::{reduce_nested, ReduceOptions};
use eui_core::surface::{OutEvent, Surface, SurfaceOptions};
use eui_core::types::NestedNode;
use eui_core::CORE_CATALOG_ID;
use serde_json::{json, Map, Value};

const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../../../packages/exponential-ui/fixtures");

fn fixture() -> Value {
    serde_json::from_str(&std::fs::read_to_string(format!("{FIXTURES}/embed.json")).expect("fixture")).expect("json")
}

fn cases<'a>(f: &'a Value, key: &str) -> &'a Vec<Value> {
    f[key].as_array().unwrap_or_else(|| panic!("{key}"))
}

#[test]
fn parse_cases_decide_valid_and_the_card_data() {
    let f = fixture();
    for c in cases(&f, "parse") {
        let name = c["name"].as_str().unwrap();
        let card = embed_card(&c["surface"]);
        assert_eq!(card.is_some(), c["valid"] == json!(true), "{name}");
        let mount = embed_mount(&c["surface"]);
        assert_eq!(mount.valid, card.is_some(), "{name}");
        if let Some(card) = card {
            assert_eq!(Value::Object(card.data.clone()), c["data"], "{name}");
            assert_eq!(card.root, c["surface"]["root"], "{name}: the root as written");
            assert!(card.node().is_some(), "{name}: the root deserializes");
        } else {
            assert_eq!(mount.card.root, f["errorRoot"], "{name}: the error box");
            assert!(mount.card.data.is_empty(), "{name}");
        }
    }
}

#[test]
fn the_error_root_is_the_fixtures() {
    let f = fixture();
    assert_eq!(embed_error_root_value(), f["errorRoot"]);
    assert_eq!(serde_json::to_value(embed_error_root()).unwrap(), f["errorRoot"]);
    assert_eq!(EMBED_EVENT, "action");
    assert_eq!(embed_surface_id("host", "card"), "host/card");
    assert_eq!(embed_payload("reply", Some(&json!({"text": "Yes"}))), json!({"name": "reply", "context": {"text": "Yes"}}));
}

#[test]
fn every_host_root_reduces_without_issues() {
    let f = fixture();
    let options = ReduceOptions::new(CORE_CATALOG_ID);
    for key in ["render", "events"] {
        for c in cases(&f, key) {
            let name = c["name"].as_str().unwrap();
            let tree: NestedNode = serde_json::from_value(c["root"].clone()).unwrap_or_else(|e| panic!("{name}: {e}"));
            let result = reduce_nested(&tree, &options);
            assert!(result.issues.is_empty(), "{key}/{name}: {:?}", result.issues);
        }
    }
    let error: NestedNode = embed_error_root();
    assert!(reduce_nested(&error, &options).issues.is_empty());
}

fn mount(root: &Value, data: &Map<String, Value>, id: &str) -> Surface {
    let tree: NestedNode = serde_json::from_value(root.clone()).expect("tree");
    let mut s = Surface::new(id, SurfaceOptions::default());
    s.set_nested(tree);
    for (k, v) in data {
        s.set_data(&data_key_path(k), Some(v.clone())).unwrap();
    }
    s.set_viewport(400.0, 0.0, None);
    s.layout(&mut FixedMeasure { sizes: HashMap::new(), wrap: true });
    s
}

#[test]
fn a_literal_surface_stays_verbatim_and_a_top_level_binding_resolves() {
    let f = fixture();
    let render = cases(&f, "render");
    let by = |n: &str| render.iter().find(|c| c["name"] == json!(n)).unwrap().clone();
    // own-data: the inner `{"@path": "/name"}` is never resolved against the host.
    let c = by("own-data");
    let mut s = mount(&c["root"], c["data"].as_object().unwrap(), "host");
    let card = s.nodes().into_iter().find(|n| n.id == "card").unwrap();
    assert_eq!(card.props["surface"], c["root"]["children"][0]["props"]["surface"]);
    // bound-surface: the top-level binding resolves against the host data.
    let c = by("bound-surface");
    let mut s = mount(&c["root"], c["data"].as_object().unwrap(), "host");
    let card = s.nodes().into_iter().find(|n| n.id == "card").unwrap();
    assert_eq!(card.props["surface"], c["data"]["card"]);
    // unbound: nothing (the painter shows the error box).
    let c = by("unbound-surface-error-box");
    let mut s = mount(&c["root"], c["data"].as_object().unwrap(), "host");
    let card = s.nodes().into_iter().find(|n| n.id == "card").unwrap();
    assert!(!embed_mount(card.props.get("surface").unwrap_or(&Value::Null)).valid);
    assert!(eui_core::catalog::is_data_prop("Embed", "surface"));
}

/// A host surface with every Embed mounted as its own surface (painted
/// order), recursively.
struct Mounted {
    surface: Surface,
    /// The painted Embeds: (host node id, the nested mount).
    embeds: Vec<(String, Mounted)>,
}

fn preorder(s: &mut Surface) -> Vec<eui_core::surface::PlacedNode> {
    let nodes = s.nodes();
    let mut out = Vec::new();
    let Some(root) = nodes.iter().find(|n| n.parent.is_none() && n.layer == 0 && !n.removed) else { return out };
    let mut stack = vec![root.index];
    while let Some(i) = stack.pop() {
        let n = nodes[i as usize].clone();
        if n.removed || n.hidden {
            continue;
        }
        stack.extend(n.children.iter().rev().copied());
        out.push(n);
    }
    out
}

fn mount_tree(root: &Value, data: &Map<String, Value>, id: &str) -> Mounted {
    let mut surface = mount(root, data, id);
    let mut embeds = Vec::new();
    for n in preorder(&mut surface) {
        if n.component == "Embed" {
            let m = embed_mount(n.props.get("surface").unwrap_or(&Value::Null));
            embeds.push((n.id.clone(), mount_tree(&m.card.root, &m.card.data, &embed_surface_id(id, &n.id))));
        }
    }
    Mounted { surface, embeds }
}

/// Every painted string (Text `text`, a Button `label`), pre-order, the
/// nested surfaces in place of their Embeds.
fn texts(m: &mut Mounted) -> Vec<String> {
    let mut out = Vec::new();
    let mut embeds = m.embeds.iter_mut();
    for n in preorder(&mut m.surface) {
        if n.component == "Embed" {
            if let Some((_, inner)) = embeds.next() {
                out.extend(texts(inner));
            }
            continue;
        }
        for key in ["text", "label"] {
            if let Some(Value::String(s)) = n.props.get(key) {
                out.push(s.clone());
            }
        }
    }
    out
}

fn assert_texts(name: &str, got: &[String], c: &Value) {
    let mut at = 0;
    for want in c["texts"].as_array().unwrap() {
        let want = want.as_str().unwrap();
        match got[at..].iter().position(|t| t == want) {
            Some(p) => at += p + 1,
            None => panic!("{name}: `{want}` not painted (in order) in {got:?}"),
        }
    }
    for absent in c["absent"].as_array().unwrap() {
        assert!(!got.iter().any(|t| t == absent.as_str().unwrap()), "{name}: `{absent}` painted in {got:?}");
    }
}

#[test]
fn render_cases_paint_the_nested_surface_in_place() {
    let f = fixture();
    for c in cases(&f, "render") {
        let name = c["name"].as_str().unwrap();
        let mut m = mount_tree(&c["root"], c["data"].as_object().unwrap(), "host");
        assert_texts(name, &texts(&mut m), c);
    }
}

/// The host side of one mounted Embed: what the core's events become.
#[derive(Default)]
struct Heard {
    actions: Vec<Value>,
    functions: Vec<Value>,
}

/// Run an inner surface's OutEvents: an inner host function is DENIED (its
/// action stops), an inner `event` re-fires on the host Embed as `action`
/// with `{name, context}`.
fn route(host: &mut Surface, embed_id: &str, events: Vec<OutEvent>, heard: &mut Heard) {
    let mut events = events.into_iter().peekable();
    while let Some(e) = events.next() {
        match e {
            OutEvent::FunctionCall { .. } => {
                if matches!(events.peek(), Some(OutEvent::Action { .. })) {
                    events.next();
                }
            }
            OutEvent::Action { name, context, .. } => {
                let index = host.index_of(embed_id).expect("the Embed");
                for out in host.event(index, EMBED_EVENT, Some(embed_payload(&name, Some(&context)))) {
                    match out {
                        OutEvent::Action { name, context, .. } => heard.actions.push(json!({"name": name, "context": context})),
                        OutEvent::FunctionCall { name, args, .. } => heard.functions.push(json!({"name": name, "args": args})),
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }
}

#[test]
fn event_cases_bubble_inner_events_as_the_embeds_action() {
    let f = fixture();
    for c in cases(&f, "events") {
        let name = c["name"].as_str().unwrap();
        let mut m = mount_tree(&c["root"], c["data"].as_object().unwrap(), "host");
        let k = c["press"]["embed"].as_u64().unwrap() as usize;
        let (embed_id, inner) = &mut m.embeds[k];
        let embed_id = embed_id.clone();
        let index = inner.surface.index_of(c["press"]["node"].as_str().unwrap()).unwrap_or_else(|| panic!("{name}: no inner node"));
        let events = inner.surface.event(index, "press", None);
        inner.surface.layout(&mut FixedMeasure { sizes: HashMap::new(), wrap: true });
        let mut heard = Heard::default();
        route(&mut m.surface, &embed_id, events, &mut heard);
        assert_eq!(Value::Array(heard.actions), c["actions"], "{name}: actions");
        assert_eq!(Value::Array(heard.functions), c["functions"], "{name}: functions");
        m.surface.layout(&mut FixedMeasure { sizes: HashMap::new(), wrap: true });
        assert_texts(name, &texts(&mut m), c);
    }
}
