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

pub const RECORD_UPDATED_WIDTH: i64 = 136;
/// A date-time cell's formatted `Nov 12, 2026, 3:30 AM` (the Data tab's raw editor keeps 152).
pub const RECORD_DATE_TIME_WIDTH: i64 = 188;
pub const RECORD_MENU_WIDTH: i64 = 80;
/// FEED-202: a `currency` column without the author's width.
pub const RECORD_CURRENCY_WIDTH: i64 = 96;
/// FEED-202: what an author's column object may set over the field's
/// defaults (Table's column shape; never a slot).
const COLUMN_KEYS: [&str; 9] = ["label", "type", "currency", "decimals", "align", "width", "format", "truncate", "sortable"];
/// A string longer than this (its `maxLength`) is a Textarea.
pub const RECORD_LONG_TEXT: f64 = 200.0;
pub const RECORD_FORM_ROOT: &str = "/recordForm";
pub const RECORD_TABLE_ROOT: &str = "/recordTable";
/// The team's members and issues the macros' Selects read (one binding per
/// surface, outside the package's `sources`: the macro's own).
pub const RECORD_MEMBERS_PATH: &str = "/recordMembers";
pub const RECORD_ISSUES_PATH: &str = "/recordIssues";
const MEMBERS_SOURCE: &str = "exp:members";
const ISSUES_SOURCE: &str = "exp:issues";

/// A record macro's own source at its own path passes any `sources`.
pub fn is_record_macro_binding(path: &str, source: &str) -> bool {
    (path == RECORD_MEMBERS_PATH && source == MEMBERS_SOURCE) || (path == RECORD_ISSUES_PATH && source == ISSUES_SOURCE)
}

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
        let value = json!({"@path": format!("{path}/{name}")});
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
            let list = if reference == Some("member") { RECORD_MEMBERS_PATH } else { RECORD_ISSUES_PATH };
            o.insert("options".into(), json!({"@path": format!("{list}/rows")}));
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
            widget = "datetime";
            o.insert("wide".into(), json!(true));
            o.insert("date".into(), json!({"@path": format!("{path}Parts/{name}/date")}));
            o.insert("time".into(), json!({"@path": format!("{path}Parts/{name}/time")}));
        } else if f.get("maxLength").and_then(Value::as_f64).is_none_or(|m| m > RECORD_LONG_TEXT) {
            widget = "textarea";
            o.insert("wide".into(), json!(true));
        }
        o.insert("widget".into(), json!(widget));
        let mut checks = Vec::new();
        if is_required && widget != "switch" {
            checks.push(json!({"condition": {"@call": "required", "args": {"value": value}}, "message": "$string.required"}));
        }
        if ty == "string" && f.get("enum").is_none() && widget != "select" {
            if let Some(max) = f.get("maxLength") {
                checks.push(json!({"condition": {"@call": "length", "args": {"value": value, "max": max}}, "message": "$string.invalidValue"}));
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
            checks.push(json!({"condition": {"@call": "numeric", "args": args}, "message": "$string.invalidValue"}));
        }
        if !checks.is_empty() {
            o.insert("checks".into(), Value::Array(checks));
        }
        out.push(Value::Object(o));
    }
    out
}

/// The shown entries of `columns` (default every field), files left out: a
/// field name or a column object (`{key, …}`), keyed by its field.
fn shown_entries(kind: &Value, columns: Option<&[Value]>) -> Vec<(String, Option<Map<String, Value>>)> {
    let props = properties(kind);
    let entries: Vec<Value> = columns.map(<[Value]>::to_vec).unwrap_or_else(|| record_columns(kind).into_iter().map(Value::String).collect());
    entries
        .into_iter()
        .filter_map(|e| match e {
            Value::String(key) => Some((key, None)),
            Value::Object(o) => o.get("key").and_then(Value::as_str).map(|k| (k.to_string(), Some(o.clone()))),
            _ => None,
        })
        .filter(|(n, _)| props.get(n).and_then(Value::as_object).is_some_and(|f| !is_file(f)))
        .collect()
}

fn shown_columns(kind: &Value, columns: Option<&[Value]>) -> Vec<String> {
    shown_entries(kind, columns).into_iter().map(|(k, _)| k).collect()
}

/// The table columns: the chosen fields (default every one), typed and
/// sized like the Data tab, an author's column object over them, then
/// `updatedAt`, then the row menu.
pub fn record_table_columns(kind: &Value, columns: Option<&[Value]>, editable: bool) -> Vec<Value> {
    let props = properties(kind);
    let mut out: Vec<Value> = shown_entries(kind, columns)
        .into_iter()
        .map(|(key, entry)| {
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
            } else if f.get("format").and_then(Value::as_str) == Some("date-time") {
                col.insert("type".into(), json!("dateTime"));
            }
            let entry = entry.unwrap_or_default();
            for k in COLUMN_KEYS {
                if let Some(v) = entry.get(k).filter(|v| !v.is_null() && !(k == "type" && v.as_str() == Some("slot"))) {
                    col.insert(k.into(), v.clone());
                }
            }
            if entry.get("width").is_none_or(Value::is_null) {
                let width = if col.get("type").and_then(Value::as_str) == Some("currency") {
                    Some(RECORD_CURRENCY_WIDTH)
                } else if f.get("format").and_then(Value::as_str) == Some("date-time") {
                    Some(RECORD_DATE_TIME_WIDTH)
                } else {
                    column_width(&f)
                };
                match width {
                    // A fixed column never truncates its own header: the label at
                    // the header's 8 px a letter, its 8 + 8 padding and the 16 + 4
                    // sort arrow.
                    Some(w) => {
                        let label = col.get("label").map(crate::layout_tree::js).unwrap_or_default();
                        col.insert("width".into(), json!(w.max(label.chars().count() as i64 * 8 + 36)));
                    }
                    None => {
                        col.entry("truncate").or_insert(json!(true));
                    }
                }
            }
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
    json!({"functionCall": {"@call": "set", "args": {"path": path, "value": value}}})
}

fn set_on(node: &mut Map<String, Value>, event: &str, action: Value) {
    let on = node.entry("on").or_insert_with(|| Value::Object(Map::new()));
    if !on.is_object() {
        *on = Value::Object(Map::new());
    }
    let on = on.as_object_mut().expect("an object");
    on.entry(event.to_string()).or_insert(action);
}

/// A date-time field's value: its picked day + time in the team zone; no
/// day picked = the draft's value as it is.
fn date_time_value(f: &Value) -> Value {
    json!({"@call": "cond", "args": {"if": f.get("date"), "then": {"@call": "dateTime", "args": {"date": f.get("date"), "time": f.get("time")}}, "else": f.get("value")}})
}

fn my_id() -> Value {
    json!({"@path": format!("{RECORD_MEMBERS_PATH}/me")})
}

/// The Author Select's options: the members for a team owner, else none (so
/// no picker).
fn author_options() -> Value {
    let rows = json!({"@path": format!("{RECORD_MEMBERS_PATH}/rows")});
    let mine = json!({"@call": "filter", "args": {"items": rows, "where": {"id": my_id(), "role": "owner"}}});
    json!({"@call": "cond", "args": {"if": {"@call": "lt", "args": {"a": 0, "b": {"@call": "len", "args": {"value": mine}}}}, "then": rows, "else": null}})
}

/// The form's `author`: an owner's pick, only when it moved off the record's
/// author (domain-contract app-records.json `formAuthor`), else nothing.
fn author_arg(path: &str) -> Value {
    let picked = json!({"@path": format!("{path}Author")});
    let base = json!({"@path": format!("{path}AuthorBase")});
    json!({"@call": "cond", "args": {"if": author_options(), "then": {"@call": "cond", "args": {"if": {"@call": "or", "args": {"values": [{"@call": "not", "args": {"value": picked}}, {"@call": "eq", "args": {"a": picked, "b": base}}]}}, "then": null, "else": picked}}, "else": null}})
}

fn has_issue_ref(fields: &[Value]) -> bool {
    let rows = format!("{RECORD_ISSUES_PATH}/rows");
    fields.iter().any(|f| f.get("options").and_then(|o| o.get("@path")).and_then(Value::as_str) == Some(rows.as_str()))
}

fn bind_once(binds: &mut Vec<Value>, source: &str, path: &str) {
    if !binds.iter().any(|b| b.get("path").and_then(Value::as_str) == Some(path)) {
        binds.push(json!({"path": path, "source": source}));
    }
}

/// Opens the editor on a row: the draft = the row, a date-time's parts = its
/// day and time in the team zone, the Author = the row's.
fn edit_action(base: &str, fields: &[Value]) -> Value {
    let mut parts = Map::new();
    for f in fields {
        let Some(name) = f.get("name").and_then(Value::as_str).filter(|_| f.get("widget").and_then(Value::as_str) == Some("datetime")) else { continue };
        let value = json!({"@path": name});
        parts.insert(name.into(), json!({"date": {"@call": "formatDate", "args": {"value": value, "format": "yyyy-MM-dd"}}, "time": {"@call": "formatDate", "args": {"value": value, "format": "HH:mm"}}}));
    }
    let mut editor = json!({"open": true, "draft": {"@path": ""}, "draftAuthor": {"@call": "coalesce", "args": {"values": [{"@path": "authorId"}, my_id()]}}, "draftAuthorBase": {"@path": "authorId"}});
    if !parts.is_empty() {
        editor["draftParts"] = Value::Object(parts);
    }
    set(&format!("{base}/editor"), editor)
}

fn upsert_action(kind: &str, fields: &[Value], record_id: Option<&Value>, path: &str, close: Option<&str>) -> Value {
    let mut data = Map::new();
    for f in fields {
        if let (Some(name), Some(value)) = (f.get("name").and_then(Value::as_str), f.get("value")) {
            let value = if f.get("widget").and_then(Value::as_str) == Some("datetime") { date_time_value(f) } else { value.clone() };
            // VAPP-206: an optional field left "" (an unpicked Select, an
            // emptied Input) writes null: absent on a create, cleared on a patch.
            let bound = if f.get("required") == Some(&Value::Bool(true)) { value } else { json!({"@call": "expr", "args": {"expr": "v == '' ? null : v", "vars": {"v": value}, "default": null}}) };
            data.insert(name.into(), bound);
        }
    }
    let mut args = Map::new();
    args.insert("kind".into(), json!(kind));
    if let Some(id) = record_id {
        args.insert("id".into(), id.clone());
    }
    args.insert("data".into(), Value::Object(data));
    args.insert("author".into(), author_arg(path));
    let mut form = json!({"error": format!("{path}Error")});
    if let Some(close) = close {
        form["close"] = json!(close);
    }
    args.insert("form".into(), form);
    json!({"functionCall": {"@call": "records_upsert", "args": args}})
}

/// True when a template uses a record macro.
pub fn uses_record_macros(components: &[Value]) -> bool {
    components.iter().any(|c| matches!(c.get("component").and_then(Value::as_str), Some("RecordForm" | "RecordTable")))
}

/// Every RecordForm / RecordTable of a template's flat component list,
/// filled from `records` (the package's `records` object); other
/// components untouched. Returns `(components, bindings)`: a RecordTable
/// appends its phone row `<id>__row`, when editable its Delete
/// `<id>__rowDelete`, and the binding `/recordTable/<id>/rows`.
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
            if node.get("readOnly") != Some(&Value::Bool(true)) {
                node.insert("error".into(), json!({"@path": format!("{path}Error")}));
                node.insert("author".into(), json!({"@path": format!("{path}Author")}));
                node.insert("authorOptions".into(), author_options());
                bind_once(&mut binds, MEMBERS_SOURCE, RECORD_MEMBERS_PATH);
            }
            if has_issue_ref(&fields) {
                bind_once(&mut binds, ISSUES_SOURCE, RECORD_ISSUES_PATH);
            }
            let record_id = node.get("recordId").cloned();
            set_on(&mut node, "submit", upsert_action(&kind_name, &fields, record_id.as_ref(), &path, None));
            out.push(Value::Object(node));
            continue;
        }
        let base = format!("{RECORD_TABLE_ROOT}/{id}");
        let editable = node.get("editable") != Some(&Value::Bool(false)) && !read_only;
        let columns: Option<Vec<Value>> = node.get("columns").and_then(Value::as_array).cloned();
        let draft = format!("{base}/editor/draft");
        let fields = record_form_fields(kind, &draft);
        let title = kind.get("title").and_then(Value::as_str).map(str::to_string).unwrap_or_else(|| field_label(&kind_name));
        node.insert("title".into(), json!(title));
        node.insert("cols".into(), Value::Array(record_table_columns(kind, columns.as_deref(), editable)));
        // FEED-202: the search (over the shown text columns) and the enum
        // filter narrow the rows through the core `filter`; the count, the
        // totals and the phone rows follow.
        let props = properties(kind);
        let shown = shown_columns(kind, columns.as_deref());
        let search_fields: Vec<String> = if node.get("search") == Some(&Value::Bool(true)) {
            shown.iter().filter(|n| props.get(*n).is_some_and(|f| f.get("type").and_then(Value::as_str) == Some("string") && f.get("format").is_none())).cloned().collect()
        } else {
            Vec::new()
        };
        let filter_by = node.get("filter").and_then(Value::as_str).map(str::to_string).filter(|n| props.get(n).is_some_and(|f| f.get("type").and_then(Value::as_str) == Some("string") && f.get("enum").is_some_and(Value::is_array)));
        let mut narrow = Map::new();
        if !search_fields.is_empty() {
            narrow.insert("query".into(), json!({"@path": format!("{base}/search")}));
            narrow.insert("fields".into(), json!(search_fields));
        }
        if let Some(by) = &filter_by {
            let mut wh = Map::new();
            wh.insert(by.clone(), json!({"@path": format!("{base}/filter")}));
            narrow.insert("where".into(), Value::Object(wh));
        }
        let narrows = !narrow.is_empty();
        if node.contains_key("search") {
            node.insert("search".into(), json!(!search_fields.is_empty()));
        }
        if !search_fields.is_empty() {
            node.insert("searchValue".into(), json!({"@path": format!("{base}/search")}));
        }
        if let Some(by) = &filter_by {
            node.insert("filterLabel".into(), json!(field_label(by)));
            let mut opts = vec![json!({"label": "$string.all", "value": ""})];
            if let Some(Value::Array(values)) = props.get(by).and_then(|f| f.get("enum")) {
                opts.extend(values.iter().map(|v| json!({"label": v, "value": v})));
            }
            node.insert("filterOptions".into(), Value::Array(opts));
            node.insert("filterValue".into(), json!({"@path": format!("{base}/filter")}));
        } else {
            node.remove("filter");
        }
        let all_rows = json!({"@path": format!("{base}/rows/rows")});
        let rows = if narrows {
            let mut args = Map::new();
            args.insert("items".into(), all_rows);
            args.extend(narrow.clone());
            json!({"@call": "filter", "args": args})
        } else {
            all_rows
        };
        let total = if narrows { json!({"@call": "len", "args": {"value": rows}}) } else { json!({"@call": "coalesce", "args": {"values": [{"@path": format!("{base}/rows/count")}, 0]}}) };
        node.insert("rows".into(), rows);
        node.insert("count".into(), json!({"@call": "pluralize", "args": {"value": total, "one": "$string.recordCountOne", "other": "$string.recordCount"}}));
        node.insert("loading".into(), json!({"@call": "not", "args": {"value": {"@path": format!("{base}/rows")}}}));
        if read_only {
            node.insert("add".into(), json!(false));
        }
        node.insert("editable".into(), json!(editable));
        node.insert("children".into(), json!({"componentId": format!("{id}__row"), "path": format!("{base}/rows/rows"), "key": "id"}));
        if editable {
            node.insert("fields".into(), Value::Array(fields.clone()));
            node.insert("path".into(), json!(draft));
            node.insert("error".into(), json!({"@path": format!("{draft}Error")}));
            node.insert("author".into(), json!({"@path": format!("{draft}Author")}));
            node.insert("authorOptions".into(), author_options());
            bind_once(&mut binds, MEMBERS_SOURCE, RECORD_MEMBERS_PATH);
            if has_issue_ref(&fields) {
                bind_once(&mut binds, ISSUES_SOURCE, RECORD_ISSUES_PATH);
            }
            node.insert("editorOpen".into(), json!({"@path": format!("{base}/editor/open")}));
            node.insert("editorId".into(), json!({"@path": format!("{draft}/id")}));
            node.insert("deleteOpen".into(), json!({"@path": format!("{base}/delete/open")}));
            set_on(&mut node, "add", set(&format!("{base}/editor"), json!({"open": true, "draft": {}, "draftAuthor": my_id(), "draftAuthorBase": my_id()})));
            set_on(&mut node, "edit", edit_action(&base, &fields));
            set_on(&mut node, "delete", set(&format!("{base}/delete"), json!({"open": true, "id": {"@path": "id"}})));
            set_on(&mut node, "submit", upsert_action(&kind_name, &fields, Some(&json!({"@path": format!("{draft}/id")})), &draft, Some(format!("{base}/editor/open").as_str())));
            set_on(&mut node, "cancel", set(&format!("{base}/editor/open"), json!(false)));
            set_on(&mut node, "confirm", json!({"functionCall": {"@call": "records_delete", "args": {"id": {"@path": format!("{base}/delete/id")}}}}));
            set_on(&mut node, "cancelDelete", set(&format!("{base}/delete/open"), json!(false)));
        } else {
            node.insert("add".into(), json!(false));
        }
        let delete_action = node.get("on").and_then(|on| on.get("delete")).cloned();
        let query = node.get("query").and_then(Value::as_str).map(str::trim).filter(|q| !q.is_empty()).map(|q| q.strip_prefix('?').unwrap_or(q).to_string()).unwrap_or_else(|| "sort=updatedAt&order=desc".into());
        out.push(Value::Object(node));
        let first = shown.first().cloned().unwrap_or_else(|| "id".into());
        let mut row = Map::new();
        row.insert("id".into(), json!(format!("{id}__row")));
        row.insert("component".into(), json!("Row"));
        row.insert("title".into(), json!({"@call": "coalesce", "args": {"values": [{"@call": "text", "args": {"value": {"@path": first}}}, "—"]}}));
        if let Some(second) = shown.get(1) {
            row.insert("subtitle".into(), json!({"@call": "coalesce", "args": {"values": [{"@call": "text", "args": {"value": {"@path": second}}}, ""]}}));
        }
        row.insert("meta".into(), json!({"@call": "formatRelativeTime", "args": {"value": {"@path": "updatedAt"}}}));
        if narrows {
            let mut args = Map::new();
            args.insert("items".into(), json!([{"@path": ""}]));
            args.extend(narrow);
            row.insert("visible".into(), json!({"@call": "lt", "args": {"a": 0, "b": {"@call": "len", "args": {"value": {"@call": "filter", "args": args}}}}}));
        }
        if editable {
            row.insert("on".into(), json!({"press": edit_action(&base, &fields)}));
            // The row's own Delete (the table's Delete cell is hidden on phones).
            row.insert("slots".into(), json!({"trailing": format!("{id}__rowDelete")}));
        }
        extra.push(Value::Object(row));
        if editable {
            extra.push(json!({"id": format!("{id}__rowDelete"), "component": "Button", "icon": "ui-delete", "variant": "ghost", "size": "icon", "accessibility": {"label": "$string.delete"}, "on": {"press": delete_action}}));
        }
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
            if writes && !allows("records_upsert") {
                issues.push((at.clone(), format!("{component} writes with records_upsert: list it in functions")));
            }
            if component == "RecordTable" && writes && !allows("records_delete") {
                issues.push((at.clone(), "RecordTable deletes with records_delete: list it in functions".into()));
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
