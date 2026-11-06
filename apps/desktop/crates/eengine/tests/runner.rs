//! Replays `packages/eengine/fixtures/runner.json` (the TS twin's `runner.test.ts` replays the same cases).

use eengine::contract::RUNNER;
use eengine::runner::{runner_config_value, runner_failure, runner_package, runner_toast, FormFactor, RunnerFailure};
use serde_json::Value;

fn cases(key: &str) -> &'static Vec<Value> {
    RUNNER[key].as_array().unwrap_or_else(|| panic!("{key} is a list"))
}

#[test]
fn config_cases() {
    for c in cases("configCases") {
        assert_eq!(runner_config_value(&c["input"]), c["expected"], "{}", c["name"]);
    }
}

#[test]
fn shell_cases() {
    for c in cases("shellCases") {
        let form: FormFactor = serde_json::from_value(c["formFactor"].clone()).unwrap();
        let got = runner_package(&c["package"], form);
        assert_eq!(got, c["expected"], "{}", c["name"]);
        let issues = eengine::package::validate(&got);
        assert!(issues.is_empty(), "{}: genUI accepts the wrapped package: {issues:?}", c["name"]);
    }
}

#[test]
fn failure_and_toast_cases() {
    for c in cases("failureCases") {
        let kind: RunnerFailure = serde_json::from_value(c["kind"].clone()).unwrap();
        assert_eq!(Value::Array(runner_failure(kind)), c["expected"], "{}", c["kind"]);
    }
    for c in cases("toastCases") {
        assert_eq!(Value::Array(runner_toast(c["message"].as_str().unwrap())), c["expected"]);
    }
}
