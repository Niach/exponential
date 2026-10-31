//! EXP-1493: `fixtures/carousel.json` replayed case by case (THE Carousel
//! spec, `src/carousel.ts`, round-6 contract §3): shared by
//! `carousel_controls.rs` and the conformance runner (suite `carousel`).
//! The core paints no pixels, so a case checks what a painter draws from:
//! the root's flow frames (order, gap, the counter line above the track on
//! the start edge, rtl mirrored), the counter's resolved visual + text
//! style, and the `Carousel/indicator` recipe resolved per dot (`selected`
//! on the current page).

use std::collections::HashMap;

use eui_core::measure::FixedMeasure;
use eui_core::surface::{Surface, SurfaceOptions};
use eui_core::theme::{resolve_recipe, Mode, RecipeQuery};
use eui_core::themes::builtin_theme;
use eui_core::NestedNode;
use serde_json::{json, Value};

use super::round2::case;
use super::{fixture, Case};

fn ensure(ok: bool, msg: impl FnOnce() -> String) -> Result<(), String> {
    if ok {
        Ok(())
    } else {
        Err(msg())
    }
}

/// `#rgb[a]` / `#rrggbb[aa]` → lowercase `#rrggbbaa` (opaque = `ff`).
fn norm(c: &str) -> String {
    let h = c.trim().trim_start_matches('#').to_ascii_lowercase();
    let h = if h.len() <= 4 { h.chars().flat_map(|ch| [ch, ch]).collect() } else { h };
    if h.len() == 6 {
        format!("#{h}ff")
    } else {
        format!("#{h}")
    }
}

fn near(a: f32, b: f32) -> bool {
    (a - b).abs() <= 1.0
}

fn run(c: &Value, width: f32) -> Result<(), String> {
    let theme = builtin_theme(c["theme"].as_str().unwrap_or("")).ok_or("not a built-in")?;
    let mode = Mode::parse(c["mode"].as_str().unwrap_or("")).ok_or("bad mode")?;
    let rtl = c["direction"] == "rtl";
    let pages = c["pages"].as_u64().unwrap_or(0) as usize;
    let children: Vec<Value> = (1..=pages).map(|n| json!({"id": format!("p{n}"), "component": "Text", "props": {"text": format!("Page {n}")}})).collect();
    let tree = json!({"id": "deck", "component": "Carousel", "props": c["props"], "style": {"direction": c["direction"]}, "children": children});
    let tree: NestedNode = serde_json::from_value(tree).map_err(|e| e.to_string())?;
    let mut s = Surface::new("carousel", SurfaceOptions { theme: Some(theme.clone()), mode, ..SurfaceOptions::default() });
    let outcome = s.set_nested(tree);
    ensure(outcome.issues.is_empty(), || format!("issues {:?}", outcome.issues))?;
    s.set_viewport(width, 0.0, None);
    let out = s.layout(&mut FixedMeasure { sizes: HashMap::new(), wrap: true });
    let nodes = s.nodes();
    let frame = |i: u32| out.frames.iter().find(|f| f.index == i).copied();
    let root = s.index_of("deck").ok_or("no root")?;
    let rf = frame(root).ok_or("root unframed")?;
    let e = &c["expected"];

    // The root's flow parts top to bottom, `gap` apart.
    let mut flow: Vec<(String, _)> = nodes
        .iter()
        .filter(|n| n.parent == Some(root) && !n.removed && !n.hidden)
        .filter_map(|n| Some((n.part.clone()?, frame(n.index)?)))
        .collect();
    flow.sort_by(|a, b| a.1.y.total_cmp(&b.1.y));
    let order: Vec<&str> = flow.iter().map(|(p, _)| p.as_str()).collect();
    let want: Vec<&str> = e["order"].as_array().unwrap().iter().filter_map(Value::as_str).collect();
    ensure(order == want, || format!("order {order:?}, expected {want:?}"))?;
    let gap = e["gap"].as_f64().unwrap_or(0.0) as f32;
    for w in flow.windows(2) {
        let (a, b) = (&w[0], &w[1]);
        ensure(near(b.1.y - (a.1.y + a.1.h), gap), || format!("{} → {}: {} apart, expected {gap}", a.0, b.0, b.1.y - (a.1.y + a.1.h)))?;
    }

    // The counter: its own line above the track, on the root's start edge.
    let counter = s.index_of("deck.counter").filter(|i| nodes.get(*i as usize).is_some_and(|n| !n.removed));
    match (&e["counter"], counter) {
        (Value::Null, None) => {}
        (Value::Null, Some(_)) => return Err("a counter, expected none".into()),
        (_, None) => return Err("no counter".into()),
        (want, Some(ci)) => {
            let cf = frame(ci).ok_or("counter unframed")?;
            let track = frame(s.index_of("deck.page").ok_or("no track")?).ok_or("track unframed")?;
            ensure(cf.y + cf.h <= track.y + 0.001, || format!("the counter {cf:?} intersects the track {track:?}"))?;
            let edge = if rtl { (rf.x + rf.w) - (cf.x + cf.w) } else { cf.x - rf.x };
            ensure(edge.abs() <= 1.0, || format!("{edge} px off the root's start edge"))?;
            ensure(cf.w < rf.w, || format!("the counter spans the root ({}), expected it to hug its text", cf.w))?;
            ensure(near(cf.h, want["height"].as_f64().unwrap() as f32), || format!("counter height {}", cf.h))?;
            let text = nodes[ci as usize].props.get("text").and_then(Value::as_str).unwrap_or("");
            ensure(text == want["text"].as_str().unwrap(), || format!("counter text {text:?}"))?;
            let v = s.visual(ci).ok_or("counter unresolved")?;
            let color = v.color.as_deref().map(norm);
            ensure(color.as_deref() == Some(norm(want["color"].as_str().unwrap()).as_str()), || format!("counter colour {color:?}"))?;
            ensure(v.background_color.as_deref().is_none_or(|b| norm(b).ends_with("00")), || format!("the counter paints a fill {:?} (no pill)", v.background_color))?;
            ensure(v.border_radius.unwrap_or(0.0) == 0.0, || format!("the counter has a radius {:?} (no pill)", v.border_radius))?;
            let ts = s.text_style(ci).ok_or("no text style")?;
            ensure(ts.font_size == want["fontSize"].as_f64().unwrap() as f32, || format!("counter font size {}", ts.font_size))?;
        }
    }

    // The dots: the indicator recipe resolved per dot, the current = selected.
    let dots = e["dots"].as_array().unwrap();
    let ind = s.index_of("deck.indicator").filter(|i| nodes.get(*i as usize).is_some_and(|n| !n.removed));
    match ind {
        None => ensure(dots.is_empty(), || format!("no dots row, expected {}", dots.len())),
        Some(ii) => {
            let p = &nodes[ii as usize].props;
            let count = p.get("count").and_then(Value::as_u64).unwrap_or(0) as usize;
            let page = p.get("page").and_then(Value::as_u64).unwrap_or(0) as usize;
            ensure(count == dots.len(), || format!("{count} dots, expected {}", dots.len()))?;
            let owner = s.layout_node(root).map(|n| n.props.clone()).unwrap_or_default();
            for (i, want) in dots.iter().enumerate() {
                let states = if i == page { vec!["selected".to_string()] } else { vec![] };
                let r = resolve_recipe(&theme, &RecipeQuery::new("Carousel", "indicator", owner.clone(), states), mode);
                let fill = r.get("backgroundColor").and_then(Value::as_str).map(norm);
                ensure(fill.as_deref() == Some(norm(want["color"].as_str().unwrap()).as_str()), || format!("dot {i} fill {fill:?}"))?;
                for k in ["width", "height"] {
                    let got = r.get(k).and_then(Value::as_f64);
                    ensure(got == want[k].as_f64(), || format!("dot {i} {k} {got:?}"))?;
                }
            }
            Ok(())
        }
    }
}

pub fn carousel_cases() -> Vec<Case> {
    let f = fixture("carousel.json");
    let width = f["width"].as_f64().unwrap_or(360.0) as f32;
    f["cases"].as_array().unwrap().iter().map(|c| case(c["name"].as_str().unwrap_or(""), || run(c, width))).collect()
}
