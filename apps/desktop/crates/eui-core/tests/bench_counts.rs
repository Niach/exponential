//! VAPP-289: the benchmark's deterministic COUNTS, computed by the core from
//! the committed scenarios (`packages/exponential-ui-bench/scenarios/*.json`),
//! must equal `packages/exponential-ui-bench/counts.json`. The scenarios
//! paint the kitchen sink (every catalog component once), so a catalog change
//! moves these numbers; this gate (the PR's `cargo test -p eui-core`) and the
//! package's `src/counts.test.ts` (budgets.json = counts.json + each
//! renderer's offset) catch it on the PR instead of the next release suite.
//! Fix: `bun run --filter @exponential_app/eui-bench counts --write` (runs
//! this test with `EXPONENTIAL_UI_BENCH_COUNTS=write`, then rewrites the
//! budgets).
//!
//! The same surface setup as `examples/bench_suite.rs` (`Bench::new`): the
//! scenario's theme, mode, data, tree and viewport, laid out with the fixed
//! fake measure. Canonical counts per scenario:
//! - `nodes`: live nodes (`Surface::node_count`, what the core and gpui
//!   report);
//! - `visibleNodes`: live and not hidden (what SwiftUI and Compose report:
//!   their `SurfaceModel.nodes` without the hidden ones);
//! - `restyled`: the most nodes one theme switch restyled (theme-switch);
//! - `depth`: the deepest visible node's depth + 1 (deep-nesting).

use std::collections::BTreeMap;
use std::path::PathBuf;

use eui_core::measure::FixedMeasure;
use eui_core::surface::{Surface, SurfaceOptions};
use eui_core::theme::Mode;
use eui_core::themes::builtin_theme;
use eui_core::types::{FlatComponent, NestedNode};
use serde_json::Value;

fn bench_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../../packages/exponential-ui-bench")
}

fn scenario(id: &str) -> Value {
    let path = bench_dir().join(format!("scenarios/{id}.json"));
    serde_json::from_str(&std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))).expect("scenario json")
}

/// src/scenarios.ts `generateItems`: `{i}` = the index, `{i % N}` = modulo N.
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

struct Bench {
    surface: Surface,
    measure: FixedMeasure,
    restyled: u32,
}

impl Bench {
    fn new(sc: &Value) -> Bench {
        let (w, h) = (sc["viewport"]["width"].as_f64().unwrap() as f32, sc["viewport"]["height"].as_f64().unwrap() as f32);
        let mode = if sc["mode"].as_str() == Some("dark") { Mode::Dark } else { Mode::Light };
        let options = SurfaceOptions { theme: builtin_theme(sc["theme"].as_str().unwrap()), catalog_id: sc["catalogId"].as_str().unwrap().to_string(), mode, ..SurfaceOptions::default() };
        let mut surface = Surface::new(sc["id"].as_str().unwrap_or("bench"), options);
        match (&sc["data"], sc["data"].get("generate")) {
            (_, Some(spec)) => surface.set_data(spec["path"].as_str().unwrap(), Some(Value::Array(generate_items(spec)))).expect("generated data"),
            (Value::Null, None) => {}
            (data, None) => surface.set_data("", Some(data.clone())).expect("literal data"),
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
        Bench { surface, measure, restyled: last.restyled }
    }

    fn pass(&mut self) {
        self.restyled = self.surface.layout(&mut self.measure).restyled;
    }

    fn counts(&mut self) -> BTreeMap<String, u64> {
        let nodes = self.surface.node_count() as u64;
        let visible = self.surface.nodes().iter().filter(|n| !n.hidden).count() as u64;
        BTreeMap::from([("nodes".to_string(), nodes), ("visibleNodes".to_string(), visible)])
    }
}

/// The canonical counts, by scenario id.
fn canonical() -> BTreeMap<String, BTreeMap<String, u64>> {
    let mut out = BTreeMap::new();
    out.insert("cold-start".to_string(), Bench::new(&scenario("cold-start")).counts());

    let sc = scenario("theme-switch");
    let mut b = Bench::new(&sc);
    let mut restyled = 0u32;
    for theme in sc["steps"]["themes"].as_array().unwrap() {
        b.surface.set_theme(builtin_theme(theme.as_str().unwrap()));
        b.pass();
        restyled = restyled.max(b.restyled);
    }
    out.insert("theme-switch".to_string(), BTreeMap::from([("restyled".to_string(), restyled as u64)]));

    let sc = scenario("streaming");
    let mut b = Bench::new(&sc);
    for message in sc["steps"]["messages"].as_array().unwrap() {
        b.surface.apply(message).expect("message applies");
        b.pass();
    }
    out.insert("streaming".to_string(), b.counts());

    let sc = scenario("template-10k");
    let mut b = Bench::new(&sc);
    let spec = &sc["steps"]["rebind"];
    b.surface.set_data(spec["path"].as_str().unwrap(), Some(Value::Array(generate_items(spec)))).expect("rebind");
    b.pass();
    out.insert("template-10k".to_string(), b.counts());

    let mut b = Bench::new(&scenario("deep-nesting"));
    let depth = b.surface.nodes().iter().filter(|n| !n.hidden).map(|n| n.depth).max().unwrap_or(0) + 1;
    out.insert("deep-nesting".to_string(), BTreeMap::from([("depth".to_string(), depth as u64)]));
    out
}

const COMMENT: &str = "VAPP-289: GENERATED by the core (apps/desktop/crates/eui-core/tests/bench_counts.rs) from the committed scenarios: the canonical count of each derived metric. budgets.json's `derivedCounts` maps every renderer's count budget to one of these plus that renderer's offset; src/counts.test.ts gates it. Rewrite both: `bun run --filter @exponential_app/eui-bench counts --write`.";

fn render(counts: &BTreeMap<String, BTreeMap<String, u64>>) -> String {
    let mut doc = serde_json::Map::new();
    doc.insert("$comment".into(), Value::String(COMMENT.into()));
    for (id, m) in counts {
        doc.insert(id.clone(), serde_json::to_value(m).unwrap());
    }
    serde_json::to_string_pretty(&Value::Object(doc)).unwrap() + "\n"
}

#[test]
fn the_bench_counts_match_counts_json() {
    let path = bench_dir().join("counts.json");
    let want = render(&canonical());
    if std::env::var("EXPONENTIAL_UI_BENCH_COUNTS").as_deref() == Ok("write") {
        std::fs::write(&path, &want).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        return;
    }
    let have = std::fs::read_to_string(&path).unwrap_or_default();
    assert!(have == want, "packages/exponential-ui-bench/counts.json is stale (the scenarios or the catalog changed the bench's counts):\n{want}\nrun `bun run --filter @exponential_app/eui-bench counts --write` and commit counts.json + budgets.json");
}
