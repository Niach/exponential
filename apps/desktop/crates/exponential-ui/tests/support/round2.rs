//! The round-1 bind/style/code fixtures and the round-2 contract fixtures
//! replayed case by case (`src/round2.test.ts`, `src/round1.test.ts`), each
//! case a name and `Err(first difference)`: shared by `round2_fixtures.rs`
//! and the conformance runner (manifest v2 counts them).

use std::panic::{catch_unwind, AssertUnwindSafe};

use exponential_ui::animation::{animation_frame, animation_timing, keyframes_css, ANIMATION_NAMES};
use exponential_ui::code::tokenize_code;
use exponential_ui::conditions::{default_breakpoints, resolve_conditions, ConditionContext};
use exponential_ui::data::{bind_row_slot, bind_tree, resolve_value, run_action, ResolveContext};
use exponential_ui::direction::{node_directions, node_text_align, TextDirection};
use exponential_ui::format::display_string;
use exponential_ui::list::{item_extents, item_offsets, list_sections, scroll_offset_for_index, section_rows, sticky_header, table_row_keys, template_instances, template_item_keys, virtual_window, ScrollAlign, SectionRow};
use exponential_ui::reducer::{reduce_nested, reduce_surface, ReduceOptions};
use exponential_ui::resizable::{drag_delta, keyboard_resize, normalize_sizes, panel_extents, resize_panels, Orientation, PanelLimits};
use exponential_ui::strings::DEFAULT_STRINGS;
use exponential_ui::themes::builtin_theme;
use exponential_ui::types::Template;
use exponential_ui::{FlatComponent, NestedNode, UiNode, CORE_CATALOG_ID};
use indexmap::IndexMap;
use serde_json::{json, Value};

use super::{fixture, same, Case};

fn to_value<T: serde::Serialize>(v: &T) -> Value {
    serde_json::to_value(v).expect("serializable")
}

/// One case: a panic is a failure, never the end of the run.
pub fn case(name: impl Into<String>, f: impl FnOnce() -> Result<(), String>) -> Case {
    let name = name.into();
    let result = catch_unwind(AssertUnwindSafe(f)).unwrap_or_else(|p| {
        let msg = p.downcast_ref::<String>().cloned().or_else(|| p.downcast_ref::<&str>().map(|s| s.to_string())).unwrap_or_else(|| "panicked".into());
        Err(format!("panic: {msg}"))
    });
    (name, result)
}

fn ensure(ok: bool, msg: impl FnOnce() -> String) -> Result<(), String> {
    if ok {
        Ok(())
    } else {
        Err(msg())
    }
}

fn arr(v: &Value) -> &Vec<Value> {
    v.as_array().expect("array")
}

fn f64s(v: &Value) -> Vec<f64> {
    arr(v).iter().map(|x| x.as_f64().expect("number")).collect()
}

fn near(got: &[f64], want: &[f64], eps: f64) -> Result<(), String> {
    ensure(got.len() == want.len() && got.iter().zip(want).all(|(a, b)| (a - b).abs() <= eps), || format!("got {got:?} / expected {want:?}"))
}

fn find<'a>(node: &'a UiNode, id: &str) -> Option<&'a UiNode> {
    if node.id == id {
        return Some(node);
    }
    node.slots.iter().flat_map(|s| s.values()).chain(node.children.iter()).find_map(|c| find(c, id))
}

// ---------------------------------------------------------------------------
// Round 1 fixtures (now counted: bind, style-conditions, code-tokens)
// ---------------------------------------------------------------------------

fn replay_bind(c: &Value) -> Result<(), String> {
    let input: NestedNode = serde_json::from_value(c["input"].clone()).map_err(|e| e.to_string())?;
    let reduced = reduce_nested(&input, &ReduceOptions::new(CORE_CATALOG_ID));
    same(&to_value(&reduced.issues), &c.get("issues").cloned().unwrap_or(json!([]))).map_err(|e| format!("issues {e}"))?;
    same(&to_value(&reduced.root), &c["expanded"]).map_err(|e| format!("expanded {e}"))?;
    let expanded: UiNode = serde_json::from_value(c["expanded"].clone()).map_err(|e| e.to_string())?;
    for d in arr(&c["datasets"]) {
        let data = &d["data"];
        let ctx = ResolveContext::new(data, "").with_strings(&DEFAULT_STRINGS);
        let bound = bind_tree(&expanded, &ctx);
        same(&to_value(&bound), &d["bound"]).map_err(|e| format!("bound {data}: {e}"))?;
        for p in arr(&d["presses"]) {
            let id = p["id"].as_str().unwrap_or("");
            let node = find(&expanded, id).ok_or_else(|| format!("no {id}"))?;
            let action = &node.on.as_ref().ok_or("no on")?["press"];
            same(&to_value(&run_action(action, &ctx)), &p["outcome"]).map_err(|e| format!("press {id}: {e}"))?;
        }
        for t in d.get("rowSlots").and_then(Value::as_array).into_iter().flatten() {
            let id = t["id"].as_str().unwrap_or("");
            let bound = bound.as_ref().ok_or("no bound tree")?;
            let node = find(bound, id).ok_or_else(|| format!("no {id}"))?;
            let rows_prop = find(&expanded, id).and_then(|n| n.props.get("rows").cloned()).unwrap_or(Value::Null);
            let rows = node.props.get("rows").and_then(Value::as_array).cloned().unwrap_or_default();
            for r in arr(&t["rows"]) {
                let index = r["index"].as_u64().unwrap_or(0) as usize;
                for (slot_name, expected) in r["slots"].as_object().ok_or("slots")? {
                    let slot = &node.slots.as_ref().ok_or("no slots")?[slot_name];
                    same(&to_value(&bind_row_slot(slot, &rows_prop, &rows, index, &ctx)), expected).map_err(|e| format!("row {index} {slot_name}: {e}"))?;
                }
            }
        }
    }
    Ok(())
}

pub fn bind_cases() -> Vec<Case> {
    let f = fixture("bind-time.json");
    arr(&f["cases"]).iter().chain(arr(&f["extra"])).map(|c| case(c["name"].as_str().unwrap_or(""), || replay_bind(c))).collect()
}

pub fn style_condition_cases() -> Vec<Case> {
    let f = fixture("style-conditions.json");
    let defaults: IndexMap<String, f32> = serde_json::from_value(f["breakpoints"].clone()).unwrap_or_else(|_| default_breakpoints());
    arr(&f["cases"])
        .iter()
        .map(|c| {
            case(c["name"].as_str().unwrap_or(""), || {
                let style = c["style"].as_object().ok_or("style")?;
                for (ctx, expected) in arr(&c["contexts"]).iter().zip(arr(&c["expected"])) {
                    let mut context: ConditionContext = serde_json::from_value(ctx.clone()).map_err(|e| e.to_string())?;
                    if ctx.get("breakpoints").is_none() {
                        context.breakpoints = defaults.clone();
                    }
                    same(&Value::Object(resolve_conditions(style, &context)), expected).map_err(|e| format!("{}: {e}", ctx["label"]))?;
                }
                Ok(())
            })
        })
        .collect()
}

pub fn code_token_cases() -> Vec<Case> {
    let f = fixture("code-tokens.json");
    arr(&f["cases"])
        .iter()
        .map(|c| case(c["name"].as_str().unwrap_or(""), || same(&to_value(&tokenize_code(c["code"].as_str().unwrap_or(""), c["language"].as_str().unwrap_or(""))), &c["expected"])))
        .collect()
}

// ---------------------------------------------------------------------------
// Round 2
// ---------------------------------------------------------------------------

/// `format.json`: every call through the English fallback; every display value.
pub fn format_cases() -> Vec<Case> {
    let f = fixture("format.json");
    let empty = json!({});
    let mut out: Vec<Case> = arr(&f["calls"])
        .iter()
        .map(|c| {
            case(format!("call {}", c["name"].as_str().unwrap_or("")), || {
                let got = resolve_value(&c["call"], &ResolveContext::new(&empty, "")).unwrap_or(Value::Null);
                same(&got, &c["expected"])
            })
        })
        .collect();
    // `zoned`: the fallback at the host's offset (`offsetMinutes`).
    out.extend(arr(&f["zoned"]).iter().map(|c| {
        case(format!("zoned {}", c["name"].as_str().unwrap_or("")), || {
            let zone = exponential_ui::format::ZonedEnglishFormatter::fixed(c["offsetMinutes"].as_i64().unwrap_or(0) as i32);
            let got = resolve_value(&c["call"], &ResolveContext::new(&empty, "").with_formatter(&zone)).unwrap_or(Value::Null);
            same(&got, &c["expected"])
        })
    }));
    out.extend(arr(&f["display"]).iter().map(|d| case(format!("display {}", d["name"].as_str().unwrap_or("")), || same(&json!(display_string(&d["value"])), &d["expected"]))));
    out
}

/// `template-items.json`: keys, row keys, instances (suffixes accumulate),
/// reduce (templates lifted, never in place).
pub fn template_item_cases() -> Vec<Case> {
    let f = fixture("template-items.json");
    let mut out = Vec::new();
    for c in arr(&f["keys"]) {
        out.push(case(format!("keys {}", c["name"].as_str().unwrap_or("")), || same(&json!(template_item_keys(arr(&c["items"]), c["key"].as_str())), &c["expected"])));
    }
    for c in arr(&f["rowKeys"]) {
        out.push(case(format!("rowKeys {}", c["name"].as_str().unwrap_or("")), || same(&json!(table_row_keys(arr(&c["rows"]), c["rowKey"].as_str().unwrap_or("id"))), &c["expected"])));
    }
    for c in arr(&f["instances"]) {
        out.push(case(format!("instances {}", c["name"].as_str().unwrap_or("")), || {
            let template: Template = serde_json::from_value(c["template"].clone()).map_err(|e| e.to_string())?;
            let scope = c["scope"].as_str().unwrap_or("");
            let instance = c["instance"].as_str().unwrap_or("");
            let outer = template_instances(&c["data"], &template, scope, instance);
            let Some(inner) = c.get("inner") else { return same(&to_value(&outer), &c["expected"]) };
            let inner: Template = serde_json::from_value(inner.clone()).map_err(|e| e.to_string())?;
            let nested: Vec<Value> = outer
                .iter()
                .flat_map(|o| {
                    template_instances(&c["data"], &inner, &o.path, &o.instance).into_iter().map(move |i| {
                        json!({"outer": o.key, "key": i.key, "path": i.path, "index": i.index, "instance": i.instance, "ids": {"issue": format!("issue{}", i.instance), "title": format!("issue.title{}", i.instance)}})
                    })
                })
                .collect();
            same(&Value::Array(nested), &c["expected"])
        }));
    }
    for c in arr(&f["reduce"]) {
        out.push(case(format!("reduce {}", c["name"].as_str().unwrap_or("")), || {
            let options = ReduceOptions::new(c["catalogId"].as_str().unwrap_or(CORE_CATALOG_ID));
            let got = match c.get("nested") {
                Some(n) => reduce_nested(&serde_json::from_value::<NestedNode>(n.clone()).map_err(|e| e.to_string())?, &options),
                None => reduce_surface(&serde_json::from_value::<Vec<FlatComponent>>(c["components"].clone()).map_err(|e| e.to_string())?, &options),
            };
            same(&to_value(&got), &c["expected"])?;
            let ids = exponential_ui::reducer::preorder(&got.root);
            for id in got.templates.iter().flat_map(|t| t.keys()) {
                ensure(!ids.contains(id), || format!("{id} in place"))?;
            }
            Ok(())
        }));
    }
    out
}

/// `text-direction.json`: every node's direction and physical alignment.
pub fn text_direction_cases() -> Vec<Case> {
    let f = fixture("text-direction.json");
    arr(&f["cases"])
        .iter()
        .map(|c| {
            case(c["name"].as_str().unwrap_or(""), || {
                let tree: NestedNode = serde_json::from_value(c["tree"].clone()).map_err(|e| e.to_string())?;
                let root = reduce_nested(&tree, &ReduceOptions::new(CORE_CATALOG_ID)).root;
                let surface = TextDirection::parse(c["surface"].as_str().unwrap_or("ltr")).ok_or("surface")?;
                let dirs = node_directions(&root, surface, None);
                let mut got = serde_json::Map::new();
                root.walk(&mut |n| {
                    let d = dirs[&n.id];
                    got.insert(n.id.clone(), json!({"direction": d.as_str(), "textAlign": node_text_align(n, d, None).as_str()}));
                });
                same(&Value::Object(got), &c["expected"])
            })
        })
        .collect()
}

fn limits(v: &Value) -> Vec<PanelLimits> {
    PanelLimits::list(Some(v))
}

fn orientation(v: &Value) -> Orientation {
    Orientation::parse(v.as_str().unwrap_or("horizontal")).unwrap_or_default()
}

/// `resizable.json`, within 1e-6 (and every size list sums to 100).
pub fn resizable_cases() -> Vec<Case> {
    let f = fixture("resizable.json");
    let mut out = Vec::new();
    let sum100 = |v: &[f64]| ensure((v.iter().sum::<f64>() - 100.0).abs() <= 1e-5, || format!("sum {v:?}"));
    for c in arr(&f["normalize"]) {
        out.push(case(format!("normalize {}", c["name"].as_str().unwrap_or("")), || {
            let got = normalize_sizes(c["sizes"].as_array().map(Vec::as_slice), c["count"].as_u64().unwrap_or(0) as usize, &limits(&c["panels"]));
            near(&got, &f64s(&c["expected"]), 1e-6)?;
            sum100(&got)
        }));
    }
    for c in arr(&f["resize"]) {
        out.push(case(format!("resize {}", c["name"].as_str().unwrap_or("")), || {
            let got = resize_panels(&f64s(&c["sizes"]), c["handle"].as_u64().unwrap_or(0) as usize, c["delta"].as_f64().unwrap_or(0.0), &limits(&c["panels"]));
            near(&got, &f64s(&c["expected"]), 1e-6)?;
            sum100(&got)
        }));
    }
    for c in arr(&f["keys"]) {
        out.push(case(format!("keys {}", c["name"].as_str().unwrap_or("")), || {
            let rtl = c["direction"].as_str() == Some("rtl");
            let got = keyboard_resize(&f64s(&c["sizes"]), c["handle"].as_u64().unwrap_or(0) as usize, c["key"].as_str().unwrap_or(""), orientation(&c["orientation"]), rtl, &limits(&c["panels"]));
            near(&got, &f64s(&c["expected"]), 1e-6)?;
            sum100(&got)
        }));
    }
    for c in arr(&f["extents"]) {
        out.push(case(format!("extents {}", c["name"].as_str().unwrap_or("")), || near(&panel_extents(&f64s(&c["sizes"]), c["container"].as_f64().unwrap_or(0.0), c["handle"].as_f64().unwrap_or(1.0)), &f64s(&c["expected"]), 1e-6)));
    }
    for c in arr(&f["drag"]) {
        out.push(case(format!("drag {}", c["name"].as_str().unwrap_or("")), || {
            let got = drag_delta(c["px"].as_f64().unwrap_or(0.0), c["container"].as_f64().unwrap_or(0.0), c["panels"].as_u64().unwrap_or(0) as usize, orientation(&c["orientation"]), c["direction"].as_str() == Some("rtl"), 1.0);
            near(&[got], &[c["expected"].as_f64().unwrap_or(f64::NAN)], 1e-6)
        }));
    }
    out
}

fn extents(v: &Value) -> Vec<f64> {
    match v {
        Value::Array(_) => f64s(v),
        _ => vec![v["extent"].as_f64().unwrap_or(0.0); v["count"].as_u64().unwrap_or(0) as usize],
    }
}

/// `virtual-list.json`: windows, scrollToIndex, sections, sticky headers.
pub fn virtual_list_cases() -> Vec<Case> {
    let f = fixture("virtual-list.json");
    let mut out = Vec::new();
    for c in arr(&f["windows"]) {
        out.push(case(format!("window {}", c["name"].as_str().unwrap_or("")), || {
            let overscan = c["overscan"].as_u64().map(|o| o as usize).unwrap_or(exponential_ui::list::DEFAULT_OVERSCAN);
            if let Some(measured) = c["measured"].as_array() {
                // Unmeasured items: `row` until one is measured, then the mean.
                let measured: Vec<Option<f64>> = measured.iter().map(Value::as_f64).collect();
                same(&json!(item_extents(&measured, c["row"].as_f64().unwrap_or(0.0))), &c["extents"])?;
            }
            same(&to_value(&virtual_window(&extents(&c["extents"]), c["gap"].as_f64().unwrap_or(0.0), c["scroll"].as_f64().unwrap_or(0.0), c["viewport"].as_f64().unwrap_or(0.0), overscan)), &c["expected"])
        }));
    }
    for c in arr(&f["scrollTo"]) {
        out.push(case(format!("scrollTo {}", c["name"].as_str().unwrap_or("")), || {
            let align = c["align"].as_str().and_then(ScrollAlign::parse).unwrap_or_default();
            let got = scroll_offset_for_index(&extents(&c["extents"]), c["gap"].as_f64().unwrap_or(0.0), c["index"].as_u64().unwrap_or(0) as usize, c["viewport"].as_f64().unwrap_or(0.0), c["scroll"].as_f64().unwrap_or(0.0), align, c["inset"].as_f64().unwrap_or(0.0));
            same(&json!(got), &c["expected"])
        }));
    }
    for c in arr(&f["sections"]) {
        out.push(case(format!("sections {}", c["name"].as_str().unwrap_or("")), || {
            let sections = list_sections(arr(&c["items"]), c["sectionBy"].as_str().unwrap_or(""));
            let rows: Vec<Value> = section_rows(&sections)
                .into_iter()
                .map(|r| match r {
                    SectionRow::Header(h) => json!({"header": h}),
                    SectionRow::Item(i) => json!({"item": i}),
                })
                .collect();
            same(&json!({"sections": to_value(&sections), "rows": rows}), &c["expected"])
        }));
    }
    for c in arr(&f["sectionedScrollTo"]) {
        out.push(case(format!("sectionedScrollTo {}", c["name"].as_str().unwrap_or("")), || {
            let rows = section_rows(&list_sections(arr(&c["items"]), c["sectionBy"].as_str().unwrap_or("")));
            let (header, item) = (c["headerExtent"].as_f64().unwrap_or(0.0), c["itemExtent"].as_f64().unwrap_or(0.0));
            let row_extents: Vec<f64> = rows.iter().map(|r| if matches!(r, SectionRow::Header(_)) { header } else { item }).collect();
            let align = c["align"].as_str().and_then(ScrollAlign::parse).unwrap_or_default();
            let got = exponential_ui::list::scroll_offset_for_item(&rows, &row_extents, c["gap"].as_f64().unwrap_or(0.0), c["index"].as_u64().unwrap_or(0) as usize, c["viewport"].as_f64().unwrap_or(0.0), c["scroll"].as_f64().unwrap_or(0.0), align, c["stickyHeaders"].as_bool().unwrap_or(false));
            same(&json!(got), &c["expected"])
        }));
    }
    for c in arr(&f["sticky"]) {
        out.push(case(format!("sticky {}", c["name"].as_str().unwrap_or("")), || {
            let row_extents = f64s(&c["rowExtents"]);
            let offsets = item_offsets(&row_extents, 0.0);
            let headers: Vec<usize> = arr(&c["headerRows"]).iter().map(|h| h.as_u64().unwrap_or(0) as usize).collect();
            let got: Vec<Value> = f64s(&c["scrolls"]).into_iter().map(|s| to_value(&sticky_header(&offsets, &row_extents, &headers, s))).collect();
            same(&Value::Array(got), &c["expected"])
        }));
    }
    out
}

fn frame_near(got: &Value, want: &Value) -> Result<(), String> {
    for (k, v) in want.as_object().ok_or("frame")? {
        let g = &got[k];
        let ok = match (v.as_f64(), g.as_f64()) {
            (Some(a), Some(b)) => (a - b).abs() <= 1e-3,
            (None, None) => v.is_null() && g.is_null(),
            _ => false,
        };
        ensure(ok, || format!("{k}: got {g} / expected {v}"))?;
    }
    Ok(())
}

/// `animations.json`: theme × animation (timing, frames, reduced motion,
/// the CSS keyframes) + the `animationDuration` override.
pub fn animation_cases() -> Vec<Case> {
    let f = fixture("animations.json");
    let mut out = Vec::new();
    for (id, entries) in f["themes"].as_object().expect("themes") {
        for (name, e) in entries.as_object().expect("entries") {
            out.push(case(format!("{id}/{name}"), || {
                let theme = builtin_theme(id).ok_or("not a built-in")?;
                ensure(ANIMATION_NAMES.contains(&name.as_str()), || "unknown name".into())?;
                same(&to_value(&animation_timing(name, &theme, None)), &e["timing"]).map_err(|x| format!("timing {x}"))?;
                for fr in arr(&e["frames"]) {
                    let t = fr["t"].as_f64().unwrap_or(0.0);
                    let got = to_value(&animation_frame(name, t, &theme, false, None).ok_or("no frame")?);
                    frame_near(&got, &fr["frame"]).map_err(|x| format!("@{t} {x}"))?;
                }
                same(&to_value(&animation_frame(name, 5.0, &theme, true, None)), &e["reduced"]).map_err(|x| format!("reduced {x}"))?;
                same(&json!(keyframes_css(name)), &f["css"][name.as_str()]).map_err(|x| format!("css {x}"))
            }));
        }
    }
    let o = &f["override"];
    out.push(case("override", || {
        let theme = builtin_theme(o["theme"].as_str().unwrap_or("")).ok_or("theme")?;
        let name = o["name"].as_str().unwrap_or("");
        let token = o["durationToken"].as_str();
        same(&to_value(&animation_timing(name, &theme, token)), &o["timing"])?;
        frame_near(&to_value(&animation_frame(name, 60.0, &theme, false, token).ok_or("no frame")?), &o["frame"])?;
        same(&json!(exponential_ui::animation::ANIMATION_PROPERTIES_CSS), &f["properties"]).map_err(|x| format!("properties {x}"))
    }));
    // `opacity`: the painted opacity = own × the frame's (the CSS half is web-only).
    for c in arr(&f["opacity"]) {
        out.push(case(format!("opacity {}", c["name"].as_str().unwrap_or("")), || {
            let theme = builtin_theme(c["theme"].as_str().unwrap_or("")).ok_or("theme")?;
            let frame = animation_frame(c["animation"].as_str().unwrap_or(""), c["t"].as_f64().unwrap_or(0.0), &theme, false, None);
            let got = exponential_ui::animation::painted_opacity(c["own"].as_f64(), frame.as_ref());
            let want = c["expected"].as_f64().unwrap_or(f64::NAN);
            ensure((got - want).abs() <= 1e-3, || format!("{got} vs {want}"))
        }));
    }
    out
}
