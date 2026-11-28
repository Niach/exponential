//! #PERF-15: ONE prop delta in a 250-component surface (`bench_delta.json`,
//! the gpui → zgui spike's surface: 1 Box root, 1 heading Text, 62 cards of
//! Box + Text + Row + Button) through the gpui painter, HEADLESS (as
//! `bench_list`): gpui's Linux text system with the conformance fonts,
//! 1280 × 1240. Every delta is an `updateComponents` of one component,
//! then a full frame: the core pass (`layout_now`) plus gpui's layout,
//! prepaint and paint of the element tree (`Window::draw`). The split:
//! `apply` (the update), `pass` (reduce + rebuild + restyle + taffy +
//! measure + the painter's caches; `core` = the core's own share of it),
//! `flush` (gpui's effects after the update) and `draw` (gpui). 20 warm-up
//! deltas, 200 timed.
//!
//! ```text
//! RUSTUP_TOOLCHAIN=1.96.0 cargo run --release -p eui-gpui --example bench_delta
//! ```

#![allow(dead_code)]

#[path = "conformance_dump.rs"]
mod conformance_dump;

fn main() {
    #[cfg(any(target_os = "linux", target_os = "freebsd"))]
    linux::run();
    #[cfg(not(any(target_os = "linux", target_os = "freebsd")))]
    eprintln!("bench_delta: the headless text system is wired for Linux/FreeBSD only");
}

#[cfg(any(target_os = "linux", target_os = "freebsd"))]
mod linux {
    use std::borrow::Cow;
    use std::rc::Rc;
    use std::sync::Arc;
    use std::time::Instant;

    use eui_core::theme::Mode;
    use eui_core::types::FlatComponent;
    use eui_gpui::host::HostPlugin;
    use eui_gpui::view::{SurfaceView, SurfaceViewOptions};
    use gpui::{px, size, AppContext as _, HeadlessAppContext, PlatformTextSystem};
    use serde_json::{json, Value};

    use super::conformance_dump::{font_set, manifest, setup::FontHost};

    #[derive(Default, Clone, Copy)]
    struct Split {
        apply: f64,
        flush: f64,
        pass: f64,
        core: f64,
        draw: f64,
        total: f64,
    }

    fn stats(mut v: Vec<f64>) -> (f64, f64, f64) {
        let mean = v.iter().sum::<f64>() / v.len() as f64;
        v.sort_by(f64::total_cmp);
        let p95 = v[((v.len() as f64 * 0.95).ceil() as usize).min(v.len()) - 1];
        (mean, v[v.len() / 2], p95)
    }

    pub fn run() {
        let fixture: Value = serde_json::from_str(include_str!("bench_delta.json")).expect("bench_delta.json");
        let components: Vec<FlatComponent> = serde_json::from_value(fixture["components"].clone()).expect("components");
        let (w, h) = (fixture["viewport"]["width"].as_f64().unwrap() as f32, fixture["viewport"]["height"].as_f64().unwrap() as f32);

        let fonts = font_set(&manifest());
        let text_system = Arc::new(gpui_wgpu::CosmicTextSystem::new_without_system_fonts(&fonts.default));
        text_system.add_fonts(fonts.files.iter().map(|(_, b)| Cow::Owned(b.clone())).collect()).expect("the conformance fonts load");
        let mut cx = HeadlessAppContext::new(text_system);
        cx.update(gpui_component::init);
        let host: Rc<dyn HostPlugin> = Rc::new(FontHost(fonts.families.clone()));
        let options = SurfaceViewOptions {
            surface_id: "bench".into(),
            theme: eui_core::themes::builtin_theme(fixture["theme"].as_str().unwrap()),
            mode: Mode::Dark,
            host,
            width: Some(w),
            ..Default::default()
        };
        let window = cx.open_window(size(px(w), px(h)), move |window, cx| cx.new(|cx| SurfaceView::new(options, window, cx))).expect("a headless window");

        let t = Instant::now();
        window
            .update(&mut cx, |view, window, cx| {
                view.set_components(components.clone(), cx);
                view.set_viewport_height(h, cx);
                view.layout_now(window, cx);
                cx.notify();
            })
            .unwrap();
        cx.update_window(window.into(), |_, window, cx| window.draw(cx).clear(cx)).unwrap();
        let first = t.elapsed().as_secs_f64() * 1000.0;

        let only = std::env::var("BENCH_DELTA").ok();
        // Each case: a name and the one component it re-sends (k = the step).
        type Case = (&'static str, fn(usize) -> Value);
        let cases: [Case; 4] = [
            ("text", |k| json!({"id": "t17", "component": "Text", "text": format!("Item 17 changed {k}"), "variant": "caption"})),
            ("same-size text", |k| json!({"id": "t17", "component": "Text", "text": format!("Item 17 caption{}", k % 2), "variant": "caption"})),
            ("paint-only style", |k| json!({"id": "c17", "component": "Box", "style": {"display": "flex", "flexDirection": "column", "gap": 4, "width": 196, "padding": 8, "borderWidth": 1, "borderColor": if k % 2 == 0 { "$color.primary" } else { "$color.border" }, "borderRadius": 8}, "children": ["t17", "r17", "b17"]})),
            ("button label", |k| json!({"id": "b17", "component": "Button", "label": format!("Open {k}"), "variant": "outline", "size": "sm"})),
        ];
        println!("gpui delta (250 components, bench_delta.json, headless frames, ms mean / median / p95 of 200); first frame {first:.1} ms");
        for (name, make) in cases {
            if only.as_deref().is_some_and(|o| o != name) {
                continue;
            }
            let mut splits = Vec::new();
            for k in 0..220 {
                let c: FlatComponent = serde_json::from_value(make(k)).unwrap();
                let mut s = Split::default();
                let t0 = Instant::now();
                let closed = window
                    .update(&mut cx, |view, window, cx| {
                        let a = Instant::now();
                        view.update_components(vec![c], cx);
                        s.apply = a.elapsed().as_secs_f64() * 1000.0;
                        let p = Instant::now();
                        let stats = view.layout_now(window, cx);
                        s.core = stats.layout_ns as f64 / 1e6;
                        s.pass = p.elapsed().as_secs_f64() * 1000.0;
                        cx.notify();
                        Instant::now()
                    })
                    .unwrap();
                s.flush = closed.elapsed().as_secs_f64() * 1000.0;
                let d = Instant::now();
                cx.update_window(window.into(), |_, window, cx| window.draw(cx).clear(cx)).unwrap();
                s.draw = d.elapsed().as_secs_f64() * 1000.0;
                s.total = t0.elapsed().as_secs_f64() * 1000.0;
                if k >= 20 {
                    splits.push(s);
                }
            }
            let col = |f: fn(&Split) -> f64| stats(splits.iter().map(f).collect());
            let fmt = |(m, med, p): (f64, f64, f64)| format!("{m:.3} / {med:.3} / {p:.3}");
            println!("  {name:<17} total {}", fmt(col(|s| s.total)));
            println!("    apply {} · pass {} (core {}) · flush {} · draw {}", fmt(col(|s| s.apply)), fmt(col(|s| s.pass)), fmt(col(|s| s.core)), fmt(col(|s| s.flush)), fmt(col(|s| s.draw)));
        }
        std::mem::forget(cx);
    }
}
