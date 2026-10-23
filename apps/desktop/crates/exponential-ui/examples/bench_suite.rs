//! VAPP-105: the Exponential UI benchmark suite through the Rust core alone
//! (`packages/exponential-ui-bench/scenarios/*.json`): every scenario the
//! manifest lists, laid out with the fixed fake measure (no text shaping, no
//! painting), so the numbers are the engine's own. Writes the result
//! fragment (`packages/exponential-ui-bench/src/results.ts`) to
//! `$EXPONENTIAL_UI_BENCH_OUT` and prints it.
//!
//! ```text
//! cargo run --release -p exponential-ui --example bench_suite -- [--scenario <id>] [--repeat <n>]
//! ```
//!
//! `cold-start` runs FIRST in the process: the catalog, macros and themes
//! parse on the first `Surface::new`, and a counting allocator reports the
//! allocations of that start.

use std::alloc::{GlobalAlloc, Layout, System};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use exponential_ui::list::ScrollAlign;
use exponential_ui::measure::FixedMeasure;
use exponential_ui::surface::{LayoutOutput, Surface, SurfaceOptions};
use exponential_ui::theme::Mode;
use exponential_ui::themes::builtin_theme;
use exponential_ui::types::{FlatComponent, NestedNode};
use serde_json::{json, Map, Value};

// ---------------------------------------------------------------------------
// The counting allocator (`allocations` of the cold start)
// ---------------------------------------------------------------------------

struct Counting;

static ALLOCATIONS: AtomicU64 = AtomicU64::new(0);

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
        System.alloc(layout)
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        System.dealloc(ptr, layout)
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
        System.realloc(ptr, layout, new_size)
    }
}

#[global_allocator]
static GLOBAL: Counting = Counting;

// ---------------------------------------------------------------------------
// Scenario files + the data generator (src/scenarios.ts `generateItems`)
// ---------------------------------------------------------------------------

fn scenarios_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../../packages/exponential-ui-bench/scenarios")
}

fn read_json(name: &str) -> Value {
    let path = scenarios_dir().join(name);
    serde_json::from_str(&std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))).expect("scenario json")
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

// ---------------------------------------------------------------------------
// Process facts: memory, date, platform
// ---------------------------------------------------------------------------

/// The process's resident footprint in MB (Linux: VmRSS; macOS: the task's resident size).
fn memory_mb() -> Option<f64> {
    #[cfg(target_os = "linux")]
    {
        let status = std::fs::read_to_string("/proc/self/status").ok()?;
        let line = status.lines().find(|l| l.starts_with("VmRSS:"))?;
        let kb: f64 = line.split_whitespace().nth(1)?.parse().ok()?;
        return Some(kb / 1024.0);
    }
    #[cfg(target_os = "macos")]
    {
        let mut info: libc::proc_taskinfo = unsafe { std::mem::zeroed() };
        let size = std::mem::size_of::<libc::proc_taskinfo>() as i32;
        let got = unsafe { libc::proc_pidinfo(std::process::id() as i32, libc::PROC_PIDTASKINFO, 0, &mut info as *mut _ as *mut libc::c_void, size) };
        if got == size {
            return Some(info.pti_resident_size as f64 / (1024.0 * 1024.0));
        }
        let mut usage: libc::rusage = unsafe { std::mem::zeroed() };
        if unsafe { libc::getrusage(libc::RUSAGE_SELF, &mut usage) } == 0 {
            return Some(usage.ru_maxrss as f64 / (1024.0 * 1024.0));
        }
        return None;
    }
    #[allow(unreachable_code)]
    None
}

fn iso_now() -> String {
    let secs = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0) as i64;
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    // Howard Hinnant's civil_from_days.
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

fn platform() -> &'static str {
    if cfg!(target_os = "linux") {
        "linux"
    } else if cfg!(target_os = "macos") {
        "macos"
    } else if cfg!(target_os = "windows") {
        "windows"
    } else {
        "other"
    }
}

// ---------------------------------------------------------------------------
// Statistics
// ---------------------------------------------------------------------------

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

// ---------------------------------------------------------------------------
// One surface from a scenario
// ---------------------------------------------------------------------------

struct Bench {
    surface: Surface,
    measure: FixedMeasure,
    w: f32,
    h: f32,
    /// reduce + bind + viewport + the first layout, ms.
    first_paint_ms: f64,
    last: LayoutOutput,
}

impl Bench {
    /// Build the scenario's surface and paint it once. `started` = an earlier
    /// instant the first-paint clock starts at (the cold start).
    fn new(sc: &Value, started: Option<Instant>) -> Bench {
        let (w, h) = (sc["viewport"]["width"].as_f64().unwrap() as f32, sc["viewport"]["height"].as_f64().unwrap() as f32);
        let mode = if sc["mode"].as_str() == Some("dark") { Mode::Dark } else { Mode::Light };
        let t = started.unwrap_or_else(Instant::now);
        let options = SurfaceOptions { theme: builtin_theme(sc["theme"].as_str().unwrap()), catalog_id: sc["catalogId"].as_str().unwrap().to_string(), mode, ..SurfaceOptions::default() };
        let mut surface = Surface::new(sc["id"].as_str().unwrap_or("bench"), options);
        match &sc["data"] {
            Value::Null => {}
            data if data.get("generate").is_some() => {
                let spec = &data["generate"];
                surface.set_data(spec["path"].as_str().unwrap(), Some(Value::Array(generate_items(spec)))).expect("generated data");
            }
            data => surface.set_data("", Some(data.clone())).expect("literal data"),
        }
        if let Some(nested) = sc["surface"].get("nested") {
            let tree: NestedNode = serde_json::from_value(nested.clone()).expect("nested tree");
            let out = surface.set_nested(tree);
            assert!(out.issues.is_empty(), "{}: {:?}", sc["id"], out.issues);
        } else {
            let components: Vec<FlatComponent> = serde_json::from_value(sc["surface"]["components"].clone()).expect("components");
            let out = surface.set_components(components);
            assert!(out.issues.is_empty(), "{}: {:?}", sc["id"], out.issues);
        }
        surface.set_viewport(w, h, None);
        let mut measure = FixedMeasure { sizes: Default::default(), wrap: true };
        let last = surface.layout(&mut measure);
        let first_paint_ms = ms(t);
        Bench { surface, measure, w, h, first_paint_ms, last }
    }

    fn pass(&mut self) -> f64 {
        let t = Instant::now();
        self.last = self.surface.layout(&mut self.measure);
        ms(t)
    }

    fn nodes(&mut self) -> usize {
        self.surface.node_count()
    }

    /// Items alive in the windowed list `id` (end − start of its window).
    fn rendered(&self, id: &str) -> f64 {
        self.last.lists.iter().find(|l| l.id == id).map(|l| (l.end - l.start) as f64).unwrap_or(0.0)
    }

    /// Alternate `widths` for `passes` passes: (median ms, max upcalls).
    fn width_passes(&mut self, steps: &Value) -> (f64, f64) {
        let widths: Vec<f32> = steps["widths"].as_array().unwrap().iter().map(|v| v.as_f64().unwrap() as f32).collect();
        let passes = steps["passes"].as_u64().unwrap() as usize;
        let mut times = Vec::with_capacity(passes);
        let mut upcalls = 0u32;
        for p in 0..passes {
            self.surface.set_viewport(widths[p % widths.len()], self.h, None);
            times.push(self.pass());
            upcalls = upcalls.max(self.last.upcalls);
        }
        self.surface.set_viewport(self.w, self.h, None);
        (median(&times), upcalls as f64)
    }
}

// ---------------------------------------------------------------------------
// The scenarios
// ---------------------------------------------------------------------------

fn run_scenario(sc: &Value, first_in_process: bool) -> Metrics {
    let id = sc["id"].as_str().unwrap();
    let steps = &sc["steps"];
    let mut m = Metrics::new();
    match id {
        "cold-start" => {
            if first_in_process {
                ALLOCATIONS.store(0, Ordering::Relaxed);
                let t = Instant::now();
                let mut cold = Bench::new(sc, Some(t));
                m.insert("coldStartMs".into(), cold.first_paint_ms);
                m.insert("allocations".into(), ALLOCATIONS.load(Ordering::Relaxed) as f64);
                m.insert("nodes".into(), cold.nodes() as f64);
            }
            let mut warm = Bench::new(sc, None);
            m.insert("firstPaintMs".into(), warm.first_paint_ms);
            m.insert("nodes".into(), warm.nodes() as f64);
            if let Some(mb) = memory_mb() {
                m.insert("memoryMb".into(), mb);
            }
        }
        "card-grid" => {
            let mut b = Bench::new(sc, None);
            let (pass, upcalls) = b.width_passes(steps);
            m.insert("warmPassMs".into(), pass);
            m.insert("upcallsPerPass".into(), upcalls);
        }
        "list-100k" => {
            let mut b = Bench::new(sc, None);
            m.insert("firstPaintMs".into(), b.first_paint_ms);
            let count = steps["scroll"]["count"].as_u64().unwrap() as usize;
            let mut total = 0.0;
            for k in 1..=count {
                b.surface.scroll("root", k as f32 * b.h);
                total += b.pass();
            }
            m.insert("scrollStepMs".into(), total / count as f64);
            let index = steps["scrollToIndex"]["index"].as_u64().unwrap() as usize;
            let t = Instant::now();
            b.surface.scroll_to_index("root", index, ScrollAlign::Start);
            b.pass();
            m.insert("scrollToIndexMs".into(), ms(t));
            assert!(b.surface.index_of(&format!("bench-row.r{index}")).is_some(), "item {index} rendered after the jump");
            m.insert("renderedItems".into(), b.rendered("root"));
            if let Some(mb) = memory_mb() {
                m.insert("memoryMb".into(), mb);
            }
        }
        "streaming" => {
            let mut b = Bench::new(sc, None);
            let messages = steps["messages"].as_array().unwrap();
            let mut times = Vec::with_capacity(messages.len());
            let t0 = Instant::now();
            for message in messages {
                let t = Instant::now();
                b.surface.apply(message).expect("message applies");
                b.pass();
                times.push(ms(t));
            }
            m.insert("totalMs".into(), ms(t0));
            m.insert("applyMs".into(), times.iter().sum::<f64>() / times.len() as f64);
            m.insert("p95ApplyMs".into(), percentile(&times, 95.0));
            m.insert("nodes".into(), b.nodes() as f64);
        }
        "theme-switch" => {
            let mut b = Bench::new(sc, None);
            let themes: Vec<&str> = steps["themes"].as_array().unwrap().iter().map(|v| v.as_str().unwrap()).collect();
            let cycles = steps["cycles"].as_u64().unwrap() as usize;
            let mut times = Vec::new();
            let mut restyled = 0u32;
            for _ in 0..cycles {
                for theme in &themes {
                    let t = Instant::now();
                    b.surface.set_theme(builtin_theme(theme));
                    b.pass();
                    times.push(ms(t));
                    restyled = restyled.max(b.last.restyled);
                }
            }
            m.insert("themeSwitchMs".into(), median(&times));
            m.insert("restyled".into(), restyled as f64);
            let modes: Vec<Mode> = steps["modes"].as_array().unwrap().iter().map(|v| if v.as_str() == Some("dark") { Mode::Dark } else { Mode::Light }).collect();
            let mode_cycles = steps["modeCycles"].as_u64().unwrap() as usize;
            let mut times = Vec::new();
            for _ in 0..mode_cycles {
                for mode in &modes {
                    let t = Instant::now();
                    b.surface.set_mode(*mode);
                    b.pass();
                    times.push(ms(t));
                }
            }
            m.insert("modeSwitchMs".into(), median(&times));
        }
        "resize" => {
            let mut b = Bench::new(sc, None);
            let (pass, upcalls) = b.width_passes(steps);
            m.insert("resizeMs".into(), pass);
            m.insert("upcallsPerResize".into(), upcalls);
        }
        "deep-nesting" => {
            let mut b = Bench::new(sc, None);
            m.insert("firstPaintMs".into(), b.first_paint_ms);
            let (pass, _) = b.width_passes(steps);
            m.insert("warmPassMs".into(), pass);
            // The root is depth 0: levels = the deepest node's depth + 1.
            let depth = b.surface.nodes().iter().filter(|n| !n.hidden).map(|n| n.depth).max().unwrap_or(0) + 1;
            m.insert("depth".into(), depth as f64);
        }
        "template-10k" => {
            let mut b = Bench::new(sc, None);
            m.insert("firstPaintMs".into(), b.first_paint_ms);
            let spec = &steps["rebind"];
            let items = generate_items(spec);
            let t = Instant::now();
            b.surface.set_data(spec["path"].as_str().unwrap(), Some(Value::Array(items))).expect("rebind");
            b.pass();
            m.insert("rebindMs".into(), ms(t));
            m.insert("nodes".into(), b.nodes() as f64);
        }
        "overlay" => {
            let mut b = Bench::new(sc, None);
            let path = steps["path"].as_str().unwrap();
            let toggles = steps["toggles"].as_u64().unwrap() as usize;
            let (mut opens, mut closes) = (Vec::new(), Vec::new());
            for _ in 0..toggles {
                let t = Instant::now();
                b.surface.set_data(path, Some(json!(true))).unwrap();
                b.pass();
                opens.push(ms(t));
                assert!(!b.last.layers.is_empty(), "the dialog opened");
                let t = Instant::now();
                b.surface.set_data(path, Some(json!(false))).unwrap();
                b.pass();
                closes.push(ms(t));
                assert!(b.last.layers.is_empty(), "the dialog closed");
            }
            m.insert("openMs".into(), median(&opens));
            m.insert("closeMs".into(), median(&closes));
        }
        "table-1k" => {
            let mut b = Bench::new(sc, None);
            m.insert("firstPaintMs".into(), b.first_paint_ms);
            let count = steps["scroll"]["count"].as_u64().unwrap() as usize;
            let mut total = 0.0;
            for k in 1..=count {
                b.surface.scroll("table", k as f32 * b.h);
                total += b.pass();
            }
            m.insert("scrollStepMs".into(), total / count as f64);
            m.insert("renderedRows".into(), b.rendered("table"));
            let t = Instant::now();
            b.surface.apply(&steps["sort"]).expect("sort update applies");
            b.pass();
            m.insert("sortMs".into(), ms(t));
        }
        other => panic!("unknown scenario {other}"),
    }
    m
}

fn main() {
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
    let manifest = read_json("manifest.json");
    let mut scenarios = Map::new();
    let mut first = true;
    for entry in manifest["scenarios"].as_array().expect("scenarios") {
        let id = entry["id"].as_str().unwrap();
        if only.as_deref().is_some_and(|o| o != id) {
            continue;
        }
        let sc = read_json(&format!("{id}.json"));
        let mut runs: Vec<Metrics> = Vec::with_capacity(repeat);
        for _ in 0..repeat {
            runs.push(run_scenario(&sc, first));
            first = false;
        }
        // The median per metric; a metric only the first run has (the cold
        // numbers) is taken from it.
        let mut merged = Map::new();
        let keys: Vec<String> = runs.iter().flat_map(|r| r.keys().cloned()).collect::<std::collections::BTreeSet<_>>().into_iter().collect();
        for key in keys {
            let values: Vec<f64> = runs.iter().filter_map(|r| r.get(&key).copied()).collect();
            merged.insert(key, json!(round3(median(&values))));
        }
        eprintln!("{id}: {}", serde_json::to_string(&merged).unwrap());
        scenarios.insert(id.to_string(), Value::Object(merged));
    }
    let fragment = json!({ "renderer": "core", "platform": platform(), "date": iso_now(), "scenarios": scenarios });
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
