//! VAPP-103: the core against hostile or huge agent input. Every case here
//! used to hang, allocate without bound or abort the process.
//! The time budgets are generous for a debug build; the release numbers are
//! in the PR.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use exponential_ui::limits::{self, MAX_COMPONENTS, MAX_DEPTH, MAX_MESSAGE_BYTES, MAX_POINTER_SEGMENTS, MAX_TEMPLATE_ITEMS};
use exponential_ui::measure::FixedMeasure;
use exponential_ui::reducer::{reduce_surface, ReduceOptions};
use exponential_ui::surface::{LayoutOutput, Surface, SurfaceOptions};
use exponential_ui::types::{FlatComponent, ReduceIssue};
use exponential_ui::host::HostRouter;
use serde_json::{json, Value};

const CORE: &str = "https://ui.exponential.at/catalogs/core/v1";

/// `root` → `n1` → … → `n<depth-1>` → a Text leaf, each a `component`.
fn chain(component: &str, depth: usize) -> Vec<Value> {
    let id = |i: usize| if i == 0 { "root".to_string() } else { format!("n{i}") };
    let mut out: Vec<Value> = (0..depth).map(|i| json!({"id": id(i), "component": component, "children": [id(i + 1)]})).collect();
    out.push(json!({"id": id(depth), "component": "Text", "text": "deep"}));
    out
}

fn flat(list: &[Value]) -> Vec<FlatComponent> {
    list.iter().map(|v| serde_json::from_value(v.clone()).expect("component")).collect()
}

fn surface(list: Vec<Value>) -> Surface {
    let mut s = Surface::new("t", SurfaceOptions { theme: exponential_ui::themes::builtin_theme("neutral"), ..SurfaceOptions::default() });
    s.apply(&json!({"updateComponents": {"surfaceId": "t", "components": list}})).expect("message");
    s.set_viewport(400.0, 800.0, None);
    s
}

fn layout(s: &mut Surface) -> LayoutOutput {
    s.layout(&mut FixedMeasure { sizes: HashMap::new(), wrap: true })
}

fn messages(issues: &[ReduceIssue]) -> Vec<String> {
    issues.iter().map(|i| format!("{}: {}", i.id, i.message)).collect()
}

/// Run `f` on a thread with a small stack (an iOS secondary thread has
/// 512 KB; this is a quarter of it).
fn on_small_stack<R: Send + 'static>(f: impl FnOnce() -> R + Send + 'static) -> R {
    std::thread::Builder::new().stack_size(128 * 1024).spawn(f).expect("thread").join().expect("no panic")
}

fn timed<R>(budget_ms: u64, what: &str, f: impl FnOnce() -> R) -> R {
    let t = Instant::now();
    let out = f();
    let took = t.elapsed();
    assert!(took < Duration::from_millis(budget_ms), "{what}: {took:?} (budget {budget_ms} ms)");
    eprintln!("{what}: {took:?}");
    out
}

/// Item 1: layout time is linear in flex nesting depth (taffy 0.12's cache
/// made it double per level: 20 nested Stacks = 0.6 s, 14 Cards = 160 s).
/// The engine alone is gated at 50 levels (`engine::cache_tests`).
#[test]
fn depth_30_and_max_depth_nested_containers_lay_out_in_milliseconds() {
    for component in ["Stack", "Card", "Section", "Group", "Box"] {
        for depth in [30, MAX_DEPTH - 1] {
            let mut s = surface(chain(component, depth));
            let out = timed(2_000, &format!("{depth} nested {component}"), || layout(&mut s));
            assert!(out.frames.len() > depth, "{component}: {} frames", out.frames.len());
        }
    }
}

/// Item 3: a surface `maxDepth` deep (Cards: the heaviest macro) lays out on
/// a quarter of an iOS thread's stack; deeper is an issue and an Unknown.
#[test]
fn a_max_depth_surface_lays_out_on_a_small_stack() {
    for component in ["Card", "Stack", "Section", "Group"] {
        let frames = on_small_stack(move || {
            let mut s = surface(chain(component, MAX_DEPTH - 1));
            assert!(s.issues().is_empty(), "{component}: {:?}", s.issues());
            layout(&mut s).frames.len()
        });
        assert!(frames >= MAX_DEPTH, "{component}: {frames} frames");
    }
}

#[test]
fn a_1500_deep_chain_is_an_issue_not_an_abort() {
    let (issues, frames) = on_small_stack(|| {
        let mut s = surface(chain("Box", 1500));
        let issues = s.issues().to_vec();
        (issues, layout(&mut s).frames.len())
    });
    assert_eq!(messages(&issues), vec![format!("n{MAX_DEPTH}: {}", limits::depth_issue())]);
    assert_eq!(frames, MAX_DEPTH + 1, "the levels up to the cap plus the Unknown placeholder");
}

/// Item 2: an id listed twice renders once (21 components each naming the
/// next twice: 2^21 builds, 14 s and 5.4 GB before).
#[test]
fn an_id_listed_twice_renders_once() {
    let mut list: Vec<Value> = (0..21).map(|i| json!({"id": if i == 0 { "root".into() } else { format!("n{i}") }, "component": "Stack", "children": [format!("n{}", i + 1), format!("n{}", i + 1)]})).collect();
    list.push(json!({"id": "n21", "component": "Text", "text": "x"}));
    let result = timed(2_000, "21 components naming the next twice", || reduce_surface(&flat(&list), &ReduceOptions::new(CORE)));
    let twice: Vec<String> = (1..=21).map(|i| format!("n{i}: {}", limits::USED_TWICE_ISSUE)).collect();
    assert_eq!(messages(&result.issues), twice.into_iter().rev().collect::<Vec<_>>());
    let mut count = 0;
    result.root.walk(&mut |_| count += 1);
    assert!(count < 200, "{count} nodes");
    // Two parents: the first place wins.
    let two = flat(&[
        json!({"id": "root", "component": "Stack", "children": ["a", "b"]}),
        json!({"id": "a", "component": "Stack", "children": ["shared"]}),
        json!({"id": "b", "component": "Stack", "children": ["shared"], "slots": {}}),
        json!({"id": "shared", "component": "Text", "text": "once"}),
    ]);
    let r = reduce_surface(&two, &ReduceOptions::new(CORE));
    assert_eq!(messages(&r.issues), vec![format!("shared: {}", limits::USED_TWICE_ISSUE)]);
    assert_eq!(r.root.children[0].children[0].id, "shared");
    assert!(r.root.children[1].children.is_empty());
    // A cycle is still a cycle.
    let cyc = flat(&[json!({"id": "root", "component": "Stack", "children": ["a"]}), json!({"id": "a", "component": "Stack", "children": ["root"]})]);
    assert_eq!(messages(&reduce_surface(&cyc, &ReduceOptions::new(CORE)).issues), vec!["root: cycle through this id".to_string()]);
}

/// Item 5: past `maxComponents` the rest is dropped (one issue).
#[test]
fn past_max_components_the_rest_is_dropped() {
    let n = MAX_COMPONENTS + 50;
    let mut list = vec![json!({"id": "root", "component": "Stack", "children": (0..n).map(|i| format!("c{i}")).collect::<Vec<_>>()})];
    list.extend((0..n).map(|i| json!({"id": format!("c{i}"), "component": "Text", "text": "x"})));
    let r = timed(10_000, "maxComponents + 50", || reduce_surface(&flat(&list), &ReduceOptions::new(CORE)));
    assert_eq!(messages(&r.issues), vec![format!("c{}: {}", MAX_COMPONENTS - 1, limits::components_issue())]);
    assert_eq!(r.root.children.len(), MAX_COMPONENTS - 1);
}

/// Item 5: past `maxTemplateItems` instances a surface stops instantiating.
#[test]
fn past_max_template_items_the_rest_is_not_rendered() {
    let mut s = surface(vec![
        json!({"id": "root", "component": "Stack", "children": {"componentId": "row", "path": "/outer"}}),
        json!({"id": "row", "component": "Stack", "children": {"componentId": "cell", "path": "inner"}}),
        json!({"id": "cell", "component": "Text", "text": "x"}),
    ]);
    // 200 × 200 = 40,000 instances asked for.
    let inner: Vec<Value> = (0..200).map(|i| json!(i)).collect();
    s.set_data("/outer", Some(Value::Array((0..200).map(|_| json!({"inner": inner.clone()})).collect()))).unwrap();
    let out = timed(20_000, "200 × 200 template items", || layout(&mut s));
    assert!(out.frames.len() <= MAX_TEMPLATE_ITEMS + 10, "{} frames", out.frames.len());
    assert!(messages(s.issues()).iter().any(|m| m.ends_with(&limits::template_items_issue())), "{:?}", s.issues());
}

/// Item 6: a surface streamed one component per message costs linear
/// (each message re-reduced the whole surface: 2,000 messages = 3.45 s).
#[test]
fn two_thousand_streamed_messages_cost_linear() {
    let mut s = Surface::new("t", SurfaceOptions::default());
    s.apply(&json!({"createSurface": {"surfaceId": "t", "catalogId": CORE}})).unwrap();
    let ids: Vec<String> = (0..2000).map(|i| format!("c{i}")).collect();
    s.set_viewport(400.0, 800.0, None);
    timed(3_000, "2,000 streamed updateComponents", || {
        s.apply(&json!({"updateComponents": {"surfaceId": "t", "components": [{"id": "root", "component": "List", "children": ids}]}})).unwrap();
        for i in 0..2000 {
            s.apply(&json!({"updateComponents": {"surfaceId": "t", "components": [{"id": format!("c{i}"), "component": "Text", "text": format!("Row {i}")}]}})).unwrap();
        }
        layout(&mut s)
    });
    assert!(s.issues().is_empty(), "{:?}", &s.issues()[..3]);
    // A replaced component replaces in place (order kept).
    s.apply(&json!({"updateComponents": {"surfaceId": "t", "components": [{"id": "c5", "component": "Text", "text": "five"}]}})).unwrap();
    assert_eq!(s.root().unwrap().children.len(), 2000);
}

/// Items 3 + 4: data pointers.
#[test]
fn data_pointers_refuse_gaps_huge_indices_and_long_paths() {
    let mut s = Surface::new("t", SurfaceOptions::default());
    s.set_data("", Some(json!({"a": [1, 2, 3]}))).unwrap();
    let err = |s: &mut Surface, p: &str| s.set_data(p, Some(json!(0))).unwrap_err();
    assert_eq!(err(&mut s, "/a/4000000000"), "data: index 4000000000 is past the end of the array (3 items)");
    assert_eq!(err(&mut s, "/a/4"), "data: index 4 is past the end of the array (3 items)");
    assert_eq!(err(&mut s, "/a/x"), "data: \"x\" is not an array index");
    s.set_data("/a/3", Some(json!(4))).unwrap();
    s.set_data("/a/-", Some(json!(5))).unwrap();
    s.set_data("/a/5/b", Some(json!(6))).unwrap();
    assert_eq!(s.data(), &json!({"a": [1, 2, 3, 4, 5, {"b": 6}]}));
    let deep = "/x".repeat(MAX_POINTER_SEGMENTS + 1);
    assert_eq!(err(&mut s, &deep), format!("data: pointer has more than {MAX_POINTER_SEGMENTS} segments"));
    let long = format!("/{}", "x".repeat(limits::MAX_POINTER_BYTES));
    assert_eq!(err(&mut s, &long), format!("data: pointer longer than {} bytes", limits::MAX_POINTER_BYTES));
    // 20,000 segments: refused, never a recursion.
    let huge = "/a".repeat(20_000);
    on_small_stack(move || {
        let mut s = Surface::new("t", SurfaceOptions::default());
        assert!(s.set_data(&huge, Some(json!(1))).is_err());
    });
    // The longest allowed pointer writes (iteratively).
    s.set_data(&"/k".repeat(MAX_POINTER_SEGMENTS), Some(json!(1))).unwrap();
    // updateDataModel reports the refusal as an issue.
    let out = s.apply(&json!({"updateDataModel": {"surfaceId": "t", "path": "/a/99", "value": 1}})).unwrap();
    assert_eq!(messages(&out.issues), vec!["/a/99: data: index 99 is past the end of the array (6 items)".to_string()]);
}

/// Item 5: the router refuses a message past `maxMessageBytes`.
#[test]
fn the_router_refuses_an_oversized_message() {
    let mut router = HostRouter::new::<&str>(&[]);
    router.route(&json!({"version": "v0.9", "createSurface": {"surfaceId": "s", "catalogId": CORE}}));
    let big = "x".repeat(MAX_MESSAGE_BYTES);
    let ops = router.route(&json!({"version": "v0.9", "updateComponents": {"surfaceId": "s", "components": [{"id": "root", "component": "Text", "text": big}]}}));
    assert_eq!(ops.len(), 1);
    assert_eq!(ops[0]["message"]["error"]["code"], "INVALID_MESSAGE");
    assert_eq!(ops[0]["message"]["error"]["surfaceId"], "s");
    assert_eq!(ops[0]["message"]["error"]["message"], limits::message_bytes_issue());
    let ok = router.route(&json!({"version": "v0.9", "updateComponents": {"surfaceId": "s", "components": [{"id": "root", "component": "Text", "text": "x".repeat(1000)}]}}));
    assert_eq!(ok[0]["op"], "components");
}

