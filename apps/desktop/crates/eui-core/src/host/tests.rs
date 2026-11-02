//! VAPP-196 (eApps T1): PACKAGE TESTS, the twin of
//! `packages/exponential-ui/src/host/tests.ts`. A package carries `tests:
//! [{name, records?, now?, as?, expr | query | action, expect}]`;
//! [`validate_package`](super::validate_package) runs every one on an
//! otherwise valid package, each against its OWN in-memory store seeded with
//! `records`, on a fixed clock (`now`, UTC) as the member `as`. A failing
//! test is an install issue at `/tests/<i>` (expected vs got). Locked by
//! `fixtures/package-tests.json`.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use super::actions::{apply_record_batch, run_action, ActionContext};
use super::package::PackageIssue;
use super::query::{expand_query_source, parse_records_source, records_value_owned, RecordRow, RecordsContext, RECORDS_SCHEME};
use super::records::validate_record;
use super::signature::canonical_json;
use super::sources::{parse_source, source_with_params};
use super::{js_trim, utf16_cmp};
use crate::cel::{evaluate_expression, parse_timestamp, ExpressionEnv};

/// Caps: tests per package.
pub const PACKAGE_TEST_LIMIT_TESTS: usize = 50;
/// The characters (code points) of a `got` an issue prints.
pub const PACKAGE_TEST_LIMIT_GOT_MAX: usize = 300;
/// The clock a test runs at when it names none.
pub const PACKAGE_TEST_DEFAULT_NOW: &str = "2026-01-01T00:00:00.000Z";
/// The member a test runs as when it names none.
pub const PACKAGE_TEST_DEFAULT_AS: &str = "member";
/// What a test runs: exactly one of these keys.
pub const PACKAGE_TEST_KINDS: &[&str] = &["expr", "query", "action"];

/// One test's outcome (`runPackageTests`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PackageTestResult {
    pub name: String,
    pub pass: bool,
    /// What the test produced (`{error}` for a failure).
    pub got: Value,
}

/// The seed row keys that are not data.
const SEED_KEYS: [&str; 2] = ["id", "author"];

fn issue(path: impl Into<String>, message: impl Into<String>) -> PackageIssue {
    PackageIssue { path: path.into(), message: message.into() }
}

fn escape(key: &str) -> String {
    key.replace('~', "~0").replace('/', "~1")
}

fn sorted_keys(m: &Map<String, Value>) -> Vec<&String> {
    let mut keys: Vec<&String> = m.keys().collect();
    keys.sort_by(|a, b| utf16_cmp(a, b));
    keys
}

/// Does `got` satisfy `expect`? An object matches by the keys it names
/// (recursively; a missing key reads null), a list item by item with the
/// same length, anything else by JSON equality (`1` equals `1.0`).
pub fn test_matches(expect: &Value, got: &Value) -> bool {
    match expect {
        Value::Object(e) => got.as_object().is_some_and(|g| e.iter().all(|(k, v)| test_matches(v, g.get(k).unwrap_or(&Value::Null)))),
        Value::Array(e) => got.as_array().is_some_and(|g| g.len() == e.len() && e.iter().zip(g).all(|(e, g)| test_matches(e, g))),
        _ => crate::json::strict_eq(expect, got),
    }
}

/// The shape issues of a package's `tests` (they run only without any).
pub fn validate_package_tests(pkg: &Map<String, Value>) -> Vec<PackageIssue> {
    let Some(tests) = pkg.get("tests") else { return Vec::new() };
    let Some(tests) = tests.as_array() else { return vec![issue("/tests", "tests is a list of {name, expr | query | action, expect}")] };
    let mut issues = Vec::new();
    if tests.len() > PACKAGE_TEST_LIMIT_TESTS {
        issues.push(issue("/tests", format!("at most {PACKAGE_TEST_LIMIT_TESTS} tests")));
    }
    let empty = Map::new();
    let records = pkg.get("records").and_then(Value::as_object).unwrap_or(&empty);
    let queries = pkg.get("queries").and_then(Value::as_object).unwrap_or(&empty);
    let actions = pkg.get("actions").and_then(Value::as_object).unwrap_or(&empty);
    for (i, t) in tests.iter().enumerate() {
        let at = format!("/tests/{i}");
        let Some(t) = t.as_object() else {
            issues.push(issue(at, "a test is {name, expr | query | action, expect}"));
            continue;
        };
        if !t.get("name").and_then(Value::as_str).is_some_and(|n| !js_trim(n).is_empty()) {
            issues.push(issue(format!("{at}/name"), "name is a non-empty string"));
        }
        let kinds: Vec<&str> = PACKAGE_TEST_KINDS.iter().copied().filter(|k| t.contains_key(*k)).collect();
        if kinds.len() != 1 {
            issues.push(issue(&at, format!("a test runs one of {}", PACKAGE_TEST_KINDS.join(", "))));
        }
        if !t.contains_key("expect") {
            issues.push(issue(format!("{at}/expect"), "expect is required"));
        }
        if let Some(now) = t.get("now") {
            if now.as_str().and_then(parse_timestamp).is_none() {
                issues.push(issue(format!("{at}/now"), "now is an ISO timestamp"));
            }
        }
        if let Some(r#as) = t.get("as") {
            if !r#as.as_str().is_some_and(|s| !s.is_empty()) {
                issues.push(issue(format!("{at}/as"), "as is a member id"));
            }
        }
        if let Some(seed) = t.get("records") {
            match seed.as_object() {
                None => issues.push(issue(format!("{at}/records"), "records is {<kind>: [rows]}")),
                Some(seed) => {
                    for kind in sorted_keys(seed) {
                        let kat = format!("{at}/records/{}", escape(kind));
                        let Some(schema) = records.get(kind).and_then(Value::as_object).and_then(|d| d.get("schema")).filter(|s| s.is_object()) else {
                            issues.push(issue(kat, format!("records:{kind} names no declared kind")));
                            continue;
                        };
                        let Some(rows) = seed[kind.as_str()].as_array() else {
                            issues.push(issue(kat, "a kind's sample is a list of rows"));
                            continue;
                        };
                        for (j, row) in rows.iter().enumerate() {
                            let Some(row) = row.as_object() else {
                                issues.push(issue(format!("{kat}/{j}"), "a row is an object"));
                                continue;
                            };
                            for key in SEED_KEYS {
                                if let Some(v) = row.get(key) {
                                    if !v.as_str().is_some_and(|s| !s.is_empty()) {
                                        issues.push(issue(format!("{kat}/{j}/{key}"), format!("{key} is a non-empty string")));
                                    }
                                }
                            }
                            for i in validate_record(schema, &seed_data(row)) {
                                issues.push(issue(format!("{kat}/{j}{}", i.path), i.message));
                            }
                        }
                    }
                }
            }
        }
        let [kind] = kinds.as_slice() else { continue };
        let body = &t[*kind];
        let kat = format!("{at}/{kind}");
        match *kind {
            "expr" => match body.as_object() {
                Some(b) if b.get("expr").is_some_and(Value::is_string) => {
                    if b.get("vars").is_some_and(|v| !v.is_object()) {
                        issues.push(issue(format!("{kat}/vars"), "vars is an object"));
                    }
                }
                _ => issues.push(issue(format!("{kat}/expr"), "expr is a string")),
            },
            "query" => {
                let name = body.get("name").and_then(Value::as_str).unwrap_or("");
                if !queries.contains_key(name) {
                    issues.push(issue(format!("{kat}/name"), format!("query {name} names no package query")));
                }
                if body.as_object().and_then(|b| b.get("params")).is_some_and(|p| !p.is_object()) {
                    issues.push(issue(format!("{kat}/params"), "params is an object"));
                }
            }
            _ => {
                let name = body.get("name").and_then(Value::as_str).unwrap_or("");
                if !actions.contains_key(name) {
                    issues.push(issue(format!("{kat}/name"), format!("action {name} names no package action")));
                }
            }
        }
    }
    issues
}

fn seed_data(row: &Map<String, Value>) -> Value {
    Value::Object(row.iter().filter(|(k, _)| !SEED_KEYS.contains(&k.as_str())).map(|(k, v)| (k.clone(), v.clone())).collect())
}

/// A test's store: its `records` (kinds sorted, rows in order), ids
/// `<kind>-<n>` unless given, authored by `author` (else the test's member),
/// stamped `now`.
fn seed_rows(records: Option<&Value>, me: &str, now: &str) -> Vec<RecordRow> {
    let mut rows = Vec::new();
    let Some(records) = records.and_then(Value::as_object) else { return rows };
    for kind in sorted_keys(records) {
        let Some(list) = records[kind.as_str()].as_array() else { continue };
        for (j, row) in list.iter().enumerate() {
            let Some(row) = row.as_object() else { continue };
            let author = row.get("author").and_then(Value::as_str).unwrap_or(me).to_string();
            let id = row.get("id").and_then(Value::as_str).map_or_else(|| format!("{kind}-{}", j + 1), str::to_string);
            rows.push(RecordRow { id, kind: kind.clone(), data: seed_data(row), created_by: Some(author.clone()), author_id: Some(author), created_at: now.into(), updated_at: now.into() });
        }
    }
    rows
}

fn error(message: String) -> Value {
    let mut m = Map::new();
    m.insert("error".into(), Value::String(message));
    Value::Object(m)
}

/// A records source's value over the test's store (`{error}` when it is
/// not a records source).
fn records_source_value(source: &str, pkg: &Map<String, Value>, rows: &[RecordRow], me: &str, now: &str) -> Value {
    let expanded = expand_query_source(source, pkg.get("queries").filter(|q| q.is_object()));
    let query = parse_source(&expanded).filter(|p| p.scheme == RECORDS_SCHEME).and_then(|p| parse_records_source(&p));
    let Some(query) = query else { return error(format!("{source} is not a records source")) };
    let names = HashMap::new();
    records_value_owned(rows, &query, RecordsContext { me: Some(me), now, names: &names }).unwrap_or(Value::Null)
}

/// Run one (valid) test: what it produced.
fn run_test(pkg: &Map<String, Value>, t: &Map<String, Value>) -> Value {
    let now = t.get("now").and_then(Value::as_str).unwrap_or(PACKAGE_TEST_DEFAULT_NOW);
    let now_ms = parse_timestamp(now);
    let me = t.get("as").and_then(Value::as_str).unwrap_or(PACKAGE_TEST_DEFAULT_AS);
    let rows = seed_rows(t.get("records"), me, now);
    if let Some(expr) = t.get("expr").and_then(Value::as_object) {
        let mut vars = Map::new();
        if let Some(given) = expr.get("vars").and_then(Value::as_object) {
            for (k, v) in given {
                // `{source: "records:…"}` = that binding's value; anything else the literal.
                let value = match v.as_object() {
                    Some(o) if o.len() == 1 && o.get("source").is_some_and(Value::is_string) => records_source_value(o["source"].as_str().unwrap_or(""), pkg, &rows, me, now),
                    _ => v.clone(),
                };
                vars.insert(k.clone(), value);
            }
        }
        let env = ExpressionEnv { now: now_ms, offset: None, max_steps: None };
        return evaluate_expression(expr.get("expr").and_then(Value::as_str).unwrap_or(""), &vars, &env).unwrap_or_else(error);
    }
    if let Some(query) = t.get("query").and_then(Value::as_object) {
        let name = query.get("name").and_then(Value::as_str).unwrap_or("");
        let from = pkg.get("queries").and_then(|q| q.get(name)).and_then(|q| q.get("from")).and_then(Value::as_str).unwrap_or("");
        let mut params = Map::new();
        params.insert("query".into(), Value::String(name.into()));
        if let Some(given) = query.get("params").and_then(Value::as_object) {
            for (k, v) in given {
                params.insert(k.clone(), Value::String(v.as_str().map_or_else(|| canonical_json(v), str::to_string)));
            }
        }
        let source = source_with_params(&format!("{RECORDS_SCHEME}:{from}"), Some(&Value::Object(params)));
        return records_source_value(&source, pkg, &rows, me, now);
    }
    let a = t.get("action").and_then(Value::as_object);
    let field = |k: &str| a.and_then(|a| a.get(k));
    let name = field("name").and_then(Value::as_str).unwrap_or("");
    let ctx = ActionContext { data: field("data"), me: Some(me), now: now_ms, offset: None, max_steps: None };
    let pkg_value = Value::Object(pkg.clone());
    let plan = match run_action(&pkg_value, name, field("params").unwrap_or(&Value::Null), &ctx) {
        Ok(plan) => plan,
        Err(e) => return error(e),
    };
    let start: Vec<Value> = rows.iter().map(|r| serde_json::to_value(r).unwrap_or(Value::Null)).collect();
    let writes = plan.get("writes").and_then(Value::as_array).map_or(&[][..], Vec::as_slice);
    let (after, _) = match apply_record_batch(&start, writes, pkg.get("records").filter(|r| r.is_object()), Some(me), now, |i| format!("new-{}", i + 1)) {
        Ok(out) => out,
        Err(e) => return error(e),
    };
    let mut records: Map<String, Value> = Map::new();
    for row in &after {
        let mut out = row.get("data").and_then(Value::as_object).cloned().unwrap_or_default();
        out.insert("id".into(), row.get("id").cloned().unwrap_or(Value::Null));
        let author = row.get("authorId").filter(|v| !v.is_null()).or_else(|| row.get("createdBy")).cloned().unwrap_or(Value::Null);
        out.insert("author".into(), author);
        let kind = row.get("kind").and_then(Value::as_str).unwrap_or("").to_string();
        if let Value::Array(list) = records.entry(kind).or_insert_with(|| Value::Array(Vec::new())) {
            list.push(Value::Object(out));
        }
    }
    let mut got = Map::new();
    got.insert("records".into(), Value::Object(records));
    got.insert("sets".into(), plan.get("sets").cloned().unwrap_or(Value::Array(Vec::new())));
    got.insert("calls".into(), plan.get("calls").cloned().unwrap_or(Value::Array(Vec::new())));
    Value::Object(got)
}

/// Every test of a package, run (a package whose tests are malformed runs none).
pub fn run_package_tests(pkg: &Value) -> Vec<PackageTestResult> {
    let Some(p) = pkg.as_object() else { return Vec::new() };
    let Some(tests) = p.get("tests").and_then(Value::as_array) else { return Vec::new() };
    if !validate_package_tests(p).is_empty() {
        return Vec::new();
    }
    let empty = Map::new();
    tests
        .iter()
        .map(|t| {
            let t = t.as_object().unwrap_or(&empty);
            let got = run_test(p, t);
            let pass = test_matches(t.get("expect").unwrap_or(&Value::Null), &got);
            PackageTestResult { name: t.get("name").and_then(Value::as_str).unwrap_or("").to_string(), pass, got }
        })
        .collect()
}

/// The issue text of a failed test: expected vs got, both canonical JSON
/// (got cut at [`PACKAGE_TEST_LIMIT_GOT_MAX`] code points + `…`).
pub fn test_failure(name: &str, expect: &Value, got: &Value) -> String {
    let g = canonical_json(got);
    let text = match g.char_indices().nth(PACKAGE_TEST_LIMIT_GOT_MAX) {
        Some((cut, _)) => format!("{}…", &g[..cut]),
        None => g,
    };
    format!("{name}: expected {}, got {text}", canonical_json(expect))
}

/// The failing tests as install issues (`/tests/<i>`).
pub fn package_test_issues(pkg: &Value) -> Vec<PackageIssue> {
    let tests = pkg.get("tests").and_then(Value::as_array);
    run_package_tests(pkg)
        .into_iter()
        .enumerate()
        .filter(|(_, r)| !r.pass)
        .map(|(i, r)| issue(format!("/tests/{i}"), test_failure(&r.name, tests.and_then(|t| t[i].get("expect")).unwrap_or(&Value::Null), &r.got)))
        .collect()
}
