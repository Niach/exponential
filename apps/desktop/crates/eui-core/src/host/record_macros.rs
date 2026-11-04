//! VAPP-217 (eApps U2): the HOST half of the RecordForm and RecordTable
//! macros, the mirror of `src/host/record-macros.ts`
//! (`fixtures/record-macros.json` locks both). A macro sees only its node's
//! props, so before a template is reduced the host fills every RecordForm /
//! RecordTable node of the flat component list from its kind's schema: the
//! fields with their checks, the table columns (the Data tab's typed
//! widths), the rows binding, the draft path and the actions. The macro
//! table then expands the filled node like any other.

use serde_json::{json, Map, Value};

/// The Data tab's typed column widths (by format, then by type).
fn column_width(field: &Map<String, Value>) -> Option<i64> {
    let ty = field.get("type").and_then(Value::as_str).unwrap_or("");
    let key = if ty == "string" { field.get("format").and_then(Value::as_str)? } else { ty };
    match key {
        "date" => Some(104),
        "time" => Some(64),
        "date-time" => Some(152),
        "number" | "integer" => Some(64),
        "boolean" => Some(56),
        _ => None,
    }
}

pub const RECORD_UPDATED_WIDTH: i64 = 88;
pub const RECORD_MENU_WIDTH: i64 = 80;
/// A string longer than this (its `maxLength`) is a Textarea.
pub const RECORD_LONG_TEXT: f64 = 200.0;
pub const RECORD_FORM_ROOT: &str = "/recordForm";
pub const RECORD_TABLE_ROOT: &str = "/recordTable";

/// `closeDate` / `close_date` → `Close date`.
pub fn field_label(name: &str) -> String {
    let mut spaced = String::new();
    let mut prev: Option<char> = None;
    for ch in name.chars() {
        let ch = if ch == '_' { ' ' } else { ch };
        if ch.is_ascii_uppercase() && prev.is_some_and(|p| p.is_ascii_lowercase() || p.is_ascii_digit()) {
            spaced.push(' ');
        }
        spaced.push(ch);
        prev = Some(ch);
    }
    let words = spaced.trim().to_lowercase();
    let mut chars = words.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => name.to_string(),
    }
}

fn properties(kind: &Value) -> Map<String, Value> {
    kind.get("schema").and_then(|s| s.get("properties")).and_then(Value::as_object).cloned().unwrap_or_default()
}

fn required(kind: &Value) -> Vec<String> {
    kind.get("schema").and_then(|s| s.get("required")).and_then(Value::as_array).map(|l| l.iter().filter_map(Value::as_str).map(str::to_string).collect()).unwrap_or_default()
}

/// The Data tab's column order: required (in order), then the rest by name.
pub fn record_columns(kind: &Value) -> Vec<String> {
    let props = properties(kind);
    let mut req: Vec<String> = Vec::new();
    for n in required(kind) {
        if props.contains_key(&n) && !req.contains(&n) {
            req.push(n);
        }
    }
    let mut rest: Vec<String> = props.keys().filter(|n| !req.contains(n)).cloned().collect();
    rest.sort();
    req.extend(rest);
    req
}

fn is_file(f: &Map<String, Value>) -> bool {
    f.get("format").and_then(Value::as_str) == Some("attachment")
        || (f.get("type").and_then(Value::as_str) == Some("array") && f.get("items").and_then(|i| i.get("format")).and_then(Value::as_str) == Some("attachment"))
}

fn options(values: &Value) -> Value {
    Value::Array(values.as_array().into_iter().flatten().map(|v| json!({"label": v, "value": v})).collect())
}

/// The form fields of a kind, values under `path` (files left out).
pub fn record_form_fields(kind: &Value, path: &str) -> Vec<Value> {
    let props = properties(kind);
    let req = required(kind);
    let refs = kind.get("refs").and_then(Value::as_object).cloned().unwrap_or_default();
    let mut out = Vec::new();
    for name in record_columns(kind) {
        let Some(f) = props.get(&name).and_then(Value::as_object) else { continue };
        if is_file(f) {
            continue;
        }
        let ty = f.get("type").and_then(Value::as_str).unwrap_or("");
        let format = f.get("format").and_then(Value::as_str);
        let value = json!({"path": format!("{path}/{name}")});
        let is_required = req.contains(&name);
        let mut o = Map::new();
        o.insert("name".into(), json!(name));
        o.insert("label".into(), json!(field_label(&name)));
        o.insert("widget".into(), json!("text"));
        o.insert("required".into(), json!(is_required));
        o.insert("wide".into(), json!(false));
        o.insert("value".into(), value.clone());
        if let Some(d) = f.get("description").and_then(Value::as_str).filter(|d| !d.is_empty()) {
            o.insert("placeholder".into(), json!(d));
        }
        let reference = refs.get(&name).and_then(Value::as_str);
        let mut widget = "text";
        if reference == Some("member") || reference == Some("issue") {
            widget = "select";
            o.insert("source".into(), json!(if reference == Some("member") { "exp:members" } else { "exp:issues" }));
            o.insert("searchable".into(), json!(true));
        } else if ty == "boolean" {
            widget = "switch";
        } else if ty == "number" || ty == "integer" {
            widget = "number";
            if let Some(m) = f.get("minimum") {
                o.insert("min".into(), m.clone());
            }
            if let Some(m) = f.get("maximum") {
                o.insert("max".into(), m.clone());
            }
            if ty == "integer" {
                o.insert("step".into(), json!(1));
                o.insert("precision".into(), json!(0));
            }
        } else if ty == "array" {
            if let Some(values) = f.get("items").and_then(|i| i.get("enum")) {
                widget = "select";
                o.insert("multiple".into(), json!(true));
                o.insert("options".into(), options(values));
            } else {
                widget = "chips";
            }
            if let Some(m) = f.get("maxLength") {
                o.insert("max".into(), m.clone());
            }
            o.insert("wide".into(), json!(true));
        } else if let Some(values) = f.get("enum") {
            widget = "select";
            o.insert("options".into(), options(values));
        } else if format == Some("date") {
            widget = "date";
        } else if format == Some("time") {
            widget = "time";
        } else if format == Some("date-time") {
            o.entry("placeholder").or_insert(json!("YYYY-MM-DDTHH:MM"));
        } else if f.get("maxLength").and_then(Value::as_f64).is_none_or(|m| m > RECORD_LONG_TEXT) {
            widget = "textarea";
            o.insert("wide".into(), json!(true));
        }
        o.insert("widget".into(), json!(widget));
        let mut checks = Vec::new();
        if is_required && widget != "switch" {
            checks.push(json!({"condition": {"call": "required", "args": {"value": value}}, "message": "$string.required"}));
        }
        if ty == "string" && f.get("enum").is_none() && widget != "select" {
            if let Some(max) = f.get("maxLength") {
                checks.push(json!({"condition": {"call": "length", "args": {"value": value, "max": max}}, "message": "$string.invalidValue"}));
            }
        }
        if (ty == "number" || ty == "integer") && (f.contains_key("minimum") || f.contains_key("maximum")) {
            let mut args = Map::new();
            args.insert("value".into(), value.clone());
            if let Some(m) = f.get("minimum") {
                args.insert("min".into(), m.clone());
            }
            if let Some(m) = f.get("maximum") {
                args.insert("max".into(), m.clone());
            }
            checks.push(json!({"condition": {"call": "numeric", "args": args}, "message": "$string.invalidValue"}));
        }
        if !checks.is_empty() {
            o.insert("checks".into(), Value::Array(checks));
        }
        out.push(Value::Object(o));
    }
    out
}

fn shown_columns(kind: &Value, columns: Option<&[String]>) -> Vec<String> {
    let props = properties(kind);
    let names = columns.map(<[String]>::to_vec).unwrap_or_else(|| record_columns(kind));
    names.into_iter().filter(|n| props.get(n).and_then(Value::as_object).is_some_and(|f| !is_file(f))).collect()
}

/// The table columns: the chosen fields (default every one), typed and
/// sized like the Data tab, then `updatedAt`, then the row menu.
pub fn record_table_columns(kind: &Value, columns: Option<&[String]>, editable: bool) -> Vec<Value> {
    let props = properties(kind);
    let mut out: Vec<Value> = shown_columns(kind, columns)
        .into_iter()
        .map(|key| {
            let f = props.get(&key).and_then(Value::as_object).cloned().unwrap_or_default();
            let ty = f.get("type").and_then(Value::as_str).unwrap_or("");
            let mut col = Map::new();
            col.insert("key".into(), json!(key));
            col.insert("label".into(), json!(field_label(&key)));
            col.insert("sortable".into(), json!(true));
            if ty == "number" || ty == "integer" {
                col.insert("type".into(), json!("number"));
                col.insert("align".into(), json!("end"));
            } else if ty == "boolean" {
                col.insert("type".into(), json!("boolean"));
            } else if f.get("format").and_then(Value::as_str) == Some("date") {
                col.insert("type".into(), json!("date"));
            }
            match column_width(&f) {
                Some(w) => col.insert("width".into(), json!(w)),
                None => col.insert("truncate".into(), json!(true)),
            };
            Value::Object(col)
        })
        .collect();
    out.push(json!({"key": "updatedAt", "label": "Updated", "type": "relativeTime", "width": RECORD_UPDATED_WIDTH, "sortable": true}));
    if editable {
        out.push(json!({"key": "menu", "label": "", "type": "slot", "slot": "menu", "width": RECORD_MENU_WIDTH}));
    }
    out
}

fn set(path: &str, value: Value) -> Value {
    json!({"functionCall": {"call": "set", "args": {"path": path, "value": value}}})
}

fn set_on(node: &mut Map<String, Value>, event: &str, action: Value) {
    let on = node.entry("on").or_insert_with(|| Value::Object(Map::new()));
    if !on.is_object() {
        *on = Value::Object(Map::new());
    }
    let on = on.as_object_mut().expect("an object");
    on.entry(event.to_string()).or_insert(action);
}

fn upsert_action(kind: &str, fields: &[Value], record_id: Option<&Value>) -> Value {
    let mut data = Map::new();
    for f in fields {
        if let (Some(name), Some(value)) = (f.get("name").and_then(Value::as_str), f.get("value")) {
            data.insert(name.into(), value.clone());
        }
    }
    let mut args = Map::new();
    args.insert("kind".into(), json!(kind));
    if let Some(id) = record_id {
        args.insert("id".into(), id.clone());
    }
    args.insert("data".into(), Value::Object(data));
    json!({"functionCall": {"call": "records.upsert", "args": args}})
}

/// True when a template uses a record macro.
pub fn uses_record_macros(components: &[Value]) -> bool {
    components.iter().any(|c| matches!(c.get("component").and_then(Value::as_str), Some("RecordForm" | "RecordTable")))
}

/// Every RecordForm / RecordTable of a template's flat component list,
/// filled from `records` (the package's `records` object); other
/// components untouched. Returns `(components, bindings)`: a RecordTable
/// appends its phone row `<id>__row` and the binding `/recordTable/<id>/rows`.
pub fn fill_record_macros(components: &[Value], bindings: &[Value], records: Option<&Value>, read_only: bool) -> (Vec<Value>, Vec<Value>) {
    let mut out = Vec::new();
    let mut extra = Vec::new();
    let mut binds: Vec<Value> = bindings.to_vec();
    for c in components {
        let Some(obj) = c.as_object().filter(|o| matches!(o.get("component").and_then(Value::as_str), Some("RecordForm" | "RecordTable"))) else {
            out.push(c.clone());
            continue;
        };
        let mut node = obj.clone();
        let id = match node.get("id") {
            Some(Value::String(s)) => s.clone(),
            Some(v) => v.to_string(),
            None => "undefined".into(),
        };
        let kind_name = node.get("kind").and_then(Value::as_str).unwrap_or("").to_string();
        let Some(kind) = records.and_then(Value::as_object).and_then(|r| r.get(&kind_name)) else {
            node.insert("missing".into(), json!(true));
            out.push(Value::Object(node));
            continue;
        };
        if node.get("component").and_then(Value::as_str) == Some("RecordForm") {
            let path = node.get("path").and_then(Value::as_str).map(str::to_string).unwrap_or_else(|| format!("{RECORD_FORM_ROOT}/{id}"));
            let fields = record_form_fields(kind, &path);
            node.insert("path".into(), json!(path));
            node.insert("fields".into(), Value::Array(fields.clone()));
            if read_only {
                node.insert("readOnly".into(), json!(true));
            }
            let record_id = node.get("recordId").cloned();
            set_on(&mut node, "submit", upsert_action(&kind_name, &fields, record_id.as_ref()));
            out.push(Value::Object(node));
            continue;
        }
        let base = format!("{RECORD_TABLE_ROOT}/{id}");
        let editable = node.get("editable") != Some(&Value::Bool(false)) && !read_only;
        let columns: Option<Vec<String>> = node.get("columns").and_then(Value::as_array).map(|l| l.iter().filter_map(Value::as_str).map(str::to_string).collect());
        let draft = format!("{base}/editor/draft");
        let fields = record_form_fields(kind, &draft);
        let title = kind.get("title").and_then(Value::as_str).map(str::to_string).unwrap_or_else(|| field_label(&kind_name));
        node.insert("title".into(), json!(title));
        node.insert("cols".into(), Value::Array(record_table_columns(kind, columns.as_deref(), editable)));
        node.insert("rows".into(), json!({"path": format!("{base}/rows/rows")}));
        node.insert("count".into(), json!({"call": "fill", "args": {"template": "$string.recordCount", "params": {"count": {"call": "coalesce", "args": {"values": [{"path": format!("{base}/rows/count")}, 0]}}}}}));
        node.insert("loading".into(), json!({"call": "not", "args": {"value": {"path": format!("{base}/rows")}}}));
        if read_only {
            node.insert("add".into(), json!(false));
        }
        node.insert("editable".into(), json!(editable));
        node.insert("children".into(), json!({"componentId": format!("{id}__row"), "path": format!("{base}/rows/rows"), "key": "id"}));
        if editable {
            node.insert("fields".into(), Value::Array(fields.clone()));
            node.insert("path".into(), json!(draft));
            node.insert("editorOpen".into(), json!({"path": format!("{base}/editor/open")}));
            node.insert("editorId".into(), json!({"path": format!("{draft}/id")}));
            node.insert("deleteOpen".into(), json!({"path": format!("{base}/delete/open")}));
            set_on(&mut node, "add", set(&format!("{base}/editor"), json!({"open": true, "draft": {}})));
            set_on(&mut node, "edit", set(&format!("{base}/editor"), json!({"open": true, "draft": {"path": ""}})));
            set_on(&mut node, "delete", set(&format!("{base}/delete"), json!({"open": true, "id": {"path": "id"}})));
            set_on(&mut node, "submit", upsert_action(&kind_name, &fields, Some(&json!({"path": format!("{draft}/id")}))));
            set_on(&mut node, "cancel", set(&format!("{base}/editor/open"), json!(false)));
            set_on(&mut node, "confirm", json!({"functionCall": {"call": "records.delete", "args": {"id": {"path": format!("{base}/delete/id")}}}}));
            set_on(&mut node, "cancelDelete", set(&format!("{base}/delete/open"), json!(false)));
        } else {
            node.insert("add".into(), json!(false));
        }
        let query = node.get("query").and_then(Value::as_str).map(str::trim).filter(|q| !q.is_empty()).map(|q| q.strip_prefix('?').unwrap_or(q).to_string()).unwrap_or_else(|| "sort=updatedAt&order=desc".into());
        out.push(Value::Object(node));
        let shown = shown_columns(kind, columns.as_deref());
        let first = shown.first().cloned().unwrap_or_else(|| "id".into());
        let mut row = Map::new();
        row.insert("id".into(), json!(format!("{id}__row")));
        row.insert("component".into(), json!("Row"));
        row.insert("title".into(), json!({"call": "coalesce", "args": {"values": [{"call": "text", "args": {"value": {"path": first}}}, "—"]}}));
        if let Some(second) = shown.get(1) {
            row.insert("subtitle".into(), json!({"call": "coalesce", "args": {"values": [{"call": "text", "args": {"value": {"path": second}}}, ""]}}));
        }
        row.insert("meta".into(), json!({"call": "formatRelativeTime", "args": {"value": {"path": "updatedAt"}}}));
        if editable {
            row.insert("on".into(), json!({"press": set(&format!("{base}/editor"), json!({"open": true, "draft": {"path": ""}}))}));
        }
        extra.push(Value::Object(row));
        binds.push(json!({"path": format!("{base}/rows"), "source": format!("records:{kind_name}?{query}")}));
    }
    out.extend(extra);
    (out, binds)
}

/// VAPP-217: a writing RecordForm / editable RecordTable needs
/// `records.upsert` (a table also `records.delete`) in `functions`; a
/// RecordTable's `records:<kind>` must pass `sources` when set. At the
/// macro's component.
pub fn record_macro_issues(pkg: &Map<String, Value>) -> Vec<(String, String)> {
    let mut issues = Vec::new();
    let Some(templates) = pkg.get("templates").and_then(Value::as_object) else { return issues };
    let functions: Vec<String> = pkg.get("functions").and_then(Value::as_array).map(|l| l.iter().filter_map(Value::as_str).map(str::to_string).collect()).unwrap_or_default();
    let allows = |name: &str| functions.iter().any(|p| super::policy::matches_pattern(p, name));
    let sources = super::package::package_sources(&Value::Object(pkg.clone()));
    for (id, template) in templates {
        let Some(components) = template.get("components").and_then(Value::as_array) else { continue };
        for (i, c) in components.iter().enumerate() {
            let Some(c) = c.as_object() else { continue };
            let component = c.get("component").and_then(Value::as_str).unwrap_or("");
            if component != "RecordForm" && component != "RecordTable" {
                continue;
            }
            let at = format!("/templates/{id}/components/{i}");
            let writes = if component == "RecordForm" { c.get("readOnly") != Some(&Value::Bool(true)) } else { c.get("editable") != Some(&Value::Bool(false)) };
            if writes && !allows("records.upsert") {
                issues.push((at.clone(), format!("{component} writes with records.upsert: list it in functions")));
            }
            if component == "RecordTable" && writes && !allows("records.delete") {
                issues.push((at.clone(), "RecordTable deletes with records.delete: list it in functions".into()));
            }
            if component == "RecordTable" {
                if let (Some(kind), Some(list)) = (c.get("kind").and_then(Value::as_str), sources.as_deref()) {
                    if !super::package::source_allowed(Some(list), &format!("records:{kind}")) {
                        issues.push((at, format!("RecordTable reads records:{kind}: list it in sources")));
                    }
                }
            }
        }
    }
    issues
}
