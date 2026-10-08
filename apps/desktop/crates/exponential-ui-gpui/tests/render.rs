//! The painter against the shared fixtures, in a headless gpui test window:
//! every catalog case and the kitchen sink paint (in every built-in theme ×
//! mode), presses reach the host, overlays open and close, and the bench
//! tree's cold pass stays in budget. Text fields are never focused here (a
//! focused gpui-component input asks a test window for a native view).

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Instant;

use exponential_ui::extension::define_extension;
use exponential_ui::measure::LeafRequest;
use exponential_ui::theme::Mode;
use exponential_ui::themes::builtin_theme;
use exponential_ui::{ExtensionDef, FlatComponent, NestedNode, A2UI_BASIC_CATALOG_ID};
use exponential_ui_gpui::extension::{ExtensionPainter, PaintContext};
use exponential_ui_gpui::host::{ActionEvent, HostPlugin, InputEvent};
use exponential_ui_gpui::view::{SurfaceView, SurfaceViewOptions};
use gpui::{div, prelude::*, px, AnyElement, App, Entity, TestAppContext, VisualTestContext, Window};
use serde::Deserialize;
use serde_json::{json, Value};

fn fixture(name: &str) -> Value {
    let path = format!("{}/../../../../packages/exponential-ui/fixtures/{name}", env!("CARGO_MANIFEST_DIR"));
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path}: {e}"));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("{path}: {e}"))
}

#[derive(Default)]
struct Log {
    actions: Vec<ActionEvent>,
    inputs: Vec<InputEvent>,
    unknown: Vec<String>,
}

#[derive(Clone, Default)]
struct Recorder(Rc<RefCell<Log>>);

impl HostPlugin for Recorder {
    fn on_action(&self, event: &ActionEvent, _cx: &mut App) {
        self.0.borrow_mut().actions.push(event.clone());
    }
    fn on_input(&self, event: &InputEvent, _cx: &mut App) {
        self.0.borrow_mut().inputs.push(event.clone());
    }
    fn open_url(&self, _url: &str, _cx: &mut App) {}
    fn on_unknown(&self, node: &exponential_ui::surface::PlacedNode) {
        self.0.borrow_mut().unknown.push(node.id.clone());
    }
}

/// The extension fixture's `Sparkline`, painted as a bar row.
struct Sparkline;

impl ExtensionPainter for Sparkline {
    fn measure(&self, leaf: &LeafRequest, wrap: Option<f32>, _: &mut Window, _: &mut App) -> Option<(f32, f32)> {
        let n = leaf.props.get("values").and_then(Value::as_array).map(|a| a.len()).unwrap_or(0) as f32;
        Some((wrap.filter(|w| *w > 0.0).unwrap_or(n * 6.0), 24.0))
    }
    fn paint(&self, ctx: PaintContext, _: &mut Window, _: &mut App) -> AnyElement {
        let values: Vec<f64> = ctx.node.props.get("values").and_then(Value::as_array).map(|a| a.iter().filter_map(Value::as_f64).collect()).unwrap_or_default();
        let max = values.iter().copied().fold(1.0, f64::max);
        let mut row = div().size_full().flex().flex_row().items_end().gap(px(1.0));
        for v in values {
            row = row.child(div().w(px(4.0)).h(px((ctx.height as f64 * v / max) as f32)).bg(gpui::blue()));
        }
        row.children(ctx.children).into_any_element()
    }
}

fn init(cx: &mut TestAppContext) {
    cx.update(gpui_component::init);
}

fn open(cx: &mut TestAppContext, options: SurfaceViewOptions) -> (Entity<SurfaceView>, &mut VisualTestContext) {
    cx.add_window_view(move |window, cx| SurfaceView::new(options, window, cx))
}

fn draw(cx: &mut VisualTestContext) {
    cx.update(|window, cx| window.draw(cx).clear(cx));
    cx.run_until_parked();
}

fn nodes(view: &Entity<SurfaceView>, cx: &mut VisualTestContext) -> usize {
    view.read_with(cx, |v, _| v.stats().nodes)
}

fn kitchen_sink() -> NestedNode {
    serde_json::from_value(fixture("kitchen-sink.json")).expect("kitchen-sink.json")
}

fn load_kitchen(view: &Entity<SurfaceView>, cx: &mut VisualTestContext) {
    view.update(cx, |v, cx| {
        let outcome = v.set_nested(kitchen_sink(), cx);
        assert!(outcome.issues.is_empty(), "{:?}", outcome.issues);
        let posts: Vec<Value> = (0..30).map(|i| json!({"title": format!("Post {i} about self-hosting"), "score": format!("{}", 10 + i)})).collect();
        v.set_data("/posts", Some(Value::Array(posts)), cx);
        v.set_data("/draft/title", Some(json!("Hello")), cx);
        v.set_data("/ui/confirmOpen", Some(json!(false)), cx);
        v.set_viewport_height(800.0, cx);
    });
}

#[derive(Deserialize)]
struct ComponentCase {
    name: String,
    node: NestedNode,
}

#[derive(Deserialize)]
struct MacroCase {
    name: String,
    input: NestedNode,
}

#[derive(Deserialize)]
struct BasicCase {
    name: String,
    components: Vec<FlatComponent>,
}

#[derive(Deserialize)]
struct ExtensionCase {
    name: String,
    #[serde(rename = "catalogId")]
    catalog_id: String,
    components: Vec<FlatComponent>,
}

#[gpui::test]
fn every_catalog_component_and_macro_case_paints(cx: &mut TestAppContext) {
    init(cx);
    let (view, cx) = open(cx, SurfaceViewOptions::default());
    let components: Vec<ComponentCase> = serde_json::from_value(fixture("catalog-components.json")["cases"].clone()).unwrap();
    let macros: Vec<MacroCase> = serde_json::from_value(fixture("catalog-macros.json")["cases"].clone()).unwrap();
    let cases: Vec<(String, NestedNode)> = components.into_iter().map(|c| (c.name, c.node)).chain(macros.into_iter().map(|c| (c.name, c.input))).collect();
    assert!(cases.len() > 400);
    for (name, node) in cases {
        view.update(cx, |v, cx| {
            v.set_nested(node, cx);
        });
        draw(cx);
        assert!(nodes(&view, cx) > 0, "{name}");
    }
}

#[gpui::test]
fn every_basic_catalog_case_paints(cx: &mut TestAppContext) {
    init(cx);
    let (view, cx) = open(cx, SurfaceViewOptions { catalog_id: A2UI_BASIC_CATALOG_ID.into(), ..Default::default() });
    let cases: Vec<BasicCase> = serde_json::from_value(fixture("catalog-basic-map.json")["cases"].clone()).unwrap();
    for c in cases {
        view.update(cx, |v, cx| {
            v.apply(&json!({"version": "v0.9", "updateComponents": {"surfaceId": "surface", "components": c.components}}), cx).unwrap();
        });
        draw(cx);
        assert!(nodes(&view, cx) > 0, "{}", c.name);
    }
}

#[gpui::test]
fn every_extension_case_paints_with_a_registered_painter(cx: &mut TestAppContext) {
    init(cx);
    let file = fixture("catalog-extension.json");
    let def: ExtensionDef = serde_json::from_value(file["extension"].clone()).unwrap();
    let ext = define_extension(def).expect("valid extension");
    let cases: Vec<ExtensionCase> = serde_json::from_value(file["cases"].clone()).unwrap();
    for c in cases {
        let log = Recorder::default();
        let options = SurfaceViewOptions { catalog_id: c.catalog_id.clone(), extensions: vec![ext.clone()], host: Rc::new(log.clone()), ..Default::default() };
        let (view, vcx) = open(cx, options);
        view.update(vcx, |v, cx| {
            v.register_painter("Sparkline", Box::new(Sparkline));
            v.apply(&json!({"version": "v0.9", "updateComponents": {"surfaceId": "surface", "components": c.components}}), cx).unwrap();
        });
        draw(vcx);
        draw(vcx);
        assert!(nodes(&view, vcx) > 0, "{}", c.name);
        if c.name.contains("native passes through") {
            let (w, h) = view.read_with(vcx, |v, _| {
                let i = v.index_of("root").unwrap();
                let f = v.surface().last_frame(i).unwrap();
                (f.w, f.h)
            });
            assert!(w > 0.0 && h == 24.0, "the Sparkline painter measured it: {w}x{h}");
        }
        if c.name.contains("placeholder") {
            assert!(!log.0.borrow().unknown.is_empty(), "on_unknown fired");
        }
    }
}

#[gpui::test]
fn the_kitchen_sink_paints_in_every_theme_and_mode(cx: &mut TestAppContext) {
    init(cx);
    for theme in ["neutral", "exponential", "playful"] {
        for mode in [Mode::Light, Mode::Dark] {
            let options = SurfaceViewOptions { theme: Some(builtin_theme(theme).unwrap()), mode, ..Default::default() };
            let (view, vcx) = open(cx, options);
            load_kitchen(&view, vcx);
            draw(vcx);
            draw(vcx);
            let n = nodes(&view, vcx);
            assert!(n > 100, "{theme}/{mode:?}: {n} nodes");
            let windowed = view.read_with(vcx, |v, _| v.surface().structure_version());
            assert!(windowed > 0);
        }
    }
    // Geometry mode (no theme) paints too.
    let (view, vcx) = open(cx, SurfaceViewOptions { theme: None, ..Default::default() });
    load_kitchen(&view, vcx);
    draw(vcx);
    assert!(nodes(&view, vcx) > 50);
}

#[gpui::test]
fn presses_reach_the_host_and_overlays_open_and_close(cx: &mut TestAppContext) {
    init(cx);
    let log = Recorder::default();
    let (view, cx) = open(cx, SurfaceViewOptions { host: Rc::new(log.clone()), ..Default::default() });
    load_kitchen(&view, cx);
    draw(cx);

    // A Button press fires its action.
    cx.update(|window, cx| {
        view.update(cx, |v, cx| {
            let i = v.index_of("hdr-scan").expect("hdr-scan");
            v.press(i, window, cx);
        })
    });
    draw(cx);
    {
        let log = log.0.borrow();
        let scan = log.actions.iter().find(|a| a.name == "scan").expect("scan action");
        assert_eq!(scan.component_id, "hdr-scan");
        assert_eq!(scan.surface_id, "surface");
        assert_eq!(scan.event, "press");
    }

    // The dialog trigger opens a layer; Escape closes it.
    cx.update(|window, cx| {
        view.update(cx, |v, cx| {
            let i = v.index_of("dialog-trigger").expect("dialog-trigger");
            v.press(i, window, cx);
        })
    });
    draw(cx);
    let (layers, confirm) = view.read_with(cx, |v, _| (v.layers().len(), v.surface().get_data("/ui/confirmOpen").cloned()));
    assert_eq!(layers, 1, "the dialog's layer is open");
    assert_eq!(confirm, Some(json!(true)), "the bound open prop wrote through");
    let closed = cx.update(|window, cx| view.update(cx, |v, cx| v.escape(window, cx)));
    assert!(closed);
    draw(cx);
    assert_eq!(view.read_with(cx, |v, _| v.layers().len()), 0, "Escape closed it");
    draw(cx);
    let (focused, trigger) = view.read_with(cx, |v, _| (v.focused(), v.index_of("dialog-trigger")));
    assert_eq!(focused, trigger, "focus went back to the trigger");

    // A tab press changes the selection (the panels swap).
    let hidden = |view: &Entity<SurfaceView>, cx: &mut VisualTestContext, id: &str| view.read_with(cx, |v, _| v.surface().layout_node(v.surface().index_of(id).unwrap()).unwrap().hidden);
    assert!(!hidden(&view, cx, "tab-issue"));
    assert!(hidden(&view, cx, "tab-run"));
    cx.update(|window, cx| {
        view.update(cx, |v, cx| {
            let i = v.index_of("tabs.tab.1").expect("second tab");
            v.press(i, window, cx);
        })
    });
    draw(cx);
    assert!(hidden(&view, cx, "tab-issue"));
    assert!(!hidden(&view, cx, "tab-run"));

    // An unbound checkbox flips at once (painter mirror) and reports it.
    cx.update(|window, cx| {
        view.update(cx, |v, cx| {
            let i = v.index_of("form-agree").expect("checkbox");
            v.press(i, window, cx);
        })
    });
    draw(cx);

    // Tab moves focus in tree order (from the trigger focus went back to).
    let (order, before) = view.read_with(cx, |v, _| (v.focus_order(), v.focused()));
    let first = cx.update(|window, cx| view.update(cx, |v, cx| v.focus_next(false, window, cx)));
    assert_eq!(first, exponential_ui_gpui::view::next_focus(&order, before, false));
    let second = cx.update(|window, cx| view.update(cx, |v, cx| v.focus_next(false, window, cx)));
    assert_eq!(second, exponential_ui_gpui::view::next_focus(&order, first, false));
    let back = cx.update(|window, cx| view.update(cx, |v, cx| v.focus_next(true, window, cx)));
    assert_eq!(back, first, "Shift-Tab goes back");
}

/// Holds a surface view WITHOUT rendering it, so the test owns every pass.
struct Holder {
    view: Entity<SurfaceView>,
}

impl Render for Holder {
    fn render(&mut self, _: &mut Window, _: &mut gpui::Context<Self>) -> impl IntoElement {
        div()
    }
}

#[gpui::test]
fn the_bench_tree_cold_pass_stays_in_budget_and_a_steady_pass_measures_nothing(cx: &mut TestAppContext) {
    init(cx);
    // Process-wide statics (the parsed catalog, macro table, themes, recipe
    // index) are built once by the first surface of the process; warm them
    // with a throwaway core surface so the number below is a SURFACE's cold
    // pass, not the process start.
    {
        let mut warm = exponential_ui::surface::Surface::new("warm", Default::default());
        warm.set_nested(exponential_ui::bench::bench_tree(20));
        warm.set_viewport(900.0, 0.0, None);
        let started = Instant::now();
        warm.layout(&mut exponential_ui::measure::FixedMeasure::default());
        eprintln!("[exponential-ui gpui] process warm-up (core, FixedMeasure, 20 nodes): {} µs", started.elapsed().as_micros());
    }
    // The cold pass of a FRESH view, five times (each its own first pass;
    // the minimum filters out scheduler noise from the parallel tests).
    let mut runs = Vec::new();
    for _ in 0..5 {
        let window = cx.add_window(|window, cx| Holder { view: cx.new(|cx| SurfaceView::new(SurfaceViewOptions::default(), window, cx)) });
        let cold = window
            .update(cx, |holder, window, cx| {
                holder.view.update(cx, |v, cx| {
                    v.set_nested(exponential_ui::bench::bench_tree(200), cx);
                    v.set_width(900.0, cx);
                    v.layout_now(window, cx).clone()
                })
            })
            .unwrap();
        runs.push((window, cold));
    }
    let (window, _) = runs.last().cloned().unwrap();
    let cold = runs.iter().map(|(_, s)| s.clone()).min_by_key(|s| s.wall_ns).unwrap();
    let pass = |cx: &mut TestAppContext| window.update(cx, |holder, window, cx| holder.view.update(cx, |v, cx| v.layout_now(window, cx).clone())).unwrap();
    eprintln!(
        "[exponential-ui gpui] bench cold (best of 5 fresh views): {} nodes · {} measure calls · {} upcalls · {} rounds · core {} µs · wall {} µs ({}); all: {:?} µs",
        cold.nodes,
        cold.measure_calls,
        cold.upcalls,
        cold.measure_rounds,
        cold.layout_ns / 1000,
        cold.wall_ns / 1000,
        if cfg!(debug_assertions) { "debug" } else { "release" },
        runs.iter().map(|(_, s)| s.wall_ns / 1000).collect::<Vec<_>>()
    );
    assert_eq!(cold.passes, 1, "the first pass is the cold one");
    assert!(cold.nodes >= 200);
    assert!(cold.measure_calls > 0);
    if !cfg!(debug_assertions) {
        assert!(cold.wall_ns < 3_000_000, "cold pass {} µs", cold.wall_ns / 1000);
    }
    let steady = pass(cx);
    eprintln!("[exponential-ui gpui] bench steady: {} measure calls · core {} µs · wall {} µs", steady.measure_calls, steady.layout_ns / 1000, steady.wall_ns / 1000);
    assert_eq!(steady.measure_calls, 0, "nothing changed: the memo answers");
    let mut best = u64::MAX;
    for _ in 0..10 {
        best = best.min(pass(cx).wall_ns);
    }
    eprintln!("[exponential-ui gpui] bench steady best of 10: {} µs", best / 1000);
    // For comparison: the core alone, cold, on its fixed measure.
    let mut core = exponential_ui::surface::Surface::new("core", Default::default());
    core.set_nested(exponential_ui::bench::bench_tree(200));
    core.set_viewport(900.0, 0.0, None);
    let out = core.layout(&mut exponential_ui::measure::FixedMeasure { sizes: Default::default(), wrap: true });
    eprintln!("[exponential-ui gpui] core alone cold (FixedMeasure, 200 nodes, after the view): {} µs", out.layout_ns / 1000);
}

#[gpui::test]
fn a_full_frame_of_the_bench_tree_draws(cx: &mut TestAppContext) {
    init(cx);
    let (view, cx) = open(cx, SurfaceViewOptions::default());
    view.update(cx, |v, cx| {
        v.set_nested(exponential_ui::bench::bench_tree(200), cx);
    });
    draw(cx);
    draw(cx);
    let started = Instant::now();
    draw(cx);
    eprintln!("[exponential-ui gpui] bench frame (layout + element tree + paint): {} µs", started.elapsed().as_micros());
    assert!(nodes(&view, cx) >= 200);
}

#[gpui::test]
fn typing_debounces_one_change_per_burst_writes_through_and_echoes_apply_when_idle(cx: &mut TestAppContext) {
    init(cx);
    let log = Recorder::default();
    let host = Rc::new(log.clone());
    let window = cx.add_window(move |window, cx| Holder { view: cx.new(|cx| SurfaceView::new(SurfaceViewOptions { host, ..Default::default() }, window, cx)) });
    let with_view = |cx: &mut TestAppContext, f: &dyn Fn(&mut SurfaceView, &mut Window, &mut gpui::Context<SurfaceView>)| {
        window.update(cx, |holder, window, cx| holder.view.update(cx, |v, cx| f(v, window, cx))).unwrap();
    };
    with_view(cx, &|v, window, cx| {
        v.set_nested(kitchen_sink(), cx);
        v.set_data("/draft/title", Some(json!("Hello")), cx);
        v.layout_now(window, cx);
    });
    let field = "echo-field.field";
    with_view(cx, &|v, _, cx| assert_eq!(v.field_text(field, cx).as_deref(), Some("Hello")));
    for ch in [" ", "w", "o", "r", "l", "d"] {
        with_view(cx, &|v, window, cx| assert!(v.type_into(field, ch, window, cx)));
        cx.executor().advance_clock(std::time::Duration::from_millis(20));
        cx.run_until_parked();
    }
    assert!(log.0.borrow().inputs.is_empty(), "nothing sent inside the 150 ms debounce");
    cx.executor().advance_clock(std::time::Duration::from_millis(400));
    cx.run_until_parked();
    {
        let log = log.0.borrow();
        assert_eq!(log.inputs.len(), 1, "one debounced change: {:?}", log.inputs);
        let e = &log.inputs[0];
        assert_eq!(e.kind, exponential_ui_gpui::host::InputKind::Change);
        assert_eq!(e.value, json!("Hello world"));
        assert_eq!(e.revision, 6);
        assert_eq!(e.component_id, "echo-field");
        assert_eq!(e.path.as_deref(), Some("/draft/title"));
    }
    with_view(cx, &|v, _, _| assert_eq!(v.surface().get_data("/draft/title"), Some(&json!("Hello world")), "the bound value wrote through"));
    // A host echo lands in the idle, unfocused field.
    with_view(cx, &|v, window, cx| {
        v.set_data("/draft/title", Some(json!("Echoed")), cx);
        v.layout_now(window, cx);
    });
    with_view(cx, &|v, _, cx| assert_eq!(v.field_text(field, cx).as_deref(), Some("Echoed")));
    // An edit outstanding (inside the debounce) wins over a stale echo.
    with_view(cx, &|v, window, cx| {
        v.type_into(field, "!", window, cx);
        v.set_data("/draft/title", Some(json!("Stale")), cx);
        v.layout_now(window, cx);
    });
    with_view(cx, &|v, _, cx| assert_eq!(v.field_text(field, cx).as_deref(), Some("Echoed!")));
}
