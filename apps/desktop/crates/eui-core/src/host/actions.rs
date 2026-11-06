//! VAPP-194 (eApps E1, spec docs/spec/apps.md §4.5): typed effects, the
//! twin of `packages/exponential-ui/src/host/actions.ts` (read its header).
//!
//! - EXPRESSIONS ARE PURE: [`validate_package_effects`] refuses a write or
//!   host function in a template's pure places (the parser refuses one in a
//!   CEL expression, [`crate::cel::function_effect`]).
//! - ACTIONS (`actions: {<name>: {params?, steps}}`) are the only place a
//!   package writes. [`run_action`] runs one against ONE snapshot and
//!   returns the PLAN (`{writes, sets, calls}`) or the one error that
//!   discards it; the host commits the writes as ONE unit (`records.batch`,
//!   [`apply_record_batch`] over in-memory rows), applies the sets, runs the
//!   calls. `fixtures/actions.json` locks it (TS, Rust, Swift, Kotlin).
//! - VAPP-271: a `call` step may name its answer (`result: <name>`). It
//!   runs when the plan reaches it, before the commit: [`run_action`]
//!   answers [`ActionRun::Pending`] for the first one not in
//!   [`ActionContext::results`]; the host runs it and runs the action again
//!   over the same snapshot with the answer added, so later steps read it
//!   as `results.<name>`.

use serde_json::{json, Map, Value};

use super::expressions::{literal_type, row_of, schema_type};
use super::package::PackageIssue;
use super::records::{validate_params_schema, validate_record, without_nulls};
use super::utf16_cmp;
use crate::cel::{check_expression, effect_words, evaluate_counted, is_optional, optional_type, parse_expression, required_type, type_display, type_name, ExpressionEnv, FunctionEffect, MAX_STEPS};

pub use crate::cel::function_effect;

/// Caps: actions per package, steps per action, and the evaluation steps
/// ONE run may spend over all of its expressions (each keeps its own 10 000).
pub const ACTION_LIMIT_ACTIONS: usize = 32;
pub const ACTION_LIMIT_STEPS: usize = 64;
pub const ACTION_LIMIT_MAX_STEPS: usize = 100_000;
/// VAPP-271: a `call` step's `result` name (`results.<name>`).
pub const ACTION_RESULT_PATTERN: &str = "^[a-z][A-Za-z0-9_]{0,31}$";
/// The step forms, by their one key.
pub const ACTION_STEP_KINDS: &[&str] = &["require", "records.upsert", "records.delete", "set", "call"];
/// An action's name (`actions.<name>`).
pub const ACTION_NAME_PATTERN: &str = "^[a-z][A-Za-z0-9_]{0,31}$";
/// The function-call prefix that runs a package action.
pub const ACTION_CALL_PREFIX: &str = "actions.";
/// The host function a host commits an action's record writes through.
pub const RECORDS_BATCH_FUNCTION: &str = "records.batch";

/// The snapshot an action runs against.
#[derive(Default, Clone, Copy)]
pub struct ActionContext<'a> {
    /// The surface data model at the press (the `data` var; `None` = null).
    pub data: Option<&'a Value>,
    /// The member pressing (the `me` var; `None` = null).
    pub me: Option<&'a str>,
    /// The clock (epoch ms; `None` = the wall clock).
    pub now: Option<f64>,
    /// The team zone: minutes east of UTC at an instant (`None` = UTC).
    pub offset: Option<&'a (dyn Fn(f64) -> i32 + Send + Sync)>,
    /// The run's evaluation budget (default [`ACTION_LIMIT_MAX_STEPS`]).
    pub max_steps: Option<usize>,
    /// VAPP-271: the answers of the `call` steps with a `result` so far, by
    /// name (the `results` var; `None` = none yet).
    pub results: Option<&'a Map<String, Value>>,
}

/// VAPP-271: the next `call` step with a `result` whose answer the host
/// must get first (TS `PendingCall`).
#[derive(Debug, Clone, PartialEq)]
pub struct PendingCall {
    pub step: usize,
    pub name: String,
    pub args: Map<String, Value>,
    pub result: String,
}

impl PendingCall {
    /// `{step, name, args, result}` (`fixtures/actions.json` `results.run`).
    pub fn to_value(&self) -> Value {
        json!({"step": self.step, "name": self.name, "args": self.args, "result": self.result})
    }
}

/// What [`run_action`] answers when no step failed: the plan, or the
/// result call the host must run first (VAPP-271).
#[derive(Debug, Clone, PartialEq)]
pub enum ActionRun {
    /// `{writes, sets, calls}`.
    Plan(Value),
    /// The first `call` step with an unanswered `result`.
    Pending(PendingCall),
}

impl ActionRun {
    /// `{"plan": …}` | `{"pending": …}` (TS `ActionResult`).
    pub fn to_value(&self) -> Value {
        match self {
            ActionRun::Plan(plan) => json!({ "plan": plan }),
            ActionRun::Pending(p) => json!({ "pending": p.to_value() }),
        }
    }
}

fn escape(key: &str) -> String {
    key.replace('~', "~0").replace('/', "~1")
}

fn issue(path: impl Into<String>, message: impl Into<String>) -> PackageIssue {
    PackageIssue { path: path.into(), message: message.into() }
}

fn ty(name: &str) -> Value {
    Value::String(name.into())
}

/// `^(\/([^/~]|~[01])*)*$`: a JSON pointer.
fn is_pointer(path: &str) -> bool {
    if path.is_empty() {
        return true;
    }
    if !path.starts_with('/') {
        return false;
    }
    let mut chars = path.chars();
    while let Some(c) = chars.next() {
        if c == '~' && !matches!(chars.next(), Some('0' | '1')) {
            return false;
        }
    }
    true
}

/// `^[a-z][A-Za-z0-9_]{0,31}$`.
pub fn is_action_name(name: &str) -> bool {
    let b = name.as_bytes();
    !b.is_empty() && b.len() <= 32 && b[0].is_ascii_lowercase() && b[1..].iter().all(|c| c.is_ascii_alphanumeric() || *c == b'_')
}

/// The action a call name runs (`actions.log` → `log`), else `None`.
pub fn action_call_name(call: &str) -> Option<&str> {
    call.strip_prefix(ACTION_CALL_PREFIX)
}

/// A step's one key, or `None`.
fn step_kind(step: &Value) -> Option<&'static str> {
    let step = step.as_object()?;
    let keys: Vec<&'static str> = step.keys().filter_map(|k| ACTION_STEP_KINDS.iter().find(|s| **s == k.as_str()).copied()).collect();
    if keys.len() == 1 {
        Some(keys[0])
    } else {
        None
    }
}

fn sorted_keys(m: &Map<String, Value>) -> Vec<&String> {
    let mut keys: Vec<&String> = m.keys().collect();
    keys.sort_by(|a, b| utf16_cmp(a, b));
    keys
}

/// The names a step expression reads, typed: `params` (the action's
/// params schema), `data` (the surface data: open), `me` (a member id) and
/// `results` (VAPP-271: the answers EARLIER steps name, each `dyn`).
pub fn action_var_types(action: &Map<String, Value>, results: &[&str]) -> Map<String, Value> {
    let schema = action.get("params").and_then(Value::as_object);
    let empty = Map::new();
    let props = schema.and_then(|s| s.get("properties")).and_then(Value::as_object).unwrap_or(&empty);
    let mut types = Map::new();
    types.insert("params".into(), row_of(props, schema.and_then(|s| s.get("required"))));
    // VAPP-195: the surface data is untyped and the member may be unknown.
    types.insert("data".into(), optional_type(ty("dyn")));
    types.insert("me".into(), optional_type(ty("string")));
    types.insert("results".into(), Value::Object(results.iter().map(|r| (r.to_string(), ty("dyn"))).collect()));
    types
}

fn textual(t: &Value) -> bool {
    matches!(t.as_str(), Some("string" | "date" | "timestamp"))
}

/// May a value of type `t` land in a field of type `field`?
fn fits(field: &Value, t: &Value) -> bool {
    // VAPP-195: a value that may be missing fits only a field that may be.
    if is_optional(t) {
        return is_optional(field) && fits(&required_type(field), &required_type(t));
    }
    if is_optional(field) {
        return fits(&required_type(field), t);
    }
    if field == "dyn" || t == "dyn" || t == "null" {
        return true;
    }
    // VAPP-271: a list of open items (an answer mapped) fits any list; the schema checks it when it runs.
    if let (Some(f), Some(t)) = (field.as_array(), t.as_array()) {
        return fits(f.first().unwrap_or(&Value::Null), t.first().unwrap_or(&Value::Null));
    }
    if textual(field) {
        return textual(t);
    }
    type_display(field) == type_display(t)
}

// ---------------------------------------------------------------------------
// Validation (validate_package)
// ---------------------------------------------------------------------------

/// Every problem with a package's `actions`.
pub fn validate_package_actions(pkg: &Map<String, Value>) -> Vec<PackageIssue> {
    let mut issues = Vec::new();
    let Some(actions) = pkg.get("actions") else { return issues };
    let Some(actions) = actions.as_object() else {
        issues.push(issue("/actions", "actions is an object of named actions"));
        return issues;
    };
    let names = sorted_keys(actions);
    if names.len() > ACTION_LIMIT_ACTIONS {
        issues.push(issue("/actions", format!("at most {ACTION_LIMIT_ACTIONS} actions")));
    }
    for name in names {
        issues.extend(validate_action(pkg, name, &actions[name.as_str()], &format!("/actions/{}", escape(name))));
    }
    issues
}

fn validate_action(pkg: &Map<String, Value>, name: &str, action: &Value, at: &str) -> Vec<PackageIssue> {
    let mut issues = Vec::new();
    if !is_action_name(name) {
        issues.push(issue(at, "an action name is a letter, then letters, digits and underscores"));
    }
    let Some(action) = action.as_object() else {
        issues.push(issue(at, "an action is {params?, steps}"));
        return issues;
    };
    if let Some(params) = action.get("params") {
        issues.extend(validate_params_schema(params, &format!("{at}/params")));
    }
    let steps = match action.get("steps").and_then(Value::as_array) {
        Some(steps) if !steps.is_empty() => steps,
        _ => {
            issues.push(issue(format!("{at}/steps"), "steps is a non-empty array"));
            return issues;
        }
    };
    if steps.len() > ACTION_LIMIT_STEPS {
        issues.push(issue(format!("{at}/steps"), format!("at most {ACTION_LIMIT_STEPS} steps")));
    }
    let empty = Map::new();
    let records = pkg.get("records").and_then(Value::as_object).unwrap_or(&empty);
    let functions: Option<Vec<&str>> = pkg.get("functions").and_then(Value::as_array).map(|l| l.iter().filter_map(Value::as_str).collect());
    // VAPP-271: a step reads the results of the steps before it.
    let mut named: Vec<(&str, usize)> = Vec::new();
    for (i, step) in steps.iter().enumerate() {
        let earlier: Vec<&str> = named.iter().map(|(n, _)| *n).collect();
        issues.extend(validate_step(step, &format!("{at}/steps/{i}"), &action_var_types(action, &earlier), records, functions.as_deref()));
        let Some(result) = step.as_object().and_then(|s| s.get("result")) else { continue };
        let here = format!("{at}/steps/{i}/result");
        if step_kind(step) != Some("call") {
            issues.push(issue(here, "result names a call's answer: only a call step has one"));
        } else if !result.as_str().is_some_and(is_action_name) {
            issues.push(issue(here, "a result name is a letter, then letters, digits and underscores"));
        } else if let Some((_, j)) = named.iter().find(|(n, _)| Some(*n) == result.as_str()) {
            issues.push(issue(here, format!("result {} is already named by step {j}", result.as_str().unwrap_or(""))));
        } else {
            named.push((result.as_str().unwrap_or(""), i));
        }
    }
    issues
}

/// An expression slot: a string = CEL (type-checked), anything else = a
/// literal. The type, or `None` after an issue.
fn slot(value: &Value, at: &str, types: &Map<String, Value>, issues: &mut Vec<PackageIssue>) -> Option<Value> {
    let Value::String(source) = value else { return Some(literal_type(value)) };
    match check_expression(source, types) {
        Ok(t) => Some(t),
        Err(message) => {
            issues.push(issue(at, message));
            None
        }
    }
}

fn validate_step(step: &Value, at: &str, types: &Map<String, Value>, records: &Map<String, Value>, functions: Option<&[&str]>) -> Vec<PackageIssue> {
    let mut issues = Vec::new();
    let (Some(kind), Some(step)) = (step_kind(step), step.as_object()) else {
        return vec![issue(at, format!("a step is one of {}", ACTION_STEP_KINDS.join(", ")))];
    };
    let body = &step[kind];
    let here = format!("{at}/{}", escape(kind));
    match kind {
        "require" => {
            if body.is_string() {
                if let Some(t) = slot(body, &here, types, &mut issues) {
                    if t != "bool" && t != "dyn" {
                        issues.push(issue(&here, format!("require needs a bool, got {}", type_display(&t))));
                    }
                }
            } else {
                issues.push(issue(&here, "require is an expression"));
            }
            if !step.get("message").and_then(Value::as_str).is_some_and(|m| !m.is_empty()) {
                issues.push(issue(format!("{at}/message"), "message is the text shown when the requirement fails"));
            }
        }
        "records.upsert" => {
            let Some(body) = body.as_object() else { return vec![issue(&here, "records.upsert is {kind, data, id?}")] };
            let kind_name = body.get("kind").and_then(Value::as_str).unwrap_or("");
            let def = records.get(kind_name).and_then(Value::as_object);
            if def.is_none() {
                issues.push(issue(format!("{here}/kind"), format!("records:{kind_name} names no declared kind")));
            }
            // VAPP-206: an id that may be missing = create or patch.
            let mut may_create = body.get("id").is_none();
            if let Some(id) = body.get("id") {
                let id_at = format!("{here}/id");
                if let Some(t) = slot(id, &id_at, types, &mut issues) {
                    if !fits(&optional_type(ty("string")), &t) {
                        issues.push(issue(&id_at, format!("id is a string, got {}", type_display(&t))));
                    }
                    if is_optional(&t) {
                        may_create = true;
                    }
                }
            }
            let Some(data) = body.get("data").and_then(Value::as_object) else {
                issues.push(issue(format!("{here}/data"), "data is an object of field expressions"));
                return issues;
            };
            let schema = def.and_then(|d| d.get("schema")).and_then(Value::as_object);
            let empty = Map::new();
            let props = schema.and_then(|s| s.get("properties")).and_then(Value::as_object).unwrap_or(&empty);
            for field in sorted_keys(data) {
                let field_at = format!("{here}/data/{}", escape(field));
                let t = slot(&data[field.as_str()], &field_at, types, &mut issues);
                let Some(schema) = schema else { continue };
                let Some(prop) = props.get(field.as_str()) else {
                    if schema.get("additionalProperties") != Some(&Value::Bool(true)) {
                        issues.push(issue(&field_at, format!("{field} is not in the schema of {kind_name}")));
                    }
                    continue;
                };
                let required = schema.get("required").and_then(Value::as_array).is_some_and(|r| r.iter().any(|x| x.as_str() == Some(field.as_str())));
                let want = if required { schema_type(Some(prop)) } else { optional_type(schema_type(Some(prop))) };
                if let Some(t) = t {
                    if !fits(&want, &t) {
                        issues.push(issue(&field_at, format!("{field} is {}, got {}", type_display(&want), type_display(&t))));
                    }
                }
            }
            // A create names every required field (a patch keeps the row's).
            if let (Some(schema), true) = (schema, may_create) {
                if let Some(required) = schema.get("required").and_then(Value::as_array) {
                    for field in required.iter().filter_map(Value::as_str) {
                        if !data.contains_key(field) {
                            issues.push(issue(format!("{here}/data"), format!("{field} is required by {kind_name}")));
                        }
                    }
                }
            }
        }
        "records.delete" => {
            let Some(id) = body.as_object().and_then(|b| b.get("id")) else { return vec![issue(&here, "records.delete is {id}")] };
            let id_at = format!("{here}/id");
            if let Some(t) = slot(id, &id_at, types, &mut issues) {
                if !fits(&ty("string"), &t) {
                    issues.push(issue(&id_at, format!("id is a string, got {}", type_display(&t))));
                }
            }
        }
        "set" => {
            let body = body.as_object();
            if !body.and_then(|b| b.get("path")).and_then(Value::as_str).is_some_and(is_pointer) {
                issues.push(issue(format!("{here}/path"), "path is a JSON pointer"));
            }
            if let Some(body) = body {
                slot(body.get("value").unwrap_or(&Value::Null), &format!("{here}/value"), types, &mut issues);
            }
        }
        "call" => {
            let Some(name) = body.as_str().filter(|n| !n.is_empty()) else { return vec![issue(&here, "call names a host function")] };
            let effect = function_effect(name);
            if effect == FunctionEffect::Write {
                issues.push(issue(&here, format!("{}: use a records.upsert, records.delete or set step", effect_words(name, effect))));
            } else if effect == FunctionEffect::Pure {
                issues.push(issue(&here, format!("{name} is pure: compute it in an expression")));
            } else if action_call_name(name).is_some() {
                issues.push(issue(&here, "an action never runs another action"));
            } else if let Some(functions) = functions.filter(|_| name != super::screens::NAVIGATE_FUNCTION) {
                if !functions.iter().any(|p| *p == name || p.strip_suffix('*').is_some_and(|prefix| name.starts_with(prefix))) {
                    issues.push(issue(&here, format!("{name} is not in the package's functions")));
                }
            }
            if let Some(args) = step.get("args") {
                match args.as_object() {
                    None => issues.push(issue(format!("{at}/args"), "args is an object of expressions")),
                    Some(args) => {
                        for k in sorted_keys(args) {
                            slot(&args[k.as_str()], &format!("{at}/args/{}", escape(k)), types, &mut issues);
                        }
                    }
                }
            }
        }
        _ => {}
    }
    issues
}

fn pure(value: &Value, at: &str, issues: &mut Vec<PackageIssue>) {
    match value {
        Value::Array(list) => {
            for (i, v) in list.iter().enumerate() {
                pure(v, &format!("{at}/{i}"), issues);
            }
        }
        Value::Object(obj) => {
            if let Some(call) = obj.get("call").and_then(Value::as_str) {
                let effect = function_effect(call);
                if effect != FunctionEffect::Pure {
                    issues.push(issue(format!("{at}/call"), format!("{}: a binding is pure, name an action in on.press", effect_words(call, effect))));
                }
            }
            for (k, v) in obj {
                pure(v, &format!("{at}/{}", escape(k)), issues);
            }
        }
        _ => {}
    }
}

/// VAPP-194: every function call in a template's PURE places (component
/// props, a call's args, an event's context) is pure; an action's
/// `functionCall` may name a host or write function, and an
/// `actions.<name>` one names a declared action.
pub fn validate_package_effects(pkg: &Map<String, Value>) -> Vec<PackageIssue> {
    let mut issues = Vec::new();
    let Some(templates) = pkg.get("templates").and_then(Value::as_object) else { return issues };
    let empty = Map::new();
    let actions = pkg.get("actions").and_then(Value::as_object).unwrap_or(&empty);
    for (id, template) in super::package::sorted_entries(templates) {
        let Some(components) = template.as_object().and_then(|t| t.get("components")).and_then(Value::as_array) else { continue };
        for (i, c) in components.iter().enumerate() {
            let Some(c) = c.as_object() else { continue };
            let at = format!("/templates/{}/components/{i}", escape(id));
            for (k, v) in c {
                if k != "on" {
                    pure(v, &format!("{at}/{}", escape(k)), &mut issues);
                    continue;
                }
                let Some(on) = v.as_object() else { continue };
                for (event, handler) in on {
                    let hat = format!("{at}/on/{}", escape(event));
                    let Some(handler) = handler.as_object() else { continue };
                    for (hk, hv) in handler {
                        let fc = hv.as_object().filter(|_| hk == "functionCall");
                        let Some(fc) = fc else {
                            pure(hv, &format!("{hat}/{}", escape(hk)), &mut issues);
                            continue;
                        };
                        let call = fc.get("call").and_then(Value::as_str).unwrap_or("");
                        if let Some(action) = action_call_name(call) {
                            if !actions.contains_key(action) {
                                issues.push(issue(format!("{hat}/functionCall/call"), format!("action {action} names no package action")));
                            }
                            // VAPP-204: a press's args are the action's params.
                            if let Some(def) = actions.get(action).and_then(Value::as_object) {
                                press_arg_issues(action, def, fc.get("args"), &format!("{hat}/functionCall/args"), &mut issues);
                            }
                        }
                        // VAPP-204: a bind's source is one the package may follow.
                        if call == "bind" {
                            bind_source_issues(pkg, fc.get("args"), &format!("{hat}/functionCall/args"), &mut issues);
                        }
                        for (fk, fv) in fc {
                            if fk != "call" {
                                pure(fv, &format!("{hat}/functionCall/{}", escape(fk)), &mut issues);
                            }
                        }
                    }
                }
            }
        }
    }
    issues
}

/// VAPP-204: a press's `actions.<name>` args against the action's params:
/// every key a param (unless the schema allows additional properties),
/// every required param given, a literal scalar of the param's type (a
/// dynamic value is checked when it runs).
fn press_arg_issues(name: &str, action: &Map<String, Value>, args: Option<&Value>, at: &str, issues: &mut Vec<PackageIssue>) {
    if args.is_some_and(|a| !a.is_object()) {
        issues.push(issue(at, format!("args is an object of action {name}'s params")));
        return;
    }
    let empty = Map::new();
    let schema = action.get("params").and_then(Value::as_object).unwrap_or(&empty);
    let props = schema.get("properties").and_then(Value::as_object).unwrap_or(&empty);
    let required: Vec<&str> = schema.get("required").and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_str).collect();
    let given = args.and_then(Value::as_object).unwrap_or(&empty);
    let mut keys: Vec<&String> = given.keys().collect();
    keys.sort();
    for key in keys {
        let here = format!("{at}/{}", escape(key));
        let Some(prop) = props.get(key) else {
            if schema.get("additionalProperties") != Some(&Value::Bool(true)) {
                issues.push(issue(here, format!("{key} is not a param of action {name}")));
            }
            continue;
        };
        let v = &given[key];
        if !(v.is_string() || v.is_number() || v.is_boolean()) {
            continue;
        }
        let want = if required.contains(&key.as_str()) { schema_type(Some(prop)) } else { optional_type(schema_type(Some(prop))) };
        let t = literal_type(v);
        if !fits(&want, &t) {
            issues.push(issue(here, format!("{key} is {}, got {}", type_display(&want), type_display(&t))));
        }
    }
    for field in required {
        if !given.contains_key(field) {
            issues.push(issue(at, format!("{field} is required by action {name}")));
        }
    }
}

/// VAPP-204: a `bind` press's literal source, checked like a template
/// binding: a scheme:name URI, in the package's `sources`, a declared kind
/// and a package query.
fn bind_source_issues(pkg: &Map<String, Value>, args: Option<&Value>, at: &str, issues: &mut Vec<PackageIssue>) {
    let Some(source) = args.and_then(|a| a.get("source")).and_then(Value::as_str) else { return };
    let here = format!("{at}/source");
    let Some(parsed) = super::sources::parse_source(source) else {
        issues.push(issue(here, "source is a scheme:name URI"));
        return;
    };
    if let Some(list) = pkg.get("sources").and_then(Value::as_array).filter(|l| l.iter().all(Value::is_string)) {
        let allowed: Vec<String> = list.iter().filter_map(Value::as_str).map(str::to_string).collect();
        if !super::package::source_allowed(Some(&allowed), source) {
            issues.push(issue(&here, format!("source {source} is not in the package's sources")));
        }
    }
    if parsed.scheme == super::record_store::RECORDS_SCHEME {
        if !pkg.get("records").and_then(Value::as_object).is_some_and(|r| r.contains_key(&parsed.name)) {
            issues.push(issue(&here, format!("records:{} names no declared kind", parsed.name)));
        }
        if let Some(query) = parsed.params.get("query") {
            if !pkg.get("queries").and_then(Value::as_object).is_some_and(|q| q.contains_key(query)) {
                issues.push(issue(&here, format!("query {query} names no package query")));
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Running (pure: the plan, or the one error)
// ---------------------------------------------------------------------------

fn issue_text(issues: &[PackageIssue]) -> String {
    issues.iter().map(|i| format!("{} {}", if i.path.is_empty() { "/" } else { &i.path }, i.message)).collect::<Vec<_>>().join("; ")
}

/// VAPP-207: a bad param in words (the toast): `stage must be one of a, b`.
fn param_issue_text(issues: &[PackageIssue]) -> String {
    let text = |i: &PackageIssue| {
        let field = i.path.strip_prefix('/').unwrap_or(&i.path);
        if i.message == "required" || i.message == "not in the schema" {
            format!("{field} is {}", i.message)
        } else if i.message.starts_with("one of ") {
            format!("{field} must be {}", i.message.replace('|', ", "))
        } else {
            format!("{field} must be {}", i.message)
        }
    };
    issues.iter().map(text).collect::<Vec<_>>().join("; ")
}

/// Run a package action against one snapshot: its plan `{writes, sets,
/// calls}`, or the error that discards it whole (a false `require` = its
/// message; else `step <i>: …`). VAPP-271: a `call` step with a `result`
/// not yet in `ctx.results` stops the run with [`ActionRun::Pending`] (the
/// plan so far is dropped); an answered one binds `results.<name>` and adds
/// no call to the plan.
pub fn run_action(pkg: &Value, name: &str, params: &Value, ctx: &ActionContext) -> Result<ActionRun, String> {
    let action = pkg.get("actions").and_then(Value::as_object).and_then(|a| a.get(name)).and_then(Value::as_object);
    let Some((action, steps)) = action.and_then(|a| a.get("steps").and_then(Value::as_array).map(|s| (a, s))) else { return Err(format!("no action {name}")) };
    let p = if params.is_null() { Value::Object(Map::new()) } else { params.clone() };
    if !p.is_object() {
        return Err("params is an object".into());
    }
    if let Some(schema) = action.get("params").filter(|s| s.is_object()) {
        // VAPP-206: an optional param left "" (a Select unpicked) passes as absent.
        let required = schema.get("required").and_then(Value::as_array);
        let mut given = p.clone();
        if let Some(m) = given.as_object_mut() {
            m.retain(|k, v| v.as_str() != Some("") || required.is_some_and(|r| r.iter().any(|x| x.as_str() == Some(k.as_str()))));
        }
        let issues = validate_record(schema, &given);
        if !issues.is_empty() {
            return Err(param_issue_text(&issues));
        }
    }
    let mut vars = Map::new();
    vars.insert("params".into(), p);
    vars.insert("data".into(), ctx.data.cloned().unwrap_or(Value::Null));
    vars.insert("me".into(), ctx.me.map_or(Value::Null, |m| Value::String(m.into())));
    vars.insert("results".into(), Value::Object(ctx.results.cloned().unwrap_or_default()));
    let empty = Map::new();
    let records = pkg.get("records").and_then(Value::as_object).unwrap_or(&empty);
    let mut budget: i64 = ctx.max_steps.unwrap_or(ACTION_LIMIT_MAX_STEPS) as i64;
    let mut writes = Vec::new();
    let mut sets = Vec::new();
    let mut calls = Vec::new();
    for (i, step) in steps.iter().enumerate() {
        let r#where = format!("step {i}");
        let limit = || format!("{where}: action step limit exceeded");
        budget -= 1;
        if budget < 0 {
            return Err(limit());
        }
        let mut value = |v: &Value| -> Result<Value, String> {
            let Value::String(source) = v else { return Ok(v.clone()) };
            let ast = parse_expression(source).map_err(|e| format!("{where}: {e}"))?;
            let cap = (MAX_STEPS as i64).min(budget);
            let env = ExpressionEnv { now: ctx.now, offset: ctx.offset, max_steps: Some(cap.max(0) as usize) };
            let (result, used) = evaluate_counted(&ast, &vars, &env);
            match result {
                Err(e) if e == "step limit exceeded" && cap == budget => Err(limit()),
                Err(e) => Err(format!("{where}: {e}")),
                Ok(v) => {
                    budget -= used as i64;
                    if budget < 0 {
                        return Err(limit());
                    }
                    Ok(v)
                }
            }
        };
        let (Some(kind), Some(step)) = (step_kind(step), step.as_object()) else {
            return Err(format!("{where}: a step is one of {}", ACTION_STEP_KINDS.join(", ")));
        };
        let body = &step[kind];
        match kind {
            "require" => {
                let ok = value(body)?;
                let Value::Bool(ok) = ok else { return Err(format!("{where}: require needs a bool, got {}", type_name(&ok))) };
                if !ok {
                    return Err(match step.get("message").and_then(Value::as_str).filter(|m| !m.is_empty()) {
                        Some(m) => m.to_string(),
                        None => format!("{where}: requirement failed"),
                    });
                }
            }
            "records.upsert" => {
                let Some((body, data_exprs)) = body.as_object().and_then(|b| b.get("data").and_then(Value::as_object).map(|d| (b, d))) else {
                    return Err(format!("{where}: records.upsert is {{kind, data, id?}}"));
                };
                let kind_name = body.get("kind").and_then(Value::as_str).unwrap_or("");
                let Some(schema) = records.get(kind_name).and_then(Value::as_object).and_then(|d| d.get("schema")).filter(|s| s.is_object()) else {
                    return Err(format!("{where}: records: no kind {kind_name}"));
                };
                // VAPP-206: a null or empty id = a create, a string = a patch.
                let mut id = None;
                if let Some(expr) = body.get("id") {
                    match value(expr)? {
                        Value::Null => {}
                        Value::String(s) => {
                            if !s.is_empty() {
                                id = Some(s);
                            }
                        }
                        other => return Err(format!("{where}: id is a string, got {}", type_name(&other))),
                    }
                }
                let required = schema.get("required").and_then(Value::as_array);
                let mut data = Map::new();
                for field in sorted_keys(data_exprs) {
                    let raw = &data_exprs[field.as_str()];
                    let v = value(raw)?;
                    // VAPP-206: absent stays absent. An expression's null (a param
                    // left out) writes nothing; "" in an optional field (an
                    // unpicked Select, an emptied Input) is no value. On a patch a
                    // literal null and that "" clear the field.
                    let is_required = required.is_some_and(|r| r.iter().any(|x| x.as_str() == Some(field.as_str())));
                    let none = v.is_null() || (v.as_str() == Some("") && !is_required);
                    if none && (id.is_none() || (v.is_null() && raw.is_string())) {
                        continue;
                    }
                    data.insert(field.clone(), if none { Value::Null } else { v });
                }
                let data = Value::Object(data);
                // A patch is checked field by field (the store merges and checks the row).
                let issues = if id.is_none() {
                    validate_record(schema, &data)
                } else {
                    let mut patch = schema.clone();
                    patch["required"] = json!([]);
                    validate_record(&patch, &data)
                };
                if !issues.is_empty() {
                    return Err(format!("{where}: records: {}", issue_text(&issues)));
                }
                writes.push(match id {
                    None => json!({"op": "upsert", "kind": kind_name, "data": data}),
                    Some(id) => json!({"op": "upsert", "kind": kind_name, "data": data, "id": id}),
                });
            }
            "records.delete" => {
                let Some(body) = body.as_object() else { return Err(format!("{where}: records.delete is {{id}}")) };
                // A missing id evaluates like JS `undefined` (not a string).
                let v = match body.get("id") {
                    Some(expr) => value(expr)?,
                    None => return Err(format!("{where}: id is a string, got null")),
                };
                match v {
                    Value::String(s) if !s.is_empty() => writes.push(json!({"op": "delete", "id": s})),
                    other => return Err(format!("{where}: id is a string, got {}", type_name(&other))),
                }
            }
            "set" => {
                let Some((body, path)) = body.as_object().and_then(|b| b.get("path").and_then(Value::as_str).filter(|p| is_pointer(p)).map(|p| (b, p))) else {
                    return Err(format!("{where}: path is a JSON pointer"));
                };
                let v = value(body.get("value").unwrap_or(&Value::Null))?;
                sets.push(json!({"path": path, "value": v}));
            }
            "call" => {
                let Some(name) = body.as_str().filter(|n| function_effect(n) == FunctionEffect::Host && action_call_name(n).is_none()) else {
                    return Err(format!("{where}: call names a host function"));
                };
                let mut args = Map::new();
                if let Some(given) = step.get("args").and_then(Value::as_object) {
                    for k in sorted_keys(given) {
                        args.insert(k.clone(), value(&given[k.as_str()])?);
                    }
                }
                let Some(result) = step.get("result").and_then(Value::as_str) else {
                    calls.push(json!({"name": name, "args": args}));
                    continue;
                };
                // VAPP-271: an answered result is bound; the first unanswered one stops the run.
                if ctx.results.is_some_and(|r| r.contains_key(result)) {
                    continue;
                }
                return Ok(ActionRun::Pending(PendingCall { step: i, name: name.to_string(), args, result: result.to_string() }));
            }
            _ => {}
        }
    }
    Ok(ActionRun::Plan(json!({"writes": writes, "sets": sets, "calls": calls})))
}

// ---------------------------------------------------------------------------
// The batch (one unit over in-memory rows)
// ---------------------------------------------------------------------------

/// Apply a batch of writes (`{op: "upsert", kind, data, id?}` | `{op:
/// "delete", id}`) to `rows` (`{id, kind, data, createdBy, createdAt,
/// updatedAt}`), all or nothing: the new rows and each write's id, or the
/// first refusal (`write <i>: …`). A patch merges onto its row and the
/// merged row is checked; a create's id is `new_id(<i-th create>)`, its
/// `createdBy` = `me`, every touched row stamped `now`.
pub fn apply_record_batch(rows: &[Value], writes: &[Value], records: Option<&Value>, me: Option<&str>, now: &str, new_id: impl FnMut(usize) -> String) -> Result<(Vec<Value>, Vec<String>), String> {
    apply_record_batch_as(rows, writes, records, me, None, None, now, new_id)
}

/// eApps R3: [`apply_record_batch`] with every write judged by its kind's
/// policy for the writer `me` with team `role` and app `access` (TS
/// `BatchOptions.role` / `access`); `role` `None` = not judged.
#[allow(clippy::too_many_arguments)]
pub fn apply_record_batch_as(
    rows: &[Value],
    writes: &[Value],
    records: Option<&Value>,
    me: Option<&str>,
    role: Option<&str>,
    access: Option<&str>,
    now: &str,
    mut new_id: impl FnMut(usize) -> String,
) -> Result<(Vec<Value>, Vec<String>), String> {
    use super::record_policy::{record_policy, record_refusal, RecordOp, RecordViewer};
    let judged = role.is_some();
    let viewer = RecordViewer { id: me, role, access };
    let kind_def = |kind: &str| records.and_then(|r| r.get(kind));
    let judge = |kind: &str, op: RecordOp, row: Option<&Value>| if judged { record_refusal(&record_policy(kind_def(kind)), op, &viewer, row) } else { None };
    // A row the writer may not read is not there for them.
    let readable = |r: &Value| !judged || record_refusal(&record_policy(kind_def(r.get("kind").and_then(Value::as_str).unwrap_or(""))), RecordOp::Read, &viewer, Some(r)).is_none();
    let mut out: Vec<Value> = rows.to_vec();
    let mut ids = Vec::new();
    let mut creates = 0;
    let row_id = |r: &Value| r.get("id").and_then(Value::as_str).map(str::to_string);
    for (i, w) in writes.iter().enumerate() {
        let r#where = format!("write {i}");
        let wid = w.get("id").map(|v| v.as_str().map_or_else(|| v.to_string(), str::to_string));
        if w.get("op").and_then(Value::as_str) == Some("delete") {
            let id = wid.unwrap_or_else(|| "undefined".into());
            let Some(row) = out.iter().find(|r| row_id(r).as_deref() == Some(id.as_str()) && readable(r)) else {
                return Err(format!("{where}: record {id} not found"));
            };
            if let Some(refusal) = judge(row.get("kind").and_then(Value::as_str).unwrap_or(""), RecordOp::Delete, Some(row)) {
                return Err(format!("{where}: {refusal}"));
            }
            out.retain(|r| row_id(r).as_deref() != Some(id.as_str()));
            ids.push(id);
            continue;
        }
        let kind = w.get("kind").and_then(Value::as_str).unwrap_or("undefined");
        let Some(schema) = records.and_then(Value::as_object).and_then(|r| r.get(kind)).and_then(Value::as_object).and_then(|d| d.get("schema")).filter(|s| s.is_object()) else {
            return Err(format!("{where}: records: no kind {kind}"));
        };
        let wdata = w.get("data").cloned().unwrap_or(Value::Null);
        if let Some(id) = wid {
            let Some(index) = out.iter().position(|r| row_id(r).as_deref() == Some(id.as_str()) && readable(r)) else { return Err(format!("{where}: record {id} not found")) };
            if out[index].get("kind").and_then(Value::as_str) != Some(kind) {
                return Err(format!("{where}: record {id} is not of kind {kind}"));
            }
            if let Some(refusal) = judge(kind, RecordOp::Update, Some(&out[index])) {
                return Err(format!("{where}: {refusal}"));
            }
            let mut data = out[index].get("data").and_then(Value::as_object).cloned().unwrap_or_default();
            if let Some(patch) = wdata.as_object() {
                for (k, v) in patch {
                    data.insert(k.clone(), v.clone());
                }
            }
            let data = Value::Object(data);
            let issues = validate_record(schema, &data);
            if !issues.is_empty() {
                return Err(format!("{where}: records: {}", issue_text(&issues)));
            }
            if let Some(row) = out[index].as_object_mut() {
                row.insert("data".into(), data);
                row.insert("updatedAt".into(), Value::String(now.into()));
            }
            ids.push(id);
            continue;
        }
        if let Some(refusal) = judge(kind, RecordOp::Create, None) {
            return Err(format!("{where}: {refusal}"));
        }
        let wdata = without_nulls(&wdata);
        let issues = validate_record(schema, &wdata);
        if !issues.is_empty() {
            return Err(format!("{where}: records: {}", issue_text(&issues)));
        }
        let id = new_id(creates);
        creates += 1;
        out.push(json!({"id": id, "kind": kind, "data": wdata, "createdBy": me, "authorId": me, "createdAt": now, "updatedAt": now}));
        ids.push(id);
    }
    Ok((out, ids))
}
