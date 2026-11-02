//! VAPP-196: `fixtures/package-tests.json` replayed through the Rust core
//! (the TS reference replays it in `src/host/tests.test.ts`): the limits and
//! defaults against `catalog/host.json`, `matches`, `runs` and `validate`.

mod support;

use eui_core::host::{self, run_package_tests, test_failure, test_matches, validate_package};
use eui_core::json;
use serde_json::{json, Value};
use support::{first_diff, fixture, read};

fn check(name: &str, got: &Value, want: &Value, failures: &mut Vec<String>) {
    if !json::equal(got, want) {
        failures.push(format!("{name}: {}", first_diff(&json::canonical(got), &json::canonical(want), "").unwrap_or_else(|| format!("{got} != {want}"))));
    }
}

/// A validate case's package: the base with its `with` keys replaced.
fn case_package(base: &Value, with: &Value) -> Value {
    let mut pkg = base.as_object().unwrap().clone();
    for (k, v) in with.as_object().unwrap() {
        pkg.insert(k.clone(), v.clone());
    }
    Value::Object(pkg)
}

#[test]
fn package_tests_fixture() {
    let f = fixture("package-tests.json");
    let limits = json!({"tests": host::PACKAGE_TEST_LIMIT_TESTS, "gotMax": host::PACKAGE_TEST_LIMIT_GOT_MAX});
    let defaults = json!({"now": host::PACKAGE_TEST_DEFAULT_NOW, "as": host::PACKAGE_TEST_DEFAULT_AS});
    assert_eq!(f["limits"], limits);
    assert_eq!(f["defaults"], defaults);
    let contract = read("catalog/host.json");
    let tests = &contract["package"]["tests"];
    assert_eq!(tests["limits"], limits);
    assert_eq!(tests["defaults"], defaults);
    assert_eq!(tests["kinds"], json!(host::PACKAGE_TEST_KINDS));

    let mut failures = Vec::new();
    let mut n = 0;
    for (i, m) in f["matches"].as_array().unwrap().iter().enumerate() {
        check(&format!("matches {i}"), &json!(test_matches(&m["expect"], &m["got"])), &m["match"], &mut failures);
        n += 1;
    }
    let base = &f["package"];
    let mut pkg = base.as_object().unwrap().clone();
    pkg.insert("tests".into(), f["runs"]["tests"].clone());
    let results = run_package_tests(&Value::Object(pkg));
    check("runs", &serde_json::to_value(&results).unwrap(), &f["runs"]["results"], &mut failures);
    n += 1;
    for c in f["validate"].as_array().unwrap() {
        let issues = validate_package(&case_package(base, &c["with"]), &host::supported_catalog_ids::<&str>(&[]));
        check(&format!("validate: {}", c["name"].as_str().unwrap()), &serde_json::to_value(&issues).unwrap(), &c["issues"], &mut failures);
        n += 1;
    }
    assert!(failures.is_empty(), "{} of {n} cases failed:\n{}", failures.len(), failures.join("\n"));
    assert!(n >= 19, "{n}");
}

/// The `got` cut counts code points, not bytes or UTF-16 units.
#[test]
fn a_long_got_is_cut_at_code_points() {
    let got = Value::String("é😀".repeat(200));
    let message = test_failure("t", &json!(1), &got);
    let printed = message.strip_prefix("t: expected 1, got ").unwrap();
    assert_eq!(printed.chars().count(), host::PACKAGE_TEST_LIMIT_GOT_MAX + 1);
    assert!(printed.ends_with('…'));
    assert_eq!(test_failure("t", &json!(1), &json!("short")), "t: expected 1, got \"short\"");
}

/// A test list over the cap is a shape issue (and runs nothing).
#[test]
fn at_most_fifty_tests() {
    let f = fixture("package-tests.json");
    let mut pkg = f["package"].as_object().unwrap().clone();
    let one = json!({"name": "one", "expr": {"expr": "1"}, "expect": 2});
    pkg.insert("tests".into(), Value::Array(vec![one; host::PACKAGE_TEST_LIMIT_TESTS + 1]));
    let pkg = Value::Object(pkg);
    let issues = validate_package(&pkg, &host::supported_catalog_ids::<&str>(&[]));
    assert_eq!(serde_json::to_value(&issues).unwrap(), json!([{"path": "/tests", "message": "at most 50 tests"}]));
    assert!(run_package_tests(&pkg).is_empty());
}
