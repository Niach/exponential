//! VAPP-105: the Exponential UI benchmark suite through the gpui painter,
//! HEADLESS (as `bench_list`): gpui's Linux text system with the conformance
//! fonts, a window of the scenario's viewport, every scenario of
//! `packages/exponential-ui-bench/scenarios/manifest.json`. A pass is a full
//! frame: the core pass (`layout_now`) plus gpui's own layout, prepaint and
//! paint of the element tree (`Window::draw`). Writes the result fragment
//! (`packages/exponential-ui-bench/src/results.ts`) to
//! `$EXPONENTIAL_UI_BENCH_OUT` and prints it.
//!
//! ```text
//! RUSTUP_TOOLCHAIN=1.96.0 cargo run --release -p exponential-ui-gpui --example bench_suite -- [--scenario <id>] [--repeat <n>]
//! ```

#![allow(dead_code)]

#[path = "conformance_dump.rs"]
mod conformance_dump;

fn main() {
    #[cfg(any(target_os = "linux", target_os = "freebsd"))]
    linux::run();
    #[cfg(not(any(target_os = "linux", target_os = "freebsd")))]
    eprintln!("bench_suite: the headless text system is wired for Linux/FreeBSD only");
}

#[cfg(any(target_os = "linux", target_os = "freebsd"))]
mod linux {
    use std::borrow::Cow;
    use std::collections::{BTreeMap, BTreeSet};
    use std::path::PathBuf;
    use std::rc::Rc;
    use std::sync::Arc;
    use std::time::{Instant, SystemTime, UNIX_EPOCH};

    use exponential_ui::theme::Mode;
    use exponential_ui::themes::builtin_theme;
    use exponential_ui::types::{FlatComponent, NestedNode};
    use exponential_ui_gpui::host::HostPlugin;
    use exponential_ui_gpui::view::{SurfaceView, SurfaceViewOptions};
    use gpui::{px, size, AppContext as _, HeadlessAppContext, PlatformTextSystem, Window, WindowHandle};
    use serde_json::{json, Map, Value};

    use super::conformance_dump::{font_set, manifest, read_json, setup::FontHost};

    // -- scenario files + the data generator (src/scenarios.ts `generateItems`) --

    fn scenario(name: &str) -> Value {
        read_json(&format!("packages/exponential-ui-bench/scenarios/{name}"))
    }

    /// `{i}` = the index, `{i % N}` = the index modulo N; every field a string.
    fn fill(template: &str, i: usize) -> String {
        let mut s = template.to_string();
        while let Some(start) = s.find("{i % ") {
            let Some(end) = s[start..].find('}') else { break };
            let n: usize = s[start + 5..start + end].trim().parse().expect("modulus");
            s.replace_range(start..start + end + 1, &(i % n).to_string());
        }
        s.replace("{i}", &i.to_string())
    }

    fn generate_items(spec: &Value) -> Vec<Value> {
        let count = spec["count"].as_u64().expect("count") as usize;
        let item = spec["item"].as_object().expect("item");
        (0..count).map(|i| Value::Object(item.iter().map(|(k, t)| (k.clone(), Value::String(fill(t.as_str().expect("template"), i)))).collect())).collect()
    }

    // -- process facts --

    /// The process's resident footprint in MB (`/proc/self/status` VmRSS).
    fn memory_mb() -> Option<f64> {
        let status = std::fs::read_to_string("/proc/self/status").ok()?;
        let line = status.lines().find(|l| l.starts_with("VmRSS:"))?;
        let kb: f64 = line.split_whitespace().nth(1)?.parse().ok()?;
        Some(kb / 1024.0)
    }

    fn iso_now() -> String {
        let secs = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0) as i64;
        let days = secs.div_euclid(86_400);
        let rem = secs.rem_euclid(86_400);
        let z = days + 719_468;
        let era = z.div_euclid(146_097);
        let doe = z.rem_euclid(146_097);
        let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
        let y = yoe + era * 400;
        let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
        let mp = (5 * doy + 2) / 153;
        let d = doy - (153 * mp + 2) / 5 + 1;
        let m = if mp < 10 { mp + 3 } else { mp - 9 };
        let y = if m <= 2 { y + 1 } else { y };
        format!("{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z", rem / 3600, (rem % 3600) / 60, rem % 60)
    }

    // -- statistics --

    fn median(v: &[f64]) -> f64 {
        if v.is_empty() {
            return 0.0;
        }
        let mut s = v.to_vec();
        s.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let n = s.len();
        if n % 2 == 1 {
            s[n / 2]
        } else {
            (s[n / 2 - 1] + s[n / 2]) / 2.0
        }
    }

    fn percentile(v: &[f64], p: f64) -> f64 {
        if v.is_empty() {
            return 0.0;
        }
        let mut s = v.to_vec();
        s.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let rank = ((p / 100.0) * (s.len() as f64 - 1.0)).round() as usize;
        s[rank.min(s.len() - 1)]
    }

    fn ms(t: Instant) -> f64 {
        t.elapsed().as_secs_f64() * 1000.0
    }

    fn round3(v: f64) -> f64 {
        (v * 1000.0).round() / 1000.0
    }

    type Metrics = BTreeMap<String, f64>;

    // -- one headless surface view per scenario --

    struct Bench {
        window: WindowHandle<SurfaceView>,
        w: f32,
        h: f32,
        first_paint_ms: f64,
        /// The last frame's stats: (upcalls, restyled, nodes).
        upcalls: u32,
        restyled: u32,
    }

    impl Bench {
        /// Open a headless window of the scenario's viewport, hand it the
        /// content and data, draw the first frame. `started` = an earlier
        /// instant the first-paint clock starts at (the cold start).
        fn new(cx: &mut HeadlessAppContext, host: &Rc<dyn HostPlugin>, sc: &Value, started: Option<Instant>) -> Bench {
            let (w, h) = (sc["viewport"]["width"].as_f64().unwrap() as f32, sc["viewport"]["height"].as_f64().unwrap() as f32);
            let mode = if sc["mode"].as_str() == Some("dark") { Mode::Dark } else { Mode::Light };
            // VAPP-149: the generated rows are built BEFORE the clock starts
            // and handed over inside it (firstPaintMs times the renderer).
            let mut generated = sc["data"].get("generate").map(|spec| (spec["path"].as_str().unwrap().to_string(), Value::Array(generate_items(spec))));
            let t = started.unwrap_or_else(Instant::now);
            let options = SurfaceViewOptions {
                surface_id: sc["id"].as_str().unwrap_or("bench").into(),
                theme: builtin_theme(sc["theme"].as_str().unwrap()),
                host: host.clone(),
                width: Some(w),
                ..Default::default()
            };
            let window = cx.open_window(size(px(w), px(h)), move |window, cx| cx.new(|cx| SurfaceView::new(options, window, cx))).expect("a headless window");
            let mut bench = Bench { window, w, h, first_paint_ms: 0.0, upcalls: 0, restyled: 0 };
            let data = sc["data"].clone();
            let surface = sc["surface"].clone();
            bench.frame(cx, &mut |view, _, cx| {
                view.set_mode(mode, cx);
                match (&data, generated.take()) {
                    (_, Some((path, items))) => view.set_data(&path, Some(items), cx).expect("generated data"),
                    (Value::Null, None) => {}
                    (d, None) => view.set_data("", Some(d.clone()), cx).expect("literal data"),
                }
                if let Some(nested) = surface.get("nested") {
                    let tree: NestedNode = serde_json::from_value(nested.clone()).expect("nested tree");
                    let out = view.set_nested(tree, cx);
                    assert!(out.issues.is_empty(), "{:?}", out.issues);
                } else {
                    let components: Vec<FlatComponent> = serde_json::from_value(surface["components"].clone()).expect("components");
                    let out = view.set_components(components, cx);
                    assert!(out.issues.is_empty(), "{:?}", out.issues);
                }
                view.set_viewport_height(h, cx);
            });
            bench.first_paint_ms = ms(t);
            bench
        }

        /// One full frame: `f`, the core pass, gpui's layout + prepaint + paint.
        fn frame(&mut self, cx: &mut HeadlessAppContext, f: &mut dyn FnMut(&mut SurfaceView, &mut Window, &mut gpui::Context<SurfaceView>)) -> f64 {
            let t = Instant::now();
            let (upcalls, restyled) = self
                .window
                .update(cx, |view, window, cx| {
                    f(view, window, cx);
                    let stats = view.layout_now(window, cx);
                    let out = (stats.upcalls, stats.restyled);
                    cx.notify();
                    out
                })
                .expect("the window updates");
            cx.update_window(self.window.into(), |_, window, cx| window.draw(cx).clear(cx)).expect("a frame draws");
            self.upcalls = upcalls;
            self.restyled = restyled;
            ms(t)
        }

        fn read<T>(&self, cx: &mut HeadlessAppContext, f: impl FnOnce(&SurfaceView) -> T) -> T {
            self.window.update(cx, |view, _, _| f(view)).expect("the window reads")
        }

        fn nodes(&self, cx: &mut HeadlessAppContext) -> f64 {
            self.read(cx, |view| view.placed_nodes().iter().filter(|n| !n.removed).count() as f64)
        }

        /// Alternate `widths` for `passes` frames: (median ms, max upcalls).
        fn width_passes(&mut self, cx: &mut HeadlessAppContext, steps: &Value) -> (f64, f64) {
            let widths: Vec<f32> = steps["widths"].as_array().unwrap().iter().map(|v| v.as_f64().unwrap() as f32).collect();
            let passes = steps["passes"].as_u64().unwrap() as usize;
            let mut times = Vec::with_capacity(passes);
            let mut upcalls = 0u32;
            for p in 0..passes {
                let w = widths[p % widths.len()];
                times.push(self.frame(cx, &mut |view, _, cx| view.set_width(w, cx)));
                upcalls = upcalls.max(self.upcalls);
            }
            let w = self.w;
            self.frame(cx, &mut |view, _, cx| view.set_width(w, cx));
            (median(&times), upcalls as f64)
        }

        fn close(self, cx: &mut HeadlessAppContext) {
            let _ = self.window.update(cx, |_, window, _| window.remove_window());
        }
    }

    fn run_scenario(cx: &mut HeadlessAppContext, host: &Rc<dyn HostPlugin>, sc: &Value, first_in_process: bool) -> Metrics {
        let id = sc["id"].as_str().unwrap();
        let steps = &sc["steps"];
        let mut m = Metrics::new();
        match id {
            "cold-start" => {
                if first_in_process {
                    let cold = Bench::new(cx, host, sc, Some(Instant::now()));
                    m.insert("coldStartMs".into(), cold.first_paint_ms);
                    cold.close(cx);
                }
                let warm = Bench::new(cx, host, sc, None);
                m.insert("firstPaintMs".into(), warm.first_paint_ms);
                m.insert("nodes".into(), warm.nodes(cx));
                if let Some(mb) = memory_mb() {
                    m.insert("memoryMb".into(), mb);
                }
                warm.close(cx);
            }
            "card-grid" => {
                let mut b = Bench::new(cx, host, sc, None);
                let (pass, upcalls) = b.width_passes(cx, steps);
                m.insert("warmPassMs".into(), pass);
                m.insert("upcallsPerPass".into(), upcalls);
                b.close(cx);
            }
            "list-100k" => {
                let mut b = Bench::new(cx, host, sc, None);
                m.insert("firstPaintMs".into(), b.first_paint_ms);
                let count = steps["scroll"]["count"].as_u64().unwrap() as usize;
                let h = b.h;
                let mut total = 0.0;
                for k in 1..=count {
                    total += b.frame(cx, &mut |view, _, _| {
                        view.surface_mut().scroll("root", k as f32 * h);
                    });
                }
                m.insert("scrollStepMs".into(), total / count as f64);
                let index = steps["scrollToIndex"]["index"].as_u64().unwrap() as u32;
                let jump = b.frame(cx, &mut |view, _, cx| view.scroll_to_index("root", index, Some("start"), cx));
                m.insert("scrollToIndexMs".into(), jump);
                let rendered = b.read(cx, |view| {
                    assert!(view.index_of(&format!("bench-row.r{index}")).is_some(), "item {index} rendered after the jump");
                    view.placed_nodes().iter().filter(|n| !n.removed && n.id.strip_prefix("bench-row.r").is_some_and(|k| !k.contains('.'))).count() as f64
                });
                m.insert("renderedItems".into(), rendered);
                if let Some(mb) = memory_mb() {
                    m.insert("memoryMb".into(), mb);
                }
                b.close(cx);
            }
            "streaming" => {
                let mut b = Bench::new(cx, host, sc, None);
                let messages = steps["messages"].as_array().unwrap();
                let mut times = Vec::with_capacity(messages.len());
                let t0 = Instant::now();
                for message in messages {
                    times.push(b.frame(cx, &mut |view, _, cx| {
                        view.apply(message, cx).expect("message applies");
                    }));
                }
                m.insert("totalMs".into(), ms(t0));
                m.insert("applyMs".into(), times.iter().sum::<f64>() / times.len() as f64);
                m.insert("p95ApplyMs".into(), percentile(&times, 95.0));
                m.insert("nodes".into(), b.nodes(cx));
                b.close(cx);
            }
            "theme-switch" => {
                let mut b = Bench::new(cx, host, sc, None);
                let themes: Vec<String> = steps["themes"].as_array().unwrap().iter().map(|v| v.as_str().unwrap().to_string()).collect();
                let cycles = steps["cycles"].as_u64().unwrap() as usize;
                let mut times = Vec::new();
                let mut restyled = 0u32;
                for _ in 0..cycles {
                    for theme in &themes {
                        times.push(b.frame(cx, &mut |view, _, cx| view.set_theme(builtin_theme(theme), cx)));
                        restyled = restyled.max(b.restyled);
                    }
                }
                m.insert("themeSwitchMs".into(), median(&times));
                m.insert("restyled".into(), restyled as f64);
                let modes: Vec<Mode> = steps["modes"].as_array().unwrap().iter().map(|v| if v.as_str() == Some("dark") { Mode::Dark } else { Mode::Light }).collect();
                let mode_cycles = steps["modeCycles"].as_u64().unwrap() as usize;
                let mut times = Vec::new();
                for _ in 0..mode_cycles {
                    for mode in &modes {
                        times.push(b.frame(cx, &mut |view, _, cx| view.set_mode(*mode, cx)));
                    }
                }
                m.insert("modeSwitchMs".into(), median(&times));
                b.close(cx);
            }
            "resize" => {
                let mut b = Bench::new(cx, host, sc, None);
                let (pass, upcalls) = b.width_passes(cx, steps);
                m.insert("resizeMs".into(), pass);
                m.insert("upcallsPerResize".into(), upcalls);
                b.close(cx);
            }
            "deep-nesting" => {
                let mut b = Bench::new(cx, host, sc, None);
                m.insert("firstPaintMs".into(), b.first_paint_ms);
                let (pass, _) = b.width_passes(cx, steps);
                m.insert("warmPassMs".into(), pass);
                // The root is depth 0: levels = the deepest node's depth + 1.
                let depth = b.read(cx, |view| view.placed_nodes().iter().filter(|n| !n.removed && !n.hidden).map(|n| n.depth).max().unwrap_or(0) + 1);
                m.insert("depth".into(), depth as f64);
                b.close(cx);
            }
            "template-10k" => {
                let mut b = Bench::new(cx, host, sc, None);
                m.insert("firstPaintMs".into(), b.first_paint_ms);
                let spec = &steps["rebind"];
                let items = Value::Array(generate_items(spec));
                let path = spec["path"].as_str().unwrap().to_string();
                let rebind = b.frame(cx, &mut |view, _, cx| view.set_data(&path, Some(items.clone()), cx).expect("rebind"));
                m.insert("rebindMs".into(), rebind);
                m.insert("nodes".into(), b.nodes(cx));
                b.close(cx);
            }
            "overlay" => {
                let mut b = Bench::new(cx, host, sc, None);
                let path = steps["path"].as_str().unwrap().to_string();
                let toggles = steps["toggles"].as_u64().unwrap() as usize;
                let (mut opens, mut closes) = (Vec::new(), Vec::new());
                for _ in 0..toggles {
                    opens.push(b.frame(cx, &mut |view, _, cx| view.set_data(&path, Some(json!(true)), cx).unwrap()));
                    assert!(b.read(cx, |view| !view.layers().is_empty()), "the dialog opened");
                    closes.push(b.frame(cx, &mut |view, _, cx| view.set_data(&path, Some(json!(false)), cx).unwrap()));
                    assert!(b.read(cx, |view| view.layers().is_empty()), "the dialog closed");
                }
                m.insert("openMs".into(), median(&opens));
                m.insert("closeMs".into(), median(&closes));
                b.close(cx);
            }
            "table-1k" => {
                let mut b = Bench::new(cx, host, sc, None);
                m.insert("firstPaintMs".into(), b.first_paint_ms);
                let count = steps["scroll"]["count"].as_u64().unwrap() as usize;
                let h = b.h;
                let mut total = 0.0;
                for k in 1..=count {
                    total += b.frame(cx, &mut |view, _, _| {
                        view.surface_mut().scroll("table", k as f32 * h);
                    });
                }
                m.insert("scrollStepMs".into(), total / count as f64);
                // The Table's rows are its `row` parts.
                let rows = b.read(cx, |view| view.placed_nodes().iter().filter(|n| !n.removed && n.part.as_deref() == Some("row") && n.owner.as_deref() == Some("table")).count() as f64);
                m.insert("renderedRows".into(), rows);
                let sort = steps["sort"].clone();
                let sort_ms = b.frame(cx, &mut |view, _, cx| {
                    view.apply(&sort, cx).expect("sort update applies");
                });
                m.insert("sortMs".into(), sort_ms);
                b.close(cx);
            }
            other => panic!("unknown scenario {other}"),
        }
        m
    }

    pub fn run() {
        let args: Vec<String> = std::env::args().skip(1).collect();
        let mut only: Option<String> = None;
        let mut repeat = 3usize;
        let mut i = 0;
        while i < args.len() {
            match args[i].as_str() {
                "--scenario" => {
                    only = args.get(i + 1).cloned();
                    i += 1;
                }
                "--repeat" => {
                    repeat = args.get(i + 1).and_then(|v| v.parse().ok()).unwrap_or(repeat);
                    i += 1;
                }
                other => eprintln!("bench_suite: ignoring {other}"),
            }
            i += 1;
        }

        let fonts = font_set(&manifest());
        let text_system = Arc::new(gpui_wgpu::CosmicTextSystem::new_without_system_fonts(&fonts.default));
        text_system.add_fonts(fonts.files.iter().map(|(_, b)| Cow::Owned(b.clone())).collect()).expect("the conformance fonts load");
        let mut cx = HeadlessAppContext::new(text_system);
        cx.update(gpui_component::init);
        let host: Rc<dyn HostPlugin> = Rc::new(FontHost(fonts.families.clone()));

        let manifest = scenario("manifest.json");
        let mut scenarios = Map::new();
        let mut first = true;
        for entry in manifest["scenarios"].as_array().expect("scenarios") {
            let id = entry["id"].as_str().unwrap();
            if only.as_deref().is_some_and(|o| o != id) {
                continue;
            }
            let sc = scenario(&format!("{id}.json"));
            let mut runs: Vec<Metrics> = Vec::with_capacity(repeat);
            for _ in 0..repeat {
                runs.push(run_scenario(&mut cx, &host, &sc, first));
                first = false;
            }
            let mut merged = Map::new();
            let keys: Vec<String> = runs.iter().flat_map(|r| r.keys().cloned()).collect::<BTreeSet<_>>().into_iter().collect();
            for key in keys {
                let values: Vec<f64> = runs.iter().filter_map(|r| r.get(&key).copied()).collect();
                merged.insert(key, json!(round3(median(&values))));
            }
            eprintln!("{id}: {}", serde_json::to_string(&merged).unwrap());
            scenarios.insert(id.to_string(), Value::Object(merged));
        }
        let fragment = json!({ "renderer": "gpui", "platform": "linux", "device": "headless (cosmic-text, the conformance fonts)", "date": iso_now(), "scenarios": scenarios });
        let text = serde_json::to_string_pretty(&fragment).unwrap();
        if let Ok(out) = std::env::var("EXPONENTIAL_UI_BENCH_OUT") {
            let path = PathBuf::from(out);
            if let Some(dir) = path.parent() {
                std::fs::create_dir_all(dir).expect("fragment dir");
            }
            std::fs::write(&path, format!("{text}\n")).expect("fragment written");
            eprintln!("wrote {}", path.display());
        }
        println!("{text}");
    }
}
