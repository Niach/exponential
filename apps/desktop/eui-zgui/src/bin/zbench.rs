//! The spike harness metrics on eui-zgui (both renderers through eui-core).
//!
//! ```text
//! SPIKE_T0=$(date +%s%N) zbench bench250   # cold start → first frame, one-prop delta, RSS
//! zbench benchlist                         # 10k fixed rows, scroll 100 viewports
//! zbench benchvar                          # 10k chat rows (1-8 wrapped lines), 100 viewports
//! zbench window 15                         # real window (250 surface), exits after 15 s
//! zbench two 15                            # two surfaces in one real window, both updating
//! ```
//! A bench frame = `Doc::pass` (eui-core layout through the zgui measurer +
//! the mirror) + `Harness::settle` (zgui restyle, layout, paint into the
//! display list) with a renderer that drops the scene (no GPU submit) —
//! the same cut as the gpui reference's `layout_now` + `Window::draw`.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::{Duration, Instant};

use eui_core::theme::Mode;
use eui_zgui::{Doc, bench, fonts, host, rss_mb, since_t0_ms, stats};
use serde_json::json;
use zgui::view::View as _;

const SCALE: f64 = 2.0;

fn theme() -> Option<std::sync::Arc<eui_core::theme::ResolvedTheme>> {
    eui_core::themes::builtin_theme("exponential")
}

fn bench250() {
    let t_main = since_t0_ms();
    let started = Instant::now();
    let (comps, w, h) = bench::surface250();
    let fonts = fonts::fonts();
    let mut doc = Doc::new("s250", &fonts, theme(), Mode::Dark, w, h);
    doc.surface.set_pixel_snap(Some(SCALE as f32));
    doc.surface.set_components(comps);
    doc.pass();
    let doc = Rc::new(RefCell::new(doc));
    let d = doc.clone();
    let rt = host::runtime(&fonts, w, h, host::Paint::Nowhere, move |cx| Box::new(d.borrow().view().build(cx)));
    let mut harness = host::open(rt, w, h, SCALE);
    let first_in_process = started.elapsed().as_secs_f64() * 1000.0;
    let first_from_t0 = since_t0_ms();
    let rss_first = rss_mb();
    let mut deltas = Vec::new();
    let mut restyled = 0;
    for k in 0..220 {
        let c = serde_json::from_value(json!({"id": "t17", "component": "Text", "text": format!("Item 17 changed {k}"), "variant": "caption"})).unwrap();
        let t = Instant::now();
        {
            let mut d = doc.borrow_mut();
            d.surface.update_components(vec![c]);
            restyled = d.pass().restyled;
        }
        host::frame(&mut harness);
        if k >= 20 {
            deltas.push(t.elapsed().as_secs_f64() * 1000.0);
        }
    }
    let (dm, dp, dx, dmed) = stats(deltas);
    println!("{}", json!({"renderer": "zgui", "t0_to_main_ms": t_main, "first_frame_in_process_ms": first_in_process, "first_frame_from_t0_ms": first_from_t0, "rss_after_first_frame_mb": rss_first, "delta_mean_ms": dm, "delta_p95_ms": dp, "delta_max_ms": dx, "delta_median_ms": dmed, "delta_restyled_nodes": restyled}));
    std::mem::forget(harness);
}

fn benchlist(variable: bool) {
    let (lw, lh) = (390.0_f32, 800.0_f32);
    let fonts = fonts::fonts();
    let data = bench::rows(variable);
    let t = Instant::now();
    let mut doc = Doc::new("list", &fonts, theme(), Mode::Dark, lw, lh);
    doc.surface.set_pixel_snap(Some(SCALE as f32));
    doc.surface.set_data("/rows", Some(data)).unwrap();
    doc.surface.set_components(bench::list_components(variable));
    doc.pass();
    let doc = Rc::new(RefCell::new(doc));
    let d = doc.clone();
    let rt = host::runtime(&fonts, lw, lh, host::Paint::Nowhere, move |cx| Box::new(d.borrow().view().build(cx)));
    let mut harness = host::open(rt, lw, lh, SCALE);
    let list_first = t.elapsed().as_secs_f64() * 1000.0;
    let rss_first = rss_mb();
    let mut steps = Vec::new();
    let mut layout = Vec::new();
    let mut mirror = Vec::new();
    let mut settle = Vec::new();
    let mut restyled = Vec::new();
    let mut nodes = 0;
    for k in 1..=100 {
        let t = Instant::now();
        {
            let mut d = doc.borrow_mut();
            d.surface.scroll("root", k as f32 * lh);
            let s = d.pass();
            layout.push(s.layout_ms);
            mirror.push(s.mirror_ms);
            restyled.push(s.restyled as f64);
            nodes = s.nodes;
        }
        let ts = Instant::now();
        host::frame(&mut harness);
        settle.push(ts.elapsed().as_secs_f64() * 1000.0);
        steps.push(t.elapsed().as_secs_f64() * 1000.0);
    }
    let reached = doc.borrow().surface.scroll_offset("root").1;
    let (m, p, x, med) = stats(steps);
    let (lm, _, _, _) = stats(layout);
    let (mm, _, _, _) = stats(mirror);
    let (sm, _, _, _) = stats(settle);
    let (rm, _, _, _) = stats(restyled);
    println!("{}", json!({"renderer": "zgui", "variable": variable, "list_first_frame_ms": list_first, "rss_after_list_first_mb": rss_first, "scroll_mean_ms": m, "scroll_p95_ms": p, "scroll_max_ms": x, "scroll_median_ms": med, "core_layout_mean_ms": lm, "mirror_mean_ms": mm, "zgui_frame_mean_ms": sm, "restyled_mean": rm, "scroll_reached_px": reached, "placed_nodes": nodes, "rss_after_scroll_mb": rss_mb()}));
    std::mem::forget(harness);
}

/// A real window on the live display; exits by itself after `secs`.
fn window(secs: u64, two: bool) {
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_secs(secs));
        std::process::exit(0);
    });
    let (comps, w, h) = bench::surface250();
    let fonts = fonts::fonts();
    let mut a = Doc::new("a", &fonts, theme(), Mode::Dark, w, h.min(900.0));
    a.surface.set_components(comps.clone());
    a.pass();
    let a = Rc::new(RefCell::new(a));
    let b = two.then(|| {
        let mut b = Doc::new("b", &fonts, theme(), Mode::Dark, w, h.min(900.0));
        b.surface.set_components(comps);
        b.pass();
        Rc::new(RefCell::new(b))
    });
    let (a2, b2) = (a.clone(), b.clone());
    let total_w = if two { w * 2.0 } else { w };
    zgui::app()
        .with_title("eui-zgui")
        .with_size(total_w, h.min(900.0))
        .with_fonts(fonts)
        .run(move || {
            eprintln!("ZGUI first_build_from_t0_ms {:?}", since_t0_ms());
            let mut root = zgui::elements::r#box().style_text(Some(format!("position:relative;width:{total_w}px;height:{}px", h.min(900.0))));
            root = root.child(a2.borrow().view());
            if let Some(b) = &b2 {
                root = root.child(zgui::elements::r#box().style_text(Some(format!("position:absolute;left:{w}px;top:0px;width:{w}px;height:{}px", h.min(900.0)))).child(b.borrow().view()));
            }
            root
        })
        .expect("the window runs");
}

fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    match a.first().map(String::as_str) {
        Some("bench250") => bench250(),
        Some("benchlist") => benchlist(false),
        Some("benchvar") => benchlist(true),
        Some("window") => window(a.get(1).and_then(|s| s.parse().ok()).unwrap_or(15), false),
        Some("two") => window(a.get(1).and_then(|s| s.parse().ok()).unwrap_or(15), true),
        _ => eprintln!("usage: zbench bench250 | benchlist | benchvar | window <secs> | two <secs>"),
    }
}
