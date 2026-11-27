//! #EXP-2204 (`packages/exponential-ui/docs/ci-components-contract.md`):
//! the gpui painters of LogLines (through LogView), JobGraph and the
//! CodeBlock anchors + blame, through a headless window: the leaves paint,
//! the rows window, sections fold through the core, search marks and
//! stepping, gutter anchors with Shift, Follow sticking and turning off on a
//! person's scroll up, a selection revealing its line, the JobGraph's
//! connectors only for the hovered node's upstream set, the list below md,
//! the keyboard, CodeBlock anchors, blame and windowing.

use std::cell::RefCell;
use std::rc::Rc;

use eui_core::ci_log::LogRow;
use eui_core::NestedNode;
use eui_gpui::host::{ActionEvent, HostPlugin};
use eui_gpui::view::{SurfaceView, SurfaceViewOptions};
use gpui::{px, App, Entity, Modifiers, MouseButton, TestAppContext, VisualTestContext};
use serde_json::{json, Value};

#[derive(Clone, Default)]
struct Recorder(Rc<RefCell<Vec<ActionEvent>>>);

impl HostPlugin for Recorder {
    fn on_action(&self, event: &ActionEvent, _cx: &mut App) {
        self.0.borrow_mut().push(event.clone());
    }
    fn open_url(&self, _url: &str, _cx: &mut App) {}
}

impl Recorder {
    /// `(name, context)` of every action so far.
    fn seen(&self) -> Vec<(String, Value)> {
        self.0.borrow().iter().map(|a| (a.name.clone(), a.payload.clone().filter(|p| !p.is_null()).unwrap_or_else(|| a.context.clone()))).collect()
    }
    fn named(&self, name: &str) -> Vec<Value> {
        self.seen().into_iter().filter(|(n, _)| n == name).map(|(_, c)| c).collect()
    }
    fn clear(&self) {
        self.0.borrow_mut().clear();
    }
}

fn draw(cx: &mut VisualTestContext) {
    cx.update(|window, cx| window.draw(cx).clear(cx));
    cx.run_until_parked();
}

fn settle(cx: &mut VisualTestContext) {
    for _ in 0..3 {
        draw(cx);
    }
}

fn surface(cx: &mut TestAppContext, width: f32, child: Value, data: Option<Value>) -> (Entity<SurfaceView>, Recorder, &mut VisualTestContext) {
    cx.update(gpui_component::init);
    let log = Recorder::default();
    let host = Rc::new(log.clone());
    let options = SurfaceViewOptions { theme: eui_core::themes::builtin_theme("neutral"), mode: eui_core::theme::Mode::Light, host, width: Some(width), ..Default::default() };
    let (view, vcx) = cx.add_window_view(move |window, cx| SurfaceView::new(options, window, cx));
    let tree = json!({"id": "root", "component": "Box", "style": {"display": "flex", "flexDirection": "column", "padding": 16}, "children": [child, {"id": "after", "component": "Button", "props": {"label": "After"}}]});
    view.update(vcx, |v, cx| {
        v.record_bounds(true);
        if let Some(d) = data {
            v.set_data("", Some(d), cx).expect("data");
        }
        let out = v.set_nested(serde_json::from_value::<NestedNode>(tree).unwrap(), cx);
        assert!(out.issues.is_empty(), "{:?}", out.issues);
    });
    settle(vcx);
    (view, log, vcx)
}

fn bounds(view: &Entity<SurfaceView>, cx: &mut VisualTestContext, key: &str) -> gpui::Bounds<gpui::Pixels> {
    view.read_with(cx, |v, _| v.painted_bounds(key)).unwrap_or_else(|| panic!("{key} was not painted"))
}

fn click(view: &Entity<SurfaceView>, cx: &mut VisualTestContext, key: &str, modifiers: Modifiers) {
    let at = bounds(view, cx, key).center();
    cx.simulate_mouse_move(at, None, modifiers);
    cx.simulate_mouse_down(at, MouseButton::Left, modifiers);
    cx.simulate_mouse_up(at, MouseButton::Left, modifiers);
    settle(cx);
}

fn lines_id(view: &Entity<SurfaceView>, cx: &mut VisualTestContext) -> String {
    view.read_with(cx, |v, _| v.placed_nodes().iter().find(|n| n.component == "LogLines" && !n.removed).map(|n| n.id.clone())).expect("a LogLines leaf")
}

/// A CI log: a short setup step, then `count` test lines in a failed step.
fn log_lines(count: usize) -> Vec<String> {
    let mut lines = vec!["##[group]Set up job".to_string(), "Runner name: 'runner-3'".into(), "##[endgroup]".into(), "##[group]Unit tests".into()];
    for i in 0..count {
        if i == count / 2 {
            lines.push("\u{1b}[31m(fail)\u{1b}[0m store > get returns the cached row".into());
        } else {
            lines.push(format!("\u{1b}[32m(pass)\u{1b}[0m case {i} [0.2ms]"));
        }
    }
    lines.push("##[error]Process completed with exit code 1.".into());
    lines.push("##[endgroup]".into());
    lines
}

fn log_view(props: Value) -> Value {
    let mut p = json!({"lines": log_lines(200), "sections": [{"status": "passed", "duration": "2s"}, {"status": "failed", "duration": "41s"}], "maxLines": 10});
    for (k, v) in props.as_object().unwrap() {
        p[k] = v.clone();
    }
    json!({"id": "log", "component": "LogView", "props": p, "on": {
        "toggle": {"event": {"name": "toggled"}}, "search": {"event": {"name": "searched"}}, "select": {"event": {"name": "selected"}},
        "follow": {"event": {"name": "followed"}}, "timestamps": {"event": {"name": "stamped"}}
    }})
}

#[gpui::test]
fn log_lines_paints_a_window_of_its_rows_and_follows_the_end(cx: &mut TestAppContext) {
    let (view, _log, cx) = surface(cx, 900.0, log_view(json!({})), None);
    let id = lines_id(&view, cx);
    let st = view.read_with(cx, |v, _| v.log_lines_state(&id)).expect("painted");
    // The failed step opens on its own: 2 step rows + 1 + 201 lines.
    assert_eq!(st.rows, 2 + 200 + 1);
    // Windowed: the 10 lines in view ± 5 overscan.
    assert!(st.built.len() <= 10 + 2 * 5 + 1, "{} rows built", st.built.len());
    // Follow (the default) sticks to the bottom: the last line is built.
    assert!(st.max_scroll.1 > 0.0);
    assert!((st.scroll.1 - st.max_scroll.1).abs() < 0.5, "{st:?}");
    assert_eq!(st.built.last(), Some(&LogRow::Line(202)));
    // The leaf is a region named Log.
    let info = view.read_with(cx, |v, _| v.accessible_info(&id)).expect("a11y");
    assert_eq!(info.role, gpui::Role::Region);
    assert_eq!(info.label.map(|s| s.to_string()).as_deref(), Some("Log"));
}

#[gpui::test]
fn the_toolbar_is_one_row_from_md(cx: &mut TestAppContext) {
    // Wide: one row, the search 240 wide, Follow on the search's row.
    let (view, _log, cx) = surface(cx, 900.0, log_view(json!({})), None);
    let id = lines_id(&view, cx);
    let (search, follow, body) = (bounds(&view, cx, &format!("{id}.search")), bounds(&view, cx, &format!("{id}.follow")), bounds(&view, cx, &format!("{id}.body")));
    near(f32::from(search.size.width), 240.0, "the search: logView.searchWidth");
    assert_eq!(search.center().y, follow.center().y, "one row");
    near(f32::from(body.origin.y - search.origin.y), 36.0 + 8.0, "the body under one row");
    near(f32::from(body.size.height), LOG_BODY_10, "the body: maxLines lines");
}

/// Within the body's hairline (the probe may sit inside its border).
fn near(got: f32, want: f32, what: &str) {
    assert!((got - want).abs() <= 2.0 + 0.01, "{what}: {got} vs {want}");
}

/// The body of a 10-line LogLines: the lines + 2 × (padding + hairline).
const LOG_BODY_10: f32 = 10.0 * 22.75 + 2.0 * (8.0 + 1.0);

#[gpui::test]
fn the_toolbar_takes_two_rows_below_md_and_the_body_keeps_its_lines(cx: &mut TestAppContext) {
    // Compact (the core's `compact`, the surface below md): row 1 = the
    // search filling the width with prev/next, row 2 = the toggles at the end.
    let (view, _log, cx) = surface(cx, 390.0, log_view(json!({})), None);
    let id = lines_id(&view, cx);
    let leaf = view.read_with(cx, |v, _| v.placed_nodes().iter().find(|n| n.id == id).map(|n| n.props.get("compact").cloned())).flatten();
    assert_eq!(leaf, Some(json!(true)));
    let get = |k: &str, cx: &mut VisualTestContext| bounds(&view, cx, &format!("{id}.{k}"));
    let (search, next, follow, stamps, body) = (get("search", cx), get("next", cx), get("follow", cx), get("timestamps", cx), get("body", cx));
    assert_eq!(search.center().y, next.center().y, "search, prev, next share row 1");
    assert_eq!(f32::from(follow.center().y - search.center().y), 36.0 + 8.0, "the toggles one row + $spacing.sm below");
    assert!(f32::from(search.size.width) > 240.0, "the search fills row 1: {search:?}");
    near(f32::from(stamps.right()), f32::from(next.right()), "both rows end at the toolbar's end");
    assert!(follow.right() <= stamps.left());
    near(f32::from(body.origin.y - search.origin.y), 2.0 * 36.0 + 8.0 + 8.0, "the body under two rows");
    near(f32::from(body.size.height), LOG_BODY_10, "the body keeps maxLines lines");
}

#[gpui::test]
fn a_section_row_folds_through_the_core(cx: &mut TestAppContext) {
    let (view, log, cx) = surface(cx, 900.0, log_view(json!({"follow": false})), None);
    let id = lines_id(&view, cx);
    click(&view, cx, &format!("{id}.section.1"), Modifiers::default());
    assert_eq!(log.named("toggled"), vec![json!({"section": 1, "open": false})]);
    let st = view.read_with(cx, |v, _| v.log_lines_state(&id)).unwrap();
    assert_eq!(st.rows, 2, "both steps folded");
    assert_eq!(st.max_scroll.1, 0.0);
    click(&view, cx, &format!("{id}.section.0"), Modifiers::default());
    assert_eq!(log.named("toggled").last(), Some(&json!({"section": 0, "open": true})));
    let st = view.read_with(cx, |v, _| v.log_lines_state(&id)).unwrap();
    assert_eq!(st.built, vec![LogRow::Section(0), LogRow::Line(1), LogRow::Section(1)]);
}

#[gpui::test]
fn search_marks_matches_and_steps_between_them(cx: &mut TestAppContext) {
    let (view, log, cx) = surface(cx, 900.0, log_view(json!({"follow": false, "query": "case 1"})), None);
    let id = lines_id(&view, cx);
    let st = view.read_with(cx, |v, _| v.log_lines_state(&id)).unwrap();
    // `case 1`, `case 10`…`case 19`, `case 101`…`case 199` (100 is the
    // failing line): 110 lines.
    assert_eq!((st.matches, st.current), (110, 0));
    log.clear();
    // Next steps to the second match and scrolls it into view.
    click(&view, cx, &format!("{id}.next"), Modifiers::default());
    let st = view.read_with(cx, |v, _| v.log_lines_state(&id)).unwrap();
    assert_eq!(st.current, 1);
    assert!(st.marked.iter().any(|(line, _, current)| *current && *line == 12), "{:?}", st.marked);
    // Previous twice wraps to the last match.
    click(&view, cx, &format!("{id}.prev"), Modifiers::default());
    click(&view, cx, &format!("{id}.prev"), Modifiers::default());
    let st = view.read_with(cx, |v, _| v.log_lines_state(&id)).unwrap();
    assert_eq!(st.current, 109);
    assert!(st.built.contains(&LogRow::Line(201)), "the last match (case 199, line 201) is in view: {:?}", st.built);
    // Typing into the field fires `search` and the count follows.
    let field = view.read_with(cx, |v, _| v.log_search_field(&id)).expect("the search field");
    cx.update(|window, cx| {
        use gpui::EntityInputHandler as _;
        field.update(cx, |s, cx| s.replace_text_in_range(Some(0..6), "store", window, cx));
    });
    settle(cx);
    assert!(log.named("searched").iter().any(|q| q == &json!({"query": "store"})), "{:?}", log.seen());
    let st = view.read_with(cx, |v, _| v.log_lines_state(&id)).unwrap();
    assert_eq!((st.matches, st.current), (1, 0));
    assert!(st.marked.iter().any(|(line, n, current)| *line == 102 && *n == 1 && *current), "{:?}", st.marked);
}

#[gpui::test]
fn gutter_numbers_anchor_lines_shift_extends_and_follow_turns_off(cx: &mut TestAppContext) {
    let (view, log, cx) = surface(cx, 900.0, log_view(json!({"lines": log_lines(6)})), None);
    let id = lines_id(&view, cx);
    click(&view, cx, &format!("{id}.gutter.3"), Modifiers::default());
    assert_eq!(log.named("selected"), vec![json!({"anchor": "L3", "start": 3, "end": 3})]);
    assert_eq!(log.named("followed"), vec![json!({"follow": false})], "a press on a number turns Follow off");
    click(&view, cx, &format!("{id}.gutter.6"), Modifiers::shift());
    assert_eq!(log.named("selected").last(), Some(&json!({"anchor": "L3-L6", "start": 3, "end": 6})));
}

#[gpui::test]
fn a_persons_scroll_up_turns_follow_off_and_the_toggle_jumps_back(cx: &mut TestAppContext) {
    let (view, log, cx) = surface(cx, 900.0, log_view(json!({})), None);
    let id = lines_id(&view, cx);
    view.update(cx, |v, cx| v.log_scroll_by(&id, 0.0, 200.0, cx));
    settle(cx);
    assert_eq!(log.named("followed"), vec![json!({"follow": false})]);
    let st = view.read_with(cx, |v, _| v.log_lines_state(&id)).unwrap();
    assert!(st.scroll.1 < st.max_scroll.1 - 100.0, "{st:?}");
    // Lines arriving no longer move it.
    click(&view, cx, &format!("{id}.follow"), Modifiers::default());
    assert_eq!(log.named("followed").last(), Some(&json!({"follow": true})));
    let st = view.read_with(cx, |v, _| v.log_lines_state(&id)).unwrap();
    assert!((st.scroll.1 - st.max_scroll.1).abs() < 0.5, "back at the end: {st:?}");
    click(&view, cx, &format!("{id}.timestamps"), Modifiers::default());
    assert_eq!(log.named("stamped"), vec![json!({"timestamps": true})]);
}

#[gpui::test]
fn a_selection_from_outside_opens_its_step_lands_in_the_top_third_and_stops_follow(cx: &mut TestAppContext) {
    // Line 150 sits in the failed step; fold it first (open: [0]).
    let (view, log, cx) = surface(cx, 900.0, log_view(json!({"open": [0], "selection": "L150"})), None);
    let id = lines_id(&view, cx);
    settle(cx);
    assert_eq!(log.named("followed"), vec![json!({"follow": false})]);
    let st = view.read_with(cx, |v, _| v.log_lines_state(&id)).unwrap();
    assert!(st.built.contains(&LogRow::Line(150)), "revealed: {:?}", st.built);
    assert!(st.scroll.1 > 0.0 && st.scroll.1 < st.max_scroll.1, "{st:?}");
}

fn jobs() -> Value {
    json!([
        {"id": "checkout", "name": "checkout", "status": "passed", "duration": "4s"},
        {"id": "build", "name": "build", "status": "passed", "duration": "1m 2s", "needs": ["checkout"]},
        {"id": "lint", "name": "lint", "status": "passed", "duration": "21s", "needs": ["checkout"]},
        {"id": "test", "name": "test", "status": "running", "needs": ["build"]},
        {"id": "publish", "name": "publish", "status": "cancelled", "needs": ["test", "lint"]}
    ])
}

fn job_graph(extra: Value) -> Value {
    let mut props = json!({"jobs": jobs()});
    for (k, v) in extra.as_object().unwrap() {
        props[k] = v.clone();
    }
    json!({"id": "jobs", "component": "JobGraph", "props": props, "on": {"select": {"event": {"name": "picked"}}}})
}

#[gpui::test]
fn job_graph_draws_connectors_only_for_the_hovered_nodes_upstream(cx: &mut TestAppContext) {
    let (view, log, cx) = surface(cx, 1200.0, job_graph(json!({})), None);
    let st = view.read_with(cx, |v, _| v.job_graph_state("jobs")).expect("painted");
    assert_eq!(st.mode, "graph");
    assert_eq!(st.nodes, vec!["checkout", "build", "lint", "test", "publish"]);
    assert_eq!(st.edges, 0, "no connector without a hover");
    click(&view, cx, "jobs.node.lint", Modifiers::default());
    assert_eq!(log.named("picked"), vec![json!({"id": "lint"})]);
    let at = bounds(&view, cx, "jobs.node.test").center();
    cx.simulate_mouse_move(at, None, Modifiers::default());
    settle(cx);
    cx.simulate_mouse_move(at + gpui::point(px(1.0), px(1.0)), None, Modifiers::default());
    settle(cx);
    let st = view.read_with(cx, |v, _| v.job_graph_state("jobs")).unwrap();
    let leaf = view.read_with(cx, |v, _| v.painted_bounds("jobs"));
    assert_eq!(st.hovered.as_deref(), Some("test"), "{st:?} at {at:?}, the leaf {leaf:?}, the node {:?}", bounds(&view, cx, "jobs.node.test"));
    assert_eq!(st.lit, vec!["checkout", "build", "test"]);
    assert_eq!(st.edges, 2, "checkout→build, build→test");
    let at = bounds(&view, cx, "jobs.node.publish").center();
    cx.simulate_mouse_move(at, None, Modifiers::default());
    settle(cx);
    let st = view.read_with(cx, |v, _| v.job_graph_state("jobs")).unwrap();
    assert_eq!(st.edges, 5, "every edge leads to publish");
    // Off the graph: nothing lit.
    cx.simulate_mouse_move(gpui::point(px(5.0), px(5.0)), None, Modifiers::default());
    settle(cx);
    assert_eq!(view.read_with(cx, |v, _| v.job_graph_state("jobs")).unwrap().edges, 0);
    let info = view.read_with(cx, |v, _| v.accessible_info("jobs")).expect("a11y");
    assert_eq!(info.role, gpui::Role::Group);
}

#[gpui::test]
fn job_graph_is_a_status_list_below_md(cx: &mut TestAppContext) {
    let (view, log, cx) = surface(cx, 420.0, job_graph(json!({})), None);
    let st = view.read_with(cx, |v, _| v.job_graph_state("jobs")).expect("painted");
    assert_eq!(st.mode, "list");
    let row = bounds(&view, cx, "jobs.row.build");
    // (the probe sits inside the row's hairline bottom border)
    assert!((39.0..=40.0).contains(&f32::from(row.size.height)), "{row:?}");
    let first = bounds(&view, cx, "jobs.row.checkout");
    assert_eq!(f32::from(row.origin.y - first.origin.y), 40.0, "rows in column order");
    assert!(f32::from(row.size.width) > 300.0, "full width");
    click(&view, cx, "jobs.row.build", Modifiers::default());
    assert_eq!(log.named("picked"), vec![json!({"id": "build"})]);
}

#[gpui::test]
fn job_graph_is_one_tab_stop_its_nodes_move_with_tab_and_enter_selects(cx: &mut TestAppContext) {
    let (view, log, cx) = surface(cx, 1200.0, job_graph(json!({})), None);
    let first = cx.update(|window, cx| view.update(cx, |v, cx| v.focus_next(false, window, cx)));
    let first = view.read_with(cx, |v, _| first.and_then(|i| v.surface().layout_node(i).map(|n| n.id.clone())));
    assert_eq!(first.as_deref(), Some("jobs"));
    settle(cx);
    let st = view.read_with(cx, |v, _| v.job_graph_state("jobs")).unwrap();
    assert_eq!(st.keyboard.as_deref(), Some("checkout"));
    assert_eq!(st.lit, vec!["checkout"], "the focused node is lit like a hovered one");
    let handled = cx.update(|window, cx| view.update(cx, |v, cx| v.handle_key("tab", false, None, window, cx)));
    assert!(handled);
    settle(cx);
    assert_eq!(view.read_with(cx, |v, _| v.job_graph_state("jobs")).unwrap().keyboard.as_deref(), Some("build"));
    cx.update(|window, cx| view.update(cx, |v, cx| v.handle_key("enter", false, None, window, cx)));
    settle(cx);
    assert_eq!(log.named("picked"), vec![json!({"id": "build"})]);
    // Tab past the last node leaves the graph.
    for _ in 0..3 {
        cx.update(|window, cx| view.update(cx, |v, cx| v.handle_key("tab", false, None, window, cx)));
    }
    cx.update(|window, cx| view.update(cx, |v, cx| v.handle_key("tab", false, None, window, cx)));
    settle(cx);
    let focused = view.read_with(cx, |v, _| v.leaf_states("after"));
    assert!(focused.iter().any(|s| s == "focus"), "{focused:?}");
}

const TRAIN: &str = "import { runChecks } from \"./checks\"\nimport type { Pull } from \"./types\"\n\nexport function compose(pulls: Pull[], max = 4) {\n  return pulls\n}\n";

#[gpui::test]
fn code_block_anchors_select_lines_and_shift_extends(cx: &mut TestAppContext) {
    let block = json!({"id": "code", "component": "CodeBlock", "props": {"code": TRAIN, "language": "ts", "anchors": true, "selection": {"@path": "/sel"}}, "on": {"select": {"event": {"name": "anchor"}}}});
    let (view, log, cx) = surface(cx, 900.0, block, Some(json!({"sel": "L2"})));
    let numbers: Vec<(String, u64)> = view.read_with(cx, |v, _| v.placed_nodes().iter().filter(|n| n.owner.as_deref() == Some("code") && n.part.as_deref() == Some("lineNumber")).map(|n| (n.id.clone(), n.props["line"].as_u64().unwrap())).collect());
    assert_eq!(numbers.len(), 7, "anchors show the numbers");
    let selected: Vec<u64> = view.read_with(cx, |v, _| v.placed_nodes().iter().filter(|n| n.owner.as_deref() == Some("code") && n.part.as_deref() == Some("line") && n.props.get("selected") == Some(&json!(true))).map(|n| n.props["line"].as_u64().unwrap()).collect());
    assert_eq!(selected, vec![2]);
    let four = numbers.iter().find(|(_, l)| *l == 4).unwrap().0.clone();
    click(&view, cx, &four, Modifiers::shift());
    assert_eq!(log.named("anchor"), vec![json!({"anchor": "L2-L4", "start": 2, "end": 4})]);
    let one = numbers.iter().find(|(_, l)| *l == 1).unwrap().0.clone();
    click(&view, cx, &one, Modifiers::default());
    assert_eq!(log.named("anchor").last(), Some(&json!({"anchor": "L1", "start": 1, "end": 1})));
    let info = view.read_with(cx, |v, _| v.accessible_info(&one)).expect("a11y");
    assert_eq!(info.role, gpui::Role::Link);
    assert_eq!(info.label.map(|s| s.to_string()).as_deref(), Some("Line 1"));
}

#[gpui::test]
fn code_block_blame_names_each_block_once_with_a_hairline_between(cx: &mut TestAppContext) {
    let blame = json!([{"start": 1, "author": "Ada Lovelace", "age": "3 weeks ago"}, {"start": 4, "author": "Grace Hopper", "age": "2 days ago"}]);
    let block = json!({"id": "code", "component": "CodeBlock", "props": {"code": TRAIN, "language": "ts", "lineNumbers": true, "gutter": "blame", "blame": blame}});
    let (view, _log, cx) = surface(cx, 900.0, block, None);
    let authors: Vec<String> = view.read_with(cx, |v, _| v.placed_nodes().iter().filter(|n| n.owner.as_deref() == Some("code") && n.part.as_deref() == Some("blameAuthor")).map(|n| n.props["text"].as_str().unwrap().to_string()).collect());
    assert_eq!(authors, vec!["Ada Lovelace", "Grace Hopper"]);
    let col = view.read_with(cx, |v, _| v.placed_nodes().iter().find(|n| n.owner.as_deref() == Some("code") && n.part.as_deref() == Some("blame")).map(|n| n.id.clone())).unwrap();
    assert_eq!(f32::from(bounds(&view, cx, &col).size.width), 200.0, "the blame column");
    let border_row = view.read_with(cx, |v, _| v.placed_nodes().iter().find(|n| n.owner.as_deref() == Some("code") && n.part.as_deref() == Some("line") && n.props.get("blameBorder") == Some(&json!(true))).map(|n| (n.id.clone(), n.props["line"].clone()))).unwrap();
    assert_eq!(border_row.1, json!(4));
}

#[gpui::test]
fn a_long_self_scrolling_code_block_builds_only_its_window(cx: &mut TestAppContext) {
    let code: Vec<String> = (0..300).map(|i| format!("const line{i} = {i}")).collect();
    let block = json!({"id": "code", "component": "CodeBlock", "props": {"code": code.join("\n"), "language": "ts", "lineNumbers": true, "maxLines": 10, "anchors": true, "selection": "L200"}});
    let (view, _log, cx) = surface(cx, 900.0, block, None);
    settle(cx);
    let built = view.read_with(cx, |v, _| v.code_block_built_lines("code")).unwrap();
    assert!(built < 40, "{built} of 300 lines built");
    // The selection scrolled into view: line 200's row is built.
    let row = view.read_with(cx, |v, _| v.placed_nodes().iter().find(|n| n.owner.as_deref() == Some("code") && n.part.as_deref() == Some("line") && n.props["line"] == json!(200)).map(|n| n.id.clone())).unwrap();
    assert!(view.read_with(cx, |v, _| v.painted_bounds(&row)).is_some(), "line 200 painted");
}

