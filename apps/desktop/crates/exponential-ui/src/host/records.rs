//! VAPP-122: an app's RECORDS, its own data. The manifest declares every
//! kind with a JSON Schema SUBSET (`records: {<kind>: {schema}}`); a host
//! validates a record against it before it is stored. Mirrors
//! `packages/exponential-ui/src/host/records.ts` line by line;
//! `fixtures/host-router.json` `records` replays it.

use serde_json::Value;

use super::package::PackageIssue;

/// `^[a-z][a-z0-9_]{0,31}$`: a kind's name (catalog/host.json `package.records`).
pub fn is_record_kind(name: &str) -> bool {
    let mut chars = name.chars();
    chars.next().is_some_and(|c| c.is_ascii_lowercase()) && name.len() <= 32 && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

/// The keys every record row carries; a schema may not declare them.
pub const RECORD_RESERVED_KEYS: &[&str] = &["id", "createdAt", "updatedAt", "createdBy", "author"];
pub const RECORD_FIELD_TYPES: &[&str] = &["string", "number", "integer", "boolean", "array"];
pub const RECORD_ITEM_TYPES: &[&str] = &["string", "number", "integer", "boolean"];
pub const RECORD_FORMATS: &[&str] = &["date", "date-time", "time"];

fn issue(path: impl Into<String>, message: impl Into<String>) -> PackageIssue {
    PackageIssue { path: path.into(), message: message.into() }
}

fn string_list(v: &Value) -> Option<Vec<&str>> {
    v.as_array().and_then(|list| list.iter().map(Value::as_str).collect())
}

fn non_empty_string_list(v: &Value) -> bool {
    string_list(v).is_some_and(|l| !l.is_empty())
}

/// Every problem with a kind's schema, each with a JSON pointer under `at`.
pub fn validate_record_schema(schema: &Value, at: &str) -> Vec<PackageIssue> {
    let mut issues = Vec::new();
    let Some((obj, properties)) = schema.as_object().filter(|o| o.get("type").and_then(Value::as_str) == Some("object")).and_then(|o| o.get("properties").and_then(Value::as_object).map(|p| (o, p))) else {
        return vec![issue(at, "a record schema is {type: \"object\", properties}")];
    };
    for (name, field) in properties {
        let fp = format!("{at}/properties/{name}");
        if RECORD_RESERVED_KEYS.contains(&name.as_str()) {
            issues.push(issue(&fp, format!("{name} is a reserved record key")));
        }
        let Some(field) = field.as_object().filter(|f| f.get("type").and_then(Value::as_str).is_some_and(|t| RECORD_FIELD_TYPES.contains(&t))) else {
            issues.push(issue(&fp, format!("a field is {{type: {}}}", RECORD_FIELD_TYPES.join("|"))));
            continue;
        };
        let ty = field["type"].as_str().unwrap_or("");
        if let Some(e) = field.get("enum") {
            if !non_empty_string_list(e) || ty != "string" {
                issues.push(issue(format!("{fp}/enum"), "enum is a non-empty list of strings on a string field"));
            }
        }
        if ty == "array" {
            match field.get("items").and_then(Value::as_object).filter(|i| i.get("type").and_then(Value::as_str).is_some_and(|t| RECORD_ITEM_TYPES.contains(&t))) {
                None => issues.push(issue(format!("{fp}/items"), format!("items is {{type: {}}}", RECORD_ITEM_TYPES.join("|")))),
                Some(items) => {
                    if let Some(e) = items.get("enum") {
                        if !non_empty_string_list(e) || items["type"].as_str() != Some("string") {
                            issues.push(issue(format!("{fp}/items/enum"), "enum is a non-empty list of strings on a string item"));
                        }
                    }
                }
            }
        } else if field.get("items").is_some() {
            issues.push(issue(format!("{fp}/items"), "items belongs to an array field"));
        }
        if let Some(f) = field.get("format") {
            if !f.as_str().is_some_and(|f| RECORD_FORMATS.contains(&f)) || ty != "string" {
                issues.push(issue(format!("{fp}/format"), format!("format is {} on a string field", RECORD_FORMATS.join("|"))));
            }
        }
        for key in ["maxLength", "minimum", "maximum"] {
            if field.get(key).is_some_and(|v| !v.is_number()) {
                issues.push(issue(format!("{fp}/{key}"), format!("{key} is a number")));
            }
        }
    }
    if let Some(required) = obj.get("required") {
        match string_list(required) {
            None => issues.push(issue(format!("{at}/required"), "required is a list of field names")),
            Some(list) => {
                for (i, name) in list.iter().enumerate() {
                    if !properties.contains_key(*name) {
                        issues.push(issue(format!("{at}/required/{i}"), format!("{name} is not a declared field")));
                    }
                }
            }
        }
    }
    if obj.get("additionalProperties").is_some_and(|v| !v.is_boolean()) {
        issues.push(issue(format!("{at}/additionalProperties"), "additionalProperties is a boolean"));
    }
    issues
}

fn is_date(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() == 10 && b.iter().enumerate().all(|(i, c)| if i == 4 || i == 7 { *c == b'-' } else { c.is_ascii_digit() })
}

fn is_time(s: &str) -> bool {
    let b = s.as_bytes();
    let digits = |r: std::ops::Range<usize>| b[r].iter().all(u8::is_ascii_digit);
    (b.len() == 5 && digits(0..2) && b[2] == b':' && digits(3..5)) || (b.len() == 8 && digits(0..2) && b[2] == b':' && digits(3..5) && b[5] == b':' && digits(6..8))
}

/// `^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}(:\d{2}(\.\d+)?)?(Z|[+-]\d{2}:\d{2})$`
fn is_date_time(s: &str) -> bool {
    let b = s.as_bytes();
    if b.len() < 17 || !is_date(&s[..10]) || b[10] != b'T' {
        return false;
    }
    let digits = |r: std::ops::Range<usize>| r.end <= b.len() && b[r].iter().all(u8::is_ascii_digit);
    if !(digits(11..13) && b[13] == b':' && digits(14..16)) {
        return false;
    }
    let mut i = 16;
    if b.get(i) == Some(&b':') {
        if !digits(i + 1..i + 3) {
            return false;
        }
        i += 3;
        if b.get(i) == Some(&b'.') {
            let start = i + 1;
            let mut j = start;
            while j < b.len() && b[j].is_ascii_digit() {
                j += 1;
            }
            if j == start {
                return false;
            }
            i = j;
        }
    }
    match b.get(i) {
        Some(b'Z') => i + 1 == b.len(),
        Some(b'+' | b'-') => i + 6 == b.len() && digits(i + 1..i + 3) && b[i + 3] == b':' && digits(i + 4..i + 6),
        _ => false,
    }
}

fn number_text(v: &Value) -> String {
    crate::json::to_js_string(v)
}

fn check_scalar(ty: &str, value: &Value, at: &str, field: &serde_json::Map<String, Value>) -> Vec<PackageIssue> {
    let mut issues = Vec::new();
    match ty {
        "string" => {
            let Some(s) = value.as_str() else { return vec![issue(at, "a string")] };
            if let Some(list) = field.get("enum").and_then(string_list) {
                if !list.contains(&s) {
                    issues.push(issue(at, format!("one of {}", list.join("|"))));
                }
            }
            if let Some(max) = field.get("maxLength").and_then(Value::as_f64) {
                if (s.chars().count() as f64) > max {
                    issues.push(issue(at, format!("at most {} characters", number_text(&field["maxLength"]))));
                }
            }
            match field.get("format").and_then(Value::as_str) {
                Some("date") if !is_date(s) => issues.push(issue(at, "a date (YYYY-MM-DD)")),
                Some("date-time") if !is_date_time(s) => issues.push(issue(at, "a date-time (ISO 8601)")),
                Some("time") if !is_time(s) => issues.push(issue(at, "a time (HH:MM)")),
                _ => {}
            }
            issues
        }
        "number" | "integer" => {
            let Some(n) = value.as_f64().filter(|n| n.is_finite()) else { return vec![issue(at, "a number")] };
            if ty == "integer" && n.fract() != 0.0 {
                return vec![issue(at, "an integer")];
            }
            if let Some(min) = field.get("minimum").and_then(Value::as_f64) {
                if n < min {
                    issues.push(issue(at, format!("at least {}", number_text(&field["minimum"]))));
                }
            }
            if let Some(max) = field.get("maximum").and_then(Value::as_f64) {
                if n > max {
                    issues.push(issue(at, format!("at most {}", number_text(&field["maximum"]))));
                }
            }
            issues
        }
        "boolean" => {
            if value.is_boolean() {
                issues
            } else {
                vec![issue(at, "a boolean")]
            }
        }
        _ => issues,
    }
}

/// Every problem with `data` against `schema` (a valid one): `/field`
/// pointers. A null or missing optional field passes; a required one does
/// not; a key the schema does not declare is refused unless
/// `additionalProperties` is true.
pub fn validate_record(schema: &Value, data: &Value) -> Vec<PackageIssue> {
    let Some(data) = data.as_object() else { return vec![issue("", "a record is an object")] };
    let mut issues = Vec::new();
    let properties = schema.get("properties").and_then(Value::as_object);
    let required: Vec<&str> = schema.get("required").and_then(string_list).unwrap_or_default();
    for (name, field) in properties.into_iter().flatten() {
        let Some(field) = field.as_object() else { continue };
        let value = data.get(name);
        if value.is_none_or(Value::is_null) {
            if required.contains(&name.as_str()) {
                issues.push(issue(format!("/{name}"), "required"));
            }
            continue;
        }
        let value = value.unwrap();
        let ty = field.get("type").and_then(Value::as_str).unwrap_or("");
        if ty == "array" {
            let Some(list) = value.as_array() else {
                issues.push(issue(format!("/{name}"), "a list"));
                continue;
            };
            if let Some(max) = field.get("maxLength").and_then(Value::as_f64) {
                if (list.len() as f64) > max {
                    issues.push(issue(format!("/{name}"), format!("at most {} items", number_text(&field["maxLength"]))));
                }
            }
            let fallback = serde_json::Map::from_iter([("type".to_string(), Value::String("string".into()))]);
            let items = field.get("items").and_then(Value::as_object).unwrap_or(&fallback);
            let item_type = items.get("type").and_then(Value::as_str).unwrap_or("string");
            let mut item_field = serde_json::Map::new();
            if let Some(e) = items.get("enum") {
                item_field.insert("enum".into(), e.clone());
            }
            for (i, item) in list.iter().enumerate() {
                issues.extend(check_scalar(item_type, item, &format!("/{name}/{i}"), &item_field));
            }
            continue;
        }
        issues.extend(check_scalar(ty, value, &format!("/{name}"), field));
    }
    if schema.get("additionalProperties").and_then(Value::as_bool) != Some(true) {
        for key in data.keys() {
            if !properties.is_some_and(|p| p.contains_key(key)) {
                issues.push(issue(format!("/{key}"), "not in the schema"));
            }
        }
    }
    issues
}
