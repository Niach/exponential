//! VAPP-191 (eApps Q2): expressions are type-checked at package validation,
//! the twin of `src/host/expressions.ts`. Every `{"call": "expr", "args":
//! {"expr": "<CEL>", "vars": {…}}}` anywhere in a template is parsed and
//! checked ([`crate::cel::check_expression`]) against the types of its
//! vars: a literal's own type, a records binding's value (the kind's schema
//! fields) or the template data's; anything else is open (`dyn`). An issue
//! names the JSON pointer of the expression and the reason
//! (`fixtures/expressions.json` `packages`).

use serde_json::{json, Map, Value};

use super::package::PackageIssue;
use super::record_store::RECORDS_SCHEME;
use super::sources::parse_source;
use crate::cel::{check_expression, type_name};

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
            Some(props) => row_of(props),
            None => ty("map"),
        },
        _ => ty("dyn"),
    }
}

fn row_of(props: &Map<String, Value>) -> Value {
    Value::Object(props.iter().map(|(k, v)| (k.clone(), schema_type(Some(v)))).collect())
}

/// A record row of a kind: its schema's fields plus the row keys.
pub fn record_row_type(kind: Option<&Value>) -> Value {
    let props = kind.and_then(|k| k.get("schema")).and_then(|s| s.get("properties")).and_then(Value::as_object);
    let mut row = match props {
        Some(p) => row_of(p),
        None => Value::Object(Map::new()),
    };
    if let Value::Object(m) = &mut row {
        for (k, t) in [("id", "string"), ("createdAt", "timestamp"), ("updatedAt", "timestamp"), ("createdBy", "string"), ("authorId", "string"), ("author", "string")] {
            m.insert(k.into(), ty(t));
        }
    }
    row
}

/// The value a records binding lands (docs/package-format.md "Records").
pub fn records_value_type(kind: Option<&Value>) -> Value {
    json!({"rows": [record_row_type(kind)], "count": "number", "sum": "number", "groups": ["map"], "totals": "map", "before": "map", "month": "map"})
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

fn walk(t: Value, segments: &[String]) -> Value {
    let mut t = t;
    for seg in segments {
        t = match t {
            Value::Array(mut item) => {
                if seg.is_empty() || !seg.bytes().all(|b| b.is_ascii_digit()) {
                    return ty("dyn");
                }
                item.pop().unwrap_or_else(|| ty("dyn"))
            }
            Value::Object(mut m) => match m.shift_remove(seg) {
                Some(f) => f,
                None => return ty("dyn"),
            },
            _ => return ty("dyn"),
        };
    }
    t
}

fn is_binding(v: &Value) -> bool {
    v.as_object().is_some_and(|o| o.len() == 1 && o.get("path").is_some_and(Value::is_string))
}

fn is_call(v: &Value) -> bool {
    v.as_object().is_some_and(|o| o.get("call").is_some_and(Value::is_string))
}

/// The type of an absolute data path in a template: below a records
/// binding its value, else the template data's literal, else dyn.
fn path_type(path: &str, template: &Map<String, Value>, records: Option<&Value>) -> Value {
    if !path.starts_with('/') {
        return ty("dyn");
    }
    for b in template.get("bindings").and_then(Value::as_array).into_iter().flatten() {
        let (Some(bp), Some(source)) = (b.get("path").and_then(Value::as_str), b.get("source").and_then(Value::as_str)) else { continue };
        if path != bp && !path.starts_with(&format!("{bp}/")) {
            continue;
        }
        let parsed = parse_source(source);
        let kind = parsed.as_ref().and_then(|p| {
            if p.scheme == RECORDS_SCHEME {
                Some(p.name.clone())
            } else if p.scheme == "exp" && p.name == "records" {
                p.params.get("kind").cloned()
            } else {
                None
            }
        });
        let Some(kind) = kind.filter(|k| records.and_then(Value::as_object).is_some_and(|r| r.contains_key(k))) else { return ty("dyn") };
        let rest: Vec<String> = path[bp.len()..].split('/').enumerate().filter(|(i, s)| *i > 0 || !s.is_empty()).map(|(_, s)| unescape(s)).collect();
        return walk(records_value_type(records.and_then(|r| r.get(&kind))), &rest);
    }
    let mut cur = template.get("data");
    let segments: Vec<String> = if path == "/" { vec![] } else { path[1..].split('/').map(unescape).collect() };
    for seg in segments {
        cur = match cur {
            Some(Value::Array(list)) if !seg.is_empty() && seg.bytes().all(|b| b.is_ascii_digit()) => seg.parse::<usize>().ok().and_then(|i| list.get(i)),
            Some(Value::Object(m)) => match m.get(&seg) {
                Some(v) => Some(v),
                None => return ty("dyn"),
            },
            _ => return ty("dyn"),
        };
    }
    cur.map_or_else(|| ty("dyn"), literal_type)
}

fn var_type(v: &Value, template: &Map<String, Value>, records: Option<&Value>) -> Value {
    if is_binding(v) {
        return path_type(v["path"].as_str().unwrap_or_default(), template, records);
    }
    if is_call(v) {
        return ty("dyn");
    }
    match v {
        Value::Array(items) => list_type(items.iter().map(|x| var_type(x, template, records)).collect()),
        Value::Object(m) => Value::Object(m.iter().map(|(k, x)| (k.clone(), var_type(x, template, records))).collect()),
        other => ty(type_name(other)),
    }
}

fn visit(value: &Value, at: &str, template: &Map<String, Value>, records: Option<&Value>, issues: &mut Vec<PackageIssue>) {
    match value {
        Value::Array(list) => {
            for (i, v) in list.iter().enumerate() {
                visit(v, &format!("{at}/{i}"), template, records, issues);
            }
        }
        Value::Object(obj) => {
            if let (Some("expr"), Some(Value::Object(args))) = (obj.get("call").and_then(Value::as_str), obj.get("args")) {
                let here = format!("{at}/args");
                match (args.get("expr"), args.get("vars")) {
                    (Some(Value::String(expr)), vars) if vars.is_none_or(Value::is_object) => {
                        let mut types = Map::new();
                        for (name, v) in vars.and_then(Value::as_object).into_iter().flatten() {
                            types.insert(name.clone(), var_type(v, template, records));
                        }
                        if let Err(message) = check_expression(expr, &types) {
                            issues.push(PackageIssue { path: format!("{here}/expr"), message });
                        }
                    }
                    (Some(Value::String(_)), _) => issues.push(PackageIssue { path: format!("{here}/vars"), message: "vars is an object".into() }),
                    _ => issues.push(PackageIssue { path: format!("{here}/expr"), message: "expr is a string".into() }),
                }
            }
            for (k, v) in obj {
                visit(v, &format!("{at}/{}", escape(k)), template, records, issues);
            }
        }
        _ => {}
    }
}

/// Every `expr` call in a package's templates, checked.
pub fn validate_package_expressions(pkg: &Map<String, Value>) -> Vec<PackageIssue> {
    let mut issues = Vec::new();
    let records = pkg.get("records");
    for (id, template) in pkg.get("templates").and_then(Value::as_object).into_iter().flatten() {
        let Some(template) = template.as_object() else { continue };
        let Some(components) = template.get("components").and_then(Value::as_array) else { continue };
        for (i, c) in components.iter().enumerate() {
            visit(c, &format!("/templates/{}/components/{i}", escape(id)), template, records, &mut issues);
        }
    }
    issues
}
