//! E6 (#VAPP-266): the runner binary carries no updater, no telemetry and
//! no log or crash sink, and links nothing of the Exponential app. Walked
//! over Cargo.lock (every target's dependencies: a superset of any one
//! platform's), so a crate pulled in deep in the graph trips it too.

use std::collections::{BTreeSet, HashMap};

/// The app's crates and the update path.
const APP: &[&str] = &["api", "app", "cli", "coding", "codemode", "domain", "engine", "steer", "sync", "terminal", "theme", "ui", "updater", "eagent", "exponential-agent-runtime", "observability"];

/// Log, trace, crash and analytics sinks.
const SINKS: &[&str] = &[
    "tracing-subscriber", "tracing-appender", "tracing-journald", "env_logger", "simplelog", "fern", "log4rs", "flexi_logger", "syslog", "sentry", "sentry-core", "minidumper", "crash-handler", "opentelemetry", "opentelemetry_sdk", "posthog-rs", "segment",
];

/// The self-update crates.
const UPDATERS: &[&str] = &["self_update", "self-replace", "cargo-packager-updater", "velopack", "minisign-verify"];

fn graph() -> (HashMap<String, Vec<(String, Vec<String>)>>, BTreeSet<String>) {
    let lock: toml::Table = include_str!("../../../Cargo.lock").parse().expect("Cargo.lock parses");
    let mut by_name: HashMap<String, Vec<(String, Vec<String>)>> = HashMap::new();
    for p in lock["package"].as_array().expect("packages") {
        let name = p["name"].as_str().unwrap().to_string();
        let version = p["version"].as_str().unwrap().to_string();
        let deps = p.get("dependencies").and_then(|d| d.as_array()).map(|d| d.iter().filter_map(|x| x.as_str().map(str::to_string)).collect()).unwrap_or_default();
        by_name.entry(name).or_default().push((version, deps));
    }
    let mut seen = BTreeSet::new();
    let mut stack = vec!["eengine-runner".to_string()];
    while let Some(spec) = stack.pop() {
        let mut parts = spec.split(' ');
        let name = parts.next().unwrap().to_string();
        let version = parts.next();
        if !seen.insert(format!("{name} {}", version.unwrap_or(""))) {
            continue;
        }
        for (v, deps) in by_name.get(&name).into_iter().flatten() {
            if version.is_none_or(|want| want == v) {
                stack.extend(deps.iter().cloned());
            }
        }
    }
    let names = seen.iter().map(|s| s.split(' ').next().unwrap().to_string()).collect();
    (by_name, names)
}

#[test]
fn the_runner_links_no_app_crate_no_updater_and_no_sink() {
    let (_, names) = graph();
    assert!(names.contains("eengine") && names.contains("eui-gpui"), "the walk found the runner's graph");
    for forbidden in APP.iter().chain(SINKS).chain(UPDATERS) {
        assert!(!names.contains(*forbidden), "the runner's graph pulls `{forbidden}`");
    }
}

#[test]
fn the_manifest_names_no_app_crate() {
    let manifest: toml::Table = include_str!("../Cargo.toml").parse().expect("Cargo.toml parses");
    let mut sections: Vec<&toml::Table> = ["dependencies", "build-dependencies"].iter().filter_map(|s| manifest.get(*s).and_then(|v| v.as_table())).collect();
    if let Some(targets) = manifest.get("target").and_then(|t| t.as_table()) {
        sections.extend(targets.values().filter_map(|t| t.get("dependencies").and_then(|d| d.as_table())));
    }
    for deps in sections {
        for name in deps.keys() {
            assert!(!APP.contains(&name.as_str()), "the runner must not depend on `{name}`");
        }
    }
    // Diagnostics compiled out of the release build.
    let log = &manifest["dependencies"]["log"];
    assert!(log["features"].as_array().unwrap().iter().any(|f| f.as_str() == Some("release_max_level_off")));
}
