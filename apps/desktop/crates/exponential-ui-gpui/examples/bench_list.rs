//! Round 2 (docs/round-2-contract.md §5): the shared 100,000-row list bench
//! (`packages/exponential-ui/fixtures/bench-list.json`) through the gpui
//! painter, HEADLESS: gpui's Linux text system with the conformance fonts
//! (as `conformance_dump`), a 390 × 800 window, the neutral theme. Every
//! step is a full frame: the core pass (`layout_now`) plus gpui's own
//! layout, prepaint and paint of the element tree (`Window::draw`).
//!
//! ```text
//! RUSTUP_TOOLCHAIN=1.96.0 cargo run --release -p exponential-ui-gpui --example bench_list
//! ```

#![allow(dead_code)]

#[path = "conformance_dump.rs"]
mod conformance_dump;

fn main() {
    #[cfg(any(target_os = "linux", target_os = "freebsd"))]
    linux::run();
    #[cfg(not(any(target_os = "linux", target_os = "freebsd")))]
    eprintln!("bench_list: the headless text system is wired for Linux/FreeBSD only");
}

#[cfg(any(target_os = "linux", target_os = "freebsd"))]
mod linux {
    use std::borrow::Cow;
    use std::rc::Rc;
    use std::sync::Arc;
    use std::time::Instant;

    use exponential_ui::types::FlatComponent;
    use exponential_ui_gpui::host::HostPlugin;
    use exponential_ui_gpui::view::{SurfaceView, SurfaceViewOptions};
    use gpui::{px, size, AppContext as _, HeadlessAppContext, PlatformTextSystem};
    use serde_json::{json, Value};

    use super::conformance_dump::{font_set, manifest, read_json, setup::FontHost};

    pub fn run() {
        let fixture = read_json("packages/exponential-ui/fixtures/bench-list.json");
        let count = fixture["rows"]["count"].as_u64().expect("count") as usize;
        let rows: Vec<Value> = (0..count).map(|i| json!({"id": format!("r{i}"), "title": format!("Row {i}"), "meta": (i % 97).to_string()})).collect();
        let components: Vec<FlatComponent> = serde_json::from_value(fixture["components"].clone()).expect("components");
        let (w, h) = (fixture["viewport"]["width"].as_f64().unwrap() as f32, fixture["viewport"]["height"].as_f64().unwrap() as f32);
        let steps = fixture["steps"]["scroll"]["count"].as_u64().unwrap() as usize;
        let index = fixture["steps"]["scrollToIndex"]["index"].as_u64().unwrap() as u32;

        let fonts = font_set(&manifest());
        let text_system = Arc::new(gpui_wgpu::CosmicTextSystem::new_without_system_fonts(&fonts.default));
        text_system.add_fonts(fonts.files.iter().map(|(_, b)| Cow::Owned(b.clone())).collect()).expect("the conformance fonts load");
        let mut cx = HeadlessAppContext::new(text_system);
        cx.update(gpui_component::init);
        let host: Rc<dyn HostPlugin> = Rc::new(FontHost(fonts.families.clone()));
        let options = SurfaceViewOptions {
            surface_id: "bench".into(),
            theme: exponential_ui::themes::builtin_theme(fixture["theme"].as_str().unwrap()),
            host,
            width: Some(w),
            ..Default::default()
        };
        let window = cx.open_window(size(px(w), px(h)), move |window, cx| cx.new(|cx| SurfaceView::new(options, window, cx))).expect("a headless window");
        let frame = |cx: &mut HeadlessAppContext, f: &mut dyn FnMut(&mut SurfaceView, &mut gpui::Window, &mut gpui::Context<SurfaceView>)| {
            window
                .update(cx, |view, window, cx| {
                    f(view, window, cx);
                    view.layout_now(window, cx);
                    cx.notify();
                })
                .expect("the window updates");
            cx.update_window(window.into(), |_, window, cx| window.draw(cx).clear(cx)).expect("a frame draws");
        };

        let t = Instant::now();
        frame(&mut cx, &mut |view, _, cx| {
            view.set_data("/rows", Some(Value::Array(rows.clone())), cx).expect("data");
            view.set_components(components.clone(), cx);
            view.set_viewport_height(h, cx);
        });
        let first_paint = t.elapsed().as_secs_f64() * 1000.0;

        let mut total = 0.0;
        for k in 1..=steps {
            let t = Instant::now();
            frame(&mut cx, &mut |view, _, _| {
                view.surface_mut().scroll("root", k as f32 * h);
            });
            total += t.elapsed().as_secs_f64() * 1000.0;
        }

        let t = Instant::now();
        frame(&mut cx, &mut |view, _, cx| view.scroll_to_index("root", index, Some("start"), cx));
        let jump = t.elapsed().as_secs_f64() * 1000.0;
        let (rendered, bound) = window
            .update(&mut cx, |view, _, _| {
                let alive: Vec<u32> = view.placed_nodes().iter().filter(|n| !n.removed && n.id.strip_prefix("bench-row.r").is_some_and(|k| !k.contains('.'))).map(|n| n.index).collect();
                assert!(view.index_of(&format!("bench-row.r{index}")).is_some(), "item {index} rendered after the jump");
                // bench-list.json: renderedItems ≤ the window + 2 × overscan
                // + the pinned header (the window = the rows the viewport
                // can show, the shortest rendered row's height).
                let shortest = alive.iter().filter_map(|i| view.frame(*i)).map(|f| f.h).filter(|h| *h > 0.0).fold(f32::INFINITY, f32::min);
                let window_rows = (h / shortest).ceil() as usize + 1;
                (alive.len(), window_rows + 2 * exponential_ui::list::DEFAULT_OVERSCAN + 1)
            })
            .expect("the window updates");
        assert!(rendered <= bound, "renderedItems {rendered} > the window + 2 × overscan + the pinned header ({bound})");
        println!("gpui list (100,000 ListRows, bench-list.json, headless frames): firstPaintMs {first_paint:.1}, scrollStepMs {:.3}, scrollToIndexMs {jump:.3}, renderedItems {rendered}", total / steps as f64);
    }
}
