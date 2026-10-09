//! VAPP-91: the gpui painter's Exponential UI conformance runner. Every suite
//! of `packages/exponential-ui/conformance/manifest.json`, counted exactly as
//! the manifest's `unit` says. The painted suites go THROUGH the painter: a
//! `SurfaceView` in a headless `#[gpui::test]` window, measured by
//! `GpuiMeasure` (catalog, control-geometry, replay) or by the fixture's fixed
//! measure (layout, overlay); the pure suites through the core this painter
//! links directly (no FFI). Writes the report to
//! `$EXPONENTIAL_UI_CONFORMANCE_REPORT`, default
//! `<repo>/.conformance/exponential-ui-gpui.json`; check it with
//! `bun run --filter @exponential-at/ui conformance:check <report>`.
//!
//! The headless window shapes text with gpui's NoopTextSystem: text widths
//! are not CoreText's, so control-geometry checks the BORDER BOX the
//! measurer reports from the recipe's `ControlBox` (fixed / minimum sizes,
//! padding, border) and the painted part visuals, not shaped label widths.

use std::collections::{BTreeMap, HashMap};
use std::panic::{catch_unwind, AssertUnwindSafe};

use exponential_ui::catalog::{component_def, CatalogView};
use exponential_ui::extension::define_extension;
use exponential_ui::host::{
    client_capabilities, combine_decisions, decide_function, decide_url, mcp_action_call, media_request, messages_from_mcp_result, parse_source,
    supported_catalog_ids, validate_package, Decoded, FunctionDecision, FunctionPolicy, HostRouter, JsonlDecoder, MediaOptions, SseDecoder, UrlPolicy,
};
use exponential_ui::json;
use exponential_ui::measure::FixedMeasure;
use exponential_ui::overlay::{place_overlay, OverlaySide, PlaceOptions, Rect, Size};
use exponential_ui::reducer::{reduce_nested, reduce_surface, ReduceOptions};
use exponential_ui::theme::{load_theme, resolve_recipe, try_load_theme, Mode, RecipeQuery, ThemeOptions};
use exponential_ui::themes::{builtin_refs, builtin_theme, BUILTIN_THEME_IDS};
use exponential_ui::types::{ExtensionDef, FlatComponent, NestedNode};
use exponential_ui::{Props, A2UI_BASIC_CATALOG_ID, CORE_CATALOG_ID, UNKNOWN_COMPONENT};
use exponential_ui_gpui::paint::parts::{part_props, part_visual, px_prop};
use exponential_ui_gpui::view::{SurfaceView, SurfaceViewOptions};
use gpui::{div, prelude::*, px, Entity, TestAppContext, VisualTestContext, Window};
use serde_json::{json, Value};

/// The round-1 bind/style/code and round-2 contract suites (manifest v2):
/// the cases the core replays, shared case for case (this painter links the
/// core directly, so its answers ARE the core's).
#[path = "../../exponential-ui/tests/support/round2.rs"]
#[allow(dead_code)]
mod round2;

const MODES: [Mode; 2] = [Mode::Light, Mode::Dark];
const PKG: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../../../packages/exponential-ui");

type Case = (String, Result<(), String>);

fn read(rel: &str) -> Value {
    let path = format!("{PKG}/{rel}");
    serde_json::from_str(&std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path}: {e}"))).unwrap_or_else(|e| panic!("{path}: {e}"))
}

fn fixture(name: &str) -> Value {
    read(&format!("fixtures/{name}"))
}

fn to_value<T: serde::Serialize>(v: &T) -> Value {
    serde_json::to_value(v).expect("serializable")
}

fn same(got: &Value, expected: &Value) -> Result<(), String> {
    if json::equal(got, expected) {
        Ok(())
    } else {
        let (g, e) = (got.to_string(), expected.to_string());
        Err(format!("got {} / expected {}", &g[..g.len().min(300)], &e[..e.len().min(300)]))
    }
}

fn ensure(ok: bool, msg: impl FnOnce() -> String) -> Result<(), String> {
    if ok {
        Ok(())
    } else {
        Err(msg())
    }
}

fn obj(v: &Value) -> Props {
    v.as_object().cloned().unwrap_or_default()
}

fn strings(v: Option<&Value>) -> Vec<String> {
    v.and_then(Value::as_array).map(|a| a.iter().filter_map(|s| s.as_str().map(str::to_string)).collect()).unwrap_or_default()
}

fn list<'a>(f: &'a Value, key: &str) -> &'a Vec<Value> {
    f[key].as_array().unwrap_or_else(|| panic!("{key}"))
}

/// One case: a panic is a failure, never the end of the run.
fn case(name: impl Into<String>, f: impl FnOnce() -> Result<(), String>) -> Case {
    let result = catch_unwind(AssertUnwindSafe(f)).unwrap_or_else(|p| {
        let msg = p.downcast_ref::<String>().cloned().or_else(|| p.downcast_ref::<&str>().map(|s| s.to_string())).unwrap_or_else(|| "panicked".into());
        Err(format!("panic: {msg}"))
    });
    (name.into(), result)
}

// --- the painting harness ----------------------------------------------------

/// The window's root: the surface under test at a fixed width.
struct Holder {
    view: Option<Entity<SurfaceView>>,
    width: f32,
}

impl Render for Holder {
    fn render(&mut self, _: &mut Window, _: &mut gpui::Context<Self>) -> impl IntoElement {
        div().w(px(self.width)).children(self.view.clone())
    }
}

struct Painter<'a> {
    holder: Entity<Holder>,
    cx: &'a mut VisualTestContext,
}

impl Painter<'_> {
    /// A fresh view at `width`, set up, painted (twice: the width probe
    /// reports one frame late) with its paint order traced.
    fn paint(&mut self, options: SurfaceViewOptions, width: f32, setup: impl FnOnce(&mut SurfaceView, &mut gpui::Context<SurfaceView>)) -> Entity<SurfaceView> {
        let view = self.cx.update(|window, cx| cx.new(|cx| SurfaceView::new(options, window, cx)));
        view.update(self.cx, |v, cx| {
            v.trace_paint(true);
            v.set_width(width, cx);
            setup(v, cx);
        });
        let v = view.clone();
        self.holder.update(self.cx, |h, cx| {
            h.view = Some(v);
            h.width = width;
            cx.notify();
        });
        for _ in 0..2 {
            self.cx.update(|window, cx| window.draw(cx).clear(cx));
            self.cx.run_until_parked();
        }
        view
    }

    fn read<R>(&mut self, view: &Entity<SurfaceView>, f: impl FnOnce(&SurfaceView) -> R) -> R {
        view.read_with(self.cx, |v, _| f(v))
    }
}

/// Every node the painter must have painted: not hidden, no hidden
/// ancestor, in the main tree or an open layer.
fn visible(v: &SurfaceView) -> Vec<u32> {
    let nodes = v.placed_nodes();
    let open: Vec<u32> = v.layers().iter().map(|l| l.layer).collect();
    let mut shown = vec![false; nodes.len()];
    for n in nodes {
        let parent_ok = match n.parent {
            Some(p) => shown[p as usize] || nodes[p as usize].layer != n.layer,
            None => true,
        };
        shown[n.index as usize] = !n.hidden && parent_ok && (n.layer == 0 || open.contains(&n.layer));
    }
    nodes.iter().filter(|n| shown[n.index as usize]).map(|n| n.index).collect()
}

fn painted_clean(v: &SurfaceView) -> Result<(), String> {
    if let Some(n) = v.placed_nodes().iter().find(|n| n.component == UNKNOWN_COMPONENT) {
        return Err(format!("Unknown placeholder at {}", n.id));
    }
    let trace = v.paint_trace();
    let shown = visible(v);
    // A closed overlay (round 1: `Toast/open=false`) shows nothing at all.
    ensure(!trace.is_empty() || shown.is_empty(), || "nothing painted".into())?;
    let missing: Vec<String> = shown.into_iter().filter(|i| !trace.contains(i)).filter_map(|i| v.placed_nodes().get(i as usize).map(|n| n.id.clone())).collect();
    ensure(missing.is_empty(), || format!("visible but not painted: {missing:?}"))
}

// --- painted suites ----------------------------------------------------------

fn catalog_suite(p: &mut Painter) -> Vec<Case> {
    let f = fixture("catalog-components.json");
    let core = ReduceOptions::new(CORE_CATALOG_ID);
    let mut out = Vec::new();
    for c in list(&f, "cases") {
        let name = c["name"].as_str().unwrap().to_string();
        let node: Result<NestedNode, String> = serde_json::from_value(c["node"].clone()).map_err(|e| e.to_string());
        let result = (|| {
            let node = node?;
            let reduced = reduce_nested(&node, &core);
            ensure(reduced.issues.is_empty(), || format!("issues {:?}", reduced.issues))?;
            let mut issues = Vec::new();
            let view = p.paint(SurfaceViewOptions { surface_id: "catalog".into(), theme: builtin_theme("exponential"), mode: Mode::Light, ..Default::default() }, 400.0, |v, cx| {
                v.set_viewport_height(900.0, cx);
                issues = v.set_nested(node, cx).issues;
            });
            ensure(issues.is_empty(), || format!("surface issues {issues:?}"))?;
            p.read(&view, painted_clean)
        })();
        out.push((name, result));
    }
    out
}

/// The node whose box IS the control (`CONTROL_PARTS`). The recipe part
/// `Radio/item` is the circle (Radix's RadioGroup.Item); the core
/// synthesizes that circle as `<id>.dot.N` (its `.item.N` is the row).
fn control_node(v: &SurfaceView, component: &str, part: &str) -> Option<u32> {
    let part = if (component, part) == ("Radio", "item") { "dot" } else { part };
    v.placed_nodes()
        .iter()
        .find(|n| {
            if part == "root" {
                n.component == component && n.part.is_none()
            } else {
                n.part.as_deref() == Some(part) && (n.owner_component.as_deref() == Some(component) || n.component == component)
            }
        })
        .map(|n| n.index)
}

fn control_geometry_suite(p: &mut Painter) -> Vec<Case> {
    let f = fixture("control-geometry.json");
    let mut out = Vec::new();
    for (theme_id, by_component) in f["themes"].as_object().unwrap() {
        for (component, entry) in by_component.as_object().unwrap() {
            let part = entry["part"].as_str().unwrap();
            for (name, c) in entry["cases"].as_object().unwrap() {
                let label = format!("{theme_id} {component} {name}");
                let result = (|| {
                    let theme = builtin_theme(theme_id).ok_or("not a built-in")?;
                    let mut props = component_def(component).and_then(|d| d.example.clone()).unwrap_or_default();
                    props.extend(obj(&c["props"]));
                    let node: NestedNode = serde_json::from_value(json!({"id": "root", "component": component, "props": props})).map_err(|e| e.to_string())?;
                    let view = p.paint(SurfaceViewOptions { surface_id: "control".into(), theme: Some(theme.clone()), mode: Mode::Light, ..Default::default() }, 360.0, |v, cx| {
                        v.set_nested(node, cx);
                    });
                    let measured: BTreeMap<&str, Option<f64>> = p.read(&view, |v| match control_node(v, component, part) {
                        // A synthesized part: the measurer's border box (the
                        // frame) and the visual the painter paints it with.
                        Some(i) => {
                            let fr = v.frame(i).unwrap_or_default();
                            let vis = v.surface().visual(i).cloned().unwrap_or_default();
                            // A CONTAINER part (round 1: NumberField / ChipInput
                            // `field` expand into parts) keeps its padding in
                            // the layout, not the visual: the recipe's, which the
                            // core lays its children out with.
                            let leaf = v.placed_nodes().get(i as usize).is_some_and(|n| n.kind == exponential_ui::layout_tree::NodeKind::Leaf);
                            let recipe = (!leaf).then(|| part_visual(Some(&theme), Mode::Light, component, part, &props, &[]));
                            let pad = |own: Option<f32>, of: fn(&exponential_ui::style::Visual) -> Option<f32>| own.or_else(|| recipe.as_ref().and_then(of)).unwrap_or(0.0) as f64;
                            BTreeMap::from([
                                ("width", Some(fr.w as f64)),
                                ("height", Some(fr.h as f64)),
                                ("paddingHorizontal", Some(pad(vis.padding_horizontal, |r| r.padding_horizontal))),
                                ("paddingVertical", Some(pad(vis.padding_vertical, |r| r.padding_vertical))),
                                ("borderWidth", Some(vis.border_width.unwrap_or(0.0) as f64)),
                                ("borderRadius", Some(vis.border_radius.unwrap_or(0.0) as f64)),
                            ])
                        }
                        // A part the core does not synthesize (Slider thumb):
                        // the painter draws it from the owner's recipe.
                        None => {
                            let pp = part_props(Some(&theme), Mode::Light, component, part, &props, &[]);
                            let vis = part_visual(Some(&theme), Mode::Light, component, part, &props, &[]);
                            BTreeMap::from([
                                ("width", px_prop(&pp, "width").map(f64::from)),
                                ("height", px_prop(&pp, "height").map(f64::from)),
                                ("paddingHorizontal", Some(vis.padding_horizontal.unwrap_or(0.0) as f64)),
                                ("paddingVertical", Some(vis.padding_vertical.unwrap_or(0.0) as f64)),
                                ("borderWidth", Some(vis.border_width.unwrap_or(0.0) as f64)),
                                ("borderRadius", Some(vis.border_radius.unwrap_or(0.0) as f64)),
                            ])
                        }
                    });
                    check_geometry(&c["geometry"], &measured, 0.5)
                })();
                out.push((label, result));
            }
        }
    }
    out
}

/// `checkGeometry` (src/geometry.ts): every key the fixture fixes within
/// `tolerance`, except `height` when it has `minHeight` (then the measured
/// height only has to reach it).
fn check_geometry(expected: &Value, measured: &BTreeMap<&str, Option<f64>>, tolerance: f64) -> Result<(), String> {
    let mut issues = Vec::new();
    let want = |k: &str| expected.get(k).and_then(Value::as_f64);
    let min_height = want("minHeight");
    for key in ["width", "height", "paddingHorizontal", "paddingVertical", "borderWidth", "borderRadius"] {
        if key == "height" && min_height.is_some() {
            continue;
        }
        let Some(w) = want(key) else { continue };
        match measured.get(key).copied().flatten() {
            Some(a) if (a - w).abs() <= tolerance => {}
            a => issues.push(format!("{key} {w}≠{a:?}")),
        }
    }
    if let Some(m) = min_height {
        match measured.get("height").copied().flatten() {
            Some(h) if h + tolerance >= m => {}
            h => issues.push(format!("minHeight {m}≠{h:?}")),
        }
    }
    ensure(issues.is_empty(), || issues.join(", "))
}

fn layout_suite(p: &mut Painter) -> Vec<Case> {
    let fx = fixture("layout-geometry.json");
    let sizes: HashMap<String, (f32, f32)> = fx["measures"].as_object().unwrap().iter().map(|(id, m)| (id.clone(), (m["w"].as_f64().unwrap() as f32, m["h"].as_f64().unwrap() as f32))).collect();
    let mut out = Vec::new();
    for (name, c) in fx["cases"].as_object().unwrap() {
        let result = (|| {
            let mut tree: NestedNode = serde_json::from_value(fx["surface"].clone()).map_err(|e| e.to_string())?;
            tree.style.get_or_insert_with(Default::default).insert("direction".into(), c["direction"].clone());
            let width = c["width"].as_f64().unwrap() as f32;
            let mut issues = Vec::new();
            let view = p.paint(SurfaceViewOptions { surface_id: "geometry".into(), theme: None, ..Default::default() }, width, |v, cx| {
                v.set_measure(Some(Box::new(FixedMeasure::with_sizes(sizes.clone()))), cx);
                issues = v.set_nested(tree, cx).issues;
            });
            ensure(issues.is_empty(), || format!("issues {issues:?}"))?;
            p.read(&view, |v| {
                let expected = c["frames"].as_array().unwrap();
                let trace = v.paint_trace();
                let painted: Vec<&str> = trace.iter().map(|i| v.placed_nodes()[*i as usize].id.as_str()).collect();
                let want: Vec<&str> = expected.iter().map(|f| f["id"].as_str().unwrap()).collect();
                ensure(painted == want, || format!("painted {painted:?}, expected {want:?}"))?;
                for (k, f) in expected.iter().enumerate() {
                    let got = v.frame(trace[k]).ok_or("no frame")?;
                    for (key, val) in [("x", got.x), ("y", got.y), ("w", got.w), ("h", got.h)] {
                        let e = f[key].as_f64().unwrap() as f32;
                        ensure((val - e).abs() <= 0.001, || format!("{}.{key} = {val}, expected {e}", want[k]))?;
                    }
                }
                Ok(())
            })
        })();
        out.push((name.clone(), result));
    }
    out
}

fn overlay_suite(p: &mut Painter) -> Vec<Case> {
    let fx = fixture("overlay-geometry.json");
    let tolerance = fx["tolerancePx"].as_f64().unwrap_or(0.0);
    let mut out = Vec::new();
    for c in list(&fx, "cases") {
        let result = (|| {
            let (a, s, vp) = (&c["anchor"], &c["size"], &c["viewport"]);
            let tree: NestedNode = serde_json::from_value(json!({
                "id": "root", "component": "Box",
                "style": {"position": "relative", "width": vp["width"], "height": vp["height"], "overflow": "hidden"},
                "children": [{
                    "id": "pop", "component": "Popover", "props": {"open": true, "side": c["side"]},
                    "style": {"position": "absolute", "left": a["x"], "top": a["y"], "width": a["width"], "height": a["height"]},
                    "slots": {"trigger": {"id": "anchor", "component": "Box", "style": {"width": a["width"], "height": a["height"], "backgroundColor": "$color.primary"}}},
                    "children": [{"id": "content", "component": "Box", "style": {"width": s["width"], "height": s["height"], "backgroundColor": "$color.success"}}]
                }]
            }))
            .map_err(|e| e.to_string())?;
            let view = p.paint(SurfaceViewOptions { surface_id: "ov".into(), theme: builtin_theme("neutral"), mode: Mode::Light, ..Default::default() }, vp["width"].as_f64().unwrap() as f32, |v, cx| {
                v.set_measure(Some(Box::new(FixedMeasure::default())), cx);
                v.set_viewport_height(vp["height"].as_f64().unwrap() as f32, cx);
                v.set_nested(tree, cx);
            });
            p.read(&view, |v| {
                let layer = v.layers().iter().find(|l| l.kind == "Popover").ok_or("no Popover layer")?;
                ensure(v.paint_trace().contains(&layer.root), || "the layer was not painted".into())?;
                let got = v.frame(layer.root).ok_or("no frame")?;
                let side = layer.placement.as_ref().map(|pl| pl.side.as_str().to_string()).ok_or("no placement")?;
                let rect = |v: &Value| Rect { x: v["x"].as_f64().unwrap(), y: v["y"].as_f64().unwrap(), width: v["width"].as_f64().unwrap(), height: v["height"].as_f64().unwrap() };
                let want = place_overlay(&rect(a), &Size { width: got.w as f64, height: got.h as f64 }, &Size { width: vp["width"].as_f64().unwrap(), height: vp["height"].as_f64().unwrap() }, &PlaceOptions::side(OverlaySide::parse(c["side"].as_str().unwrap()).ok_or("bad side")?));
                let e = &c["expected"];
                ensure(side == e["side"].as_str().unwrap() && want.side.as_str() == e["side"].as_str().unwrap(), || format!("side {side}"))?;
                ensure((got.x as f64 - want.x).abs() <= tolerance && (got.y as f64 - want.y).abs() <= tolerance, || format!("at {},{} want {},{}", got.x, got.y, want.x, want.y))
            })
        })();
        out.push((c["name"].as_str().unwrap().to_string(), result));
    }
    out
}

fn preorder(node: &Value, out: &mut Vec<String>) {
    if let Some(id) = node["id"].as_str() {
        out.push(id.to_string());
    }
    if let Some(slots) = node.get("slots").and_then(Value::as_object) {
        for s in slots.values() {
            preorder(s, out);
        }
    }
    for c in node.get("children").and_then(Value::as_array).into_iter().flatten() {
        preorder(c, out);
    }
}

fn replay_suite(p: &mut Painter) -> Vec<Case> {
    let sink: NestedNode = serde_json::from_value(fixture("kitchen-sink.json")).unwrap();
    let mut expected = fixture("kitchen-sink.expanded.json");
    expected.as_object_mut().unwrap().remove("$comment");
    let mut order = Vec::new();
    preorder(&expected["root"], &mut order);
    let mut out = Vec::new();
    for id in BUILTIN_THEME_IDS {
        for mode in MODES {
            let result = (|| {
                let reduced = reduce_nested(&sink, &ReduceOptions::new(CORE_CATALOG_ID));
                same(&to_value(&reduced), &expected)?;
                let mut issues = Vec::new();
                let view = p.paint(SurfaceViewOptions { surface_id: "ks".into(), theme: Some(builtin_theme(id).ok_or("not a built-in")?), mode, ..Default::default() }, 900.0, |v, cx| {
                    issues = v.set_nested(sink.clone(), cx).issues;
                    v.set_data("", Some(json!({"posts": [{"title": "One"}, {"title": "Two"}], "ui": {"confirmOpen": false}, "draft": {"title": ""}})), cx);
                    v.set_viewport_height(800.0, cx);
                });
                ensure(issues.is_empty(), || format!("issues {issues:?}"))?;
                p.read(&view, |v| {
                    painted_clean(v)?;
                    // Paint order = pre-order (the a11y order) for every
                    // authored node the painter drew.
                    let idx: Vec<usize> = v.paint_trace().iter().filter_map(|i| order.iter().position(|o| *o == v.placed_nodes()[*i as usize].id)).collect();
                    ensure(idx.len() >= 50, || format!("only {} authored nodes painted", idx.len()))?;
                    ensure(idx.windows(2).all(|w| w[1] > w[0]), || "paint order is not the pre-order".into())
                })
            })();
            out.push((format!("{id}/{}", mode.as_str()), result));
        }
    }
    out
}

// --- pure suites (the core, linked directly) ------------------------------------

fn macros_suite() -> Vec<Case> {
    let core = ReduceOptions::new(CORE_CATALOG_ID);
    list(&fixture("catalog-macros.json"), "cases")
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

fn reduce_cases(file: &str, view: Option<std::sync::Arc<CatalogView>>, catalog: impl Fn(&Value) -> String) -> Vec<Case> {
    list(&fixture(file), "cases")
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

fn extension_suite(p: &mut Painter) -> Vec<Case> {
    let f = fixture("catalog-extension.json");
    let def: ExtensionDef = serde_json::from_value(f["extension"].clone()).unwrap();
    let ext = define_extension(def).expect("the example extension is valid");
    let view = CatalogView::with(std::slice::from_ref(&ext));
    let mut cases = reduce_cases("catalog-extension.json", Some(view), |c| c["catalogId"].as_str().unwrap().to_string());
    // Its natives reach the extension painter: an `Extension` leaf, painted.
    if let Some((_, result)) = cases.first_mut() {
        if result.is_ok() {
            let c = &f["cases"][0];
            *result = (|| {
                let components: Vec<FlatComponent> = serde_json::from_value(c["components"].clone()).map_err(|e| e.to_string())?;
                let view = p.paint(SurfaceViewOptions { surface_id: "ext".into(), catalog_id: ext.id.clone(), extensions: vec![ext.clone()], ..Default::default() }, 400.0, |v, cx| {
                    v.set_components(components, cx);
                });
                p.read(&view, |v| {
                    painted_clean(v)?;
                    let leaf = v.placed_nodes().iter().find(|n| n.component == "Extension").ok_or("no Extension leaf")?;
                    ensure(v.paint_trace().contains(&leaf.index), || "the Extension leaf was not painted".into())
                })
            })();
        }
    }
    cases
}

fn theme_suites(id: &str) -> Vec<Case> {
    let refs = builtin_refs();
    let options = ThemeOptions::core(&refs);
    match id {
        "theme-resolved" => fixture("theme-resolved.json")["themes"].as_object().unwrap().iter().map(|(id, expected)| case(id.clone(), || same(&to_value(&*builtin_theme(id).ok_or("not a built-in")?), expected))).collect(),
        "theme-recipes" => list(&fixture("theme-recipes.json"), "cases")
            .iter()
            .map(|c| {
                let (component, part) = (c["component"].as_str().unwrap(), c["part"].as_str().unwrap());
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
            .collect(),
        "theme-extends" => list(&fixture("theme-extends.json"), "cases")
            .iter()
            .map(|c| {
                case(c["name"].as_str().unwrap(), || {
                    let t = load_theme(&c["theme"], &options).map_err(|e| e.to_string())?;
                    ensure(t.chain == strings(c["expected"].get("chain")), || format!("chain {:?}", t.chain))?;
                    for p in c["expected"]["probes"].as_array().unwrap() {
                        let q = RecipeQuery::new(p["component"].as_str().unwrap(), p["part"].as_str().unwrap(), obj(&p["props"]), strings(p.get("states")));
                        same(&Value::Object(resolve_recipe(&t, &q, Mode::parse(p["mode"].as_str().unwrap()).ok_or("bad mode")?)), &p["style"])?;
                    }
                    Ok(())
                })
            })
            .collect(),
        "theme-invalid" => list(&fixture("theme-invalid.json"), "cases")
            .iter()
            .map(|c| {
                case(c["name"].as_str().unwrap(), || match try_load_theme(&c["theme"], &options) {
                    Ok(_) => Err("loaded".into()),
                    Err(issues) => same(&to_value(&issues), &c["issues"]),
                })
            })
            .collect(),
        _ => unreachable!(),
    }
}

fn feed(mut push: impl FnMut(&str) -> Decoded, end: impl FnOnce() -> Decoded, chunks: &[Value]) -> Decoded {
    let mut out = Decoded::default();
    for c in chunks {
        out.extend(push(c.as_str().expect("chunk")));
    }
    out.extend(end());
    out
}

fn host_suite(id: &str) -> Vec<Case> {
    let mut out = Vec::new();
    match id {
        "host-transport" => {
            let f = fixture("host-transport.json");
            for c in list(&f, "jsonl") {
                let mut d = JsonlDecoder::new();
                let got = feed(|s| d.push(s), Decoded::default, list(c, "chunks"));
                let got = { let mut g = got; g.extend(d.end()); g };
                out.push((format!("jsonl: {}", c["name"].as_str().unwrap()), same(&to_value(&got), &c["expected"])));
            }
            for c in list(&f, "sse") {
                let mut d = SseDecoder::new();
                let got = feed(|s| d.push(s), Decoded::default, list(c, "chunks"));
                let got = { let mut g = got; g.extend(d.end()); g };
                out.push((format!("sse: {}", c["name"].as_str().unwrap()), same(&to_value(&got), &c["expected"])));
            }
            for c in list(&f, "mcp") {
                out.push((format!("mcp: {}", c["name"].as_str().unwrap()), same(&to_value(&messages_from_mcp_result(&c["result"])), &c["expected"])));
            }
            for c in list(&f, "mcpAction") {
                out.push((format!("mcpAction: {}", c["name"].as_str().unwrap()), same(&mcp_action_call(&c["message"], c.get("tool").and_then(Value::as_str)), &c["expected"])));
            }
        }
        "host-policy" => {
            let f = fixture("host-policy.json");
            let decision = |v: &Value| FunctionDecision::parse(v.as_str().unwrap()).unwrap();
            for c in list(&f, "functions") {
                let policy: Option<FunctionPolicy> = c.get("policy").map(|p| serde_json::from_value(p.clone()).unwrap());
                let got = decide_function(policy.as_ref(), c["fn"].as_str().unwrap(), c["registered"].as_bool().unwrap());
                out.push((format!("function: {}", c["name"].as_str().unwrap()), same(&Value::from(got.as_str()), &c["expected"])));
            }
            for c in list(&f, "combine") {
                let got = combine_decisions(decision(&c["a"]), decision(&c["b"]));
                out.push((format!("combine: {} + {}", c["a"].as_str().unwrap(), c["b"].as_str().unwrap()), same(&Value::from(got.as_str()), &c["expected"])));
            }
            for c in list(&f, "urls") {
                let policy: Option<UrlPolicy> = c.get("policy").map(|p| serde_json::from_value(p.clone()).unwrap());
                out.push((format!("url: {}", c["name"].as_str().unwrap()), same(&to_value(&decide_url(policy.as_ref(), c["url"].as_str().unwrap())), &c["expected"])));
            }
            for c in list(&f, "media") {
                let options: MediaOptions = serde_json::from_value(c["options"].clone()).unwrap();
                out.push((format!("media: {}", c["name"].as_str().unwrap()), same(&to_value(&media_request(c["url"].as_str().unwrap(), &options)), &c["expected"])));
            }
            for c in list(&f, "sources") {
                let uri = c["uri"].as_str().unwrap();
                out.push((format!("source: {uri}"), same(&to_value(&parse_source(uri)), &c["expected"])));
            }
            for c in list(&f, "negotiation") {
                let ids: Vec<String> = serde_json::from_value(c["extensionIds"].clone()).unwrap();
                let r = same(&to_value(&supported_catalog_ids(&ids)), &c["expected"]["supportedCatalogIds"]).and_then(|_| same(&client_capabilities(&ids), &c["expected"]["clientCapabilities"]));
                out.push((format!("negotiation: {} extension ids", ids.len()), r));
            }
        }
        "host-router" => {
            let f = fixture("host-router.json");
            let packages = &f["packages"];
            for v in list(&f, "validation") {
                let id = v["package"].as_str().unwrap();
                out.push((format!("validation: {id}"), same(&to_value(&validate_package(&packages[id], &supported_catalog_ids::<&str>(&[]))), &v["expected"])));
            }
            for flow in list(&f, "flows") {
                let ext: Vec<String> = flow.get("extensionIds").map(|e| serde_json::from_value(e.clone()).unwrap()).unwrap_or_default();
                let mut router = HostRouter::new(&ext);
                let mut result = Ok(());
                for id in flow.get("packages").and_then(Value::as_array).into_iter().flatten() {
                    let id = id.as_str().unwrap();
                    let got = router.install_package(&packages[id]);
                    result = result.and_then(|_| same(&to_value(&got), &flow["installIssues"][id]).map_err(|e| format!("install {id}: {e}")));
                }
                for (i, step) in list(flow, "steps").iter().enumerate() {
                    let got = Value::Array(router.route(&step["message"]));
                    result = result.and_then(|_| same(&got, &step["expected"]).map_err(|e| format!("step {i}: {e}")));
                }
                out.push((format!("flow: {}", flow["name"].as_str().unwrap()), result));
            }
        }
        _ => unreachable!(),
    }
    out
}

fn suite(id: &str, p: &mut Painter) -> Vec<Case> {
    match id {
        "catalog" => catalog_suite(p),
        "macros" => macros_suite(),
        "basic-map" => reduce_cases("catalog-basic-map.json", None, |_| A2UI_BASIC_CATALOG_ID.to_string()),
        "extension" => extension_suite(p),
        "theme-resolved" | "theme-recipes" | "theme-extends" | "theme-invalid" => theme_suites(id),
        "control-geometry" => control_geometry_suite(p),
        "layout" => layout_suite(p),
        "overlay" => overlay_suite(p),
        "replay" => replay_suite(p),
        "host-transport" | "host-policy" | "host-router" => host_suite(id),
        "bind" => round2::bind_cases(),
        "style-conditions" => round2::style_condition_cases(),
        "code-tokens" => round2::code_token_cases(),
        "format" => round2::format_cases(),
        "template-items" => round2::template_item_cases(),
        "text-direction" => round2::text_direction_cases(),
        "resizable" => round2::resizable_cases(),
        "virtual-list" => round2::virtual_list_cases(),
        "animations" => round2::animation_cases(),
        other => panic!("the runner does not know the suite {other}: add it"),
    }
}

fn report_path() -> std::path::PathBuf {
    match std::env::var("EXPONENTIAL_UI_CONFORMANCE_REPORT") {
        Ok(p) if !p.is_empty() => p.into(),
        _ => std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../../..")).join(".conformance").join("exponential-ui-gpui.json"),
    }
}

#[gpui::test]
fn the_gpui_painter_passes_every_conformance_suite_and_writes_its_report(cx: &mut TestAppContext) {
    cx.update(gpui_component::init);
    let (holder, vcx) = cx.add_window_view(|_, _| Holder { view: None, width: 400.0 });
    let mut painter = Painter { holder, cx: vcx };
    let manifest = read("conformance/manifest.json");
    let mut suites = BTreeMap::new();
    let mut problems = Vec::new();
    for s in manifest["suites"].as_array().unwrap() {
        let id = s["id"].as_str().unwrap();
        let cases = suite(id, &mut painter);
        let failed: Vec<String> = cases.iter().filter_map(|(name, r)| r.as_ref().err().map(|e| format!("{name}: {e}"))).collect();
        let want = s["cases"].as_u64().unwrap() as usize;
        if cases.len() != want {
            problems.push(format!("{id}: ran {} of {want} cases", cases.len()));
        }
        if !failed.is_empty() {
            problems.push(format!("{id}: {} failed\n  {}", failed.len(), failed.iter().take(20).cloned().collect::<Vec<_>>().join("\n  ")));
        }
        suites.insert(id.to_string(), json!({"cases": cases.len(), "passed": cases.len() - failed.len(), "failed": failed}));
    }
    let report = json!({
        "renderer": "exponential-ui-gpui",
        "platform": "desktop",
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
