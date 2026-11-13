//! eCode's GitHub workflow reader (`fixtures/workflow.json` is the spec,
//! #EXP-1617): a GitHub Actions workflow, already PARSED from YAML into a JSON
//! tree, read into the same model as `.exponential/ci.json`, or every reason
//! it cannot be, as JSON-pointer errors into the workflow. A port of
//! `packages/ecode/src/workflow.ts`; YAML text parsing stays with the host.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::{json, Map, Value};

use crate::{is_valid_label, PipelineError, MATRIX_ENV_PREFIX, PIPELINE_NAME_MAX, PIPELINE_VERSION};

const TOP_KEYS: &[&str] = &["name", "on", "env", "jobs"];
const TOP_IGNORED: &[&str] = &["permissions", "concurrency", "run-name"];
const JOB_KEYS: &[&str] = &["name", "runs-on", "needs", "timeout-minutes", "env", "strategy", "steps"];
const JOB_IGNORED: &[&str] = &["permissions", "concurrency"];
const STRATEGY_KEYS: &[&str] = &["matrix"];
const STRATEGY_IGNORED: &[&str] = &["fail-fast", "max-parallel"];
const STEP_KEYS: &[&str] =
    &["name", "run", "env", "timeout-minutes", "continue-on-error", "working-directory", "shell", "uses", "with"];
const STEP_IGNORED: &[&str] = &["id"];
const SHELLS: &[&str] = &["bash", "sh"];
/// `uses:` our runner covers itself: the checkout and the cache (a no-op).
const ACTIONS: &[&str] = &["actions/checkout", "actions/cache"];

fn event_keys(event: &str) -> Option<&'static [&'static str]> {
    match event {
        "push" => Some(&["branches", "tags", "paths"]),
        "pull_request" => Some(&["branches", "paths", "types"]),
        "workflow_dispatch" => Some(&["inputs"]),
        "merge_group" => Some(&["types"]),
        _ => None,
    }
}

/// What [`read_workflow`] returns: the pipeline file, or the errors.
#[derive(Clone, Debug, PartialEq)]
pub struct WorkflowRead {
    /// The `.exponential/ci.json` it reads as; `None` when there are errors.
    pub file: Option<Value>,
    pub errors: Vec<PipelineError>,
}

fn ptr(key: &str) -> String {
    format!("/{}", key.replace('~', "~0").replace('/', "~1"))
}

fn is_job_key(key: &str) -> bool {
    let mut chars = key.chars();
    chars.next().is_some_and(|c| c.is_ascii_lowercase())
        && key.len() <= 64
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-')
}

fn is_env_name(name: &str) -> bool {
    let mut chars = name.chars();
    chars.next().is_some_and(|c| c.is_ascii_uppercase() || c == '_')
        && chars.all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
}

fn is_matrix_key(key: &str) -> bool {
    let mut chars = key.chars();
    chars.next().is_some_and(|c| c.is_ascii_lowercase())
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

fn strings(value: &Value) -> Option<Vec<String>> {
    match value {
        Value::String(s) => Some(vec![s.clone()]),
        Value::Array(items) => items.iter().map(|v| v.as_str().map(str::to_string)).collect(),
        _ => None,
    }
}

fn scalar(value: &Value) -> Option<String> {
    match value {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => Some(js_number(n)),
        Value::Bool(b) => Some(b.to_string()),
        _ => None,
    }
}

/// `String(n)` in JS for the numbers a workflow holds (integers print bare).
fn js_number(n: &serde_json::Number) -> String {
    match n.as_f64() {
        Some(f) if f.fract() == 0.0 && f.abs() < 1e21 => format!("{}", f as i64),
        _ => n.to_string(),
    }
}

fn is_int(value: Option<&Value>) -> Option<i64> {
    let v = value?;
    if let Some(i) = v.as_i64() {
        return Some(i);
    }
    v.as_f64().filter(|f| f.fract() == 0.0).map(|f| f as i64)
}

/// A GitHub `runs-on` label as ours: `Some("")` = dropped (self-hosted).
fn label(raw: &str) -> Option<String> {
    let l = raw.trim().to_lowercase();
    if l == "self-hosted" {
        return Some(String::new());
    }
    if l.starts_with("ubuntu") || l == "linux" {
        return Some("linux".into());
    }
    if l.starts_with("macos") || l == "mac" {
        return Some("mac".into());
    }
    if l.starts_with("windows") {
        return Some("windows".into());
    }
    is_valid_label(&l).then_some(l)
}

/// One `${{ … }}` expression: its inner text, its whole match range.
fn expressions(text: &str) -> Vec<(usize, usize, String)> {
    let mut out = vec![];
    let mut from = 0;
    while let Some(start) = text[from..].find("${{").map(|i| from + i) {
        let Some(end) = text[start + 3..].find('}').map(|i| start + 3 + i) else { break };
        // The pattern is `\$\{\{\s*([^}]*?)\s*\}\}`: the inner text has no `}`.
        if !text[end..].starts_with("}}") {
            from = end + 1;
            continue;
        }
        out.push((start, end + 2, text[start + 3..end].trim().to_string()));
        from = end + 2;
    }
    out
}

struct Reader {
    errors: Vec<PipelineError>,
}

impl Reader {
    fn err(&mut self, path: impl Into<String>, code: &str) {
        self.errors.push(PipelineError { path: path.into(), code: code.into() });
    }

    fn unknown(&mut self, o: &Map<String, Value>, known: &[&str], ignored: &[&str], at: &str) {
        for k in o.keys() {
            if !known.contains(&k.as_str()) && !ignored.contains(&k.as_str()) {
                self.err(format!("{at}{}", ptr(k)), "unknown_field");
            }
        }
    }

    fn no_expression(&mut self, text: &str, at: &str) -> bool {
        if text.contains("${{") {
            self.err(at, "expression");
            false
        } else {
            true
        }
    }

    fn run_text(&mut self, text: &str, at: &str, matrix_keys: &[String], secrets: &mut BTreeSet<String>) -> String {
        let mut out = String::new();
        let mut last = 0;
        for (start, end, expr) in expressions(text) {
            out.push_str(&text[last..start]);
            let parts: Vec<&str> = expr.split('.').collect();
            let replaced = match parts.as_slice() {
                ["matrix", name] if matrix_keys.iter().any(|k| k == name) => {
                    Some(format!("${{{MATRIX_ENV_PREFIX}{}}}", name.to_uppercase()))
                }
                [scope @ ("env" | "secrets"), name] if is_env_name(name) => {
                    if *scope == "secrets" {
                        secrets.insert(name.to_string());
                    }
                    Some(format!("${{{name}}}"))
                }
                _ => None,
            };
            match replaced {
                Some(r) => out.push_str(&r),
                None => {
                    self.err(at, "expression");
                    out.push_str(&text[start..end]);
                }
            }
            last = end;
        }
        out.push_str(&text[last..]);
        out
    }

    fn env_map(&mut self, value: Option<&Value>, at: &str, secrets: &mut BTreeSet<String>) -> Option<Value> {
        let value = value?;
        let Some(o) = value.as_object() else {
            self.err(at, "type");
            return None;
        };
        let mut env = BTreeMap::new();
        for (k, raw) in o {
            let here = format!("{at}{}", ptr(k));
            if !is_env_name(k) {
                self.err(&here, "invalid_name");
            }
            let Some(v) = scalar(raw) else {
                self.err(&here, "type");
                continue;
            };
            let found = expressions(&v);
            if found.len() == 1 && found[0].0 == 0 && found[0].1 == v.len() {
                if let Some(name) = found[0].2.strip_prefix("secrets.").filter(|n| is_env_name(n)) {
                    if name == k {
                        secrets.insert(k.clone());
                    } else {
                        self.err(&here, "expression");
                    }
                    continue;
                }
            }
            if self.no_expression(&v, &here) {
                env.insert(k.clone(), Value::String(v));
            }
        }
        (!env.is_empty()).then(|| Value::Object(env.into_iter().collect()))
    }
}

/// Reads a GitHub workflow (parsed YAML) into a pipeline file.
pub fn read_workflow(workflow: &Value) -> WorkflowRead {
    let Some(wf) = workflow.as_object() else {
        return WorkflowRead { file: None, errors: vec![PipelineError { path: String::new(), code: "type".into() }] };
    };
    let mut r = Reader { errors: vec![] };
    r.unknown(wf, TOP_KEYS, TOP_IGNORED, "");

    // ── name ──
    let mut name: Option<String> = None;
    if let Some(n) = wf.get("name") {
        match n.as_str() {
            None => r.err("/name", "type"),
            Some(n) if r.no_expression(n, "/name") => name = Some(n.chars().take(PIPELINE_NAME_MAX).collect()),
            Some(_) => {}
        }
    }

    // ── on ──
    let mut on = Map::new();
    let mut paths: Option<Vec<String>> = None;
    match wf.get("on") {
        None => r.err("/on", "missing"),
        Some(o) => {
            let events: Vec<(String, Value)> = match o {
                Value::String(s) => vec![(s.clone(), Value::Null)],
                Value::Array(items) => items
                    .iter()
                    .map(|e| (e.as_str().map_or_else(|| e.to_string(), str::to_string), Value::Null))
                    .collect(),
                Value::Object(m) => m.iter().map(|(k, v)| (k.clone(), v.clone())).collect(),
                _ => vec![],
            };
            if events.is_empty() {
                r.err("/on", "type");
            }
            for (event, body) in events {
                let at = format!("/on{}", ptr(&event));
                let Some(known) = event_keys(&event) else {
                    r.err(&at, "unsupported_event");
                    continue;
                };
                let empty = Map::new();
                let o = match &body {
                    Value::Null => &empty,
                    Value::Object(m) => m,
                    _ => {
                        r.err(&at, "type");
                        continue;
                    }
                };
                r.unknown(o, known, &[], &at);
                let filter = |r: &mut Reader, key: &str| -> Option<Vec<String>> {
                    let v = o.get(key)?;
                    let list = strings(v);
                    if list.is_none() {
                        r.err(format!("{at}/{key}"), "type");
                    }
                    list
                };
                if o.contains_key("paths") {
                    if let Some(p) = filter(&mut r, "paths") {
                        if paths.as_ref().is_some_and(|existing| *existing != p) {
                            r.err(format!("{at}/paths"), "invalid_value");
                        }
                        paths = Some(p);
                    }
                }
                match event.as_str() {
                    "push" => {
                        let branches = filter(&mut r, "branches");
                        let tags = filter(&mut r, "tags");
                        if branches.is_some() || !o.contains_key("tags") {
                            on.insert("push".into(), branches.map_or_else(|| json!({}), |b| json!({ "branches": b })));
                        }
                        if let Some(tags) = tags {
                            on.insert("tag".into(), json!({ "tags": tags }));
                        }
                    }
                    "pull_request" => {
                        let branches = filter(&mut r, "branches");
                        on.insert("pull_request".into(), branches.map_or_else(|| json!({}), |b| json!({ "branches": b })));
                    }
                    "workflow_dispatch" => {
                        on.insert("manual".into(), json!({}));
                    }
                    _ => {
                        on.insert("merge_train".into(), json!({}));
                    }
                }
            }
        }
    }

    // ── env ──
    let mut top_secrets = BTreeSet::new();
    let top_env = r.env_map(wf.get("env"), "/env", &mut top_secrets);

    // ── jobs ──
    let mut jobs = Map::new();
    match wf.get("jobs") {
        None => r.err("/jobs", "missing"),
        Some(v) if !v.is_object() => r.err("/jobs", "type"),
        Some(v) => {
            let all = v.as_object().unwrap();
            let mut ids: Vec<(String, String)> = vec![];
            for id in all.keys() {
                let key = id.to_lowercase();
                if !is_job_key(&key) || ids.iter().any(|(_, k)| *k == key) {
                    r.err(format!("/jobs{}", ptr(id)), "invalid_name");
                }
                ids.push((id.clone(), key));
            }
            let lower = |id: &str| ids.iter().find(|(i, _)| i == id).map_or_else(|| id.to_lowercase(), |(_, k)| k.clone());
            for (id, raw) in all {
                let at = format!("/jobs{}", ptr(id));
                let Some(raw) = raw.as_object() else {
                    r.err(&at, "type");
                    continue;
                };
                r.unknown(raw, JOB_KEYS, JOB_IGNORED, &at);
                let mut job = Map::new();
                let mut secrets = BTreeSet::new();

                if let Some(n) = raw.get("name") {
                    match n.as_str() {
                        None => r.err(format!("{at}/name"), "type"),
                        Some(n) if r.no_expression(n, &format!("{at}/name")) => {
                            job.insert("name".into(), json!(n));
                        }
                        Some(_) => {}
                    }
                }

                match raw.get("runs-on") {
                    None => r.err(format!("{at}/runs-on"), "missing"),
                    Some(v) => match strings(v) {
                        None => r.err(format!("{at}/runs-on"), "type"),
                        Some(list) => {
                            let mut labels: Vec<String> = vec![];
                            for l in list {
                                if !r.no_expression(&l, &format!("{at}/runs-on")) {
                                    continue;
                                }
                                match label(&l) {
                                    None => r.err(format!("{at}/runs-on"), "invalid_label"),
                                    Some(ours) if !ours.is_empty() && !labels.contains(&ours) => labels.push(ours),
                                    Some(_) => {}
                                }
                            }
                            job.insert("runsOn".into(), json!(labels));
                        }
                    },
                }

                if let Some(v) = raw.get("needs") {
                    match strings(v) {
                        None => r.err(format!("{at}/needs"), "type"),
                        Some(needs) => {
                            job.insert("needs".into(), json!(needs.iter().map(|n| lower(n)).collect::<Vec<_>>()));
                        }
                    }
                }

                if raw.contains_key("timeout-minutes") {
                    match is_int(raw.get("timeout-minutes")) {
                        None => r.err(format!("{at}/timeout-minutes"), "type"),
                        Some(t) => {
                            job.insert("timeoutMin".into(), json!(t));
                        }
                    }
                }

                if let Some(env) = r.env_map(raw.get("env"), &format!("{at}/env"), &mut secrets) {
                    job.insert("env".into(), env);
                }

                let mut matrix_keys: Vec<String> = vec![];
                if let Some(strategy) = raw.get("strategy") {
                    match strategy.as_object() {
                        None => r.err(format!("{at}/strategy"), "type"),
                        Some(s) => {
                            r.unknown(s, STRATEGY_KEYS, STRATEGY_IGNORED, &format!("{at}/strategy"));
                            if let Some(m) = s.get("matrix") {
                                match m.as_object() {
                                    None => r.err(format!("{at}/strategy/matrix"), "type"),
                                    Some(m) => {
                                        let mut matrix = Map::new();
                                        for (k, values) in m {
                                            let here = format!("{at}/strategy/matrix{}", ptr(k));
                                            if k == "include" || k == "exclude" {
                                                r.err(&here, "unknown_field");
                                                continue;
                                            }
                                            if !is_matrix_key(k) {
                                                r.err(&here, "invalid_name");
                                            }
                                            let list: Option<Vec<String>> =
                                                values.as_array().and_then(|vs| vs.iter().map(scalar).collect());
                                            match list {
                                                None => r.err(&here, "type"),
                                                Some(list) => {
                                                    matrix_keys.push(k.clone());
                                                    matrix.insert(k.clone(), json!(list));
                                                }
                                            }
                                        }
                                        if !matrix.is_empty() {
                                            job.insert("matrix".into(), Value::Object(matrix));
                                        }
                                    }
                                }
                            }
                        }
                    }
                }

                let mut steps: Vec<Value> = vec![];
                match raw.get("steps") {
                    None => r.err(format!("{at}/steps"), "missing"),
                    Some(v) if !v.is_array() => r.err(format!("{at}/steps"), "type"),
                    Some(v) => {
                        for (i, s) in v.as_array().unwrap().iter().enumerate() {
                            let sat = format!("{at}/steps/{i}");
                            let Some(s) = s.as_object() else {
                                r.err(&sat, "type");
                                continue;
                            };
                            r.unknown(s, STEP_KEYS, STEP_IGNORED, &sat);
                            if let Some(uses) = s.get("uses") {
                                let action = uses.as_str().map(|u| u.split('@').next().unwrap_or_default().to_lowercase());
                                if !action.is_some_and(|a| ACTIONS.contains(&a.as_str())) {
                                    r.err(format!("{sat}/uses"), "unsupported_action");
                                }
                                if s.contains_key("run") {
                                    r.err(format!("{sat}/run"), "invalid_value");
                                }
                                continue;
                            }
                            if s.contains_key("with") {
                                r.err(format!("{sat}/with"), "unknown_field");
                            }
                            let Some(run) = s.get("run").and_then(Value::as_str) else {
                                r.err(format!("{sat}/run"), if s.contains_key("run") { "type" } else { "missing" });
                                continue;
                            };
                            if let Some(shell) = s.get("shell") {
                                if !shell.as_str().is_some_and(|sh| SHELLS.contains(&sh)) {
                                    r.err(format!("{sat}/shell"), "invalid_value");
                                }
                            }
                            let run = r.run_text(run, &format!("{sat}/run"), &matrix_keys, &mut secrets);
                            let mut step = Map::new();
                            if let Some(n) = s.get("name") {
                                match n.as_str() {
                                    None => r.err(format!("{sat}/name"), "type"),
                                    Some(n) if r.no_expression(n, &format!("{sat}/name")) => {
                                        step.insert("name".into(), json!(n));
                                    }
                                    Some(_) => {}
                                }
                            }
                            if !step.contains_key("name") {
                                let first = run
                                    .split('\n')
                                    .find(|line| !line.trim().is_empty())
                                    .map_or_else(|| "run".to_string(), |line| line.trim().chars().take(64).collect());
                                step.insert("name".into(), json!(first));
                            }
                            step.insert("run".into(), json!(run));
                            if let Some(env) = r.env_map(s.get("env"), &format!("{sat}/env"), &mut secrets) {
                                step.insert("env".into(), env);
                            }
                            if s.contains_key("timeout-minutes") {
                                match is_int(s.get("timeout-minutes")) {
                                    None => r.err(format!("{sat}/timeout-minutes"), "type"),
                                    Some(t) => {
                                        step.insert("timeoutMin".into(), json!(t));
                                    }
                                }
                            }
                            if let Some(c) = s.get("continue-on-error") {
                                match c.as_bool() {
                                    None => r.err(format!("{sat}/continue-on-error"), "type"),
                                    Some(c) => {
                                        step.insert("continueOnError".into(), json!(c));
                                    }
                                }
                            }
                            if let Some(w) = s.get("working-directory") {
                                match w.as_str() {
                                    None => r.err(format!("{sat}/working-directory"), "type"),
                                    Some(w) if r.no_expression(w, &format!("{sat}/working-directory")) => {
                                        step.insert("workingDirectory".into(), json!(w));
                                    }
                                    Some(_) => {}
                                }
                            }
                            steps.push(Value::Object(step));
                        }
                    }
                }
                job.insert("steps".into(), json!(steps));
                if let Some(p) = &paths {
                    job.insert("paths".into(), json!(p));
                }
                if !secrets.is_empty() {
                    job.insert("secrets".into(), json!(secrets.into_iter().collect::<Vec<_>>()));
                }
                jobs.insert(lower(id), Value::Object(job));
            }
        }
    }

    let mut errors = r.errors;
    errors.sort();
    errors.dedup();
    if !errors.is_empty() {
        return WorkflowRead { file: None, errors };
    }
    let mut file = Map::new();
    file.insert("version".into(), json!(PIPELINE_VERSION));
    if let Some(name) = name {
        file.insert("name".into(), json!(name));
    }
    file.insert("on".into(), Value::Object(on));
    if let Some(env) = top_env {
        file.insert("env".into(), env);
    }
    if !top_secrets.is_empty() {
        file.insert("secrets".into(), json!(top_secrets.into_iter().collect::<Vec<_>>()));
    }
    file.insert("jobs".into(), Value::Object(jobs));
    WorkflowRead { file: Some(Value::Object(file)), errors: vec![] }
}
