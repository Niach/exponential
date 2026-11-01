//! EXP-1602: replays `packages/exponential-ui/fixtures/motion-validate.json`
//! through `eui_motion::validate::check(bytes, &Limits::default())`.
//!
//! `UPDATE_MOTION_FIXTURES=1 cargo test -p eui-motion --test validate`
//! regenerates the expected codes (inputs kept) and rewrites `$comment`.

mod common;

use common::{fixture_path, write_fixture};
use eui_motion::validate::check;
use eui_motion::Limits;
use serde_json::{json, Map, Value};

const COMMENT: &str = "EXP-1602: eUI Motion validation (apps/desktop/crates/eui-motion/src/validate.rs; the limits are catalog/limits.json › motion): what a Motion source is refused for (`errors`) and what it loads with as notes (`warnings`). AUTO-GENERATED expected codes: `UPDATE_MOTION_FIXTURES=1 cargo test -p eui-motion --test validate` (inputs kept). Each case: `id`, `covers`, ONE input, then `errors` and `warnings`: the issue CODES, compared as sorted multisets (paths and messages are informative, not part of the contract). Inputs: `lottie` = a document, serialised as compact JSON (no whitespace, keys in the order given); `source` = the raw UTF-8 bytes (parse errors); `build` = a generated input: `build.lottie` (compact JSON as above, integers and plain strings only, so every language serialises it byte-identically) with `repeatShapes: {count, shape}` = the first layer's `shapes` set to `count` copies of `shape`, then `padToBytes: N` = ASCII spaces appended until the source is exactly N bytes (trailing whitespace is valid JSON). Codes: `parse` (not JSON), `timing` (needs fr > 0 and op > ip), `limit-bytes` (source > maxBytes, checked first: nothing else runs), `limit-duration` ((op − ip) / fr > maxSeconds), `limit-shapes` (more than maxShapes shape items: every object with a string `ty` inside a layer's `shapes` or a group's `it`), `expression` (an animated property, an object with `k`, carrying an `x` STRING; one per property; ease handles' `x` arrays are not expressions); warnings are the engine's notes on features it renders approximately or skips.";

fn source(case: &Value) -> Vec<u8> {
    if let Some(s) = case.get("source").and_then(Value::as_str) {
        return s.as_bytes().to_vec();
    }
    if let Some(l) = case.get("lottie") {
        return serde_json::to_vec(l).unwrap();
    }
    let build = case.get("build").expect("lottie, source or build");
    let mut lottie = build["lottie"].clone();
    if let Some(r) = build.get("repeatShapes") {
        let n = r["count"].as_u64().unwrap() as usize;
        lottie["layers"][0]["shapes"] = Value::Array(vec![r["shape"].clone(); n]);
    }
    let mut bytes = serde_json::to_vec(&lottie).unwrap();
    if let Some(n) = build.get("padToBytes").and_then(Value::as_u64) {
        let n = n as usize;
        assert!(bytes.len() <= n, "{}: padToBytes below the document's size", case["id"]);
        bytes.resize(n, b' ');
    }
    bytes
}

fn codes(issues: &[eui_motion::Issue]) -> Vec<String> {
    let mut v: Vec<String> = issues.iter().map(|i| i.code.clone()).collect();
    v.sort();
    v
}

fn sorted(v: &Value) -> Vec<String> {
    let mut v: Vec<String> = v.as_array().map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect()).unwrap_or_default();
    v.sort();
    v
}

#[test]
fn validation_matches_the_fixture() {
    let path = fixture_path("motion-validate.json");
    let fixture: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    let update = std::env::var("UPDATE_MOTION_FIXTURES").is_ok();
    let mut out = Vec::new();
    let mut problems = Vec::new();
    for case in fixture["cases"].as_array().unwrap() {
        let id = case["id"].as_str().unwrap();
        let report = check(&source(case), &Limits::default());
        let (errors, warnings) = (codes(&report.errors), codes(&report.warnings));
        if update {
            let mut c = Map::new();
            for k in ["id", "covers", "lottie", "source", "build"] {
                if let Some(v) = case.get(k) {
                    c.insert(k.into(), v.clone());
                }
            }
            c.insert("errors".into(), json!(errors));
            c.insert("warnings".into(), json!(warnings));
            out.push(Value::Object(c));
            continue;
        }
        if errors != sorted(&case["errors"]) {
            problems.push(format!("{id}: errors {errors:?} != {:?}", sorted(&case["errors"])));
        }
        if warnings != sorted(&case["warnings"]) {
            problems.push(format!("{id}: warnings {warnings:?} != {:?}", sorted(&case["warnings"])));
        }
    }
    if update {
        let mut root = Map::new();
        root.insert("$comment".into(), json!(COMMENT));
        root.insert("cases".into(), Value::Array(out));
        write_fixture(&path, &Value::Object(root), &["lottie", "build"]);
        return;
    }
    assert!(problems.is_empty(), "{} mismatch(es) (regenerate with UPDATE_MOTION_FIXTURES=1 if intended):\n{}", problems.len(), problems.join("\n"));
}

#[test]
fn every_refusal_is_covered() {
    let fixture: Value = serde_json::from_slice(&std::fs::read(fixture_path("motion-validate.json")).unwrap()).unwrap();
    let all: Vec<String> = fixture["cases"].as_array().unwrap().iter().flat_map(|c| sorted(&c["errors"])).collect();
    for code in ["parse", "timing", "limit-bytes", "limit-duration", "limit-shapes", "expression"] {
        assert!(all.iter().any(|c| c == code), "no case expects `{code}`");
    }
}

#[test]
fn limits_are_limits_json() {
    let limits: Value = serde_json::from_slice(
        &std::fs::read(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../../packages/exponential-ui/catalog/limits.json")).unwrap(),
    )
    .unwrap();
    let m = &limits["motion"];
    let d = Limits::default();
    assert_eq!(m["maxBytes"].as_u64(), Some(d.max_bytes as u64));
    assert_eq!(m["maxShapes"].as_u64(), Some(d.max_shapes as u64));
    assert_eq!(m["maxVertices"].as_u64(), Some(d.max_vertices as u64));
    assert_eq!(m["maxSeconds"].as_f64(), Some(d.max_seconds));
}
