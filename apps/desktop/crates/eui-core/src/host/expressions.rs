//! VAPP-191 (eApps Q2): expressions are type-checked at package validation,
//! the twin of `src/host/expressions.ts`. Every `{"call": "expr", "args":
//! {"expr": "<CEL>", "vars": {…}}}` anywhere in a template is parsed and
//! checked ([`crate::cel::check_expression`]) against the types of its
//! vars: a literal's own type, a records binding's value (the kind's schema
//! fields) or the template data's; anything else is open (`dyn`). An issue
//! names the JSON pointer of the expression and the reason
//! (`fixtures/expressions.json` `packages`).

use std::collections::HashMap;

use serde_json::{json, Map, Value};

use super::package::PackageIssue;
use super::query::{expand_query_source, parse_records_source, RecordsQuery};
use super::record_store::RECORDS_SCHEME;
use super::sources::parse_source;
use super::query::{RECORDS_COMPUTED_MAX, RECORDS_COMPUTED_ROW_KEYS, RECORDS_ROW_KEYS};
use crate::cel::{check_expression, expression_names, is_optional, optional_type, parse_expression, required_type, type_name};

fn ty(name: &str) -> Value {
    Value::String(name.into())
}

fn escape(key: &str) -> String {
    key.replace('~', "~0").replace('/', "~1")
}

fn unescape(key: &str) -> String {
    key.replace("~1", "/").replace("~0", "~")
}

/// A JSON Schema property → its expression type.
pub fn schema_type(schema: Option<&Value>) -> Value {
    let Some(schema) = schema.and_then(Value::as_object) else { return ty("dyn") };
    let types: Vec<Option<&str>> = match schema.get("type") {
        Some(Value::Array(list)) => list.iter().filter(|t| t.as_str() != Some("null")).map(Value::as_str).collect(),
        other => vec![other.and_then(Value::as_str)],
    };
    let kind = if types.len() == 1 { types[0] } else { None };
    match kind {
        Some("number" | "integer") => ty("number"),
        Some("boolean") => ty("bool"),
        Some("string") => match schema.get("format").and_then(Value::as_str) {
            Some("date") => ty("date"),
            Some("date-time") => ty("timestamp"),
            _ => ty("string"),
        },
        Some("array") => Value::Array(vec![schema_type(schema.get("items"))]),
        Some("object") => match schema.get("properties").and_then(Value::as_object) {
            Some(props) => row_of(props, schema.get("required")),
            None => ty("map"),
        },
        _ => ty("dyn"),
    }
}

/// VAPP-195 (E2): a schema's fields; one not in `required` may be missing
/// (`T?`).
pub fn row_of(props: &Map<String, Value>, required: Option<&Value>) -> Value {
    let names = required.and_then(Value::as_array);
    let is_required = |k: &str| names.is_some_and(|n| n.iter().any(|x| x.as_str() == Some(k)));
    Value::Object(
        props
            .iter()
            .map(|(k, v)| {
                let t = schema_type(Some(v));
                (k.clone(), if is_required(k) { t } else { optional_type(t) })
            })
            .collect(),
    )
}

/// A record row of a kind: its schema's fields (`T?` unless required) plus
/// the row keys (always there).
pub fn record_row_type(kind: Option<&Value>) -> Value {
    let schema = kind.and_then(Value::as_object).and_then(|k| k.get("schema")).and_then(Value::as_object);
    let props = schema.and_then(|s| s.get("properties")).and_then(Value::as_object);
    let mut row = match props {
        Some(p) => row_of(p, schema.and_then(|s| s.get("required"))),
        None => Value::Object(Map::new()),
    };
    if let Value::Object(m) = &mut row {
        for (k, t) in [("id", "string"), ("createdAt", "timestamp"), ("updatedAt", "timestamp"), ("createdBy", "string"), ("authorId", "string"), ("author", "string")] {
            m.insert(k.into(), ty(t));
        }
        // VAPP-214: plus its computed fields, each its expression's type
        // (`dyn?` when it does not check).
        if let Some(computed) = kind.and_then(|k| k.get("computed")).and_then(Value::as_object) {
            let vars = computed_vars(kind);
            let mut names: Vec<&String> = computed.keys().collect();
            names.sort();
            for name in names {
                if m.contains_key(name.as_str()) {
                    continue;
                }
                let t = computed[name.as_str()].as_str().and_then(|src| check_expression(src, &vars).ok()).unwrap_or_else(|| optional_type(ty("dyn")));
                m.insert(name.clone(), t);
            }
        }
    }
    row
}

/// VAPP-214: the names a kind's computed field reads: its schema's fields
/// (`T?` unless required) and the row keys but `author`.
fn computed_vars(kind: Option<&Value>) -> Map<String, Value> {
    let schema = kind.and_then(Value::as_object).and_then(|k| k.get("schema")).and_then(Value::as_object);
    let props = schema.and_then(|s| s.get("properties")).and_then(Value::as_object);
    let mut vars = match props.map(|p| row_of(p, schema.and_then(|s| s.get("required")))) {
        Some(Value::Object(m)) => m,
        _ => Map::new(),
    };
    for key in RECORDS_COMPUTED_ROW_KEYS {
        vars.insert(key.into(), ty(if key == "createdAt" || key == "updatedAt" { "timestamp" } else { "string" }));
    }
    vars
}

/// VAPP-214 (Q3): a kind's `computed` ({name: CEL}), each issue at
/// `<at>/computed/<name>`: at most [`RECORDS_COMPUTED_MAX`]; a name is not a
/// schema field nor a row key; the expression is a string that reads no
/// other computed field and type-checks against the kind's fields.
pub fn validate_record_computed(kind: &Value, at: &str) -> Vec<PackageIssue> {
    let Some(computed) = kind.get("computed") else { return Vec::new() };
    let Some(computed) = computed.as_object() else {
        return vec![PackageIssue { path: format!("{at}/computed"), message: "computed is {name: expression}".into() }];
    };
    let mut issues = Vec::new();
    let mut names: Vec<&String> = computed.keys().collect();
    names.sort();
    if names.len() > RECORDS_COMPUTED_MAX {
        issues.push(PackageIssue { path: format!("{at}/computed"), message: format!("at most {RECORDS_COMPUTED_MAX} computed fields per kind") });
    }
    let props = kind.get("schema").and_then(|s| s.get("properties")).and_then(Value::as_object);
    let declared = |n: &str| props.is_some_and(|p| p.contains_key(n));
    let vars = computed_vars(Some(kind));
    for name in names {
        let fp = format!("{at}/computed/{}", escape(name));
        let source = &computed[name.as_str()];
        if declared(name) {
            issues.push(PackageIssue { path: fp, message: format!("{name} shadows a field of the schema") });
        } else if RECORDS_ROW_KEYS.contains(&name.as_str()) {
            issues.push(PackageIssue { path: fp, message: format!("{name} is a reserved record key") });
        } else if let Some(source) = source.as_str() {
            let reads: Vec<String> = parse_expression(source).map(|ast| expression_names(&ast)).unwrap_or_default().into_iter().filter(|n| computed.contains_key(n) && !declared(n)).collect();
            if !reads.is_empty() {
                issues.push(PackageIssue { path: fp, message: format!("a computed field reads no computed field: {}", reads.join(", ")) });
            } else if let Err(message) = check_expression(source, &vars) {
                issues.push(PackageIssue { path: fp, message });
            }
        } else {
            issues.push(PackageIssue { path: fp, message: "a computed field is an expression (a string)".into() });
        }
    }
    issues
}

/// eApps U3: a calendar day cell (calendar.rs).
fn day_cell_type() -> Value {
    json!({"date": "date", "day": "number", "weekday": "string", "initial": "string", "inMonth": "bool", "weekend": "bool", "today": "bool", "selected": "bool", "total": "number", "dots": [{"tone": "string"}]})
}

/// eApps U3: a week segment (a "+N more" marker has no tone and no row).
fn segment_type(row: &Value) -> Value {
    json!({"id": "string", "col": "number", "span": "number", "lane": "number", "contL": "bool", "contR": "bool", "tone": optional_type(ty("string")), "more": "number", "row": optional_type(row.clone())})
}

/// A month view's value (`RecordsMonth`; eApps U3: + weeks, day, agenda).
fn month_type(row: &Value) -> Value {
    json!({
        "key": "string", "label": "string", "prev": "string", "next": "string", "today": "date",
        "days": [{"date": "date", "day": "number", "weekday": "string", "count": "number", "sum": "number", "running": optional_type(ty("map"))}],
        "labels": ["string"], "counts": ["number"], "sums": ["number"],
        "weeks": [{"key": "date", "days": [day_cell_type()], "segments": [segment_type(row)]}],
        "day": "date",
        "agenda": {"date": "date", "label": "string", "count": "number", "items": [{"id": "string", "tone": "string", "start": "date", "end": "date", "days": "number", "row": row}]}
    })
}

/// eApps U3: the week view's value.
fn week_type(row: &Value) -> Value {
    json!({"key": "date", "label": "string", "prev": "date", "next": "date", "today": "date", "days": [day_cell_type()], "segments": [segment_type(row)]})
}

/// eApps U3: the timeline's value.
fn timeline_type(row: &Value) -> Value {
    json!({
        "scale": "string", "key": "date", "label": "string", "prev": "date", "next": "date", "today": "date",
        "columns": [{"key": "date", "label": "string", "end": "date", "today": "bool"}],
        "bands": [{"label": "string", "col": "number", "span": "number"}],
        "rows": [{"key": "string", "index": "number", "lanes": "number", "count": "number", "items": [{"id": "string", "rowIndex": "number", "col": "number", "span": "number", "lane": "number", "contL": "bool", "contR": "bool", "tone": "string", "row": row}]}],
        "now": optional_type(json!({"col": "number", "percent": "number"}))
    })
}

/// The value a records binding lands (docs/package-format.md "Records").
/// VAPP-204: what the binding's query leaves out may be missing (`T?`):
/// `sum` without a sum, `month` without a month view, `groups` without a
/// group, `totals` without an aggregate, `before` without a before (eApps
/// U3: `week`, `timeline` without theirs); with no query known, all of them.
pub fn records_value_type(kind: Option<&Value>, query: Option<&RecordsQuery>) -> Value {
    let has = |present: Option<bool>, t: Value| if present == Some(true) { t } else { optional_type(t) };
    let mut row = record_row_type(kind);
    let plain = row.clone();
    if let (Some(q), Value::Object(m)) = (query, &mut row) {
        if !q.running.is_empty() && q.group.is_empty() {
            m.insert("running".into(), ty("map"));
        }
    }
    let mut out = Map::new();
    out.insert("rows".into(), Value::Array(vec![row]));
    out.insert("count".into(), ty("number"));
    out.insert("sum".into(), has(query.map(|q| q.sum.is_some()), ty("number")));
    out.insert("groups".into(), has(query.map(|q| !q.group.is_empty()), json!(["map"])));
    out.insert("totals".into(), has(query.map(|q| !q.aggregate.is_empty()), ty("map")));
    out.insert("before".into(), has(query.map(|q| q.before.is_some()), ty("map")));
    out.insert("month".into(), has(query.map(|q| q.month.is_some()), month_type(&plain)));
    out.insert("week".into(), has(query.map(|q| q.week.is_some()), week_type(&plain)));
    out.insert("timeline".into(), has(query.map(|q| q.timeline.is_some()), timeline_type(&plain)));
    Value::Object(out)
}

fn list_type(items: Vec<Value>) -> Value {
    Value::Array(vec![match items.first() {
        None => ty("dyn"),
        Some(first) if items.iter().all(|t| t == first) => first.clone(),
        Some(_) => ty("dyn"),
    }])
}

/// A literal's type: scalars, lists (one item type, else dyn), rows.
pub fn literal_type(v: &Value) -> Value {
    match v {
        Value::Array(items) => list_type(items.iter().map(literal_type).collect()),
        Value::Object(m) => Value::Object(m.iter().map(|(k, x)| (k.clone(), literal_type(x))).collect()),
        other => ty(type_name(other)),
    }
}

/// A walk's answer: the type, or the first field a closed row has not;
/// `strict` = the rows below are closed (a records binding's).
struct Walked {
    t: Result<Value, String>,
    strict: bool,
}

fn open(t: Value) -> Walked {
    Walked { t: Ok(t), strict: false }
}

fn is_index(seg: &str) -> bool {
    !seg.is_empty() && seg.bytes().all(|b| b.is_ascii_digit())
}

/// The type at the rest of a pointer below a type. VAPP-204: an index may
/// be past the end and a map's field missing (`T?`); `strict` = a row's
/// unknown field is an error (a records row), else open (`dyn`).
fn walk(t: Value, segments: &[String], strict: bool) -> Result<Value, String> {
    let mut t = t;
    let mut opt = false;
    for seg in segments {
        if is_optional(&t) {
            opt = true;
            t = required_type(&t);
        }
        t = match t {
            Value::Array(mut item) => {
                if !is_index(seg) {
                    return Ok(ty("dyn"));
                }
                opt = true;
                item.pop().unwrap_or_else(|| ty("dyn"))
            }
            Value::String(name) if name == "map" => {
                opt = true;
                ty("dyn")
            }
            Value::Object(mut m) => match m.shift_remove(seg) {
                Some(f) => f,
                None if strict => return Err(seg.clone()),
                None => return Ok(ty("dyn")),
            },
            _ => return Ok(ty("dyn")),
        };
    }
    Ok(if opt { optional_type(t) } else { t })
}

fn is_binding(v: &Value) -> bool {
    v.as_object().is_some_and(|o| o.len() == 1 && o.get("@path").is_some_and(Value::is_string))
}

fn is_call(v: &Value) -> bool {
    v.as_object().is_some_and(|o| o.get("@call").is_some_and(Value::is_string))
}

/// A kind whose rows have only their schema's fields and the row keys.
fn closed_kind(kind: Option<&Value>) -> bool {
    kind.and_then(|k| k.get("schema")).and_then(|s| s.get("additionalProperties")) != Some(&Value::Bool(true))
}

/// What the expression checker reads a template against.
struct Ctx<'a> {
    template: &'a Map<String, Value>,
    records: Option<&'a Value>,
    queries: Option<&'a Value>,
}

/// The type of an absolute data path in a template: below a records
/// binding its value, else the template data's literal, else dyn.
fn path_type(path: &str, ctx: &Ctx) -> Walked {
    if !path.starts_with('/') {
        return open(ty("dyn"));
    }
    for b in ctx.template.get("bindings").and_then(Value::as_array).into_iter().flatten() {
        let (Some(bp), Some(source)) = (b.get("path").and_then(Value::as_str), b.get("source").and_then(Value::as_str)) else { continue };
        if path != bp && !path.starts_with(&format!("{bp}/")) {
            continue;
        }
        let parsed = parse_source(source);
        let kind = parsed.as_ref().filter(|p| p.scheme == RECORDS_SCHEME).map(|p| p.name.clone());
        let Some(kind) = kind.filter(|k| ctx.records.and_then(Value::as_object).is_some_and(|r| r.contains_key(k))) else { return open(ty("dyn")) };
        let rest: Vec<String> = path[bp.len()..].split('/').enumerate().filter(|(i, s)| *i > 0 || !s.is_empty()).map(|(_, s)| unescape(s)).collect();
        let def = ctx.records.and_then(|r| r.get(&kind));
        let strict = closed_kind(def);
        let query = parse_source(&expand_query_source(source, ctx.queries.filter(|q| q.is_object()))).and_then(|p| parse_records_source(&p));
        return Walked { t: walk(records_value_type(def, query.as_ref()), &rest, strict), strict };
    }
    let mut cur = ctx.template.get("data");
    let segments: Vec<String> = if path == "/" { vec![] } else { path[1..].split('/').map(unescape).collect() };
    for seg in segments {
        cur = match cur {
            Some(Value::Array(list)) if is_index(&seg) => seg.parse::<usize>().ok().and_then(|i| list.get(i)),
            Some(Value::Object(m)) => match m.get(&seg) {
                Some(v) => Some(v),
                None => return open(ty("dyn")),
            },
            _ => return open(ty("dyn")),
        };
    }
    open(cur.map_or_else(|| ty("dyn"), literal_type))
}

/// A type and whether its rows are closed.
#[derive(Clone)]
struct Scope {
    t: Value,
    strict: bool,
}

/// A data path's type where it is read: absolute against the template,
/// relative against the row scope it sits in (VAPP-204), else open.
fn binding_type(path: &str, scope: Option<&Scope>, ctx: &Ctx) -> Walked {
    if path.starts_with('/') {
        return path_type(path, ctx);
    }
    let Some(scope) = scope else { return open(ty("dyn")) };
    let segments: Vec<String> = path.split('/').filter(|s| !s.is_empty()).map(unescape).collect();
    Walked { t: walk(scope.t.clone(), &segments, scope.strict), strict: scope.strict }
}

/// VAPP-204: the row each component of a template renders in: a
/// `children: {componentId, path}` template gives its row component (and
/// everything under it) the list's item type; first place wins, as in the
/// reducer. Components outside every row are absent.
fn row_scopes(ctx: &Ctx) -> HashMap<String, Option<Scope>> {
    let mut by_id: HashMap<&str, &Map<String, Value>> = HashMap::new();
    for c in ctx.template.get("components").and_then(Value::as_array).into_iter().flatten() {
        if let Some((id, o)) = c.as_object().and_then(|o| o.get("id").and_then(Value::as_str).map(|id| (id, o))) {
            by_id.entry(id).or_insert(o);
        }
    }
    let mut scopes: HashMap<String, Option<Scope>> = HashMap::new();
    fn place(id: &str, scope: Option<Scope>, by_id: &HashMap<&str, &Map<String, Value>>, ctx: &Ctx, scopes: &mut HashMap<String, Option<Scope>>) {
        let Some(c) = by_id.get(id) else { return };
        if scopes.contains_key(id) {
            return;
        }
        scopes.insert(id.to_string(), scope.clone());
        match c.get("children") {
            Some(Value::Array(list)) => {
                for child in list.iter().filter_map(Value::as_str) {
                    place(child, scope.clone(), by_id, ctx, scopes);
                }
            }
            Some(Value::Object(t)) => {
                if let (Some(component_id), Some(path)) = (t.get("componentId").and_then(Value::as_str), t.get("path").and_then(Value::as_str)) {
                    let list = binding_type(path, scope.as_ref(), ctx);
                    let lt = list.t.as_ref().map(required_type).unwrap_or_else(|_| ty("dyn"));
                    let item = match &lt {
                        Value::Array(items) => Scope { t: items.first().cloned().unwrap_or_else(|| ty("dyn")), strict: list.strict },
                        _ => Scope { t: ty("dyn"), strict: false },
                    };
                    place(component_id, Some(item), by_id, ctx, scopes);
                }
            }
            _ => {}
        }
        if let Some(slots) = c.get("slots").and_then(Value::as_object) {
            for child in slots.values().filter_map(Value::as_str) {
                place(child, scope.clone(), by_id, ctx, scopes);
            }
        }
    }
    place("root", None, &by_id, ctx, &mut scopes);
    scopes
}

/// A var's type; one the checker cannot type may be missing (`dyn?`). A
/// binding to a field a closed row has not is an issue at its path.
fn var_type(v: &Value, at: &str, scope: Option<&Scope>, ctx: &Ctx, issues: &mut Vec<PackageIssue>) -> Value {
    if is_binding(v) {
        let path = v["@path"].as_str().unwrap_or_default();
        return match binding_type(path, scope, ctx).t {
            Err(field) => {
                issues.push(PackageIssue { path: format!("{at}/@path"), message: format!("unknown field {field} in {path}") });
                optional_type(ty("dyn"))
            }
            Ok(t) if t == ty("dyn") => optional_type(t),
            Ok(t) => t,
        };
    }
    if is_call(v) {
        return optional_type(ty("dyn"));
    }
    match v {
        Value::Array(items) => list_type(items.iter().enumerate().map(|(i, x)| var_type(x, &format!("{at}/{i}"), scope, ctx, issues)).collect()),
        Value::Object(m) => Value::Object(m.iter().map(|(k, x)| (k.clone(), var_type(x, &format!("{at}/{}", escape(k)), scope, ctx, issues))).collect()),
        other => ty(type_name(other)),
    }
}

fn visit(value: &Value, at: &str, scope: Option<&Scope>, ctx: &Ctx, issues: &mut Vec<PackageIssue>) {
    match value {
        Value::Array(list) => {
            for (i, v) in list.iter().enumerate() {
                visit(v, &format!("{at}/{i}"), scope, ctx, issues);
            }
        }
        Value::Object(obj) => {
            if let (Some("expr"), Some(Value::Object(args))) = (obj.get("@call").and_then(Value::as_str), obj.get("args")) {
                let here = format!("{at}/args");
                // VAPP-195: every expression binding shows a default when it fails.
                if !args.contains_key("default") {
                    issues.push(PackageIssue { path: format!("{here}/default"), message: "an expression binding needs a default, shown when it fails".into() });
                }
                match (args.get("expr"), args.get("vars")) {
                    (Some(Value::String(expr)), vars) if vars.is_none_or(Value::is_object) => {
                        let mut types = Map::new();
                        let mut unknown = Vec::new();
                        for (name, v) in vars.and_then(Value::as_object).into_iter().flatten() {
                            types.insert(name.clone(), var_type(v, &format!("{here}/vars/{}", escape(name)), scope, ctx, &mut unknown));
                        }
                        let checked = unknown.is_empty();
                        issues.extend(unknown);
                        if checked {
                            if let Err(message) = check_expression(expr, &types) {
                                issues.push(PackageIssue { path: format!("{here}/expr"), message });
                            }
                        }
                    }
                    (Some(Value::String(_)), _) => issues.push(PackageIssue { path: format!("{here}/vars"), message: "vars is an object".into() }),
                    _ => issues.push(PackageIssue { path: format!("{here}/expr"), message: "expr is a string".into() }),
                }
            }
            for (k, v) in obj {
                visit(v, &format!("{at}/{}", escape(k)), scope, ctx, issues);
            }
        }
        _ => {}
    }
}

/// Every `expr` call in a package's templates, checked.
pub fn validate_package_expressions(pkg: &Map<String, Value>) -> Vec<PackageIssue> {
    let mut issues = Vec::new();
    for (id, template) in pkg.get("templates").and_then(Value::as_object).map(super::package::sorted_entries).into_iter().flatten() {
        let Some(template) = template.as_object() else { continue };
        let Some(components) = template.get("components").and_then(Value::as_array) else { continue };
        let ctx = Ctx { template, records: pkg.get("records"), queries: pkg.get("queries") };
        let scopes = row_scopes(&ctx);
        for (i, c) in components.iter().enumerate() {
            let scope = c.get("id").and_then(Value::as_str).and_then(|cid| scopes.get(cid)).and_then(Option::as_ref);
            visit(c, &format!("/templates/{}/components/{i}", escape(id)), scope, &ctx, &mut issues);
        }
    }
    issues
}
