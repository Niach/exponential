//! VAPP-103: the core against hostile or huge agent input. Every case here
//! used to hang, allocate without bound or abort the process.
//! The time budgets are generous for a debug build; the release numbers are
//! in the PR.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use exponential_ui::limits::{self, MAX_COMPONENTS, MAX_DEPTH, MAX_MESSAGE_BYTES, MAX_POINTER_SEGMENTS, MAX_TEMPLATE_ITEMS};
use exponential_ui::measure::FixedMeasure;
use exponential_ui::reducer::{reduce_surface, ReduceOptions};
use exponential_ui::measure::Intrinsics;
use exponential_ui::surface::{LayoutOutput, LayoutStep, Surface, SurfaceOptions};
use exponential_ui::types::{FlatComponent, ReduceIssue};
use exponential_ui::host::HostRouter;
use serde_json::{json, Value};

const CORE: &str = "https://ui.exponential.at/catalogs/core/v1";

/// `root` → `n1` → … → `n<depth-1>` → a Text leaf that WRAPS (its height
/// follows the width it gets), each a `component`.
fn chain(component: &str, depth: usize) -> Vec<Value> {
    let id = |i: usize| if i == 0 { "root".to_string() } else { format!("n{i}") };
    let mut out: Vec<Value> = (0..depth).map(|i| json!({"id": id(i), "component": component, "children": [id(i + 1)]})).collect();
    out.push(json!({"id": id(depth), "component": "Text", "text": "the deepest words wrap at a narrow width"}));
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
/// made it double per level: 20 nested Stacks = 0.6 s, 14 Cards = 160 s),
/// and in BLOCK nesting over a wrapping text (an inherent-size and a
/// content-size request shared a cache slot: 47 nested Boxes never ended).
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

/// Item 3: a surface `maxDepth` deep lays out on a quarter of an iOS
/// thread's stack; deeper is an issue and an Unknown. VAPP-103 rfix: the cap
/// holds on the REDUCED tree (a Card's macro adds levels): a 47-deep Card
/// chain (Section and Group too) is cut where its expansion passes `maxDepth`, and every reduced
/// tree round-trips through serde_json's default recursion limit.
#[test]
fn a_max_depth_surface_lays_out_on_a_small_stack() {
    for component in ["Card", "Stack", "Section", "Group", "Box"] {
        let (issues, frames) = on_small_stack(move || {
            let mut s = surface(chain(component, MAX_DEPTH - 1));
            let issues = s.issues().to_vec();
            (issues, layout(&mut s).frames.len())
        });
        let depth_cut = issues.iter().any(|i| i.message == limits::depth_issue());
        assert_eq!(depth_cut, matches!(component, "Card" | "Section" | "Group"), "{component}: {:?}", messages(&issues));
        assert!(frames >= MAX_DEPTH, "{component}: {frames} frames");
        let r = reduce_surface(&flat(&chain(component, MAX_DEPTH - 1)), &ReduceOptions::new(CORE));
        assert!(tree_depth(&r.root) <= MAX_DEPTH + 1, "{component}: reduced depth {}", tree_depth(&r.root));
        let text = serde_json::to_string(&r).expect("serialize");
        serde_json::from_str::<Value>(&text).unwrap_or_else(|e| panic!("{component}: the reduced JSON reparses: {e}"));
    }
}

/// Levels of a reduced tree (the root = 1; children and slots).
fn tree_depth(root: &exponential_ui::types::UiNode) -> usize {
    let mut max = 0;
    let mut stack = vec![(root, 1usize)];
    while let Some((n, d)) = stack.pop() {
        max = max.max(d);
        stack.extend(n.children.iter().map(|c| (c, d + 1)));
        stack.extend(n.slots.iter().flat_map(|s| s.values()).map(|c| (c, d + 1)));
    }
    max
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
/// VAPP-103 rfix: template items charge their template subtree's nodes
/// against `maxComponents` (on top of the reduced tree's): a 500-Text row ×
/// 1000 items was 501k nodes and 3.7 GB. Shrinking the data clears the issue.
#[test]
fn template_items_charge_their_nodes_against_max_components() {
    let k = 500;
    let mut list = vec![
        json!({"id": "root", "component": "Stack", "children": {"componentId": "row", "path": "/items"}}),
        json!({"id": "row", "component": "Stack", "children": (0..k).map(|i| format!("t{i}")).collect::<Vec<_>>()}),
    ];
    list.extend((0..k).map(|i| json!({"id": format!("t{i}"), "component": "Text", "text": "x"})));
    let mut s = surface(list);
    s.set_data("/items", Some(Value::Array((0..1000).map(|i| json!(i)).collect()))).unwrap();
    let out = timed(10_000, "500-node row x 1000 items", || layout(&mut s));
    // The root (1 node) + 39 items × 501 nodes = 19,540 ≤ 20,000 < + 501.
    let items = (MAX_COMPONENTS - 1) / (k + 1);
    assert!(out.frames.len() <= MAX_COMPONENTS + 10, "{} frames", out.frames.len());
    assert!(out.frames.len() >= items * (k + 1), "{} frames", out.frames.len());
    assert_eq!(messages(s.issues()), vec![format!("row: {}", limits::components_issue())]);
    s.set_data("/items", Some(Value::Array((0..3).map(|i| json!(i)).collect()))).unwrap();
    let out = layout(&mut s);
    assert!(out.frames.len() >= 3 * (k + 1));
    assert!(s.issues().is_empty(), "the issue clears when the data shrinks: {:?}", s.issues());
}

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

fn count_nodes(n: &exponential_ui::types::UiNode) -> usize {
    1 + n.children.iter().map(count_nodes).sum::<usize>() + n.slots.iter().flat_map(|s| s.values()).map(count_nodes).sum::<usize>()
}

/// R8 F51: a Rating's `max` of 1e8 or 1e300 neither hangs nor allocates
/// that many stars: `range()` stops at 10,000 (`MAX_RANGE_ITEMS`), the
/// expanded parts spend `maxComponents` (one issue), and the catalog's
/// `maximum: 100` refuses the value.
#[test]
fn a_rating_with_a_huge_max_reduces_within_the_budget() {
    for max in [1e8, 1e300] {
        let list = flat(&[json!({"id": "root", "component": "Rating", "max": max})]);
        let r = timed(10_000, "Rating max", || reduce_surface(&list, &ReduceOptions::new(CORE)));
        let msgs = messages(&r.issues);
        assert!(msgs.contains(&"root: props.max: expected at most 100".to_string()), "{msgs:?}");
        assert!(msgs.iter().any(|m| m.ends_with(&limits::components_issue())), "{msgs:?}");
        assert_eq!(msgs.iter().filter(|m| m.ends_with(&limits::components_issue())).count(), 1);
        assert!(count_nodes(&r.root) <= MAX_COMPONENTS, "{} nodes", count_nodes(&r.root));
        assert_eq!(r.root.children.len(), exponential_ui::expr::MAX_RANGE_ITEMS, "every star placed, the last icon dropped");
    }
    // A sane max keeps its stars and passes validation.
    let r = reduce_surface(&flat(&[json!({"id": "root", "component": "Rating", "max": 7})]), &ReduceOptions::new(CORE));
    assert!(messages(&r.issues).is_empty(), "{:?}", r.issues);
    assert_eq!(r.root.children.len(), 7);
}

/// R8 F52: a children list that repeats an UNKNOWN id a hundred thousand
/// times stops at `maxComponents` placeholders (they spend the budget before
/// they are made); the same list naming a KNOWN id costs ONE used-twice
/// issue, not one per reference.
#[test]
fn placeholder_floods_stop_at_the_budget() {
    let n = 100_000;
    let unknown = flat(&[json!({"id": "root", "component": "Stack", "children": vec!["x"; n]})]);
    let r = timed(10_000, "100k unknown children", || reduce_surface(&unknown, &ReduceOptions::new(CORE)));
    assert_eq!(r.root.children.len(), MAX_COMPONENTS - 1);
    let msgs = messages(&r.issues);
    assert_eq!(msgs.iter().filter(|m| m.ends_with(&limits::components_issue())).count(), 1);
    assert_eq!(msgs.iter().filter(|m| *m == "x: no component with this id").count(), MAX_COMPONENTS - 1);
    assert_eq!(msgs.len(), MAX_COMPONENTS, "nothing is reported past the budget");

    let twice = flat(&[json!({"id": "root", "component": "Stack", "children": vec!["a"; n]}), json!({"id": "a", "component": "Text", "text": "x"})]);
    let r = timed(10_000, "100k repeated children", || reduce_surface(&twice, &ReduceOptions::new(CORE)));
    assert_eq!(r.root.children.len(), 1);
    assert_eq!(messages(&r.issues), vec![format!("a: {}", limits::USED_TWICE_ISSUE)]);
}

/// R8 F53: a host measurer answering NaN or infinity never panics the memo
/// (`f32::clamp` on NaN did): the answers are sanitised on store.
#[test]
fn nan_and_infinite_intrinsics_are_sanitised() {
    let mut s = surface(vec![
        json!({"id": "root", "component": "Stack", "children": ["a", "b"]}),
        json!({"id": "a", "component": "Text", "text": "some words that wrap at a narrow width"}),
        json!({"id": "b", "component": "Text", "text": "more words that wrap at a narrow width"}),
    ]);
    let LayoutStep::Intrinsics(leaves) = s.layout_begin(1) else { panic!("intrinsics first") };
    assert!(!leaves.is_empty());
    let bad = [
        Intrinsics { min_content_width: f32::NAN, max_content_width: f32::NAN, height_at_max_content: f32::NAN, baseline: Some(f32::NAN) },
        Intrinsics { min_content_width: f32::INFINITY, max_content_width: f32::NEG_INFINITY, height_at_max_content: f32::INFINITY, baseline: Some(f32::NEG_INFINITY) },
    ];
    let answers: Vec<Intrinsics> = leaves.iter().enumerate().map(|(k, _)| bad[k % 2]).collect();
    let mut step = s.layout_intrinsics(&answers);
    while let LayoutStep::Heights(_, r) = step {
        step = s.layout_heights(&vec![f32::NAN; r.len()]);
    }
    let LayoutStep::Done(out) = step else { panic!("done") };
    assert!(out.surface_height.is_finite(), "{}", out.surface_height);
    assert!(out.frames.iter().all(|f| f.x.is_finite() && f.y.is_finite() && f.w.is_finite() && f.h.is_finite()));
    // The sanitiser itself: NaN/inf → 0, widths ordered, a non-finite baseline = none.
    let i = Intrinsics { min_content_width: 30.0, max_content_width: 10.0, height_at_max_content: -5.0, baseline: Some(f32::NAN) }.sanitised();
    assert_eq!(i, Intrinsics { min_content_width: 10.0, max_content_width: 30.0, height_at_max_content: 0.0, baseline: None });
}
