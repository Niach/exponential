//! The renderer CONFORMANCE gate, desktop side
//! (`packages/exponential-ui/conformance`): the gpui painter, headless, with
//! gpui's own Linux text system holding ONLY the conformance font files
//! (`examples/conformance_dump.rs`), lays out every case of
//! `fixtures/conformance-cases.json`; each case is compared with the
//! committed WEB baseline (`fixtures/conformance-baseline.json`, the React
//! renderer in headless Chromium with the same fonts) under the matrix's
//! tolerance, with the rules of `conformance/compare.ts` (ported below).
//!
//! Every divergence is PRINTED as a list to fix (`-- --nocapture`): per case
//! the ORIGINS (`FIX`, a node whose own size diverges with nothing below
//! explaining it, or that moved while its parent did not), then the count of
//! the cascade they cause (`EXP_UI_CONFORMANCE_VERBOSE=1` lists every
//! cascaded node too), then the origins grouped across cases. The gate is a RATCHET over `fixtures/conformance-known.json` (the
//! divergence counts per case today): a case that gets WORSE fails, one that
//! improves asks for the budget to be lowered. Rewrite the budget after a
//! fix: `EXP_UI_WRITE_FIXTURES=1 cargo test -p exponential-ui-gpui --test
//! conformance`.

#![cfg(any(target_os = "linux", target_os = "freebsd"))]

#[path = "../examples/conformance_dump.rs"]
mod conformance_dump;

use std::collections::{BTreeMap, HashMap, HashSet};

use conformance_dump::{read_json, repo_root, CaseDump, DumpNode};
use serde_json::{json, Value};

const BASELINE: &str = "packages/exponential-ui/fixtures/conformance-baseline.json";
const KNOWN: &str = "packages/exponential-ui/fixtures/conformance-known.json";

/// `conformance/baseline.ts` `decodeBaseline`.
fn decode_baseline(b: &Value) -> BTreeMap<String, CaseDump> {
    assert_eq!(b["format"], "xui-conformance-baseline/1", "baseline format");
    let mut out = BTreeMap::new();
    for (key, c) in b["cases"].as_object().expect("cases") {
        let fixture = key.split('/').next().unwrap_or_default();
        let table = b["fixtures"][fixture]["nodes"].as_array().expect("node table");
        let s = |v: &Value| v.as_str().map(String::from);
        let f = |v: &Value| v.as_f64().unwrap_or(0.0) as f32;
        let nodes = c["frames"]
            .as_array()
            .expect("frames")
            .iter()
            .map(|row| {
                let row = row.as_array().expect("frame row");
                let n = table[row[0].as_u64().expect("node index") as usize].as_array().expect("node row");
                DumpNode { id: s(&n[0]).unwrap_or_default(), component: s(&n[1]).unwrap_or_default(), part: s(&n[2]), parent: s(&n[3]), x: f(&row[1]), y: f(&row[2]), w: f(&row[3]), h: f(&row[4]), text: s(&n[4]), lines: row.get(5).and_then(Value::as_u64).map(|l| l as u32), lh: row.get(6).map(f) }
            })
            .collect();
        out.insert(key.clone(), CaseDump { width: f(&c["width"]), height: f(&c["height"]), nodes });
    }
    out
}

#[derive(Default, Debug)]
struct Diff {
    id: String,
    component: String,
    kinds: Vec<&'static str>,
    origin: bool,
    line: String,
}

#[derive(Default, Debug)]
struct CaseReport {
    matched: usize,
    size: usize,
    position: usize,
    wrap: usize,
    wrap_tolerated: Vec<String>,
    only_ref: Vec<String>,
    only_cand: Vec<String>,
    diffs: Vec<Diff>,
}

fn r2(v: f32) -> f32 {
    (v * 100.0).round() / 100.0
}

fn fmt_box(n: &DumpNode) -> String {
    let lines = n.lines.map(|l| format!(" {l}ln@{}", n.lh.unwrap_or(0.0))).unwrap_or_default();
    format!("{},{} {}×{}{lines}", n.x, n.y, n.w, n.h)
}

/// `conformance/compare.ts` `compareCase`, same rules.
fn compare_case(reference: &CaseDump, cand: &CaseDump, px: f32, text_lines: f32) -> CaseReport {
    let mut ref_by: HashMap<&str, &DumpNode> = HashMap::new();
    let mut ref_order = Vec::new();
    for n in &reference.nodes {
        if !ref_by.contains_key(n.id.as_str()) {
            ref_by.insert(&n.id, n);
            ref_order.push(n);
        }
    }
    let mut cand_by: HashMap<&str, &DumpNode> = HashMap::new();
    for n in &cand.nodes {
        cand_by.entry(&n.id).or_insert(n);
    }
    let mut report = CaseReport::default();
    for a in ref_order {
        let Some(b) = cand_by.get(a.id.as_str()) else {
            report.only_ref.push(a.id.clone());
            continue;
        };
        report.matched += 1;
        let (dx, dy, dw, dh) = (r2(b.x - a.x), r2(b.y - a.y), r2(b.w - a.w), r2(b.h - a.h));
        let has_lines = a.lines.is_some() || b.lines.is_some();
        let lh = a.lh.unwrap_or(0.0).max(b.lh.unwrap_or(0.0));
        let h_tol = if has_lines { px.max(text_lines * lh) } else { px };
        let mut kinds = Vec::new();
        if dw.abs() > px || dh.abs() > h_tol {
            kinds.push("size");
        } else if dx.abs() > px || dy.abs() > px {
            kinds.push("position");
        }
        if let (Some(la), Some(lb)) = (a.lines, b.lines) {
            if la != lb {
                if (la as f32 - lb as f32).abs() > text_lines {
                    kinds.push("wrap");
                } else {
                    report.wrap_tolerated.push(format!("{}: {la} → {lb} lines", a.id));
                }
            }
        }
        if kinds.is_empty() {
            continue;
        }
        let text = a.text.as_ref().or(b.text.as_ref()).map(|t| format!("  \"{}\"", t.chars().take(40).collect::<String>())).unwrap_or_default();
        let line = format!("{:<13} {} ({}) web {} → gpui {}  Δ {dx},{dy} {dw}×{dh}{text}", kinds.join("+"), a.id, a.component, fmt_box(a), fmt_box(b));
        report.diffs.push(Diff { id: a.id.clone(), component: a.component.clone(), kinds, origin: false, line });
    }
    for n in &cand.nodes {
        if !ref_by.contains_key(n.id.as_str()) && !report.only_cand.contains(&n.id) {
            report.only_cand.push(n.id.clone());
        }
    }
    let diverged: HashSet<String> = report.diffs.iter().map(|d| d.id.clone()).collect();
    let mut size_below: HashSet<String> = HashSet::new();
    for d in report.diffs.iter().filter(|d| d.kinds.contains(&"size") || d.kinds.contains(&"wrap")) {
        let mut p = ref_by.get(d.id.as_str()).and_then(|n| n.parent.clone());
        while let Some(id) = p {
            p = ref_by.get(id.as_str()).and_then(|n| n.parent.clone());
            size_below.insert(id);
        }
    }
    for d in &mut report.diffs {
        let parent = ref_by.get(d.id.as_str()).and_then(|n| n.parent.clone());
        d.origin = if d.kinds.contains(&"size") || d.kinds.contains(&"wrap") { !size_below.contains(&d.id) } else { parent.is_none_or(|p| !diverged.contains(&p)) };
        report.size += d.kinds.contains(&"size") as usize;
        report.position += d.kinds.contains(&"position") as usize;
        report.wrap += d.kinds.contains(&"wrap") as usize;
    }
    report
}

fn counts(r: &CaseReport) -> Value {
    json!({ "size": r.size, "position": r.position, "wrap": r.wrap, "onlyRef": r.only_ref.len(), "onlyCand": r.only_cand.len() })
}

#[test]
fn gpui_matches_the_web_baseline_within_the_known_budget() {
    let manifest = conformance_dump::manifest();
    let cases = conformance_dump::cases(&manifest);
    let px = manifest["tolerance"]["px"].as_f64().unwrap_or(1.0) as f32;
    let text_lines = manifest["tolerance"]["textLines"].as_f64().unwrap_or(1.0) as f32;
    let baseline = decode_baseline(&read_json(BASELINE));
    let dump = conformance_dump::headless::dump(&cases);
    let known_path = repo_root().join(KNOWN);
    let known: Value = std::fs::read_to_string(&known_path).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or(Value::Null);
    let write = std::env::var_os("EXP_UI_WRITE_FIXTURES").is_some();
    let verbose = std::env::var_os("EXP_UI_CONFORMANCE_VERBOSE").is_some();

    let mut worse = Vec::new();
    let mut better = Vec::new();
    let mut budget = serde_json::Map::new();
    let mut origin_groups: BTreeMap<String, (usize, String)> = BTreeMap::new();
    for case in &cases {
        let reference = baseline.get(&case.key).unwrap_or_else(|| panic!("{}: not in the baseline (rewrite it: bun run --filter @exponential-at/ui conformance -- --write-baseline)", case.key));
        let cand = &dump.cases[&case.key];
        let r = compare_case(reference, cand, px, text_lines);
        let origins: Vec<&Diff> = r.diffs.iter().filter(|d| d.origin).collect();
        eprintln!("\n## {}  matched {} · size {} · position {} · wrap {} · origins {} · only web {} · only gpui {} · height web {} / gpui {}", case.key, r.matched, r.size, r.position, r.wrap, origins.len(), r.only_ref.len(), r.only_cand.len(), reference.height, cand.height);
        for d in &origins {
            eprintln!("  FIX  {}", d.line);
            let g = origin_groups.entry(format!("{} ({}) {}", d.id, d.component, d.kinds.join("+"))).or_insert((0, case.key.clone()));
            g.0 += 1;
        }
        let cascade: Vec<&Diff> = r.diffs.iter().filter(|d| !d.origin).collect();
        if verbose {
            for d in &cascade {
                eprintln!("       {}", d.line);
            }
        } else if !cascade.is_empty() {
            let ids: Vec<&str> = cascade.iter().take(8).map(|d| d.id.as_str()).collect();
            eprintln!("  + {} cascaded (moved/resized by the origins; EXP_UI_CONFORMANCE_VERBOSE=1 lists them): {}{}", cascade.len(), ids.join(", "), if cascade.len() > 8 { " …" } else { "" });
        }
        if !r.wrap_tolerated.is_empty() {
            eprintln!("  rewrapped within tolerance: {}", r.wrap_tolerated.join("; "));
        }
        let list = |ids: &[String]| if verbose || ids.len() <= 12 { ids.join(", ") } else { format!("{} …", ids[..12].join(", ")) };
        if !r.only_ref.is_empty() {
            eprintln!("  only in web ({}): {}", r.only_ref.len(), list(&r.only_ref));
        }
        if !r.only_cand.is_empty() {
            eprintln!("  only in gpui ({}; parts the DOM paints without data-xui-id): {}", r.only_cand.len(), list(&r.only_cand));
        }
        let now = counts(&r);
        let was = &known["cases"][&case.key];
        for k in ["size", "position", "wrap", "onlyRef", "onlyCand"] {
            let (n, b) = (now[k].as_u64().unwrap_or(0), was[k].as_u64());
            match b {
                None => worse.push(format!("{}: {k} {n} (no budget)", case.key)),
                Some(b) if n > b => worse.push(format!("{}: {k} {n} > budget {b}", case.key)),
                Some(b) if n < b => better.push(format!("{}: {k} {n} < budget {b}", case.key)),
                _ => {}
            }
        }
        budget.insert(case.key.clone(), now);
    }
    eprintln!("\n# divergence origins across cases (fix these; the rest cascades from them)");
    let mut groups: Vec<_> = origin_groups.into_iter().collect();
    groups.sort_by(|a, b| b.1 .0.cmp(&a.1 .0).then(a.0.cmp(&b.0)));
    for (what, (n, example)) in &groups {
        eprintln!("  {n:>3}× {what}   e.g. {example}");
    }
    if write {
        let out = json!({
            "$comment": "The desktop conformance RATCHET (apps/desktop/crates/exponential-ui-gpui/tests/conformance.rs): per case, the divergence counts of the gpui painter (headless, gpui's cosmic-text system, the conformance fonts) against the web baseline (fixtures/conformance-baseline.json) today. A case may only go DOWN; rewrite after a fix with EXP_UI_WRITE_FIXTURES=1 cargo test -p exponential-ui-gpui --test conformance. `onlyRef`/`onlyCand` = nodes only the web / only gpui placed (coverage, not compared).",
            "renderer": conformance_dump::RENDERER,
            "cases": Value::Object(budget),
        });
        std::fs::write(&known_path, serde_json::to_string_pretty(&out).expect("json") + "\n").expect("write the known budget");
        eprintln!("\nwrote {KNOWN}");
        return;
    }
    if !better.is_empty() {
        eprintln!("\nIMPROVED — lower the budget (EXP_UI_WRITE_FIXTURES=1):\n  {}", better.join("\n  "));
    }
    assert!(worse.is_empty(), "the gpui painter diverges MORE from the web baseline than {KNOWN} allows:\n  {}\n(run with -- --nocapture for every divergence)", worse.join("\n  "));
}

fn n(id: &str, parent: Option<&str>, x: f32, y: f32, w: f32, h: f32) -> DumpNode {
    DumpNode { id: id.into(), component: "Box".into(), part: None, parent: parent.map(String::from), x, y, w, h, text: None, lines: None, lh: None }
}

fn case(nodes: Vec<DumpNode>) -> CaseDump {
    CaseDump { width: 400.0, height: 400.0, nodes }
}

/// The port keeps `conformance/compare.test.ts`'s rules (tolerance, origins,
/// wrap, coverage) and `dropCollapsed`.
#[test]
fn compare_rules_match_the_ts_comparator() {
    // ±1 px is in tolerance, 1.5 px is a position origin.
    let a = case(vec![n("root", None, 0.0, 0.0, 400.0, 100.0), n("a", Some("root"), 10.0, 10.0, 50.0, 20.0)]);
    assert!(compare_case(&a, &case(vec![n("root", None, 0.0, 0.0, 400.0, 100.0), n("a", Some("root"), 11.0, 9.0, 51.0, 21.0)]), 1.0, 1.0).diffs.is_empty());
    let r = compare_case(&a, &case(vec![n("root", None, 0.0, 0.0, 400.0, 100.0), n("a", Some("root"), 11.5, 10.0, 50.0, 20.0)]), 1.0, 1.0);
    assert_eq!(r.diffs.iter().map(|d| (d.id.as_str(), d.kinds.clone(), d.origin)).collect::<Vec<_>>(), vec![("a", vec!["position"], true)]);

    // Origins: the deepest size divergence; a sibling moved inside a diverging parent cascades.
    let a = case(vec![n("root", None, 0.0, 0.0, 400.0, 100.0), n("card", Some("root"), 0.0, 0.0, 400.0, 60.0), n("text", Some("card"), 0.0, 0.0, 100.0, 20.0), n("below", Some("root"), 0.0, 60.0, 400.0, 40.0), n("belowChild", Some("below"), 0.0, 60.0, 10.0, 10.0)]);
    let b = case(vec![n("root", None, 0.0, 0.0, 400.0, 120.0), n("card", Some("root"), 0.0, 0.0, 400.0, 80.0), n("text", Some("card"), 0.0, 0.0, 100.0, 40.0), n("below", Some("root"), 0.0, 80.0, 400.0, 40.0), n("belowChild", Some("below"), 0.0, 80.0, 10.0, 10.0)]);
    let r = compare_case(&a, &b, 1.0, 1.0);
    let origins: Vec<(&str, bool)> = r.diffs.iter().map(|d| (d.id.as_str(), d.origin)).collect();
    assert_eq!(origins, vec![("root", false), ("card", false), ("text", true), ("below", false), ("belowChild", false)]);
    assert_eq!((r.size, r.position), (3, 2));

    // Text: one more line is tolerated and reported, two is a wrap divergence.
    let t = |lines: u32| case(vec![DumpNode { component: "Text".into(), lines: Some(lines), lh: Some(20.0), ..n("t", None, 0.0, 0.0, 200.0, 20.0 * lines as f32) }]);
    let one = compare_case(&t(2), &t(3), 1.0, 1.0);
    assert!(one.diffs.is_empty());
    assert_eq!(one.wrap_tolerated, vec!["t: 2 → 3 lines".to_string()]);
    let two = compare_case(&t(2), &t(4), 1.0, 1.0);
    assert_eq!(two.diffs[0].kinds, vec!["size", "wrap"]);

    // Coverage is not a divergence.
    let r = compare_case(&case(vec![n("a", None, 0.0, 0.0, 1.0, 1.0), n("webOnly", None, 0.0, 0.0, 1.0, 1.0)]), &case(vec![n("a", None, 0.0, 0.0, 1.0, 1.0), n("gpuiOnly", None, 0.0, 0.0, 1.0, 1.0)]), 1.0, 1.0);
    assert_eq!((r.only_ref, r.only_cand, r.diffs.len()), (vec!["webOnly".to_string()], vec!["gpuiOnly".to_string()], 0));

    // dropCollapsed: any order, 0×0 parents of placed nodes stay, thin boxes stay.
    let kept: Vec<String> = conformance_dump::drop_collapsed(vec![n("leaf", Some("zeroParent"), 0.0, 0.0, 10.0, 10.0), n("root", None, 0.0, 0.0, 100.0, 100.0), n("zeroParent", Some("root"), 0.0, 0.0, 0.0, 0.0), n("gone", Some("root"), 5.0, 5.0, 0.0, 0.0), n("goneChild", Some("gone"), 5.0, 5.0, 0.0, 0.0), n("rule", Some("root"), 0.0, 0.0, 100.0, 0.0)]).into_iter().map(|n| n.id).collect();
    assert_eq!(kept, vec!["leaf", "root", "zeroParent", "rule"]);
}
